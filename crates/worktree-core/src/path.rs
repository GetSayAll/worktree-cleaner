//! Path validation before deletion.
//!
//! Any deletion action must first pass [`validate_path_for_deletion`] (development
//! contract D10). Validation may only ever **tighten**: resolving a symlink can only
//! make the verdict stricter, never accept an originally-rejected input just because
//! it resolves to a path that "looks normal".
//!
//! Checks (corresponding to product plan §6.4):
//!
//! 1. Non-empty and absolute;
//! 2. If the path itself is a symlink, resolve the target and re-validate it;
//! 3. If an **ancestor** of the path is a symlink, re-validate against the resolved
//!    real path;
//! 4. Reject `..` appearing as a full path component;
//! 5. Reject paths containing NUL or control characters;
//! 6. After normalisation, reject system locations and top-level roots;
//! 7. Reject empty-variable collapse, where the concatenated result equals the
//!    expected parent;
//! 8. Reject on user whitelist hit.
//!
//! Checks 2 and 3 are implemented after check 6 (pure string checks first, then
//! filesystem resolution): the order differs from the list above but the set of checks
//! is the same. The resolved result likewise runs through checks 6 and 7 again.

use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};

/// System locations that are deeply forbidden from deletion (including the whole
/// subtree).
///
/// Note: directories such as `/Applications`, `/Library`, `/Volumes`, and `/Users` use
/// the semantics of "only the directory itself is forbidden", uniformly covered by the
/// "component count ≤ 2" rule in check 6, so they are not listed here as subtree
/// prefixes — otherwise legitimate project paths such as `/Volumes/External/repo` would
/// be killed by mistake.
#[cfg(unix)]
const DENIED_PREFIXES: &[&str] = &[
    "/System",
    "/bin",
    "/sbin",
    "/etc",
    "/private/etc",
    "/var/db",
    "/Library/Extensions",
    "/dev",
    "/proc",
    "/sys",
];

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PathRejection {
    #[error("path is empty")]
    Empty,
    #[error("path is not absolute: {0}")]
    NotAbsolute(String),
    #[error("path contains NUL or control characters: {0}")]
    ControlCharacter(String),
    #[error("path contains a .. component: {0}")]
    ParentTraversal(String),
    #[error("refusing to delete a top-level root: {0}")]
    TopLevelRoot(String),
    #[error("points at a protected system location: {0}")]
    ProtectedLocation(String),
    #[error("concatenated result equals the expected parent, likely an empty variable: {0}")]
    EmptyVariableCollapse(String),
    #[error("symlink points into a protected location: {0} -> {1}")]
    SymlinkEscape(String, String),
    #[error("matches the user whitelist: {0}")]
    Whitelisted(String),
    #[error("cannot resolve path: {0}")]
    Unresolvable(String),
}

/// Validation context.
#[derive(Debug, Clone, Default)]
pub struct DeletionContext {
    /// Expected parent directory. A candidate equal to it is judged an empty-variable
    /// collapse.
    pub expected_parent: Option<PathBuf>,
    /// User whitelist. Rules must be **expanded absolute paths** (the caller expands
    /// `~`).
    pub whitelist: Vec<PathBuf>,
}

impl DeletionContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_expected_parent(mut self, parent: impl Into<PathBuf>) -> Self {
        self.expected_parent = Some(parent.into());
        self
    }

    pub fn with_whitelist(mut self, whitelist: Vec<PathBuf>) -> Self {
        self.whitelist = whitelist;
        self
    }
}

/// A path that passed validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedPath {
    /// The path that will actually be removed (**does not follow** a leaf symlink;
    /// what is removed is always the link itself).
    pub target: PathBuf,
    /// Resolved real path, used for a second review and for showing evidence.
    pub resolved: PathBuf,
    /// Whether the leaf is a symlink. When true, only the link itself may be moved;
    /// recursive deletion is forbidden.
    pub is_symlink: bool,
}

impl ValidatedPath {
    pub fn as_path(&self) -> &Path {
        &self.target
    }
}

