//! Worktree checks (product plan section 5.1).
//!
//! The decision table is the specification: every branch below maps to exactly one
//! `wt.*` reason code, and the level of the record is **derived** from the reasons
//! (D2) — this module never scores a level on its own. Every failure path adds
//! `wt.cannot_verify` with level `Review` and is never allowed to look like a
//! positive result (D3).
//!
//! Data sources are read-only Git commands plus two filesystem facts (the directory
//! mtime and the presence of `.git/index.lock`); the `.git` file format is never
//! parsed (development contract section 2).

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::git::{display_command, GitError, GitOutput, GitRunner};
use crate::path::whitelist_hit;
use crate::reason::{Reason, ReasonCode, SafetyAssessment, SafetyStatus};
use crate::timefmt::{elapsed_days, format_rfc3339, from_unix_seconds, humanize_days};
use crate::WorktreeRecord;

/// One entry of `git worktree list --porcelain`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeEntry {
    pub path: PathBuf,
    /// Full HEAD SHA as reported by porcelain; `None` for a bare repository or an
    /// unborn branch.
    pub head: Option<String>,
    /// Branch name without the `refs/heads/` prefix; `None` when detached or bare.
    pub branch: Option<String>,
    pub is_bare: bool,
    pub is_detached: bool,
    pub is_locked: bool,
    /// Lock reason, when porcelain carries one.
    pub lock_reason: Option<String>,
    /// Porcelain marks the entry prunable: Git metadata exists, the directory is gone.
    pub is_prunable: bool,
    /// Porcelain's own explanation for a prunable entry, kept as evidence.
    pub prunable_reason: Option<String>,
}

/// Parses `git worktree list --porcelain -z`.
///
/// `-z` is mandatory, not an optimisation: without it a path that contains a
/// newline is emitted raw, and a line-based parser silently truncates the path at
/// the newline — the remainder looks like an unknown attribute. The truncated path
/// then fails the existence check, and the entry is routed to the prune-only
/// channel as if its directory were gone (D3: a parsing failure must never read
/// as a safe fact). With `-z` every attribute is NUL-terminated, so any path
/// survives intact.
///
/// A new `worktree` attribute starts the next entry; the **first** entry is the
/// main worktree. Unknown attributes are ignored on purpose: porcelain gains
/// fields over time, and an unknown attribute must never fail a scan.
pub fn parse_worktree_list(porcelain_z: &str) -> Vec<WorktreeEntry> {
    let mut entries: Vec<WorktreeEntry> = Vec::new();
    let mut current: Option<WorktreeEntry> = None;

    for attribute in porcelain_z.split('\0') {
        if let Some(path) = attribute.strip_prefix("worktree ") {
            if let Some(entry) = current.take() {
                entries.push(entry);
            }
            current = Some(WorktreeEntry {
                path: PathBuf::from(path),
                head: None,
                branch: None,
                is_bare: false,
                is_detached: false,
                is_locked: false,
                lock_reason: None,
                is_prunable: false,
                prunable_reason: None,
            });
            continue;
        }
        let Some(entry) = current.as_mut() else {
            continue;
        };
        if let Some(sha) = attribute.strip_prefix("HEAD ") {
            entry.head = non_empty(sha);
        } else if let Some(reference) = attribute.strip_prefix("branch ") {
            entry.branch = Some(short_branch(reference.trim()));
        } else if attribute == "detached" {
            entry.is_detached = true;
        } else if attribute == "bare" {
            entry.is_bare = true;
        } else if attribute == "locked" {
            entry.is_locked = true;
        } else if let Some(reason) = attribute.strip_prefix("locked ") {
            entry.is_locked = true;
            entry.lock_reason = non_empty(reason);
        } else if attribute == "prunable" {
            entry.is_prunable = true;
        } else if let Some(reason) = attribute.strip_prefix("prunable ") {
            entry.is_prunable = true;
            entry.prunable_reason = non_empty(reason);
        }
    }
    if let Some(entry) = current.take() {
        entries.push(entry);
    }
    entries
}

/// Summary of `git status --porcelain=v2 --branch`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StatusInfo {
    /// Branch name without the `refs/heads/` prefix; `None` when detached.
    pub branch: Option<String>,
    /// HEAD SHA as reported by the header; `None` for an unborn branch.
    pub head: Option<String>,
    /// Upstream branch, when the header reports one.
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    /// Tracked files with uncommitted changes (ordinary, rename/copy, unmerged).
    pub dirty_files: u32,
    /// Untracked, non-ignored files. An untracked **directory** is folded into
    /// one entry by git, so this is a lower bound; the exact count comes from
    /// `ls-files --others` when that call succeeds.
    pub untracked_files: u32,
    /// Whether any `# branch.*` header line was seen. Without the header the
    /// counts above are not "zero" — they are unknown (D3).
    pub header_seen: bool,
    /// `# branch.upstream` was present but `# branch.ab` was not: git omits the
    /// ahead/behind line when the upstream ref itself is gone, so the counts are
    /// unknown rather than zero.
    pub upstream_without_ab: bool,
}

/// Parses `git status --porcelain=v2 --branch`.
///
/// Counting rules follow the porcelain v2 format: `1 ` / `2 ` / `u ` lines are
/// changes, `? ` lines are untracked files, and `! ` (ignored) lines are not counted
/// at all.
pub fn parse_status(porcelain_v2: &str) -> StatusInfo {
    let mut info = StatusInfo::default();
    let mut ab_seen = false;
    for line in porcelain_v2.lines() {
        if let Some(rest) = line.strip_prefix("# branch.oid ") {
            info.header_seen = true;
            let oid = rest.trim();
            // An unborn branch reports `(initial)`, which is not a SHA.
            info.head = if oid == "(initial)" {
                None
            } else {
                Some(oid.to_string())
            };
        } else if let Some(rest) = line.strip_prefix("# branch.head ") {
            info.header_seen = true;
            let head = rest.trim();
            info.branch = if head == "(detached)" {
                None
            } else {
                Some(short_branch(head))
            };
        } else if let Some(rest) = line.strip_prefix("# branch.upstream ") {
            info.header_seen = true;
            info.upstream = Some(short_branch(rest.trim()));
        } else if let Some(rest) = line.strip_prefix("# branch.ab ") {
            info.header_seen = true;
            ab_seen = true;
            for token in rest.split_whitespace() {
                if let Some(value) = token.strip_prefix('+') {
                    if let Ok(ahead) = value.parse() {
                        info.ahead = ahead;
                    }
                } else if let Some(value) = token.strip_prefix('-') {
                    if let Ok(behind) = value.parse() {
                        info.behind = behind;
                    }
                }
            }
        } else if line.starts_with("1 ") || line.starts_with("2 ") || line.starts_with("u ") {
            info.dirty_files += 1;
        } else if line.starts_with("? ") {
            info.untracked_files += 1;
        }
    }
    info.upstream_without_ab = info.upstream.is_some() && !ab_seen;
    info
}

/// Injection points for [`check_worktree`].
///
/// Everything that depends on the clock, on a measurement, or on user policy is
/// passed in, so the decision logic is a pure function of Git output and can be
/// tested without a repository (D2/D3).
#[derive(Debug, Clone)]
pub struct WorktreeCheckOptions {
    /// Default branch name (e.g. `main`). `None` = undetermined, which forces
    /// `merged_into_default = None` plus a `wt.cannot_verify` reason: an unknown
    /// merge status must never be read as merged.
    pub default_branch: Option<String>,
    /// Activity window in days (settings.active_days).
    pub recent_activity_days: u32,
    /// Treat untracked files as `Unsafe` instead of `Review`
    /// (settings.untracked_is_unsafe).
    pub untracked_as_unsafe: bool,
    /// Whitelist rules, already expanded to absolute paths by the caller.
    pub whitelist: Vec<PathBuf>,
    /// Injected clock, so "recent activity" is testable.
    pub now: SystemTime,
    /// Worktree directory size measured by the caller; `None` = unmeasurable, which
    /// must stay distinguishable from `0` (development contract section 3).
    pub size_bytes: Option<u64>,
    // There is deliberately no per-command timeout here: the timeout belongs to
    // the [`crate::git::GitRunner`], which owns the processes. The orchestrator
    // passes the setting into `ProcessGitRunner::new`, so one knob, one owner.
}

impl Default for WorktreeCheckOptions {
    fn default() -> Self {
        Self {
            default_branch: None,
            recent_activity_days: 7,
            untracked_as_unsafe: false,
            whitelist: Vec::new(),
            now: SystemTime::now(),
            size_bytes: None,
        }
    }
}

/// Best-effort default branch detection, run once per scan.
///
/// Order: the remote's `origin/HEAD` (authoritative when it exists), then a local
/// `refs/heads/main`, then `refs/heads/master`. `None` means undetermined, and the
/// worktree checks must then treat the merge status as unknown rather than merged
/// (product plan section 5.3). The repository's current branch is deliberately not
/// used as a fallback: that would be a guess.
pub fn detect_default_branch(runner: &dyn GitRunner, repo: &Path) -> Option<String> {
    if let Ok(output) = runner.run(
        repo,
        &["symbolic-ref", "--quiet", "refs/remotes/origin/HEAD"],
    ) {
        if output.success() {
            let reference = output.stdout.trim();
            if let Some(name) = reference.strip_prefix("refs/remotes/origin/") {
                if !name.is_empty() {
                    return Some(name.to_string());
                }
            }
        }
    }
    for candidate in ["refs/heads/main", "refs/heads/master"] {
        if let Ok(output) = runner.run(repo, &["rev-parse", "--verify", "--quiet", candidate]) {
            if output.success() {
                return Some(
                    candidate
                        .strip_prefix("refs/heads/")
                        .unwrap_or(candidate)
                        .to_string(),
                );
            }
        }
    }
    None
}

/// Agents recognised by a path token (product plan section 4).
///
/// The worktree directory name is the strongest signal (`repo--codex-fix-ble`), so
/// every path component is split into tokens and each token is compared
/// case-insensitively. An unrecognised path stays `None`: the label is inferred from
/// the path only and is never guessed.
const AGENT_MARKERS: &[&str] = &["codex", "claude", "cursor", "aider", "workbuddy"];

/// Infers the owning agent from the path, or `None` when no marker is present.
pub fn infer_agent(path: &Path) -> Option<String> {
    let components: Vec<String> = path
        .components()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .collect();
    // Deepest component first: the worktree directory name outranks its parents.
    for name in components.iter().rev() {
        for token in name.split(|ch: char| !ch.is_alphanumeric()) {
            for marker in AGENT_MARKERS {
                if token.eq_ignore_ascii_case(marker) {
                    return Some((*marker).to_string());
                }
            }
        }
    }
    None
}

