# herdr-plugin-last

tmux-style `last-window` and `switch-client -l` for [Herdr](https://herdr.dev).

- **last tab** — jump to the most recently visited tab *in the current workspace*
- **last workspace** — jump to the most recently visited workspace

Both toggle: press twice and you are back where you started.

## Requires

Herdr 0.7.5+, Linux or macOS, and a Rust toolchain.

`herdr plugin install` compiles the plugin from source. Herdr captures build
output and only shows it on failure, so install looks like it has hung for the
duration of the build — usually under 10 seconds cold, longer on a loaded
machine.

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
cargo test
```

`herdr plugin link` does **not** run manifest build commands, so a linked plugin
happily keeps running a stale binary until you rebuild. Rebuild before linking,
and after any change you want the live session to pick up.

`cargo test` runs everything — 43 unit tests and 20 integration tests.

The unit tests in `src/` cover the pure logic: MRU ordering and capping, target
selection, and CLI response parsing. The integration tests in `tests/cli.rs`
cover what unit tests cannot reach, because they spawn the real binary: argv
dispatch, exit codes, silence on every path, and which `herdr` calls each
entrypoint actually issues. Nothing else verifies that `main.rs` wires the
modules together correctly.

They run the binary against a temporary state directory with a stub `herdr`,
and they are hermetic by construction: each child process starts from
`env_clear()` with only the variables that test needs, so ambient `HERDR_*`
values from a live Herdr session cannot leak in and quietly satisfy a guard
that was supposed to fire. That also makes them parallel-safe with no locking,
since they never touch the parent process's environment.

Three defects the shell version had to fix by hand are structurally impossible
in the port: dropping an MRU file's final entry when it lacks a trailing
newline, matching ids by prefix or as a regex instead of literally, and
continuing with an empty state directory when `HERDR_PLUGIN_STATE_DIR` is
missing. Their tests remain as regression guards.
