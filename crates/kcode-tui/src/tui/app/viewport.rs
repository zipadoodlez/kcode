//! App-side chat viewport scroll state.

use super::HistoryScrollAnchor;
use crate::tui::ui;

/// Where the chat transcript is scrolled and how it follows new content: the
/// scroll offset, the auto-scroll pause, the prepend anchor, the bookmark, and
/// the typing scroll lock. One home, so the scroll math and the keys that drive
/// it read one struct.
///
/// The keys and commands that also touch status notices or the compacted-history
/// loader stay on `App` and read through this struct.
#[derive(Default)]
pub(super) struct Viewport {
    pub(super) scroll_offset: usize,
    /// Pauses auto-scroll when user scrolls up during streaming.
    pub(super) auto_scroll_paused: bool,
    /// When older compacted history has just been loaded, this anchors the
    /// viewport to the content the reader was looking at so the prepend does not
    /// visibly jump. Resolved into `scroll_offset` by the next render frame.
    pub(super) pending_history_anchor: Option<HistoryScrollAnchor>,
    /// Stashed scroll position for quick teleport back.
    pub(super) scroll_bookmark: Option<usize>,
    /// Keep the current chat viewport while typing instead of snapping to bottom.
    pub(super) typing_scroll_lock: bool,
}

impl Viewport {
    pub(super) fn follow_chat_bottom(&mut self) {
        self.pending_history_anchor = None;
        self.scroll_offset = 0;
        self.auto_scroll_paused = false;
    }

    pub(super) fn debug_scroll_top(&mut self) {
        self.scroll_offset = 0;
        self.auto_scroll_paused = true;
    }

    pub(super) fn follow_chat_bottom_for_typing(&mut self) {
        if !self.typing_scroll_lock {
            self.follow_chat_bottom();
        }
    }

    /// Capture a viewport anchor describing the reader's current distance from
    /// the bottom of the transcript, plus any leftover upward scroll intent that
    /// could not be satisfied because the view was already at the top of the
    /// currently-loaded content. The next render that includes the newly loaded
    /// (prepended) history resolves this back into an absolute `scroll_offset`,
    /// keeping the content under the reader stable across the load.
    pub(super) fn capture_history_anchor(&mut self, overshoot: usize) {
        // Don't clobber an anchor that is still waiting to be resolved; the
        // original distance-from-bottom remains correct across further prepends.
        if self.pending_history_anchor.is_some() {
            return;
        }
        let total = ui::last_total_wrapped_lines();
        if total == 0 {
            return;
        }
        // The top of the viewport currently sits at absolute line `scroll_offset`
        // within the pre-prepend transcript (length `total`). Its distance from
        // the bottom is invariant when older lines are prepended, so capture it
        // (plus any unsatisfied upward intent as `overshoot`) and let the next
        // render map it back to an absolute offset against the larger total.
        let scroll = self.scroll_offset.min(total);
        let lines_from_bottom = total.saturating_sub(scroll).saturating_add(overshoot);
        self.pending_history_anchor = Some(HistoryScrollAnchor {
            lines_from_bottom,
            base_total: total,
        });
    }

    /// Adopt a resolved history anchor once a frame containing the newly loaded
    /// content has rendered. Returns true when the scroll position changed.
    pub(super) fn reconcile_history_anchor(&mut self) -> bool {
        let Some(anchor) = self.pending_history_anchor else {
            return false;
        };
        let total = ui::last_total_wrapped_lines();
        // Wait until a frame with the prepended content has actually rendered
        // (its total wrapped-line count differs from the captured base).
        if total == 0 || total == anchor.base_total {
            return false;
        }
        let resolved = ui::last_resolved_chat_scroll();
        self.pending_history_anchor = None;
        let changed = self.scroll_offset != resolved || !self.auto_scroll_paused;
        self.scroll_offset = resolved;
        self.auto_scroll_paused = true;
        changed
    }

    /// Scroll to the next user prompt (scroll down - later in conversation)
    pub fn scroll_to_next_prompt(&mut self) {
        let positions = ui::last_user_prompt_positions();
        if positions.is_empty() || !self.auto_scroll_paused {
            return;
        }
        self.pending_history_anchor = None;

        let current = self.scroll_offset;

        // Find the first position strictly greater than current (i.e. later/below).
        for &pos in &positions {
            if pos > current {
                self.scroll_offset = pos;
                return;
            }
        }

        // No more prompts below - go to bottom
        self.follow_chat_bottom();
    }
}