/// The `--force` confirmation page lists untracked files line by line; the list is
/// capped so a pathological repository cannot balloon `snapshot.json`. The count in
/// `untracked_files` comes from `git status` and stays exact.
const MAX_UNTRACKED_LIST: usize = 200;

/// Filesystem state of a worktree directory.
///
/// The distinction matters: `Path::exists()` reports `false` for a permission
/// error just as it does for a missing path, and treating "cannot stat" as "gone"
/// would route an unreadable worktree to the prune-only channel with a Safe
/// verdict (D3). Only [`DirectoryState::Gone`] may skip the per-directory checks.
enum DirectoryState {
    Present,
    Gone,
    Unreadable(std::io::Error),
}

fn directory_state(path: &Path) -> DirectoryState {
    match std::fs::metadata(path) {
        Ok(_) => DirectoryState::Present,
        // Only "not found" is evidence of absence; everything else is unknown.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => DirectoryState::Gone,
        Err(error) => DirectoryState::Unreadable(error),
    }
}

/// Runs the worktree checks for one entry and assembles its [`WorktreeRecord`].
///
/// `repo` is any existing directory of the same repository (the main worktree in
/// practice): the merge check runs there so it still works when this worktree's
/// directory is gone. `is_main` marks the first entry of `git worktree list`.
pub fn check_worktree(
    runner: &dyn GitRunner,
    repo: &Path,
    entry: &WorktreeEntry,
    is_main: bool,
    opts: &WorktreeCheckOptions,
) -> WorktreeRecord {
    let mut reasons: Vec<Reason> = Vec::new();
    let path_text = entry.path.display().to_string();
    let directory = directory_state(&entry.path);
    // The directory must be verifiably gone before the entry may take the
    // prune-only channel; a porcelain `prunable` flag alone is not enough (the
    // directory may have come back), and a stat error is never enough.
    let directory_gone = matches!(directory, DirectoryState::Gone);
    let is_prunable = entry.is_prunable && directory_gone;
    if let DirectoryState::Unreadable(error) = &directory {
        reasons.push(cannot_verify_io("the worktree directory", error));
    }

    // ---- porcelain-only reasons: no command can change these ----
    if is_main {
        reasons.push(Reason::blocking(
            ReasonCode::WtMain,
            "this is the main worktree",
            "first entry of git worktree list",
        ));
    }
    if entry.is_locked {
        let evidence = match entry.lock_reason.as_deref() {
            Some(reason) => format!("porcelain marks it as locked: {reason}"),
            None => "porcelain marks it as locked".to_string(),
        };
        reasons.push(Reason::blocking(
            ReasonCode::WtLocked,
            "locked by Git",
            evidence,
        ));
    }
    if let Some(rule) = whitelist_hit(&entry.path, &opts.whitelist) {
        reasons.push(Reason::blocking(
            ReasonCode::WtWhitelisted,
            "matches a whitelist rule",
            format!("matches whitelist: {}", rule.display()),
        ));
    }

    // ---- per-worktree checks: impossible when the directory is gone ----
    let mut status: Option<StatusInfo> = None;
    let mut stash_count: u32 = 0;
    let mut untracked_file_list: Vec<String> = Vec::new();
    // Exact untracked file count from `ls-files --others`, when that call
    // succeeded. `None` = not measured; the folded status count is then used.
    let mut untracked_listed: Option<u32> = None;
    let mut activity: Option<SystemTime> = None;

    if directory_gone {
        reasons.push(Reason::safe(
            ReasonCode::WtPrunable,
            "the working directory no longer exists",
            prunable_evidence(entry),
        ));
    } else {
        if let Some(output) = run_checked(
            runner,
            &entry.path,
            "the worktree status",
            &["status", "--porcelain=v2", "--branch"],
            &mut reasons,
        ) {
            let info = parse_status(&output.stdout);
            if !info.header_seen {
                // A status without the `# branch.*` header cannot be read as
                // "zero changes": the counts would be a guess (D3).
                reasons.push(Reason::review(
                    ReasonCode::WtCannotVerify,
                    "cannot read the worktree status",
                    "the status header (# branch.oid / # branch.head) was missing, so the file counts are unknown",
                ));
            } else if info.upstream_without_ab {
                // Git omits `# branch.ab` when the upstream ref itself is gone;
                // ahead/behind are then unknown, not zero.
                reasons.push(Reason::review(
                    ReasonCode::WtCannotVerify,
                    "cannot read the worktree status",
                    "the upstream branch no longer exists, so the ahead/behind counts are unknown",
                ));
            } else {
                if info.dirty_files > 0 {
                    reasons.push(Reason::blocking(
                        ReasonCode::WtDirty,
                        "has uncommitted changes",
                        format!("{} files have uncommitted changes", info.dirty_files),
                    ));
                }
                if info.ahead > 0 {
                    reasons.push(Reason::blocking(
                        ReasonCode::WtUnpushed,
                        "has commits that were never pushed",
                        format!("{} commits not pushed", info.ahead),
                    ));
                }
                if info.dirty_files == 0 && info.untracked_files == 0 {
                    reasons.push(Reason::safe(
                        ReasonCode::WtClean,
                        "the working tree is clean",
                        "no uncommitted changes, no untracked files",
                    ));
                }
                status = Some(info);
            }
        }

        if let Some(output) = run_checked(
            runner,
            &entry.path,
            "the stash list",
            &["stash", "list", "--format=%gd"],
            &mut reasons,
        ) {
            stash_count = output
                .stdout
                .lines()
                .filter(|line| !line.is_empty())
                .count() as u32;
            if stash_count > 0 {
                reasons.push(Reason::review(
                    ReasonCode::WtStash,
                    "has stash entries",
                    format!("{stash_count} stash entries"),
                ));
            }
        }

        if status.as_ref().is_some_and(|info| info.untracked_files > 0) {
            if let Some(output) = run_checked(
                runner,
                &entry.path,
                "the untracked file list",
                &["ls-files", "--others", "--exclude-standard"],
                &mut reasons,
            ) {
                // `ls-files --others` lists individual files, so this is the exact
                // count where `git status` folds a whole directory into one entry.
                let listed: Vec<&str> = output
                    .stdout
                    .lines()
                    .filter(|line| !line.is_empty())
                    .collect();
                untracked_listed = Some(listed.len() as u32);
                untracked_file_list = listed
                    .into_iter()
                    .take(MAX_UNTRACKED_LIST)
                    .map(str::to_string)
                    .collect();
            }
        }

        // The untracked reason is emitted here, after the exact count is known.
        if let Some(info) = &status {
            if info.untracked_files > 0 {
                let severity = if opts.untracked_as_unsafe {
                    SafetyStatus::Unsafe
                } else {
                    SafetyStatus::Review
                };
                let count = untracked_listed.unwrap_or(info.untracked_files);
                let mut evidence = format!("{count} untracked files");
                if untracked_listed.is_some_and(|listed| listed > MAX_UNTRACKED_LIST as u32) {
                    evidence.push_str(&format!(
                        " (the list below shows the first {MAX_UNTRACKED_LIST})"
                    ));
                }
                reasons.push(Reason::new(
                    ReasonCode::WtUntracked,
                    severity,
                    "has untracked files",
                    evidence,
                ));
            }
        }

        // Last activity = the later of the HEAD commit time and the directory mtime.
        let commit_time = run_checked(
            runner,
            &entry.path,
            "the HEAD commit time",
            &["log", "-1", "--format=%ct"],
            &mut reasons,
        )
        .and_then(|output| match output.stdout.trim().parse::<i128>() {
            Ok(seconds) => Some(from_unix_seconds(seconds)),
            Err(_) => {
                reasons.push(Reason::review(
                    ReasonCode::WtCannotVerify,
                    "cannot read the HEAD commit time",
                    format!(
                        "git reported a non-numeric commit time: {:?}",
                        output.stdout.trim()
                    ),
                ));
                None
            }
        });
        let directory_mtime = match std::fs::metadata(&entry.path).and_then(|meta| meta.modified())
        {
            Ok(time) => Some(time),
            // Gone cannot happen here (the branch above caught it); any other
            // error leaves the activity unknown rather than silently absent.
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => {
                reasons.push(cannot_verify_io("the directory modification time", &error));
                None
            }
        };
        activity = later_of(commit_time, directory_mtime);

        // A Git command holding `index.lock` may be running right now.
        // `--git-path` resolves the per-worktree gitdir, so a linked worktree's lock
        // under `<main>/.git/worktrees/<name>/` is found as well.
        if let Some(output) = run_checked(
            runner,
            &entry.path,
            "the index lock path",
            &["rev-parse", "--git-path", "index.lock"],
            &mut reasons,
        ) {
            let reported = output.stdout.trim();
            if !reported.is_empty() {
                let lock_path = absolutize(&entry.path, Path::new(reported));
                if lock_path.exists() {
                    reasons.push(Reason::review(
                        ReasonCode::WtAgentActive,
                        "a Git command may be running",
                        format!("index.lock present: {}", lock_path.display()),
                    ));
                }
            }
        }
    }

    if let Some(activity) = activity {
        let age_days = elapsed_days(opts.now, activity);
        if age_days < u64::from(opts.recent_activity_days) {
            reasons.push(Reason::review(
                ReasonCode::WtRecentActivity,
                "changed within the activity window",
                format!("last activity: {}", humanize_days(age_days)),
            ));
        }
    }

    // ---- merge status: runs in `repo`, so it survives a missing directory ----
    let merged = match (opts.default_branch.as_deref(), entry.head.as_deref()) {
        (Some(default_branch), Some(head)) => {
            match merge_base_is_ancestor(runner, repo, head, default_branch) {
                Ok((true, evidence)) => {
                    reasons.push(Reason::safe(
                        ReasonCode::WtMerged,
                        "merged into the default branch",
                        evidence,
                    ));
                    Some(true)
                }
                Ok((false, evidence)) => {
                    reasons.push(Reason::review(
                        ReasonCode::WtUnmerged,
                        "not merged into the default branch",
                        evidence,
                    ));
                    Some(false)
                }
                // Anything that is neither exit 0 nor exit 1 answers nothing and
                // must never be read as "merged" (D3).
                Err(error) => {
                    reasons.push(cannot_verify("the merge status", &error));
                    None
                }
            }
        }
        (None, _) => {
            reasons.push(Reason::review(
                ReasonCode::WtCannotVerify,
                "cannot read the merge status",
                "the default branch could not be determined, so the merge status is unknown",
            ));
            None
        }
        (_, None) => {
            reasons.push(Reason::review(
                ReasonCode::WtCannotVerify,
                "cannot read the merge status",
                "porcelain reported no HEAD for this worktree",
            ));
            None
        }
    };

    // ---- reasons that need both the status header and the merge answer ----
    if let Some(info) = &status {
        if info.upstream.is_none() && merged != Some(true) {
            let evidence = match merged {
                Some(false) => format!(
                    "no upstream branch and not merged into {}",
                    opts.default_branch
                        .as_deref()
                        .unwrap_or("the default branch")
                ),
                _ => "no upstream branch and the merge status is unknown".to_string(),
            };
            reasons.push(Reason::review(
                ReasonCode::WtNoUpstream,
                "has no upstream branch and is not merged",
                evidence,
            ));
        }
        // A detached HEAD only reaches this check when status ran: without the
        // header the "no upstream" half of the condition cannot be verified.
        if entry.is_detached && info.upstream.is_none() {
            reasons.push(Reason::review(
                ReasonCode::WtDetached,
                "HEAD is detached",
                "porcelain reports a detached HEAD and status reports no upstream",
            ));
        }
    }

    let safety = SafetyAssessment::from_reasons(reasons);
    WorktreeRecord {
        path: path_text,
        branch: entry.branch.clone(),
        head: entry.head.as_deref().map(short_sha),
        is_main,
        is_detached: entry.is_detached
            || status
                .as_ref()
                .is_some_and(|info| info.branch.is_none() && !entry.is_bare),
        is_locked: entry.is_locked,
        is_prunable,
        dirty_files: status.as_ref().map_or(0, |info| info.dirty_files),
        untracked_files: untracked_listed
            .or_else(|| status.as_ref().map(|info| info.untracked_files))
            .unwrap_or(0),
        untracked_file_list,
        ahead: status.as_ref().map_or(0, |info| info.ahead),
        behind: status.as_ref().map_or(0, |info| info.behind),
        has_upstream: status.as_ref().is_some_and(|info| info.upstream.is_some()),
        merged_into_default: merged,
        stash_count,
        last_updated_at: activity.map(format_rfc3339),
        agent: infer_agent(&entry.path),
        size_bytes: opts.size_bytes,
        safety,
    }
}

