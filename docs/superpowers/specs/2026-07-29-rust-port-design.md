# herdr-plugin-last — Rust port design

Date: 2026-07-29
Status: approved, ready for implementation planning
Supersedes the runtime choice in `2026-07-29-last-tab-workspace-design.md`. Every
behavioural decision in that document still stands.

## Goal

Replace the four POSIX `sh` entrypoints and `lib.sh` with a single compiled Rust
binary, with **no user-visible change in behaviour**. The plugin keeps its id,
its actions, its keybindings, its state format, and its silence.

This is a port, not a redesign. Anything that alters what a user observes is out
of scope.

## Why

Four motivations, all endorsed:

| Motivation | What it actually buys |
|---|---|
| Fewer processes | `record-tab.sh` fires on **every tab focus** and spawns ~10 processes doing it (`sh`, `dirname`, `mktemp`, `cat`, `grep`, `head`, `mv`, …). One static binary replaces all of them. |
| Real JSON parsing | `live_tab_ids` currently extracts ids with `tr ',' '\n' \| sed -n 's/.*"tab_id":"\([^"]*\)".*/\1/p'`. That survives today only because a reviewer empirically confirmed Herdr escapes quotes in user-set tab labels (BUILD-NOTES, Task 4). A parser removes a standing fragility. |
| Learning | Preference for working in Rust. Legitimate, and it shifts the tie-breaker toward idiomatic code over minimal user friction. |
| Ecosystem fit | Herdr is Rust; the plugin reads like the thing it extends. |

The speed argument is won entirely by compiling. **The hot path never talks to
Herdr** — the record hooks only write a file. Only the jump path, which fires at
keypress rate, invokes the `herdr` CLI. This is why the jump path's I/O strategy
is a design-taste question rather than a performance one.

## Herdr capability findings

Verified against Herdr 0.7.5 and the source at `~/Desktop/m4salah/herdr`
(`2cc88e5`). A compiled plugin is supported:

- Manifests accept `[[build]]` entries with `command` and an optional
  `platforms` filter (`src/app/api/plugins/manifest.rs:23,158-161,260-264`;
  documented in `docs/next/website/src/content/docs/plugins.mdx:69-73`).
- Build commands run at install time, after the trust preview and before
  registration (`src/cli/plugin.rs:205,210`). A failing build aborts the install
  and the plugin is not registered.
- The trust preview prints every build command, so `cargo build --release` is
  visible to the user before they confirm (`print_install_preview`).
- The manifest is re-read after the build and must be unchanged
  (`ensure_manifest_unchanged_after_build`, `src/cli/plugin.rs:215`) — a build
  step cannot rewrite the manifest to point at what it just produced.
- Plugin commands spawn with `current_dir(plugin_root)`
  (`src/app/api/plugins/runtime.rs:123`), so `./target/release/last` resolves.

Three constraints follow, and all three are load-bearing:

**`plugin link` does not run build commands**
(`docs/next/website/src/content/docs/plugins.mdx:223`). The build is skipped
exactly where iteration happens, so the dev loop must build explicitly.

**Build output is invisible until it fails.** `run_plugin_build_command`
(`src/cli/plugin.rs:1266`) pipes stdout and stderr to capped reader threads and
surfaces them only in the error branch. There is no timeout on `child.wait()`.
An install therefore shows nothing at all while `cargo` runs.

**Every installing user needs a Rust toolchain.** This is a real regression in
installability against `sh`, accepted deliberately, and it must be stated in the
README under Requires.

Measured, not estimated: a cold `cargo build --release` of a binary crate with
`serde`, `serde_derive`, and `serde_json` takes **8.55s** on this machine
(`tempfile` was not in the probe and will add marginally). The silent window is
small enough that minimising dependencies is not a real constraint.

## Decisions

