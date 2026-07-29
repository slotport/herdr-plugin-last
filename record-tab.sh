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