/// Asks `git merge-base --is-ancestor` whether `head` is an ancestor of
/// `default_branch`.
///
/// Two candidate names are tried, in order: the branch as detected, then
/// `origin/<name>`. The fallback exists because `origin/HEAD` names a branch that
/// often exists only as a remote-tracking ref — `git merge-base` does not resolve
/// the bare name to it (verified against git 2.54), which would degrade every
/// healthy worktree to `cannot_verify`. `--` keeps a branch name that starts with
/// `-` from being parsed as an option. The exit code is the answer: 0 = ancestor,
/// 1 = not an ancestor; every other outcome is an error the caller must treat as
/// unknown (D3). A transport error (timeout, spawn failure) is returned as-is:
/// retrying another name cannot fix it.
fn merge_base_is_ancestor(
    runner: &dyn GitRunner,
    repo: &Path,
    head: &str,
    default_branch: &str,
) -> Result<(bool, String), GitError> {
    let mut candidates = vec![default_branch.to_string()];
    if !default_branch.contains('/') {
        candidates.push(format!("origin/{default_branch}"));
    }
    let mut last_error: Option<GitError> = None;
    for candidate in &candidates {
        let args = [
            "merge-base",
            "--is-ancestor",
            "--",
            head,
            candidate.as_str(),
        ];
        match runner.run(repo, &args) {
            Ok(output) if output.code == Some(0) => {
                return Ok((
                    true,
                    format!("merge-base reports {head} is an ancestor of {candidate}"),
                ));
            }
            Ok(output) if output.code == Some(1) => {
                return Ok((
                    false,
                    format!("merge-base reports {head} is not an ancestor of {candidate}"),
                ));
            }
            Ok(output) => {
                last_error = Some(GitError::Failed {
                    command: display_command(repo, &args),
                    code: output.code,
                    stderr: output.stderr.clone(),
                });
            }
            Err(error) => return Err(error),
        }
    }
    Err(last_error.unwrap_or_else(|| GitError::Failed {
        command: display_command(repo, &["merge-base", "--is-ancestor", "--", head]),
        code: None,
        stderr: "no merge-base candidate resolved".to_string(),
    }))
}

/// Runs one Git call and maps **every** failure onto a `wt.cannot_verify` reason
/// (D3). Returns `None` when the call failed; the caller must then treat the datum as
/// unknown and must not emit a positive reason for it.
fn run_checked(
    runner: &dyn GitRunner,
    repo: &Path,
    label: &str,
    args: &[&str],
    reasons: &mut Vec<Reason>,
) -> Option<GitOutput> {
    match runner.run(repo, args) {
        Ok(output) if output.success() => Some(output),
        Ok(output) => {
            let error = GitError::Failed {
                command: display_command(repo, args),
                code: output.code,
                stderr: output.stderr.clone(),
            };
            reasons.push(cannot_verify(label, &error));
            None
        }
        Err(error) => {
            reasons.push(cannot_verify(label, &error));
            None
        }
    }
}

/// The single shape of a failed check: `Review` level, never `Safe` (D3).
fn cannot_verify(label: &str, error: &GitError) -> Reason {
    Reason::review(
        ReasonCode::WtCannotVerify,
        format!("cannot read {label}"),
        error.to_string(),
    )
}

/// The same shape for a filesystem failure (a stat or mtime read), which is not a
/// Git error but must fail closed just the same (D3).
fn cannot_verify_io(label: &str, error: &std::io::Error) -> Reason {
    Reason::review(
        ReasonCode::WtCannotVerify,
        format!("cannot read {label}"),
        error.to_string(),
    )
}

fn prunable_evidence(entry: &WorktreeEntry) -> String {
    match entry.prunable_reason.as_deref() {
        Some(reason) => format!("only cleans .git/worktrees metadata ({reason})"),
        None if entry.is_prunable => "only cleans .git/worktrees metadata".to_string(),
        // The directory is gone but porcelain has not marked the entry yet.
        None => "the directory does not exist, so git worktree prune will clean the metadata"
            .to_string(),
    }
}

/// `git rev-parse --git-path` answers with a relative path inside the main worktree
/// and an absolute one inside a linked worktree; resolve the former against the
/// worktree so the existence check looks in the right place.
fn absolutize(base: &Path, reported: &Path) -> PathBuf {
    if reported.is_absolute() {
        reported.to_path_buf()
    } else {
        base.join(reported)
    }
}

fn later_of(first: Option<SystemTime>, second: Option<SystemTime>) -> Option<SystemTime> {
    match (first, second) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (Some(a), None) => Some(a),
        (None, b) => b,
    }
}

/// Porcelain reports full SHAs; the record and the UI show the short form.
fn short_sha(sha: &str) -> String {
    sha.chars().take(7).collect()
}

/// `refs/heads/feature/x` to `feature/x`; anything without the prefix is returned
/// unchanged (the status header already uses the short form).
fn short_branch(reference: &str) -> String {
    reference
        .strip_prefix("refs/heads/")
        .unwrap_or(reference)
        .to_string()
}

