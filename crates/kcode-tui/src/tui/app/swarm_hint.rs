//! One-time TUI nudge teaching that the swarm config is a prompt file.
//!
//! Swarms are complicated, dynamic systems, so their routing policy (which
//! model/effort each spawned worker gets) is passed to the model as a prompt
//! (`swarm-prompt.md`) rather than as options in a standard config file. Users
//! do not discover this on their own, so the first few times the root session
//! actually invokes the `swarm` tool we surface a short hint pointing at the
//! editable prompt file.
//!
//! Follows the same shape as `shortcut_hints`: at most one show per session,
//! a small lifetime show cap persisted to disk, rendered in the learn-hint
//! pop-out slot so it is visually distinct from tool output.

use serde::{Deserialize, Serialize};

use super::App;
use super::hint_state;

/// Never show the swarm-config hint more than this many times, ever.
const MAX_SHOWS: u32 = 3;

const STATE_FILE: &str = "swarm_config_hint.json";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct SwarmHintState {
    #[serde(default)]
    shows: u32,
}

/// Pure decision: should the hint be shown given the persisted show count and
/// whether it was already shown this session?
pub(super) fn should_show(shows: u32, shown_this_session: bool) -> bool {
    !shown_this_session && shows < MAX_SHOWS
}

/// The hint text pointing at the editable swarm prompt/config file.
pub(super) fn hint_message() -> String {
    "\u{2699} Swarm routing (models, effort) is configured by a prompt, not a config file. Edit ~/.kcode/swarm-prompt.md (or ./.kcode/swarm-prompt.md) to tune it".to_string()
}

impl App {
    /// Surface the swarm-config hint the first few times the user's session
    /// invokes the `swarm` tool. No-op after the lifetime cap or once shown
    /// this session. Rendered in the learn-hint pop-out slot.
    pub(in crate::tui::app) fn maybe_surface_swarm_config_hint(&mut self) {
        if !should_show(
            hint_state::load::<SwarmHintState>(STATE_FILE).shows,
            self.swarm_hint_shown_this_session,
        ) {
            return;
        }
        let mut state = hint_state::load::<SwarmHintState>(STATE_FILE);
        state.shows = state.shows.saturating_add(1);
        hint_state::save(STATE_FILE, &state);
        self.swarm_hint_shown_this_session = true;
        self.learn_hint = Some((hint_message(), std::time::Instant::now()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shows_until_lifetime_cap() {
        assert!(should_show(0, false));
        assert!(should_show(MAX_SHOWS - 1, false));
        assert!(!should_show(MAX_SHOWS, false));
        assert!(!should_show(MAX_SHOWS + 5, false));
    }

    #[test]
    fn shows_at_most_once_per_session() {
        assert!(!should_show(0, true));
    }

    #[test]
    fn hint_mentions_the_prompt_file_and_that_it_is_the_config() {
        let message = hint_message();
        assert!(message.contains("swarm-prompt.md"));
        assert!(message.contains("prompt, not a config file"));
    }

    #[test]
    fn state_persists_show_count() {
        let _guard = crate::storage::lock_test_env();
        let temp = tempfile::tempdir().expect("tempdir");
        let prev = std::env::var_os("KCODE_HOME");
        crate::env::set_var("KCODE_HOME", temp.path());

        let mut state = hint_state::load::<SwarmHintState>(STATE_FILE);
        assert_eq!(state.shows, 0);
        state.shows = 2;
        hint_state::save(STATE_FILE, &state);
        assert_eq!(hint_state::load::<SwarmHintState>(STATE_FILE).shows, 2);

        if let Some(prev) = prev {
            crate::env::set_var("KCODE_HOME", prev);
        } else {
            crate::env::remove_var("KCODE_HOME");
        }
    }
}
