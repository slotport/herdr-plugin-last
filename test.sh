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

# --- stub herdr ----------------------------------------------------------

make_stub_herdr() {
    mkdir -p "$SANDBOX/bin" "$SANDBOX/data"
    STUB_DATA="$SANDBOX/data"
    STUB_CALLS="$SANDBOX/calls"
    STUB_INVOCATIONS="$SANDBOX/invocations"
    : > "$STUB_CALLS"
    : > "$STUB_INVOCATIONS"
    export STUB_DATA STUB_CALLS STUB_INVOCATIONS
    cat > "$SANDBOX/bin/herdr" <<'STUB'
#!/bin/sh
# Test stub for the herdr CLI. Serves canned list JSON from $STUB_DATA,
# appends every focus call to $STUB_CALLS, and logs every invocation
# (including list calls) to $STUB_INVOCATIONS.
set -eu
printf '%s\n' "$*" >> "$STUB_INVOCATIONS"
case "${1:-} ${2:-}" in
    "tab list") cat "$STUB_DATA/tabs.json" ;;
    "workspace list") cat "$STUB_DATA/workspaces.json" ;;
    "tab focus") printf 'tab %s\n' "${3:-}" >> "$STUB_CALLS" ;;
    "workspace focus") printf 'workspace %s\n' "${3:-}" >> "$STUB_CALLS" ;;
esac
STUB
    chmod +x "$SANDBOX/bin/herdr"
    HERDR_BIN_PATH="$SANDBOX/bin/herdr"
    export HERDR_BIN_PATH
}

# Real Herdr 0.7.5 output shape, single line.
stub_tabs_two() {
    printf '%s\n' '{"id":"cli:tab:list","result":{"tabs":[{"agent_status":"working","focused":true,"label":"1","number":1,"pane_count":1,"tab_id":"w5:t1","workspace_id":"w5"},{"agent_status":"unknown","focused":false,"label":"2","number":2,"pane_count":1,"tab_id":"w5:t2","workspace_id":"w5"}],"type":"tab_list"}}' > "$STUB_DATA/tabs.json"
}

stub_workspaces_two() {
    printf '%s\n' '{"id":"cli:workspace:list","result":{"type":"workspace_list","workspaces":[{"active_tab_id":"w5:t1","focused":true,"label":"Herdr","number":1,"workspace_id":"w5"},{"active_tab_id":"w9:t1","focused":false,"label":"Other","number":2,"workspace_id":"w9"}]}}' > "$STUB_DATA/workspaces.json"
}

# --- live id queries -----------------------------------------------------

new_sandbox
make_stub_herdr
stub_tabs_two
assert_eq "live_tab_ids extracts tab ids without a JSON parser" \
    "w5:t1
w5:t2" "$(live_tab_ids w5)"

stub_workspaces_two
assert_eq "live_workspace_ids extracts workspace ids only" \
    "w5
w9" "$(live_workspace_ids)"
drop_sandbox

# --- pick_target ---------------------------------------------------------

new_sandbox
f="$SANDBOX/tabs"
printf 'w5:t2\nw5:t1\n' > "$f"
assert_eq "pick_target skips the current id" \
    "w5:t1" "$(pick_target "$f" "w5:t2" "w5:t1
w5:t2")"

printf 'w5:t2\nw5:t9\nw5:t1\n' > "$f"
assert_eq "pick_target skips ids that no longer exist" \
    "w5:t1" "$(pick_target "$f" "w5:t2" "w5:t1
w5:t2")"

printf 'w5:t2\n' > "$f"
assert_eq "pick_target prints nothing when only the current id remains" \
    "" "$(pick_target "$f" "w5:t2" "w5:t1
w5:t2")"

printf 'w5:t9\n' > "$f"
assert_eq "pick_target prints nothing when every candidate is dead" \
    "" "$(pick_target "$f" "w5:t2" "w5:t1
w5:t2")"

assert_eq "pick_target prints nothing for a missing file" \
    "" "$(pick_target "$SANDBOX/absent" "w5:t2" "w5:t1")"

: > "$f"
assert_eq "pick_target prints nothing for an empty file" \
    "" "$(pick_target "$f" "w5:t2" "w5:t1")"

printf 'w5:t2\nw5:t1\n' > "$f"
assert_eq "pick_target still works when the current id is unknown" \
    "w5:t2" "$(pick_target "$f" "" "w5:t1
w5:t2")"

# grep -x guard: a dead id that is a literal prefix of a live id must not be
# treated as a match.
printf 'w5:t1\n' > "$f"
assert_eq "pick_target requires an exact match, not a prefix (grep -x)" \
    "" "$(pick_target "$f" "w5:t9" "w5:t12")"

# grep -F guard: an id containing a regex metacharacter must be matched
# literally against the live set, mirroring the record_mru metachar tests.
printf 'w5:t1.2\n' > "$f"
assert_eq "pick_target matches live ids literally, not as a regex (grep -F)" \
    "" "$(pick_target "$f" "w5:t9" "w5:t1X2")"

