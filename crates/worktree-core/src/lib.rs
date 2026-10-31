//! Worktree Cleaner core.
//!
//! Scope: Git parsing, status assessment, artifact scanning, and path validation.
//!
//! **This crate is read-only**: it never performs any deletion. Deletion lives in
//! a separate place and can only be driven by a plan object (development contract §2).

pub mod artifact;
pub mod cleanup;
pub mod git;
pub mod hygiene;
pub mod log;
pub mod path;
pub mod reason;
pub mod timefmt;
pub mod worktree;

pub use git::{GitError, GitOutput, GitRunner, ProcessGitRunner, DEFAULT_GIT_TIMEOUT};
pub use hygiene::{
    audit_hygiene, HygieneFinding, HygieneLevel, HygieneOptions, HygieneReport, HygieneRule,
    HygieneRuleStatus, VolumeSpace,
};
pub use path::{validate_path_for_deletion, DeletionContext, PathRejection, ValidatedPath};
pub use reason::{Reason, ReasonCode, SafetyAssessment, SafetyStatus};
pub use worktree::{
    check_worktree, detect_default_branch, infer_agent, parse_status, parse_worktree_list,
    WorktreeCheckOptions, WorktreeEntry,
};

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::artifact::{measure_dir, scan_artifacts, ArtifactScanOptions};
use crate::path::{expand_tilde, is_in_system_trash};
use crate::timefmt::format_rfc3339;

/// Contract version of `snapshot.json`. Must be bumped whenever fields are added,
/// removed, or change meaning (development contract D20).
///
/// Version 2 widened `WorktreeRecord.size_bytes` from `u64` to `Option<u64>`: an
/// unmeasurable size must stay distinguishable from `0`, and a version 1 consumer
/// would render `null` as `0 B`.
///
/// Version 3 added `hygiene`: the read-only policy audit of `docs/DISK_HYGIENE.md`.
/// A version 2 consumer must reject the snapshot rather than quietly show a
/// repository with no compliance report at all.
pub const SNAPSHOT_SCHEMA_VERSION: u32 = 3;

/// Why a scan could not be produced.
///
/// Every variant is a distinct, honest answer: an empty path, a path that is not
/// there, and a path that is not a Git repository must never collapse into one
/// generic failure the UI would have to guess about (D3).
#[derive(Debug, Error)]
pub enum ScanError {
    #[error("the path is empty")]
    Empty,
    #[error("the path does not exist: {0}")]
    NotFound(String),
    #[error("not a Git repository: {0}")]
    NotAGitRepository(String),
    #[error("git could not be queried: {0}")]
    GitFailed(String),
}

/// Knobs the caller (Tauri shell, CLI) passes into a scan.
///
/// Durations are milliseconds on the wire so the front end does not have to know
/// Rust's `Duration` encoding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScanOptions {
    /// How deep below a worktree root an artifact candidate may sit.
    pub max_depth: usize,
    /// Budget for one directory size measurement.
    pub per_dir_timeout_ms: u64,
    /// Budget for the whole scan. When it runs out, the unscanned remainder is
    /// reported in `warnings` instead of being silently skipped.
    pub scan_budget_ms: u64,
    /// Activity window in days.
    pub recent_activity_days: u32,
    /// Treat untracked files as `Unsafe` instead of `Review`.
    pub untracked_as_unsafe: bool,
    /// Whitelist rules as written by the user; `~` is expanded here.
    pub whitelist: Vec<String>,
    /// Budget for one Git command.
    pub git_timeout_ms: u64,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            max_depth: 6,
            per_dir_timeout_ms: 30_000,
            scan_budget_ms: 600_000,
            recent_activity_days: 7,
            untracked_as_unsafe: false,
            whitelist: Vec::new(),
            git_timeout_ms: 10_000,
        }
    }
}

/// Which channel a plan item travels through.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PlanItemKind {
    Worktree,
    Artifact,
}

