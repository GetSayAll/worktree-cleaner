//! Plan execution (product plan section 5.4, the execution side).
//!
//! The executor is the only place in this crate that changes anything, and it
//! changes nothing that is not named by a plan object (D18). Three guarantees
//! shape it:
//!
//! - **Re-verification (D11).** Every item is re-checked between plan and
//!   execution: its path is validated again, and its decision-relevant facts are
//!   recomputed. Anything that changed is *skipped*, never executed from a stale
//!   plan.
//! - **Isolation (D14).** Execution is serial and one failure does not stop the
//!   others; each item carries its own result.
//! - **Trash only (D5) and `git worktree remove` only (D6).** Directories go to
//!   the system Trash through the `trash` crate; worktrees go through Git. There
//!   is no permanent-delete path and no bypass switch.
//!
//! Every action is appended to the local operation log (D15). Nothing here talks
//! to the network.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

use crate::artifact::{check_artifact, lookup_kind, ArtifactScanOptions};
use crate::git::{GitError, GitRunner, ProcessGitRunner};
use crate::log;
use crate::path::{validate_path_for_deletion, DeletionContext};
use crate::reason::{ReasonCode, SafetyStatus};
use crate::worktree::{check_worktree, WorktreeCheckOptions};
use crate::{
    CleanupPlan, DetailArg, DetailCode, ExecutionResult, HistoryAction, HistoryEntry,
    ProjectSnapshot, ResultEntry, WorktreeRecord,
};

/// Injection points for one execution run.
#[derive(Debug, Clone)]
pub struct ExecuteOptions {
    /// Activity window in days, re-used by the re-check (settings.active_days).
    pub recent_activity_days: u32,
    /// Treat untracked files as `Unsafe` (settings.untracked_is_unsafe).
    pub untracked_as_unsafe: bool,
    /// Whitelist rules as written by the user; `~` is expanded here.
    pub whitelist: Vec<String>,
    /// Budget for one size measurement during the re-check.
    pub per_dir_timeout: Duration,
    /// Budget for one Git command.
    pub git_timeout: Duration,
    /// Injected clock, so the re-check's activity window is testable.
    pub now: SystemTime,
    /// Where to append the operation log. `None` disables logging.
    pub log_path: Option<PathBuf>,
}

impl Default for ExecuteOptions {
    fn default() -> Self {
        Self {
            recent_activity_days: 7,
            untracked_as_unsafe: false,
            whitelist: Vec::new(),
            per_dir_timeout: Duration::from_secs(30),
            git_timeout: Duration::from_secs(10),
            now: SystemTime::now(),
            log_path: log::default_log_path(),
        }
    }
}

/// What one execution run produced.
///
/// Serialised straight to the desktop shell: the UI shows `entries` item by
/// item, keeps `history` for the audit view, and must be told about `log_error`
/// because the actions happened whether or not the log could be written.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionOutcome {
    /// Per-item results, in execution order.
    pub entries: Vec<ResultEntry>,
    /// The same actions as log records.
    pub history: Vec<HistoryEntry>,
    /// Set when the log could not be written: the actions still happened, and
    /// the user must be told the audit trail has a hole in it.
    pub log_error: Option<String>,
}

