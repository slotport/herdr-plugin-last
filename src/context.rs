use std::path::PathBuf;

pub struct Context {
    pub state_dir: PathBuf,
}

impl Context {
    /// Returns `None` when `HERDR_PLUGIN_STATE_DIR` is unset or empty, which is
    /// the caller's signal to exit 0 silently.
    ///
    /// The shell version expressed this as `${HERDR_PLUGIN_STATE_DIR:?}`, which
    /// aborted only the `$( )` subshell it ran in and let the parent continue
    /// with an empty path. Returning an `Option` the caller must destructure
    /// makes that failure mode inexpressible.
    pub fn from_env() -> Option<Context> {
        let state_dir = env_id("HERDR_PLUGIN_STATE_DIR")?;
        let socket = env_id("HERDR_SOCKET_PATH").unwrap_or_else(|| "default".to_string());
        Some(Context {
            state_dir: PathBuf::from(state_dir).join(session_key(&socket)),
        })
    }

    pub fn workspaces_file(&self) -> PathBuf {
        self.state_dir.join("workspaces")
    }

    pub fn tabs_file(&self, workspace_id: &str) -> PathBuf {
        self.state_dir.join("tabs").join(workspace_id)
    }
}

/// The whole socket path is sanitised, not its basename. Named sessions live at
/// `~/.config/herdr/sessions/<name>/herdr.sock` and the default session at
/// `~/.config/herdr/herdr.sock`, so every session shares the basename
/// `herdr.sock`; keying on it would make concurrent sessions share one history.
pub fn session_key(socket_path: &str) -> String {
    socket_path.replace('/', "_")
}

/// Reads an environment variable, treating unset and empty as identical. This
/// matches the shell's `${VAR:-}` and `${VAR:?}` semantics, which the whole
/// plugin relies on.
pub fn env_id(name: &str) -> Option<String> {
    std::env::var(name).ok().and_then(non_empty)
}

fn non_empty(value: String) -> Option<String> {
    if value.is_empty() { None } else { Some(value) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_key_sanitises_the_full_socket_path() {
        assert_eq!(
            session_key("/home/test/.config/herdr/herdr.sock"),
            "_home_test_.config_herdr_herdr.sock"
        );
    }

    // Regression guard: named sessions all share the basename herdr.sock, so a
    // basename-derived key would silently merge two sessions' histories.
    #[test]
    fn session_key_distinguishes_default_from_named_sessions() {
        let default = session_key("/home/test/.config/herdr/herdr.sock");
        let named = session_key("/home/test/.config/herdr/sessions/work/herdr.sock");
        assert_ne!(default, named);
    }

    #[test]
    fn non_empty_treats_empty_as_absent() {
        assert_eq!(non_empty("w5".to_string()), Some("w5".to_string()));
        assert_eq!(non_empty(String::new()), None);
    }

    // env_id's contract is "unset or empty means absent". Exercised through the
    // real environment because that is how every caller reaches it. Uses a
    // dedicated variable name so it cannot collide with the HERDR_* vars the
    // binary reads.
    #[test]
    fn env_id_reads_unset_and_empty_as_absent() {
        std::env::set_var("HERDR_TEST_ENV_ID", "w5");
        assert_eq!(env_id("HERDR_TEST_ENV_ID"), Some("w5".to_string()));
        std::env::set_var("HERDR_TEST_ENV_ID", "");
        assert_eq!(env_id("HERDR_TEST_ENV_ID"), None);
        std::env::remove_var("HERDR_TEST_ENV_ID");
        assert_eq!(env_id("HERDR_TEST_ENV_ID"), None);
    }

    #[test]
    fn tabs_file_nests_under_the_session_key() {
        let ctx = Context {
            state_dir: PathBuf::from("/state/_home_test_herdr.sock"),
        };
        assert_eq!(
            ctx.tabs_file("w5"),
            PathBuf::from("/state/_home_test_herdr.sock/tabs/w5")
        );
    }

    #[test]
    fn workspaces_file_sits_beside_the_tabs_directory() {
        let ctx = Context {
            state_dir: PathBuf::from("/state/_home_test_herdr.sock"),
        };
        assert_eq!(
            ctx.workspaces_file(),
            PathBuf::from("/state/_home_test_herdr.sock/workspaces")
        );
    }
}
