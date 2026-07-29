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
