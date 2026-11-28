//! Workspace discovery.
//!
//! A developer root such as `~/Develop/Src` is usually not itself a Git
//! repository; it contains many repositories. `scan_project` is intentionally
//! scoped to one repository, because every worktree verdict is anchored to that
//! repository's default branch. This module finds the repository roots without
//! changing that model: the user chooses a root, sees the repositories below it,
//! and then scans one through the ordinary project path.
//!
//! Discovery is read-only and deliberately shallow. It skips generated trees,
//! dependency checkouts, directories tagged with `CACHEDIR.TAG` and hidden
//! directories, and it never follows a symlink. A repository found at one level
//! is reported once and is not descended into, so nested worktrees are not
//! mistaken for separate projects.
//!
//! Selecting a repository also searches its parent folder for independent clones
//! whose `origin` remote matches. This is automatic discovery, not a
//! cross-repository cleanup model: every returned repository is still scanned
//! and cleaned on its own.

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::git::GitRunner;
use crate::git2_runner::Git2Runner;
use crate::path::{expand_tilde, host_path, is_in_system_trash};
use crate::timefmt::format_rfc3339;
use crate::worktree::{parse_worktree_list, WorktreeEntry};
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
                | "checkouts"
                | "SourcePackages"
                | "ci-temp"
                | "Library"
                | "Applications"
                | "Downloads"
                | "Movies"
                | "Music"
                | "Pictures"
        )
}