| Decision | Choice | Rationale |
|---|---|---|
| Herdr I/O on the jump path | Spawn the `herdr` CLI, parse with serde | Keeps `min_herdr_version` at 0.7.5, no socket-protocol coupling. The socket API would remove two spawns per *keypress* — imperceptible — in exchange for owning framing and versioning. |
| Distribution | `cargo build --release` at install | One manifest line, no release pipeline, no committed binaries. Cost is the toolchain requirement and the 8.5s silent build. |
| Test strategy | Keep `test.sh`, retargeted; add Rust unit tests | See Testing. |
| Binary name | `last` | Manifest reads `./target/release/last`. Cargo package stays `herdr-plugin-last`. |
| Atomic write | `tempfile` crate | `NamedTempFile::persist` is the `mktemp`+`mv` dance done correctly, including the `O_EXCL` retry. Not worth reimplementing. |
| Dependencies | `serde`, `serde_json`, `tempfile` | Measured build cost is 8.5s. |
| Platforms | `["linux", "macos"]` | Unchanged. No Windows. |
| Plugin id / version | `m4salah.last`, bump to `0.2.0` | Same id so existing installs upgrade in place. |

## Architecture

### Manifest

```toml
id = "m4salah.last"
name = "Last"
version = "0.2.0"
min_herdr_version = "0.7.5"
description = "tmux-style last-tab and last-workspace navigation"
platforms = ["linux", "macos"]

[[build]]
command = ["cargo", "build", "--release"]

[[events]]
on = "tab.focused"
command = ["./target/release/last", "record-tab"]

[[events]]
on = "workspace.focused"
command = ["./target/release/last", "record-workspace"]

[[actions]]
id = "last-tab"
title = "Last tab"
contexts = ["workspace"]
command = ["./target/release/last", "jump-tab"]

[[actions]]
id = "last-workspace"
title = "Last workspace"
contexts = ["workspace"]
command = ["./target/release/last", "jump-workspace"]
```

`[[build]]` needs no `platforms` filter: it inherits the plugin's
`["linux", "macos"]`, and `cargo` is the same invocation on both.

### Modules

One binary, four subcommands, four modules. Each module has one job and no
knowledge of the others.

| Module | Responsibility | Depends on |
|---|---|---|
| `main.rs` | argv dispatch and the exit-code policy | all |
| `context.rs` | read and validate the `HERDR_*` environment; compute the session directory | — |
| `mru.rs` | read, record, atomically write, and `pick_target` over an MRU file | tempfile |
| `herdr.rs` | spawn the `herdr` CLI, parse its JSON, focus a tab or workspace | serde |

`mru.rs` does not know Herdr exists. `herdr.rs` does not know the MRU exists.
`main.rs` wires them together and owns every decision about exit codes. That
separation is what makes the pure logic unit-testable, which the shell version
could only approach through the filesystem.

### Layout

```
herdr-plugin-last/
  Cargo.toml
  Cargo.lock          # committed — install-time builds must be reproducible
  src/
    main.rs
    context.rs
    mru.rs
    herdr.rs
  herdr-plugin.toml
  test.sh
  README.md
  .gitignore          # + /target
```

`lib.sh`, `record-tab.sh`, `record-workspace.sh`, `jump-tab.sh`, and
`jump-workspace.sh` are deleted.

## Behaviour held invariant

Carried over unchanged from the shell implementation. Every item here is
observable, and every item here has test coverage today.

1. The session directory is keyed on the **full** `HERDR_SOCKET_PATH` with `/`
   replaced by `_`, never the basename. Named sessions live at
   `~/.config/herdr/sessions/<name>/herdr.sock` and the default at
   `~/.config/herdr/herdr.sock`, so every session shares the basename
   `herdr.sock`; keying on it would make concurrent sessions share one history.
2. `MRU_LIMIT = 8`. Recording an id already present moves it to the front rather
   than duplicating it.
3. A blank id is a no-op that writes nothing.
4. Jump candidates are filtered against the **live** id list. This is required,
   not defensive: `herdr tab focus <dead-id>` prints a JSON error and **exits
   0**, so exit status cannot drive a try-the-next-candidate fallback.
5. Jump candidates are also filtered against the live current id from the
   invocation environment, not against the head of the MRU list, so a missed
   focus event cannot land the jump on the tab the user is already looking at.
6. Toggling stays emergent. Focusing a tab emits `tab.focused`, which re-records
   it, swapping the top two entries. There is no explicit toggle state.

### State compatibility

