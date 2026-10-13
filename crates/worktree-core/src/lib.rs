//! Worktree Cleaner core.
//!
//! Scope: Git parsing, status assessment, artifact scanning, and path validation.
//!
//! **This crate is read-only**: it never performs any deletion. Deletion lives in a
//! separate place and can only be driven by a plan object (development contract §2).

pub mod git;
pub mod path;
pub mod reason;
pub mod timefmt;
pub mod worktree;

pub use git::{GitError, GitOutput, GitRunner, ProcessGitRunner, DEFAULT_GIT_TIMEOUT};
pub use path::{validate_path_for_deletion, DeletionContext, PathRejection, ValidatedPath};
pub use reason::{Reason, ReasonCode, SafetyAssessment, SafetyStatus};
pub use worktree::{
    check_worktree, detect_default_branch, infer_agent, parse_status, parse_worktree_list,
    WorktreeCheckOptions, WorktreeEntry,
};

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Contract version of `snapshot.json`. Must be bumped whenever fields are added,
/// removed, or change meaning (development contract D20).
///
/// Version 2 widened `WorktreeRecord.size_bytes` from `u64` to `Option<u64>`: an
/// unmeasurable size must stay distinguishable from `0`, and a version 1 consumer
/// would render `null` as `0 B`.
pub const SNAPSHOT_SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Error)]
pub enum ScanError {
    /// The orchestrator (default-branch detection, artifact walk, snapshot assembly)
    /// lands with the CLI stage. The worktree checks it will call already exist in
    /// [`crate::worktree`].
    #[error("the scan orchestrator is not implemented yet; the worktree checks live in worktree::check_worktree")]
    NotImplemented,
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
    /// Scan downgrade notes (timeout, permission errors, etc.).
    pub warnings: Vec<String>,
}

pub fn scan_project(_path: &str) -> Result<ProjectSnapshot, ScanError> {
    Err(ScanError::NotImplemented)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_schema_version_is_exposed() {
        // The contract version must be readable by both the CLI and the web UI, and
        // match the constant in src/types.ts.
        let version = SNAPSHOT_SCHEMA_VERSION;
        assert_eq!(version, 2);
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

    #[test]
    fn project_snapshot_round_trips_through_json() {
        let snapshot = ProjectSnapshot {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            root: "/Users/andy/MySrc".to_string(),
            scanned_at: "2026-01-01T00:00:00Z".to_string(),
            default_branch: Some("main".to_string()),
            worktrees: vec![sample_worktree()],
            artifacts: vec![sample_artifact()],
            warnings: vec!["size measurement timed out for .nuxt".to_string()],
        };
        let json = serde_json::to_string(&snapshot).expect("serialize snapshot");
        assert!(json.contains("\"schema_version\":2"));
        let back: ProjectSnapshot = serde_json::from_str(&json).expect("deserialize snapshot");
        assert_eq!(snapshot, back);
    }
}
