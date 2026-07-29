#!/bin/sh
# Dependency-free test runner for herdr-plugin-last.
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
TESTS=0
FAILURES=0

assert_eq() {
    # assert_eq <name> <expected> <actual>
    TESTS=$((TESTS + 1))
    if [ "$2" = "$3" ]; then
        printf 'ok - %s\n' "$1"
    else
        FAILURES=$((FAILURES + 1))
        printf 'FAIL - %s\n  expected: [%s]\n  actual:   [%s]\n' "$1" "$2" "$3" >&2
    fi
}

assert_ne() {
    # assert_ne <name> <a> <b>
    TESTS=$((TESTS + 1))
    if [ "$2" != "$3" ]; then
        printf 'ok - %s\n' "$1"
    else
        FAILURES=$((FAILURES + 1))
        printf 'FAIL - %s\n  both were: [%s]\n' "$1" "$2" >&2
    fi
}

new_sandbox() {
    SANDBOX=$(mktemp -d)
    HERDR_PLUGIN_STATE_DIR="$SANDBOX/state"
    HERDR_SOCKET_PATH="/home/test/.config/herdr/herdr.sock"
    export HERDR_PLUGIN_STATE_DIR HERDR_SOCKET_PATH
    mkdir -p "$HERDR_PLUGIN_STATE_DIR"
}

drop_sandbox() {
    if [ -n "${SANDBOX:-}" ]; then
        rm -rf "$SANDBOX"
        SANDBOX=""
    fi
}

. "$SCRIPT_DIR/lib.sh"

# --- herdr_bin / MRU_LIMIT ---

assert_eq "MRU_LIMIT is 8" "8" "$MRU_LIMIT"

new_sandbox
export HERDR_BIN_PATH="/custom/herdr"
assert_eq "herdr_bin prints HERDR_BIN_PATH when set" \
    "/custom/herdr" "$(herdr_bin)"

# Test fallback to 'herdr' in a subshell to safely unset HERDR_BIN_PATH under set -u
assert_eq "herdr_bin falls back to 'herdr' when HERDR_BIN_PATH is unset" \
    "herdr" "$( ( unset HERDR_BIN_PATH; herdr_bin ) )"
drop_sandbox

# --- session_dir ---------------------------------------------------------

new_sandbox
HERDR_PLUGIN_STATE_DIR="/state"
HERDR_SOCKET_PATH="/home/test/.config/herdr/herdr.sock"
assert_eq "session_dir sanitises the full socket path" \
    "/state/_home_test_.config_herdr_herdr.sock" \
    "$(session_dir)"

# Regression guard: named sessions all share the basename herdr.sock, so a
# basename-derived key would silently merge two sessions' histories.
HERDR_SOCKET_PATH="/home/test/.config/herdr/herdr.sock"
default_key=$(session_dir)
HERDR_SOCKET_PATH="/home/test/.config/herdr/sessions/work/herdr.sock"
named_key=$(session_dir)
assert_ne "session_dir distinguishes default from named sessions" \
    "$default_key" "$named_key"
drop_sandbox

# --- read_mru ------------------------------------------------------------

new_sandbox
assert_eq "read_mru on a missing file is empty" \
    "" "$(read_mru "$SANDBOX/nope")"

printf 'a\nb\n' > "$SANDBOX/f"
assert_eq "read_mru returns file contents" \
    "a
b" "$(read_mru "$SANDBOX/f")"
drop_sandbox

# --- write_atomic --------------------------------------------------------

new_sandbox
printf 'one\ntwo\n' | write_atomic "$SANDBOX/deep/nested/f"
assert_eq "write_atomic creates parent directories and writes" \
    "one
two" "$(cat "$SANDBOX/deep/nested/f")"

printf 'replaced\n' | write_atomic "$SANDBOX/deep/nested/f"
assert_eq "write_atomic replaces existing content" \
    "replaced" "$(cat "$SANDBOX/deep/nested/f")"

assert_eq "write_atomic leaves no temp files behind" \
    "0" "$(find "$SANDBOX/deep/nested" -name '.tmp.*' | wc -l | tr -d ' ')"
drop_sandbox

# --- summary -------------------------------------------------------------

printf '\n%s tests, %s failures\n' "$TESTS" "$FAILURES"
if [ "$FAILURES" -gt 0 ]; then
    exit 1
fi
