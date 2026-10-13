# Worktree Cleaner Product Plan

A cross-agent Git worktree and build-artifact safety audit tool. Read-only scanning by default; before deletion it gives verifiable reasons, and deletion goes through the system Trash while also cleaning up Git worktree metadata.

Platforms: macOS, Windows. Tech: Vue 3 + Tauri 2 + Rust (worktree-core).

---

## 1. Product positioning

**One-liner**: let developers dare to delete the worktrees and build artifacts on their own machines that they "don't know who created, and don't know whether they can be deleted."

**Three real problems to solve**

1. Codex / Claude Code / Cursor / WorkBuddy / Aider each create their own worktrees, scattered next to the repository, and nobody remembers which one is still useful. `git worktree list` only shows paths and HEAD; it can't tell you "whether this one can be deleted."
2. Build artifacts (`node_modules`, `target`, `dist`) easily run to several GB, but one wrong deletion means re-downloading or recompiling; some directories have names that look like artifacts but have actually been committed into the repository, so deleting them loses work.
3. Existing tools are either pure command line (like `mo purge`) with no GUI, or "one-click cleanup" that takes the judgment away — users can't see the deletion reason and don't dare to use it.

**Core invariants (spanning all features)**

- A deletion decision must rest on a verifiable piece of evidence (command output, path, count), not "looks like garbage."
- If evidence can't be obtained, treat it as "unknown", and unknown is never judged safe to delete.
- Never permanently delete user directory contents; everything goes to the system Trash.
- The main working tree can never be deleted.

**Target users**: intermediate and advanced developers who use 2 or more AI coding agents at the same time; their own machines, with sufficient local permissions, and no enterprise centralized control.

**Delivery form**: no pure web version. There are three delivery forms — "read-only report → local daemon + browser → Tauri desktop shell" — defined in Section 10, with phasing in Section 8.

---

## 2. Core concept: three safety levels

All worktrees and build artifacts fall into one of three levels, and the level is derived from the "reason list", not from an independent score.

| Level | Meaning | UI presentation | Selectable by default |
|---|---|---|---|
| **Safe** (safe to delete) | all checks pass, with positive evidence (merged / already gitignored) | green badge | yes, checked by default |
| **Review** (needs confirmation) | not dangerous, but with uncertain factors (unmerged, activity within the last 7 days, read failure) | amber badge | yes, but **not** checked by default; the user must check it explicitly |
| **Unsafe** (must not be deleted) | hits a hard prohibition condition (main working tree, uncommitted changes, unpushed commits, Git-locked, already tracked) | red badge | no, checkbox disabled |

One rule: **the level is determined by the most severe reason in `reasons`**. Wherever the UI displays a level, the reasons must be expandable right next to it.

---

## 3. Information architecture and pages

Fixed left-side navigation + top project bar.

```
Top bar: project path · scan time · three-level counts · [Rescan]
Sidebar: Overview / Worktree / Build artifacts / Cleanup plan / History / Settings
```

Before a scan, only "Welcome" in the sidebar is available; data pages appear only after the scan completes.

### 1. Welcome / project selection

- Path input field + "Choose folder" (Tauri dialog plugin); supports dragging a folder into the window.
- "Recent projects" card list: path, last scan time, summary of the last result (N items cleanable / X releasable), three-level dots.
- Three capability description cards: read-only scan, verifiable reasons, Trash deletion.
- Default behavior on first use: automatically list repositories under common directories such as `~/MySrc`, `~/dev`, `~/Projects`, `~/Code`, as a fallback for "Recent projects".
- An explicit read-only promise: the scan phase writes no files.

### 2. Scan progress

- Stage list (checked off item by item): parse Git worktrees → check each worktree's status → scan build artifacts → summarize.
- Each stage shows the currently processed path in real time (single-line scrolling); no progress bar percentage (the number of directories is unknown in advance, and a fake progress bar lies).
- Elapsed time; a "Cancel" button (after canceling, the already-completed portion is still viewable).
- Degradation notice area: worktrees whose check times out or can't be read due to permissions are listed here, with a note that the entry will be judged "unknown".
- Timeout strategy for long-running stages: a per-directory probe timeout plus an overall budget for the whole scan; when the budget is exhausted, stop and truthfully report "not fully scanned", without silently filling in the gaps.

