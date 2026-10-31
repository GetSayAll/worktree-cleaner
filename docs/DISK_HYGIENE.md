# Disk Hygiene Policy

> Translations: [简体中文](./DISK_HYGIENE.zh-CN.md)

A build artifact must not outlive the workspace that produced it. A worktree must
not outlive the task that created it.

Generated things belong next to the thing that generates them. Worktrees and build
caches live inside the repository that owns them, so that removing the repository
removes everything it produced — no orphans, no second place to look.

Applies to every repository on the machine and to every actor working in one, human
or automated.

---

## 1. Layout

**L1 — Worktrees live inside the repository.**
A worktree MUST be created at `<repo>/.worktree/<name>`. Nothing sits beside the
repository, and there is no machine-wide worktree root. The directory is ignored by
Git, so it never appears as a change.

> **The known hazard.** Because `.worktree/` is ignored, `git clean -x` inside the
> repository would delete every live worktree at once, together with uncommitted work
> that exists nowhere else. That command is therefore forbidden outright — see L6.
> This is the cost of cohesion; the alternative, a global root, is a directory nobody
> remembers to look at.

**L2 — One clone per repository.**
A repository has exactly one canonical clone. Variants are worktrees, not
additional clones. A second top-level directory pointing at the canonical clone
MUST be relocated or removed.

**L3 — No nesting.**
A worktree MUST NOT contain another worktree. `.worktree/<name>/.worktree/` is a
violation, not a deeper level of organisation.

**L4 — Naming.**
`<repo>-<branch-slug>-<YYYYMMDD>`, lowercase, hyphen-separated, ≤ 60 characters.
Identifiers internal to a tool (session ids, hashes) belong inside the name, never
as a parent directory.

**L5 — Ignore entries are part of the layout.**
A repository that hosts worktrees MUST have both paths in its `.gitignore`:

```
/.worktree/
/build-cache/
```

The leading slash anchors both patterns to the repository root and is not optional.
Without it, `.gitignore` matches those names at any depth, so a worktree that runs its
own build quietly creates a second `build-cache/` inside itself — invisible to Git,
outside the accounting, and a copy of B2 that B2 cannot see.

These entries are committed, not local exclusions: an ignored path that only exists
in one developer's `.git/info/exclude` will be committed by everyone else.

**L6 — `git clean -x` and `git clean -X` are forbidden. Everywhere, in every form.**
Both flags delete ignored paths, and `.worktree/` and `build-cache/` are ignored. The
command that reads as "clear the build output" therefore also removes every live
worktree in the repository — including uncommitted work that exists in no other
place, and the worktree's Git metadata along with it. There is no undo.

Forbidden forms include `-x`, `-X`, and any combination carrying either: `-xdf`,
`-fdX`, `--force --ignored`, and shell aliases that expand to them. Narrowing the
command with `-e` is not an acceptable substitute: it protects only the paths someone
remembered to list, and it must be repeated correctly every time, by every tool,
forever. A rule that depends on remembering an exception is not a rule.

| Goal | Use |
|---|---|
| Remove untracked, non-ignored files | `git clean -fd` |
| See what would be removed first | `git clean -nd` |
| Clear build output | delete the slot directly — `build-cache/<name>/` or `build-cache/main/` |
| Remove one worktree | `git worktree remove <path>` |
| Trim stale Git metadata | `git worktree prune` |

`git clean` is not commit-adjacent: no hook fires, and no scan can detect what it has
already destroyed. The only hard gate is a shell wrapper that refuses the flags. Where
that wrapper is not installed, this rule is discipline — and should be reviewed as
such whenever a change touches a cleanup script.

**L7 — The declared paths are excluded from every indexer.**
`.worktree/` holds a complete checkout per live worktree, and `build-cache/` holds a
build tree for each. Both MUST be excluded from the editor's file search and from
platform indexing. Left in, one search returns the same file fifty times and the
system index grows by everything the disk already has.

---

## 2. Lifetime

**T1 — Concurrency cap: 50 per repository.**
At most 50 registered worktrees per repository. Creating the 51st requires removing
one first. The cap counts registered worktrees, not directories.

**T2 — Merge closes the worktree.**
A worktree MUST be removed within 24 h of its branch reaching the mainline.
Removing it is part of finishing the change, not a separate cleanup task.

Removal is both halves: `git worktree remove <path>`, and deleting the matching
`build-cache/<name>/`. A cache left behind after its worktree is gone is an orphan
that no scan can attribute to anything.

