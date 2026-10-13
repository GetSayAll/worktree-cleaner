//! A throwaway Git repository plus its worktrees, shared by the end-to-end tests.
//!
//! Cargo compiles this once per test binary, so an item one binary does not use
//! would otherwise warn as dead code.
#![allow(dead_code)]
// A method that does not read `self` is a method here on purpose. `repo.git_in_env(…)`
// reads as "this repository runs git", and turning some of these into associated
// functions to satisfy one lint would leave the helper set half one thing and half
// another.
#![allow(clippy::unused_self)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// A repository whose whole tree lives under `base`, so `Drop` can take it away
/// in one step and no fixture can outlive its test.
pub struct Repo {
    pub base: PathBuf,
    /// The main worktree.
    pub root: PathBuf,
}

impl Repo {
    /// Creates a repository whose first commit is dated 90 days ago, so the
    /// activity window never fires on the initial state.
    pub fn new(name: &str) -> Self {
        let base = std::env::temp_dir().join(format!(
            "worktree-cleaner-exec-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).expect("create base directory");
        let root = base.join("repo");
        std::fs::create_dir_all(&root).expect("create repo");
        let repo = Self { base, root };
        repo.git(&["init", "--quiet", "-b", "main"]);
        repo.git(&["config", "user.email", "test@example.com"]);
        repo.git(&["config", "user.name", "Test"]);
        repo.git(&["config", "commit.gpgsign", "false"]);
        std::fs::write(repo.root.join("a.txt"), "one\n").expect("write a.txt");
        repo.git(&["add", "a.txt"]);
        repo.git_dated_in(&repo.root, &["commit", "--quiet", "-m", "init"], 90);
        repo
    }

    pub fn git(&self, args: &[&str]) -> Output {
        self.git_in(&self.root, args)
    }

    pub fn git_in(&self, dir: &Path, args: &[&str]) -> Output {
        self.git_in_env(dir, &[], args)
    }

    pub fn git_in_env(&self, dir: &Path, env: &[(&str, &str)], args: &[&str]) -> Output {
        let mut command = Command::new("git");
        command.arg("-C").arg(dir).args(args);
        for (key, value) in env {
            command.env(key, value);
        }
        let output = command.output().expect("spawn git");
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }

    /// Runs a Git command whose failure is part of the test (state inspection).
    pub fn git_try_in(&self, dir: &Path, args: &[&str]) -> Output {
        Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
            .expect("spawn git")
    }

    /// Commits with both author and committer dated `days` in the past, so the
    /// activity window is deterministic without depending on the system clock.
    pub fn git_dated_in(&self, dir: &Path, args: &[&str], days: u64) {
        let stamp = format!("@{} +0000", days_ago_secs(days));
        self.git_in_env(
            dir,
            &[("GIT_AUTHOR_DATE", &stamp), ("GIT_COMMITTER_DATE", &stamp)],
            args,
        );
    }

    pub fn stdout_of(&self, args: &[&str]) -> String {
        String::from_utf8_lossy(&self.git(args).stdout).into_owned()
    }

    pub fn stdout_in(&self, dir: &Path, args: &[&str]) -> String {
        String::from_utf8_lossy(&self.git_in(dir, args).stdout).into_owned()
    }

    /// Adds a linked worktree beside the repository (not inside it), so removing
    /// the repository's own tree cannot disturb the linked one.
    pub fn add_worktree(&self, name: &str, branch: &str) -> PathBuf {
        let path = self.base.join(name);
        self.add_worktree_at(&path, branch);
        path
    }

    /// Adds a linked worktree at an explicit path, wherever that path is.
    ///
    /// `git worktree add` accepts any directory at all — a sibling of the repository,
    /// `~/worktrees/…`, a scratch directory in `/tmp`, a path on another volume — and
    /// nothing in the scanner may assume a worktree sits under the directory the user
    /// picked.
    pub fn add_worktree_at(&self, path: &Path, branch: &str) -> PathBuf {
        self.git(&[
            "worktree",
            "add",
            "--quiet",
            path.to_str().expect("utf-8 path"),
            "-b",
            branch,
        ]);
        path.to_path_buf()
    }

    /// The worktree paths Git currently reports.
    pub fn worktree_paths(&self) -> Vec<String> {
        self.stdout_of(&["worktree", "list", "--porcelain"])
            .lines()
            .filter_map(|line| line.strip_prefix("worktree "))
            .map(str::to_string)
            .collect()
    }
}

impl Drop for Repo {
    fn drop(&mut self) {
        // Every fixture, including the linked worktrees, lives under `base`.
        let _ = std::fs::remove_dir_all(&self.base);
    }
}

/// Seconds since the Unix epoch for a moment `days` ago.
pub fn days_ago_secs(days: u64) -> u64 {
    (SystemTime::now() - Duration::from_secs(days * 86_400))
        .duration_since(UNIX_EPOCH)
        .expect("time before the epoch")
        .as_secs()
}

/// Stamps every entry under `root`, and `root` itself, `days` in the past.
///
/// The activity check reads the newest mtime in the tree against the real clock
/// and does not offer a way to inject the file system's timestamps, so a fixture
/// built moments ago is always inside the window; a test about another signal
/// has to move the tree out of it.
pub fn backdate_tree(root: &Path, days: u64) {
    let target = SystemTime::now() - Duration::from_secs(days * 86_400);
    let times = || {
        std::fs::FileTimes::new()
            .set_accessed(target)
            .set_modified(target)
    };
    let mut stack = vec![root.to_path_buf()];
    let mut dirs: Vec<PathBuf> = Vec::new();
    while let Some(dir) = stack.pop() {
        dirs.push(dir.clone());
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let is_dir = entry.metadata().is_ok_and(|meta| meta.is_dir());
            if is_dir {
                stack.push(path);
            } else if let Ok(file) = std::fs::File::open(&path) {
                let _ = file.set_times(times());
            }
        }
    }
    // A directory is stamped after its children so nothing re-dates it.
    for dir in dirs.into_iter().rev() {
        if let Ok(file) = std::fs::File::open(&dir) {
            let _ = file.set_times(times());
        }
    }
}

/// The system Trash's own location for `name`, so a test can take back the one
/// entry it created instead of leaving garbage in the user's Trash.
///
/// Returns an empty list where the layout is not known (Windows), which makes
/// the cleanup a no-op rather than a failure.
pub fn trash_entry_paths(name: &str) -> Vec<PathBuf> {
    let _ = name;
    let mut found: Vec<PathBuf> = Vec::new();
    #[cfg(target_os = "macos")]
    {
        if let Some(home) = std::env::var_os("HOME") {
            found.push(PathBuf::from(home).join(".Trash").join(name));
        }
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Some(data) = std::env::var_os("XDG_DATA_HOME") {
            let data = PathBuf::from(data);
            found.push(data.join("Trash/files").join(name));
            found.push(data.join("Trash/info").join(format!("{name}.trashinfo")));
        }
        if let Some(home) = std::env::var_os("HOME") {
            let share = PathBuf::from(home).join(".local/share/Trash");
            found.push(share.join("files").join(name));
            found.push(share.join("info").join(format!("{name}.trashinfo")));
        }
    }
    found
}

/// Whether the system Trash can be inspected at all.
///
/// macOS gates the Trash behind a privacy prompt, so a process without it sees
/// `Operation not permitted` on its own Trash directory. A test that moves a
/// fixture there must check this first, or it would leave its fixture behind
/// with no way to take it back.
pub fn trash_inspectable() -> bool {
    match trash_entry_paths("worktree-cleaner-probe").first() {
        Some(entry) => entry
            .parent()
            .is_some_and(|dir| std::fs::read_dir(dir).is_ok()),
        // An unknown Trash layout (Windows) is not an inspectable one.
        None => false,
    }
}

/// Best-effort removal of a test directory this run moved to the Trash.
///
/// The Trash belongs to the user, so an entry is only deleted when it still
/// holds a file whose contents name this run (`marker`); anything else —
/// including a real `node_modules` that happens to sit there — is left alone.
///
/// A Trash that renamed the entry on a name collision keeps it, because
/// guessing which suffixed entry this test created would mean deleting
/// something it cannot prove it owns.
///
/// Returns the entries it took back, so a test can assert the move really
/// landed in the Trash rather than somewhere else.
pub fn purge_marked_from_trash(name: &str, marker: &str) -> Vec<String> {
    let mut removed = Vec::new();
    for entry in trash_entry_paths(name) {
        if !entry.is_dir() || !holds_marker(&entry, marker) {
            continue;
        }
        if std::fs::remove_dir_all(&entry).is_ok() {
            removed.push(entry.display().to_string());
        }
    }
    removed
}

/// Whether any file below `dir` mentions `marker`.
fn holds_marker(dir: &Path, marker: &str) -> bool {
    let mut stack = vec![dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            // `is_dir` follows a symlink; a link is never followed here.
            if path.is_dir() && !path.is_symlink() {
                stack.push(path);
            } else if std::fs::read_to_string(&path).is_ok_and(|text| text.contains(marker)) {
                return true;
            }
        }
    }
    false
}
