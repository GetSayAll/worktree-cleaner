# Worktree Cleaner — Development Rules

## Language

**English only.** Everything that lands in this repository is written in English:
README, commit messages, code comments, documentation, identifiers, and file
names. This project is an international open-source project, so English is the
single language of the codebase.

User-visible application strings follow the same rule. English is the default and
the source locale. Every string a user can see must live in one place so other
locales can be added later — do not scatter display literals across templates.

## Project rules

- The UI is built with Vue 3.
- The application host is Tauri 2.
- Git parsing, state evaluation, and artifact scanning live in the pure Rust
  crate `worktree-core`.
- Core scanning must not depend on any single agent vendor's directory layout.
- Scanning is read-only by default.
- Worktrees are removed with `git worktree remove`.
- Directories are removed through the macOS Trash or the Windows Recycle Bin.
- Never use permanent-delete commands to clean up a user's directories.
- Every deletion decision must be backed by a verifiable reason.
- macOS and Windows behaviour are verified separately.

## Frozen design

- Product design and decision tables: `docs/PRODUCT_PLAN.md`.
- Development constraints and acceptance criteria: `docs/DEV_CONTRACT.md`.
  Implementation must follow that file.
- Changes touching D1–D23, field contracts, or reason-code semantics require the
  documentation change to land in the same commit.
- UI structure and information architecture follow the frozen prototype under
  `src/`. Once production data is wired in, `src/mock/` leaves the production path.
