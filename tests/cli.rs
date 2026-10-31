#![cfg(unix)]

//! End-to-end tests for the `last` binary's entrypoint layer.
//!
//! This is the Rust port of the old `test.sh`. The pure logic (MRU ordering,
//! target selection, JSON parsing) is unit-tested inside `src/`; nothing there
//! spawns the binary, so nothing there proves `main.rs` wires the modules
//! together correctly. These tests drive the real compiled binary as a
//! subprocess against a temporary state directory and a stub `herdr`, and
//! cover exactly what the unit tests cannot reach: exit codes, silence on
//! every guard/success path, and which `herdr` calls each entrypoint actually
//! issues.
//!
//! Two things are load-bearing and deliberately preserved from `test.sh`:
//!
//! - **The child's environment is set explicitly, never mutated on the test
//!   process.** Every [`Sandbox::command`] starts from `.env_clear()`. This
//!   repo is developed inside a live Herdr session, so `HERDR_WORKSPACE_ID`,
//!   `HERDR_TAB_ID`, and `HERDR_PANE_ID` are ambient in the test process's own
//!   environment; if they leaked into a child that is supposed to be missing
//!   them, a guard test would pass for the wrong reason. Because nothing here
//!   touches the process environment, these tests need no locking and are
//!   safe to run in parallel (contrast `src/`'s unit tests, which need an
//!   `ENV_LOCK` mutex for exactly this reason).
//! - **`session_dir` independently reimplements the session-key rule**
//!   (`HERDR_SOCKET_PATH` with every `/` replaced by `_`, or the literal
//!   `default` when unset/empty) rather than asking the binary where it
//!   wrote. That way these assertions check the binary against the *spec*,
//!   not against its own idea of where things go.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// Matches the socket path `test.sh` used throughout.
const SOCKET_PATH: &str = "/home/test/.config/herdr/herdr.sock";

/// Independent reimplementation of `Context::from_env` + `session_key` from
/// `src/context.rs`. Deliberately not a call into the binary: it exists so
/// tests assert the binary writes where the *spec* says, not merely where the
/// binary itself thinks it should.
fn session_dir(state_dir: &Path, socket_path: &str) -> PathBuf {
    let effective = if socket_path.is_empty() {
        "default"
    } else {
        socket_path
    };
    state_dir.join(effective.replace(std::path::MAIN_SEPARATOR, "_"))
}

/// A throwaway sandbox: a temp directory containing a `state` subdirectory
/// (`HERDR_PLUGIN_STATE_DIR`) plus, on demand, a stub `herdr` binary and its
/// data/log files. Dropping it removes everything.
struct Sandbox {
    root: tempfile::TempDir,
}

impl Sandbox {
    fn new() -> Self {
        let root = tempfile::tempdir().expect("create sandbox tempdir");
        fs::create_dir_all(root.path().join("state")).expect("create state dir");
        Sandbox { root }
    }

    fn path(&self) -> &Path {
        self.root.path()
    }

    fn state_dir(&self) -> PathBuf {
        self.path().join("state")
    }

    /// Where the binary is expected to write for this sandbox's fixed
    /// `HERDR_SOCKET_PATH`, per the independently reimplemented rule above.
    fn session_dir(&self) -> PathBuf {
        session_dir(&self.state_dir(), SOCKET_PATH)
    }

