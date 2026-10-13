//! Worktree Cleaner core.
//!
//! Scope: Git parsing, status assessment, artifact scanning, and path validation.
//!
//! **This crate is read-only**: it never performs any deletion. Deletion lives in a
//! separate place and can only be driven by a plan object (development contract §2).

pub mod path;
pub mod reason;

pub use path::{validate_path_for_deletion, DeletionContext, PathRejection, ValidatedPath};
pub use reason::{Reason, ReasonCode, SafetyAssessment, SafetyStatus};

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Contract version of `snapshot.json`. Must be bumped whenever fields are added,
/// removed, or change meaning (development contract D20).
pub const SNAPSHOT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Error)]
pub enum ScanError {
    #[error("scanner is not implemented in the initial project skeleton")]
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
    pub size_bytes: u64,
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
        assert_eq!(version, 1);
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
}
