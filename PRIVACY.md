# Privacy policy

Last updated: 2026-11-26

Worktree Cleaner is a local desktop tool for inspecting and cleaning Git
worktrees and build artifacts. This policy covers the macOS and Windows builds
distributed from GitHub and the macOS build distributed through the Mac App
Store.

## Data collection

Worktree Cleaner does not collect personal data. It has no account system, no
telemetry, no analytics, no crash reporting, no advertising, and no tracking. It
does not sell or share personal data, and it has no server of its own.

## What the app processes on your machine

- The repository folder you choose. Scans read Git metadata and directory
  information; a scan never changes the repository.
- Git state and build-artifact sizes, read locally to produce the report.
- Your settings and the local audit log, stored in the app's data directory.
- The deletion actions you confirm. Directories are moved to the system Trash
  or Recycle Bin; worktrees are removed with `git worktree remove`.

This information stays on your machine. It is not uploaded to us or to anyone
else.

## Network access

The build distributed from GitHub checks the project's GitHub Releases feed once
at launch to see whether a newer version exists. That HTTPS request goes to
GitHub, which receives the request metadata any web request carries, including
the IP address. The request contains no account, identifier, or usage data. The
app uploads nothing.

The Mac App Store build does not check for updates and does not contact the
release feed. Apple delivers updates through the App Store under Apple's own
terms and privacy policy.

## Third parties

- GitHub hosts the source code, release downloads, and the update feed for the
  direct-download build.
- Apple distributes the Mac App Store build and processes App Store purchases
  and downloads under Apple's policies.

We do not provide personal data to these or any other third parties.

## Retention

Settings and the audit log remain on your machine until you delete them or
uninstall the app. Removing the app does not remove repositories or files you
chose to keep.

## Children

The app is a developer tool and is not directed at children. It does not collect
data from anyone, including children.

## Changes

If this policy changes, the updated version will be published at this URL with a
new date.

## Contact

For privacy questions, open an issue at
<https://github.com/GetSayAll/worktree-cleaner/issues>. For a security report,
use the private advisory flow described in [SECURITY.md](SECURITY.md).
