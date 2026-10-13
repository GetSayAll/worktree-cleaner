//! End-to-end worktree checks against a real Git repository.
//!
//! The unit tests in `src/worktree.rs` pin the decision table against canned output.
//! This file proves the parsers match what real Git prints, and that a scan leaves the
//! repository untouched (development contract D1: "`git status` is unchanged after a
//! scan").

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, SystemTime};

use worktree_core::git::ProcessGitRunner;
use worktree_core::reason::{ReasonCode, SafetyStatus};
use worktree_core::worktree::{
    check_worktree, detect_default_branch, parse_worktree_list, WorktreeCheckOptions,
};
use worktree_core::{GitRunner, WorktreeRecord};

/// A throwaway repository plus its worktrees, removed on drop.
struct Repo {
    root: PathBuf,
    worktrees: Vec<PathBuf>,
}

impl Repo {
    fn run(&self, args: &[&str]) -> Output {
        self.run_in(&self.root, args)
    }

    fn run_in(&self, dir: &Path, args: &[&str]) -> Output {
        let output = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
            .expect("spawn git");
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }

    fn stdout_of(&self, args: &[&str]) -> String {
        String::from_utf8_lossy(&self.run(args).stdout).into_owned()
    }

    fn add_worktree(&mut self, name: &str, args: &[&str]) -> PathBuf {
        let path = self
            .root
            .parent()
            .expect("repo lives in a base directory")
            .join(name);
        let mut full = vec!["worktree", "add", "--quiet"];
        full.push(path.to_str().expect("utf-8 path"));
        full.extend_from_slice(args);
        self.run(&full);
        self.worktrees.push(path.clone());
        path
    }

    /// Every worktree check, in the order `git worktree list` reports them.
    fn scan(&self, opts: &WorktreeCheckOptions) -> Vec<WorktreeRecord> {
        let runner = ProcessGitRunner::default();
        let porcelain = runner
            .run(&self.root, &["worktree", "list", "--porcelain"])
            .expect("worktree list");
        assert!(porcelain.success());
        let entries = parse_worktree_list(&porcelain.stdout);
        entries
            .iter()
            .enumerate()
            .map(|(index, entry)| check_worktree(&runner, &self.root, entry, index == 0, opts))
            .collect()
    }
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
        for worktree in &self.worktrees {
            let _ = std::fs::remove_dir_all(worktree);
        }
    }
}

