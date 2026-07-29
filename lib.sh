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
    read_mru "$_path" | while IFS= read -r _id || [ -n "$_id" ]; do
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
