//! The desktop shell: the only place where the front end meets the core.
//!
//! Every command here is a thin adapter. The scanning and the executing live in
//! `worktree-core`, and the shell's job is to translate the wire shape the UI
//! speaks into the core's own types — no decisions are made on this side, and
//! nothing is decided twice.
//!
//! **Every command that does real work is `async`, and its work runs on
//! `spawn_blocking`.** A command without the `async` keyword runs on the main
//! thread, and the main thread is the one that draws the window: a scan of a real
//! repository takes over a second — measured at 1.16 s for a single-worktree
//! project on this machine — and an execution spawns Git and moves directories, so
//! either one freezes the UI for its whole duration. The work itself is blocking by
//! nature (it starts processes and walks the filesystem), so it belongs on the
//! blocking pool rather than on an async worker.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use serde::Deserialize;
use worktree_core::cleanup::{execute_plan as core_execute, ExecuteOptions, ExecutionOutcome};
use worktree_core::log;
use worktree_core::{
    scan_project as core_scan, ActivityAction, ActivityEvent, CleanupPlan, DetailArg, HistoryEntry,
    ProjectSnapshot, ScanOptions,
};

/// Runs one scan.
///
/// `options` carries milliseconds rather than Rust durations, so the front end
/// never has to know how `Duration` encodes.
#[tauri::command]
async fn scan_project(path: String, options: ScanOptions) -> Result<ProjectSnapshot, String> {
    tauri::async_runtime::spawn_blocking(move || {
        core_scan(&path, &options).map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("the scan task did not finish: {error}"))?
}

/// Everything the executor needs, in one request.
///
/// `ExecuteOptions` in the core carries a `Duration`, a `SystemTime`, and a
/// `PathBuf`; none of those belong on the wire, so the shell rebuilds them here
/// and takes the rest from the core's defaults.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct ExecuteRequest {
    /// The plan the user confirmed, exactly as the UI built it.
    plan: CleanupPlan,
    /// The scan result the plan was built from: the executor re-checks against
    /// it and refuses a plan that arrives without one (D11).
    snapshot: ProjectSnapshot,
    recent_activity_days: u32,
    untracked_as_unsafe: bool,
    whitelist: Vec<String>,
    /// Budget for one directory size measurement during the re-check, in
    /// milliseconds. It travels from the settings page rather than being taken from
    /// `ExecuteOptions::default()`: the scan and the re-check measure the same
    /// directories, so honouring one setting and ignoring it in the other would make
    /// the same knob mean two different things in one run.
    per_dir_timeout_ms: u64,
    /// Budget for one Git command during the re-check, in milliseconds.
    git_timeout_ms: u64,
    /// Where to append the audit log; `null` disables logging.
    log_path: Option<String>,
}

/// Runs a confirmed plan.
#[tauri::command]
async fn execute_plan(request: ExecuteRequest) -> Result<ExecutionOutcome, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let options = ExecuteOptions {
            recent_activity_days: request.recent_activity_days,
            untracked_as_unsafe: request.untracked_as_unsafe,
            whitelist: request.whitelist,
            // `max(1)`: a zero budget would make every measurement time out before it
            // starts, which reads as "unmeasurable" rather than as the setting the
            // user chose. The same guard the scan path applies.
            per_dir_timeout: Duration::from_millis(request.per_dir_timeout_ms.max(1)),
            git_timeout: Duration::from_millis(request.git_timeout_ms.max(1)),
            log_path: request.log_path.map(PathBuf::from),
            ..ExecuteOptions::default()
        };
        core_execute(&request.plan, &request.snapshot, &options)
    })
    .await
    .map_err(|error| format!("the execution task did not finish: {error}"))
}

/// Where the operation log lives on this machine, so the settings page can show
/// the path it is actually writing to instead of a guess.
#[tauri::command]
async fn log_path() -> Option<String> {
    log::default_log_path().map(|path| path.display().to_string())
}

