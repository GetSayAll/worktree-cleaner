//! In-process Git transport backed by libgit2.
//!
//! The decision logic in this crate talks to Git through [`GitRunner`]. This
//! implementation answers the same calls without spawning a process: it opens the
//! repository with libgit2, performs the operation, and renders the small part of
//! Git's command output that the parsers consume.
//!
//! Why a compatibility layer rather than a typed backend: the parsers and their
//! tests are the contract, and keeping them while replacing the transport makes the
//! migration reviewable. A later refactor can replace the raw argument interface with
//! typed operations; this module is where that change would start.
//!
//! What it deliberately does not do:
//!
//! - read `~/.gitconfig` or `/etc/gitconfig`: the global and system search paths
//!   point at an empty directory, both for determinism and because a sandboxed app
//!   cannot read them;
//! - run hooks, filters, external diff tools, or textconv: libgit2 does not run
//!   them, so a repository cannot execute code through a scan;
//! - use the network: the vendored build has no HTTP/SSH transport enabled.
//!
//! `git worktree remove` is the one operation libgit2 does not expose: it can add,
//! list, lock and prune, but not remove a linked worktree. gix (gitoxide) implements
//! safe recursive removal in-process, so the worktree-removal command is served by
//! that crate rather than by a child process.

use std::fmt::Write as _;
use std::path::Path;
use std::sync::mpsc;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use git2::{
    BranchType, ConfigLevel, DiffOptions, Oid, Repository, Status, StatusOptions,
    WorktreeLockStatus,
};

use crate::git::{display_command, GitError, GitOutput, GitRunner};

/// Runs Git operations in-process through libgit2.
#[derive(Debug, Clone)]
pub struct Git2Runner {
    timeout: Duration,
}

impl Git2Runner {
    /// Creates a runner with the same per-command budget as the process runner.
    pub fn new(timeout: Duration) -> Self {
        configure_libgit2();
        Self { timeout }
    }
}

impl Default for Git2Runner {
    fn default() -> Self {
        Self::new(crate::git::DEFAULT_GIT_TIMEOUT)
    }
}

impl GitRunner for Git2Runner {
    fn run(&self, repo: &Path, args: &[&str]) -> Result<GitOutput, GitError> {
        let command = display_command(repo, args);
        let is_write = matches!(args.first(), Some(&"worktree"))
            && matches!(args.get(1), Some(&"remove" | &"prune"));
        if is_write {
            // A timed-out removal cannot be cancelled: the operation would keep
            // running after we returned "timeout" and the next scan would race it.
            // A slow removal is the lesser evil, so write operations run to
            // completion on the caller's blocking thread.
            return run_once(repo, args, &command);
        }

        let repo_path = repo.to_path_buf();
        let owned_args: Vec<String> = args.iter().map(|arg| (*arg).to_string()).collect();
        let timeout = self.timeout;
        let command_for_thread = command.clone();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let arg_refs: Vec<&str> = owned_args.iter().map(String::as_str).collect();
            let _ = tx.send(run_once(&repo_path, &arg_refs, &command_for_thread));
        });
        match rx.recv_timeout(timeout) {
            Ok(result) => result,
            Err(_) => Err(GitError::Timeout {
                command,
                seconds: timeout.as_secs().max(1),
            }),
        }
    }
}

/// libgit2 reads its configuration from process-wide search paths. Set them once
/// to an empty directory so no scan depends on the developer's `~/.gitconfig`.
///
/// `set_search_path` is unsafe because it mutates process-global state. It is
/// called exactly once, before any repository is opened, from `Git2Runner::new`.
/// The directory lives for the process; libgit2 copies the path it needs.
#[allow(unsafe_code)]
fn configure_libgit2() {
    static ONCE: OnceLock<()> = OnceLock::new();
    ONCE.get_or_init(|| {
        let empty = std::env::temp_dir().join("worktree-cleaner-empty-git-config");
        let _ = std::fs::create_dir_all(&empty);
        unsafe {
            let _ = git2::opts::set_search_path(ConfigLevel::System, &empty);
            let _ = git2::opts::set_search_path(ConfigLevel::Global, &empty);
        }
    });
}

