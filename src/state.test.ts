import { describe, expect, it, vi } from "vitest";
import { mockRoot, mockSnapshot } from "./mock/snapshot";
import { assess } from "./types";
import type { ExecutionOutcome } from "./types";
import { basename, isAbsolutePath, normalizePathKey, pathMatchesRule } from "./format";
import { messages } from "./i18n";
import { primaryReason } from "./state";

// The state module keeps mutable refs at module scope, so every test imports a
// fresh copy: `vi.resetModules()` clears the registry and the dynamic import below
// re-evaluates it. Nothing leaks from one test into the next.
async function freshState(): Promise<typeof import("./state")> {
  vi.resetModules();
  return await import("./state");
}

/**
 * The same fresh module, with the desktop boundary stubbed.
 *
 * Executing for real needs the Tauri backend (the core's `execute_plan` does the
 * work, and the Rust integration tests cover that side). What these tests are
 * about is what the front end does *with* the executor's answer, so the answer is
 * handed in directly: `plan` is recorded, and the outcome comes back verbatim.
 */
async function freshStateWithExecutor(
  outcome: ExecutionOutcome
): Promise<{ state: typeof import("./state"); plans: unknown[] }> {
  vi.resetModules();
  const plans: unknown[] = [];
  vi.doMock("./platform", () => ({
    isDesktop: () => true,
    scanProject: vi.fn(),
    executePlan: async (plan: unknown) => {
      plans.push(plan);
      return outcome;
    },
    logPath: async () => null,
    readHistory: async () => outcome.history
  }));
  const state = await import("./state");
  return { state, plans };
}

/** Undoes `freshStateWithExecutor` so the next test sees the real boundary. */
function unmockPlatform() {
  vi.doUnmock("./platform");
  vi.resetModules();
}

const codex = `${mockRoot}--codex-fix-ble`;
const cursor = `${mockRoot}--cursor-perf`;
const gone = `${mockRoot}--wt-gone`;

const venv = `${mockRoot}/.venv`;
const dist = `${mockRoot}/dist`;
const turbo = `${mockRoot}/.turbo`;
const rootNodeModules = `${mockRoot}/node_modules`;
const cursorNodeModules = `${cursor}/node_modules`;
const cursorCoverage = `${cursor}/coverage`;
const nuxt = `${cursor}/.nuxt`;
const trackedBuild = `${mockRoot}/crates/remote-mic-core/build`;
// Safe to delete, but its size walk timed out: `size_bytes` is null and every
// total that includes it is a lower bound.
const mainCoverage = `${mockRoot}/coverage`;

const MIN_MB = 10 * 1024 * 1024;

describe("default selection (D12)", () => {
  it("selects Safe items, leaves Review unselected, and never offers Unsafe", async () => {
    const state = await freshState();
    state.snapshot.value = mockSnapshot;
    state.applyDefaultSelection();

    // Safe worktrees are selected by default.
    expect(state.isSelectedWorktree(codex)).toBe(true);
    expect(state.isSelectedWorktree(gone)).toBe(true);
    // Review is not selected by default.
    expect(state.isSelectedWorktree(cursor)).toBe(false);
    expect(state.isSelectedArtifact(nuxt)).toBe(false);
    expect(state.isSelectedArtifact(rootNodeModules)).toBe(false);
    // Unsafe is not selected.
    expect(state.isSelectedWorktree(mockRoot)).toBe(false);

    // An Unsafe path cannot be toggled on.
    state.toggleWorktree(mockRoot);
    expect(state.isSelectedWorktree(mockRoot)).toBe(false);
    state.toggleArtifact(trackedBuild);
    expect(state.isSelectedArtifact(trackedBuild)).toBe(false);

    // A Review worktree can be toggled on (only Unsafe is refused).
    state.toggleWorktree(cursor);
    expect(state.isSelectedWorktree(cursor)).toBe(true);
  });
});