# `read` returns non-zero on an unterminated final line; the loop condition
# must still process it instead of silently dropping the last entry.
printf 'w5:t1' > "$f"
assert_eq "pick_target selects the final entry even without a trailing newline" \
    "w5:t1" "$(pick_target "$f" "w9" "w5:t1")"
drop_sandbox

# --- jump entrypoints ----------------------------------------------------

new_sandbox
make_stub_herdr
stub_tabs_two
mkdir -p "$(session_dir)/tabs"
printf 'w5:t2\nw5:t1\n' > "$(session_dir)/tabs/w5"
HERDR_WORKSPACE_ID="w5" HERDR_TAB_ID="w5:t2" sh "$SCRIPT_DIR/jump-tab.sh"
assert_eq "jump-tab.sh focuses the last live tab" \
    "tab w5:t1" "$(cat "$STUB_CALLS")"
drop_sandbox

new_sandbox
make_stub_herdr
stub_tabs_two
mkdir -p "$(session_dir)/tabs"
printf 'w5:t2\nw5:t9\n' > "$(session_dir)/tabs/w5"
HERDR_WORKSPACE_ID="w5" HERDR_TAB_ID="w5:t2" sh "$SCRIPT_DIR/jump-tab.sh"
assert_eq "jump-tab.sh issues no call when every candidate is dead" \
    "" "$(cat "$STUB_CALLS")"
drop_sandbox

new_sandbox
make_stub_herdr
stub_tabs_two
HERDR_WORKSPACE_ID="w5" HERDR_TAB_ID="w5:t2" sh "$SCRIPT_DIR/jump-tab.sh"
assert_eq "jump-tab.sh issues no call with no history file" \
    "" "$(cat "$STUB_CALLS")"
drop_sandbox

new_sandbox
make_stub_herdr
stub_tabs_two
mkdir -p "$(session_dir)/tabs"
printf 'w5:t2\nw5:t1\n' > "$(session_dir)/tabs/w5"
HERDR_TAB_ID="w5:t2" sh "$SCRIPT_DIR/jump-tab.sh"
assert_eq "jump-tab.sh issues no call without HERDR_WORKSPACE_ID" \
    "" "$(cat "$STUB_CALLS")"
assert_eq "jump-tab.sh invokes herdr zero times without HERDR_WORKSPACE_ID" \
    "" "$(cat "$STUB_INVOCATIONS")"
drop_sandbox

new_sandbox
make_stub_herdr
stub_workspaces_two
mkdir -p "$(session_dir)"
printf 'w5\nw9\n' > "$(session_dir)/workspaces"
HERDR_WORKSPACE_ID="w5" sh "$SCRIPT_DIR/jump-workspace.sh"
assert_eq "jump-workspace.sh focuses the last live workspace" \
    "workspace w9" "$(cat "$STUB_CALLS")"
drop_sandbox

new_sandbox
make_stub_herdr
stub_workspaces_two
mkdir -p "$(session_dir)"
printf 'w5\nw77\n' > "$(session_dir)/workspaces"
HERDR_WORKSPACE_ID="w5" sh "$SCRIPT_DIR/jump-workspace.sh"
assert_eq "jump-workspace.sh issues no call when every candidate is dead" \
    "" "$(cat "$STUB_CALLS")"
drop_sandbox

# Toggle: drive record between jumps the way Herdr's focus events would, and
# confirm two presses return to the starting tab.
new_sandbox
make_stub_herdr
stub_tabs_two
HERDR_WORKSPACE_ID="w5" HERDR_TAB_ID="w5:t1" sh "$SCRIPT_DIR/record-tab.sh"
HERDR_WORKSPACE_ID="w5" HERDR_TAB_ID="w5:t2" sh "$SCRIPT_DIR/record-tab.sh"

HERDR_WORKSPACE_ID="w5" HERDR_TAB_ID="w5:t2" sh "$SCRIPT_DIR/jump-tab.sh"
assert_eq "toggle: first press leaves t2 for t1" \
    "tab w5:t1" "$(cat "$STUB_CALLS")"
HERDR_WORKSPACE_ID="w5" HERDR_TAB_ID="w5:t1" sh "$SCRIPT_DIR/record-tab.sh"

HERDR_WORKSPACE_ID="w5" HERDR_TAB_ID="w5:t1" sh "$SCRIPT_DIR/jump-tab.sh"
assert_eq "toggle: second press returns to t2" \
    "tab w5:t1
tab w5:t2" "$(cat "$STUB_CALLS")"
drop_sandbox

# --- summary -------------------------------------------------------------

printf '\n%s tests, %s failures\n' "$TESTS" "$FAILURES"
if [ "$FAILURES" -gt 0 ]; then
    exit 1
fi