/// One `git worktree remove` / `git worktree prune` in a plan.
///
/// Mirrors `WorktreeRemovalItem` in `src/types.ts` field for field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorktreeRemovalItem {
    pub kind: PlanItemKind,
    pub path: String,
    pub branch: Option<String>,
    /// Display only: the executor re-derives the command from the re-checked
    /// record instead of parsing this string, so a stale plan cannot run a
    /// command the re-check disagrees with.
    pub command: String,
    pub force: bool,
    pub untracked_files: Vec<String>,
    /// `null` = unmeasurable, so the plan does not claim a size it cannot know.
    pub size_bytes: Option<u64>,
}

/// One move to the system Trash in a plan.
///
/// Mirrors `TrashMoveItem` in `src/types.ts` field for field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrashMoveItem {
    pub kind: PlanItemKind,
    pub path: String,
    pub artifact_kind: String,
    pub size_bytes: Option<u64>,
    pub file_count: u64,
    /// Set when the artifact disappears together with a selected worktree; it is
    /// then not a separate deletion.
    pub bundled_with_worktree: Option<String>,
}

/// A cleanup plan, as produced by the UI and consumed by the executor.
///
/// Mirrors `CleanupPlan` in `src/types.ts` field for field.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CleanupPlan {
    pub worktree_removals: Vec<WorktreeRemovalItem>,
    pub trash_moves: Vec<TrashMoveItem>,
    pub excluded_unsafe: u32,
    pub total_bytes: u64,
    /// How many independently deleted items have an unknown size; while this is
    /// above zero, `total_bytes` is a lower bound.
    pub total_bytes_unknown: u32,
    pub blockers: Vec<String>,
    pub ack_reasons: Vec<String>,
}

/// What an executed action was.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HistoryAction {
    WorktreeRemove,
    TrashMove,
    Prune,
}

/// How one executed action ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ExecutionResult {
    Ok,
    Skipped,
    Failed,
}

/// One argument to a [`DetailCode`].
///
/// Only raw values cross this boundary. The front end formats them — a byte
/// count through `formatBytes`, so the unit follows the language the reader is
/// using — and the wording belongs to the message table. A core that formatted
/// "1.29 GB" here would freeze an English unit into a Chinese screen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DetailArg {
    Text(String),
    Bytes(u64),
    Count(u64),
    Flag(bool),
    /// The absence of a value, which is not a zero and not an empty string: an
    /// unmeasurable size, a Git fact that was never established.
    Unknown,
}

/// What one executed action did, as a code rather than a sentence.
///
/// A free English sentence used to travel in `detail` and be printed as it
/// stood, which is why the detail column stayed English after a language switch,
/// and why a re-check failure once put `size=Some(1320274909) files=80703` on the
/// screen. Only the code and its raw arguments cross; the front end owns the
/// sentence. Dotted names, like the reason codes, so both tables read the same
/// way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DetailCode {
    /// The worktree is not in the scan result any more; nothing ran.
    #[serde(rename = "worktree.not_in_scan")]
    WorktreeNotInScan,
    /// The re-check found a field different from the one the plan recorded.
    /// Carries `field`, `before` and `after`.
    #[serde(rename = "worktree.changed")]
    WorktreeChanged,
    /// The re-check no longer reports the worktree as safe. Carries `reason`,
    /// which is a reason code.
    #[serde(rename = "worktree.no_longer_safe")]
    WorktreeNoLongerSafe,
    /// `git worktree remove` succeeded. Carries `prune_error` when the follow-up
    /// prune did not.
    #[serde(rename = "worktree.removed")]
    WorktreeRemoved,
    /// Git refused or failed while removing the worktree. Carries `error`.
    #[serde(rename = "worktree.remove_failed")]
    WorktreeRemoveFailed,
    /// `git worktree prune` finalised the metadata cleanup.
    #[serde(rename = "worktree.pruned")]
    WorktreePruned,
    /// `git worktree prune` failed. Carries `error`.
    #[serde(rename = "worktree.prune_failed")]
    WorktreePruneFailed,
    /// The working directory was already gone; only metadata was cleaned up.
    #[serde(rename = "prunable.metadata_only")]
    PrunableMetadataOnly,
    /// The artifact is not in the scan result any more; nothing ran.
    #[serde(rename = "artifact.not_in_scan")]
    ArtifactNotInScan,
    /// The directory is no longer a name this tool recognises as an artifact.
    #[serde(rename = "artifact.not_a_known_kind")]
    ArtifactNotAKnownKind,
    /// The re-check found a field different from the one the plan recorded.
    /// Carries `field`, `before` and `after`.
    #[serde(rename = "artifact.changed")]
    ArtifactChanged,
    /// The re-check no longer reports the directory as safe to delete. Carries
    /// `reason`, which is a reason code.
    #[serde(rename = "artifact.no_longer_safe")]
    ArtifactNoLongerSafe,
    /// The directory is not on disk any more.
    #[serde(rename = "artifact.vanished")]
    ArtifactVanished,
    /// The re-check could not measure the directory, so nothing was decided.
    #[serde(rename = "artifact.unmeasurable")]
    ArtifactUnmeasurable,
    /// Moved to the system Trash.
    #[serde(rename = "artifact.trashed")]
    ArtifactTrashed,
    /// The move to the system Trash failed. Carries `error`.
    #[serde(rename = "artifact.trash_failed")]
    ArtifactTrashFailed,
    /// A deletion guard refused the item before execution.
    #[serde(rename = "precondition.failed")]
    PreconditionFailed,
    /// Removed together with its worktree; not a separate operation.
    #[serde(rename = "bundled.with_worktree")]
    BundledWithWorktree,
}