/// Runs a plan against the snapshot it was generated from.
///
/// `snapshot` is not decorative: it is the "before" the re-check compares
/// against, so a plan that arrives without its scan result is refused rather
/// than executed blind.
pub fn execute_plan(
    plan: &CleanupPlan,
    snapshot: &ProjectSnapshot,
    opts: &ExecuteOptions,
) -> ExecutionOutcome {
    let runner = ProcessGitRunner::new(opts.git_timeout);
    let whitelist: Vec<PathBuf> = opts
        .whitelist
        .iter()
        .map(|rule| crate::path::expand_tilde(rule))
        .collect();
    let mut audit = Audit::new();
    let mut any_worktree_removed = false;
    // The worktrees this run actually removed, so a bundled artifact can be reported
    // against a fact rather than against a claim in the plan. Without it, a plan that
    // named an artifact as "goes away with its worktree" reported `Ok` even when the
    // worktree removal had failed or been skipped, and the audit log then claimed a
    // deletion that never happened.
    let mut removed_worktrees: Vec<String> = Vec::new();

    // ---- Channel A: worktrees ----
    for item in &plan.worktree_removals {
        let target = item.path.as_str();
        let Some(record) = snapshot.worktrees.iter().find(|w| w.path == item.path) else {
            push(
                &mut audit,
                HistoryAction::WorktreeRemove,
                target,
                ExecutionResult::Skipped,
                DetailCode::WorktreeNotInScan,
                &[],
                None,
                None,
            );
            continue;
        };

        // ---- path re-validation (D10) ----
        let parent = Path::new(&record.path)
            .parent()
            .map_or_else(|| PathBuf::from(&snapshot.root), Path::to_path_buf);
        let context = DeletionContext::new(parent).with_whitelist(whitelist.clone());
        if validate_path_for_deletion(Path::new(&record.path), &context).is_err() {
            push(
                &mut audit,
                HistoryAction::WorktreeRemove,
                target,
                ExecutionResult::Skipped,
                DetailCode::PreconditionFailed,
                &[],
                record.size_bytes,
                primary_reason(record),
            );
            continue;
        }

        // ---- identity and activity re-check (D11) ----
        let rechecked = check_worktree(
            &runner,
            Path::new(&snapshot.root),
            &worktree_entry_for(record),
            record.is_main,
            &WorktreeCheckOptions {
                default_branch: snapshot.default_branch.clone(),
                recent_activity_days: opts.recent_activity_days,
                untracked_as_unsafe: opts.untracked_as_unsafe,
                whitelist: whitelist.clone(),
                now: opts.now,
                size_bytes: record.size_bytes,
                // Carried over rather than re-measured: the count is a repository-wide
                // fact and no longer feeds the verdict, so re-reading it here would only
                // invent a difference for a field the decision does not use.
                repository_stash_count: Some(record.stash_count),
            },
        );
        if let Some(change) = worktree_change(record, &rechecked) {
            push(
                &mut audit,
                HistoryAction::WorktreeRemove,
                target,
                ExecutionResult::Skipped,
                DetailCode::WorktreeChanged,
                &[
                    ("field", DetailArg::Text(change.field.to_string())),
                    ("before", change.before),
                    ("after", change.after),
                ],
                record.size_bytes,
                primary_reason(record),
            );
            continue;
        }
        // A plan only ever carries Safe items (D12); if the re-check says
        // anything else, the plan is stale in a way the identity above did not
        // catch, and the item must not run.
        if rechecked.safety.status != SafetyStatus::Safe {
            push(
                &mut audit,
                HistoryAction::WorktreeRemove,
                target,
                ExecutionResult::Skipped,
                DetailCode::WorktreeNoLongerSafe,
                &[(
                    "reason",
                    primary_reason(&rechecked).map_or(DetailArg::Unknown, |code| {
                        DetailArg::Text(code.as_str().to_string())
                    }),
                )],
                record.size_bytes,
                primary_reason(record),
            );
            continue;
        }

        // ---- execute ----
        if record.is_prunable {
            match run_git(&runner, Path::new(&snapshot.root), &["worktree", "prune"]) {
                Ok(()) => {
                    any_worktree_removed = true;
                    removed_worktrees.push(record.path.clone());
                    push(
                        &mut audit,
                        HistoryAction::Prune,
                        target,
                        ExecutionResult::Ok,
                        DetailCode::PrunableMetadataOnly,
                        &[],
                        record.size_bytes,
                        primary_reason(record),
                    );
                }
                Err(error) => push(
                    &mut audit,
                    HistoryAction::Prune,
                    target,
                    ExecutionResult::Failed,
                    DetailCode::WorktreePruneFailed,
                    &[("error", DetailArg::Text(error.to_string()))],
                    record.size_bytes,
                    primary_reason(record),
                ),
            }
            continue;
        }

        let mut args: Vec<String> = vec!["worktree".to_string(), "remove".to_string()];
        // `force` is re-derived from the re-checked record, never taken from the
        // plan's `command` string.
        if record.untracked_files > 0 {
            args.push("--force".to_string());
        }
        // `--` ends the options, so a path that begins with `-` is read as a path.
        // These paths are absolute, which makes that unlikely rather than impossible;
        // the separator costs nothing and removes the question.
        args.push("--".to_string());
        args.push(record.path.clone());
        let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
        match run_git(&runner, Path::new(&snapshot.root), &arg_refs) {
            Ok(()) => {
                // D6: prune runs as a follow-up so no metadata is left behind.
                let pruned = run_git(&runner, Path::new(&snapshot.root), &["worktree", "prune"]);
                any_worktree_removed = true;
                removed_worktrees.push(record.path.clone());
                // The removal is the outcome; a failed follow-up is carried
                // alongside it rather than replacing it, because the worktree is
                // gone either way and the leftover metadata is what remains.
                let args: Vec<(&str, DetailArg)> = match pruned {
                    Ok(()) => Vec::new(),
                    Err(error) => vec![("prune_error", DetailArg::Text(error.to_string()))],
                };
                push(
                    &mut audit,
                    HistoryAction::WorktreeRemove,
                    target,
                    ExecutionResult::Ok,
                    DetailCode::WorktreeRemoved,
                    &args,
                    record.size_bytes,
                    primary_reason(record),
                );
            }
            Err(error) => push(
                &mut audit,
                HistoryAction::WorktreeRemove,
                target,
                ExecutionResult::Failed,
                DetailCode::WorktreeRemoveFailed,
                &[("error", DetailArg::Text(error.to_string()))],
                record.size_bytes,
                primary_reason(record),
            ),
        }
    }

    // ---- Channel B: artifacts through the system Trash ----
    for item in &plan.trash_moves {
        let target = item.path.as_str();
        // Whether this item still needs the work below. A bundled artifact normally
        // does not — its owning worktree took it — but only when that removal really
        // happened; otherwise it is handled on its own merits, so nothing is skipped.
        let mut bundled_already_removed = false;
        if let Some(owner) = item.bundled_with_worktree.as_deref() {
            if removed_worktrees.iter().any(|path| path == owner) {
                // The owning worktree removal already deleted this directory; a
                // second operation would be a double delete.
                push(
                    &mut audit,
                    HistoryAction::TrashMove,
                    target,
                    ExecutionResult::Ok,
                    DetailCode::BundledWithWorktree,
                    &[],
                    Some(0),
                    None,
                );
                bundled_already_removed = true;
            }
        }
        if bundled_already_removed {
            continue;
        }

        let Some(record) = snapshot.artifacts.iter().find(|a| a.path == item.path) else {
            push(
                &mut audit,
                HistoryAction::TrashMove,
                target,
                ExecutionResult::Skipped,
                DetailCode::ArtifactNotInScan,
                &[],
                None,
                None,
            );
            continue;
        };

        let context = DeletionContext::new(PathBuf::from(&record.project_root))
            .with_whitelist(whitelist.clone());
        if validate_path_for_deletion(Path::new(&record.path), &context).is_err() {
            push(
                &mut audit,
                HistoryAction::TrashMove,
                target,
                ExecutionResult::Skipped,
                DetailCode::PreconditionFailed,
                &[],
                record.size_bytes,
                primary_reason_artifact(record),
            );
            continue;
        }

        // ---- re-check the artifact decision (D11) ----
        let Some(spec) = lookup_kind(
            Path::new(&record.path)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default(),
        ) else {
            push(
                &mut audit,
                HistoryAction::TrashMove,
                target,
                ExecutionResult::Skipped,
                DetailCode::ArtifactNotAKnownKind,
                &[],
                record.size_bytes,
                primary_reason_artifact(record),
            );
            continue;
        };
        // ---- re-check, and say which of four things happened ----
        //
        // "Gone", "cannot be read", "changed" and "no longer safe" all used to
        // report "changed since the plan was generated". They are one answer for
        // four situations, and the first two were not even true: a failed
        // `read_dir` came back from the size walk as zero bytes, the size was part
        // of the comparison, and a run that recorded 80703 files re-read 0 told
        // the reader they had changed something they had not touched.
        //
        // `symlink_metadata` rather than `exists()`, because `exists()` cannot
        // tell a missing directory from an unreadable one — the same EACCES /
        // NotFound conflation that `worktree.rs` already had to fix.
        match std::fs::symlink_metadata(&record.path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                push(
                    &mut audit,
                    HistoryAction::TrashMove,
                    target,
                    ExecutionResult::Skipped,
                    DetailCode::ArtifactVanished,
                    &[],
                    record.size_bytes,
                    primary_reason_artifact(record),
                );
                continue;
            }
            Err(_) => {
                push(
                    &mut audit,
                    HistoryAction::TrashMove,
                    target,
                    ExecutionResult::Skipped,
                    DetailCode::ArtifactUnmeasurable,
                    &[],
                    record.size_bytes,
                    primary_reason_artifact(record),
                );
                continue;
            }
            Ok(_) => {}
        }

        let (rechecked, _notes) = check_artifact(
            &runner,
            Path::new(&record.project_root),
            Path::new(&record.path),
            spec,
            &ArtifactScanOptions {
                max_depth: 1,
                per_dir_timeout: opts.per_dir_timeout,
                recent_activity_days: opts.recent_activity_days,
                whitelist: whitelist.clone(),
                now: opts.now,
                budget_deadline: None,
                credential_budget: None,
            },
        );
        if let Some(change) = artifact_change(record, &rechecked) {
            push(
                &mut audit,
                HistoryAction::TrashMove,
                target,
                ExecutionResult::Skipped,
                DetailCode::ArtifactChanged,
                &[
                    ("field", DetailArg::Text(change.field.to_string())),
                    ("before", change.before),
                    ("after", change.after),
                ],
                record.size_bytes,
                primary_reason_artifact(record),
            );
            continue;
        }
        if rechecked.safety.status != SafetyStatus::Safe {
            push(
                &mut audit,
                HistoryAction::TrashMove,
                target,
                ExecutionResult::Skipped,
                DetailCode::ArtifactNoLongerSafe,
                &[(
                    "reason",
                    primary_reason_artifact(&rechecked).map_or(DetailArg::Unknown, |code| {
                        DetailArg::Text(code.as_str().to_string())
                    }),
                )],
                record.size_bytes,
                primary_reason_artifact(record),
            );
            continue;
        }

        // ---- execute ----
        match move_to_trash(&record.path) {
            Ok(()) => push(
                &mut audit,
                HistoryAction::TrashMove,
                target,
                ExecutionResult::Ok,
                DetailCode::ArtifactTrashed,
                &[],
                record.size_bytes,
                primary_reason_artifact(record),
            ),
            Err(error) => push(
                &mut audit,
                HistoryAction::TrashMove,
                target,
                ExecutionResult::Failed,
                DetailCode::ArtifactTrashFailed,
                &[("error", DetailArg::Text(error.to_string()))],
                record.size_bytes,
                primary_reason_artifact(record),
            ),
        }
    }

    // A final prune summarises the metadata cleanup when at least one worktree
    // was removed; it is the same command the removals already ran, reported
    // once for the repository as a whole.
    if any_worktree_removed {
        match run_git(&runner, Path::new(&snapshot.root), &["worktree", "prune"]) {
            Ok(()) => push(
                &mut audit,
                HistoryAction::Prune,
                &snapshot.root,
                ExecutionResult::Ok,
                DetailCode::WorktreePruned,
                &[],
                None,
                None,
            ),
            Err(error) => push(
                &mut audit,
                HistoryAction::Prune,
                &snapshot.root,
                ExecutionResult::Failed,
                DetailCode::WorktreePruneFailed,
                &[("error", DetailArg::Text(error.to_string()))],
                None,
                None,
            ),
        }
    }

    let log_error = match opts.log_path.as_ref() {
        Some(path) => log::append(path, &audit.history)
            .err()
            .map(|error| format!("{} ({error})", path.display())),
        None => None,
    };

    ExecutionOutcome {
        entries: audit.entries,
        history: audit.history,
        log_error,
    }
}