fn run_once(repo_path: &Path, args: &[&str], command: &str) -> Result<GitOutput, GitError> {
    if args == ["--version"] {
        return Ok(out("git version 2 (libgit2)\n"));
    }
    // A repository that cannot be opened is a command-level failure (Git exits
    // 128), not a transport error: `scan_project` turns it into
    // `NotAGitRepository`, and the per-check callers turn it into
    // `cannot_verify`, exactly as they did for the CLI.
    let mut repo = match Repository::open(repo_path) {
        Ok(repo) => repo,
        Err(error) => {
            return Ok(GitOutput {
                stdout: String::new(),
                stderr: format!("cannot open the repository: {error}"),
                code: Some(128),
            });
        }
    };
    let deadline = Instant::now() + Duration::from_secs(10);

    match args {
        ["worktree", "list", "--porcelain", "-z"] => worktree_list(&repo),
        ["status", "--porcelain=v2", "--branch"] => status_v2(&repo, deadline),
        ["status", "--porcelain", "--untracked-files=all", "--", path] => {
            status_v1_path(&repo, path)
        }
        ["ls-files", "--", path] => ls_files_path(&repo, path),
        ["ls-files", "--others", "--exclude-standard"] => ls_files_others(&repo),
        ["ls-files", "--error-unmatch", "--", path] => ls_files_error_unmatch(&repo, path),
        ["check-ignore", "-v", "--", path]
        | ["check-ignore", "-v", "-n", "--no-index", "--", path] => check_ignore(
            &repo,
            path,
            args.contains(&"-n"),
            args.contains(&"--no-index"),
        ),
        ["log", "-1", "--format=%ct"] => head_commit_time(&repo),
        ["rev-parse", "--git-path", "index.lock"] => Ok(git_path(&repo)),
        ["rev-parse", "--show-toplevel"] => Ok(show_toplevel(&repo)),
        ["rev-parse", "--verify", "--quiet", reference] => Ok(verify_reference(&repo, reference)),
        ["symbolic-ref", "--quiet", "refs/remotes/origin/HEAD"] => Ok(origin_head(&repo)),
        ["merge-base", "--is-ancestor", "--", head, candidate] => {
            is_ancestor(&repo, head, candidate)
        }
        ["diff", "--name-only", range, "--"] => diff_name_only(&repo, range),
        ["diff", "--quiet", head, target, "--", paths @ ..] => {
            diff_quiet(&repo, head, target, paths)
        }
        ["stash", "list", "--format=%gd"] => stash_list(&mut repo),
        ["worktree", "remove", rest @ ..] => worktree_remove(repo_path, rest),
        ["worktree", "prune"] => worktree_prune(&repo),
        _ => Err(failed(
            command,
            format!("unsupported Git operation: {}", args.join(" ")),
        )),
    }
}

// ---------------------------------------------------------------- worktrees

fn worktree_list(repo: &Repository) -> Result<GitOutput, GitError> {
    let mut output = String::new();
    let (main_head, main_branch, main_detached) = head_facts(repo);
    let main_path = repo
        .workdir()
        .map_or_else(|| repo.path().to_path_buf(), Path::to_path_buf);
    push_worktree(
        &mut output,
        &main_path,
        main_head.as_deref(),
        main_branch.as_deref(),
        repo.is_bare(),
        main_detached,
        None,
        None,
    );

    let names = repo.worktrees().map_err(|error| {
        failed(
            "git worktree list",
            format!("cannot list worktrees: {error}"),
        )
    })?;
    for name in names.iter().flatten() {
        let worktree = repo
            .find_worktree(name)
            .map_err(|error| failed("git worktree list", format!("cannot read {name}: {error}")))?;
        let (head, branch, detached) = worktree_head_facts(&worktree);
        let locked = match worktree.is_locked() {
            Ok(WorktreeLockStatus::Locked(reason)) => Some(reason.unwrap_or_default()),
            Ok(WorktreeLockStatus::Unlocked) | Err(_) => None,
        };
        let prunable = worktree.is_prunable(None).unwrap_or(false);
        push_worktree(
            &mut output,
            worktree.path(),
            head.as_deref(),
            branch.as_deref(),
            false,
            detached,
            locked.as_deref(),
            prunable.then_some("the working directory is gone"),
        );
    }

    Ok(out(output))
}

