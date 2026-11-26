//! Guarded system-cache inventory.
//!
//! This is deliberately a separate surface from the repository cleanup. A cache
//! directory is not inside a worktree and has no Git history to judge, so it
//! cannot borrow the artifact verdict. The rules here are narrower:
//!
//! - only directories under a known cache root are considered;
//! - a known cache gets a positive `af.rebuildable` reason, while an unknown
//!   owner gets `af.unknown_cache` at `Review` and is never selected by default;
//! - a credential-looking file is `af.credentials` at `Review` (a package cache
//!   carries third-party fixtures), while a nested repository, an escaping symlink
//!   or a whitelist hit stays `Unsafe`;
//! - every directory is moved through the system Trash, never permanently
//!   deleted;
//! - the executor re-checks the record before it moves anything.
//!
//! The scan is read-only. The `CacheSnapshot` is intentionally separate from
//! `ProjectSnapshot`: a project's cleanup plan must not silently start touching
//! `~/Library/Caches` because a cache record happened to be in the same payload.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

use crate::artifact::{credential_in_within, credential_in_worktree, measure_dir, CredentialScan};
use crate::cleanup::{move_to_trash, ExecuteOptions, ExecutionOutcome};
use crate::log;
use crate::path::{is_in_system_trash, validate_path_for_deletion, whitelist_hit, DeletionContext};
use crate::reason::{Reason, ReasonCode, SafetyAssessment, SafetyStatus};
use crate::timefmt::{elapsed_days, format_rfc3339, humanize_days};
use crate::{
    ArtifactRecord, CleanupPlan, DetailArg, DetailCode, ExecutionResult, HistoryAction,
    HistoryEntry, ResultEntry,
};

/// Contract version of a cache snapshot. It is separate from the project
/// snapshot: the two payloads are discovered and consumed independently.
pub const CACHE_SCHEMA_VERSION: u32 = 1;

/// Injection points for one cache scan.
#[derive(Debug, Clone)]
pub struct CacheScanOptions {
    /// Budget for one directory size measurement.
    pub per_dir_timeout: Duration,
    /// A cache changed within this window is `Review`, not `Safe`.
    pub recent_activity_days: u32,
    /// User whitelist rules, already expanded to absolute paths.
    pub whitelist: Vec<PathBuf>,
    /// Injected clock, so the activity window is testable.
    pub now: SystemTime,
}

impl Default for CacheScanOptions {
    fn default() -> Self {
        Self {
            per_dir_timeout: Duration::from_mins(5),
            recent_activity_days: crate::DEFAULT_ACTIVITY_DAYS,
            whitelist: Vec::new(),
            now: SystemTime::now(),
        }
    }
}

/// The result of one system-cache inventory.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CacheSnapshot {
    pub schema_version: u32,
    pub scanned_at: String,
    /// Cache directories are represented with the existing artifact record so the
    /// UI can reuse the safety badge, reason list and size formatting. They are
    /// never merged into a project snapshot.
    pub records: Vec<ArtifactRecord>,
    pub warnings: Vec<String>,
}

/// Contract version of a generated-build-cache inventory.
pub const GENERATED_CACHE_SCHEMA_VERSION: u32 = 1;

/// A read-only inventory of build directories marked by `CACHEDIR.TAG`.
///
/// This is separate from both `ProjectSnapshot` and `CacheSnapshot`: a build root
/// found under an arbitrary folder is not a repository and not one of the known
/// system-cache roots. The tag is the evidence that the directory is a
/// disposable build cache.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GeneratedCacheSnapshot {
    pub schema_version: u32,
    /// The folder the user selected for this inventory.
    pub root: String,
    pub scanned_at: String,
    pub records: Vec<ArtifactRecord>,
    pub warnings: Vec<String>,
}

/// One candidate cache directory and the root it belongs to.
#[derive(Debug, Clone)]
struct CacheSource {
    path: PathBuf,
    root: PathBuf,
    kind: &'static str,
    known: bool,
}

fn home() -> Option<PathBuf> {
    for variable in ["HOME", "USERPROFILE"] {
        if let Some(value) = std::env::var_os(variable).filter(|value| !value.is_empty()) {
            return Some(PathBuf::from(value));
        }
    }
    None
}

#[cfg(target_os = "windows")]
fn local_app_data() -> Option<PathBuf> {
    if let Some(value) = std::env::var_os("LOCALAPPDATA").filter(|value| !value.is_empty()) {
        return Some(PathBuf::from(value));
    }
    home().map(|home| home.join("AppData").join("Local"))
}

// The two helpers below enumerate the children of a cache parent, which only the
// macOS and Linux tables do. Windows names each package-manager cache directly
// under `%LOCALAPPDATA%`, so compiling the walk there would leave it unused — and
// the workspace gate denies `unused`.
#[cfg(not(target_os = "windows"))]
fn is_known_cache_name(name: &str, known: &[&str]) -> bool {
    known
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(name))
}

