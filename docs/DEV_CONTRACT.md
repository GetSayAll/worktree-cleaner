# Development Contract

This file is the single source of constraints for development. `docs/PRODUCT_PLAN.md`
and the frozen UI prototype under `src/` are the design source of truth. Any
deviation must change this file or the product plan first, and only then the code.

---

## 0. Baseline

The following content was frozen by these commits and counts as "design agreed":

| Category | Files | Status |
|---|---|---|
| Product design | `docs/PRODUCT_PLAN.md` | Frozen |
| Development constraints | `docs/DEV_CONTRACT.md` (this file) | Frozen |
| Data contract | `src/types.ts` | Frozen (field-for-field with the Rust structs) |
| UI prototype | `src/App.vue`, `src/views/`, `src/components/`, `src/state.ts`, `src/format.ts`, `src/styles.css`, `src/mock/snapshot.ts` | Frozen (interaction and information structure) |
| Prototype build | `scripts/build-preview.mjs` | Frozen |
| Not yet covered | `crates/` (partially), `src-tauri/` | `worktree-core` is in progress; `src-tauri` is a running shell (window, icons, one command) |

Frozen means **the UI structure and the decision semantics are not to be changed
casually**. The freedom left at the implementation level is described in section 6.

**Baseline commits**: `a45c0ea` (product design and UI prototype), `408db1d`
(baseline record). Any later change to the files above must state its
justification in the commit message (change control is in section 6).

The five local commits made before the language convention was adopted were
rewritten so that every commit message is English. Their content is unchanged —
the tree hash is `f4928e0c1cb52b73eab087ada3232e0679feae4c` both before and after
the rewrite. The earlier hashes map as bdfc990 → a45c0ea, a8e2edc → 408db1d,
3fe4d9f → db2a517, a215f63 → 5f6f87c, 29a982c → 6f39c30.

**Form-factor change record (2026-10-11)**: the delivery form was extended from
"a Tauri desktop application" to "Form A read-only report → Form B local daemon +
browser → Form C Tauri desktop shell", per section 10 of the product plan. The
same change added D17–D22, reworked the P2–P6 stages (section 5) and settled the
dependency-fix question that section 7 previously listed as open.

**Language rule (2026-10-11)**: the project is an international open-source
project, so everything that ships as open source is English. See D23. Content that
is deliberately kept out of the open-source distribution may be written in
Chinese; the local notes under `.workbuddy/` are gitignored and are the only such
case today.

**Language rule extension (2026-10-13)**: UI chrome may additionally ship as
locale tables under `src/i18n/`. `en.ts` remains the source locale and the
default; every further locale is a translated table whose keys must match `en.ts`
exactly (asserted by a test). Scanner-produced reason `text` and `evidence` are
data, not UI chrome, and stay English. See D23 and section 3.

---

## 1. Immutable design decisions

Numbered D1–D23. Each one is a hard constraint; violating it is an implementation
bug. D17–D23 are form-factor and repository-wide constraints.