fn worktree_head_facts(worktree: &git2::Worktree) -> (Option<String>, Option<String>, bool) {
    let Ok(repo) = Repository::open_from_worktree(worktree) else {
        return (None, None, false);
    };
    head_facts(&repo)
}

fn head_facts(repo: &Repository) -> (Option<String>, Option<String>, bool) {
    let head = repo.head().ok();
    let oid = head
        .as_ref()
        .and_then(|head| head.peel_to_commit().ok())
        .map(|commit| commit.id().to_string());
    let branch = repo
        .find_reference("HEAD")
        .ok()
        .and_then(|reference| reference.symbolic_target().map(short_branch));
    let detached = oid.is_some() && branch.is_none();
    (oid, branch, detached)
}

#[allow(clippy::too_many_arguments)]
fn push_worktree(
    output: &mut String,
    path: &Path,
    head: Option<&str>,
    branch: Option<&str>,
    bare: bool,
    detached: bool,
    locked: Option<&str>,
    prunable: Option<&str>,
) {
    output.push_str("worktree ");
    output.push_str(&path.to_string_lossy());
    output.push('\0');
    if let Some(head) = head {
        output.push_str("HEAD ");
        output.push_str(head);
        output.push('\0');
    }
    if let Some(branch) = branch {
        output.push_str("branch refs/heads/");
        output.push_str(branch);
        output.push('\0');
    } else if detached {
        output.push_str("detached\0");
    }
    if bare {
        output.push_str("bare\0");
    }
    if let Some(reason) = locked {
        if reason.is_empty() {
            output.push_str("locked\0");
        } else {
            output.push_str("locked ");
            output.push_str(reason);
            output.push('\0');
        }
    }
    if let Some(reason) = prunable {
        output.push_str("prunable ");
        output.push_str(reason);
        output.push('\0');
    }
}

fn worktree_remove(repo_path: &Path, args: &[&str]) -> Result<GitOutput, GitError> {
    let Some(path) = args.iter().rev().find(|arg| **arg != "--") else {
        return Err(failed(
            "git worktree remove",
            "no worktree path was given".to_string(),
        ));
    };
    let force = if args.contains(&"--force") {
        gix::worktree::remove::Force::DiscardChanges
    } else {
        gix::worktree::remove::Force::Never
    };
    let repo = gix::open(repo_path).map_err(|error| {
        failed(
            "git worktree remove",
            format!("cannot open the repository: {error}"),
        )
    })?;
    let target = repo.prepare_remove_worktree(path).map_err(|error| {
        failed(
            "git worktree remove",
            format!("cannot resolve the worktree: {error}"),
        )
    })?;
    target
        .remove(force, gix::progress::Discard)
        .map_err(|error| {
            failed(
                "git worktree remove",
                format!("cannot remove the worktree: {error}"),
            )
        })?;
    Ok(out(""))
}

fn worktree_prune(repo: &Repository) -> Result<GitOutput, GitError> {
    let names = repo.worktrees().map_err(|error| {
        failed(
            "git worktree prune",
            format!("cannot list worktrees: {error}"),
        )
    })?;
    for name in names.iter().flatten() {
        let worktree = repo.find_worktree(name).map_err(|error| {
            failed("git worktree prune", format!("cannot read {name}: {error}"))
        })?;
        let prunable = worktree.is_prunable(None).map_err(|error| {
            failed(
                "git worktree prune",
                format!("cannot inspect {name}: {error}"),
            )
        })?;
        if prunable {
            worktree
                .prune(None)
                .map_err(|error| failed("git worktree prune", error.to_string()))?;
        }
    }
    Ok(out(""))
}

// ------------------------------------------------------------------- status

