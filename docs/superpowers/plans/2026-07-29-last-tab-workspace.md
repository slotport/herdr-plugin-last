# herdr-plugin-last Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A Herdr plugin providing tmux-style `last-tab` (per workspace) and `last-workspace` navigation that toggles on repeated presses.

**Architecture:** Two manifest event hooks (`tab.focused`, `workspace.focused`) append ids to per-session MRU files; two manifest actions read those files and focus the most recent still-live entry. Toggling is emergent — jumping fires another focus event that re-prepends the destination, swapping the top two entries. No changes to Herdr core.

**Tech Stack:** POSIX `sh` only. No jq, no node, no compiled code. Herdr 0.7.5 CLI (`herdr tab list|focus`, `herdr workspace list|focus`).

## Global Constraints

- Target Herdr version floor: `min_herdr_version = "0.7.5"`.
- Platforms: `["linux", "macos"]`. No Windows support.
- Plugin id: `m4salah.last`. Action ids: `last-tab`, `last-workspace` (action ids may not contain dots).
- POSIX `sh` only — no bashisms (no arrays, no `[[`, no `local`, no `${x^^}`).
- No external dependencies beyond coreutils/`sed`/`grep`/`tr`/`mktemp`.
- MRU depth: 8 entries per list.
- Every script starts `set -eu`.
- Scripts must never write to stdout/stderr on the happy path; Herdr captures plugin output into its command log.
- Scripts must always exit 0. A failed jump is a silent no-op, never an error dialog.
- All shared helper function names are prefixed with nothing, but all helper *locals* are prefixed `_` to avoid clobbering caller variables (POSIX `sh` has no `local`).

**Verified facts this plan depends on** — do not re-derive, and do not "fix" code that looks redundant because of them:

- `herdr tab focus <dead-id>` prints a JSON error but **exits 0**. Exit status cannot drive fallback logic; the live-id set must be fetched and filtered first.
- Named sessions live at `~/.config/herdr/sessions/<name>/herdr.sock`, default at `~/.config/herdr/herdr.sock`. The **basename is always `herdr.sock`** — the full socket path must be sanitised for the state key.
- Herdr injects `HERDR_WORKSPACE_ID` and `HERDR_TAB_ID` into both event hooks and actions, so the record path needs no JSON parsing.
- Tab ids are `<workspace_id>:t<n>`; workspace ids are opaque (`w5`, `w6534795cc50391`) but contain no `/`, so they are safe as filenames.
- `set -e` does **not** exit on a failing `cmd && other` AND-list, but this plan uses explicit `if` blocks anyway for readability.

---

## File Structure

| File | Responsibility |
|---|---|
| `lib.sh` | All shared logic: state paths, atomic write, MRU update, live-id queries, target selection. Sourced by all four entrypoints. |
| `record-tab.sh` | Entrypoint for `tab.focused`. Reads env, calls `record_mru`. |
| `record-workspace.sh` | Entrypoint for `workspace.focused`. Reads env, calls `record_mru`. |
| `jump-tab.sh` | Entrypoint for the `last-tab` action. |
| `jump-workspace.sh` | Entrypoint for the `last-workspace` action. |
| `herdr-plugin.toml` | Manifest wiring events and actions to the scripts. |
| `test.sh` | Dependency-free test runner for everything in `lib.sh` and the four entrypoints. |
| `README.md` | Install, bind, and troubleshoot instructions. |

The four entrypoints stay deliberately thin (under ~15 lines each) so each is readable on its own; everything with logic worth testing lives in `lib.sh`.

---

### Task 1: Test harness, state paths, atomic write

**Files:**
- Create: `lib.sh`
- Create: `test.sh`

**Interfaces:**
- Consumes: nothing.
- Produces:
  - `MRU_LIMIT` — integer, `8`.
  - `herdr_bin()` → prints path to the herdr binary (`$HERDR_BIN_PATH`, else `herdr`).
  - `session_dir()` → prints `$HERDR_PLUGIN_STATE_DIR/<sanitised-socket-path>`.
  - `read_mru(path)` → prints file contents, empty string if absent.
  - `write_atomic(path)` → reads stdin, replaces `path` atomically, creating parent dirs.

- [ ] **Step 1: Write the failing test**

Create `test.sh`:

```sh
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
```