#[cfg(not(target_os = "windows"))]
fn add_children(sources: &mut Vec<CacheSource>, parent: &Path, known: &[&str], kind: &'static str) {
    let Ok(entries) = std::fs::read_dir(parent) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == ".Trash" || name == ".Trashes" || name == "$Recycle.Bin" {
            continue;
        }
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_dir() && !file_type.is_symlink() {
            continue;
        }
        sources.push(CacheSource {
            path,
            root: parent.to_path_buf(),
            kind,
            known: is_known_cache_name(&name, known),
        });
    }
}

fn add_known(sources: &mut Vec<CacheSource>, path: PathBuf, kind: &'static str) {
    let root = path
        .parent()
        .map_or_else(|| path.clone(), Path::to_path_buf);
    sources.push(CacheSource {
        path,
        root,
        kind,
        known: true,
    });
}

#[cfg(target_os = "macos")]
fn default_sources() -> Vec<CacheSource> {
    let Some(home) = home() else {
        return Vec::new();
    };
    let mut sources = Vec::new();
    add_children(
        &mut sources,
        &home.join("Library").join("Caches"),
        &[
            "Homebrew",
            "ms-playwright",
            "org.swift.swiftpm",
            "cargo-xwin",
        ],
        "cache",
    );
    add_children(
        &mut sources,
        &home.join(".cache"),
        &["uv", "codex-runtimes", "whisper", "huggingface", "prisma"],
        "cache",
    );
    add_children(
        &mut sources,
        &home
            .join("Library")
            .join("Application Support")
            .join("Caches"),
        &[],
        "cache",
    );
    add_known(
        &mut sources,
        home.join("Library")
            .join("Developer")
            .join("Xcode")
            .join("DerivedData"),
        "xcode-derived-data",
    );
    add_known(
        &mut sources,
        home.join("Library")
            .join("Developer")
            .join("Xcode")
            .join("iOS DeviceSupport"),
        "xcode-device-support",
    );
    add_known(
        &mut sources,
        home.join(".npm").join("_cacache"),
        "npm-cache",
    );
    add_known(
        &mut sources,
        home.join("Library").join("pnpm").join("store"),
        "pnpm-store",
    );
    add_known(
        &mut sources,
        home.join(".cargo").join("registry").join("cache"),
        "cargo-registry-cache",
    );
    add_known(&mut sources, home.join(".pub-cache"), "pub-cache");
    sources
}

