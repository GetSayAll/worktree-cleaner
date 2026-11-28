import type { WorktreeRecord } from "./types";

/**
 * The three ways a `Review` worktree can be reviewed.
 *
 * `batch` and `untracked` may be selected with one explicit group action;
 * `attention` never is. This is a presentation distinction, not a new safety
 * level: every record stays `Review`, and the executor re-checks it.
 */
export type ReviewLane = "batch" | "untracked" | "attention";

/**
 * Reasons that say something about the branch, not about files in the worktree.
 * Worktree removal keeps every branch ref, so a worktree whose only Review
 * reasons are in this set can be selected as a group without losing a commit.
 *
 * Every other Review code (recent activity, detached HEAD, an incomplete check,
 * an index lock, an old log code) sends the record to `attention`. An unknown
 * code also lands there: a future scanner reason must never be batched by a UI
 * that has not been taught what it means.
 */
const BRANCH_ONLY_REVIEW_CODES = new Set<string>([
  "wt.unpushed_branch",
  "wt.no_upstream",
  "wt.unmerged"
]);

/**
 * Groups one `Review` worktree by what removing it can lose.
 *
 * The record's own fields, not a hand-maintained list of paths, decide the lane:
 *
 * - `untracked`: untracked files are deleted with the directory. The plan lists
 *   them, so this group may be selected only after that warning is shown.
 * - `batch`: no untracked files, no recent/detached/unknown reason. Removing the
 *   directory keeps the branch ref; Git-ignored files inside it are still lost,
 *   which is why the page says so before the group action.
 * - `attention`: recent activity, a detached HEAD, an index lock, an incomplete
 *   check, or a reason this build does not recognise. These need a reader's eye
 *   one row at a time.
 *
 * Returns `null` for anything that is not `Review`; `Safe` is selected by
 * default and `Unsafe` cannot be selected at all.
 */
export function reviewLane(record: WorktreeRecord): ReviewLane | null {
  if (record.safety.status !== "Review") return null;

  // These fields should already make a record Unsafe. Checking them here keeps
  // an inconsistent snapshot from turning an unknown state into a batch-select.
  if (
    record.is_main ||
    record.is_locked ||
    record.is_detached ||
    record.dirty_files > 0 ||
    record.merged_into_default === null
  ) {
    return "attention";
  }

  const reviewCodes = record.safety.reasons
    .filter((reason) => reason.severity === "Review")
    .map((reason) => reason.code as string);
  const untracked = record.untracked_files > 0 || record.untracked_file_list.length > 0;
  const allowed = new Set(BRANCH_ONLY_REVIEW_CODES);
  if (untracked) allowed.add("wt.untracked");

  // An empty list is not evidence that the branch-only lane applies. A Review
  // record with no Review reason at all is an inconsistency, not a clean record.
  if (reviewCodes.length === 0 || !reviewCodes.every((code) => allowed.has(code))) {
    return "attention";
  }
  return untracked ? "untracked" : "batch";
}
