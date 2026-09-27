//! Remote-client presentation state: the render-facing snapshot of a server
//! connection the TUI does not own.
//!
//! The live connection (`RemoteConnection`) and the connection loop's own
//! bookkeeping (`RemoteRunState`) are loop-local. What lives here is what the
//! renderer reads. `RemoteServerInfo` is a flat hello snapshot with no derived
//! state; `RemoteStartup` and `HistoryRecovery` are small state machines and
//! keep their fields private behind the ops that maintain them.

use super::RemoteStartupPhase;
use std::time::{Duration, Instant};

/// The server's hello snapshot, written from the History event.
///
/// Flat and invariant-free: the fields are exactly what the server reported, so
/// the one writer and the readers name them directly. The fallbacks around them
/// (`is_canary` defaulting to the session flag; name/icon/version falling back
/// to the registry) are caller-side, not state held here.
#[derive(Default)]
pub(super) struct RemoteServerInfo {
    pub(super) short_name: Option<String>,
    pub(super) icon: Option<String>,
    pub(super) version: Option<String>,
    pub(super) has_update: Option<bool>,
    pub(super) is_canary: Option<bool>,
    pub(super) sessions: Vec<String>,
    pub(super) client_count: Option<usize>,
}

/// The remote startup handshake phase and when it began.
///
/// Invariant: `started` is reset to now only when the phase changes (or when it
/// was unset), so re-setting the same phase does not restart the elapsed-time
/// budget the header uses to decide when to surface the phase label.
#[derive(Default)]
pub(super) struct RemoteStartup {
    pub(super) phase: Option<RemoteStartupPhase>,
    pub(super) started: Option<Instant>,
}

impl RemoteStartup {
    /// Move to `phase`. The elapsed-time baseline is reset only when the phase
    /// changes (or was unset), so re-issuing the same phase does not restart the
    /// header's grace period. This is the only production mutation; tests set
    /// `started` directly to age a phase.
    pub(super) fn set(&mut self, phase: RemoteStartupPhase) {
        let changed = self.phase.as_ref() != Some(&phase);
        self.phase = Some(phase);
        if changed || self.started.is_none() {
            self.started = Some(Instant::now());
        }
    }

    pub(super) fn clear(&mut self) {
        self.phase = None;
        self.started = None;
    }
}

/// First wait before the watchdog re-requests history. Generous enough that a
/// normal (slow) bootstrap completes on its own, short enough that a genuinely
/// stuck session recovers in seconds instead of requiring a manual `/restart`.
const FIRST_DELAY: Duration = Duration::from_secs(6);
/// Spacing between subsequent re-requests after the first one.
const RETRY_INTERVAL: Duration = Duration::from_secs(5);
/// How many times we re-request history before giving up and telling the user
/// to `/restart`. Bounded so a server that genuinely never answers does not
/// spin forever.
pub(super) const REMOTE_HISTORY_RECOVERY_MAX_ATTEMPTS: u32 = 4;

/// What the history-recovery watchdog should do on this tick.
pub(super) enum HistoryRecoveryStep {
    /// Not due yet, throttled, or already gave up.
    Wait,
    /// Budget exhausted; the caller surfaces the give-up hint once.
    GiveUp,
    /// The caller should re-request history.
    Retry { waited_secs: u64, attempt: u32 },
}

/// Watchdog for the "stuck on loading session…" bug.
///
/// Every remote prompt path is gated behind `RemoteConnection::has_loaded_history()`,
/// which only clears when the server delivers a `History` event. If that event
/// never arrives after a (re)connect or reload handoff, the client is stuck on
/// "loading session…" until a manual `/restart`. This holds the per-connection
/// retry budget; `step` is its only production mutation.
#[derive(Default)]
pub(super) struct HistoryRecovery {
    wait_started: Option<Instant>,
    attempts: u32,
    last_attempt: Option<Instant>,
}

impl HistoryRecovery {
    /// Arm a fresh budget for a new connection, so a stall on one connection
    /// does not exhaust the retries available to the next.
    pub(super) fn begin(&mut self) {
        *self = Self::default();
        self.wait_started = Some(Instant::now());
    }

    /// Disarm once history has loaded (or the connection is no longer waiting).
    pub(super) fn clear(&mut self) {
        *self = Self::default();
    }

    pub(super) fn is_waiting(&self) -> bool {
        self.wait_started.is_some()
    }

    /// Advance the watchdog. `frame_buffered` is true when a partial History
    /// frame is already in flight, which proves the response was not dropped.
    pub(super) fn step(&mut self, now: Instant, frame_buffered: bool) -> HistoryRecoveryStep {
        let Some(started) = self.wait_started else {
            // Begin tracking from the first tick that observes unloaded history
            // on a live connection.
            self.wait_started = Some(now);
            return HistoryRecoveryStep::Wait;
        };
        let waited = now.saturating_duration_since(started);
        if waited < FIRST_DELAY || frame_buffered {
            return HistoryRecoveryStep::Wait;
        }
        if self.attempts >= REMOTE_HISTORY_RECOVERY_MAX_ATTEMPTS {
            // Keep `attempts` at max so we do not re-enter the retry path, but
            // clear `last_attempt` so the give-up hint is surfaced exactly once.
            return if self.last_attempt.take().is_some() {
                HistoryRecoveryStep::GiveUp
            } else {
                HistoryRecoveryStep::Wait
            };
        }
        // Rate-limit re-requests so we don't flood the server.
        if let Some(last) = self.last_attempt
            && now.saturating_duration_since(last) < RETRY_INTERVAL
        {
            return HistoryRecoveryStep::Wait;
        }
        self.attempts += 1;
        self.last_attempt = Some(now);
        HistoryRecoveryStep::Retry {
            waited_secs: waited.as_secs(),
            attempt: self.attempts,
        }
    }

    #[cfg(test)]
    pub(super) fn attempts(&self) -> u32 {
        self.attempts
    }

    #[cfg(test)]
    pub(super) fn has_last_attempt(&self) -> bool {
        self.last_attempt.is_some()
    }

    #[cfg(test)]
    pub(super) fn force_waiting_since(&mut self, since: Instant) {
        self.wait_started = Some(since);
    }

    #[cfg(test)]
    pub(super) fn force_attempts(&mut self, attempts: u32) {
        self.attempts = attempts;
    }

    #[cfg(test)]
    pub(super) fn force_last_attempt(&mut self, at: Option<Instant>) {
        self.last_attempt = at;
    }
}