    /// A `Command` for the binary under test with a fully explicit child
    /// environment: `.env_clear()`, then only `HERDR_PLUGIN_STATE_DIR` and
    /// `HERDR_SOCKET_PATH`. Callers add whatever `HERDR_WORKSPACE_ID` /
    /// `HERDR_TAB_ID` / `HERDR_BIN_PATH` the case needs.
    fn command(&self, subcommand: &str) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_last"));
        cmd.env_clear();
        cmd.env("HERDR_PLUGIN_STATE_DIR", self.state_dir());
        cmd.env("HERDR_SOCKET_PATH", SOCKET_PATH);
        cmd.arg(subcommand);
        cmd
    }

    /// A `Command` for the missing-`HERDR_PLUGIN_STATE_DIR` guard tests:
    /// `.env_clear()` with no state dir set at all, and the child's working
    /// directory pinned inside the sandbox.
    ///
    /// The `current_dir` is not cosmetic. A regression where
    /// `Context::from_env` falls back to a relative empty path instead of
    /// returning `None` would write under the process's current working
    /// directory. Pinning the cwd here is what makes that kind of write land
    /// where the "creates no file" assertions look, instead of silently
    /// escaping them (in production, an unguarded relative write would
    /// quietly litter the plugin's install directory on every focus event).
    fn command_without_state_dir(&self, subcommand: &str) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_last"));
        cmd.env_clear();
        cmd.current_dir(self.path());
        cmd.arg(subcommand);
        cmd
    }

    /// Writes a stub `herdr` into the sandbox and points nothing at it yet
    /// (callers set `HERDR_BIN_PATH` explicitly on the commands that need
    /// it). Mirrors `test.sh`'s `make_stub_herdr` exactly:
    ///
    /// - every invocation's arguments are appended to an invocations log
    /// - `tab list` / `workspace list` serve canned JSON from files
    /// - `tab focus <id>` / `workspace focus <id>` append to a calls log
    /// - only the two focus branches emit noise, on *both* stdout and
    ///   stderr — never the list branches, whose stdout is parsed as JSON
    ///
    /// That noise is load-bearing: it is what makes the success-path silence
    /// assertions able to catch a regression from `.output()` to `.status()`
    /// in `herdr.rs`, which would inherit the child's streams and leak a real
    /// `herdr`'s JSON envelope into the user's terminal.
    fn make_stub_herdr(&self) -> StubHerdr {
        let bin_dir = self.path().join("bin");
        let data_dir = self.path().join("data");
        fs::create_dir_all(&bin_dir).expect("create stub bin dir");
        fs::create_dir_all(&data_dir).expect("create stub data dir");

        let calls_file = self.path().join("calls");
        let invocations_file = self.path().join("invocations");
        fs::write(&calls_file, "").expect("init calls file");
        fs::write(&invocations_file, "").expect("init invocations file");

        let script = format!(
            r#"#!/bin/sh
set -eu
printf '%s\n' "$*" >> '{invocations}'
case "${{1:-}} ${{2:-}}" in
    "tab list") cat '{tabs}' ;;
    "workspace list") cat '{workspaces}' ;;
    "tab focus") printf 'tab %s\n' "${{3:-}}" >> '{calls}'; printf 'stub-stdout-noise\n'; printf 'stub-stderr-noise\n' >&2 ;;
    "workspace focus") printf 'workspace %s\n' "${{3:-}}" >> '{calls}'; printf 'stub-stdout-noise\n'; printf 'stub-stderr-noise\n' >&2 ;;
esac
"#,
            invocations = invocations_file.display(),
            tabs = data_dir.join("tabs.json").display(),
            workspaces = data_dir.join("workspaces.json").display(),
            calls = calls_file.display(),
        );

        let bin_path = bin_dir.join("herdr");
        fs::write(&bin_path, script).expect("write stub herdr script");
        let mut perms = fs::metadata(&bin_path)
            .expect("stat stub herdr")
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&bin_path, perms).expect("chmod stub herdr");

        StubHerdr {
            bin_path,
            calls_file,
            invocations_file,
            data_dir,
        }
    }
}

struct StubHerdr {
    bin_path: PathBuf,
    calls_file: PathBuf,
    invocations_file: PathBuf,
    data_dir: PathBuf,
}

impl StubHerdr {
    /// Real Herdr 0.7.5 output shape, single line: two tabs in workspace w5,
    /// both live.
    fn stub_tabs_two(&self) {
        let json = "{\"id\":\"cli:tab:list\",\"result\":{\"tabs\":[\
            {\"agent_status\":\"working\",\"focused\":true,\"label\":\"1\",\"number\":1,\"pane_count\":1,\"tab_id\":\"w5:t1\",\"workspace_id\":\"w5\"},\
            {\"agent_status\":\"unknown\",\"focused\":false,\"label\":\"2\",\"number\":2,\"pane_count\":1,\"tab_id\":\"w5:t2\",\"workspace_id\":\"w5\"}\
            ],\"type\":\"tab_list\"}}\n";
        fs::write(self.data_dir.join("tabs.json"), json).expect("write tabs.json");
    }