describe("default selection and bundled artifacts", () => {
  it("does not double-count artifacts owned by a selected worktree", async () => {
    const state = await freshState();
    state.snapshot.value = mockSnapshot;
    state.applyDefaultSelection();

    // The owning worktree is selected, so its artifacts are pruned from the selection.
    expect(state.isSelectedArtifact(`${codex}/target`)).toBe(false);
    expect(state.isSelectedArtifact(`${codex}/dist`)).toBe(false);

    const selectedWorktreePaths = new Set(
      mockSnapshot.worktrees.filter((w) => w.safety.status === "Safe").map((w) => w.path)
    );
    const owned = (worktreePath: string | null) => worktreePath !== null && selectedWorktreePaths.has(worktreePath);
    const safeArtifacts = mockSnapshot.artifacts.filter(
      (a) => a.safety.status === "Safe" && (a.size_bytes ?? 0) >= MIN_MB
    );

    const expectedWorktrees = mockSnapshot.worktrees
      .filter((w) => w.safety.status === "Safe")
      .reduce((sum, w) => sum + (w.size_bytes ?? 0), 0);
    const expectedArtifacts = safeArtifacts
      .filter((a) => !owned(a.worktree_path))
      .reduce((sum, a) => sum + (a.size_bytes ?? 0), 0);
    const doubleCounted = safeArtifacts.reduce((sum, a) => sum + (a.size_bytes ?? 0), 0);

    // The naive sum counts the bundled artifacts twice; the summary must not.
    expect(expectedWorktrees + doubleCounted).toBeGreaterThan(expectedWorktrees + expectedArtifacts);
    expect(state.selectedSummary.value.bytes).toBe(expectedWorktrees + expectedArtifacts);

    // The bottom bar and the plan page must report the same figure.
    expect(state.selectedSummary.value.bytes).toBe(state.cleanupPlan.value.total_bytes);
  });
});

describe("high-risk confirmation threshold (D13)", () => {
  it("requires confirmation for exactly five independently deleted items", async () => {
    const state = await freshState();
    state.snapshot.value = mockSnapshot;
    state.settings.whitelist_paths = [];
    state.selectedWorktrees.value = [];
    state.selectedArtifacts.value = [venv, dist, turbo, cursorNodeModules, cursorCoverage];

    const plan = state.cleanupPlan.value;
    const independent =
      plan.worktree_removals.length + plan.trash_moves.filter((t) => t.bundled_with_worktree === null).length;

    expect(independent).toBe(5);
    expect(plan.total_bytes).toBeLessThan(10 * 1024 * 1024 * 1024);
    expect(plan.worktree_removals.some((w) => w.force)).toBe(false);
    expect(plan.ack_reasons).toEqual([messages.plan.ack.count(5)]);
  });

  it("does not require confirmation for four independently deleted items", async () => {
    const state = await freshState();
    state.snapshot.value = mockSnapshot;
    state.settings.whitelist_paths = [];
    state.selectedWorktrees.value = [];
    state.selectedArtifacts.value = [venv, dist, turbo, cursorNodeModules];

    const plan = state.cleanupPlan.value;
    const independent =
      plan.worktree_removals.length + plan.trash_moves.filter((t) => t.bundled_with_worktree === null).length;

    expect(independent).toBe(4);
    expect(plan.ack_reasons).not.toContain(messages.plan.ack.count(4));
    expect(plan.ack_reasons).toEqual([]);
  });

  it("does not count artifacts bundled with a selected worktree toward the threshold", async () => {
    const state = await freshState();
    state.snapshot.value = mockSnapshot;
    state.settings.whitelist_paths = [];
    state.selectedWorktrees.value = [codex, cursor, gone];
    state.selectedArtifacts.value = [
      `${codex}/target`,
      `${codex}/dist`,
      `${cursor}/target`,
      cursorNodeModules,
      cursorCoverage,
      nuxt
    ];

    const plan = state.cleanupPlan.value;
    expect(plan.worktree_removals.map((w) => w.path).sort()).toEqual([codex, cursor, gone].sort());
    expect(plan.trash_moves.length).toBeGreaterThan(0);
    expect(plan.trash_moves.every((t) => t.bundled_with_worktree !== null)).toBe(true);
    expect(plan.total_bytes).toBeLessThan(10 * 1024 * 1024 * 1024);
    expect(plan.ack_reasons).toEqual([]);
  });
});

