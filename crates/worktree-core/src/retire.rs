//! Clone retirement.
//!
//! A clone is not a worktree. Removing its main worktree through Git's worktree
//! API would leave the repository without a working tree; deleting the clone with
//! a raw directory operation would bypass every guard this project has. This
//! module therefore owns one separate action:
//!
//! 1. The main worktree must be clean and every linked worktree must be `Safe`.
//! 2. The clone root passes the path guard and whitelist before anything moves.
//! 3. Linked worktrees leave through the ordinary Git worktree executor.
//! 4. Only when every linked removal succeeded is the whole clone moved to the
//!    system Trash, where it remains recoverable.
//!
//! Nothing here changes the ordinary [`crate::CleanupPlan`]. A main worktree is
//! still never a selectable worktree row (D4); clone retirement is a separate
//! clone-level decision.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::cleanup::{execute_plan, move_to_trash, ExecuteOptions, ExecutionOutcome};
use crate::git::GitRunner;
use crate::git2_runner::Git2Runner;
use crate::log;
use crate::path::{expand_tilde, is_in_system_trash, validate_path_for_deletion, DeletionContext};
use crate::reason::{Reason, ReasonCode, SafetyStatus};
use crate::timefmt::format_rfc3339;
use crate::worktree::{check_worktree, parse_worktree_list, WorktreeCheckOptions, WorktreeEntry};
use crate::{
    CleanupPlan, CloneRetirement, DetailArg, DetailCode, ExecutionResult, HistoryAction,
    HistoryEntry, PlanItemKind, ProjectSnapshot, ResultEntry, WorktreeRecord, WorktreeRemovalItem,
};

/// Decides whether the clone as a whole can be retired.
///
/// A clean main worktree is necessary but not sufficient: a linked worktree with
/// any Review or Unsafe reason is a blocker, because it would be removed before the
/// clone moves. The main worktree's ordinary `wt.main` reason is intentionally not
/// considered here; it is the reason that protects it from the ordinary worktree
/// plan, not a reason the clone cannot be retired.
pub fn assess_clone_retirement(
    worktrees: &[WorktreeRecord],
    worktree_scan_complete: bool,
) -> CloneRetirement {
    let Some(main) = worktrees.iter().find(|record| record.is_main) else {
        return CloneRetirement {
            eligible: false,
            main_path: String::new(),
            linked_worktrees: Vec::new(),
            reasons: vec![Reason::blocking(
                ReasonCode::CloneNotReady,
                "the scan did not report a main worktree",
                "no worktree record has is_main = true",
            )],
        };
    };

    let linked: Vec<&WorktreeRecord> = worktrees.iter().filter(|record| !record.is_main).collect();
    let mut reasons = main_blockers(main);

    if !worktree_scan_complete {
        reasons.push(Reason::review(
            ReasonCode::CloneNotReady,
            "not every worktree was scanned",
            "the worktree scan was incomplete; rescan before retiring the clone",
        ));
    }

    for record in &linked {
        if record.safety.status == SafetyStatus::Safe {
            continue;
        }
        let strongest = record
            .safety
            .reasons
            .iter()
            .max_by_key(|reason| reason.severity);
        match strongest {
            Some(reason) => {
                let mut blocker = reason.clone();
                blocker.evidence = format!("{}: {}", record.path, reason.evidence);
                reasons.push(blocker);
            }
            None => reasons.push(Reason::review(
                ReasonCode::CloneNotReady,
                "a linked worktree is not Safe to remove",
                format!("{}: no reason recorded", record.path),
            )),
        }
    }

    let eligible = reasons.is_empty();
    if eligible {
        reasons.push(Reason::safe(
            ReasonCode::CloneEligible,
            "the clone can be retired",
            format!(
                "main worktree is clean; {} linked worktree(s) are Safe and will be removed before the clone moves to the Trash",
                linked.len()
            ),
        ));
    }

    CloneRetirement {
        eligible,
        main_path: main.path.clone(),
        linked_worktrees: linked.iter().map(|record| record.path.clone()).collect(),
        reasons,
    }
}

