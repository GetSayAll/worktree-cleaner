//! End-to-end worktree checks against a real Git repository.
//!
//! The unit tests in `src/worktree.rs` pin the decision table against canned output.
//! This file proves the parsers match what real Git prints, and that a scan leaves the
//! repository untouched (development contract D1: "`git status` is unchanged after a
//! scan").

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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

// `run_in_env` does not read `self` and is a method anyway: `repo.run_in_env(…)` reads
// as "this repository runs git", and mixed conventions in one helper set are worse
// than one lint.
#[allow(clippy::unused_self)]
impl Repo {
    fn run(&self, args: &[&str]) -> Output {
        self.run_in(&self.root, args)
    }

    fn run_in(&self, dir: &Path, args: &[&str]) -> Output {
        self.run_in_env(dir, &[], args)
    }

    /// Runs a Git command with extra environment variables (used to date a commit
    /// deterministically, without depending on the system clock's formatting).
    fn run_in_env(&self, dir: &Path, env: &[(&str, &str)], args: &[&str]) -> Output {
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
            .run(&self.root, &["worktree", "list", "--porcelain", "-z"])
            .expect("worktree list");
        assert!(porcelain.success());
        // The `-z` contract is what makes a newline inside a path survive; a
        // line-based capture would silently truncate it.
        assert!(
            porcelain.stdout.contains('\0'),
            "porcelain -z must be NUL-separated"
        );
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
/// a detached one, one whose path contains a newline, and a prunable one.
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
    // The initial commit is dated 90 days ago, so a worktree that sits on it (the
    // detached and newline ones) is not "recently active" by construction and its
    // verdict stays deterministic.
    let old_init = SystemTime::now() - Duration::from_hours(2160);
    let init_stamp = format!(
        "@{} +0000",
        old_init
            .duration_since(UNIX_EPOCH)
            .expect("time before the epoch")
            .as_secs()
    );
    repo.run_in_env(
        &repo.root,
        &[
            ("GIT_AUTHOR_DATE", init_stamp.as_str()),
            ("GIT_COMMITTER_DATE", init_stamp.as_str()),
        ],
        &["commit", "--quiet", "-m", "init"],
    );

    // A linked worktree: one commit ahead of main (with main as its upstream, which is
    // what makes `# branch.ab` appear), one dirty file, one untracked file, locked.
    // The commit is dated 60 days ago so the activity test can tell the HEAD commit
    // time apart from the directory mtime.
    let feature = repo.add_worktree("wt-feature", &["-b", "feature/x"]);
    repo.run_in(&feature, &["branch", "--set-upstream-to=main"]);
    std::fs::write(feature.join("b.txt"), "two\n").expect("write b.txt");
    repo.run_in(&feature, &["add", "b.txt"]);
    let old = SystemTime::now() - Duration::from_hours(1440);
    let stamp = format!(
        "@{} +0000",
        old.duration_since(UNIX_EPOCH)
            .expect("time before the epoch")
            .as_secs()
    );
    repo.run_in_env(
        &feature,
        &[
            ("GIT_AUTHOR_DATE", stamp.as_str()),
            ("GIT_COMMITTER_DATE", stamp.as_str()),
        ],
        &["commit", "--quiet", "-m", "second"],
    );
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

    // A worktree whose path contains a newline: the `-z` parser must keep it whole
    // (a line-based parser would truncate it and misread the entry as gone).
    // Unix-only: NTFS forbids a newline in a path, so the fixture cannot exist on
    // Windows.
    #[cfg(unix)]
    {
        let newline = repo.add_worktree("wt/new\nline", &["-b", "newline-branch"]);
        // Backdate the directory so the record is not "recently active" by
        // construction; the initial commit is already 90 days old.
        let target = SystemTime::now() - Duration::from_hours(720);
        let file = std::fs::File::open(&newline).expect("open newline worktree dir");
        file.set_times(
            std::fs::FileTimes::new()
                .set_accessed(target)
                .set_modified(target),
        )
        .expect("backdate newline worktree dir");
    }

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
    #[cfg(unix)]
    assert_eq!(records.len(), 6, "one record per worktree");
    #[cfg(not(unix))]
    assert_eq!(
        records.len(),
        5,
        "one record per worktree (the newline-path fixture is Unix-only)"
    );

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

    // ---- a worktree whose path contains a newline ----
    // The `-z` parser keeps the path whole; the line-based one would truncate it
    // at the newline and misreport the entry as a directory that no longer exists.
    #[cfg(unix)]
    {
        let newline = by_branch(&records, "newline-branch");
        assert!(
            newline.path.contains('\n') && newline.path.ends_with("new\nline"),
            "the path must survive intact: {:?}",
            newline.path
        );
        assert!(!newline.is_prunable);
        assert_eq!(newline.safety.status, SafetyStatus::Safe);
        assert_eq!(codes(newline), vec!["wt.clean", "wt.merged"]);
    }

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
    let before_worktrees = repo.stdout_of(&["worktree", "list", "--porcelain", "-z"]);
    let before_head = repo.stdout_of(&["rev-parse", "HEAD"]);

    let _ = repo.scan(&options());

    assert_eq!(
        repo.stdout_of(&["status", "--porcelain=v2", "--branch"]),
        before_status,
        "git status output must be identical after a scan (D1)"
    );
    assert_eq!(
        repo.stdout_of(&["worktree", "list", "--porcelain", "-z"]),
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
/// The activity signal is the HEAD commit time, and a bulk touch of the directory
/// is not activity.
///
/// This is the counter-example that settled it. The directory mtime moves when a
/// top-level entry is created, renamed or deleted — not when a file deeper inside
/// is edited — so an indexer, a backup, or a cleanup script marks a worktree
/// "recent" that nobody has worked in. While mtime was one of the two signals it
/// downgraded every clean worktree a backup had walked over: measured on a
/// 109-worktree repository, 48 of 48 clean-and-merged worktrees.
///
/// Both halves are pinned here, against real Git: an idle worktree is not
/// "recent", and touching its directory does not make it so.
fn a_bulk_directory_touch_does_not_make_a_worktree_recent() {
    let repo = build_repo("activity");
    let mut opts = options();
    opts.recent_activity_days = 7;

    let feature = repo
        .worktrees
        .iter()
        .find(|path| path.ends_with("wt-feature"))
        .expect("feature worktree")
        .clone();

    // Phase 1: this worktree's HEAD commit is 60 days old (see `build_repo`), so it
    // is outside the seven-day window and the recorded activity is that commit.
    let records = repo.scan(&opts);
    let feature_record = by_branch(&records, "feature/x");
    assert!(
        !has(feature_record, ReasonCode::WtRecentActivity),
        "an idle worktree is not recently active: {:?}",
        codes(feature_record)
    );
    let stamped = parse_rfc3339_secs(
        feature_record
            .last_updated_at
            .as_deref()
            .expect("last_updated_at"),
    );
    let age = secs(SystemTime::now()) - stamped;
    assert!(
        age > 30 * 86_400,
        "last_updated_at must be the HEAD commit time (60 days old), got {stamped}"
    );

    // Phase 2: touch the directory itself, the way a backup or an indexer does.
    // Nothing in the repository changed, so nothing about the verdict may change.
    let now = SystemTime::now();
    let dir = std::fs::File::open(&feature).expect("open worktree dir");
    dir.set_times(
        std::fs::FileTimes::new()
            .set_accessed(now)
            .set_modified(now),
    )
    .expect("touch the worktree directory");
    drop(dir);

    let records = repo.scan(&opts);
    let feature_record = by_branch(&records, "feature/x");
    assert!(
        !has(feature_record, ReasonCode::WtRecentActivity),
        "a directory mtime is not activity: {:?}",
        codes(feature_record)
    );
    let stamped = parse_rfc3339_secs(
        feature_record
            .last_updated_at
            .as_deref()
            .expect("last_updated_at"),
    );
    assert!(
        (secs(now) - stamped).abs() > 30 * 86_400,
        "the touch must not become the recorded activity, got {stamped}"
    );
}

fn secs(time: SystemTime) -> i128 {
    match time.duration_since(UNIX_EPOCH) {
        Ok(delta) => i128::from(delta.as_secs()),
        Err(error) => -i128::from(error.duration().as_secs()),
    }
}

/// Minimal parser for the `YYYY-MM-DDTHH:MM:SSZ` form, re-derived independently of
/// the production formatter so the assertions below are real checks.
fn parse_rfc3339_secs(text: &str) -> i128 {
    fn days_from_civil(year: i64, month: u32, day: u32) -> i128 {
        let year = i128::from(year);
        let month = i128::from(month);
        let day = i128::from(day);
        let year = if month <= 2 { year - 1 } else { year };
        let era = year.div_euclid(400);
        let year_of_era = year - era * 400;
        let day_of_year = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
        let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
        era * 146_097 + day_of_era - 719_468
    }
    let (date, time) = text.split_once('T').expect("date/time separator");
    let (year, rest) = date.split_once('-').expect("year");
    let (month, day) = rest.split_once('-').expect("month/day");
    let (hour, rest) = time.split_once(':').expect("hour");
    let (minute, second) = rest.split_once(':').expect("minute/second");
    let second = second.strip_suffix('Z').expect("Z suffix");
    let days = days_from_civil(
        year.parse().expect("year"),
        month.parse().expect("month"),
        day.parse().expect("day"),
    );
    let seconds: i128 = hour.parse().expect("hour");
    let seconds = seconds * 3600
        + i128::from(minute.parse::<u32>().expect("minute")) * 60
        + i128::from(second.parse::<u32>().expect("second"));
    days * 86_400 + seconds
}