describe("plan shape and the whitelist", () => {
  it("keeps a whitelist hit in blockers and out of both channels", async () => {
    const state = await freshState();
    state.snapshot.value = mockSnapshot;
    state.settings.whitelist_paths = [venv];
    state.selectedWorktrees.value = [];
    state.selectedArtifacts.value = [venv, dist];

    const plan = state.cleanupPlan.value;
    expect(plan.blockers).toEqual([messages.plan.blocker(venv)]);
    expect(plan.trash_moves.map((t) => t.path)).toEqual([dist]);
    expect(plan.worktree_removals).toEqual([]);
    expect(plan.total_bytes).toBe(48_000_000);
  });

  it("blocks a whole worktree including its bundled artifacts", async () => {
    const state = await freshState();
    state.snapshot.value = mockSnapshot;
    state.settings.whitelist_paths = [codex];
    state.selectedWorktrees.value = [codex];
    state.selectedArtifacts.value = [`${codex}/target`];

    const plan = state.cleanupPlan.value;
    const blocked = new Set(plan.blockers);
    expect(blocked.has(messages.plan.blocker(codex))).toBe(true);
    expect(blocked.has(messages.plan.blocker(`${codex}/target`))).toBe(true);
    expect(plan.worktree_removals.some((w) => w.path === codex)).toBe(false);
    expect(plan.trash_moves.some((t) => t.path === `${codex}/target`)).toBe(false);
  });
});

describe("assess", () => {
  it("returns Review for an empty reason list, matching the scanner", () => {
    expect(assess([]).status).toBe("Review");
  });

  it("uses the most severe reason", () => {
    expect(
      assess([
        { code: "wt.clean", severity: "Safe", text: "clean", evidence: "no changes" },
        { code: "wt.unmerged", severity: "Review", text: "not merged", evidence: "not an ancestor" }
      ]).status
    ).toBe("Review");
    expect(
      assess([
        { code: "wt.unmerged", severity: "Review", text: "not merged", evidence: "not an ancestor" },
        { code: "wt.locked", severity: "Unsafe", text: "locked", evidence: "porcelain marks it locked" }
      ]).status
    ).toBe("Unsafe");
  });
});

describe("path helpers", () => {
  it("normalises separators, repeats, and a trailing separator", () => {
    expect(normalizePathKey("C:\\Users\\u\\\\MySrc\\proj\\")).toBe("C:/Users/u/MySrc/proj");
    expect(normalizePathKey("/Users/andy/MySrc//proj/")).toBe("/Users/andy/MySrc/proj");
    expect(normalizePathKey("/")).toBe("/");
  });

  it("computes the basename for both separators", () => {
    expect(basename("/Users/andy/MySrc/proj")).toBe("proj");
    expect(basename("C:\\Users\\u\\MySrc\\proj")).toBe("proj");
    expect(basename("C:/Users/u/MySrc/proj/")).toBe("proj");
    // The strong-protection switch compares this to the typed project name.
    expect(basename("C:\\Users\\u\\MySrc\\proj")).toBe(basename("/Users/andy/MySrc/proj"));
  });

  it("matches whitelist rules component by component", () => {
    expect(pathMatchesRule("/Users/andy/MySrc/keep-me", "/Users/andy/MySrc/keep-me")).toBe(true);
    expect(pathMatchesRule("/Users/andy/MySrc/keep-me/child", "/Users/andy/MySrc/keep-me")).toBe(true);
    // A prefix must end on a component boundary.
    expect(pathMatchesRule("/Users/andy/MySrc/keep-me-other", "/Users/andy/MySrc/keep-me")).toBe(false);
    expect(pathMatchesRule("/Users/andy/MySrc/keep", "/Users/andy/MySrc/keep-me")).toBe(false);
    // Mixed separators on either side still compare.
    expect(pathMatchesRule("C:\\Users\\u\\proj", "C:/Users/u/proj")).toBe(true);
    expect(pathMatchesRule("C:\\Users\\u\\proj\\dist", "C:/Users/u/proj")).toBe(true);
  });

  it("recognises absolute paths on POSIX and Windows", () => {
    expect(isAbsolutePath("/Users/andy/proj")).toBe(true);
    expect(isAbsolutePath("C:\\Users\\u\\proj")).toBe(true);
    expect(isAbsolutePath("\\\\host\\share\\proj")).toBe(true);
    expect(isAbsolutePath("relative/proj")).toBe(false);
    expect(isAbsolutePath("")).toBe(false);
  });
});

describe("visibleArtifacts", () => {
  it("keeps an artifact whose size could not be measured", async () => {
    const state = await freshState();
    state.snapshot.value = mockSnapshot;
    state.settings.min_artifact_mb = 500;

    const paths = state.visibleArtifacts.value.map((a) => a.path);
    // `.nuxt` timed out during the scan: size_bytes is null, and "cannot be
    // measured" is not the same as "smaller than the threshold".
    expect(paths).toContain(nuxt);
    expect(state.visibleArtifacts.value.find((a) => a.path === nuxt)?.size_bytes).toBeNull();
  });
});

