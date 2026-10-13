# Proposals

**Status: reviewed and settled on 2026-10-12.** Every item is now either implemented
or rejected; the outcome per item is in the table at the end of this file, and the
reasoning is in `docs/DEV_CONTRACT.md` §7. The body of each proposal is kept exactly
as it was written, because it is the record of what was found and why — the code it
describes has moved on since.

Each item is approved or rejected on its own. On approval, the code change and the
documentation change it implies land in the same commit, as section 6 of
`docs/DEV_CONTRACT.md` requires.

**How to read an item.** Every item names the files it touches, the contract text it
changes, an executable acceptance check, and its risk. Claim markers:

- **measured** — reproduced on this machine with the command quoted beside it
  (macOS 27.0.1, root volume `apfs, sealed, local, read-only, journaled`,
  2026-10-12).
- **by reading** — established from the source in this repository, with the file
  and function named.

**Basis.** Two inputs produced this list: a review of `tobi/disktree` — a Rust
disk-usage explorer that scans, marks paths and then removes them, and whose
deletion guards are the closest prior art to ours — and a pass over this
repository's own guards and working tree. Both are recorded here only where they
bear on a proposal.

---

## 0. State of the working tree

**Snapshot taken 2026-10-12 11:10, and it moved while this was being written.** The
tree was being edited by something else during the review — files under `src/`
changed between two reads minutes apart. Re-run the two checks named below before
acting on anything in this section; they are cheap, and they are what the section
actually rests on.

`git status` is **not clean**. Two unrelated bodies of work are in flight:

**a. The desktop wiring — half done.** This is the one that matters for P-5.

| File | State | What it carries |
|---|---|---|
| `src-tauri/src/lib.rs` | modified | `execute_plan`, `log_path`, `read_history`, `clear_log` commands; the dialog and opener plugins; `scan_project` grew an `options` argument |
| `src/platform.ts` | **untracked** | the front-end IPC layer for exactly those commands, with `isDesktop()` as the one place that knows shell from browser |
| `src/App.vue` | modified | `import { isDesktop } from "./platform"` — the shell is the first place to reach it (`src/App.vue:22`) |
| `crates/worktree-core/src/cleanup.rs` | modified | `Serialize` / `Deserialize` on `ExecutionOutcome` |
| `src-tauri/Cargo.toml`, `src-tauri/capabilities/default.json`, `package.json`, `Cargo.lock` | modified | the two Tauri plugins and their capability entries |

Check 1 — `grep -rn "platform" src/` returns the import in `App.vue` and nothing
else. The IPC layer has been reached, but only for the shell/browser question.

Check 2 — `grep -n "executeCleanup" src/views/CleanupPlanView.vue` returns the
import and the button handler (`src/views/CleanupPlanView.vue:284`). **The Execute
button still runs the front-end simulation**, which marks every item `result: "ok"`,
sums the planned sizes and reports them as freed space, without touching the
filesystem — **by reading** `src/state.ts:395`. Nothing in `src/` imports
`platform.executePlan`, so the real executor is still unreachable from the UI.

> This observation is from 2026-10-12 and no longer holds: see the resolution note
> on P-5. It is kept because it is what the proposals below were written against.

So `DEV_CONTRACT.md` section 7, "Decided (2026-10-13)" items 3 and 4 — *the desktop
shell runs the real scanner and the real executor*, and *the buttons now perform the
real action* — describe an end state the tree has not reached. That is a status note,
not a criticism of the decision, and finishing it is **P-5**.

**b. The locale tables.** `src/i18n/zh-CN.ts` and `src/i18n/locale.test.ts` are new
and untracked; `src/i18n/en.ts`, `src/i18n/index.ts`, `src/types.ts`,
`src/App.vue` and `src/components/SafetyBadge.vue` are modified. This matches the
"Decided (2026-10-13)" locale entry. It is unrelated to every proposal below except
P-6 and P-7, which touch the same files.

Neither body of work is committed.

---

## 1. Proposed changes