    /// Real Herdr 0.7.5 output shape, single line: workspaces w5 and w9, both
    /// live.
    fn stub_workspaces_two(&self) {
        let json = "{\"id\":\"cli:workspace:list\",\"result\":{\"type\":\"workspace_list\",\"workspaces\":[\
            {\"active_tab_id\":\"w5:t1\",\"focused\":true,\"label\":\"Herdr\",\"number\":1,\"workspace_id\":\"w5\"},\
            {\"active_tab_id\":\"w9:t1\",\"focused\":false,\"label\":\"Other\",\"number\":2,\"workspace_id\":\"w9\"}\
            ]}}\n";
        fs::write(self.data_dir.join("workspaces.json"), json).expect("write workspaces.json");
    }

    fn calls(&self) -> String {
        fs::read_to_string(&self.calls_file).unwrap_or_default()
    }

    fn invocations(&self) -> String {
        fs::read_to_string(&self.invocations_file).unwrap_or_default()
    }
}

fn read_file(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()))
}

/// Recursively counts regular files under `path`. Zero (not an error) when
/// `path` does not exist, matching `find "$dir" -type f 2>/dev/null | wc -l`.
fn count_files(path: &Path) -> usize {
    let mut n = 0;
    count_files_rec(path, &mut n);
    n
}

fn count_files_rec(path: &Path, n: &mut usize) {
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let entry_path = entry.path();
        if entry_path.is_dir() {
            count_files_rec(&entry_path, n);
        } else if entry_path.is_file() {
            *n += 1;
        }
    }
}

