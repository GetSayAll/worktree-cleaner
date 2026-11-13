# Security policy

## What this tool does, and what it can do to you

Worktree Cleaner deletes directories. It is built so that it cannot delete
something by accident, and this page says how, and how to report it when it
does.

The rules the code follows:

- A scan never writes. It reads Git metadata and walks directories.
- A directory is removed by moving it to the **system Trash** (macOS) or the
  **Recycle Bin** (Windows). There is no permanent delete, and there is no
  "force" that bypasses it.
- A worktree is removed by `git worktree remove`.
- Every deletion is backed by a reason that names what was observed, and the
  executor re-checks that reason against the current state before it acts. An
  item that cannot be verified is *needs review*, never *safe*.
- The whitelist and the protected branches are checked again at execution time.
- A directory that holds what looks like a credential file, or a symlink
  pointing outside the project, is never *safe to delete*.

## Versions

Only the latest published release is supported. Each release is signed, and the
app refuses an update whose signature does not verify.

## Reporting a vulnerability

Please report a security problem privately rather than in a public issue. Use
GitHub's "Report a vulnerability" button on the Security tab of the repository,
which opens a private advisory.

What helps:

- what you did, step by step;
- what you expected and what happened instead;
- the version you were running (`Settings → Updates` shows it);
- the operating system and its version;
- the audit log, if the run wrote one. It lives beside the app's own data
  directory and is plain text; it names what was deleted and why, and it never
  leaves the machine.

Please do not open a public issue for a vulnerability before it has been
addressed.

## What is out of scope

- The app trusts the repository it is pointed at. A repository whose Git
  metadata is hostile can make Git report whatever it likes; the tool's job is
  to show the reader that report rather than to interpret it.
- The app runs `git` from the machine's `PATH`. It is the same `git` the reader
  would run by hand.
- The Trash and the Recycle Bin are the operating system's own. What is in them
  afterwards is the operating system's business, not this tool's.

## The network

The app makes one request: to the project's release page, to see whether a
newer version exists. It uploads nothing — no telemetry, no usage data, no
machine identifier. The update that the request may lead to is verified against
a public key that ships inside the application.
