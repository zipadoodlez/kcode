//! The passively refreshed overnight progress card in the transcript.
//!
//! The card is a single transcript row keyed by `run_id`, rewritten in place on
//! each poll while the run is active. `OvernightCard` owns the poll throttle;
//! the operations below build the row and append the coordinator session's new
//! messages, and stay free functions because they touch the transcript and the
//! session, not the throttle.

use super::App;
use super::state_ui_messages::is_background_task_lifecycle_message;
use crate::message::{ContentBlock, Role};
use crate::overnight::OvernightRunStatus;
use jcode_tui_messages::DisplayMessage;
use std::time::{Duration, Instant};

const OVERNIGHT_CARD_REFRESH_INTERVAL: Duration = Duration::from_secs(5);

/// Poll throttle for the passive overnight progress card.
///
/// Invariant: the run directory is rescanned at most once per
/// `OVERNIGHT_CARD_REFRESH_INTERVAL`. `begin_refresh` is the only mutation, and
/// it records the attempt even when no run file is present, so an idle app does
/// not stat the run directory every frame.
#[derive(Default)]
pub(super) struct OvernightCard {
    last_refresh: Option<Instant>,
}

impl OvernightCard {
    /// True when the poll interval has elapsed; records `now` when it has.
    fn begin_refresh(&mut self, now: Instant) -> bool {
        if self
            .last_refresh
            .is_some_and(|last| now.duration_since(last) < OVERNIGHT_CARD_REFRESH_INTERVAL)
        {
            return false;
        }
        self.last_refresh = Some(now);
        true
    }
}

/// Poll the overnight run files and refresh the transcript card if due.
pub(super) fn refresh_if_due(app: &mut App) -> bool {
    if crate::tui::is_ssh_remote() {
        return false;
    }
    if !app.overnight_card.begin_refresh(Instant::now()) {
        return false;
    }

    let has_card = app
        .transcript
        .messages()
        .iter()
        .any(|message| message.role == "overnight");
    let Ok(Some(manifest)) = crate::overnight::latest_manifest() else {
        return false;
    };
    let active = matches!(
        manifest.status,
        OvernightRunStatus::Running | OvernightRunStatus::CancelRequested
    );
    if !has_card && !active {
        return false;
    }
    let card_changed = upsert_card(app, &manifest);
    let transcript_changed = maybe_tail_current_session_transcript(app, &manifest);
    card_changed || transcript_changed
}

/// Create or update the overnight progress card for `manifest`.
pub(super) fn upsert_card(app: &mut App, manifest: &crate::overnight::OvernightManifest) -> bool {
    let Ok(content) = crate::overnight::format_progress_card_content(manifest) else {
        return false;
    };
    let title = Some("Overnight".to_string());
    let idx = app.transcript.messages().iter().rposition(|message| {
        message.role == "overnight"
            && serde_json::from_str::<crate::overnight::OvernightProgressCard>(&message.content)
                .is_ok_and(|card| card.run_id == manifest.run_id)
    });
    if let Some(idx) = idx {
        app.replace_display_message_title_and_content(idx, title, content)
    } else {
        app.push_display_message(DisplayMessage::overnight(content));
        true
    }
}

/// Append the coordinator session's messages that arrived since the last poll.
fn maybe_tail_current_session_transcript(
    app: &mut App,
    manifest: &crate::overnight::OvernightManifest,
) -> bool {
    if manifest.coordinator_session_id != app.session.id {
        return false;
    }
    let Ok(latest_session) = crate::session::Session::load(&app.session.id) else {
        return false;
    };
    if latest_session.messages.len() <= app.session.messages.len() {
        return false;
    }

    let appended: Vec<DisplayMessage> = latest_session.messages[app.session.messages.len()..]
        .iter()
        .filter_map(display_message_from_stored_message)
        .collect();
    app.session = latest_session;
    if appended.is_empty() {
        return false;
    }
    for message in appended {
        app.push_display_message(message);
    }
    true
}

fn display_message_from_stored_message(
    message: &crate::session::StoredMessage,
) -> Option<DisplayMessage> {
    let text = stored_message_visible_text(message);
    if text.trim().is_empty() {
        return None;
    }
    if is_background_task_lifecycle_message(&text) {
        return None;
    }
    match message.display_role {
        Some(crate::session::StoredDisplayRole::System) => Some(DisplayMessage::system(text)),
        Some(crate::session::StoredDisplayRole::BackgroundTask) => None,
        None => match message.role {
            Role::User => {
                if crate::session::is_scheduled_task_message(message) {
                    return Some(DisplayMessage::system(text));
                }
                // Synthetic auto-poke continuations are persisted as user
                // turns for the model but must not display as user prompts.
                if crate::todo::is_auto_poke_message(&text) {
                    // Gate continuations are written for the model; the user
                    // only needs to know the check happened.
                    match crate::todo::auto_poke_display_summary(&text) {
                        Some(summary) => Some(DisplayMessage::system(summary.to_string())),
                        None => Some(DisplayMessage::system(text)),
                    }
                } else {
                    Some(DisplayMessage::user(text))
                }
            }
            Role::Assistant => Some(DisplayMessage::assistant(text)),
        },
    }
}

fn stored_message_visible_text(message: &crate::session::StoredMessage) -> String {
    let mut parts = Vec::new();
    for block in &message.content {
        match block {
            ContentBlock::Text { text, .. }
            | ContentBlock::Reasoning { text }
            | ContentBlock::ReasoningTrace { text } => {
                if !text.trim().is_empty() {
                    parts.push(text.trim().to_string());
                }
            }
            ContentBlock::AnthropicThinking { .. } | ContentBlock::OpenAIReasoning { .. } => {}
            ContentBlock::ToolUse { name, input, .. } => {
                parts.push(format!("[tool:{} {}]", name, input));
            }
            ContentBlock::ToolResult { content, .. } => {
                if !content.trim().is_empty() {
                    parts.push(content.trim().to_string());
                }
            }
            ContentBlock::Image { media_type, .. } => {
                parts.push(format!("[image:{}]", media_type));
            }
            ContentBlock::OpenAICompaction { .. } => {}
        }
    }
    parts.join("\n\n")
}
