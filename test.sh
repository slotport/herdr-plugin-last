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
    unset HERDR_WORKSPACE_ID HERDR_TAB_ID HERDR_PANE_ID
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

# --- record_mru ----------------------------------------------------------

new_sandbox
f="$SANDBOX/tabs"

record_mru "$f" "w5:t1"
record_mru "$f" "w5:t2"
record_mru "$f" "w5:t3"
assert_eq "record_mru keeps most-recent-first order" \
    "w5:t3
w5:t2
w5:t1" "$(cat "$f")"

record_mru "$f" "w5:t1"
assert_eq "record_mru moves a repeat to the front without duplicating" \
    "w5:t1
w5:t3
w5:t2" "$(cat "$f")"

record_mru "$f" ""
assert_eq "record_mru ignores an empty id" \
    "w5:t1
w5:t3
w5:t2" "$(cat "$f")"
drop_sandbox

new_sandbox
f="$SANDBOX/capped"
i=1
while [ "$i" -le 12 ]; do
    record_mru "$f" "w5:t$i"
    i=$((i + 1))
done
assert_eq "record_mru caps the file at MRU_LIMIT lines" \
    "8" "$(wc -l < "$f" | tr -d ' ')"
assert_eq "record_mru drops the oldest entries when capping" \
    "w5:t12
w5:t11
w5:t10
w5:t9
w5:t8
w5:t7
w5:t6
w5:t5" "$(cat "$f")"
drop_sandbox

new_sandbox
record_mru "$(session_dir)/tabs/w5" "w5:t1"
record_mru "$(session_dir)/tabs/w9" "w9:t1"
assert_eq "record_mru keeps workspaces separate (w5)" \
    "w5:t1" "$(cat "$(session_dir)/tabs/w5")"
assert_eq "record_mru keeps workspaces separate (w9)" \
    "w9:t1" "$(cat "$(session_dir)/tabs/w9")"
drop_sandbox

new_sandbox
HERDR_SOCKET_PATH="/home/test/.config/herdr/herdr.sock"
record_mru "$(session_dir)/tabs/w5" "w5:t1"
HERDR_SOCKET_PATH="/home/test/.config/herdr/sessions/work/herdr.sock"
record_mru "$(session_dir)/tabs/w5" "w5:t7"
assert_eq "record_mru keeps concurrent sessions separate" \
    "w5:t7" "$(cat "$(session_dir)/tabs/w5")"
HERDR_SOCKET_PATH="/home/test/.config/herdr/herdr.sock"
assert_eq "record_mru leaves the other session's history intact" \
    "w5:t1" "$(cat "$(session_dir)/tabs/w5")"
drop_sandbox

new_sandbox
f="$SANDBOX/prefix"
record_mru "$f" "w5:t12"
record_mru "$f" "w5:t1"
record_mru "$f" "w5:t1"
assert_eq "grep -vxF filters whole lines only; w5:t12 survives when w5:t1 is re-recorded" \
    "w5:t1
w5:t12" "$(cat "$f")"
drop_sandbox

new_sandbox
f="$SANDBOX/metachar"
record_mru "$f" "w5:t1X2"
record_mru "$f" "w5:t1.2"
record_mru "$f" "w5:t1.2"
assert_eq "grep -vxF uses literal strings; w5:t1X2 survives when w5:t1.2 is re-recorded" \
    "w5:t1.2
w5:t1X2" "$(cat "$f")"
drop_sandbox

# --- record entrypoints --------------------------------------------------

new_sandbox
HERDR_WORKSPACE_ID="w5" HERDR_TAB_ID="w5:t1" sh "$SCRIPT_DIR/record-tab.sh"
HERDR_WORKSPACE_ID="w5" HERDR_TAB_ID="w5:t2" sh "$SCRIPT_DIR/record-tab.sh"
assert_eq "record-tab.sh records into the workspace's tab file" \
    "w5:t2
w5:t1" "$(cat "$(session_dir)/tabs/w5")"
drop_sandbox

new_sandbox
out=$(HERDR_WORKSPACE_ID="w5" sh "$SCRIPT_DIR/record-tab.sh" 2>&1 || true)
assert_eq "record-tab.sh is a no-op without HERDR_TAB_ID (silence)" \
    "" "$out"
assert_eq "record-tab.sh is a no-op without HERDR_TAB_ID (no files created)" \
    "0" "$(find "$(session_dir)" -type f 2>/dev/null | wc -l | tr -d ' ')"
drop_sandbox

new_sandbox
out=$(HERDR_TAB_ID="w9:t1" sh "$SCRIPT_DIR/record-tab.sh" 2>&1 || true)
assert_eq "record-tab.sh is a no-op without HERDR_WORKSPACE_ID (silence)" \
    "" "$out"
assert_eq "record-tab.sh is a no-op without HERDR_WORKSPACE_ID (no files created)" \
    "0" "$(find "$(session_dir)" -type f 2>/dev/null | wc -l | tr -d ' ')"
drop_sandbox

new_sandbox
HERDR_WORKSPACE_ID="w5" sh "$SCRIPT_DIR/record-workspace.sh"
HERDR_WORKSPACE_ID="w9" sh "$SCRIPT_DIR/record-workspace.sh"
assert_eq "record-workspace.sh records into the workspaces file" \
    "w9
w5" "$(cat "$(session_dir)/workspaces")"
drop_sandbox

new_sandbox
out=$(sh "$SCRIPT_DIR/record-workspace.sh" 2>&1 || true)
assert_eq "record-workspace.sh is a no-op without HERDR_WORKSPACE_ID (silence)" \
    "" "$out"
assert_eq "record-workspace.sh is a no-op without HERDR_WORKSPACE_ID (no files created)" \
    "0" "$(find "$(session_dir)" -type f 2>/dev/null | wc -l | tr -d ' ')"
drop_sandbox

# --- summary -------------------------------------------------------------

printf '\n%s tests, %s failures\n' "$TESTS" "$FAILURES"
if [ "$FAILURES" -gt 0 ]; then
    exit 1
fi
