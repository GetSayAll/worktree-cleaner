//! Worktree checks (product plan section 5.1).
//!
//! The decision table is the specification: every branch below maps to exactly one
//! `wt.*` reason code, and the level of the record is **derived** from the reasons
//! (D2) — this module never scores a level on its own. Every failure path adds
//! `wt.cannot_verify` with level `Review` and is never allowed to look like a
//! positive result (D3).
//!
//! Data sources are read-only Git commands plus one filesystem fact (the presence of
//! `.git/index.lock`); the `.git` file format is never parsed (development contract
//! section 2).
//!
//! One datum is deliberately **not** a reason: the stash count. `refs/stash` lives in
//! the common Git directory, so every linked worktree reports the same list, and
//! `git worktree remove` never touches it. A stash is therefore a fact about the
//! repository, not about the worktree being judged: reporting it per worktree
//! downgraded every clean worktree of a repository holding a single stash (measured
//! on a 109-worktree repository: 48 of 48 clean-and-merged worktrees). The count is
//! measured once per scan and carried on the record as a Git fact, without a reason.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::artifact::{credential_in_worktree, CredentialScan};
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
    /// Stash entries held by the **repository**, measured once per scan by the
    /// orchestrator. Not a per-worktree measurement: `refs/stash` is shared by every
    /// worktree, so asking each one returns the same list. `None` = the call failed,
    /// which is reported as a scan warning; the record then carries `0` because
    /// `stash_count` has no "unknown" state. It influences no verdict either way.
    pub repository_stash_count: Option<u32>,
    /// Budget for the worktree-level credential walk.
    ///
    /// The walk skips the recognised build and cache directories, because those
    /// are artifact candidates and the artifact scan checks them separately. What
    /// remains is a project tree, so the bound is small; it exists so a worktree
    /// on a slow or network mount cannot hold the whole scan. Passing it produces
    /// `wt.cannot_verify`, never a silent pass.
    pub credential_scan_timeout: Duration,
    // There is deliberately no per-command timeout here: the timeout belongs to
    // the [`crate::git::GitRunner`], which owns the processes. The orchestrator
    // passes the setting into `ProcessGitRunner::new`, so one knob, one owner.
}

