#!/bin/sh
# Dependency-free test runner for herdr-plugin-last.
#
# Drives the real binary as a subprocess against a temporary state directory
# and a stub herdr. The pure logic (MRU ordering, target selection, JSON
# parsing) is unit-tested in `cargo test`; this suite covers the entrypoint
# behaviour those tests cannot reach — exit codes, silence, and which herdr
# calls actually get issued.
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
BIN="$SCRIPT_DIR/target/release/last"
TESTS=0
FAILURES=0

# Build first so the suite can never assert against a stale binary. `herdr
# plugin link` does not run build commands, so this is the only thing standing
# between a local edit and a misleading pass.
cargo build --release --manifest-path "$SCRIPT_DIR/Cargo.toml"

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

# Deliberately an independent reimplementation of the path rule rather than a
# call into the binary: it checks that the binary writes where the spec says,
# not merely where the binary thinks it should.
session_dir() {
    _key=$(printf '%s' "${HERDR_SOCKET_PATH:-default}" | tr '/' '_')
    printf '%s/%s' "$HERDR_PLUGIN_STATE_DIR" "$_key"
}

# --- record entrypoints --------------------------------------------------

new_sandbox
HERDR_WORKSPACE_ID="w5" HERDR_TAB_ID="w5:t1" "$BIN" record-tab
HERDR_WORKSPACE_ID="w5" HERDR_TAB_ID="w5:t2" "$BIN" record-tab
assert_eq "record-tab records into the workspace's tab file" \
    "w5:t2
w5:t1" "$(cat "$(session_dir)/tabs/w5")"
drop_sandbox

new_sandbox
out=$(HERDR_WORKSPACE_ID="w5" "$BIN" record-tab 2>&1 || true)
assert_eq "record-tab is a no-op without HERDR_TAB_ID (silence)" \
    "" "$out"
assert_eq "record-tab is a no-op without HERDR_TAB_ID (no files created)" \
    "0" "$(find "$(session_dir)" -type f 2>/dev/null | wc -l | tr -d ' ')"
drop_sandbox

new_sandbox
out=$(HERDR_TAB_ID="w9:t1" "$BIN" record-tab 2>&1 || true)
assert_eq "record-tab is a no-op without HERDR_WORKSPACE_ID (silence)" \
    "" "$out"
assert_eq "record-tab is a no-op without HERDR_WORKSPACE_ID (no files created)" \
    "0" "$(find "$(session_dir)" -type f 2>/dev/null | wc -l | tr -d ' ')"
drop_sandbox

new_sandbox
HERDR_WORKSPACE_ID="w5" "$BIN" record-workspace
HERDR_WORKSPACE_ID="w9" "$BIN" record-workspace
assert_eq "record-workspace records into the workspaces file" \
    "w9
w5" "$(cat "$(session_dir)/workspaces")"
drop_sandbox

new_sandbox
out=$("$BIN" record-workspace 2>&1 || true)
assert_eq "record-workspace is a no-op without HERDR_WORKSPACE_ID (silence)" \
    "" "$out"
assert_eq "record-workspace is a no-op without HERDR_WORKSPACE_ID (no files created)" \
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

# --- jump entrypoints ----------------------------------------------------

new_sandbox
make_stub_herdr
stub_tabs_two
mkdir -p "$(session_dir)/tabs"
printf 'w5:t2\nw5:t1\n' > "$(session_dir)/tabs/w5"
HERDR_WORKSPACE_ID="w5" HERDR_TAB_ID="w5:t2" "$BIN" jump-tab
assert_eq "jump-tab focuses the last live tab" \
    "tab w5:t1" "$(cat "$STUB_CALLS")"
drop_sandbox

new_sandbox
make_stub_herdr
stub_tabs_two
mkdir -p "$(session_dir)/tabs"
printf 'w5:t2\nw5:t9\n' > "$(session_dir)/tabs/w5"
HERDR_WORKSPACE_ID="w5" HERDR_TAB_ID="w5:t2" "$BIN" jump-tab
assert_eq "jump-tab issues no call when every candidate is dead" \
    "" "$(cat "$STUB_CALLS")"
drop_sandbox

new_sandbox
make_stub_herdr
stub_tabs_two
HERDR_WORKSPACE_ID="w5" HERDR_TAB_ID="w5:t2" "$BIN" jump-tab
assert_eq "jump-tab issues no call with no history file" \
    "" "$(cat "$STUB_CALLS")"