/// The conditions that prevent the whole clone from being retired.
///
/// The same list is used after the linked worktrees have been removed, so the
/// executor re-checks the main worktree against the exact rule the scan used.
fn main_blockers(main: &WorktreeRecord) -> Vec<Reason> {
    let mut reasons = Vec::new();
    if main.is_locked {
        reasons.push(Reason::blocking(
            ReasonCode::WtLocked,
            "the main worktree is locked by Git",
            "git worktree list marks the main worktree as locked",
        ));
    }
    if main.is_prunable {
        reasons.push(Reason::review(
            ReasonCode::WtPrunable,
            "the main working directory no longer exists",
            "the main worktree is prunable",
        ));
    }
    if main.is_detached || main.branch.as_deref().is_none_or(str::is_empty) {
        reasons.push(Reason::review(
            ReasonCode::WtDetached,
            "the main worktree is not on a branch",
            "the main worktree has no checked-out branch",
        ));
    }
    if main.dirty_files > 0 {
        reasons.push(Reason::blocking(
            ReasonCode::WtDirty,
            "the main worktree has uncommitted changes",
            format!("{} changed file(s)", main.dirty_files),
        ));
    }
    if main.untracked_files > 0 || !main.untracked_file_list.is_empty() {
        reasons.push(Reason::review(
            ReasonCode::WtUntracked,
            "the main worktree has untracked files",
            format!(
                "{} untracked file(s): {}",
                main.untracked_files,
                main.untracked_file_list.join(", ")
            ),
        ));
    }
    if main.stash_count > 0 {
        reasons.push(Reason::review(
            ReasonCode::CloneStash,
            "the repository has stash entries",
            format!(
                "{} stash entries; the clone cannot move until they are cleared",
                main.stash_count
            ),
        ));
    }
    if !main.has_upstream {
        reasons.push(Reason::review(
            ReasonCode::CloneNoUpstream,
            "the main branch has no upstream",
            "cannot verify that the clone contains no unpushed commits",
        ));
    } else if main.ahead > 0 {
        reasons.push(Reason::review(
            ReasonCode::WtUnpushedBranch,
            "the main branch has commits not pushed upstream",
            format!("{} commit(s) ahead of the upstream branch", main.ahead),
        ));
    }
    if main
        .safety
        .reasons
        .iter()
        .any(|reason| reason.code == ReasonCode::WtCannotVerify)
    {
        reasons.push(Reason::review(
            ReasonCode::WtCannotVerify,
            "the main worktree check did not finish",
            "the scan recorded wt.cannot_verify for the main worktree",
        ));
    }
    for reason in &main.safety.reasons {
        if reason.severity == SafetyStatus::Safe
            || matches!(
                reason.code,
                ReasonCode::WtMain
                    | ReasonCode::WtClean
                    | ReasonCode::WtMerged
                    | ReasonCode::WtAbsorbed
                    | ReasonCode::WtLocked
                    | ReasonCode::WtDirty
                    | ReasonCode::WtUntracked
                    | ReasonCode::WtDetached
                    | ReasonCode::WtPrunable
                    | ReasonCode::WtUnpushedBranch
                    | ReasonCode::WtNoUpstream
                    | ReasonCode::WtCannotVerify
            )
        {
            continue;
        }
        reasons.push(Reason::review(
            ReasonCode::CloneNotReady,
            "the main worktree has another blocking condition",
            format!("{}: {}", reason.code.as_str(), reason.evidence),
        ));
    }
    reasons
}

