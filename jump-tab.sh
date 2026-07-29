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

# session_dir() is called here, not inline in the pick_target argument: in
# `$(session_dir)/tabs/$ws`, session_dir's `:?` only aborts the subshell, so
# an unset HERDR_PLUGIN_STATE_DIR would leave the parent script continuing
# with an empty string and reading from the filesystem root instead.
# Stderr is discarded too: command substitution only captures stdout, so
# session_dir's `:?` diagnostic would otherwise still leak to the terminal
# even though this script must stay silent.
dir=$(session_dir 2>/dev/null) || exit 0
if [ -z "$dir" ]; then
    exit 0
fi

target=$(pick_target "$dir/tabs/$ws" "${HERDR_TAB_ID:-}" "$(live_tab_ids "$ws")")
if [ -z "$target" ]; then
    exit 0
fi

# Jump actions swallow every failure and always exit 0, unlike the record
# hooks which let `set -e` surface a write failure to Herdr's plugin log. A
# jump is a best-effort convenience; a failed one should never interrupt or
# error out on the user, so `|| true` here is deliberate, not a missing
# error path to add back.
"$(herdr_bin)" tab focus "$target" >/dev/null 2>&1 || true
