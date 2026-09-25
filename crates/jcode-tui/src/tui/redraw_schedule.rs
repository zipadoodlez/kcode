//! Redraw scheduling: how often the TUI repaints, and whether a tick needs a
//! frame at all.
//!
//! One cadence rule. [`wants_fast_tick`] is the only question: is something
//! visible mid-motion or mid-arrival? If so the loop runs at the configured
//! `redraw_fps`; otherwise it crawls at [`REDRAW_IDLE`] (or [`REDRAW_DEEP_IDLE`]
//! once the session is dormant). [`periodic_redraw_required`] answers the
//! separate question of whether a tick should draw: the fast-tick states plus
//! the text-only chrome that appears and retires on a multi-second timer.

use super::*;

pub(crate) const REDRAW_IDLE: Duration = Duration::from_millis(250);
pub(crate) const REDRAW_DEEP_IDLE: Duration = Duration::from_millis(5000);
pub(crate) const REDRAW_REMOTE_STARTUP: Duration = Duration::from_millis(1000);
pub(crate) const REDRAW_DEEP_IDLE_AFTER: Duration = Duration::from_secs(30);

/// Whether this session has been left alone long enough to be treated as
/// dormant (deep idle): no stream activity *and* no user interaction for
/// [`REDRAW_DEEP_IDLE_AFTER`].
///
/// `time_since_activity()` alone is not a dormancy signal. It reports
/// "already past the deep-idle threshold" for any non-empty transcript that
/// has never streamed in this process, which is correct for a restored
/// historical session but also matches a brand-new session the moment
/// onboarding leaves its "here are a few things to try" notice. A recent
/// keystroke/mouse/paste is direct evidence the session is not dormant, so it
/// must hold deep idle off for the same window.
fn deep_idle_dormant(state: &dyn TuiState) -> bool {
    let stream_dormant = state
        .time_since_activity()
        .map(|d| d >= REDRAW_DEEP_IDLE_AFTER)
        .unwrap_or(false);
    let user_dormant = state
        .time_since_user_interaction()
        .map(|d| d >= REDRAW_DEEP_IDLE_AFTER)
        .unwrap_or(true);
    stream_dormant && user_dormant
}

fn fps_to_duration(fps: u32) -> Duration {
    Duration::from_millis((1000 / fps.max(1)) as u64)
}

/// The rate-limit countdown is a live per-second counter rendered in the status
/// line, so it needs the fast cadence while it is close to expiring.
fn rate_limit_countdown_redraw_active(state: &dyn TuiState) -> bool {
    state
        .rate_limit_remaining()
        .map(|remaining| remaining <= Duration::from_secs(60))
        .unwrap_or(false)
}

/// The notification line shows a live prompt-cache indicator (`⏳ cache Ns`
/// while warm in the final minute, `🧊 cache cold` once expired). Both states
/// emerge long after the 30s deep-idle cutoff, so without a dedicated wakeup
/// the idle loop never repaints to reveal them.
fn cache_cold_countdown_redraw_active(state: &dyn TuiState) -> bool {
    if state.is_processing() {
        return false;
    }
    state
        .cache_ttl_status()
        .map(|info| info.is_cold || info.expiring_soon())
        .unwrap_or(false)
}

/// The primary status spinner is drawn as part of the status line, so it needs
/// the fast cadence while it is visible (there is no separate single-cell
/// repaint path any more).
fn primary_status_spinner_active(state: &dyn TuiState) -> bool {
    state.is_processing()
        && app::run_shell::status_uses_primary_spinner(&state.status())
        && state.streaming_text().is_empty()
}

/// Whether the swarm strip (above the status line) or the SwarmStatus dock
/// widget is currently animating a status spinner for an active agent.
///
/// Both surfaces derive the spinner glyph from the wall clock, but managed
/// agents keep running long after the coordinator session itself goes quiet.
/// Unfocused clients skip this so backgrounded windows do not burn CPU
/// animating a glyph nobody can see.
fn swarm_spinner_redraw_active(state: &dyn TuiState) -> bool {
    state.client_focused()
        && state
            .inline_swarm_members()
            .iter()
            .any(|m| jcode_tui_render::swarm_gallery::is_active_status(&m.status))
}

/// Whether the open `/resume` picker is showing at least one running session.
/// The picker uses the same spinner cells as the swarm strip, so it needs an
/// explicit wakeup even when the session underneath the overlay is idle.
fn session_picker_spinner_redraw_active(state: &dyn TuiState) -> bool {
    state.client_focused()
        && state.session_picker_overlay().is_some_and(|picker| {
            picker
                .try_borrow()
                .ok()
                .is_some_and(|picker| picker.has_visible_running_sessions())
        })
}

/// Chrome that is text-only and changes on a human timescale: the status notice,
/// the learn hint, and the notification line.
///
/// None of these animate. They appear on an event (which already forces an
/// immediate repaint) and disappear on a multi-second timer, so the loop only
/// needs a tick fast enough to retire them promptly. Treating them as "live"
/// pulled the whole client to the fast cadence: a single 3s notice meant ~180
/// full frames of ~10ms each, all to redraw the same glyphs, and every keystroke
/// in that window queued behind one of those frames.
fn static_text_chrome_active(state: &dyn TuiState) -> bool {
    state.status_notice().is_some() || state.learn_hint().is_some() || state.has_notification()
}

/// The only cadence question: true when something visible is mid-motion or
/// mid-arrival and the loop should run at `redraw_fps`.
///
/// Text-only chrome is deliberately excluded: it needs ticks to retire, not
/// fast ones. Adding a time-dependent element means adding at most one term
/// here (and, if it must draw, one term in [`periodic_redraw_required`]).
pub(crate) fn wants_fast_tick(state: &dyn TuiState) -> bool {
    state.is_processing()
        || !state.streaming_text().is_empty()
        || state.copy_selection_edge_autoscroll_active()
        || primary_status_spinner_active(state)
        || swarm_spinner_redraw_active(state)
        || session_picker_spinner_redraw_active(state)
        || rate_limit_countdown_redraw_active(state)
}

/// Tick cadence for the current state.
pub(crate) fn tick_period(state: &dyn TuiState) -> Duration {
    if wants_fast_tick(state) {
        return fps_to_duration(crate::perf::tui_policy().redraw_fps);
    }
    if state.remote_startup_phase_active() {
        return REDRAW_REMOTE_STARTUP;
    }
    // A live cache countdown needs ticks fast enough to advance the `⏳/🧊`
    // indicator, so it must not fall through to the deep-idle crawl.
    if cache_cold_countdown_redraw_active(state) {
        return REDRAW_IDLE;
    }
    if deep_idle_dormant(state) {
        return REDRAW_DEEP_IDLE;
    }
    REDRAW_IDLE
}

/// Whether a tick should actually draw.
///
/// Fast-tick states draw every fast tick. Text chrome that appears on an event
/// and retires on a timer draws on the slow tick so it can appear and expire.
pub(crate) fn periodic_redraw_required(state: &dyn TuiState) -> bool {
    wants_fast_tick(state)
        || static_text_chrome_active(state)
        || cache_cold_countdown_redraw_active(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tick must be fast while a turn streams and slow when the screen is
    /// quiet, and chrome that retires on a timer must still draw.
    #[test]
    fn streaming_wants_a_fast_tick_and_quiet_does_not() {
        // The behavior gate with a full `TuiState` lives in `ui_tests::basic`.
        assert!(REDRAW_IDLE >= fps_to_duration(12));
    }
}