/// Removes the linked worktrees, then moves the whole clone to the system Trash.
///
/// This is deliberately serial: linked removals are executed by the ordinary
/// worktree executor, one at a time, and the clone move is attempted only when
/// every one of them reported `Ok`. If a linked removal fails, the main clone is
/// left untouched and the result says why.
pub fn retire_clone(snapshot: &ProjectSnapshot, opts: &ExecuteOptions) -> ExecutionOutcome {
    // `eligible` is the scan's own answer; an incomplete worktree scan is false.
    // Recalculate the current facts below, but never turn that recorded "no" into a
    // "yes" by looking only at the subset that was scanned.
    if !snapshot.clone_retirement.eligible {
        return clone_only_outcome(
            &snapshot.root,
            ExecutionResult::Skipped,
            DetailCode::CloneNotReady,
            Some(ReasonCode::CloneNotReady),
            &[],
            opts,
        );
    }
    let assessed = assess_clone_retirement(&snapshot.worktrees, true);
    if !assessed.eligible {
        return clone_only_outcome(
            &snapshot.root,
            ExecutionResult::Skipped,
            DetailCode::CloneNotReady,
            Some(ReasonCode::CloneNotReady),
            &[],
            opts,
        );
    }

    // Refuse a protected clone before removing anything. Doing this only at the
    // final trash move would leave the linked worktrees permanently removed while
    // the main clone stayed behind.
    if let Err(blocker) = validate_clone_path(snapshot, opts) {
        return clone_only_outcome(
            &snapshot.root,
            ExecutionResult::Skipped,
            DetailCode::CloneNotReady,
            Some(ReasonCode::CloneNotReady),
            &[("reason", DetailArg::Text(blocker))],
            opts,
        );
    }

    let linked_plan = linked_worktree_plan(snapshot);
    let linked_outcome = if linked_plan.worktree_removals.is_empty() {
        empty_outcome()
    } else {
        execute_plan(&linked_plan, snapshot, opts)
    };

    let linked_ok = linked_plan.worktree_removals.iter().all(|item| {
        linked_outcome.entries.iter().any(|entry| {
            entry.target == item.path
                && entry.result == ExecutionResult::Ok
                && !matches!(
                    entry.detail_code,
                    Some(DetailCode::WorktreeRemnants | DetailCode::WorktreeRemoveFailed)
                )
        })
    });

    if !linked_ok {
        return append_clone_entry(
            linked_outcome,
            &snapshot.root,
            ExecutionResult::Skipped,
            DetailCode::CloneNotReady,
            Some(ReasonCode::CloneNotReady),
            &[],
            opts,
        );
    }

    // The linked worktrees are gone; verify the main worktree again and make sure
    // no new worktree appeared between the plan and this point.
    if let Err(blocker) = recheck_main(snapshot, opts) {
        return append_clone_entry(
            linked_outcome,
            &snapshot.root,
            ExecutionResult::Skipped,
            DetailCode::CloneNotReady,
            Some(ReasonCode::CloneNotReady),
            &[("reason", DetailArg::Text(blocker))],
            opts,
        );
    }

    match move_to_trash(&snapshot.root) {
        Ok(()) => append_clone_entry(
            linked_outcome,
            &snapshot.root,
            ExecutionResult::Ok,
            DetailCode::CloneRetired,
            Some(ReasonCode::CloneEligible),
            &[],
            opts,
        ),
        Err(error) => append_clone_entry(
            linked_outcome,
            &snapshot.root,
            ExecutionResult::Failed,
            DetailCode::CloneRetireFailed,
            Some(ReasonCode::CloneEligible),
            &[("error", DetailArg::Text(error.to_string()))],
            opts,
        ),
    }
}

/// The plan for all linked worktrees. Eligibility already proved every one Safe.
fn linked_worktree_plan(snapshot: &ProjectSnapshot) -> CleanupPlan {
    let worktree_removals = snapshot
        .worktrees
        .iter()
        .filter(|record| !record.is_main)
        .map(|record| {
            let force = !record.is_prunable && record.untracked_files > 0;
            WorktreeRemovalItem {
                kind: PlanItemKind::Worktree,
                path: record.path.clone(),
                branch: record.branch.clone(),
                command: if record.is_prunable {
                    "git worktree prune".to_string()
                } else {
                    format!(
                        "git worktree remove{} -- \"{}\"",
                        if force { " --force" } else { "" },
                        record.path
                    )
                },
                force,
                untracked_files: record.untracked_file_list.clone(),
                size_bytes: record.size_bytes,
            }
        })
        .collect();
    CleanupPlan {
        worktree_removals,
        ..CleanupPlan::default()
    }
}

