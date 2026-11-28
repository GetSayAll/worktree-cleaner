//! The CLI transport is the reference; the in-process transport is what ships.
//!
//! These tests are the safety net for the compatibility layer: they run both
//! transports against one real repository and compare the values the decision
//! tables consume. A byte-level difference in porcelain is only interesting when it
//! changes a parsed fact, so the comparison is on the parsed answers.

// The two sides of every comparison are named after the transport (`cli_` and
// `lib_`); the similarity is the point, not an accident.
#![allow(clippy::similar_names)]

mod common;

use std::path::Path;
use std::time::Duration;

use common::Repo;
use worktree_core::worktree::{parse_status, parse_worktree_list};
use worktree_core::{Git2Runner, GitRunner, ProcessGitRunner};

fn transports() -> (ProcessGitRunner, Git2Runner) {
    (
        ProcessGitRunner::new(Duration::from_secs(20)),
        Git2Runner::new(Duration::from_secs(20)),
    )
}

fn sorted_lines(text: &str) -> Vec<String> {
    let mut lines: Vec<String> = text
        .lines()
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect();
    lines.sort();
    lines
}

fn nul_paths(text: &str) -> Vec<String> {
    let mut paths: Vec<String> = text
        .split('\0')
        .filter(|path| !path.is_empty())
        .map(str::to_string)
        .collect();
    paths.sort();
    paths
}

fn run_both(
    cli: &ProcessGitRunner,
    lib: &Git2Runner,
    repo: &Path,
    args: &[&str],
) -> (worktree_core::GitOutput, worktree_core::GitOutput) {
    (
        cli.run(repo, args).expect("cli transport"),
        lib.run(repo, args).expect("in-process transport"),
    )
}

