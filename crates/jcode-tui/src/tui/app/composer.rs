//! App-side input composer state.

use crate::tui::core;

/// The text the user is composing: the buffer, the cursor, the stash, the undo
/// history, and the tab-completion cycle. One home, so the composer's small
/// operations read one struct.
///
/// The keys, commands, and suggestion logic that also touch other `App` state
/// stay on `App` and read through this struct.
#[derive(Default)]
pub(super) struct Composer {
    pub(super) input: String,
    pub(super) cursor_pos: usize,
    /// Stashed input: saved via Ctrl+S for later retrieval.
    pub(super) stashed_input: Option<(String, usize)>,
    /// Undo history for in-progress input editing (Ctrl+Z).
    pub(super) input_undo_stack: Vec<(String, usize)>,
    /// Tab-completion cycle: (base_input, suggestion_index).
    pub(super) tab_completion_state: Option<(String, usize)>,
    /// Large pastes, shown as placeholders in `input` and expanded on submit.
    pub(super) pasted_contents: Vec<String>,
    /// Images (media_type, base64) attached to the next message.
    pub(super) pending_images: Vec<(String, String)>,
}

impl Composer {
    const INPUT_UNDO_LIMIT: usize = 128;

    pub(super) fn clear_input_undo_history(&mut self) {
        self.input_undo_stack.clear();
    }

    pub(super) fn remember_input_undo_state(&mut self) {
        let snapshot = (self.input.clone(), self.cursor_pos.min(self.input.len()));
        if self.input_undo_stack.last() == Some(&snapshot) {
            return;
        }
        if self.input_undo_stack.len() >= Self::INPUT_UNDO_LIMIT {
            self.input_undo_stack.remove(0);
        }
        self.input_undo_stack.push(snapshot);
    }

    pub(super) fn find_word_boundary_back(&self) -> usize {
        if self.cursor_pos == 0 {
            return 0;
        }
        let mut pos = self.cursor_pos;

        // Move back one char
        pos = core::prev_char_boundary(&self.input, pos);

        // Skip trailing whitespace
        while pos > 0 {
            let ch = self.input[pos..].chars().next().unwrap_or(' ');
            if !ch.is_whitespace() {
                break;
            }
            pos = core::prev_char_boundary(&self.input, pos);
        }

        // Skip word characters
        while pos > 0 {
            let prev = core::prev_char_boundary(&self.input, pos);
            let ch = self.input[prev..].chars().next().unwrap_or(' ');
            if ch.is_whitespace() {
                break;
            }
            pos = prev;
        }

        pos
    }

    pub(super) fn find_word_boundary_forward(&self) -> usize {
        let len = self.input.len();
        if self.cursor_pos >= len {
            return len;
        }
        let mut pos = self.cursor_pos;

        // Skip current word
        while pos < len {
            let ch = self.input[pos..].chars().next().unwrap_or(' ');
            if ch.is_whitespace() {
                break;
            }
            pos = core::next_char_boundary(&self.input, pos);
        }

        // Skip whitespace
        while pos < len {
            let ch = self.input[pos..].chars().next().unwrap_or(' ');
            if !ch.is_whitespace() {
                break;
            }
            pos = core::next_char_boundary(&self.input, pos);
        }

        pos
    }
}
