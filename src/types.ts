import { messages } from "./i18n";
import type { LocaleChoice } from "./i18n";

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
  | "af.credentials"
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
  /** Verifiable evidence: a concrete command, file count, or path. Must not be empty. */
  evidence: string;
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
  /** `null` = unmeasurable (no permission / timeout); must stay distinguishable from 0. */
  size_bytes: number | null;
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

export const SNAPSHOT_SCHEMA_VERSION = 2;

export interface WorktreeRemovalItem {
  kind: "worktree";
  path: string;
  branch: string | null;
  command: string;
  force: boolean;
  untracked_files: string[];
  /** `null` = unmeasurable, so the plan does not claim a size it cannot know. */
  size_bytes: number | null;
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
  /**
   * How many of the independently deleted items have an unknown size. While this
   * is above zero, `total_bytes` is a lower bound and the 10 GB confirmation
   * threshold cannot be evaluated — the UI must say so instead of implying the
   * total is exact.
   */
  total_bytes_unknown: number;
  blockers: string[];
  ack_reasons: string[];
}

/**
 * What one executed action did, as a stable code rather than a sentence.
 *
 * Mirrors `DetailCode` in `crates/worktree-core/src/lib.rs`. A free English
 * sentence used to travel in the record and be printed as it stood, which is why
 * the detail column stayed English after a language switch and why a re-check
 * failure once put `size=Some(1320274909) files=80703` on the screen. Only the
 * code and raw arguments cross; the wording is in `src/i18n/`.
 */
export type DetailCode =
  | "worktree.not_in_scan"
  | "worktree.changed"
  | "worktree.no_longer_safe"
  | "worktree.removed"
  | "worktree.remove_failed"
  | "worktree.pruned"
  | "worktree.prune_failed"
  | "prunable.metadata_only"
  | "artifact.not_in_scan"
  | "artifact.not_a_known_kind"
  | "artifact.changed"
  | "artifact.no_longer_safe"
  | "artifact.vanished"
  | "artifact.unmeasurable"
  | "artifact.trashed"
  | "artifact.trash_failed"
  | "precondition.failed"
  | "bundled.with_worktree";

/** One argument to a `DetailCode`.
 *
 * Raw values only: a byte count arrives as a number and is formatted by
 * `detailArg` in `src/format.ts`, so the unit follows the language the reader is
 * using. `null` is the absence of a measurement and must never render as zero.
 */
export type DetailArg = string | number | boolean | null;

export interface HistoryEntry {
  at: string;
  action: "worktree-remove" | "trash-move" | "prune";
  target: string;
  /** The verdict this deletion was based on; `null` when the record has none. */
  reason_code: ReasonCode | null;
  /** A record written before the codes existed; shown as it stands. */
  reason: string | null;
  result: "ok" | "skipped" | "failed";
  /** `null` = the size was never measurable; must not collapse to 0. */
  size_bytes: number | null;
  detail_code: DetailCode | null;
  detail_args: Record<string, DetailArg>;
  /** A record written before the codes existed; shown as it stands. */
  detail: string | null;
}

export interface ResultEntry {
  action: "worktree-remove" | "trash-move" | "prune";
  target: string;
  result: "ok" | "skipped" | "failed";
  detail_code: DetailCode | null;
  detail_args: Record<string, DetailArg>;
  detail: string | null;
  /** `null` = the size was never measurable; must not collapse to 0. */
  size_bytes: number | null;
}

/**
 * The scan knobs the core expects.
 *
 * Durations are milliseconds on the wire so the front end never has to know how
 * Rust encodes a `Duration`. Mirrors `ScanOptions` in
 * crates/worktree-core/src/lib.rs field for field.
 */
export interface ScanOptions {
  max_depth: number;
  per_dir_timeout_ms: number;
  scan_budget_ms: number;
  recent_activity_days: number;
  untracked_as_unsafe: boolean;
  whitelist: string[];
  git_timeout_ms: number;
}

/**
 * What one execution run produced.
 *
 * Mirrors `ExecutionOutcome` in crates/worktree-core/src/cleanup.rs. `log_error`
 * being set does not mean the run failed: the actions happened, and only the
 * audit trail has a hole in it.
 */
export interface ExecutionOutcome {
  entries: ResultEntry[];
  history: HistoryEntry[];
  log_error: string | null;
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
  /** One language, or follow the system. Persisted with the rest of the settings. */
  locale: LocaleChoice;
  require_project_name: boolean;
}

/**
 * The full label for a status.
 *
 * A function rather than a `Record`, because the message table is rebound when
 * the language changes: a map built at import time would keep handing out the
 * language that was active when the module was first evaluated.
 */
export function statusLabel(status: SafetyStatus): string {
  return messages.status.label[status];
}

/** The one-word form of a status, used where the full label would not fit. */
export function statusShort(status: SafetyStatus): string {
  return messages.status.short[status];
}

export const SEVERITY_RANK: Record<SafetyStatus, number> = {
  Safe: 0,
  Review: 1,
  Unsafe: 2
};

/**
 * Derive a safety assessment from a set of reasons: the level is the most severe
 * one (D2), and an empty list is `Review`, never `Safe` (D3 — silence without
 * evidence cannot be called safe).
 *
 * This helper exists **only** to build the mock snapshot in `src/mock/snapshot.ts`.
 * Production code must consume the `status` produced by the scanner
 * (`crates/worktree-core/src/reason.rs`, `from_reasons`) instead of recomputing it
 * here: two implementations of the same rule will drift.
 */
export function assess(reasons: Reason[]): SafetyAssessment {
  const status = reasons.reduce<SafetyStatus>(
    (worst, reason) => (SEVERITY_RANK[reason.severity] > SEVERITY_RANK[worst] ? reason.severity : worst),
    reasons.length === 0 ? "Review" : "Safe"
  );
  return { status, reasons };
}