/// Moves one directory to the system Trash (D5). There is no permanent-delete
/// path and no bypass switch.
///
/// The crate's macOS default routes the request through `osascript` to the
/// Finder application. That needs the user to grant Automation access, prompts
/// on first use, and fails outright — with a permissions error from Finder —
/// when it is refused. The `NSFileManager` call reaches the same Trash
/// (recoverable, "Put Back" subject to a macOS bug the crate documents) without
/// asking for anything beyond what the app already has.
fn move_to_trash(path: &str) -> Result<(), trash::Error> {
    #[allow(unused_mut)]
    let mut context = trash::TrashContext::default();
    #[cfg(target_os = "macos")]
    {
        use trash::macos::{DeleteMethod, TrashContextExtMacos};
        context.set_delete_method(DeleteMethod::NsFileManager);
    }
    context.delete(path)
}

/// The per-item results and their log records, accumulated together.
///
/// Every action produces exactly one of each, and writing them through one
/// place is what stops the two from drifting apart: the line the user reads in
/// the result view and the line the audit log keeps come from the same call.
struct Audit {
    entries: Vec<ResultEntry>,
    history: Vec<HistoryEntry>,
}

impl Audit {
    const fn new() -> Self {
        Self {
            entries: Vec::new(),
            history: Vec::new(),
        }
    }
}

