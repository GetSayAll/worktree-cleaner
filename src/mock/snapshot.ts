import type {
  ActivityEvent,
  ArtifactRecord,
  DetailArg,
  HistoryEntry,
  HygieneFinding,
  HygieneLevel,
  HygieneReport,
  HygieneRule,
  HygieneRuleStatus,
  ProjectSnapshot,
  Reason,
  WorktreeRecord
} from "../types";
import { assess, SNAPSHOT_SCHEMA_VERSION } from "../types";

const DAY = 86400_000;

function daysAgo(days: number): string {
  return new Date(Date.now() - days * DAY).toISOString();
}

export const mockRoot = "/Users/andy/MySrc/remote-mic-app";

// Scan output, i.e. data rather than UI chrome: the scanner produces the warning
// text and it travels with the record (development contract D23 / section 3).
// Only the "Scan incomplete · " label in front of it lives in src/i18n/en.ts.
const mockScanWarning =
  "Scanning the .nuxt directory under worktree cursor/perf-audio timed out; the size is recorded as unknown and treated as Needs review.";

// The same walk timing out on a *different* directory must not downgrade the
// safety verdict: coverage is git-ignored and regenerable, so it stays Safe while
// only its size stays unknown. "Unmeasurable" and "unsafe" are independent axes.
const mockCoverageWarning =
  "The size of coverage under the main worktree timed out; only the size is unknown, the rebuildable judgement stands and the item stays Safe to delete.";

// ---- sample policy audit ----------------------------------------------------
//
// The browser preview has no repository to audit, so this is the report a real
// scan of this layout would produce: four worktrees beside the repository rather
// than under `.worktree/`, one of them past the 30-day ceiling, build output next
// to the manifests, and a volume two points above the freeze line. The roll-up is
// computed from the findings here because sample data has to stay self-consistent;
// the real roll-up happens in `crates/worktree-core/src/hygiene.rs` and the front
// end never derives a level of its own.

const HYGIENE_ORDER: HygieneRule[] = [
  "L1",
  "L2",
  "L3",
  "L4",
  "L5",
  "L6",
  "L7",
  "T1",
  "T2",
  "T3",
  "T4",
  "T5",
  "B1",
  "B1.1",
  "B2",
  "B3",
  "B4",
  "B5",
  "Q1",
  "Q2",
  "Q3",
  "X1"
];

function hygieneFinding(
  rule: HygieneRule,
  level: HygieneLevel,
  subject: string,
  evidence: string,
  args: Record<string, DetailArg>,
  waivedUntil?: string
): HygieneFinding {
  return {
    rule,
    level,
    subject,
    evidence,
    args,
    ...(waivedUntil === undefined ? {} : { waived_until: waivedUntil })
  };
}

function hygieneRules(findings: HygieneFinding[]): HygieneRuleStatus[] {
  const rank: Record<HygieneLevel, number> = { ok: 0, unknown: 1, advisory: 2, violation: 3 };
  return HYGIENE_ORDER.map((rule) => {
    const mine = findings.filter((finding) => finding.rule === rule);
    const level = mine.reduce<HygieneLevel>(
      (worst, finding) => (rank[finding.level] > rank[worst] ? finding.level : worst),
      "ok"
    );
    return { rule, level, findings: mine.length };
  });
}

function notChecked(rule: HygieneRule, evidence: string): HygieneFinding {
  return hygieneFinding(rule, "unknown", mockRoot, evidence, { state: "not_checkable" });
}

