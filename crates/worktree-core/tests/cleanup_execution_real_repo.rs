//! End-to-end plan execution against a real repository, real Git, and the real
//! system Trash.
//!
//! The unit tests in `src/cleanup.rs` pin the decision logic against fixtures.
//! This file proves the three execution guarantees hold in the world the app
//! actually runs in: D11 (a stale plan is skipped, never run), D5 (a directory
//! leaves through the system Trash) and D6 (a worktree leaves through
//! `git worktree remove` plus `prune`).

mod common;

use std::path::{Path, PathBuf};

use common::{backdate_tree, purge_marked_from_trash, trash_inspectable, Repo};
use worktree_core::cleanup::{execute_plan, ExecuteOptions};
use worktree_core::{
    log, scan_project, CleanupPlan, ExecutionResult, HistoryAction, PlanItemKind, ProjectSnapshot,
    SafetyStatus, ScanOptions, TrashMoveItem, WorktreeRecord, WorktreeRemovalItem,
};

fn scan_options() -> ScanOptions {
    ScanOptions {
        recent_activity_days: 7,
        ..ScanOptions::default()
    }
}

/// Logging off, so a test that is not about the log cannot touch the user's.
fn options_without_log() -> ExecuteOptions {
    ExecuteOptions {
        log_path: None,
        ..ExecuteOptions::default()
    }
}

fn options_with_log(path: &Path) -> ExecuteOptions {
    ExecuteOptions {
        log_path: Some(path.to_path_buf()),
        ..ExecuteOptions::default()
    }
}

/// A repository plus one linked worktree that is clean and merged into main with
/// main as its upstream — the only combination the decision table calls Safe
/// (D12) — and an ignored `node_modules` in the main worktree for the Trash
/// tests.
///
/// Both trees are backdated 30 days: the activity window would otherwise add a
/// Review and nothing would ever be Safe.
///
/// The returned `marker` is written into the module file so the Trash cleanup
/// can prove an entry is this run's fixture and not the user's own directory.
fn repo_with_a_safe_worktree(name: &str) -> (Repo, PathBuf, String) {
    let repo = Repo::new(name);
    let marker = format!("worktree-cleaner-test-{}", std::process::id());

    std::fs::write(repo.root.join(".gitignore"), "node_modules/\n").expect("write .gitignore");
    std::fs::write(repo.root.join("package.json"), "{}\n").expect("write manifest");
    std::fs::create_dir_all(repo.root.join("node_modules/left-pad")).expect("create modules");
    std::fs::write(
        repo.root.join("node_modules/left-pad/index.js"),
        format!("// {marker}\nmodule.exports = 1;\n"),
    )
    .expect("write module");
    repo.git(&["add", ".gitignore", "package.json"]);
    repo.git_dated_in(&repo.root, &["commit", "--quiet", "-m", "layout"], 80);

    // Created after the layout commit, so the linked worktree sits exactly on
    // main and is both clean and merged.
    let linked = repo.add_worktree("wt-removable", "removable/x");
    repo.git_in(&linked, &["branch", "--set-upstream-to=main"]);

    backdate_tree(&repo.root, 30);
    backdate_tree(&linked, 30);
    (repo, linked, marker)
}

fn scan(repo: &Repo) -> ProjectSnapshot {
    scan_project(&repo.root.display().to_string(), &scan_options()).expect("scan")
}

/// The linked worktree of [`repo_with_a_safe_worktree`], which is the only
/// deletable worktree in the fixture.
fn safe_worktree(snapshot: &ProjectSnapshot) -> &WorktreeRecord {
    snapshot
        .worktrees
        .iter()
        .find(|record| !record.is_main && record.path.ends_with("wt-removable"))
        .expect("the linked worktree is reported")
}

/// The ignored `node_modules` the Trash tests move.
fn ignored_modules(snapshot: &ProjectSnapshot) -> &worktree_core::ArtifactRecord {
    snapshot
        .artifacts
        .iter()
        .find(|artifact| {
            artifact.path.ends_with("node_modules") && artifact.git_ignored == Some(true)
        })
        .expect("the ignored node_modules is reported")
}