| ID | Decision | Why | How to verify |
|---|---|---|---|
| D1 | Scanning is read-only: it does not write to the repository or to target directories. The only write is the local operation log | Users must be able to scan first and decide later | `git status` is unchanged after a scan; target directory mtimes are unchanged |
| D2 | The level of a record is decided by the **most severe** entry in `reasons`. No independent scoring | Every level must trace back to concrete evidence | Unit test: the presence of any Unsafe reason forces the whole record to Unsafe |
| D3 | Failures downgrade: a timeout, a permission error, or a non-zero command exit produces `Review`, **never `Safe`** | Without evidence you cannot call something safe | Inject a timeout and assert the level is Review and carries `cannot_verify` |
| D4 | The main worktree is never deletable, even when clean | Deleting the main worktree deletes the repository | Assert the main worktree carries `wt.main` and is Unsafe |
| D5 | Directories are removed only through the system Trash. **No permanent delete, no bypass switch** | Deleting inside a user's directories must be recoverable | On macOS, a deleted item can be found in the Trash and put back |
| D6 | Worktrees are removed only with `git worktree remove` / `git worktree prune`. Raw `rm -rf` on a worktree directory is forbidden | Git metadata must stay consistent | `git worktree list` shows no leftovers after removal; no bare-delete call exists in the code paths |
| D7 | An artifact directory name is only a candidate hint. The verdict must rest on: whether Git tracks it, whether it holds authored content, its activity, and whether it is a symlink escaping the project | "It is called build, so delete it" destroys source code | Counter-example test: a `build/` directory containing tracked files must be Unsafe |
| D8 | An artifact directory must never be treated as a project root and walked into | Treating `node_modules` as a project root deletes package-internal `dist/`, leaving a tree that looks installed but is corrupt and needs a network restore | Counter-example test: a `node_modules` containing `package.json` must not be recognised as a project root |
| D9 | Every decision carries four elements: `code` + `severity` + `text` + `evidence` | "A verifiable reason" is the core promise of this product | Struct fields are not optional; every reason in the UI can be expanded to show its evidence |
| D10 | Before deletion, the path must pass the eight checks **in order**, and verification may only tighten — a resolved path never grants permission the literal path lacked. Protection is decided on a case-folded comparison key, so a protected location is refused in **any** spelling of it (macOS ignores case, and `realpath` does not correct it), and on Windows a component Win32 would read as a different name is refused outright | Guards against symlink escape, spelling tricks, and empty-variable collapse | The whole fuzz corpus is rejected — including the case variants and both canonical spellings of every symlinked protected location; the ancestor-symlink case is rejected; and the precedence of the checks is pinned by a test that applies two rules at once and asserts which one is reported |
| D11 | Between plan generation and execution, the target identity (path + size + mtime) and its activity are re-checked | Guards against "safe when planned, changed when executed" | Modify the target after planning and assert the item is skipped |
| D12 | Default selection: `Safe` is selected, `Review` is not, `Unsafe` checkboxes are disabled | Defaults must be conservative | Both the UI and the execution layer have tests for this |
| D13 | High-risk confirmation: 5 or more items, or any `--force`, or more than 10 GB freed in one run requires an explicit manual acknowledgement | Large actions need a human pause | The plan object exposes `ack_reasons`; the execute button stays disabled until acknowledged |
| D14 | Execution is serial. A failure on one item does not affect the others. No concurrent deletion | Failures must stay attributable; no confusing partial success | Single-failure test: the remaining items still complete |
| D15 | The log is written locally only, never sent anywhere, and records time / action / target / reason / result / size | After-the-fact traceability is the basis for trust | Assert on log contents; no network call exists anywhere in the code |
| D16 | macOS and Windows are verified **separately** on real machines | Trash and Recycle Bin are two different mechanisms; verifying one does not verify the other | One verification record per platform |
| D17 | A read-only form (the Form A web page, and any display-only artifact) must not contain any deletion or write capability — not even an entry point that asks a backend to delete | The responsibility of each form must be unambiguous: read-only means read-only | A code-scan assertion over the build output: no deletion-related call is present |
| D18 | Deletion and writing may only be performed by a local process, and only when driven by a plan object. No form may let a web page trigger a filesystem write directly | Execution authority must live locally and stay reviewable | Form A has no write path; Form B's `execute` accepts only a plan object |
| D19 | Form B's daemon must: bind to `127.0.0.1` only; generate a random token on every start; strictly validate `Origin` and `Host` against DNS rebinding; never use a wildcard CORS policy; and require in-UI confirmation in addition to the token for destructive endpoints | A local port is reachable from the browser, so a malicious page can send requests | `lsof` shows no externally reachable listener; requests without a token and cross-origin requests are rejected; execution without confirmation is refused |
| D20 | `snapshot.json` must carry `schema_version` and is the only contract between the CLI and the web page. On a higher version the page must say the viewer needs an upgrade; it must never guess at fields | Once published, an interface cannot assume both ends upgrade together | Round-trip test on the contract; feed a higher version and assert a clear error |
| D21 | No form may weaken the existing constraints: D5 (Trash only) and D6 (`git worktree remove` only) hold in every form | Changing the form is not a reason to relax safety | One verification record per form, each covering D5 and D6 |
| D22 | The report web page must open with no network and no server (assets inlined) and must not introduce a runtime backend dependency | The availability of a read-only form should not depend on the environment | Open it offline over `file://` and complete a full review |
| D23 | **English for everything that ships as open source.** Code, comments, identifiers, documentation, commit messages, file names, and user-visible strings in the repository are English, and English is the default and the source locale. The one exception is UI locale data under `src/i18n/`: the English source table plus one translated table per additional locale, keys identical to the source table, file names and keys in English. Content deliberately kept out of the open-source distribution may be written in Chinese | This is an international open-source project; a mixed-language repository is not reviewable by the community it targets | A scan of tracked files finds no non-English natural language outside `src/i18n/`; every commit message reads in English; a test fails when any locale table's keys differ from `en.ts` |

---

## 2. Architecture constraints

All three forms share one core; only the outermost layer differs. Dependencies are
single-direction and must never be reversed:

```
Form A: static web page ────── reads snapshot.json (read-only, no execution)
Form B: browser ── HTTP(127.0.0.1 + token) ──┐
Form C: Tauri desktop shell ── IPC ──────────┤
                                            ↓
       worktree-core (pure Rust: Git parsing, state evaluation, artifact scan, path verification)
                                            ↓
                                     local filesystem / system Trash
```

- `worktree-core` does not depend on Tauri, and it does not depend on any single
  agent vendor's directory layout as a precondition. Recognising an agent only
  labels a path; failing to recognise one is not an error and must not be guessed.
- Form B's daemon and Form C's Tauri host only forward calls and adapt to the
  platform. They must contain no decision logic and only hand a plan object to the
  execution module.
- `worktree-core` performs **read-only scanning only**. Execution lives in a
  separate module and is driven exclusively by a plan object; scanning must never
  delete anything as a side effect.
- All Git calls are collected in a `git` submodule of `worktree-core`, and the
  internal `.git` file format is never parsed (use command output such as
  `git worktree list --porcelain`).
- The front end never re-implements decision logic: the UI only displays, filters,
  and selects. Levels and reasons always come from Rust output.
- Contract direction: Rust `serde` output is the front-end contract. Fields are
  `snake_case`, and unit enum variants serialise to `"Safe" | "Review" | "Unsafe"`.

---

## 3. Naming and contract rules

- Reason codes use the `wt.*` / `af.*` form (worktree / artifact). They are
  **additive only**: a semantic change requires a new code, and retired codes are
  marked deprecated but never reused.
- `src/types.ts` matches the Rust structs field for field. A change on either side
  must be mirrored on the other side within the same commit, and
  `src/architecture.test.ts` now enforces it mechanically: it reads the field names
  out of both sources and fails when they disagree, for `ProjectSnapshot`,
  `WorktreeRecord`, `ArtifactRecord`, `CleanupPlan`, `ResultEntry`, `HistoryEntry`,
  `ScanOptions`, `ExecutionOutcome`, `Reason` and `SafetyAssessment`, and compares
  the `wt.*` / `af.*` code lists the same way. The Rust side additionally asserts a
  serialise/deserialise round trip on a sample.