### 3. Overview

- Three large cards: safe to clean (count + releasable space), needs confirmation (count), must not be deleted (count).
- Main number: **estimated releasable space** (the sum of checked Safe + Review items). It is an estimate and is presented as one: items whose size could not be measured contribute nothing to it and are counted separately, so any total that includes one is shown as a lower bound (`≥`).
- Two summary lists: the 5 largest worktrees and the 5 largest build artifacts; each row includes a level badge and a one-line primary reason, and clicking jumps to the corresponding page.
- Scan warning area (if any).
- Primary button: "Generate cleanup plan" — disabled with an explanation when nothing is checked.

### 4. Worktree list

Table columns: checkbox / branch / status (clean · dirty N · locked) / Agent / last activity / size / safety level / expand.

Expanded detail panel:

- Full path, short HEAD SHA, whether detached.
- Relationship to upstream: ahead / behind; when there is no upstream, explicitly write "no upstream branch" rather than leaving it blank.
- Whether merged into the default branch (the default branch name is displayed and can be changed).
- stash count, untracked file count.
- The inferred owning agent (identify `codex` / `claude` / `cursor` / `aider` / `workbuddy` by path features; show "unknown" when it can't be identified — don't guess).
- **Reason list**: one line per reason; green is positive evidence (merged / clean), amber and red are risk items, each followed by its evidence text.

Top: search box + filter (All / Safe / Review / Unsafe / Locked / Unmerged).

Bottom floating action bar: `N selected · X to free` → "View cleanup plan".

### 5. Build artifacts

- Grouped by owning worktree / project root, collapsible; the group header shows the group's total size.
- Each row: type badge (`node_modules` / `target` / `dist` / `.venv` …), relative path, size, file count, last activity time, Git status (ignored / untracked / tracked), safety level.
- Sorting: by size (default) / by last activity.
- Size threshold slider: by default, show only directories larger than 10 MB (small directories are noisy).
- Relationship to worktrees: if the worktree containing an artifact has already been checked for deletion, that artifact row shows "removed together with the worktree" and its independent checkbox is automatically cleared, to avoid double-counting size.

### 6. Cleanup plan (confirmation page — the most important page in the product)

Split "what will be done" into two channels shown separately, each landing on a concrete command or a concrete path:

**Channel A: remove worktrees**

- Each entry shows: path, branch, the command to be executed (`git worktree remove <path>`).
- Entries that need `--force` (untracked files present) are marked separately, and the **list of untracked files that will be force-deleted** is listed line by line — this is what users most need to see.
- Lost worktrees (directory no longer exists) go through `git worktree prune`, with a note that only metadata is cleaned up and no files are touched.
- Undeletable entries do not appear here, but the footer shows "N undeletable items automatically excluded, view".

**Channel B: move to Trash**

- Each entry shows: path, size, file count, whether recoverable (the Trash is recoverable, with a note on "recovering from the Trash").
- Sorted by size descending.

**Precondition verification (dry run)**

Before execution, walk through each item; if one fails, show the blocking reason at the top rather than erroring out only at execution time:

- Path verification: non-empty absolute path, symlink resolution (including ancestors), reject `..` components, reject system root directories.
- Whitelist match check.
- Duplicate deletion check (a worktree and an artifact inside it are both selected).
- The target still exists and its identity is unchanged (record inode / size when the plan is generated; re-check before execution; if inconsistent, require a rescan).
- Target activity re-check: for a directory judged "safe" when the plan was generated, re-check before execution whether there was new writing within the last 7 days; if it has become active, suspend that item and ask the user to confirm.

**High-risk confirmation**

- When `--force` appears, or the number of independently deleted items reaches a threshold (default 5), or the space released in a single run exceeds a threshold (default 10 GB), the user must manually check "I have confirmed the contents listed above". Artifacts that disappear together with a worktree do not count towards the item threshold.
- Optional strong protection (a setting, off by default): the user must type the project directory name into the input field to enable "Execute".

**Export**: export the plan as plain text / JSON, for prior manual review or script-based auditing.

### 7. Execution result