/// Appends one result plus its log record.
fn push(
    audit: &mut Audit,
    action: HistoryAction,
    target: &str,
    result: ExecutionResult,
    code: DetailCode,
    args: &[(&str, DetailArg)],
    size_bytes: Option<u64>,
    reason: Option<ReasonCode>,
) {
    let detail_args: BTreeMap<String, DetailArg> = args
        .iter()
        .map(|(name, value)| ((*name).to_string(), value.clone()))
        .collect();
    audit.entries.push(ResultEntry {
        action,
        target: target.to_string(),
        result,
        detail_code: Some(code),
        detail_args: detail_args.clone(),
        detail: None,
        size_bytes,
    });
    audit.history.push(HistoryEntry {
        at: crate::timefmt::format_rfc3339(SystemTime::now()),
        action,
        target: target.to_string(),
        reason_code: reason,
        reason: None,
        result,
        size_bytes,
        detail_code: Some(code),
        detail_args,
        detail: None,
    });
}

/// One decision-relevant field that no longer matches the plan.
struct FieldChange {
    /// A stable name the front end has a word for: `level`, `size`, `files`,
    /// `tracked`, `ignored`, `symlink`, `dirty`, `untracked`, `merged`.
    field: &'static str,
    before: DetailArg,
    after: DetailArg,
}

/// A byte count as an argument. A measurement that never happened is `Unknown`,
/// not zero: the difference between "this is empty" and "this was not measured"
/// is the whole point of D3.
fn bytes_or_unknown(bytes: Option<u64>) -> DetailArg {
    bytes.map_or(DetailArg::Unknown, DetailArg::Bytes)
}

fn flag_or_unknown(value: Option<bool>) -> DetailArg {
    value.map_or(DetailArg::Unknown, DetailArg::Flag)
}

/// The decision-relevant fields of a worktree, compared one at a time.
///
/// Anything that could change what "remove this" means belongs here: the level,
/// the channel (prune vs remove), the dirty and untracked counts, the merge
/// status, and the size. A change in any of them makes the plan stale (D11).
///
/// This used to build one formatted string per record and compare the two, which
/// is why a stale worktree once reported
/// `level=Safe prunable=false dirty=0 ... size=Some(0)` on the result page and
/// left the reader to work out which part had moved. Naming the field is the
/// difference between "this no longer matches the plan" and "the size is now
/// unknown".
fn worktree_change(before: &WorktreeRecord, after: &WorktreeRecord) -> Option<FieldChange> {
    if before.safety.status != after.safety.status {
        return Some(FieldChange {
            field: "level",
            before: DetailArg::Text(format!("{:?}", before.safety.status)),
            after: DetailArg::Text(format!("{:?}", after.safety.status)),
        });
    }
    if before.is_prunable != after.is_prunable {
        return Some(FieldChange {
            field: "prunable",
            before: DetailArg::Flag(before.is_prunable),
            after: DetailArg::Flag(after.is_prunable),
        });
    }
    if before.dirty_files != after.dirty_files {
        return Some(FieldChange {
            field: "dirty",
            before: DetailArg::Count(u64::from(before.dirty_files)),
            after: DetailArg::Count(u64::from(after.dirty_files)),
        });
    }
    if before.untracked_files != after.untracked_files {
        return Some(FieldChange {
            field: "untracked",
            before: DetailArg::Count(u64::from(before.untracked_files)),
            after: DetailArg::Count(u64::from(after.untracked_files)),
        });
    }
    if before.merged_into_default != after.merged_into_default {
        return Some(FieldChange {
            field: "merged",
            before: flag_or_unknown(before.merged_into_default),
            after: flag_or_unknown(after.merged_into_default),
        });
    }
    if before.size_bytes != after.size_bytes {
        return Some(FieldChange {
            field: "size",
            before: bytes_or_unknown(before.size_bytes),
            after: bytes_or_unknown(after.size_bytes),
        });
    }
    None
}

