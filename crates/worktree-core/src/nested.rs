//! Nested Git checkouts inside a worktree.
//!
//! A worktree can contain another Git checkout, usually a dependency kept in an
//! ignored directory such as `.private-dependencies/…`. The outer `git status`
//! does not look inside it, and the credential walk sees it as an ordinary
//! ignored tree. Treating every `.env.example` there as an untracked secret made
//! the whole worktree `Unsafe`, even when the nested checkout was a clean,
//! remote-backed clone that could be recreated.
//!
//! The check below is deliberately evidence-based. A nested checkout is only
//! considered reproducible when all of these hold:
//!
//! - the parent worktree ignores the checkout's path;
//! - the nested worktree has no tracked changes and no untracked files;
//! - the nested repository has a remote and a remote-tracking ref;
//! - every commit reachable from its HEAD and local branches is reachable from a
//!   remote-tracking ref, so there are no local-only commits;
//! - it has no stash; and
//! - it has no ignored credential-looking file.
//!
//! A name or an ignore rule alone is never enough. An unknown answer is returned
//! as `CannotVerify`, never as reproducible (D3).

use std::path::{Path, PathBuf};

use crate::artifact::looks_like_credential;
use crate::git::GitRunner;
use crate::worktree::parse_status;

/// The answer for one nested checkout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NestedCheckoutState {
    Reproducible { path: PathBuf, evidence: String },
    NotReproducible { path: PathBuf, evidence: String },
    CannotVerify { path: PathBuf, evidence: String },
}

/// Finds the nearest nested repository containing `relative_file`.
///
/// The caller passes a credential-looking path relative to the outer worktree.
/// A `.git` directory or file is the only marker read; the `.git` file format is
/// not parsed (development contract section 2).
pub fn containing_checkout(worktree: &Path, relative_file: &Path) -> Option<PathBuf> {
    let mut current = worktree.join(relative_file).parent()?.to_path_buf();
    while current != worktree && current.starts_with(worktree) {
        if current.join(".git").exists() {
            return Some(current);
        }
        current = current.parent()?.to_path_buf();
    }
    None
}

/// Checks one nested checkout against the parent worktree and its own Git state.
pub fn inspect(runner: &dyn GitRunner, worktree: &Path, nested: &Path) -> NestedCheckoutState {
    let Some(relative) = nested.strip_prefix(worktree).ok() else {
        return cannot(nested, "the checkout is not below the worktree root");
    };
    let relative_text = relative.to_string_lossy();

    let ignored = match runner.run(
        worktree,
        &["check-ignore", "-v", "--", relative_text.as_ref()],
    ) {
        Ok(output) if output.code == Some(0) => output.stdout.trim().to_string(),
        Ok(output) if output.code == Some(1) => {
            return not_reproducible(nested, "the parent worktree does not ignore this checkout");
        }
        Ok(output) => {
            return cannot(
                nested,
                &format!(
                    "git check-ignore exited with {:?}: {}",
                    output.code,
                    output.stderr.trim()
                ),
            );
        }
        Err(error) => {
            return cannot(
                nested,
                &format!("cannot check the parent ignore rule: {error}"),
            )
        }
    };

    let status = match runner.run(
        nested,
        &["status", "--porcelain=v2", "--ignored=matching", "--branch"],
    ) {
        Ok(output) if output.success() => output,
        Ok(output) => {
            return cannot(
                nested,
                &format!(
                    "git status exited with {:?}: {}",
                    output.code,
                    output.stderr.trim()
                ),
            );
        }
        Err(error) => return cannot(nested, &format!("cannot read the nested status: {error}")),
    };
    let info = parse_status(&status.stdout);
    if !info.header_seen {
        return cannot(
            nested,
            "git status did not report the branch header, so the checkout state is unknown",
        );
    }
    if info.dirty_files > 0 || info.untracked_files > 0 {
        return not_reproducible(
            nested,
            &format!(
                "the nested checkout has {} changed files and {} untracked files",
                info.dirty_files, info.untracked_files
            ),
        );
    }
    if let Some(credential) = ignored_credential(&status.stdout) {
        return not_reproducible(
            nested,
            &format!("an ignored credential-looking file is inside: {credential}"),
        );
    }

    let remotes = match runner.run(nested, &["remote"]) {
        Ok(output) if output.success() => output.stdout,
        Ok(output) => {
            return cannot(
                nested,
                &format!(
                    "git remote exited with {:?}: {}",
                    output.code,
                    output.stderr.trim()
                ),
            );
        }
        Err(error) => return cannot(nested, &format!("cannot list the nested remotes: {error}")),
    };
    if remotes.trim().is_empty() {
        return not_reproducible(nested, "the nested checkout has no remote");
    }

    let remote_refs = match runner.run(
        nested,
        &["for-each-ref", "--format=%(refname)", "refs/remotes"],
    ) {
        Ok(output) if output.success() => output.stdout,
        Ok(output) => {
            return cannot(
                nested,
                &format!(
                    "git for-each-ref exited with {:?}: {}",
                    output.code,
                    output.stderr.trim()
                ),
            );
        }
        Err(error) => {
            return cannot(
                nested,
                &format!("cannot list the nested remote refs: {error}"),
            )
        }
    };
    let Some(remote_ref) = remote_refs.lines().find(|line| !line.trim().is_empty()) else {
        return not_reproducible(nested, "the nested checkout has no remote-tracking ref");
    };

    let local_only = match runner.run(
        nested,
        &["rev-list", "HEAD", "--branches", "--not", "--remotes"],
    ) {
        Ok(output) if output.success() => output.stdout,
        Ok(output) => {
            return cannot(
                nested,
                &format!(
                    "git rev-list exited with {:?}: {}",
                    output.code,
                    output.stderr.trim()
                ),
            );
        }
        Err(error) => {
            return cannot(
                nested,
                &format!("cannot compare the nested commits: {error}"),
            )
        }
    };
    if !local_only.trim().is_empty() {
        return not_reproducible(
            nested,
            "the nested checkout has commits that are not reachable from a remote-tracking ref",
        );
    }

    match runner.run(nested, &["stash", "list", "--format=%gd"]) {
        Ok(output) if output.success() && output.stdout.trim().is_empty() => {}
        Ok(output) if output.success() => {
            return not_reproducible(nested, "the nested checkout has a stash");
        }
        Ok(output) => {
            return cannot(
                nested,
                &format!(
                    "git stash list exited with {:?}: {}",
                    output.code,
                    output.stderr.trim()
                ),
            );
        }
        Err(error) => return cannot(nested, &format!("cannot list the nested stashes: {error}")),
    }

    let evidence = format!(
        "{relative_text} is clean, ignored by the parent worktree ({ignored}), has no stash or ignored credential, and its HEAD/local branches are reachable from {remote_ref}"
    );
    NestedCheckoutState::Reproducible {
        path: nested.to_path_buf(),
        evidence,
    }
}

