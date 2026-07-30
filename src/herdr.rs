use serde::Deserialize;
use std::process::Command;

/// Every CLI response is an envelope carrying either `result` or `error`.
/// Modelling `result` as an `Option` means an error response deserialises to
/// `None` and degrades to an empty id list rather than failing.
#[derive(Deserialize)]
struct Envelope<T> {
    result: Option<T>,
}

#[derive(Deserialize)]
struct TabList {
    tabs: Vec<Tab>,
}

#[derive(Deserialize)]
struct Tab {
    tab_id: String,
}

#[derive(Deserialize)]
struct WorkspaceList {
    workspaces: Vec<Workspace>,
}

#[derive(Deserialize)]
struct Workspace {
    workspace_id: String,
}

pub fn parse_tab_ids(json: &str) -> Vec<String> {
    serde_json::from_str::<Envelope<TabList>>(json)
        .ok()
        .and_then(|envelope| envelope.result)
        .map(|list| list.tabs.into_iter().map(|tab| tab.tab_id).collect())
        .unwrap_or_default()
}

pub fn parse_workspace_ids(json: &str) -> Vec<String> {
    serde_json::from_str::<Envelope<WorkspaceList>>(json)
        .ok()
        .and_then(|envelope| envelope.result)
        .map(|list| {
            list.workspaces
                .into_iter()
                .map(|workspace| workspace.workspace_id)
                .collect()
        })
        .unwrap_or_default()
}

/// Herdr injects `HERDR_BIN_PATH` into plugin commands; falling back to `PATH`
/// keeps the binary runnable by hand.
fn herdr_bin() -> String {
    crate::context::env_id("HERDR_BIN_PATH").unwrap_or_else(|| "herdr".to_string())
}