describe("an unmeasurable size travels as unknown", () => {
  it("is selected by default even above the size threshold", async () => {
    const state = await freshState();
    state.snapshot.value = mockSnapshot;
    state.settings.min_artifact_mb = 500;
    state.applyDefaultSelection();

    // Safe, so selected by default; unmeasurable, so the threshold must not hide
    // it — a user cannot weigh an item whose size the scan never measured.
    expect(state.isSelectedArtifact(mainCoverage)).toBe(true);
  });

  it("is counted separately and keeps every total a lower bound", async () => {
    const state = await freshState();
    state.snapshot.value = mockSnapshot;
    state.settings.whitelist_paths = [];
    state.selectedWorktrees.value = [];
    state.selectedArtifacts.value = [venv, mainCoverage];

    const summary = state.selectedSummary.value;
    // venv is measured; coverage contributes nothing to the bytes but one to the
    // unknown count, so the figure is a lower bound.
    expect(summary.bytes).toBe(214_000_000);
    expect(summary.unknown).toBe(1);

    const plan = state.cleanupPlan.value;
    expect(plan.total_bytes).toBe(214_000_000);
    expect(plan.total_bytes_unknown).toBe(1);
    // With an unknown size in the plan the 10 GB threshold cannot be evaluated,
    // so the run is gated on its own reason.
    expect(plan.ack_reasons).toEqual([messages.plan.ack.unmeasurable(1)]);
  });

  it("stays null through execution instead of collapsing to zero", async () => {
    const { state } = await freshStateWithExecutor({
      entries: [
        {
          action: "trash-move",
          target: mainCoverage,
          result: "ok",
          detail: "moved to the system Trash; recoverable from Trash",
          size_bytes: null
        }
      ],
      history: [
        {
          at: new Date().toISOString(),
          action: "trash-move",
          target: mainCoverage,
          reason: "regenerable",
          result: "ok",
          size_bytes: null,
          detail: "moved to the system Trash; recoverable from Trash"
        }
      ],
      log_error: null
    });
    state.snapshot.value = mockSnapshot;
    state.settings.whitelist_paths = [];
    state.selectedWorktrees.value = [];
    state.selectedArtifacts.value = [mainCoverage];
    await state.executeCleanup();
    unmockPlatform();

    const entry = state.lastResult.value.find((item) => item.target === mainCoverage);
    expect(entry?.size_bytes).toBeNull();
    // "0 B freed" would be a measurement that never happened.
    expect(state.resultSummary.value.freed).toBe(0);
    expect(state.resultSummary.value.freed_unknown).toBe(1);
    // The history entry keeps the same value.
    expect(state.history.value[0]?.size_bytes).toBeNull();
  });
});