fn plan_removing(path: &str) -> CleanupPlan {
    CleanupPlan {
        worktree_removals: vec![WorktreeRemovalItem {
            kind: PlanItemKind::Worktree,
            path: path.to_string(),
            branch: Some("removable/x".to_string()),
            // Display only: the executor re-derives the command from its own
            // re-check instead of parsing this string.
            command: format!("git worktree remove -- {path}"),
            force: false,
            untracked_files: Vec::new(),
            size_bytes: None,
        }],
        ..CleanupPlan::default()
    }
}

// ---- D6: worktrees leave through Git ---------------------------------------

#[test]
fn a_safe_worktree_is_removed_by_git_and_pruned() {
    let (repo, linked, _marker) = repo_with_a_safe_worktree("remove");
    let snapshot = scan(&repo);
    let record = safe_worktree(&snapshot);
    assert_eq!(
        record.safety.status,
        SafetyStatus::Safe,
        "{:?}",
        record.safety.reasons
    );

    let log_path = repo.base.join("operations.log");
    let outcome = execute_plan(
        &plan_removing(&record.path),
        &snapshot,
        &options_with_log(&log_path),
    );

    // The removal, plus one repository-level prune summarising the metadata
    // cleanup (D6).
    assert_eq!(outcome.entries.len(), 2, "{:?}", outcome.entries);
    assert_eq!(outcome.entries[0].action, HistoryAction::WorktreeRemove);
    assert_eq!(
        outcome.entries[0].result,
        ExecutionResult::Ok,
        "{:?}",
        outcome.entries[0]
    );
    assert_eq!(outcome.entries[0].target, record.path);
    assert_eq!(outcome.entries[1].action, HistoryAction::Prune);
    assert_eq!(outcome.entries[1].result, ExecutionResult::Ok);
    assert!(outcome.log_error.is_none());

    // The directory is gone and Git no longer lists it.
    assert!(!linked.exists(), "the worktree directory is gone");
    let listed = repo.worktree_paths();
    assert!(
        !listed.iter().any(|path| path.ends_with("wt-removable")),
        "git worktree list still reports {listed:?}"
    );
    assert_eq!(listed.len(), 1, "only the main worktree is left");

    // One log line per action, each carrying the result.
    let history = log::read(&log_path);
    assert_eq!(history.len(), 2, "{history:?}");
    assert_eq!(history[0].action, HistoryAction::WorktreeRemove);
    assert_eq!(history[0].result, ExecutionResult::Ok);
    assert_eq!(history[0].target, record.path);
    assert!(!history[0].reason.is_empty());
    assert!(!history[0].at.is_empty());
}

#[test]
fn a_prunable_worktree_is_cleaned_up_with_prune_only() {
    let (repo, linked, _marker) = repo_with_a_safe_worktree("prune");
    // The state a crashed `rm -rf` leaves behind: Git metadata without a
    // directory. Nothing but `git worktree prune` should run for it.
    std::fs::remove_dir_all(&linked).expect("remove the worktree directory");

    let snapshot = scan(&repo);
    let record = safe_worktree(&snapshot);
    assert!(record.is_prunable);
    assert_eq!(
        record.safety.status,
        SafetyStatus::Safe,
        "{:?}",
        record.safety.reasons
    );

    let log_path = repo.base.join("operations.log");
    let outcome = execute_plan(
        &plan_removing(&record.path),
        &snapshot,
        &options_with_log(&log_path),
    );

    assert_eq!(outcome.entries.len(), 2, "{:?}", outcome.entries);
    assert_eq!(outcome.entries[0].action, HistoryAction::Prune);
    assert_eq!(
        outcome.entries[0].result,
        ExecutionResult::Ok,
        "{:?}",
        outcome.entries[0]
    );
    assert!(
        outcome.entries[0].detail.contains("no files were touched"),
        "{:?}",
        outcome.entries[0]
    );

    let listed = repo.worktree_paths();
    assert!(
        !listed.iter().any(|path| path.ends_with("wt-removable")),
        "git worktree list still reports {listed:?}"
    );
}

// ---- D11: a stale plan is skipped, never run -------------------------------

