//! The merge judgement against a real repository with a real remote.
//!
//! `src/worktree.rs` pins the decision table against canned Git output, and canned
//! output cannot catch a command that is malformed: the fixture checks an argument
//! list, not what Git does with it. That gap produced a live defect — the content
//! confirmation was first written as `git diff --name-only -- <range>`, where `--`
//! before the revision makes Git read the range as a *path*, so the diff came back
//! empty and every branch passed the check. The unit test stayed green because the
//! canned response matched the wrong argument list exactly.
//!
//! So this file builds the four shapes that matter with real Git and a real bare
//! remote, and asserts what the scan says about each.

mod common;

use std::path::{Path, PathBuf};

use common::Repo;
use worktree_core::reason::ReasonCode;
use worktree_core::{scan_project, ProjectSnapshot, SafetyStatus, ScanOptions, WorktreeRecord};

/// The defaults, with a window narrow enough that a commit dated 30 days ago is
/// outside it no matter what the machine's clock does.
fn options() -> ScanOptions {
    ScanOptions {
        recent_activity_days: 3,
        ..ScanOptions::default()
    }
}

/// Writes `content` to `file` inside `dir` and commits it, dated `days` ago.
fn commit_file(repo: &Repo, dir: &Path, file: &str, content: &str, message: &str, days: u64) {
    std::fs::write(dir.join(file), content).expect("write file");
    repo.git_in(dir, &["add", file]);
    repo.git_dated_in(dir, &["commit", "--quiet", "-m", message], days);
}

/// The record for `path`, compared after canonicalising both sides: on macOS the
/// temporary directory is reached through `/var`, which is a symlink.
fn record<'a>(snapshot: &'a ProjectSnapshot, path: &Path) -> &'a WorktreeRecord {
    let wanted = std::fs::canonicalize(path).expect("canonicalise want");
    snapshot
        .worktrees
        .iter()
        .find(|worktree| std::fs::canonicalize(&worktree.path).is_ok_and(|actual| actual == wanted))
        .expect("a record for the worktree")
}

fn codes(record: &WorktreeRecord) -> Vec<&'static str> {
    record
        .safety
        .reasons
        .iter()
        .map(|reason| reason.code.as_str())
        .collect()
}

/// The reason codes as one string, for assertion messages. `Vec<String>` is not
/// `Display`, and a message is the one place this list is read by a person.
fn codes_text(record: &WorktreeRecord) -> String {
    codes(record).join(", ")
}

fn evidence(record: &WorktreeRecord, code: ReasonCode) -> String {
    record
        .safety
        .reasons
        .iter()
        .find(|reason| reason.code == code)
        .map(|reason| reason.evidence.clone())
        .expect("the reason is present")
}

/// Builds a repository whose remote default branch is ahead of the local one, and
/// four worktrees that differ only in how their work reached that remote.
struct Fixture {
    repo: Repo,
    merged: PathBuf,
    squashed: PathBuf,
    reverted: PathBuf,
    unique: PathBuf,
}

fn build() -> Fixture {
    let repo = Repo::new("merge-semantics");

    // A real bare remote. `origin/main` has to be a genuine remote-tracking ref,
    // because that is the ref the check asks first and the local `main` is about to
    // fall behind it.
    let origin = repo.base.join("origin.git");
    repo.git_in(&repo.base, &["init", "--bare", "--quiet", "origin.git"]);
    repo.git(&[
        "remote",
        "add",
        "origin",
        origin.to_str().expect("utf-8 path"),
    ]);
    repo.git(&["push", "--quiet", "-u", "origin", "main"]);
    // Point `refs/remotes/origin/HEAD` at main the way a clone would. The bare
    // remote is initialised on `master`, so `--auto` cannot resolve it.
    repo.git(&["remote", "set-head", "origin", "main"]);

    // 1. Merged into the remote default branch, while local `main` stays behind.
    //    This is the case the local-branch comparison got wrong.
    let merged = repo.add_worktree("wt-merged", "feat/merged");
    commit_file(
        &repo,
        &merged,
        "a.txt",
        "merged content\n",
        "feat: merged",
        40,
    );
    repo.git_in(&merged, &["push", "--quiet", "origin", "feat/merged"]);
    repo.git(&["push", "--quiet", "origin", "feat/merged:main"]);

    // 2. Squash-merged: the change is on the remote under a rewritten commit.
    let squashed = repo.add_worktree("wt-squashed", "feat/squashed");
    commit_file(
        &repo,
        &squashed,
        "b.txt",
        "squashed content\n",
        "feat: squashed",
        40,
    );
    repo.git_in(&squashed, &["push", "--quiet", "origin", "feat/squashed"]);
    squash_onto_remote(&repo, "squash-holder", "feat/squashed", 35);

    // 3. Applied to the remote and then reverted: the patch-id still matches, the
    //    content does not exist anywhere on the remote any more.
    let reverted = repo.add_worktree("wt-reverted", "feat/reverted");
    commit_file(
        &repo,
        &reverted,
        "c.txt",
        "revert me\n",
        "feat: reverted",
        40,
    );
    repo.git_in(&reverted, &["push", "--quiet", "origin", "feat/reverted"]);
    squash_onto_remote(&repo, "revert-holder", "feat/reverted", 35);
    revert_on_remote(&repo, "c.txt", 30);

    // 4. A commit that reached the remote as a branch and nowhere else.
    let unique = repo.add_worktree("wt-unique", "feat/unique");
    commit_file(
        &repo,
        &unique,
        "d.txt",
        "unique content\n",
        "feat: unique",
        40,
    );
    repo.git_in(&unique, &["push", "--quiet", "origin", "feat/unique"]);

    repo.git(&["fetch", "--quiet", "origin"]);

    Fixture {
        repo,
        merged,
        squashed,
        reverted,
        unique,
    }
}