describe("group selection and blocked execution", () => {
  it("counts only what was really removed", async () => {
    // The executor's answer decides the figures, not the plan. A skipped item removed
    // nothing, a prune frees nothing, and an unmeasurable size is counted rather than
    // summed — reporting "0 B freed" for it would be a measurement that never happened.
    const { state } = await freshStateWithExecutor({
      entries: [
        { action: "trash-move", target: mainCoverage, result: "ok", detail: "moved", size_bytes: 100 },
        {
          action: "trash-move",
          target: `${mockRoot}/went-stale`,
          result: "skipped",
          detail: "changed since the plan was generated",
          size_bytes: 200
        },
        { action: "trash-move", target: venv, result: "ok", detail: "moved", size_bytes: null },
        { action: "prune", target: mockRoot, result: "ok", detail: "pruned", size_bytes: null }
      ],
      history: [],
      log_error: null
    });
    state.snapshot.value = mockSnapshot;
    state.settings.whitelist_paths = [];
    state.selectedWorktrees.value = [];
    state.selectedArtifacts.value = [mainCoverage];
    await state.executeCleanup();
    unmockPlatform();

    const summary = state.resultSummary.value;
    // 100 from the one measured move: the skipped item's 200 was not freed.
    expect(summary.freed).toBe(100);
    // Two things were moved to the Trash, one of them with an unknown size.
    expect(summary.trashed).toBe(2);
    expect(summary.failed).toBe(1);
    expect(summary.freed_unknown).toBe(1);
  });

  it("refuses Unsafe and bundled items in a group toggle", async () => {
    const state = await freshState();
    state.snapshot.value = mockSnapshot;
    // The owning worktree is selected, so its artifacts are bundled with it.
    state.selectedWorktrees.value = [codex];
    state.toggleGroupArtifacts([trackedBuild, `${codex}/target`, venv], true);

    expect(state.isSelectedArtifact(trackedBuild)).toBe(false);
    expect(state.isSelectedArtifact(`${codex}/target`)).toBe(false);
    expect(state.isSelectedArtifact(venv)).toBe(true);
  });

  it("carries the path as given, never what a symlink points at", async () => {
    const state = await freshState();
    // This artifact is a symlink to a shared package store. Moving the store instead
    // of the link would take every project that shares it, so the plan has to name the
    // link — and the target stays evidence to show, not a path to act on.
    const link = `${mockRoot}/apps/desktop/node_modules`;
    const source = mockSnapshot.artifacts.find((a) => a.path === link);
    expect(source?.is_symlink, "the fixture is a symlink").toBe(true);
    expect(source?.symlink_target).toBeTruthy();

    state.snapshot.value = {
      ...mockSnapshot,
      artifacts: [
        ...mockSnapshot.artifacts.filter((a) => a.path !== link),
        {
          ...source!,
          // Deletable, so it reaches the plan at all.
          safety: assess([
            {
              code: "af.empty",
              severity: "Safe",
              text: "the directory is empty",
              evidence: "it contains no files"
            }
          ])
        }
      ]
    };
    state.settings.whitelist_paths = [];
    state.selectedWorktrees.value = [];
    state.selectedArtifacts.value = [link];

    const item = state.cleanupPlan.value.trash_moves.find((move) => move.path === link);
    expect(item, "the link is in the plan as given").toBeDefined();
    expect(item?.path).not.toBe(source?.symlink_target);
  });

  it("skips a whitelist-blocked item instead of executing it", async () => {
    // A blocked item never reaches the executor: it is filtered out of both
    // channels before the plan is built, and the result page adds it back as its
    // own skipped row. The executor's answer below is therefore empty.
    const { state, plans } = await freshStateWithExecutor({
      entries: [],
      history: [],
      log_error: null
    });
    state.snapshot.value = mockSnapshot;
    state.settings.whitelist_paths = [venv];
    state.selectedWorktrees.value = [];
    state.selectedArtifacts.value = [venv];
    await state.executeCleanup();
    unmockPlatform();

    const plan = plans[0] as { trash_moves: unknown[]; blockers: string[] };
    expect(plan.trash_moves, "a protected path is not in the executable channel").toEqual([]);
    expect(plan.blockers).toHaveLength(1);

    expect(state.lastResult.value).toHaveLength(1);
    expect(state.lastResult.value[0]?.result).toBe("skipped");
    expect(state.lastResult.value[0]?.target).toBe(venv);
    // Skipped counts toward the failure figure and frees nothing.
    expect(state.resultSummary.value.failed).toBe(1);
    expect(state.resultSummary.value.freed).toBe(0);
  });
});

describe("primaryReason", () => {
  it("surfaces the most severe reason", () => {
    const primary = primaryReason([
      { code: "wt.clean", severity: "Safe", text: "clean", evidence: "no changes" },
      { code: "wt.unmerged", severity: "Review", text: "not merged", evidence: "not an ancestor" },
      { code: "wt.locked", severity: "Unsafe", text: "locked", evidence: "porcelain marks it locked" }
    ]);
    expect(primary?.code).toBe("wt.locked");
  });
});

describe("canOpen", () => {
  it("gates each view on the data it needs", async () => {
    const state = await freshState();
    expect(state.canOpen("welcome")).toBe(true);
    expect(state.canOpen("settings")).toBe(true);
    // Without a scan there is nothing to show; without a run there is no result.
    expect(state.canOpen("overview")).toBe(false);
    expect(state.canOpen("result")).toBe(false);

    state.snapshot.value = mockSnapshot;
    expect(state.canOpen("overview")).toBe(true);
    expect(state.canOpen("result")).toBe(false);

    state.lastResult.value = [
      { action: "prune", target: mockRoot, result: "ok", detail: "prune", size_bytes: 0 }
    ];
    expect(state.canOpen("result")).toBe(true);
  });
});
