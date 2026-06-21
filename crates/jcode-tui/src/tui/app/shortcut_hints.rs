//! Shortcut discovery nudges.
//!
//! When a user performs an action "the long way" (typing a slash command,
//! relaunching with a CLI flag, etc.) and that same action has a configured
//! keyboard shortcut, we surface a short one-line nudge telling them the
//! shortcut. The goal is to teach the keybindings passively, without nagging:
//! each distinct hint is shown at most [`MAX_SHOWS_PER_HINT`] times total,
//! tracked across restarts in a small JSON file.
//!
//! The system is intentionally generic: callers describe an action by a stable
//! id and provide the resolved shortcut label, and this module decides whether
//! to emit the nudge and records that it did.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::App;

/// How many times a single hint id may be shown before we stop nudging.
const MAX_SHOWS_PER_HINT: u32 = 3;

const HINT_STATE_FILE: &str = "shortcut_hints.json";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct HintState {
    #[serde(default)]
    version: u8,
    /// Number of times each hint id has been shown.
    #[serde(default)]
    shows: HashMap<String, u32>,
}

fn state_path() -> Option<std::path::PathBuf> {
    crate::storage::app_config_dir()
        .ok()
        .map(|dir| dir.join(HINT_STATE_FILE))
}

fn load_state() -> HintState {
    let Some(path) = state_path() else {
        return HintState::default();
    };
    crate::storage::read_json::<HintState>(&path).unwrap_or_default()
}

fn save_state(state: &HintState) {
    let Some(path) = state_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Err(error) = crate::storage::write_json(&path, state) {
        crate::logging::info(&format!(
            "Failed to persist shortcut-hint state {}: {}",
            path.display(),
            error
        ));
    }
}

/// Whether `hint_id` still has remaining nudges to show. Pure read; does not
/// record anything. Used to decide before building a (possibly costly) message.
pub(crate) fn should_show(hint_id: &str) -> bool {
    load_state()
        .shows
        .get(hint_id)
        .copied()
        .unwrap_or(0)
        < MAX_SHOWS_PER_HINT
}

/// Record that `hint_id` was shown once, persisting the bumped counter.
pub(crate) fn record_shown(hint_id: &str) {
    let mut state = load_state();
    let counter = state.shows.entry(hint_id.to_string()).or_insert(0);
    *counter = counter.saturating_add(1);
    save_state(&state);
}

/// Build a one-line nudge for an action that has a configured shortcut, or
/// `None` when the action is unbound or the hint has been shown enough times.
///
/// `hint_id` is a stable identifier (e.g. `"resume"`). `action` is a short
/// human phrase describing what the user just did the long way (e.g.
/// `"open the session picker"`). `shortcut_label` is the resolved, display
/// form of the binding (e.g. `"Cmd+R"`).
///
/// This does NOT record the show; call [`record_shown`] when the message is
/// actually surfaced so we only count nudges the user could see.
pub(crate) fn nudge_message(
    hint_id: &str,
    action: &str,
    shortcut_label: Option<&str>,
) -> Option<String> {
    let label = shortcut_label?;
    let label = label.trim();
    if label.is_empty() {
        return None;
    }
    if !should_show(hint_id) {
        return None;
    }
    Some(format!("💡 Tip: press {label} to {action}"))
}

impl App {
    /// Surface a shortcut nudge as a transient status notice, if the action is
    /// bound and the hint hasn't been shown too many times. Records the show.
    ///
    /// Returns true when a nudge was surfaced.
    pub(crate) fn maybe_hint_shortcut(
        &mut self,
        hint_id: &str,
        action: &str,
        shortcut_label: Option<&str>,
    ) -> bool {
        let Some(message) = nudge_message(hint_id, action, shortcut_label) else {
            return false;
        };
        self.set_status_notice(message);
        record_shown(hint_id);
        true
    }

    /// Nudge the user toward the `open_resume` shortcut after they reach the
    /// session picker the long way (typing `/resume`).
    pub(crate) fn hint_resume_shortcut(&mut self) {
        let label = crate::tui::keybind::load_open_resume_key().label;
        self.maybe_hint_shortcut("resume", "open the session picker", label.as_deref());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nudge_is_throttled_after_max_shows() {
        let _guard = crate::storage::lock_test_env();
        let temp = tempfile::tempdir().expect("tempdir");
        let prev = std::env::var_os("JCODE_HOME");
        crate::env::set_var("JCODE_HOME", temp.path());

        let id = "resume";
        // First MAX shows produce a message; record each.
        for _ in 0..MAX_SHOWS_PER_HINT {
            let msg = nudge_message(id, "open the session picker", Some("Cmd+R"));
            assert!(msg.is_some(), "expected a nudge while under the cap");
            record_shown(id);
        }
        // Now exhausted.
        assert!(
            nudge_message(id, "open the session picker", Some("Cmd+R")).is_none(),
            "nudge should stop after the cap"
        );

        if let Some(prev) = prev {
            crate::env::set_var("JCODE_HOME", prev);
        } else {
            crate::env::remove_var("JCODE_HOME");
        }
    }

    #[test]
    fn unbound_shortcut_yields_no_nudge() {
        let _guard = crate::storage::lock_test_env();
        let temp = tempfile::tempdir().expect("tempdir");
        let prev = std::env::var_os("JCODE_HOME");
        crate::env::set_var("JCODE_HOME", temp.path());

        assert!(nudge_message("x", "do the thing", None).is_none());
        assert!(nudge_message("x", "do the thing", Some("   ")).is_none());

        if let Some(prev) = prev {
            crate::env::set_var("JCODE_HOME", prev);
        } else {
            crate::env::remove_var("JCODE_HOME");
        }
    }
}