fn status_v2(repo: &Repository, deadline: Instant) -> Result<GitOutput, GitError> {
    let (head, branch, detached) = head_facts(repo);
    let mut output = String::new();
    output.push_str("# branch.oid ");
    output.push_str(head.as_deref().unwrap_or("(initial)"));
    output.push('\n');
    output.push_str("# branch.head ");
    output.push_str(
        branch
            .as_deref()
            .unwrap_or(if detached { "(detached)" } else { "" }),
    );
    output.push('\n');

    if let (Some(branch_name), Some(head_oid)) = (branch.as_deref(), head.as_deref()) {
        if let Ok(local) = repo.find_branch(branch_name, BranchType::Local) {
            if let Ok(upstream) = local.upstream() {
                if let Some(name) = upstream.name().ok().flatten() {
                    output.push_str("# branch.upstream ");
                    output.push_str(name);
                    output.push('\n');
                    if let (Ok(head_oid), Some(upstream_oid)) =
                        (Oid::from_str(head_oid), upstream.get().target())
                    {
                        if let Ok((ahead, behind)) = repo.graph_ahead_behind(head_oid, upstream_oid)
                        {
                            let _ = writeln!(output, "# branch.ab +{ahead} -{behind}");
                        }
                    }
                }
            }
        }
    }

    let mut options = StatusOptions::new();
    options
        .include_untracked(true)
        .recurse_untracked_dirs(false)
        .include_ignored(false)
        .renames_head_to_index(true)
        .renames_index_to_workdir(true)
        .no_refresh(true);
    let statuses = repo
        .statuses(Some(&mut options))
        .map_err(|error| failed("git status", format!("cannot read the status: {error}")))?;
    for entry in statuses.iter() {
        if Instant::now() > deadline {
            return Err(GitError::Timeout {
                command: "git status --porcelain=v2 --branch".to_string(),
                seconds: 10,
            });
        }
        let Some(path) = entry.path() else {
            continue;
        };
        if entry.status().is_ignored() {
            continue;
        }
        if is_untracked(entry.status()) {
            output.push_str("? ");
        } else if entry.status().is_conflicted() {
            output.push_str("u ");
        } else if entry.status().is_index_renamed() || entry.status().is_wt_renamed() {
            output.push_str("2 ");
        } else if has_tracked_change(entry.status()) {
            output.push_str("1 ");
        } else {
            continue;
        }
        output.push_str(path);
        output.push('\n');
    }
    Ok(out(output))
}

fn status_v1_path(repo: &Repository, path: &str) -> Result<GitOutput, GitError> {
    let relative = repo_relative(repo, path)?;
    let mut options = StatusOptions::new();
    options
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_ignored(false)
        .no_refresh(true)
        .pathspec(relative);
    let statuses = repo
        .statuses(Some(&mut options))
        .map_err(|error| failed("git status", format!("cannot read the status: {error}")))?;
    let mut output = String::new();
    for entry in statuses.iter() {
        if entry.status().is_ignored() || !is_untracked(entry.status()) {
            continue;
        }
        if let Some(path) = entry.path() {
            output.push_str("?? ");
            output.push_str(path);
            output.push('\n');
        }
    }
    Ok(out(output))
}

fn is_untracked(status: Status) -> bool {
    status.is_wt_new() && !status.is_index_new()
}

fn has_tracked_change(status: Status) -> bool {
    status.is_index_new()
        || status.is_index_modified()
        || status.is_index_deleted()
        || status.is_index_renamed()
        || status.is_index_typechange()
        || status.is_wt_modified()
        || status.is_wt_deleted()
        || status.is_wt_renamed()
        || status.is_wt_typechange()
        || status.is_conflicted()
}

fn ls_files_path(repo: &Repository, path: &str) -> Result<GitOutput, GitError> {
    let relative = repo_relative(repo, path)?;
    let index = repo
        .index()
        .map_err(|error| failed("git ls-files", format!("cannot read the index: {error}")))?;
    let mut output = String::new();
    for entry in index.iter() {
        let entry_path = entry_path(&entry);
        if entry_path.is_empty() {
            continue;
        }
        if entry_path == relative
            || (!relative.is_empty() && entry_path.starts_with(&format!("{relative}/")))
        {
            output.push_str(&entry_path);
            output.push('\n');
        }
    }
    Ok(out(output))
}

fn ls_files_others(repo: &Repository) -> Result<GitOutput, GitError> {
    let mut options = StatusOptions::new();
    options
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_ignored(false)
        .no_refresh(true);
    let statuses = repo
        .statuses(Some(&mut options))
        .map_err(|error| failed("git ls-files", format!("cannot read the status: {error}")))?;
    let mut output = String::new();
    for entry in statuses.iter() {
        if entry.status().is_ignored() || !is_untracked(entry.status()) {
            continue;
        }
        if let Some(path) = entry.path() {
            output.push_str(path);
            output.push('\n');
        }
    }
    Ok(out(output))
}

