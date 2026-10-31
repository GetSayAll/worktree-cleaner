# Worktree Triage

The judgement behind "can this worktree go?", written down so it can be repeated
rather than re-derived. Tiers A and B are implemented in the core as `wt.merged`
and `wt.absorbed`; tiers C and D are deliberately not, and this document says why.
`docs/DEV_CONTRACT.md` §7 records the decision.

Everything below was measured on one developer machine on 2026-10-31 and
2026-11-01: 359 worktrees across 21 repositories, 389 GB, 95% of a 926 GiB disk.

## The one question

**Does this directory still hold value whose only copy is here?**

Build output does not count. Cache does not count. A commit that is already in the
default branch does not count. What counts is whatever would have to be rebuilt,
retyped or lost:

- uncommitted changes to tracked files;
- untracked files that Git does not ignore;
- commits that are not in the default branch.

Everything else on disk is a function of the source tree, and a source tree is a
function of a commit. Delete the directory and `git worktree add` gives it back.

That reduction is what makes the tiers below possible: they are all attempts to
answer "are the commits in the default branch?" with evidence rather than with a
guess.

## Tiers

| Tier | Condition | Command | Evidence for the merge question |
|---|---|---|---|
| **A** | `HEAD` is an ancestor of the default branch | `git merge-base --is-ancestor HEAD origin/<default>` | verifiable |
| **B** | every local commit's patch is already in the default branch | `git cherry origin/<default> HEAD` prints no `+` line | verifiable |
| **C** | every local commit touches only version or build stamps | read `git log --name-only origin/<default>..HEAD` per branch | judgement |
| **D** | the uncommitted changes are only stamps a build wrote | `git diff --name-only` | judgement |

The last column answers one question only — whether the commits are already in the
default branch. It is not a verdict. A worktree still has to pass the other checks
before it is `Safe`: it is not the main working tree, it is clean, it has no
untracked files, and nobody touched it inside the activity window.

A and B are verifiable: the answer is a command's exit status or its output, and it
is either true or it is not. C and D are judgements about whether a diff has value,
and a judgement must never reach `Safe` on its own — that is D2/D3 read plainly.

**Tier B is two questions, not one.** A matching patch-id proves the patch *was
applied*, not that it is still there: apply a change, revert it, and the patch-id of
the applying commit still matches while the default branch no longer holds the code.
So B also compares content — `git diff <head> <target> -- <the files this branch
touched>` — and requires the two sides to agree. A revert makes them disagree and the
branch stays `Review`. The comparison only covers the branch's own files, so unrelated
work on the default branch cannot make it fail, and it is strict on purpose: if the
default branch changed one of those files again after the merge, the answer is "cannot
confirm", which costs a re-read rather than a silent deletion.

Measured cost of getting this wrong: of 22 branches that survived A and B, **19
carried real code**. A stamp-only tail is the exception, not the rule. Tier C is
worth doing by hand on a repository family that generates build-number branches; it
is not worth encoding as a rule that promotes to `Safe`.

## Hard rules

1. **Deleting a worktree is not deleting a branch.** The ref lives in the common Git
   directory and survives `git worktree remove`. What is lost is the checkout, and
   with it the build output. Say this out loud before proposing a bulk delete,
   because "389 GB of branches" is what it looks like from the size column.
2. **A commit on a remote is preserved, but pushed is not the same as merged.**
   A branch pushed to `origin/feature/x` and never merged loses no commits if its
   worktree goes — yet the only checkout of unfinished work goes with it. That is
   neither A nor B; it is `Review`, because "the work still exists" and "I am done
   with it" are different claims.
3. **Never delete while a build is running.** See trap 2.

## Traps, measured

### 1. `du` counts nested worktrees twice

A repository that keeps its worktrees inside itself (`repo/.worktrees/*`) makes
`du -sh repo` include every one of them. Measured: 104.6 G reported for a set whose
actual worktree total was 91.2 G — the main checkout, which also contains
`.worktrees/`, was counted alongside the worktrees inside it.

Size must be attributed to the worktree that owns it, never summed from a repository
root. The same mistake in the product would double the number the user is asked to
approve.

### 2. Builds write into the working tree

A running `swift-test` rewrote `Resources/Info.plist` in a main checkout at
02:52:37, ninety seconds after the same directory had been read as clean. `git
status` then reports a dirty worktree that no person edited. Several worktrees showed
`M Resources/Info.plist` for exactly this reason, and reading them as "someone is
editing code" wastes the reviewer's attention on the one signal that is usually real.

The product's `wt.agent_active` watches `.git/index.lock`, which a compiler does not
take. A verdict taken while another process writes is stale by construction, which is
why the executor re-checks identity and activity immediately before deleting (D11) —
that re-check is the defence, not the scan.

### 3. Squash merges defeat ancestry

`merge-base --is-ancestor` answers "is this commit in the default branch", not "is
this change in it". GitHub's default merge strategy rewrites the commit, so a branch
that was merged an hour ago still reports unmerged.

Measured: of 33 branches reported unmerged, **11 were fully absorbed**.

Verified example — `d5e32229 test(membership): avoid same-process file lock
simulation` is not an ancestor of `origin/main`, yet its patch-id `2d2a550b…`
matches `70cdbc6 test(membership): avoid same-process file lock simulation (#333)`.
Tier A calls it unmerged; tier B calls it absorbed; the change is in main.

`git cherry` compares patch-ids, which is why B exists and why A alone is not enough
on any repository that squash-merges.

### 4. `main` does not mean the default branch

`git merge-base --is-ancestor HEAD main` resolves `main` to the **local** branch
first. Local `main` is an accident of checkout state: measured on this machine it was
behind `origin/main` by 6, 15, 209, 506 and 1336 commits across five repositories.

The error runs both ways. A stale local `main` reports merged work as unmerged
(noise, and 116 of 199 safe worktrees were misreported this way); a local `main`
carrying unpushed commits reports unmerged work as merged (**the dangerous
direction** — one worktree was called safe because its HEAD was in a local-only
`main` commit).

Always compare against `origin/<default>`. Never against a bare branch name.

## What the tiers cannot see

- **Applied then reverted.** The patch-id still matches while the default branch no
  longer contains the change, which is why tier B pairs the patch question with the
  content comparison above. A branch that was merged and then had its file rewritten
  afterwards also fails that comparison and stays `Review` — the cost of never calling
  something absorbed that is not.
- **A branch pushed nowhere reachable.** Reconciled by requiring the default branch
  to be a real remote-tracking ref; a fork nobody can reach is not covered.
- **Ignored files that matter.** Credentials and local configuration are usually
  ignored by Git, which is why the artifact scanner has a separate `af.credentials`
  code and the triage above says nothing about them.
- **Unmerged work that the user still wants.** No command answers this. It is the
  reason `Review` is a real category and not a failure of the tool.

## Procedure

1. `git fetch` every repository first, in parallel. Every comparison below is
   against a remote-tracking ref, so an unfetched repository answers with old facts.
2. Enumerate worktrees per repository; drop the main one (it is never removable).
3. Measure size per worktree, never per repository root.
4. Classify with A, then B. Stop there for anything automated.
5. Review C and D by hand if the repository family generates build-number or
   dependency-bump branches, reading the diff, not the message.
6. Delete worktrees in ascending risk: largest A first, because it is both the
   biggest win and the least ambiguous.
7. After deleting, `git worktree prune`, then report what was actually reclaimed
   rather than what was estimated.
