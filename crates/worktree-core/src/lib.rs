//! Worktree Cleaner 核心。
//!
//! 职责范围：Git 解析、状态判断、产物扫描、路径校验。
//!
//! **本 crate 只读**：不执行任何删除动作。删除执行位于独立位置，且只能由
//! 计划对象驱动（开发契约第 2 节）。

pub mod path;
pub mod reason;

pub use path::{validate_path_for_deletion, DeletionContext, PathRejection, ValidatedPath};
pub use reason::{Reason, ReasonCode, SafetyAssessment, SafetyStatus};

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// `snapshot.json` 的契约版本。字段有增删或语义变化时必须递增（开发契约 D20）。
pub const SNAPSHOT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Error)]
pub enum ScanError {
    #[error("scanner is not implemented in the initial project skeleton")]
    NotImplemented,
}

/// 一个 Git worktree 的检查结果。
///
/// 字段与 `src/types.ts` 的 `WorktreeRecord` 逐字段对应，任一侧改动必须在
/// 同一次提交内同步另一侧。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorktreeRecord {
    pub path: String,
    pub branch: Option<String>,
    /// 短 SHA。
    pub head: Option<String>,
    pub is_main: bool,
    pub is_detached: bool,
    pub is_locked: bool,
    /// Git 元数据还在，但工作目录已丢失。
    pub is_prunable: bool,
    pub dirty_files: u32,
    pub untracked_files: u32,
    /// 未跟踪文件清单。删除该 worktree 需要 `--force`，执行前必须逐行展示。
    pub untracked_file_list: Vec<String>,
    pub ahead: u32,
    pub behind: u32,
    pub has_upstream: bool,
    /// `None` 表示无法判定，**不得**当作已合并。
    pub merged_into_default: Option<bool>,
    pub stash_count: u32,
    pub last_updated_at: Option<String>,
    /// 按路径特征推断的占用 Agent；识别不出时为 `None`，不猜。
    pub agent: Option<String>,
    pub size_bytes: u64,
    pub safety: SafetyAssessment,
}

/// 一个候选产物目录的检查结果。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArtifactRecord {
    pub path: String,
    /// 产物类型名，如 `node_modules` / `target` / `dist`。
    pub kind: String,
    /// 所属生态，如 `node` / `rust` / `python`。
    pub ecosystem: String,
    pub project_root: String,
    /// 归属的 worktree；主工作区为 `Some(root)`。
    pub worktree_path: Option<String>,
    /// `None` 表示无法测量（超时 / 无权限 / 符号链接），必须与 `0` 区分。
    pub size_bytes: Option<u64>,
    pub file_count: u64,
    pub last_updated_at: Option<String>,
    pub git_ignored: Option<bool>,
    pub git_tracked: Option<bool>,
    pub is_symlink: bool,
    pub symlink_target: Option<String>,
    pub safety: SafetyAssessment,
}

/// 一次扫描的完整结果，也是 `snapshot.json` 的载荷。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectSnapshot {
    pub schema_version: u32,
    pub root: String,
    pub scanned_at: String,
    pub default_branch: Option<String>,
    pub worktrees: Vec<WorktreeRecord>,
    pub artifacts: Vec<ArtifactRecord>,
    /// 扫描降级说明（超时、无权限等）。
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
        // 契约版本必须可被 CLI 与网页读到，且与 src/types.ts 的常量一致。
        let version = SNAPSHOT_SCHEMA_VERSION;
        assert_eq!(version, 1);
    }

    #[test]
    fn reason_is_serialized_as_structured_object() {
        // 契约：原因不是字符串，而是带 code / severity / text / evidence 的对象（D9）。
        let reason = Reason::safe(ReasonCode::WtClean, "工作区干净", "无未提交改动");
        let json = serde_json::to_value(&reason).expect("序列化原因");
        assert_eq!(json["code"], "wt.clean");
        assert_eq!(json["severity"], "Safe");
        assert_eq!(json["text"], "工作区干净");
        assert_eq!(json["evidence"], "无未提交改动");
    }

    #[test]
    fn safety_status_wire_format() {
        for (status, expected) in [
            (SafetyStatus::Safe, "Safe"),
            (SafetyStatus::Review, "Review"),
            (SafetyStatus::Unsafe, "Unsafe"),
        ] {
            assert_eq!(serde_json::to_string(&status).expect("序列化等级"), format!("\"{expected}\""));
        }
    }
}