fn ls_files_error_unmatch(repo: &Repository, path: &str) -> Result<GitOutput, GitError> {
    let relative = repo_relative(repo, path)?;
    let index = repo
        .index()
        .map_err(|error| failed("git ls-files", format!("cannot read the index: {error}")))?;
    if index.get_path(Path::new(&relative), 0).is_some() {
        Ok(out(format!("{relative}\n")))
    } else {
        Ok(GitOutput {
            stdout: String::new(),
            stderr: format!("error: pathspec '{path}' did not match any file(s) known to git\n"),
            code: Some(1),
        })
    }
}

fn check_ignore(
    repo: &Repository,
    path: &str,
    non_matching: bool,
    no_index: bool,
) -> Result<GitOutput, GitError> {
    let relative = repo_relative(repo, path)?;
    // Without `--no-index`, `git check-ignore` does not report a tracked path as
    // ignored; libgit2's rule query does not consult the index. Preserve the CLI
    // answer: the tracked `dist/` in the fixture is ignored by a pattern but is
    // not an ignored path.
    if !no_index {
        let index = repo.index().map_err(|error| {
            failed(
                "git check-ignore",
                format!("cannot read the index: {error}"),
            )
        })?;
        let tracked = index.iter().any(|entry| {
            let entry = entry_path(&entry);
            entry == relative
                || (!relative.is_empty() && entry.starts_with(&format!("{relative}/")))
        });
        if tracked {
            if non_matching {
                return Ok(out(format!("::\t{path}\n")));
            }
            return Ok(GitOutput {
                stdout: String::new(),
                stderr: String::new(),
                code: Some(1),
            });
        }
    }
    let ignored = repo
        .is_path_ignored(Path::new(&relative))
        .map_err(|error| {
            failed(
                "git check-ignore",
                format!("cannot read ignore rules: {error}"),
            )
        })?;
    if ignored {
        let rule = matching_rule(repo, &relative).map_or_else(
            || format!("gitignore:0:ignored\t{path}\n"),
            |(source, line, pattern)| format!("{source}:{line}:{pattern}\t{path}\n"),
        );
        return Ok(out(rule));
    }
    if non_matching {
        return Ok(out(format!("::\t{path}\n")));
    }
    Ok(GitOutput {
        stdout: String::new(),
        stderr: String::new(),
        code: Some(1),
    })
}

/// The rule that a person can act on: the repository's own `.gitignore`, then its
/// `.git/info/exclude`. A rule from a nested or global file is still reported by
/// the boolean above, but not attributed to a repository file it is not in.
fn matching_rule(repo: &Repository, relative: &str) -> Option<(String, usize, String)> {
    let workdir = repo.workdir()?;
    for (source, file) in [
        (".gitignore".to_string(), workdir.join(".gitignore")),
        (
            ".git/info/exclude".to_string(),
            repo.path().join("info").join("exclude"),
        ),
    ] {
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        for (index, raw) in text.lines().enumerate() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with('!') {
                continue;
            }
            if pattern_matches(line, relative) {
                return Some((source, index + 1, line.to_string()));
            }
        }
    }
    None
}

/// The policy asks for two exact anchored entries. Matching a pattern against the
/// first component of the path is enough for those entries and deliberately does
/// not turn this module into a second Gitignore engine: an unusual pattern still
/// has the boolean answer and is reported as `elsewhere`, never as met.
fn pattern_matches(pattern: &str, relative: &str) -> bool {
    let pattern = pattern.trim_end_matches('/');
    let pattern = pattern.trim_start_matches('/');
    let pattern = pattern.trim_start_matches("**/");
    let pattern = pattern.trim_end_matches("/**");
    if pattern.is_empty() {
        return false;
    }
    let first = relative.split('/').next().unwrap_or_default();
    pattern == relative || pattern == first
}

// --------------------------------------------------------------------- refs

fn head_commit_time(repo: &Repository) -> Result<GitOutput, GitError> {
    let commit = repo
        .head()
        .and_then(|head| head.peel_to_commit())
        .map_err(|error| failed("git log", format!("cannot read HEAD: {error}")))?;
    Ok(out(format!("{}\n", commit.time().seconds())))
}