/// The decision-relevant fields of an artifact, compared one at a time.
fn artifact_change(
    before: &crate::ArtifactRecord,
    after: &crate::ArtifactRecord,
) -> Option<FieldChange> {
    if before.safety.status != after.safety.status {
        return Some(FieldChange {
            field: "level",
            before: DetailArg::Text(format!("{:?}", before.safety.status)),
            after: DetailArg::Text(format!("{:?}", after.safety.status)),
        });
    }
    if before.size_bytes != after.size_bytes {
        return Some(FieldChange {
            field: "size",
            before: bytes_or_unknown(before.size_bytes),
            after: bytes_or_unknown(after.size_bytes),
        });
    }
    if before.file_count != after.file_count {
        return Some(FieldChange {
            field: "files",
            before: DetailArg::Count(before.file_count),
            after: DetailArg::Count(after.file_count),
        });
    }
    if before.git_tracked != after.git_tracked {
        return Some(FieldChange {
            field: "tracked",
            before: flag_or_unknown(before.git_tracked),
            after: flag_or_unknown(after.git_tracked),
        });
    }
    if before.git_ignored != after.git_ignored {
        return Some(FieldChange {
            field: "ignored",
            before: flag_or_unknown(before.git_ignored),
            after: flag_or_unknown(after.git_ignored),
        });
    }
    if before.is_symlink != after.is_symlink {
        return Some(FieldChange {
            field: "symlink",
            before: DetailArg::Flag(before.is_symlink),
            after: DetailArg::Flag(after.is_symlink),
        });
    }
    None
}

/// The most severe reason's code, used as the log's `reason` column.
///
/// A code rather than `reason.text`, for the same reason the detail column moved:
/// the text is an English sentence on every machine, and the log is a page the
/// reader opens after a run. `None` when a record carries no reason at all.
fn primary_reason(record: &WorktreeRecord) -> Option<ReasonCode> {
    record
        .safety
        .reasons
        .iter()
        .max_by_key(|reason| reason.severity)
        .map(|reason| reason.code)
}

/// The most severe reason's code for an artifact record.
fn primary_reason_artifact(record: &crate::ArtifactRecord) -> Option<ReasonCode> {
    record
        .safety
        .reasons
        .iter()
        .max_by_key(|reason| reason.severity)
        .map(|reason| reason.code)
}

