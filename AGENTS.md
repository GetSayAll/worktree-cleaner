# Worktree Cleaner — Development Rules

## Language

**Everything that ships as open source is written in English**: README, commit
messages, code comments, documentation, identifiers, file names, and user-visible
strings. English is the default and the source locale.

Content that is deliberately kept out of the open-source distribution — local
working notes, private configuration, internal scratch material — may be written
in Chinese. `.workbuddy/` is gitignored and is the only such case today.

User-visible application strings live in the message table under `src/i18n/` and
are read through `src/i18n/index.ts`. Never scatter display literals across
templates; add them to the table so other locales can be added later.

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