/// Validation before deletion. Returning `Err` means the deletion action **must be
/// rejected**.
pub fn validate_path_for_deletion(
    candidate: &Path,
    ctx: &DeletionContext,
) -> Result<ValidatedPath, PathRejection> {
    // ---- 1 / 5: non-empty, absolute, no control characters ----
    if candidate.as_os_str().is_empty() {
        return Err(PathRejection::Empty);
    }
    if !candidate.is_absolute() {
        return Err(PathRejection::NotAbsolute(display(candidate)));
    }
    let literal = display(candidate);
    if literal.chars().any(|c| c == '\0' || c.is_control()) {
        return Err(PathRejection::ControlCharacter(escape(&literal)));
    }

    // ---- 4: `..` as a full path component ----
    for component in candidate.components() {
        if matches!(component, Component::ParentDir) {
            return Err(PathRejection::ParentTraversal(literal));
        }
    }

    let normalized = lexical_normalize(candidate);

    // ---- 6 / 7 (literal): top-level root, system location, empty-variable collapse ----
    reject_roots(&normalized)?;
    reject_empty_variable_collapse(&normalized, ctx)?;

    // ---- 2 / 3: resolve symlinks (leaf + ancestors) ----
    let (resolved, is_symlink) = resolve_for_inspection(candidate)?;

    // ---- 6 / 7 (resolved): run the same checks on the real path, tighten only ----
    if resolved != normalized {
        if let Err(rejection) = reject_roots(&resolved) {
            return Err(match (rejection, is_symlink) {
                (PathRejection::ProtectedLocation(hit), true) => {
                    PathRejection::SymlinkEscape(literal, hit)
                }
                (PathRejection::TopLevelRoot(hit), true) => {
                    PathRejection::SymlinkEscape(literal, hit)
                }
                (other, _) => other,
            });
        }
        reject_empty_variable_collapse_resolved(&resolved, ctx)?;
    }

    // ---- 8: whitelist ----
    for rule in &ctx.whitelist {
        let rule_normalized = lexical_normalize(rule);
        if normalized == rule_normalized || normalized.starts_with(&rule_normalized) {
            return Err(PathRejection::Whitelisted(display(rule)));
        }
    }

    Ok(ValidatedPath {
        target: normalized,
        resolved,
        is_symlink,
    })
}

/// Top-level root + system location checks.
fn reject_roots(path: &Path) -> Result<(), PathRejection> {
    // A component count ≤ 2 means the "/xxx" level: no top-level directory itself may
    // be deleted. This single rule also covers / itself, /etc, /Applications,
    // /Library, /Volumes, /Users, /tmp, /opt, etc., without relying on a
    // platform-specific list.
    if path.components().count() <= 2 {
        return Err(PathRejection::TopLevelRoot(display(path)));
    }

    if let Some(hit) = protected_location(path) {
        return Err(PathRejection::ProtectedLocation(hit));
    }

    Ok(())
}

/// Returns the matched rule text when a protected location is hit, otherwise `None`.
fn protected_location(path: &Path) -> Option<String> {
    #[cfg(unix)]
    {
        for prefix in DENIED_PREFIXES {
            let prefix_path = Path::new(prefix);
            if path == prefix_path || path.starts_with(prefix_path) {
                return Some((*prefix).to_string());
            }
        }

        // /usr is forbidden as a whole, but /usr/local is where users and package
        // managers install software.
        let usr = Path::new("/usr");
        let usr_local = Path::new("/usr/local");
        if path == usr || (path.starts_with(usr) && !path.starts_with(usr_local)) {
            return Some("/usr".to_string());
        }
        if path == usr_local {
            return Some("/usr/local".to_string());
        }
    }

    #[cfg(windows)]
    {
        if let Some(prefix) = path.components().next() {
            if matches!(prefix, Component::Prefix(_)) && path.components().count() <= 1 {
                return Some(display(path));
            }
        }
        for key in ["WINDIR", "ProgramFiles", "ProgramFiles(x86)", "ProgramData"] {
            if let Some(root) = std::env::var_os(key) {
                let root_path = Path::new(&root);
                if path == root_path || path.starts_with(root_path) {
                    return Some(display(root_path));
                }
            }
        }
    }

    // The user's home directory itself may not be deleted; contents under it are not
    // restricted by this rule.
    if let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
        let home_path = Path::new(&home);
        if !home_path.as_os_str().is_empty() && path == home_path {
            return Some(display(home_path));
        }
    }

    None
}

fn reject_empty_variable_collapse(path: &Path, ctx: &DeletionContext) -> Result<(), PathRejection> {
    if let Some(parent) = ctx.expected_parent.as_ref() {
        if path == lexical_normalize(parent) {
            return Err(PathRejection::EmptyVariableCollapse(display(path)));
        }
    }
    Ok(())
}

fn reject_empty_variable_collapse_resolved(
    resolved: &Path,
    ctx: &DeletionContext,
) -> Result<(), PathRejection> {
    if let Some(parent) = ctx.expected_parent.as_ref() {
        let parent_resolved = resolve_for_inspection(parent)
            .map(|(path, _)| path)
            .unwrap_or_else(|_| lexical_normalize(parent));
        if resolved == parent_resolved {
            return Err(PathRejection::EmptyVariableCollapse(display(resolved)));
        }
    }
    Ok(())
}