#[test]
fn a_plan_is_skipped_when_the_worktree_changed_after_the_scan() {
    let (repo, linked, _marker) = repo_with_a_safe_worktree("stale");
    let snapshot = scan(&repo);
    let record = safe_worktree(&snapshot);
    assert_eq!(record.safety.status, SafetyStatus::Safe);

    // Something writes into the worktree between the scan and the confirmation.
    // The plan no longer describes what is on disk, so it must not run (D11).
    std::fs::write(linked.join("later.txt"), "written after the scan\n").expect("write later.txt");

    let log_path = repo.base.join("operations.log");
    let outcome = execute_plan(
        &plan_removing(&record.path),
        &snapshot,
        &options_with_log(&log_path),
    );

    assert_eq!(outcome.entries.len(), 1, "{:?}", outcome.entries);
    assert_eq!(outcome.entries[0].result, ExecutionResult::Skipped);
    assert!(
        outcome.entries[0]
            .detail
            .contains("changed since the plan was generated"),
        "{:?}",
        outcome.entries[0]
    );
    assert!(linked.exists(), "nothing was executed");
    assert!(
        linked.join("later.txt").exists(),
        "the new file is untouched"
    );

    let history = log::read(&log_path);
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].result, ExecutionResult::Skipped);
}

#[test]
fn a_whitelisted_worktree_is_skipped_by_the_path_check() {
    let (repo, linked, _marker) = repo_with_a_safe_worktree("whitelist");
    let snapshot = scan(&repo);
    let record = safe_worktree(&snapshot);

    // The rule is applied to the path a second time, right before the removal
    // (D10): a plan that predates the rule cannot slip past it.
    let mut options = options_without_log();
    options.whitelist = vec![record.path.clone()];
    let outcome = execute_plan(&plan_removing(&record.path), &snapshot, &options);

    assert_eq!(outcome.entries.len(), 1, "{:?}", outcome.entries);
    assert_eq!(outcome.entries[0].result, ExecutionResult::Skipped);
    assert!(
        outcome.entries[0].detail.contains("path check failed"),
        "{:?}",
        outcome.entries[0]
    );
    assert!(linked.exists(), "a protected worktree is never removed");
}

// ---- D5: directories leave through the system Trash ------------------------

#[test]
fn an_ignored_artifact_is_moved_to_the_system_trash() {
    // macOS gates the Trash behind a privacy prompt: a process without it can
    // still move a file there but cannot look inside. The move is asserted
    // either way; only reading the entry back needs the permission.
    let inspectable = trash_inspectable();
    let (repo, _linked, marker) = repo_with_a_safe_worktree("trash");
    let snapshot = scan(&repo);
    let artifact = ignored_modules(&snapshot);
    assert_eq!(
        artifact.safety.status,
        SafetyStatus::Safe,
        "{:?}",
        artifact.safety.reasons
    );

    let log_path = repo.base.join("operations.log");
    let plan = CleanupPlan {
        trash_moves: vec![TrashMoveItem {
            kind: PlanItemKind::Artifact,
            path: artifact.path.clone(),
            artifact_kind: artifact.kind.clone(),
            size_bytes: artifact.size_bytes,
            file_count: artifact.file_count,
            bundled_with_worktree: None,
        }],
        ..CleanupPlan::default()
    };
    let outcome = execute_plan(&plan, &snapshot, &options_with_log(&log_path));

    // Take the fixture back out of the Trash before asserting anything, so a
    // failed assertion cannot leave it sitting in the user's Trash.
    let recovered = if inspectable {
        purge_marked_from_trash("node_modules", &marker)
    } else {
        Vec::new()
    };

    assert_eq!(outcome.entries.len(), 1, "{:?}", outcome.entries);
    assert_eq!(
        outcome.entries[0].result,
        ExecutionResult::Ok,
        "the system Trash accepted the move: {:?}",
        outcome.entries[0]
    );
    assert!(outcome.entries[0].detail.contains("Trash"));
    if inspectable {
        assert_eq!(
            recovered.len(),
            1,
            "the directory should have landed in the system Trash (D5); found {recovered:?}"
        );
    }
    assert!(
        !Path::new(&artifact.path).exists(),
        "the directory left the working tree"
    );
    assert!(
        repo.root.join("package.json").exists(),
        "only the artifact moved; the project stayed"
    );

    let history = log::read(&log_path);
    assert_eq!(history.len(), 1, "{history:?}");
    assert_eq!(history[0].action, HistoryAction::TrashMove);
    assert_eq!(history[0].result, ExecutionResult::Ok);
    assert_eq!(history[0].target, artifact.path);
    assert!(history[0].size_bytes.is_some_and(|bytes| bytes > 0));
}