const mockHygieneFindings: HygieneFinding[] = [
  hygieneFinding("L1", "violation", `${mockRoot}--cursor-perf`, `declared root: ${mockRoot}/.worktree`, {
    state: "outside_repository",
    expected: `${mockRoot}/.worktree`
  }),
  hygieneFinding("L1", "violation", `${mockRoot}--codex-fix-ble`, `declared root: ${mockRoot}/.worktree`, {
    state: "outside_repository",
    expected: `${mockRoot}/.worktree`
  }),
  hygieneFinding("L1", "violation", `${mockRoot}--claude-ui`, `declared root: ${mockRoot}/.worktree`, {
    state: "outside_repository",
    expected: `${mockRoot}/.worktree`
  }),
  hygieneFinding("T4", "violation", `${mockRoot}--cursor-perf`, `directory created at ${daysAgo(41)}`, {
    state: "past_ceiling",
    days: 41,
    ceiling: 30
  }),
  hygieneFinding("B1", "violation", `${mockRoot}/node_modules`, `not under ${mockRoot}/build-cache/: node_modules`, {
    state: "outside_declared",
    kind: "node_modules"
  }),
  // The one rule whose id carries a dot. It is in the sample on purpose: the
  // message table cannot hold a dotted key, so this is the id most likely to
  // render as a bare `B1.1` with no sentence beside it.
  hygieneFinding("B1.1", "advisory", `${mockRoot}/target`, "git status --porcelain: ?? target/", {
    state: "untracked_in_status",
    kind: "target"
  }),
  hygieneFinding(
    "L5",
    "violation",
    mockRoot,
    ".git/info/exclude:3: /.worktree/ (for .worktree/__probe__)",
    {
      state: "local_only",
      entry: "/.worktree/",
      pattern: "/.worktree/",
      source: ".git/info/exclude",
      line: 3
    }
  ),
  hygieneFinding("Q1", "advisory", mockRoot, "57 GB of 494 GB available (12%)", {
    state: "warning",
    percent: 12,
    available: 57_000_000_000,
    total: 494_000_000_000
  }),
  hygieneFinding("T2", "advisory", `${mockRoot}--claude-ui`, "merged into the default branch: claude/ui", {
    state: "merged_still_present",
    branch: "claude/ui"
  }),
  hygieneFinding("T3", "advisory", `${mockRoot}--cursor-perf`, `last commit at ${daysAgo(9)}`, {
    state: "stale",
    days: 9,
    dirty: 0,
    untracked: 0
  }),
  hygieneFinding("B2", "advisory", `${mockRoot}/build-cache`, "remote-mic-app--claude-ui, remote-mic-app--cursor-perf", {
    state: "missing_slots",
    count: 2,
    slots: "remote-mic-app--claude-ui, remote-mic-app--cursor-perf"
  }),
  hygieneFinding("X1", "advisory", `${mockRoot}/.claude/worktrees/payout-legal-name`, ".worktree-keep:4: expired 2026-09-30", {
    state: "dangling",
    line: 4
  }),
  // One accepted exception, so the page shows what a waiver looks like: the path
  // stays, the reason is on record, and the date it stops applying is visible.
  hygieneFinding(
    "B1",
    "ok",
    `${mockRoot}/coverage`,
    `not under ${mockRoot}/build-cache/: coverage`,
    { state: "outside_declared", kind: "coverage" },
    "2026-12-31"
  ),
  notChecked("L2", "a second clone of this repository, if one exists, is not inside the scanned path"),
  notChecked("L6", "git clean is not logged, so no scan can see a command that has already run"),
  notChecked("L7", "editor search and platform indexing are configured outside the repository"),
  notChecked("T5", "a session's exit is not recorded anywhere a scan can read"),
  notChecked("B3", "which dependencies a build shares is decided by its own configuration"),
  notChecked("B4", "whether something reads the cache as an input cannot be established by reading it"),
  notChecked("Q3", "admission runs before creation; a scan only sees what already exists")
];

const mockHygiene: HygieneReport = {
  rules: hygieneRules(mockHygieneFindings),
  findings: mockHygieneFindings,
  warnings: []
};