#[cfg(target_os = "windows")]
fn default_sources() -> Vec<CacheSource> {
    let Some(local) = local_app_data() else {
        return Vec::new();
    };
    let mut sources = Vec::new();
    add_known(&mut sources, local.join("npm-cache"), "npm-cache");
    add_known(&mut sources, local.join("pnpm").join("store"), "pnpm-store");
    add_known(&mut sources, local.join("uv").join("cache"), "uv-cache");
    add_known(&mut sources, local.join("ms-playwright"), "playwright");
    // Cargo keeps its registry cache in the user profile, not in
    // %LOCALAPPDATA%. The path was wrong when this list first shipped; the
    // separate `home()` lookup is what keeps the Windows table honest.
    if let Some(home) = home() {
        add_known(
            &mut sources,
            home.join(".cargo").join("registry").join("cache"),
            "cargo-registry-cache",
        );
    }
    add_known(&mut sources, local.join("Pub").join("Cache"), "pub-cache");
    sources
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn default_sources() -> Vec<CacheSource> {
    let Some(home) = home() else {
        return Vec::new();
    };
    let mut sources = Vec::new();
    add_children(
        &mut sources,
        home.join(".cache"),
        &["uv", "whisper"],
        "cache",
    );
    add_known(
        &mut sources,
        home.join(".npm").join("_cacache"),
        "npm-cache",
    );
    add_known(
        &mut sources,
        home.join(".local").join("share").join("pnpm").join("store"),
        "pnpm-store",
    );
    add_known(
        &mut sources,
        home.join(".cargo").join("registry").join("cache"),
        "cargo-registry-cache",
    );
    sources
}

/// Runs one cache scan. This is read-only.
pub fn scan_system_caches(options: &CacheScanOptions) -> CacheSnapshot {
    let sources = default_sources();
    scan_sources(&sources, options)
}

/// One depth bound for the optional walk below a folder the user selected.
///
/// The generated cache is a build root, not a repository; it normally sits one
/// or two levels below the folder the user picked. The bound keeps a mistaken
/// selection of a home directory from walking forever.
const GENERATED_MAX_DEPTH: usize = 6;
const GENERATED_MAX_ENTRIES: usize = 200_000;

fn is_tagged_build_cache(path: &Path) -> bool {
    path.join("CACHEDIR.TAG").is_file()
}

/// Names that cannot contain a build cache worth walking into.
///
/// A tagged directory is checked before this list: if a build tool named its
/// root `build`, the tag still makes it a candidate.
fn skip_generated_walk(name: &str) -> bool {
    name.starts_with('.')
        || matches!(
            name,
            "node_modules"
                | "target"
                | "Pods"
                | "Carthage"
                | "DerivedData"
                | "checkouts"
                | "SourcePackages"
                | "Library"
                | "Applications"
                | "Downloads"
                | "Movies"
                | "Music"
                | "Pictures"
        )
}

fn generated_source(path: &Path) -> CacheSource {
    CacheSource {
        path: path.to_path_buf(),
        root: path
            .parent()
            .map_or_else(|| path.to_path_buf(), Path::to_path_buf),
        kind: "build-cache",
        known: true,
    }
}

/// Finds directories carrying `CACHEDIR.TAG` below `root`.
///
/// The tagged directory is the candidate, not a child: the whole build root is
/// what a build tool can recreate, and its dependency checkouts are already part
/// of it. Walking stops at each tag, so one cache is not reported several times.
fn collect_generated_sources(
    root: &Path,
    sources: &mut Vec<CacheSource>,
    warnings: &mut Vec<String>,
) {
    if is_tagged_build_cache(root) {
        sources.push(generated_source(root));
        return;
    }

    let mut stack = vec![(root.to_path_buf(), 0usize)];
    let mut visited = 0usize;
    while let Some((directory, depth)) = stack.pop() {
        if is_in_system_trash(&directory) {
            continue;
        }
        let entries = match std::fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                warnings.push(format!(
                    "{} could not be read while looking for build caches: {error}",
                    directory.display()
                ));
                continue;
            }
        };
        for entry in entries.flatten() {
            visited += 1;
            if visited > GENERATED_MAX_ENTRIES {
                warnings.push(format!(
                    "the build-cache scan stopped after {GENERATED_MAX_ENTRIES} entries; some caches may be missing"
                ));
                return;
            }
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_symlink() || !file_type.is_dir() {
                continue;
            }
            if is_in_system_trash(&path) {
                continue;
            }
            if is_tagged_build_cache(&path) {
                sources.push(generated_source(&path));
                continue;
            }
            if depth < GENERATED_MAX_DEPTH && !skip_generated_walk(&name) {
                stack.push((path, depth + 1));
            }
        }
    }
}

/// Inventories `CACHEDIR.TAG` build roots below one user-selected folder.
///
/// This is a separate surface from `scan_system_caches`: the root is chosen by
/// the reader, the tag is the evidence, and the result still goes through the
/// same cache record and Trash executor. It never scans a repository, and it
/// never decides that an untagged directory is a cache.
pub fn scan_generated_caches(root: &Path, options: &CacheScanOptions) -> GeneratedCacheSnapshot {
    let root = crate::path::expand_tilde(&root.to_string_lossy());
    let mut warnings = Vec::new();
    let mut sources = Vec::new();
    if root.is_dir() {
        collect_generated_sources(&root, &mut sources, &mut warnings);
    } else {
        warnings.push(format!(
            "the build-cache root does not exist or is not a directory: {}",
            root.display()
        ));
    }

    let CacheSnapshot {
        records,
        warnings: mut cache_warnings,
        ..
    } = scan_sources(&sources, options);
    warnings.append(&mut cache_warnings);
    if records.is_empty() && warnings.is_empty() {
        warnings.push(format!(
            "no directories carrying CACHEDIR.TAG were found below {}",
            root.display()
        ));
    }

    GeneratedCacheSnapshot {
        schema_version: GENERATED_CACHE_SCHEMA_VERSION,
        root: root.display().to_string(),
        scanned_at: format_rfc3339(SystemTime::now()),
        records,
        warnings,
    }
}

fn scan_sources(sources: &[CacheSource], options: &CacheScanOptions) -> CacheSnapshot {
    let mut records = Vec::new();
    let mut warnings = Vec::new();
    let mut seen: HashSet<PathBuf> = HashSet::new();

    for source in sources {
        if !seen.insert(source.path.clone()) {
            continue;
        }
        // A path inside the Trash is never read: the Trash is a one-way boundary
        // for this app (D5).
        if is_in_system_trash(&source.path) {
            continue;
        }
        match std::fs::symlink_metadata(&source.path) {
            Ok(metadata) if metadata.file_type().is_dir() || metadata.file_type().is_symlink() => {}
            Ok(_) => continue,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                warnings.push(format!(
                    "{} could not be inspected: {error}",
                    source.path.display()
                ));
                continue;
            }
        }
        let mut notes = Vec::new();
        if let Some(record) = check_cache(source, options, &mut notes) {
            records.push(record);
        }
        warnings.append(&mut notes);
    }

    records.sort_by(|left, right| {
        right
            .size_bytes
            .unwrap_or(0)
            .cmp(&left.size_bytes.unwrap_or(0))
            .then_with(|| left.path.cmp(&right.path))
    });

    CacheSnapshot {
        schema_version: CACHE_SCHEMA_VERSION,
        scanned_at: format_rfc3339(SystemTime::now()),
        records,
        warnings,
    }
}

