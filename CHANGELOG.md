# Changelog

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and
the project follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

Nothing yet.

## [0.1.0] — not yet published

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