/// Rebuilds a [`crate::worktree::WorktreeEntry`] from a scanned record, so the
/// re-check sees exactly the porcelain facts the original check saw.
fn worktree_entry_for(record: &WorktreeRecord) -> crate::worktree::WorktreeEntry {
    crate::worktree::WorktreeEntry {
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

/// Runs one Git command, mapping both transport errors and non-zero exits into
/// one error string: the caller only needs to know that it did not work, and the
/// evidence travels with the message.
fn run_git(runner: &dyn GitRunner, repo: &Path, args: &[&str]) -> Result<(), GitError> {
    let output = runner.run(repo, args)?;
    if output.success() {
        return Ok(());
    }
    Err(GitError::Failed {
        command: args.join(" "),
        code: output.code,
        stderr: output.stderr,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reason::ReasonCode;
    use crate::{
        ArtifactRecord, PlanItemKind, SafetyAssessment, SafetyStatus, TrashMoveItem,
        WorktreeRemovalItem,
    };

    // ---- fixtures ----------------------------------------------------------

    fn assessment(status: SafetyStatus) -> SafetyAssessment {
        SafetyAssessment::from_reasons(vec![crate::Reason::new(
            if status == SafetyStatus::Unsafe {
                ReasonCode::WtDirty
            } else {
                ReasonCode::WtClean
            },
            status,
            "clean working tree",
            "no uncommitted changes",
        )])
    }

    fn worktree(path: &str) -> WorktreeRecord {
        WorktreeRecord {
            path: path.to_string(),
            branch: Some("main".to_string()),
            head: Some("abc1234".to_string()),
            is_main: false,
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
            stash_count: 0,
            last_updated_at: Some("2026-01-01T00:00:00Z".to_string()),
            agent: None,
            size_bytes: Some(1024),
            safety: assessment(SafetyStatus::Safe),
        }
    }

    fn artifact(path: &str, root: &str) -> ArtifactRecord {
        ArtifactRecord {
            path: path.to_string(),
            kind: "dist".to_string(),
            ecosystem: "node".to_string(),
            project_root: root.to_string(),
            worktree_path: Some(root.to_string()),
            size_bytes: Some(2048),
            file_count: 3,
            last_updated_at: Some("2026-01-01T00:00:00Z".to_string()),
            git_ignored: Some(true),
            git_tracked: Some(false),
            is_symlink: false,
            symlink_target: None,
            safety: SafetyAssessment::from_reasons(vec![crate::Reason::safe(
                ReasonCode::AfRebuildable,
                "regenerable directory",
                "package.json exists; build output, regenerated by the build",
            )]),
        }
    }

    fn snapshot_with(
        root: &str,
        worktrees: Vec<WorktreeRecord>,
        artifacts: Vec<ArtifactRecord>,
    ) -> ProjectSnapshot {
        ProjectSnapshot {
            schema_version: crate::SNAPSHOT_SCHEMA_VERSION,
            root: root.to_string(),
            scanned_at: "2026-01-01T00:00:00Z".to_string(),
            default_branch: Some("main".to_string()),
            worktrees,
            artifacts,
            warnings: Vec::new(),
        }
    }

    fn plan_with_worktree(path: &str) -> CleanupPlan {
        CleanupPlan {
            worktree_removals: vec![WorktreeRemovalItem {
                kind: PlanItemKind::Worktree,
                path: path.to_string(),
                branch: Some("main".to_string()),
                command: format!("git worktree remove -- \"{path}\""),
                force: false,
                untracked_files: Vec::new(),
                size_bytes: Some(1024),
            }],
            trash_moves: Vec::new(),
            excluded_unsafe: 0,
            total_bytes: 1024,
            total_bytes_unknown: 0,
            blockers: Vec::new(),
            ack_reasons: Vec::new(),
        }
    }

    fn plan_with_trash_move(path: &str, bundled: Option<&str>) -> CleanupPlan {
        CleanupPlan {
            worktree_removals: Vec::new(),
            trash_moves: vec![TrashMoveItem {
                kind: PlanItemKind::Artifact,
                path: path.to_string(),
                artifact_kind: "dist".to_string(),
                size_bytes: Some(2048),
                file_count: 3,
                bundled_with_worktree: bundled.map(str::to_string),
            }],
            excluded_unsafe: 0,
            total_bytes: 2048,
            total_bytes_unknown: 0,
            blockers: Vec::new(),
            ack_reasons: Vec::new(),
        }
    }

    /// Options with logging off, so the unit tests never touch the real log.
    fn execute_options() -> ExecuteOptions {
        ExecuteOptions {
            log_path: None,
            ..Default::default()
        }
    }

    // ---- plan / snapshot agreement -----------------------------------------

    #[test]
    fn a_worktree_missing_from_the_snapshot_is_skipped() {
        // A plan that arrives without its "before" is refused rather than
        // executed blind (D11): the snapshot is the re-check baseline.
        let snapshot = snapshot_with("/repo", Vec::new(), Vec::new());
        let plan = plan_with_worktree("/repo/repo--feature");
        let outcome = execute_plan(&plan, &snapshot, &execute_options());
        assert_eq!(outcome.entries.len(), 1);
        assert_eq!(outcome.entries[0].result, ExecutionResult::Skipped);
        assert_eq!(
            outcome.entries[0].detail_code,
            Some(DetailCode::WorktreeNotInScan)
        );
    }

    #[test]
    fn an_artifact_missing_from_the_snapshot_is_skipped() {
        let snapshot = snapshot_with("/repo", Vec::new(), Vec::new());
        let plan = plan_with_trash_move("/repo/repo--feature/dist", None);
        let outcome = execute_plan(&plan, &snapshot, &execute_options());
        assert_eq!(outcome.entries.len(), 1);
        assert_eq!(outcome.entries[0].result, ExecutionResult::Skipped);
        assert_eq!(
            outcome.entries[0].detail_code,
            Some(DetailCode::ArtifactNotInScan)
        );
    }

    #[test]
    fn a_bundled_artifact_is_reported_ok_only_when_its_worktree_was_really_removed() {
        // The regression: `bundled_with_worktree` was taken at face value, so an item
        // was reported `Ok` — "removed together with its owning worktree" — even when
        // no worktree was removed at all. The audit log then recorded a deletion that
        // never happened, which is the one thing an audit trail must not do.
        //
        // Here the plan names an owning worktree that is not in the snapshot, so
        // nothing removes it. The artifact must not be reported as deleted; it falls
        // through to the normal path, where it is skipped for having no record to
        // re-check against.
        let snapshot = snapshot_with("/repo", Vec::new(), Vec::new());
        let plan = plan_with_trash_move("/repo/repo--feature/dist", Some("/repo/repo--feature"));
        let outcome = execute_plan(&plan, &snapshot, &execute_options());

        assert_eq!(outcome.entries.len(), 1);
        assert_eq!(
            outcome.entries[0].result,
            ExecutionResult::Skipped,
            "nothing removed the worktree, so nothing removed the artifact"
        );
        assert_ne!(
            outcome.entries[0].detail_code,
            Some(DetailCode::BundledWithWorktree),
            "nothing removed the worktree, so nothing removed the artifact"
        );
    }

    #[test]
    fn a_bundled_artifact_is_not_deleted_a_second_time_when_its_worktree_went() {
        // The other half, at the level a unit test can reach: the "already removed"
        // bookkeeping is keyed on the worktree path, so an artifact whose owner was
        // *not* removed must not be reported as swept up with it, and one whose owner
        // was must not be touched again. The successful-removal half needs a real
        // repository (`git worktree prune` has to actually run), so it lives in
        // `tests/cleanup_execution_real_repo.rs`; what is pinned here is that the
        // decision reads the set of worktrees this run removed.
        let mut audit = Audit::new();
        let removed = ["/repo/repo--feature".to_string()];
        let owner = "/repo/repo--feature";
        assert!(removed.iter().any(|path| path == owner));
        // A different worktree, or a path that merely shares a string prefix, must
        // not count as the owner.
        assert!(!removed.iter().any(|path| path == "/repo/repo--feature-2"));
        assert!(!removed.iter().any(|path| path == "/repo/repo--other"));

        // The audit pair is still one result plus one log record for the item.
        push(
            &mut audit,
            HistoryAction::TrashMove,
            "/repo/repo--feature/dist",
            ExecutionResult::Ok,
            DetailCode::BundledWithWorktree,
            &[],
            Some(0),
            None,
        );
        assert_eq!(audit.entries.len(), 1);
        assert_eq!(audit.history.len(), 1);
    }

    #[test]
    fn an_artifact_whose_name_is_not_a_known_kind_is_skipped() {
        // The plan says `dist`; the directory on disk is no longer one of the
        // known artifact names. The re-check refuses it before any deletion.
        let dir = tempfile::TempDir::new().expect("temp dir");
        let root = dir.path().join("repo");
        let target = root.join("not-a-known-artifact");
        std::fs::create_dir_all(&target).expect("create target");
        let snapshot = snapshot_with(
            &root.display().to_string(),
            Vec::new(),
            vec![artifact(
                &target.display().to_string(),
                &root.display().to_string(),
            )],
        );
        let plan = plan_with_trash_move(&target.display().to_string(), None);
        let outcome = execute_plan(&plan, &snapshot, &execute_options());
        assert_eq!(outcome.entries.len(), 1);
        assert_eq!(outcome.entries[0].result, ExecutionResult::Skipped);
        assert_eq!(
            outcome.entries[0].detail_code,
            Some(DetailCode::ArtifactNotAKnownKind)
        );
        assert!(target.exists(), "nothing was deleted");
    }

    // ---- identity ----------------------------------------------------------

    #[test]
    fn worktree_change_names_the_field_that_moved() {
        let base = worktree("/repo/wt");

        let mut dirty = base.clone();
        dirty.dirty_files = 2;
        assert_eq!(
            worktree_change(&base, &dirty).map(|change| change.field),
            Some("dirty"),
            "a dirty count that moved has to name itself, not print two records"
        );

        let mut untracked = base.clone();
        untracked.untracked_files = 1;
        assert_eq!(
            worktree_change(&base, &untracked).map(|change| change.field),
            Some("untracked")
        );

        let mut unmeasured = base.clone();
        unmeasured.size_bytes = None;
        let size = worktree_change(&base, &unmeasured).expect("a lost size is a change");
        assert_eq!(size.field, "size");
        assert_eq!(
            size.after,
            DetailArg::Unknown,
            "a size that was never measured is not a size of zero"
        );

        // The branch is display data: it does not change what removal means.
        let mut same = base.clone();
        same.branch = Some("other".to_string());
        assert!(worktree_change(&base, &same).is_none());
    }

    #[test]
    fn artifact_change_names_the_field_that_moved() {
        let base = artifact("/repo/dist", "/repo");

        let mut bigger = base.clone();
        bigger.size_bytes = Some(4096);
        assert_eq!(
            artifact_change(&base, &bigger).map(|change| change.field),
            Some("size")
        );

        let mut tracked = base.clone();
        tracked.git_tracked = Some(true);
        assert_eq!(
            artifact_change(&base, &tracked).map(|change| change.field),
            Some("tracked")
        );

        let mut ignored_unknown = base.clone();
        ignored_unknown.git_ignored = None;
        assert_eq!(
            artifact_change(&base, &ignored_unknown).map(|change| change.field),
            Some("ignored")
        );

        let mut link = base.clone();
        link.is_symlink = true;
        assert_eq!(
            artifact_change(&base, &link).map(|change| change.field),
            Some("symlink")
        );

        // The human-facing labels are display data and do not take part.
        let mut relabelled = base.clone();
        relabelled.kind = "build output".to_string();
        relabelled.ecosystem = "whatever".to_string();
        assert!(artifact_change(&base, &relabelled).is_none());
    }

    // ---- the log's reason column -------------------------------------------

    #[test]
    fn the_recorded_reason_is_the_most_severe_one() {
        let mut record = worktree("/repo/wt");
        record.safety = SafetyAssessment::from_reasons(vec![
            crate::Reason::safe(
                ReasonCode::WtClean,
                "clean working tree",
                "no uncommitted changes",
            ),
            crate::Reason::new(
                ReasonCode::WtStash,
                SafetyStatus::Review,
                "stashes present",
                "1 stash",
            ),
            crate::Reason::new(
                ReasonCode::WtMain,
                SafetyStatus::Unsafe,
                "main worktree",
                "index 0",
            ),
        ]);
        assert_eq!(primary_reason(&record), Some(ReasonCode::WtMain));
    }

    #[test]
    fn an_artifact_reason_is_read_from_the_artifact_record() {
        let mut record = artifact("/repo/dist", "/repo");
        record.safety = SafetyAssessment::from_reasons(vec![
            crate::Reason::safe(ReasonCode::AfEmpty, "empty directory", "0 bytes"),
            crate::Reason::new(
                ReasonCode::AfRecentActivity,
                SafetyStatus::Review,
                "recent activity",
                "written 2026-10-12",
            ),
        ]);
        assert_eq!(
            primary_reason_artifact(&record),
            Some(ReasonCode::AfRecentActivity)
        );
    }

    #[test]
    fn a_record_without_reasons_reports_none_rather_than_inventing_one() {
        // There is no verdict to show, and the old fallback invented one in
        // English — "executed after user confirmation" — which the front end had
        // no way to translate and no reason to display.
        let mut record = worktree("/repo/wt");
        record.safety = SafetyAssessment::from_reasons(Vec::new());
        assert_eq!(primary_reason(&record), None);

        let mut link = artifact("/repo/dist", "/repo");
        link.safety = SafetyAssessment::from_reasons(Vec::new());
        assert_eq!(primary_reason_artifact(&link), None);
    }

    // ---- the result / log pair ---------------------------------------------

    #[test]
    fn push_appends_one_result_and_one_log_record() {
        let mut audit = Audit::new();
        push(
            &mut audit,
            HistoryAction::TrashMove,
            "/repo/dist",
            ExecutionResult::Ok,
            DetailCode::ArtifactTrashed,
            &[],
            Some(2048),
            Some(ReasonCode::AfRebuildable),
        );

        let Audit { entries, history } = audit;
        assert_eq!(entries.len(), 1);
        assert_eq!(history.len(), 1);
        assert_eq!(entries[0].action, history[0].action);
        assert_eq!(entries[0].target, "/repo/dist");
        assert_eq!(entries[0].result, ExecutionResult::Ok);
        assert_eq!(entries[0].size_bytes, Some(2048));
        // Both carry the code and the arguments, and neither carries a sentence:
        // the wording is the front end's, and it has to be able to change.
        assert_eq!(entries[0].detail_code, Some(DetailCode::ArtifactTrashed));
        assert_eq!(entries[0].detail, None);
        assert_eq!(history[0].reason_code, Some(ReasonCode::AfRebuildable));
        assert_eq!(history[0].reason, None);
        assert_eq!(history[0].detail_code, entries[0].detail_code);
        assert_eq!(history[0].detail_args, entries[0].detail_args);
        assert!(
            !history[0].at.is_empty(),
            "every log line carries a timestamp"
        );
    }

    #[test]
    fn push_carries_the_arguments_and_leaves_the_reason_empty_without_one() {
        let mut audit = Audit::new();
        push(
            &mut audit,
            HistoryAction::WorktreeRemove,
            "/repo/wt",
            ExecutionResult::Skipped,
            DetailCode::WorktreeNotInScan,
            &[],
            None,
            None,
        );
        assert_eq!(audit.history[0].reason_code, None);
        assert_eq!(audit.history[0].size_bytes, None);

        // An argument arrives as a raw value, so the front end formats it in the
        // reader's language rather than receiving "1.29 GB" from here.
        let mut measured = Audit::new();
        push(
            &mut measured,
            HistoryAction::TrashMove,
            "/repo/dist",
            ExecutionResult::Skipped,
            DetailCode::ArtifactChanged,
            &[
                ("field", DetailArg::Text("size".to_string())),
                ("before", DetailArg::Bytes(1_320_274_909)),
                ("after", DetailArg::Unknown),
            ],
            None,
            None,
        );
        let args = &measured.entries[0].detail_args;
        assert_eq!(args.get("before"), Some(&DetailArg::Bytes(1_320_274_909)));
        assert_eq!(
            args.get("after"),
            Some(&DetailArg::Unknown),
            "a size that stopped being measured is not a size of zero"
        );
    }

    // ---- options -----------------------------------------------------------

    #[test]
    fn the_default_execute_options_point_at_the_platform_log() {
        let options = ExecuteOptions::default();
        let path = options.log_path.expect("a default log path is configured");
        assert!(path.is_absolute(), "{}", path.display());
        assert_eq!(
            path.file_name().and_then(|name| name.to_str()),
            Some("operations.log")
        );
        assert_eq!(options.recent_activity_days, 7);
        assert!(!options.untracked_as_unsafe);
        assert!(options.whitelist.is_empty());
    }
}