fn git_path(repo: &Repository) -> GitOutput {
    out(format!(
        "{}\n",
        repo.path().join("index.lock").to_string_lossy()
    ))
}

fn show_toplevel(repo: &Repository) -> GitOutput {
    let path = repo
        .workdir()
        .map_or_else(|| repo.path().to_path_buf(), Path::to_path_buf);
    out(format!("{}\n", path.to_string_lossy()))
}

fn verify_reference(repo: &Repository, reference: &str) -> GitOutput {
    match repo.revparse_single(reference) {
        Ok(object) => out(format!("{}\n", object.id())),
        Err(_) => GitOutput {
            stdout: String::new(),
            stderr: String::new(),
            code: Some(1),
        },
    }
}

fn origin_head(repo: &Repository) -> GitOutput {
    match repo
        .find_reference("refs/remotes/origin/HEAD")
        .and_then(|reference| {
            reference
                .symbolic_target()
                .map(str::to_string)
                .ok_or_else(|| git2::Error::from_str("origin/HEAD is not symbolic"))
        }) {
        Ok(target) => out(format!("{target}\n")),
        Err(_) => GitOutput {
            stdout: String::new(),
            stderr: String::new(),
            code: Some(1),
        },
    }
}

fn is_ancestor(repo: &Repository, head: &str, candidate: &str) -> Result<GitOutput, GitError> {
    // A missing ref is a command-level answer (exit 128), not a transport error:
    // `merge_base_is_ancestor` tries `origin/<name>` first and then the bare name,
    // and it must see the 128 to try the next candidate.
    let resolve = |reference: &str| match repo.revparse_single(reference) {
        Ok(object) => Ok(object),
        Err(error) => Err(GitOutput {
            stdout: String::new(),
            stderr: format!("git merge-base: cannot resolve {reference}: {error}"),
            code: Some(128),
        }),
    };
    let head = match resolve(head) {
        Ok(object) => object,
        Err(output) => return Ok(output),
    };
    let candidate = match resolve(candidate) {
        Ok(object) => object,
        Err(output) => return Ok(output),
    };
    // `git merge-base --is-ancestor A A` exits 0; libgit2's graph query is a
    // strict-descendant question, so an equal commit has to be answered here.
    let ancestor = head.id() == candidate.id()
        || repo
            .graph_descendant_of(candidate.id(), head.id())
            .map_err(|error| {
                failed("git merge-base", format!("cannot compare commits: {error}"))
            })?;
    Ok(GitOutput {
        stdout: String::new(),
        stderr: String::new(),
        code: Some(i32::from(!ancestor)),
    })
}

// --------------------------------------------------------------------- diff

fn diff_name_only(repo: &Repository, range: &str) -> Result<GitOutput, GitError> {
    let (base, head) = resolve_range(repo, range)?;
    let base_tree = repo
        .find_commit(base)
        .and_then(|commit| commit.tree())
        .map_err(|error| failed("git diff", format!("cannot read the merge base: {error}")))?;
    let head_tree = repo
        .find_commit(head)
        .and_then(|commit| commit.tree())
        .map_err(|error| failed("git diff", format!("cannot read the branch: {error}")))?;
    let mut options = DiffOptions::new();
    options.include_typechange(true);
    let diff = repo
        .diff_tree_to_tree(Some(&base_tree), Some(&head_tree), Some(&mut options))
        .map_err(|error| failed("git diff", format!("cannot compare trees: {error}")))?;
    let mut output = String::new();
    for delta in diff.deltas() {
        if let Some(path) = delta.new_file().path() {
            output.push_str(&git_path_text(path));
            output.push('\n');
        }
    }
    Ok(out(output))
}