#[test]
fn both_transports_read_the_same_worktrees_and_status() {
    let repo = Repo::new("transport-parity");
    std::fs::write(repo.root.join(".gitignore"), "node_modules/\ndist/\n").expect(".gitignore");
    std::fs::write(repo.root.join("package.json"), "{}\n").expect("manifest");
    std::fs::write(repo.root.join("src.js"), "console.log(1);\n").expect("source");
    std::fs::create_dir_all(repo.root.join("node_modules/left-pad")).expect("modules");
    std::fs::write(
        repo.root.join("node_modules/left-pad/index.js"),
        "module.exports = 1;\n",
    )
    .expect("module");
    std::fs::create_dir_all(repo.root.join("dist")).expect("dist");
    std::fs::write(repo.root.join("dist/generated.js"), "// generated\n").expect("dist file");
    repo.git(&["add", "-f", "dist/generated.js"]);
    repo.git(&["add", ".gitignore", "package.json", "src.js"]);
    repo.git_dated_in(&repo.root, &["commit", "--quiet", "-m", "layout"], 30);

    let linked = repo.add_worktree("wt-linked", "linked/x");
    std::fs::write(linked.join("linked.txt"), "linked\n").expect("linked file");
    repo.git_in(&linked, &["add", "linked.txt"]);
    repo.git_dated_in(&linked, &["commit", "--quiet", "-m", "linked"], 10);

    // A tracked change and an untracked file in the main worktree.
    std::fs::write(repo.root.join("src.js"), "console.log(2);\n").expect("modified");
    std::fs::write(repo.root.join("notes.md"), "notes\n").expect("untracked");

    let (cli, lib) = transports();

    // ---- worktree list ----
    let (cli_list, lib_list) = run_both(
        &cli,
        &lib,
        &repo.root,
        &["worktree", "list", "--porcelain", "-z"],
    );
    let cli_entries = parse_worktree_list(&cli_list.stdout);
    let lib_entries = parse_worktree_list(&lib_list.stdout);
    assert_eq!(cli_entries.len(), lib_entries.len());
    for (cli, lib) in cli_entries.iter().zip(&lib_entries) {
        assert_eq!(cli.path, lib.path);
        assert_eq!(
            cli.head,
            lib.head,
            "HEAD differs for {}",
            cli.path.display()
        );
        assert_eq!(
            cli.branch,
            lib.branch,
            "branch differs for {}",
            cli.path.display()
        );
        assert_eq!(cli.is_bare, lib.is_bare);
        assert_eq!(cli.is_detached, lib.is_detached);
        assert_eq!(cli.is_locked, lib.is_locked);
        assert_eq!(cli.is_prunable, lib.is_prunable);
    }

    // ---- status, in both the main and the linked worktree ----
    for worktree in [&repo.root, &linked] {
        let (cli_status, lib_status) = run_both(
            &cli,
            &lib,
            worktree,
            &["status", "--porcelain=v2", "--branch"],
        );
        let cli = parse_status(&cli_status.stdout);
        let lib = parse_status(&lib_status.stdout);
        assert_eq!(cli, lib, "status differs in {}", worktree.display());
    }

    // ---- exact untracked list ----
    let (cli_others, lib_others) = run_both(
        &cli,
        &lib,
        &repo.root,
        &["ls-files", "--others", "--exclude-standard"],
    );
    assert_eq!(
        sorted_lines(&cli_others.stdout),
        sorted_lines(&lib_others.stdout)
    );

    // ---- tracked lookup and tracked-file listing ----
    let (cli_tracked, lib_tracked) = run_both(
        &cli,
        &lib,
        &repo.root,
        &["ls-files", "--error-unmatch", "--", "src.js"],
    );
    assert_eq!(cli_tracked.code, lib_tracked.code);
    assert_eq!(
        sort_paths(&cli_tracked.stdout),
        sort_paths(&lib_tracked.stdout)
    );

    let (cli_dist, lib_dist) = run_both(&cli, &lib, &repo.root, &["ls-files", "--", "dist"]);
    assert_eq!(sort_paths(&cli_dist.stdout), sort_paths(&lib_dist.stdout));

    // The branch-ref-preserved rule reads this NUL list to find ignored entries.
    // `--directory` collapses a tree, so both transports must agree on the
    // top-level entries rather than on every file below them.
    let (cli_ignored, lib_ignored) = run_both(
        &cli,
        &lib,
        &repo.root,
        &[
            "ls-files",
            "--others",
            "--ignored",
            "--exclude-standard",
            "--directory",
            "-z",
        ],
    );
    assert_eq!(
        nul_paths(&cli_ignored.stdout),
        nul_paths(&lib_ignored.stdout)
    );

    // ---- ignore answers: the tracked `dist` is not ignored, `node_modules` is ----
    for (path, expected) in [("dist", 1), ("node_modules", 0)] {
        let (cli_ignore, lib_ignore) =
            run_both(&cli, &lib, &repo.root, &["check-ignore", "-v", "--", path]);
        assert_eq!(
            cli_ignore.code,
            Some(expected),
            "CLI ignore answer for {path}"
        );
        assert_eq!(
            lib_ignore.code,
            Some(expected),
            "libgit2 ignore answer for {path}"
        );
    }

    // ---- refs and merge-base ----
    for reference in ["refs/heads/main", "refs/heads/linked/x"] {
        let (cli_ref, lib_ref) = run_both(
            &cli,
            &lib,
            &repo.root,
            &["rev-parse", "--verify", "--quiet", reference],
        );
        assert_eq!(cli_ref.code, lib_ref.code);
        assert_eq!(cli_ref.stdout.trim(), lib_ref.stdout.trim());
    }

    let head = parse_status(
        &cli.run(&repo.root, &["status", "--porcelain=v2", "--branch"])
            .expect("main status")
            .stdout,
    )
    .head
    .expect("main head");
    let linked_head = parse_status(
        &cli.run(&linked, &["status", "--porcelain=v2", "--branch"])
            .expect("linked status")
            .stdout,
    )
    .head
    .expect("linked head");

    let (cli_ancestor, lib_ancestor) = run_both(
        &cli,
        &lib,
        &repo.root,
        &["merge-base", "--is-ancestor", "--", &head, "main"],
    );
    assert_eq!(cli_ancestor.code, lib_ancestor.code);
    assert_eq!(cli_ancestor.code, Some(0));

    let (cli_not, lib_not) = run_both(
        &cli,
        &lib,
        &repo.root,
        &["merge-base", "--is-ancestor", "--", &linked_head, "main"],
    );
    assert_eq!(cli_not.code, lib_not.code);
    assert_eq!(cli_not.code, Some(1));

    // ---- diff ----
    let range = "main...linked/x";
    let (cli_diff, lib_diff) = run_both(
        &cli,
        &lib,
        &repo.root,
        &["diff", "--name-only", range, "--"],
    );
    assert_eq!(sort_paths(&cli_diff.stdout), sort_paths(&lib_diff.stdout));
    assert_eq!(sort_paths(&lib_diff.stdout), vec!["linked.txt".to_string()]);

    let (cli_quiet, lib_quiet) = run_both(
        &cli,
        &lib,
        &repo.root,
        &["diff", "--quiet", &linked_head, &head, "--", "linked.txt"],
    );
    assert_eq!(cli_quiet.code, lib_quiet.code);
    assert_eq!(cli_quiet.code, Some(1));

    // ---- stash count ----
    repo.git(&["stash", "push", "--quiet", "-m", "parity stash"]);
    let (cli_stash, lib_stash) =
        run_both(&cli, &lib, &repo.root, &["stash", "list", "--format=%gd"]);
    assert_eq!(sorted_lines(&cli_stash.stdout).len(), 1);
    assert_eq!(
        sorted_lines(&cli_stash.stdout),
        sorted_lines(&lib_stash.stdout)
    );
}

#[test]
fn both_transports_remove_a_worktree_and_leave_the_others() {
    let repo = Repo::new("transport-remove-parity");
    let cli_worktree = repo.add_worktree("wt-remove-cli", "remove/cli");
    let lib_worktree = repo.add_worktree("wt-remove-lib", "remove/lib");
    repo.add_worktree("wt-keep", "keep/me");

    let (cli, lib) = transports();
    let cli_args = [
        "worktree",
        "remove",
        "--",
        cli_worktree.to_str().expect("utf-8 path"),
    ];
    assert!(cli
        .run(&repo.root, &cli_args)
        .expect("cli remove")
        .success());

    let lib_args = [
        "worktree",
        "remove",
        "--",
        lib_worktree.to_str().expect("utf-8 path"),
    ];
    assert!(lib
        .run(&repo.root, &lib_args)
        .expect("libgit2 remove")
        .success());

    let remaining = repo.worktree_paths();
    // macOS reports /private/var for a /var path, so compare the last component.
    let named = |name: &str| {
        remaining
            .iter()
            .any(|path| Path::new(path).file_name().is_some_and(|leaf| leaf == name))
    };
    assert!(
        named("wt-keep"),
        "the untouched worktree must remain: {remaining:?}"
    );
    assert!(
        !named("wt-remove-cli"),
        "the CLI removal must disappear: {remaining:?}"
    );
    assert!(
        !named("wt-remove-lib"),
        "the in-process removal must disappear: {remaining:?}"
    );
    assert!(!cli_worktree.exists());
    assert!(!lib_worktree.exists());
}

fn sort_paths(text: &str) -> Vec<String> {
    sorted_lines(text)
}
