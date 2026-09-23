//! Client exit actions: when the TUI asks to restart, reload, rebuild or
//! update, re-exec the running binary and resume the session.
//!
//! Self-dev build and the update channels are gone, so reload/rebuild/update no
//! longer have a different binary to exec into. Every requested action is now a
//! plain restart of the current binary.

use anyhow::Result;
use std::process::Command as ProcessCommand;

use crate::tui::RunResult;

pub fn has_requested_action(run_result: &RunResult) -> bool {
    run_result.reload_session.is_some()
        || run_result.rebuild_session.is_some()
        || run_result.update_session.is_some()
        || run_result.restart_session.is_some()
}

pub fn execute_requested_action(run_result: &RunResult) -> Result<()> {
    let session_id = run_result
        .reload_session
        .as_ref()
        .or(run_result.restart_session.as_ref())
        .or(run_result.rebuild_session.as_ref())
        .or(run_result.update_session.as_ref());
    if let Some(session_id) = session_id {
        hot_restart(session_id)?;
    }
    Ok(())
}

pub fn hot_restart(session_id: &str) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let exe = std::env::current_exe()?;

    crate::logging::info(&format!("Restarting with current binary: {:?}", exe));
    crate::env::set_var("JCODE_RESUMING", "1");

    let mut cmd = ProcessCommand::new(&exe);
    cmd.arg("--resume").arg(session_id).current_dir(&cwd);
    let err = crate::platform::replace_process(&mut cmd);

    Err(anyhow::anyhow!("Failed to exec {:?}: {}", exe, err))
}