- Per-item results: success / skipped / failed, each with its reason.
- Summary: space released, number of directories moved to the Trash, worktree metadata cleanup result. **Space released counts only items whose result was `Ok`, and only their measured bytes**: an artifact that left together with its worktree was not a separate move, a skipped or failed item freed nothing, and an item whose size was never measurable is counted as unknown rather than as `0 B`. The volume's free space is not read before and after, so nothing here claims to be exact.
- Failed items: provide copyable reason text and the corresponding command, for manual handling.
- Action buttons: "Open Trash", "View log", "Rescan".

### 8. History

- Reverse-chronological list: time, action (remove worktree / move to Trash), target path, reason summary, result, size.
- Filter: All / Worktree / Artifact; supports export.
- A fixed note: the log is recorded only on this machine and is never reported.

### 9. Settings

- **Scan scope**: additional scan paths (one per line), maximum scan depth, per-directory timeout, whole-scan budget.
- **Protection rules**: whitelist paths (matched directories are never deleted under any circumstances), protected branch names (the default branch is auto-detected and displayed by default), agent directories to protect.
- **Decision thresholds**: activity days (default 7), artifact display size threshold (default 10 MB), whether "untracked files" count as unsafe (counts by default).
- **Deletion method**: Trash (default and not changeable, with the note "this tool does not provide permanent deletion").
- Appearance: follow system / light / dark.
- Language: follow system / English / 中文. English is the source locale; the interface follows the system language by default and can be switched manually.

---

## 4. Data contract

Frontend types stay consistent with the Rust-side `serde` output: enum unit variants serialize to `"Safe" | "Review" | "Unsafe"`, and fields use snake_case.

The existing structures in `crates/worktree-core/src/lib.rs` need additional fields; the signature of `scan_project` is unchanged.

```rust
pub enum SafetyStatus { Safe, Review, Unsafe }

pub enum ReasonCode {          // serializes to strings like "wt.dirty"
    WtDirty,
    // …all reason codes are in the Section 5 decision table, 26 in total
}

pub struct Reason {
    pub code: ReasonCode,      // stable identifier, serializes to "wt.dirty"
    pub severity: SafetyStatus, // the most severe level this reason leads to
    pub text: String,          // human-readable
    pub evidence: String,      // verifiable evidence: command output summary / count / path; must not be empty
}

pub struct SafetyAssessment {
    pub status: SafetyStatus,  // determined by the most severe entry in reasons
    pub reasons: Vec<Reason>,
}

pub struct WorktreeRecord {
    pub path: String,
    pub branch: Option<String>,
    pub head: Option<String>,          // short SHA
    pub is_main: bool,
    pub is_detached: bool,
    pub is_locked: bool,
    pub is_prunable: bool,             // the directory is gone
    pub dirty_files: u32,
    pub untracked_files: u32,
    pub untracked_file_list: Vec<String>, // listed line by line when --force is needed
    pub ahead: u32,
    pub behind: u32,
    pub has_upstream: bool,
    pub merged_into_default: Option<bool>, // None = cannot determine
    pub stash_count: u32,
    pub last_updated_at: Option<String>,
    pub agent: Option<String>,
    pub size_bytes: Option<u64>,   // None = cannot measure (no permission / timeout)
    pub safety: SafetyAssessment,
}

pub struct ArtifactRecord {
    pub path: String,
    pub kind: String,                  // "node_modules" | "target" | ...
    pub ecosystem: String,             // "node" | "rust" | "python" | ...
    pub project_root: String,
    pub worktree_path: Option<String>, // the owning worktree; Some(root) for the main working tree
    pub size_bytes: Option<u64>,       // None = cannot measure (timeout / no permission / symlink)
    pub file_count: u64,
    pub last_updated_at: Option<String>,
    pub git_ignored: Option<bool>,
    pub git_tracked: Option<bool>,
    pub is_symlink: bool,
    pub symlink_target: Option<String>,
    pub safety: SafetyAssessment,
}

pub struct ProjectSnapshot {
    pub schema_version: u32,           // contract version, see Section 10.5
    pub root: String,
    pub scanned_at: String,
    pub default_branch: Option<String>,
    pub worktrees: Vec<WorktreeRecord>,
    pub artifacts: Vec<ArtifactRecord>,
    pub warnings: Vec<String>,         // scan degradation notes
}
```

`is_prunable` must be distinguished from "the directory exists": `is_prunable` means the Git metadata exists but the working directory is gone.

---

## 5. Check logic

