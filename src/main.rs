mod context;
mod herdr;
mod mru;

use context::{env_id, Context};
use std::process::ExitCode;

fn main() -> ExitCode {
    let Some(subcommand) = std::env::args().nth(1) else {
        return ExitCode::SUCCESS;
    };
    match subcommand.as_str() {
        // Record hooks surface a genuine write failure to Herdr's plugin log.
        "record-tab" => report(record_tab()),
        "record-workspace" => report(record_workspace()),
        // Jump actions are a best-effort convenience. A failed jump must never
        // interrupt the user, so every error is swallowed and the exit is
        // always 0. This asymmetry with the record hooks is deliberate.
        "jump-tab" => {
            jump_tab();
            ExitCode::SUCCESS
        }
        "jump-workspace" => {
            jump_workspace();
            ExitCode::SUCCESS
        }
        _ => ExitCode::SUCCESS,
    }
}

/// Guard conditions return `Ok(())` and exit 0 silently. Only a real I/O
/// failure reaches the error arm, where the message is what makes the failure
/// legible in Herdr's plugin log.
fn report(result: std::io::Result<()>) -> ExitCode {
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("herdr-plugin-last: {err}");
            ExitCode::FAILURE
        }
    }
}

fn record_tab() -> std::io::Result<()> {
    let (Some(workspace), Some(tab)) = (env_id("HERDR_WORKSPACE_ID"), env_id("HERDR_TAB_ID"))
    else {
        return Ok(());
    };
    let Some(context) = Context::from_env() else {
        return Ok(());
    };
    mru::record(&context.tabs_file(&workspace), &tab)
}

fn record_workspace() -> std::io::Result<()> {
    let Some(workspace) = env_id("HERDR_WORKSPACE_ID") else {
        return Ok(());
    };
    let Some(context) = Context::from_env() else {
        return Ok(());
    };
    mru::record(&context.workspaces_file(), &workspace)
}

fn jump_tab() {
    let Some(workspace) = env_id("HERDR_WORKSPACE_ID") else {
        return;
    };
    let Some(context) = Context::from_env() else {
        return;
    };
    let current = env_id("HERDR_TAB_ID").unwrap_or_default();
    let live = herdr::live_tab_ids(&workspace);
    if let Some(target) = mru::pick_target(&context.tabs_file(&workspace), &current, &live) {
        herdr::focus_tab(&target);
    }
}

fn jump_workspace() {
    let Some(context) = Context::from_env() else {
        return;
    };
    let current = env_id("HERDR_WORKSPACE_ID").unwrap_or_default();
    let live = herdr::live_workspace_ids();
    if let Some(target) = mru::pick_target(&context.workspaces_file(), &current, &live) {
        herdr::focus_workspace(&target);
    }
}
