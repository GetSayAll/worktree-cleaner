//! End-to-end scans against a real Git repository.
//!
//! `src/artifact.rs` and `src/worktree.rs` pin the decision tables against
//! canned Git output. This file proves the orchestration matches what real Git
//! answers: the `git ls-files` / `check-ignore` / `status` calls carry absolute
//! paths, the walk starts at each worktree root (D8), and a scan changes nothing
//! (D1).

mod common;

use std::path::Path;

use common::{backdate_tree, Repo};
use worktree_core::reason::ReasonCode;
use worktree_core::{
    scan_project, ArtifactRecord, ProjectSnapshot, SafetyStatus, ScanError, ScanOptions,
};

fn options() -> ScanOptions {
    ScanOptions {
        recent_activity_days: 7,
        ..ScanOptions::default()
    }
}

/// A repository whose main worktree and linked worktree each hold the same
/// layout: an ignored `node_modules`, plus one `dist/` that Git tracks.
///
/// The two `dist/` directories are the D7 pair — identical names, opposite
/// verdicts — and the two `node_modules` are the D8 check: one scan run must
/// treat each worktree as its own project root.
fn build_repo(name: &str) -> Repo {
    let repo = Repo::new(name);
    std::fs::write(repo.root.join(".gitignore"), "node_modules/\ndist/\n")
        .expect("write .gitignore");
    std::fs::write(repo.root.join("package.json"), "{}\n").expect("write manifest");
    std::fs::write(repo.root.join("src.js"), "console.log(1);\n").expect("write source");

    // Ignored build output.
    std::fs::create_dir_all(repo.root.join("node_modules/left-pad")).expect("create node_modules");
    std::fs::write(
        repo.root.join("node_modules/left-pad/index.js"),
        "module.exports = 1;\n",
    )
    .expect("write module");

    // A `dist/` with the same name that Git tracks: `-f` overrides the ignore
    // rule above, which is exactly the situation D7 warns about.
    std::fs::create_dir_all(repo.root.join("dist")).expect("create dist");
    std::fs::write(repo.root.join("dist/generated.js"), "// generated\n").expect("write dist file");
    repo.git(&["add", "-f", "dist/generated.js"]);

    repo.git(&["add", ".gitignore", "package.json", "src.js"]);
    repo.git_dated_in(&repo.root, &["commit", "--quiet", "-m", "layout"], 80);

    // The linked worktree checks the same layout out, then grows its own
    // ignored build output.
    let linked = repo.add_worktree("wt-linked", "linked/x");
    std::fs::create_dir_all(linked.join("node_modules/left-pad")).expect("create linked modules");
    std::fs::write(
        linked.join("node_modules/left-pad/index.js"),
        "module.exports = 2;\n",
    )
    .expect("write linked module");

    // Both trees are moved out of the activity window: this test is about the
    // Git facts, and a fresh mtime would add an unrelated Review.
    backdate_tree(&repo.root, 30);
    backdate_tree(&linked, 30);
    repo
}

/// The artifact named `name` whose project root is the worktree ending in
/// `worktree_leaf`.
///
/// Git reports `/private/var/...` for a `/var/...` path on macOS, so the paths
/// are matched by their last component rather than by string equality.
fn artifact_at<'a>(
    snapshot: &'a ProjectSnapshot,
    worktree_leaf: &str,
    name: &str,
) -> &'a ArtifactRecord {
    snapshot
        .artifacts
        .iter()
        .find(|artifact| {
            artifact.path.ends_with(name)
                && Path::new(&artifact.project_root)
                    .file_name()
                    .is_some_and(|leaf| leaf.to_string_lossy() == worktree_leaf)
        })
        .unwrap_or_else(|| panic!("no {name} under {worktree_leaf}; got {:?}", paths(snapshot)))
}

fn paths(snapshot: &ProjectSnapshot) -> Vec<String> {
    snapshot
        .artifacts
        .iter()
        .map(|artifact| artifact.path.clone())
        .collect()
}

fn codes(record: &ArtifactRecord) -> Vec<&'static str> {
    record
        .safety
        .reasons
        .iter()
        .map(|reason| reason.code.as_str())
        .collect()
}

