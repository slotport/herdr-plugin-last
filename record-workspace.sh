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