export const mockSnapshot: ProjectSnapshot = {
  // Never hand-write this number: the Rust core defines the same constant
  // (crates/worktree-core/src/lib.rs) and src/architecture.test.ts fails when the
  // two drift apart.
  schema_version: SNAPSHOT_SCHEMA_VERSION,
  root: mockRoot,
  scanned_at: new Date().toISOString(),
  default_branch: "main",
  warnings: [mockScanWarning, mockCoverageWarning],
  worktrees: [
    {
      path: mockRoot,
      branch: "main",
      head: "7c41d9a",
      is_main: true,
      is_detached: false,
      is_locked: false,
      is_prunable: false,
      dirty_files: 0,
      untracked_files: 0,
      untracked_file_list: [],
      ahead: 0,
      behind: 0,
      has_upstream: true,
      merged_into_default: true,
      stash_count: 1,
      last_updated_at: daysAgo(0),
      agent: null,
      size_bytes: 486_000_000,
      safety: assess([
        { code: "wt.main", severity: "Unsafe", text: "This is the main worktree", evidence: "first entry in git worktree list" },
        { code: "wt.clean", severity: "Safe", text: "clean working tree", evidence: "no uncommitted changes" }
      ])
    },
    {
      path: `${mockRoot}--codex-fix-ble`,
      branch: "codex/fix-ble-reconnect",
      head: "b83e210",
      is_main: false,
      is_detached: false,
      is_locked: false,
      is_prunable: false,
      dirty_files: 0,
      untracked_files: 0,
      untracked_file_list: [],
      ahead: 0,
      behind: 0,
      has_upstream: true,
      merged_into_default: true,
      stash_count: 1,
      last_updated_at: daysAgo(26),
      agent: "codex",
      size_bytes: 1_900_000_000,
      safety: assess([
        { code: "wt.clean", severity: "Safe", text: "clean working tree", evidence: "no uncommitted changes, no untracked files" },
        { code: "wt.merged", severity: "Safe", text: "merged into main", evidence: "merge-base determines it is an ancestor" }
      ])
    },
    {
      path: `${mockRoot}--claude-ui`,
      branch: "claude/ui-refresh",
      head: "41ab7c3",
      is_main: false,
      is_detached: false,
      is_locked: false,
      is_prunable: false,
      dirty_files: 3,
      untracked_files: 2,
      untracked_file_list: ["src/components/AudioMeter.vue.bak", "notes/scratch.md"],
      ahead: 2,
      behind: 0,
      has_upstream: true,
      merged_into_default: false,
      stash_count: 1,
      last_updated_at: daysAgo(1),
      agent: "claude",
      size_bytes: 1_100_000_000,
      safety: assess([
        { code: "wt.dirty", severity: "Unsafe", text: "uncommitted changes", evidence: "3 files have uncommitted changes" },
        { code: "wt.unpushed", severity: "Unsafe", text: "unpushed commits", evidence: "2 commits not pushed" },
        { code: "wt.unmerged", severity: "Review", text: "not merged into main", evidence: "merge-base determines it is not an ancestor" }
      ])
    },
    {
      path: `${mockRoot}--cursor-perf`,
      branch: "cursor/perf-audio",
      head: "9de04f1",
      is_main: false,
      is_detached: false,
      is_locked: false,
      is_prunable: false,
      dirty_files: 0,
      untracked_files: 0,
      untracked_file_list: [],
      ahead: 0,
      behind: 0,
      has_upstream: false,
      merged_into_default: false,
      stash_count: 1,
      last_updated_at: daysAgo(2),
      agent: "cursor",
      size_bytes: 2_400_000_000,
      safety: assess([
        { code: "wt.unmerged", severity: "Review", text: "not merged into main", evidence: "merge-base determines it is not an ancestor" },
        { code: "wt.recent_activity", severity: "Review", text: "changed within the last 7 days", evidence: "last activity: 2 days ago" }
      ])
    },
    {
      path: `${mockRoot}--review-locked`,
      branch: "review/audio-regression",
      head: "2f8b5c0",
      is_main: false,
      is_detached: false,
      is_locked: true,
      is_prunable: false,
      dirty_files: 0,
      untracked_files: 4,
      untracked_file_list: [
        "fixtures/loop-48k.wav",
        "fixtures/loop-44k.wav",
        "fixtures/noise.wav",
        "scripts/repro.sh"
      ],
      ahead: 0,
      behind: 1,
      has_upstream: true,
      merged_into_default: false,
      stash_count: 1,
      last_updated_at: daysAgo(11),
      agent: null,
      size_bytes: 640_000_000,
      safety: assess([
        { code: "wt.locked", severity: "Unsafe", text: "locked by Git", evidence: "porcelain marks it as locked" },
        { code: "wt.untracked", severity: "Review", text: "untracked files", evidence: "4 untracked files" }
      ])
    },
    {
      path: `${mockRoot}--wt-gone`,
      branch: "spike/old-encoder",
      head: "1a7f002",
      is_main: false,
      is_detached: false,
      is_locked: false,
      is_prunable: true,
      dirty_files: 0,
      untracked_files: 0,
      untracked_file_list: [],
      ahead: 0,
      behind: 0,
      has_upstream: true,
      merged_into_default: true,
      stash_count: 1,
      last_updated_at: daysAgo(58),
      agent: null,
      size_bytes: 0,
      safety: assess([
        {
          code: "wt.prunable",
          severity: "Safe",
          text: "working directory no longer exists",
          evidence: "only cleans .git/worktrees metadata"
        }
      ])
    }
  ] as WorktreeRecord[],
  artifacts: [
    {
      path: `${mockRoot}/node_modules`,
      kind: "node_modules",
      ecosystem: "node",
      project_root: mockRoot,
      worktree_path: mockRoot,
      size_bytes: 412_000_000,
      file_count: 18240,
      last_updated_at: daysAgo(3),
      git_ignored: true,
      git_tracked: false,
      is_symlink: false,
      symlink_target: null,
      safety: assess([
        { code: "af.rebuildable", severity: "Safe", text: "rebuildable with npm install", evidence: "package-lock.json exists" },
        { code: "af.git_ignored", severity: "Safe", text: "ignored by .gitignore", evidence: ".gitignore:12" },
        { code: "af.recent_activity", severity: "Review", text: "changed within the last 7 days", evidence: "last activity: 3 days ago" }
      ])
    },
    {
      path: `${mockRoot}/.venv`,
      kind: ".venv",
      ecosystem: "python",
      project_root: mockRoot,
      worktree_path: mockRoot,
      size_bytes: 214_000_000,
      file_count: 9860,
      last_updated_at: daysAgo(44),
      git_ignored: true,
      git_tracked: false,
      is_symlink: false,
      symlink_target: null,
      safety: assess([
        { code: "af.rebuildable", severity: "Safe", text: "rebuildable with uv sync", evidence: "pyproject.toml exists" },
        { code: "af.git_ignored", severity: "Safe", text: "ignored by .gitignore", evidence: ".gitignore:24" }
      ])
    },
    {
      path: `${mockRoot}/dist`,
      kind: "dist",
      ecosystem: "node",
      project_root: mockRoot,
      worktree_path: mockRoot,
      size_bytes: 48_000_000,
      file_count: 210,
      last_updated_at: daysAgo(31),
      git_ignored: true,
      git_tracked: false,
      is_symlink: false,
      symlink_target: null,
      safety: assess([
        { code: "af.rebuildable", severity: "Safe", text: "rebuildable with npm run build", evidence: "scripts.build exists" },
        { code: "af.git_ignored", severity: "Safe", text: "ignored by .gitignore", evidence: ".gitignore:6" }
      ])
    },
    {
      path: `${mockRoot}/.turbo`,
      kind: ".turbo",
      ecosystem: "node",
      project_root: mockRoot,
      worktree_path: mockRoot,
      size_bytes: 96_000_000,
      file_count: 1204,
      last_updated_at: daysAgo(61),
      git_ignored: true,
      git_tracked: false,
      is_symlink: false,
      symlink_target: null,
      safety: assess([
        { code: "af.rebuildable", severity: "Safe", text: "build cache, can be regenerated", evidence: "turbo.json exists" },
        { code: "af.git_ignored", severity: "Safe", text: "ignored by .gitignore", evidence: ".gitignore:9" }
      ])
    },
    {
      path: `${mockRoot}/crates/remote-mic-core/build`,
      kind: "build",
      ecosystem: "rust",
      project_root: mockRoot,
      worktree_path: mockRoot,
      size_bytes: 6_200_000,
      file_count: 23,
      last_updated_at: daysAgo(70),
      git_ignored: false,
      git_tracked: true,
      is_symlink: false,
      symlink_target: null,
      safety: assess([
        {
          code: "af.git_tracked",
          severity: "Unsafe",
          text: "the directory contains files committed to the repository",
          evidence: "git ls-files matches 23 files, so it is not a pure build artifact"
        }
      ])
    },
    {
      path: `${mockRoot}/apps/desktop/node_modules`,
      kind: "node_modules",
      ecosystem: "node",
      project_root: mockRoot,
      worktree_path: mockRoot,
      size_bytes: null,
      file_count: 0,
      last_updated_at: daysAgo(12),
      git_ignored: true,
      git_tracked: false,
      is_symlink: true,
      symlink_target: "/Users/andy/toolchains/pnpm-store/node_modules",
      safety: assess([
        {
          code: "af.symlink_outside",
          severity: "Unsafe",
          text: "symlink pointing outside the project",
          evidence: "target /Users/andy/toolchains/pnpm-store/node_modules is outside the project root; deleting it would damage a shared directory"
        }
      ])
    },
    {
      path: `${mockRoot}--codex-fix-ble/target`,
      kind: "target",
      ecosystem: "rust",
      project_root: `${mockRoot}--codex-fix-ble`,
      worktree_path: `${mockRoot}--codex-fix-ble`,
      size_bytes: 3_100_000_000,
      file_count: 24880,
      last_updated_at: daysAgo(22),
      git_ignored: true,
      git_tracked: false,
      is_symlink: false,
      symlink_target: null,
      safety: assess([
        { code: "af.rebuildable", severity: "Safe", text: "rebuildable with cargo build", evidence: "Cargo.toml exists" },
        { code: "af.git_ignored", severity: "Safe", text: "ignored by .gitignore", evidence: ".gitignore:1" }
      ])
    },
    {
      path: `${mockRoot}--codex-fix-ble/dist`,
      kind: "dist",
      ecosystem: "node",
      project_root: `${mockRoot}--codex-fix-ble`,
      worktree_path: `${mockRoot}--codex-fix-ble`,
      size_bytes: 62_000_000,
      file_count: 264,
      last_updated_at: daysAgo(25),
      git_ignored: true,
      git_tracked: false,
      is_symlink: false,
      symlink_target: null,
      safety: assess([
        { code: "af.rebuildable", severity: "Safe", text: "rebuildable with npm run build", evidence: "scripts.build exists" },
        { code: "af.git_ignored", severity: "Safe", text: "ignored by .gitignore", evidence: ".gitignore:6" }
      ])
    },
    {
      path: `${mockRoot}--claude-ui/node_modules`,
      kind: "node_modules",
      ecosystem: "node",
      project_root: `${mockRoot}--claude-ui`,
      worktree_path: `${mockRoot}--claude-ui`,
      size_bytes: 388_000_000,
      file_count: 17910,
      last_updated_at: daysAgo(1),
      git_ignored: true,
      git_tracked: false,
      is_symlink: false,
      symlink_target: null,
      safety: assess([
        { code: "af.rebuildable", severity: "Safe", text: "rebuildable with npm install", evidence: "package-lock.json exists" },
        { code: "af.git_ignored", severity: "Safe", text: "ignored by .gitignore", evidence: ".gitignore:12" },
        { code: "af.recent_activity", severity: "Review", text: "changed within the last 7 days", evidence: "last activity: 1 day ago" }
      ])
    },
    {
      path: `${mockRoot}--claude-ui/.next`,
      kind: ".next",
      ecosystem: "node",
      project_root: `${mockRoot}--claude-ui`,
      worktree_path: `${mockRoot}--claude-ui`,
      size_bytes: 178_000_000,
      file_count: 3410,
      last_updated_at: daysAgo(1),
      git_ignored: true,
      git_tracked: false,
      is_symlink: false,
      symlink_target: null,
      safety: assess([
        { code: "af.rebuildable", severity: "Safe", text: "rebuildable with next build", evidence: "next.config.ts exists" },
        { code: "af.git_ignored", severity: "Safe", text: "ignored by .gitignore", evidence: ".gitignore:8" },
        { code: "af.recent_activity", severity: "Review", text: "changed within the last 7 days", evidence: "last activity: 1 day ago" }
      ])
    },
    {
      // The longest path shape a real scan produces: an agent's nested worktree
      // directory, whose relative path is long enough to have been the one value
      // on the list pages that could not be shortened. The layout checks read it,
      // so a row that wraps its name again fails `pnpm test:layout` rather than
      // waiting to be seen on a screen.
      path: `${mockRoot}/.claude/worktrees/payout-legal-name/node_modules`,
      kind: "node_modules",
      ecosystem: "node",
      project_root: mockRoot,
      worktree_path: null,
      size_bytes: 1_350_000_000,
      file_count: 83_401,
      last_updated_at: daysAgo(34),
      git_ignored: true,
      git_tracked: false,
      is_symlink: false,
      symlink_target: null,
      safety: assess([
        { code: "af.rebuildable", severity: "Safe", text: "rebuildable with npm install", evidence: "package-lock.json exists" },
        { code: "af.git_ignored", severity: "Safe", text: "ignored by .gitignore", evidence: ".gitignore:72" }
      ])
    },
    {
      path: `${mockRoot}--cursor-perf/target`,
      kind: "target",
      ecosystem: "rust",
      project_root: `${mockRoot}--cursor-perf`,
      worktree_path: `${mockRoot}--cursor-perf`,
      size_bytes: 2_200_000_000,
      file_count: 21340,
      last_updated_at: daysAgo(2),
      git_ignored: true,
      git_tracked: false,
      is_symlink: false,
      symlink_target: null,
      safety: assess([
        { code: "af.rebuildable", severity: "Safe", text: "rebuildable with cargo build", evidence: "Cargo.toml exists" },
        { code: "af.git_ignored", severity: "Safe", text: "ignored by .gitignore", evidence: ".gitignore:1" },
        { code: "af.recent_activity", severity: "Review", text: "changed within the last 7 days", evidence: "last activity: 2 days ago" }
      ])
    },
    {
      path: `${mockRoot}--cursor-perf/node_modules`,
      kind: "node_modules",
      ecosystem: "node",
      project_root: `${mockRoot}--cursor-perf`,
      worktree_path: `${mockRoot}--cursor-perf`,
      size_bytes: 401_000_000,
      file_count: 18020,
      last_updated_at: daysAgo(9),
      git_ignored: true,
      git_tracked: false,
      is_symlink: false,
      symlink_target: null,
      safety: assess([
        { code: "af.rebuildable", severity: "Safe", text: "rebuildable with npm install", evidence: "package-lock.json exists" },
        { code: "af.git_ignored", severity: "Safe", text: "ignored by .gitignore", evidence: ".gitignore:12" }
      ])
    },
    {
      path: `${mockRoot}--cursor-perf/coverage`,
      kind: "coverage",
      ecosystem: "node",
      project_root: `${mockRoot}--cursor-perf`,
      worktree_path: `${mockRoot}--cursor-perf`,
      size_bytes: 12_000_000,
      file_count: 84,
      last_updated_at: daysAgo(45),
      git_ignored: true,
      git_tracked: false,
      is_symlink: false,
      symlink_target: null,
      safety: assess([
        { code: "af.rebuildable", severity: "Safe", text: "can be regenerated by running tests", evidence: "vitest --coverage" },
        { code: "af.git_ignored", severity: "Safe", text: "ignored by .gitignore", evidence: ".gitignore:15" }
      ])
    },
    {
      path: `${mockRoot}--cursor-perf/.nuxt`,
      kind: ".nuxt",
      ecosystem: "node",
      project_root: `${mockRoot}--cursor-perf`,
      worktree_path: `${mockRoot}--cursor-perf`,
      size_bytes: null,
      file_count: 0,
      last_updated_at: null,
      git_ignored: true,
      git_tracked: null,
      is_symlink: false,
      symlink_target: null,
      safety: assess([
        {
          code: "af.cannot_verify",
          severity: "Review",
          text: "could not complete the check",
          evidence: "size scan timed out (30s); cannot confirm whether it contains committed files"
        }
      ])
    },
    {
      // Safe to delete, but the size walk timed out: `size_bytes` stays null so
      // the UI shows "unknown" and every total that includes it is a lower bound.
      path: `${mockRoot}/coverage`,
      kind: "coverage",
      ecosystem: "node",
      project_root: mockRoot,
      worktree_path: mockRoot,
      size_bytes: null,
      file_count: 0,
      last_updated_at: daysAgo(38),
      git_ignored: true,
      git_tracked: false,
      is_symlink: false,
      symlink_target: null,
      safety: assess([
        { code: "af.rebuildable", severity: "Safe", text: "can be regenerated by running tests", evidence: "vitest --coverage" },
        { code: "af.git_ignored", severity: "Safe", text: "ignored by .gitignore", evidence: ".gitignore:15" }
      ])
    }
  ] as ArtifactRecord[],
  hygiene: mockHygiene
};

