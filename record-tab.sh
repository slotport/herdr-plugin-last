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

# session_dir() is called here, not inline in the record_mru argument: in
# `$(session_dir)/tabs/$ws`, session_dir's `:?` only aborts the subshell, so
# an unset HERDR_PLUGIN_STATE_DIR would leave the parent script continuing
# with an empty string and writing under the filesystem root instead.
# Stderr is discarded too: command substitution only captures stdout, so
# session_dir's `:?` diagnostic would otherwise still leak to the terminal
# even though this script must stay silent.
dir=$(session_dir 2>/dev/null) || exit 0
if [ -z "$dir" ]; then
    exit 0
fi

record_mru "$dir/tabs/$ws" "$tab"