fn check_cache(
    source: &CacheSource,
    options: &CacheScanOptions,
    warnings: &mut Vec<String>,
) -> Option<ArtifactRecord> {
    let path = source.path.as_path();
    let mut reasons = Vec::new();
    let dir_text = path.display().to_string();
    let link_meta = std::fs::symlink_metadata(path).ok()?;
    let is_symlink = link_meta.file_type().is_symlink();
    let mut symlink_target = None;

    if is_symlink {
        match std::fs::canonicalize(path) {
            Ok(target) => {
                let root =
                    std::fs::canonicalize(&source.root).unwrap_or_else(|_| source.root.clone());
                symlink_target = Some(target.display().to_string());
                if target.starts_with(&root) {
                    reasons.push(Reason::review(
                        ReasonCode::AfCannotVerify,
                        "could not complete the check",
                        format!(
                            "symlink target {} is inside the cache root; deleting it removes only the link",
                            target.display()
                        ),
                    ));
                } else {
                    reasons.push(Reason::blocking(
                        ReasonCode::AfSymlinkOutside,
                        "symlink pointing outside the cache root",
                        format!(
                            "target {} is outside {}; deleting the link would leave an unrelated directory in place",
                            target.display(),
                            source.root.display()
                        ),
                    ));
                }
            }
            Err(error) => {
                reasons.push(Reason::review(
                    ReasonCode::AfCannotVerify,
                    "could not complete the check",
                    format!("the symlink target of {dir_text} could not be resolved: {error}"),
                ));
            }
        }
    }

    if let Some(rule) = whitelist_hit(path, &options.whitelist) {
        reasons.push(Reason::blocking(
            ReasonCode::AfWhitelisted,
            "protected by a whitelist rule",
            format!("matches the whitelist rule {}", rule.display()),
        ));
    }

    if !is_symlink && path.join(".git").exists() {
        reasons.push(Reason::blocking(
            ReasonCode::AfNestedGit,
            "contains a nested Git repository",
            "a .git entry was found inside; deleting it would destroy repository history",
        ));
    }

    if !is_symlink {
        let budget = options.per_dir_timeout.min(Duration::from_mins(1)) / 2;
        let scan = if source.kind == "build-cache" {
            // Dependency checkouts inside a tagged build root contain their own
            // test fixtures, including publishable test keys. Those are part of
            // the generated tree, not a secret this cleanup would lose; the walk
            // still checks the build root's own files and outputs.
            credential_in_worktree(path, budget)
        } else {
            credential_in_within(path, 1_000_000, budget)
        };
        match scan {
            // Review, not Unsafe: a package cache is downloaded third-party
            // content, and its fixtures and public certificate material match the
            // same names a personal secret would. `Unsafe` would make the cache
            // permanently unselectable — the evidence is what a reader needs to
            // confirm, not a prohibition that leaves them no move.
            CredentialScan::Complete(Some(name)) => reasons.push(Reason::review(
                ReasonCode::AfCredentials,
                "contains what looks like a credential file",
                format!("{name} is inside; deleting the cache would delete it too"),
            )),
            CredentialScan::Complete(None) => {}
            CredentialScan::Truncated { scanned } => reasons.push(Reason::review(
                ReasonCode::AfCannotVerify,
                "could not finish checking for credentials",
                format!("looked at {scanned} entries before the time limit; a credential deeper inside would not have been found"),
            )),
            CredentialScan::Failed(error) => reasons.push(Reason::review(
                ReasonCode::AfCannotVerify,
                "could not complete the check",
                format!("could not list {dir_text} to check for credentials: {error}"),
            )),
        }
    }

    let measurement = if is_symlink {
        crate::artifact::Measurement {
            bytes: None,
            files: 0,
            newest: None,
            vanished: false,
        }
    } else {
        measure_dir(path, options.per_dir_timeout)
    };
    if measurement.vanished {
        return None;
    }
    if measurement.bytes.is_none() && !is_symlink {
        reasons.push(Reason::review(
            ReasonCode::AfCannotVerify,
            "could not complete the check",
            format!(
                "the size measurement did not complete within {}s; the size is unknown",
                options.per_dir_timeout.as_secs()
            ),
        ));
        warnings.push(format!(
            "size measurement of {dir_text} did not complete; the cache is shown as unmeasurable"
        ));
    }
    if !is_symlink && measurement.bytes.is_some() && measurement.files == 0 {
        reasons.push(Reason::safe(
            ReasonCode::AfEmpty,
            "the directory is empty",
            "it contains no files",
        ));
    }

    if source.known {
        reasons.push(Reason::safe(
            ReasonCode::AfRebuildable,
            "regenerable cache directory",
            format!(
                "known cache {} ({kind}); it can be rebuilt by its owning tool",
                path.file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or(&dir_text),
                kind = source.kind
            ),
        ));
    } else {
        reasons.push(Reason::review(
            ReasonCode::AfUnknownCache,
            "cache owner is not recognised",
            format!(
                "{} is under a known cache root but has no entry in the cache table; confirm its owner before removing it",
                path.file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or(&dir_text)
            ),
        ));
    }

    if let Some(newest) = measurement.newest {
        let age_days = elapsed_days(options.now, newest);
        if age_days <= u64::from(options.recent_activity_days) {
            reasons.push(Reason::review(
                ReasonCode::AfRecentActivity,
                "changed recently",
                format!("last activity: {}", humanize_days(age_days)),
            ));
        }
    }

    Some(ArtifactRecord {
        path: dir_text,
        kind: source.kind.to_string(),
        ecosystem: "system".to_string(),
        project_root: source.root.display().to_string(),
        worktree_path: None,
        size_bytes: measurement.bytes,
        file_count: measurement.files,
        last_updated_at: measurement.newest.map(format_rfc3339),
        git_ignored: None,
        git_tracked: None,
        is_symlink,
        symlink_target,
        safety: SafetyAssessment::from_reasons(reasons),
    })
}