fn non_empty(text: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::testing::{Canned, FakeGitRunner};
    use std::time::{Duration, UNIX_EPOCH};

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

    fn failing(args: &[&str], code: Option<i32>, stderr: &str) -> (Vec<String>, Canned) {
        (
            args.iter().map(|arg| (*arg).to_string()).collect(),
            Canned::Failed {
                code,
                stderr: stderr.to_string(),
            },
        )
    }

    /// Porcelain output captured from a real repository (git 2.54, `--porcelain -z`):
    /// a main worktree, a detached one, a locked one, one whose path contains a
    /// newline, and a prunable one.
    ///
    /// Every attribute is NUL-terminated and entries are separated by one extra
    /// NUL (verified with `od -c` against git 2.54). The newline inside the fourth
    /// path is emitted raw: a line-based parser would truncate the path there and
    /// misread the entry as gone, which is exactly why `-z` is mandatory.
    const PORCELAIN: &str = "\
worktree /private/tmp/wt-fixture\0HEAD 19402c62a2bdb19e2271df95ecfc1a272aad4bf6\0branch refs/heads/main\0\0\
worktree /private/tmp/wt-fixture-detached\0HEAD 19402c62a2bdb19e2271df95ecfc1a272aad4bf6\0detached\0\0\
worktree /private/tmp/wt-fixture-feature\0HEAD 19402c62a2bdb19e2271df95ecfc1a272aad4bf6\0branch refs/heads/feature/x\0locked holding for review\0\0\
worktree /private/tmp/wt-fixture/new\nline\0HEAD 19402c62a2bdb19e2271df95ecfc1a272aad4bf6\0branch refs/heads/newline-branch\0\0\
worktree /private/tmp/wt-fixture-gone\0HEAD 19402c62a2bdb19e2271df95ecfc1a272aad4bf6\0branch refs/heads/gone-branch\0prunable gitdir file points to non-existent location\0";

    const CLEAN_STATUS: &str = "\
# branch.oid 3704a98b0e299445f52e13555002a5ffe7d13814
# branch.head feature/x
";

    const DIRTY_STATUS: &str = "\
# branch.oid 3704a98b0e299445f52e13555002a5ffe7d13814
# branch.head feature/x
1 .M N... 100644 100644 100644 814f4a422927b82f5f8a43f8fab6d3839e3983f2 814f4a422927b82f5f8a43f8fab6d3839e3983f2 a.txt
? notes.md
";

    const AHEAD_STATUS: &str = "\
# branch.oid 3704a98b0e299445f52e13555002a5ffe7d13814
# branch.head feature/x
# branch.upstream origin/feature/x
# branch.ab +2 -1
";

    /// Dirty, untracked and unpushed at once, with an upstream: the shape that
    /// produces the most reasons from the status check alone.
    const MESSY_STATUS: &str = "\
# branch.oid 3704a98b0e299445f52e13555002a5ffe7d13814
# branch.head feature/x
# branch.upstream origin/feature/x
# branch.ab +2 -1
1 .M N... 100644 100644 100644 814f4a422927b82f5f8a43f8fab6d3839e3983f2 814f4a422927b82f5f8a43f8fab6d3839e3983f2 a.txt
? notes.md
";

    /// An upstream that no longer exists: git prints `# branch.upstream` but omits
    /// `# branch.ab` entirely (verified against git 2.54), so ahead/behind are
    /// unknown rather than zero.
    const UPSTREAM_WITHOUT_AB: &str = "\
# branch.oid 3704a98b0e299445f52e13555002a5ffe7d13814
# branch.head feature/x
# branch.upstream origin/feature/x
";

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "worktree-cleaner-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    /// Backdates a directory's mtime so "recent activity" can be tested.
    ///
    /// A read-only descriptor is enough: `futimens` needs the file to exist, not to be
    /// opened for writing (and a directory cannot be).
    fn backdate(path: &Path, seconds_ago: u64) {
        let target = SystemTime::now() - Duration::from_secs(seconds_ago);
        let file = std::fs::File::open(path).expect("open dir for set_times");
        let times = std::fs::FileTimes::new()
            .set_accessed(target)
            .set_modified(target);
        file.set_times(times).expect("set directory mtime");
    }

    fn feature_entry(dir: &Path) -> WorktreeEntry {
        WorktreeEntry {
            path: dir.to_path_buf(),
            head: Some("19402c62a2bdb19e2271df95ecfc1a272aad4bf6".to_string()),
            branch: Some("feature/x".to_string()),
            is_bare: false,
            is_detached: false,
            is_locked: false,
            lock_reason: None,
            is_prunable: false,
            prunable_reason: None,
        }
    }

    fn default_options() -> WorktreeCheckOptions {
        WorktreeCheckOptions {
            default_branch: Some("main".to_string()),
            now: UNIX_EPOCH + Duration::from_secs(1_800_000_000),
            size_bytes: Some(1_234),
            ..WorktreeCheckOptions::default()
        }
    }

    /// The canned answers every check needs for a clean, merged, inactive worktree.
    fn calm_rules(status: &str) -> Vec<(Vec<String>, Canned)> {
        vec![
            canned(&["status", "--porcelain=v2", "--branch"], status),
            canned(&["stash", "list", "--format=%gd"], ""),
            canned(&["log", "-1", "--format=%ct"], "1700000000\n"),
            canned(
                &["rev-parse", "--git-path", "index.lock"],
                "/nonexistent/.git/index.lock\n",
            ),
            (
                vec![
                    "merge-base".to_string(),
                    "--is-ancestor".to_string(),
                    "--".to_string(),
                    "19402c62a2bdb19e2271df95ecfc1a272aad4bf6".to_string(),
                    "main".to_string(),
                ],
                Canned::Out(GitOutput {
                    stdout: String::new(),
                    stderr: String::new(),
                    code: Some(0),
                }),
            ),
        ]
    }

    fn codes(record: &WorktreeRecord) -> Vec<&'static str> {
        record
            .safety
            .reasons
            .iter()
            .map(|reason| reason.code.as_str())
            .collect()
    }

    fn has(record: &WorktreeRecord, code: ReasonCode) -> bool {
        record
            .safety
            .reasons
            .iter()
            .any(|reason| reason.code == code)
    }

    // ---- parsing -----------------------------------------------------------

    #[test]
    fn parses_real_porcelain_output() {
        let entries = parse_worktree_list(PORCELAIN);
        assert_eq!(entries.len(), 5);
        assert_eq!(entries[0].path, Path::new("/private/tmp/wt-fixture"));
        assert_eq!(entries[0].branch.as_deref(), Some("main"));
        assert_eq!(
            entries[0].head.as_deref(),
            Some("19402c62a2bdb19e2271df95ecfc1a272aad4bf6")
        );
        assert!(!entries[0].is_locked);
        assert!(!entries[0].is_prunable);

        assert!(entries[1].is_detached);
        assert!(entries[1].branch.is_none());

        assert!(entries[2].is_locked);
        assert_eq!(
            entries[2].lock_reason.as_deref(),
            Some("holding for review")
        );
        assert_eq!(entries[2].branch.as_deref(), Some("feature/x"));

        // A path containing a newline survives intact under `-z`; a line-based
        // parser would truncate it to "/private/tmp/wt-fixture/new" and treat the
        // rest as garbage.
        assert_eq!(
            entries[3].path,
            Path::new("/private/tmp/wt-fixture/new\nline")
        );
        assert_eq!(entries[3].branch.as_deref(), Some("newline-branch"));

        assert!(entries[4].is_prunable);
        assert_eq!(
            entries[4].prunable_reason.as_deref(),
            Some("gitdir file points to non-existent location")
        );
    }

    #[test]
    fn porcelain_parsing_tolerates_unknown_attributes() {
        // Porcelain gains fields over time; an unknown attribute must never fail a
        // scan. The entry separator (an empty attribute) is ignored as well, and so
        // is any attribute printed before the first `worktree` line.
        let text = "brand-new-field whatever\0worktree /repo\0HEAD abc\0branch refs/heads/main\0brand-new-field whatever\0\0worktree /other\0HEAD def\0detached\0";
        let entries = parse_worktree_list(text);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].branch.as_deref(), Some("main"));
        assert!(entries[1].is_detached);
        assert_eq!(entries[1].path, Path::new("/other"));
    }

    #[test]
    fn a_newline_in_a_path_never_truncates_it() {
        // The regression this `-z` contract exists for: the line-based parser saw
        // "/repo/new" and reported the entry as a directory that no longer exists.
        let text = "worktree /repo/new\nline\0HEAD abc\0branch refs/heads/x\0";
        let entries = parse_worktree_list(text);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, Path::new("/repo/new\nline"));
        assert_eq!(entries[0].branch.as_deref(), Some("x"));
    }

    #[test]
    fn empty_porcelain_yields_no_entries() {
        assert!(parse_worktree_list("").is_empty());
        assert!(parse_worktree_list("\n\n").is_empty());
    }

    #[test]
    fn parses_status_headers_and_counts() {
        let info = parse_status(DIRTY_STATUS);
        assert_eq!(info.branch.as_deref(), Some("feature/x"));
        assert_eq!(
            info.head.as_deref(),
            Some("3704a98b0e299445f52e13555002a5ffe7d13814")
        );
        assert!(info.upstream.is_none());
        assert_eq!(info.dirty_files, 1);
        assert_eq!(info.untracked_files, 1);

        let ahead = parse_status(AHEAD_STATUS);
        assert_eq!(ahead.upstream.as_deref(), Some("origin/feature/x"));
        assert_eq!(ahead.ahead, 2);
        assert_eq!(ahead.behind, 1);
        assert_eq!(ahead.dirty_files, 0);
    }

    #[test]
    fn status_parsing_handles_detached_and_unborn() {
        let detached = parse_status("# branch.oid abc\n# branch.head (detached)\n");
        assert!(detached.branch.is_none());

        let unborn = parse_status("# branch.oid (initial)\n# branch.head main\n");
        assert!(unborn.head.is_none());
        assert_eq!(unborn.branch.as_deref(), Some("main"));
    }

    #[test]
    fn ignored_files_are_not_counted() {
        let info = parse_status("! ignored.txt\n? real.txt\n");
        assert_eq!(info.dirty_files, 0);
        assert_eq!(info.untracked_files, 1);
    }

    // ---- agent inference ---------------------------------------------------

    #[test]
    fn agent_is_inferred_from_path_tokens() {
        assert_eq!(
            infer_agent(Path::new("/repo--codex-fix-ble")).as_deref(),
            Some("codex")
        );
        assert_eq!(
            infer_agent(Path::new("/srv/repo--cursor-perf")).as_deref(),
            Some("cursor")
        );
        assert_eq!(
            infer_agent(Path::new("/srv/WorkBuddy-notes/wt")).as_deref(),
            Some("workbuddy")
        );
    }

    #[test]
    fn agent_is_none_when_unrecognised() {
        assert!(infer_agent(Path::new("/Users/andy/MySrc/remote-mic-app")).is_none());
        assert!(infer_agent(Path::new("/tmp/plain-worktree")).is_none());
    }

    // ---- the happy path ----------------------------------------------------

    #[test]
    fn a_clean_merged_worktree_is_safe() {
        let dir = temp_dir("clean");
        backdate(&dir, 30 * 86_400);
        let runner = FakeGitRunner::new(calm_rules(CLEAN_STATUS));
        let record = check_worktree(
            &runner,
            &dir,
            &feature_entry(&dir),
            false,
            &default_options(),
        );
        assert_eq!(record.safety.status, SafetyStatus::Safe);
        assert!(has(&record, ReasonCode::WtClean));
        assert!(has(&record, ReasonCode::WtMerged));
        // The exact reason set, not just "contains": a stray reason would change the
        // confirmation page the user reads.
        assert_eq!(codes(&record), vec!["wt.clean", "wt.merged"]);
        assert!(!has(&record, ReasonCode::WtCannotVerify));
        assert_eq!(record.size_bytes, Some(1_234));
        assert_eq!(record.merged_into_default, Some(true));
        assert_eq!(record.dirty_files, 0);
        assert!(record.agent.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_clean_merged_worktree_with_an_agent_path_is_labelled() {
        let parent = temp_dir("agent");
        let dir = parent.join("remote-mic-app--claude-ui");
        std::fs::create_dir_all(&dir).expect("create worktree dir");
        backdate(&dir, 30 * 86_400);
        let runner = FakeGitRunner::new(calm_rules(CLEAN_STATUS));
        let record = check_worktree(
            &runner,
            &dir,
            &feature_entry(&dir),
            false,
            &default_options(),
        );
        assert_eq!(record.agent.as_deref(), Some("claude"));
        let _ = std::fs::remove_dir_all(&parent);
    }

    // ---- one positive and one negative case per reason code ----------------

    #[test]
    fn wt_main_marks_the_first_entry_unsafe() {
        let dir = temp_dir("main");
        backdate(&dir, 30 * 86_400);
        let runner = FakeGitRunner::new(calm_rules(CLEAN_STATUS));
        let record = check_worktree(
            &runner,
            &dir,
            &feature_entry(&dir),
            true,
            &default_options(),
        );
        assert!(has(&record, ReasonCode::WtMain));
        assert_eq!(record.safety.status, SafetyStatus::Unsafe);
        assert!(record.is_main);

        let record = check_worktree(
            &runner,
            &dir,
            &feature_entry(&dir),
            false,
            &default_options(),
        );
        assert!(!has(&record, ReasonCode::WtMain));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn wt_locked_blocks_a_locked_worktree() {
        let dir = temp_dir("locked");
        backdate(&dir, 30 * 86_400);
        let mut entry = feature_entry(&dir);
        entry.is_locked = true;
        entry.lock_reason = Some("holding for review".to_string());
        let runner = FakeGitRunner::new(calm_rules(CLEAN_STATUS));
        let record = check_worktree(&runner, &dir, &entry, false, &default_options());
        assert!(has(&record, ReasonCode::WtLocked));
        assert_eq!(record.safety.status, SafetyStatus::Unsafe);
        assert!(record.is_locked);
        let reason = record
            .safety
            .reasons
            .iter()
            .find(|reason| reason.code == ReasonCode::WtLocked)
            .expect("locked reason");
        assert!(reason.evidence.contains("holding for review"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn wt_dirty_blocks_a_dirty_worktree() {
        let dir = temp_dir("dirty");
        backdate(&dir, 30 * 86_400);
        let runner = FakeGitRunner::new(calm_rules(DIRTY_STATUS));
        let record = check_worktree(
            &runner,
            &dir,
            &feature_entry(&dir),
            false,
            &default_options(),
        );
        assert!(has(&record, ReasonCode::WtDirty));
        assert_eq!(record.safety.status, SafetyStatus::Unsafe);
        assert_eq!(record.dirty_files, 1);
        // A dirty worktree is not clean, and the untracked list is collected.
        assert!(!has(&record, ReasonCode::WtClean));
        assert!(has(&record, ReasonCode::WtUntracked));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn wt_unpushed_blocks_unpushed_commits() {
        let dir = temp_dir("unpushed");
        backdate(&dir, 30 * 86_400);
        let runner = FakeGitRunner::new(calm_rules(AHEAD_STATUS));
        let record = check_worktree(
            &runner,
            &dir,
            &feature_entry(&dir),
            false,
            &default_options(),
        );
        assert!(has(&record, ReasonCode::WtUnpushed));
        assert_eq!(record.safety.status, SafetyStatus::Unsafe);
        assert_eq!(record.ahead, 2);
        assert_eq!(record.behind, 1);
        assert!(record.has_upstream);

        let runner = FakeGitRunner::new(calm_rules(CLEAN_STATUS));
        let record = check_worktree(
            &runner,
            &dir,
            &feature_entry(&dir),
            false,
            &default_options(),
        );
        assert!(!has(&record, ReasonCode::WtUnpushed));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn wt_detached_requires_a_detached_head_without_upstream() {
        let dir = temp_dir("detached");
        backdate(&dir, 30 * 86_400);
        let mut entry = feature_entry(&dir);
        entry.is_detached = true;
        entry.branch = None;
        let status =
            "# branch.oid 3704a98b0e299445f52e13555002a5ffe7d13814\n# branch.head (detached)\n";
        let runner = FakeGitRunner::new(calm_rules(status));
        let record = check_worktree(&runner, &dir, &entry, false, &default_options());
        assert!(has(&record, ReasonCode::WtDetached));
        assert!(record.is_detached);
        assert!(record.branch.is_none());
        assert_eq!(record.safety.status, SafetyStatus::Review);

        let runner = FakeGitRunner::new(calm_rules(CLEAN_STATUS));
        let record = check_worktree(
            &runner,
            &dir,
            &feature_entry(&dir),
            false,
            &default_options(),
        );
        assert!(!has(&record, ReasonCode::WtDetached));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn wt_no_upstream_fires_only_without_a_merge() {
        let dir = temp_dir("no-upstream");
        backdate(&dir, 30 * 86_400);
        let runner = FakeGitRunner::new(calm_rules(CLEAN_STATUS));
        let record = check_worktree(
            &runner,
            &dir,
            &feature_entry(&dir),
            false,
            &default_options(),
        );
        // Clean + merged: the upstream line is absent, but merging removes the reason.
        assert!(!has(&record, ReasonCode::WtNoUpstream));

        let not_merged = vec![
            canned(&["status", "--porcelain=v2", "--branch"], CLEAN_STATUS),
            canned(&["stash", "list", "--format=%gd"], ""),
            canned(&["log", "-1", "--format=%ct"], "1700000000\n"),
            canned(
                &["rev-parse", "--git-path", "index.lock"],
                "/nonexistent/.git/index.lock\n",
            ),
            (
                vec![
                    "merge-base".to_string(),
                    "--is-ancestor".to_string(),
                    "--".to_string(),
                    "19402c62a2bdb19e2271df95ecfc1a272aad4bf6".to_string(),
                    "main".to_string(),
                ],
                Canned::Out(GitOutput {
                    stdout: String::new(),
                    stderr: String::new(),
                    code: Some(1),
                }),
            ),
        ];
        let runner = FakeGitRunner::new(not_merged);
        let record = check_worktree(
            &runner,
            &dir,
            &feature_entry(&dir),
            false,
            &default_options(),
        );
        assert!(has(&record, ReasonCode::WtNoUpstream));
        assert!(has(&record, ReasonCode::WtUnmerged));
        assert_eq!(record.safety.status, SafetyStatus::Review);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn wt_unmerged_downgrades_a_clean_worktree_to_review() {
        let dir = temp_dir("unmerged");
        backdate(&dir, 30 * 86_400);
        let mut rules = calm_rules(CLEAN_STATUS);
        rules.pop();
        rules.push((
            vec![
                "merge-base".to_string(),
                "--is-ancestor".to_string(),
                "--".to_string(),
                "19402c62a2bdb19e2271df95ecfc1a272aad4bf6".to_string(),
                "main".to_string(),
            ],
            Canned::Out(GitOutput {
                stdout: String::new(),
                stderr: String::new(),
                code: Some(1),
            }),
        ));
        let runner = FakeGitRunner::new(rules);
        let record = check_worktree(
            &runner,
            &dir,
            &feature_entry(&dir),
            false,
            &default_options(),
        );
        assert!(has(&record, ReasonCode::WtUnmerged));
        assert!(!has(&record, ReasonCode::WtMerged));
        assert_eq!(record.merged_into_default, Some(false));
        assert_eq!(record.safety.status, SafetyStatus::Review);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn wt_untracked_is_review_by_default_and_unsafe_on_request() {
        let dir = temp_dir("untracked");
        backdate(&dir, 30 * 86_400);
        let mut rules = calm_rules(DIRTY_STATUS);
        // The untracked-file list is a second call; keep it succeeding.
        rules.push(canned(
            &["ls-files", "--others", "--exclude-standard"],
            "notes.md\n",
        ));
        let runner = FakeGitRunner::new(rules);
        let record = check_worktree(
            &runner,
            &dir,
            &feature_entry(&dir),
            false,
            &default_options(),
        );
        assert!(has(&record, ReasonCode::WtUntracked));
        assert_eq!(record.safety.status, SafetyStatus::Unsafe);
        assert_eq!(record.untracked_file_list, vec!["notes.md".to_string()]);
        let untracked = record
            .safety
            .reasons
            .iter()
            .find(|reason| reason.code == ReasonCode::WtUntracked)
            .expect("untracked reason");
        assert_eq!(untracked.severity, SafetyStatus::Review);

        let mut opts = default_options();
        opts.untracked_as_unsafe = true;
        let runner = FakeGitRunner::new({
            let mut rules = calm_rules(DIRTY_STATUS);
            rules.push(canned(
                &["ls-files", "--others", "--exclude-standard"],
                "notes.md\n",
            ));
            rules
        });
        let record = check_worktree(&runner, &dir, &feature_entry(&dir), false, &opts);
        let untracked = record
            .safety
            .reasons
            .iter()
            .find(|reason| reason.code == ReasonCode::WtUntracked)
            .expect("untracked reason");
        assert_eq!(untracked.severity, SafetyStatus::Unsafe);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn wt_stash_counts_entries() {
        let dir = temp_dir("stash");
        backdate(&dir, 30 * 86_400);
        let mut rules = calm_rules(CLEAN_STATUS);
        for rule in &mut rules {
            if rule.0
                == vec![
                    "stash".to_string(),
                    "list".to_string(),
                    "--format=%gd".to_string(),
                ]
            {
                if let Canned::Out(output) = &mut rule.1 {
                    output.stdout = "stash@{0}\nstash@{1}\n".to_string();
                }
            }
        }
        let runner = FakeGitRunner::new(rules);
        let record = check_worktree(
            &runner,
            &dir,
            &feature_entry(&dir),
            false,
            &default_options(),
        );
        assert!(has(&record, ReasonCode::WtStash));
        assert_eq!(record.stash_count, 2);
        assert_eq!(record.safety.status, SafetyStatus::Review);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn wt_recent_activity_uses_the_directory_mtime() {
        let dir = temp_dir("recent");
        // The clock is injected, but the mtime is a real filesystem fact, so the two
        // must agree: backdating is expressed against the same instant.
        let mut opts = default_options();
        opts.now = SystemTime::now();
        // The directory mtime is "now" (just created) and the commit is old: the later
        // of the two wins, so the worktree counts as recently active.
        let runner = FakeGitRunner::new(calm_rules(CLEAN_STATUS));
        let record = check_worktree(&runner, &dir, &feature_entry(&dir), false, &opts);
        assert!(has(&record, ReasonCode::WtRecentActivity));
        assert_eq!(record.safety.status, SafetyStatus::Review);

        backdate(&dir, 30 * 86_400);
        let runner = FakeGitRunner::new(calm_rules(CLEAN_STATUS));
        let record = check_worktree(&runner, &dir, &feature_entry(&dir), false, &opts);
        assert!(!has(&record, ReasonCode::WtRecentActivity));
        assert_eq!(record.safety.status, SafetyStatus::Safe);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn wt_agent_active_fires_on_a_present_index_lock() {
        let dir = temp_dir("agent-active");
        backdate(&dir, 30 * 86_400);
        let lock = dir.join("index.lock");
        std::fs::write(&lock, "").expect("create lock file");
        let mut rules = calm_rules(CLEAN_STATUS);
        for rule in &mut rules {
            if rule.0
                == vec![
                    "rev-parse".to_string(),
                    "--git-path".to_string(),
                    "index.lock".to_string(),
                ]
            {
                if let Canned::Out(output) = &mut rule.1 {
                    output.stdout = format!("{}\n", lock.display());
                }
            }
        }
        let runner = FakeGitRunner::new(rules);
        let record = check_worktree(
            &runner,
            &dir,
            &feature_entry(&dir),
            false,
            &default_options(),
        );
        assert!(has(&record, ReasonCode::WtAgentActive));
        assert_eq!(record.safety.status, SafetyStatus::Review);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn wt_agent_active_resolves_a_relative_git_path() {
        let dir = temp_dir("agent-relative");
        backdate(&dir, 30 * 86_400);
        let lock = dir.join(".git/index.lock");
        std::fs::create_dir_all(dir.join(".git")).expect("create .git dir");
        std::fs::write(&lock, "").expect("create lock file");
        // The main worktree answers with a relative path; it must resolve against the
        // worktree directory.
        let mut rules = calm_rules(CLEAN_STATUS);
        for rule in &mut rules {
            if rule.0
                == vec![
                    "rev-parse".to_string(),
                    "--git-path".to_string(),
                    "index.lock".to_string(),
                ]
            {
                if let Canned::Out(output) = &mut rule.1 {
                    output.stdout = ".git/index.lock\n".to_string();
                }
            }
        }
        let runner = FakeGitRunner::new(rules);
        let record = check_worktree(
            &runner,
            &dir,
            &feature_entry(&dir),
            false,
            &default_options(),
        );
        assert!(has(&record, ReasonCode::WtAgentActive));
        let reason = record
            .safety
            .reasons
            .iter()
            .find(|reason| reason.code == ReasonCode::WtAgentActive)
            .expect("agent active reason");
        assert!(reason.evidence.contains(".git/index.lock"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn wt_whitelisted_blocks_a_matching_path() {
        let dir = temp_dir("whitelisted");
        backdate(&dir, 30 * 86_400);
        let mut opts = default_options();
        opts.whitelist = vec![dir.clone()];
        let runner = FakeGitRunner::new(calm_rules(CLEAN_STATUS));
        let record = check_worktree(&runner, &dir, &feature_entry(&dir), false, &opts);
        assert!(has(&record, ReasonCode::WtWhitelisted));
        assert_eq!(record.safety.status, SafetyStatus::Unsafe);
        let reason = record
            .safety
            .reasons
            .iter()
            .find(|reason| reason.code == ReasonCode::WtWhitelisted)
            .expect("whitelisted reason");
        assert!(reason.evidence.contains("matches whitelist:"));

        let runner = FakeGitRunner::new(calm_rules(CLEAN_STATUS));
        let record = check_worktree(
            &runner,
            &dir,
            &feature_entry(&dir),
            false,
            &default_options(),
        );
        assert!(!has(&record, ReasonCode::WtWhitelisted));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn wt_prunable_is_safe_only_when_merged() {
        let gone = std::env::temp_dir().join(format!(
            "worktree-cleaner-prunable-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&gone);
        let mut entry = feature_entry(&gone);
        entry.is_prunable = true;
        entry.prunable_reason = Some("gitdir file points to non-existent location".to_string());

        let runner = FakeGitRunner::new(calm_rules(CLEAN_STATUS));
        let record = check_worktree(
            &runner,
            Path::new("/repo"),
            &entry,
            false,
            &default_options(),
        );
        assert!(has(&record, ReasonCode::WtPrunable));
        assert_eq!(record.safety.status, SafetyStatus::Safe);
        assert!(record.is_prunable);
        // No per-worktree command runs against a directory that is not there.
        assert!(!runner.called(&["status", "--porcelain=v2", "--branch"]));
        assert!(!runner.called(&["stash", "list", "--format=%gd"]));
        // The merge check still runs, in the repository rather than the worktree,
        // and carries the `--` separator that keeps a ref name from being parsed
        // as an option.
        assert!(runner.called(&[
            "merge-base",
            "--is-ancestor",
            "--",
            "19402c62a2bdb19e2271df95ecfc1a272aad4bf6",
            "main"
        ]));

        let mut rules = calm_rules(CLEAN_STATUS);
        rules.pop();
        rules.push((
            vec![
                "merge-base".to_string(),
                "--is-ancestor".to_string(),
                "--".to_string(),
                "19402c62a2bdb19e2271df95ecfc1a272aad4bf6".to_string(),
                "main".to_string(),
            ],
            Canned::Out(GitOutput {
                stdout: String::new(),
                stderr: String::new(),
                code: Some(1),
            }),
        ));
        let runner = FakeGitRunner::new(rules);
        let record = check_worktree(
            &runner,
            Path::new("/repo"),
            &entry,
            false,
            &default_options(),
        );
        assert!(has(&record, ReasonCode::WtUnmerged));
        assert_eq!(record.safety.status, SafetyStatus::Review);
    }

    #[test]
    fn wt_prunable_negative_case_when_the_directory_exists() {
        let dir = temp_dir("prunable-negative");
        backdate(&dir, 30 * 86_400);
        let runner = FakeGitRunner::new(calm_rules(CLEAN_STATUS));
        let record = check_worktree(
            &runner,
            &dir,
            &feature_entry(&dir),
            false,
            &default_options(),
        );
        assert!(!has(&record, ReasonCode::WtPrunable));
        assert!(!record.is_prunable);
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---- fail closed (D3) --------------------------------------------------

    #[test]
    fn a_timed_out_status_yields_review_with_cannot_verify() {
        let dir = temp_dir("timeout");
        backdate(&dir, 30 * 86_400);
        let mut rules = calm_rules(CLEAN_STATUS);
        rules[0] = (
            vec![
                "status".to_string(),
                "--porcelain=v2".to_string(),
                "--branch".to_string(),
            ],
            Canned::Timeout,
        );
        let runner = FakeGitRunner::new(rules);
        let record = check_worktree(
            &runner,
            &dir,
            &feature_entry(&dir),
            false,
            &default_options(),
        );
        assert!(has(&record, ReasonCode::WtCannotVerify));
        assert_eq!(record.safety.status, SafetyStatus::Review);
        // No positive reason may survive the failed check itself.
        assert!(!has(&record, ReasonCode::WtClean));
        // The merge check is independent, so it may still answer; the level stays
        // Review regardless.
        let reason = record
            .safety
            .reasons
            .iter()
            .find(|reason| reason.code == ReasonCode::WtCannotVerify)
            .expect("cannot verify reason");
        assert!(reason.evidence.contains("timed out"));
        assert!(!reason.evidence.trim().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_permission_error_on_stash_yields_review_with_cannot_verify() {
        let dir = temp_dir("permission");
        backdate(&dir, 30 * 86_400);
        let mut rules = calm_rules(CLEAN_STATUS);
        rules[1] = failing(
            &["stash", "list", "--format=%gd"],
            Some(128),
            "fatal: unable to read .git/worktrees/x",
        );
        let runner = FakeGitRunner::new(rules);
        let record = check_worktree(
            &runner,
            &dir,
            &feature_entry(&dir),
            false,
            &default_options(),
        );
        assert!(has(&record, ReasonCode::WtCannotVerify));
        assert_eq!(record.safety.status, SafetyStatus::Review);
        // The clean reason came from a check that did succeed, so it stays; the level
        // is still Review because the stash check failed.
        assert!(has(&record, ReasonCode::WtClean));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_undetermined_default_branch_blocks_the_merge_verdict() {
        let dir = temp_dir("no-default");
        backdate(&dir, 30 * 86_400);
        let mut opts = default_options();
        opts.default_branch = None;
        let runner = FakeGitRunner::new(calm_rules(CLEAN_STATUS));
        let record = check_worktree(&runner, &dir, &feature_entry(&dir), false, &opts);
        assert_eq!(record.merged_into_default, None);
        assert!(has(&record, ReasonCode::WtCannotVerify));
        assert_eq!(record.safety.status, SafetyStatus::Review);
        // The merge check is never even attempted without a default branch.
        // (`called_prefix`: the real call carries two more arguments, so an exact
        // match here would pass vacuously even if the check did run.)
        assert!(!runner.called_prefix(&["merge-base", "--is-ancestor"]));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_unknown_merge_status_is_never_reported_as_merged() {
        let dir = temp_dir("merge-unknown");
        backdate(&dir, 30 * 86_400);
        let head = "19402c62a2bdb19e2271df95ecfc1a272aad4bf6";
        // Both candidate names are tried and both exit 128: the verdict must stay
        // unknown rather than collapse to either answer (D3).
        let mut rules = calm_rules(CLEAN_STATUS);
        rules.pop();
        rules.push((
            vec![
                "merge-base".to_string(),
                "--is-ancestor".to_string(),
                "--".to_string(),
                head.to_string(),
                "main".to_string(),
            ],
            Canned::Out(GitOutput {
                stdout: String::new(),
                stderr: "fatal: Not a valid object name deadbeef\n".to_string(),
                code: Some(128),
            }),
        ));
        rules.push((
            vec![
                "merge-base".to_string(),
                "--is-ancestor".to_string(),
                "--".to_string(),
                head.to_string(),
                "origin/main".to_string(),
            ],
            Canned::Out(GitOutput {
                stdout: String::new(),
                stderr: "fatal: Not a valid object name deadbeef\n".to_string(),
                code: Some(128),
            }),
        ));
        let runner = FakeGitRunner::new(rules);
        let record = check_worktree(
            &runner,
            &dir,
            &feature_entry(&dir),
            false,
            &default_options(),
        );
        assert_eq!(record.merged_into_default, None);
        assert!(!has(&record, ReasonCode::WtMerged));
        assert!(!has(&record, ReasonCode::WtUnmerged));
        assert!(has(&record, ReasonCode::WtCannotVerify));
        assert_eq!(record.safety.status, SafetyStatus::Review);
        assert!(runner.called(&["merge-base", "--is-ancestor", "--", head, "main"]));
        assert!(runner.called(&["merge-base", "--is-ancestor", "--", head, "origin/main"]));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn every_reason_carries_non_empty_evidence() {
        let dir = temp_dir("evidence");
        backdate(&dir, 30 * 86_400);
        let lock = dir.join("index.lock");
        std::fs::write(&lock, "").expect("create lock file");
        let mut rules = calm_rules(DIRTY_STATUS);
        rules.push(canned(
            &["ls-files", "--others", "--exclude-standard"],
            "notes.md\nscratch/\n",
        ));
        for rule in &mut rules {
            if rule.0
                == vec![
                    "rev-parse".to_string(),
                    "--git-path".to_string(),
                    "index.lock".to_string(),
                ]
            {
                if let Canned::Out(output) = &mut rule.1 {
                    output.stdout = format!("{}\n", lock.display());
                }
            }
            if rule.0
                == vec![
                    "stash".to_string(),
                    "list".to_string(),
                    "--format=%gd".to_string(),
                ]
            {
                if let Canned::Out(output) = &mut rule.1 {
                    output.stdout = "stash@{0}\n".to_string();
                }
            }
        }
        let runner = FakeGitRunner::new(rules);
        let record = check_worktree(
            &runner,
            &dir,
            &feature_entry(&dir),
            true,
            &default_options(),
        );
        assert!(!record.safety.reasons.is_empty());
        for reason in &record.safety.reasons {
            assert!(
                !reason.evidence.trim().is_empty(),
                "empty evidence for {}",
                reason.code.as_str()
            );
            assert!(
                !reason.text.trim().is_empty(),
                "empty text for {}",
                reason.code.as_str()
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---- default branch detection -----------------------------------------

    #[test]
    fn default_branch_prefers_the_remote_pointer() {
        let runner = FakeGitRunner::new(vec![
            canned(
                &["symbolic-ref", "--quiet", "refs/remotes/origin/HEAD"],
                "refs/remotes/origin/develop\n",
            ),
            canned(
                &["rev-parse", "--verify", "--quiet", "refs/heads/main"],
                "abc\n",
            ),
        ]);
        assert_eq!(
            detect_default_branch(&runner, Path::new("/repo")).as_deref(),
            Some("develop")
        );
    }

    #[test]
    fn default_branch_falls_back_to_main_then_master() {
        let runner = FakeGitRunner::new(vec![canned(
            &["rev-parse", "--verify", "--quiet", "refs/heads/main"],
            "abc\n",
        )]);
        assert_eq!(
            detect_default_branch(&runner, Path::new("/repo")).as_deref(),
            Some("main")
        );

        let runner = FakeGitRunner::new(vec![canned(
            &["rev-parse", "--verify", "--quiet", "refs/heads/master"],
            "abc\n",
        )]);
        assert_eq!(
            detect_default_branch(&runner, Path::new("/repo")).as_deref(),
            Some("master")
        );
    }

    #[test]
    fn default_branch_is_none_when_nothing_matches() {
        let runner = FakeGitRunner::new(vec![]);
        assert!(detect_default_branch(&runner, Path::new("/repo")).is_none());
    }

    // ---- record shape ------------------------------------------------------

    #[test]
    fn record_carries_the_parsed_fields() {
        let dir = temp_dir("record");
        // Older than the canned commit time, so the commit time wins as the later of
        // the two activity signals.
        backdate(&dir, 5 * 365 * 86_400);
        let runner = FakeGitRunner::new(calm_rules(CLEAN_STATUS));
        let record = check_worktree(
            &runner,
            &dir,
            &feature_entry(&dir),
            false,
            &default_options(),
        );
        assert_eq!(record.branch.as_deref(), Some("feature/x"));
        assert_eq!(record.head.as_deref(), Some("19402c6"));
        assert_eq!(record.path, dir.display().to_string());
        assert!(!record.is_detached);
        assert!(!record.is_locked);
        assert!(!record.is_prunable);
        assert_eq!(record.dirty_files, 0);
        assert_eq!(record.untracked_files, 0);
        assert!(record.untracked_file_list.is_empty());
        assert_eq!(record.ahead, 0);
        assert_eq!(record.behind, 0);
        assert!(!record.has_upstream);
        assert_eq!(record.stash_count, 0);
        // last_updated_at is the (old) commit time, formatted as RFC-3339 UTC.
        assert_eq!(
            record.last_updated_at.as_deref(),
            Some("2023-11-14T22:13:20Z")
        );
        assert_eq!(record.size_bytes, Some(1_234));
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---- review findings, each pinned by its own regression test -------------

    #[test]
    fn a_spawn_error_on_status_yields_review_with_cannot_verify() {
        // GAP-1: a git that cannot be spawned at all is a transport failure, not an
        // empty-but-fine result (D3).
        let dir = temp_dir("spawn-status");
        backdate(&dir, 30 * 86_400);
        let mut rules = calm_rules(CLEAN_STATUS);
        rules[0] = (
            vec![
                "status".to_string(),
                "--porcelain=v2".to_string(),
                "--branch".to_string(),
            ],
            Canned::Io,
        );
        let runner = FakeGitRunner::new(rules);
        let record = check_worktree(
            &runner,
            &dir,
            &feature_entry(&dir),
            false,
            &default_options(),
        );
        assert!(has(&record, ReasonCode::WtCannotVerify));
        assert!(!has(&record, ReasonCode::WtClean));
        assert!(!has(&record, ReasonCode::WtDirty));
        assert_eq!(record.safety.status, SafetyStatus::Review);
        let reason = record
            .safety
            .reasons
            .iter()
            .find(|reason| reason.code == ReasonCode::WtCannotVerify)
            .expect("cannot verify reason");
        assert!(reason.evidence.contains("could not run"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_spawn_error_on_the_merge_check_yields_review_with_cannot_verify() {
        // GAP-1, merge branch: the verdict must stay unknown, and a transport
        // error must not trigger the second name candidate (retrying cannot fix
        // a process that would not start).
        let dir = temp_dir("spawn-merge");
        backdate(&dir, 30 * 86_400);
        let head = "19402c62a2bdb19e2271df95ecfc1a272aad4bf6";
        let mut rules = calm_rules(CLEAN_STATUS);
        rules.pop();
        rules.push((
            vec![
                "merge-base".to_string(),
                "--is-ancestor".to_string(),
                "--".to_string(),
                head.to_string(),
                "main".to_string(),
            ],
            Canned::Io,
        ));
        let runner = FakeGitRunner::new(rules);
        let record = check_worktree(
            &runner,
            &dir,
            &feature_entry(&dir),
            false,
            &default_options(),
        );
        assert_eq!(record.merged_into_default, None);
        assert!(!has(&record, ReasonCode::WtMerged));
        assert!(!has(&record, ReasonCode::WtUnmerged));
        assert!(has(&record, ReasonCode::WtCannotVerify));
        assert_eq!(record.safety.status, SafetyStatus::Review);
        assert!(!runner.called(&["merge-base", "--is-ancestor", "--", head, "origin/main"]));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn merge_check_falls_back_to_the_remote_tracking_name() {
        // Finding 5: `origin/HEAD` often names a branch that exists only as
        // `refs/remotes/origin/<name>`, and `git merge-base` does not resolve the
        // bare name to it (exit 128, no DWIM). Without the fallback every healthy
        // worktree would degrade to cannot_verify.
        let dir = temp_dir("merge-fallback");
        backdate(&dir, 30 * 86_400);
        let head = "19402c62a2bdb19e2271df95ecfc1a272aad4bf6";
        let mut opts = default_options();
        opts.default_branch = Some("develop".to_string());
        let runner = FakeGitRunner::new(vec![
            canned(&["status", "--porcelain=v2", "--branch"], CLEAN_STATUS),
            canned(&["stash", "list", "--format=%gd"], ""),
            canned(&["log", "-1", "--format=%ct"], "1700000000\n"),
            canned(
                &["rev-parse", "--git-path", "index.lock"],
                "/nonexistent/.git/index.lock\n",
            ),
            // The bare name exits 128: git *ran* and answered "no such ref" (an
            // `Ok` with a non-zero code, which is what `ProcessGitRunner` returns
            // for a command that failed — a transport-level `Err` would instead
            // short-circuit the candidate loop).
            (
                vec![
                    "merge-base".to_string(),
                    "--is-ancestor".to_string(),
                    "--".to_string(),
                    head.to_string(),
                    "develop".to_string(),
                ],
                Canned::Out(GitOutput {
                    stdout: String::new(),
                    stderr: "fatal: not a valid ref: develop\n".to_string(),
                    code: Some(128),
                }),
            ),
            (
                vec![
                    "merge-base".to_string(),
                    "--is-ancestor".to_string(),
                    "--".to_string(),
                    head.to_string(),
                    "origin/develop".to_string(),
                ],
                Canned::Out(GitOutput {
                    stdout: String::new(),
                    stderr: String::new(),
                    code: Some(0),
                }),
            ),
        ]);
        let record = check_worktree(&runner, &dir, &feature_entry(&dir), false, &opts);
        assert_eq!(record.merged_into_default, Some(true));
        assert!(has(&record, ReasonCode::WtMerged));
        assert!(!has(&record, ReasonCode::WtCannotVerify));
        assert_eq!(record.safety.status, SafetyStatus::Safe);
        let reason = record
            .safety
            .reasons
            .iter()
            .find(|reason| reason.code == ReasonCode::WtMerged)
            .expect("merged reason");
        assert!(
            reason.evidence.contains("origin/develop"),
            "the evidence must name the candidate that answered: {}",
            reason.evidence
        );
        // Both candidates were tried, the bare name first.
        assert!(runner.called(&["merge-base", "--is-ancestor", "--", head, "develop"]));
        assert!(runner.called(&["merge-base", "--is-ancestor", "--", head, "origin/develop"]));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_status_without_the_header_is_never_read_as_clean() {
        // Without the `# branch.*` header the counts are not "zero", they are
        // unknown (D3): a positive reason must not be minted from them.
        let dir = temp_dir("no-header");
        backdate(&dir, 30 * 86_400);
        let runner = FakeGitRunner::new(calm_rules(
            "1 .M N... 100644 100644 100644 814f4a422927b82f5f8a43f8fab6d3839e3983f2 814f4a422927b82f5f8a43f8fab6d3839e3983f2 a.txt\n",
        ));
        let record = check_worktree(
            &runner,
            &dir,
            &feature_entry(&dir),
            false,
            &default_options(),
        );
        assert!(has(&record, ReasonCode::WtCannotVerify));
        let reason = record
            .safety
            .reasons
            .iter()
            .find(|reason| reason.code == ReasonCode::WtCannotVerify)
            .expect("cannot verify reason");
        assert!(reason.evidence.contains("status header"));
        assert!(!has(&record, ReasonCode::WtClean));
        assert!(!has(&record, ReasonCode::WtDirty));
        assert!(!has(&record, ReasonCode::WtUntracked));
        assert_eq!(record.dirty_files, 0);
        assert_eq!(record.untracked_files, 0);
        assert_eq!(record.safety.status, SafetyStatus::Review);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_upstream_without_ahead_behind_is_unknown_not_zero() {
        // Finding 3b: git omits `# branch.ab` when the upstream ref itself is
        // gone, so "0 commits not pushed" would be a lie.
        let dir = temp_dir("upstream-without-ab");
        backdate(&dir, 30 * 86_400);
        let runner = FakeGitRunner::new(calm_rules(UPSTREAM_WITHOUT_AB));
        let record = check_worktree(
            &runner,
            &dir,
            &feature_entry(&dir),
            false,
            &default_options(),
        );
        assert!(has(&record, ReasonCode::WtCannotVerify));
        let reason = record
            .safety
            .reasons
            .iter()
            .find(|reason| reason.code == ReasonCode::WtCannotVerify)
            .expect("cannot verify reason");
        assert!(reason.evidence.contains("upstream branch no longer exists"));
        assert!(!has(&record, ReasonCode::WtUnpushed));
        assert!(!has(&record, ReasonCode::WtClean));
        assert_eq!(record.ahead, 0);
        assert_eq!(record.safety.status, SafetyStatus::Review);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    #[cfg(unix)]
    fn an_unreadable_directory_is_never_reported_as_gone() {
        // Finding 2: `Path::exists()` conflates EACCES with NotFound, which routed
        // an unreadable worktree to the prune-only channel with a Safe verdict.
        use std::os::unix::fs::PermissionsExt;

        let parent = temp_dir("unreadable");
        let dir = parent.join("wt");
        std::fs::create_dir_all(&dir).expect("create worktree dir");
        backdate(&dir, 30 * 86_400);
        // Search permission on the *parent* is what a stat of the child needs;
        // the leaf's own mode is irrelevant to stat(2).
        let sealed = std::fs::Permissions::from_mode(0o000);
        std::fs::set_permissions(&parent, sealed).expect("seal parent");
        let stat_fails = std::fs::metadata(&dir).is_err();
        if !stat_fails {
            // Running as root defeats the permission model; nothing to assert.
            std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o755))
                .expect("restore parent mode");
            let _ = std::fs::remove_dir_all(&parent);
            return;
        }

        let runner = FakeGitRunner::new(calm_rules(CLEAN_STATUS));
        let record = check_worktree(
            &runner,
            &parent.join("wt"),
            &feature_entry(&dir),
            false,
            &default_options(),
        );
        std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o755))
            .expect("restore parent mode");

        assert!(has(&record, ReasonCode::WtCannotVerify));
        assert!(
            record
                .safety
                .reasons
                .iter()
                .any(|reason| reason.text == "cannot read the worktree directory"),
            "the failure must name the directory stat: {:?}",
            codes(&record)
        );
        assert!(!record.is_prunable, "unreadable is not gone");
        assert!(!has(&record, ReasonCode::WtPrunable));
        assert_eq!(record.safety.status, SafetyStatus::Review);
        let _ = std::fs::remove_dir_all(&parent);
    }

    #[test]
    fn the_untracked_count_is_exact_and_truncation_is_disclosed() {
        // Finding 10: `git status` folds a whole untracked directory into one
        // entry; `ls-files --others` lists every file, so the record must carry
        // the exact count — and must say so when the displayed list is capped.
        let dir = temp_dir("untracked-exact");
        backdate(&dir, 30 * 86_400);

        let mut rules = calm_rules(DIRTY_STATUS);
        rules.push(canned(
            &["ls-files", "--others", "--exclude-standard"],
            "notes.md\nscratch/a\nscratch/b\n",
        ));
        let runner = FakeGitRunner::new(rules);
        let record = check_worktree(
            &runner,
            &dir,
            &feature_entry(&dir),
            false,
            &default_options(),
        );
        assert_eq!(record.untracked_files, 3, "exact count from ls-files");
        assert_eq!(
            record.untracked_file_list,
            vec![
                "notes.md".to_string(),
                "scratch/a".to_string(),
                "scratch/b".to_string()
            ]
        );
        let reason = record
            .safety
            .reasons
            .iter()
            .find(|reason| reason.code == ReasonCode::WtUntracked)
            .expect("untracked reason");
        assert_eq!(reason.evidence, "3 untracked files");
        assert!(!reason.evidence.contains("the list below"));

        // More files than the display cap: the count stays exact, the list is
        // capped, and the evidence discloses the cap.
        let many: String = (0..201)
            .map(|index| format!("f{index:03}.txt\n"))
            .collect::<Vec<_>>()
            .concat();
        let mut rules = calm_rules(DIRTY_STATUS);
        rules.push(canned(
            &["ls-files", "--others", "--exclude-standard"],
            &many,
        ));
        let runner = FakeGitRunner::new(rules);
        let record = check_worktree(
            &runner,
            &dir,
            &feature_entry(&dir),
            false,
            &default_options(),
        );
        assert_eq!(record.untracked_files, 201);
        assert_eq!(record.untracked_file_list.len(), 200);
        let reason = record
            .safety
            .reasons
            .iter()
            .find(|reason| reason.code == ReasonCode::WtUntracked)
            .expect("untracked reason");
        assert_eq!(
            reason.evidence,
            "201 untracked files (the list below shows the first 200)"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_non_numeric_commit_time_fails_closed() {
        // A `%ct` that does not parse must not silently drop the activity signal:
        // the directory mtime still answers, and the failure is visible.
        let dir = temp_dir("bad-commit-time");
        backdate(&dir, 30 * 86_400);
        let mut rules = calm_rules(CLEAN_STATUS);
        for rule in &mut rules {
            if rule.0
                == vec![
                    "log".to_string(),
                    "-1".to_string(),
                    "--format=%ct".to_string(),
                ]
            {
                if let Canned::Out(output) = &mut rule.1 {
                    output.stdout = "not-a-time\n".to_string();
                }
            }
        }
        let runner = FakeGitRunner::new(rules);
        let record = check_worktree(
            &runner,
            &dir,
            &feature_entry(&dir),
            false,
            &default_options(),
        );
        assert!(has(&record, ReasonCode::WtCannotVerify));
        let reason = record
            .safety
            .reasons
            .iter()
            .find(|reason| reason.code == ReasonCode::WtCannotVerify)
            .expect("cannot verify reason");
        assert!(reason.evidence.contains("non-numeric"));
        assert!(
            record.last_updated_at.is_some(),
            "the directory mtime still answers"
        );
        assert!(!has(&record, ReasonCode::WtRecentActivity));
        assert_eq!(record.safety.status, SafetyStatus::Review);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_failed_log_ls_files_or_rev_parse_each_fails_closed() {
        // GAP-8: every secondary call has its own fail-closed path.
        let dir = temp_dir("secondary-failures");
        backdate(&dir, 30 * 86_400);

        // -- log fails: activity falls back to the directory mtime
        let mut rules = calm_rules(CLEAN_STATUS);
        rules[2] = failing(
            &["log", "-1", "--format=%ct"],
            Some(128),
            "fatal: bad default revision",
        );
        let runner = FakeGitRunner::new(rules);
        let record = check_worktree(
            &runner,
            &dir,
            &feature_entry(&dir),
            false,
            &default_options(),
        );
        assert!(has(&record, ReasonCode::WtCannotVerify));
        assert!(record.last_updated_at.is_some());
        assert_eq!(record.safety.status, SafetyStatus::Review);

        // -- ls-files fails: the folded status count is used, the list stays
        //    empty, and no truncation note is invented
        let mut rules = calm_rules(DIRTY_STATUS);
        rules.push(failing(
            &["ls-files", "--others", "--exclude-standard"],
            Some(128),
            "fatal: not a git repository",
        ));
        let runner = FakeGitRunner::new(rules);
        let record = check_worktree(
            &runner,
            &dir,
            &feature_entry(&dir),
            false,
            &default_options(),
        );
        assert!(has(&record, ReasonCode::WtUntracked));
        assert_eq!(record.untracked_files, 1, "falls back to the status count");
        assert!(record.untracked_file_list.is_empty());
        let reason = record
            .safety
            .reasons
            .iter()
            .find(|reason| reason.code == ReasonCode::WtUntracked)
            .expect("untracked reason");
        assert_eq!(reason.evidence, "1 untracked files");
        assert!(has(&record, ReasonCode::WtCannotVerify));

        // -- rev-parse --git-path fails: no lock verdict may be produced
        let mut rules = calm_rules(CLEAN_STATUS);
        for rule in &mut rules {
            if rule.0
                == vec![
                    "rev-parse".to_string(),
                    "--git-path".to_string(),
                    "index.lock".to_string(),
                ]
            {
                *rule = failing(
                    &["rev-parse", "--git-path", "index.lock"],
                    Some(128),
                    "fatal: not a git repository",
                );
            }
        }
        let runner = FakeGitRunner::new(rules);
        let record = check_worktree(
            &runner,
            &dir,
            &feature_entry(&dir),
            false,
            &default_options(),
        );
        assert!(has(&record, ReasonCode::WtCannotVerify));
        assert!(!has(&record, ReasonCode::WtAgentActive));
        assert_eq!(record.safety.status, SafetyStatus::Review);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn every_worktree_reason_code_reports_non_empty_text_and_evidence() {
        // GAP-5: one record that carries as many wt.* codes as the decision table
        // can produce at once, so every one of them is proven to render text and
        // evidence (D9). The remaining codes have dedicated tests above.
        let dir = temp_dir("all-codes");
        let lock = dir.join("index.lock");
        std::fs::write(&lock, "").expect("create lock file");
        let mut opts = default_options();
        opts.now = SystemTime::now();
        opts.whitelist = vec![dir.clone()];
        let mut entry = feature_entry(&dir);
        entry.is_locked = true;
        entry.lock_reason = Some("holding for review".to_string());

        let mut rules = calm_rules(MESSY_STATUS);
        // Swap the merge-base rule for "not an ancestor" *before* appending the
        // ls-files rule: `pop()` removes the last element, and appending first
        // would silently drop the ls-files rule instead.
        rules.pop();
        rules.push((
            vec![
                "merge-base".to_string(),
                "--is-ancestor".to_string(),
                "--".to_string(),
                "19402c62a2bdb19e2271df95ecfc1a272aad4bf6".to_string(),
                "main".to_string(),
            ],
            Canned::Out(GitOutput {
                stdout: String::new(),
                stderr: String::new(),
                code: Some(1),
            }),
        ));
        // The log call fails, which is what puts wt.cannot_verify into the set.
        rules[2] = failing(
            &["log", "-1", "--format=%ct"],
            Some(128),
            "fatal: bad default revision",
        );
        rules.push(canned(
            &["ls-files", "--others", "--exclude-standard"],
            "notes.md\n",
        ));
        // Two stash entries, and the lock file the rev-parse call must report.
        for rule in &mut rules {
            if rule.0
                == vec![
                    "stash".to_string(),
                    "list".to_string(),
                    "--format=%gd".to_string(),
                ]
            {
                if let Canned::Out(output) = &mut rule.1 {
                    output.stdout = "stash@{0}\nstash@{1}\n".to_string();
                }
            }
            if rule.0
                == vec![
                    "rev-parse".to_string(),
                    "--git-path".to_string(),
                    "index.lock".to_string(),
                ]
            {
                if let Canned::Out(output) = &mut rule.1 {
                    output.stdout = format!("{}\n", lock.display());
                }
            }
        }

        let runner = FakeGitRunner::new(rules);
        let record = check_worktree(&runner, &dir, &entry, true, &opts);
        assert_eq!(
            codes(&record),
            vec![
                "wt.main",
                "wt.locked",
                "wt.whitelisted",
                "wt.dirty",
                "wt.unpushed",
                "wt.stash",
                "wt.untracked",
                "wt.cannot_verify",
                "wt.agent_active",
                "wt.recent_activity",
                "wt.unmerged",
            ]
        );
        for reason in &record.safety.reasons {
            assert!(
                !reason.text.trim().is_empty(),
                "empty text for {}",
                reason.code.as_str()
            );
            assert!(
                !reason.evidence.trim().is_empty(),
                "empty evidence for {}",
                reason.code.as_str()
            );
        }
        assert_eq!(record.safety.status, SafetyStatus::Unsafe);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
