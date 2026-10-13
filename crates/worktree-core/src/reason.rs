//! 判定原因模型。
//!
//! 每个安全判定都由一组带证据的 [`Reason`] 组成，整体等级由其中**最严重**的
//! 一条决定（开发契约 D2）。失败（超时 / 无权限 / 命令非零退出）一律产出
//! [`ReasonCode::WtCannotVerify`] 一类的 `cannot_verify` 原因，等级为
//! `Review`，永不降级为 `Safe`（D3）。

use serde::{Deserialize, Serialize};

/// 安全等级。
///
/// 派生顺序即严重程度顺序：`Safe` < `Review` < `Unsafe`，
/// 因此比较两个等级的大小就等于比较严重程度。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum SafetyStatus {
    Safe,
    Review,
    Unsafe,
}

/// 判定原因码。
///
/// 序列化后的字符串（`wt.*` / `af.*`）是对外契约的一部分，**只增不改语义**：
/// 语义变化必须新增原因码，旧码标记废弃但不复用。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ReasonCode {
    // ---- worktree ----
    #[serde(rename = "wt.main")]
    WtMain,
    #[serde(rename = "wt.locked")]
    WtLocked,
    #[serde(rename = "wt.dirty")]
    WtDirty,
    #[serde(rename = "wt.unpushed")]
    WtUnpushed,
    #[serde(rename = "wt.detached")]
    WtDetached,
    #[serde(rename = "wt.no_upstream")]
    WtNoUpstream,
    #[serde(rename = "wt.unmerged")]
    WtUnmerged,
    #[serde(rename = "wt.untracked")]
    WtUntracked,
    #[serde(rename = "wt.stash")]
    WtStash,
    #[serde(rename = "wt.recent_activity")]
    WtRecentActivity,
    #[serde(rename = "wt.agent_active")]
    WtAgentActive,
    #[serde(rename = "wt.cannot_verify")]
    WtCannotVerify,
    #[serde(rename = "wt.whitelisted")]
    WtWhitelisted,
    #[serde(rename = "wt.prunable")]
    WtPrunable,
    #[serde(rename = "wt.merged")]
    WtMerged,
    #[serde(rename = "wt.clean")]
    WtClean,
    // ---- artifact ----
    #[serde(rename = "af.git_tracked")]
    AfGitTracked,
    #[serde(rename = "af.nested_git")]
    AfNestedGit,
    #[serde(rename = "af.authored_content")]
    AfAuthoredContent,
    #[serde(rename = "af.symlink_outside")]
    AfSymlinkOutside,
    #[serde(rename = "af.whitelisted")]
    AfWhitelisted,
    #[serde(rename = "af.recent_activity")]
    AfRecentActivity,
    #[serde(rename = "af.cannot_verify")]
    AfCannotVerify,
    #[serde(rename = "af.empty")]
    AfEmpty,
    #[serde(rename = "af.git_ignored")]
    AfGitIgnored,
    #[serde(rename = "af.rebuildable")]
    AfRebuildable,
}

impl ReasonCode {
    /// 全部原因码。用于契约漂移测试（序列化结果必须与 [`Self::as_str`] 一致）。
    pub const ALL: [ReasonCode; 26] = [
        ReasonCode::WtMain,
        ReasonCode::WtLocked,
        ReasonCode::WtDirty,
        ReasonCode::WtUnpushed,
        ReasonCode::WtDetached,
        ReasonCode::WtNoUpstream,
        ReasonCode::WtUnmerged,
        ReasonCode::WtUntracked,
        ReasonCode::WtStash,
        ReasonCode::WtRecentActivity,
        ReasonCode::WtAgentActive,
        ReasonCode::WtCannotVerify,
        ReasonCode::WtWhitelisted,
        ReasonCode::WtPrunable,
        ReasonCode::WtMerged,
        ReasonCode::WtClean,
        ReasonCode::AfGitTracked,
        ReasonCode::AfNestedGit,
        ReasonCode::AfAuthoredContent,
        ReasonCode::AfSymlinkOutside,
        ReasonCode::AfWhitelisted,
        ReasonCode::AfRecentActivity,
        ReasonCode::AfCannotVerify,
        ReasonCode::AfEmpty,
        ReasonCode::AfGitIgnored,
        ReasonCode::AfRebuildable,
    ];

