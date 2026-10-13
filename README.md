# Worktree Cleaner

A cross-agent safety audit tool for Git worktrees and build artifacts.

Target platforms: macOS, Windows  
Tech stack: Vue 3, Tauri 2, Rust workspace  
License: GPL-3.0-only

## Goals

- Identify every worktree of a given project using standard Git metadata;
- Support agents such as Codex, Claude Code, WorkBuddy, Cursor, and Aider;
- Show branch, status, last-updated time, and the reason it is safe to delete;
- Detect common artifact directories and show their size, last-modified time, and the reason they are safe to delete;
- Provide verifiable reasons before deletion; read-only by default;
- Use the system Trash or Recycle Bin for deletion and clean up Git worktree metadata.

## Initial state

The current commit contains only the cross-platform project skeleton and the scanner interface. No dependencies were installed, no build cache was generated, and no builds or tests were run.

## Directory structure

    Vue 3 UI
       ↓ Tauri IPC
    Tauri 2 App Host
       ↓
    worktree-core
       ├── Git worktree parsing
       ├── Git status and safety checks
       └── artifact directory scanning

## License

The code is licensed under GPL-3.0-only. See LICENSE for details.