/// What the user did, as distinct from what was deleted.
///
/// `operations.log` is an audit trail and keeps its shape: one row per deletion,
/// with the reason and the result. This is the other half — scanning, switching
/// project, generating a plan, changing a setting, confirming a run. They live in
/// their own file on purpose: mixing "the user clicked a button" into a record of
/// "this tool deleted something" would make both harder to read, and the deletion
/// rows are the ones that may matter later.
///
/// Mirrors `ActivityEvent` in `src/types.ts` field for field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivityEvent {
    pub at: String,
    pub action: ActivityAction,
    /// What it happened to: a project path, a setting name, or empty.
    pub target: String,
    #[serde(default)]
    pub args: BTreeMap<String, DetailArg>,
}

/// The user actions worth being able to look up afterwards.
///
/// Hovering and focusing are not here: they are not things a reader would come
/// back for. Nor is every checkbox — a log of every toggle answers nothing, and
/// the selection as it stood when the plan was made is already in `plan.generated`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActivityAction {
    #[serde(rename = "scan.started")]
    ScanStarted,
    #[serde(rename = "scan.finished")]
    ScanFinished,
    #[serde(rename = "scan.failed")]
    ScanFailed,
    #[serde(rename = "scan.cancelled")]
    ScanCancelled,
    #[serde(rename = "project.switched")]
    ProjectSwitched,
    #[serde(rename = "plan.generated")]
    PlanGenerated,
    #[serde(rename = "plan.exported")]
    PlanExported,
    #[serde(rename = "settings.changed")]
    SettingsChanged,
    #[serde(rename = "cleanup.confirmed")]
    CleanupConfirmed,
    #[serde(rename = "trash.opened")]
    TrashOpened,
}

impl ActivityAction {
    /// The string form the front end keys its copy by.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ScanStarted => "scan.started",
            Self::ScanFinished => "scan.finished",
            Self::ScanFailed => "scan.failed",
            Self::ScanCancelled => "scan.cancelled",
            Self::ProjectSwitched => "project.switched",
            Self::PlanGenerated => "plan.generated",
            Self::PlanExported => "plan.exported",
            Self::SettingsChanged => "settings.changed",
            Self::CleanupConfirmed => "cleanup.confirmed",
            Self::TrashOpened => "trash.opened",
        }
    }
}

/// One line of the per-item result table.
///
/// Mirrors `ResultEntry` in `src/types.ts` field for field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResultEntry {
    pub action: HistoryAction,
    pub target: String,
    pub result: ExecutionResult,
    /// `None` on a record written before the codes existed; `detail` then holds
    /// that record's sentence and the front end shows it as it stands.
    #[serde(default)]
    pub detail_code: Option<DetailCode>,
    #[serde(default)]
    pub detail_args: BTreeMap<String, DetailArg>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// `null` = the size was never measurable; must not collapse to 0.
    pub size_bytes: Option<u64>,
}