    /// 对外字符串形式，与 `serde` 的序列化结果必须一致。
    pub const fn as_str(self) -> &'static str {
        match self {
            ReasonCode::WtMain => "wt.main",
            ReasonCode::WtLocked => "wt.locked",
            ReasonCode::WtDirty => "wt.dirty",
            ReasonCode::WtUnpushed => "wt.unpushed",
            ReasonCode::WtDetached => "wt.detached",
            ReasonCode::WtNoUpstream => "wt.no_upstream",
            ReasonCode::WtUnmerged => "wt.unmerged",
            ReasonCode::WtUntracked => "wt.untracked",
            ReasonCode::WtStash => "wt.stash",
            ReasonCode::WtRecentActivity => "wt.recent_activity",
            ReasonCode::WtAgentActive => "wt.agent_active",
            ReasonCode::WtCannotVerify => "wt.cannot_verify",
            ReasonCode::WtWhitelisted => "wt.whitelisted",
            ReasonCode::WtPrunable => "wt.prunable",
            ReasonCode::WtMerged => "wt.merged",
            ReasonCode::WtClean => "wt.clean",
            ReasonCode::AfGitTracked => "af.git_tracked",
            ReasonCode::AfNestedGit => "af.nested_git",
            ReasonCode::AfAuthoredContent => "af.authored_content",
            ReasonCode::AfSymlinkOutside => "af.symlink_outside",
            ReasonCode::AfWhitelisted => "af.whitelisted",
            ReasonCode::AfRecentActivity => "af.recent_activity",
            ReasonCode::AfCannotVerify => "af.cannot_verify",
            ReasonCode::AfEmpty => "af.empty",
            ReasonCode::AfGitIgnored => "af.git_ignored",
            ReasonCode::AfRebuildable => "af.rebuildable",
        }
    }

    /// 属于 worktree 判定还是产物判定。
    pub fn is_worktree(self) -> bool {
        self.as_str().starts_with("wt.")
    }
}

/// 一条判定原因。四个字段缺一不可（D9）：
/// 稳定标识、该原因导向的最严重等级、人类可读文本、可验证证据。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reason {
    pub code: ReasonCode,
    pub severity: SafetyStatus,
    pub text: String,
    /// 证据：具体命令、文件计数或路径。**不允许为空**。
    pub evidence: String,
}

impl Reason {
    /// 构造一条原因。文本与证据都不允许为空，违反时在测试构建下直接断言失败。
    pub fn new(
        code: ReasonCode,
        severity: SafetyStatus,
        text: impl Into<String>,
        evidence: impl Into<String>,
    ) -> Self {
        let text = text.into();
        let evidence = evidence.into();
        debug_assert!(!text.trim().is_empty(), "原因文本不能为空：{}", code.as_str());
        debug_assert!(
            !evidence.trim().is_empty(),
            "原因证据不能为空：{}",
            code.as_str()
        );
        Self {
            code,
            severity,
            text,
            evidence,
        }
    }

    /// 正向证据：说明为什么可以删除。
    pub fn safe(code: ReasonCode, text: impl Into<String>, evidence: impl Into<String>) -> Self {
        Self::new(code, SafetyStatus::Safe, text, evidence)
    }

    /// 不确定：不构成危险，但需要用户确认。
    pub fn review(code: ReasonCode, text: impl Into<String>, evidence: impl Into<String>) -> Self {
        Self::new(code, SafetyStatus::Review, text, evidence)
    }

    /// 硬性禁止：该项不可删除。
    pub fn blocking(code: ReasonCode, text: impl Into<String>, evidence: impl Into<String>) -> Self {
        Self::new(code, SafetyStatus::Unsafe, text, evidence)
    }
}

/// 一组原因及其推导出的整体等级。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SafetyAssessment {
    pub status: SafetyStatus,
    pub reasons: Vec<Reason>,
}

impl SafetyAssessment {
    /// 等级 = 所有原因中最严重的那条（D2）。
    ///
    /// 一条原因都没有时判为 `Review` 而不是 `Safe`：没有任何证据的沉默
    /// 不能当作安全（D3）。
    pub fn from_reasons(reasons: Vec<Reason>) -> Self {
        let status = reasons
            .iter()
            .map(|reason| reason.severity)
            .max()
            .unwrap_or(SafetyStatus::Review);
        Self { status, reasons }
    }

    /// 便捷构造：单项判定。
    pub fn single(reason: Reason) -> Self {
        Self::from_reasons(vec![reason])
    }