fn primary_reason(record: &ArtifactRecord) -> Option<ReasonCode> {
    record
        .safety
        .reasons
        .iter()
        .max_by_key(|reason| reason.severity)
        .map(|reason| reason.code)
}

const fn runs_at_planned_level(before: SafetyStatus, after: SafetyStatus) -> bool {
    matches!(
        (before, after),
        (SafetyStatus::Safe, SafetyStatus::Safe) | (SafetyStatus::Review, SafetyStatus::Review)
    )
}

struct CacheAudit {
    entries: Vec<ResultEntry>,
    history: Vec<HistoryEntry>,
}

fn push(
    audit: &mut CacheAudit,
    target: &str,
    result: ExecutionResult,
    code: DetailCode,
    args: &[(&str, DetailArg)],
    size_bytes: Option<u64>,
    reason: Option<ReasonCode>,
) {
    let detail_args: std::collections::BTreeMap<String, DetailArg> = args
        .iter()
        .map(|(name, value)| ((*name).to_string(), value.clone()))
        .collect();
    let entry = ResultEntry {
        action: HistoryAction::TrashMove,
        target: target.to_string(),
        result,
        detail_code: Some(code),
        detail_args: detail_args.clone(),
        detail: None,
        size_bytes,
    };
    audit.history.push(HistoryEntry {
        at: format_rfc3339(SystemTime::now()),
        action: HistoryAction::TrashMove,
        target: target.to_string(),
        reason_code: reason,
        reason: None,
        result,
        size_bytes,
        detail_code: Some(code),
        detail_args,
        detail: None,
    });
    audit.entries.push(entry);
}

