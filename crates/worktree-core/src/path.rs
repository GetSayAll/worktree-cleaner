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
//! 6. After normalisation, reject system locations and top-level roots, plus the
//!    user's home directory itself (protected structurally as well, so an unset
//!    `HOME` / `USERPROFILE` cannot disable it);
//! 7. Reject empty-variable collapse, where the concatenated result equals the
//!    expected parent;
//! 8. Reject on user whitelist hit.
//!
//! Checks 2 and 3 are implemented after check 6 (pure string checks first, then
//! filesystem resolution): the order differs from the list above but the set of checks
//! is the same. The resolved result likewise runs through checks 6 and 7 again, and
//! the whitelist (check 8) is matched against both the literal and the resolved path.

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
#[derive(Debug, Clone)]
pub struct DeletionContext {
    /// Expected parent directory. A candidate equal to it is judged an empty-variable
    /// collapse (check 7 of product plan §6.4).
    ///
    /// This is a **required** constructor argument: making it optional would let a
    /// caller forget to set it and silently disable the guard, which is fail-open.
    pub expected_parent: PathBuf,
    /// User whitelist. Rules must be **expanded absolute paths** (the caller expands
    /// `~`).
    pub whitelist: Vec<PathBuf>,
}

impl DeletionContext {
    pub fn new(expected_parent: impl Into<PathBuf>) -> Self {
        Self {
            expected_parent: expected_parent.into(),
            whitelist: Vec::new(),
        }
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
    // `'\0'.is_control()` is true, so NUL needs no separate test here.
    if literal.chars().any(char::is_control) {
        return Err(PathRejection::ControlCharacter(escape(&literal)));
    }

    // ---- 4: `..` as a full path component ----
    for component in candidate.components() {
        if matches!(component, Component::ParentDir) {
            return Err(PathRejection::ParentTraversal(literal));
        }
    }

    let normalized = lexical_normalize(candidate);

    // The expected parent is resolved unconditionally (with a lexical fallback), so
    // the empty-variable-collapse guard also fires when the parent is itself a symlink.
    let (parent_literal, parent_resolved) = expected_parent_forms(ctx);

    // ---- 6 / 7 (literal): top-level root, system location, empty-variable collapse ----
    reject_roots(&normalized)?;
    if is_empty_variable_collapse(&normalized, &parent_literal, &parent_resolved) {
        return Err(PathRejection::EmptyVariableCollapse(display(&normalized)));
    }

    // ---- 2 / 3: resolve symlinks (leaf + ancestors) ----
    let resolution = resolve_for_inspection(candidate)?;
    let resolved = resolution.resolved;

    // ---- 6 (resolved): run the same checks on the real path, tighten only ----
    if resolved != normalized {
        if let Err(rejection) = reject_roots(&resolved) {
            return Err(match (rejection, resolution.crossed_symlink) {
                (PathRejection::ProtectedLocation(hit), true)
                | (PathRejection::TopLevelRoot(hit), true) => {
                    PathRejection::SymlinkEscape(literal.clone(), hit)
                }
                (other, _) => other,
            });
        }
    }

    // ---- 7 (resolved): the collapse guard runs unconditionally, not only when the
    // path resolved to something else ----
    if is_empty_variable_collapse(&resolved, &parent_literal, &parent_resolved) {
        return Err(PathRejection::EmptyVariableCollapse(display(&normalized)));
    }

    // ---- 8: whitelist ----
    // Compare a candidate against **both** its literal and its resolved form, and
    // resolve each rule itself first. Otherwise a symlink that satisfies the whitelist
    // only after resolution would slip through (D10: verification may only tighten).
    for rule in &ctx.whitelist {
        let rule_literal = lexical_normalize(rule);
        let rule_resolved = resolve_for_inspection(rule)
            .map(|resolution| resolution.resolved)
            .unwrap_or_else(|_| rule_literal.clone());
        for rule_form in [&rule_literal, &rule_resolved] {
            for candidate_form in [&normalized, &resolved] {
                if candidate_form == rule_form || candidate_form.starts_with(rule_form) {
                    return Err(PathRejection::Whitelisted(display(rule)));
                }
            }
        }
    }

    Ok(ValidatedPath {
        target: normalized,
        resolved,
        is_symlink: resolution.is_leaf_symlink,
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
        // Compare on normalised strings rather than `Component`s: `std::fs::canonicalize`
        // yields a verbatim path (`\\?\C:\...`) whose `Prefix` never equals the `Prefix`
        // of `%WINDIR%` / `%ProgramFiles%`, so a junction into `C:\Windows` would
        // otherwise pass. `windows_path_key` folds both forms into one comparable key.
        let key = windows_path_key(&display(path));

        // A bare drive root (`C:\`) is a top-level root.
        if key.len() == 2 && key.ends_with(':') {
            return Some(display(path));
        }

        for name in ["WINDIR", "ProgramFiles", "ProgramFiles(x86)", "ProgramData"] {
            if let Some(root) = std::env::var_os(name) {
                let root_display = root.to_string_lossy().into_owned();
                if windows_key_under(&key, &windows_path_key(&root_display)) {
                    return Some(root_display);
                }
            }
        }
    }

    // The user's home directory itself may not be deleted; contents under it are not
    // restricted by this rule.
    if let Some(home) = env_home_dir() {
        if path == home {
            return Some(display(&home));
        }
    }

    // Structural fallback that does not depend on the environment at all: reject a path
    // whose component shape is a home root (`<root>/Users/<name>` or
    // `<root>/home/<name>`). Without it, an unset HOME / USERPROFILE (Windows services,
    // containers, launchd) would silently disable this protection for `/Users/andy`,
    // which the "component count ≤ 2" rule does not catch.
    if is_home_root_shape(path) {
        return Some(display(path));
    }

    None
}

/// The user's home directory, chosen from the environment.
///
/// Order: `HOME`, then `USERPROFILE`, then `HOMEDRIVE` + `HOMEPATH`.
fn env_home_dir() -> Option<PathBuf> {
    select_home_dir(
        std::env::var_os("HOME"),
        std::env::var_os("USERPROFILE"),
        std::env::var_os("HOMEDRIVE"),
        std::env::var_os("HOMEPATH"),
    )
}

/// Pure home-directory selection, so the precedence can be unit-tested without
/// touching the process environment.
fn select_home_dir(
    home: Option<OsString>,
    userprofile: Option<OsString>,
    homedrive: Option<OsString>,
    homepath: Option<OsString>,
) -> Option<PathBuf> {
    if let Some(home) = home.filter(|value| !value.is_empty()) {
        return Some(PathBuf::from(home));
    }
    if let Some(userprofile) = userprofile.filter(|value| !value.is_empty()) {
        return Some(PathBuf::from(userprofile));
    }
    match (homedrive, homepath) {
        (Some(drive), Some(path)) if !drive.is_empty() || !path.is_empty() => {
            let mut combined = PathBuf::from(drive);
            combined.push(path);
            Some(combined)
        }
        _ => None,
    }
}

/// Whether a path has the shape of a home root: exactly two normal components where
/// the first is `Users` (any casing, any drive) or `home` (on a unix root).
///
/// `/Users/<name>`, `/home/<name>`, and `C:\Users\<name>` match; `/Users/<name>/proj`
/// and `/home/<name>/proj` do not.
fn is_home_root_shape(path: &Path) -> bool {
    let mut segments = path.components().filter_map(|component| match component {
        Component::Normal(segment) => Some(segment.to_string_lossy().into_owned()),
        _ => None,
    });
    let Some(first) = segments.next() else {
        return false;
    };
    let Some(_user) = segments.next() else {
        return false;
    };
    // Exactly two normal components: a third would be a child such as `/Users/x/proj`.
    if segments.next().is_some() {
        return false;
    }
    first.eq_ignore_ascii_case("Users") || first == "home"
}

/// Canonical, platform-independent comparison key for a Windows path string.
///
/// Folds the forms that mean the same thing: `/` becomes `\`, a verbatim prefix is
/// stripped (`\\?\C:\...` → `C:\...`, `\\?\UNC\server\share` → `\\server\share`),
/// trailing separators are dropped, and the result is lowercased. Being string-based,
/// it works regardless of how `Path` classifies the prefix — and is unit-testable on
/// macOS.
#[cfg(any(windows, test))]
fn windows_path_key(raw: &str) -> String {
    let mut key = raw.replace('/', "\\");
    if let Some(rest) = key.strip_prefix(r"\\?\UNC\") {
        key = format!(r"\\{rest}");
    } else if let Some(rest) = key.strip_prefix(r"\\?\") {
        key = rest.to_string();
    }
    key.trim_end_matches('\\').to_lowercase()
}

/// Whether `path_key` is `root_key` itself or sits under it, respecting component
/// boundaries (so `c:\windows` is not treated as being under `c:\win`).
#[cfg(windows)]
fn windows_key_under(path_key: &str, root_key: &str) -> bool {
    !root_key.is_empty()
        && path_key
            .strip_prefix(root_key)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('\\'))
}

/// The literal (lexically normalised) and resolved forms of the expected parent.
///
/// Both are computed unconditionally so the collapse guard compares a candidate
/// against either form, even when the parent is a symlink.
fn expected_parent_forms(ctx: &DeletionContext) -> (PathBuf, PathBuf) {
    let literal = lexical_normalize(&ctx.expected_parent);
    let resolved = resolve_for_inspection(&ctx.expected_parent)
        .map(|resolution| resolution.resolved)
        .unwrap_or_else(|_| literal.clone());
    (literal, resolved)
}

/// Whether the candidate equals either form of the expected parent (check 7).
fn is_empty_variable_collapse(
    candidate: &Path,
    parent_literal: &Path,
    parent_resolved: &Path,
) -> bool {
    candidate == parent_literal || candidate == parent_resolved
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

/// The outcome of inspecting a path's real location.
struct Resolution {
    /// The resolved real path.
    resolved: PathBuf,
    /// Whether the **leaf** is a symlink. When true, only the link itself may be
    /// moved; recursive deletion is forbidden.
    is_leaf_symlink: bool,
    /// Whether **any** component (leaf or ancestor) was a symlink. Used to label a
    /// rejection as a symlink escape rather than a plain protected location: the leaf
    /// flag alone would hide the fact that a symlink was involved when only an ancestor
    /// was one.
    crossed_symlink: bool,
}

/// Resolve the real location of a path, for symlink checks.
///
/// When the path does not exist, normalise the "longest existing ancestor" and append
/// the remaining components back — so even if the directory to be deleted is already
/// gone, we can still tell whether an ancestor has been hijacked by a symlink.
fn resolve_for_inspection(path: &Path) -> Result<Resolution, PathRejection> {
    let is_leaf_symlink = is_symlink(path);
    let crossed_symlink = is_leaf_symlink || has_symlink_ancestor(path);

    let mut current = path.to_path_buf();
    let mut tail: Vec<OsString> = Vec::new();
    loop {
        if let Ok(canonical) = std::fs::canonicalize(&current) {
            let mut out = canonical;
            for segment in tail.iter().rev() {
                out.push(segment);
            }
            return Ok(Resolution {
                resolved: out,
                is_leaf_symlink,
                crossed_symlink,
            });
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

/// Whether the path itself is a symlink (without following it).
fn is_symlink(path: &Path) -> bool {
    std::fs::symlink_metadata(path)
        .map(|meta| meta.file_type().is_symlink())
        .unwrap_or(false)
}

/// Whether any strict ancestor of the path is a symlink.
fn has_symlink_ancestor(path: &Path) -> bool {
    let mut current = path.parent();
    while let Some(dir) = current {
        if is_symlink(dir) {
            return true;
        }
        current = dir.parent();
    }
    false
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

    /// Expected parent used by tests that do not exercise the collapse guard; it must
    /// never coincide with a candidate under test.
    const UNRELATED_PARENT: &str = "/expected/parent";

    fn ctx() -> DeletionContext {
        DeletionContext::new(UNRELATED_PARENT)
    }

    /// A home-root shape that exercises the structural fallback with no dependence on
    /// the environment.
    fn synthetic_home_root() -> PathBuf {
        #[cfg(windows)]
        {
            PathBuf::from(r"C:\Users\example")
        }
        #[cfg(not(windows))]
        {
            PathBuf::from("/Users/example")
        }
    }

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

    fn assert_rejected(path: &str, context: &DeletionContext) {
        let result = validate_path_for_deletion(Path::new(path), context);
        assert!(result.is_err(), "should be rejected but passed: {}", path);
    }

    #[test]
    fn accepts_an_ordinary_project_path() {
        let validated =
            validate_path_for_deletion(Path::new("/Users/andy/MySrc/proj/node_modules"), &ctx())
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
            validate_path_for_deletion(Path::new(""), &ctx()),
            Err(PathRejection::Empty)
        );
    }

    #[test]
    fn rejects_relative_path() {
        assert_rejected("node_modules", &ctx());
        assert_rejected("./build", &ctx());
        assert_rejected("../target", &ctx());
    }

    #[test]
    fn rejects_control_characters() {
        assert_rejected("/tmp/a\0b", &ctx());
        assert_rejected("/tmp/a\tb", &ctx());
        assert_rejected("/tmp/a\nb", &ctx());
    }

    #[test]
    fn rejects_parent_traversal_components() {
        assert_rejected("/Users/andy/../etc/passwd", &ctx());
        assert_rejected("/..", &ctx());
        assert_rejected("/Users/andy/proj/..", &ctx());
    }

    #[test]
    fn allows_legitimate_names_containing_dots() {
        // A `..` inside a name is not path traversal and must not be rejected outright.
        let context = ctx();
        assert!(validate_path_for_deletion(Path::new("/tmp/name..files"), &context).is_ok());
        assert!(validate_path_for_deletion(Path::new("/tmp/..hidden"), &context).is_ok());
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
            assert_rejected(path, &ctx());
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
            assert_rejected(path, &ctx());
        }
    }

    #[test]
    fn allows_usr_local_but_not_usr_local_itself() {
        let context = ctx();
        assert!(validate_path_for_deletion(Path::new("/usr/local/bin/mytool"), &context).is_ok());
        assert_rejected("/usr/local", &context);
    }

    #[test]
    fn home_dir_is_selected_in_documented_order() {
        let os = |value: &str| Some(OsString::from(value));

        // HOME wins.
        assert_eq!(
            select_home_dir(os("/Users/andy"), os("D:\\Users\\bob"), None, None),
            Some(PathBuf::from("/Users/andy"))
        );
        // USERPROFILE is used when HOME is missing...
        assert_eq!(
            select_home_dir(None, os("D:\\Users\\bob"), None, None),
            Some(PathBuf::from("D:\\Users\\bob"))
        );
        // ...or empty.
        assert_eq!(
            select_home_dir(os(""), os("D:\\Users\\bob"), None, None),
            Some(PathBuf::from("D:\\Users\\bob"))
        );
        // HOMEDRIVE + HOMEPATH are the last resort (empty drive keeps this assertion
        // platform-independent).
        assert_eq!(
            select_home_dir(None, None, Some(OsString::new()), os("Users/carol")),
            Some(PathBuf::from("Users/carol"))
        );
        #[cfg(windows)]
        assert_eq!(
            select_home_dir(None, None, os("C:"), os(r"\Users\carol")),
            Some(PathBuf::from(r"C:\Users\carol"))
        );
        // Nothing advertised.
        assert_eq!(select_home_dir(None, None, None, None), None);
    }

    #[test]
    fn home_root_shape_is_detected_structurally() {
        // Positives: a home root is rejected with no environment support at all.
        for path in ["/Users/andy", "/home/andy", "/Users/Andy"] {
            assert!(
                is_home_root_shape(Path::new(path)),
                "should be a home-root shape: {path}"
            );
        }
        #[cfg(windows)]
        assert!(is_home_root_shape(Path::new(r"C:\Users\andy")));

        // Negatives: children of a home directory, shallower roots, and unrelated
        // layouts must not be treated as home roots.
        for path in [
            "/Users/andy/proj",
            "/home/andy/proj",
            "/Users",
            "/home",
            "/opt/home/andy",
            "/Users/andy/proj/node_modules",
        ] {
            assert!(
                !is_home_root_shape(Path::new(path)),
                "should not be a home-root shape: {path}"
            );
        }
    }

    #[test]
    fn rejects_home_directory_itself_but_allows_children() {
        let context = ctx();
        let mut homes: Vec<PathBuf> = Vec::new();
        if let Some(home) = env_home_dir() {
            homes.push(home);
        }
        // The synthetic home root makes this assertion unconditional: it is rejected by
        // the structural rule even when HOME / USERPROFILE are unset.
        homes.push(synthetic_home_root());

        for home in homes {
            assert!(
                validate_path_for_deletion(&home, &context).is_err(),
                "the home directory itself must not be deleted: {}",
                home.display()
            );
            let child = home.join("proj").join("node_modules");
            assert!(
                validate_path_for_deletion(&child, &context).is_ok(),
                "contents under the home directory are not restricted: {}",
                child.display()
            );
        }
    }

    #[test]
    fn rejects_empty_variable_collapse() {
        let context = DeletionContext::new("/Users/andy/proj");
        // The concatenated result collapses to the parent directory itself, which means
        // some variable is empty.
        assert_eq!(
            validate_path_for_deletion(Path::new("/Users/andy/proj"), &context),
            Err(PathRejection::EmptyVariableCollapse(
                "/Users/andy/proj".to_string()
            ))
        );
        // A normal concatenation is unaffected.
        assert!(validate_path_for_deletion(Path::new("/Users/andy/proj/target"), &context).is_ok());
    }

    #[test]
    #[cfg(unix)]
    fn rejects_candidate_equal_to_a_symlinked_expected_parent() {
        // Finding 5: the parent is itself a symlink, so `resolved == normalized` for the
        // candidate and the old guard (which only ran when they differed) never fired.
        let temp = TempDir::new("parent-link");
        let real = temp.path().join("proj");
        std::fs::create_dir_all(&real).expect("create proj dir");
        let real = std::fs::canonicalize(&real).expect("canonicalize proj");
        let link = temp.path().join("link");
        symlink(&real, &link).expect("create symlink");

        let context = DeletionContext::new(&link);
        match validate_path_for_deletion(&real, &context) {
            Err(PathRejection::EmptyVariableCollapse(reported)) => {
                assert_eq!(reported, display(&real));
            }
            other => panic!(
                "a candidate equal to the resolved parent must be rejected, got: {:?}",
                other
            ),
        }
    }

    #[test]
    fn rejects_whitelisted_paths_and_their_children() {
        let context = DeletionContext::new(UNRELATED_PARENT)
            .with_whitelist(vec![PathBuf::from("/Users/andy/MySrc/keep-me")]);
        assert_rejected("/Users/andy/MySrc/keep-me", &context);
        assert_rejected("/Users/andy/MySrc/keep-me/node_modules", &context);
        // A different path that shares the prefix but not the component is unaffected.
        assert!(
            validate_path_for_deletion(Path::new("/Users/andy/MySrc/keep-me-other"), &context)
                .is_ok()
        );
    }

    #[test]
    #[cfg(unix)]
    fn whitelist_blocks_symlink_whose_resolved_target_is_whitelisted() {
        // Finding 1 repro: the literal path misses the whitelist, but the resolved path
        // lands inside it. Validation must compare the resolved form too.
        let temp = TempDir::new("whitelist-ancestor");
        let kept = temp.path().join("kept");
        std::fs::create_dir_all(&kept).expect("create kept dir");
        let kept_real = std::fs::canonicalize(&kept).expect("canonicalize kept");

        let link = temp.path().join("link");
        symlink(&kept_real, &link).expect("create symlink");

        let context = DeletionContext::new(UNRELATED_PARENT).with_whitelist(vec![kept_real]);

        // Literal `/…/link/sub` is outside the whitelist; resolved `/…/kept/sub` is not.
        let candidate = link.join("sub");
        match validate_path_for_deletion(&candidate, &context) {
            Err(PathRejection::Whitelisted(_)) => {}
            other => panic!(
                "the whitelist must apply to the resolved path as well, got: {:?}",
                other
            ),
        }
    }

    #[test]
    #[cfg(unix)]
    fn whitelist_blocks_leaf_symlink_resolving_into_whitelisted_directory() {
        // Behaviour chosen: REJECT (returns `Whitelisted`).
        //
        // Rationale: the whitelist is an absolute prohibition ("matched directories are
        // never deleted under any circumstances", product plan §6.4) and D10 requires
        // verification to tighten only, so a link whose resolved target lies inside a
        // whitelisted directory must not be deletable even though the literal link path
        // does not match the rule.
        let temp = TempDir::new("whitelist-leaf");
        let kept = temp.path().join("kept");
        let inside = kept.join("inside");
        std::fs::create_dir_all(&inside).expect("create kept tree");
        let kept_real = std::fs::canonicalize(&kept).expect("canonicalize kept");
        let inside_real = std::fs::canonicalize(&inside).expect("canonicalize inside");

        let link = temp.path().join("other-link");
        symlink(&inside_real, &link).expect("create symlink");

        let context = DeletionContext::new(UNRELATED_PARENT).with_whitelist(vec![kept_real]);
        match validate_path_for_deletion(&link, &context) {
            Err(PathRejection::Whitelisted(_)) => {}
            other => panic!(
                "a link resolving into a whitelisted directory must be rejected, got: {:?}",
                other
            ),
        }
    }

    #[test]
    #[cfg(unix)]
    fn rejects_symlink_pointing_into_protected_location() {
        let temp = TempDir::new("leaf-link");
        let link = temp.path().join("leaf-link");
        symlink("/System", &link).expect("create symlink");

        let result = validate_path_for_deletion(&link, &ctx());
        match result {
            Err(PathRejection::SymlinkEscape(from, hit)) => {
                assert_eq!(
                    from,
                    display(&link),
                    "the first field must be the literal link path"
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
        // Finding 3: only the leaf used to be inspected for symlink-ness, so a rejection
        // caused by a symlinked *ancestor* was mislabelled `ProtectedLocation` and lost
        // the fact that a symlink was involved. The variant must be `SymlinkEscape` and
        // the first field the literal candidate path.
        let temp = TempDir::new("ancestor-link");
        let link = temp.path().join("usr-link");
        symlink("/usr", &link).expect("create symlink");

        // The leaf does not exist (/usr/bin exists, so we use a deeper, nonexistent
        // location), but the ancestor usr-link points at /usr, so the resolved path
        // lands inside the protected subtree.
        let candidate = link.join("bin/definitely-not-here");
        match validate_path_for_deletion(&candidate, &ctx()) {
            Err(PathRejection::SymlinkEscape(from, hit)) => {
                assert_eq!(
                    from,
                    display(&candidate),
                    "the first field must be the literal candidate path"
                );
                assert_eq!(
                    hit, "/usr",
                    "the second field must name the protected location"
                );
            }
            other => panic!("expected a symlink escape, got: {:?}", other),
        }
    }

    #[test]
    #[cfg(unix)]
    fn accepts_symlink_to_ordinary_directory_outside_the_project() {
        let temp = TempDir::new("plain-link");
        let real = temp.path().join("real-target");
        std::fs::create_dir_all(&real).expect("create target dir");
        let real = std::fs::canonicalize(&real).expect("canonicalize target");
        let link = temp.path().join("link");
        symlink(&real, &link).expect("create symlink");

        let validated = validate_path_for_deletion(&link, &ctx())
            .expect("a link to an ordinary directory is allowed");

        // `target` is the link itself (that is what gets moved to the Trash), `resolved`
        // is the real target used as evidence, and the leaf is flagged as a symlink.
        assert_eq!(validated.target, lexical_normalize(&link));
        assert_eq!(validated.resolved, real);
        assert!(validated.is_symlink);
        assert_ne!(validated.target, validated.resolved);
    }

    #[test]
    fn windows_path_key_folds_equivalent_forms() {
        // Finding 2: `canonicalize` yields a verbatim path on Windows; folding the
        // string forms makes `\\?\C:\Windows\System32\x` compare equal to the plain form.
        assert_eq!(
            windows_path_key(r"C:\Windows\System32\x"),
            r"c:\windows\system32\x"
        );
        assert_eq!(
            windows_path_key(r"C:/Windows/System32/x"),
            r"c:\windows\system32\x"
        );
        assert_eq!(
            windows_path_key(r"\\?\C:\Windows\System32\x"),
            r"c:\windows\system32\x"
        );
        assert_eq!(
            windows_path_key(r"C:\Program Files\a"),
            r"c:\program files\a"
        );
        assert_eq!(
            windows_path_key(r"\\?\UNC\server\share\dir"),
            r"\\server\share\dir"
        );
        assert_eq!(
            windows_path_key(r"\\server\share\dir"),
            r"\\server\share\dir"
        );
        // A bare drive root and a trailing separator are folded away.
        assert_eq!(windows_path_key(r"C:\"), "c:");
        assert_eq!(windows_path_key(r"C:\Windows\"), r"c:\windows");
    }

    #[test]
    fn root_path_is_resolvable() {
        // An absolute path can always walk its ancestors up to `/`, so a resolution
        // failure can only be a defensive branch.
        let resolution = resolve_for_inspection(Path::new("/")).expect("root is resolvable");
        assert_eq!(resolution.resolved, Path::new("/"));
        assert!(!resolution.is_leaf_symlink);
        assert!(!resolution.crossed_symlink);
    }

    #[test]
    fn resolving_nonexistent_path_keeps_tail_components() {
        let temp = TempDir::new("missing-tail");
        let candidate = temp.path().join("gone/also-gone");
        let resolution =
            resolve_for_inspection(&candidate).expect("resolvable to an existing ancestor");
        assert!(!resolution.is_leaf_symlink);
        assert!(
            resolution.resolved.ends_with("gone/also-gone"),
            "the tail components should be kept: {}",
            resolution.resolved.display()
        );
    }

    #[test]
    fn validates_normalized_target_form() {
        let validated = validate_path_for_deletion(Path::new("/tmp/./a/./b"), &ctx())
            .expect("passes validation");
        assert_eq!(validated.as_path(), Path::new("/tmp/a/b"));
    }
}