    pub fn is_safe(&self) -> bool {
        self.status == SafetyStatus::Safe
    }

    pub fn is_unsafe(&self) -> bool {
        self.status == SafetyStatus::Unsafe
    }

    /// 该项是否可以被用户选中（不可删除项不能被选中，D12）。
    pub fn is_selectable(&self) -> bool {
        self.status != SafetyStatus::Unsafe
    }

    /// 是否默认勾选（Safe 默认勾选，Review 默认不勾，D12）。
    pub fn is_default_selected(&self) -> bool {
        self.status == SafetyStatus::Safe
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severity_order_is_safe_review_unsafe() {
        assert!(SafetyStatus::Safe < SafetyStatus::Review);
        assert!(SafetyStatus::Review < SafetyStatus::Unsafe);
    }

    #[test]
    fn worst_reason_decides_status() {
        let assessment = SafetyAssessment::from_reasons(vec![
            Reason::safe(ReasonCode::WtClean, "工作区干净", "无未提交改动"),
            Reason::review(ReasonCode::WtUnmerged, "未合入 main", "merge-base 判定不是祖先"),
            Reason::blocking(ReasonCode::WtDirty, "有未提交改动", "3 个文件有未提交改动"),
        ]);
        assert_eq!(assessment.status, SafetyStatus::Unsafe);
    }

    #[test]
    fn review_beats_safe() {
        let assessment = SafetyAssessment::from_reasons(vec![
            Reason::safe(ReasonCode::WtClean, "工作区干净", "无未提交改动"),
            Reason::review(ReasonCode::WtRecentActivity, "近 7 天有改动", "最后活动：2 天前"),
        ]);
        assert_eq!(assessment.status, SafetyStatus::Review);
        assert!(!assessment.is_default_selected());
        assert!(assessment.is_selectable());
    }

    #[test]
    fn empty_reasons_is_review_not_safe() {
        // D3：拿不到任何证据的时候不能说安全。
        let assessment = SafetyAssessment::from_reasons(vec![]);
        assert_eq!(assessment.status, SafetyStatus::Review);
    }

    #[test]
    fn unsafe_is_not_selectable() {
        let assessment = SafetyAssessment::single(Reason::blocking(
            ReasonCode::WtMain,
            "这是主工作区",
            "git worktree list 首项",
        ));
        assert!(assessment.is_unsafe());
        assert!(!assessment.is_selectable());
        assert!(!assessment.is_default_selected());
    }

    #[test]
    fn reason_code_wire_format_matches_as_str() {
        // 契约漂移防护：serde 输出必须与 as_str() 一致。
        for code in ReasonCode::ALL {
            let json = serde_json::to_string(&code).expect("序列化原因码");
            assert_eq!(
                json,
                format!("\"{}\"", code.as_str()),
                "原因码 {:?} 的 serde 输出与 as_str 不一致",
                code
            );
        }
    }

    #[test]
    fn reason_codes_are_unique_and_prefixed() {
        let mut seen = std::collections::HashSet::new();
        for code in ReasonCode::ALL {
            assert!(seen.insert(code.as_str()), "原因码重复：{}", code.as_str());
            let s = code.as_str();
            assert!(
                s.starts_with("wt.") || s.starts_with("af."),
                "原因码必须带 wt. / af. 前缀：{}",
                s
            );
        }
        assert_eq!(ReasonCode::ALL.len(), 26);
    }

    #[test]
    fn reason_round_trips_through_json() {
        let reason = Reason::review(ReasonCode::AfCannotVerify, "无法完成检查", "体积统计超时（30s）");
        let json = serde_json::to_string(&reason).expect("序列化原因");
        let back: Reason = serde_json::from_str(&json).expect("反序列化原因");
        assert_eq!(reason, back);
        assert!(json.contains("\"af.cannot_verify\""));
    }

    #[test]
    #[should_panic(expected = "原因证据不能为空")]
    fn empty_evidence_is_rejected() {
        let _ = Reason::new(ReasonCode::WtClean, SafetyStatus::Safe, "工作区干净", "   ");
    }

    #[test]
    #[should_panic(expected = "原因文本不能为空")]
    fn empty_text_is_rejected() {
        let _ = Reason::new(ReasonCode::WtClean, SafetyStatus::Safe, "", "无未提交改动");
    }
}