/// One line of the operation log.
///
/// Mirrors `HistoryEntry` in `src/types.ts` field for field. The log records
/// time / action / target / reason / result / size (D15) and never leaves the
/// machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub at: String,
    pub action: HistoryAction,
    pub target: String,
    /// The verdict this deletion was based on, as a code the front end has a
    /// word for. `None` when the record carries no reason.
    #[serde(default)]
    pub reason_code: Option<ReasonCode>,
    /// Superseded by `reason_code`, kept so a log written by an earlier version
    /// still reads back rather than dropping out of the history.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub result: ExecutionResult,
    pub size_bytes: Option<u64>,
    #[serde(default)]
    pub detail_code: Option<DetailCode>,
    #[serde(default)]
    pub detail_args: BTreeMap<String, DetailArg>,
    /// Superseded by `detail_code`, kept for the same reason as `reason`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// The check result for a single Git worktree.
///
/// Fields map one-to-one onto `WorktreeRecord` in `src/types.ts`; any change on one
/// side must update the other in the same commit.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorktreeRecord {
    pub path: String,
    pub branch: Option<String>,
    /// Short SHA.
    pub head: Option<String>,
    pub is_main: bool,
    pub is_detached: bool,
    pub is_locked: bool,
    /// Git metadata is still present, but the working directory is gone.
    ///
    /// When this is `true`, the count fields below (`dirty_files`,
    /// `untracked_files`, `untracked_file_list`, `ahead`, `behind`) are
    /// **not-checked** values: the working directory is gone, so the checks that
    /// would produce them never ran. Their zeros must never be read as
    /// "nothing to report" — the actionable fact is the prune itself.
    pub is_prunable: bool,
    pub dirty_files: u32,
    pub untracked_files: u32,
    /// List of untracked files. Deleting this worktree requires `--force`, and every
    /// line must be shown before the deletion runs.
    pub untracked_file_list: Vec<String>,
    pub ahead: u32,
    pub behind: u32,
    pub has_upstream: bool,
    /// `None` means undetermined, and **must not** be treated as merged.
    pub merged_into_default: Option<bool>,
    pub stash_count: u32,
    /// The HEAD commit time — the only "last activity" signal. `None` when it could
    /// not be read (a `wt.cannot_verify` reason then holds the verdict at Review).
    pub last_updated_at: Option<String>,
    /// Agent inferred from path characteristics; `None` when it cannot be identified —
    /// never guess.
    pub agent: Option<String>,
    /// Directory size measured by the caller. `None` = unmeasurable (no permission /
    /// timeout), which must stay distinguishable from `0` (development contract
    /// section 3); the UI shows `unknown` rather than `0 B`.
    pub size_bytes: Option<u64>,
    pub safety: SafetyAssessment,
}

/// The check result for a single candidate artifact directory.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArtifactRecord {
    pub path: String,
    /// Artifact kind name, e.g. `node_modules` / `target` / `dist`.
    pub kind: String,
    /// Ecosystem, e.g. `node` / `rust` / `python`.
    pub ecosystem: String,
    pub project_root: String,
    /// Owning worktree; `Some(root)` for the main worktree.
    pub worktree_path: Option<String>,
    /// `None` means unmeasurable (timeout / permission error / symlink), and must be
    /// distinguished from `0`.
    pub size_bytes: Option<u64>,
    pub file_count: u64,
    pub last_updated_at: Option<String>,
    pub git_ignored: Option<bool>,
    pub git_tracked: Option<bool>,
    pub is_symlink: bool,
    pub symlink_target: Option<String>,
    pub safety: SafetyAssessment,
}

/// The complete result of one scan, also the payload of `snapshot.json`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectSnapshot {
    pub schema_version: u32,
    pub root: String,
    pub scanned_at: String,
    pub default_branch: Option<String>,
    pub worktrees: Vec<WorktreeRecord>,
    pub artifacts: Vec<ArtifactRecord>,
    /// Compliance with the disk-hygiene policy (`docs/DISK_HYGIENE.md`), read-only.
    /// It decides nothing about deletion: an item can be perfectly safe to remove
    /// and still break the layout, or the other way round.
    pub hygiene: HygieneReport,
    /// Scan downgrade notes (timeout, permission errors, etc.).
    pub warnings: Vec<String>,
}

