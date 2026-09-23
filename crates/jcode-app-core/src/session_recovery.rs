//! Interrupted-session recovery.
//!
//! When a session is interrupted mid-generation (a crash, a disconnect, or a
//! server restart) the server records a continuation intent; on reconnect the
//! client queues the continuation so the turn resumes instead of stalling for
//! the user.
//!
//! This is what remains of the old self-dev "reload recovery" surface. The
//! build/install machinery is gone (the package manager owns installed
//! binaries), so there is no build-specific reconnect notice any more; the
//! `ReloadContext` persisted-context seam is kept so the interruption path and
//! its callers are unchanged.

use anyhow::Result;
use serde::{Deserialize, Serialize};

pub use crate::protocol::ReloadRecoveryDirective;

/// Context saved before a reload, restored after restart.
///
/// With self-dev reload removed this is only ever constructed by tests; the
/// production interruption path uses [`recovery_directive`] with no context.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReloadContext {
    /// What the agent was working on (user-provided or auto-detected)
    pub task_context: Option<String>,
    /// Version before reload
    pub version_before: String,
    /// New version (target)
    pub version_after: String,
    /// Session ID
    pub session_id: String,
    /// Timestamp
    pub timestamp: String,
}

impl ReloadContext {
    fn sanitize_session_id(session_id: &str) -> String {
        session_id
            .chars()
            .map(|ch| {
                if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                    ch
                } else {
                    '_'
                }
            })
            .collect()
    }

    pub fn path_for_session(session_id: &str) -> Result<std::path::PathBuf> {
        let sanitized = Self::sanitize_session_id(session_id);
        Ok(crate::storage::jcode_dir()?.join(format!("reload-context-{}.json", sanitized)))
    }

    fn legacy_path() -> Result<std::path::PathBuf> {
        Ok(crate::storage::jcode_dir()?.join("reload-context.json"))
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::path_for_session(&self.session_id)?;
        crate::storage::write_json(&path, self)?;
        Ok(())
    }

    pub fn load() -> Result<Option<Self>> {
        let legacy = Self::legacy_path()?;
        if !legacy.exists() {
            return Ok(None);
        }
        let ctx: Self = crate::storage::read_json(&legacy)?;
        let _ = std::fs::remove_file(&legacy);
        Ok(Some(ctx))
    }

    /// Peek at context for a specific session without consuming it.
    pub fn peek_for_session(session_id: &str) -> Result<Option<Self>> {
        let session_path = Self::path_for_session(session_id)?;
        if session_path.exists() {
            let ctx: Self = crate::storage::read_json(&session_path)?;
            return Ok(Some(ctx));
        }

        let legacy = Self::legacy_path()?;
        if !legacy.exists() {
            return Ok(None);
        }

        let ctx: Self = crate::storage::read_json(&legacy)?;
        if ctx.session_id == session_id {
            Ok(Some(ctx))
        } else {
            Ok(None)
        }
    }

    /// Load context only if it belongs to the given session; consumes on success.
    pub fn load_for_session(session_id: &str) -> Result<Option<Self>> {
        let session_path = Self::path_for_session(session_id)?;
        if session_path.exists() {
            let ctx: Self = crate::storage::read_json(&session_path)?;
            let _ = std::fs::remove_file(&session_path);
            return Ok(Some(ctx));
        }

        let legacy = Self::legacy_path()?;
        if !legacy.exists() {
            return Ok(None);
        }

        let ctx: Self = crate::storage::read_json(&legacy)?;
        if ctx.session_id == session_id {
            let _ = std::fs::remove_file(&legacy);
            Ok(Some(ctx))
        } else {
            Ok(None)
        }
    }

    fn task_info_suffix(&self) -> String {
        self.task_context
            .as_ref()
            .map(|task| format!("\nTask context: {}", task))
            .unwrap_or_default()
    }

    pub fn reconnect_notice_line(&self) -> String {
        format!("Reloaded with build {}", self.version_after)
    }

    pub fn continuation_message(
        &self,
        background_task_note: &str,
        restored_turns: Option<usize>,
    ) -> String {
        let task_info = self.task_info_suffix();
        let turns_note = restored_turns
            .map(|turns| format!(" Session restored with {} turns.", turns))
            .unwrap_or_default();
        format!(
            "Reload succeeded ({} → {}).{}{}{} Continue immediately from where you left off. Do not ask the user what to do next. Do not summarize the reload.",
            self.version_before, self.version_after, task_info, background_task_note, turns_note
        )
    }

    pub fn interrupted_session_continuation_message() -> String {
        "Your session was interrupted by a server reload while a tool was running. The tool was aborted and results may be incomplete. Continue exactly where you left off and do not ask the user what to do next.".to_string()
    }

    pub fn recovery_continuation_message(
        reload_ctx: Option<&Self>,
        background_task_note: &str,
        restored_turns: Option<usize>,
    ) -> String {
        reload_ctx
            .map(|ctx| ctx.continuation_message(background_task_note, restored_turns))
            .unwrap_or_else(Self::interrupted_session_continuation_message)
    }

    pub fn recovery_directive(
        reload_ctx: Option<&Self>,
        was_interrupted: bool,
        background_task_note: &str,
        restored_turns: Option<usize>,
    ) -> Option<ReloadRecoveryDirective> {
        if let Some(ctx) = reload_ctx {
            return Some(ReloadRecoveryDirective {
                reconnect_notice: Some(ctx.reconnect_notice_line()),
                continuation_message: ctx
                    .continuation_message(background_task_note, restored_turns),
            });
        }

        if was_interrupted {
            return Some(ReloadRecoveryDirective {
                reconnect_notice: None,
                continuation_message: Self::interrupted_session_continuation_message(),
            });
        }

        None
    }

    pub fn recovery_directive_for_session(
        session_id: &str,
        reload_ctx: Option<&Self>,
        was_interrupted: bool,
        restored_turns: Option<usize>,
    ) -> Option<ReloadRecoveryDirective> {
        Self::recovery_directive(
            reload_ctx,
            was_interrupted,
            &persisted_background_tasks_note(session_id),
            restored_turns,
        )
    }

    pub fn log_recovery_outcome(flow: &str, session_id: &str, outcome: &str, detail: &str) {
        crate::logging::info(&format!(
            "reload recovery flow={} session_id={} outcome={} detail={}",
            flow, session_id, outcome, detail
        ));
    }
}

