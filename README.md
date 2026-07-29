# herdr-plugin-last

tmux-style `last-window` and `switch-client -l` for [Herdr](https://herdr.dev).

- **last tab** — jump to the most recently visited tab *in the current workspace*
- **last workspace** — jump to the most recently visited workspace

Both toggle: press twice and you are back where you started.

## Requires

Herdr 0.7.5+, Linux or macOS.

## Install

```bash
herdr plugin install m4salah/herdr-plugin-last
```

Then bind the keys in `~/.config/herdr/config.toml`:

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

Reload with `herdr server reload-config`.

## How it works

Two event hooks on `tab.focused` and `workspace.focused` keep a small
most-recently-used list (8 entries) under the plugin's state directory. The two
actions read that list, skip the id you are currently on, skip anything that no
longer exists, and focus the first survivor.

Jumping fires another focus event, which is what makes repeated presses toggle.
There is no explicit toggle state.

Two details are load-bearing and deliberately not simplified:

- **State is keyed by the full socket path, not its basename.** Herdr's named
  sessions live at `~/.config/herdr/sessions/<name>/herdr.sock` and the default
  at `~/.config/herdr/herdr.sock`, so every session shares the basename
  `herdr.sock`. Keying on that would make concurrent sessions share one history.
- **Candidates are checked against the live id list before focusing.** On Herdr
  0.7.5, `herdr tab focus <dead-id>` prints a JSON error but exits 0, so exit
  status cannot be used to fall through to the next candidate.

## Behaviour notes

- Closing the tab you would jump back to is fine — the next-most-recent live tab
  is used instead.
- With no history yet, or nothing live to jump to, the key does nothing. It never
  raises an error and never writes to the terminal.
- Two focus changes within a few milliseconds can race on the history file and
  drop one update, leaving the order briefly stale. It corrects itself on the
  next switch.
- History is not cleared on restart. Ids that no longer exist are skipped, so a
  stale entry costs nothing.

## Development

```bash
herdr plugin link "$PWD"
./test.sh
```

`test.sh` needs no dependencies — no jq, no node, no test framework. It runs the
real scripts against a temporary state directory with a stub `herdr` binary, and
is hermetic: it passes identically inside a Herdr session and outside one.

Tests are mutation-checked. Each defensive detail in the implementation has a
test that fails when that detail is removed — the `grep -x` whole-line match, the
`grep -F` literal match, the missing-variable guards, and the unterminated-final-
line handling in `pick_target`. If you change any of those, a test should go red.