### 5.1 Worktree checks

All data sources are read-only Git commands; the `.git` file format is never parsed:

- `git worktree list --porcelain` — path, HEAD, branch, `locked`, `prunable`, bare; the first entry is the main working tree.
- `git -C <wt> status --porcelain=v2 --branch` — number of uncommitted changes, number of untracked files, ahead/behind, upstream branch.
- `git -C <wt> merge-base --is-ancestor <wt-head> <default>` — whether merged (exit code 0 means yes, 1 means no, anything else means cannot determine).
- `git -C <wt> stash list --format=%gd` — stash count.
- The later of the HEAD commit time and the directory mtime is taken as "last activity".
- `git -C <wt> ls-files --others --exclude-standard` — list of untracked files (shown when `--force`).

Decision table:

| Reason code | Condition | Level | Evidence text |
|---|---|---|---|
| `wt.main` | is the main working tree | Unsafe | first entry of `git worktree list` |
| `wt.locked` | has `locked` in porcelain | Unsafe | lock reason (if any) |
| `wt.dirty` | uncommitted changes > 0 | Unsafe | `N files with uncommitted changes` |
| `wt.unpushed` | ahead > 0 | Unsafe | `N commits not pushed` |
| `wt.detached` | detached HEAD with no upstream | Review | `HEAD is detached` |
| `wt.no_upstream` | no upstream and not merged | Review | `branch has no upstream and is not merged into main` |
| `wt.unmerged` | not merged into the default branch | Review | `not merged into main` |
| `wt.untracked` | untracked files > 0 | Review (configurable to Unsafe) | `N untracked files` |
| `wt.stash` | stash > 0 | Review | `N stash entries` |
| `wt.recent_activity` | last activity < activity days | Review | `last activity: 3 days ago` |
| `wt.agent_active` | a `.git/index.lock` exists in the directory | Review | `index.lock present, possibly in use` |
| `wt.cannot_verify` | any check timed out / no permission / command failed | Review | `cannot read status: <reason>` |
| `wt.whitelisted` | matches the user whitelist | Unsafe | `matches whitelist: <rule>` |
| `wt.prunable` | the directory is gone | Safe | `working directory no longer exists, cleaning up metadata only` |
| `wt.merged` | merged into the default branch | Safe (positive) | `merged into main` |
| `wt.clean` | no changes, no untracked files | Safe (positive) | `working tree clean` |

The necessary and sufficient condition for **Safe**: not the main working tree, not locked, no uncommitted changes, ahead = 0, merged into the default branch (`merged_into_default == Some(true)`), stash = 0, no sign of activity, no `cannot_verify`. If any one is not satisfied, downgrade.

**It is not allowed to treat `merged_into_default == None` (cannot determine) as merged.**

### 5.2 Build artifact checks

The artifact type list is grouped by ecosystem (editable in settings):

- Node / JS: `node_modules`, `dist`, `build`, `.next`, `.nuxt`, `.output`, `.turbo`, `.parcel-cache`, `.angular`, `.svelte-kit`, `.astro`, `.expo`, `coverage`
- Rust / system: `target`, `.build`
- Python: `venv`, `.venv`, `__pycache__`, `.pytest_cache`, `.mypy_cache`, `.ruff_cache`, `.tox`, `.nox`
- JVM: `build`, `.gradle`, `obj`
- Mobile: `DerivedData`, `Pods`, `.cxx`, `.dart_tool`
- Other: `vendor`, `.zig-cache`, `zig-out`, `.terragrunt-cache`, `bin` (requires .NET context evidence)

Decision table:

| Reason code | Condition | Level | Evidence text |
|---|---|---|---|
| `af.git_tracked` | files tracked by git exist in the directory | Unsafe | `N files committed to the repository, not a pure artifact` |
| `af.nested_git` | a nested `.git` exists in the directory | Unsafe | `the directory contains a standalone repository` |
| `af.authored_content` | a directory with a generic name such as `dist/` holds files that are neither tracked nor ignored | Unsafe | `N files inside are neither tracked by Git nor ignored; they look like authored content` |
| `af.credentials` | a credential-looking file (`.env`, `id_rsa`, `*.pem`, `credentials.json`, …) sits anywhere inside the directory, at any depth | Unsafe | `<relative path> is inside; deleting the directory would delete it too` |
| `af.symlink_outside` | the directory is a symlink and points outside the project root | Unsafe | `points to <target path>` |
| `af.whitelisted` | matches the whitelist | Unsafe | `matches whitelist: <rule>` |
| `af.recent_activity` | a write occurred within the recent activity days | Review | `last activity: 3 days ago` |
| `af.cannot_verify` | a Git call failed, or the size computation or directory listing did not complete, or the credential walk hit its entry bound | Review | `could not complete the check` |
| `af.empty` | the directory contains no files **and the size measurement completed** — an unmeasurable directory also reports a file count of zero, and "could not measure" is not "empty" | Safe (positive) | `it contains no files` |
| `af.git_ignored` | the directory as a whole is ignored by gitignore | Safe (positive) | `ignored by .gitignore` |
| `af.rebuildable` | matches a known rebuildable artifact type | Safe (positive) | `can be rebuilt via npm install / cargo build` |

The necessary and sufficient condition for **Safe**: matches a rebuildable type, is gitignore-ignored or untracked, has no tracked files, no nested repository, no credential files, is not an out-of-bounds symlink, had no writes within the recent activity period, and the checks completed fully.

**Key counterexample (must be written into the tests)**: a directory named `build/` must never be deleted if it contains files tracked by git — some projects commit build artifacts into the repository, so deleting it is losing source code. An artifact name is only a "candidate", not a "criterion."

Also: the artifact directory itself must never be treated as a "project root" and expanded for scanning. Treating `node_modules` as a project root would delete the package's internal `dist/`, leaving a state where "npm thinks the dependency tree is intact but the package is actually broken", and recovery requires re-installing over the network. This must be hard-excluded in worktree boundary detection.

**The credential walk is bounded, and the bound is a review rather than a pass.** The check descends into the directory rather than reading only its top level, because a secret is as ordinary in `config/` as in the directory itself. A large `node_modules` is tens of thousands of entries, so the walk stops at a fixed entry count (50,000, comfortably above an ordinary dependency tree). Crossing that bound produces `af.cannot_verify` at `Review` with the count it reached, and never "nothing found": a partial walk has not established that the directory is clean. The bound must stay high enough to cover a real dependency tree — set it too low and the check silently stops protecting the single directory kind it matters most for.

### 5.3 Fail closed

Any failure during the scan must raise the level, never lower it:

| Failure case | Handling |
|---|---|
| git command timed out / non-zero exit | add `wt.cannot_verify` to that worktree, judge it Review |
| no permission to read a directory | record the size as unknown (show `unknown` instead of `0`), add `af.cannot_verify` |
| size measurement timed out | show `≥ X MB (scan incomplete)`, add `af.cannot_verify` |
| failed to determine the default branch | `merged_into_default = None`; all entries that depend on merge status are judged Review |

---

## 6. Cleanup logic

### 6.1 Execution order and deduplication

1. **Remove worktrees first, then handle artifacts.** Artifacts inside a worktree checked for deletion are not deleted separately — `git worktree remove` already removes the entire directory.
2. Deduplication is done when the plan is generated: any artifact inside a "worktree to be removed" is moved out of the Trash channel and labeled "removed together with the worktree" in the UI, with its size counted only once.
3. Serial execution, no concurrent deletion; each item's result is recorded independently, so a single failure doesn't affect the rest.

### 6.2 Worktree removal

- Clean and merged: `git worktree remove <path>`.
- Untracked files present: `git worktree remove --force <path>`, and the untracked file list must have been shown line by line on the confirmation page; `--force` is used only after the user confirms.
- Directory already gone: `git worktree prune` (or fall back to prune after `git worktree remove` errors), cleaning up only the `.git/worktrees/<name>` metadata.
- After removal, always run `git worktree prune` once to finish up and clean up leftover metadata.
- **Do not call `rm -rf`**. When removal fails, keep the original directory and truthfully report the failure; do not fall back to a brute-force delete.

### 6.3 Artifact deletion: system Trash

- macOS fallback chain (try in order; move to the next tier only when the previous one fails):
  1. `/usr/bin/trash` (if present).
  2. Atomic overwrite-free rename into the Trash directory on the same volume (no cross-volume; when the volume differs, downgrade to the next tier).
  3. Finder AppleScript as a last resort.
  - Each step has a 30-second timeout; if any step times out or fails, record the reason and try the next tier; if all three tiers fail, report failure — never delete permanently.