fn ignored_credential(status: &str) -> Option<&str> {
    status.lines().find_map(|line| {
        let path = line.strip_prefix("! ")?;
        let name = Path::new(path).file_name()?.to_string_lossy();
        looks_like_credential(&name).then_some(path)
    })
}

fn cannot(path: &Path, evidence: &str) -> NestedCheckoutState {
    NestedCheckoutState::CannotVerify {
        path: path.to_path_buf(),
        evidence: evidence.to_string(),
    }
}

fn not_reproducible(path: &Path, evidence: &str) -> NestedCheckoutState {
    NestedCheckoutState::NotReproducible {
        path: path.to_path_buf(),
        evidence: evidence.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::testing::{Canned, FakeGitRunner};
    use crate::git::GitOutput;
    use std::fs;
    use tempfile::tempdir;

    const CLEAN_STATUS: &str = "\
# branch.oid 5ea49a9a27d681e25add68f5bd26fad7f0c78b7f
# branch.head (detached)
";

    fn out(stdout: &str) -> Canned {
        Canned::Out(GitOutput {
            stdout: stdout.to_string(),
            stderr: String::new(),
            code: Some(0),
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn rules(
        ignore: &str,
        status: &str,
        remotes: &str,
        refs: &str,
        local_only: &str,
        stash: &str,
    ) -> Vec<(Vec<String>, Canned)> {
        vec![
            (
                vec![
                    "check-ignore".into(),
                    "-v".into(),
                    "--".into(),
                    "vendor/private".into(),
                ],
                out(ignore),
            ),
            (
                vec![
                    "status".into(),
                    "--porcelain=v2".into(),
                    "--ignored=matching".into(),
                    "--branch".into(),
                ],
                out(status),
            ),
            (vec!["remote".into()], out(remotes)),
            (
                vec![
                    "for-each-ref".into(),
                    "--format=%(refname)".into(),
                    "refs/remotes".into(),
                ],
                out(refs),
            ),
            (
                vec![
                    "rev-list".into(),
                    "HEAD".into(),
                    "--branches".into(),
                    "--not".into(),
                    "--remotes".into(),
                ],
                out(local_only),
            ),
            (
                vec!["stash".into(), "list".into(), "--format=%gd".into()],
                out(stash),
            ),
        ]
    }

    fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let root = tempdir().expect("temp dir");
        let nested = root.path().join("vendor/private");
        fs::create_dir_all(nested.join(".git")).expect("create nested git marker");
        let credential = nested.join("services/wechat-gateway/.env.example");
        fs::create_dir_all(credential.parent().expect("credential parent")).expect("create dirs");
        fs::write(&credential, "TOKEN=example\n").expect("write credential");
        (root, nested, credential)
    }

    #[test]
    fn finds_the_nearest_nested_checkout() {
        let (root, nested, credential) = fixture();
        let relative = credential.strip_prefix(root.path()).expect("relative");
        assert_eq!(containing_checkout(root.path(), relative), Some(nested));
    }

    #[test]
    fn accepts_a_clean_ignored_remote_backed_checkout() {
        let (root, nested, _credential) = fixture();
        let runner = FakeGitRunner::new(rules(
            ".gitignore:1:.private-dependencies/\n",
            CLEAN_STATUS,
            "origin\n",
            "refs/remotes/origin/main\n",
            "",
            "",
        ));
        let state = inspect(&runner, root.path(), &nested);
        assert!(matches!(state, NestedCheckoutState::Reproducible { .. }));
        assert!(runner.called(&["remote"]));
    }

    #[test]
    fn rejects_a_nested_checkout_not_ignored_by_the_parent() {
        let (root, nested, _credential) = fixture();
        let mut canned = rules(
            "",
            CLEAN_STATUS,
            "origin\n",
            "refs/remotes/origin/main\n",
            "",
            "",
        );
        canned[0].1 = Canned::Out(GitOutput {
            stdout: String::new(),
            stderr: String::new(),
            code: Some(1),
        });
        let state = inspect(&FakeGitRunner::new(canned), root.path(), &nested);
        assert!(matches!(state, NestedCheckoutState::NotReproducible { .. }));
    }

    #[test]
    fn rejects_dirty_or_untracked_nested_contents() {
        let (root, nested, _credential) = fixture();
        let dirty =
            "# branch.oid abc\n# branch.head (detached)\n1 .M N... 100644 100644 100644 a a a.txt\n";
        let state = inspect(
            &FakeGitRunner::new(rules(
                ".gitignore:1:.private-dependencies/\n",
                dirty,
                "origin\n",
                "refs/remotes/origin/main\n",
                "",
                "",
            )),
            root.path(),
            &nested,
        );
        assert!(matches!(state, NestedCheckoutState::NotReproducible { .. }));
    }

    #[test]
    fn rejects_ignored_credentials_inside_the_nested_checkout() {
        let (root, nested, _credential) = fixture();
        let status = "# branch.oid abc\n# branch.head (detached)\n! services/wechat-gateway/.env\n";
        let state = inspect(
            &FakeGitRunner::new(rules(
                ".gitignore:1:.private-dependencies/\n",
                status,
                "origin\n",
                "refs/remotes/origin/main\n",
                "",
                "",
            )),
            root.path(),
            &nested,
        );
        assert!(matches!(state, NestedCheckoutState::NotReproducible { .. }));
    }

    #[test]
    fn rejects_local_only_commits_or_a_stash() {
        let (root, nested, _credential) = fixture();
        let local_only = "0123456789abcdef\n";
        let state = inspect(
            &FakeGitRunner::new(rules(
                ".gitignore:1:.private-dependencies/\n",
                CLEAN_STATUS,
                "origin\n",
                "refs/remotes/origin/main\n",
                local_only,
                "",
            )),
            root.path(),
            &nested,
        );
        assert!(matches!(state, NestedCheckoutState::NotReproducible { .. }));

        let stash = "stash@{0}\n";
        let state = inspect(
            &FakeGitRunner::new(rules(
                ".gitignore:1:.private-dependencies/\n",
                CLEAN_STATUS,
                "origin\n",
                "refs/remotes/origin/main\n",
                "",
                stash,
            )),
            root.path(),
            &nested,
        );
        assert!(matches!(state, NestedCheckoutState::NotReproducible { .. }));
    }

    #[test]
    fn an_unreadable_nested_status_is_unknown() {
        let (root, nested, _credential) = fixture();
        let mut canned = rules(
            "",
            CLEAN_STATUS,
            "origin\n",
            "refs/remotes/origin/main\n",
            "",
            "",
        );
        canned[1].1 = Canned::Timeout;
        let state = inspect(&FakeGitRunner::new(canned), root.path(), &nested);
        assert!(matches!(state, NestedCheckoutState::CannotVerify { .. }));
    }
}