#[test]
fn a_bundled_artifact_is_not_deleted_a_second_time() {
    let (repo, _linked, marker) = repo_with_a_safe_worktree("bundled");
    let snapshot = scan(&repo);
    let artifact = ignored_modules(&snapshot);

    // The owning worktree is selected too, so the artifact disappears with it;
    // a second operation would be a double delete.
    let plan = CleanupPlan {
        trash_moves: vec![TrashMoveItem {
            kind: PlanItemKind::Artifact,
            path: artifact.path.clone(),
            artifact_kind: artifact.kind.clone(),
            size_bytes: artifact.size_bytes,
            file_count: artifact.file_count,
            bundled_with_worktree: Some(snapshot.root.clone()),
        }],
        ..CleanupPlan::default()
    };
    let outcome = execute_plan(&plan, &snapshot, &options_without_log());

    assert_eq!(outcome.entries.len(), 1, "{:?}", outcome.entries);
    assert_eq!(outcome.entries[0].result, ExecutionResult::Ok);
    assert!(
        outcome.entries[0].detail.contains("no separate operation"),
        "{:?}",
        outcome.entries[0]
    );
    let target = Path::new(&artifact.path);
    assert!(
        target.exists(),
        "the artifact is left for the worktree removal"
    );
    assert!(target.join("left-pad/index.js").exists());
    if trash_inspectable() {
        assert!(
            purge_marked_from_trash("node_modules", &marker).is_empty(),
            "nothing was moved to the Trash"
        );
    }
}

#[test]
fn a_whitelisted_artifact_is_skipped_by_the_path_check() {
    let (repo, _linked, marker) = repo_with_a_safe_worktree("whitelist-artifact");
    let snapshot = scan(&repo);
    let artifact = ignored_modules(&snapshot);

    let plan = CleanupPlan {
        trash_moves: vec![TrashMoveItem {
            kind: PlanItemKind::Artifact,
            path: artifact.path.clone(),
            artifact_kind: artifact.kind.clone(),
            size_bytes: artifact.size_bytes,
            file_count: artifact.file_count,
            bundled_with_worktree: None,
        }],
        ..CleanupPlan::default()
    };
    let mut options = options_without_log();
    options.whitelist = vec![artifact.path.clone()];
    let outcome = execute_plan(&plan, &snapshot, &options);

    assert_eq!(outcome.entries.len(), 1, "{:?}", outcome.entries);
    assert_eq!(outcome.entries[0].result, ExecutionResult::Skipped);
    assert!(
        outcome.entries[0].detail.contains("path check failed"),
        "{:?}",
        outcome.entries[0]
    );
    assert!(
        Path::new(&artifact.path).exists(),
        "a protected artifact stays"
    );
    if trash_inspectable() {
        assert!(
            purge_marked_from_trash("node_modules", &marker).is_empty(),
            "nothing was moved to the Trash"
        );
    }
}

// ---- D14: one item's outcome does not change another's ---------------------

/// A marker in the name of a directory that lives outside the repository's own tree.
const OUTSIDE_MARKER: &str = "worktree-cleaner-outside";

