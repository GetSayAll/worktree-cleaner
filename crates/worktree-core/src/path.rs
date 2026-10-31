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
//!
//! Every comparison that decides protection goes through [`guard_key`], which folds
//! case (and, on Windows, separators and the verbatim prefix). Without it the list is
//! only as good as the spelling the user happened to type: on a filesystem that
//! ignores case, `/SYSTEM/Library/CoreServices` reached the same directory as
//! `/System/Library/CoreServices` and was accepted. Folding can only add refusals,
//! which is the direction D10 allows.

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
///
/// Every entry whose immediate parent is a symlink is listed under **both** its
/// spellings. `/etc` and `/var` are symlinks into `/private`, so `/var/db` is also
/// `/private/var/db`; the list carried only the short form, and a candidate written
/// the canonical way matched neither the literal comparison nor the resolved one —
/// two spellings of one directory, two answers, and the canonical one was accepted.
/// An entry that is a no-op on another platform costs nothing.
#[cfg(unix)]
const DENIED_PREFIXES: &[&str] = &[
    "/System",
    "/bin",
    "/sbin",
    "/etc",
    "/private/etc",
    "/var/db",
    "/private/var/db",
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
    #[error("path contains a name the platform reads as a different name: {0}")]
    AmbiguousName(String),
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

    /// Adds whitelist rules to the context.
    #[must_use]
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

    // ---- 5 (continued): names Win32 would open as something else ----
    //
    // NTFS keeps a name that Win32 would not accept when it arrives through `\\?\` or
    // from WSL, and a directory listing returns it as stored — so a scan can surface
    // one, and the delete call would open through Win32 and act on a different name.
    // No other platform has these rules, so nothing runs elsewhere.
    #[cfg(windows)]
    for component in candidate.components() {
        if let Component::Normal(name) = component {
            let name = name.to_string_lossy();
            if let Some(reason) = win32_ambiguous_name(&name) {
                return Err(PathRejection::AmbiguousName(format!("{name} ({reason})")));
            }
        }
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

    // Looked up once and passed down: the resolved form is checked again below, and
    // asking the environment and the filesystem twice would be two chances to disagree.
    let home = Home::detect();

    // ---- 6 / 7 (literal): top-level root, system location, empty-variable collapse ----
    reject_roots(&normalized, home.as_ref())?;
    if is_empty_variable_collapse(&normalized, &parent_literal, &parent_resolved) {
        return Err(PathRejection::EmptyVariableCollapse(display(&normalized)));
    }

    // ---- 2 / 3: resolve symlinks (leaf + ancestors) ----
    let resolution = resolve_for_inspection(candidate)?;
    let resolved = resolution.resolved;

    // ---- 6 (resolved): run the same checks on the real path, tighten only ----
    if resolved != normalized {
        if let Err(rejection) = reject_roots(&resolved, home.as_ref()) {
            return Err(match (rejection, resolution.crossed_symlink) {
                (
                    PathRejection::ProtectedLocation(hit) | PathRejection::TopLevelRoot(hit),
                    true,
                ) => PathRejection::SymlinkEscape(literal, hit),
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
    if let Some(rule) = whitelist_hit(candidate, &ctx.whitelist) {
        return Err(PathRejection::Whitelisted(display(&rule)));
    }

    Ok(ValidatedPath {
        target: normalized,
        resolved,
        is_symlink: resolution.is_leaf_symlink,
    })
}

/// Whether `candidate` matches one of the whitelist `rules`, returning the matched
/// rule.
///
/// Both the literal and the symlink-resolved form of the candidate are compared
/// against both forms of each rule, so a rule can only ever be *more* likely to hit
/// after resolution (D10: verification may only tighten). Shared with the worktree
/// checks (`wt.whitelisted`) so whitelist semantics live in exactly one place.
pub fn whitelist_hit(candidate: &Path, rules: &[PathBuf]) -> Option<PathBuf> {
    if rules.is_empty() {
        return None;
    }
    let normalized = lexical_normalize(candidate);
    let resolved = resolve_for_inspection(candidate)
        .map_or_else(|_| normalized.clone(), |resolution| resolution.resolved);
    for rule in rules {
        // An empty rule would lexically normalise to the root and therefore match
        // every absolute path — fail-closed, but a surprising silent "protect
        // everything" for what is really a malformed rule. Skip it instead; the UI
        // never emits empty rules, this is a defence in depth for other callers.
        if rule.as_os_str().is_empty() {
            continue;
        }
        let rule_literal = lexical_normalize(rule);
        let rule_resolved = resolve_for_inspection(rule)
            .map_or_else(|_| rule_literal.clone(), |resolution| resolution.resolved);
        for rule_form in [&rule_literal, &rule_resolved] {
            let rule_key = guard_key(rule_form);
            for candidate_form in [&normalized, &resolved] {
                // Compared through the guard key for the same reason the protected
                // list is: a rule typed `~/Dev` and a path written `~/dev` name one
                // directory, and a rule that stops matching when the case changes is
                // a rule that stops protecting.
                if key_under(&guard_key(candidate_form), &rule_key) {
                    return Some(rule.clone());
                }
            }
        }
    }
    None
}

/// Whether `path` lives inside the operating system's Trash.
///
/// A directory the user already moved to the Trash is not part of the project
/// any more, but Git does not know that: `git worktree list` keeps reporting a
/// worktree whose directory was trashed rather than deleted, because the
/// directory is still there — only `prunable` once Git notices it is *gone*,
/// which a Trashed directory is not. A scan that lists such an entry measures
/// it, reports it and offers to remove it: reaching into the one place this app
/// promises to leave alone. Deletion goes **through** the Trash (D5), never the
/// other way round, and the same boundary applies to reading.
///
/// Recognised by a path **component** named (case-insensitively):
///
/// - `.Trash` — the user's own Trash on macOS (`~/.Trash`);
/// - `.Trashes` — the per-volume Trash on macOS (`/.Trashes/<uid>` on the boot
///   volume, `/Volumes/<name>/.Trashes/<uid>` on any other);
/// - `$Recycle.Bin` — the Recycle Bin at the root of a drive on Windows.
///
/// Component matching rather than a root list, and deliberately so: it needs no
/// `HOME` (a scan must not change its answer because an environment variable
/// went missing), no drive enumeration, and it survives a symlinked spelling of
/// the home directory. The cost is that a project directory the user
/// deliberately named `.Trash` is skipped too — the conservative direction,
/// reported through a scan warning rather than applied silently.
pub fn is_in_system_trash(path: &Path) -> bool {
    lexical_normalize(path)
        .components()
        .any(|component| match component {
            Component::Normal(name) => {
                let name = name.to_string_lossy();
                name.eq_ignore_ascii_case(".Trash")
                    || name.eq_ignore_ascii_case(".Trashes")
                    || name.eq_ignore_ascii_case("$Recycle.Bin")
            }
            _ => false,
        })
}

/// Expands a leading `~` (or `~/`) to the user's home directory.
///
/// Whitelist rules and scan paths are typed by a human, so they arrive as
/// `~/dev` rather than as an absolute path. The expansion happens here — once,
/// before any comparison — so every caller works with absolute paths, which is
/// what [`validate_path_for_deletion`] and [`whitelist_hit`] require.
///
/// When no home directory can be determined the prefix is kept literally: a rule
/// that cannot be expanded must not silently become a relative path that matches
/// nothing.
pub fn expand_tilde(raw: &str) -> PathBuf {
    let trimmed = raw.trim();
    if trimmed == "~" {
        return home_dir();
    }
    for prefix in ["~/", "~\\"] {
        if let Some(rest) = trimmed.strip_prefix(prefix) {
            if rest.is_empty() {
                return home_dir();
            }
            return home_dir().join(rest);
        }
    }
    PathBuf::from(trimmed)
}

/// The user's home directory, or an empty path when it cannot be determined.
fn home_dir() -> PathBuf {
    for variable in ["HOME", "USERPROFILE"] {
        if let Some(value) = std::env::var_os(variable).filter(|value| !value.is_empty()) {
            return PathBuf::from(value);
        }
    }
    PathBuf::new()
}

/// The user's home directory, for the protection check.
///
/// Carried as a value rather than read inside the check, so the identity half can be
/// driven by a test: it needs a directory that really exists, and the process
/// environment is not a fixture.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Home {
    path: PathBuf,
    /// What the directory *is*, as opposed to how it is spelled.
    identity: Option<FileIdentity>,
}

impl Home {
    /// The home directory this process should protect, from the environment.
    fn detect() -> Option<Self> {
        let path = env_home_dir()?;
        let identity = file_identity(&path);
        Some(Self { path, identity })
    }
}

/// A file's identity on its volume, as the platform reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FileIdentity {
    volume: u64,
    file: u64,
}

/// The identity of what `path` leads to — a symlink is followed, because what matters
/// is the directory the deletion would reach.
#[cfg(unix)]
fn file_identity(path: &Path) -> Option<FileIdentity> {
    use std::os::unix::fs::MetadataExt;
    let metadata = std::fs::metadata(path).ok()?;
    Some(FileIdentity {
        volume: metadata.dev(),
        file: metadata.ino(),
    })
}

/// Windows: a stable (volume serial, file id) pair needs `GetFileInformationByHandle`,
/// and the `MetadataExt` fields that expose it are unstable. Until the Windows path is
/// verified on a real machine (D16) this answers nothing, which leaves the spelling and
/// structural guards exactly as strong as they were — it does not weaken them.
#[cfg(not(unix))]
fn file_identity(_path: &Path) -> Option<FileIdentity> {
    None
}

/// Top-level root + system location checks.
fn reject_roots(path: &Path, home: Option<&Home>) -> Result<(), PathRejection> {
    // A component count ≤ 2 means the "/xxx" level: no top-level directory itself may
    // be deleted. This single rule also covers / itself, /etc, /Applications,
    // /Library, /Volumes, /Users, /tmp, /opt, etc., without relying on a
    // platform-specific list.
    if path.components().count() <= 2 {
        return Err(PathRejection::TopLevelRoot(display(path)));
    }

    if let Some(hit) = protected_location(path, home) {
        return Err(PathRejection::ProtectedLocation(hit));
    }

    Ok(())
}

/// Returns the matched rule text when a protected location is hit, otherwise `None`.
///
/// Every comparison here goes through [`guard_key`]: the list is written in one
/// spelling, and a path can reach the same directory in another.
fn protected_location(path: &Path, home: Option<&Home>) -> Option<String> {
    let key = guard_key(path);

    #[cfg(unix)]
    {
        for prefix in DENIED_PREFIXES {
            if key_under(&key, &guard_key(Path::new(prefix))) {
                return Some((*prefix).to_string());
            }
        }

        // /usr is forbidden as a whole, but /usr/local is where users and package
        // managers install software.
        let usr = guard_key(Path::new("/usr"));
        let usr_local = guard_key(Path::new("/usr/local"));
        if key == usr || (key_under(&key, &usr) && !key_under(&key, &usr_local)) {
            return Some("/usr".to_string());
        }
        if key == usr_local {
            return Some("/usr/local".to_string());
        }
    }

    #[cfg(windows)]
    {
        // The key already folds `\\?\` and separators, so a junction into `C:\Windows`
        // compares equal to `%WINDIR%` even though their `Component::Prefix` values
        // differ — which is why this compares strings rather than `Path`s.
        if key.len() == 2 && key.ends_with(':') {
            return Some(display(path));
        }

        for name in ["WINDIR", "ProgramFiles", "ProgramFiles(x86)", "ProgramData"] {
            if let Some(root) = std::env::var_os(name) {
                let root_display = root.to_string_lossy().into_owned();
                if key_under(&key, &windows_path_key(&root_display)) {
                    return Some(root_display);
                }
            }
        }
    }

    // The user's home directory itself may not be deleted; contents under it are not
    // restricted by this rule.
    //
    // Matched by identity as well as by spelling. A second route to the same directory
    // — a link, or a parent reached through one — is invisible to a string comparison
    // and visible to this one, and the symlink checks do not cover it because the link
    // sits above the scan root, which is the user's own choice of root.
    if let Some(home) = home {
        if key == guard_key(&home.path) {
            return Some(display(&home.path));
        }
        if let (Some(candidate), Some(known)) = (file_identity(path), home.identity) {
            if candidate == known {
                return Some(display(&home.path));
            }
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
    // Both roots are folded: `/Users` was already, while `/HOME/andy` was not
    // recognised at all — the same directory, reachable under a spelling the
    // structural fallback did not cover.
    first.eq_ignore_ascii_case("Users") || first.eq_ignore_ascii_case("home")
}

/// The comparison key for a path, used by every check that decides protection.
///
/// `Path::starts_with` compares components byte for byte and never asks the
/// filesystem, while `realpath` does not correct the case of a name it passes
/// through. On a filesystem that ignores case — macOS by default, Windows always —
/// `/SYSTEM` and `/System` are the same directory, so a candidate spelled the first
/// way used to walk past the protected-location list. Folding case before comparing
/// is what makes the guard independent of how the path was written.
///
/// Folding can only ever **add** refusals, which is the direction D10 requires, so it
/// is applied on every unix as well rather than only where the filesystem ignores
/// case: a name that folds onto a protected prefix is a name no legitimate project
/// uses, and the two platforms then behave the same way.
///
/// What is *reported* is unaffected — a rejection still quotes the path as it was
/// given (see `ValidatedPath::target` and the rejection's evidence).
fn guard_key(path: &Path) -> String {
    // Windows additionally folds separators and the verbatim prefix; both forms of
    // `\\?\C:\...` have to compare equal.
    #[cfg(windows)]
    let key = windows_path_key(&display(path));
    #[cfg(not(windows))]
    let key = display(path).to_lowercase();
    key
}

/// Whether `path_key` names `root_key` itself or sits underneath it.
///
/// The comparison respects component boundaries: `/var/dbx` is **not** under
/// `/var/db`. A plain string prefix would say it was, which is a looser answer than
/// the component comparison this replaces — the opposite of the direction D10 allows.
/// Both separators are accepted because the key's separator depends on the platform.
fn key_under(path_key: &str, root_key: &str) -> bool {
    if root_key.is_empty() {
        return false;
    }
    match path_key.strip_prefix(root_key) {
        Some(rest) => rest.is_empty() || rest.starts_with('/') || rest.starts_with('\\'),
        None => false,
    }
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

/// Why Win32 would read a single component as a *different* name, or `None`.
///
/// Four rules make the name on disk differ from the name Win32 opens: a trailing `.`
/// or space is stripped, `NUL` / `CON` / `PRN` / `AUX` / `COM1`…`LPT9` name devices in
/// any directory, and what follows a `:` is an alternate data stream. Only Win32 has
/// them; the predicate is a pure function of `&str` so its table can be tested
/// anywhere, and it is *applied* only on Windows.
#[cfg(any(windows, test))]
fn win32_ambiguous_name(component: &str) -> Option<&'static str> {
    if component.ends_with('.') || component.ends_with(' ') {
        return Some("a trailing dot or space, which Win32 strips");
    }
    if component.contains(':') {
        return Some("a colon, which Win32 reads as an alternate data stream");
    }
    // The device rules match the stem — everything before the first dot, right-trimmed
    // — so `NUL.txt` is as reserved as `NUL`.
    let stem = component.split('.').next().unwrap_or(component).trim_end();
    if is_reserved_device_name(stem) {
        return Some("a device name, reserved in every directory");
    }
    None
}

/// Whether `stem` is one of the names Win32 treats as a device.
///
/// The digits are `1`–`9`: `COM0` and `LPT0` are ordinary names, and `COM10` is not a
/// device either, so a prefix match on `COM` alone would refuse both.
#[cfg(any(windows, test))]
fn is_reserved_device_name(stem: &str) -> bool {
    const FIXED: &[&str] = &["CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$"];
    if FIXED.iter().any(|name| stem.eq_ignore_ascii_case(name)) {
        return true;
    }
    let upper = stem.to_ascii_uppercase();
    for prefix in ["COM", "LPT"] {
        if let Some(digit) = upper.strip_prefix(prefix) {
            if digit.len() == 1 && matches!(digit.as_bytes()[0], b'1'..=b'9') {
                return true;
            }
        }
    }
    false
}

/// The literal (lexically normalised) and resolved forms of the expected parent.
///
/// Both are computed unconditionally so the collapse guard compares a candidate
/// against either form, even when the parent is a symlink.
fn expected_parent_forms(ctx: &DeletionContext) -> (PathBuf, PathBuf) {
    let literal = lexical_normalize(&ctx.expected_parent);
    let resolved = resolve_for_inspection(&ctx.expected_parent)
        .map_or_else(|_| literal.clone(), |resolution| resolution.resolved);
    (literal, resolved)
}

/// Whether the candidate equals either form of the expected parent (check 7).
///
/// Compared through the guard key: `~/Dev` and `~/dev` are one directory, and a
/// collapse guard that misses one spelling of the parent is a guard that lets an
/// empty variable through on the machine where it matters.
fn is_empty_variable_collapse(
    candidate: &Path,
    parent_literal: &Path,
    parent_resolved: &Path,
) -> bool {
    let key = guard_key(candidate);
    key == guard_key(parent_literal) || key == guard_key(parent_resolved)
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
    // An empty path normalises to the root. Written as a `return` rather than a
    // `push`: pushing a root component *replaces* whatever came before, and although
    // `out` is empty in this branch, saying the result outright is clearer than
    // leaning on that.
    if out.as_os_str().is_empty() {
        return PathBuf::from(Component::RootDir.as_os_str());
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
    std::fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_symlink())
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
                    .map_or(0, |d| d.as_nanos())
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
        assert!(result.is_err(), "should be rejected but passed: {path}");
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

    /// A mixed-case spelling of `path`, the way a paste or a completion produces one.
    #[cfg(unix)]
    fn alternate_case(path: &str) -> String {
        path.chars()
            .enumerate()
            .map(|(index, ch)| {
                if index % 2 == 0 {
                    ch.to_ascii_uppercase()
                } else {
                    ch
                }
            })
            .collect()
    }

    /// The protected list under three spellings.
    ///
    /// This is also the mutation check the proposal asks for: remove the folding from
    /// `guard_key` and every upper- or mixed-case row below stops being refused, so the
    /// test fails rather than silently passing.
    #[test]
    #[cfg(unix)]
    fn refuses_a_protected_location_whatever_case_it_is_written_in() {
        for prefix in DENIED_PREFIXES {
            // A depth-2 child: every entry is a *prefix* rule, so this is what a real
            // candidate looks like.
            let child = format!("{prefix}/child");
            for spelling in [
                child.to_uppercase(),
                child.to_lowercase(),
                alternate_case(&child),
            ] {
                assert_rejected(&spelling, &ctx());
            }
        }

        // The same, for the `/usr` rule, which is written separately from the list
        // because `/usr/local` is carved out of it.
        assert_rejected("/USR/bin/tool", &ctx());
        assert_rejected("/UsR/BiN/tool", &ctx());
    }

    #[test]
    #[cfg(unix)]
    fn refuses_the_canonical_spelling_of_a_symlinked_protected_location() {
        // `/var` and `/etc` are symlinks into `/private`, so each of these names one
        // directory under two spellings. The list carried only the short form, and a
        // candidate written the canonical way matched neither the literal comparison
        // nor the resolved one — it was accepted.
        assert_rejected("/private/var/db/x", &ctx());
        assert_rejected("/private/etc/x", &ctx());
        // The short form keeps being refused.
        assert_rejected("/var/db/x", &ctx());
        assert_rejected("/etc/x", &ctx());
    }

    #[test]
    #[cfg(unix)]
    fn folding_case_does_not_turn_ordinary_paths_into_protected_ones() {
        // Each of these differs from a protected prefix by at least one whole
        // component, which is what `key_under` has to keep true: a plain string prefix
        // would treat `/var/db-backup` as being under `/var/db`, and a guard that
        // refuses a user's backup directory is a guard that gets turned off.
        for path in [
            "/Users/andy/MySrc/proj/node_modules",
            "/Volumes/External/repo/dist",
            "/usr/local/src/proj/target",
            "/var/db-backup/proj",
            "/Systematic/proj",
            "/development/proj",
            "/etcetera/proj",
        ] {
            validate_path_for_deletion(Path::new(path), &ctx())
                .unwrap_or_else(|error| panic!("should be accepted: {path} ({error})"));
        }
    }

    #[test]
    fn the_home_root_shape_is_recognised_in_either_spelling() {
        // `Users` was folded and `home` was compared exactly, so `/HOME/andy` — the
        // same shape on the platforms where the root is spelled that way — was not
        // recognised at all.
        for path in [
            "/Users/example",
            "/USERS/example",
            "/home/example",
            "/HOME/example",
        ] {
            assert_rejected(path, &ctx());
        }
        // A child of a home root is not protected by this rule.
        validate_path_for_deletion(Path::new("/Users/example/proj"), &ctx())
            .expect("a project under a home root is ordinary");
    }

    #[test]
    fn whitelist_rules_match_whatever_case_either_side_uses() {
        let rules = vec![PathBuf::from("/Users/andy/Dev")];
        for candidate in [
            "/Users/andy/Dev/proj",
            "/Users/andy/dev/proj",
            "/USERS/ANDY/DEV/proj",
        ] {
            assert!(
                whitelist_hit(Path::new(candidate), &rules).is_some(),
                "a rule protects one directory, not one spelling of it: {candidate}"
            );
        }
        // A sibling that only shares a string prefix is not protected.
        assert!(whitelist_hit(Path::new("/Users/andy/Development/proj"), &rules).is_none());
    }

    #[test]
    fn the_collapse_guard_compares_through_the_guard_key() {
        // The parent written with a different case is still the parent; missing it
        // would let an empty variable through on a filesystem that ignores case.
        let context = DeletionContext::new("/expected/parent");
        assert!(matches!(
            validate_path_for_deletion(Path::new("/EXPECTED/PARENT"), &context),
            Err(PathRejection::EmptyVariableCollapse(_))
        ));
    }

    #[test]
    fn the_guard_key_folds_case_and_respects_component_boundaries() {
        assert_eq!(guard_key(Path::new("/System/Library")), "/system/library");
        assert!(key_under("/var/db/x", "/var/db"));
        assert!(!key_under("/var/db-backup", "/var/db"));
        assert!(!key_under("/anywhere", ""));
        // Both separators are accepted: the key's separator depends on the platform.
        assert!(key_under(r"c:\windows\system32", r"c:\windows"));
        assert!(!key_under(r"c:\windowsapps", r"c:\windows"));
    }

    #[test]
    fn refuses_the_home_directory_reached_by_a_different_route() {
        // The environment's home is protected by spelling; a second route to the same
        // directory is not a second spelling. The identity half is what catches it, and
        // it needs a directory that exists — hence a fixture rather than `HOME`.
        let temp = TempDir::new("home-identity");
        let real_home = temp.path().join("real-home");
        std::fs::create_dir_all(&real_home).expect("create the fake home");
        let sibling = temp.path().join("sibling");
        std::fs::create_dir_all(&sibling).expect("create the sibling");

        let home = Home {
            path: real_home.clone(),
            identity: file_identity(&real_home),
        };
        assert!(
            home.identity.is_some(),
            "the fixture is a real directory, so its identity is known"
        );

        // The canonical spelling, matched as before.
        assert!(protected_location(&real_home, Some(&home)).is_some());

        // The same directory through a link: a different string, the same file.
        let link = temp.path().join("link-to-home");
        std::os::unix::fs::symlink(&real_home, &link).expect("symlink to the fake home");
        assert!(
            protected_location(&link, Some(&home)).is_some(),
            "a link to the home directory is the home directory"
        );

        // A sibling of it is not, and neither is a child of the home directory: the
        // rule protects the directory itself, not everything near or under it.
        assert!(protected_location(&sibling, Some(&home)).is_none());
        assert!(protected_location(&real_home.join("projects"), Some(&home)).is_none());
    }

    #[test]
    fn the_checks_run_in_the_order_the_contract_states() {
        // D10 says the path must pass *the eight checks in order*. The set was tested;
        // the order was not — two checks could swap and every other test would still
        // pass, even though the order decides which rejection the user is shown. Each
        // case below applies at least two rules, so it pins the one that wins.
        let protected = DeletionContext::new("/expected/parent");
        let whitelisted = DeletionContext::new("/elsewhere")
            .with_whitelist(vec![PathBuf::from("/protected/area")]);
        let collapsing = DeletionContext::new("/expected/parent")
            .with_whitelist(vec![PathBuf::from("/expected/parent")]);

        // A control character (check 5) beats a `..` component (check 4).
        assert!(matches!(
            validate_path_for_deletion(Path::new("/a\u{7}/../b"), &protected),
            Err(PathRejection::ControlCharacter(_))
        ));
        // `..` beats the protected list.
        assert!(matches!(
            validate_path_for_deletion(Path::new("/System/../bin/tool"), &protected),
            Err(PathRejection::ParentTraversal(_))
        ));
        // Within check 6, being a top-level root is decided before the protected list.
        assert!(matches!(
            validate_path_for_deletion(Path::new("/System"), &protected),
            Err(PathRejection::TopLevelRoot(_))
        ));
        // A protected location beats an empty-variable collapse.
        assert!(matches!(
            validate_path_for_deletion(Path::new("/System/Library"), &protected),
            Err(PathRejection::ProtectedLocation(_))
        ));
        // A collapse beats the whitelist, which is the last check of all.
        assert!(matches!(
            validate_path_for_deletion(Path::new("/expected/parent"), &collapsing),
            Err(PathRejection::EmptyVariableCollapse(_))
        ));
        // And what the whitelist still catches, once nothing earlier applies.
        assert!(matches!(
            validate_path_for_deletion(Path::new("/protected/area/proj"), &whitelisted),
            Err(PathRejection::Whitelisted(_))
        ));
    }

    #[test]
    fn recognises_the_names_win32_would_read_as_something_else() {
        for name in [
            // Devices, in any directory, matched on the stem.
            "NUL",
            "nul",
            "CON",
            "PRN",
            "AUX",
            "NUL.txt",
            "con.log",
            "COM1",
            "COM9",
            "LPT1",
            "LPT9",
            "COM1.dat",
            // Stripped by Win32, so the name on disk is not the name it opens.
            "trailing.",
            "trailing ",
            // An alternate data stream.
            "stream:name",
        ] {
            assert!(
                win32_ambiguous_name(name).is_some(),
                "{name} should be read as a different name by Win32"
            );
        }

        for name in [
            // The rules match whole names, not prefixes: these are ordinary files.
            "concepts",
            "nul-test",
            "console",
            "COM0",
            "LPT0",
            "COM10",
            "a.b",
            "archive.tar.gz",
        ] {
            assert!(
                win32_ambiguous_name(name).is_none(),
                "{name} is an ordinary name and must not be refused"
            );
        }

        // `Report.` is legal on macOS and is named here on purpose: the *predicate*
        // answers what Win32 would do with the name — it strips the dot — while the
        // guard that uses it runs only on Windows. A macOS path may keep the dot.
        assert!(win32_ambiguous_name("Report.").is_some());
        #[cfg(not(windows))]
        validate_path_for_deletion(Path::new("/tmp/Report."), &ctx())
            .expect("the Win32 rule must not reach a platform that does not have it");
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
            other => {
                panic!("a candidate equal to the resolved parent must be rejected, got: {other:?}")
            }
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
    fn empty_whitelist_rules_are_skipped_not_matched_everywhere() {
        // A malformed (empty) rule normalises to the root and would otherwise match
        // every absolute path — silently protecting everything.
        let context = DeletionContext::new(UNRELATED_PARENT).with_whitelist(vec![
            PathBuf::new(),
            PathBuf::from("/Users/andy/MySrc/keep-me"),
        ]);
        // The real rule still works.
        assert_rejected("/Users/andy/MySrc/keep-me/target", &context);
        // The empty rule does not turn every other path into a whitelist hit.
        assert!(
            validate_path_for_deletion(Path::new("/Users/andy/MySrc/other/target"), &context)
                .is_ok()
        );
        // A whitelist that contains *only* empty rules protects nothing.
        let only_empty = DeletionContext::new(UNRELATED_PARENT)
            .with_whitelist(vec![PathBuf::new(), PathBuf::new()]);
        assert!(validate_path_for_deletion(
            Path::new("/Users/andy/MySrc/other/target"),
            &only_empty
        )
        .is_ok());
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
            other => {
                panic!("the whitelist must apply to the resolved path as well, got: {other:?}")
            }
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
                "a link resolving into a whitelisted directory must be rejected, got: {other:?}"
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
                    "evidence should point at the protected location: {hit}"
                );
            }
            other => panic!("expected a symlink escape, got: {other:?}"),
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
            other => panic!("expected a symlink escape, got: {other:?}"),
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

    #[test]
    fn a_path_inside_the_system_trash_is_recognised() {
        // The real case this exists for: Codex registered a worktree under the
        // wrong repository, the user trashed the directory, and `git worktree
        // list` kept reporting it — the directory is still there, so Git does
        // not mark it prunable. Without this check the scan lists it, measures
        // it and offers to remove it.
        for path in [
            "/Users/andy/.Trash/open-voice-bridge-wrong-repo-20260907",
            "/Users/andy/.Trash/open-voice-bridge-wrong-repo-20260907/.git",
            "/Users/andy/.Trash",
            // The per-volume Trash: the boot volume, and an external one.
            "/.Trashes/501/repo--feature",
            "/Volumes/Backup/.Trashes/501/repo--feature",
        ] {
            assert!(
                is_in_system_trash(Path::new(path)),
                "should be inside the Trash: {path}"
            );
        }
    }

    #[test]
    #[cfg(windows)]
    fn a_path_inside_the_windows_recycle_bin_is_recognised() {
        // On Windows the drive letter is a prefix component, so `$Recycle.Bin`
        // is the first normal component under the drive root. On Unix the same
        // string is one long file name — which is why this case is
        // Windows-only, and why the shared check matches whole components
        // rather than string prefixes.
        for path in [
            r"C:\$Recycle.Bin\S-1-5-21-1234\repo--feature",
            r"d:\$recycle.bin\repo--feature",
        ] {
            assert!(
                is_in_system_trash(Path::new(path)),
                "should be inside the Recycle Bin: {path}"
            );
        }
    }

    #[test]
    #[cfg(unix)]
    fn the_trash_check_ignores_case_but_not_component_prefixes() {
        // Case folding, because the filesystem that matters ignores it.
        for path in [
            "/users/andy/.trash/repo--feature",
            "/Users/Andy/.TRASH/repo--feature",
            "/volumes/backup/.trashes/501/repo--feature",
        ] {
            assert!(
                is_in_system_trash(Path::new(path)),
                "one directory, several spellings: {path}"
            );
        }
        // A directory whose name merely *starts* with a Trash name is not the
        // Trash: matching by string prefix would silently skip a real project.
        for path in [
            "/Users/andy/.Trashcan/repo",
            "/Users/andy/.Trashes-old/repo",
            "/Users/andy/MySrc/proj/$Recycle.Bin-backup/repo",
            "/Users/andy/MySrc/proj/node_modules",
            "/Volumes/External/repo/dist",
            "/Users/andy/Develop/Src/AISrc/.worktrees/repo--feature",
        ] {
            assert!(
                !is_in_system_trash(Path::new(path)),
                "an ordinary path must not read as Trash: {path}"
            );
        }
    }
}
