//! Disk-hygiene audit for one repository: the half of `docs/DISK_HYGIENE.md` that
//! a scan can observe.
//!
//! The policy says where worktrees and build output belong (L), how long they may
//! live (T), where build output may be written (B), how much space they may take
//! (Q), and what a written exception has to contain (X). This module answers, for
//! one repository, which of those rules hold — and where a rule cannot be observed
//! from inside a repository at all, it says so instead of calling it satisfied
//! (D3: without evidence you cannot call something safe).
//!
//! It is read-only (D1). Nothing here deletes, moves, or writes: every finding
//! names the rule, a level, the path it is about, and the measurement behind it,
//! and the front end turns that into a sentence in the reader's language (D23).
//! The copy that says what to do about it lives in `src/i18n/`, because a fix is
//! advice and advice is read, not acted on.
//!
//! Two boundaries are deliberate:
//!
//! - the audit never looks above the repository. A second clone (L2) and a
//!   machine-level cache (C1–C3) are outside the scanned path by construction, so
//!   those rules are reported as unchecked rather than guessed at;
//! - a rule that is only knowable at creation time (Q3) or at session exit (T5) is
//!   reported as unchecked too. The report says what it found, not what it hopes.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use serde::{Deserialize, Serialize};

use crate::artifact::measure_dir;
use crate::git::GitRunner;
use crate::timefmt::{elapsed_days, format_rfc3339, parse_rfc3339};
use crate::{ArtifactRecord, DetailArg, WorktreeRecord};

/// T1: registered worktrees per repository.
pub const WORKTREE_CAP: u32 = 50;
/// T3/T4 are counted in days.
pub const IDLE_DAYS: u32 = 7;
pub const CEILING_DAYS: u32 = 30;
/// B5: one worktree's cache slot.
pub const CACHE_SLOT_CAP_BYTES: u64 = 10 * 1024 * 1024 * 1024;
/// Q1: free space above 15% is normal, below 10% freezes new work.
const Q1_NORMAL_PERCENT: u64 = 15;
const Q1_FROZEN_PERCENT: u64 = 10;
/// Q2: one repository's cache share of the volume.
const Q2_SHARE_PERCENT: u64 = 10;
/// T1 warns before the cap is reached rather than only after it is crossed.
const NEAR_CAP_NUMERATOR: u32 = 9;
const NEAR_CAP_DENOMINATOR: u32 = 10;
/// L4: the declared worktree name length.
const NAME_MAX_CHARS: usize = 60;
/// The file a written exception lives in, at the repository root (X1).
const KEEP_FILE: &str = ".worktree-keep";
/// The two declared paths (L1, L5).
const WORKTREE_DIR: &str = ".worktree";
const CACHE_DIR: &str = "build-cache";

/// One numbered rule of the policy.
///
/// The identifiers are the document's own (`L1`, `B1.1`, `Q2`), so a finding can be
/// read against the rule it breaks without a lookup table in between. They are part
/// of the contract in the same way a reason code is: additive only, never reused for
/// another meaning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum HygieneRule {
    #[serde(rename = "L1")]
    L1,
    #[serde(rename = "L2")]
    L2,
    #[serde(rename = "L3")]
    L3,
    #[serde(rename = "L4")]
    L4,
    #[serde(rename = "L5")]
    L5,
    #[serde(rename = "L6")]
    L6,
    #[serde(rename = "L7")]
    L7,
    #[serde(rename = "T1")]
    T1,
    #[serde(rename = "T2")]
    T2,
    #[serde(rename = "T3")]
    T3,
    #[serde(rename = "T4")]
    T4,
    #[serde(rename = "T5")]
    T5,
    #[serde(rename = "B1")]
    B1,
    #[serde(rename = "B1.1")]
    B11,
    #[serde(rename = "B2")]
    B2,
    #[serde(rename = "B3")]
    B3,
    #[serde(rename = "B4")]
    B4,
    #[serde(rename = "B5")]
    B5,
    #[serde(rename = "Q1")]
    Q1,
    #[serde(rename = "Q2")]
    Q2,
    #[serde(rename = "Q3")]
    Q3,
    #[serde(rename = "X1")]
    X1,
}

impl HygieneRule {
    /// Every rule this audit knows, in policy order.
    pub const ALL: [HygieneRule; 22] = [
        HygieneRule::L1,
        HygieneRule::L2,
        HygieneRule::L3,
        HygieneRule::L4,
        HygieneRule::L5,
        HygieneRule::L6,
        HygieneRule::L7,
        HygieneRule::T1,
        HygieneRule::T2,
        HygieneRule::T3,
        HygieneRule::T4,
        HygieneRule::T5,
        HygieneRule::B1,
        HygieneRule::B11,
        HygieneRule::B2,
        HygieneRule::B3,
        HygieneRule::B4,
        HygieneRule::B5,
        HygieneRule::Q1,
        HygieneRule::Q2,
        HygieneRule::Q3,
        HygieneRule::X1,
    ];

    /// The identifier the document uses, and the wire form.
    pub const fn as_str(self) -> &'static str {
        match self {
            HygieneRule::L1 => "L1",
            HygieneRule::L2 => "L2",
            HygieneRule::L3 => "L3",
            HygieneRule::L4 => "L4",
            HygieneRule::L5 => "L5",
            HygieneRule::L6 => "L6",
            HygieneRule::L7 => "L7",
            HygieneRule::T1 => "T1",
            HygieneRule::T2 => "T2",
            HygieneRule::T3 => "T3",
            HygieneRule::T4 => "T4",
            HygieneRule::T5 => "T5",
            HygieneRule::B1 => "B1",
            HygieneRule::B11 => "B1.1",
            HygieneRule::B2 => "B2",
            HygieneRule::B3 => "B3",
            HygieneRule::B4 => "B4",
            HygieneRule::B5 => "B5",
            HygieneRule::Q1 => "Q1",
            HygieneRule::Q2 => "Q2",
            HygieneRule::Q3 => "Q3",
            HygieneRule::X1 => "X1",
        }
    }
}

/// How much a finding matters.
///
/// `Violation` means the rule is broken in a way that costs disk, hides work, or
/// breaks the accounting; `Advisory` means the rule is broken in a way that costs
/// tidiness — a name, a stale entry, a recommendation. `Unknown` is not a level on
/// this scale: it says the rule could not be observed, which must never be rendered
/// as either of the other two (D3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HygieneLevel {
    Ok,
    Advisory,
    Violation,
    Unknown,
}

impl HygieneLevel {
    /// Order used to roll a rule up from its findings. `Unknown` sits between `Ok`
    /// and `Advisory` so a rule that has one real finding is not reported as
    /// unchecked because another probe failed.
    const fn rank(self) -> u8 {
        match self {
            HygieneLevel::Ok => 0,
            HygieneLevel::Unknown => 1,
            HygieneLevel::Advisory => 2,
            HygieneLevel::Violation => 3,
        }
    }
}

/// One policy finding.
///
/// Mirrors `HygieneFinding` in `src/types.ts` field for field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HygieneFinding {
    pub rule: HygieneRule,
    pub level: HygieneLevel,
    /// What the finding is about: a worktree path, a cache slot, a file, or the
    /// repository root. This is what the reader acts on, so it is never empty.
    pub subject: String,
    /// The measurement, in one line: the path Git reported, the count, the rule
    /// that matched, the size. English on purpose — it is data, not interface copy
    /// (D23) — and never empty, so a finding can be checked rather than believed.
    pub evidence: String,
    /// Raw values the sentence needs. `state` selects which sentence; the rest are
    /// filled in the reader's language by the front end, the same way a
    /// [`DetailArg`] is.
    #[serde(default)]
    pub args: BTreeMap<String, DetailArg>,
    /// `Some(date)` when `.worktree-keep` covers this finding. It is then accepted
    /// until that date and carries level `Ok`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub waived_until: Option<String>,
}

impl HygieneFinding {
    fn new(
        rule: HygieneRule,
        level: HygieneLevel,
        subject: impl Into<String>,
        evidence: impl Into<String>,
    ) -> Self {
        Self {
            rule,
            level,
            subject: subject.into(),
            evidence: evidence.into(),
            args: BTreeMap::new(),
            waived_until: None,
        }
    }

    /// Names the situation, which is what the message table keys its sentence by.
    fn state(mut self, state: &str) -> Self {
        self.args
            .insert("state".to_string(), DetailArg::Text(state.to_string()));
        self
    }

    fn arg(mut self, key: &str, value: DetailArg) -> Self {
        self.args.insert(key.to_string(), value);
        self
    }
}

/// What one rule came to, across all its findings.
///
/// Mirrors `HygieneRuleStatus` in `src/types.ts` field for field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HygieneRuleStatus {
    pub rule: HygieneRule,
    pub level: HygieneLevel,
    /// How many findings the rule produced, including accepted ones.
    pub findings: u32,
}

/// The audit result for one repository.
///
/// Mirrors `HygieneReport` in `src/types.ts` field for field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HygieneReport {
    /// One entry per rule, in policy order, so the page can render the whole
    /// checklist without re-deriving any level (D9).
    pub rules: Vec<HygieneRuleStatus>,
    /// Violations first, then advisories, then the rules that could not be checked.
    pub findings: Vec<HygieneFinding>,
    /// Notes about the audit itself (a measurement that timed out, a file that
    /// could not be read). They are not findings: nothing is known to be wrong.
    #[serde(default)]
    pub warnings: Vec<String>,
}

/// The volume a repository lives on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VolumeSpace {
    pub available_bytes: u64,
    pub total_bytes: u64,
}

/// Injection points for one audit.
///
/// Everything a decision depends on — the clock, the policy numbers, the volume
/// — is a field, so the checks are a pure function of the filesystem and of the
/// scan's own facts and can be tested without a ten-gigabyte directory.
#[derive(Debug, Clone)]
pub struct HygieneOptions {
    pub now: SystemTime,
    /// T3's idle window, shared with the scan's activity window.
    pub idle_days: u32,
    pub worktree_cap: u32,
    pub ceiling_days: u32,
    pub cache_slot_cap_bytes: u64,
    pub per_dir_timeout: Duration,
    /// When the scan's budget is spent, the remaining slot measurements are
    /// skipped and reported rather than run past the budget.
    pub budget_deadline: Option<Instant>,
    /// `None` reads the volume with `fs4`; tests inject a size instead of
    /// depending on the machine they run on.
    pub volume: Option<VolumeSpace>,
    /// Creation times to use instead of the filesystem's, per path.
    ///
    /// A directory's birth time cannot be set by a test on every platform, and T3
    /// and T4 are decisions about age: without this, the two rules could only be
    /// exercised by waiting a month. `None` in production.
    pub creation_override: Option<Vec<(PathBuf, SystemTime)>>,
}