/** Sample user actions for the browser preview, newest last like the real log. */
export const mockActivity: ActivityEvent[] = [
  {
    at: "2026-10-12T09:04:11.000Z",
    action: "scan.started",
    target: mockRoot,
    args: {}
  },
  {
    at: "2026-10-12T09:04:13.000Z",
    action: "scan.finished",
    target: mockRoot,
    args: { worktrees: 5, artifacts: 14, sample: true }
  },
  {
    at: "2026-10-12T09:05:02.000Z",
    action: "settings.changed",
    target: "whitelist_paths",
    args: { before: "", after: "~/Archive" }
  },
  {
    at: "2026-10-12T09:05:40.000Z",
    action: "plan.generated",
    target: mockRoot,
    args: { worktrees: 2, artifacts: 12 }
  },
  {
    at: "2026-10-12T09:06:01.000Z",
    action: "cleanup.confirmed",
    target: mockRoot,
    args: { worktrees: 2, artifacts: 12, forced: 1, typed_name: true }
  }
];

export const mockHistory: HistoryEntry[] = [
  {
    at: daysAgo(4),
    action: "trash-move",
    target: `${mockRoot}--codex-fix-ble/target/debug/incremental`,
    reason_code: "af.rebuildable", reason: null,
    result: "ok",
    size_bytes: 812_000_000,
    detail_code: "artifact.trashed", detail_args: {}, detail: null
  },
  {
    at: daysAgo(4),
    action: "worktree-remove",
    target: `${mockRoot}--codex-old-spike`,
    reason_code: "wt.clean", reason: null,
    result: "ok",
    size_bytes: 1_240_000_000,
    detail_code: "worktree.removed", detail_args: {}, detail: null
  },
  {
    at: daysAgo(4),
    action: "trash-move",
    target: `${mockRoot}/apps/desktop/node_modules`,
    reason_code: "af.symlink_outside", reason: null,
    result: "skipped",
    size_bytes: 0,
    detail_code: "precondition.failed", detail_args: {}, detail: null
  },
  {
    at: daysAgo(11),
    action: "trash-move",
    target: `${mockRoot}/crates/remote-mic-core/build`,
    reason_code: "af.rebuildable", reason: null,
    result: "failed",
    size_bytes: 6_200_000,
    detail_code: "artifact.no_longer_safe",
      detail_args: { reason: "af.git_tracked" },
      detail: null
  }
];

export const scanTrace = [
  // The real scanner parses the NUL-separated form: a line-based parse would
  // truncate a path containing a newline and report a live worktree as gone.
  "git worktree list --porcelain -z",
  "status --porcelain=v2 --branch @ remote-mic-app--claude-ui",
  "merge-base --is-ancestor 9de04f1 main @ remote-mic-app--cursor-perf",
  "stash list --format=%gd @ remote-mic-app → 1 entry, repository-wide",
  "scan remote-mic-app--codex-fix-ble/target …",
  "scan remote-mic-app--cursor-perf/target …",
  "check-ignore remote-mic-app/.turbo",
  "ls-files crates/remote-mic-core/build → 23 tracked files",
  "readlink apps/desktop/node_modules → /Users/andy/toolchains/pnpm-store/node_modules",
  "activity re-check: remote-mic-app--cursor-perf/target last activity 2 days ago"
];
