//! Idle-time retained-heap release.
//!
//! The turn-completion trim hook rarely fires for clients that sit idle for
//! long stretches (or that only observe another session's work), so glibc
//! arena retention accumulates: measured ~105 MB per long-lived client, of
//! which malloc_trim(0) recovers ~90-100 MB. This module trims once per idle
//! period from the tick loop: when the app has been quiet past the deep-idle
//! threshold, release retained heap, then arm again only after activity
//! resumes.

use super::*;

/// How long the client must be quiet before an idle trim fires. Matches the
/// deep-idle redraw threshold so trims never race active rendering.
const IDLE_TRIM_AFTER: std::time::Duration = std::time::Duration::from_secs(60);

#[derive(Default)]
pub(super) struct IdleHeapRelease {
    /// True once the current idle period has already been trimmed. Reset when
    /// activity resumes so the next idle period trims again.
    trimmed_this_idle_period: bool,
}

impl App {
    /// Called from the periodic tick loops (local and remote). Trims retained
    /// heap once per idle period, going quiet until the next busy->idle edge.
    pub(super) fn maybe_release_idle_heap(&mut self) {
        let idle = !crate::tui::TuiState::is_processing(self)
            && self.streaming.streaming_text.is_empty()
            && crate::tui::TuiState::time_since_activity(self)
                .is_none_or(|since| since >= IDLE_TRIM_AFTER);

        if !idle {
            self.idle_heap_release.trimmed_this_idle_period = false;
            return;
        }

        if self.idle_heap_release.trimmed_this_idle_period {
            return;
        }

        // Shared debounce with the turn-completion hook, so a turn that just
        // trimmed does not get an immediate duplicate idle trim.
        if crate::process_memory::release_retained_heap_debounced(
            "client_idle",
            std::time::Duration::from_secs(60),
        ) {
            self.idle_heap_release.trimmed_this_idle_period = true;
        }
    }
}