/// Applies `branch` to the remote default branch as one new commit, through a
/// throwaway worktree so the repository's own checkout is never moved.
fn squash_onto_remote(repo: &Repo, holder: &str, branch: &str, days: u64) {
    let work = repo.add_worktree(holder, holder);
    repo.git_in(&work, &["reset", "--quiet", "--hard", "origin/main"]);
    repo.git_in(&work, &["merge", "--quiet", "--squash", branch]);
    repo.git_dated_in(&work, &["commit", "--quiet", "-m", "squash"], days);
    repo.git_in(&work, &["push", "--quiet", "origin", "HEAD:main"]);
    repo.git(&[
        "worktree",
        "remove",
        "--force",
        work.to_str().expect("utf-8 path"),
    ]);
    repo.git(&["fetch", "--quiet", "origin"]);
}

/// Reverts `file` on the remote default branch and publishes it.
fn revert_on_remote(repo: &Repo, file: &str, days: u64) {
    let work = repo.add_worktree("revert-worker", "revert-worker");
    repo.git_in(&work, &["reset", "--quiet", "--hard", "origin/main"]);
    repo.git_in(&work, &["rm", "--quiet", file]);
    repo.git_dated_in(
        &work,
        &["commit", "--quiet", "-m", &format!("revert: {file}")],
        days,
    );
    repo.git_in(&work, &["push", "--quiet", "origin", "HEAD:main"]);
    repo.git(&[
        "worktree",
        "remove",
        "--force",
        work.to_str().expect("utf-8 path"),
    ]);
    repo.git(&["fetch", "--quiet", "origin"]);
}

