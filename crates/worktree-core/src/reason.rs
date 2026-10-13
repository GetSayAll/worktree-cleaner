//! The decision-reason model.
//!
//! Every safety decision is made up of a set of [`Reason`] values with evidence, and
//! the overall level is decided by the **most severe** one among them (development
//! contract D2). Failures (timeout / permission error / non-zero command exit) always
//! produce a `cannot_verify` reason such as [`ReasonCode::WtCannotVerify`] with level
//! `Review`, and are never downgraded to `Safe` (D3).

use serde::{Deserialize, Serialize};

/// Safety level.
///
/// The derive order is the severity order: `Safe` < `Review` < `Unsafe`,
/// so comparing two levels is the same as comparing their severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum SafetyStatus {
    Safe,
    Review,
    Unsafe,
}

/// Decision reason code.
///
/// The serialised strings (`wt.*` / `af.*`) are part of the external contract and may
/// **only be added, never change meaning**: a semantic change must introduce a new
/// reason code; old codes are marked deprecated but never reused.
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
    /// All reason codes. Used by the contract-drift test (the serialised value must
    /// match [`Self::as_str`]).
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

    /// External string form, which must match `serde`'s serialised value.
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

    /// Whether the reason belongs to a worktree decision or an artifact decision.
    pub fn is_worktree(self) -> bool {
        self.as_str().starts_with("wt.")
    }
}

/// A single decision reason. All four fields are required (D9):
/// a stable identifier, the most severe level this reason leads to, human-readable
/// text, and verifiable evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reason {
    pub code: ReasonCode,
    pub severity: SafetyStatus,
    pub text: String,
    /// Evidence: a concrete command, file count, or path. **Must not be empty**.
    pub evidence: String,
}

impl Reason {
    /// Construct a reason. Neither text nor evidence may be empty; a violation fails
    /// the assertion immediately in test builds.
    pub fn new(
        code: ReasonCode,
        severity: SafetyStatus,
        text: impl Into<String>,
        evidence: impl Into<String>,
    ) -> Self {
        let text = text.into();
        let evidence = evidence.into();
        debug_assert!(
            !text.trim().is_empty(),
            "reason text must not be empty: {}",
            code.as_str()
        );
        debug_assert!(
            !evidence.trim().is_empty(),
            "reason evidence must not be empty: {}",
            code.as_str()
        );
        Self {
            code,
            severity,
            text,
            evidence,
        }
    }

    /// Positive evidence: explains why this can be deleted.
    pub fn safe(code: ReasonCode, text: impl Into<String>, evidence: impl Into<String>) -> Self {
        Self::new(code, SafetyStatus::Safe, text, evidence)
    }

    /// Uncertain: not dangerous on its own, but needs user confirmation.
    pub fn review(code: ReasonCode, text: impl Into<String>, evidence: impl Into<String>) -> Self {
        Self::new(code, SafetyStatus::Review, text, evidence)
    }

    /// Hard prohibition: this item must not be deleted.
    pub fn blocking(
        code: ReasonCode,
        text: impl Into<String>,
        evidence: impl Into<String>,
    ) -> Self {
        Self::new(code, SafetyStatus::Unsafe, text, evidence)
    }
}

/// A set of reasons together with the overall level derived from them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SafetyAssessment {
    pub status: SafetyStatus,
    pub reasons: Vec<Reason>,
}

impl SafetyAssessment {
    /// Level = the most severe of all reasons (D2).
    ///
    /// With no reasons at all the verdict is `Review` rather than `Safe`: silence
    /// without any evidence cannot be treated as safe (D3).
    pub fn from_reasons(reasons: Vec<Reason>) -> Self {
        let status = reasons
            .iter()
            .map(|reason| reason.severity)
            .max()
            .unwrap_or(SafetyStatus::Review);
        Self { status, reasons }
    }

    /// Convenience constructor: a single-item decision.
    pub fn single(reason: Reason) -> Self {
        Self::from_reasons(vec![reason])
    }

    pub fn is_safe(&self) -> bool {
        self.status == SafetyStatus::Safe
    }

