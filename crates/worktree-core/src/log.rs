//! The local operation log (D15).
//!
//! Two files, both on this machine only — there is no network call anywhere in
//! this crate, and none may be added.
//!
//! * `operations.log` is the audit trail: one JSON object per line for every
//!   executed action, with time, action, target, reason, result and size.
//! * `activity.log` is what the *user* did: scans, project switches, generated
//!   plans, changed settings. It is a different record with a different reader,
//!   and mixing the two would make both worse.
//!
//! A corrupt line is skipped on read rather than failing the whole history:
//! losing one record must not blind the user to the others.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::{ActivityEvent, HistoryEntry};

/// Directory the log lives in, without the file name.
pub fn log_directory() -> Option<PathBuf> {
    if cfg!(target_os = "macos") {
        return Some(home().join("Library").join("Logs").join("Worktree Cleaner"));
    }
    if cfg!(target_os = "windows") {
        if let Some(local) = std::env::var_os("LOCALAPPDATA").filter(|value| !value.is_empty()) {
            return Some(PathBuf::from(local).join("Worktree Cleaner").join("Logs"));
        }
        return Some(
            home()
                .join("AppData")
                .join("Local")
                .join("Worktree Cleaner")
                .join("Logs"),
        );
    }
    if let Some(state) = std::env::var_os("XDG_STATE_HOME").filter(|value| !value.is_empty()) {
        return Some(PathBuf::from(state).join("worktree-cleaner"));
    }
    Some(home().join(".local").join("state").join("worktree-cleaner"))
}

/// The default log file path, or `None` when no home directory can be determined.
pub fn default_log_path() -> Option<PathBuf> {
    log_directory().map(|directory| directory.join("operations.log"))
}

/// The activity log that goes beside it: what the user did, as opposed to what
/// was deleted.
pub fn default_activity_log_path() -> Option<PathBuf> {
    log_directory().map(|directory| directory.join("activity.log"))
}

fn home() -> PathBuf {
    for variable in ["HOME", "USERPROFILE"] {
        if let Some(value) = std::env::var_os(variable).filter(|value| !value.is_empty()) {
            return PathBuf::from(value);
        }
    }
    PathBuf::new()
}

/// Appends entries to the log, creating the directory when it is missing.
///
/// One line per entry, so a crash mid-write can cost at most the last line.
pub fn append(path: &Path, entries: &[HistoryEntry]) -> std::io::Result<()> {
    if entries.is_empty() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    for entry in entries {
        let line = serde_json::to_string(entry)
            .map_err(|error| std::io::Error::other(format!("serialise log entry: {error}")))?;
        file.write_all(line.as_bytes())?;
        file.write_all(b"\n")?;
    }
    file.flush()
}

/// Reads the log back, oldest first.
///
/// A line that does not parse is skipped: the log is an audit trail, and one
/// damaged record must not hide the rest.
pub fn read(path: &Path) -> Vec<HistoryEntry> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|line| serde_json::from_str::<HistoryEntry>(line).ok())
        .collect()
}

/// Appends user actions, one JSON object per line.
///
/// Best effort at the call site: an unwritable activity log must not stop a scan
/// or a deletion, because it records what happened afterwards rather than
/// deciding what happens now. The result page reports a failed operation log; it
/// does not report a failed activity log, which costs nothing.
pub fn append_activity(path: &Path, events: &[ActivityEvent]) -> std::io::Result<()> {
    if events.is_empty() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    for event in events {
        let line = serde_json::to_string(event)
            .map_err(|error| std::io::Error::other(format!("serialise activity: {error}")))?;
        file.write_all(line.as_bytes())?;
        file.write_all(b"\n")?;
    }
    file.flush()
}

/// Reads the activity log back, oldest first, skipping a damaged line.
pub fn read_activity(path: &Path) -> Vec<ActivityEvent> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|line| serde_json::from_str::<ActivityEvent>(line).ok())
        .collect()
}

