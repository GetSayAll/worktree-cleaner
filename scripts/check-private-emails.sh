#!/bin/sh
# Reject the maintainers' private email addresses in files and commit metadata.
#
# The addresses are assembled from parts so this file does not contain either
# one in full. Matches are reported by file, commit, or identity field, never by
# value: a checker that printed the address would publish it again.

set -eu

local_one='hd838a'
domain_one='outlook.com'
local_two='qeeing'
domain_two='foxmail.com'
pattern="${local_one}@${domain_one}|${local_two}@${domain_two}"

failed=0

report() {
    echo "error: a private email address was found in $1" >&2
    failed=1
}

check_tree() {
    list=$(git grep -lE "$pattern" -- . || true)
    while IFS= read -r file; do
        if [ -n "$file" ]; then
            report "the checked-out file $file"
        fi
    done <<EOF
$list
EOF
}

check_index() {
    list=$(git grep --cached -lE "$pattern" -- . || true)
    while IFS= read -r file; do
        if [ -n "$file" ]; then
            report "the staged content of $file"
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
    if grep -qE "$pattern" "$message"; then
        report "the commit message"
    fi
    if git var GIT_AUTHOR_IDENT | grep -qE "$pattern"; then
        report "the author identity"
    fi
    if git var GIT_COMMITTER_IDENT | grep -qE "$pattern"; then
        report "the committer identity"
    fi
    exit "$failed"
    ;;
esac

check_tree

base="${1:-}"
if [ -n "$base" ] && git rev-parse --verify "$base^{commit}" >/dev/null 2>&1; then
    for commit in $(git log --format=%H "$base..HEAD"); do
        if git show -s --format='%ae%n%ce%n%b' "$commit" | grep -qE "$pattern"; then
            report "commit $commit"
        fi
    done
fi

exit "$failed"
