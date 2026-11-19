// Without this, a Rust binary is linked as a console program on Windows, and the system
// has to give a console program a console: Windows 11 draws that console in Windows
// Terminal, so a release build opened a terminal window on every launch. Debug builds
// keep the console, which is where `pnpm tauri dev` writes its logs.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    worktree_cleaner_lib::run();
}