impl Default for WorktreeCheckOptions {
    fn default() -> Self {
        Self {
            default_branch: None,
            recent_activity_days: crate::DEFAULT_ACTIVITY_DAYS,
            untracked_as_unsafe: false,
            whitelist: Vec::new(),
            now: SystemTime::now(),
            size_bytes: None,
            repository_stash_count: None,
            credential_scan_timeout: Duration::from_secs(5),
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

/// Agents recognised by a path token, or by a branch when the path has none
/// (product plan section 4).
///
/// The worktree directory name is the strongest signal (`repo--codex-fix-ble`),
/// so every path component is split into tokens and compared case-insensitively.
/// The branch is a weaker fallback: it identifies the work by its convention
/// (`codex/update-check`), not by a filesystem layout. The label never changes a
/// safety verdict.
const AGENT_MARKERS: &[&str] = &["codex", "claude", "cursor", "aider", "workbuddy"];

/// The first agent marker in one piece of text, if any.
fn agent_in(text: &str) -> Option<String> {
    for token in text.split(|ch: char| !ch.is_alphanumeric()) {
        for marker in AGENT_MARKERS {
            if token.eq_ignore_ascii_case(marker) {
                return Some((*marker).to_string());
            }
        }
    }
    None
}

/// Infers the owning agent from the path, falling back to the branch name.
pub fn infer_agent(path: &Path, branch: Option<&str>) -> Option<String> {
    let components: Vec<String> = path
        .components()
        .map(|component| component.as_os_str().to_string_lossy().into_owned())
        .collect();
    // Deepest component first: the worktree directory name outranks its parents.
    for name in components.iter().rev() {
        if let Some(agent) = agent_in(name) {
            return Some(agent);
        }
    }
    branch.and_then(agent_in)
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
    let mut untracked_file_list: Vec<String> = Vec::new();
    // Exact untracked file count from `ls-files --others`, when that call
    // succeeded. `None` = not measured; the folded status count is then used.
    let mut untracked_listed: Option<u32> = None;
    let mut activity: Option<SystemTime> = None;
    // The status header names an upstream, but Git omits `# branch.ab` when that
    // ref is gone: ahead/behind are unknown, while the file counts are still real.
    let mut upstream_without_ab = false;

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
            if info.header_seen {
                // The file counts are independent of the ahead/behind line: Git
                // omits `# branch.ab` when the upstream ref is gone, but it still
                // prints the changed and untracked entries. Reading only the
                // confident half is what keeps a dirty worktree from hiding behind
                // an "unknown ahead/behind" Review (D3: fail closed, but do not
                // throw away the facts that did arrive).
                upstream_without_ab = info.upstream_without_ab;
                if info.dirty_files > 0 {
                    reasons.push(Reason::blocking(
                        ReasonCode::WtDirty,
                        "has uncommitted changes",
                        format!("{} files have uncommitted changes", info.dirty_files),
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
            } else {
                // A status without the `# branch.*` header cannot be read as
                // "zero changes": the counts would be a guess (D3).
                reasons.push(Reason::review(
                    ReasonCode::WtCannotVerify,
                    "cannot read the worktree status",
                    "the status header (# branch.oid / # branch.head) was missing, so the file counts are unknown",
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
                    // `write!` rather than `push_str(&format!(…))`: one allocation
                    // instead of two, and the result is discarded either way because
                    // writing into a `String` cannot fail.
                    let _ = write!(
                        evidence,
                        " (the list below shows the first {MAX_UNTRACKED_LIST})"
                    );
                }
                reasons.push(Reason::new(
                    ReasonCode::WtUntracked,
                    severity,
                    "has untracked files",
                    evidence,
                ));
            }
        }

        // The worktree itself is deleted by `git worktree remove`, ignored files
        // included, so the credential check has to run at the root. It is limited
        // to a worktree with no tracked changes: a dirty worktree is already
        // blocked, and walking it would only add cost. Untracked files do not stop
        // the check — an untracked `.env` is exactly the file this exists to catch.
        // Build and cache directories are skipped by the walker; the artifact scan
        // checks those when it visits them as candidates.
        if !is_main && !entry.is_locked && !is_prunable {
            if let Some(info) = &status {
                if info.dirty_files == 0 {
                    match credential_in_worktree(&entry.path, opts.credential_scan_timeout) {
                        CredentialScan::Complete(Some(name)) => {
                            // A tracked `.env.example` is in Git; deleting the
                            // worktree does not lose it, and blocking on it would
                            // make every repository that commits a template
                            // Unsafe. Only an untracked or ignored file is a
                            // worktree-level loss.
                            match runner.run(
                                &entry.path,
                                &["ls-files", "--error-unmatch", "--", &name],
                            ) {
                                Ok(output) if output.code == Some(0) => {}
                                Ok(output) if output.code == Some(1) => {
                                    reasons.push(Reason::blocking(
                                        ReasonCode::WtCredentials,
                                        "contains what looks like a credential file",
                                        format!(
                                            "{name} is inside; deleting the worktree would delete it too"
                                        ),
                                    ));
                                }
                                Ok(output) => reasons.push(Reason::review(
                                    ReasonCode::WtCannotVerify,
                                    "could not check whether the credential file is tracked",
                                    format!(
                                        "git ls-files exited with {:?} for {name}: {}",
                                        output.code,
                                        output.stderr.trim()
                                    ),
                                )),
                                Err(error) => reasons.push(cannot_verify(
                                    "whether the credential file is tracked",
                                    &error,
                                )),
                            }
                        }
                        CredentialScan::Complete(None) => {}
                        CredentialScan::Truncated { scanned } => reasons.push(Reason::review(
                            ReasonCode::WtCannotVerify,
                            "could not finish checking the worktree for credentials",
                            format!(
                                "looked at {scanned} entries before the time limit, without covering the whole worktree; a credential further inside would not have been found"
                            ),
                        )),
                        CredentialScan::Failed(error) => reasons.push(Reason::review(
                            ReasonCode::WtCannotVerify,
                            "could not check the worktree for credentials",
                            format!(
                                "could not list {} to check for credentials: {error}",
                                entry.path.display()
                            ),
                        )),
                    }
                }
            }
        }

        // Last activity = the HEAD commit time, and only the HEAD commit time.
        //
        // The directory mtime used to be half of this signal, and was removed: it
        // changes only when a *top-level entry* is created, deleted or renamed, not
        // when a file deeper inside is edited. The result was a false-positive class
        // that drowned the verdict — one bulk touch (an indexer, a backup, a cleanup
        // script) refreshed 22 worktrees of a repository whose last commit was 8-10
        // days old, and every one of them was downgraded to "recent activity".
        // Genuine recent work is already visible as `wt.dirty` / `wt.untracked` /
        // `wt.agent_active`, so the mtime side carried no weight it could bear.
        let commit_time = run_checked(
            runner,
            &entry.path,
            "the HEAD commit time",
            &["log", "-1", "--format=%ct"],
            &mut reasons,
        )
        .and_then(|output| {
            if let Ok(seconds) = output.stdout.trim().parse::<i128>() {
                Some(from_unix_seconds(seconds))
            } else {
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
        // With no second signal to fall back on, an unreadable HEAD time leaves the
        // timestamp empty — the `wt.cannot_verify` reason above already keeps the
        // verdict at Review, so nothing is silently called Safe (D3).
        activity = commit_time;

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

    // ---- merge status: runs in `repo`, so it survives a missing directory ----
    let mut absorbed = false;
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
                    // Not an ancestor, which is what a squash merge looks like: the
                    // change is in the default branch but as a different commit. Ask
                    // the patch question before falling back to "unmerged". The three
                    // answers are distinct: the patch is there and so is the content
                    // (`wt.absorbed`, Safe); the patch is there but the content
                    // changed (`wt.absorbed` plus `wt.absorbed_content_changed`,
                    // Review); or at least one commit has no equivalent patch
                    // (`wt.unmerged`, Review). A patch check that could not run is not
                    // an upgrade — it must never make a worktree look safer than it
                    // did (D3).
                    match commits_absorbed(runner, repo, head, default_branch) {
                        Ok(AbsorbedAnswer::Absorbed(absorbed_evidence)) => {
                            absorbed = true;
                            reasons.push(Reason::safe(
                                ReasonCode::WtAbsorbed,
                                "every commit has an equivalent patch in the default branch",
                                absorbed_evidence,
                            ));
                        }
                        // The patch-id matched, but the files it touched no longer
                        // hold the content this branch produced. The patch answer
                        // keeps its original meaning and stays Safe; the content
                        // answer is a second reason at Review, so the overall
                        // verdict cannot be Safe. A later revert and an ordinary
                        // edit after the merge look the same here, so the second
                        // reason says what was observed rather than claiming a
                        // revert.
                        Ok(AbsorbedAnswer::ContentChanged(why)) => {
                            absorbed = true;
                            reasons.push(Reason::safe(
                                ReasonCode::WtAbsorbed,
                                "every commit has an equivalent patch in the default branch",
                                why.clone(),
                            ));
                            reasons.push(Reason::review(
                                ReasonCode::WtAbsorbedContentChanged,
                                "the patch is in the default branch, but the content changed afterwards",
                                format!("{evidence}; {why}"),
                            ));
                        }
                        Ok(AbsorbedAnswer::NotAbsorbed(why)) => reasons.push(Reason::review(
                            ReasonCode::WtUnmerged,
                            "not merged into the default branch",
                            format!("{evidence}; {why}"),
                        )),
                        Err(_) => reasons.push(Reason::review(
                            ReasonCode::WtUnmerged,
                            "not merged into the default branch",
                            evidence,
                        )),
                    }
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

    // A recent commit is an attention signal, not evidence that deleting the
    // worktree loses something. When the worktree is clean and its work is already
    // in the default branch, the only thing a recent HEAD says is "this was used
    // recently" — and `last_updated_at` already carries that fact to the detail
    // panel. Keep the Review reason only when the worktree is not in that state.
    let clean = status
        .as_ref()
        .is_some_and(|info| info.dirty_files == 0 && info.untracked_files == 0);
    let in_default = merged == Some(true) || absorbed;
    if let Some(activity) = activity {
        let age_days = elapsed_days(opts.now, activity);
        if age_days < u64::from(opts.recent_activity_days) && !(clean && in_default) {
            reasons.push(Reason::review(
                ReasonCode::WtRecentActivity,
                "changed within the activity window",
                format!("last activity: {}", humanize_days(age_days)),
            ));
        }
    }

    // The upstream ref is gone, so ahead/behind are unknown. That matters only when
    // the merge answer is not already "the work is in the default branch"; in that
    // case the missing ref changes nothing about whether the worktree can go.
    if upstream_without_ab && merged != Some(true) && !absorbed {
        reasons.push(Reason::review(
            ReasonCode::WtCannotVerify,
            "cannot read the worktree status",
            "the upstream branch no longer exists, so the ahead/behind counts are unknown",
        ));
    }

    // Unpushed commits live on the branch, not in the worktree: `git worktree
    // remove` keeps the ref. When the work is already in the default branch the
    // upstream being behind says nothing about this directory, so no reason is
    // emitted. Otherwise it is a Review — the reader should see the fact, but the
    // directory can still be removed without losing the commits. The old
    // `wt.unpushed` code was Unsafe for this fact and is retained for audit logs;
    // `wt.unpushed_branch` is the append-only replacement.
    if let Some(info) = &status {
        if !upstream_without_ab && info.ahead > 0 && merged != Some(true) && !absorbed {
            let upstream = info.upstream.as_deref().unwrap_or("its upstream");
            reasons.push(Reason::review(
                ReasonCode::WtUnpushedBranch,
                "commits are not pushed to the upstream branch",
                format!(
                    "{} commits not pushed to {upstream}; the branch ref is kept when the worktree is removed",
                    info.ahead
                ),
            ));
        }
    }

    // ---- reasons that need both the status header and the merge answer ----
    if let Some(info) = &status {
        // `absorbed` counts as merged here: the change is in the default branch, so
        // "no upstream branch" carries none of the risk this reason exists to
        // report — the work is not stranded.
        if info.upstream.is_none() && merged != Some(true) && !absorbed {
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
        // A detached HEAD is only a reason when its commit is not already in the
        // default branch: then the commit is preserved there, and the worktree
        // holds no branch that could be lost. The check needs the status header
        // for the "no upstream" half.
        if entry.is_detached && info.upstream.is_none() && merged != Some(true) && !absorbed {
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
        absorbed_into_default: absorbed,
        stash_count: opts.repository_stash_count.unwrap_or(0),
        last_updated_at: activity.map(format_rfc3339),
        agent: infer_agent(&entry.path, entry.branch.as_deref()),
        size_bytes: opts.size_bytes,
        safety,
    }
}

/// Two candidate names are tried, in order: `origin/<name>` first, then the bare
/// name.
///
/// The remote-tracking ref is tried first because it is the one that answers the
/// question actually being asked — "is this change in the default branch" — while a
/// local branch of the same name is an accident of checkout state. Measured on one
/// machine, local `main` trailed `origin/main` by 6 to 1336 commits across five
/// repositories; a bare name reported merged work as unmerged in 116 of 199 clean
/// worktrees, and, where the local branch held commits that were never pushed,
/// reported unmerged work as merged — the direction D3 forbids. The bare name stays
/// as the fallback for a repository with no remote, where `origin/<name>` exits 128
/// rather than answering.
///
/// `--` keeps a branch name that starts with `-` from being parsed as an option.
/// The exit code is the answer: 0 = ancestor, 1 = not an ancestor; every other
/// outcome is an error the caller must treat as unknown (D3). A transport error
/// (timeout, spawn failure) is returned as-is: retrying another name cannot fix it.
fn merge_base_is_ancestor(
    runner: &dyn GitRunner,
    repo: &Path,
    head: &str,
    default_branch: &str,
) -> Result<(bool, String), GitError> {
    let mut candidates = Vec::with_capacity(2);
    if !default_branch.starts_with("origin/") {
        candidates.push(format!("origin/{default_branch}"));
    }
    candidates.push(default_branch.to_string());
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

/// What `git cherry` plus the content comparison established.
enum AbsorbedAnswer {
    /// Every commit has an equivalent patch, and the default branch still holds
    /// the content. This is the only answer that may be `Safe`.
    Absorbed(String),
    /// Every commit has an equivalent patch, but the files it touched no longer
    /// hold the content this branch produced. A later revert and an ordinary edit
    /// after the merge look the same; the answer is `Review`, not "unmerged".
    ContentChanged(String),
    /// At least one commit has no equivalent patch upstream.
    NotAbsorbed(String),
}

/// Asks `git cherry` whether every commit on this branch already has an equivalent
/// patch in the default branch.
///
/// `git cherry <upstream> <head>` prints one line per commit in
/// `<upstream>..<head>`: `-` when an equivalent patch is already upstream, `+` when
/// it is not. No `+` line means the work is there even though the commits are not
/// ancestors of it, which is what a squash merge produces and what
/// `merge-base --is-ancestor` cannot see.
///
/// The candidates mirror [`merge_base_is_ancestor`] — `origin/<name>` first, then
/// the bare name — so both halves of the merge answer are asked against the same
/// ref. A name that does not resolve exits 128 and the next one is tried; a
/// transport error is returned as-is, because another name cannot fix it.
///
/// A patch-id survives a revert, so the patch answer is paired with a content
/// answer. When the content differs, the result is [`AbsorbedAnswer::ContentChanged`]
/// rather than "absorbed": the patch is upstream, but its effect is not confirmed.
fn commits_absorbed(
    runner: &dyn GitRunner,
    repo: &Path,
    head: &str,
    default_branch: &str,
) -> Result<AbsorbedAnswer, GitError> {
    let mut candidates = Vec::with_capacity(2);
    if !default_branch.starts_with("origin/") {
        candidates.push(format!("origin/{default_branch}"));
    }
    candidates.push(default_branch.to_string());
    let mut last_error: Option<GitError> = None;
    for candidate in &candidates {
        let args = ["cherry", "--", candidate.as_str(), head];
        match runner.run(repo, &args) {
            Ok(output) if output.code == Some(0) => {
                let mut unique = 0_usize;
                let mut absorbed = 0_usize;
                for line in output.stdout.lines() {
                    match line.chars().next() {
                        Some('+') => unique += 1,
                        Some('-') => absorbed += 1,
                        _ => {}
                    }
                }
                if unique > 0 {
                    return Ok(AbsorbedAnswer::NotAbsorbed(format!(
                        "{unique} commit(s) not in {candidate}"
                    )));
                }
                // A matching patch-id says the patch was applied, not that it is still
                // there: a later revert leaves the patch-id matching and the code gone.
                let (present, detail) = content_still_present(runner, repo, head, candidate)?;
                let subject = if absorbed == 0 {
                    format!("no commit of its own beyond {candidate}")
                } else {
                    format!("{absorbed} commit(s) have an equivalent patch in {candidate}")
                };
                return Ok(if present {
                    AbsorbedAnswer::Absorbed(format!("{subject}, and {detail}"))
                } else {
                    AbsorbedAnswer::ContentChanged(format!("{subject}, but {detail}"))
                });
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
        command: display_command(repo, &["cherry", "--", head]),
        code: None,
        stderr: "no cherry candidate resolved".to_string(),
    }))
}

/// How many paths the content confirmation will ask about in one command.
///
/// Past this the answer is "cannot confirm" rather than a command line long enough to
/// matter: the ordinary squash merge touches a handful of files, and a branch touching
/// hundreds is not one this check should sign off on without reading it.
const MAX_CONFIRM_PATHS: usize = 200;

/// Confirms that the default branch still holds the content this branch produces.
///
/// `git cherry` compares patch-ids, and a patch-id survives a revert: the commit that
/// applied the change still matches even after another commit undid it. Matching the
/// patch is therefore not the same as the change being there now — the hole the
/// triage document names and this closes.
///
/// The test is a tree comparison, not a history one. `git diff <head> <target> -- <the
/// files this branch touched>` asks whether the two sides hold the same content for
/// exactly those files; the file list comes from `<target>...<head>`, which is the
/// branch's own diff against the merge base. A revert or an ordinary edit after
/// the merge makes the two sides differ, so the branch stays `Review`. Because the
/// comparison covers only the files the branch touched, unrelated work on the
/// default branch cannot make it fail.
///
/// The answer is "cannot confirm", not "reverted": whole-file equality cannot tell
/// the two apart. The caller reports that distinction as
/// `wt.absorbed_content_changed`.
fn content_still_present(
    runner: &dyn GitRunner,
    repo: &Path,
    head: &str,
    target: &str,
) -> Result<(bool, String), GitError> {
    let range = format!("{target}...{head}");
    let listing = runner.run(repo, &["diff", "--name-only", range.as_str(), "--"])?;
    if listing.code != Some(0) {
        return Err(GitError::Failed {
            command: display_command(repo, &["diff", "--name-only", range.as_str(), "--"]),
            code: listing.code,
            stderr: listing.stderr,
        });
    }
    let paths: Vec<&str> = listing
        .stdout
        .lines()
        .filter(|line| !line.is_empty())
        .collect();
    if paths.is_empty() {
        // Nothing of its own to lose: every commit here is a merge whose parents are
        // already in the default branch.
        return Ok((true, "it changes no file of its own".to_string()));
    }
    if paths.len() > MAX_CONFIRM_PATHS {
        return Ok((
            false,
            format!(
                "it changes {} files, too many to confirm one by one",
                paths.len()
            ),
        ));
    }
    let mut args: Vec<&str> = vec!["diff", "--quiet", head, target, "--"];
    args.extend(paths.iter().copied());
    let output = runner.run(repo, &args)?;
    match output.code {
        Some(0) => Ok((
            true,
            format!("the {} touched file(s) hold the same content", paths.len()),
        )),
        Some(1) => Ok((
            false,
            format!(
                "{} touched file(s) changed after the merge, so the content this branch produced is no longer identical",
                paths.len()
            ),
        )),
        _ => Err(GitError::Failed {
            command: display_command(repo, &args),
            code: output.code,
            stderr: output.stderr,
        }),
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
                stderr: output.stderr,
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

    /// A successful process whose exit code is 1: `git ls-files --error-unmatch`
    /// uses that for "not tracked", which is an answer, not a failure.
    fn not_tracked(args: &[&str]) -> (Vec<String>, Canned) {
        (
            args.iter().map(|arg| (*arg).to_string()).collect(),
            Canned::Out(GitOutput {
                stdout: String::new(),
                stderr: String::new(),
                code: Some(1),
            }),
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

    /// The same missing `# branch.ab`, with one tracked file changed. The file
    /// count is still a real answer even though ahead/behind are not.
    const DIRTY_UPSTREAM_WITHOUT_AB: &str = "\
# branch.oid 3704a98b0e299445f52e13555002a5ffe7d13814
# branch.head feature/x
# branch.upstream origin/feature/x
1 .M N... 100644 100644 100644 814f4a422927b82f5f8a43f8fab6d3839e3983f2 814f4a422927b82f5f8a43f8fab6d3839e3983f2 a.txt
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
            now: UNIX_EPOCH + Duration::from_hours(500_000),
            size_bytes: Some(1_234),
            ..WorktreeCheckOptions::default()
        }
    }

    /// The canned answers every check needs for a clean, merged, inactive worktree.
    ///
    /// The merge answer is keyed on `origin/main`, not `main`: the remote-tracking
    /// ref is the candidate the check tries first, and a fixture keyed on the bare
    /// name would answer a call that is never made.
    fn calm_rules(status: &str) -> Vec<(Vec<String>, Canned)> {
        vec![
            canned(&["status", "--porcelain=v2", "--branch"], status),
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
                    "origin/main".to_string(),
                ],
                Canned::Out(GitOutput {
                    stdout: String::new(),
                    stderr: String::new(),
                    code: Some(0),
                }),
            ),
        ]
    }

    /// The same answers, with the branch not merged and one unique commit. Used by
    /// the tests whose reason only exists when the work is not in the default
    /// branch.
    fn not_merged_rules(status: &str) -> Vec<(Vec<String>, Canned)> {
        let mut rules = calm_rules(status);
        rules.pop();
        rules.push((
            vec![
                "merge-base".to_string(),
                "--is-ancestor".to_string(),
                "--".to_string(),
                "19402c62a2bdb19e2271df95ecfc1a272aad4bf6".to_string(),
                "origin/main".to_string(),
            ],
            Canned::Out(GitOutput {
                stdout: String::new(),
                stderr: String::new(),
                code: Some(1),
            }),
        ));
        rules.push(canned(
            &[
                "cherry",
                "--",
                "origin/main",
                "19402c62a2bdb19e2271df95ecfc1a272aad4bf6",
            ],
            "+ 2222222222222222222222222222222222222222\n",
        ));
        rules
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
            infer_agent(Path::new("/repo--codex-fix-ble"), None).as_deref(),
            Some("codex")
        );
        assert_eq!(
            infer_agent(Path::new("/srv/repo--cursor-perf"), None).as_deref(),
            Some("cursor")
        );
        assert_eq!(
            infer_agent(Path::new("/srv/WorkBuddy-notes/wt"), None).as_deref(),
            Some("workbuddy")
        );
    }

    #[test]
    fn agent_falls_back_to_the_branch_when_the_path_has_no_marker() {
        assert_eq!(
            infer_agent(
                Path::new("/Users/andy/Develop/Src/AISrc/GetSayAll/.worktrees/remote-mic-update-checks-20261111"),
                Some("codex/update-check-coalescing-20261111")
            )
            .as_deref(),
            Some("codex")
        );
    }

    #[test]
    fn a_path_marker_outranks_the_branch_marker() {
        assert_eq!(
            infer_agent(
                Path::new("/repo--claude-ui"),
                Some("codex/update-check-coalescing-20261111")
            )
            .as_deref(),
            Some("claude")
        );
    }

    #[test]
    fn agent_is_none_when_the_path_and_branch_have_no_marker() {
        assert!(infer_agent(Path::new("/Users/andy/MySrc/remote-mic-app"), None).is_none());
        assert!(infer_agent(
            Path::new("/Users/andy/MySrc/remote-mic-app"),
            Some("fix/update-check")
        )
        .is_none());
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
    fn wt_unpushed_is_review_because_the_branch_ref_is_kept() {
        let dir = temp_dir("unpushed");
        backdate(&dir, 30 * 86_400);
        let runner = FakeGitRunner::new(not_merged_rules(AHEAD_STATUS));
        let record = check_worktree(
            &runner,
            &dir,
            &feature_entry(&dir),
            false,
            &default_options(),
        );
        assert!(has(&record, ReasonCode::WtUnpushedBranch));
        assert!(
            !has(&record, ReasonCode::WtUnpushed),
            "the old Unsafe code is retained for audit logs and must not be emitted"
        );
        assert_eq!(record.safety.status, SafetyStatus::Review);
        assert_eq!(record.ahead, 2);
        assert_eq!(record.behind, 1);
        assert!(record.has_upstream);
        let reason = record
            .safety
            .reasons
            .iter()
            .find(|reason| reason.code == ReasonCode::WtUnpushedBranch)
            .expect("unpushed reason");
        assert!(
            reason.evidence.contains("origin/feature/x")
                && reason.evidence.contains("branch ref is kept"),
            "the evidence must name the upstream and the kept ref: {}",
            reason.evidence
        );

        // Merged: the upstream being behind says nothing about this directory.
        // `git worktree remove` keeps the branch, and the work is in the default
        // branch already, so the reason must not appear at all.
        let runner = FakeGitRunner::new(calm_rules(AHEAD_STATUS));
        let record = check_worktree(
            &runner,
            &dir,
            &feature_entry(&dir),
            false,
            &default_options(),
        );
        assert!(!has(&record, ReasonCode::WtUnpushedBranch));
        assert_eq!(record.safety.status, SafetyStatus::Safe);

        let runner = FakeGitRunner::new(calm_rules(CLEAN_STATUS));
        let record = check_worktree(
            &runner,
            &dir,
            &feature_entry(&dir),
            false,
            &default_options(),
        );
        assert!(!has(&record, ReasonCode::WtUnpushedBranch));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn wt_detached_is_only_a_reason_when_the_commit_is_not_in_the_default_branch() {
        let dir = temp_dir("detached");
        backdate(&dir, 30 * 86_400);
        let mut entry = feature_entry(&dir);
        entry.is_detached = true;
        entry.branch = None;
        let status =
            "# branch.oid 3704a98b0e299445f52e13555002a5ffe7d13814\n# branch.head (detached)\n";

        // Unmerged: the commit exists only here, so the detached HEAD is a fact
        // the reader should see.
        let runner = FakeGitRunner::new(not_merged_rules(status));
        let record = check_worktree(&runner, &dir, &entry, false, &default_options());
        assert!(has(&record, ReasonCode::WtDetached));
        assert!(record.is_detached);
        assert!(record.branch.is_none());
        assert_eq!(record.safety.status, SafetyStatus::Review);

        // Merged: the commit is preserved in the default branch, so the detached
        // HEAD is not a reason to keep the directory.
        let runner = FakeGitRunner::new(calm_rules(status));
        let record = check_worktree(&runner, &dir, &entry, false, &default_options());
        assert!(!has(&record, ReasonCode::WtDetached));
        assert_eq!(record.safety.status, SafetyStatus::Safe);

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
                    "origin/main".to_string(),
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
    fn the_remote_default_branch_answers_before_the_local_one() {
        // What this pins: `merge-base --is-ancestor HEAD main` resolves `main` to the
        // *local* branch, and in a worktree-heavy repository the local default is
        // usually behind the remote — measured at 6 to 1336 commits behind across five
        // repositories, which reported 116 of 199 clean worktrees as unmerged when they
        // had in fact been merged. The local branch is canned as "not an ancestor"
        // here, so a verdict of merged can only come from the remote name having been
        // asked first, and the last assertion proves the local name was never asked.
        let dir = temp_dir("remote-default");
        backdate(&dir, 30 * 86_400);
        let head = "19402c62a2bdb19e2271df95ecfc1a272aad4bf6";
        let mut rules = calm_rules(CLEAN_STATUS);
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
        assert_eq!(record.merged_into_default, Some(true));
        assert!(has(&record, ReasonCode::WtMerged));
        assert!(!has(&record, ReasonCode::WtUnmerged));
        assert_eq!(record.safety.status, SafetyStatus::Safe);
        assert!(
            !runner.called(&["merge-base", "--is-ancestor", "--", head, "main"]),
            "the local branch must not be consulted once the remote one has answered"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_default_branch_with_a_slash_is_still_asked_as_a_remote_ref() {
        // `origin/HEAD` can name `release/2.0`. The old prefix test was
        // `contains('/')`, which read that name as if it were already
        // remote-qualified and asked only the local `release/2.0`; a stale or
        // missing local branch then made merged work look unmerged. The prefix
        // test is now `starts_with("origin/")`, so both halves ask the remote
        // name first.
        let dir = temp_dir("slash-default");
        backdate(&dir, 30 * 86_400);
        let head = "19402c62a2bdb19e2271df95ecfc1a272aad4bf6";
        let mut opts = default_options();
        opts.default_branch = Some("release/2.0".to_string());
        let mut rules = calm_rules(CLEAN_STATUS);
        rules.pop();
        rules.push((
            vec![
                "merge-base".to_string(),
                "--is-ancestor".to_string(),
                "--".to_string(),
                head.to_string(),
                "origin/release/2.0".to_string(),
            ],
            Canned::Out(GitOutput {
                stdout: String::new(),
                stderr: String::new(),
                code: Some(1),
            }),
        ));
        rules.push(canned(
            &["cherry", "--", "origin/release/2.0", head],
            "- 1111111111111111111111111111111111111111\n",
        ));
        let range = format!("origin/release/2.0...{head}");
        rules.push(canned(&["diff", "--name-only", &range, "--"], "a.txt\n"));
        rules.push(canned(
            &["diff", "--quiet", head, "origin/release/2.0", "--", "a.txt"],
            "",
        ));
        let runner = FakeGitRunner::new(rules);
        let record = check_worktree(&runner, &dir, &feature_entry(&dir), false, &opts);
        assert_eq!(record.merged_into_default, Some(false));
        assert!(record.absorbed_into_default);
        assert!(has(&record, ReasonCode::WtAbsorbed));
        assert!(!has(&record, ReasonCode::WtUnmerged));
        assert!(
            runner.called(&["cherry", "--", "origin/release/2.0", head]),
            "the remote-qualified name must be asked"
        );
        assert!(
            !runner.called(&["cherry", "--", "release/2.0", head]),
            "the bare name must not be asked before the remote one"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_squash_merged_branch_is_absorbed_rather_than_unmerged() {
        // `git cherry` printing only `-` lines means every commit has an equivalent
        // patch upstream: the change is in the default branch under a rewritten
        // commit, which is what GitHub's default merge strategy produces and what
        // `merge-base --is-ancestor` reports as "not merged".
        let dir = temp_dir("absorbed");
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
                "origin/main".to_string(),
            ],
            Canned::Out(GitOutput {
                stdout: String::new(),
                stderr: String::new(),
                code: Some(1),
            }),
        ));
        rules.push(canned(
            &["cherry", "--", "origin/main", head],
            "- 1111111111111111111111111111111111111111\n- 2222222222222222222222222222222222222222\n",
        ));
        // The patch is in the default branch, and so is the content: the confirmation
        // pass has to agree before the branch may be called absorbed.
        let range = format!("origin/main...{head}");
        rules.push(canned(&["diff", "--name-only", &range, "--"], "a.txt\n"));
        rules.push(canned(
            &["diff", "--quiet", head, "origin/main", "--", "a.txt"],
            "",
        ));
        let runner = FakeGitRunner::new(rules);
        let record = check_worktree(
            &runner,
            &dir,
            &feature_entry(&dir),
            false,
            &default_options(),
        );
        // The ancestry answer stays honest — it is not an ancestor — and the patch
        // answer is what carries the verdict.
        assert_eq!(record.merged_into_default, Some(false));
        assert!(record.absorbed_into_default);
        assert!(has(&record, ReasonCode::WtAbsorbed));
        assert!(!has(&record, ReasonCode::WtUnmerged));
        // `CLEAN_STATUS` carries no upstream, so this also pins that an absorbed
        // branch is not reported as stranded work.
        assert!(!has(&record, ReasonCode::WtNoUpstream));
        assert_eq!(record.safety.status, SafetyStatus::Safe);
        let reason = record
            .safety
            .reasons
            .iter()
            .find(|reason| reason.code == ReasonCode::WtAbsorbed)
            .expect("absorbed reason");
        assert!(
            reason.evidence.contains("2 commit(s)") && reason.evidence.contains("origin/main"),
            "the evidence must count the patches and name the ref: {}",
            reason.evidence
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_content_change_after_the_merge_is_review_but_the_patch_is_absorbed() {
        // The hole that `git cherry` alone leaves open: the commit that applied the
        // change still matches its patch-id after the content changed, so the patch
        // answer is still "absorbed" while the default branch no longer holds the
        // same content. The patch reason keeps its original meaning; a second reason
        // at Review carries the content answer and keeps the verdict out of Safe. It
        // must not claim a revert: a later revert and an ordinary edit look the same
        // to a tree comparison.
        let dir = temp_dir("absorbed-then-changed");
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
                "origin/main".to_string(),
            ],
            Canned::Out(GitOutput {
                stdout: String::new(),
                stderr: String::new(),
                code: Some(1),
            }),
        ));
        rules.push(canned(
            &["cherry", "--", "origin/main", head],
            "- 1111111111111111111111111111111111111111\n",
        ));
        let range = format!("origin/main...{head}");
        rules.push(canned(&["diff", "--name-only", &range, "--"], "a.txt\n"));
        rules.push((
            vec![
                "diff".to_string(),
                "--quiet".to_string(),
                head.to_string(),
                "origin/main".to_string(),
                "--".to_string(),
                "a.txt".to_string(),
            ],
            Canned::Out(GitOutput {
                stdout: String::new(),
                stderr: String::new(),
                // 1 = "they differ": the default branch does not hold this content.
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
        assert_eq!(record.merged_into_default, Some(false));
        assert!(
            record.absorbed_into_default,
            "the patch-id is upstream; the content answer is separate"
        );
        assert!(has(&record, ReasonCode::WtAbsorbed));
        assert!(has(&record, ReasonCode::WtAbsorbedContentChanged));
        assert!(!has(&record, ReasonCode::WtUnmerged));
        assert_eq!(record.safety.status, SafetyStatus::Review);
        let reason = record
            .safety
            .reasons
            .iter()
            .find(|reason| reason.code == ReasonCode::WtAbsorbedContentChanged)
            .expect("content-changed reason");
        assert!(
            reason.evidence.contains("equivalent patch")
                && reason.evidence.contains("changed after the merge"),
            "the evidence must show both halves: {}",
            reason.evidence
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn one_unabsorbed_commit_keeps_the_branch_unmerged() {
        // A single `+` line means work that exists only here. The verdict must stay
        // `Review`, and the code must not claim the branch was absorbed.
        let dir = temp_dir("partly-absorbed");
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
                "origin/main".to_string(),
            ],
            Canned::Out(GitOutput {
                stdout: String::new(),
                stderr: String::new(),
                code: Some(1),
            }),
        ));
        rules.push(canned(
            &["cherry", "--", "origin/main", head],
            "- 1111111111111111111111111111111111111111\n+ 3333333333333333333333333333333333333333\n",
        ));
        let runner = FakeGitRunner::new(rules);
        let record = check_worktree(
            &runner,
            &dir,
            &feature_entry(&dir),
            false,
            &default_options(),
        );
        assert_eq!(record.merged_into_default, Some(false));
        assert!(!record.absorbed_into_default);
        assert!(has(&record, ReasonCode::WtUnmerged));
        assert!(!has(&record, ReasonCode::WtAbsorbed));
        assert_eq!(record.safety.status, SafetyStatus::Review);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_failing_patch_check_never_upgrades_a_branch_to_absorbed() {
        // `git cherry` failing answers nothing, and an unanswerable check must not
        // make a worktree look safer than the ancestry answer alone did (D3).
        let dir = temp_dir("cherry-failed");
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
                "origin/main".to_string(),
            ],
            Canned::Out(GitOutput {
                stdout: String::new(),
                stderr: String::new(),
                code: Some(1),
            }),
        ));
        rules.push((
            vec![
                "cherry".to_string(),
                "--".to_string(),
                "origin/main".to_string(),
                head.to_string(),
            ],
            Canned::Failed {
                code: Some(128),
                stderr: "fatal: bad revision\n".to_string(),
            },
        ));
        let runner = FakeGitRunner::new(rules);
        let record = check_worktree(
            &runner,
            &dir,
            &feature_entry(&dir),
            false,
            &default_options(),
        );
        assert_eq!(record.merged_into_default, Some(false));
        assert!(!record.absorbed_into_default);
        assert!(has(&record, ReasonCode::WtUnmerged));
        assert!(!has(&record, ReasonCode::WtAbsorbed));
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
                "origin/main".to_string(),
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
    fn a_repository_stash_count_is_a_fact_and_never_a_reason() {
        // The regression this replaces: `refs/stash` lives in the common Git directory,
        // so every linked worktree answered with the same list. A repository holding a
        // single stash therefore downgraded *all* of its clean worktrees to Review —
        // 48 of 48 clean-and-merged worktrees in a 109-worktree repository — even
        // though `git worktree remove` never touches a stash. The count is now measured
        // once per scan and carried as a fact: it must not reach the reason list, and no
        // per-worktree stash command may run.
        let dir = temp_dir("stash");
        backdate(&dir, 30 * 86_400);
        let mut opts = default_options();
        opts.repository_stash_count = Some(2);
        let runner = FakeGitRunner::new(calm_rules(CLEAN_STATUS));
        let record = check_worktree(&runner, &dir, &feature_entry(&dir), false, &opts);
        assert_eq!(record.stash_count, 2);
        assert!(!has(&record, ReasonCode::WtStash));
        assert_eq!(record.safety.status, SafetyStatus::Safe);
        assert!(!runner.called(&["stash", "list", "--format=%gd"]));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_recent_head_commit_fires_recent_activity_when_the_work_is_not_in_the_default_branch() {
        // The directory is 30 days old; only the commit is fresh. The activity signal
        // is the HEAD commit time, so this must fire while the branch is unmerged.
        let dir = temp_dir("recent");
        backdate(&dir, 30 * 86_400);
        let mut opts = default_options();
        opts.now = SystemTime::now();
        let recent_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("after epoch")
            .as_secs()
            - 2 * 86_400;
        let mut rules = not_merged_rules(CLEAN_STATUS);
        for rule in &mut rules {
            if rule.0
                == vec![
                    "log".to_string(),
                    "-1".to_string(),
                    "--format=%ct".to_string(),
                ]
            {
                if let Canned::Out(output) = &mut rule.1 {
                    output.stdout = format!("{recent_secs}\n");
                }
            }
        }
        let runner = FakeGitRunner::new(rules);
        let record = check_worktree(&runner, &dir, &feature_entry(&dir), false, &opts);
        assert!(has(&record, ReasonCode::WtRecentActivity));
        assert_eq!(record.safety.status, SafetyStatus::Review);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_clean_merged_recent_commit_is_attention_not_a_verdict() {
        // The work is already in the default branch and the worktree is clean, so
        // the only thing a recent commit says is "used recently". The timestamp
        // remains on the record for the detail panel; the safety level must not
        // drop because of it.
        let dir = temp_dir("recent-merged");
        backdate(&dir, 30 * 86_400);
        let mut opts = default_options();
        opts.now = SystemTime::now();
        let recent_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("after epoch")
            .as_secs()
            - 2 * 86_400;
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
                    output.stdout = format!("{recent_secs}\n");
                }
            }
        }
        let runner = FakeGitRunner::new(rules);
        let record = check_worktree(&runner, &dir, &feature_entry(&dir), false, &opts);
        assert!(!has(&record, ReasonCode::WtRecentActivity));
        assert!(record.last_updated_at.is_some());
        assert_eq!(record.safety.status, SafetyStatus::Safe);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_touched_directory_with_an_old_commit_is_not_recent() {
        // The false positive this replaces: a bulk touch (indexer, backup, cleanup
        // script) refreshes the top-level directory mtime while the last commit is
        // months old. Measured in the wild, one such touch downgraded 22 otherwise
        // clean-and-merged worktrees. The mtime is no longer part of the signal, so
        // the verdict must stay Safe.
        let dir = temp_dir("touched");
        // No `backdate`: the directory was just created, so its mtime is "now".
        let mut opts = default_options();
        opts.now = SystemTime::now();
        let runner = FakeGitRunner::new(calm_rules(CLEAN_STATUS)); // the commit is from 2023
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
            "origin/main"
        ]));

        let mut rules = calm_rules(CLEAN_STATUS);
        rules.pop();
        rules.push((
            vec![
                "merge-base".to_string(),
                "--is-ancestor".to_string(),
                "--".to_string(),
                "19402c62a2bdb19e2271df95ecfc1a272aad4bf6".to_string(),
                "origin/main".to_string(),
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
    fn a_permission_error_on_the_commit_time_yields_review_with_cannot_verify() {
        let dir = temp_dir("permission");
        backdate(&dir, 30 * 86_400);
        let mut rules = calm_rules(CLEAN_STATUS);
        rules[1] = failing(
            &["log", "-1", "--format=%ct"],
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
        // is still Review because the commit-time check failed.
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
                "origin/main".to_string(),
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
        // A transport error on the first candidate ends the search: the second name
        // cannot fix a spawn failure, so it must never be tried.
        assert!(!runner.called(&["merge-base", "--is-ancestor", "--", head, "main"]));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn merge_check_falls_back_to_the_local_name() {
        // The candidate order is `origin/<name>` then `<name>`, so the fallback that
        // matters is the local one: a repository with no remote has no
        // `refs/remotes/origin/develop`, and `git merge-base` answers that with exit
        // 128 rather than a verdict. Without the second candidate every healthy
        // worktree of such a repository would degrade to cannot_verify.
        let dir = temp_dir("merge-fallback");
        backdate(&dir, 30 * 86_400);
        let head = "19402c62a2bdb19e2271df95ecfc1a272aad4bf6";
        let mut opts = default_options();
        opts.default_branch = Some("develop".to_string());
        let runner = FakeGitRunner::new(vec![
            canned(&["status", "--porcelain=v2", "--branch"], CLEAN_STATUS),
            canned(&["log", "-1", "--format=%ct"], "1700000000\n"),
            canned(
                &["rev-parse", "--git-path", "index.lock"],
                "/nonexistent/.git/index.lock\n",
            ),
            // The remote-tracking name exits 128: git *ran* and answered "no such
            // ref" (an `Ok` with a non-zero code, which is what `ProcessGitRunner`
            // returns for a command that failed — a transport-level `Err` would
            // instead short-circuit the candidate loop).
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
                    stderr: "fatal: not a valid ref: origin/develop\n".to_string(),
                    code: Some(128),
                }),
            ),
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
            reason.evidence.contains("ancestor of develop"),
            "the evidence must name the candidate that answered: {}",
            reason.evidence
        );
        // Both candidates were tried, the remote-tracking name first.
        assert!(runner.called(&["merge-base", "--is-ancestor", "--", head, "origin/develop"]));
        assert!(runner.called(&["merge-base", "--is-ancestor", "--", head, "develop"]));
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
        // gone, so "0 commits not pushed" would be a lie. The missing ref only
        // matters while the work is not in the default branch.
        let dir = temp_dir("upstream-without-ab");
        backdate(&dir, 30 * 86_400);

        // Unmerged: ahead/behind are unknown, so the verdict stays Review; the
        // clean file counts are still read and reported.
        let runner = FakeGitRunner::new(not_merged_rules(UPSTREAM_WITHOUT_AB));
        let record = check_worktree(
            &runner,
            &dir,
            &feature_entry(&dir),
            false,
            &default_options(),
        );
        let reason = record
            .safety
            .reasons
            .iter()
            .find(|reason| reason.code == ReasonCode::WtCannotVerify)
            .expect("cannot verify reason");
        assert!(reason.evidence.contains("upstream branch no longer exists"));
        assert!(has(&record, ReasonCode::WtClean));
        assert!(!has(&record, ReasonCode::WtUnpushedBranch));
        assert_eq!(record.ahead, 0);
        assert_eq!(record.safety.status, SafetyStatus::Review);

        // Merged: the deleted upstream cannot hide any work; the worktree is
        // clean and its commit is in the default branch, so it is Safe.
        let runner = FakeGitRunner::new(calm_rules(UPSTREAM_WITHOUT_AB));
        let record = check_worktree(
            &runner,
            &dir,
            &feature_entry(&dir),
            false,
            &default_options(),
        );
        assert!(!has(&record, ReasonCode::WtCannotVerify));
        assert_eq!(record.safety.status, SafetyStatus::Safe);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_dirty_worktree_with_a_deleted_upstream_is_still_unsafe() {
        // The regression this pins: the old code skipped the whole status block
        // when `# branch.ab` was absent, so a dirty worktree was reported as a
        // Review `cannot_verify` with `dirty_files: 0`. The file counts are not
        // unknown just because ahead/behind are, and uncommitted changes are the
        // one thing the worktree deletion would actually destroy.
        let dir = temp_dir("dirty-upstream-without-ab");
        backdate(&dir, 30 * 86_400);
        let runner = FakeGitRunner::new(calm_rules(DIRTY_UPSTREAM_WITHOUT_AB));
        let record = check_worktree(
            &runner,
            &dir,
            &feature_entry(&dir),
            false,
            &default_options(),
        );
        assert!(has(&record, ReasonCode::WtDirty));
        assert_eq!(record.dirty_files, 1);
        assert!(!has(&record, ReasonCode::WtClean));
        assert_eq!(record.safety.status, SafetyStatus::Unsafe);
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
        // there is no second signal to fall back on, so the timestamp stays empty
        // and the failure is visible.
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
            record.last_updated_at.is_none(),
            "no signal, no timestamp: an unparseable commit time must not invent one"
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

        // -- log fails: no second signal exists, so the timestamp stays empty and
        //    the verdict is held at Review by wt.cannot_verify
        let mut rules = calm_rules(CLEAN_STATUS);
        rules[1] = failing(
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
        assert!(record.last_updated_at.is_none());
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
    fn a_credential_in_the_worktree_blocks_removal() {
        // `git worktree remove` deletes ignored files, so a `.env` at the root of
        // an otherwise clean worktree is exactly the file the Safe verdict must
        // not miss. The artifact channel had `af.credentials`; this is the
        // worktree-level counterpart.
        let dir = temp_dir("worktree-credential");
        backdate(&dir, 30 * 86_400);
        std::fs::write(dir.join(".env"), "TOKEN=secret\n").expect("write .env");
        let mut rules = calm_rules(CLEAN_STATUS);
        rules.push(not_tracked(&["ls-files", "--error-unmatch", "--", ".env"]));
        let runner = FakeGitRunner::new(rules);
        let record = check_worktree(
            &runner,
            &dir,
            &feature_entry(&dir),
            false,
            &default_options(),
        );
        assert!(has(&record, ReasonCode::WtCredentials));
        assert_eq!(record.safety.status, SafetyStatus::Unsafe);
        let reason = record
            .safety
            .reasons
            .iter()
            .find(|reason| reason.code == ReasonCode::WtCredentials)
            .expect("credential reason");
        assert!(
            reason.evidence.contains(".env"),
            "the evidence must name the file: {}",
            reason.evidence
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_tracked_credential_template_is_not_a_worktree_loss() {
        // `.env.example` is matched by the credential table on purpose: a template
        // filled in where it stood is a mistake. But a tracked template is already
        // in Git, so deleting the worktree loses nothing, and blocking on it would
        // make every repository that commits one Unsafe.
        let dir = temp_dir("worktree-credential-template");
        backdate(&dir, 30 * 86_400);
        std::fs::write(dir.join(".env.example"), "TOKEN=\n").expect("write template");
        let mut rules = calm_rules(CLEAN_STATUS);
        rules.push(canned(
            &["ls-files", "--error-unmatch", "--", ".env.example"],
            ".env.example\n",
        ));
        let runner = FakeGitRunner::new(rules);
        let record = check_worktree(
            &runner,
            &dir,
            &feature_entry(&dir),
            false,
            &default_options(),
        );
        assert!(!has(&record, ReasonCode::WtCredentials));
        assert_eq!(record.safety.status, SafetyStatus::Safe);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_untracked_credential_is_unsafe_not_only_review() {
        // An untracked `.env` is a Review under `wt.untracked`, which the user can
        // force through. The credential check must run anyway and make it Unsafe.
        let dir = temp_dir("worktree-credential-untracked");
        backdate(&dir, 30 * 86_400);
        std::fs::write(dir.join(".env"), "TOKEN=secret\n").expect("write .env");
        let status = format!("{CLEAN_STATUS}? .env\n");
        let mut rules = calm_rules(&status);
        rules.push(not_tracked(&["ls-files", "--error-unmatch", "--", ".env"]));
        let runner = FakeGitRunner::new(rules);
        let record = check_worktree(
            &runner,
            &dir,
            &feature_entry(&dir),
            false,
            &default_options(),
        );
        assert!(has(&record, ReasonCode::WtUntracked));
        assert!(has(&record, ReasonCode::WtCredentials));
        assert_eq!(record.safety.status, SafetyStatus::Unsafe);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_credential_inside_a_cache_directory_is_left_to_the_artifact_scan() {
        // The walk skips recognized build and cache directories: they are artifact
        // candidates, and the artifact scanner checks them for credentials. Walking
        // every `node_modules` here would make the worktree verdict pay for the
        // dependency tree twice.
        let dir = temp_dir("worktree-credential-cache");
        backdate(&dir, 30 * 86_400);
        let cache = dir.join("node_modules");
        std::fs::create_dir_all(&cache).expect("create node_modules");
        std::fs::write(cache.join(".env"), "TOKEN=secret\n").expect("write .env");
        let runner = FakeGitRunner::new(calm_rules(CLEAN_STATUS));
        let record = check_worktree(
            &runner,
            &dir,
            &feature_entry(&dir),
            false,
            &default_options(),
        );
        assert!(!has(&record, ReasonCode::WtCredentials));
        assert_eq!(record.safety.status, SafetyStatus::Safe);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_dependency_checkout_is_not_scanned_for_worktree_credentials() {
        // The measured false positive: a Sparkle test public key lives under
        // `.build/checkouts/.../Tests/Resources/test-pubkey.pem` in most Swift
        // worktrees. It is a reproducible dependency fixture, not a secret that
        // the worktree deletion would lose.
        let dir = temp_dir("worktree-credential-dependency");
        backdate(&dir, 30 * 86_400);
        let checkout = dir.join(".build/checkouts/Sparkle/Tests/Resources");
        std::fs::create_dir_all(&checkout).expect("create checkout");
        std::fs::write(checkout.join("test-pubkey.pem"), "public test key\n")
            .expect("write test key");
        let runner = FakeGitRunner::new(calm_rules(CLEAN_STATUS));
        let record = check_worktree(
            &runner,
            &dir,
            &feature_entry(&dir),
            false,
            &default_options(),
        );
        assert!(!has(&record, ReasonCode::WtCredentials));
        assert_eq!(record.safety.status, SafetyStatus::Safe);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_truncated_worktree_credential_walk_is_review_not_a_pass() {
        // A partial walk cannot claim the worktree is clean. The bound is time, so
        // a directory large enough to outlast a zero budget is the fixture.
        let dir = temp_dir("worktree-credential-truncated");
        backdate(&dir, 30 * 86_400);
        for index in 0..2_000 {
            std::fs::write(dir.join(format!("f{index}.txt")), "x").expect("write");
        }
        let mut opts = default_options();
        opts.credential_scan_timeout = Duration::ZERO;
        let runner = FakeGitRunner::new(calm_rules(CLEAN_STATUS));
        let record = check_worktree(&runner, &dir, &feature_entry(&dir), false, &opts);
        assert!(has(&record, ReasonCode::WtCannotVerify));
        assert_eq!(record.safety.status, SafetyStatus::Review);
        let reason = record
            .safety
            .reasons
            .iter()
            .find(|reason| reason.code == ReasonCode::WtCannotVerify)
            .expect("cannot verify reason");
        assert!(
            reason.evidence.contains("time limit"),
            "the evidence must say the walk was partial: {}",
            reason.evidence
        );
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
                "origin/main".to_string(),
            ],
            Canned::Out(GitOutput {
                stdout: String::new(),
                stderr: String::new(),
                code: Some(1),
            }),
        ));
        // ls-files fails, which is what puts wt.cannot_verify into the set (the folded
        // status count still carries wt.untracked). The log call must succeed with a
        // recent commit time: recent_activity now tracks the HEAD commit alone, so a
        // failing log could no longer let it appear beside wt.cannot_verify.
        let recent_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("after epoch")
            .as_secs()
            - 2 * 86_400;
        for rule in &mut rules {
            if rule.0
                == vec![
                    "log".to_string(),
                    "-1".to_string(),
                    "--format=%ct".to_string(),
                ]
            {
                if let Canned::Out(output) = &mut rule.1 {
                    output.stdout = format!("{recent_secs}\n");
                }
            }
        }
        rules.push(failing(
            &["ls-files", "--others", "--exclude-standard"],
            Some(128),
            "fatal: bad repository",
        ));
        // The lock file the rev-parse call must report.
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
        let record = check_worktree(&runner, &dir, &entry, true, &opts);
        assert_eq!(
            codes(&record),
            vec![
                "wt.main",
                "wt.locked",
                "wt.whitelisted",
                "wt.dirty",
                "wt.cannot_verify",
                "wt.untracked",
                "wt.agent_active",
                "wt.unmerged",
                "wt.recent_activity",
                "wt.unpushed_branch",
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
