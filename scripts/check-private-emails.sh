#!/bin/sh
# Reject the maintainers' blocked email addresses in files and commit metadata.
#
# Only SHA-256 digests are stored here. The checker extracts address-shaped
# candidates from the content, hashes them, and compares. An address is never
# stored or printed; a failed check names the file, commit, or identity field.

set -eu

blocked='9320d1655cd951297a1c0e1d65db51e2e78377d2a28189e0c17a99a8a11cf34f 7fe0578ed4935375cc367a919ee30353dcbce114f6608b42f8381f2ca6585a86'
email_re='[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}'

failed=0

report() {
    echo "error: a blocked email address was found in $1" >&2
    failed=1
}

hash_of() {
    if command -v shasum >/dev/null 2>&1; then
        printf '%s' "$1" | tr '[:upper:]' '[:lower:]' | shasum -a 256 | awk '{print $1}'
    else
        printf '%s' "$1" | tr '[:upper:]' '[:lower:]' | sha256sum | awk '{print $1}'
    fi
}

check_candidates() {
    candidates=$1
    description=$2
    while IFS= read -r candidate; do
        if [ -z "$candidate" ]; then
            continue
        fi
        digest=$(hash_of "$candidate")
        case " $blocked " in
        *" $digest "*)
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