/// Runs a `herdr` subcommand and returns its stdout, or `None` on any failure.
/// `.output()` captures both streams, so the CLI can never write to our
/// terminal.
fn run(args: &[&str]) -> Option<String> {
    let output = Command::new(herdr_bin()).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

pub fn live_tab_ids(workspace_id: &str) -> Vec<String> {
    run(&["tab", "list", "--workspace", workspace_id])
        .map(|json| parse_tab_ids(&json))
        .unwrap_or_default()
}

pub fn live_workspace_ids() -> Vec<String> {
    run(&["workspace", "list"])
        .map(|json| parse_workspace_ids(&json))
        .unwrap_or_default()
}

/// Focus calls are best effort. Failures are swallowed: a jump must never
/// interrupt the user or write to the terminal.
pub fn focus_tab(id: &str) {
    let _ = Command::new(herdr_bin()).args(["tab", "focus", id]).output();
}

pub fn focus_workspace(id: &str) {
    let _ = Command::new(herdr_bin())
        .args(["workspace", "focus", id])
        .output();
}

#[cfg(test)]
mod tests {
    use super::*;

    const TABS: &str = r#"{"id":"cli:tab:list","result":{"tabs":[{"agent_status":"working","focused":true,"label":"1","number":1,"pane_count":1,"tab_id":"w5:t1","workspace_id":"w5"},{"agent_status":"unknown","focused":false,"label":"2","number":2,"pane_count":1,"tab_id":"w5:t2","workspace_id":"w5"}],"type":"tab_list"}}"#;

    const WORKSPACES: &str = r#"{"id":"cli:workspace:list","result":{"type":"workspace_list","workspaces":[{"active_tab_id":"w5:t1","focused":true,"label":"Herdr","number":1,"workspace_id":"w5"},{"active_tab_id":"w9:t1","focused":false,"label":"Other","number":2,"workspace_id":"w9"}]}}"#;

    const ERROR: &str = r#"{"error":{"code":"tab_not_found","message":"tab w5:t99 not found"},"id":"cli:tab:focus"}"#;

    #[test]
    fn parses_tab_ids_in_order() {
        assert_eq!(parse_tab_ids(TABS), vec!["w5:t1", "w5:t2"]);
    }

    #[test]
    fn parses_workspace_ids_and_ignores_active_tab_ids() {
        assert_eq!(parse_workspace_ids(WORKSPACES), vec!["w5", "w9"]);
    }

    // An error envelope has no `result`, so it degrades to an empty live set,
    // which makes the jump a silent no-op. That is the intended behaviour.
    #[test]
    fn an_error_envelope_yields_no_ids() {
        assert!(parse_tab_ids(ERROR).is_empty());
        assert!(parse_workspace_ids(ERROR).is_empty());
        // "result": null present-but-null is the shape only Option<T> accepts.
        assert!(parse_tab_ids(r#"{"id":"x","result":null}"#).is_empty());
    }

    #[test]
    fn malformed_json_yields_no_ids() {
        assert!(parse_tab_ids("not json at all").is_empty());
        assert!(parse_workspace_ids("").is_empty());
    }

    // Herdr will add response fields. Only tab_id and workspace_id are read;
    // everything else must be tolerated, so never use deny_unknown_fields.
    #[test]
    fn unknown_fields_are_tolerated() {
        let future = r#"{"result":{"tabs":[{"tab_id":"w5:t1","brand_new_field":42}]}}"#;
        assert_eq!(parse_tab_ids(future), vec!["w5:t1"]);
    }

    // A user-set tab label containing a comma and quotes cannot inject a bogus
    // id. The shell version extracted ids with `tr ',' '\n' | sed`, which was
    // safe only because Herdr escapes the quotes; a real parser makes it
    // structural.
    #[test]
    fn a_hostile_label_cannot_inject_an_id() {
        let hostile =
            r#"{"result":{"tabs":[{"label":"hello,\"tab_id\":\"w9:t9","tab_id":"w5:t1"}]}}"#;
        assert_eq!(parse_tab_ids(hostile), vec!["w5:t1"]);
    }

    /// Restores an environment variable when dropped, so a test cannot leave the
    /// process environment mutated — including when an assertion panics and
    /// unwinds past the end of the test body. `cargo test` runs tests as threads
    /// in one process, so a dirty variable would make sibling failures hard to
    /// attribute.
    struct RestoreEnv(&'static str, Option<String>);

    impl Drop for RestoreEnv {
        fn drop(&mut self) {
            match self.1.take() {
                Some(value) => std::env::set_var(self.0, value),
                None => std::env::remove_var(self.0),
            }
        }
    }

    // herdr_bin's fallback is what keeps the binary runnable by hand outside a
    // Herdr session. Exercised through the real environment because that is how
    // callers reach it, and with a save/restore so it cannot disturb the
    // HERDR_BIN_PATH that other tests in this binary may rely on.
    #[test]
    fn herdr_bin_falls_back_when_path_is_unset_or_empty() {
        let _env = crate::context::ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _restore = RestoreEnv("HERDR_BIN_PATH", std::env::var("HERDR_BIN_PATH").ok());

        std::env::set_var("HERDR_BIN_PATH", "/custom/herdr");
        assert_eq!(herdr_bin(), "/custom/herdr");

        std::env::set_var("HERDR_BIN_PATH", "");
        assert_eq!(herdr_bin(), "herdr");

        std::env::remove_var("HERDR_BIN_PATH");
        assert_eq!(herdr_bin(), "herdr");
    }

    #[test]
    fn restore_env_guard_actually_restores_on_panic() {
        let _env = crate::context::ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let original = std::env::var("HERDR_BIN_PATH").ok();

        // Simulate a test that panics while holding the guard
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _restore = RestoreEnv("HERDR_BIN_PATH", std::env::var("HERDR_BIN_PATH").ok());
            std::env::set_var("HERDR_BIN_PATH", "/test/value");
            panic!("deliberate panic to test guard restoration");
        }));

        // The panic should have occurred
        assert!(result.is_err());

        // After the panic, the guard should have restored the original value
        let restored = std::env::var("HERDR_BIN_PATH").ok();
        assert_eq!(
            original, restored,
            "guard did not restore environment on panic"
        );
    }
}
