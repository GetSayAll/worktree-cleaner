import type {
  ArtifactRecord,
  HistoryEntry,
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
      stash_count: 0,
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
      stash_count: 0,
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
      stash_count: 0,
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
        { code: "wt.stash", severity: "Review", text: "has stash entries", evidence: "1 stash entry" },
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
      stash_count: 0,
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
      stash_count: 0,
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
  ] as ArtifactRecord[]
};

export const mockHistory: HistoryEntry[] = [
  {
    at: daysAgo(4),
    action: "trash-move",
    target: `${mockRoot}--codex-fix-ble/target/debug/incremental`,
    reason: "Matched a rebuildable artifact type · unchanged for over 30 days",
    result: "ok",
    size_bytes: 812_000_000,
    detail: "Moved to Trash, recoverable"
  },
  {
    at: daysAgo(4),
    action: "worktree-remove",
    target: `${mockRoot}--codex-old-spike`,
    reason: "clean working tree · merged into main",
    result: "ok",
    size_bytes: 1_240_000_000,
    detail: "git worktree remove succeeded, then prune cleaned up metadata"
  },
  {
    at: daysAgo(4),
    action: "trash-move",
    target: `${mockRoot}/apps/desktop/node_modules`,
    reason: "symlink pointing outside the project",
    result: "skipped",
    size_bytes: 0,
    detail: "path.symlink_outside check failed; not executed"
  },
  {
    at: daysAgo(11),
    action: "trash-move",
    target: `${mockRoot}/crates/remote-mic-core/build`,
    reason: "suspected build artifact",
    result: "failed",
    size_bytes: 6_200_000,
    detail: "git ls-files found 23 committed files; aborted. Confirm manually before retrying"
  }
];

export const scanTrace = [
  // The real scanner parses the NUL-separated form: a line-based parse would
  // truncate a path containing a newline and report a live worktree as gone.
  "git worktree list --porcelain -z",
  "status --porcelain=v2 --branch @ remote-mic-app--claude-ui",
  "merge-base --is-ancestor 9de04f1 main @ remote-mic-app--cursor-perf",
  "stash list --format=%gd @ remote-mic-app--cursor-perf",
  "scan remote-mic-app--codex-fix-ble/target …",
  "scan remote-mic-app--cursor-perf/target …",
  "check-ignore remote-mic-app/.turbo",
  "ls-files crates/remote-mic-core/build → 23 tracked files",
  "readlink apps/desktop/node_modules → /Users/andy/toolchains/pnpm-store/node_modules",
  "activity re-check: remote-mic-app--cursor-perf/target last activity 2 days ago"
];
