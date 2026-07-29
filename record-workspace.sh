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

# session_dir() is called here, not inline in the record_mru argument: in
# `$(session_dir)/workspaces`, session_dir's `:?` only aborts the subshell,
# so an unset HERDR_PLUGIN_STATE_DIR would leave the parent script continuing
# with an empty string and writing under the filesystem root instead.
# Stderr is discarded too: command substitution only captures stdout, so
# session_dir's `:?` diagnostic would otherwise still leak to the terminal
# even though this script must stay silent.
dir=$(session_dir 2>/dev/null) || exit 0
if [ -z "$dir" ]; then
    exit 0
fi

record_mru "$dir/workspaces" "$ws"
