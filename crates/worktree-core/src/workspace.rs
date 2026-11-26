//! Workspace discovery.
//!
//! A developer root such as `~/Develop/Src` is usually not itself a Git
//! repository; it contains many repositories. `scan_project` is intentionally
//! scoped to one repository, because every worktree verdict is anchored to that
//! repository's default branch. This module finds the repository roots without
//! changing that model: the user chooses a root, sees the repositories below it,
//! and then scans one through the ordinary project path.
//!
//! Discovery is read-only and deliberately shallow. It skips generated trees
//! (`.worktrees`, dependency stores, build output) and hidden directories, and it
//! never follows a symlink. A repository found at one level is reported once and
//! is not descended into, so nested worktrees are not mistaken for separate
//! projects.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::git::GitRunner;
use crate::git2_runner::Git2Runner;
use crate::path::{expand_tilde, is_in_system_trash};
use crate::timefmt::format_rfc3339;
use crate::worktree::parse_worktree_list;
use crate::{ScanError, WorkspaceRepository, WorkspaceSnapshot, WORKSPACE_SCHEMA_VERSION};

/// How deep below the selected root discovery may look.
///
/// Repositories normally sit one or two levels below a developer root; a deeper
/// limit mostly finds vendored or generated trees. A repository that is deeper
/// than this can still be selected directly.
const MAX_DEPTH: usize = 5;

/// A last-resort bound for a root that contains an enormous number of entries.
const MAX_ENTRIES: usize = 250_000;

/// A last-resort bound for the number of repositories reported at once.
const MAX_REPOSITORIES: usize = 2_000;

/// Names that are never repositories worth reporting, or are generated trees
/// whose contents belong to a repository already found.
fn skip_directory(name: &str) -> bool {
    name.starts_with('.')
        || matches!(
            name,
            "node_modules"
                | "target"
                | "build"
                | "dist"
                | "out"
                | ".build"
                | ".swiftpm"
                | "Pods"
                | "Carthage"
                | "DerivedData"
                | "build-cache"
                | "Library"
                | "Applications"
                | "Downloads"
                | "Movies"
                | "Music"
                | "Pictures"
        )
}

fn is_repository_root(path: &Path) -> bool {
    let git = path.join(".git");
    match std::fs::symlink_metadata(git) {
        Ok(metadata) => metadata.file_type().is_dir() || metadata.file_type().is_file(),
        Err(_) => false,
    }
}

/// Whether `path` itself is inside a repository, even though it is not the
/// repository root.
///
/// Selecting a subdirectory of a repository is a normal scan path; discovery
/// must not turn it into "no repositories found" just because the `.git` entry
/// lives above it. The scan still starts at the selected directory, which is how
/// `scan_project` has always behaved.
fn has_repository_ancestor(path: &Path) -> bool {
    path.ancestors().skip(1).any(is_repository_root)
}

/// The worktree count for one repository, or `None` when Git could not answer.
///
/// The count is display-only. A failure is a warning on the workspace result,
/// never an error: discovery still has a useful repository path to offer.
fn worktree_count(path: &Path, warnings: &mut Vec<String>) -> Option<u32> {
    let runner = Git2Runner::new(Duration::from_secs(10));
    match runner.run(path, &["worktree", "list", "--porcelain", "-z"]) {
        Ok(output) if output.success() => {
            let count = parse_worktree_list(&output.stdout).len();
            if count == 0 {
                warnings.push(format!(
                    "Git reported no worktrees for {}; its count is unknown",
                    path.display()
                ));
                None
            } else {
                u32::try_from(count).ok()
            }
        }
        Ok(output) => {
            warnings.push(format!(
                "the worktree count for {} could not be read: git worktree list exited with {:?}: {}",
                path.display(),
                output.code,
                output.stderr.trim()
            ));
            None
        }
        Err(error) => {
            warnings.push(format!(
                "the worktree count for {} could not be read: {error}",
                path.display()
            ));
            None
        }
    }
}

fn repository(path: &Path, warnings: &mut Vec<String>) -> WorkspaceRepository {
    let fallback = path.to_string_lossy().into_owned();
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map_or(fallback, str::to_string);
    WorkspaceRepository {
        path: host_path(path),
        name,
        worktree_count: worktree_count(path, warnings),
    }
}

