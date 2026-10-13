# Localized Result Copy and Operation Log — Plan

作者：Andy（hd838a@outlook.com）

This plan covers five defects observed in the desktop app, all of them in the
same area: what the result page says about what just happened, and what the
operation log records. Three of the five are text the reader cannot use, one is
a safety defect in the size walk, and one is a gap in what the log keeps.

Every claim below carries the file and line it came from, so each can be checked
before any of this is built.

---

## 1. What is wrong today

### 1.1 The result page prints Rust strings, not the message table

`ResultView.vue:126` renders `{{ item.detail }}`. `item` is a `ResultEntry`
(`src/types.ts:146-150`), whose `detail: string` comes straight from the Rust
`HistoryEntry.detail` (`crates/worktree-core/src/lib.rs:198`). That string is
written as a hardcoded English literal at every `push()` in
`crates/worktree-core/src/cleanup.rs` — for example:

- `cleanup.rs:396` — `"moved to the system Trash; recoverable from Trash"`
- `cleanup.rs:364` — `"the artifact changed since the plan was generated (was {}, is now {}); not executed"`
- `cleanup.rs:336` — `"the directory is no longer a recognised artifact kind; not executed"`
- `cleanup.rs:422` — `"git worktree prune finalised the metadata cleanup"`

The message table already has Chinese for some of these
(`result.detail.trashed`, `result.stats.movedToTrashSub`), so the translations
exist and are never called. Switching to Chinese does not change that column.

The same holds for the `reason` column: `primary_reason` /
`primary_reason_artifact` (`cleanup.rs:550-573`) copy `Reason.text`, which is
also an English literal in the scanner.

### 1.2 Internal struct debug output reaches the interface

`cleanup.rs:537-547` defines:

```rust
fn artifact_identity(record: &crate::ArtifactRecord) -> String {
    format!(
        "level={:?} size={:?} files={} tracked={:?} ignored={:?} symlink={}",
        record.safety.status, record.size_bytes, record.file_count,
        record.git_tracked, record.git_ignored, record.is_symlink
    )
}
```

`{:?}` on an enum and on an `Option` is the Rust debug form, and
`cleanup.rs:363-367` interpolates it into the user-visible `detail`. What the
reader sees:

```
the artifact changed since the plan was generated (was level=Safe
size=Some(1320274909) files=80703 tracked=Some(false) ignored=Some(true)
symlink=false, is now level=Review size=Some(0) files=0
tracked=Some(false) ignored=Some(true); not executed)
```

`Some(...)`, `level=`, field names — none of it means anything to the person
reading it, and `Some(false)` next to `size=Some(0)` reads as a statement about
the directory when it is a statement about a struct.

### 1.3 A directory that no longer exists is reported as "changed"

The execution phase re-checks every artifact before deleting it
(`cleanup.rs:342-387`) and compares six fields for exact equality:

| field | type |
|---|---|
| `safety.status` | enum, compared by debug string |
| `size_bytes` | `Option<u64>` |
| `file_count` | count |
| `git_tracked` | `Option<bool>` |
| `git_ignored` | `Option<bool>` |
| `is_symlink` | bool |

Any difference and the item is skipped (`cleanup.rs:357-372`). `walk_size_with_cap`
(`artifact.rs:967-970`) is what produces the size and file count:

```rust
while let Some(dir) = stack.pop() {
    let Ok(entries) = std::fs::read_dir(&dir) else {
        continue;
    };
```

A `read_dir` that fails — because the directory is gone, or unreadable — is
skipped silently. The walk finishes and reports **zero bytes and zero files**,
which is `Some(0)`, not `None`. So a directory that no longer exists becomes an
identity of `size=Some(0) files=0`, which never matches the recorded identity,
and the reason printed is "the artifact changed since the plan was generated".

The measured run that prompted this: an artifact recorded at `size=Some(1320274909)
files=80703` (1.29 GB, 80703 files) came back as `size=Some(0) files=0`. A
directory does not shrink from 80703 files to 0 on its own. What happened is
that it could not be read at all. The reader is told they changed something.

### 1.4 "Could not read" is recorded as "measured zero bytes" — safety defect

The scanner is careful about this elsewhere. `measure_dir` (`artifact.rs:941-946`)
returns `Measurement::unknown()` when the walk exceeds `per_dir_timeout`, so a
timeout yields `size_bytes: null`. And the emptiness test is explicitly gated on
the measurement having completed (`artifact.rs:654-667`):

