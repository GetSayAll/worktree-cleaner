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
- Move artifact directories to the system Trash or Recycle Bin; remove worktrees permanently with `git worktree remove` and clean up their Git metadata.

## Status

The scanner, cleanup planner and executor, audit log, disk-hygiene audit, and the Tauri desktop shell are implemented. The browser preview reads sample data from `src/mock/`; the desktop build reads real scan results over Tauri IPC. The local gates are `pnpm lint:rust` for the Rust workspace, `pnpm test` for the front end, and `pnpm build` for the type check and bundle. `docs/DEV_CONTRACT.md` records the acceptance matrix and the open items.

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
