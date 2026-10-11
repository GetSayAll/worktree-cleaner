use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ScanError {
    #[error("scanner is not implemented in the initial project skeleton")]
    NotImplemented,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SafetyStatus {
    Safe,
    Review,
    Unsafe,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SafetyAssessment {
    pub status: SafetyStatus,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorktreeRecord {
    pub path: String,
    pub branch: Option<String>,
    pub last_updated_at: Option<String>,
    pub safety: SafetyAssessment,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArtifactRecord {
    pub path: String,
    pub kind: String,
    pub size_bytes: u64,
    pub last_updated_at: Option<String>,
    pub safety: SafetyAssessment,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectSnapshot {
    pub root: String,
    pub worktrees: Vec<WorktreeRecord>,
    pub artifacts: Vec<ArtifactRecord>,
}

pub fn scan_project(_path: &str) -> Result<ProjectSnapshot, ScanError> {
    Err(ScanError::NotImplemented)
}