/// A directory of its own, unrelated to `Repo::base`. The caller removes it: the
/// repository's `Drop` only knows about its own tree.
fn outside_directory(tag: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("{OUTSIDE_MARKER}-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).expect("create the outside directory");
    path
}

#[test]
fn a_worktree_outside_the_scanned_directory_is_scanned_and_removed() {
    // `git worktree add` takes any path at all: a sibling directory, `~/worktrees/…`,
    // a scratch directory in `/tmp`, a path on another volume. Nothing may assume a
    // worktree sits under the directory the user picked — the scanner asks Git where
    // they are, and the executor hands Git the absolute path Git reported.
    let repo = Repo::new("outside");
    let elsewhere = outside_directory("worktree");
    assert!(
        !elsewhere.starts_with(&repo.base),
        "the fixture has to be somewhere else for this to prove anything"
    );

    // Committed on `main` first, so the worktree checks out the same tree and comes
    // out clean and merged.
    std::fs::write(repo.root.join(".gitignore"), "node_modules/\n").expect("write .gitignore");
    std::fs::write(repo.root.join("package.json"), "{}\n").expect("write manifest");
    repo.git(&["add", ".gitignore", "package.json"]);
    repo.git_dated_in(&repo.root, &["commit", "--quiet", "-m", "layout"], 80);

    let linked = repo.add_worktree_at(&elsewhere, "outside/x");
    repo.git_in(&linked, &["branch", "--set-upstream-to=main"]);
    // Ignored, so the worktree stays clean and can come out Safe.
    std::fs::create_dir_all(linked.join("node_modules/left-pad")).expect("create modules");
    std::fs::write(
        linked.join("node_modules/left-pad/index.js"),
        "module.exports = 1;\n",
    )
    .expect("write module");
    backdate_tree(&linked, 30);

    let snapshot = scan(&repo);

    // Reported, with the path Git gave, and demonstrably not under the scanned root.
    let record = snapshot
        .worktrees
        .iter()
        .find(|record| record.path.contains(OUTSIDE_MARKER))
        .expect("a worktree outside the scanned directory is still reported");
    assert!(!record.path.starts_with(&snapshot.root));
    assert_eq!(record.branch.as_deref(), Some("outside/x"));
    assert_eq!(
        record.safety.status,
        SafetyStatus::Safe,
        "{:?}",
        record.safety.reasons
    );

    // Its build output belongs to it, not to the main worktree.
    let artifact = snapshot
        .artifacts
        .iter()
        .find(|artifact| artifact.path.contains(OUTSIDE_MARKER))
        .expect("the artifact inside it is scanned");
    assert!(artifact.path.ends_with("node_modules"));
    assert!(artifact.project_root.contains(OUTSIDE_MARKER));

    // And it can be removed: `git worktree remove -- <path>` takes the path wherever
    // it points.
    let log_path = repo.base.join("operations.log");
    let outcome = execute_plan(
        &plan_removing(&record.path),
        &snapshot,
        &options_with_log(&log_path),
    );
    assert_eq!(
        outcome.entries[0].result,
        ExecutionResult::Ok,
        "{:?}",
        outcome.entries[0]
    );
    assert!(!linked.exists(), "the worktree directory is gone");
    assert!(
        !repo
            .worktree_paths()
            .iter()
            .any(|path| path.contains(OUTSIDE_MARKER)),
        "git no longer lists it"
    );

    let _ = std::fs::remove_dir_all(&elsewhere);
}

#[test]
fn a_skipped_item_does_not_stop_the_items_after_it() {
    let (repo, linked, _marker) = repo_with_a_safe_worktree("isolation");
    let snapshot = scan(&repo);
    let record = safe_worktree(&snapshot);

    // A stale entry the executor cannot match sits ahead of the real one.
    let mut plan = plan_removing(&record.path);
    plan.worktree_removals.insert(
        0,
        WorktreeRemovalItem {
            kind: PlanItemKind::Worktree,
            path: repo.base.join("wt-vanished").display().to_string(),
            branch: Some("gone".to_string()),
            command: String::new(),
            force: false,
            untracked_files: Vec::new(),
            size_bytes: None,
        },
    );

    let log_path = repo.base.join("operations.log");
    let outcome = execute_plan(&plan, &snapshot, &options_with_log(&log_path));

    assert_eq!(outcome.entries.len(), 3, "{:?}", outcome.entries);
    assert_eq!(outcome.entries[0].result, ExecutionResult::Skipped);
    assert_eq!(
        outcome.entries[1].result,
        ExecutionResult::Ok,
        "the item after a skipped one still runs (D14): {:?}",
        outcome.entries[1]
    );
    assert!(!linked.exists(), "the second item executed");
}
