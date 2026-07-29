# herdr-plugin-last

tmux-style `last-window` and `switch-client -l` for [Herdr](https://herdr.dev).

- **last tab** — jump to the most recently visited tab *in the current workspace*
- **last workspace** — jump to the most recently visited workspace

Both toggle: press twice and you are back where you started.

## Requires

Herdr 0.7.5+, Linux or macOS, and a Rust toolchain.

`herdr plugin install` compiles the plugin from source. Herdr captures build
output and only shows it on failure, so install looks like it has hung for the
duration of the build — around 10 seconds cold.

## Install

```bash
herdr plugin install m4salah/herdr-plugin-last
```

A public repo is listed in Herdr's plugin marketplace by adding the GitHub topic
`herdr-plugin` to it.

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

`prefix+semicolon` mirrors tmux's `last-window`, but check it is free first —
Herdr's `last_pane` action ships unbound, and `prefix+;` is a popular place to
put it. If you already use it there, `prefix+tab` is a good alternative for
last-tab:

```toml
[[keys.command]]
key = "prefix+tab"
type = "plugin_action"
command = "m4salah.last.last-tab"
description = "last tab"
```

A binding that collides is not reported as an error, so it is worth checking
`~/.config/herdr/config.toml` rather than assuming.

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
cargo build --release && herdr plugin link "$PWD"
cargo test && ./test.sh
```

`herdr plugin link` does **not** run manifest build commands, so a linked plugin
happily keeps running a stale binary until you rebuild. `test.sh` builds before
asserting anything for exactly this reason.

`cargo test` runs 42 tests covering the pure logic: MRU ordering and capping,
target selection, and CLI response parsing. `test.sh` runs 33 assertions
covering what unit tests cannot reach — exit codes, silence, and which `herdr`
calls each entrypoint actually issues. Beyond a POSIX shell and a Rust
toolchain (`test.sh` invokes `cargo build --release` itself, see above), it
needs nothing — no jq, no node, no test framework. It runs the real binary
against a temporary state directory with a stub `herdr`, and is hermetic: it
passes identically inside a Herdr session and outside one.

Three defects the shell version had to fix by hand are structurally impossible
in the port: dropping an MRU file's final entry when it lacks a trailing
newline, matching ids by prefix or as a regex instead of literally, and
continuing with an empty state directory when `HERDR_PLUGIN_STATE_DIR` is
missing. Their tests remain as regression guards.