impl Default for HygieneOptions {
    fn default() -> Self {
        Self {
            now: SystemTime::now(),
            idle_days: IDLE_DAYS,
            worktree_cap: WORKTREE_CAP,
            ceiling_days: CEILING_DAYS,
            cache_slot_cap_bytes: CACHE_SLOT_CAP_BYTES,
            per_dir_timeout: Duration::from_secs(30),
            budget_deadline: None,
            volume: None,
            creation_override: None,
        }
    }
}

/// Audits one repository against the policy.
///
/// `worktrees` and `artifacts` are the scan's own results: the verdicts are already
/// there (merged, dirty, last activity, ignore status, size), so this reads them
/// instead of paying for a second pass. Only what the scan does not ask is asked
/// here — the ignore rule that covers each declared path, and the volume's free
/// space.
pub fn audit_hygiene(
    runner: &dyn GitRunner,
    root: &Path,
    worktrees: &[WorktreeRecord],
    artifacts: &[ArtifactRecord],
    opts: &HygieneOptions,
) -> HygieneReport {
    let repo = repository_root(runner, root);
    let mut audit = Audit {
        repo,
        worktrees,
        artifacts,
        opts,
        findings: Vec::new(),
        warnings: Vec::new(),
        volume: None,
        cache_bytes: None,
    };

    let waivers = audit.exceptions();
    audit.layout(runner);
    audit.lifetime();
    audit.build_output();
    audit.quotas();

    for (rule, why) in NOT_CHECKABLE {
        audit.findings.push(
            HygieneFinding::new(
                *rule,
                HygieneLevel::Unknown,
                audit.repo.display().to_string(),
                *why,
            )
            .state("not_checkable"),
        );
    }

    let mut findings = audit.findings;
    apply_waivers(&mut findings, &waivers);
    findings.sort_by(compare_findings);

    let rules = HygieneRule::ALL
        .iter()
        .map(|rule| {
            let mut level = HygieneLevel::Ok;
            let mut count = 0_u32;
            for finding in findings.iter().filter(|finding| finding.rule == *rule) {
                count += 1;
                if finding.level.rank() > level.rank() {
                    level = finding.level;
                }
            }
            HygieneRuleStatus {
                rule: *rule,
                level,
                findings: count,
            }
        })
        .collect();

    HygieneReport {
        rules,
        findings,
        warnings: audit.warnings,
    }
}

/// Rules a scan cannot observe, with the reason it cannot.
///
/// Each one is a real limit of looking at a repository from the inside rather than
/// a gap in the implementation, and the sentence says which. They are reported as
/// `Unknown` so the checklist never claims more than it checked (D3).
const NOT_CHECKABLE: &[(HygieneRule, &str)] = &[
    (
        HygieneRule::L2,
        "a second clone of this repository, if one exists, is not inside the scanned path",
    ),
    (
        HygieneRule::L6,
        "git clean is not logged, so no scan can see a command that has already run",
    ),
    (
        HygieneRule::L7,
        "editor search and platform indexing are configured outside the repository",
    ),
    (
        HygieneRule::T5,
        "a session's exit is not recorded anywhere a scan can read",
    ),
    (
        HygieneRule::B3,
        "which dependencies a build shares is decided by its own configuration",
    ),
    (
        HygieneRule::B4,
        "whether something reads the cache as an input cannot be established by reading it",
    ),
    (
        HygieneRule::Q3,
        "admission runs before creation; a scan only sees what already exists",
    ),
];

/// One written exception (X1).
struct Waiver {
    path: PathBuf,
    /// Rule ids the waiver covers; empty means all of them.
    rules: Vec<String>,
    expires: String,
}

struct Audit<'a> {
    repo: PathBuf,
    worktrees: &'a [WorktreeRecord],
    artifacts: &'a [ArtifactRecord],
    opts: &'a HygieneOptions,
    findings: Vec<HygieneFinding>,
    warnings: Vec<String>,
    volume: Option<VolumeSpace>,
    /// Measured cache slots and how many could not be measured, filled in by
    /// [`Self::slot_sizes`] and read by [`Self::quotas`].
    cache_bytes: Option<(u64, u32)>,
}

