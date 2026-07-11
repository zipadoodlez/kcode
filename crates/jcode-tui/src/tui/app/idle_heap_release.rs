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

/// Recheck retained-heap growth while a client remains idle. A client can keep
/// receiving remote snapshots after its once-per-idle trim without becoming
/// "active" again, so the original edge-triggered trim alone can miss later
/// allocator growth for the rest of a long idle period.
const RETENTION_CHECK_INTERVAL: std::time::Duration = std::time::Duration::from_secs(30);

#[derive(Default)]
pub(super) struct IdleHeapRelease {
    /// True once the current idle period has already been trimmed. Reset when
    /// activity resumes so the next idle period trims again.
    trimmed_this_idle_period: bool,
    last_retention_check: Option<std::time::Instant>,
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
            self.idle_heap_release.last_retention_check = None;
            return;
        }

        let now = std::time::Instant::now();
        if retention_check_due(self.idle_heap_release.last_retention_check, now) {
            self.idle_heap_release.last_retention_check = Some(now);
            let threshold = crate::process_memory::retention_trim_threshold_bytes();
            if threshold != u64::MAX
                && crate::process_memory::release_retained_heap_if_excessive(
                    "client_retention_watchdog",
                    threshold,
                    RETENTION_CHECK_INTERVAL,
                )
            {
                self.idle_heap_release.trimmed_this_idle_period = true;
                return;
            }
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

fn retention_check_due(last_check: Option<std::time::Instant>, now: std::time::Instant) -> bool {
    last_check.is_none_or(|last| now.saturating_duration_since(last) >= RETENTION_CHECK_INTERVAL)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retention_check_runs_immediately_then_obeys_interval() {
        let now = std::time::Instant::now();
        assert!(retention_check_due(None, now));
        assert!(!retention_check_due(Some(now), now));
        assert!(retention_check_due(
            Some(now - RETENTION_CHECK_INTERVAL),
            now
        ));
    }
}
