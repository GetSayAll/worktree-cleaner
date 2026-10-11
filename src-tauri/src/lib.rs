use worktree_core::ProjectSnapshot;

#[tauri::command]
fn scan_project(path: String) -> Result<ProjectSnapshot, String> {
    worktree_core::scan_project(&path).map_err(|error| error.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![scan_project])
        .run(tauri::generate_context!())
        .expect("error while running Worktree Cleaner");
}
