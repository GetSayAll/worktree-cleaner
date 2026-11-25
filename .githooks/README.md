# Git hooks

Install once per clone:

    git config core.hooksPath .githooks

The blocked-address list is not stored in this repository. For local hooks,
write one address per line to `.git/info/blocked-emails` (or set the
`BLOCKED_EMAILS` environment variable). CI reads the `BLOCKED_EMAILS` repository
secret; fork pull requests do not receive secrets, so the check is skipped
there.

`pre-commit` rejects staged files that contain a blocked address. `commit-msg`
rejects a message, author, committer, or `Co-authored-by` trailer that does.