impl Audit<'_> {
    fn push(&mut self, finding: HygieneFinding) {
        self.findings.push(finding);
    }

    /// Worktrees that are not the main checkout and still exist.
    fn linked_worktrees(&self) -> impl Iterator<Item = &WorktreeRecord> {
        self.worktrees
            .iter()
            .filter(|record| !record.is_main && !record.is_prunable)
    }

    fn worktree_root(&self) -> PathBuf {
        self.repo.join(WORKTREE_DIR)
    }

    fn cache_root(&self) -> PathBuf {
        self.repo.join(CACHE_DIR)
    }

    /// L1, L3, L4: where the worktrees are and what they are called.
    fn layout(&mut self, runner: &dyn GitRunner) {
        let declared = self.worktree_root();
        let repo_name = self
            .repo
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let linked: Vec<WorktreeRecord> = self.linked_worktrees().cloned().collect();

        for record in &linked {
            let path = PathBuf::from(&record.path);
            if path.starts_with(&declared) {
                let name = path
                    .strip_prefix(&declared)
                    .map(|rest| rest.to_string_lossy().into_owned())
                    .unwrap_or_default();
                self.check_name(&name, &repo_name, &record.path);
            } else {
                let state = if path.starts_with(&self.repo) {
                    "inside_but_undeclared"
                } else {
                    "outside_repository"
                };
                self.push(
                    HygieneFinding::new(
                        HygieneRule::L1,
                        HygieneLevel::Violation,
                        record.path.clone(),
                        format!("declared root: {}", declared.display()),
                    )
                    .state(state)
                    .arg("expected", DetailArg::Text(declared.display().to_string())),
                );
            }
        }

        // L3: a worktree inside another worktree. The nested case is an ancestor
        // relation, so one comparison over the linked worktrees covers both
        // `.worktree/a/.worktree/b` and a plain directory nested in another.
        for inner in &linked {
            let inner_path = Path::new(&inner.path);
            for outer in &linked {
                if inner.path == outer.path {
                    continue;
                }
                let outer_path = Path::new(&outer.path);
                if inner_path.starts_with(outer_path) && inner_path != outer_path {
                    self.push(
                        HygieneFinding::new(
                            HygieneRule::L3,
                            HygieneLevel::Violation,
                            inner.path.clone(),
                            format!("inside the worktree at {}", outer.path),
                        )
                        .state("nested_worktree")
                        .arg("parent", DetailArg::Text(outer.path.clone())),
                    );
                    break;
                }
            }
        }

        self.ignore_entries(runner, &linked);
    }

    /// L4: the declared name shape, `<repo>-<branch-slug>-<YYYYMMDD>`.
    fn check_name(&mut self, name: &str, repo_name: &str, path: &str) {
        if name.is_empty() {
            return;
        }
        let slug_ok = name
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
            && !name.starts_with('-')
            && !name.ends_with('-')
            && !name.contains("--");
        if !slug_ok {
            self.push(
                HygieneFinding::new(
                    HygieneRule::L4,
                    HygieneLevel::Advisory,
                    path,
                    format!("directory name: {name}"),
                )
                .state("not_a_slug")
                .arg("name", DetailArg::Text(name.to_string())),
            );
        }
        if name.chars().count() > NAME_MAX_CHARS {
            self.push(
                HygieneFinding::new(
                    HygieneRule::L4,
                    HygieneLevel::Advisory,
                    path,
                    format!("directory name is {} characters", name.chars().count()),
                )
                .state("too_long")
                .arg("name", DetailArg::Text(name.to_string()))
                .arg("length", DetailArg::Count(name.chars().count() as u64))
                .arg("limit", DetailArg::Count(NAME_MAX_CHARS as u64)),
            );
        }
        match date_suffix(name) {
            None => self.push(
                HygieneFinding::new(
                    HygieneRule::L4,
                    HygieneLevel::Advisory,
                    path,
                    format!("directory name: {name}"),
                )
                .state("no_date_suffix")
                .arg("name", DetailArg::Text(name.to_string())),
            ),
            Some(false) => self.push(
                HygieneFinding::new(
                    HygieneRule::L4,
                    HygieneLevel::Advisory,
                    path,
                    format!("date suffix of {name} is not a real date"),
                )
                .state("invalid_date")
                .arg("name", DetailArg::Text(name.to_string())),
            ),
            Some(true) => {}
        }
        let prefix = repo_slug(repo_name);
        if !prefix.is_empty() && !name.starts_with(&format!("{prefix}-")) {
            self.push(
                HygieneFinding::new(
                    HygieneRule::L4,
                    HygieneLevel::Advisory,
                    path,
                    format!("directory name: {name}"),
                )
                .state("missing_repo_prefix")
                .arg("name", DetailArg::Text(name.to_string()))
                .arg("prefix", DetailArg::Text(prefix)),
            );
        }
    }

    /// L5: both declared paths are ignored, anchored to the root, and committed.
    fn ignore_entries(&mut self, runner: &dyn GitRunner, linked: &[WorktreeRecord]) {
        let hosting = !linked.is_empty();
        for (required, probe) in [
            (
                format!("/{WORKTREE_DIR}/"),
                format!("{WORKTREE_DIR}/__probe__"),
            ),
            (format!("/{CACHE_DIR}/"), format!("{CACHE_DIR}/__probe__")),
        ] {
            let output = match runner.run(
                &self.repo,
                &["check-ignore", "-v", "-n", "--no-index", "--", &probe],
            ) {
                // Exit status 1 is an answer, not a failure: it is how
                // `git check-ignore` says "no rule matches this path", which is
                // precisely the finding this rule is about. Only a fatal error
                // (128 — not a repository, an unusable index) leaves the question
                // unanswered, and that is the case that must not be read as "met".
                Ok(output) if output.success() || output.code == Some(1) => output,
                Ok(output) => {
                    self.warnings.push(format!(
                        "git check-ignore could not answer for {probe}: exited with {:?}: {}",
                        output.code,
                        output.stderr.trim()
                    ));
                    continue;
                }
                Err(error) => {
                    self.warnings.push(format!(
                        "git check-ignore could not answer for {probe}: {error}"
                    ));
                    continue;
                }
            };
            let rule = parse_ignore_rule(&output.stdout);
            let subject = self.repo.display().to_string();
            match rule {
                None => {
                    let level = if hosting {
                        HygieneLevel::Violation
                    } else {
                        HygieneLevel::Advisory
                    };
                    let state = if hosting {
                        "missing_entry"
                    } else {
                        "no_worktrees_yet"
                    };
                    self.push(
                        HygieneFinding::new(
                            HygieneRule::L5,
                            level,
                            subject,
                            format!("git check-ignore {probe}: no rule matches"),
                        )
                        .state(state)
                        .arg("entry", DetailArg::Text(required)),
                    );
                }
                Some(found) => {
                    let source = found.source.clone();
                    let anchored = found.pattern.starts_with('/');
                    let local = source.ends_with("info/exclude");
                    // Only the file at the repository root counts: a rule in a parent
                    // `.gitignore` does not travel with a clone of this repository.
                    let repo_file = source == ".gitignore"
                        || Path::new(&source) == self.repo.join(".gitignore");
                    let state = if local {
                        "local_only"
                    } else if !repo_file {
                        "elsewhere"
                    } else if !anchored {
                        "unanchored_pattern"
                    } else {
                        continue;
                    };
                    let level = if state == "elsewhere" {
                        HygieneLevel::Advisory
                    } else {
                        HygieneLevel::Violation
                    };
                    self.push(
                        HygieneFinding::new(
                            HygieneRule::L5,
                            level,
                            subject,
                            format!(
                                "{}:{}: {} (for {probe})",
                                found.source, found.line, found.pattern
                            ),
                        )
                        .state(state)
                        .arg("entry", DetailArg::Text(required))
                        .arg("pattern", DetailArg::Text(found.pattern))
                        .arg("source", DetailArg::Text(source))
                        .arg("line", DetailArg::Count(found.line)),
                    );
                }
            }
        }

        // The anchoring rule exists because an unanchored pattern matches the name
        // at any depth, so a build inside a worktree quietly creates a second
        // `.worktree/` or `build-cache/`. That is observable, and it is the harm
        // the rule is about.
        for record in linked {
            let path = Path::new(&record.path);
            let nested = path.join(WORKTREE_DIR);
            if nested.is_dir() {
                self.push(
                    HygieneFinding::new(
                        HygieneRule::L5,
                        HygieneLevel::Violation,
                        nested.display().to_string(),
                        format!("a {WORKTREE_DIR}/ directory inside a worktree"),
                    )
                    .state("nested_worktree_dir")
                    .arg("entry", DetailArg::Text(format!("/{WORKTREE_DIR}/"))),
                );
            }
        }
    }

    /// T1–T4: how many worktrees there are and how long they have been there.
    fn lifetime(&mut self) {
        let registered = u32::try_from(self.worktrees.len()).unwrap_or(u32::MAX);
        let subject = self.repo.display().to_string();
        if registered > self.opts.worktree_cap {
            self.push(
                HygieneFinding::new(
                    HygieneRule::T1,
                    HygieneLevel::Violation,
                    &subject,
                    format!("{registered} registered worktrees"),
                )
                .state("over_cap")
                .arg("count", DetailArg::Count(u64::from(registered)))
                .arg("cap", DetailArg::Count(u64::from(self.opts.worktree_cap))),
            );
        } else if registered.saturating_mul(NEAR_CAP_DENOMINATOR)
            >= self.opts.worktree_cap.saturating_mul(NEAR_CAP_NUMERATOR)
        {
            self.push(
                HygieneFinding::new(
                    HygieneRule::T1,
                    HygieneLevel::Advisory,
                    &subject,
                    format!("{registered} registered worktrees"),
                )
                .state("near_cap")
                .arg("count", DetailArg::Count(u64::from(registered)))
                .arg("cap", DetailArg::Count(u64::from(self.opts.worktree_cap))),
            );
        }

        let mut age_unknown: u32 = 0;
        let linked: Vec<WorktreeRecord> = self.linked_worktrees().cloned().collect();
        for record in &linked {
            if record.merged_into_default == Some(true) {
                let branch = record.branch.clone().unwrap_or_default();
                self.push(
                    HygieneFinding::new(
                        HygieneRule::T2,
                        HygieneLevel::Advisory,
                        record.path.clone(),
                        format!("merged into the default branch: {branch}"),
                    )
                    .state("merged_still_present")
                    .arg("branch", DetailArg::Text(branch)),
                );
            }

            // T3 measures the last sign of life, not the last commit: a worktree
            // checked out yesterday from a branch whose HEAD is a year old is new,
            // and calling it idle for a year would send the reader to remove work
            // they just created.
            let head_age = record
                .last_updated_at
                .as_deref()
                .and_then(parse_rfc3339)
                .map(|then| elapsed_days(self.opts.now, then));
            let created = self.creation_time(Path::new(&record.path));
            let dir_age = created.map(|then| elapsed_days(self.opts.now, then));
            if dir_age.is_none() {
                age_unknown += 1;
            }
            if let Some(idle) = head_age.map(|head| match dir_age {
                Some(dir) => head.min(dir),
                None => head,
            }) {
                if idle >= u64::from(self.opts.idle_days) {
                    let dirty = record.dirty_files > 0 || record.untracked_files > 0;
                    let state = if dirty { "stale_with_changes" } else { "stale" };
                    let evidence = match record.last_updated_at.as_deref() {
                        Some(at) => format!("last commit at {at}"),
                        None => "no commit time could be read".to_string(),
                    };
                    self.push(
                        HygieneFinding::new(
                            HygieneRule::T3,
                            HygieneLevel::Advisory,
                            &record.path,
                            evidence,
                        )
                        .state(state)
                        .arg("days", DetailArg::Count(idle))
                        .arg("dirty", DetailArg::Count(u64::from(record.dirty_files)))
                        .arg(
                            "untracked",
                            DetailArg::Count(u64::from(record.untracked_files)),
                        ),
                    );
                }
            }

            if let Some(age) = dir_age {
                if age >= u64::from(self.opts.ceiling_days) {
                    let created = created.map_or_else(|| "unknown".to_string(), format_rfc3339);
                    self.push(
                        HygieneFinding::new(
                            HygieneRule::T4,
                            HygieneLevel::Violation,
                            &record.path,
                            format!("directory created at {created}"),
                        )
                        .state("past_ceiling")
                        .arg("days", DetailArg::Count(age))
                        .arg(
                            "ceiling",
                            DetailArg::Count(u64::from(self.opts.ceiling_days)),
                        ),
                    );
                }
            }
        }
        if age_unknown > 0 {
            self.push(
                HygieneFinding::new(
                    HygieneRule::T4,
                    HygieneLevel::Unknown,
                    subject,
                    format!("{age_unknown} worktree directories report no creation time"),
                )
                .state("age_unknown")
                .arg("count", DetailArg::Count(u64::from(age_unknown))),
            );
        }
    }

    /// B1, B1.1, B2, B5: where the output is, and what it costs.
    fn build_output(&mut self) {
        let cache_root = self.cache_root();
        for record in self.artifacts {
            let path = PathBuf::from(&record.path);
            if !path.starts_with(&cache_root) {
                self.push(
                    HygieneFinding::new(
                        HygieneRule::B1,
                        HygieneLevel::Violation,
                        record.path.clone(),
                        format!("not under {}: {}", cache_root.display(), record.kind),
                    )
                    .state("outside_declared")
                    .arg("kind", DetailArg::Text(record.kind.clone())),
                );
            }
            if record.git_tracked == Some(true) {
                self.push(
                    HygieneFinding::new(
                        HygieneRule::B11,
                        HygieneLevel::Violation,
                        record.path.clone(),
                        "git tracks files inside it".to_string(),
                    )
                    .state("tracked_in_git")
                    .arg("kind", DetailArg::Text(record.kind.clone())),
                );
            } else if record.git_ignored == Some(false) {
                self.push(
                    HygieneFinding::new(
                        HygieneRule::B11,
                        HygieneLevel::Violation,
                        record.path.clone(),
                        "git status lists it as untracked".to_string(),
                    )
                    .state("untracked_in_status")
                    .arg("kind", DetailArg::Text(record.kind.clone())),
                );
            }
        }

        let slots = subdirectory_names(&cache_root);
        let mut worktree_names: Vec<String> = self
            .linked_worktrees()
            .filter_map(|record| {
                Path::new(&record.path)
                    .strip_prefix(self.worktree_root())
                    .ok()
                    .and_then(|rest| rest.components().next())
                    .map(|first| first.as_os_str().to_string_lossy().into_owned())
            })
            .collect();
        worktree_names.sort();
        worktree_names.dedup();

        if !cache_root.is_dir() && !worktree_names.is_empty() {
            self.push(
                HygieneFinding::new(
                    HygieneRule::B2,
                    HygieneLevel::Advisory,
                    cache_root.display().to_string(),
                    "no build-cache/ directory at the repository root".to_string(),
                )
                .state("no_cache_root"),
            );
        } else if cache_root.is_dir() {
            for slot in &slots {
                if slot == "main" || slot == "shared" || worktree_names.contains(slot) {
                    continue;
                }
                self.push(
                    HygieneFinding::new(
                        HygieneRule::B2,
                        HygieneLevel::Violation,
                        cache_root.join(slot).display().to_string(),
                        format!("no worktree named {slot}"),
                    )
                    .state("orphan_slot")
                    .arg("slot", DetailArg::Text(slot.clone())),
                );
            }
            let missing: Vec<String> = worktree_names
                .iter()
                .filter(|name| !slots.contains(name))
                .cloned()
                .collect();
            if !missing.is_empty() {
                self.push(
                    HygieneFinding::new(
                        HygieneRule::B2,
                        HygieneLevel::Advisory,
                        cache_root.display().to_string(),
                        missing.join(", "),
                    )
                    .state("missing_slots")
                    .arg("count", DetailArg::Count(missing.len() as u64))
                    .arg("slots", DetailArg::Text(missing.join(", "))),
                );
            }
        }

        // A cache root inside a worktree is the second entry point B2 forbids: it is
        // outside the accounting, outside the quota, and no scan of the repository
        // root would attribute it to anything.
        let linked: Vec<WorktreeRecord> = self.linked_worktrees().cloned().collect();
        for record in &linked {
            let nested = Path::new(&record.path).join(CACHE_DIR);
            if nested.is_dir() {
                self.push(
                    HygieneFinding::new(
                        HygieneRule::B2,
                        HygieneLevel::Violation,
                        nested.display().to_string(),
                        format!("a second {CACHE_DIR}/ inside a worktree"),
                    )
                    .state("nested_cache_root")
                    .arg("slot", DetailArg::Text(CACHE_DIR.to_string())),
                );
            }
        }

        self.slot_sizes(&slots);
    }

    /// B5 and Q2's input: the size of every cache slot.
    fn slot_sizes(&mut self, slots: &[String]) {
        if slots.is_empty() {
            return;
        }
        let cache_root = self.cache_root();
        let mut total: u64 = 0;
        let mut unmeasured: u32 = 0;
        for slot in slots {
            if let Some(deadline) = self.opts.budget_deadline {
                if Instant::now() >= deadline {
                    self.warnings.push(format!(
                        "the scan budget ran out; {slot} and the slots after it were not measured"
                    ));
                    unmeasured += 1;
                    continue;
                }
            }
            let path = cache_root.join(slot);
            let measurement = measure_dir(&path, self.opts.per_dir_timeout);
            if let Some(bytes) = measurement.bytes {
                total = total.saturating_add(bytes);
                if slot != "shared" && bytes > self.opts.cache_slot_cap_bytes {
                    self.push(
                        HygieneFinding::new(
                            HygieneRule::B5,
                            HygieneLevel::Violation,
                            path.display().to_string(),
                            format!("{bytes} bytes in the slot"),
                        )
                        .state("slot_over_cap")
                        .arg("size", DetailArg::Bytes(bytes))
                        .arg(
                            "cap_bytes",
                            DetailArg::Bytes(self.opts.cache_slot_cap_bytes),
                        ),
                    );
                }
            } else {
                unmeasured += 1;
                self.push(
                    HygieneFinding::new(
                        HygieneRule::B5,
                        HygieneLevel::Unknown,
                        path.display().to_string(),
                        "the slot could not be measured".to_string(),
                    )
                    .state("slot_unmeasurable")
                    .arg("slot", DetailArg::Text(slot.clone())),
                );
            }
        }
        self.cache_bytes = Some((total, unmeasured));
    }

    /// Q1, Q2: free space and the cache's share of it.
    fn quotas(&mut self) {
        let subject = self.repo.display().to_string();
        let space = match self.volume_space() {
            Ok(space) => space,
            Err(error) => {
                self.push(
                    HygieneFinding::new(HygieneRule::Q1, HygieneLevel::Unknown, &subject, error)
                        .state("space_unknown"),
                );
                return;
            }
        };
        if space.total_bytes == 0 {
            self.push(
                HygieneFinding::new(
                    HygieneRule::Q1,
                    HygieneLevel::Unknown,
                    &subject,
                    "the volume reports a total size of 0 bytes".to_string(),
                )
                .state("space_unknown"),
            );
            return;
        }
        let percent = space.available_bytes.saturating_mul(100) / space.total_bytes;
        let evidence = format!(
            "{} of {} bytes available ({percent}%)",
            space.available_bytes, space.total_bytes
        );
        let args = |finding: HygieneFinding| {
            finding
                .arg("percent", DetailArg::Count(percent))
                .arg("available", DetailArg::Bytes(space.available_bytes))
                .arg("total", DetailArg::Bytes(space.total_bytes))
        };
        if percent < Q1_FROZEN_PERCENT {
            self.push(args(
                HygieneFinding::new(HygieneRule::Q1, HygieneLevel::Violation, &subject, evidence)
                    .state("frozen"),
            ));
        } else if percent < Q1_NORMAL_PERCENT {
            self.push(args(
                HygieneFinding::new(HygieneRule::Q1, HygieneLevel::Advisory, &subject, evidence)
                    .state("warning"),
            ));
        }

        let Some((bytes, unmeasured)) = self.cache_bytes else {
            return;
        };
        if unmeasured > 0 {
            self.push(
                HygieneFinding::new(
                    HygieneRule::Q2,
                    HygieneLevel::Unknown,
                    self.cache_root().display().to_string(),
                    format!("{unmeasured} cache slots could not be measured"),
                )
                .state("share_unknown")
                .arg("count", DetailArg::Count(u64::from(unmeasured))),
            );
            return;
        }
        let share = bytes.saturating_mul(100) / space.total_bytes;
        if share > Q2_SHARE_PERCENT {
            self.push(
                HygieneFinding::new(
                    HygieneRule::Q2,
                    HygieneLevel::Advisory,
                    self.cache_root().display().to_string(),
                    format!("{bytes} bytes of {} ({share}%)", space.total_bytes),
                )
                .state("over_share")
                .arg("size", DetailArg::Bytes(bytes))
                .arg("percent", DetailArg::Count(share))
                .arg("limit", DetailArg::Count(Q2_SHARE_PERCENT)),
            );
        }
    }

    fn volume_space(&mut self) -> Result<VolumeSpace, String> {
        if let Some(space) = self.opts.volume {
            self.volume = Some(space);
            return Ok(space);
        }
        if let Some(space) = self.volume {
            return Ok(space);
        }
        let available = fs4::available_space(&self.repo)
            .map_err(|error| format!("the volume's free space could not be read: {error}"))?;
        let total = fs4::total_space(&self.repo)
            .map_err(|error| format!("the volume's capacity could not be read: {error}"))?;
        let space = VolumeSpace {
            available_bytes: available,
            total_bytes: total,
        };
        self.volume = Some(space);
        Ok(space)
    }

    /// X1: the written exceptions, and what is wrong with the file itself.
    fn exceptions(&mut self) -> Vec<Waiver> {
        let path = self.repo.join(KEEP_FILE);
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
            Err(error) => {
                self.warnings.push(format!(
                    "{KEEP_FILE} could not be read, so its exceptions do not apply: {error}"
                ));
                return Vec::new();
            }
        };
        let today = format_rfc3339(self.opts.now);
        let today = today.get(0..10).unwrap_or_default().to_string();
        let mut waivers = Vec::new();
        for (index, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let number = index + 1;
            let mut fields = line.split('\t');
            let entry = fields.next().unwrap_or_default().trim();
            let rules = fields.next().unwrap_or_default().trim();
            let expires = fields.next().unwrap_or_default().trim();
            let subject = self.exempt_path(entry);
            if entry.is_empty() || expires.is_empty() {
                // The document's own words: an exception without an expiry date is
                // the old behaviour with extra steps.
                self.push(
                    HygieneFinding::new(
                        HygieneRule::X1,
                        HygieneLevel::Violation,
                        subject.display().to_string(),
                        format!("{KEEP_FILE}:{number}: {line}"),
                    )
                    .state("entry_without_expiry")
                    .arg("line", DetailArg::Count(number as u64))
                    .arg("entry", DetailArg::Text(line.to_string())),
                );
                continue;
            }
            if !is_date(expires) {
                self.push(
                    HygieneFinding::new(
                        HygieneRule::X1,
                        HygieneLevel::Violation,
                        subject.display().to_string(),
                        format!("{KEEP_FILE}:{number}: {expires}"),
                    )
                    .state("expiry_not_a_date")
                    .arg("line", DetailArg::Count(number as u64))
                    .arg("expires", DetailArg::Text(expires.to_string())),
                );
                continue;
            }
            if expires < today.as_str() {
                self.push(
                    HygieneFinding::new(
                        HygieneRule::X1,
                        HygieneLevel::Advisory,
                        subject.display().to_string(),
                        format!("{KEEP_FILE}:{number}: expired {expires}"),
                    )
                    .state("expired")
                    .arg("line", DetailArg::Count(number as u64))
                    .arg("expires", DetailArg::Text(expires.to_string())),
                );
                continue;
            }
            if !subject.exists() {
                self.push(
                    HygieneFinding::new(
                        HygieneRule::X1,
                        HygieneLevel::Advisory,
                        subject.display().to_string(),
                        format!("{KEEP_FILE}:{number}: {entry}"),
                    )
                    .state("dangling")
                    .arg("line", DetailArg::Count(number as u64)),
                );
            }
            let rules = if rules.is_empty() || rules == "*" {
                Vec::new()
            } else {
                rules
                    .split(',')
                    .map(|rule| rule.trim().to_string())
                    .filter(|rule| !rule.is_empty())
                    .collect()
            };
            waivers.push(Waiver {
                path: subject,
                rules,
                expires: expires.to_string(),
            });
        }
        waivers
    }

    /// The directory's creation time, when the platform reports one.
    ///
    /// `None` is "not measured", never "old": the caller reports it as unchecked
    /// instead of guessing an age (D3).
    fn creation_time(&self, path: &Path) -> Option<SystemTime> {
        if let Some(overrides) = &self.opts.creation_override {
            if let Some((_, at)) = overrides.iter().find(|(known, _)| known == path) {
                return Some(*at);
            }
        }
        std::fs::metadata(path).ok()?.created().ok()
    }

    /// A waiver's path, made absolute against the repository it was written in.
    fn exempt_path(&self, entry: &str) -> PathBuf {
        let path = PathBuf::from(entry);
        if path.is_absolute() {
            path
        } else {
            self.repo.join(path)
        }
    }
}