fn assert_silent(output: &Output, label: &str) {
    assert!(
        output.stdout.is_empty(),
        "{label}: expected empty stdout, got {:?}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(
        output.stderr.is_empty(),
        "{label}: expected empty stderr, got {:?}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn assert_exit_zero(output: &Output, label: &str) {
    assert!(
        output.status.success(),
        "{label}: expected exit 0, got {:?} (stdout {:?}, stderr {:?})",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

// --- record entrypoints -----------------------------------------------

/// Assertion 1: record-tab records into the workspace's tab file.
#[test]
fn record_tab_records_into_workspace_tab_file() {
    let sandbox = Sandbox::new();
    let out1 = sandbox
        .command("record-tab")
        .env("HERDR_WORKSPACE_ID", "w5")
        .env("HERDR_TAB_ID", "w5:t1")
        .output()
        .expect("run record-tab");
    assert_exit_zero(&out1, "first record-tab");
    let out2 = sandbox
        .command("record-tab")
        .env("HERDR_WORKSPACE_ID", "w5")
        .env("HERDR_TAB_ID", "w5:t2")
        .output()
        .expect("run record-tab");
    assert_exit_zero(&out2, "second record-tab");

    let contents = read_file(&sandbox.session_dir().join("tabs").join("w5"));
    assert_eq!(
        contents, "w5:t2\nw5:t1\n",
        "record-tab records into the workspace's tab file"
    );
}

/// Assertions 2-3: record-tab is a no-op without HERDR_TAB_ID.
#[test]
fn record_tab_noop_without_tab_id() {
    let sandbox = Sandbox::new();
    let out = sandbox
        .command("record-tab")
        .env("HERDR_WORKSPACE_ID", "w5")
        .output()
        .expect("run record-tab");
    assert_silent(&out, "record-tab is a no-op without HERDR_TAB_ID (silence)");
    assert_eq!(
        count_files(&sandbox.session_dir()),
        0,
        "record-tab is a no-op without HERDR_TAB_ID (no files created)"
    );
}

/// Assertions 4-5: record-tab is a no-op without HERDR_WORKSPACE_ID.
#[test]
fn record_tab_noop_without_workspace_id() {
    let sandbox = Sandbox::new();
    let out = sandbox
        .command("record-tab")
        .env("HERDR_TAB_ID", "w9:t1")
        .output()
        .expect("run record-tab");
    assert_silent(
        &out,
        "record-tab is a no-op without HERDR_WORKSPACE_ID (silence)",
    );
    assert_eq!(
        count_files(&sandbox.session_dir()),
        0,
        "record-tab is a no-op without HERDR_WORKSPACE_ID (no files created)"
    );
}

/// Assertion 6: record-workspace records into the workspaces file.
#[test]
fn record_workspace_records_into_workspaces_file() {
    let sandbox = Sandbox::new();
    let out1 = sandbox
        .command("record-workspace")
        .env("HERDR_WORKSPACE_ID", "w5")
        .output()
        .expect("run record-workspace");
    assert_exit_zero(&out1, "first record-workspace");
    let out2 = sandbox
        .command("record-workspace")
        .env("HERDR_WORKSPACE_ID", "w9")
        .output()
        .expect("run record-workspace");
    assert_exit_zero(&out2, "second record-workspace");

    let contents = read_file(&sandbox.session_dir().join("workspaces"));
    assert_eq!(
        contents, "w9\nw5\n",
        "record-workspace records into the workspaces file"
    );
}

/// Assertions 7-8: record-workspace is a no-op without HERDR_WORKSPACE_ID.
#[test]
fn record_workspace_noop_without_workspace_id() {
    let sandbox = Sandbox::new();
    let out = sandbox
        .command("record-workspace")
        .output()
        .expect("run record-workspace");
    assert_silent(
        &out,
        "record-workspace is a no-op without HERDR_WORKSPACE_ID (silence)",
    );
    assert_eq!(
        count_files(&sandbox.session_dir()),
        0,
        "record-workspace is a no-op without HERDR_WORKSPACE_ID (no files created)"
    );
}

// --- jump entrypoints ---------------------------------------------------

/// Assertion 9: jump-tab focuses the last live tab.
#[test]
fn jump_tab_focuses_last_live_tab() {
    let sandbox = Sandbox::new();
    let stub = sandbox.make_stub_herdr();
    stub.stub_tabs_two();
    let tabs_dir = sandbox.session_dir().join("tabs");
    fs::create_dir_all(&tabs_dir).expect("create tabs dir");
    fs::write(tabs_dir.join("w5"), "w5:t2\nw5:t1\n").expect("seed tab history");

    let out = sandbox
        .command("jump-tab")
        .env("HERDR_WORKSPACE_ID", "w5")
        .env("HERDR_TAB_ID", "w5:t2")
        .env("HERDR_BIN_PATH", &stub.bin_path)
        .output()
        .expect("run jump-tab");
    assert_exit_zero(&out, "jump-tab focuses the last live tab");
    assert_eq!(stub.calls(), "tab w5:t1\n");
}

/// Assertion 10: jump-tab issues no call when every candidate is dead.
#[test]
fn jump_tab_no_call_when_every_candidate_dead() {
    let sandbox = Sandbox::new();
    let stub = sandbox.make_stub_herdr();
    stub.stub_tabs_two();
    let tabs_dir = sandbox.session_dir().join("tabs");
    fs::create_dir_all(&tabs_dir).expect("create tabs dir");
    fs::write(tabs_dir.join("w5"), "w5:t2\nw5:t9\n").expect("seed tab history");

    let out = sandbox
        .command("jump-tab")
        .env("HERDR_WORKSPACE_ID", "w5")
        .env("HERDR_TAB_ID", "w5:t2")
        .env("HERDR_BIN_PATH", &stub.bin_path)
        .output()
        .expect("run jump-tab");
    assert_exit_zero(&out, "jump-tab issues no call when every candidate is dead");
    assert_eq!(
        stub.calls(),
        "",
        "jump-tab issues no call when every candidate is dead"
    );
}

/// Assertion 11: jump-tab issues no call with no history file.
#[test]
fn jump_tab_no_call_with_no_history_file() {
    let sandbox = Sandbox::new();
    let stub = sandbox.make_stub_herdr();
    stub.stub_tabs_two();

    let out = sandbox
        .command("jump-tab")
        .env("HERDR_WORKSPACE_ID", "w5")
        .env("HERDR_TAB_ID", "w5:t2")
        .env("HERDR_BIN_PATH", &stub.bin_path)
        .output()
        .expect("run jump-tab");
    assert_exit_zero(&out, "jump-tab issues no call with no history file");
    assert_eq!(
        stub.calls(),
        "",
        "jump-tab issues no call with no history file"
    );
}

/// Assertions 12-13: jump-tab issues no call, and invokes herdr zero times,
/// without HERDR_WORKSPACE_ID. The invocations check (13) is the important
/// one: it proves the guard returns *before* any herdr invocation at all, not
/// merely that no focus call happened.
#[test]
fn jump_tab_no_call_and_no_invocation_without_workspace_id() {
    let sandbox = Sandbox::new();
    let stub = sandbox.make_stub_herdr();
    stub.stub_tabs_two();
    let tabs_dir = sandbox.session_dir().join("tabs");
    fs::create_dir_all(&tabs_dir).expect("create tabs dir");
    fs::write(tabs_dir.join("w5"), "w5:t2\nw5:t1\n").expect("seed tab history");

    let out = sandbox
        .command("jump-tab")
        .env("HERDR_TAB_ID", "w5:t2")
        .env("HERDR_BIN_PATH", &stub.bin_path)
        .output()
        .expect("run jump-tab");
    assert_exit_zero(
        &out,
        "jump-tab issues no call without HERDR_WORKSPACE_ID",
    );
    assert_eq!(
        stub.calls(),
        "",
        "jump-tab issues no call without HERDR_WORKSPACE_ID"
    );
    assert_eq!(
        stub.invocations(),
        "",
        "jump-tab invokes herdr zero times without HERDR_WORKSPACE_ID"
    );
}

/// Assertion 14: jump-workspace focuses the last live workspace.
#[test]
fn jump_workspace_focuses_last_live_workspace() {
    let sandbox = Sandbox::new();
    let stub = sandbox.make_stub_herdr();
    stub.stub_workspaces_two();
    fs::create_dir_all(sandbox.session_dir()).expect("create session dir");
    fs::write(sandbox.session_dir().join("workspaces"), "w5\nw9\n")
        .expect("seed workspace history");

    let out = sandbox
        .command("jump-workspace")
        .env("HERDR_WORKSPACE_ID", "w5")
        .env("HERDR_BIN_PATH", &stub.bin_path)
        .output()
        .expect("run jump-workspace");
    assert_exit_zero(&out, "jump-workspace focuses the last live workspace");
    assert_eq!(stub.calls(), "workspace w9\n");
}

/// Assertion 15: jump-workspace issues no call when every candidate is dead.
#[test]
fn jump_workspace_no_call_when_every_candidate_dead() {
    let sandbox = Sandbox::new();
    let stub = sandbox.make_stub_herdr();
    stub.stub_workspaces_two();
    fs::create_dir_all(sandbox.session_dir()).expect("create session dir");
    fs::write(sandbox.session_dir().join("workspaces"), "w5\nw77\n")
        .expect("seed workspace history");

    let out = sandbox
        .command("jump-workspace")
        .env("HERDR_WORKSPACE_ID", "w5")
        .env("HERDR_BIN_PATH", &stub.bin_path)
        .output()
        .expect("run jump-workspace");
    assert_exit_zero(
        &out,
        "jump-workspace issues no call when every candidate is dead",
    );
    assert_eq!(
        stub.calls(),
        "",
        "jump-workspace issues no call when every candidate is dead"
    );
}

// --- missing HERDR_PLUGIN_STATE_DIR guard -------------------------------
//
// Regression guard carried over from the shell implementation, where
// `${HERDR_PLUGIN_STATE_DIR:?}` aborted only the `$( )` subshell it ran in
// and let the parent continue with an empty state dir, turning the path into
// an absolute path at the filesystem root. `Context::from_env()` returns an
// `Option` that each entrypoint must destructure, so that failure mode is now
// inexpressible — but Herdr always sets the variable in production, so this
// is the only place the guard is exercised at all.
//
// Each command runs with its cwd pinned inside the sandbox (see
// `Sandbox::command_without_state_dir`), and the "no files created" check
// counts every file anywhere under the sandbox, not just under a configured
// state dir — see that method's doc comment for why.

/// Assertions 16-18: record-tab without HERDR_PLUGIN_STATE_DIR.
#[test]
fn record_tab_guard_without_state_dir() {
    let sandbox = Sandbox::new();
    let out = sandbox
        .command_without_state_dir("record-tab")
        .env("HERDR_WORKSPACE_ID", "w5")
        .env("HERDR_TAB_ID", "w5:t1")
        .output()
        .expect("run record-tab");
    assert_exit_zero(&out, "record-tab exits 0 without HERDR_PLUGIN_STATE_DIR");
    assert_silent(&out, "record-tab is silent without HERDR_PLUGIN_STATE_DIR");
    assert_eq!(
        count_files(sandbox.path()),
        0,
        "record-tab creates no file without HERDR_PLUGIN_STATE_DIR"
    );
}

/// Assertions 19-21: record-workspace without HERDR_PLUGIN_STATE_DIR.
#[test]
fn record_workspace_guard_without_state_dir() {
    let sandbox = Sandbox::new();
    let out = sandbox
        .command_without_state_dir("record-workspace")
        .env("HERDR_WORKSPACE_ID", "w5")
        .output()
        .expect("run record-workspace");
    assert_exit_zero(
        &out,
        "record-workspace exits 0 without HERDR_PLUGIN_STATE_DIR",
    );
    assert_silent(
        &out,
        "record-workspace is silent without HERDR_PLUGIN_STATE_DIR",
    );
    assert_eq!(
        count_files(sandbox.path()),
        0,
        "record-workspace creates no file without HERDR_PLUGIN_STATE_DIR"
    );
}

/// Assertions 22-24: jump-tab without HERDR_PLUGIN_STATE_DIR.
#[test]
fn jump_tab_guard_without_state_dir() {
    let sandbox = Sandbox::new();
    let out = sandbox
        .command_without_state_dir("jump-tab")
        .env("HERDR_WORKSPACE_ID", "w5")
        .env("HERDR_TAB_ID", "w5:t1")
        .output()
        .expect("run jump-tab");
    assert_exit_zero(&out, "jump-tab exits 0 without HERDR_PLUGIN_STATE_DIR");
    assert_silent(&out, "jump-tab is silent without HERDR_PLUGIN_STATE_DIR");
    assert_eq!(
        count_files(sandbox.path()),
        0,
        "jump-tab creates no file without HERDR_PLUGIN_STATE_DIR"
    );
}

/// Assertions 25-27: jump-workspace without HERDR_PLUGIN_STATE_DIR.
#[test]
fn jump_workspace_guard_without_state_dir() {
    let sandbox = Sandbox::new();
    let out = sandbox
        .command_without_state_dir("jump-workspace")
        .env("HERDR_WORKSPACE_ID", "w5")
        .output()
        .expect("run jump-workspace");
    assert_exit_zero(
        &out,
        "jump-workspace exits 0 without HERDR_PLUGIN_STATE_DIR",
    );
    assert_silent(
        &out,
        "jump-workspace is silent without HERDR_PLUGIN_STATE_DIR",
    );
    assert_eq!(
        count_files(sandbox.path()),
        0,
        "jump-workspace creates no file without HERDR_PLUGIN_STATE_DIR"
    );
}

// --- toggle --------------------------------------------------------------

/// Assertions 28-29: drive record between jumps the way Herdr's focus events
/// would, and confirm two presses return to the starting tab.
#[test]
fn toggle_round_trip() {
    let sandbox = Sandbox::new();
    let stub = sandbox.make_stub_herdr();
    stub.stub_tabs_two();

    sandbox
        .command("record-tab")
        .env("HERDR_WORKSPACE_ID", "w5")
        .env("HERDR_TAB_ID", "w5:t1")
        .output()
        .expect("record t1");
    sandbox
        .command("record-tab")
        .env("HERDR_WORKSPACE_ID", "w5")
        .env("HERDR_TAB_ID", "w5:t2")
        .output()
        .expect("record t2");

    sandbox
        .command("jump-tab")
        .env("HERDR_WORKSPACE_ID", "w5")
        .env("HERDR_TAB_ID", "w5:t2")
        .env("HERDR_BIN_PATH", &stub.bin_path)
        .output()
        .expect("first jump");
    assert_eq!(
        stub.calls(),
        "tab w5:t1\n",
        "toggle: first press leaves t2 for t1"
    );

    sandbox
        .command("record-tab")
        .env("HERDR_WORKSPACE_ID", "w5")
        .env("HERDR_TAB_ID", "w5:t1")
        .output()
        .expect("record the jump");

    sandbox
        .command("jump-tab")
        .env("HERDR_WORKSPACE_ID", "w5")
        .env("HERDR_TAB_ID", "w5:t1")
        .env("HERDR_BIN_PATH", &stub.bin_path)
        .output()
        .expect("second jump");
    assert_eq!(
        stub.calls(),
        "tab w5:t1\ntab w5:t2\n",
        "toggle: second press returns to t2"
    );
}

// --- record/jump exit asymmetry ------------------------------------------
//
// The record hooks let a genuine write failure reach Herdr's plugin log; the
// jump actions swallow everything and always exit 0. That asymmetry is the
// most important behaviour in this port, and every other test in this file
// reaches only guard conditions, which exit 0 on both paths by design — so
// without this section, deleting the asymmetry entirely would go undetected.
//
// The failure is induced by creating the directory record needs as a regular
// FILE, so create_dir_all fails with EEXIST. No permissions are involved, so
// this behaves the same when run as root.

/// Assertions 30-31: record-tab exits non-zero and reports the failure on
/// stderr when the write fails.
#[test]
fn record_tab_exits_nonzero_on_write_failure() {
    let sandbox = Sandbox::new();
    let stub = sandbox.make_stub_herdr();
    stub.stub_tabs_two();
    fs::create_dir_all(sandbox.session_dir()).expect("create session dir");
    fs::write(sandbox.session_dir().join("tabs"), "").expect("make tabs a file, not a dir");

    let out = sandbox
        .command("record-tab")
        .env("HERDR_WORKSPACE_ID", "w5")
        .env("HERDR_TAB_ID", "w5:t1")
        .env("HERDR_BIN_PATH", &stub.bin_path)
        .output()
        .expect("run record-tab");

    assert_ne!(
        out.status.code(),
        Some(0),
        "record-tab exits non-zero when the write fails"
    );
    assert!(
        !out.stderr.is_empty(),
        "record-tab reports the failure on stderr"
    );
}

/// Assertions 32-33: jump-tab exits 0 and stays silent when the state path is
/// unusable.
#[test]
fn jump_tab_silent_when_state_path_unusable() {
    let sandbox = Sandbox::new();
    let stub = sandbox.make_stub_herdr();
    stub.stub_tabs_two();
    fs::create_dir_all(sandbox.session_dir()).expect("create session dir");
    fs::write(sandbox.session_dir().join("tabs"), "").expect("make tabs a file, not a dir");

    let out = sandbox
        .command("jump-tab")
        .env("HERDR_WORKSPACE_ID", "w5")
        .env("HERDR_TAB_ID", "w5:t1")
        .env("HERDR_BIN_PATH", &stub.bin_path)
        .output()
        .expect("run jump-tab");

    assert_exit_zero(&out, "jump-tab exits 0 when the state path is unusable");
    assert_silent(&out, "jump-tab stays silent when the state path is unusable");
}

// --- success-path silence --------------------------------------------------
//
// The binding constraint is silence on EVERY guard and success path, but
// every other captured invocation in this file reaches a guard or the
// record-failure path. Without these two, switching herdr.rs's `.output()`
// to `.status()` would inherit the CLI's stdout — and a real `herdr tab
// focus` prints a JSON envelope, so it would leak into the user's terminal —
// with nothing going red.

/// Assertions 34-35: jump-tab is silent on a successful jump and still
/// focused the target.
#[test]
fn jump_tab_silent_on_successful_jump() {
    let sandbox = Sandbox::new();
    let stub = sandbox.make_stub_herdr();
    stub.stub_tabs_two();
    let tabs_dir = sandbox.session_dir().join("tabs");
    fs::create_dir_all(&tabs_dir).expect("create tabs dir");
    fs::write(tabs_dir.join("w5"), "w5:t2\nw5:t1\n").expect("seed tab history");

    let out = sandbox
        .command("jump-tab")
        .env("HERDR_WORKSPACE_ID", "w5")
        .env("HERDR_TAB_ID", "w5:t2")
        .env("HERDR_BIN_PATH", &stub.bin_path)
        .output()
        .expect("run jump-tab");

    // The critical assertion: this is what would catch herdr.rs's focus_tab
    // regressing from .output() to .status().
    assert_silent(&out, "jump-tab is silent on a successful jump");
    assert_eq!(
        stub.calls(),
        "tab w5:t1\n",
        "jump-tab still focused the target"
    );
}

/// Assertions 36-37: record-tab is silent on a successful record and still
/// wrote the entry.
#[test]
fn record_tab_silent_on_successful_record() {
    let sandbox = Sandbox::new();
    let out = sandbox
        .command("record-tab")
        .env("HERDR_WORKSPACE_ID", "w5")
        .env("HERDR_TAB_ID", "w5:t1")
        .output()
        .expect("run record-tab");

    assert_silent(&out, "record-tab is silent on a successful record");
    assert_eq!(
        read_file(&sandbox.session_dir().join("tabs").join("w5")),
        "w5:t1\n",
        "record-tab still wrote the entry"
    );
}
