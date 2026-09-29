#!/bin/sh
set -eu
root=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT HUP INT TERM
mkdir -p "$tmp/bin"
export MOCK_STATE="$tmp/account" MOCK_LOG="$tmp/log"

# These mocks deliberately avoid real authentication or Git configuration.
cat > "$tmp/bin/gh" <<'MOCK'
#!/bin/sh
case "$1" in
    api)
        [ "${MOCK_API_FAIL:-0}" = 0 ] || exit 1
        cat "$MOCK_STATE"
        ;;
    auth)
        echo switch >> "$MOCK_LOG"
        [ "${MOCK_SWITCH_FAIL:-0}" = 0 ] || exit 1
        [ "${MOCK_OVERRIDE:-0}" = 0 ] || exit 0
        echo Carlo1911 > "$MOCK_STATE"
        ;;
    *) exit 1 ;;
esac
MOCK
cat > "$tmp/bin/git" <<'MOCK'
#!/bin/sh
printf '%s\n' "$*" >> "$MOCK_LOG"
MOCK
chmod +x "$tmp/bin/gh" "$tmp/bin/git"
export PATH="$tmp/bin:$PATH"

reset_case() {
    echo "$1" > "$MOCK_STATE"
    : > "$MOCK_LOG"
    unset MOCK_SWITCH_FAIL MOCK_OVERRIDE MOCK_API_FAIL
}
run_check() {
    sh "$root/scripts/ensure-github-account.sh" > "$tmp/output" 2>&1
}
expect_failure() {
    if run_check; then
        echo "Expected authentication failure" >&2
        exit 1
    fi
    if grep -q '^config ' "$MOCK_LOG"; then
        echo "Git credentials must not be configured after authentication failure" >&2
        exit 1
    fi
}

reset_case Carlo1911
run_check
! grep -q '^switch$' "$MOCK_LOG"
grep -q 'config --local --add credential.https://github.com.helper !gh auth git-credential' "$MOCK_LOG"

reset_case OtherAccount
run_check
grep -q '^switch$' "$MOCK_LOG"
[ "$(cat "$MOCK_STATE")" = Carlo1911 ]

reset_case OtherAccount
export MOCK_SWITCH_FAIL=1
expect_failure

reset_case OtherAccount
export MOCK_OVERRIDE=1
expect_failure

reset_case OtherAccount
export MOCK_API_FAIL=1
expect_failure

echo "Account verification tests: 5 passed."