drop_sandbox

new_sandbox
make_stub_herdr
stub_tabs_two
mkdir -p "$(session_dir)/tabs"
printf 'w5:t2\nw5:t1\n' > "$(session_dir)/tabs/w5"
HERDR_TAB_ID="w5:t2" "$BIN" jump-tab
assert_eq "jump-tab issues no call without HERDR_WORKSPACE_ID" \
    "" "$(cat "$STUB_CALLS")"
assert_eq "jump-tab invokes herdr zero times without HERDR_WORKSPACE_ID" \
    "" "$(cat "$STUB_INVOCATIONS")"
drop_sandbox

new_sandbox
make_stub_herdr
stub_workspaces_two
mkdir -p "$(session_dir)"
printf 'w5\nw9\n' > "$(session_dir)/workspaces"
HERDR_WORKSPACE_ID="w5" "$BIN" jump-workspace
assert_eq "jump-workspace focuses the last live workspace" \
    "workspace w9" "$(cat "$STUB_CALLS")"
drop_sandbox

new_sandbox
make_stub_herdr
stub_workspaces_two
mkdir -p "$(session_dir)"
printf 'w5\nw77\n' > "$(session_dir)/workspaces"
HERDR_WORKSPACE_ID="w5" "$BIN" jump-workspace
assert_eq "jump-workspace issues no call when every candidate is dead" \
    "" "$(cat "$STUB_CALLS")"
drop_sandbox

# --- missing HERDR_PLUGIN_STATE_DIR guard ---------------------------------
#
# Regression guard carried over from the shell implementation, where
# ${HERDR_PLUGIN_STATE_DIR:?} aborted only the $( ) subshell it ran in and let
# the parent continue with an empty state dir, turning the path into an
# absolute path at the filesystem root. Context::from_env() returns an Option
# that each entrypoint must destructure, so the failure mode is now
# inexpressible — but Herdr always sets the variable in production, so this is
# the only place the guard is exercised at all.
#
# HERDR_PLUGIN_STATE_DIR is unset inside a "(...)" subshell so the unset cannot
# leak into later tests under this suite's own "set -u". Nothing here asserts
# on an absolute path like /tabs; only exit status, output, and the sandbox's
# own contents are checked, so this test cannot touch the real filesystem root.
#
# Each subshell also `cd`s into the sandbox before running the binary. The
# guard must prevent writes anywhere, not merely inside a configured state
# dir: a plausible regression is `from_env` falling back to a relative empty
# path instead of returning None, which would write under the process's
# current working directory rather than under $SANDBOX. In production that
# CWD is the plugin's install directory, so an unguarded relative write would
# quietly litter it on every focus event. Running from the sandbox is what
# makes that kind of write land where the existing "creates no file"
# assertion below already looks, rather than silently escaping it. $BIN is an
# absolute path (built from $SCRIPT_DIR), so the `cd` cannot break the
# invocation.

new_sandbox
status=0
out=$( (cd "$SANDBOX" && unset HERDR_PLUGIN_STATE_DIR; HERDR_WORKSPACE_ID="w5" HERDR_TAB_ID="w5:t1" "$BIN" record-tab) 2>&1) || status=$?
assert_eq "record-tab exits 0 without HERDR_PLUGIN_STATE_DIR" \
    "0" "$status"
assert_eq "record-tab is silent without HERDR_PLUGIN_STATE_DIR" \
    "" "$out"
assert_eq "record-tab creates no file without HERDR_PLUGIN_STATE_DIR" \
    "0" "$(find "$SANDBOX" -type f | wc -l | tr -d ' ')"
drop_sandbox

new_sandbox
status=0
out=$( (cd "$SANDBOX" && unset HERDR_PLUGIN_STATE_DIR; HERDR_WORKSPACE_ID="w5" "$BIN" record-workspace) 2>&1) || status=$?
assert_eq "record-workspace exits 0 without HERDR_PLUGIN_STATE_DIR" \
    "0" "$status"
assert_eq "record-workspace is silent without HERDR_PLUGIN_STATE_DIR" \
    "" "$out"
assert_eq "record-workspace creates no file without HERDR_PLUGIN_STATE_DIR" \
    "0" "$(find "$SANDBOX" -type f | wc -l | tr -d ' ')"