| ID | Proposal | Priority | Kind |
|---|---|---|---|
| P-1 | Protected-location matching is spelling-sensitive, in two ways | **Blocker** | Safety |
| P-2 | Refuse names Win32 reads as a different name | High (before Windows deletion) | Safety |
| P-3 | Match the home directory by identity, not only by spelling | Medium | Defence in depth |
| P-4 | Deletion-call hygiene: `--` before paths, and the residual window | Low | Hardening |
| P-5 | Commit and finish the desktop wiring | **Blocker** for the desktop form | Correctness |
| P-6 | A simulation must never be presented as a result | High | Contract + UI |
| P-7 | Contract test: field for field, mechanically | Medium | Drift guard |
| P-8 | Rust gate: lint policy, pinned toolchain, formatter | High | Process |
| P-9 | `AGENTS.md` gains a "where a change belongs" table | Low | Process |
| P-10 | Pin D10's check order, and stop the fuzz corpus shrinking | Low | Test strength |

---

### P-1 · The protected-location check is spelling-sensitive, in two ways

**Priority** Blocker (safety) · **Kind** security · **Files** `crates/worktree-core/src/path.rs`

**Problem.** `protected_location` compares with `Path::starts_with`, which compares
path components byte for byte and never asks the filesystem. macOS volumes ignore
case by default, and `realpath` does not correct the case of a name it passes
through — so a protected directory is reachable under a spelling the list does not
contain.

**measured**

```sh
$ python3 -c "import os; print(os.path.exists('/SYSTEM'), os.path.realpath('/SYSTEM'))"
True /SYSTEM
$ python3 -c "import os; print(os.path.exists('/BIN/sh'), os.path.realpath('/BIN/sh'))"
True /BIN/sh
$ python3 -c "import os; print(os.path.exists('/LIBRARY/EXTENSIONS'), os.path.realpath('/LIBRARY/EXTENSIONS'))"
True /LIBRARY/EXTENSIONS
```

The same holds for `/SBIN`, `/DEV`, `/PRIVATE/ETC` and `/var/DB`. `realpath`
preserves the case in every one of them. Check 6 is therefore passed by
`/SYSTEM/Library/CoreServices`, `/BIN/sh`, `/LIBRARY/EXTENSIONS/<name>` and
`/DEV/null` — on the literal form, and again on the resolved form, which is the same
string. (`/etc` and `/var` accidentally survive, because they are symlinks into
`/private` whose targets happen to be spelled canonically; that is luck, not a
guard.)

The second way is the list's own content. It carries `/var/db` — the spelling that
reaches the directory through the `/var` symlink — but not `/private/var/db`, its
canonical name.

**measured**

```sh
$ python3 -c "import os; print(os.path.exists('/private/var/db'), os.path.realpath('/var/db'))"
True /private/var/db
```

A candidate written `/var/db/x` is refused. A candidate written `/private/var/db/x`
matches neither the literal comparison nor the resolved one, and is accepted. Two
spellings of one directory, two answers, and the canonical spelling gets the wrong
one.

**Scope of the damage, honestly.** On a stock machine the read-only system volume
and SIP stand between most of these paths and a destroyed system, so the likely
outcome is a failed deletion rather than damage: the guard fails to refuse, and the
permission system saves us. That is not a reason to leave it. SIP is routinely
disabled by exactly the audience this tool is for; `/Library/Extensions` and
`/private/var/db` are ordinary admin-writable directories with no such backstop; and
D10 promises that verification may only ever tighten. A guard that a changed letter
walks past is a bug whatever happens to catch the fall.

**Change.**