fn apply_waivers(findings: &mut [HygieneFinding], waivers: &[Waiver]) {
    for finding in findings.iter_mut() {
        if finding.rule == HygieneRule::X1 {
            continue;
        }
        let subject = Path::new(&finding.subject);
        let Some(waiver) = waivers.iter().find(|waiver| {
            subject.starts_with(&waiver.path)
                && (waiver.rules.is_empty()
                    || waiver
                        .rules
                        .iter()
                        .any(|rule| rule == finding.rule.as_str()))
        }) else {
            continue;
        };
        finding.level = HygieneLevel::Ok;
        finding.waived_until = Some(waiver.expires.clone());
    }
}

fn compare_findings(a: &HygieneFinding, b: &HygieneFinding) -> std::cmp::Ordering {
    b.level
        .rank()
        .cmp(&a.level.rank())
        .then(a.rule.cmp(&b.rule))
        .then_with(|| a.subject.cmp(&b.subject))
}

/// The repository root as Git reports it, so every path comparison uses one
/// spelling. A `~`-relative or symlinked argument would otherwise make every
/// worktree look like it sits outside the repository.
fn repository_root(runner: &dyn GitRunner, root: &Path) -> PathBuf {
    if let Ok(output) = runner.run(root, &["rev-parse", "--show-toplevel"]) {
        let text = output.stdout.trim();
        if output.success() && !text.is_empty() {
            return PathBuf::from(text);
        }
    }
    std::path::absolute(root).unwrap_or_else(|_| root.to_path_buf())
}