#[test]
fn a_scan_reads_worktrees_and_their_artifacts_from_a_real_repository() {
    let repo = build_repo("scan");
    let snapshot = scan_project(&repo.root.display().to_string(), &options()).expect("scan");

    // ---- the worktrees ----
    assert_eq!(snapshot.worktrees.len(), 2, "{:?}", snapshot.worktrees);
    let main = &snapshot.worktrees[0];
    assert!(main.is_main);
    assert_eq!(
        main.safety.status,
        SafetyStatus::Unsafe,
        "the main worktree is never deletable"
    );
    assert!(
        main.safety
            .reasons
            .iter()
            .any(|reason| reason.code == ReasonCode::WtMain),
        "{:?}",
        main.safety.reasons
    );
    let linked = snapshot
        .worktrees
        .iter()
        .find(|record| record.path.ends_with("wt-linked"))
        .expect("the linked worktree is reported");
    assert!(!linked.is_main);
    assert_eq!(linked.branch.as_deref(), Some("linked/x"));
    assert!(!linked.is_prunable);

    // ---- the ignored build output, twice, each under its own root (D8) ----
    let main_modules = artifact_at(&snapshot, "repo", "node_modules");
    assert_eq!(main_modules.kind, "node_modules");
    assert_eq!(main_modules.ecosystem, "node");
    assert_eq!(main_modules.git_ignored, Some(true));
    assert_eq!(main_modules.git_tracked, Some(false));
    assert_eq!(
        main_modules.safety.status,
        SafetyStatus::Safe,
        "codes {:?}",
        codes(main_modules)
    );
    assert!(codes(main_modules).contains(&"af.git_ignored"));
    assert!(codes(main_modules).contains(&"af.rebuildable"));
    assert_eq!(main_modules.file_count, 1);
    assert!(main_modules.size_bytes.is_some_and(|bytes| bytes > 0));

    let linked_modules = artifact_at(&snapshot, "wt-linked", "node_modules");
    assert_eq!(linked_modules.safety.status, SafetyStatus::Safe);
    assert_ne!(
        linked_modules.path, main_modules.path,
        "each worktree is scanned under its own root"
    );
    assert_ne!(
        linked_modules.project_root, main_modules.project_root,
        "a linked worktree is not the main worktree's project root"
    );

    // ---- the same name that Git tracks is not an artifact (D7) ----
    let tracked = artifact_at(&snapshot, "repo", "dist");
    assert_eq!(tracked.git_tracked, Some(true));
    assert_eq!(tracked.git_ignored, Some(false));
    assert_eq!(tracked.safety.status, SafetyStatus::Unsafe);
    assert!(
        tracked
            .safety
            .reasons
            .iter()
            .any(|reason| reason.code == ReasonCode::AfGitTracked),
        "{:?}",
        tracked.safety.reasons
    );

    // ---- the walk never descends into an artifact (D8) ----
    for artifact in &snapshot.artifacts {
        let inside = artifact
            .path
            .trim_end_matches('/')
            .split('/')
            .collect::<Vec<_>>();
        assert!(
            !inside[..inside.len().saturating_sub(1)].contains(&"node_modules"),
            "the walk descended into {}",
            artifact.path
        );
    }

    // ---- a healthy repository produces no downgrade warnings ----
    // A failing `git ls-files` / `check-ignore` would have produced a
    // `cannot_verify` reason and a Review verdict instead of the Safe ones above.
    assert_eq!(snapshot.warnings, Vec::<String>::new());
}

#[test]
fn an_unusable_path_is_rejected_with_a_specific_reason() {
    // An empty path, a missing path, and a non-repository must stay three
    // different answers (D3).
    assert!(matches!(
        scan_project("   ", &options()),
        Err(ScanError::Empty)
    ));

    let missing = std::env::temp_dir().join("worktree-cleaner-does-not-exist-1a2b3c");
    assert!(matches!(
        scan_project(&missing.display().to_string(), &options()),
        Err(ScanError::NotFound(_))
    ));

    let plain = tempfile::TempDir::new().expect("temp dir");
    match scan_project(&plain.path().display().to_string(), &options()) {
        Err(ScanError::NotAGitRepository(_)) => {}
        other => panic!("expected NotAGitRepository, got {other:?}"),
    }
}

#[test]
fn a_scan_of_a_real_repository_changes_nothing() {
    let repo = build_repo("read-only");
    let before_status = repo.stdout_of(&["status", "--porcelain=v2", "--branch"]);
    let before_worktrees = repo.stdout_of(&["worktree", "list", "--porcelain", "-z"]);
    let before_head = repo.stdout_of(&["rev-parse", "HEAD"]);

    let snapshot = scan_project(&repo.root.display().to_string(), &options()).expect("scan");
    assert!(!snapshot.artifacts.is_empty());

    assert_eq!(
        repo.stdout_of(&["status", "--porcelain=v2", "--branch"]),
        before_status,
        "git status is identical after a scan (D1)"
    );
    assert_eq!(
        repo.stdout_of(&["worktree", "list", "--porcelain", "-z"]),
        before_worktrees,
        "a scan never prunes or adds a worktree (D1)"
    );
    assert_eq!(repo.stdout_of(&["rev-parse", "HEAD"]), before_head);
    assert!(
        !repo.root.join(".git/index.lock").exists(),
        "a scan leaves no index lock behind"
    );
    assert!(repo.root.join("node_modules/left-pad/index.js").exists());
    assert!(repo.root.join("dist/generated.js").exists());
}
