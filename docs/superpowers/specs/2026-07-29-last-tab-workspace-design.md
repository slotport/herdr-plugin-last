# herdr-plugin-last — design

Date: 2026-07-29
Status: approved, ready for implementation planning

## Goal

Give Herdr the two tmux bindings that are missing from it:

```tmux
bind-key ";" last-window
bind-key "'" switch-client -l
```

- `last-tab` — focus the most recently visited tab **within the current workspace**.
- `last-workspace` — focus the most recently visited workspace.

Both must toggle: pressing the key twice returns you to where you started.

## Why a plugin

Herdr has no built-in last-tab or last-workspace action. It does expose
everything needed to build one externally, so this requires no changes to Herdr
core:

- `tab.focused` and `workspace.focused` are both in `PLUGIN_HOOK_EVENT_KINDS`
  (`src/api/schema/events.rs:286`), so a plugin can observe every focus change.
- Event hooks and actions receive `HERDR_WORKSPACE_ID`, `HERDR_TAB_ID`,
  `HERDR_SOCKET_PATH`, `HERDR_BIN_PATH`, and `HERDR_PLUGIN_STATE_DIR`
  (`src/app/api/plugins/runtime.rs:40-72`).
- `herdr tab focus` and `herdr workspace focus` exist in the CLI.
- Actions bind to keys via `[[keys.command]] type = "plugin_action"`.

## Decisions

| Decision | Choice | Rationale |
|---|---|---|
| History model | MRU list, skip dead entries | Tabs get closed often; a single stored id would frequently dead-end |
| MRU depth | 8 per list | Deep enough to survive a few closes, shallow enough to stay cheap |
| Runtime | POSIX `sh` | Record hook runs on every focus change; `sh` spawns in ~2-3ms with no runtime dependency |
| Platforms | `["linux", "macos"]` | No Windows support in v1 |
| Location | Own git repo, publishable | Can be tagged `herdr-plugin` for the marketplace later |
| Plugin id | `m4salah.last` | |

## Architecture

Four manifest entries. Two record hooks write history, two actions read it.

| Manifest entry | Fires when | Does |
|---|---|---|
| `[[events]] on = "tab.focused"` | any tab focus | prepend `$HERDR_TAB_ID` to that workspace's MRU file |
| `[[events]] on = "workspace.focused"` | any workspace focus | prepend `$HERDR_WORKSPACE_ID` to the global MRU file |
| `[[actions]] id = "last-tab"` | keybinding | focus most recent still-live tab in this workspace |
| `[[actions]] id = "last-workspace"` | keybinding | focus most recent still-live workspace |

The record hooks need no JSON parsing: Herdr injects the ids as environment
variables directly. Each hook is roughly ten lines of `sh`.

### Toggle behaviour is emergent

Jumping to a tab causes Herdr to emit `tab.focused`, which re-prepends the
tab just landed on. The top two entries swap, so pressing the key again returns
to the original tab. This is exactly tmux's `last-window` behaviour and requires
no explicit toggle state.

Worked example, current tab first:

```
[B, A, C]   on B, press last-tab
[A, B, C]   now on A
[B, A, C]   press again, back on B
```

## State layout

```
$HERDR_PLUGIN_STATE_DIR/
  <session-key>/
    workspaces          # one workspace id per line, most recent first
    tabs/
      w5                # one tab id per line, most recent first
      w6534795cc50391
```

`<session-key>` is `$HERDR_SOCKET_PATH` with `/` replaced by `_`.

Sanitising the **whole** socket path is required, not cosmetic. Named sessions
live at `~/.config/herdr/sessions/<name>/herdr.sock` while the default session
is `~/.config/herdr/herdr.sock` (`src/session.rs:161-171`) — the basename is
always `herdr.sock`, so keying on basename alone would make concurrent named
sessions share and corrupt one history.

Workspace ids are not always short: real sessions contain ids like
`w6534795cc50391` alongside `w5`. Tab ids are `<workspace_id>:t<n>`
(`src/workspace.rs:141`), so the workspace is the prefix before the first `:`,
but the hooks read `$HERDR_WORKSPACE_ID` directly rather than parsing.

### Writes

Each update rewrites the file to a temp file in the same directory and `mv`s it
into place. Rename is atomic, so a reader can never observe a half-written file.

## Record algorithm

On `tab.focused` for tab `T` in workspace `W`:

1. Read `tabs/<W>` if it exists.
2. Remove any line equal to `T`.
3. Prepend `T`.
4. Truncate to 8 lines.
5. Atomically replace `tabs/<W>`.

`workspace.focused` is identical against the single `workspaces` file.

## Jump algorithm

For `last-tab`:

1. `ws=$HERDR_WORKSPACE_ID`; exit quietly if unset.
2. Read `tabs/$ws`; exit quietly if missing or empty.
3. Drop every entry equal to `$HERDR_TAB_ID` — the *live* current tab.
4. Collect live tab ids: `herdr tab list --workspace "$ws"`.
5. Focus the first remaining MRU entry present in that live set.
6. If none survives, exit 0 silently.

`last-workspace` is identical using the `workspaces` file,
`$HERDR_WORKSPACE_ID`, `herdr workspace list`, and `herdr workspace focus`.

Two properties of this algorithm are load-bearing:

**Filter on the live current id, not the list head.** Assuming the head is
accurate would make the action wrong if a focus event were ever missed or
dropped. Comparing against `$HERDR_TAB_ID` from the action's own invocation
context keeps it correct regardless of history quality.

**Validating against `tab list` is required, not defensive.** `herdr tab focus`
on a dead tab prints a `tab_not_found` JSON error but **exits 0**, verified
directly:

```
$ herdr tab focus w5:t99
{"error":{"code":"tab_not_found","message":"tab w5:t99 not found"},"id":"cli:tab:focus"}
$ echo $status
0
```

Exit status therefore cannot drive a try-next-entry fallback. The live-id set
must be fetched first.

Ids are extracted without a JSON dependency:

```sh
"$HERDR_BIN_PATH" tab list --workspace "$ws" \
  | tr ',' '\n' \
  | sed -n 's/.*"tab_id":"\([^"]*\)".*/\1/p'
```

## No startup hook

An earlier draft reset state from a `[[startup]]` hook to avoid stale ids after
a restart. Dropped, for two reasons:

- Startup hooks also re-run on live handoff, so it would wipe history
  mid-session for no benefit.
- Validating on read already handles deleted ids, and session restore brings
  tab ids back pointing at the same tabs.

## Keybindings

`semicolon` and `quote` are the correct Herdr key names for `;` and `'`
(`src/config/keybinds.rs:1242,1244`).

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

## Manifest

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

`min_herdr_version = "0.7.5"` matches the version these APIs were verified
against. Action ids may not contain dots; `last-tab` and `last-workspace` are
valid.

## Layout

```
herdr-plugin-last/
  herdr-plugin.toml
  lib.sh                  # shared: state paths, atomic write, MRU update
  record-tab.sh
  record-workspace.sh
  jump-tab.sh
  jump-workspace.sh
  test.sh
  README.md
```

`lib.sh` holds the logic both record scripts and both jump scripts share —
resolving the session state directory, the atomic replace, the MRU prepend, and
the live-id extraction. The four entrypoints stay thin so each one is readable
on its own.

## Testing

`test.sh` runs the scripts against a temporary `HERDR_PLUGIN_STATE_DIR` with a
stub `herdr` binary on `HERDR_BIN_PATH` that returns canned `tab list` /
`workspace list` JSON and records the focus calls it receives.

Record cases:

- builds MRU in most-recent-first order
- re-focusing an already-present tab moves it to the front without duplicating
- caps at 8 entries, dropping the oldest
- keeps separate histories for separate workspaces
- keeps separate histories for two different socket paths

Jump cases:

- skips entries equal to the current id
- skips ids absent from the live set
- focuses the first surviving entry
- exits 0 and issues no focus call when the file is missing, empty, or fully dead
- A -> B -> A toggles across repeated invocations, driving record between jumps

Then a manual pass in a real session: link the plugin, bind the keys, switch
around, confirm both keys toggle and that closing the target tab degrades to the
next MRU entry.

## Known limitation

Two focus events landing within a few milliseconds can race on the same MRU
file and lose one update, leaving the list briefly stale. A lock would fix it,
but at the cost of a lock acquisition on every focus change to correct an
ordering that self-heals on the next switch. Accepted, not solved.

## Out of scope

- Windows support.
- Pane-level last-pane.
- A picker UI listing recent tabs.
- Persisting history across a full Herdr restart as a guaranteed contract.
