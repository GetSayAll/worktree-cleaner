import { describe, expect, it, vi } from "vitest";
import { mockRoot, mockSnapshot } from "./mock/snapshot";
import { assess } from "./types";
import { basename, isAbsolutePath, normalizePathKey, pathMatchesRule } from "./format";
import { messages } from "./i18n";

// The state module keeps mutable refs at module scope, so every test imports a
// fresh copy: `vi.resetModules()` clears the registry and the dynamic import below
// re-evaluates it. Nothing leaks from one test into the next.
async function freshState(): Promise<typeof import("./state")> {
  vi.resetModules();
  return await import("./state");
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
      .reduce((sum, w) => sum + w.size_bytes, 0);
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