/// Runs one scan of `path` and assembles its [`ProjectSnapshot`].
///
/// The orchestrator owns the order of operations and nothing else: worktree
/// decisions come from [`check_worktree`], artifact decisions from
/// [`crate::artifact::check_artifact`], and both derive their level from their
/// reasons (D2). Everything it cannot establish — a size measurement that timed
/// out, a budget that ran out — is reported in `warnings` rather than filled in
/// (D3).
pub fn scan_project(path: &str, options: &ScanOptions) -> Result<ProjectSnapshot, ScanError> {
    let raw = path.trim();
    if raw.is_empty() {
        return Err(ScanError::Empty);
    }
    let root = expand_tilde(raw);
    if !root.is_dir() {
        return Err(ScanError::NotFound(root.display().to_string()));
    }

    let per_dir_timeout = Duration::from_millis(options.per_dir_timeout_ms.max(1));
    let git_timeout = Duration::from_millis(options.git_timeout_ms.max(1));
    let budget = Duration::from_millis(options.scan_budget_ms);
    let runner = ProcessGitRunner::new(git_timeout);
    let started = Instant::now();

    let listing = runner
        .run(&root, &["worktree", "list", "--porcelain", "-z"])
        .map_err(|error| ScanError::GitFailed(error.to_string()))?;
    if !listing.success() {
        return Err(ScanError::NotAGitRepository(format!(
            "git worktree list failed: {}",
            listing.stderr.trim()
        )));
    }
    let entries = parse_worktree_list(&listing.stdout);
    if entries.is_empty() {
        return Err(ScanError::NotAGitRepository(
            "git reported no worktrees".to_string(),
        ));
    }

    let default_branch = detect_default_branch(&runner, &root);
    let whitelist: Vec<PathBuf> = options
        .whitelist
        .iter()
        .map(|rule| expand_tilde(rule))
        .collect();
    let now = SystemTime::now();
    let mut warnings: Vec<String> = Vec::new();

    // ---- repository-level Git facts ----
    // `refs/stash` lives in the common Git directory, so every worktree answers with
    // the same list and `git worktree remove` never touches it. It is read once here
    // rather than once per worktree: a 109-worktree repository was paying 108 extra
    // git processes for an identical answer, and — because the count was reported as
    // a per-worktree reason — every clean worktree of a repository holding a single
    // stash was downgraded. A failed read is a scan warning, never a `0`.
    let repository_stash_count = match runner.run(&root, &["stash", "list", "--format=%gd"]) {
        Ok(output) if output.success() => Some(
            output
                .stdout
                .lines()
                .filter(|line| !line.is_empty())
                .count() as u32,
        ),
        Ok(output) => {
            warnings.push(format!(
                "the repository stash count could not be read: git stash list exited with {:?}: {}",
                output.code,
                output.stderr.trim()
            ));
            None
        }
        Err(error) => {
            warnings.push(format!(
                "the repository stash count could not be read: {error}"
            ));
            None
        }
    };

    // ---- worktrees ----
    let mut worktrees: Vec<WorktreeRecord> = Vec::new();
    let mut trashed_worktrees = 0usize;
    for (index, entry) in entries.iter().enumerate() {
        // A worktree whose directory sits in the system Trash was discarded by
        // the user, not deleted: Git still lists it, but measuring it, reporting
        // it and offering to remove it would reach into the Trash (D5 runs one
        // way). Skipped before any check, and reported below rather than
        // silently dropped.
        if is_in_system_trash(&entry.path) {
            trashed_worktrees += 1;
            continue;
        }
        if started.elapsed() > budget {
            warnings.push(format!(
                "the scan budget of {}s ran out after {} of {} worktrees; the remaining ones were not scanned",
                budget.as_secs(),
                index,
                entries.len()
            ));
            break;
        }
        let is_main = index == 0 && !entry.is_bare;
        let size_bytes = measure_dir(&entry.path, per_dir_timeout).bytes;
        if size_bytes.is_none() {
            warnings.push(format!(
                "size measurement of {} did not complete within {}s; the size is recorded as unknown",
                entry.path.display(),
                per_dir_timeout.as_secs()
            ));
        }
        let check_options = WorktreeCheckOptions {
            default_branch: default_branch.clone(),
            recent_activity_days: options.recent_activity_days,
            untracked_as_unsafe: options.untracked_as_unsafe,
            whitelist: whitelist.clone(),
            now,
            size_bytes,
            repository_stash_count,
        };
        worktrees.push(check_worktree(
            &runner,
            &root,
            entry,
            is_main,
            &check_options,
        ));
    }
    if trashed_worktrees > 0 {
        warnings.push(format!(
            "{trashed_worktrees} worktree(s) inside the system Trash were not scanned; the Trash is left alone"
        ));
    }

    // ---- artifacts ----
    // Only worktrees that still exist can hold artifacts; a prunable entry's
    // directory is gone, and the walk would otherwise report the same nothing
    // twice.
    let present_roots: Vec<PathBuf> = worktrees
        .iter()
        .filter(|record| !record.is_prunable)
        .map(|record| PathBuf::from(&record.path))
        .collect();
    let artifact_options = ArtifactScanOptions {
        max_depth: options.max_depth.max(1),
        per_dir_timeout,
        recent_activity_days: options.recent_activity_days,
        whitelist,
        now,
        budget_deadline: Some(started + budget),
        credential_budget: None,
    };
    let artifact_scan = scan_artifacts(&runner, &present_roots, &artifact_options);
    warnings.extend(artifact_scan.warnings);

    // ---- policy compliance ----
    // The audit reads the verdicts the two passes above already established — a
    // second pass over the same worktrees would pay for the same Git calls twice —
    // and asks only what neither pass asks: which ignore rule covers each declared
    // path, what the cache slots cost, and how much room the volume has left.
    let hygiene = audit_hygiene(
        &runner,
        &root,
        &worktrees,
        &artifact_scan.records,
        &HygieneOptions {
            now,
            idle_days: options.recent_activity_days,
            per_dir_timeout,
            budget_deadline: Some(started + budget),
            ..HygieneOptions::default()
        },
    );
    warnings.extend(hygiene.warnings.iter().cloned());

    Ok(ProjectSnapshot {
        schema_version: SNAPSHOT_SCHEMA_VERSION,
        root: root.display().to_string(),
        scanned_at: format_rfc3339(SystemTime::now()),
        default_branch,
        worktrees,
        artifacts: artifact_scan.records,
        hygiene,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_schema_version_is_exposed() {
        // The contract version must be readable by both the CLI and the web UI, and
        // match the constant in src/types.ts.
        let version = SNAPSHOT_SCHEMA_VERSION;
        assert_eq!(version, 3);
    }

    #[test]
    fn reason_is_serialized_as_structured_object() {
        // Contract: a reason is not a string but an object carrying
        // code / severity / text / evidence (D9).
        let reason = Reason::safe(
            ReasonCode::WtClean,
            "clean working tree",
            "no uncommitted changes",
        );
        let json = serde_json::to_value(&reason).expect("serialize reason");
        assert_eq!(json["code"], "wt.clean");
        assert_eq!(json["severity"], "Safe");
        assert_eq!(json["text"], "clean working tree");
        assert_eq!(json["evidence"], "no uncommitted changes");
    }

    #[test]
    fn safety_status_wire_format() {
        for (status, expected) in [
            (SafetyStatus::Safe, "Safe"),
            (SafetyStatus::Review, "Review"),
            (SafetyStatus::Unsafe, "Unsafe"),
        ] {
            assert_eq!(
                serde_json::to_string(&status).expect("serialize status"),
                format!("\"{expected}\"")
            );
        }
    }

    fn sample_assessment() -> SafetyAssessment {
        SafetyAssessment::from_reasons(vec![Reason::review(
            ReasonCode::WtUnmerged,
            "not merged into main",
            "merge-base does not report an ancestor",
        )])
    }

    fn sample_worktree() -> WorktreeRecord {
        WorktreeRecord {
            path: "/Users/andy/MySrc/proj".to_string(),
            branch: Some("feature/x".to_string()),
            head: Some("abc1234".to_string()),
            is_main: false,
            is_detached: false,
            is_locked: false,
            is_prunable: false,
            dirty_files: 2,
            untracked_files: 1,
            untracked_file_list: vec!["scratch.txt".to_string()],
            ahead: 1,
            behind: 0,
            has_upstream: true,
            merged_into_default: Some(false),
            stash_count: 0,
            last_updated_at: Some("2026-01-01T00:00:00Z".to_string()),
            agent: Some("codex".to_string()),
            // `None` = unmeasurable, which must survive the round trip as `null` and
            // not collapse to `0`.
            size_bytes: Some(1234),
            safety: sample_assessment(),
        }
    }

    fn sample_artifact() -> ArtifactRecord {
        ArtifactRecord {
            path: "/Users/andy/MySrc/proj/node_modules".to_string(),
            kind: "node_modules".to_string(),
            ecosystem: "node".to_string(),
            project_root: "/Users/andy/MySrc/proj".to_string(),
            worktree_path: Some("/Users/andy/MySrc/proj".to_string()),
            // `None` = unmeasurable, which must survive the round trip as `null` and not
            // collapse to `0`.
            size_bytes: None,
            file_count: 10,
            last_updated_at: None,
            git_ignored: Some(true),
            git_tracked: Some(false),
            is_symlink: false,
            symlink_target: None,
            safety: sample_assessment(),
        }
    }

    #[test]
    fn worktree_record_round_trips_through_json() {
        let record = sample_worktree();
        let json = serde_json::to_string(&record).expect("serialize worktree record");
        assert!(json.contains("\"merged_into_default\":false"));
        assert!(json.contains("\"safety\""));
        let back: WorktreeRecord =
            serde_json::from_str(&json).expect("deserialize worktree record");
        assert_eq!(record, back);
    }

    #[test]
    fn worktree_size_none_survives_as_null() {
        // "Cannot be measured" must be distinguishable from "0" on the wire as well
        // (development contract section 3): a `u64` field would force a lie.
        let mut record = sample_worktree();
        record.size_bytes = None;
        let json = serde_json::to_string(&record).expect("serialize worktree record");
        assert!(json.contains("\"size_bytes\":null"));
        let back: WorktreeRecord =
            serde_json::from_str(&json).expect("deserialize worktree record");
        assert_eq!(record, back);
    }

    #[test]
    fn artifact_record_round_trips_through_json() {
        let record = sample_artifact();
        let json = serde_json::to_string(&record).expect("serialize artifact record");
        // An unmeasurable size serializes as null, distinguishable from 0.
        assert!(json.contains("\"size_bytes\":null"));
        let back: ArtifactRecord =
            serde_json::from_str(&json).expect("deserialize artifact record");
        assert_eq!(record, back);
    }

    fn sample_hygiene() -> HygieneReport {
        HygieneReport {
            rules: HygieneRule::ALL
                .iter()
                .map(|rule| HygieneRuleStatus {
                    rule: *rule,
                    level: HygieneLevel::Ok,
                    findings: 0,
                })
                .collect(),
            findings: Vec::new(),
            warnings: Vec::new(),
        }
    }

    #[test]
    fn project_snapshot_round_trips_through_json() {
        let snapshot = ProjectSnapshot {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            root: "/Users/andy/MySrc".to_string(),
            scanned_at: "2026-01-01T00:00:00Z".to_string(),
            default_branch: Some("main".to_string()),
            worktrees: vec![sample_worktree()],
            artifacts: vec![sample_artifact()],
            hygiene: sample_hygiene(),
            warnings: vec!["size measurement timed out for .nuxt".to_string()],
        };
        let json = serde_json::to_string(&snapshot).expect("serialize snapshot");
        assert!(json.contains("\"schema_version\":3"));
        // The audit travels with the snapshot: a consumer that ignores it would show
        // a repository with no compliance report and no sign that one was missing.
        assert!(json.contains("\"hygiene\""));
        assert!(json.contains("\"rules\""));
        let back: ProjectSnapshot = serde_json::from_str(&json).expect("deserialize snapshot");
        assert_eq!(snapshot, back);
    }
}