The on-disk format does not change: newline-delimited ids, most recent first, at
`$HERDR_PLUGIN_STATE_DIR/<session-key>/workspaces` and
`$HERDR_PLUGIN_STATE_DIR/<session-key>/tabs/<workspace-id>`.

An existing user's history therefore survives the upgrade — the Rust binary
reads exactly what the shell version wrote. No migration step exists, and none
is needed.

## Exit-code and output policy

This is the single most important thing to port correctly, because it is the
part most easily lost in translation. The shell version's asymmetry is
deliberate and documented in the scripts themselves.

| Path | On a guard condition | On an operational failure |
|---|---|---|
| `record-tab`, `record-workspace` | exit 0, silent | **exit non-zero**, so the failure reaches Herdr's plugin log |
| `jump-tab`, `jump-workspace` | exit 0, silent | exit 0, silent |

Guard conditions are: a missing or empty `HERDR_WORKSPACE_ID` / `HERDR_TAB_ID`,
and an unavailable `HERDR_PLUGIN_STATE_DIR`. Operational failures are things
like a failed write.

A jump is a best-effort convenience; a failed one must never interrupt the user
or print anything. A failed *record* is a real fault worth surfacing to the log.

**Nothing reaches stdout or stderr on any path in normal operation.** The
`herdr` CLI's own output is captured and discarded, never inherited.

In `main.rs` this is one `match` on the subcommand: the record arms propagate
their `Result`, the jump arms discard theirs.

## What the port fixes structurally

Three defects the shell version needed work to reach become impossible rather
than merely fixed. Their tests remain as regression guards.

**Unterminated final line.** `pick_target` silently dropped the last MRU entry
when the file lacked a trailing newline — a real bug found in Task 4, fixed with
`|| [ -n "$_id" ]`. Rust's `str::lines()` handles it natively.

**Whole-line, literal matching.** The shell dedup and live-set membership checks
need both `-x` and `-F` on `grep` to avoid substring and regex matches; Tasks 2
and 4 each added tests after finding this. Rust string equality is whole-value
and literal by construction.

**`${VAR:?}` in argument position.** The Important finding at final review:
`${HERDR_PLUGIN_STATE_DIR:?}` aborts only the `$( )` subshell, so all four
entrypoints continued with an empty state directory and wrote to `/tabs/<ws>` at
the filesystem root. The fix needed a `dir=$(session_dir 2>/dev/null) || exit 0`
dance plus an eight-line comment in each of the four scripts explaining it.

In Rust, `context.rs` returns an `Option<Context>` and each entrypoint opens
with a `let ... else { return Ok(()) }`. The failure mode cannot be expressed.
All four comment blocks delete.

## Herdr CLI parsing

Real response shapes, captured from the live 0.7.5 session:

```json
{"id":"cli:tab:list","result":{"tabs":[{"tab_id":"w3:t1","workspace_id":"w3", ...}]}}
{"id":"cli:workspace:list","result":{"type":"workspace_list","workspaces":[{"workspace_id":"w3", ...}]}}
{"id":"cli:tab:focus","error":{"code":"tab_not_found","message":"tab w5:t99 not found"}}
```

Responses are an envelope carrying either `result` or `error`:

```rust
#[derive(Deserialize)]
struct Envelope<T> { result: Option<T> }

#[derive(Deserialize)]
struct TabList { tabs: Vec<Tab> }

#[derive(Deserialize)]
struct Tab { tab_id: String }
```

Two rules for these types:

- **`result` is `Option`.** An error response deserialises to `None`, which
  yields an empty live set, which yields no jump. That is precisely the intended
  degradation, and it matches what the current `sed` pipeline does by accident.
- **Never `deny_unknown_fields`.** The responses carry `label`, `agent_status`,
  `pane_count`, and more, and Herdr will add fields. Only `tab_id` and
  `workspace_id` are read; everything else must be tolerated.

A non-zero exit from the `herdr` CLI, unparseable output, or a spawn failure are
all treated as an empty live set. Silent, exit 0.

## Testing

### The suite splits in two

The 62-assertion suite is the highest-value asset in this repository. It is
hermetic (verified under `dash`, inside a live Herdr session, and against
hostile ambient environment values), it caught a real bug and two rounds of
vacuous tests, and it is mutation-checked.

