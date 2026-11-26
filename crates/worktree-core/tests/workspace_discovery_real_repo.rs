//! Workspace discovery against real linked worktrees.
//!
//! A linked worktree has its own `.git` file, so a shallow directory walk sees
//! it as another repository root. Git's worktree list, however, belongs to the
//! whole repository family. This test pins the fold: three roots under one
//! selected folder must be one row with one count.

mod common;

use common::Repo;
use worktree_core::discover_workspace;

#[test]
fn linked_worktrees_are_one_repository_with_one_worktree_count() {
    let repo = Repo::new("workspace-linked-worktrees");
    repo.add_worktree("wt-one", "linked/one");
    repo.add_worktree("wt-two", "linked/two");

    let snapshot = discover_workspace(repo.base.to_str().expect("utf-8 workspace root"))
        .expect("discover workspace");

    assert_eq!(
        snapshot.repositories.len(),
        1,
        "linked worktrees are not separate repositories: {:?}",
        snapshot.repositories
    );
    let repository = &snapshot.repositories[0];
    assert_eq!(repository.worktree_count, Some(3));
    assert!(
        repository.path.ends_with("/repo"),
        "the main worktree is shown when it is inside the selected root: {}",
        repository.path
    );
}