/// One line of `git check-ignore -v` output.
struct IgnoreRule {
    source: String,
    line: u64,
    pattern: String,
}

fn parse_ignore_rule(stdout: &str) -> Option<IgnoreRule> {
    let line = stdout.lines().find(|line| !line.trim().is_empty())?;
    let (meta, _path) = line.split_once('\t')?;
    let (rest, pattern) = meta.rsplit_once(':')?;
    let (source, line_number) = rest.rsplit_once(':')?;
    if pattern.is_empty() {
        // `--non-matching` prints an empty rule for a path no pattern covers.
        return None;
    }
    Some(IgnoreRule {
        source: source.to_string(),
        line: line_number.parse().unwrap_or(0),
        pattern: pattern.to_string(),
    })
}

/// Names of the subdirectories of `dir`, sorted; empty when it cannot be read.
fn subdirectory_names(dir: &Path) -> Vec<String> {
    let mut names = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return names;
    };
    for entry in entries.flatten() {
        if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            names.push(entry.file_name().to_string_lossy().into_owned());
        }
    }
    names.sort();
    names
}

/// Whether the name ends with a real `YYYYMMDD`; `None` when it has no date suffix
/// at all.
fn date_suffix(name: &str) -> Option<bool> {
    let (_, date) = name.rsplit_once('-')?;
    if date.len() != 8 || !date.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let year: i32 = date.get(0..4)?.parse().ok()?;
    let month: u32 = date.get(4..6)?.parse().ok()?;
    let day: u32 = date.get(6..8)?.parse().ok()?;
    Some((1970..=2999).contains(&year) && (1..=12).contains(&month) && (1..=31).contains(&day))
}

/// A repository name reduced to the slug its worktree names are expected to start
/// with.
fn repo_slug(name: &str) -> String {
    let mut slug = String::new();
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.extend(ch.to_lowercase());
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    slug.trim_matches('-').to_string()
}