- Windows: call the Recycle Bin API (`SHFileOperation` / `IFileOperation`), not `Remove-Item -Force` directly.
- Verify the path once before and once after deletion, to prevent the target from being swapped out between verification and deletion.

### 6.4 Path verification (the single entry point for all deletion actions)

Any deletion action must pass the following in order:

1. The path is non-empty and absolute.
2. If the path itself is a symlink, resolve the target and re-verify the target.
3. If any **ancestor** of the path is a symlink, resolve it and re-verify (used only to tighten, never to loosen).
4. Reject a `..` appearing as a complete path component.
5. Reject paths containing control characters, and — on Windows — a path component that platform would read as a different name: one ending in a dot or a space (Win32 strips it), one containing a colon (an alternate data stream), or a device name such as `NUL`, `CON`, `COM1` or `LPT1` (reserved in every directory). NTFS keeps such names when they arrive through `\\?\` or from WSL, so a directory listing can hand one to the delete call while Win32 opens something else.
6. After normalization, reject system and top-level roots: `/`, `/bin`, `/sbin`, `/usr` (except `/usr/local`), `/System`, `/etc`, `/var/db`, `/Applications`, `/Library`, `/Volumes`, `/Users`, and the user's home directory itself; on Windows, the corresponding `C:\`, `%WINDIR%`, `%ProgramFiles%`, and the user directory itself. Two properties of this check are part of it, both added after they were found to be missing:

   - **The comparison ignores case on platforms whose filesystems do**, and every entry whose parent is a symlink is listed under both spellings (`/var/db` and `/private/var/db`). `realpath` does not correct the case of a name it passes through, so `/SYSTEM/Library/CoreServices` reached the same directory as `/System/Library/CoreServices` and was accepted; one spelling of one directory had two answers, and the canonical one was wrong. Folding can only add refusals.
   - **The home directory is matched by identity as well as by spelling** (on Unix, by device and inode), so a link to it is refused as the home directory. A second route to the same directory is invisible to a string comparison, and the symlink checks do not cover it: the link sits above the scan root, which is the user's own choice of root.
7. Reject an empty path that would make the parent directory become a variable after deletion (if the concatenated path equals the expected parent directory, reject).
8. Whitelist check.

Item 6 also guards against accidents such as "a variable being empty during concatenation causing the path to collapse to the parent directory".

### 6.5 Pre-execution re-check (guarding against time-of-check/time-of-use gaps)

- The plan records each target's identity (path + size + modification time, optionally inode).
- Re-check the identity before execution; if it doesn't match, skip the item and require a rescan.
- For artifacts at the "Safe" level, re-run the activity check once before execution; if it has become active, suspend the item and hand it back to the user for confirmation.

**What this guarantees, and what it does not.** The re-check closes the gap between planning and execution for the facts the plan was built from — the path, the size, the modification time, the level, and the activity window. It does not, and cannot, close the window between the re-check and the system call itself. In that window two things are outside our control, and the contract states them rather than implying otherwise:

- **A leaf swapped for a symlink is moved as the link.** Both backends resolve the parent and act on the final name, so the link is what leaves; the directory it points at is untouched. This is a property of the backend we call, and moving a link is the intended behaviour for an artifact that is itself one.
- **A parent directory replaced in that window is not detected.** The validated path names a location, not an object, and a rename of the parent between validation and the move is invisible to us. What limits the damage is that the target must still be a `Safe` item at a path the user selected, and that directories go to the Trash rather than being destroyed.

### 6.6 Logging and traceability

- Each operation appends one local log entry: time, action, target path, reason summary, executed command, result, freed size, recoverable location.
- The log location varies by platform: on macOS `~/Library/Logs/Worktree Cleaner/operations.log`, on Windows `%LOCALAPPDATA%\Worktree Cleaner\logs\`.
- Logs are written only to this machine; no network, no reporting; settings offer "Clear log".
- The history page reads this log; failed entries are written too, for later troubleshooting.

---

## 7. Boundaries and non-goals

- **No permanent deletion**, and no switch to bypass the Trash.
- **Never delete the main working tree**, even if it looks clean.
- **Never execute cleanup automatically**; scanning and cleanup are always two explicit steps.
- **No cleaning of system caches, `~/Library`, the Downloads directory, or installers** — that's another product.
- **No modifying repository contents**: apart from `git worktree remove` / `prune`, no command that writes to the repository is executed; no automatic `git branch -d`, no automatic push.
- **No judging "whether this branch is still needed"**: report only verifiable facts (unmerged, unpushed, how many days idle) and leave the conclusion to the user.
- No multi-project batch cleanup; one project at a time.

---

## 8. Delivery order

| Phase | Content | Completion marker |
|---|---|---|
| P0 | worktree-core path verifier + reason code model + worktree checks | unit tests cover every `wt.*` reason code in the decision table; the fuzzing corpus is rejected in full |
| P1 | artifact scanning + boundary detection (including the "artifact is not a project root" counterexample test) | the case with a git-tracked file inside `build/` is judged Unsafe |
| P2 | CLI skeleton + snapshot JSON contract (`scan --json`) | the output conforms to the contract and passes schema validation; the repository has no changes after the scan |
| P3 | read-only report web page (Form A) | the single-file web page opens with no network; the data matches the JSON item by item; the page has no deletion entry point whatsoever |
| P4 | local cleanup execution + logging (the execution side of Form A) | on a real macOS machine, deleted items can be put back from the Trash; `git worktree list` shows no leftovers |
| P5 | local daemon + browser UI (Form B) | binds only to 127.0.0.1; requests without a token and cross-origin requests are rejected |
| P6 | wrap-up: settings persistence, history, export, Tauri desktop shell (optional) | settings survive a restart; history comes from the real operation log |

The definitions of the delivery forms and the responsibilities of each form are in Section 10; the executable acceptance criteria for each phase are in Section 5 of `docs/DEV_CONTRACT.md`.

These pages were built as a prototype covering only look and interaction, and that description no longer holds: the desktop shell runs the real scanner and the real executor over Tauri IPC, and `src/mock/` survives only for the browser preview, where every page using it is labelled as sample data. The Form A phase still switches the data source to a snapshot JSON dragged in by the user, and the Form B phase to HTTP requests.

The same pages are reused as the Form C desktop shell. There the data source is the Tauri IPC command backed by `worktree-core::scan_project`, and `src/mock/` serves only the browser preview, where it is always labelled as sample data.

---

## 9. UI prototype file inventory

The prototype has been implemented per this plan as directly runnable Vue 3 pages; in a plain browser all data comes from `src/mock/snapshot.ts`, while the Form C desktop shell reads real scan results over Tauri IPC and writes the operation log to disk.

```
src/
  App.vue                    shell: sidebar navigation + top project bar + view switching
  state.ts                   state and plan generation: view routing, selection set, cleanup plan, execution, history
  types.ts                   types aligned with the Rust serde output (including SafetyStatus / Reason / record structures)
  format.ts                  size, time, and path display
  styles.css                 design tokens and component styles (light / dark sets)
  mock/snapshot.ts           sample data: 6 worktrees, 14 artifacts, history records, settings defaults
  components/
    SafetyBadge.vue          three-level safety badge
    ReasonList.vue           reason list (sorted by severity, with evidence lines)
    StatCard.vue             statistic card
  views/
    WelcomeView.vue          welcome / project selection
    ScanningView.vue         scan progress and degradation notices
    OverviewView.vue         overview
    WorktreesView.vue        worktree list and expanded detail
    ArtifactsView.vue        build artifacts (grouped by worktree, size threshold, sorting)
    CleanupPlanView.vue      cleanup plan and confirmation (Channel A / Channel B / preconditions / export)
    ResultView.vue           execution result
    HistoryView.vue          history log
    SettingsView.vue         settings