/// Executes the selected cache moves against the snapshot they came from.
///
/// The executor is intentionally separate from `cleanup::execute_plan`: a cache
/// directory has no Git index and no worktree, so reusing the artifact path would
/// ask questions that do not apply. The decision fields that do apply — path,
/// size, safety level, and the whitelist — are re-checked here.
pub fn execute_cache_plan(
    plan: &CleanupPlan,
    snapshot: &CacheSnapshot,
    opts: &ExecuteOptions,
) -> ExecutionOutcome {
    let whitelist: Vec<PathBuf> = opts
        .whitelist
        .iter()
        .map(|rule| crate::path::expand_tilde(rule))
        .collect();
    let options = CacheScanOptions {
        per_dir_timeout: opts.per_dir_timeout,
        recent_activity_days: opts.recent_activity_days,
        whitelist: whitelist.clone(),
        now: opts.now,
    };
    let mut audit = CacheAudit {
        entries: Vec::new(),
        history: Vec::new(),
    };

    for item in &plan.trash_moves {
        let target = item.path.as_str();
        let Some(record) = snapshot
            .records
            .iter()
            .find(|record| record.path == item.path)
        else {
            push(
                &mut audit,
                target,
                ExecutionResult::Skipped,
                DetailCode::ArtifactNotInScan,
                &[],
                None,
                None,
            );
            continue;
        };
        // The record's project_root is the cache root and therefore the
        // expected parent of the cache directory. For a known root that is itself
        // the candidate (for example DerivedData), it is that candidate's parent.
        let expected_parent = PathBuf::from(&record.project_root);
        let context = DeletionContext::new(expected_parent).with_whitelist(whitelist.clone());
        if validate_path_for_deletion(Path::new(&record.path), &context).is_err() {
            push(
                &mut audit,
                target,
                ExecutionResult::Skipped,
                DetailCode::PreconditionFailed,
                &[],
                record.size_bytes,
                primary_reason(record),
            );
            continue;
        }

        match std::fs::symlink_metadata(&record.path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                push(
                    &mut audit,
                    target,
                    ExecutionResult::Skipped,
                    DetailCode::ArtifactVanished,
                    &[],
                    record.size_bytes,
                    primary_reason(record),
                );
                continue;
            }
            Err(_) => {
                push(
                    &mut audit,
                    target,
                    ExecutionResult::Skipped,
                    DetailCode::ArtifactUnmeasurable,
                    &[],
                    record.size_bytes,
                    primary_reason(record),
                );
                continue;
            }
            Ok(_) => {}
        }

        let mut notes = Vec::new();
        let kind = if record.kind == "build-cache" {
            "build-cache"
        } else {
            "cache"
        };
        let source = CacheSource {
            path: PathBuf::from(&record.path),
            root: PathBuf::from(&record.project_root),
            kind,
            known: record
                .safety
                .reasons
                .iter()
                .any(|reason| reason.code == ReasonCode::AfRebuildable),
        };
        let Some(rechecked) = check_cache(&source, &options, &mut notes) else {
            push(
                &mut audit,
                target,
                ExecutionResult::Skipped,
                DetailCode::ArtifactVanished,
                &[],
                record.size_bytes,
                primary_reason(record),
            );
            continue;
        };
        if record.size_bytes != rechecked.size_bytes {
            push(
                &mut audit,
                target,
                ExecutionResult::Skipped,
                DetailCode::ArtifactChanged,
                &[
                    ("field", DetailArg::Text("size".to_string())),
                    (
                        "before",
                        record
                            .size_bytes
                            .map_or(DetailArg::Unknown, DetailArg::Bytes),
                    ),
                    (
                        "after",
                        rechecked
                            .size_bytes
                            .map_or(DetailArg::Unknown, DetailArg::Bytes),
                    ),
                ],
                record.size_bytes,
                primary_reason(record),
            );
            continue;
        }
        if !runs_at_planned_level(record.safety.status, rechecked.safety.status) {
            push(
                &mut audit,
                target,
                ExecutionResult::Skipped,
                DetailCode::ArtifactNoLongerSafe,
                &[(
                    "reason",
                    primary_reason(&rechecked).map_or(DetailArg::Unknown, |code| {
                        DetailArg::Text(code.as_str().to_string())
                    }),
                )],
                record.size_bytes,
                primary_reason(record),
            );
            continue;
        }

        match move_to_trash(&record.path) {
            Ok(()) => push(
                &mut audit,
                target,
                ExecutionResult::Ok,
                DetailCode::ArtifactTrashed,
                &[],
                record.size_bytes,
                primary_reason(record),
            ),
            Err(error) => push(
                &mut audit,
                target,
                ExecutionResult::Failed,
                DetailCode::ArtifactTrashFailed,
                &[("error", DetailArg::Text(error.to_string()))],
                record.size_bytes,
                primary_reason(record),
            ),
        }
    }

    let log_error = match opts.log_path.as_ref() {
        Some(path) => log::append(path, &audit.history)
            .err()
            .map(|error| format!("{} ({error})", path.display())),
        None => None,
    };
    ExecutionOutcome {
        entries: audit.entries,
        history: audit.history,
        log_error,
    }
}

