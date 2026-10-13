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
| D10 | Before deletion, the path must pass the eight checks in order, and verification may only tighten — a resolved path never grants permission the literal path lacked | Guards against symlink escape and empty-variable collapse | The whole fuzz corpus is rejected; the ancestor-symlink case is rejected |
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
| D23 | **English for everything that ships as open source.** Code, comments, identifiers, documentation, commit messages, file names, and user-visible strings in the repository are English, and English is the default and the source locale. Content deliberately kept out of the open-source distribution may be written in Chinese | This is an international open-source project; a mixed-language repository is not reviewable by the community it targets | A scan of tracked files finds no non-English natural language; every commit message reads in English |

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
  must be mirrored on the other side within the same commit, backed by a test (the
  Rust side asserts a serialise/deserialise round trip on a sample).
- Optional numbers use `Option<u64>` (front end: `number | null`):
  **"cannot be measured" must be distinguishable from "0"**, and the UI shows
  `unknown` and `0 B` respectively.
- Every reason must carry a non-empty `text` and a non-empty `evidence`. The
  constructors assert this in test builds.
- Internal naming is free, but field names that appear externally, reason codes,
  and command text (such as `git worktree remove --force "<path>"`) are part of the
  contract and must not change casually.
- Language: identifiers, comments, documentation, commit messages, file names, and
  user-visible strings are all English (D23). Commit messages are written as
  `type(scope): short imperative summary`, followed by a body explaining what
  changed and why.
- User-visible strings live in `src/i18n/en.ts` and are read through
  `src/i18n/index.ts`, which is the single swap point for adding a locale. Domain
  text produced by the scanner (a reason's `text` and `evidence`) is data rather
  than UI chrome and stays with the record.
- File names are English, lowercase, and use `kebab-case` for docs and scripts.
  Rust and TypeScript files follow their ecosystem conventions (`snake_case` for
  Rust modules, `PascalCase.vue` for components).

---

## 4. Testing requirements

- **Every reason code has at least one positive and one negative case.** The list of
  codes is in the decision tables in section 5 of the product plan.
- The path validator must have a fuzz test (cargo-fuzz or proptest). Invariant:
  **any path that passes verification must be absolute, free of NUL bytes, and free
  of `..` path components.**
- The following counter-examples are mandatory; none may be skipped:
  - a `build/` directory containing Git-tracked files;
  - `node_modules` mistaken for a project root;
  - a path that collapses into its parent because a variable was empty;
  - an **ancestor** of the path being a symlink that points somewhere dangerous;
  - artifact size measurement timing out, which must produce Review and display
    `unknown` rather than 0;
  - the target being replaced, or becoming active, after the plan was generated.
- Front end: any UI change must leave `vue-tsc --noEmit` at zero errors and the
  build passing.
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

Last updated 2026-10-12. "Met" means a passing automated check exists and would
fail if the behaviour regressed. "Blocked" means the criterion belongs to a
deliverable that does not exist yet.

| Stage | Criterion | Status |
|---|---|---|
| P0 | 1 · `cargo test` green | Met — 101 unit + 5 integration + 4 against a real repository |
| P0 | 2 · every `wt.*` reason code has a test case | Met — all 16 `wt.*` codes have a positive and a negative case in `worktree.rs`, and every reason asserts non-empty `text` and `evidence`. The 10 `af.*` codes remain a P1 criterion |
| P0 | 3 · a corpus of 60+ adversarial paths is rejected | Met — 78 entries, plus a generated property test over 20,000 random paths |
| P0 | 4 · an injected timeout or permission error yields `Review` with `cannot_verify` | Met — the injectable Git runner fails every call in turn (timeout, spawn error, non-zero exit, unknown default branch, unknown merge base) and each path yields `wt.cannot_verify` at `Review`, never a positive reason |
| P0 | 5 · `cargo clippy -- -D warnings` | Met |
| P1 | artifact scan criteria | Blocked — P1 not started |
| P2 | CLI and snapshot contract criteria | Blocked — P2 not started |
| P3 | report page criteria | Partly met — the prototype opens offline over `file://` with no console errors and no non-English text; the appearance modes are verified in a real browser (`pnpm test:theme`, 6/6 combinations of OS preference and in-app choice); `vue-tsc --noEmit` is at zero errors and the build passes. It still reads the mock rather than a snapshot file, so the item-for-item JSON comparison (criterion 2) and the version-gate message (criterion 4) remain open |
| P4 | execution criteria | Blocked — P4 not started |

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
- committing non-English content (D23).

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

**Open**

1. **How to verify the Windows Recycle Bin path**: D16 requires separate verification
   per platform. How does Form A / Form B obtain and retain that record on a real
   Windows machine?
2. **How Form B's daemon is distributed**: as a `worktree-cleaner serve` subcommand
   (single binary) or as a separate executable.
3. **Where `snapshot.json` is written**: stdout or a conventional default path, and
   the artifact naming rule for CI.
4. **How reason text becomes localisable**: a reason currently carries final English
   `text` produced by the scanner. For other locales the UI would need to map the
   stable `code` to a localised template and treat `evidence` as data. Decide before
   a second locale is added.
5. **Locale switch**: whether to ship a language picker in the settings page once a
   second locale exists.
