//! App-side redraw scheduling state.

use std::time::Instant;

/// Pending full-screen repaint requests and the resize-redraw debounce. One
/// home, so the renderer's "does this frame need a full repaint?" decision reads
/// one struct instead of four loose fields.
#[derive(Default)]
pub(super) struct Redraw {
    /// One-shot flag: force the next paint to clear the terminal first. Needed
    /// after native terminal scrolls mutate the screen outside ratatui's diff
    /// model.
    pub(super) force_full_redraw: bool,
    /// One-shot flag: force the next paint to re-emit every cell by invalidating
    /// ratatui's previous buffer, without an intermediate ED2 clear escape.
    /// Chat scrolling uses this to clear wide-grapheme ghosts (ratatui #2357)
    /// without the clear-then-repaint flicker around kitty image placeholders
    /// (issue #404).
    pub(super) force_full_repaint: bool,
    /// Debounce redraw storms while the terminal is being resized.
    pub(super) last_resize_redraw: Option<Instant>,
    /// A throttled resize still needs one trailing geometry reset and repaint at
    /// the final terminal dimensions.
    pub(super) resize_redraw_pending: bool,
}

impl Redraw {
    pub(super) fn request_full_redraw(&mut self) {
        self.force_full_redraw = true;
    }

    /// Arm a full re-emit of every cell on the next frame without an
    /// intermediate ED2 clear escape. Prefer this over `request_full_redraw`
    /// when the real screen has not diverged from ratatui's model (e.g. chat
    /// scrolling), so image placeholder cells do not flash blank (issue #404).
    pub(super) fn request_full_repaint(&mut self) {
        self.force_full_repaint = true;
    }

    const RESIZE_REDRAW_MIN_INTERVAL: std::time::Duration = std::time::Duration::from_millis(33);

    fn commit_resize_redraw(&mut self, now: Instant) -> bool {
        self.last_resize_redraw = Some(now);
        self.resize_redraw_pending = false;
        true
    }

    pub(super) fn should_redraw_after_resize(&mut self) -> bool {
        let now = Instant::now();
        match self.last_resize_redraw {
            Some(last) if now.duration_since(last) < Self::RESIZE_REDRAW_MIN_INTERVAL => {
                self.resize_redraw_pending = true;
                false
            }
            _ => self.commit_resize_redraw(now),
        }
    }

    /// Flush the trailing edge of a debounced resize burst. Without this, the
    /// last resize event can be suppressed after an intermediate frame, leaving
    /// width/height-sensitive Mermaid placeholders and image state stale until
    /// some unrelated UI event happens to redraw the terminal.
    pub(super) fn flush_pending_resize_redraw(&mut self) -> bool {
        if !self.resize_redraw_pending {
            return false;
        }
        let now = Instant::now();
        match self.last_resize_redraw {
            Some(last) if now.duration_since(last) < Self::RESIZE_REDRAW_MIN_INTERVAL => false,
            _ => self.commit_resize_redraw(now),
        }
    }
}
