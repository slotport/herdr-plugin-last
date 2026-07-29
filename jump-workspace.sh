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