**T3 — Idle expiry.**
No new commit and no uncommitted change for 7 days → `stale`, remove. With
uncommitted changes → `blocked`, report; do not remove unilaterally. `blocked` is a
delay, not an exemption: a worktree still dirty at the T4 ceiling is exported and
removed there.

**T4 — Absolute ceiling.**
No worktree survives 30 days in any state. Past that, its work MUST be pushed to a
branch or exported as a patch, then removed.

**T5 — Sessions clean up on exit.**
A worktree created by an automated session MUST be removed before that session
ends, unless the session explicitly hands it over by naming it and the next action.

> **Why 50 needs T3 and T4 to be real.** 50 is a working-set allowance, not a
> storage budget: at the 10 GB ceiling in B5 the theoretical worst case is 500 GB.
> Number limits alone therefore cannot protect the disk. Age limits do. If T3 and T4
> are not actually enforced, T1 will be reached and stop meaning anything.

---

## 3. Build output

**B1 — Inside a repository, output lives in declared locations only.**
Exactly two paths within a repository may contain generated files:

| Path | Holds |
|---|---|
| `<repo>/.worktree/` | worktrees themselves |
| `<repo>/build-cache/` | all build and test output |

Build output anywhere else inside the repository — the source root, `Sources/`, the
root of a worktree, a stray `.build` beside a package manifest — is a violation. The
rule does not forbid output from existing; it forbids it from existing *undeclared*.

Artifacts that live outside the repository by nature — release builds, distributable
binaries — are governed by C3 instead, and MUST NOT appear as a side effect of a
normal build.

**B1.1 — The test is `git status`.**
With the output present — immediately after a build or a test run —
`git status --porcelain` MUST NOT list a single generated path. Correctly placed
output is invisible to Git, because both declared locations are ignored; anything
generated that does appear in the listing is, by definition, outside them.

The check is about which paths appear, not about whether the tree is otherwise dirty.
A working tree carrying ordinary uncommitted edits is normal and proves nothing
either way — requiring an empty listing would fail on every healthy repository.

**B2 — One cache root, mirrored by worktree name.**
`<repo>/build-cache/<name>/` corresponds to `<repo>/.worktree/<name>/`, one to one.
Two slot names are reserved and correspond to no worktree: `main/` for the main
checkout, and `shared/` for the dependencies B3 shares across all of them. Tool-native
names may survive inside a slot (`build-cache/<name>/.build/`,
`build-cache/<name>/DerivedData/`), but there is exactly one entry point per repository
to audit, quota, and delete.

The mirroring does not happen by itself: no tool's default lands in the right place.
Every command that writes output MUST be pointed at its slot — through the tool's own
flag (`--scratch-path`, `-derivedDataPath`, `CARGO_TARGET_DIR`, `--virtual-store-dir`)
or through one variable, `WORKTREE_BUILD_CACHE`, exported by whatever creates the
worktree. This wiring is the part that has to be built; the rest of the policy assumes
it exists.

**B3 — Shared reads, isolated writes.**
Resolved dependencies — package checkouts, language stores, downloaded toolchains —
SHOULD be shared across all worktrees of a repository, because they are identical
and re-downloading them costs both time and space. Shared content lives in
`build-cache/shared/`. Compiled intermediates and link output MUST be isolated per
worktree, because concurrent writes to a shared build directory corrupt each other.

| Kind | Scope | Why |
|---|---|---|
| package store, resolved dependencies | shared | identical content, large, slow to fetch |
| object files, module caches, linked binaries | per worktree | concurrent writers would collide |

**B4 — Disposable by construction.**
Anything under `<repo>/build-cache/` MAY be deleted at any time without warning,
and no code may read from it as an input. A cache that cannot be deleted is not a
cache.

**B5 — Cap per worktree: 10 GB.**
Exceeding it indicates repeated clean builds rather than a large project, and is
reported as a finding.

---

## 4. Caches and artifacts

**C1 — Machine-level caches have a ceiling.**
Caches that cannot live in a repository — tool caches, package manager stores,
compiler caches under the home directory — SHOULD stay under 10 GB each. Where the
tool ships a prune command, use it; deleting the whole directory is the fallback for
caches that offer no selective reclaim. Wholesale deletion of a shared package store
forces every worktree to re-fetch, which is the cost B3 exists to avoid.

**C2 — Device and simulator images are disposable.**
Device filesystem captures and simulator images MUST NOT hold data that is not also
in a repository or an artifact store. Simulators unused for 30 days are deleted; at
most 2 kept per runtime.

**C3 — Release artifacts: keep 3.**
The current release plus the two previous. Older builds move to external storage or
are removed.

---

## 5. Quotas and gates

