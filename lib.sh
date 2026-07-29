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