drop_sandbox

new_sandbox
status=0
out=$( (cd "$SANDBOX" && unset HERDR_PLUGIN_STATE_DIR; HERDR_WORKSPACE_ID="w5" HERDR_TAB_ID="w5:t1" "$BIN" jump-tab) 2>&1) || status=$?
assert_eq "jump-tab exits 0 without HERDR_PLUGIN_STATE_DIR" \
    "0" "$status"
assert_eq "jump-tab is silent without HERDR_PLUGIN_STATE_DIR" \
    "" "$out"
assert_eq "jump-tab creates no file without HERDR_PLUGIN_STATE_DIR" \
    "0" "$(find "$SANDBOX" -type f | wc -l | tr -d ' ')"
drop_sandbox

new_sandbox
status=0
out=$( (cd "$SANDBOX" && unset HERDR_PLUGIN_STATE_DIR; HERDR_WORKSPACE_ID="w5" "$BIN" jump-workspace) 2>&1) || status=$?
assert_eq "jump-workspace exits 0 without HERDR_PLUGIN_STATE_DIR" \
    "0" "$status"
assert_eq "jump-workspace is silent without HERDR_PLUGIN_STATE_DIR" \
    "" "$out"
assert_eq "jump-workspace creates no file without HERDR_PLUGIN_STATE_DIR" \
    "0" "$(find "$SANDBOX" -type f | wc -l | tr -d ' ')"
drop_sandbox

# Toggle: drive record between jumps the way Herdr's focus events would, and
# confirm two presses return to the starting tab.
new_sandbox
make_stub_herdr
stub_tabs_two
HERDR_WORKSPACE_ID="w5" HERDR_TAB_ID="w5:t1" "$BIN" record-tab
HERDR_WORKSPACE_ID="w5" HERDR_TAB_ID="w5:t2" "$BIN" record-tab

HERDR_WORKSPACE_ID="w5" HERDR_TAB_ID="w5:t2" "$BIN" jump-tab
assert_eq "toggle: first press leaves t2 for t1" \
    "tab w5:t1" "$(cat "$STUB_CALLS")"
HERDR_WORKSPACE_ID="w5" HERDR_TAB_ID="w5:t1" "$BIN" record-tab

HERDR_WORKSPACE_ID="w5" HERDR_TAB_ID="w5:t1" "$BIN" jump-tab
assert_eq "toggle: second press returns to t2" \
    "tab w5:t1
tab w5:t2" "$(cat "$STUB_CALLS")"
drop_sandbox

# --- record/jump exit asymmetry -------------------------------------------
#
# The record hooks let a genuine write failure reach Herdr's plugin log; the
# jump actions swallow everything and always exit 0. That asymmetry is the
# most important behaviour in this port, and every other test in this file
# reaches only guard conditions, which exit 0 on both paths by design — so
# without this section, deleting the asymmetry entirely goes undetected.
#
# The failure is induced by creating the directory record needs as a regular
# FILE, so create_dir_all fails with EEXIST. No permissions are involved, so
# this behaves the same when run as root.

new_sandbox
make_stub_herdr
stub_tabs_two
mkdir -p "$(session_dir)"
: > "$(session_dir)/tabs"
status=0
out=$(HERDR_WORKSPACE_ID="w5" HERDR_TAB_ID="w5:t1" "$BIN" record-tab 2>&1) || status=$?
assert_ne "record-tab exits non-zero when the write fails" "0" "$status"
assert_ne "record-tab reports the failure on stderr" "" "$out"
drop_sandbox

new_sandbox
make_stub_herdr
stub_tabs_two
mkdir -p "$(session_dir)"
: > "$(session_dir)/tabs"
status=0
out=$(HERDR_WORKSPACE_ID="w5" HERDR_TAB_ID="w5:t1" "$BIN" jump-tab 2>&1) || status=$?
assert_eq "jump-tab exits 0 when the state path is unusable" "0" "$status"
assert_eq "jump-tab stays silent when the state path is unusable" "" "$out"
drop_sandbox

# --- summary -------------------------------------------------------------

printf '\n%s tests, %s failures\n' "$TESTS" "$FAILURES"
if [ "$FAILURES" -gt 0 ]; then
    exit 1
fi