/// The audit trail, newest last.
#[tauri::command]
async fn read_history() -> Result<Vec<HistoryEntry>, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let Some(path) = log::default_log_path() else {
            return Vec::new();
        };
        log::read(&path)
    })
    .await
    .map_err(|error| format!("the log read did not finish: {error}"))
}

/// Empties the operation log.
///
/// The file itself stays: "the log is empty" and "there is no log" are
/// different answers, and only one of them means the app has never run.
#[tauri::command]
async fn clear_log() -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(|| {
        let Some(path) = log::default_log_path() else {
            return Ok(());
        };
        log::clear(&path).map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("the log clear did not finish: {error}"))?
}

/// Appends one user action to the activity log.
///
/// The front end never writes a file itself: the path lives in the core and the
/// "stays on this machine" guarantee is enforced there. A failure is reported but
/// never blocks — an activity log records what happened afterwards and decides
/// nothing, so it must not be able to fail a scan or a deletion.
#[tauri::command]
async fn log_activity(
    action: ActivityAction,
    target: String,
    args: BTreeMap<String, DetailArg>,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let Some(path) = log::default_activity_log_path() else {
            return Ok(());
        };
        let event = ActivityEvent {
            at: worktree_core::timefmt::format_rfc3339(std::time::SystemTime::now()),
            action,
            target,
            args,
        };
        log::append_activity(&path, &[event]).map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| format!("the activity write did not finish: {error}"))?
}

/// Reads the activity log back, oldest first.
#[tauri::command]
async fn read_activity() -> Result<Vec<ActivityEvent>, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let Some(path) = log::default_activity_log_path() else {
            return Vec::new();
        };
        log::read_activity(&path)
    })
    .await
    .map_err(|error| format!("the activity read did not finish: {error}"))
}

/// The accent colour the operating system is using, as `#rrggbb`.
///
/// The stylesheet cannot ask for this itself. Both keywords that look like they
/// answer it — `-apple-system-control-accent` and `AccentColor` — parse in WebKit
/// and both resolve to the default blue even when the user has chosen another
/// accent: measured in a bare WKWebView on a machine where `NSColor.controlAccentColor`
/// reports `#F7821B`. Chromium does honour `AccentColor` for the Windows accent, so
/// the stylesheet keeps that as its own fallback and this command is what makes
/// macOS follow the setting.
///
/// The value is read through `osascript` rather than `NSColor` directly: reaching
/// AppKit from Rust means Objective-C message sends, and this crate denies
/// `unsafe_code` (see the workspace lints). One process, about 120 ms, on the
/// blocking pool. `None` means the caller keeps its own accent.
#[tauri::command]
async fn system_accent() -> Option<String> {
    tauri::async_runtime::spawn_blocking(accent_from_system)
        .await
        .ok()
        .flatten()
}

#[cfg(target_os = "macos")]
fn accent_from_system() -> Option<String> {
    const SCRIPT: &str = r#"ObjC.import("AppKit"); var c = $.NSColor.controlAccentColor; var s = c.colorUsingColorSpace($.NSColorSpace.sRGBColorSpace); [Math.round(s.redComponent * 255), Math.round(s.greenComponent * 255), Math.round(s.blueComponent * 255)].join(",")"#;
    let output = std::process::Command::new("osascript")
        .args(["-l", "JavaScript", "-e", SCRIPT])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?;
    let channels = text
        .trim()
        .split(',')
        .map(|channel| channel.trim().parse::<u8>().ok())
        .collect::<Option<Vec<u8>>>()?;
    match channels.as_slice() {
        [red, green, blue] => Some(format!("#{red:02X}{green:02X}{blue:02X}")),
        _ => None,
    }
}

/// Windows takes its accent through the stylesheet's `AccentColor`, so there is
/// nothing to ask the host for. Kept as a named function rather than an inline
/// `None` so the two platforms read the same at the call site.
#[cfg(not(target_os = "macos"))]
fn accent_from_system() -> Option<String> {
    None
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            scan_project,
            execute_plan,
            log_path,
            read_history,
            clear_log,
            log_activity,
            read_activity,
            system_accent
        ])
        .run(tauri::generate_context!())
        .expect("error while running Worktree Cleaner");
}