1. Compare through a **guard key** rather than the raw path, in every comparison
   that decides protection: `protected_location`, the home-directory equality, the
   empty-variable-collapse equality, and the whitelist match. The key is the
   normalized path, lowercased on macOS and Windows. Windows already has
   `windows_path_key` for the `\\?\` folding; promote it to be that one key.
2. Keep both spellings in the list, and drop nothing. Add the canonical form of
   every entry whose immediate parent is a symlink (`/private/var/db` beside
   `/var/db`; `/private/etc` is already there). An extra entry that is a no-op on
   Linux costs nothing.
3. Change only the comparisons. What is reported stays as it was given:
   `ValidatedPath.target`, and the path inside a rejection's evidence.
4. Fold the first component in `is_home_root_shape` for both names. Today `Users`
   is folded and `home` is compared exactly, so `/HOME/andy` is not recognised.
5. **Optional, low value**: fold the `/System/Volumes/Data` prefix. **measured** —
   on this machine `realpath` never returns that spelling
   (`realpath('/Users/andy')` is `/Users/andy`), so folding it only matters if a
   user pastes the Data-volume spelling by hand as a scan root or a whitelist rule.
   Drop it if you prefer the smaller diff.

**Contract impact.** D10 and product plan §6.4 item 6 gain one sentence: the
comparison is case-folded on platforms whose filesystems ignore case, and both the
symlink spelling and the canonical spelling of a protected location are refused.
The number of checks stays eight.

**Acceptance.** A case table over the protected list: every entry refused in upper,
lower and mixed case at depth two or more; the canonical spelling of every symlinked
entry refused; `/Users/andy/proj` and an ordinary project path still accepted.
Plus the repository's own rule — a mutation check that removes the folding from the
key and asserts the table fails.

**Risk.** Low. Every change tightens. Folding case can only add refusals, never
remove one, which is the direction D10 requires.

---

### P-2 · Refuse names Win32 reads as a different name

**Priority** High — required before any Windows deletion is verified (P4, D16) ·
**Kind** security · **File** `path.rs`

**Problem.** Four Win32 rules make a name on disk different from the name Win32
opens: a trailing `.` or space is stripped; `NUL`, `CON`, `PRN`, `AUX` and `COM1`…`LPT9`
are devices in any directory; what follows a `:` is an alternate data stream. NTFS
keeps such names when they arrive through `\\?\` or from WSL, and a directory
listing returns them as stored — so a scan can surface one and hand it to the delete
call, which opens it through Win32 and acts on something else. macOS has none of
these rules and is unaffected.

`path.rs` rejects control characters, which covers NUL, and `windows_path_key`
already folds the `\\?\` prefix. Nothing covers the rules above.

**Change.** One predicate over a single name, applied to every component on Windows:
ends in `.` or space; contains `:`; or the stem (up to the first `.`, right-trimmed,
lowercased) is a device name. Refuse with a message naming the component, in the
same family as the existing `ControlCharacter` rejection.

**Contract impact.** Widen product plan §6.4 item 5 from "control characters" to
"control characters, and names the platform would read as a different name". D10
keeps its count of eight.

**Acceptance.** A unit test of the predicate against a name table — it can run on
macOS, because the predicate is a pure function of a `&str` — with negative cases
for `concepts`, `nul-test`, `a.b` and macOS-legal `Report.`. The Windows run is a
separate record (D16).

**Risk.** Windows only. No behaviour change on macOS.

---

### P-3 · Match the home directory by identity, not only by spelling

**Priority** Medium · **Kind** defence in depth · **File** `path.rs`

**Problem.** The home directory is protected by comparing strings: `path == home`,
plus a structural fallback for the `<root>/Users/<name>` and `<root>/home/<name>`
shapes. **by reading** — `protected_location`, `path.rs`. A route to the same
directory under a different shape is invisible to both: if `/home` is a symlink to
`/data/home` and the scan root is `/data`, the candidate `/data/home/andy` is
neither equal to the home path nor of home-root shape. The symlink checks do not
help either — the link is above the scan root, which is the user's own choice of
root.

**Change.** Protect the home directory by identity as well as by spelling: resolve
it once, and compare candidates both by guard key (P-1) and by `(device, inode)` on
Unix, `(volume serial, file id)` on Windows. To keep it testable, split
`protected_location(path)` into a pure `protected_location(path, home: Option<&Home>)`
that the tests drive directly, with the public entry point building `Home` once and
the identity looked up through injected values.

**Contract impact.** Widen product plan §6.4 item 6: the user's home directory,
matched by real path and by identity. D10 keeps its count of eight.

**Acceptance.** A temporary tree plus a symlinked fake home, injected as `Home`:
the candidate is refused, and its ordinary sibling is not.

**Risk.** Low. The identity is always taken from what a link leads to, so a link to
the home directory is refused as the home directory — more careful, never less.

---

### P-4 · Deletion-call hygiene: `--` before paths, and the residual window

**Priority** Low · **Kind** hardening · **Files** `cleanup.rs`, `path.rs`

**Problem, first half.** Every path handed to Git is concatenated into an argument
vector with no separator, so the argument parser sees it as an option if it ever
starts with `-`. Paths here are absolute, which makes that unlikely — this is cheap
insurance, not a live hole.

**measured** — Git accepts the separator, so the change is safe (git 2.54.0):

```sh
$ T=$(mktemp -d); git init -q "$T/repo"; cd "$T/repo"
$ git commit -q --allow-empty -m init
$ git worktree add -q ../wt -b feat
$ cd "$T" && git -C repo worktree remove --force -- "$T/wt"   # exit code 0
$ git -C repo worktree list    # only the main worktree remains
```

**Problem, second half.** The validator runs, and then the deletion is issued. What
happens between those two moments is not ours. The belief is that macOS's Trash move
resolves the parent directory and keeps the final name, so a *leaf* swapped for a link
is moved as the link and its target is left alone — **to confirm**, by reading the
`trash` crate's own source or documentation rather than by assuming it, because it is
a property of the crate we call and not a promise anyone here wrote down. Either way
the contract should say what is guaranteed and what is not.

**Change.**

1. `git worktree remove [--force] -- <path>`; update the plan's `command` text with
   it, and the fixtures that compare it.
2. State the residual window in writing: what the validator guarantees, what the
   Trash backend does about the leaf, and what remains (a parent directory replaced
   between the two moments). Add a test for the leaf-symlink decision that does not
   touch the real Trash — build the plan item from a link and assert the item carries
   the link, not what it points at.

**Contract impact.** §3's "command text is part of the contract" applies: the
`command` string changes, so `src/types.ts`'s comment and the fixtures change with
it. The residual-window note goes into §6.5 of the product plan, beside the existing
time-of-check note.

**Risk.** Low. The `--` form is verified against a real Git above.

---

### P-5 · Commit and finish the desktop wiring

**Resolved 2026-10-12.** The Execute button and the scan button both go through
`src/platform.ts` to the Rust commands, the result page's figures come from
`ExecutionOutcome`, and `platform.ts` is committed. The steps below are kept as the
record of what the change was; the code no longer matches their "before" state.
One item was resolved differently from the proposal: the browser preview keeps the
sample snapshot rather than the simulation, and labels it (decision 7 in the
development contract).

**Priority** Blocker for the desktop form · **Kind** correctness · **Files**
`src/views/CleanupPlanView.vue`, `src/state.ts`, `src/platform.ts`, `src-tauri/src/lib.rs`

**Problem.** Section 0 describes it: the Rust commands and the IPC layer exist, the
IPC layer is reached — but only for the shell/browser question, not for the executor.
The desktop shell's Execute button therefore still runs the front-end simulation,
which reports success for every item and a freed figure that was never measured.
`src/platform.ts` is untracked, so the IPC layer is one careless `git clean` away
from disappearing along with the work that built it.

**Change.**

1. Route Execute through `platform.executePlan()` when `isDesktop()`, and route the
   scan through `platform.scanProject()` the same way; the browser preview keeps the
   simulation (which is what P-6 constrains).
2. Take the result screen's figures from `ExecutionOutcome` — the counts of `Ok`,
   `Skipped` and `Failed`, and the per-item details — rather than from the plan.
3. Read history from `platform.readHistory()` and the log path from
   `platform.logPath()`, so the History page and the Settings page stop describing a
   file they never read.
4. Commit the in-flight work as its own commit, before the changes above, so the
   review of each is separable.

**Contract impact.** Section 7, "Decided (2026-10-13)" items 3 and 4 gain a status
line: the wiring landed on this date, and the desktop shell no longer reports
simulated results.

**Acceptance.** A test asserting that the cleanup path is behind `isDesktop()` — that
no view reaches the simulation without one — in the same family as the existing
`src/architecture.test.ts` rules. Plus a manual run of the desktop app: scan a real
repository, execute a plan with one item, and confirm the item appears in the Trash
and in the log.

**Risk.** Medium. UI work, and it is the change that makes every later claim about
the desktop shell true. It must not add a deletion capability to Form A: the preview
keeps simulating, and P-6 makes it say so.

---

### P-6 · A simulation must never be presented as a result

**Priority** High · **Kind** contract + UI · **Files** `src/state.ts`,
`src/views/ResultView.vue`, `src/i18n/en.ts`, `src/i18n/zh-CN.ts`

**Problem.** Two shapes of the same mistake, and the second is already in the
product plan.

1. **The preview's result screen.** With P-5 in place, a browser preview still runs
   a simulation. That is fine — what is not fine is a result screen that looks
   identical to a real one. A page that decorates fabricated `ok` rows with a freed
   figure teaches the user to trust a number nothing measured.
2. **"Released space" has no definition.** Product plan §3 promises an *estimated
   releasable space* and §7 a summary of *space released*, and nothing says what may
   be counted. **by reading** — `src/state.ts:399` sums the planned sizes of items
   the simulation marked `ok`; a real summary must not sum a `Skipped` item, a
   `Failed` item, or an artifact that left together with its worktree.

The principle worth writing down is the one the product already believes elsewhere:
*a figure that cannot be stood behind is worse than a smaller figure.*

**Change.**

1. Define it in the contract: **released space counts only items whose result is
   `Ok`; a bundled artifact counts zero; an unmeasurable item stays unknown and is
   counted, never summed or shown as `0 B`.** We do not read the volume's free space
   before and after, so every figure is an estimate and the UI says so — `≈`, with
   the basis named. (Measuring the volume with `statvfs` before and after, and
   reporting the real delta, is a possible later step; it is not proposed here.)
2. Label the preview: when `isDesktop()` is false, the result screen and the plan
   screen carry a visible "browser preview — nothing was moved" note, in both locale
   tables.

**Contract impact.** §3 gains the released-space rule. Product plan §7 gains the
wording of the label. No D changes.

**Acceptance.** `src/state.test.ts`: a plan with one `Skipped` item contributes zero
to the freed figure; a `null`-sized item contributes to `freed_unknown` and not to
`freed`; a bundled artifact contributes zero. A test that the preview label is
rendered when `isDesktop()` is false.

**Risk.** Low, once P-5 has landed. Doing it before P-5 would label a screen that is
about to be rewired.

---

### P-7 · Contract test: field for field, mechanically

**Priority** Medium · **Kind** drift guard · **Files** `src/architecture.test.ts`

**Problem.** §3 says `src/types.ts` matches the Rust structs field for field and
that a change on either side is mirrored in the same commit. Nothing checks it.
`src/architecture.test.ts` already compares one number across the two languages (the
schema version), so the precedent and the mechanism both exist.

**Change.** Extend it to the field lists: read the `snake_case` field names of the
contract structs from the Rust source and the field names of the matching interfaces
in `src/types.ts`, and assert the two sets are equal. Cover `ProjectSnapshot`,
`WorktreeRecord`, `ArtifactRecord`, `Reason`, `SafetyAssessment`, `CleanupPlan`,
`ResultEntry` and `HistoryEntry`. Compare the `wt.*` / `af.*` code lists the same
way, so a code added on one side and not the other fails.

**Contract impact.** None; this enforces an existing rule.

**Acceptance.** Remove a field from `src/types.ts` and the test fails. Add a code to
the Rust enum and not to the TS union, and it fails.

**Risk.** Low, with one caveat worth stating: a regex over Rust source is a weaker
parse than the compiler. It is the mechanism the file already uses, and the failure
mode is a false pass, not a false alarm.

---

### P-8 · Rust gate: lint policy, pinned toolchain, formatter

**Priority** High · **Kind** process · **Files** `Cargo.toml`, `rust-toolchain.toml`
(new), `rustfmt.toml` (new), `package.json`, `AGENTS.md`

**Problem.** **by reading** — `Cargo.toml` has no `[workspace.lints]`, there is no
`rust-toolchain.toml` and no `rustfmt.toml`, and the acceptance criterion "clippy
passes" therefore depends on whatever the machine's clippy happens to flag this
month. `cargo clippy --all-targets -- -D warnings` is green today at the default
lint level; that is a floor, not a policy.

**Change.**

1. `[workspace.lints]`: `clippy::all` and `clippy::pedantic` as `deny`,
   `clippy::nursery` as `warn`, `unsafe_code` as `deny`, and every deliberate
   exception allowed **with its reason written beside it** — never silently.
2. `rustfmt.toml` with `max_width = 100`. **measured** — 26 lines in the workspace
   exceed 100 columns, so this is a small reformat, not a rewrite.
3. `rust-toolchain.toml` pinning the toolchain with the `rustfmt` and `clippy`
   components, so a fresh clone builds and can run the gate.
4. A `pnpm lint:rust` script running `fmt --check` then
   `clippy --workspace --all-targets -- -D warnings`, and one line in `AGENTS.md`
   naming it as the gate.
5. **Deliberately not changed**: `edition = "2021"` and `resolver = "2"`. Resolver
   3 belongs to edition 2024, and moving editions is not a formatting decision.

**Effort, honestly.** **measured** —
`cargo clippy --workspace --all-targets -- -W clippy::pedantic` currently emits
**124 warnings**. The policy is worth having, but it lands in two steps: first the
config with `pedantic` as `warn` plus a written list of the outstanding items, then
the triage that lets it become `deny`. Say in `AGENTS.md` which of the two states
the repository is in, so nobody believes a gate that is not yet a gate.

**Contract impact.** §4 (testing requirements) and §5 (P0's acceptance criterion 5)
gain the exact command, so "clippy passes" means one thing.

**Acceptance.** A deliberately unused variable fails the gate.

**Risk.** Low for behaviour — no runtime code changes — and the main cost is the
triage pass, which is bounded and known.

---

### P-9 · `AGENTS.md` gains a "where a change belongs" table

**Priority** Low · **Kind** process · **File** `AGENTS.md`

**Problem.** A change's home is implied by the architecture section rather than
stated: measurement and parallelism in one module, the verdict in another, the
deletion guards in a third. Someone — or something — arriving at the repository has
to read all three files to find out where a change belongs.

**Change.** One table: rows are kinds of change (a reason code's meaning, a path
guard, the worktree verdict, an artifact verdict, the executor, the log, the
contract types, a UI string, a locale table), columns are the file and the contract
section that governs it.

**Contract impact.** None.

**Acceptance.** None needed; it is a pointer, and P-7 is what makes it stay true.

**Risk.** It drifts. Keep it short enough that drift is visible.

---

### P-10 · Pin D10's order, and stop the fuzz corpus shrinking

**Priority** Low · **Kind** test strength · **File** `path.rs` tests,
`tests/path_validation_corpus.rs`

**Problem.** D10 says the path must pass *the eight checks in order*. The set is
tested; the order is not, so two checks could swap and every test would still pass —
even though the order decides which rejection a user is shown. Separately, the fuzz
corpus is a file: it could shrink and nothing would complain.

**Change.** A test that feeds one path which fails several checks and asserts *which*
rejection comes back, fixing the order for the cases where more than one applies.
And an assertion that the corpus holds at least as many entries as it does today.

**Contract impact.** None.

**Acceptance.** Swap two checks in the validator and the test fails. Delete a line
from the corpus and the test fails.

**Risk.** Low. The order assertions are a claim about behaviour, which is the point.

---

## 2. Checked, and already covered — do not re-litigate

These were examined and need nothing. Recorded so the next pass does not repeat the
work.

- **Leaf and ancestor symlink checks, with the resolved path re-run through the same
  guards** — present and tested, including a nonexistent leaf whose ancestor is the
  link (`path.rs`).
- **Empty-variable collapse**, matched against both the literal and the resolved
  parent — present, tested, and hardened by an earlier finding (a symlinked parent).
- **The whitelist is matched in both forms and can only tighten** — present, tested,
  including the empty-rule case that would otherwise protect everything.
- **The home directory's structural fallback** keeps working with `HOME` and
  `USERPROFILE` unset — present, tested. P-3 extends it; it does not replace it.
- **Windows `\\?\` folding** for the verbatim path `canonicalize` returns — present
  and unit-tested on macOS (`windows_path_key`).
- **Re-verification between plan and execution** — path re-validation, identity
  recomputation, and activity, with a `Skipped` result and a stated reason for
  anything that moved (`cleanup.rs`). This is the same guarantee disktree reaches by
  re-running its guards on the removal thread.
- **Fail closed on failure** (D3) — the Git runner's injectable transport fails every
  call in turn and each path yields `cannot_verify` at `Review`, never a positive
  reason.
- **Read-only scanning** (D1) — `GIT_OPTIONAL_LOCKS=0`, `GIT_TERMINAL_PROMPT=0`, stdin
  from `/dev/null`, and `GIT_DIR` / `GIT_WORK_TREE` / `GIT_INDEX_FILE` removed from
  the environment.
- **Serial execution with isolated failures** (D14) and a log carrying the six
  fields with no network call anywhere (D15) — present.
- **The firmlink false alarm.** I suspected that a legitimate project path might
  canonicalize to `/System/Volumes/Data/...` and be refused as "under `/System`".
  **measured** — it does not: `realpath('/Users/andy')` is `/Users/andy` and
  `realpath('/Users/andy/MySrc')` is `/Users/andy/MySrc`. Nothing to fix. This is
  why the Data-prefix fold is the optional half of P-1.
- **A name list as a decision basis.** disktree classifies directories by
  name (`.claude`, `.codex`, `worktrees`, `tries`, `target`) — but only to choose a
  colour, and it corroborates a name before trusting it: `target` is build output
  only beside a `Cargo.toml`. That is our D7, arrived at independently, and it is
  worth keeping in mind as an outside confirmation that "the name is a candidate,
  the evidence is the verdict" is a workable rule rather than a pedantic one.

---

## 3. Deliberately not proposed

- **A permanent-delete path or a walk of our own.** disktree implements
  `rm -rf --one-file-system` semantics itself, opening each directory relative to its
  parent with `O_NOFOLLOW` and unlinking through file descriptors. It is careful
  work, and it is precisely what D5 forbids here. Should the contract ever be
  reopened, that module is the reference for what the change costs; until then,
  proposing it would be proposing to weaken the product's central promise.
- **Going faster on large trees.** Parallel scanning, reading a whole NTFS volume
  from its file table, bulk directory listing, mount IDs from `statx` — all of it
  serves one question: "what is filling this disk?" We scan one repository, on
  request, while a person watches. None of it earns its complexity here.
- **A treemap.** The UI is frozen and the information architecture already answers
  "where did it go" with sizes and reasons.
- **Touching Linux.** D16 names macOS and Windows; disktree's third platform is out
  of scope.
- **Replacing the system Trash with an XDG layout.** disktree's fallback chain exists
  because Linux has no single Trash; macOS and Windows each have exactly one, and a
  second implementation would be a second thing to verify.

---

## 4. Outcome

Settled on 2026-10-12.

| ID | Proposal | Outcome |
|---|---|---|
| P-1 | Case-fold the protected-location comparison; add the canonical spellings | **Implemented** — `guard_key` in `path.rs`, both spellings of every symlinked location in the list, the home-root shape folded, the case variants and `/private/var/db` added to the corpus |
| P-1.5 | …including the optional `/System/Volumes/Data` fold | **Rejected** — nothing produces that spelling (`realpath` keeps `/Users/andy` as it is), and a hand-written one is already refused by the `/System` entry |
| P-2 | Refuse names Win32 reads as a different name | **Implemented** — `win32_ambiguous_name`, a pure predicate applied on Windows only, with its name table tested everywhere |
| P-3 | Match the home directory by identity as well | **Implemented for Unix** (`device`/`inode`); Windows waits for the D16 verification, because a stable file id needs an API `std` does not expose — the spelling and structural guards there are unchanged, not weakened |
| P-4 | `--` before paths; write down the residual window | **Implemented** — the separator is in the executor and in the plan text, and the window is written down in product plan §6.5 |
| P-5 | Commit and finish the desktop wiring | **Already landed** before this review |
| P-6 | Define released space; label the simulation | **Implemented** — the rule is in DEV_CONTRACT §3 and the pages say when there is no desktop. Note the preview no longer simulates at all: it refuses (see the P-5 resolution) |
| P-7 | Field-for-field contract test | **Implemented** — `src/architecture.test.ts` compares ten contract structs and the reason-code list across the two languages |
| P-8 | Lint policy, pinned toolchain, formatter | **Implemented** — `[workspace.lints]` with `clippy::all` and `clippy::pedantic` denied, plus `rustfmt.toml`, `rust-toolchain.toml` and `pnpm lint:rust`. The triage is done: 332 findings sorted into ten lints allowed with a written reason, and the rest fixed or applied mechanically. `clippy::nursery` stays a warning because those lints are experimental |
| P-9 | `AGENTS.md` routing table | **Implemented** |
| P-10 | Pin the check order; keep the corpus from shrinking | **Implemented** — a precedence test that applies two rules at once, and a corpus floor at the current entry count |

**Suggested order (as written).** P-5 first, because it was in flight and everything
else about the desktop form assumed it; then P-1 and P-8, one to close a hole and one
to keep holes from reopening; then P-6, P-4 and P-7; then P-3, P-2 and P-10; P-9
whenever. It is kept because the implementation followed it, and because the reasoning
is worth more than the order: a hole first, then the thing that keeps it closed.