fn is_date(text: &str) -> bool {
    let bytes = text.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return false;
    }
    let Some(year) = text.get(0..4).and_then(|part| part.parse::<i32>().ok()) else {
        return false;
    };
    let Some(month) = text.get(5..7).and_then(|part| part.parse::<u32>().ok()) else {
        return false;
    };
    let Some(day) = text.get(8..10).and_then(|part| part.parse::<u32>().ok()) else {
        return false;
    };
    (1970..=2999).contains(&year) && (1..=12).contains(&month) && (1..=31).contains(&day)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::testing::{Canned, FakeGitRunner};
    use crate::git::GitOutput;
    use crate::reason::SafetyAssessment;

    /// The policy document, compiled in: a rule that exists in one and not the other
    /// is a drift this test is the only thing that would notice.
    const POLICY: &str = include_str!("../../../docs/DISK_HYGIENE.md");

    fn now() -> SystemTime {
        SystemTime::now()
    }

    fn days_ago(days: u64) -> SystemTime {
        now()
            .checked_sub(Duration::from_secs(days * 86_400))
            .expect("a time before the epoch")
    }

    fn timestamp(days: u64) -> String {
        format_rfc3339(days_ago(days))
    }

    struct Fixture {
        _dir: tempfile::TempDir,
        repo: PathBuf,
    }

    fn fixture() -> Fixture {
        let dir = tempfile::tempdir().expect("temp dir");
        let repo = dir.path().join("repo");
        std::fs::create_dir_all(&repo).expect("create repo");
        Fixture { _dir: dir, repo }
    }

    fn canned(args: &[&str], stdout: &str) -> (Vec<String>, Canned) {
        (
            args.iter().map(|arg| (*arg).to_string()).collect(),
            Canned::Out(GitOutput {
                stdout: stdout.to_string(),
                stderr: String::new(),
                code: Some(0),
            }),
        )
    }

    /// A runner that answers the two calls the audit makes.
    ///
    /// `worktree` and `cache` are the `git check-ignore -v` lines for the two
    /// declared paths; `None` means "no rule matches".
    fn runner(repo: &Path, worktree: Option<&str>, cache: Option<&str>) -> FakeGitRunner {
        let not_matched = |path: &str| format!("::\t{path}\n");
        let mut rules = vec![canned(
            &["rev-parse", "--show-toplevel"],
            &format!("{}\n", repo.display()),
        )];
        for (probe, answer) in [
            (".worktree/__probe__", worktree),
            ("build-cache/__probe__", cache),
        ] {
            let stdout = match answer {
                Some(line) => line.to_string(),
                None => not_matched(probe),
            };
            rules.push(canned(
                &["check-ignore", "-v", "-n", "--no-index", "--", probe],
                &stdout,
            ));
        }
        FakeGitRunner::new(rules)
    }

    fn worktree(path: &Path, branch: &str) -> WorktreeRecord {
        WorktreeRecord {
            path: path.display().to_string(),
            branch: Some(branch.to_string()),
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
            merged_into_default: Some(false),
            absorbed_into_default: false,
            stash_count: 0,
            last_updated_at: Some(timestamp(0)),
            agent: None,
            size_bytes: Some(1024),
            safety: SafetyAssessment::from_reasons(Vec::new()),
        }
    }

    fn artifact(path: &Path, kind: &str) -> ArtifactRecord {
        ArtifactRecord {
            path: path.display().to_string(),
            kind: kind.to_string(),
            ecosystem: "generic".to_string(),
            project_root: path.parent().unwrap_or(path).display().to_string(),
            worktree_path: None,
            size_bytes: Some(2048),
            file_count: 3,
            last_updated_at: Some(timestamp(0)),
            git_ignored: Some(true),
            git_tracked: Some(false),
            is_symlink: false,
            symlink_target: None,
            safety: SafetyAssessment::from_reasons(Vec::new()),
        }
    }

    fn options() -> HygieneOptions {
        HygieneOptions {
            now: now(),
            volume: Some(VolumeSpace {
                available_bytes: 500,
                total_bytes: 1_000,
            }),
            creation_override: Some(Vec::new()),
            ..HygieneOptions::default()
        }
    }

    fn main_record(repo: &Path) -> WorktreeRecord {
        let mut record = worktree(repo, "main");
        record.is_main = true;
        record
    }

    fn audit(
        fixture: &Fixture,
        worktrees: &[WorktreeRecord],
        artifacts: &[ArtifactRecord],
        opts: &HygieneOptions,
        ignore: (Option<&str>, Option<&str>),
    ) -> HygieneReport {
        let runner = runner(&fixture.repo, ignore.0, ignore.1);
        audit_hygiene(&runner, &fixture.repo, worktrees, artifacts, opts)
    }

    fn states(report: &HygieneReport, rule: HygieneRule) -> Vec<String> {
        report
            .findings
            .iter()
            .filter(|finding| finding.rule == rule)
            .map(|finding| match finding.args.get("state") {
                Some(DetailArg::Text(state)) => state.clone(),
                _ => String::new(),
            })
            .collect()
    }

    fn level_of(report: &HygieneReport, rule: HygieneRule) -> HygieneLevel {
        report
            .rules
            .iter()
            .find(|status| status.rule == rule)
            .expect("every rule has a status")
            .level
    }

    /// Both declared paths ignored, anchored, in the repository's own `.gitignore`.
    const IGNORED: (Option<&str>, Option<&str>) = (
        Some(".gitignore:3:/.worktree/\t.worktree/__probe__\n"),
        Some(".gitignore:4:/build-cache/\tbuild-cache/__probe__\n"),
    );

    // ---- the document and the code ----

    #[test]
    fn every_rule_in_the_policy_document_is_known() {
        // The policy is the source of the rule ids; this module is the source of
        // what gets checked. A rule added to one and not the other is exactly the
        // drift that would make the report quietly incomplete.
        let documented: Vec<String> = POLICY
            .lines()
            .filter_map(|line| line.strip_prefix("**"))
            .filter_map(|rest| rest.split_once(" — ").map(|(id, _)| id.to_string()))
            .filter(|id| {
                let mut chars = id.chars();
                chars.next().is_some_and(|first| first.is_ascii_uppercase())
                    && id.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '.')
            })
            .collect();
        let known: Vec<&str> = HygieneRule::ALL.iter().map(|rule| rule.as_str()).collect();
        for rule in &known {
            assert!(
                documented.iter().any(|id| id == rule),
                "rule {rule} is implemented but not in docs/DISK_HYGIENE.md"
            );
        }
        // C1–C3 are machine-level: outside one repository, so outside this audit.
        let outside: Vec<&String> = documented
            .iter()
            .filter(|id| !known.contains(&id.as_str()))
            .collect();
        assert_eq!(
            outside,
            ["C1", "C2", "C3"],
            "unexpected rule in the document"
        );
    }

    #[test]
    fn rule_ids_round_trip_through_serde() {
        for rule in HygieneRule::ALL {
            let json = serde_json::to_string(&rule).expect("serialize rule");
            assert_eq!(json, format!("\"{}\"", rule.as_str()));
        }
        assert_eq!(HygieneRule::ALL.len(), 22);
    }

    #[test]
    fn levels_serialize_lowercase() {
        for (level, expected) in [
            (HygieneLevel::Ok, "ok"),
            (HygieneLevel::Advisory, "advisory"),
            (HygieneLevel::Violation, "violation"),
            (HygieneLevel::Unknown, "unknown"),
        ] {
            let json = serde_json::to_string(&level).expect("serialize level");
            assert_eq!(json, format!("\"{expected}\""));
        }
    }

    // ---- the compliant repository ----

    /// A repository that follows every rule the audit can see.
    fn compliant(fixture: &Fixture) -> (Vec<WorktreeRecord>, Vec<ArtifactRecord>) {
        let linked = fixture.repo.join(".worktree/repo-feature-20260101");
        std::fs::create_dir_all(&linked).expect("create worktree dir");
        std::fs::create_dir_all(fixture.repo.join("build-cache/repo-feature-20260101"))
            .expect("create slot");
        std::fs::create_dir_all(fixture.repo.join("build-cache/main")).expect("create main slot");
        let artifacts = vec![artifact(
            &fixture
                .repo
                .join("build-cache/repo-feature-20260101/target"),
            "target",
        )];
        (
            vec![main_record(&fixture.repo), worktree(&linked, "feature")],
            artifacts,
        )
    }

    #[test]
    fn a_compliant_repository_reports_no_violation() {
        let fixture = fixture();
        let (worktrees, artifacts) = compliant(&fixture);
        let report = audit(&fixture, &worktrees, &artifacts, &options(), IGNORED);

        let violations: Vec<&HygieneFinding> = report
            .findings
            .iter()
            .filter(|finding| finding.level == HygieneLevel::Violation)
            .collect();
        assert!(
            violations.is_empty(),
            "unexpected violations: {violations:?}"
        );
        // T4 is left out of this list on purpose: its age comes from a real birth
        // time, which not every platform reports, so it is asserted through the
        // injected clock in its own two tests instead.
        for rule in [
            HygieneRule::L1,
            HygieneRule::L3,
            HygieneRule::L4,
            HygieneRule::L5,
            HygieneRule::T1,
            HygieneRule::T2,
            HygieneRule::T3,
            HygieneRule::B1,
            HygieneRule::B11,
            HygieneRule::B2,
            HygieneRule::B5,
            HygieneRule::Q1,
            HygieneRule::Q2,
            HygieneRule::X1,
        ] {
            assert_eq!(level_of(&report, rule), HygieneLevel::Ok, "{rule:?}");
        }
    }

    #[test]
    fn the_audit_reads_the_directories_and_nothing_else() {
        // D1: a scan writes nothing. The audit is part of the scan.
        let fixture = fixture();
        let (worktrees, artifacts) = compliant(&fixture);
        let before = std::fs::read_dir(&fixture.repo).expect("read repo").count();
        let _ = audit(&fixture, &worktrees, &artifacts, &options(), IGNORED);
        let after = std::fs::read_dir(&fixture.repo).expect("read repo").count();
        assert_eq!(before, after);
    }

    // ---- L1 ----

    #[test]
    fn a_worktree_outside_the_declared_root_is_a_violation() {
        let fixture = fixture();
        let outside = fixture.repo.parent().expect("parent").join("elsewhere");
        std::fs::create_dir_all(&outside).expect("create outside worktree");
        let worktrees = vec![main_record(&fixture.repo), worktree(&outside, "feature")];
        let report = audit(&fixture, &worktrees, &[], &options(), IGNORED);

        assert_eq!(level_of(&report, HygieneRule::L1), HygieneLevel::Violation);
        assert_eq!(states(&report, HygieneRule::L1), ["outside_repository"]);
    }

    #[test]
    fn a_worktree_inside_the_repository_but_outside_the_declared_root_is_a_violation() {
        let fixture = fixture();
        let beside = fixture.repo.join("feature");
        std::fs::create_dir_all(&beside).expect("create worktree");
        let worktrees = vec![main_record(&fixture.repo), worktree(&beside, "feature")];
        let report = audit(&fixture, &worktrees, &[], &options(), IGNORED);

        assert_eq!(states(&report, HygieneRule::L1), ["inside_but_undeclared"]);
    }

    // ---- L3 ----

    #[test]
    fn a_worktree_inside_another_worktree_is_a_violation() {
        let fixture = fixture();
        let outer = fixture.repo.join(".worktree/repo-feature-20260101");
        let inner = outer.join(".worktree/repo-fix-20260102");
        std::fs::create_dir_all(&inner).expect("create nested worktree");
        let worktrees = vec![
            main_record(&fixture.repo),
            worktree(&outer, "feature"),
            worktree(&inner, "fix"),
        ];
        let report = audit(&fixture, &worktrees, &[], &options(), IGNORED);

        assert_eq!(level_of(&report, HygieneRule::L3), HygieneLevel::Violation);
        assert_eq!(states(&report, HygieneRule::L3), ["nested_worktree"]);
    }

    // ---- L4 ----

    fn name_of(fixture: &Fixture, name: &str) -> HygieneReport {
        let path = fixture.repo.join(WORKTREE_DIR).join(name);
        std::fs::create_dir_all(&path).expect("create worktree");
        let worktrees = vec![main_record(&fixture.repo), worktree(&path, "feature")];
        audit(fixture, &worktrees, &[], &options(), IGNORED)
    }

    #[test]
    fn an_unslugged_name_is_reported() {
        let fixture = fixture();
        let report = name_of(&fixture, "repo-Feature-20260101");
        assert_eq!(states(&report, HygieneRule::L4), ["not_a_slug"]);
        assert_eq!(level_of(&report, HygieneRule::L4), HygieneLevel::Advisory);
    }

    #[test]
    fn a_name_without_a_date_suffix_is_reported() {
        let fixture = fixture();
        let report = name_of(&fixture, "repo-feature");
        assert!(states(&report, HygieneRule::L4).contains(&"no_date_suffix".to_string()));
    }

    #[test]
    fn a_name_with_an_impossible_date_is_reported() {
        let fixture = fixture();
        let report = name_of(&fixture, "repo-feature-20261399");
        assert_eq!(states(&report, HygieneRule::L4), ["invalid_date"]);
    }

    #[test]
    fn a_name_over_sixty_characters_is_reported() {
        let fixture = fixture();
        let long = format!("repo-{}-20260101", "a".repeat(60));
        let report = name_of(&fixture, &long);
        assert_eq!(states(&report, HygieneRule::L4), ["too_long"]);
    }

    #[test]
    fn a_name_without_the_repository_prefix_is_reported() {
        let fixture = fixture();
        let report = name_of(&fixture, "feature-20260101");
        assert_eq!(states(&report, HygieneRule::L4), ["missing_repo_prefix"]);
    }

    #[test]
    fn a_conforming_name_produces_no_finding() {
        let fixture = fixture();
        let report = name_of(&fixture, "repo-feature-20260101");
        assert!(states(&report, HygieneRule::L4).is_empty());
    }

    // ---- L5 ----

    #[test]
    fn a_missing_ignore_entry_is_a_violation_once_a_worktree_exists() {
        let fixture = fixture();
        let (worktrees, _) = compliant(&fixture);
        let report = audit(
            &fixture,
            &worktrees,
            &[],
            &options(),
            (
                None,
                Some(".gitignore:4:/build-cache/\tbuild-cache/__probe__\n"),
            ),
        );
        assert_eq!(states(&report, HygieneRule::L5), ["missing_entry"]);
        assert_eq!(level_of(&report, HygieneRule::L5), HygieneLevel::Violation);
    }

    #[test]
    fn a_missing_ignore_entry_is_only_advice_before_the_first_worktree() {
        let fixture = fixture();
        let worktrees = vec![main_record(&fixture.repo)];
        let report = audit(&fixture, &worktrees, &[], &options(), (None, None));

        assert_eq!(level_of(&report, HygieneRule::L5), HygieneLevel::Advisory);
        assert!(states(&report, HygieneRule::L5).contains(&"no_worktrees_yet".to_string()));
    }

    #[test]
    fn an_unanchored_ignore_entry_is_a_violation() {
        // Without the leading slash the pattern matches at any depth, so a build
        // inside a worktree creates a second `build-cache/` that nothing accounts
        // for.
        let fixture = fixture();
        let (worktrees, _) = compliant(&fixture);
        let report = audit(
            &fixture,
            &worktrees,
            &[],
            &options(),
            (
                Some(".gitignore:3:/.worktree/\t.worktree/__probe__\n"),
                Some(".gitignore:4:build-cache/\tbuild-cache/__probe__\n"),
            ),
        );
        assert_eq!(states(&report, HygieneRule::L5), ["unanchored_pattern"]);
    }

    #[test]
    fn a_local_only_exclusion_is_a_violation() {
        let fixture = fixture();
        let (worktrees, _) = compliant(&fixture);
        let report = audit(
            &fixture,
            &worktrees,
            &[],
            &options(),
            (
                Some(".git/info/exclude:1:/.worktree/\t.worktree/__probe__\n"),
                Some(".gitignore:4:/build-cache/\tbuild-cache/__probe__\n"),
            ),
        );
        assert_eq!(states(&report, HygieneRule::L5), ["local_only"]);
    }

    #[test]
    fn an_ignore_rule_from_elsewhere_is_advice_not_compliance() {
        let fixture = fixture();
        let (worktrees, _) = compliant(&fixture);
        let report = audit(
            &fixture,
            &worktrees,
            &[],
            &options(),
            (
                Some("../.gitignore:2:/.worktree/\t.worktree/__probe__\n"),
                Some(".gitignore:4:/build-cache/\tbuild-cache/__probe__\n"),
            ),
        );
        assert_eq!(states(&report, HygieneRule::L5), ["elsewhere"]);
        assert_eq!(level_of(&report, HygieneRule::L5), HygieneLevel::Advisory);
    }

    #[test]
    fn a_worktree_directory_inside_a_worktree_is_reported() {
        // The observable harm of an unanchored pattern: the nested `.worktree/`
        // exists and Git does not list it.
        let fixture = fixture();
        let (worktrees, _) = compliant(&fixture);
        let nested = fixture
            .repo
            .join(".worktree/repo-feature-20260101/.worktree");
        std::fs::create_dir_all(&nested).expect("create nested dir");
        let report = audit(&fixture, &worktrees, &[], &options(), IGNORED);
        assert!(states(&report, HygieneRule::L5).contains(&"nested_worktree_dir".to_string()));
    }

    // ---- T1 ----

    #[test]
    fn more_than_fifty_worktrees_is_a_violation() {
        let fixture = fixture();
        let mut worktrees = vec![main_record(&fixture.repo)];
        for index in 0..50 {
            let path = fixture
                .repo
                .join(WORKTREE_DIR)
                .join(format!("repo-branch{index}-20260101"));
            std::fs::create_dir_all(&path).expect("create worktree");
            worktrees.push(worktree(&path, &format!("branch{index}")));
        }
        let report = audit(&fixture, &worktrees, &[], &options(), IGNORED);

        assert_eq!(level_of(&report, HygieneRule::T1), HygieneLevel::Violation);
        assert_eq!(states(&report, HygieneRule::T1), ["over_cap"]);
    }

    #[test]
    fn approaching_the_cap_is_advice() {
        let fixture = fixture();
        let mut worktrees = vec![main_record(&fixture.repo)];
        for index in 0..45 {
            let path = fixture
                .repo
                .join(WORKTREE_DIR)
                .join(format!("repo-branch{index}-20260101"));
            std::fs::create_dir_all(&path).expect("create worktree");
            worktrees.push(worktree(&path, &format!("branch{index}")));
        }
        let report = audit(&fixture, &worktrees, &[], &options(), IGNORED);
        // 46 registered: past 90% of 50, below it.
        assert_eq!(level_of(&report, HygieneRule::T1), HygieneLevel::Advisory);
        assert_eq!(states(&report, HygieneRule::T1), ["near_cap"]);
    }

    #[test]
    fn a_handful_of_worktrees_is_fine() {
        let fixture = fixture();
        let (worktrees, _) = compliant(&fixture);
        let report = audit(&fixture, &worktrees, &[], &options(), IGNORED);
        assert_eq!(level_of(&report, HygieneRule::T1), HygieneLevel::Ok);
    }

    // ---- T2 ----

    #[test]
    fn a_merged_worktree_still_present_is_reported() {
        let fixture = fixture();
        let (mut worktrees, _) = compliant(&fixture);
        worktrees[1].merged_into_default = Some(true);
        let report = audit(&fixture, &worktrees, &[], &options(), IGNORED);

        assert_eq!(states(&report, HygieneRule::T2), ["merged_still_present"]);
        // Advisory on purpose: Git does not record when the merge happened, so the
        // 24-hour window cannot be proven from here.
        assert_eq!(level_of(&report, HygieneRule::T2), HygieneLevel::Advisory);
    }

    #[test]
    fn an_undetermined_merge_is_not_reported() {
        let fixture = fixture();
        let (mut worktrees, _) = compliant(&fixture);
        worktrees[1].merged_into_default = None;
        let report = audit(&fixture, &worktrees, &[], &options(), IGNORED);
        assert!(states(&report, HygieneRule::T2).is_empty());
    }

    // ---- T3 ----

    #[test]
    fn an_idle_clean_worktree_is_reported_as_stale() {
        let fixture = fixture();
        let (mut worktrees, _) = compliant(&fixture);
        let path = PathBuf::from(&worktrees[1].path);
        worktrees[1].last_updated_at = Some(timestamp(10));
        let mut opts = options();
        opts.creation_override = Some(vec![(path, days_ago(20))]);
        let report = audit(&fixture, &worktrees, &[], &opts, IGNORED);

        assert_eq!(states(&report, HygieneRule::T3), ["stale"]);
        let finding = report
            .findings
            .iter()
            .find(|finding| finding.rule == HygieneRule::T3)
            .expect("a T3 finding");
        assert_eq!(finding.args.get("days"), Some(&DetailArg::Count(10)));
    }

    #[test]
    fn an_idle_dirty_worktree_is_reported_but_not_as_removable() {
        let fixture = fixture();
        let (mut worktrees, _) = compliant(&fixture);
        let path = PathBuf::from(&worktrees[1].path);
        worktrees[1].last_updated_at = Some(timestamp(9));
        worktrees[1].dirty_files = 2;
        let mut opts = options();
        opts.creation_override = Some(vec![(path, days_ago(30))]);
        let report = audit(&fixture, &worktrees, &[], &opts, IGNORED);

        assert_eq!(states(&report, HygieneRule::T3), ["stale_with_changes"]);
    }

    #[test]
    fn a_new_worktree_on_an_old_branch_is_not_stale() {
        // The worktree is new; the branch it checks out is not. Calling this stale
        // would send the reader to remove work they created this morning.
        let fixture = fixture();
        let (mut worktrees, _) = compliant(&fixture);
        let path = PathBuf::from(&worktrees[1].path);
        worktrees[1].last_updated_at = Some(timestamp(200));
        let mut opts = options();
        opts.creation_override = Some(vec![(path, days_ago(1))]);
        let report = audit(&fixture, &worktrees, &[], &opts, IGNORED);

        assert!(states(&report, HygieneRule::T3).is_empty());
    }

    // ---- T4 ----

    #[test]
    fn a_worktree_past_the_absolute_ceiling_is_a_violation() {
        let fixture = fixture();
        let (mut worktrees, _) = compliant(&fixture);
        let path = PathBuf::from(&worktrees[1].path);
        worktrees[1].last_updated_at = Some(timestamp(1));
        let mut opts = options();
        opts.creation_override = Some(vec![(path, days_ago(31))]);
        let report = audit(&fixture, &worktrees, &[], &opts, IGNORED);

        assert_eq!(level_of(&report, HygieneRule::T4), HygieneLevel::Violation);
        assert_eq!(states(&report, HygieneRule::T4), ["past_ceiling"]);
    }

    #[test]
    fn an_unmeasurable_age_is_unchecked_not_fine() {
        // T4 has no other probe: if the creation time cannot be read, the rule is
        // unknown. Reporting it as `Ok` would be the guess D3 forbids.
        let fixture = fixture();
        let ghost = fixture.repo.join(".worktree/repo-ghost-20260101");
        let worktrees = vec![main_record(&fixture.repo), worktree(&ghost, "ghost")];
        let report = audit(&fixture, &worktrees, &[], &options(), IGNORED);
        assert_eq!(level_of(&report, HygieneRule::T4), HygieneLevel::Unknown);
        assert_eq!(states(&report, HygieneRule::T4), ["age_unknown"]);
    }

    #[test]
    fn a_young_worktree_is_within_the_ceiling() {
        let fixture = fixture();
        let (worktrees, _) = compliant(&fixture);
        let path = PathBuf::from(&worktrees[1].path);
        let mut opts = options();
        opts.creation_override = Some(vec![(path, days_ago(5))]);
        let report = audit(&fixture, &worktrees, &[], &opts, IGNORED);
        assert_eq!(level_of(&report, HygieneRule::T4), HygieneLevel::Ok);
    }

    // ---- B1, B1.1 ----

    #[test]
    fn build_output_outside_the_cache_root_is_a_violation() {
        let fixture = fixture();
        let (worktrees, _) = compliant(&fixture);
        let loose = artifact(&fixture.repo.join("node_modules"), "node_modules");
        let report = audit(&fixture, &worktrees, &[loose], &options(), IGNORED);

        assert_eq!(level_of(&report, HygieneRule::B1), HygieneLevel::Violation);
        assert_eq!(states(&report, HygieneRule::B1), ["outside_declared"]);
    }

    #[test]
    fn tracked_build_output_breaks_the_git_status_test() {
        let fixture = fixture();
        let (worktrees, _) = compliant(&fixture);
        let mut tracked = artifact(&fixture.repo.join("build-cache/main/dist"), "dist");
        tracked.git_tracked = Some(true);
        let report = audit(&fixture, &worktrees, &[tracked], &options(), IGNORED);

        assert_eq!(level_of(&report, HygieneRule::B11), HygieneLevel::Violation);
        assert_eq!(states(&report, HygieneRule::B11), ["tracked_in_git"]);
    }

    #[test]
    fn unignored_build_output_appears_in_git_status() {
        let fixture = fixture();
        let (worktrees, _) = compliant(&fixture);
        let mut loose = artifact(&fixture.repo.join("build-cache/main/out"), "out");
        loose.git_ignored = Some(false);
        let report = audit(&fixture, &worktrees, &[loose], &options(), IGNORED);

        assert_eq!(states(&report, HygieneRule::B11), ["untracked_in_status"]);
    }

    #[test]
    fn an_unreadable_ignore_status_is_not_a_violation() {
        let fixture = fixture();
        let (worktrees, _) = compliant(&fixture);
        let mut unknown = artifact(&fixture.repo.join("build-cache/main/out"), "out");
        unknown.git_ignored = None;
        unknown.git_tracked = None;
        let report = audit(&fixture, &worktrees, &[unknown], &options(), IGNORED);
        assert_eq!(level_of(&report, HygieneRule::B11), HygieneLevel::Ok);
    }

    // ---- B2 ----

    #[test]
    fn a_cache_slot_without_a_worktree_is_a_violation() {
        let fixture = fixture();
        let (worktrees, _) = compliant(&fixture);
        std::fs::create_dir_all(fixture.repo.join("build-cache/orphan")).expect("create slot");
        let report = audit(&fixture, &worktrees, &[], &options(), IGNORED);
        assert_eq!(states(&report, HygieneRule::B2), ["orphan_slot"]);
        assert_eq!(level_of(&report, HygieneRule::B2), HygieneLevel::Violation);
    }

    #[test]
    fn a_worktree_without_a_slot_is_advice() {
        // A worktree that has not built yet has no slot, and that is normal; the
        // report says so once rather than once per worktree.
        let fixture = fixture();
        let (worktrees, _) = compliant(&fixture);
        std::fs::remove_dir_all(fixture.repo.join("build-cache/repo-feature-20260101"))
            .expect("remove slot");
        let report = audit(&fixture, &worktrees, &[], &options(), IGNORED);
        assert_eq!(states(&report, HygieneRule::B2), ["missing_slots"]);
        assert_eq!(level_of(&report, HygieneRule::B2), HygieneLevel::Advisory);
    }

    #[test]
    fn a_missing_cache_root_is_reported_once_worktrees_exist() {
        let fixture = fixture();
        let (worktrees, _) = compliant(&fixture);
        std::fs::remove_dir_all(fixture.repo.join("build-cache")).expect("remove cache root");
        let report = audit(&fixture, &worktrees, &[], &options(), IGNORED);
        assert_eq!(states(&report, HygieneRule::B2), ["no_cache_root"]);
    }

    #[test]
    fn a_second_cache_root_inside_a_worktree_is_a_violation() {
        let fixture = fixture();
        let (worktrees, _) = compliant(&fixture);
        std::fs::create_dir_all(
            fixture
                .repo
                .join(".worktree/repo-feature-20260101/build-cache"),
        )
        .expect("create nested cache");
        let report = audit(&fixture, &worktrees, &[], &options(), IGNORED);
        assert!(states(&report, HygieneRule::B2).contains(&"nested_cache_root".to_string()));
    }

    // ---- B5 ----

    #[test]
    fn a_slot_over_the_cap_is_a_violation() {
        let fixture = fixture();
        let (worktrees, _) = compliant(&fixture);
        let slot = fixture.repo.join("build-cache/main");
        std::fs::write(slot.join("blob"), vec![0_u8; 4096]).expect("write blob");
        let mut opts = options();
        opts.cache_slot_cap_bytes = 1024;
        let report = audit(&fixture, &worktrees, &[], &opts, IGNORED);

        assert_eq!(level_of(&report, HygieneRule::B5), HygieneLevel::Violation);
        assert_eq!(states(&report, HygieneRule::B5), ["slot_over_cap"]);
    }

    #[test]
    fn a_slot_under_the_cap_is_fine() {
        let fixture = fixture();
        let (worktrees, _) = compliant(&fixture);
        let report = audit(&fixture, &worktrees, &[], &options(), IGNORED);
        assert_eq!(level_of(&report, HygieneRule::B5), HygieneLevel::Ok);
    }

    // ---- Q1, Q2 ----

    fn with_volume(available: u64, total: u64) -> HygieneOptions {
        let mut opts = options();
        opts.volume = Some(VolumeSpace {
            available_bytes: available,
            total_bytes: total,
        });
        opts
    }

    #[test]
    fn free_space_below_ten_percent_freezes_new_work() {
        let fixture = fixture();
        let report = audit(&fixture, &[], &[], &with_volume(50, 1_000), IGNORED);
        assert_eq!(level_of(&report, HygieneRule::Q1), HygieneLevel::Violation);
        assert_eq!(states(&report, HygieneRule::Q1), ["frozen"]);
    }

    #[test]
    fn free_space_between_ten_and_fifteen_percent_warns() {
        let fixture = fixture();
        let report = audit(&fixture, &[], &[], &with_volume(120, 1_000), IGNORED);
        assert_eq!(level_of(&report, HygieneRule::Q1), HygieneLevel::Advisory);
        assert_eq!(states(&report, HygieneRule::Q1), ["warning"]);
    }

    #[test]
    fn free_space_above_fifteen_percent_is_normal() {
        let fixture = fixture();
        let report = audit(&fixture, &[], &[], &with_volume(500, 1_000), IGNORED);
        assert_eq!(level_of(&report, HygieneRule::Q1), HygieneLevel::Ok);
    }

    #[test]
    fn a_cache_over_a_tenth_of_the_volume_is_advice() {
        let fixture = fixture();
        let (worktrees, _) = compliant(&fixture);
        std::fs::write(fixture.repo.join("build-cache/main/blob"), vec![0_u8; 4096])
            .expect("write blob");
        // 4 KiB of a 11 KiB volume is well over a tenth.
        let report = audit(
            &fixture,
            &worktrees,
            &[],
            &with_volume(11_000, 11_000),
            IGNORED,
        );
        assert_eq!(states(&report, HygieneRule::Q2), ["over_share"]);
        assert_eq!(level_of(&report, HygieneRule::Q2), HygieneLevel::Advisory);
    }

    #[test]
    fn an_unreadable_volume_leaves_the_quota_unchecked() {
        let fixture = fixture();
        let mut opts = options();
        // An empty volume cannot be read: `total_bytes` is what a failed statfs
        // gives, and dividing by it would answer Infinity rather than "unknown".
        opts.volume = Some(VolumeSpace {
            available_bytes: 0,
            total_bytes: 0,
        });
        let report = audit(&fixture, &[], &[], &opts, IGNORED);
        assert_eq!(level_of(&report, HygieneRule::Q1), HygieneLevel::Unknown);
    }

    // ---- X1 ----

    fn keep_file(fixture: &Fixture, body: &str) {
        std::fs::write(fixture.repo.join(KEEP_FILE), body).expect("write keep file");
    }

    #[test]
    fn a_waiver_without_an_expiry_is_a_violation() {
        let fixture = fixture();
        keep_file(&fixture, "/some/path\n");
        let report = audit(&fixture, &[], &[], &options(), IGNORED);
        assert_eq!(level_of(&report, HygieneRule::X1), HygieneLevel::Violation);
        assert_eq!(states(&report, HygieneRule::X1), ["entry_without_expiry"]);
    }

    #[test]
    fn a_waiver_whose_expiry_is_not_a_date_is_a_violation() {
        let fixture = fixture();
        keep_file(&fixture, "/some/path\t*\tnext week\n");
        let report = audit(&fixture, &[], &[], &options(), IGNORED);
        assert_eq!(states(&report, HygieneRule::X1), ["expiry_not_a_date"]);
    }

    #[test]
    fn an_expired_waiver_is_reported_and_no_longer_applies() {
        let fixture = fixture();
        let loose = fixture.repo.join("node_modules");
        std::fs::create_dir_all(&loose).expect("create artifact");
        keep_file(&fixture, &format!("{}\tB1\t20200101\n", loose.display()));
        // The date is not hyphenated, so it is refused as a date at all; the
        // hyphenated expired form is the case below.
        let report = audit(&fixture, &[], &[], &options(), IGNORED);
        assert_eq!(states(&report, HygieneRule::X1), ["expiry_not_a_date"]);

        keep_file(&fixture, &format!("{}\tB1\t2020-01-01\n", loose.display()));
        let report = audit(&fixture, &[], &[], &options(), IGNORED);
        assert_eq!(states(&report, HygieneRule::X1), ["expired"]);
    }

    #[test]
    fn a_waiver_for_a_path_that_is_gone_is_reported() {
        let fixture = fixture();
        keep_file(
            &fixture,
            &format!("{}\tB1\t2999-01-01\n", fixture.repo.join("gone").display()),
        );
        let report = audit(&fixture, &[], &[], &options(), IGNORED);
        assert_eq!(states(&report, HygieneRule::X1), ["dangling"]);
    }

    #[test]
    fn a_valid_waiver_accepts_the_finding_it_names() {
        let fixture = fixture();
        let loose = fixture.repo.join("node_modules");
        std::fs::create_dir_all(&loose).expect("create artifact");
        keep_file(&fixture, &format!("{}\tB1\t2999-01-01\n", loose.display()));

        let artifacts = vec![artifact(&loose, "node_modules")];
        let report = audit(&fixture, &[], &artifacts, &options(), IGNORED);

        assert_eq!(level_of(&report, HygieneRule::B1), HygieneLevel::Ok);
        let waived = report
            .findings
            .iter()
            .find(|finding| finding.rule == HygieneRule::B1)
            .expect("the finding is kept, with its expiry");
        assert_eq!(waived.waived_until.as_deref(), Some("2999-01-01"));
    }

    #[test]
    fn a_waiver_covers_only_the_rules_it_names() {
        let fixture = fixture();
        let loose = fixture.repo.join("node_modules");
        std::fs::create_dir_all(&loose).expect("create artifact");
        keep_file(&fixture, &format!("{}\tB5\t2999-01-01\n", loose.display()));

        let artifacts = vec![artifact(&loose, "node_modules")];
        let report = audit(&fixture, &[], &artifacts, &options(), IGNORED);
        assert_eq!(level_of(&report, HygieneRule::B1), HygieneLevel::Violation);
    }

    // ---- the rules a scan cannot see ----

    #[test]
    fn rules_outside_a_repository_scan_are_unchecked_not_ok() {
        let fixture = fixture();
        let report = audit(&fixture, &[], &[], &options(), IGNORED);
        for rule in [
            HygieneRule::L2,
            HygieneRule::L6,
            HygieneRule::L7,
            HygieneRule::T5,
            HygieneRule::B3,
            HygieneRule::B4,
            HygieneRule::Q3,
        ] {
            assert_eq!(level_of(&report, rule), HygieneLevel::Unknown, "{rule:?}");
            assert_eq!(states(&report, rule), ["not_checkable"], "{rule:?}");
        }
    }

    // ---- the report itself ----

    #[test]
    fn every_rule_has_a_status_in_policy_order() {
        let fixture = fixture();
        let report = audit(&fixture, &[], &[], &options(), IGNORED);
        let rules: Vec<HygieneRule> = report.rules.iter().map(|status| status.rule).collect();
        assert_eq!(rules, HygieneRule::ALL.to_vec());
    }

    #[test]
    fn violations_come_before_advisories() {
        let fixture = fixture();
        let loose = artifact(&fixture.repo.join("node_modules"), "node_modules");
        let mut merged = worktree(
            &fixture.repo.join(".worktree/repo-feature-20260101"),
            "feature",
        );
        merged.merged_into_default = Some(true);
        std::fs::create_dir_all(&merged.path).expect("create worktree");
        let report = audit(
            &fixture,
            &[main_record(&fixture.repo), merged],
            &[loose],
            &options(),
            IGNORED,
        );
        let levels: Vec<HygieneLevel> = report
            .findings
            .iter()
            .map(|finding| finding.level)
            .collect();
        let first_advisory = levels
            .iter()
            .position(|level| *level == HygieneLevel::Advisory)
            .unwrap_or(levels.len());
        let last_violation = levels
            .iter()
            .rposition(|level| *level == HygieneLevel::Violation)
            .unwrap_or(0);
        assert!(last_violation < first_advisory, "{levels:?}");
    }

    #[test]
    fn every_finding_carries_a_subject_and_evidence() {
        let fixture = fixture();
        let loose = artifact(&fixture.repo.join("node_modules"), "node_modules");
        let report = audit(&fixture, &[], &[loose], &options(), IGNORED);
        assert!(!report.findings.is_empty());
        for finding in &report.findings {
            assert!(!finding.subject.trim().is_empty(), "{finding:?}");
            assert!(!finding.evidence.trim().is_empty(), "{finding:?}");
            assert!(
                matches!(finding.args.get("state"), Some(DetailArg::Text(_))),
                "every finding names its situation: {finding:?}"
            );
        }
    }

    #[test]
    fn a_failed_git_read_is_a_warning_not_a_finding() {
        // No canned answer at all: both check-ignore calls fail, and the audit must
        // say so rather than claim the paths are unignored.
        let fixture = fixture();
        let runner = FakeGitRunner::default();
        let report = audit_hygiene(&runner, &fixture.repo, &[], &[], &options());
        assert!(report
            .warnings
            .iter()
            .any(|line| line.contains("check-ignore")));
        assert_eq!(level_of(&report, HygieneRule::L5), HygieneLevel::Ok);
    }
}
