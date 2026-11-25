#!/bin/sh
# Reject the maintainers' blocked email addresses in files and commit metadata.
#
# The list is not stored in this repository. It comes from BLOCKED_EMAILS (the
# CI secret) or from .git/info/blocked-emails (local, untracked), one address
# per line. A failed check names the file, commit, or identity field, never an
# address.

set -eu

email_re='[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}'
blocked=${BLOCKED_EMAILS:-}

if [ -z "$blocked" ]; then
    common=$(git rev-parse --git-common-dir 2>/dev/null || true)
    for file in "$common/info/blocked-emails" "$HOME/.config/worktree-cleaner/blocked-emails"; do
        if [ -n "$file" ] && [ -f "$file" ]; then
            blocked=$(cat "$file")
            break
        fi
    done
fi

if [ -z "$blocked" ]; then
    echo "notice: no blocked-email list is configured; skipping the check" >&2
    exit 0
fi

blocked=$(printf '%s\n' "$blocked" | tr -d '\r' | tr '[:upper:]' '[:lower:]' | tr '\n' ' ')

failed=0

report() {
    echo "error: a blocked email address was found in $1" >&2
    failed=1
}

check_candidates() {
    candidates=$1
    description=$2
    while IFS= read -r candidate; do
        if [ -z "$candidate" ]; then
            continue
        fi
        normalized=$(printf '%s' "$candidate" | tr '[:upper:]' '[:lower:]')
        case " $blocked " in
        *" $normalized "*)
            report "$description"
            return
            ;;
        esac
    done <<EOF
$candidates
EOF
}

check_text() {
    check_candidates "$(printf '%s\n' "$1" | grep -oE "$email_re" || true)" "$2"
}

check_tree() {
    list=$(git grep -lE "$email_re" -- . || true)
    while IFS= read -r file; do
        if [ -n "$file" ]; then
            check_candidates "$(grep -oE "$email_re" "$file" 2>/dev/null || true)" "the checked-out file $file"
        fi
    done <<EOF
$list
EOF
}

check_index() {
    list=$(git grep --cached -lE "$email_re" -- . || true)
    while IFS= read -r file; do
        if [ -n "$file" ]; then
            check_candidates "$(git show ":$file" 2>/dev/null | grep -oE "$email_re" || true)" "the staged content of $file"
        fi
    done <<EOF
$list
EOF
}

case "${1:-}" in
--staged)
    check_index
    exit "$failed"
    ;;
--message)
    message="${2:?--message needs the commit message file}"
    check_text "$(cat "$message")" "the commit message"
    check_text "$(git var GIT_AUTHOR_IDENT)" "the author identity"
    check_text "$(git var GIT_COMMITTER_IDENT)" "the committer identity"
    exit "$failed"
    ;;
esac

check_tree

base="${1:-}"
if [ -n "$base" ] && git rev-parse --verify "$base^{commit}" >/dev/null 2>&1; then
    while IFS= read -r commit; do
        if [ -n "$commit" ]; then
            check_text "$(git show -s --format='%ae%n%ce%n%b' "$commit")" "commit $commit"
        fi
    done <<EOF
$(git log --format=%H "$base..HEAD")
EOF
fi

exit "$failed"