/// A repository with one worktree per interesting decision-table branch:
/// main, a locked dirty unpushed worktree, an unmerged worktree without an upstream,
/// a detached one, and a prunable one.
fn build_repo(name: &str) -> Repo {
    let base = std::env::temp_dir().join(format!(
        "worktree-cleaner-e2e-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).expect("create base directory");
    let mut repo = Repo {
        root: base.join("repo"),
        worktrees: Vec::new(),
    };
    std::fs::create_dir_all(&repo.root).expect("create repo");
    repo.run(&["init", "--quiet", "-b", "main"]);
    repo.run(&["config", "user.email", "test@example.com"]);
    repo.run(&["config", "user.name", "Test"]);
    std::fs::write(repo.root.join("a.txt"), "one\n").expect("write a.txt");
    repo.run(&["add", "a.txt"]);
    repo.run(&["commit", "--quiet", "-m", "init"]);

    // A linked worktree: one commit ahead of main (with main as its upstream, which is
    // what makes `# branch.ab` appear), one dirty file, one untracked file, locked.
    let feature = repo.add_worktree("wt-feature", &["-b", "feature/x"]);
    repo.run_in(&feature, &["branch", "--set-upstream-to=main"]);
    std::fs::write(feature.join("b.txt"), "two\n").expect("write b.txt");
    repo.run_in(&feature, &["add", "b.txt"]);
    repo.run_in(&feature, &["commit", "--quiet", "-m", "second"]);
    std::fs::write(feature.join("a.txt"), "one\ntwo\n").expect("dirty a.txt");
    std::fs::write(feature.join("notes.md"), "scratch\n").expect("write notes.md");
    let lock_output = Command::new("git")
        .arg("-C")
        .arg(&repo.root)
        .args(["worktree", "lock", "--reason", "holding for review"])
        .arg(&feature)
        .output()
        .expect("spawn git worktree lock");
    assert!(
        lock_output.status.success(),
        "lock failed: {}",
        String::from_utf8_lossy(&lock_output.stderr)
    );

    // A linked worktree with one local commit, no upstream: unmerged.
    let no_upstream = repo.add_worktree("wt-no-upstream", &["-b", "spike/x"]);
    std::fs::write(no_upstream.join("c.txt"), "three\n").expect("write c.txt");
    repo.run_in(&no_upstream, &["add", "c.txt"]);
    repo.run_in(&no_upstream, &["commit", "--quiet", "-m", "spike"]);

    // A detached worktree sitting on main's commit: merged, clean, no upstream.
    repo.add_worktree("wt-detached", &["--detach"]);

    // A worktree whose directory is gone: prunable metadata only.
    let gone = repo.add_worktree("wt-gone", &["-b", "gone-branch"]);
    std::fs::remove_dir_all(&gone).expect("remove the gone worktree");

    repo
}

fn options() -> WorktreeCheckOptions {
    WorktreeCheckOptions {
        default_branch: Some("main".to_string()),
        recent_activity_days: 7,
        now: SystemTime::now(),
        size_bytes: Some(4_096),
        ..WorktreeCheckOptions::default()
    }
}

fn by_branch<'a>(records: &'a [WorktreeRecord], branch: &str) -> &'a WorktreeRecord {
    records
        .iter()
        .find(|record| record.branch.as_deref() == Some(branch))
        .unwrap_or_else(|| panic!("no record for branch {branch}"))
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

#[test]
fn the_scanner_reads_a_real_repository_end_to_end() {
    let repo = build_repo("scan");
    let records = repo.scan(&options());
    assert_eq!(records.len(), 5, "one record per worktree");

    // ---- the main worktree is never deletable ----
    let main = &records[0];
    assert!(main.is_main);
    assert!(has(main, ReasonCode::WtMain));
    assert_eq!(main.safety.status, SafetyStatus::Unsafe);
    assert_eq!(main.branch.as_deref(), Some("main"));

    // ---- a locked, dirty, unpushed worktree ----
    let feature = by_branch(&records, "feature/x");
    assert!(feature.is_locked);
    assert!(has(feature, ReasonCode::WtLocked));
    assert_eq!(feature.dirty_files, 1, "real dirty count from git status");
    assert_eq!(feature.untracked_files, 1);
    assert_eq!(feature.untracked_file_list, vec!["notes.md".to_string()]);
    assert_eq!(feature.ahead, 1, "real ahead count from # branch.ab");
    assert!(feature.has_upstream);
    assert!(has(feature, ReasonCode::WtDirty));
    assert!(has(feature, ReasonCode::WtUntracked));
    assert!(has(feature, ReasonCode::WtUnpushed));
    assert_eq!(feature.safety.status, SafetyStatus::Unsafe);
    // Not merged: its commit is not an ancestor of main.
    assert_eq!(feature.merged_into_default, Some(false));
    assert!(has(feature, ReasonCode::WtUnmerged));
    assert_eq!(feature.size_bytes, Some(4_096));

    // ---- an unmerged worktree without an upstream ----
    let spike = by_branch(&records, "spike/x");
    assert!(!spike.has_upstream);
    assert!(has(spike, ReasonCode::WtNoUpstream));
    assert!(has(spike, ReasonCode::WtUnmerged));
    assert!(has(spike, ReasonCode::WtClean));
    assert_eq!(spike.safety.status, SafetyStatus::Review);
    assert!(
        !has(spike, ReasonCode::WtUnpushed),
        "no upstream means no ahead"
    );

    // ---- a detached worktree on main's commit ----
    let detached = records
        .iter()
        .find(|record| record.is_detached)
        .expect("detached record");
    assert!(detached.branch.is_none());
    assert!(has(detached, ReasonCode::WtDetached));
    assert_eq!(detached.merged_into_default, Some(true));
    assert!(has(detached, ReasonCode::WtMerged));
    assert_eq!(detached.safety.status, SafetyStatus::Review);

    // ---- a prunable worktree ----
    let gone = by_branch(&records, "gone-branch");
    assert!(gone.is_prunable);
    assert!(has(gone, ReasonCode::WtPrunable));
    // Its branch sits on main's commit, so the merge check still answers.
    assert_eq!(gone.merged_into_default, Some(true));
    assert!(has(gone, ReasonCode::WtMerged));
    assert_eq!(gone.safety.status, SafetyStatus::Safe);
    assert_eq!(codes(gone), vec!["wt.prunable", "wt.merged"]);

    // ---- every record carries a non-empty evidence string ----
    for record in &records {
        assert!(!record.safety.reasons.is_empty());
        for reason in &record.safety.reasons {
            assert!(!reason.evidence.trim().is_empty(), "{:?}", reason.code);
            assert!(!reason.text.trim().is_empty(), "{:?}", reason.code);
        }
    }
}

#[test]
fn the_default_branch_is_detected_from_the_repository() {
    let repo = build_repo("default-branch");
    let runner = ProcessGitRunner::default();
    // No remote, so the fallback chain lands on the local `main`.
    assert_eq!(
        detect_default_branch(&runner, &repo.root).as_deref(),
        Some("main")
    );
}

#[test]
fn a_scan_leaves_the_repository_untouched() {
    let repo = build_repo("read-only");
    let before_status = repo.stdout_of(&["status", "--porcelain=v2", "--branch"]);
    let before_worktrees = repo.stdout_of(&["worktree", "list", "--porcelain"]);
    let before_head = repo.stdout_of(&["rev-parse", "HEAD"]);

    let _ = repo.scan(&options());

    assert_eq!(
        repo.stdout_of(&["status", "--porcelain=v2", "--branch"]),
        before_status,
        "git status output must be identical after a scan (D1)"
    );
    assert_eq!(
        repo.stdout_of(&["worktree", "list", "--porcelain"]),
        before_worktrees,
        "a scan must not prune or add worktrees (D1)"
    );
    assert_eq!(repo.stdout_of(&["rev-parse", "HEAD"]), before_head);
    assert!(
        !repo.root.join(".git/index.lock").exists(),
        "a scan must not leave an index lock behind"
    );
}

#[test]
fn a_recently_modified_worktree_is_flagged() {
    let repo = build_repo("activity");
    let mut opts = options();
    opts.recent_activity_days = 7;

    let feature = repo
        .worktrees
        .iter()
        .find(|path| path.ends_with("wt-feature"))
        .expect("feature worktree")
        .clone();
    // Backdate the directory so only the HEAD commit time can make it "recent".
    let target = SystemTime::now() - Duration::from_secs(30 * 86_400);
    let file = std::fs::File::open(&feature).expect("open worktree dir");
    file.set_times(
        std::fs::FileTimes::new()
            .set_accessed(target)
            .set_modified(target),
    )
    .expect("backdate worktree dir");

    let records = repo.scan(&opts);
    // Match by branch rather than by path: git reports the `/private/var` form of a
    // `/var` path on macOS, so the literal strings differ.
    let feature_record = by_branch(&records, "feature/x");
    assert!(has(feature_record, ReasonCode::WtRecentActivity));
    assert!(feature_record.last_updated_at.is_some());
}