```rust
// "Could not measure" is not "empty" (D3)
if !is_symlink && measurement.bytes.is_some() && measurement.files == 0 {
    reasons.push(Reason::safe(ReasonCode::AfEmpty, "the directory is empty", ...));
}
```

The timeout path is covered. **The `read_dir` failure path is not**: it returns
`Some(0)` with `files == 0`, which satisfies exactly the condition above. A large
directory the current user cannot read — `~/Library` inside another account, a
permission-restricted tree, anything under a protected root — is reported as
"the directory is empty" with an `af.empty` **Safe** reason. Safe means it is
selected by default and can be deleted without confirmation.

The intent of D3 is right and is implemented for the timeout case; the hole is
that `read_dir` failure is silently folded into "empty" instead of "could not
measure". Fixing this is not a copy change: it changes what the scanner is
allowed to call safe.

### 1.5 The log records deletions and nothing else

`log::append` has exactly one call site in the whole crate: `cleanup.rs:439`, at
the end of one execution. `HistoryAction` has three variants, all of them
deletions (`lib.rs:157-161`):

```rust
pub enum HistoryAction { WorktreeRemove, TrashMove, Prune }
```

So `HistoryEntry` can express "a thing was deleted, here is why, here is the
result". It cannot express anything a user did. Not recorded today:

- scan started / finished / failed / cancelled
- project switched
- cleanup plan generated
- selection changed (an item checked or unchecked)
- any settings change (whitelist, thresholds, deletion method, language, theme)
- strong-protection confirmation (typing the directory name)
- plan exported / copied

The history page shows `49` records in the run in question. Those are 49 deletion
results.

---

## 2. The rules these changes follow

**Every sentence on the interface is either a message-table string or a fact the
reader can act on.** The test applied to each existing string: could this
sentence change what the reader does next? If not, it does not belong on the
screen. This is the same rule already applied to the message tables
("界面不向用户解释内部实现"), extended to the strings that arrive from the core.

**Evidence stays verbatim.** Commands, paths, file counts, sizes, SHAs are data
about the user's own repository, not copy. They are not translated, not
reworded, and not localized — `git worktree remove` stays as it is. What changes
is that they are never *mixed into* a sentence.

**Only codes cross the language boundary.** This is the mechanism the fix rests
on. The core decides *what happened* and reports it as a stable code plus
arguments; the front end owns the wording. No English sentence is constructed in
Rust and displayed verbatim again.

**Raw values cross, not formatted values.** Sizes arrive as byte counts and are
formatted by `formatBytes` on the front end, so the unit follows the interface
language — the same rule as `size_bytes` today. Rust does not bake "1.29 GB" into
a string.

---

## 3. Reason codes already exist — only the table is missing

This part needs no Rust change at all. `ReasonCode` (`reason.rs:29-88`) is a full
enum with `#[serde(rename = "...")]`, 28 codes in two families:

- `wt.*` (16): `main`, `locked`, `dirty`, `unpushed`, `detached`, `no_upstream`,
  `unmerged`, `untracked`, `stash`, `recent_activity`, `agent_active`,
  `cannot_verify`, `whitelisted`, `prunable`, `merged`, `clean`
- `af.*` (12): `git_tracked`, `nested_git`, `authored_content`, `credentials`,
  `symlink_outside`, `whitelisted`, `recent_activity`, `cannot_verify`, `empty`,
  `git_ignored`, `rebuildable`

Each `Reason` carries `code`, `severity`, `text`, `evidence` (`reason.rs`), and
the code already reaches the front end (`src/types.ts` carries the same
`code: "wt.*"` shape). The front end simply renders `text` and ignores `code`.

**Change:** add a `reasons` section to both message tables keyed by code, and
render `reason.text` through it, falling back to the raw text for a code the
table does not know (so a newer core with an older UI still says something
true). `evidence` keeps rendering verbatim underneath.

Build entries for all 28 codes even though a few may never appear on screen: the
locale test that asserts both tables carry the same keys extends naturally to
this table, and a missing entry becomes a test failure rather than a silent
English string on a Chinese screen.

作者：Andy（hd838a@outlook.com）

---

## 4. Detail codes — the part that changes the contract

### 4.1 The shape

`HistoryEntry.detail: String` (`lib.rs:198`) and `ResultEntry.detail: string`
(`src/types.ts:150`) become:

```
detail_code: DetailCode   // stable id, e.g. "artifact.vanished"
detail_args: object       // raw values only: byte counts, counts, paths
```