It is **not** uniformly black-box, which an earlier draft of this document got
wrong. `test.sh:47` sources `lib.sh` and then tests its functions directly. The
assertions divide cleanly:

| Group | Count | Fate |
|---|---|---|
| Invoke an entrypoint as a subprocess | 29 | Retargeted, not rewritten |
| Call a `lib.sh` function directly | 33 | Reimplemented as Rust unit tests |

The 33 cover `MRU_LIMIT`, `herdr_bin`, `session_dir`, `read_mru`,
`write_atomic`, `record_mru`, `live_tab_ids`, and `pick_target`. Every case
carries over one-for-one, including the three regression guards that matter
most: the prefix collision, the regex metacharacter, and the unterminated final
line. No case is dropped. Totals go from 62 to 67.

`live_tab_ids` cannot be unit-tested against a shell stub from Rust, so
`herdr.rs` splits into pure `parse_tab_ids` / `parse_workspace_ids` functions
plus thin spawn wrappers. The parsers carry the logic and are unit-tested; the
wrappers are covered end-to-end by `test.sh`'s stub binary. This is a better
boundary regardless of testing.

### The 29 that survive

Retargeted:

```diff
-  sh record-tab.sh
+  "$BIN" record-tab
```

Everything else — the sandbox, the environment scrubbing that unsets
`HERDR_WORKSPACE_ID` / `HERDR_TAB_ID` / `HERDR_PANE_ID`, the stub binary, the
invocation log — stays as is. The invocation log is what catches guard-removal
mutations (Task 5, both findings), so it is not optional.

`test.sh` gains its own three-line `session_dir` helper to replace the one it
was sourcing. Reimplementing it test-side rather than calling into the binary is
deliberate: it asserts the binary writes where the *spec* says, not merely where
the binary thinks it should.

`test.sh` runs `cargo build --release` itself before the first assertion, so it
can never test a stale binary.

These 29 are a **differential test**: the same assertions that passed against
the shell implementation must pass against the Rust one. For a port, that is
exactly the right safety net.

### Rust unit tests

Added where a unit test is sharper than a black-box one, in `mru.rs`:

- recording caps the list at 8, dropping the oldest
- recording an id already present moves it to the front without duplicating
- a blank id writes nothing
- `pick_target` skips the current id
- `pick_target` skips ids absent from the live set
- `pick_target` returns the first survivor, and `None` when there is none
- `pick_target` handles a file with no trailing newline

### Verification bar

Neither suite is sufficient alone. The port is done when `test.sh` reports 0
failures, `cargo test` passes, **and** the plugin has been linked into the real
Herdr session with both keys confirmed working by hand — the same bar Task 6
held the shell version to.

## Dev loop

Because `plugin link` does not run build commands:

```bash
cargo build --release && herdr plugin link "$PWD"
./test.sh
```

`test.sh` building first absorbs most of the risk here, but the README must
state the two-step explicitly. A linked plugin silently running a stale binary
is the most likely way to waste an hour on this project.

## Risks

| Risk | Mitigation |
|---|---|
| Stale binary during development, since `plugin link` skips builds | `test.sh` builds first; README documents the two-step loop |
| Users need a Rust toolchain | Stated under README Requires; install fails loudly with cargo missing |
| 8.5s of silent apparent hang during install | Documented in the README so it does not read as a freeze |
| `./target/release/last` relative resolution | Verified against `runtime.rs:123`; must still be live-verified before the port is called done |
| Behavioural drift from the shell version | The retargeted `test.sh` is the control |

## Known limitations

Carried forward unchanged. Two focus events within a few milliseconds can still
race on the same MRU file and lose one update, leaving the order briefly stale;
it self-heals on the next switch. A lock would fix it at the cost of an
acquisition on every focus change. Still accepted, still not solved — and note
that compiling makes the race window *narrower*, not gone.

## Out of scope

- Any change to observable behaviour.
- Direct socket-API access (revisit only if the CLI spawn ever becomes a real
  cost).
- Windows support.
- Prebuilt binaries, GitHub Releases, or a CI pipeline.
- Pane-level last-pane, and a recent-tabs picker UI.