```

The sample data deliberately covers every decision branch in the plan: the main working tree, a merged clean worktree, a worktree with uncommitted changes, a worktree that is unmerged and had activity within the last 7 days, a locked worktree, a worktree whose directory is gone, a git-tracked `build/`, a symlink pointing outside the project, and a `.nuxt` that times out during the scan.

Three ways to preview locally:

1. **Zero-dependency single file**: `dist/preview.html`, with CSS and JS already inlined; double-click to open it in a browser, no server needed.
2. **Dev mode**: run `pnpm dev` after `pnpm install`; default address `http://localhost:2430`, with hot reload.
3. **Regular build**: `pnpm build` (output in `dist/`, must be accessed over HTTP; double-clicking directly fails to load resources due to the file:// CORS restriction).

`dist/` is the build output directory; the single-file version is generated by inlining the build output and must be regenerated when the source changes (`pnpm build:preview`).

---

## 10. Delivery forms and roadmap

### 10.1 Conclusion

The core functionality cannot be accomplished in a pure browser, so there is no "pure web version". Three delivery forms are defined, to be pursued in order:

| Form | Description | Order |
|---|---|---|
| **A · read-only report** | a local CLI produces `snapshot.json`; a static web page is dragged in for viewing and decisions; the page never deletes any file | first |
| **B · local daemon + browser** | a local process listens on `127.0.0.1` and exposes an interface; the same web page, once connected over HTTP, gains execution capability | after A |
| **C · Tauri desktop shell** | a double-click-to-use desktop shell that internally reuses the same core and services | optional, last |

**Not doing**: a hosted SaaS form. Sending local repository paths, branch names, and unpushed commit information to the cloud directly conflicts with "logs are written only locally, no network" and with the product positioning; moreover, the deletion action must happen locally in any case.

### 10.2 Why a pure browser is not viable

1. **Cannot execute `git`**: the worktree list, merge status, and unpushed commit count are unavailable — all the base data the decision table depends on is missing, and the product would degenerate into "delete by directory name", exactly the approach this product explicitly rejects.
2. **Cannot write to the system Trash**: browsers have no Recycle Bin API, directly violating "directory deletion goes only through the Trash".
3. **`File System Access API` is not a solution**: only Chromium-based browsers support it; `removeEntry()` is permanent deletion with no Recycle Bin; obtaining worktree information would require rewriting git reading in JS (parsing `.git`, loose objects, and packfiles), which is both extremely costly and violates "do not parse the internal `.git` format"; and directory handles require the user to re-authorize every session.
4. **A web page cannot launch a local process**, and therefore cannot spin up a local service on its own.

### 10.3 Form A · read-only report

- **Data source**: the `snapshot.json` output by `worktree-cleaner scan --json <path>`.
- **Web page capabilities**: display three levels and reasons, filter, sort, size threshold, export the cleanup plan JSON. **Contains no deletion or write capability whatsoever.**
- **Differences from the UI prototype**: on the overview page and the plan page, "Execute cleanup" is replaced with "Export plan"; items in settings related to execution (strong protection, confirmation thresholds) serve only as plan export parameters, not as executable switches.
- **Delivery form**: a single-file HTML (assets inlined), openable with no network and no server. All data is provided by the user, so it can be hosted directly as a public page without leaking any information.
- **Typical uses**: manual review before cleanup; sending audit results to others; producing a snapshot as a build artifact in CI.

### 10.4 Form B · local daemon + browser

- A local process listens on `127.0.0.1` and exposes three endpoints, `scan` / `plan` / `execute`; opening the local address in a browser gives access to the same interface.
- **Reuse**: `worktree-core` is unchanged down to a single line; the Form A web page only switches its data source from "dragged-in JSON" to HTTP requests.
- **Security requirements** (all must be met; see D19 in `DEV_CONTRACT.md`): bind only to `127.0.0.1`; generate a random token on every start; strictly validate `Origin` and `Host` to prevent DNS rebinding; do not use CORS wildcards; destructive endpoints still require a secondary in-UI confirmation in addition to the token.
- **Cost**: the user must start the process themselves once (or configure auto-start at login). This is the only experience loss compared with Form C.

### 10.5 snapshot JSON contract

`snapshot.json` is the only interface between the CLI and the web page and must carry a version number:

```json
{
  "schema_version": 2,
  "root": "/Users/andy/MySrc/remote-mic-app",
  "scanned_at": "2026-10-11T09:12:00Z",
  "default_branch": "main",
  "worktrees": [],
  "artifacts": [],
  "warnings": []
}
```

- The web page can read a `schema_version` equal to or lower than the one it supports; on encountering a higher version, it must clearly warn "the viewer needs to be upgraded" and must not guess at fields.
- The remaining fields map one-to-one to `ProjectSnapshot` in `src/types.ts`.