pub fn persisted_background_tasks_note(session_id: &str) -> String {
    let mut notes = String::new();

    let tasks =
        crate::background::global().persisted_detached_running_tasks_for_session(session_id);
    if !tasks.is_empty() {
        let task_list = tasks
            .iter()
            .map(|task| format!("{} ({})", task.task_id, task.tool_name))
            .collect::<Vec<_>>()
            .join(", ");

        notes.push_str(&format!(
            "\nPersisted background task(s) for this session are still running: {}. Do not rerun those commands. Check them first with the `bg` tool (`bg action=\"list\"`, `bg action=\"status\" task_id=...`, or `bg action=\"output\" task_id=...`).",
            task_list
        ));
    }

    // Background awaits auto-resume on the new server and report via
    // notify/wake, so they need no agent action. Only blocking awaits, whose
    // socket waiter dies with the old process, must be rerun by the agent.
    let pending_awaits: Vec<_> = crate::server::pending_await_members_for_session(session_id)
        .into_iter()
        .filter(|state| !state.background)
        .collect();
    if !pending_awaits.is_empty() {
        let await_list = pending_awaits
            .iter()
            .map(|state| {
                let watch = if state.requested_ids.is_empty() {
                    "entire swarm".to_string()
                } else {
                    state.requested_ids.join(", ")
                };
                let remaining_secs = state.remaining_timeout().as_secs();
                format!(
                    "{} -> [{}], {}s remaining",
                    watch,
                    state.target_status.join(", "),
                    remaining_secs
                )
            })
            .collect::<Vec<_>>()
            .join("; ");

        notes.push_str(&format!(
            "\nPersisted blocking `swarm await_members` wait(s) are still pending: {}. If you still need those coordination points after reload, rerun the same `swarm` call with action `await_members` to resume them with the remaining timeout instead of starting over. (Background awaits resume automatically and will notify you.)",
            await_list
        ));
    }

    notes
}