/// Lexical normalisation: drop `.`, collapse `..`, and never touch the filesystem.
fn lexical_normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => out.push(prefix.as_os_str()),
            Component::RootDir => out.push(Component::RootDir.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            Component::Normal(segment) => out.push(segment),
        }
    }
    if out.as_os_str().is_empty() {
        out.push("/");
    }
    out
}

/// Resolve the real location of a path, for symlink checks.
///
/// When the path does not exist, normalise the "longest existing ancestor" and append
/// the remaining components back — so even if the directory to be deleted is already
/// gone, we can still tell whether an ancestor has been hijacked by a symlink.
fn resolve_for_inspection(path: &Path) -> Result<(PathBuf, bool), PathRejection> {
    let is_symlink = std::fs::symlink_metadata(path)
        .map(|meta| meta.file_type().is_symlink())
        .unwrap_or(false);

    let mut current = path.to_path_buf();
    let mut tail: Vec<OsString> = Vec::new();
    loop {
        if let Ok(canonical) = std::fs::canonicalize(&current) {
            let mut out = canonical;
            for segment in tail.iter().rev() {
                out.push(segment);
            }
            return Ok((out, is_symlink));
        }

        let name = match current.file_name() {
            Some(name) => name.to_os_string(),
            None => return Err(PathRejection::Unresolvable(display(path))),
        };
        tail.push(name);

        match current.parent() {
            Some(parent) if parent != current => current = parent.to_path_buf(),
            _ => return Err(PathRejection::Unresolvable(display(path))),
        }
    }
}