#[test]
fn every_shape_of_merge_is_judged_against_the_remote() {
    let fixture = build();
    let snapshot = scan_project(fixture.repo.root.to_str().expect("utf-8 path"), &options())
        .expect("scan the fixture");

    // The premise of the whole test: the local default branch is behind, so a
    // comparison against it would answer "not merged" for the first worktree.
    let behind = fixture
        .repo
        .stdout_of(&["rev-list", "--count", "main..origin/main"]);
    assert!(
        behind.trim().parse::<u32>().expect("a count") > 0,
        "the fixture is meaningless unless local main trails origin/main"
    );

    // 1. Merged: an ancestor of the remote default branch, and nothing else.
    let merged = record(&snapshot, &fixture.merged);
    assert_eq!(
        merged.merged_into_default,
        Some(true),
        "{:?}",
        codes(merged)
    );
    assert!(codes(merged).contains(&"wt.merged"));
    assert!(!codes(merged).contains(&"wt.unmerged"));
    assert!(!merged.absorbed_into_default);

    // 2. Squash-merged: not an ancestor, but the patch and the content are both
    //    there. This is the only combination that may read as absorbed.
    let squashed = record(&snapshot, &fixture.squashed);
    assert_eq!(
        squashed.merged_into_default,
        Some(false),
        "{:?}",
        codes(squashed)
    );
    assert!(
        squashed.absorbed_into_default,
        "a squash merge must read as absorbed: {:?}",
        codes_text(squashed)
    );
    assert!(codes(squashed).contains(&"wt.absorbed"));
    assert!(!codes(squashed).contains(&"wt.unmerged"));
    let absorbed_evidence = evidence(squashed, ReasonCode::WtAbsorbed);
    assert!(
        absorbed_evidence.contains("equivalent patch")
            && absorbed_evidence.contains("same content"),
        "the evidence must show the patch and the content: {absorbed_evidence}"
    );

    // 3. Applied then reverted: the patch-id still matches, which is exactly why
    //    the patch answer alone is not enough.
    let reverted = record(&snapshot, &fixture.reverted);
    assert_eq!(reverted.merged_into_default, Some(false));
    assert!(
        !reverted.absorbed_into_default,
        "a reverted change must not be called absorbed: {:?}",
        codes_text(reverted)
    );
    assert!(codes(reverted).contains(&"wt.unmerged"));
    assert!(!codes(reverted).contains(&"wt.absorbed"));
    let unmerged_evidence = evidence(reverted, ReasonCode::WtUnmerged);
    assert!(
        unmerged_evidence.contains("equivalent patch")
            && unmerged_evidence.contains("no longer match"),
        "the evidence must show both halves: {unmerged_evidence}"
    );

    // 4. Published but never applied: no patch upstream, no content either.
    let unique = record(&snapshot, &fixture.unique);
    assert_eq!(unique.merged_into_default, Some(false));
    assert!(!unique.absorbed_into_default);
    assert!(codes(unique).contains(&"wt.unmerged"));
    assert!(!codes(unique).contains(&"wt.absorbed"));

    // Every verdict above depends on five Git calls per worktree resolving. One
    // malformed argument shows up as `cannot_verify` rather than as a wrong answer,
    // which is how the `--` defect stayed invisible for an hour: the check passed
    // because it never ran.
    for path in [
        &fixture.merged,
        &fixture.squashed,
        &fixture.reverted,
        &fixture.unique,
    ] {
        let record = record(&snapshot, path);
        assert!(
            !codes(record).contains(&"wt.cannot_verify"),
            "{} could not be checked: {}",
            path.display(),
            codes_text(record)
        );
    }
}

#[test]
fn the_activity_window_does_not_hold_back_a_finished_branch() {
    // Three days, not seven: a commit dated five days ago is outside the window and
    // must not keep an otherwise-safe worktree at Review. The canned tests pin the
    // rule; this pins that a real clock agrees with it.
    let repo = Repo::new("activity-window");
    let origin = repo.base.join("origin.git");
    repo.git_in(&repo.base, &["init", "--bare", "--quiet", "origin.git"]);
    repo.git(&[
        "remote",
        "add",
        "origin",
        origin.to_str().expect("utf-8 path"),
    ]);
    repo.git(&["push", "--quiet", "-u", "origin", "main"]);
    // Point `refs/remotes/origin/HEAD` at main the way a clone would. The bare
    // remote is initialised on `master`, so `--auto` cannot resolve it.
    repo.git(&["remote", "set-head", "origin", "main"]);

    let five_days = repo.add_worktree("wt-five-days", "feat/five");
    commit_file(
        &repo,
        &five_days,
        "a.txt",
        "five days\n",
        "feat: five days",
        5,
    );
    repo.git_in(&five_days, &["push", "--quiet", "origin", "feat/five"]);
    repo.git(&["push", "--quiet", "origin", "feat/five:main"]);
    repo.git(&["fetch", "--quiet", "origin"]);

    let two_days = repo.add_worktree("wt-two-days", "feat/two");
    // Branch from the remote default branch, not from local main, which the first
    // push has already left behind: a commit on top of a stale base is not a
    // fast-forward and the remote would reject it.
    repo.git_in(&two_days, &["reset", "--quiet", "--hard", "origin/main"]);
    commit_file(&repo, &two_days, "b.txt", "two days\n", "feat: two days", 2);
    repo.git_in(&two_days, &["push", "--quiet", "origin", "feat/two"]);
    repo.git(&["push", "--quiet", "origin", "feat/two:main"]);
    repo.git(&["fetch", "--quiet", "origin"]);

    let snapshot = scan_project(repo.root.to_str().expect("utf-8 path"), &options())
        .expect("scan the fixture");

    let outside = record(&snapshot, &five_days);
    assert!(
        !codes(outside).contains(&"wt.recent_activity"),
        "five days is outside a three-day window: {:?}",
        codes_text(outside)
    );
    assert_eq!(outside.merged_into_default, Some(true));
    assert_eq!(
        outside.safety.status,
        SafetyStatus::Safe,
        "a merged, clean, five-day-old worktree is safe: {:?}",
        codes_text(outside)
    );

    let inside = record(&snapshot, &two_days);
    assert!(
        codes(inside).contains(&"wt.recent_activity"),
        "two days is inside the window: {:?}",
        codes_text(inside)
    );
    assert_eq!(inside.safety.status, SafetyStatus::Review);
}
