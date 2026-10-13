export type SafetyStatus = "Safe" | "Review" | "Unsafe";

export type ReasonCode =
  | "wt.main"
  | "wt.locked"
  | "wt.dirty"
  | "wt.unpushed"
  | "wt.detached"
  | "wt.no_upstream"
  | "wt.unmerged"
  | "wt.untracked"
  | "wt.stash"
  | "wt.recent_activity"
  | "wt.agent_active"
  | "wt.cannot_verify"
  | "wt.whitelisted"
  | "wt.prunable"
  | "wt.merged"
  | "wt.clean"
  | "af.git_tracked"
  | "af.nested_git"
  | "af.authored_content"
  | "af.symlink_outside"
  | "af.whitelisted"
  | "af.recent_activity"
  | "af.cannot_verify"
  | "af.empty"
  | "af.git_ignored"
  | "af.rebuildable";

export interface Reason {
  code: ReasonCode;
  severity: SafetyStatus;
  text: string;
  evidence?: string | null;
}

export interface SafetyAssessment {
  status: SafetyStatus;
  reasons: Reason[];
}

export interface WorktreeRecord {
  path: string;
  branch: string | null;
  head: string | null;
  is_main: boolean;
  is_detached: boolean;
  is_locked: boolean;
  is_prunable: boolean;
  dirty_files: number;
  untracked_files: number;
  untracked_file_list: string[];
  ahead: number;
  behind: number;
  has_upstream: boolean;
  merged_into_default: boolean | null;
  stash_count: number;
  last_updated_at: string | null;
  agent: string | null;
  size_bytes: number;
  safety: SafetyAssessment;
}

export interface ArtifactRecord {
  path: string;
  kind: string;
  ecosystem: string;
  project_root: string;
  worktree_path: string | null;
  size_bytes: number | null;
  file_count: number;
  last_updated_at: string | null;
  git_ignored: boolean | null;
  git_tracked: boolean | null;
  is_symlink: boolean;
  symlink_target: string | null;
  safety: SafetyAssessment;
}

export interface ProjectSnapshot {
  schema_version: number;
  root: string;
  scanned_at: string;
  default_branch: string | null;
  worktrees: WorktreeRecord[];
  artifacts: ArtifactRecord[];
  warnings: string[];
}

export const SNAPSHOT_SCHEMA_VERSION = 1;

export interface WorktreeRemovalItem {
  kind: "worktree";
  path: string;
  branch: string | null;
  command: string;
  force: boolean;
  untracked_files: string[];
  size_bytes: number;
}

export interface TrashMoveItem {
  kind: "artifact";
  path: string;
  artifact_kind: string;
  size_bytes: number | null;
  file_count: number;
  bundled_with_worktree: string | null;
}

export interface CleanupPlan {
  worktree_removals: WorktreeRemovalItem[];
  trash_moves: TrashMoveItem[];
  excluded_unsafe: number;
  total_bytes: number;
  blockers: string[];
  ack_reasons: string[];
}

export interface HistoryEntry {
  at: string;
  action: "worktree-remove" | "trash-move" | "prune";
  target: string;
  reason: string;
  result: "ok" | "skipped" | "failed";
  size_bytes: number;
  detail: string;
}

export interface ResultEntry {
  action: "worktree-remove" | "trash-move" | "prune";
  target: string;
  result: "ok" | "skipped" | "failed";
  detail: string;
  size_bytes: number;
}

export interface Settings {
  active_days: number;
  min_artifact_mb: number;
  untracked_is_unsafe: boolean;
  max_depth: number;
  per_dir_timeout_sec: number;
  scan_budget_sec: number;
  extra_scan_paths: string[];
  whitelist_paths: string[];
  protected_branches: string[];
  recent_projects: string[];
  theme: "system" | "light" | "dark";
  require_project_name: boolean;
}

export const STATUS_LABEL: Record<SafetyStatus, string> = {
  Safe: "可安全删除",
  Review: "需要确认",
  Unsafe: "不可删除"
};

export const SEVERITY_RANK: Record<SafetyStatus, number> = {
  Safe: 0,
  Review: 1,
  Unsafe: 2
};

export function assess(reasons: Reason[]): SafetyAssessment {
  const status = reasons.reduce<SafetyStatus>(
    (worst, reason) => (SEVERITY_RANK[reason.severity] > SEVERITY_RANK[worst] ? reason.severity : worst),
    "Safe"
  );
  return { status, reasons };
}