/// Runs a generated-build-cache plan through the same Trash executor.
///
/// The executor only needs the records and the whitelist; the selected folder is
/// an inventory fact, not a second deletion policy. Converting here keeps the
/// project and system-cache contracts unchanged.
pub fn execute_generated_cache_plan(
    plan: &CleanupPlan,
    snapshot: &GeneratedCacheSnapshot,
    opts: &ExecuteOptions,
) -> ExecutionOutcome {
    let cache_snapshot = CacheSnapshot {
        schema_version: CACHE_SCHEMA_VERSION,
        scanned_at: snapshot.scanned_at.clone(),
        records: snapshot.records.clone(),
        warnings: snapshot.warnings.clone(),
    };
    execute_cache_plan(plan, &cache_snapshot, opts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{backdate_tree, stamp};
    use tempfile::TempDir;

    fn temp() -> TempDir {
        TempDir::new().expect("temp")
    }

    fn source(path: PathBuf, root: PathBuf, kind: &'static str, known: bool) -> CacheSource {
        CacheSource {
            path,
            root,
            kind,
            known,
        }
    }

    fn options() -> CacheScanOptions {
        CacheScanOptions {
            per_dir_timeout: Duration::from_secs(5),
            recent_activity_days: 3,
            whitelist: Vec::new(),
            now: SystemTime::now(),
        }
    }

    #[test]
    fn a_known_cache_is_safe_and_an_unknown_one_needs_review() {
        let root = temp();
        let known = root.path().join("known-cache");
        let unknown = root.path().join("unknown-cache");
        std::fs::create_dir_all(&known).expect("known");
        std::fs::create_dir_all(&unknown).expect("unknown");
        std::fs::write(known.join("blob"), b"cache").expect("blob");
        std::fs::write(unknown.join("blob"), b"cache").expect("blob");
        backdate_tree(&known, 10);
        backdate_tree(&unknown, 10);

        let snapshot = scan_sources(
            &[
                source(
                    known.clone(),
                    root.path().to_path_buf(),
                    "known-cache",
                    true,
                ),
                source(unknown.clone(), root.path().to_path_buf(), "cache", false),
            ],
            &options(),
        );
        let known_record = snapshot
            .records
            .iter()
            .find(|record| record.path == known.display().to_string())
            .expect("known record");
        assert_eq!(known_record.safety.status, SafetyStatus::Safe);
        let unknown_record = snapshot
            .records
            .iter()
            .find(|record| record.path == unknown.display().to_string())
            .expect("unknown record");
        assert_eq!(unknown_record.safety.status, SafetyStatus::Review);
        assert!(unknown_record
            .safety
            .reasons
            .iter()
            .any(|reason| reason.code == ReasonCode::AfUnknownCache));
    }

    #[test]
    fn generated_scan_reports_tagged_build_roots_only() {
        let root = temp();
        let build = root.path().join("batch").join("host-build");
        let plain = root.path().join("batch").join("plain-directory");
        std::fs::create_dir_all(&build).expect("build root");
        std::fs::create_dir_all(&plain).expect("plain directory");
        std::fs::write(build.join("CACHEDIR.TAG"), b"Signature: cache\n").expect("cache tag");
        std::fs::write(build.join("manifest.pif"), b"build state").expect("build output");
        std::fs::write(plain.join("keep.txt"), b"not a cache").expect("plain file");
        backdate_tree(&build, 10);
        backdate_tree(&plain, 10);

        let snapshot = scan_generated_caches(root.path(), &options());
        assert_eq!(snapshot.records.len(), 1, "{:?}", snapshot.records);
        let record = &snapshot.records[0];
        assert_eq!(record.path, build.display().to_string());
        assert_eq!(record.kind, "build-cache");
        assert_eq!(record.safety.status, SafetyStatus::Safe);
        assert!(record
            .safety
            .reasons
            .iter()
            .any(|reason| reason.code == ReasonCode::AfRebuildable));
        assert!(
            snapshot
                .records
                .iter()
                .all(|record| !record.path.contains("plain-directory")),
            "an untagged directory is not a build cache"
        );
    }

    #[test]
    fn a_tagged_root_can_be_scanned_directly() {
        let root = temp();
        let build = root.path().join("host-build");
        std::fs::create_dir_all(&build).expect("build root");
        std::fs::write(build.join("CACHEDIR.TAG"), b"Signature: cache\n").expect("cache tag");
        std::fs::write(build.join("manifest.pif"), b"build state").expect("build output");
        backdate_tree(&build, 10);

        let snapshot = scan_generated_caches(&build, &options());
        assert_eq!(snapshot.root, build.display().to_string());
        assert_eq!(snapshot.records.len(), 1);
        assert_eq!(snapshot.records[0].path, build.display().to_string());
    }

    #[test]
    fn generated_build_cache_does_not_treat_dependency_test_keys_as_secrets() {
        let root = temp();
        let build = root.path().join("host-build");
        let fixture = build
            .join("checkouts")
            .join("Sparkle")
            .join("Tests")
            .join("Resources");
        std::fs::create_dir_all(&fixture).expect("dependency fixture");
        std::fs::write(build.join("CACHEDIR.TAG"), b"Signature: cache\n").expect("cache tag");
        std::fs::write(fixture.join("test-pubkey.pem"), b"publishable test key").expect("test key");
        backdate_tree(&build, 10);

        let snapshot = scan_generated_caches(root.path(), &options());
        assert_eq!(snapshot.records.len(), 1);
        assert_eq!(snapshot.records[0].safety.status, SafetyStatus::Safe);
        assert!(!snapshot.records[0]
            .safety
            .reasons
            .iter()
            .any(|reason| reason.code == ReasonCode::AfCredentials));
    }

    #[test]
    fn a_credential_inside_a_cache_is_review_not_unsafe() {
        // A package cache is downloaded third-party content. The hit is worth
        // showing, but `Unsafe` would leave the reader no way to act on it at all.
        let root = temp();
        let cache = root.path().join("cache");
        std::fs::create_dir_all(&cache).expect("cache");
        std::fs::write(cache.join(".env"), b"SECRET=1").expect("secret");
        backdate_tree(&cache, 10);

        let snapshot = scan_sources(
            &[source(
                cache,
                root.path().to_path_buf(),
                "known-cache",
                true,
            )],
            &options(),
        );
        assert_eq!(snapshot.records[0].safety.status, SafetyStatus::Review);
        assert!(snapshot.records[0]
            .safety
            .reasons
            .iter()
            .any(|reason| reason.code == ReasonCode::AfCredentials));
    }

    #[test]
    fn a_directory_named_credentials_does_not_block_a_cache() {
        // The regression this exists for: a uv cache's sdists tree contained
        // `winsdk/windows/security/credentials`, and the directory name alone made
        // 2.75 GB of cache `Unsafe` — permanently unselectable.
        let root = temp();
        let cache = root.path().join("cache");
        let nested = cache.join("credentials");
        std::fs::create_dir_all(&nested).expect("create nested");
        std::fs::write(nested.join("module.py"), b"source").expect("module");
        // The same tree ships a public CA bundle next to it.
        std::fs::write(
            cache.join("cacert.pem"),
            b"-----BEGIN CERTIFICATE-----\nMIIB\n-----END CERTIFICATE-----\n",
        )
        .expect("bundle");
        backdate_tree(&cache, 10);

        let snapshot = scan_sources(
            &[source(
                cache,
                root.path().to_path_buf(),
                "known-cache",
                true,
            )],
            &options(),
        );
        assert_eq!(snapshot.records[0].safety.status, SafetyStatus::Safe);
    }

    #[test]
    fn a_recent_cache_change_is_review_not_safe() {
        let root = temp();
        let cache = root.path().join("cache");
        std::fs::create_dir_all(&cache).expect("cache");
        std::fs::write(cache.join("blob"), b"cache").expect("blob");
        stamp(&cache.join("blob"), 60);

        let snapshot = scan_sources(
            &[source(
                cache,
                root.path().to_path_buf(),
                "known-cache",
                true,
            )],
            &options(),
        );
        assert_eq!(snapshot.records[0].safety.status, SafetyStatus::Review);
        assert!(snapshot.records[0]
            .safety
            .reasons
            .iter()
            .any(|reason| reason.code == ReasonCode::AfRecentActivity));
    }

    #[test]
    fn the_cache_executor_skips_a_directory_whose_size_changed() {
        let root = temp();
        let cache = root.path().join("cache");
        std::fs::create_dir_all(&cache).expect("cache");
        let file = cache.join("blob");
        std::fs::write(&file, b"one").expect("blob");
        backdate_tree(&cache, 10);
        let snapshot = scan_sources(
            &[source(
                cache.clone(),
                root.path().to_path_buf(),
                "known-cache",
                true,
            )],
            &options(),
        );
        let record = snapshot.records[0].clone();
        let plan = CleanupPlan {
            trash_moves: vec![crate::TrashMoveItem {
                kind: crate::PlanItemKind::Artifact,
                path: record.path.clone(),
                artifact_kind: record.kind.clone(),
                size_bytes: record.size_bytes,
                file_count: record.file_count,
                bundled_with_worktree: None,
            }],
            ..CleanupPlan::default()
        };
        // Grow the directory after the plan was made. The executor must notice
        // and skip it before it reaches the Trash.
        std::fs::write(&file, b"one plus more").expect("grow blob");
        let outcome = execute_cache_plan(
            &plan,
            &snapshot,
            &ExecuteOptions {
                log_path: None,
                ..ExecuteOptions::default()
            },
        );
        assert_eq!(outcome.entries.len(), 1);
        assert_eq!(outcome.entries[0].result, ExecutionResult::Skipped);
        assert_eq!(
            outcome.entries[0].detail_code,
            Some(DetailCode::ArtifactChanged)
        );
        assert!(cache.exists(), "the changed cache must stay in place");
    }

    #[test]
    fn a_cache_snapshot_round_trips_and_carries_its_version() {
        let root = temp();
        let cache = root.path().join("cache");
        std::fs::create_dir_all(&cache).expect("cache");
        std::fs::write(cache.join("blob"), b"x").expect("blob");
        backdate_tree(&cache, 10);
        let snapshot = scan_sources(
            &[source(
                cache,
                root.path().to_path_buf(),
                "known-cache",
                true,
            )],
            &options(),
        );
        let json = serde_json::to_string(&snapshot).expect("serialise");
        let back: CacheSnapshot = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, snapshot);
        assert_eq!(back.schema_version, CACHE_SCHEMA_VERSION);
    }
}
