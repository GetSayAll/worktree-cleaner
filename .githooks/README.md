# Git hooks

Install once per clone:

    git config core.hooksPath .githooks

`pre-commit` rejects staged files that contain a maintainer's private email
address. `commit-msg` rejects a message, author, committer, or `Co-authored-by`
trailer that does. CI runs the same checker, so a clone without the hooks is
still covered after the push.
