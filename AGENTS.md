# Worktree Cleaner — Development Rules

## Language

**Everything that ships as open source is written in English**: README, commit
messages, code comments, documentation, identifiers, file names, and user-visible
strings. English is the default and the source locale.

Content that is deliberately kept out of the open-source distribution — local
working notes, private configuration, internal scratch material — may be written
in Chinese. `.workbuddy/` is gitignored and is the only such case today.

User-visible application strings live in the message tables under `src/i18n/`:
`en.ts` is the source locale and every other table is a translation of it, with
**exactly the same keys**. Read them through `src/i18n/index.ts` at render time —
never capture a string in a module-level constant, because the table is rebound
when the language changes. `locale.test.ts` fails if a table's keys differ.

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
- **Every Tauri command that does real work is `async`, and its body runs on
  `spawn_blocking`.** A command without the `async` keyword runs on the main thread,
  which is the one that draws the window: a scan takes over a second (1.16 s for a
  single-worktree project, 1.34 s for five) and an execution spawns Git and moves
  directories, so either one freezes the UI for its whole duration. The work is
  blocking by nature, so it does not belong on an async worker either.

## Where a change belongs

| A change to… | Belongs in… | Governed by |
|---|---|---|
| a reason code's meaning | `crates/worktree-core/src/reason.rs` and the decision table in `docs/PRODUCT_PLAN.md` §5 | DEV_CONTRACT §6 — the documentation change lands in the same commit |
| a deletion guard | `crates/worktree-core/src/path.rs` | D10, product plan §6.4 |
| the worktree verdict | `crates/worktree-core/src/worktree.rs` | D2, D3 |
| the artifact verdict, or the recognised directory names | `crates/worktree-core/src/artifact.rs` | D7, D8 |
| executing a plan | `crates/worktree-core/src/cleanup.rs` | D5, D6, D11, D14, D18 |
| the audit log | `crates/worktree-core/src/log.rs` | D15 |
| a field of a contract type | `crates/worktree-core/src/lib.rs` **and** `src/types.ts` | §3 — `src/architecture.test.ts` fails if they disagree |
| an IPC command | `src-tauri/src/lib.rs`, plus `capabilities/default.json` for its permissions, plus `src/platform.ts` | §2 — the dependency direction is `src/` → IPC → `src-tauri/` → `worktree-core`, never back |
| a user-visible string | `src/i18n/en.ts` **and** every other table under `src/i18n/` | D23 — `locale.test.ts` fails if the key sets differ |
| what a list row shows, page width, or table columns | `src/styles.css`, and the row markup in `src/views/*.vue` (`item-row` for the three list pages, `cell-*` for the two tables left) | §7 decisions 5 and 6 — `pnpm test:layout` checks margins, row fit and name wrapping in a real browser |
| what the UI does with scan or execution results | `src/state.ts`, through `src/platform.ts` | §2 — the front end never re-makes a decision (D9) |

## The gate

`pnpm lint:rust` runs, in order:

1. `cargo fmt --all -- --check`
2. `cargo clippy --workspace --all-targets`

What that fails on is declared in `[workspace.lints]` in the root `Cargo.toml`:

- `clippy::all` and `clippy::pedantic` — **denied**. A new lint at either level fails
  the build.
- the rustc `unused` group — **denied**. A leftover variable or import fails the build
  rather than being scrolled past.
- `unsafe_code` — **denied**. Nothing here needs it: deletion goes through the Trash
  and Git, and the platform calls the `trash` crate makes are its business.
- `clippy::nursery` — a **warning**, on purpose. Those lints are explicitly
  experimental and come and go between Clippy releases; a gate that turns red because
  the toolchain moved is a gate people learn to ignore.

Ten `pedantic` lints are `allow`ed individually, each with its reason written beside
it in `Cargo.toml`: `use_self`, `must_use_candidate`, `cast_possible_truncation`,
`cast_sign_loss`, `doc_markdown`, `missing_errors_doc`, `missing_panics_doc`,
`too_many_lines`, `struct_excessive_bools` and `option_if_let_else`. Each is a
judgement about this codebase rather than a way to quiet the gate — read the reason
before adding another.

## Frozen design

- Product design and decision tables: `docs/PRODUCT_PLAN.md`.
- Development constraints and acceptance criteria: `docs/DEV_CONTRACT.md`.
  Implementation must follow that file.
- Changes touching D1–D23, field contracts, or reason-code semantics require the
  documentation change to land in the same commit.
- UI structure and information architecture follow the frozen design under `src/`.
  The desktop shell reads real scan results over Tauri IPC and writes the audit
  log to disk. `src/mock/` is what the **browser preview** reads, and every page
  that shows it says so; a desktop build never touches it.
- Open items and proposals: `docs/DEV_CONTRACT.md` §7. A proposal is implemented
  only once it is approved there.