/// A directory that declares itself a cache through the CACHEDIR.TAG convention.
///
/// SwiftPM writes this tag at the root of a build directory. A dependency
/// checkout below that root is a disposable copy of a third-party package, not a
/// local development repository, and each build can contain another copy of the
/// same package. The tag lets discovery stop at the build root without matching
/// a tool-specific directory name.
fn is_tagged_cache(path: &Path) -> bool {
    path.join("CACHEDIR.TAG").is_file()
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

/// One worktree list, or `None` when Git could not answer.
///
/// The count is display-only. A failure is a warning on the workspace result,
/// never an error: discovery still has a useful repository path to offer.
fn worktree_entries(path: &Path, warnings: &mut Vec<String>) -> Option<Vec<WorktreeEntry>> {
    let runner = Git2Runner::new(Duration::from_secs(10));
    match runner.run(path, &["worktree", "list", "--porcelain", "-z"]) {
        Ok(output) if output.success() => {
            let entries = parse_worktree_list(&output.stdout);
            if entries.is_empty() {
                warnings.push(format!(
                    "Git reported no worktrees for {}; its count is unknown",
                    path.display()
                ));
                None
            } else {
                Some(entries)
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

/// A discovered root plus the main worktree that identifies its repository.
///
/// Every linked worktree answers `git worktree list` with the same first entry,
/// so that entry is the identity used to fold the family into one row.
struct DiscoveredRepository {
    repository: WorkspaceRepository,
    main_worktree: Option<PathBuf>,
}

fn repository(path: &Path, warnings: &mut Vec<String>) -> DiscoveredRepository {
    let entries = worktree_entries(path, warnings);
    let main_worktree = entries
        .as_ref()
        .and_then(|entries| entries.first())
        .map(|entry| entry.path.clone());
    let worktree_count = entries
        .as_ref()
        .and_then(|entries| u32::try_from(entries.len()).ok());
    let fallback = path.to_string_lossy().into_owned();
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map_or(fallback, str::to_string);
    DiscoveredRepository {
        repository: WorkspaceRepository {
            path: host_path(path),
            name,
            worktree_count,
        },
        main_worktree,
    }
}

fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Reads the URL of the repository's `origin` remote.
///
/// `origin` is the clone's primary identity. Other remotes can point at related
/// projects: `open-voice-bridge` also has a `macro-platform` remote, and matching
/// every remote would report `sayall-macro-platform` as a clone of the same
/// repository. Discovery only needs this one key; it never contacts a remote. A
/// repository without `origin`, or one whose configuration cannot be read, has
/// no key and cannot be grouped with another repository.
fn origin_remote_key(path: &Path) -> Option<String> {
    let repo = git2::Repository::open(path).ok()?;
    let remote = repo.find_remote("origin").ok()?;
    remote.url().and_then(remote_key)
}

/// Reduces the transport spellings of one remote to the same identity.
///
/// `origin` may be written as `https://host/owner/repo.git`,
/// `git@host:owner/repo.git`, or `ssh://git@host/owner/repo`. A credential in an
/// HTTPS URL is removed with the other user information: it is not part of the
/// repository identity and must not be compared or displayed.
fn remote_key(url: &str) -> Option<String> {
    let trimmed = url.trim().trim_end_matches('/');
    let without_git = trimmed.strip_suffix(".git").unwrap_or(trimmed);
    let without_git = without_git.trim_end_matches('/');
    if without_git.is_empty() {
        return None;
    }

    if let Some(rest) = without_git.strip_prefix("git@") {
        let (host, path) = rest.split_once(':')?;
        return Some(format!(
            "{}/{}",
            host.to_ascii_lowercase(),
            path.trim_start_matches('/')
        ));
    }

    if let Some((_, rest)) = without_git.split_once("://") {
        let host_and_path = rest.split_once('@').map_or(rest, |(_, rest)| rest);
        let (host, path) = host_and_path.split_once('/')?;
        return Some(format!(
            "{}/{}",
            host.to_ascii_lowercase(),
            path.trim_start_matches('/')
        ));
    }

    Some(without_git.to_string())
}

fn is_independent_clone(candidate: &DiscoveredRepository) -> bool {
    let path = Path::new(&candidate.repository.path);
    is_independent_clone_path(path)
        && candidate
            .main_worktree
            .as_ref()
            .is_some_and(|main| canonical(main) == canonical(path))
}

/// Whether a repository root has its own `.git` directory.
///
/// A linked worktree and a submodule carry a `.git` file instead; they are not
/// independent clones even though the shallow walk sees them as repository
/// roots. This check runs before the worktree list, so an unrelated broken
/// gitfile in the parent folder cannot produce a discovery warning.
fn is_independent_clone_path(path: &Path) -> bool {
    is_repository_root(path)
        && std::fs::symlink_metadata(path.join(".git"))
            .is_ok_and(|metadata| metadata.file_type().is_dir())
}

/// Whether a repository's parent is a useful place to look for sibling clones.
///
/// The home directory itself and the filesystem roots are not searched. The
/// component floor is the same structural guard used elsewhere: a project needs
/// at least three normal components before its parent is a developer scope.
fn is_searchable_parent(path: &Path) -> bool {
    if !path.is_dir() || is_repository_root(path) || has_repository_ancestor(path) {
        return false;
    }
    path.components()
        .filter(|component| matches!(component, Component::Normal(_)))
        .count()
        >= 3
}

/// Folds linked worktrees into the repository they belong to.
///
/// A linked worktree has its own `.git` file, so the shallow walk finds it as a
/// repository root; its worktree list, however, contains the main worktree and
/// every other linked worktree, and repeats the same count. Reporting one row
/// per discovered root would therefore count the same repository several times.
/// The first porcelain entry is the main worktree and is the same for every
/// member of the family, so it is the grouping key. The main worktree's path is
/// shown when it is inside the selected root; otherwise the lexicographically
/// first discovered member is shown, keeping the row inside the user's scope.
fn dedupe_repositories(
    candidates: Vec<DiscoveredRepository>,
    root: &Path,
) -> Vec<WorkspaceRepository> {
    let root = canonical(root);
    let mut families: BTreeMap<PathBuf, Vec<DiscoveredRepository>> = BTreeMap::new();
    let mut unreadable = Vec::new();

    for candidate in candidates {
        if let Some(main_worktree) = candidate.main_worktree.as_ref() {
            families
                .entry(canonical(main_worktree))
                .or_default()
                .push(candidate);
        } else {
            unreadable.push(candidate.repository);
        }
    }

    let mut repositories = unreadable;
    for (main_worktree, mut members) in families {
        members.sort_by(|left, right| left.repository.path.cmp(&right.repository.path));
        let main_is_inside_root = main_worktree.starts_with(&root);
        let main_member = members
            .iter()
            .find(|member| canonical(Path::new(&member.repository.path)) == main_worktree);
        let chosen = if main_is_inside_root {
            main_member.unwrap_or_else(|| &members[0])
        } else {
            &members[0]
        };
        repositories.push(chosen.repository.clone());
    }
    repositories
}

/// Orders repositories by worktree count, largest first.
///
/// An unknown count is not a zero: it is kept after every measured repository,
/// with the path as the stable tie-breaker. Discovery sets the count from Git,
/// so this is the order the workspace page displays.
fn sort_repositories(repositories: &mut [WorkspaceRepository]) {
    repositories.sort_by(
        |left, right| match (left.worktree_count, right.worktree_count) {
            (Some(left_count), Some(right_count)) => right_count
                .cmp(&left_count)
                .then_with(|| left.path.cmp(&right.path)),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => left.path.cmp(&right.path),
        },
    );
}

/// Walks below one non-repository folder and returns every repository root.
///
/// This is the shallow breadth-first walk used for a developer root and for the
/// parent of a selected repository. It deliberately does not descend into a
/// repository it has found, so a linked worktree is reported once and its own
/// contents are not mistaken for another project.
fn walk_repository_paths(root: &Path, warnings: &mut Vec<String>) -> Vec<PathBuf> {
    let mut repositories: Vec<PathBuf> = Vec::new();
    let mut stack: Vec<(PathBuf, usize)> = vec![(root.to_path_buf(), 0)];
    let mut visited = 0usize;

    while let Some((directory, depth)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            visited += 1;
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
            if is_tagged_cache(&path) {
                continue;
            }
            if is_repository_root(&path) {
                repositories.push(path);
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
    repositories
}

/// Counts each repository found by the shallow walk.
///
/// The count is needed for a displayed repository row. It is deliberately kept
/// out of the parent-folder search, where an unrelated repository with a broken
/// gitfile must not emit a warning before its origin is compared.
fn walk_repositories(root: &Path, warnings: &mut Vec<String>) -> Vec<DiscoveredRepository> {
    walk_repository_paths(root, warnings)
        .into_iter()
        .map(|path| repository(&path, warnings))
        .collect()
}

/// Finds the Git repository roots below `path`.
///
/// If `path` is itself a repository, the result contains that repository and
/// independent sibling clones with a matching remote under its parent folder.
/// This is automatic discovery, not a cross-repository cleanup model: each
/// returned repository is still scanned and cleaned on its own.
///
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
    let mut repositories: Vec<DiscoveredRepository> = Vec::new();
    let mut result_root = root.clone();

    if is_in_system_trash(&root) {
        warnings.push(format!(
            "{} is inside the system Trash; discovery does not read the Trash",
            root.display()
        ));
        return Ok(WorkspaceSnapshot {
            schema_version: WORKSPACE_SCHEMA_VERSION,
            root: host_path(&root),
            scanned_at: format_rfc3339(SystemTime::now()),
            repositories: Vec::new(),
            warnings,
        });
    }

    if is_repository_root(&root) {
        let selected = repository(&root, &mut warnings);
        let selected_key = origin_remote_key(&root);
        let mut family = vec![selected];
        if let Some(selected_key) = selected_key.filter(|_| is_independent_clone(&family[0])) {
            if let Some(parent) = root.parent().filter(|parent| is_searchable_parent(parent)) {
                let selected_path = canonical(&root);
                let candidates = walk_repository_paths(parent, &mut warnings);
                let related: Vec<DiscoveredRepository> = candidates
                    .into_iter()
                    .filter(|path| {
                        canonical(path) != selected_path
                            && is_independent_clone_path(path)
                            && origin_remote_key(path).is_some_and(|key| key == selected_key)
                    })
                    .map(|path| repository(&path, &mut warnings))
                    .collect();
                if !related.is_empty() {
                    warnings.push(format!(
                        "found {} other local clone(s) sharing the selected repository's origin remote under {}",
                        related.len(),
                        parent.display()
                    ));
                    family.extend(related);
                    result_root = parent.to_path_buf();
                }
            }
        }
        repositories = family;
    } else if has_repository_ancestor(&root) {
        repositories.push(repository(&root, &mut warnings));
    } else {
        repositories = walk_repositories(&root, &mut warnings);
    }

    let mut repositories = dedupe_repositories(repositories, &result_root);
    sort_repositories(&mut repositories);
    if repositories.is_empty() {
        warnings.push(format!(
            "no Git repositories were found below {}",
            result_root.display()
        ));
    }

    Ok(WorkspaceSnapshot {
        schema_version: WORKSPACE_SCHEMA_VERSION,
        root: host_path(&result_root),
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

    fn make_repo_with_remote(path: &Path, url: &str) {
        std::fs::create_dir_all(path).expect("create repository directory");
        let repo = git2::Repository::init(path).expect("init repository");
        repo.remote("origin", url).expect("create remote");
    }

    fn add_remote(path: &Path, name: &str, url: &str) {
        let repo = git2::Repository::open(path).expect("open repository");
        repo.remote(name, url).expect("create extra remote");
    }

    #[test]
    fn recognises_transport_spellings_of_one_remote() {
        let expected = Some("github.com/HD838A/remote-mic-app".to_string());
        for url in [
            "https://github.com/HD838A/remote-mic-app.git",
            "https://token@github.com/HD838A/remote-mic-app",
            "git@github.com:HD838A/remote-mic-app.git",
            "ssh://git@github.com/HD838A/remote-mic-app",
        ] {
            assert_eq!(remote_key(url), expected, "{url}");
        }
        assert_eq!(
            remote_key("https://github.com/other/repository.git"),
            Some("github.com/other/repository".to_string())
        );
        assert_eq!(remote_key(""), None);
    }

    #[test]
    fn finds_independent_sibling_clones_with_the_same_remote() {
        let root = temp_workspace();
        let selected = root.path().join("selected");
        let sibling = root.path().join("sibling");
        let unrelated = root.path().join("unrelated");
        make_repo_with_remote(&selected, "https://github.com/HD838A/remote-mic-app.git");
        make_repo_with_remote(&sibling, "git@github.com:HD838A/remote-mic-app.git");
        make_repo_with_remote(&unrelated, "https://github.com/other/repository.git");

        let snapshot = discover_workspace(selected.to_str().expect("path")).expect("discover");
        let mut names: Vec<&str> = snapshot
            .repositories
            .iter()
            .map(|repository| repository.name.as_str())
            .collect();
        names.sort_unstable();
        assert_eq!(names, vec!["selected", "sibling"]);
        assert_eq!(snapshot.root, root.path().display().to_string());
        assert!(snapshot
            .warnings
            .iter()
            .any(|warning| warning.contains("other local clone")));
    }

    #[test]
    fn a_secondary_remote_does_not_join_an_unrelated_clone() {
        // `open-voice-bridge` also has a `macro-platform` remote. Matching every
        // remote would report the independent sayall-macro-platform clone as part of
        // the same family; only origin is the clone identity.
        let root = temp_workspace();
        let selected = root.path().join("selected");
        let macro_platform = root.path().join("macro-platform");
        make_repo_with_remote(&selected, "https://github.com/HD838A/remote-mic-app.git");
        add_remote(
            &selected,
            "macro-platform",
            "https://github.com/GetSayAll/sayall-macro-platform.git",
        );
        make_repo_with_remote(
            &macro_platform,
            "https://github.com/GetSayAll/sayall-macro-platform.git",
        );

        let snapshot = discover_workspace(selected.to_str().expect("path")).expect("discover");
        assert_eq!(snapshot.repositories.len(), 1);
        assert_eq!(
            snapshot.repositories[0].path,
            selected.display().to_string()
        );
    }

    #[test]
    fn a_broken_unrelated_gitfile_does_not_warn_during_sibling_search() {
        let root = temp_workspace();
        let selected = root.path().join("selected");
        let broken = root.path().join("copied-worktree");
        make_repo_with_remote(&selected, "https://github.com/HD838A/remote-mic-app.git");
        std::fs::create_dir_all(&broken).expect("create copied worktree");
        std::fs::write(
            broken.join(".git"),
            "gitdir: /Users/someone/missing-main/.git/worktrees/copied\n",
        )
        .expect("write broken gitfile");

        let snapshot = discover_workspace(selected.to_str().expect("path")).expect("discover");

        assert_eq!(snapshot.repositories.len(), 1);
        assert_eq!(
            snapshot.repositories[0].path,
            selected.display().to_string()
        );
        assert!(
            snapshot
                .warnings
                .iter()
                .all(|warning| !warning.contains("worktree count for")),
            "an unrelated broken gitfile must not produce a worktree-count warning: {:?}",
            snapshot.warnings
        );
    }

    #[test]
    fn a_linked_worktree_is_not_an_independent_clone() {
        let root = temp_workspace();
        let linked = root.path().join("linked");
        std::fs::create_dir_all(linked.join(".git")).expect("create linked .git");
        let candidate = DiscoveredRepository {
            repository: WorkspaceRepository {
                path: linked.display().to_string(),
                name: "linked".to_string(),
                worktree_count: Some(2),
            },
            main_worktree: Some(root.path().join("main")),
        };
        assert!(!is_independent_clone(&candidate));
    }

    #[test]
    fn a_repository_without_a_remote_is_not_expanded() {
        let root = temp_workspace();
        let selected = root.path().join("selected");
        let sibling = root.path().join("sibling");
        make_repo(&selected);
        make_repo_with_remote(&sibling, "https://github.com/HD838A/remote-mic-app.git");

        let snapshot = discover_workspace(selected.to_str().expect("path")).expect("discover");
        assert_eq!(snapshot.repositories.len(), 1);
        assert_eq!(
            snapshot.repositories[0].path,
            selected.display().to_string()
        );
        assert_eq!(snapshot.root, selected.display().to_string());
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
    fn repositories_are_ordered_by_worktree_count_descending() {
        let mut repositories = vec![
            WorkspaceRepository {
                path: "/root/two".to_string(),
                name: "two".to_string(),
                worktree_count: Some(2),
            },
            WorkspaceRepository {
                path: "/root/unknown".to_string(),
                name: "unknown".to_string(),
                worktree_count: None,
            },
            WorkspaceRepository {
                path: "/root/many".to_string(),
                name: "many".to_string(),
                worktree_count: Some(37),
            },
            WorkspaceRepository {
                path: "/root/one".to_string(),
                name: "one".to_string(),
                worktree_count: Some(2),
            },
        ];
        sort_repositories(&mut repositories);
        let order: Vec<&str> = repositories
            .iter()
            .map(|repository| repository.name.as_str())
            .collect();
        assert_eq!(order, vec!["many", "one", "two", "unknown"]);
    }

    #[test]
    fn discovers_nested_repositories_once_and_skips_generated_trees() {
        let root = temp_workspace();
        make_repo(&root.path().join("one"));
        make_repo(&root.path().join("nested").join("two"));
        make_repo(&root.path().join("two").join(".worktrees").join("duplicate"));
        make_repo(&root.path().join("two").join("node_modules").join("ignored"));
        make_repo(&root.path().join("checkouts").join("Sparkle"));
        make_repo(&root.path().join("SourcePackages").join("Sparkle"));
        make_repo(&root.path().join("ci-temp").join("input-test"));
        make_repo(&root.path().join("build-cache").join("Sparkle"));
        let tagged = root.path().join("tagged-build");
        make_repo(&tagged.join("checkouts").join("Sparkle"));
        std::fs::write(tagged.join("CACHEDIR.TAG"), "Signature: cache\n").expect("cache tag");

        let snapshot = discover_workspace(root.path().to_str().expect("path")).expect("discover");
        // Paths carry the host's separator, so compare components rather than
        // matching a `/`-shaped string: `C:\…\one` must satisfy this too.
        let paths: Vec<&str> = snapshot
            .repositories
            .iter()
            .map(|repo| repo.path.as_str())
            .collect();
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
        for ignored in [
            ".worktrees",
            "node_modules",
            "checkouts",
            "SourcePackages",
            "ci-temp",
            "build-cache",
            "tagged-build",
        ] {
            assert!(
                !has_component(ignored),
                "{ignored} is generated, not a local repository: {paths:?}"
            );
        }
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
        // `assert_eq!` rather than `assert!(…is_empty())`: a failure prints the
        // repositories that were found, which is what the reader needs to see.
        assert_eq!(snapshot.repositories, Vec::<WorkspaceRepository>::new());
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
        assert_eq!(snapshot.repositories, Vec::<WorkspaceRepository>::new());
        assert!(snapshot
            .warnings
            .iter()
            .any(|warning| warning.contains("no Git repositories")));
    }
}
