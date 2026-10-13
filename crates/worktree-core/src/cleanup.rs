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

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

use crate::artifact::{check_artifact, lookup_kind, ArtifactScanOptions};
use crate::git::{GitError, GitRunner, ProcessGitRunner};
use crate::log;
use crate::path::{validate_path_for_deletion, DeletionContext};
use crate::reason::SafetyStatus;
use crate::worktree::{check_worktree, WorktreeCheckOptions};
use crate::{
    CleanupPlan, ExecutionResult, HistoryAction, HistoryEntry, ProjectSnapshot, ResultEntry,
    WorktreeRecord,
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

    // ---- Channel A: worktrees ----
    for item in &plan.worktree_removals {
        let target = item.path.as_str();
        let Some(record) = snapshot.worktrees.iter().find(|w| w.path == item.path) else {
            push(
                &mut audit,
                HistoryAction::WorktreeRemove,
                target,
                ExecutionResult::Skipped,
                "the worktree is no longer in the scan result; nothing was executed",
                None,
                None,
            );
            continue;
        };

        // ---- path re-validation (D10) ----
        let parent = Path::new(&record.path)
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from(&snapshot.root));
        let context = DeletionContext::new(parent).with_whitelist(whitelist.clone());
        if let Err(rejection) = validate_path_for_deletion(Path::new(&record.path), &context) {
            push(
                &mut audit,
                HistoryAction::WorktreeRemove,
                target,
                ExecutionResult::Skipped,
                &format!("path check failed: {rejection}"),
                record.size_bytes,
                Some(primary_reason(record)),
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
            },
        );
        if worktree_identity(record) != worktree_identity(&rechecked) {
            push(
                &mut audit,
                HistoryAction::WorktreeRemove,
                target,
                ExecutionResult::Skipped,
                &format!(
                    "the worktree changed since the plan was generated (was {}, is now {}); not executed",
                    worktree_identity(record),
                    worktree_identity(&rechecked)
                ),
                record.size_bytes,
                Some(primary_reason(record)),
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
                &format!(
                    "the re-check no longer reports this worktree as safe to delete: {}",
                    primary_reason(&rechecked)
                ),
                record.size_bytes,
                Some(primary_reason(record)),
            );
            continue;
        }

        // ---- execute ----
        if record.is_prunable {
            match run_git(&runner, Path::new(&snapshot.root), &["worktree", "prune"]) {
                Ok(()) => {
                    any_worktree_removed = true;
                    push(
                        &mut audit,
                        HistoryAction::Prune,
                        target,
                        ExecutionResult::Ok,
                        "git worktree prune cleaned up leftover metadata; no files were touched",
                        record.size_bytes,
                        Some(primary_reason(record)),
                    );
                }
                Err(error) => push(
                    &mut audit,
                    HistoryAction::Prune,
                    target,
                    ExecutionResult::Failed,
                    &format!("git worktree prune failed: {error}"),
                    record.size_bytes,
                    Some(primary_reason(record)),
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
                let detail: String = match pruned {
                    Ok(()) => "git worktree remove succeeded; git worktree prune cleaned up leftover metadata".to_string(),
                    Err(error) => format!(
                        "git worktree remove succeeded, but the follow-up prune failed: {error}"
                    ),
                };
                push(
                    &mut audit,
                    HistoryAction::WorktreeRemove,
                    target,
                    ExecutionResult::Ok,
                    &detail,
                    record.size_bytes,
                    Some(primary_reason(record)),
                );
            }
            Err(error) => push(
                &mut audit,
                HistoryAction::WorktreeRemove,
                target,
                ExecutionResult::Failed,
                &format!("git worktree remove failed: {error}"),
                record.size_bytes,
                Some(primary_reason(record)),
            ),
        }
    }

    // ---- Channel B: artifacts through the system Trash ----
    for item in &plan.trash_moves {
        let target = item.path.as_str();
        if item.bundled_with_worktree.is_some() {
            // The owning worktree removal already deleted this directory; a
            // second operation would be a double delete.
            push(
                &mut audit,
                HistoryAction::TrashMove,
                target,
                ExecutionResult::Ok,
                "removed together with its owning worktree; no separate operation",
                Some(0),
                Some("removed with its owning worktree".to_string()),
            );
            continue;
        }

        let Some(record) = snapshot.artifacts.iter().find(|a| a.path == item.path) else {
            push(
                &mut audit,
                HistoryAction::TrashMove,
                target,
                ExecutionResult::Skipped,
                "the artifact is no longer in the scan result; nothing was executed",
                None,
                None,
            );
            continue;
        };

        let context = DeletionContext::new(PathBuf::from(&record.project_root))
            .with_whitelist(whitelist.clone());
        if let Err(rejection) = validate_path_for_deletion(Path::new(&record.path), &context) {
            push(
                &mut audit,
                HistoryAction::TrashMove,
                target,
                ExecutionResult::Skipped,
                &format!("path check failed: {rejection}"),
                record.size_bytes,
                Some(primary_reason_artifact(record)),
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
                "the directory is no longer a recognised artifact kind; not executed",
                record.size_bytes,
                Some(primary_reason_artifact(record)),
            );
            continue;
        };
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
            },
        );
        if artifact_identity(record) != artifact_identity(&rechecked) {
            push(
                &mut audit,
                HistoryAction::TrashMove,
                target,
                ExecutionResult::Skipped,
                &format!(
                    "the artifact changed since the plan was generated (was {}, is now {}); not executed",
                    artifact_identity(record),
                    artifact_identity(&rechecked)
                ),
                record.size_bytes,
                Some(primary_reason_artifact(record)),
            );
            continue;
        }
        if rechecked.safety.status != SafetyStatus::Safe {
            push(
                &mut audit,
                HistoryAction::TrashMove,
                target,
                ExecutionResult::Skipped,
                &format!(
                    "the re-check no longer reports this artifact as safe to delete: {}",
                    primary_reason_artifact(&rechecked)
                ),
                record.size_bytes,
                Some(primary_reason_artifact(record)),
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
                "moved to the system Trash; recoverable from Trash",
                record.size_bytes,
                Some(primary_reason_artifact(record)),
            ),
            Err(error) => push(
                &mut audit,
                HistoryAction::TrashMove,
                target,
                ExecutionResult::Failed,
                &format!("could not move to the system Trash: {error}"),
                record.size_bytes,
                Some(primary_reason_artifact(record)),
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
                "git worktree prune finalised the metadata cleanup",
                None,
                Some("metadata cleanup".to_string()),
            ),
            Err(error) => push(
                &mut audit,
                HistoryAction::Prune,
                &snapshot.root,
                ExecutionResult::Failed,
                &format!("git worktree prune failed: {error}"),
                None,
                Some("metadata cleanup".to_string()),
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
    fn new() -> Self {
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
    detail: &str,
    size_bytes: Option<u64>,
    reason: Option<String>,
) {
    audit.entries.push(ResultEntry {
        action,
        target: target.to_string(),
        result,
        detail: detail.to_string(),
        size_bytes,
    });
    audit.history.push(HistoryEntry {
        at: crate::timefmt::format_rfc3339(SystemTime::now()),
        action,
        target: target.to_string(),
        reason: reason.unwrap_or_else(|| detail.to_string()),
        result,
        size_bytes,
        detail: detail.to_string(),
    });
}

/// The decision-relevant facts of a worktree, as one comparable string.
///
/// Anything that could change what "remove this" means belongs here: the level,
/// the channel (prune vs remove), the dirty and untracked counts, the merge
/// status, and the size. A change in any of them makes the plan stale (D11).
fn worktree_identity(record: &WorktreeRecord) -> String {
    format!(
        "level={:?} prunable={} dirty={} untracked={} merged={:?} size={:?}",
        record.safety.status,
        record.is_prunable,
        record.dirty_files,
        record.untracked_files,
        record.merged_into_default,
        record.size_bytes
    )
}

/// The decision-relevant facts of an artifact, as one comparable string.
fn artifact_identity(record: &crate::ArtifactRecord) -> String {
    format!(
        "level={:?} size={:?} files={} tracked={:?} ignored={:?} symlink={}",
        record.safety.status,
        record.size_bytes,
        record.file_count,
        record.git_tracked,
        record.git_ignored,
        record.is_symlink
    )
}

/// The most severe reason's text, used as the log's `reason` column.
fn primary_reason(record: &WorktreeRecord) -> String {
    record
        .safety
        .reasons
        .iter()
        .max_by_key(|reason| reason.severity)
        .map(|reason| reason.text.clone())
        .unwrap_or_else(|| "executed after user confirmation".to_string())
}

/// The most severe reason's text for an artifact record.
fn primary_reason_artifact(record: &crate::ArtifactRecord) -> String {
    record
        .safety
        .reasons
        .iter()
        .max_by_key(|reason| reason.severity)
        .map(|reason| reason.text.clone())
        .unwrap_or_else(|| "executed after user confirmation".to_string())
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
        assert!(outcome.entries[0]
            .detail
            .contains("no longer in the scan result"));
    }

    #[test]
    fn an_artifact_missing_from_the_snapshot_is_skipped() {
        let snapshot = snapshot_with("/repo", Vec::new(), Vec::new());
        let plan = plan_with_trash_move("/repo/repo--feature/dist", None);
        let outcome = execute_plan(&plan, &snapshot, &execute_options());
        assert_eq!(outcome.entries.len(), 1);
        assert_eq!(outcome.entries[0].result, ExecutionResult::Skipped);
        assert!(outcome.entries[0]
            .detail
            .contains("no longer in the scan result"));
    }

    #[test]
    fn a_bundled_artifact_is_reported_ok_without_a_second_delete() {
        // The owning worktree removal already deleted this directory; a second
        // operation would be a double delete, so the item is reported and the
        // loop moves on before it ever looks at the file system.
        let snapshot = snapshot_with("/repo", Vec::new(), Vec::new());
        let plan = plan_with_trash_move("/repo/repo--feature/dist", Some("/repo/repo--feature"));
        let outcome = execute_plan(&plan, &snapshot, &execute_options());
        assert_eq!(outcome.entries.len(), 1);
        assert_eq!(outcome.entries[0].result, ExecutionResult::Ok);
        assert!(outcome.entries[0].detail.contains("no separate operation"));
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
        assert!(
            outcome.entries[0]
                .detail
                .contains("recognised artifact kind"),
            "{:?}",
            outcome.entries[0]
        );
        assert!(target.exists(), "nothing was deleted");
    }

    // ---- identity ----------------------------------------------------------

    #[test]
    fn worktree_identity_changes_with_the_facts_but_not_with_the_branch() {
        let base = worktree("/repo/wt");

        let mut dirty = base.clone();
        dirty.dirty_files = 2;
        assert_ne!(worktree_identity(&base), worktree_identity(&dirty));

        let mut untracked = base.clone();
        untracked.untracked_files = 1;
        assert_ne!(worktree_identity(&base), worktree_identity(&untracked));

        let mut unmeasured = base.clone();
        unmeasured.size_bytes = None;
        assert_ne!(worktree_identity(&base), worktree_identity(&unmeasured));

        // The branch is display data: it does not change what removal means.
        let mut same = base.clone();
        same.branch = Some("other".to_string());
        assert_eq!(worktree_identity(&base), worktree_identity(&same));
    }

    #[test]
    fn artifact_identity_tracks_size_tracking_flags_and_the_symlink_state() {
        let base = artifact("/repo/dist", "/repo");

        let mut bigger = base.clone();
        bigger.size_bytes = Some(4096);
        assert_ne!(artifact_identity(&base), artifact_identity(&bigger));

        let mut tracked = base.clone();
        tracked.git_tracked = Some(true);
        assert_ne!(artifact_identity(&base), artifact_identity(&tracked));

        let mut ignored_unknown = base.clone();
        ignored_unknown.git_ignored = None;
        assert_ne!(
            artifact_identity(&base),
            artifact_identity(&ignored_unknown)
        );

        let mut link = base.clone();
        link.is_symlink = true;
        assert_ne!(artifact_identity(&base), artifact_identity(&link));

        // The human-facing labels are display data and do not take part.
        let mut relabelled = base.clone();
        relabelled.kind = "build output".to_string();
        relabelled.ecosystem = "whatever".to_string();
        assert_eq!(artifact_identity(&base), artifact_identity(&relabelled));
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
        assert_eq!(primary_reason(&record), "main worktree");
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
        assert_eq!(primary_reason_artifact(&record), "recent activity");
    }

    #[test]
    fn a_record_without_reasons_falls_back_to_the_confirmation_wording() {
        let mut record = worktree("/repo/wt");
        record.safety = SafetyAssessment::from_reasons(Vec::new());
        assert_eq!(primary_reason(&record), "executed after user confirmation");

        let mut link = artifact("/repo/dist", "/repo");
        link.safety = SafetyAssessment::from_reasons(Vec::new());
        assert_eq!(
            primary_reason_artifact(&link),
            "executed after user confirmation"
        );
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
            "moved to the system Trash; recoverable from Trash",
            Some(2048),
            Some("regenerable directory".to_string()),
        );

        let Audit { entries, history } = audit;
        assert_eq!(entries.len(), 1);
        assert_eq!(history.len(), 1);
        assert_eq!(entries[0].action, history[0].action);
        assert_eq!(entries[0].target, "/repo/dist");
        assert_eq!(entries[0].result, ExecutionResult::Ok);
        assert_eq!(entries[0].size_bytes, Some(2048));
        assert_eq!(history[0].reason, "regenerable directory");
        assert_eq!(history[0].detail, entries[0].detail);
        assert!(
            !history[0].at.is_empty(),
            "every log line carries a timestamp"
        );
    }

    #[test]
    fn push_falls_back_to_the_detail_when_there_is_no_reason() {
        let mut audit = Audit::new();
        push(
            &mut audit,
            HistoryAction::WorktreeRemove,
            "/repo/wt",
            ExecutionResult::Skipped,
            "the worktree is no longer in the scan result; nothing was executed",
            None,
            None,
        );
        assert_eq!(
            audit.history[0].reason,
            "the worktree is no longer in the scan result; nothing was executed"
        );
        assert_eq!(audit.history[0].size_bytes, None);
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
