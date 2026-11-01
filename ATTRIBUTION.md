# Sources and attribution

## Bundled typefaces

Both are subset to the Latin range, shipped as WOFF2 under `public/fonts/`, and
accompanied by their licence text in the same directory. Neither is modified, so
the reserved names stay as published; the SIL Open Font License 1.1 permits
redistribution inside another work as long as the licence travels with it, which
is what those two files are for.

- **Inter** — `public/fonts/inter-latin.woff2`
  - Copyright 2016 The Inter Project Authors (https://github.com/rsms/inter)
  - SIL Open Font License 1.1 — see `public/fonts/LICENSE-Inter.txt`
  - Carries the interface text, including Chinese through the system fallback.
- **JetBrains Mono** — `public/fonts/jetbrains-mono-latin.woff2`
  - Copyright 2020 The JetBrains Mono Project Authors (https://github.com/JetBrains/JetBrainsMono)
  - SIL Open Font License 1.1 — see `public/fonts/LICENSE-JetBrainsMono.txt`
  - Carries paths, branch names, commit SHAs and command lines.

The Chinese UI is not bundled: it renders in PingFang SC on macOS, Microsoft YaHei
on Windows, and Hiragino Sans GB where neither exists.

## Tech-stack references

- GetSayAll/remote-mic-app-windows
  - Reference for Vue 3, Tauri 2, the Rust workspace, and platform boundaries.
  - This project is an independent implementation and does not copy its business code.

## Feature research

- Workstreams: agent-agnostic worktree orchestration and status sidebar.
- Agetor: multi-agent, multi-repository, and task-level worktree orchestration.
- codex-clean: a cleanup view for Codex worktrees and build artifacts.
- Git official documentation: git worktree list, git status --porcelain=v2.