Make it executable:

```sh
chmod +x test.sh
```

- [ ] **Step 2: Run test to verify it fails**

Run: `./test.sh`
Expected: FAIL — `lib.sh: No such file or directory`

- [ ] **Step 3: Write minimal implementation**

Create `lib.sh`:

```sh
# shellcheck shell=sh
# Shared helpers for herdr-plugin-last. Sourced by the record and jump
# entrypoints; not executable on its own.

MRU_LIMIT=8

# Path to the herdr binary. Herdr injects HERDR_BIN_PATH into plugin commands;
# fall back to PATH so the scripts stay runnable by hand.
herdr_bin() {
    printf '%s' "${HERDR_BIN_PATH:-herdr}"
}

# Directory holding this session's MRU files.
#
# The whole socket path is sanitised rather than its basename: named sessions
# live at ~/.config/herdr/sessions/<name>/herdr.sock and the default session at
# ~/.config/herdr/herdr.sock, so every session shares the basename herdr.sock.
# Keying on basename would make concurrent sessions share one history.
session_dir() {
    _key=$(printf '%s' "${HERDR_SOCKET_PATH:-default}" | tr '/' '_')
    printf '%s/%s' "${HERDR_PLUGIN_STATE_DIR:?HERDR_PLUGIN_STATE_DIR is not set}" "$_key"
}

# Print the contents of an MRU file, or nothing when it does not exist.
read_mru() {
    cat "$1" 2>/dev/null || true
}

# Replace a file with stdin atomically, so a concurrent reader never observes a
# half-written list. The temp file is created in the destination directory so
# the rename stays on one filesystem.
write_atomic() {
    _path=$1
    _dir=$(dirname "$_path")
    mkdir -p "$_dir"
    _tmp=$(mktemp "$_dir/.tmp.XXXXXX")
    cat > "$_tmp"
    mv "$_tmp" "$_path"
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `./test.sh`
Expected: PASS — `7 tests, 0 failures`

- [ ] **Step 5: Commit**

```sh
git add lib.sh test.sh
git commit -m "feat: add state paths and atomic write helpers"
```

---

### Task 2: MRU record logic

**Files:**
- Modify: `lib.sh` (append `record_mru`)
- Modify: `test.sh` (append record section before the summary block)

**Interfaces:**
- Consumes: `MRU_LIMIT`, `read_mru`, `write_atomic` from Task 1.
- Produces: `record_mru(path, id)` — moves `id` to the front of the MRU file at `path`, removing any earlier occurrence and capping the file at `MRU_LIMIT` lines. A blank `id` is a no-op.

- [ ] **Step 1: Write the failing test**

Insert into `test.sh` immediately **before** the `# --- summary` block:

```sh
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
```

- [ ] **Step 2: Run test to verify it fails**

Run: `./test.sh`
Expected: FAIL — `record_mru: not found`

- [ ] **Step 3: Write minimal implementation**

Append to `lib.sh`:

```sh
# Move an id to the front of an MRU file, deduplicating and capping the list.
#
#   record_mru <path> <id>
#
# A blank id is ignored so a missing HERDR_TAB_ID cannot truncate the history.
record_mru() {
    _path=$1
    _id=$2
    if [ -z "$_id" ]; then
        return 0
    fi
    _rest=$(read_mru "$_path" | grep -vxF "$_id" || true)
    if [ -n "$_rest" ]; then
        printf '%s\n%s\n' "$_id" "$_rest" | head -n "$MRU_LIMIT" | write_atomic "$_path"
    else
        printf '%s\n' "$_id" | write_atomic "$_path"
    fi
}
```

`grep -vxF` is deliberate: `-x` matches whole lines so `w5:t1` never matches `w5:t12`, and `-F` keeps ids with `:` from being read as patterns. The `|| true` absorbs grep's exit 1 when every line matched.

- [ ] **Step 4: Run test to verify it passes**

Run: `./test.sh`
Expected: PASS — `16 tests, 0 failures`

- [ ] **Step 5: Commit**

```sh
git add lib.sh test.sh
git commit -m "feat: add MRU record logic"
```

---

### Task 3: Record entrypoints

**Files:**
- Create: `record-tab.sh`
- Create: `record-workspace.sh`
- Modify: `test.sh` (append entrypoint section before the summary block)