fn diff_quiet(
    repo: &Repository,
    head: &str,
    target: &str,
    paths: &[&str],
) -> Result<GitOutput, GitError> {
    let head = repo
        .revparse_single(head)
        .map_err(|error| failed("git diff", format!("cannot resolve {head}: {error}")))?;
    let target = repo
        .revparse_single(target)
        .map_err(|error| failed("git diff", format!("cannot resolve {target}: {error}")))?;
    let head_tree = head
        .peel_to_tree()
        .map_err(|error| failed("git diff", format!("cannot read the branch: {error}")))?;
    let target_tree = target
        .peel_to_tree()
        .map_err(|error| failed("git diff", format!("cannot read the target: {error}")))?;
    let mut options = DiffOptions::new();
    options.include_typechange(true);
    for path in paths {
        if !path.is_empty() {
            options.pathspec(*path);
        }
    }
    let diff = repo
        .diff_tree_to_tree(Some(&head_tree), Some(&target_tree), Some(&mut options))
        .map_err(|error| failed("git diff", format!("cannot compare trees: {error}")))?;
    Ok(GitOutput {
        stdout: String::new(),
        stderr: String::new(),
        code: Some(i32::from(diff.deltas().next().is_some())),
    })
}

fn resolve_range(repo: &Repository, range: &str) -> Result<(Oid, Oid), GitError> {
    let (target, head) = if let Some((target, head)) = range.split_once("...") {
        (target, head)
    } else if let Some((target, head)) = range.split_once("..") {
        (target, head)
    } else {
        return Err(failed("git diff", format!("not a commit range: {range}")));
    };
    let target = repo
        .revparse_single(target)
        .map_err(|error| failed("git diff", format!("cannot resolve {target}: {error}")))?;
    let head = repo
        .revparse_single(head)
        .map_err(|error| failed("git diff", format!("cannot resolve {head}: {error}")))?;
    let base = if range.contains("...") {
        repo.merge_base(target.id(), head.id())
            .map_err(|error| failed("git diff", format!("no merge base: {error}")))?
    } else {
        target.id()
    };
    Ok((base, head.id()))
}

// --------------------------------------------------------------------- stash

fn stash_list(repo: &mut Repository) -> Result<GitOutput, GitError> {
    let mut count = 0usize;
    repo.stash_foreach(|_, _, _| {
        count += 1;
        true
    })
    .map_err(|error| failed("git stash list", format!("cannot read stashes: {error}")))?;
    let mut output = String::new();
    for index in 0..count {
        let _ = writeln!(output, "stash@{{{index}}}");
    }
    Ok(out(output))
}

// ------------------------------------------------------------------- helpers

fn repo_relative(repo: &Repository, path: &str) -> Result<String, GitError> {
    let given = Path::new(path);
    let base = repo
        .workdir()
        .map_or_else(|| repo.path().to_path_buf(), Path::to_path_buf);
    let relative = if given.is_absolute() {
        given.strip_prefix(&base).map_err(|_| {
            failed(
                "git",
                format!("{} is outside the repository", given.display()),
            )
        })?
    } else {
        given
    };
    Ok(git_path_text(relative))
}

fn git_path_text(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn entry_path(entry: &git2::IndexEntry) -> String {
    let bytes = &entry.path;
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

fn short_branch(reference: &str) -> String {
    reference
        .strip_prefix("refs/heads/")
        .or_else(|| reference.strip_prefix("refs/remotes/"))
        .unwrap_or(reference)
        .to_string()
}

fn out(stdout: impl Into<String>) -> GitOutput {
    GitOutput {
        stdout: stdout.into(),
        stderr: String::new(),
        code: Some(0),
    }
}

fn failed(command: &str, message: String) -> GitError {
    GitError::Failed {
        command: command.to_string(),
        code: Some(128),
        stderr: message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patterns_match_the_policy_entries() {
        assert!(pattern_matches("/.worktree/", ".worktree/__probe__"));
        assert!(pattern_matches(".worktree/", ".worktree/__probe__"));
        assert!(pattern_matches("build-cache/", "build-cache/__probe__"));
        assert!(!pattern_matches("*.log", ".worktree/__probe__"));
    }

    #[test]
    fn a_repository_read_returns_porcelain_for_a_real_fixture() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let repo = Repository::init(dir.path()).expect("init");
        std::fs::write(dir.path().join("README.md"), "hello\n").expect("write");
        let runner = Git2Runner::default();
        let status = runner
            .run(dir.path(), &["status", "--porcelain=v2", "--branch"])
            .expect("status");
        assert!(status.success());
        assert!(status.stdout.contains("# branch.oid"));
        let _ = repo;
    }
}
