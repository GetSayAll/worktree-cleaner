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
use std::time::{Duration, SystemTime};

use crate::git::{display_command, GitError, GitOutput, GitRunner, DEFAULT_GIT_TIMEOUT};
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

/// Parses `git worktree list --porcelain`.
///
/// Entries are separated by blank lines and each starts with a `worktree <path>`
/// line; the **first** entry is the main worktree. Unknown attribute lines are
/// ignored on purpose: porcelain gains fields over time, and an unknown line must
/// never fail a scan.
pub fn parse_worktree_list(porcelain: &str) -> Vec<WorktreeEntry> {
    let mut entries: Vec<WorktreeEntry> = Vec::new();
    let mut current: Option<WorktreeEntry> = None;

    for line in porcelain.lines() {
        let line = line.trim_end_matches('\r');
        if let Some(path) = line.strip_prefix("worktree ") {
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
        if let Some(sha) = line.strip_prefix("HEAD ") {
            entry.head = Some(sha.trim().to_string());
        } else if let Some(reference) = line.strip_prefix("branch ") {
            entry.branch = Some(short_branch(reference.trim()));
        } else if line == "detached" {
            entry.is_detached = true;
        } else if line == "bare" {
            entry.is_bare = true;
        } else if line == "locked" {
            entry.is_locked = true;
        } else if let Some(reason) = line.strip_prefix("locked ") {
            entry.is_locked = true;
            entry.lock_reason = non_empty(reason);
        } else if line == "prunable" {
            entry.is_prunable = true;
        } else if let Some(reason) = line.strip_prefix("prunable ") {
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
    /// Untracked, non-ignored files.
    pub untracked_files: u32,
}

/// Parses `git status --porcelain=v2 --branch`.
///
/// Counting rules follow the porcelain v2 format: `1 ` / `2 ` / `u ` lines are
/// changes, `? ` lines are untracked files, and `! ` (ignored) lines are not counted
/// at all.
pub fn parse_status(porcelain_v2: &str) -> StatusInfo {
    let mut info = StatusInfo::default();
    for line in porcelain_v2.lines() {
        if let Some(rest) = line.strip_prefix("# branch.oid ") {
            let oid = rest.trim();
            // An unborn branch reports `(initial)`, which is not a SHA.
            info.head = if oid == "(initial)" {
                None
            } else {
                Some(oid.to_string())
            };
        } else if let Some(rest) = line.strip_prefix("# branch.head ") {
            let head = rest.trim();
            info.branch = if head == "(detached)" {
                None
            } else {
                Some(short_branch(head))
            };
        } else if let Some(rest) = line.strip_prefix("# branch.upstream ") {
            info.upstream = Some(short_branch(rest.trim()));
        } else if let Some(rest) = line.strip_prefix("# branch.ab ") {
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
    /// Per-command timeout for the Git calls.
    pub timeout: Duration,
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
            timeout: DEFAULT_GIT_TIMEOUT,
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
    let directory_present = entry.path.exists();
    let directory_gone = entry.is_prunable || !directory_present;

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
            if info.dirty_files > 0 {
                reasons.push(Reason::blocking(
                    ReasonCode::WtDirty,
                    "has uncommitted changes",
                    format!("{} files have uncommitted changes", info.dirty_files),
                ));
            }
            if info.untracked_files > 0 {
                let severity = if opts.untracked_as_unsafe {
                    SafetyStatus::Unsafe
                } else {
                    SafetyStatus::Review
                };
                reasons.push(Reason::new(
                    ReasonCode::WtUntracked,
                    severity,
                    "has untracked files",
                    format!("{} untracked files", info.untracked_files),
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
                untracked_file_list = output
                    .stdout
                    .lines()
                    .filter(|line| !line.is_empty())
                    .take(MAX_UNTRACKED_LIST)
                    .map(str::to_string)
                    .collect();
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
        .and_then(|output| output.stdout.trim().parse::<i128>().ok())
        .map(from_unix_seconds);
        let directory_mtime = std::fs::metadata(&entry.path)
            .and_then(|meta| meta.modified())
            .ok();
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
            match runner.run(repo, &["merge-base", "--is-ancestor", head, default_branch]) {
                // The exit code is the answer: 0 = ancestor (merged), 1 = not an
                // ancestor, anything else = cannot determine.
                Ok(output) if output.code == Some(0) => {
                    reasons.push(Reason::safe(
                        ReasonCode::WtMerged,
                        "merged into the default branch",
                        format!("merge-base reports {head} is an ancestor of {default_branch}"),
                    ));
                    Some(true)
                }
                Ok(output) if output.code == Some(1) => {
                    reasons.push(Reason::review(
                        ReasonCode::WtUnmerged,
                        "not merged into the default branch",
                        format!("merge-base reports {head} is not an ancestor of {default_branch}"),
                    ));
                    Some(false)
                }
                Ok(output) => {
                    let error = GitError::Failed {
                        command: display_command(
                            repo,
                            &["merge-base", "--is-ancestor", head, default_branch],
                        ),
                        code: output.code,
                        stderr: output.stderr.clone(),
                    };
                    reasons.push(cannot_verify("the merge status", &error));
                    None
                }
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
        is_prunable: directory_gone,
        dirty_files: status.as_ref().map_or(0, |info| info.dirty_files),
        untracked_files: status.as_ref().map_or(0, |info| info.untracked_files),
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

fn prunable_evidence(entry: &WorktreeEntry) -> String {
    match entry.prunable_reason.as_deref() {
        Some(reason) => format!("only cleans .git/worktrees metadata ({reason})"),
        None => "only cleans .git/worktrees metadata".to_string(),
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
    use std::time::UNIX_EPOCH;

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

    /// Porcelain output captured from a real repository (git 2.54): a main worktree,
    /// a locked linked worktree, a detached one, and a prunable one.
    const PORCELAIN: &str = "\
worktree /private/tmp/wt-fixture
HEAD 19402c62a2bdb19e2271df95ecfc1a272aad4bf6
branch refs/heads/main

worktree /private/tmp/wt-fixture-detached
HEAD 19402c62a2bdb19e2271df95ecfc1a272aad4bf6
detached

worktree /private/tmp/wt-fixture-feature
HEAD 19402c62a2bdb19e2271df95ecfc1a272aad4bf6
branch refs/heads/feature/x
locked holding for review

worktree /private/tmp/wt-fixture-gone
HEAD 19402c62a2bdb19e2271df95ecfc1a272aad4bf6
branch refs/heads/gone-branch
prunable gitdir file points to non-existent location
";

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
        assert_eq!(entries.len(), 4);
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

        assert!(entries[3].is_prunable);
        assert_eq!(
            entries[3].prunable_reason.as_deref(),
            Some("gitdir file points to non-existent location")
        );
    }

    #[test]
    fn porcelain_parsing_tolerates_unknown_lines_and_crlf() {
        let text =
            "worktree /repo\r\nHEAD abc\r\nbranch refs/heads/main\r\nbrand-new-field whatever\r\n";
        let entries = parse_worktree_list(text);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].branch.as_deref(), Some("main"));
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
        // The merge check still runs, in the repository rather than the worktree.
        assert!(runner.called(&[
            "merge-base",
            "--is-ancestor",
            "19402c62a2bdb19e2271df95ecfc1a272aad4bf6",
            "main"
        ]));

        let mut rules = calm_rules(CLEAN_STATUS);
        rules.pop();
        rules.push((
            vec![
                "merge-base".to_string(),
                "--is-ancestor".to_string(),
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
        assert!(!runner.called(&["merge-base", "--is-ancestor"]));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_unknown_merge_status_is_never_reported_as_merged() {
        let dir = temp_dir("merge-unknown");
        backdate(&dir, 30 * 86_400);
        let mut rules = calm_rules(CLEAN_STATUS);
        rules.pop();
        rules.push(failing(
            &[
                "merge-base",
                "--is-ancestor",
                "19402c62a2bdb19e2271df95ecfc1a272aad4bf6",
                "main",
            ],
            Some(128),
            "fatal: Not a valid object name deadbeef",
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
        assert!(has(&record, ReasonCode::WtCannotVerify));
        assert_eq!(record.safety.status, SafetyStatus::Review);
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
}