**Interfaces:**
- Consumes: `session_dir`, `record_mru` from Tasks 1-2.
- Produces: two executable entrypoints driven purely by environment variables. `record-tab.sh` reads `HERDR_WORKSPACE_ID` + `HERDR_TAB_ID` and writes `<session_dir>/tabs/<workspace>`. `record-workspace.sh` reads `HERDR_WORKSPACE_ID` and writes `<session_dir>/workspaces`.

- [ ] **Step 1: Write the failing test**

Insert into `test.sh` immediately **before** the `# --- summary` block:

```sh
# --- record entrypoints --------------------------------------------------

new_sandbox
HERDR_WORKSPACE_ID="w5" HERDR_TAB_ID="w5:t1" sh "$SCRIPT_DIR/record-tab.sh"
HERDR_WORKSPACE_ID="w5" HERDR_TAB_ID="w5:t2" sh "$SCRIPT_DIR/record-tab.sh"
assert_eq "record-tab.sh records into the workspace's tab file" \
    "w5:t2
w5:t1" "$(cat "$(session_dir)/tabs/w5")"

HERDR_WORKSPACE_ID="w5" sh "$SCRIPT_DIR/record-tab.sh"
assert_eq "record-tab.sh is a no-op without HERDR_TAB_ID" \
    "w5:t2
w5:t1" "$(cat "$(session_dir)/tabs/w5")"

HERDR_TAB_ID="w9:t1" sh "$SCRIPT_DIR/record-tab.sh"
assert_eq "record-tab.sh is a no-op without HERDR_WORKSPACE_ID" \
    "0" "$(find "$(session_dir)/tabs" -name 'w9' | wc -l | tr -d ' ')"
drop_sandbox

new_sandbox
HERDR_WORKSPACE_ID="w5" sh "$SCRIPT_DIR/record-workspace.sh"
HERDR_WORKSPACE_ID="w9" sh "$SCRIPT_DIR/record-workspace.sh"
assert_eq "record-workspace.sh records into the workspaces file" \
    "w9
w5" "$(cat "$(session_dir)/workspaces")"

sh "$SCRIPT_DIR/record-workspace.sh"
assert_eq "record-workspace.sh is a no-op without HERDR_WORKSPACE_ID" \
    "w9
w5" "$(cat "$(session_dir)/workspaces")"
drop_sandbox
```

- [ ] **Step 2: Run test to verify it fails**

Run: `./test.sh`
Expected: FAIL — `record-tab.sh: No such file or directory`

- [ ] **Step 3: Write minimal implementation**

Create `record-tab.sh`:

```sh
#!/bin/sh
# Herdr event hook for tab.focused.
# Records the newly focused tab at the front of its workspace's MRU list.
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
. "$SCRIPT_DIR/lib.sh"

ws=${HERDR_WORKSPACE_ID:-}
tab=${HERDR_TAB_ID:-}
if [ -z "$ws" ] || [ -z "$tab" ]; then
    exit 0
fi

record_mru "$(session_dir)/tabs/$ws" "$tab"
```

Create `record-workspace.sh`:

```sh
#!/bin/sh
# Herdr event hook for workspace.focused.
# Records the newly focused workspace at the front of the MRU list.
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
. "$SCRIPT_DIR/lib.sh"

ws=${HERDR_WORKSPACE_ID:-}
if [ -z "$ws" ]; then
    exit 0
fi

record_mru "$(session_dir)/workspaces" "$ws"
```

Make both executable:

```sh
chmod +x record-tab.sh record-workspace.sh
```

- [ ] **Step 4: Run test to verify it passes**

Run: `./test.sh`
Expected: PASS — `21 tests, 0 failures`

- [ ] **Step 5: Commit**

```sh
git add record-tab.sh record-workspace.sh test.sh
git commit -m "feat: add tab and workspace record hooks"
```

---

### Task 4: Live-id queries and target selection

**Files:**
- Modify: `lib.sh` (append `live_tab_ids`, `live_workspace_ids`, `pick_target`)
- Modify: `test.sh` (append stub helper + selection section before the summary block)