/// Empties the log without deleting the file.
pub fn clear(path: &Path) -> std::io::Result<()> {
    if !path.exists() {
        return Ok(());
    }
    OpenOptions::new().write(true).truncate(true).open(path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ActivityAction, DetailCode, ExecutionResult, HistoryAction, ReasonCode};
    use std::collections::BTreeMap;

    fn entry(target: &str, size: Option<u64>) -> HistoryEntry {
        HistoryEntry {
            at: "2026-01-01T00:00:00Z".to_string(),
            action: HistoryAction::TrashMove,
            target: target.to_string(),
            reason_code: Some(ReasonCode::AfRebuildable),
            reason: None,
            result: ExecutionResult::Ok,
            size_bytes: size,
            detail_code: Some(DetailCode::ArtifactTrashed),
            detail_args: BTreeMap::new(),
            detail: None,
        }
    }

    fn temp_log(name: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let path = dir.path().join(name);
        (dir, path)
    }

    #[test]
    fn append_then_read_round_trips_and_keeps_order() {
        let (_guard, path) = temp_log("round-trip.log");
        let written = vec![entry("/repo/dist", Some(48)), entry("/repo/target", None)];
        append(&path, &written).expect("append");
        let read_back = read(&path);
        assert_eq!(read_back, written, "the log must round trip unchanged");
        // An unmeasurable size survives as null rather than collapsing to 0.
        let text = std::fs::read_to_string(&path).expect("read");
        assert!(text.contains("\"size_bytes\":null"), "{text}");
    }

    #[test]
    fn append_adds_to_what_is_already_there() {
        let (_guard, path) = temp_log("append-twice.log");
        append(&path, &[entry("/repo/a", Some(1))]).expect("first append");
        append(&path, &[entry("/repo/b", Some(2))]).expect("second append");
        let read_back = read(&path);
        assert_eq!(read_back.len(), 2);
        assert_eq!(read_back[0].target, "/repo/a");
        assert_eq!(read_back[1].target, "/repo/b");
    }

    #[test]
    fn read_skips_a_corrupt_line() {
        let (_guard, path) = temp_log("corrupt.log");
        append(&path, &[entry("/repo/a", Some(1))]).expect("append");
        let mut file = OpenOptions::new().append(true).open(&path).expect("open");
        file.write_all(b"this is not json\n").expect("write");
        append(&path, &[entry("/repo/b", Some(2))]).expect("append");
        let read_back = read(&path);
        assert_eq!(read_back.len(), 2, "the damaged line is skipped, not fatal");
    }

    #[test]
    fn read_of_a_missing_file_is_empty_not_an_error() {
        let (_guard, path) = temp_log("missing.log");
        assert!(read(&path).is_empty());
    }

    #[test]
    fn clear_empties_the_log() {
        let (_guard, path) = temp_log("clear.log");
        append(&path, &[entry("/repo/a", Some(1))]).expect("append");
        clear(&path).expect("clear");
        assert!(read(&path).is_empty());
        // The file itself stays, so the next append does not have to recreate it.
        assert!(path.exists());
    }

    #[test]
    fn clear_of_a_missing_file_is_fine() {
        let (_guard, path) = temp_log("clear-missing.log");
        clear(&path).expect("clear");
    }

    #[test]
    fn the_default_log_path_is_absolute_and_names_the_file() {
        let Some(path) = default_log_path() else {
            // No home directory in this environment: nothing to assert.
            return;
        };
        assert!(path.is_absolute(), "{path:?}");
        assert_eq!(
            path.file_name().and_then(|name| name.to_str()),
            Some("operations.log")
        );
    }

    #[test]
    fn the_activity_log_lives_beside_the_operation_log() {
        let (Some(operations), Some(activity)) = (default_log_path(), default_activity_log_path())
        else {
            return;
        };
        assert_eq!(
            operations.parent(),
            activity.parent(),
            "one directory, two files"
        );
        assert_ne!(
            operations.file_name(),
            activity.file_name(),
            "the deletion trail and the activity trail are different files"
        );
    }

    #[test]
    fn activity_round_trips_and_skips_a_damaged_line() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let path = dir.path().join("activity.log");
        let event = |action: ActivityAction| ActivityEvent {
            at: "2026-01-01T00:00:00Z".to_string(),
            action,
            target: "/repo".to_string(),
            args: std::collections::BTreeMap::new(),
        };

        append_activity(&path, &[event(ActivityAction::ScanStarted)]).expect("first");
        append_activity(&path, &[event(ActivityAction::PlanGenerated)]).expect("second");
        let read_back = read_activity(&path);
        assert_eq!(read_back.len(), 2);
        assert_eq!(read_back[0].action, ActivityAction::ScanStarted);

        let mut file = OpenOptions::new().append(true).open(&path).expect("open");
        file.write_all(b"this is not json\n").expect("write");
        let after = read_activity(&path);
        assert_eq!(
            after.len(),
            2,
            "a damaged line is skipped, and the records around it survive"
        );
    }

    #[test]
    fn every_activity_action_has_a_stable_name() {
        // The front end keys its copy by these strings, so they are part of the
        // contract rather than a display detail.
        let all = [
            ActivityAction::ScanStarted,
            ActivityAction::ScanFinished,
            ActivityAction::ScanFailed,
            ActivityAction::ScanCancelled,
            ActivityAction::ProjectSwitched,
            ActivityAction::PlanGenerated,
            ActivityAction::PlanExported,
            ActivityAction::SettingsChanged,
            ActivityAction::CleanupConfirmed,
            ActivityAction::TrashOpened,
        ];
        let names: Vec<&str> = all.iter().map(|action| action.as_str()).collect();
        let unique: std::collections::BTreeSet<&str> = names.iter().copied().collect();
        assert_eq!(unique.len(), names.len(), "{names:?}");
        for name in names {
            assert!(name.contains('.'), "{name} should read family.action");
        }
    }
}
