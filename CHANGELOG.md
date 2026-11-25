# Changelog

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and
the project follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- **Git is built in.** The app reads repositories through libgit2 and removes
  worktrees through gix; it no longer spawns `git` or needs the Xcode Command Line
  Tools on macOS or Git for Windows on Windows. Machine configuration is isolated
  except the status-relevant core settings the checkout already reflects (line
  endings, case folding, file mode, symlinks), so a clean Windows worktree checked
  out with `core.autocrlf=true` is not reported as dirty. Ignore rules and hooks
  still cannot change a verdict.
- **Absorption is a content check.** A branch is `wt.absorbed` when the files it
  changed relative to the merge base hold the same content in the default branch;
  a difference is `wt.unmerged` at Review. The patch-id answer and its
  `wt.absorbed_content_changed` companion are retained only so older audit logs
  still render.

## [0.2.1] — 2026-11-21

A copy release: the interface keeps the actions a reader can take, and drops the
implementation detail behind them.

### Changed

- **Implementation detail left the interface.** The scan page no longer explains
  its per-directory time limit, total scan time limit or Git commands; the
  worktree and artifact lists no longer print their data source or detection
  rules; the disk-hygiene page no longer cites its policy document. “Scan
  incomplete” and “size unknown” remain as states the reader must see.
- **Affordances stayed.** “Click to scan”, the drag-and-drop hint, the selection
  hint, the rule summaries, the pre-run checks and the permanence warnings were
  kept, shortened to one line where possible.
- **Commands became outcomes.** `git worktree remove`, `git worktree prune`,
  `--force`, `dry run`, `JSON`, `merge-base` and the path-validation list are gone
  from user-facing copy. The plan still carries the exact command as evidence, and
  the result table still names the reason a selected item was skipped.
- **Settings labels say what the reader chooses.** “Per-directory timeout”,
  “Total scan budget”, “Activity window” and “Artifact display size threshold”
  became “Time limit per directory”, “Total scan time limit”, “Quiet period” and
  “Minimum artifact size”.

### Fixed

- **Singular counts read correctly.** Messages that take a count now write
  “1 worktree” rather than “1 worktrees” where the sentence names the noun.

## [0.2.0] — 2026-11-20

A release about the first screen and the words on it: the welcome page returns to
the composer-first order on a white canvas, and path examples now name the system
the reader is on.

### Changed

- **The welcome page leads with the scan field.** The path field, the folder
  picker and the scan button come first, the three assurance cards follow, and the
  explanatory copy sits after them. The light canvas, sidebar and cards are white
  again, as the design calls for.
- **Path examples follow the host.** The path field and the extra-paths hint show
  `/Users/you/repo` on macOS and `C:\Users\you\repo` on Windows. A home directory
  is displayed as `~` on macOS and `%USERPROFILE%` on Windows, and the settings
  hint no longer claims macOS-only defaults.
- **The copy says what the field takes.** The placeholder names an absolute path,
  and the drop hint names typing a path and choosing a folder before the drag
  alternative. Sentence-ending periods are gone from the interface copy, the mock
  warnings and the one user-visible core message; dots inside paths, commands and
  versions stay.

## [0.1.2] — 2026-11-20

The first release after a real cleanup ran on Windows, and most of it comes from
reading that run's log: two of its lines contradicted each other, and following
them back found a scan that counted some directories twice and a removal that
reported success without emptying the directory.

### Added

- **The plan is written down.** The history page records the plan as it stood when
  it was reviewed — how many worktrees and artifact directories, and the total it
  promised — so a cleanup can be compared with its outcome afterwards instead of
  being reconstructed from the count on the confirmation line. A plan copied as
  JSON is recorded too.

### Fixed

- **A directory could be counted twice and offered for deletion twice.** Every
  worktree inside the project — the usual `.worktree/` layout — was walked twice,
  once from the project root and once from its own, and the two spellings of one
  path were never recognised as the same directory. The plan then held two
  entries for it (13.35 GB of the 26.81 GB that run promised), moved it once, and
  logged "vanished" for the entry behind it. Each worktree is walked once now, and
  an artifact inside a worktree that is also being removed is folded into that
  removal rather than scheduled again.
- **`git worktree remove` could report a removal it had not finished.** Git exits 0
  after deleting every file it can, and on Windows a directory holding entries it
  does not follow — a pnpm `node_modules` is a stack of reparse points — survives
  that pass. The result line now says `worktree.remnants`, and the next scan
  reports what is left, instead of claiming the directory was gone.
- **No terminal window on Windows.** The release binary was linked as a console
  program, so Windows gave it a console, and Windows 11 draws a console in Windows
  Terminal — a window opened at every launch, including the one the installer starts
  when it finishes. The release build is now a window program, and every Git call it
  makes asks for a console without a window, so a scan does not open one either.

## [0.1.1] — 2026-11-19

A one-line release whose real purpose is to give the update path something to
find: this is the first build that a published 0.1.0 can discover, download,
verify and install.

### Changed

- The privacy line under **Settings → Updates** now says what the check does not
  send — anything about the reader or their files — instead of the vaguer
  "nothing is uploaded".

## [0.1.0] — 2026-11-19

The first release. What it does, and what a reader has to know before installing
it.

### Added

- **The scan.** Every Git worktree of a repository, found through standard Git
  metadata rather than through any agent's directory layout, with its branch,
  status, last commit, upstream and merge state.
- **The artifact scan.** The usual build-artifact directories, measured with a
  timeout per directory, and judged by what they contain.
- **The verdicts.** *Safe to delete*, *needs review*, *must not delete* — each
  with the reason and the evidence behind it. An item that cannot be verified is
  never counted as safe.
- **The cleanup plan.** Everything that will be touched, shown before anything
  is touched, with the items that need a typed confirmation called out.
- **The disk-hygiene audit.** A report against the policy in
  `docs/DISK_HYGIENE.md`. It never influences a deletion decision.
- **The audit log.** Every deletion and every action, written locally in plain
  text and never uploaded.
- **Automatic updates.** The app checks the release page at launch, says which
  version it found, and downloads and installs it on request. Every artifact is
  signed, and an artifact that does not verify is refused.
- **English and Simplified Chinese**, following the system language by default.

### Notes for the reader

- **Git has to be installed.** The tool asks Git about the repository. On macOS
  it comes with the Xcode Command Line Tools; on Windows, install
  [Git for Windows](https://git-scm.com/download/win).
- **macOS 11.3 or newer**, because the interface is compiled for a WebKit of
  that age; **Windows 10 or 11**.
- **The Trash path is verified on macOS; the Recycle Bin path is not yet
  verified on Windows.** The Windows release carries the one deletion path that
  has not run on real hardware. `docs/DEV_CONTRACT.md` §7 has it as an open
  item, and `docs/RELEASE.md` has it as a manual check.
- Nothing is uploaded. The only request the app makes is the update check.