**Interfaces:**
- Consumes: `herdr_bin`, `read_mru` from Task 1.
- Produces:
  - `live_tab_ids(workspace_id)` → newline-separated tab ids that currently exist.
  - `live_workspace_ids()` → newline-separated workspace ids that currently exist.
  - `pick_target(path, current_id, live_ids)` → prints the first id in the MRU file that is neither `current_id` nor absent from `live_ids`; prints nothing when none qualifies.

- [ ] **Step 1: Write the failing test**

Insert into `test.sh` immediately **before** the `# --- summary` block:

```sh
# --- stub herdr ----------------------------------------------------------

make_stub_herdr() {
    mkdir -p "$SANDBOX/bin" "$SANDBOX/data"
    STUB_DATA="$SANDBOX/data"
    STUB_CALLS="$SANDBOX/calls"
    : > "$STUB_CALLS"
    export STUB_DATA STUB_CALLS
    cat > "$SANDBOX/bin/herdr" <<'STUB'
#!/bin/sh
# Test stub for the herdr CLI. Serves canned list JSON from $STUB_DATA and
# appends every focus call to $STUB_CALLS.
set -eu
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
drop_sandbox
```

- [ ] **Step 2: Run test to verify it fails**

Run: `./test.sh`
Expected: FAIL — `live_tab_ids: not found`

- [ ] **Step 3: Write minimal implementation**

Append to `lib.sh`:

```sh
# Ids of tabs that currently exist in a workspace, one per line.
#
# Herdr prints the whole list as one line of JSON. Splitting on commas and
# pulling the tab_id field avoids a jq dependency; the -n/p pair means lines
# without the field produce nothing.
live_tab_ids() {
    "$(herdr_bin)" tab list --workspace "$1" 2>/dev/null \
        | tr ',' '\n' \
        | sed -n 's/.*"tab_id":"\([^"]*\)".*/\1/p'
}

# Ids of workspaces that currently exist, one per line.
live_workspace_ids() {
    "$(herdr_bin)" workspace list 2>/dev/null \
        | tr ',' '\n' \
        | sed -n 's/.*"workspace_id":"\([^"]*\)".*/\1/p'
}

# Print the first MRU entry worth jumping to, or nothing.
#
#   pick_target <mru-path> <current-id> <live-ids>
#
# Entries are filtered against the *live* current id rather than assuming the
# head of the list is accurate, so a missed focus event cannot make the jump
# land on the tab the user is already looking at.
#
# Filtering against <live-ids> is required rather than defensive: `herdr tab
# focus` on a deleted tab exits 0, so a try-and-check-status fallback would
# silently do nothing instead of moving to the next candidate.
pick_target() {
    _path=$1
    _current=$2
    _live=$3
    read_mru "$_path" | while IFS= read -r _id; do
        if [ -z "$_id" ]; then
            continue
        fi
        if [ "$_id" = "$_current" ]; then
            continue
        fi
        if printf '%s\n' "$_live" | grep -qxF "$_id"; then
            printf '%s\n' "$_id"
            break
        fi
    done | head -n 1
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `./test.sh`
Expected: PASS — `30 tests, 0 failures`

- [ ] **Step 5: Commit**

```sh
git add lib.sh test.sh
git commit -m "feat: add live id queries and MRU target selection"
```

---

### Task 5: Jump entrypoints

**Files:**
- Create: `jump-tab.sh`
- Create: `jump-workspace.sh`
- Modify: `test.sh` (append jump section before the summary block)

**Interfaces:**
- Consumes: `session_dir`, `pick_target`, `live_tab_ids`, `live_workspace_ids`, `herdr_bin` from Tasks 1 and 4.
- Produces: two executable entrypoints. `jump-tab.sh` focuses the last live tab in `HERDR_WORKSPACE_ID`; `jump-workspace.sh` focuses the last live workspace. Both exit 0 and issue no focus call when there is no valid target.

- [ ] **Step 1: Write the failing test**

Insert into `test.sh` immediately **before** the `# --- summary` block:

```sh
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
```

- [ ] **Step 2: Run test to verify it fails**

Run: `./test.sh`
Expected: FAIL — `jump-tab.sh: No such file or directory`

- [ ] **Step 3: Write minimal implementation**

Create `jump-tab.sh`:

```sh
#!/bin/sh
# Herdr action: focus the most recently visited still-live tab in this
# workspace. Equivalent to tmux's `last-window`.
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
. "$SCRIPT_DIR/lib.sh"

ws=${HERDR_WORKSPACE_ID:-}
if [ -z "$ws" ]; then
    exit 0
fi

target=$(pick_target "$(session_dir)/tabs/$ws" "${HERDR_TAB_ID:-}" "$(live_tab_ids "$ws")")
if [ -z "$target" ]; then
    exit 0
fi

"$(herdr_bin)" tab focus "$target" >/dev/null 2>&1 || true
```

Create `jump-workspace.sh`:

```sh
#!/bin/sh
# Herdr action: focus the most recently visited still-live workspace.
# Equivalent to tmux's `switch-client -l`.
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
. "$SCRIPT_DIR/lib.sh"

target=$(pick_target "$(session_dir)/workspaces" "${HERDR_WORKSPACE_ID:-}" "$(live_workspace_ids)")
if [ -z "$target" ]; then
    exit 0
fi

"$(herdr_bin)" workspace focus "$target" >/dev/null 2>&1 || true
```

Make both executable:

```sh
chmod +x jump-tab.sh jump-workspace.sh
```

- [ ] **Step 4: Run test to verify it passes**

Run: `./test.sh`
Expected: PASS — `37 tests, 0 failures`

- [ ] **Step 5: Commit**

```sh
git add jump-tab.sh jump-workspace.sh test.sh
git commit -m "feat: add last-tab and last-workspace jump actions"
```

---

### Task 6: Manifest, README, and live verification

**Files:**
- Create: `herdr-plugin.toml`
- Create: `README.md`

**Interfaces:**
- Consumes: all four entrypoints from Tasks 3 and 5.
- Produces: an installable plugin. Qualified action ids for keybindings are `m4salah.last.last-tab` and `m4salah.last.last-workspace`.

- [ ] **Step 1: Write the manifest**

Create `herdr-plugin.toml`:

```toml
id = "m4salah.last"
name = "Last"
version = "0.1.0"
min_herdr_version = "0.7.5"
description = "tmux-style last-tab and last-workspace navigation"
platforms = ["linux", "macos"]

[[events]]
on = "tab.focused"
command = ["sh", "record-tab.sh"]

[[events]]
on = "workspace.focused"
command = ["sh", "record-workspace.sh"]

[[actions]]
id = "last-tab"
title = "Last tab"
contexts = ["workspace"]
command = ["sh", "jump-tab.sh"]

[[actions]]
id = "last-workspace"
title = "Last workspace"
contexts = ["workspace"]
command = ["sh", "jump-workspace.sh"]
```

Runtime commands run with the plugin directory as their working directory, so the bare script names resolve.

- [ ] **Step 2: Link the plugin and verify it registers**

```sh
herdr plugin link "$PWD"
herdr plugin action list --plugin m4salah.last
```

Expected: both `last-tab` and `last-workspace` listed. If linking reports a manifest error, fix the manifest before continuing.

- [ ] **Step 3: Verify the record hooks fire**

Switch between two tabs in one workspace by hand, then:

```sh
herdr plugin log list --plugin m4salah.last --limit 10
find "$(herdr plugin config-dir m4salah.last | sed 's/config/state/')" -type f
```

Expected: log entries for `tab.focused`, and a `tabs/<workspace>` file containing the ids of the tabs just visited, most recent first.

If the state path above does not resolve, get it from the plugin's own view instead:

```sh
herdr plugin action invoke m4salah.last.last-tab
herdr plugin log list --plugin m4salah.last --limit 3
```

- [ ] **Step 4: Bind the keys**

Add to `~/.config/herdr/config.toml`:

```toml
[[keys.command]]
key = "prefix+semicolon"
type = "plugin_action"
command = "m4salah.last.last-tab"
description = "last tab"

[[keys.command]]
key = "prefix+quote"
type = "plugin_action"
command = "m4salah.last.last-workspace"
description = "last workspace"
```

Reload:

```sh
herdr server reload-config
```

- [ ] **Step 5: Verify the full loop by hand**

Check each of these:

1. Visit tab 1, then tab 2. Press `prefix+;` → lands on tab 1. Press again → back on tab 2.
2. Visit workspace A, then workspace B. Press `prefix+'` → lands on A. Press again → back on B.
3. In a workspace with three tabs, visit 1, 2, 3, close tab 2, then from tab 3 press `prefix+;` → lands on tab 1, not an error.
4. In a fresh workspace with a single tab, press `prefix+;` → nothing happens, no error popup.
5. `prefix+?` shows "last tab" and "last workspace" in the keybind help panel.

- [ ] **Step 6: Write the README**

Create `README.md`:

````markdown
# herdr-plugin-last

tmux-style `last-window` and `switch-client -l` for [Herdr](https://herdr.dev).

- **last tab** — jump to the most recently visited tab *in the current workspace*
- **last workspace** — jump to the most recently visited workspace

Both toggle: press twice and you are back where you started.

## Requires

Herdr 0.7.5+, Linux or macOS.

## Install

```bash
herdr plugin install m4salah/herdr-plugin-last
```

Then bind the keys in `~/.config/herdr/config.toml`:

```toml
[[keys.command]]
key = "prefix+semicolon"
type = "plugin_action"
command = "m4salah.last.last-tab"
description = "last tab"

[[keys.command]]
key = "prefix+quote"
type = "plugin_action"
command = "m4salah.last.last-workspace"
description = "last workspace"
```

Reload with `herdr server reload-config`.

## How it works

Two event hooks on `tab.focused` and `workspace.focused` keep a small
most-recently-used list (8 entries) under the plugin's state directory, keyed by
session socket path so concurrent named sessions stay independent. The two
actions read that list, skip the id you are currently on, skip anything that no
longer exists, and focus the first survivor.

Jumping fires another focus event, which is what makes repeated presses toggle.

## Behaviour notes

- Closing the tab you would jump back to is fine — the next-most-recent live tab
  is used instead.
- With no history yet, or nothing live to jump to, the key does nothing. It never
  raises an error.
- Two focus changes within a few milliseconds can race on the history file and
  drop one update, leaving the order briefly stale. It corrects itself on the
  next switch.

## Development

```bash
herdr plugin link "$PWD"
./test.sh
```

`test.sh` needs no dependencies. It runs the scripts against a temporary state
directory with a stub `herdr` binary.
````

- [ ] **Step 7: Run the full test suite once more**

Run: `./test.sh`
Expected: PASS — `37 tests, 0 failures`

- [ ] **Step 8: Commit**

```sh
git add herdr-plugin.toml README.md
git commit -m "feat: add plugin manifest and readme"
```

---

## Self-Review

**Spec coverage:**

| Spec requirement | Task |
|---|---|
| MRU model, depth 8, skip dead entries | 2, 4 |
| POSIX sh, Linux + macOS only | all; declared in 6 |
| State keyed by sanitised full socket path | 1 (with named-session regression test in 1 and 2) |
| Atomic temp-write + rename | 1 |
| Record on `tab.focused` / `workspace.focused` | 3 |
| Jump filters on live current id, not list head | 4 (`pick_target`), test "still works when the current id is unknown" |
| Validate against `tab list` because focus exits 0 | 4 |
| Toggle semantics | 5 (explicit toggle test) |
| No startup hook | 6 (manifest has none) |
| `prefix+semicolon` / `prefix+quote` bindings | 6 |
| Plugin id `m4salah.last`, `min_herdr_version = "0.7.5"` | 6 |
| Test coverage listed in spec | 1-5 |
| Race limitation documented, not solved | 6 (README) |

No gaps.

**Placeholder scan:** No TBD/TODO. Every code step contains complete runnable content. No "similar to Task N" references.

**Type consistency:** `session_dir`, `read_mru`, `write_atomic`, `record_mru`, `live_tab_ids`, `live_workspace_ids`, `pick_target`, `herdr_bin`, `MRU_LIMIT` are each defined once and used with matching arity throughout. Test helpers `assert_eq`, `assert_ne`, `new_sandbox`, `drop_sandbox`, `make_stub_herdr`, `stub_tabs_two`, `stub_workspaces_two` are defined before first use — note `make_stub_herdr` is defined in Task 4's insert and used again in Task 5's, and since both are appended before the summary block in order, Task 4's definition precedes Task 5's use.

One inconsistency found and fixed inline: Task 5's first test wrote the history file before its parent directory existed. It now calls `mkdir -p "$(session_dir)/tabs"` first, matching the other jump tests.
