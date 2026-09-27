//! The chat transcript: the display messages and the state derived from them.

use super::DisplayMessageRoleExt;
use crate::tui::ui::tools_ui;
use jcode_tui_messages::DisplayMessage;

/// The rendered conversation plus the caches that must move with it.
///
/// Invariant: `messages` is the only source of truth. `user_message_count` and
/// `edit_tool_message_count` are `O(1)` caches of it (rescanning the whole
/// transcript on every append was `O(M^2)` over a long session). Every mutation
/// below maintains them, and `messages` is not reachable for mutation from
/// outside, so a caller cannot change the transcript and leave the counters
/// stale. Mutations report what they changed; deciding that downstream caches
/// need invalidating (`App::bump_display_messages_version`) stays with `App`,
/// because those are other subsystems' caches, not the transcript's.
#[derive(Default)]
pub(super) struct Transcript {
    messages: Vec<DisplayMessage>,
    version: u64,
    user_message_count: usize,
    edit_tool_message_count: usize,
}

impl Transcript {
    pub(super) fn messages(&self) -> &[DisplayMessage] {
        &self.messages
    }

    pub(super) fn version(&self) -> u64 {
        self.version
    }

    pub(super) fn user_message_count(&self) -> usize {
        self.user_message_count
    }

    pub(super) fn has_edit_tool_messages(&self) -> bool {
        self.edit_tool_message_count > 0
    }

    pub(super) fn edit_tool_message_count(&self) -> usize {
        self.edit_tool_message_count
    }

    /// Overwrite the cached edit-tool count. Test-only: a test uses this to
    /// simulate a stale counter that the fallback path has to tolerate.
    #[cfg(test)]
    pub(super) fn force_edit_tool_message_count(&mut self, count: usize) {
        self.edit_tool_message_count = count;
    }

    /// Advance the change counter every render cache keyed on the transcript
    /// compares against.
    pub(super) fn bump_version(&mut self) {
        self.version = self.version.wrapping_add(1);
    }

    /// Replace the whole transcript, e.g. after a reload or a history window.
    pub(super) fn set_all(&mut self, messages: Vec<DisplayMessage>) {
        self.messages = messages;
        self.recount();
    }

    /// Replace the whole transcript and adopt a captured version verbatim, for
    /// snapshot restore. The version travels with the messages so caches keyed
    /// on it see exactly the state they saw when the snapshot was taken.
    pub(super) fn set_all_with_version(&mut self, messages: Vec<DisplayMessage>, version: u64) {
        self.messages = messages;
        self.recount();
        self.version = version;
    }

    pub(super) fn append(&mut self, message: DisplayMessage) {
        self.adjust_counters(&message, true);
        self.messages.push(message);
    }

    /// Prepend a message. Only the scroll tests need this, to grow the
    /// transcript upward.
    #[cfg(test)]
    pub(super) fn insert(&mut self, idx: usize, message: DisplayMessage) {
        self.adjust_counters(&message, true);
        self.messages.insert(idx, message);
    }

    pub(super) fn clear(&mut self) {
        self.messages.clear();
        self.user_message_count = 0;
        self.edit_tool_message_count = 0;
    }

    pub(super) fn pop(&mut self) -> Option<DisplayMessage> {
        let popped = self.messages.pop()?;
        self.adjust_counters(&popped, false);
        Some(popped)
    }

    pub(super) fn remove(&mut self, idx: usize) -> Option<DisplayMessage> {
        if idx >= self.messages.len() {
            return None;
        }
        let removed = self.messages.remove(idx);
        self.adjust_counters(&removed, false);
        Some(removed)
    }

    /// Keep only the messages `keep` accepts. Returns whether anything was
    /// dropped.
    pub(super) fn retain(&mut self, keep: impl FnMut(&DisplayMessage) -> bool) -> bool {
        let before = self.messages.len();
        self.messages.retain(keep);
        if self.messages.len() == before {
            return false;
        }
        // An arbitrary subset changed, so recomputing beats incremental edits.
        self.recount();
        true
    }

    /// Rewrite one message's body. `None` when `idx` is out of range, otherwise
    /// whether the content actually changed.
    pub(super) fn replace_content(&mut self, idx: usize, content: String) -> Option<bool> {
        let message = self.messages.get_mut(idx)?;
        if message.content == content {
            return Some(false);
        }
        message.content = content;
        Some(true)
    }

    /// Rewrite one message's title and body. `None` when `idx` is out of range,
    /// otherwise whether either actually changed.
    pub(super) fn replace_title_and_content(
        &mut self,
        idx: usize,
        title: Option<String>,
        content: String,
    ) -> Option<bool> {
        let message = self.messages.get_mut(idx)?;
        if message.title == title && message.content == content {
            return Some(false);
        }
        message.title = title;
        message.content = content;
        Some(true)
    }

    /// Append `line` to the newest message `matches` accepts, retitling it.
    /// Returns false when nothing matched, so the caller can start a new card.
    pub(super) fn append_line_to_last_matching(
        &mut self,
        matches: impl Fn(&DisplayMessage) -> bool,
        line: &str,
        title: &str,
    ) -> bool {
        let Some(idx) = self.messages.iter().rposition(matches) else {
            return false;
        };
        let message = &mut self.messages[idx];
        if !message.content.is_empty() {
            message.content.push('\n');
        }
        message.content.push_str(line);
        message.title = Some(title.to_string());
        true
    }

    /// Write a finished tool's output into the card that carries its call id.
    /// Returns false when no card matches (the tool never got a transcript row).
    pub(super) fn replace_tool_output(&mut self, tool_call_id: &str, content: String) -> bool {
        let Some(message) = self.messages.iter_mut().rev().find(|message| {
            message.tool_data.as_ref().map(|tool| tool.id.as_str()) == Some(tool_call_id)
        }) else {
            return false;
        };
        message.content = content;
        true
    }

    /// Recompute the memoized counters from the messages.
    fn recount(&mut self) {
        self.user_message_count = self
            .messages
            .iter()
            .filter(|message| message.effective_role() == "user")
            .count();
        self.edit_tool_message_count = self
            .messages
            .iter()
            .filter(|message| Self::is_edit_tool_message(message))
            .count();
    }

    fn is_edit_tool_message(message: &DisplayMessage) -> bool {
        message
            .tool_data
            .as_ref()
            .map(|tool| tools_ui::is_edit_tool_name(&tool.name))
            .unwrap_or(false)
    }

    /// Fold a single message into the cached counters with the given sign (+1
    /// when added, -1 when removed), so a single append or removal stays O(1).
    fn adjust_counters(&mut self, message: &DisplayMessage, added: bool) {
        let delta: isize = if added { 1 } else { -1 };
        if message.effective_role() == "user" {
            self.user_message_count = (self.user_message_count as isize + delta).max(0) as usize;
        }
        if Self::is_edit_tool_message(message) {
            self.edit_tool_message_count =
                (self.edit_tool_message_count as isize + delta).max(0) as usize;
        }
    }
}