/// The path as this host writes it.
///
/// Discovery starts from the string the user typed, and `read_dir` keeps that
/// string as the prefix of every child it yields: a root typed `C:/Src` produced
/// `C:/Src\repo`. Both are valid on Windows, but every other path in the window —
/// and every path Git reports — uses the platform separator, so one line showed
/// two. The snapshot is a display string and a scan path, so it is written the
/// way the host writes paths.
fn host_path(path: &Path) -> String {
    let text = path.to_string_lossy();
    if cfg!(windows) {
        // A file name cannot contain `/` on Windows; every one of them is a
        // separator here, including in a UNC prefix.
        text.replace('/', "\\")
    } else {
        text.into_owned()
    }
}

/// Finds the Git repository roots below `path`.
///
/// If `path` is itself a repository, the result contains exactly that repository.
/// Otherwise the walk is breadth-first, shallow, and never follows a symlink.
/// An empty result is a successful answer: the caller can tell the user that the
/// folder contains no repositories rather than pretending the folder was invalid.
pub fn discover_workspace(path: &str) -> Result<WorkspaceSnapshot, ScanError> {
    let raw = path.trim();
    if raw.is_empty() {
        return Err(ScanError::Empty);
    }
    let root = expand_tilde(raw);
    if !root.is_dir() {
        return Err(ScanError::NotFound(root.display().to_string()));
    }

    let mut warnings = Vec::new();
    let mut repositories = Vec::new();
    let mut queued = 0usize;

    if is_in_system_trash(&root) {
        warnings.push(format!(
            "{} is inside the system Trash; discovery does not read the Trash",
            root.display()
        ));
        return Ok(WorkspaceSnapshot {
            schema_version: WORKSPACE_SCHEMA_VERSION,
            root: host_path(&root),
            scanned_at: format_rfc3339(SystemTime::now()),
            repositories,
            warnings,
        });
    }

    if is_repository_root(&root) || has_repository_ancestor(&root) {
        repositories.push(repository(&root, &mut warnings));
    } else {
        let mut stack: Vec<(PathBuf, usize)> = vec![(root.clone(), 0)];
        let mut visited = 0usize;
        while let Some((directory, depth)) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&directory) else {
                continue;
            };
            for entry in entries.flatten() {
                visited += 1;
                queued += 1;
                if visited > MAX_ENTRIES {
                    warnings.push(format!(
                        "workspace discovery stopped after {MAX_ENTRIES} entries; some repositories may be missing"
                    ));
                    break;
                }
                let name = entry.file_name().to_string_lossy().into_owned();
                let path = entry.path();
                if is_in_system_trash(&path) {
                    continue;
                }
                // Symlinks are not followed: a link can point outside the selected
                // root, and discovery must not broaden the user's chosen scope.
                let Ok(file_type) = entry.file_type() else {
                    continue;
                };
                if file_type.is_symlink() || !file_type.is_dir() {
                    continue;
                }
                if is_repository_root(&path) {
                    repositories.push(repository(&path, &mut warnings));
                    if repositories.len() >= MAX_REPOSITORIES {
                        warnings.push(format!(
                            "workspace discovery stopped after {MAX_REPOSITORIES} repositories"
                        ));
                        break;
                    }
                    continue;
                }
                if depth < MAX_DEPTH && !skip_directory(&name) {
                    stack.push((path, depth + 1));
                }
            }
            if repositories.len() >= MAX_REPOSITORIES || visited > MAX_ENTRIES {
                break;
            }
        }
    }

    repositories.sort_by(|left, right| left.path.cmp(&right.path));
    if repositories.is_empty() {
        warnings.push(format!(
            "no Git repositories were found below {}",
            root.display()
        ));
    }
    if queued > MAX_ENTRIES {
        warnings.push(format!(
            "workspace discovery looked at {queued} entries, above its {MAX_ENTRIES} entry limit"
        ));
    }

    Ok(WorkspaceSnapshot {
        schema_version: WORKSPACE_SCHEMA_VERSION,
        root: host_path(&root),
        scanned_at: format_rfc3339(SystemTime::now()),
        repositories,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_workspace() -> tempfile::TempDir {
        tempfile::TempDir::new().expect("temp workspace")
    }

    fn make_repo(path: &Path) {
        std::fs::create_dir_all(path.join(".git")).expect("create .git");
    }

    #[test]
    fn reports_a_repository_root_itself() {
        let root = temp_workspace();
        make_repo(root.path());
        let snapshot = discover_workspace(root.path().to_str().expect("path")).expect("discover");
        assert_eq!(snapshot.repositories.len(), 1);
        assert_eq!(
            snapshot.repositories[0].path,
            root.path().display().to_string()
        );
        // The worktree count is display-only; a bare `.git` directory has no
        // readable worktree list, so it stays unknown rather than zero.
        assert_eq!(snapshot.repositories[0].worktree_count, None);
    }

    #[test]
    fn discovers_nested_repositories_once_and_skips_generated_trees() {
        let root = temp_workspace();
        make_repo(&root.path().join("one"));
        make_repo(&root.path().join("nested").join("two"));
        make_repo(&root.path().join("two").join(".worktrees").join("duplicate"));
        make_repo(&root.path().join("two").join("node_modules").join("ignored"));

        let snapshot = discover_workspace(root.path().to_str().expect("path")).expect("discover");
        let paths: Vec<&str> = snapshot
            .repositories
            .iter()
            .map(|repo| repo.path.as_str())
            .collect();
        // Paths carry the host's separator, so compare components rather than
        // matching a `/`-shaped string: `C:\…\one` must satisfy this too.
        let has_component = |name: &str| {
            paths.iter().any(|path| {
                Path::new(path)
                    .components()
                    .any(|part| part.as_os_str() == name)
            })
        };
        assert!(paths.iter().any(|path| Path::new(path).ends_with("one")));
        assert!(paths
            .iter()
            .any(|path| Path::new(path).ends_with("nested/two")));
        assert!(
            !has_component(".worktrees"),
            "a worktree is not a separate workspace repository"
        );
        assert!(
            !has_component("node_modules"),
            "dependency trees are not searched for repositories"
        );
    }

    /// A root typed with forward slashes used to yield mixed paths — `C:/Src\repo`
    /// — because `read_dir` keeps the prefix it was given. The window then showed
    /// two separator styles on one line, and the path travelled into the scan and
    /// the recent-projects list in that shape.
    #[cfg(windows)]
    #[test]
    fn a_root_typed_with_slashes_is_reported_with_backslashes() {
        let root = temp_workspace();
        make_repo(&root.path().join("one"));
        let typed = root.path().to_string_lossy().replace('\\', "/");

        let snapshot = discover_workspace(&typed).expect("discover");
        assert_eq!(snapshot.root, root.path().display().to_string());
        assert_eq!(snapshot.repositories.len(), 1);
        assert!(
            !snapshot.repositories[0].path.contains('/'),
            "{} still carries a forward slash",
            snapshot.repositories[0].path
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_repository_is_not_followed() {
        let root = temp_workspace();
        let outside = temp_workspace();
        make_repo(outside.path());
        std::os::unix::fs::symlink(outside.path(), root.path().join("linked"))
            .expect("create symlink");
        let snapshot = discover_workspace(root.path().to_str().expect("path")).expect("discover");
        assert!(snapshot.repositories.is_empty());
    }

    #[test]
    fn a_subdirectory_of_a_repository_is_still_a_scan_path() {
        let root = temp_workspace();
        make_repo(root.path());
        let subdirectory = root.path().join("Sources").join("App");
        std::fs::create_dir_all(&subdirectory).expect("subdir");
        let snapshot = discover_workspace(subdirectory.to_str().expect("path")).expect("discover");
        assert_eq!(snapshot.repositories.len(), 1);
        assert_eq!(
            snapshot.repositories[0].path,
            subdirectory.display().to_string()
        );
    }

    #[test]
    fn an_empty_folder_is_a_successful_empty_answer_with_a_warning() {
        let root = temp_workspace();
        std::fs::create_dir(root.path().join("plain")).expect("plain dir");
        let snapshot = discover_workspace(root.path().to_str().expect("path")).expect("discover");
        assert!(snapshot.repositories.is_empty());
        assert!(snapshot
            .warnings
            .iter()
            .any(|warning| warning.contains("no Git repositories")));
    }
}