fn display(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// Render control characters into a visible form, so evidence can be written to logs
/// and the UI.
fn escape(raw: &str) -> String {
    raw.chars()
        .map(|c| match c {
            '\0' => "\\0".to_string(),
            '\n' => "\\n".to_string(),
            '\t' => "\\t".to_string(),
            '\r' => "\\r".to_string(),
            other if other.is_control() => format!("\\u{{{:x}}}", other as u32),
            other => other.to_string(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use std::os::unix::fs::symlink;

    /// Create a temporary directory used only within the current test.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "worktree-core-path-{}-{}-{}",
                tag,
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or(0)
            ));
            std::fs::create_dir_all(&dir).expect("create temp dir");
            Self(dir)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn assert_rejected(path: &str, ctx: &DeletionContext) {
        let result = validate_path_for_deletion(Path::new(path), ctx);
        assert!(result.is_err(), "should be rejected but passed: {}", path);
    }

    #[test]
    fn accepts_an_ordinary_project_path() {
        let ctx = DeletionContext::new();
        let validated =
            validate_path_for_deletion(Path::new("/Users/andy/MySrc/proj/node_modules"), &ctx)
                .expect("an ordinary project path should pass");
        assert_eq!(
            validated.as_path(),
            Path::new("/Users/andy/MySrc/proj/node_modules")
        );
        assert!(!validated.is_symlink);
    }

    #[test]
    fn rejects_empty_path() {
        assert_eq!(
            validate_path_for_deletion(Path::new(""), &DeletionContext::new()),
            Err(PathRejection::Empty)
        );
    }

    #[test]
    fn rejects_relative_path() {
        assert_rejected("node_modules", &DeletionContext::new());
        assert_rejected("./build", &DeletionContext::new());
        assert_rejected("../target", &DeletionContext::new());
    }

    #[test]
    fn rejects_control_characters() {
        assert_rejected("/tmp/a\0b", &DeletionContext::new());
        assert_rejected("/tmp/a\tb", &DeletionContext::new());
        assert_rejected("/tmp/a\nb", &DeletionContext::new());
    }

    #[test]
    fn rejects_parent_traversal_components() {
        assert_rejected("/Users/andy/../etc/passwd", &DeletionContext::new());
        assert_rejected("/..", &DeletionContext::new());
        assert_rejected("/Users/andy/proj/..", &DeletionContext::new());
    }

    #[test]
    fn allows_legitimate_names_containing_dots() {
        // A `..` inside a name is not path traversal and must not be rejected outright.
        let ctx = DeletionContext::new();
        assert!(validate_path_for_deletion(Path::new("/tmp/name..files"), &ctx).is_ok());
        assert!(validate_path_for_deletion(Path::new("/tmp/..hidden"), &ctx).is_ok());
    }

    #[test]
    fn rejects_top_level_roots() {
        for path in [
            "/",
            "/etc",
            "/Applications",
            "/Library",
            "/Volumes",
            "/Users",
            "/tmp",
            "/opt",
        ] {
            assert_rejected(path, &DeletionContext::new());
        }
    }

    #[test]
    fn rejects_deep_system_locations() {
        for path in [
            "/System/Library/CoreServices",
            "/bin/sh",
            "/sbin/mount",
            "/usr/bin/git",
            "/etc/passwd",
            "/var/db/whatever",
            "/Library/Extensions/foo.kext",
            "/dev/null",
        ] {
            assert_rejected(path, &DeletionContext::new());
        }
    }

    #[test]
    fn allows_usr_local_but_not_usr_local_itself() {
        let ctx = DeletionContext::new();
        assert!(validate_path_for_deletion(Path::new("/usr/local/bin/mytool"), &ctx).is_ok());
        assert_rejected("/usr/local", &ctx);
    }

    #[test]
    fn rejects_home_directory_itself_but_allows_children() {
        let home = match std::env::var_os("HOME") {
            Some(home) => PathBuf::from(home),
            None => return,
        };
        let ctx = DeletionContext::new();
        assert!(
            validate_path_for_deletion(&home, &ctx).is_err(),
            "the home directory itself must not be deleted"
        );
        assert!(validate_path_for_deletion(&home.join("proj/node_modules"), &ctx).is_ok());
    }

    #[test]
    fn rejects_empty_variable_collapse() {
        let ctx = DeletionContext::new().with_expected_parent("/Users/andy/proj");
        // The concatenated result collapses to the parent directory itself, which means
        // some variable is empty.
        assert_eq!(
            validate_path_for_deletion(Path::new("/Users/andy/proj"), &ctx),
            Err(PathRejection::EmptyVariableCollapse(
                "/Users/andy/proj".to_string()
            ))
        );
        // A normal concatenation is unaffected.
        assert!(validate_path_for_deletion(Path::new("/Users/andy/proj/target"), &ctx).is_ok());
    }

    #[test]
    fn rejects_whitelisted_paths_and_their_children() {
        let ctx =
            DeletionContext::new().with_whitelist(vec![PathBuf::from("/Users/andy/MySrc/keep-me")]);
        assert_rejected("/Users/andy/MySrc/keep-me", &ctx);
        assert_rejected("/Users/andy/MySrc/keep-me/node_modules", &ctx);
        // A different path that shares the prefix but not the component is unaffected.
        assert!(
            validate_path_for_deletion(Path::new("/Users/andy/MySrc/keep-me-other"), &ctx).is_ok()
        );
    }

    #[test]
    #[cfg(unix)]
    fn rejects_symlink_pointing_into_protected_location() {
        let temp = TempDir::new("leaf-link");
        let link = temp.path().join("leaf-link");
        symlink("/System", &link).expect("create symlink");

        let result = validate_path_for_deletion(&link, &DeletionContext::new());
        match result {
            Err(PathRejection::SymlinkEscape(from, hit)) => {
                assert!(
                    from.ends_with("leaf-link"),
                    "evidence should point at the link itself: {}",
                    from
                );
                assert!(
                    hit.starts_with("/System"),
                    "evidence should point at the protected location: {}",
                    hit
                );
            }
            other => panic!("expected a symlink escape, got: {:?}", other),
        }
    }

    #[test]
    #[cfg(unix)]
    fn rejects_symlinked_ancestor_pointing_into_protected_location() {
        let temp = TempDir::new("ancestor-link");
        let link = temp.path().join("usr-link");
        symlink("/usr", &link).expect("create symlink");

        // The leaf does not exist (/usr/bin exists, so we use a deeper, nonexistent
        // location), but the ancestor usr-link points at /usr, so the resolved path
        // lands inside the protected subtree.
        let candidate = link.join("bin/definitely-not-here");
        let result = validate_path_for_deletion(&candidate, &DeletionContext::new());
        assert!(
            result.is_err(),
            "an ancestor symlink pointing at /usr must be rejected, got: {:?}",
            result
        );
    }

    #[test]
    fn root_path_is_resolvable() {
        // An absolute path can always walk its ancestors up to `/`, so a resolution
        // failure can only be a defensive branch.
        let (resolved, is_symlink) =
            resolve_for_inspection(Path::new("/")).expect("root is resolvable");
        assert_eq!(resolved, Path::new("/"));
        assert!(!is_symlink);
    }

    #[test]
    fn resolving_nonexistent_path_keeps_tail_components() {
        let temp = TempDir::new("missing-tail");
        let candidate = temp.path().join("gone/also-gone");
        let (resolved, is_symlink) =
            resolve_for_inspection(&candidate).expect("resolvable to an existing ancestor");
        assert!(!is_symlink);
        assert!(
            resolved.ends_with("gone/also-gone"),
            "the tail components should be kept: {}",
            resolved.display()
        );
    }

    #[test]
    fn validates_normalized_target_form() {
        let ctx = DeletionContext::new();
        let validated =
            validate_path_for_deletion(Path::new("/tmp/./a/./b"), &ctx).expect("passes validation");
        assert_eq!(validated.as_path(), Path::new("/tmp/a/b"));
    }
}