- Optional numbers use `Option<u64>` (front end: `number | null`):
  **"cannot be measured" must be distinguishable from "0"**, and the UI shows
  `unknown` and `0 B` respectively.
- Every reason must carry a non-empty `text` and a non-empty `evidence`. The
  constructors assert this in test builds.
- Internal naming is free, but field names that appear externally, reason codes,
  and command text (such as `git worktree remove --force -- "<path>"`) are part of
  the contract and must not change casually. A path reaches Git after a `--`
  separator, and the command shown in the plan is the one the executor builds: the
  executor re-derives it from its own re-check rather than parsing that string.
- Language: identifiers, comments, documentation, commit messages, file names, and
  user-visible strings are all English (D23). The only exception is UI locale data
  under `src/i18n/`: table keys and file names stay English, the English table is
  the source, and a new locale is a translated copy of it. Commit messages are
  written as `type(scope): short imperative summary`, followed by a body explaining
  what changed and why.
- User-visible strings live in `src/i18n/en.ts` and are read through
  `src/i18n/index.ts`, which is the single swap point for adding a locale. Domain
  text produced by the scanner (a reason's `text` and `evidence`) is data rather
  than UI chrome and stays with the record.
- **"Released space" counts only items whose result is `Ok`.** An artifact that left
  together with its worktree was not a separate move, a skipped or failed item freed
  nothing, and an item whose size was never measurable contributes to
  `freed_unknown` rather than to `freed` — a figure that cannot be stood behind is
  worse than a smaller figure. Every total on screen is an estimate and is presented
  as one; the volume's free space is not read before and after, so no figure claims
  to be exact.
- The locale registry is reactive: `index.ts` exports a reactive `messages`
  object plus `applyLocale(choice)`, which resolves `system` through
  `navigator.languages`, replaces the top-level keys, and updates
  `document.documentElement.lang`. The Settings choice (`system` / `en` /
  `zh-CN`) is persisted and re-detected on the `languagechange` event while
  `system` is selected. Anything that captured copy at load time (nav labels,
  status labels, demo data) must be rebuilt or read at render time instead.
- File names are English, lowercase, and use `kebab-case` for docs and scripts.
  Rust and TypeScript files follow their ecosystem conventions (`snake_case` for
  Rust modules, `PascalCase.vue` for components).

---

## 4. Testing requirements

- **Every reason code that is still emitted has at least one positive and one negative
  case.** The list of codes is in the decision tables in section 5 of the product plan.
  A code retained only so that audit logs written by an earlier version still render is
  exempt from the positive case, but must carry a test proving it is no longer produced.
  Today that is `wt.stash` (see the note under product plan §5.1).
- The path validator must have a fuzz test (cargo-fuzz or proptest). Invariant:
  **any path that passes verification must be absolute, free of NUL bytes, and free
  of `..` path components.**
- The following counter-examples are mandatory; none may be skipped:
  - a `build/` directory containing Git-tracked files;
  - `node_modules` mistaken for a project root;
  - a path that collapses into its parent because a variable was empty;
  - an **ancestor** of the path being a symlink that points somewhere dangerous;
  - artifact size measurement timing out, which must display `unknown` rather than 0
    and must **leave the verdict where it was** — the verdict never depended on the
    size (see decision 5 in section 7);
  - the target being replaced, or becoming active, after the plan was generated.
- A protected location must be refused in **every spelling that reaches it**: any
  case (`/SYSTEM/Library/CoreServices`), and both the symlink spelling and the
  canonical spelling of a location whose parent is a symlink (`/var/db` and
  `/private/var/db`). The order of the eight checks is pinned as well — a case that
  applies two rules at once asserts which rejection is reported, because the order
  decides what the user is told.
- Front end: any UI change must leave `vue-tsc --noEmit` at zero errors and the
  build passing.
- Front end layout is checked in a **real browser**, not in a unit test, because
  where a line breaks and how wide a row is are layout results: `pnpm test:theme`
  drives the appearance modes and `pnpm test:layout` drives the page grid — every
  page's margins against the worktree page's, and on the list pages that no row
  spills, that no name wraps and no two rows overlap, in both locales, at the app's
  default window size. A stylesheet assertion cannot know how wide "Must not
  delete" is in the machine's font, and happy-dom does not lay out a row at all.
- Rust: `pnpm lint:rust` must pass. It runs `cargo fmt --all -- --check` and
  `cargo clippy --workspace --all-targets`, and what that fails on is declared in
  `[workspace.lints]` in the root `Cargo.toml`: `clippy::all`, `clippy::pedantic`,
  the rustc `unused` group and `unsafe_code` are **denied**. `clippy::nursery` stays a
  warning on purpose — those lints are experimental and change between Clippy
  releases. The ten `pedantic` lints that remain allowed are each listed with their
  reason in `Cargo.toml` and in `AGENTS.md`; adding an exception without one is the
  thing to look for in review.
- Platform: macOS verifies the Trash path on a real machine before every release;
  Windows verifies the Recycle Bin path separately. Both results are recorded
  separately.

---

## 5. Deliverables and acceptance criteria

"Done" for each stage means the checks below pass. They are executable checks, not
subjective judgement.

### P0 · Path validator and reason model

**Deliverables**: the path verification module in `worktree-core`, the `Reason` /
`SafetyAssessment` / `WorktreeRecord` structs, the worktree check implementation,
unit tests, and a fuzz target.

**Acceptance criteria**

1. `cargo test -p worktree-core` is green;
2. every `wt.*` reason code in the decision tables has a test case, and every reason
   carries non-empty `evidence`;
3. a fuzz corpus of at least 60 adversarial paths (covering `..`, ancestor symlinks,
   control characters, system roots, empty paths) is fully rejected;
4. a case that injects a timeout or a permission error asserts the level is `Review`
   and carries `cannot_verify`;
5. `cargo clippy -- -D warnings` passes.

**Status (2026-10-12): met.** 98 tests pass (89 unit, 5 path-corpus integration, 4
against a real repository); `cargo clippy --all-targets -- -D warnings` and
`cargo fmt --check` are clean. The worktree decision table lives in
`crates/worktree-core/src/worktree.rs`, its Git transport in `git.rs`, and the
real-repository checks in `tests/worktree_check_real_repo.rs`.

### P1 · Artifact scan and boundary detection

**Deliverables**: artifact type list, ecosystem attribution, size and file-count
measurement, Git tracking status, boundary detection.

**Acceptance criteria**

1. a `build/` directory containing tracked files is `Unsafe` with `af.git_tracked`,
   and its evidence names the number of matches;
2. `node_modules` is not recognised as a project root (the D8 counter-example passes);
3. a directory containing a nested `.git` or credential files is `Unsafe`;
4. an escaping symlink is `Unsafe` with `af.symlink_outside`, and its evidence names
   the target path;
5. a size-measurement timeout produces `Review`, and the UI shows `unknown`, not `0 B`;
6. every `af.*` reason code has a test case.

### P2 · CLI skeleton and the snapshot JSON contract

**Deliverables**: the `worktree-cleaner scan --json <path>` subcommand, the
`schema_version` field, serialisation, and the CLI argument and exit-code conventions.

**Acceptance criteria**

1. output matches the contract in product plan section 10.5 — complete fields,
   correct types, `schema_version` present;
2. `git status` and target directory mtimes are unchanged by a scan (D1);
3. "not a Git directory", "directory does not exist", and "permission denied" each
   produce a distinct exit code and a clear message, and never a silent success;
4. output fields match `ProjectSnapshot` in `src/types.ts` field for field;
5. degraded scans (timeouts, permission errors) appear honestly in `warnings` and in
   the `reasons` of the affected records.

### P3 · Read-only report web page (Form A)

**Deliverables**: a single-file web page (inlined assets), an entry point for
dropping in a snapshot JSON, the three-level display and filtering, and export of the
cleanup plan as JSON.

**Acceptance criteria**

1. opened offline over `file://`, a full review can be completed (D22);
2. the three level counts, reason texts, and `evidence` shown by the page match the
   JSON item for item (compare 10 randomly chosen records);
3. the page contains no delete button, no deletion call, and no filesystem write path
   (D17, asserted by scanning the build output);
4. a `schema_version` higher than the page supports produces a clear upgrade message
   and no field guessing (D20);
5. the exported cleanup plan JSON matches the plan object and is accepted by the CLI
   directly;
6. `src/mock/` is no longer referenced by production code, `vue-tsc --noEmit` is at
   zero errors, and the build passes.

### P4 · Local cleanup execution and logging (the execution side of Form A)

**Deliverables**: the `clean --plan <file>` command, the execution module, the
three-tier Trash fallback, the `git worktree remove` / `prune` calls, and operation
log writing.

**Acceptance criteria**

1. macOS: after deletion the item can be found in the Trash and put back (D5);
2. after `git worktree remove`, `git worktree list` shows no leftovers and `prune`
   runs as a follow-up (D6);
3. a worktree with untracked files **refuses to execute** until confirmed; after
   confirmation the command is `git worktree remove --force`, and the untracked file
   list was already shown line by line in the plan;
4. a whitelist hit blocks the item and keeps it out of execution;
5. a target whose identity or activity changed after planning is skipped with a
   stated reason (D11);
6. a failure on one item does not affect the others (D14);
7. the log file contains time / action / target / reason / result / size, and no
   network call exists in the code (D15);
8. Windows: the Recycle Bin path is verified separately and recorded separately (D16).

### P5 · Local daemon and browser UI (Form B)

**Deliverables**: the local HTTP service (`scan` / `plan` / `execute`), token
authentication and `Origin` / `Host` validation, and the web page's HTTP data source.

**Acceptance criteria**

1. `lsof` shows a listener on `127.0.0.1` only, with no externally reachable listener
   (D19);
2. requests without a token, with a wrong token, and with a cross-origin `Origin` are
   all rejected;
3. `execute` accepts only a plan object and refuses to run before the second
   confirmation (D18, D13);
4. the full chain runs in a browser: scan → select → plan → execute → result → history;
5. `worktree-core` is unchanged in this stage (the reuse check);
6. the Form A read-only page and this form share one component set and differ only in
   their data source.

### P6 · Wrap-up

**Deliverables**: settings persistence, history read from the real log, plan export,
and the optional Tauri desktop shell (Form C).

**Acceptance criteria**

1. settings survive a restart;
2. history comes from the real operation log, including failed entries;
3. the exported file matches the plan object field for field;
4. if Form C is implemented: launching it calls the same core and the same execution
   module, and introduces no second decision implementation.

### Current coverage status

Last updated 2026-10-24. "Met" means a passing automated check exists and would
fail if the behaviour regressed. "Blocked" means the criterion belongs to a
deliverable that does not exist yet. Three rows are marked **renegotiated**: the
criterion as written cannot hold in the design that shipped, and the replacement
guarantee is named next to it.

| Stage | Criterion | Status |
|---|---|---|
| P0 | 1 · `cargo test` green | Met — 141 unit tests plus four integration files (8 + 5 + 3 + 4) |
| P0 | 2 · every `wt.*` reason code has a test case | Met — all 15 codes in the decision table have a positive and a negative case, every reason asserts non-empty `text` and `evidence`, and `wt.stash` — retained for old audit logs, no longer emitted — has a test asserting it cannot reach a record |
| P0 | 3 · a corpus of 60+ adversarial paths is rejected | Met — 86 entries. The floor is set to the current count rather than left at 60, so the corpus cannot shrink unnoticed; a generated property test covers 20,000 random paths on top |
| P0 | 4 · an injected timeout or permission error yields `Review` with `cannot_verify` | Met — the injectable Git runner fails every call in turn (timeout, spawn error, non-zero exit, unknown default branch, unknown merge base) and each path yields `wt.cannot_verify` at `Review`, never a positive reason |
| P0 | 5 · `cargo clippy -- -D warnings` | Met, and widened into a real gate: `pnpm lint:rust` runs the format check and `cargo clippy --workspace --all-targets`, with `clippy::all`, `clippy::pedantic`, the rustc `unused` group and `unsafe_code` denied in `[workspace.lints]`. `clippy::nursery` stays a warning because those lints are experimental. The ten `pedantic` lints that remain allowed each carry their reason in `Cargo.toml` |
| P1 | 1 · a tracked `build/` is `Unsafe`, evidence naming the match count | Met — unit, plus a real repository where `dist/` is committed despite an ignore rule |
| P1 | 2 · `node_modules` is not recognised as a project root | Met — the walk never descends into a directory it has recognised as an artifact, and a real scan reports one record per worktree |
| P1 | 3 · a directory with a nested `.git` or credential files is `Unsafe` | Met — `.git` through `af.nested_git`, credentials through `af.credentials`. Credentials needed a code of their own: they are usually Git-*ignored*, which is precisely the case `af.authored_content` cannot see |
| P1 | 4 · an escaping symlink is `Unsafe` with the target in evidence | Met |
| P1 | 5 · a size-measurement timeout is reported honestly | **Renegotiated** — the criterion asked for `Review`; the verdict is now left alone and the size is recorded as `null` with a warning (decision 5) |
| P1 | 6 · every `af.*` reason code has a test case | Met — all 10 codes |
| P2 | CLI and snapshot contract criteria | Blocked — P2 not started |
| P3 | 1 · offline over `file://`, a full review can be completed (D22) | Met for the sample data. Dropping in a snapshot file waits on P2, which has no output to drop |
| P3 | 2 · the page matches the JSON item for item | Blocked — nothing to compare against until P2 |
| P3 | 3 · no delete button, no deletion call, no write path (D17) | **Renegotiated** — the preview and the desktop shell share one component set, so deletion calls exist in the bundle and only the desktop can reach them. The guarantee is behavioural: in a preview every entry point refuses, asserted by `app.mount.test.ts` (decision 6) |
| P3 | 4 · a higher `schema_version` produces an upgrade message | Blocked — the gate belongs with P2's reader |
| P3 | 5 · the exported plan is accepted by the CLI directly | Blocked — no CLI yet |
| P3 | 6 · `src/mock/` is not referenced by production code | **Renegotiated** — the preview is part of the shipped bundle, so `mock/` stays by design and its pages are labelled as sample data; `vue-tsc --noEmit` is at zero errors and the build passes (decision 7) |
| P4 | 1 · macOS: the item is found in the Trash afterwards (D5) | Met — an integration test moves a real directory through the real Trash and takes it back out |
| P4 | 2 · no leftovers after `git worktree remove`, with `prune` as a follow-up (D6) | Met |
| P4 | 3 · untracked files force confirmation, then `--force` | Met — `--force` is re-derived from the re-check and never parsed out of the plan, and the plan page shows the file list line by line before it lets the run start |
| P4 | 4 · a whitelist hit blocks the item | Met |
| P4 | 5 · a target whose identity or activity changed is skipped with a stated reason (D11) | Met |
| P4 | 6 · a failure on one item does not affect the others (D14) | Met |
| P4 | 7 · the log has time / action / target / reason / result / size, with no network call (D15) | Met |
| P4 | 8 · Windows: the Recycle Bin path verified separately (D16) | Blocked — no Windows machine has run it; open item 1 |
| P5 | daemon and browser-UI criteria | Blocked — P5 not started |
| P6 | wrap-up criteria | Partly met — settings persist on the device, the history page reads the real log, the language picker ships, and the prototype wording is gone. Plan export copies JSON to the clipboard rather than writing a file. Appearance has `pnpm test:theme` and the page grid `pnpm test:layout`, both in a real browser (decision 5) |

The rule for every row: a criterion counts as met only when a test in the
repository fails if the behaviour regresses.

---

## 6. Change control

**Free to change**: internal implementation, algorithms and performance, internal
naming, log wording, style details, and how unit tests are organised.

**Documentation change required first** (with the justification stated in the commit
message):

- any of D1–D23 in section 1;
- any field addition, removal, or type change in `src/types.ts` or the Rust structs
  (including `schema_version`);
- reason-code semantics (adding a code belongs here, and requires updating the
  decision tables in section 5 of the product plan in the same commit);
- the page inventory and information structure;
- the division of responsibility between the three forms (which form may execute).

**Forbidden**:

- providing any deletion path that bypasses the Trash;
- moving decision logic into the front end;
- turning `cannot_verify` / `uncertain` into `Safe`;
- removing the `evidence` field from a reason;
- letting the scanning stage write anything other than the log;
- committing non-English content outside the locale tables under `src/i18n/` (D23).

---

## 7. Decided and open items

**Decided (2026-10-11)**

1. **Dependency fix (done)**: `@vitejs/plugin-vue` moved from `^6.1.0` to `^6.0.10`
   (the original version does not exist in the registry, which was the direct cause
   of the install failure), and `@types/node` was added (`tsconfig.json` includes
   `vite.config.ts` but the type package was missing). All dependencies now resolve,
   `vue-tsc --noEmit` is at zero errors, `vite build` passes, and the single-file
   prototype passes its interaction check.
2. **Delivery form**: "Form A read-only report → Form B local daemon + browser →
   Form C Tauri desktop shell (optional)". **A hosted SaaS form is not built.**
3. **`.workbuddy/`**: gitignored and never committed.
4. **Single-file prototype**: not committed (`dist/` is gitignored); it is
   reproducible through `scripts/build-preview.mjs`.
5. **UI strings (done)**: all user-visible strings were extracted into
   `src/i18n/en.ts`, read through the single swap point in `src/i18n/index.ts`.
   No locale switch ships yet; adding one means changing only that binding.
6. **Commit history (done)**: the five local commits made before D23 were rewritten
   in English with their content unchanged. Every commit from now on is English.

**Decided (2026-10-12)**

1. **`size_bytes` became optional, `schema_version` 1 → 2.** Applying the existing
   section 3 rule ("cannot be measured" must be distinguishable from "0"),
   `WorktreeRecord.size_bytes` and `WorktreeRemovalItem.size_bytes` widened from
   `u64` to `Option<u64>` (front end: `number | null`), and the snapshot schema
   version was bumped. A consumer that reads the old version must reject the new
   one rather than read `null` as `0`.
2. **Appearance modes.** The UI follows the system light/dark preference by default
   and can be pinned to `light` or `dark` in Settings. The pin is written as
   `data-theme` on `<html>`; `system` removes the attribute. Because plain CSS
   cannot share a block between a media query and an attribute selector, the dark
   palette is declared twice, and `src/theme.test.ts` asserts the two copies stay
   identical. `scripts/verify-theme.mjs` checks all six combinations of OS
   preference and choice in a real browser.
3. **The Tauri crate is built through the CLI.** A bare `cargo build` links without
   the `custom-protocol` feature, so the window loads `build.devUrl` and shows a
   blank page when no dev server runs. Use `pnpm tauri build` / `pnpm tauri dev`.
4. **An unmeasurable size stays unknown end to end.** `size_bytes: null` is not
   "0 B" anywhere: totals that omit it are shown as lower bounds (`≥` prefix plus
   a count of unmeasurable items), an unmeasurable Safe artifact still passes the
   display threshold and the default selection, result and history entries keep
   `null` instead of collapsing it, and a plan containing one gains its own
   confirmation reason because the 10 GB threshold cannot be evaluated.
   `src/format.test.ts` pins the formatting, `src/state.test.ts` the selection,
   plan and execution behaviour, and `src/architecture.test.ts` keeps the safety
   derivation out of production code and the schema version in step with the
   Rust core.
5. **The theme verification runs in a real browser.** `pnpm test:theme` builds the
   single-file preview and drives Chrome through all six combinations of OS
   preference and in-app choice, asserting the attribute, the resolved tokens and
   the painted pixels; the run uses a throwaway profile and fails loudly if the
   page never mounts or a CDP call times out.

**Decided (2026-10-13)**

1. **The UI ships a locale switch, and a second locale ships with it.** English
   stays the default and the source locale (D23); `zh-CN` is the first translated
   table. The app follows the system language on first run (`navigator.languages`,
   a `zh` prefix resolves to `zh-CN`) and can be pinned to `en` or `zh-CN` in
   Settings. The choice is persisted, and the `languagechange` event re-detects it
   while `system` is selected. `src/i18n/index.ts` holds the reactive registry
   (`messages` + `applyLocale`), and a test fails when a locale table's keys differ
   from `en.ts`. This closes the "locale switch" open item.
2. **Reason text stays English data (closes the reason-localisation open item).**
   A reason's `text` and `evidence` are scanner output — data, not UI chrome — so
   they are not translated in this change; only UI chrome is localised. Mapping the
   stable `code` to a localised template, with `evidence` kept as data, stays a
   follow-up: it changes this section and section 3, not D9, because the four
   elements (`code` + `severity` + `text` + `evidence`) remain.
3. **The desktop shell runs the real scanner and the real executor.** `scan_project`
   is implemented in `worktree-core`: worktree checks (which already existed) plus
   artifact scanning and boundary detection, assembled into one `ProjectSnapshot`.
   Execution moves items through the system Trash and `git worktree remove` /
   `prune`, re-verifies identity and activity first (D11), runs serially with
   isolated failures (D14), and appends to the local operation log (D15). The
   `src/mock/` snapshot stays for the browser preview only, is always labelled as
   sample data there, and is no longer the desktop shell's data source.
4. **The prototype wording is gone.** The `prototype` / `UI prototype` copy and the
   version note's prototype suffix are removed, and the buttons that used to only
   surface a notice (choose folder, open Trash, open log directory) now perform the
   real action through the Tauri dialog and opener plugins.
5. **One page width, and nothing paints over anything beside it.** Both were
   reported from a screenshot of the build-artifacts page. The width is the content
   pane's own padding (24px) on every page: the welcome, overview, plan, history
   and settings pages were capped at 860px and centred, so their left and right
   edges sat 56px in and moved sideways whenever the user changed view. The general
   rule the layout has to keep: a box never draws outside the box it was put in. It
   was first fixed by giving every column a share of its table and letting each
   cell clip its own content (`overflow: hidden`, an ellipsis where the text is one
   line), because under `table-layout: fixed` a column given no width takes what
   is left over and paints over the cell next door — the verdict badge was sliced
   in half by the reason box beside it. Decision 6 removes the columns that made
   it necessary on the three list pages.
6. **The three list pages are rows, not columns.** Each item is three lines: the
   identity (kind, name or path, and the size and verdict fixed at the right edge
   so they scan vertically), the supporting facts in the words a column header
   used to carry, and the evidence — every reason, tinted by the verdict, not one
   behind a tooltip. This was a user decision after a screenshot showed the
   artifact list reading left-heavy: a path is the one value that cannot be
   shortened without becoming ambiguous, and as one of seven columns it shared a
   horizontal axis with six short fields, so it broke into five lines while three
   columns beside it sat half empty. A row gives the path the whole width, and a
   narrow window now truncates it rather than wrapping it. The overview's rows
   carry no checkbox — that page is a way in, not a place to choose. The cleanup
   plan and the execution result stay tables: their columns are all short and none
   of them holds a path.
   `pnpm test:layout` walks every page in a real browser, in both locales, at the
   app's default window size: it compares each page's margins with the worktree
   page's, and asserts that no row is wider than its container, that no row wraps
   its name over a second line, that the figures stay beside the text rather than
   over it, that no two rows overlap, and that every row paints its reasons. The
   sample snapshot carries a path long enough to wrap on purpose — without it that
   check cannot fail, which is how it was tested.

**Decided (2026-10-12, while building the above)**

These came out of the implementation. Each one either narrows a criterion to what
the shipped design can guarantee, or writes down a choice that would look
arbitrary later.

5. **A size-measurement timeout does not lower a verdict.** P1 criterion 5 asked
   for `Review` when the walk times out. That conflated two different things: the
   verdict is derived from Git facts and the activity window, and neither depends
   on the size. Downgrading an item because a directory was too slow to measure
   would be a false signal, and it would park a genuinely safe item behind a
   threshold the user cannot act on. A timeout now records `size_bytes: null` plus
   a warning, the verdict is untouched, and every total that includes the item is
   presented as a lower bound. `per_dir_timeout` already meant "the size becomes
   unknown"; this is what that means for the level.
6. **The preview and the desktop shell share one component set, so D17's "no
   deletion call" is now a behavioural guarantee.** P3 criterion 3 was static: the
   bundle contained no delete button and no deletion call, asserted by scanning the
   build output. Since the same pages serve the desktop shell, the bundle does
   contain deletion calls, and only a desktop build can reach them. What replaces
   the static check is that in a browser every entry point refuses — `isDesktop()`
   is false, so `invoke` raises before anything else happens — and
   `app.mount.test.ts` mounts the real app in a browser-like environment to assert
   both that the pages render and that a click which cannot work says why instead
   of doing nothing. Scanning the build output is kept for D17's other half:
   nothing in the front end writes files.
7. **`src/mock/` stays, and is labelled where it is used.** P3 criterion 6 asked
   for the sample snapshot to leave production code. The browser preview is part of
   what ships — it is how the pages can be reviewed without a repository — so the
   snapshot stays in the bundle. Instead of being unreferenced it is disclosed: the
   sidebar says the build is a preview, and the pages fall back to the snapshot only
   when there is no desktop to ask. A desktop build never reads it.
8. **One chunk, always.** The single-file preview is a classic script that opens
   over `file://`, so it can neither fetch a second chunk nor contain `import.meta`
   — which Vite emits for a lazily imported module, and which is a syntax error in
   a classic script. `vite.config.ts` therefore sets `inlineDynamicImports`, and
   `src/platform.ts` imports the Tauri packages statically: their module bodies only
   define functions, so importing them in a browser is harmless, and every entry
   point checks `isDesktop()` before touching the host.
9. **On macOS the Trash is reached through `NSFileManager`, not the Finder.** The
   `trash` crate's default routes the request through `osascript` to the Finder,
   which needs Automation access, prompts the first time, and fails outright when it
   is refused — which is exactly what the integration test observed. The executor
   selects the crate's `NSFileManager` method instead: the same recoverable Trash,
   without the extra permission and without the prompt.
10. **Settings, the language choice and the recent-project list persist on the
    device.** `localStorage` only, guarded so the module still loads outside a
    browser, and validated on read (`isLocaleChoice`) so a hand-edited entry cannot
    put the app into a state it does not support. The browser check clears storage
    between its cases, because a theme picked in one case would otherwise still be
    in effect in the next and the "follow system" case would measure the wrong thing.
11. **Interactive elements carry stable hooks for the automated checks.**
    `data-view`, `data-action`, `data-locale` and `data-theme-choice` exist because
    the UI follows the system language: a check that clicks a button labelled
    "Settings" breaks on a Chinese machine, and this repository runs on one. A
    selector is the only thing about a button that a translation cannot change.

**Proposals (reviewed, and settled)**

`docs/proposals.md` records ten proposals and one optional half. Each is now either
implemented or rejected, and that file carries the outcome per item. They came from a
review of the deletion guards in `path.rs` and of the working tree as it stood on
2026-10-12.

Implemented on 2026-10-12:

- **P-1 · the protected-location comparison.** Every comparison that decides
  protection now goes through a case-folded guard key, the list carries both
  spellings of each symlinked location, and the home-root shape is folded on both
  names. `/SYSTEM/Library/CoreServices` and `/private/var/db/x` were accepted before
  this; both are corpus entries now.
- **P-2 · names Win32 reads differently.** A component ending in a dot or a space,
  containing a colon, or naming a device is refused — on Windows only, since no other
  platform has the rules. The predicate is a pure function of `&str`, so its table is
  tested everywhere.
- **P-3 · the home directory by identity.** On Unix it is matched by `(device, inode)`
  as well as by spelling, so a link to it is refused as the home directory. Windows
  identity waits for the D16 verification: a stable file id needs an API `std` does
  not expose, and the spelling and structural guards there are unchanged rather than
  weakened.
- **P-4 · deletion-call hygiene.** Paths reach Git after `--`, and the residual window
  between validation and the Trash move is written down in product plan §6.5.
- **P-6 · released space.** Defined in section 3 of this document, and the pages that
  offer an action say on screen when there is no desktop to carry it out.
- **P-7 · the field contract.** `src/architecture.test.ts` compares the contract
  structs and the reason codes across the two languages; removing a field or adding a
  code on one side alone now fails.
- **P-8 · the Rust gate.** `[workspace.lints]`, `rustfmt.toml`, `rust-toolchain.toml`
  and `pnpm lint:rust`, with `clippy::all` and `clippy::pedantic` denied. The triage
  is done: 332 findings were sorted into ten lints allowed with a written reason and
  the rest either fixed or applied mechanically. `clippy::nursery` stays a warning on
  purpose, because those lints are experimental.
- **P-9 · a routing table.** `AGENTS.md` says where a change belongs.
- **P-10 · check order and corpus size.** The precedence of D10's checks is pinned by
  a test that applies two rules at once, and the corpus floor is its current count.

Rejected:

- **P-1.5 · folding the `/System/Volumes/Data` prefix.** Nothing produces that
  spelling — `realpath('/Users/andy')` is `/Users/andy`, measured on this machine —
  and a path written that way by hand is already refused by the `/System` entry. The
  proposal offered to drop it for the smaller diff.

**P-5** had already landed before this review, in the commit that wired the desktop
shell to the real scanner and the real executor.

A second review pass on 2026-10-12, over the whole of the accumulated work, found four
defects. Each was reproduced before it was fixed, and each fix is pinned by a test that
was checked to fail without the fix:

- **An unmeasurable directory claimed to be empty.** `Measurement::unknown()` carries a
  file count of zero, so the emptiness check read a size timeout as proof that a
  directory held nothing and stamped `af.empty` — a **Safe** reason — onto it. A
  fifty-file directory whose measurement timed out was reported as empty. Emptiness is
  now gated on the measurement having completed, which is the section 3 rule applied to
  a field it had not reached.
- **The credential check was off where it mattered most.** Its entry bound was 2,000,
  while this repository's own `node_modules` holds about 5,800 entries, so the most
  common artifact kind always ended in `cannot_verify` — a `Review` on nearly every
  `node_modules`, which is how a check stops being read. The bound is now 50,000, and
  crossing it is reported as a partial walk with the count it reached instead of as
  either "nothing found" or "unable to look at all".
- **A bundled artifact was reported as deleted when it was not.** `bundled_with_worktree`
  was taken at face value, so an item was logged `Ok` — "removed together with its
  owning worktree" — even when no worktree was removed at all. The audit log recorded
  deletions that never happened, which is the one thing an audit trail must not do. The
  decision now reads the set of worktrees the run actually removed, and an artifact
  whose owner stayed is handled on its own merits.
- **The desktop shell discarded the execution settings.** `execute_plan` filled
  `ExecuteOptions` from `Default`, so the per-directory measurement budget the user set
  governed the scan and was then replaced by a hard-coded 30 s during execution. Both
  budgets now travel from the front end, and the scan and the re-check measure the same
  directories with the same numbers.

**Open**

1. **How to verify the Windows Recycle Bin path**: D16 requires separate verification
   per platform. How does Form A / Form B obtain and retain that record on a real
   Windows machine?
2. **How Form B's daemon is distributed**: as a `worktree-cleaner serve` subcommand
   (single binary) or as a separate executable.
3. **Where `snapshot.json` is written**: stdout or a conventional default path, and
   the artifact naming rule for CI.
4. ~~**Whether an artifact directory holding credential files should be blocked
   outright.**~~ **Resolved 2026-10-12**: it is blocked outright, by a new
   `af.credentials` code. The open question was whether to widen
   `af.authored_content`'s evidence instead, and the answer is no: credentials are
   usually Git-*ignored*, so the untracked-content check cannot fire for them and the
   two conditions are genuinely different. The walk also descends: an integration test
   found that a `.env` written one directory below the candidate — the ordinary case —
   was missed by a check that only read the top level, so the check now recurses under
   a fixed entry bound and reports crossing that bound as `Review` rather than as a
   pass. What stays open: a secret whose name gives nothing away (`config.json` holding
   an API key).
5. **The one thing the automated checks cannot reach.** Driving the native folder
   picker, the system Trash and `git worktree remove` in the packaged app needs a
   human at the keyboard: the checks in this repository can dock the window and
   screenshot it, but not click a button in another process. The Rust integration
   tests cover the executor and `pnpm test:theme` covers rendering, so what is left
   is one manual pass on a real machine — opening the app, choosing a repository,
   and confirming the picker appears.