`detail_args` carries **raw** values (`size_bytes_before: 1320274909`,
`file_count_before: 80703`) and the front end formats them through
`formatBytes` / the message table. The alternative — Rust formatting "1.29 GB"
into the argument — would freeze the unit to English the moment the core is used
with a Chinese UI, which is the defect this whole plan exists to remove.

### 4.2 The codes

Every `push()` in `cleanup.rs` becomes one code. From a read of the file:

| code | replaces | args |
|---|---|---|
| `worktree.removed` | `git worktree remove` succeeded | — |
| `worktree.remove_failed` | git error text | `error` |
| `worktree.pruned` | `cleanup.rs:422` | — |
| `worktree.prune_failed` | `cleanup.rs:431` | `error` |
| `prunable.metadata_only` | `cleanup.rs` prunable path | — |
| `artifact.trashed` | `cleanup.rs:396` | — |
| `artifact.trash_failed` | `cleanup.rs:405` | `error` |
| `artifact.not_a_known_kind` | `cleanup.rs:336` | — |
| `artifact.changed` | `cleanup.rs:364` | `field`, `before`, `after` |
| `artifact.no_longer_safe` | `cleanup.rs:380` | `reason_code` |
| `artifact.vanished` | new (§5) | — |
| `artifact.unmeasurable` | new (§5) | — |
| `precondition.failed` | front-end `messages.result.detail.preconditionFailed` | — |
| `bundled.with_worktree` | `cleanup.rs` bundled path | — |

`precondition.failed` already exists on the front end (`state.ts:711`); it
moves onto the same table so the column is rendered by one mechanism.

### 4.3 What this touches

- `lib.rs`: `HistoryEntry` fields, new `DetailCode` enum
- `src/types.ts`: `ResultEntry`, `HistoryEntry`
- `src/architecture.test.ts`: this file already asserts struct-field parity
  between the two sides (`:101` lists `ResultEntry` ↔ `lib.rs`); the new code
  table joins the existing `wt.*`/`af.*` assertion
- `ResultView.vue`, `HistoryView.vue`: render through the table
- `src/mock/snapshot.ts`: sample entries move to codes too, otherwise the browser
  preview shows the old shape
- The Rust integration test at
  `crates/worktree-core/tests/cleanup_execution_real_repo.rs:505` matches on
  `entry.detail.contains("moved to the system Trash")` and has to match on the
  code instead

---

## 5. The re-check needs more than "same or not"

Today the comparison has two outcomes: identical (proceed) or different (skip,
with one sentence for every cause). Four different situations collapse into that
one branch, and they call for different responses from the reader.

| outcome | condition | what the reader should be told | what to do |
|---|---|---|---|
| `unchanged` | all six fields equal | — | delete |
| `changed` | a field differs, and the directory was readable both times | **which** field, old → new: size / file count / now tracked by Git / safety level | judge, then rescan |
| `vanished` | `read_dir` returned `NotFound` | "this directory no longer exists; nothing to delete" | nothing — this is not a failure |
| `unmeasurable` | `read_dir` returned `PermissionDenied`, or the walk timed out | "could not re-measure this directory; it was left alone" | check permissions, then rescan |

Distinguishing them requires §6: today `NotFound`, `PermissionDenied` and "empty"
all arrive as `Some(0)`.

With that in place, `artifact.changed` carries the differing **field name** as an
argument and the interface names it in words — "the size changed since the plan
was made: 1.29 GB → 820 MB", "this directory is now tracked by Git" — instead of
printing two debug structs and asking the reader to diff them.

---

## 6. The walk must distinguish missing from empty

`walk_size_with_cap` (`artifact.rs:967-970`) currently discards the `io::Error`
kind. The change:

1. `read_dir` `NotFound` → record the directory as vanished, return
   `Measurement::vanished()`; do not report `0` bytes.
2. `read_dir` `PermissionDenied` (or any other error) → `Measurement::unknown()`,
   i.e. `size_bytes: null`, exactly as the timeout path already does. This alone
   closes the §1.4 hole, because `af.empty` requires
   `measurement.bytes.is_some()`.
3. Only a walk that actually read the directory may report `files == 0` as empty.

Effect on existing behaviour, stated plainly: artifacts that were previously
called "empty, safe to delete" because they were unreadable will now be reported
as unmeasurable and land in "Needs review". **That is a safety improvement and it
will change verdicts on some machines.** Any machine that sees more "needs
review" items after this change is seeing the old verdict being corrected, not a
regression.

Tests for this section:

- a directory removed between plan and execution → `vanished`, no delete, no
  "changed" claim