/// Re-checks the one remaining worktree before the clone itself moves.
fn recheck_main(snapshot: &ProjectSnapshot, opts: &ExecuteOptions) -> Result<(), String> {
    let main = snapshot
        .worktrees
        .iter()
        .find(|record| record.is_main)
        .ok_or_else(|| "the scan did not report a main worktree".to_string())?;
    let root = Path::new(&snapshot.root);
    let runner = Git2Runner::new(opts.git_timeout);
    let listing = runner
        .run(root, &["worktree", "list", "--porcelain", "-z"])
        .map_err(|error| format!("the worktree list could not be read: {error}"))?;
    if !listing.success() {
        return Err(format!(
            "the worktree list exited with {:?}: {}",
            listing.code,
            listing.stderr.trim()
        ));
    }
    let current = parse_worktree_list(&listing.stdout);
    if current.len() != 1 || !same_worktree_path(Path::new(&current[0].path), root) {
        return Err(format!(
            "{} worktree(s) remain; the main clone was left in place",
            current.len().saturating_sub(1)
        ));
    }

    let whitelist: Vec<PathBuf> = opts
        .whitelist
        .iter()
        .map(|rule| expand_tilde(rule))
        .collect();
    let rechecked = check_worktree(
        &runner,
        root,
        &worktree_entry_for(main),
        true,
        &WorktreeCheckOptions {
            default_branch: snapshot.default_branch.clone(),
            recent_activity_days: opts.recent_activity_days,
            untracked_as_unsafe: opts.untracked_as_unsafe,
            whitelist,
            now: opts.now,
            size_bytes: main.size_bytes,
            repository_stash_count: Some(main.stash_count),
            credential_scan_timeout: opts.per_dir_timeout.min(std::time::Duration::from_secs(5)),
        },
    );
    let blockers = main_blockers(&rechecked);
    if let Some(reason) = blockers.first() {
        return Err(format!("{}: {}", reason.code.as_str(), reason.evidence));
    }

    validate_clone_path(snapshot, opts)
}

/// The path guard and whitelist for the clone root.
///
/// It runs before any linked worktree is touched and again after the removals, so
/// a protected clone never loses a linked worktree on the way to a refused move.
fn validate_clone_path(snapshot: &ProjectSnapshot, opts: &ExecuteOptions) -> Result<(), String> {
    let root = Path::new(&snapshot.root);
    let parent = root
        .parent()
        .map_or_else(|| root.to_path_buf(), Path::to_path_buf);
    let whitelist: Vec<PathBuf> = opts
        .whitelist
        .iter()
        .map(|rule| expand_tilde(rule))
        .collect();
    let context = DeletionContext::new(parent).with_whitelist(whitelist);
    validate_path_for_deletion(root, &context).map_err(|error| error.to_string())?;
    if is_in_system_trash(root) {
        return Err("the clone is already inside the system Trash".to_string());
    }
    Ok(())
}

/// Whether two spellings name the same worktree directory. `git worktree list`
/// reports the main worktree through libgit2's workdir, which may carry a trailing
/// separator, while the snapshot root came from the user's path.
fn same_worktree_path(left: &Path, right: &Path) -> bool {
    if left == right {
        return true;
    }
    match (std::fs::canonicalize(left), std::fs::canonicalize(right)) {
        (Ok(left), Ok(right)) => left == right,
        _ => left.components().eq(right.components()),
    }
}