    pub fn is_unsafe(&self) -> bool {
        self.status == SafetyStatus::Unsafe
    }

    /// Whether the item can be selected by the user (non-deletable items cannot be
    /// selected, D12).
    pub fn is_selectable(&self) -> bool {
        self.status != SafetyStatus::Unsafe
    }

    /// Whether it is checked by default (Safe is checked by default, Review is not, D12).
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
            Reason::safe(
                ReasonCode::WtClean,
                "clean working tree",
                "no uncommitted changes",
            ),
            Reason::review(
                ReasonCode::WtUnmerged,
                "not merged into main",
                "merge-base does not report an ancestor",
            ),
            Reason::blocking(
                ReasonCode::WtDirty,
                "has uncommitted changes",
                "3 files have uncommitted changes",
            ),
        ]);
        assert_eq!(assessment.status, SafetyStatus::Unsafe);
    }

    #[test]
    fn review_beats_safe() {
        let assessment = SafetyAssessment::from_reasons(vec![
            Reason::safe(
                ReasonCode::WtClean,
                "clean working tree",
                "no uncommitted changes",
            ),
            Reason::review(
                ReasonCode::WtRecentActivity,
                "changed within the last 7 days",
                "last activity: 2 days ago",
            ),
        ]);
        assert_eq!(assessment.status, SafetyStatus::Review);
        assert!(!assessment.is_default_selected());
        assert!(assessment.is_selectable());
    }

    #[test]
    fn empty_reasons_is_review_not_safe() {
        // D3: when no evidence can be obtained, we must not call it safe.
        let assessment = SafetyAssessment::from_reasons(vec![]);
        assert_eq!(assessment.status, SafetyStatus::Review);
    }

    #[test]
    fn unsafe_is_not_selectable() {
        let assessment = SafetyAssessment::single(Reason::blocking(
            ReasonCode::WtMain,
            "this is the main worktree",
            "first entry of git worktree list",
        ));
        assert!(assessment.is_unsafe());
        assert!(!assessment.is_selectable());
        assert!(!assessment.is_default_selected());
    }

    #[test]
    fn reason_code_wire_format_matches_as_str() {
        // Contract-drift guard: the serde output must match as_str().
        for code in ReasonCode::ALL {
            let json = serde_json::to_string(&code).expect("serialize reason code");
            assert_eq!(
                json,
                format!("\"{}\"", code.as_str()),
                "serde output for reason code {:?} does not match as_str",
                code
            );
        }
    }

    #[test]
    fn reason_codes_are_unique_and_prefixed() {
        let mut seen = std::collections::HashSet::new();
        for code in ReasonCode::ALL {
            assert!(
                seen.insert(code.as_str()),
                "duplicate reason code: {}",
                code.as_str()
            );
            let s = code.as_str();
            assert!(
                s.starts_with("wt.") || s.starts_with("af."),
                "reason code must carry a wt. / af. prefix: {}",
                s
            );
        }
        assert_eq!(ReasonCode::ALL.len(), 26);
    }

    #[test]
    fn reason_round_trips_through_json() {
        let reason = Reason::review(
            ReasonCode::AfCannotVerify,
            "cannot complete the check",
            "size measurement timed out (30s)",
        );
        let json = serde_json::to_string(&reason).expect("serialize reason");
        let back: Reason = serde_json::from_str(&json).expect("deserialize reason");
        assert_eq!(reason, back);
        assert!(json.contains("\"af.cannot_verify\""));
    }

    #[test]
    #[should_panic(expected = "reason evidence must not be empty")]
    fn empty_evidence_is_rejected() {
        let _ = Reason::new(
            ReasonCode::WtClean,
            SafetyStatus::Safe,
            "clean working tree",
            "   ",
        );
    }

    #[test]
    #[should_panic(expected = "reason text must not be empty")]
    fn empty_text_is_rejected() {
        let _ = Reason::new(
            ReasonCode::WtClean,
            SafetyStatus::Safe,
            "",
            "no uncommitted changes",
        );
    }
}