**Q1 — Free-space thresholds.**
Normal above 15% of the volume; warning between 10% and 15%; **frozen** below 10% —
no new worktree, cache entry or release artifact until back above 12%. The gap between
the freeze point and the thaw point is deliberate hysteresis: one shared threshold
would flap every time a build or an eviction moves the number across it.

**Q2 — Global caps.**
`<repo>/build-cache/` SHOULD stay under 10% of the volume per repository. The 10 GB
figure in B5 is a ceiling for one worktree, not an allocation: average size is what
T3 and T4 control.

When the cap is reached, evict before creating. Deleting the slot of an idle worktree
is always permitted under B4, and refusing a build someone actually needs is the worse
of the two outcomes.

**Q3 — Admission, not cleanup.**
Creating a worktree, a cache entry or an artifact requires passing T1, Q1 and Q2
first. A violation is refused at creation, not reported afterwards. Every rule here
is cheaper to enforce on the way in than to unwind on the way out.

---

## 6. Enforcement

| Moment | Mechanism | Rules |
|---|---|---|
| any command | shell wrapper refuses `git clean` with `-x` or `-X` | L6 |
| repo setup | anchored `.gitignore` entries committed; both paths excluded from editor search and platform index; output redirect wired | L5, L7, B2 |
| creation | admission check refuses on cap or threshold | T1, Q1, Q2, Q3 |
| session exit | teardown removes declared worktrees; leftovers are logged | T5 |
| worktree removal | `git worktree remove` plus deletion of the matching cache slot | T2 |
| before commit | `git status --porcelain` lists no generated path; no cache path force-added | L5, B1, B1.1 |
| scheduled scan | read-only report of per-repo counts, ages, sizes | L1–L4, T2–T4 |
| periodic | top consumers vs. previous period; cache and simulator inventory | B5, C1–C3 |

The scheduled scan reports; it does not delete. Deletion stays an explicit,
confirmed action with a reason attached to every path.

---

## 7. Exceptions

**X1 — An exception names the rule, the path, and the date it expires.**

An exception is valid only when written down: the rule waived, the path, the
reason, and the date it expires. Keep them in one file at the repository root —
`.worktree-keep`, one entry per line, so the same file serves as both the waiver and
the record of why the path is still there.

The line is tab-separated, because a path may contain spaces:

```
<path>	<rule[,rule…] | *>	<YYYY-MM-DD>	# why this path stays
```

All four fields are required. `*` waives every rule for that path; a rule list
waives only the rules it names, so an accepted violation of B1 does not silently
excuse a T4. The date is the day the waiver stops applying — the path is then
reported like any other.

An exception without an expiry date is not an exception — it is the old behaviour
with extra steps. Any rule waived twice for the same reason is either wrong or
under-specified; fix the rule instead of adding a third exception.

---

## 8. How this policy is checked

Worktree Cleaner audits one repository at a time against the rules above and
reports each one as met, worth fixing, broken, or **not checked**. It is read-only:
it reports, it never deletes, and a finding never changes whether something is safe
to delete.

| Rule | What the audit reads |
|---|---|
| L1, L3 | the registered worktree paths, against `<repo>/.worktree/` and against each other |
| L4 | each worktree directory name: slug, length, date suffix, repository prefix |
| L5 | `git check-ignore -v` for both declared paths: present, anchored, and in the repository's own `.gitignore` |
| T1 | the number of registered worktrees, warned at 90% of the cap |
| T2 | worktrees whose branch is already merged into the default branch |
| T3 | the newer of the HEAD commit time and the directory's creation time |
| T4 | the directory's creation time |
| B1 | every recognised artifact directory, against `<repo>/build-cache/` |
| B1.1 | the scan's own Git facts: tracked, or ignored |
| B2 | the slots under `build-cache/` against the registered worktrees, and for a second `build-cache/` inside a worktree |
| B5 | the size of each slot |
| Q1, Q2 | the volume's free and total space |
| X1 | `.worktree-keep`, field by field |

Reported as **not checked** rather than as met: L2 (a second clone is outside the
scanned path), L6 (a command that has already run), L7 (editor and platform index
configuration), T5 (a session's exit), B3 and B4 (how a build uses the cache), Q3
(admission happens at creation time), and C1–C3 (machine-level, outside any one
repository). A scan that cannot see a rule says so; silence is not compliance.

The prevention half lives on the same page. Every rule carries the wiring that
makes it true — the two ignore entries, the cache-slot variable, the shell wrapper
that refuses `git clean -x` / `-X`, and the two commands that close a worktree and
delete its slot — so the page can be worked through before the first worktree is
created rather than after the disk is full.