/// A [`WorktreeEntry`] reconstructed from the scan record for the re-check.
fn worktree_entry_for(record: &WorktreeRecord) -> WorktreeEntry {
    WorktreeEntry {
        path: PathBuf::from(&record.path),
        head: record.head.clone(),
        branch: record.branch.clone(),
        is_bare: false,
        is_detached: record.is_detached,
        is_locked: record.is_locked,
        lock_reason: None,
        is_prunable: record.is_prunable,
        prunable_reason: None,
    }
}

const fn empty_outcome() -> ExecutionOutcome {
    ExecutionOutcome {
        entries: Vec::new(),
        history: Vec::new(),
        log_error: None,
    }
}

fn clone_only_outcome(
    target: &str,
    result: ExecutionResult,
    detail: DetailCode,
    reason: Option<ReasonCode>,
    args: &[(&str, DetailArg)],
    opts: &ExecuteOptions,
) -> ExecutionOutcome {
    append_clone_entry(empty_outcome(), target, result, detail, reason, args, opts)
}

/// Adds the clone row to an existing outcome and appends it to the operation log.
fn append_clone_entry(
    mut outcome: ExecutionOutcome,
    target: &str,
    result: ExecutionResult,
    detail: DetailCode,
    reason: Option<ReasonCode>,
    args: &[(&str, DetailArg)],
    opts: &ExecuteOptions,
) -> ExecutionOutcome {
    let detail_args: BTreeMap<String, DetailArg> = args
        .iter()
        .map(|(name, value)| ((*name).to_string(), value.clone()))
        .collect();
    let entry = ResultEntry {
        action: HistoryAction::CloneRetire,
        target: target.to_string(),
        result,
        detail_code: Some(detail),
        detail_args: detail_args.clone(),
        detail: None,
        size_bytes: None,
    };
    let history = HistoryEntry {
        at: format_rfc3339(SystemTime::now()),
        action: HistoryAction::CloneRetire,
        target: target.to_string(),
        reason_code: reason,
        reason: None,
        result,
        size_bytes: None,
        detail_code: Some(detail),
        detail_args,
        detail: None,
    };
    outcome.entries.push(entry);
    outcome.history.push(history.clone());
    if let Some(path) = opts.log_path.as_ref() {
        if let Err(error) = log::append(path, &[history]) {
            let failure = format!("{} ({error})", path.display());
            outcome.log_error = Some(match outcome.log_error {
                Some(existing) => format!("{existing}; {failure}"),
                None => failure,
            });
        }
    }
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reason::SafetyAssessment;

    fn record(path: &str, is_main: bool, status: SafetyStatus) -> WorktreeRecord {
        let reason = if status == SafetyStatus::Safe {
            Reason::safe(
                ReasonCode::WtClean,
                "clean working tree",
                "no uncommitted changes",
            )
        } else {
            Reason::review(
                ReasonCode::WtUnmerged,
                "not merged into the default branch",
                "a branch commit is not in main",
            )
        };
        WorktreeRecord {
            path: path.to_string(),
            branch: Some(if is_main { "main" } else { "feature" }.to_string()),
            head: Some("0123456".to_string()),
            is_main,
            is_detached: false,
            is_locked: false,
            is_prunable: false,
            dirty_files: 0,
            untracked_files: 0,
            untracked_file_list: Vec::new(),
            ahead: 0,
            behind: 0,
            has_upstream: true,
            merged_into_default: Some(true),
            absorbed_into_default: false,
            stash_count: 0,
            last_updated_at: None,
            agent: None,
            size_bytes: Some(10),
            safety: SafetyAssessment::from_reasons(vec![
                if is_main {
                    Reason::blocking(
                        ReasonCode::WtMain,
                        "this is the main worktree",
                        "first entry of git worktree list",
                    )
                } else {
                    reason.clone()
                },
                reason,
            ]),
        }
    }

    fn snapshot(worktrees: Vec<WorktreeRecord>) -> ProjectSnapshot {
        let clone_retirement = assess_clone_retirement(&worktrees, true);
        ProjectSnapshot {
            schema_version: crate::SNAPSHOT_SCHEMA_VERSION,
            root: "/repo".to_string(),
            scanned_at: "2026-01-01T00:00:00Z".to_string(),
            default_branch: Some("main".to_string()),
            worktrees,
            artifacts: Vec::new(),
            clone_retirement,
            hygiene: crate::HygieneReport {
                rules: Vec::new(),
                findings: Vec::new(),
                warnings: Vec::new(),
            },
            warnings: Vec::new(),
        }
    }

    #[test]
    fn a_clean_main_and_safe_linked_worktrees_are_eligible() {
        let assessed = assess_clone_retirement(
            &[
                record("/repo", true, SafetyStatus::Safe),
                record("/repo--feature", false, SafetyStatus::Safe),
            ],
            true,
        );
        assert!(assessed.eligible);
        assert_eq!(assessed.main_path, "/repo");
        assert_eq!(assessed.linked_worktrees, vec!["/repo--feature"]);
        assert_eq!(assessed.reasons[0].code, ReasonCode::CloneEligible);
    }

    #[test]
    fn a_dirty_main_or_an_unsafe_linked_worktree_blocks_retirement() {
        let mut main = record("/repo", true, SafetyStatus::Safe);
        main.dirty_files = 2;
        let dirty = assess_clone_retirement(&[main], true);
        assert!(!dirty.eligible);
        assert!(dirty
            .reasons
            .iter()
            .any(|reason| reason.code == ReasonCode::WtDirty));

        let linked = record("/repo--feature", false, SafetyStatus::Review);
        let unsafe_link =
            assess_clone_retirement(&[record("/repo", true, SafetyStatus::Safe), linked], true);
        assert!(!unsafe_link.eligible);
        assert!(unsafe_link
            .reasons
            .iter()
            .any(|reason| reason.code == ReasonCode::WtUnmerged));
    }

    #[test]
    fn a_stash_or_a_missing_upstream_blocks_retirement() {
        let mut main = record("/repo", true, SafetyStatus::Safe);
        main.stash_count = 1;
        assert!(!assess_clone_retirement(&[main], true).eligible);

        let mut no_upstream = record("/repo", true, SafetyStatus::Safe);
        no_upstream.has_upstream = false;
        assert!(!assess_clone_retirement(&[no_upstream], true).eligible);
    }

    #[test]
    fn an_ineligible_clone_is_skipped_before_any_trash_move() {
        let mut main = record("/repo", true, SafetyStatus::Safe);
        main.dirty_files = 1;
        let outcome = retire_clone(
            &snapshot(vec![main]),
            &ExecuteOptions {
                log_path: None,
                ..ExecuteOptions::default()
            },
        );
        assert_eq!(outcome.entries.len(), 1);
        assert_eq!(outcome.entries[0].action, HistoryAction::CloneRetire);
        assert_eq!(outcome.entries[0].result, ExecutionResult::Skipped);
        assert_eq!(
            outcome.entries[0].detail_code,
            Some(DetailCode::CloneNotReady)
        );
        assert_eq!(outcome.history.len(), 1);
        assert!(outcome.log_error.is_none());
    }

    #[test]
    fn an_incomplete_worktree_scan_blocks_retirement() {
        let assessed = assess_clone_retirement(
            &[
                record("/repo", true, SafetyStatus::Safe),
                record("/repo--feature", false, SafetyStatus::Safe),
            ],
            false,
        );
        assert!(!assessed.eligible);
        assert!(assessed
            .reasons
            .iter()
            .any(|reason| reason.code == ReasonCode::CloneNotReady));
    }

    #[test]
    fn a_plan_for_retirement_contains_only_linked_worktrees() {
        let snapshot = snapshot(vec![
            record("/repo", true, SafetyStatus::Safe),
            record("/repo--feature", false, SafetyStatus::Safe),
        ]);
        let plan = linked_worktree_plan(&snapshot);
        assert_eq!(plan.worktree_removals.len(), 1);
        assert_eq!(plan.worktree_removals[0].path, "/repo--feature");
        assert!(plan.trash_moves.is_empty());
    }
}