- a directory with mode `000` → `unmeasurable`, never `af.empty`
- a genuinely empty directory → still `af.empty` / Safe
- the existing timeout tests (`artifact.rs:1410`, `:1964`) must keep passing

---

## 7. The log: keep the audit trail, add the activity trail

### 7.1 Why two files

`operations.log` is an audit trail: it answers "what did this tool delete, when,
and what happened". Mixing "user changed a threshold" into that record degrades
both readings, and the deletion records are the ones that may matter later.
So the deletion log keeps its name, its shape and its meaning, and user actions
go to a second file.

```
~/Library/Logs/Worktree Cleaner/
  operations.log    // unchanged: deletions, one JSON object per line
  activity.log      // new: user actions, one JSON object per line
```

Both stay local-only, as `log.rs:4-7` requires.

### 7.2 The event stream

A new `ActivityEvent` struct and a new Tauri command, because the front end must
not write the log itself: the path logic lives in `log.rs` and the "local only"
guarantee is enforced there.

```
scan.started        { project, source: desktop|preview }
scan.finished       { project, worktrees, artifacts, duration_ms, complete }
scan.failed         { project, error }
scan.cancelled      { project, elapsed_ms }
project.switched    { from, to }
plan.generated      { worktrees, artifacts, total_bytes, ack_reasons }
plan.exported       { format, item_count }
settings.changed    { key, before, after }     // one event per changed key
cleanup.confirmed   { forced_items, typed_name: bool }
trash.opened        {}
```

`trash.opened` is included because a run is exactly when a user might want to
know they opened it; drop it if it turns out to be noise.

**Not recorded:** hovering, focusing, re-rendering, language and theme changes
while reading a page. The rule from §2 applies here too — if the reader would
never look it up, it does not get written.

**Frequency:** `selection.changed` is deliberately absent. A checkbox is toggled
dozens of times per run; a log of every toggle answers nothing. The selection as
it stood at plan time is already captured by `plan.generated`.

### 7.3 The history page

One view, two sources, interleaved newest-first, with the record kind shown. The
existing filters (`history.filterAll`, `filterWorktreeRemove`, `filterTrashMove`)
gain the activity categories; otherwise the new records would be invisible behind
"all".

### 7.4 Old records

`operations.log` lines written by earlier versions have `detail` as a string and
no `detail_code`. `read()` (`log.rs:78-85`) must keep parsing them: a record
without a code renders its text verbatim, marked as predating the codes. Losing
the existing history to a schema change would be a worse regression than the
English text ever was.

---

## 8. Order of work

Each phase stands on its own and can be reviewed on its own.

| phase | change | why in this position |
|---|---|---|
| 1 | Reason localization (§3) | no Rust, no contract change; removes the English from the reason column on both pages immediately |
| 2 | Walk semantics (§6) | a safety fix that is independent of any copy; do it before the detail codes so `vanished` exists to be reported |
| 3 | Detail codes (§4, §5) | needs §2's outcome distinction to be meaningful |
| 4 | Activity log (§7) | largest surface, no effect on what the result page says |

Phases 1 and 2 do not depend on each other and can be reviewed independently.
Phase 3 is the one that needs the most care: it changes a frozen contract, and
`architecture.test.ts` is the test that catches a drift.

---

## 9. Open questions

1. **A tracked artifact that reads as Safe.** In the same run, one skipped entry
   had `tracked=Some(true)` in its recorded identity while its level was `Safe`.
   A directory containing Git-tracked files is expected to be `af.git_tracked` /
   not-Safe. Whether `git_tracked` participates in the level decision, or is
   recorded and then ignored, needs checking — this is a verdict question, not a
   copy question, and it is not covered by this plan.
2. **Why the directory was gone.** §1.3 establishes that it could not be read.
   Whether it disappeared because an earlier step in the same run removed a
   worktree that contained it, or because the user removed it between the scan
   and the cleanup, is not yet established. If the answer is the first, the
   bundling logic is wrong and that is a plan-construction defect.
3. **Re-check depth.** The execution re-check passes `max_depth: 1`
   (`cleanup.rs:348`) while the scan uses the configured depth. The size walk
   does not read `max_depth`, so identity is unaffected, but the constant should
   be confirmed as deliberate.
4. **Whether every reason code needs copy.** §3 proposes all 28. If some can
   never reach the interface, dropping them keeps the table smaller — but then the
   key-parity test has to be told which ones are exempt.

---

作者：Andy（hd838a@outlook.com）