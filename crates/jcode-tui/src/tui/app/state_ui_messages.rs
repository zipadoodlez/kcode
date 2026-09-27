use super::state_ui_storage::{
    compact_display_message_tool_data, compact_display_messages_for_storage,
};
use super::*;

const COMPACTED_HISTORY_CHUNK_MESSAGES: usize = 64;
const COMPACTED_HISTORY_LOAD_SCROLL_THRESHOLD: usize = 2;
const COMPACTED_HISTORY_MARKER_PREFIX: &str = "Earlier conversation compacted - ";

pub(super) fn is_background_task_lifecycle_message(content: &str) -> bool {
    let content = content.trim_start();
    content.starts_with("**Background task**")
        || content.starts_with("**Background task started**")
        || content.starts_with("**Background task progress**")
        || content.starts_with("**Background task stalled**")
}

impl App {
    pub fn push_display_message(&mut self, mut message: DisplayMessage) {
        if is_background_task_lifecycle_message(&message.content) {
            return;
        }
        compact_display_message_tool_data(&mut message);
        // A trailing Ctrl+L spacer only exists to keep the screen clear while
        // idle. The moment real content arrives, drop it so the transcript
        // stays contiguous instead of keeping a screenful of blank rows
        // embedded in the scrollback.
        if message.role != "spacer"
            && self
                .transcript
                .messages()
                .last()
                .is_some_and(|last| last.role == "spacer")
        {
            self.transcript.pop();
        }
        if self.try_coalesce_repeated_display_message(&message) {
            return;
        }
        let is_tool = message.role == "tool";
        // Track the trailing run of assistant messages so a provider
        // RetryRollback can remove exactly the current attempt's committed
        // output. Any non-assistant message (user/tool/system) is a fence: it
        // proves earlier assistant messages belong to completed work.
        if message.role == "assistant" {
            self.attempt_committed_assistant_messages += 1;
        } else {
            self.attempt_committed_assistant_messages = 0;
        }
        self.transcript.append(message);
        self.bump_display_messages_version();
        if is_tool && self.diff_mode.has_side_pane() && self.diff_pane_auto_scroll {
            self.diff_pane_scroll = usize::MAX;
        }
    }

    pub(super) fn replace_display_messages(&mut self, mut messages: Vec<DisplayMessage>) {
        messages.retain(|message| !is_background_task_lifecycle_message(&message.content));
        compact_display_messages_for_storage(&mut messages);
        self.transcript.set_all(messages);
        self.attempt_committed_assistant_messages = 0;
        self.sync_compacted_history_lazy_from_display_messages();
        self.bump_display_messages_version();
        self.note_runtime_memory_event_force("display_messages_replaced", "display_history_reset");
    }

    pub(super) fn replace_display_message_content(&mut self, idx: usize, content: String) -> bool {
        match self.transcript.replace_content(idx, content) {
            None => false,
            Some(changed) => {
                if changed {
                    self.bump_display_messages_version();
                }
                true
            }
        }
    }

    pub(super) fn replace_display_message_title_and_content(
        &mut self,
        idx: usize,
        title: Option<String>,
        content: String,
    ) -> bool {
        match self
            .transcript
            .replace_title_and_content(idx, title, content)
        {
            None => false,
            Some(changed) => {
                if changed {
                    self.bump_display_messages_version();
                }
                true
            }
        }
    }

    pub(super) fn replace_latest_tool_display_message(
        &mut self,
        tool_call_id: &str,
        title: Option<String>,
        content: String,
    ) -> bool {
        let Some(idx) = self.transcript.messages().iter().rposition(|message| {
            message.tool_data.as_ref().map(|tool| tool.id.as_str()) == Some(tool_call_id)
        }) else {
            return false;
        };

        // A tool moved to the background finishes its foreground card with the
        // background lifecycle notification returned by the tool. The same
        // notification also drives the retained row in the pinned status band,
        // so remove the transient tool card instead of turning it into a second
        // transcript representation.
        if is_background_task_lifecycle_message(&content) {
            self.remove_display_message(idx);
            return true;
        }

        self.replace_display_message_title_and_content(idx, title, content)
    }

    pub(super) fn remove_display_message(&mut self, idx: usize) -> Option<DisplayMessage> {
        let removed = self.transcript.remove(idx);
        if removed.is_some() {
            self.bump_display_messages_version();
        }
        removed
    }

    pub(super) fn append_reload_message(&mut self, line: &str) {
        if self
            .transcript
            .append_line_to_last_matching(Self::is_reload_message, line, "Reload")
        {
            self.bump_display_messages_version();
        } else {
            self.push_display_message(
                DisplayMessage::system(line.to_string()).with_title("Reload"),
            );
        }
    }

    pub(super) fn is_client_maintenance_message(message: &DisplayMessage, title: &str) -> bool {
        message.role == "system" && message.title.as_deref() == Some(title)
    }

    pub(super) fn is_reload_message(message: &DisplayMessage) -> bool {
        message.role == "system"
            && message
                .title
                .as_deref()
                .is_some_and(|title| title == "Reload" || title.starts_with("Reload: "))
    }

    fn try_coalesce_repeated_display_message(&mut self, message: &DisplayMessage) -> bool {
        if !Self::is_repeat_compactable_display_message(message) {
            return false;
        }

        let Some(last) = self.transcript.messages().last() else {
            return false;
        };
        if !Self::is_repeat_compactable_display_message(last) {
            return false;
        }

        let (last_base, last_count) = Self::split_repeat_suffix(&last.content);
        if last.role != message.role
            || last.title != message.title
            || last.tool_calls != message.tool_calls
            || last.duration_secs != message.duration_secs
            || last_base != message.content
        {
            return false;
        }

        let next_count = last_count.saturating_add(1);
        let merged = Self::format_repeated_display_content(message.content.as_str(), next_count);
        let idx = self.transcript.messages().len() - 1;
        if self.transcript.replace_content(idx, merged) == Some(true) {
            self.bump_display_messages_version();
        }
        true
    }

    fn is_repeat_compactable_display_message(message: &DisplayMessage) -> bool {
        matches!(message.role.as_str(), "system" | "error")
            && message.title.is_none()
            && message.tool_calls.is_empty()
            && message.tool_data.is_none()
            && message.duration_secs.is_none()
            && !message.content.contains(['\n', '\r'])
    }

    fn split_repeat_suffix(content: &str) -> (&str, u32) {
        const REPEAT_PREFIX: &str = " [×";

        let Some(prefix_idx) = content.rfind(REPEAT_PREFIX) else {
            return (content, 1);
        };
        if !content.ends_with(']') {
            return (content, 1);
        }

        let digits = &content[prefix_idx + REPEAT_PREFIX.len()..content.len() - 1];
        if digits.is_empty() || !digits.chars().all(|ch| ch.is_ascii_digit()) {
            return (content, 1);
        }

        match digits.parse::<u32>() {
            Ok(count) if count >= 2 => (&content[..prefix_idx], count),
            _ => (content, 1),
        }
    }

    fn format_repeated_display_content(content: &str, repeat_count: u32) -> String {
        if repeat_count <= 1 {
            content.to_string()
        } else {
            format!("{content} [×{repeat_count}]")
        }
    }

    pub(super) fn clear_display_messages(&mut self) {
        self.compacted_history_lazy = CompactedHistoryLazyState::default();
        // The transcript is about to be discarded; forget where the live reasoning
        // block started so a stale offset can't slice the new stream.
        self.reasoning.block_start = None;
        self.reasoning.turn_traces.clear();
        if !self.transcript.messages().is_empty() {
            self.transcript.clear();
            self.bump_display_messages_version();
        }
    }

    /// Terminal-style clear (Ctrl+L): append a viewport-height blank spacer
    /// and snap to the bottom, so the screen shows a clean prompt while the
    /// whole transcript stays one scroll-up away, exactly like a terminal's
    /// clear-with-scrollback. Nothing is deleted; context, queue, and draft
    /// are untouched. Contrast with `/cls` (`clear_view_keep_context`), which
    /// actually wipes the rendered transcript.
    pub(super) fn clear_view_terminal_style(&mut self) {
        let rows = super::super::ui::last_chat_viewport_height();
        // Pressing Ctrl+L repeatedly (or on an already-empty screen) should
        // not stack blank pages: with a trailing spacer and nothing after it,
        // the viewport is already visually clear, so just re-snap.
        let already_clear = self
            .transcript
            .messages()
            .last()
            .is_some_and(|message| message.role == "spacer");
        if rows > 0 && !already_clear && !self.transcript.messages().is_empty() {
            self.push_display_message(DisplayMessage::spacer(rows));
        }
        self.viewport.follow_chat_bottom();
    }

    /// Whether the view is in the terminal-style cleared state: the transcript
    /// ends in a Ctrl+L spacer, the viewport is pinned to the bottom, and no
    /// new output has arrived since. In that state every visible transcript row
    /// is blank, so the renderer collapses the messages area and the numbered
    /// prompt sits at the top of the screen like a terminal after `clear`.
    /// Any new message, stream, or scroll-up immediately ends it.
    pub(crate) fn terminal_clear_collapsed(&self) -> bool {
        !self.viewport.auto_scroll_paused
            && self.viewport.pending_history_anchor.is_none()
            && !self.is_processing
            && self.streaming.streaming_text.is_empty()
            && self
                .transcript
                .messages()
                .last()
                .is_some_and(|message| message.role == "spacer")
    }

    /// View-only clear (`/cls`): wipe the rendered transcript while
    /// keeping provider context, queued messages, and the input draft intact,
    /// so the model still remembers everything. Contrast with `/clear`
    /// (`reset_current_session`), which discards context too, and Ctrl+L,
    /// which merely snaps to the bottom of the chat.
    pub(super) fn clear_view_keep_context(&mut self) {
        self.clear_display_messages();
        self.viewport.scroll_offset = 0;
        self.viewport.auto_scroll_paused = false;
        self.set_status_notice("View cleared (context kept)");
    }

    pub(super) fn apply_compacted_history_window(
        &mut self,
        mut messages: Vec<DisplayMessage>,
        _images: Vec<crate::session::RenderedImage>,
        total_messages: usize,
        visible_messages: usize,
        remaining_messages: usize,
        hidden_user_prompts: usize,
    ) {
        compact_display_messages_for_storage(&mut messages);
        self.transcript.set_all(messages);
        self.compacted_history_lazy = CompactedHistoryLazyState {
            total_messages,
            visible_messages,
            remaining_messages,
            hidden_user_prompts,
            pending_request_visible: None,
        };
        self.viewport.auto_scroll_paused = true;
        // Older messages are prepended above the current view. If the reader had
        // an anchor captured (they scrolled up to trigger this load), leave the
        // scroll position for the next render to resolve so the content under
        // them stays put instead of teleporting to the new absolute top. Only
        // fall back to the top when there is no anchor to honor.
        if self.viewport.pending_history_anchor.is_none() {
            self.viewport.scroll_offset = 0;
        }
        self.bump_display_messages_version();
        self.note_runtime_memory_event_force(
            "compacted_history_loaded",
            "display_history_lazy_window",
        );
        if remaining_messages > 0 {
            self.set_status_notice(format!(
                "Loaded {} compacted messages · {} older hidden",
                visible_messages, remaining_messages
            ));
        } else if total_messages > 0 {
            self.set_status_notice(format!("Loaded all {} compacted messages", total_messages));
        }
    }

    /// Number of wrapped lines from the top of the chat viewport that should be
    /// treated as the "near the top" zone that proactively loads older history.
    /// Prefetching roughly one viewport ahead means scrolling up keeps flowing
    /// instead of stalling at a hard wall and then jumping.
    fn compacted_history_prefetch_threshold(&self) -> usize {
        let viewport = crate::tui::ui::last_layout_snapshot()
            .map(|layout| layout.messages_area.height as usize)
            .unwrap_or(0);
        // Trigger when within ~one viewport of the top, with a small floor so the
        // behavior is sensible even before the first layout snapshot exists.
        viewport.max(COMPACTED_HISTORY_LOAD_SCROLL_THRESHOLD)
    }

    /// Capture a viewport anchor describing the reader's current distance from
    /// the bottom of the transcript, plus any leftover upward scroll intent that
    /// could not be satisfied because the view was already at the top of the
    /// currently-loaded content. The next render that includes the newly loaded
    /// (prepended) history resolves this back into an absolute `scroll_offset`,
    /// keeping the content under the reader stable across the load.

    /// Adopt a resolved history anchor once a frame containing the newly loaded
    /// content has rendered. Returns true when the scroll position changed.

    pub(super) fn maybe_queue_compacted_history_load(&mut self) {
        self.maybe_queue_compacted_history_load_with_overshoot(0);
    }

    pub(super) fn maybe_queue_compacted_history_load_with_overshoot(&mut self, overshoot: usize) {
        if !self.viewport.auto_scroll_paused {
            return;
        }
        if self.viewport.scroll_offset > self.compacted_history_prefetch_threshold() {
            return;
        }
        if self.compacted_history_lazy.remaining_messages == 0 {
            return;
        }
        if self
            .compacted_history_lazy
            .pending_request_visible
            .is_some()
        {
            return;
        }
        // Throttle to one chunk per settled frame: while an anchor is still
        // waiting to resolve on screen, hold off so prepends never compound into
        // a visible jump.
        if self.viewport.pending_history_anchor.is_some() {
            return;
        }

        let next_visible = self
            .compacted_history_lazy
            .visible_messages
            .saturating_add(COMPACTED_HISTORY_CHUNK_MESSAGES)
            .min(self.compacted_history_lazy.total_messages);
        if next_visible <= self.compacted_history_lazy.visible_messages {
            return;
        }

        // Anchor the viewport before mutating so the prepend stays seamless.
        self.viewport.capture_history_anchor(overshoot);

        if self.is_remote_client() {
            self.compacted_history_lazy.pending_request_visible = Some(next_visible);
            self.set_status_notice(format!(
                "Loading older compacted history… {} of {}",
                next_visible, self.compacted_history_lazy.total_messages
            ));
        } else {
            self.apply_local_compacted_history_window(next_visible);
        }
    }

    pub(super) fn take_pending_compacted_history_load(&mut self) -> Option<usize> {
        self.compacted_history_lazy.pending_request_visible.take()
    }

    /// Whether there are older compacted-history messages not yet loaded into the
    /// display transcript.
    pub(super) fn compacted_history_has_remaining(&self) -> bool {
        self.compacted_history_lazy.remaining_messages > 0
    }

    pub(super) fn restore_pending_compacted_history_load(&mut self, visible_messages: usize) {
        self.compacted_history_lazy.pending_request_visible = Some(visible_messages);
    }

    #[cfg(test)]
    pub(super) fn compacted_history_lazy_state(&self) -> &CompactedHistoryLazyState {
        &self.compacted_history_lazy
    }

    fn sync_compacted_history_lazy_from_display_messages(&mut self) {
        let mut lazy = self
            .transcript
            .messages()
            .first()
            .and_then(parse_compacted_history_marker)
            .unwrap_or_default();
        // The marker text does not encode how many prompts are hidden, so derive
        // it from the session render info when a compacted window is in effect.
        // This keeps prompt numbering absolute (the first visible prompt keeps
        // its real turn number).
        if lazy.remaining_messages > 0 || lazy.total_messages > 0 {
            let visible = if lazy.remaining_messages == 0 {
                usize::MAX
            } else {
                lazy.visible_messages
            };
            let (_, _, compacted_info) =
                crate::session::render_messages_and_images_with_compacted_history(
                    &self.session,
                    visible,
                );
            if let Some(info) = compacted_info {
                lazy.hidden_user_prompts = info.hidden_user_prompts;
            }
        }
        self.compacted_history_lazy = lazy;
    }

    fn apply_local_compacted_history_window(&mut self, visible_messages: usize) {
        let (rendered_messages, images, compacted_info) =
            crate::session::render_messages_and_images_with_compacted_history(
                &self.session,
                visible_messages,
            );
        let Some(compacted_info) = compacted_info else {
            return;
        };
        let display_messages = rendered_messages
            .into_iter()
            .map(|msg| DisplayMessage {
                role: msg.role,
                content: msg.content,
                tool_calls: msg.tool_calls,
                duration_secs: None,
                title: None,
                tool_data: msg.tool_data,
            })
            .collect();
        self.apply_compacted_history_window(
            display_messages,
            images,
            compacted_info.total_messages,
            compacted_info.visible_messages,
            compacted_info.remaining_messages,
            compacted_info.hidden_user_prompts,
        );
    }
}

fn parse_compacted_history_marker(message: &DisplayMessage) -> Option<CompactedHistoryLazyState> {
    if message.role != "system" {
        return None;
    }
    let rest = message
        .content
        .strip_prefix(COMPACTED_HISTORY_MARKER_PREFIX)?;

    if let Some(rest) = rest.strip_prefix("showing all ") {
        let (total, _) = parse_leading_usize(rest)?;
        return Some(CompactedHistoryLazyState {
            total_messages: total,
            visible_messages: total,
            remaining_messages: 0,
            hidden_user_prompts: 0,
            pending_request_visible: None,
        });
    }

    let (first, after_first) = parse_leading_usize(rest)?;
    if after_first.starts_with(" older historical messages hidden. Showing ") {
        let showing = after_first.strip_prefix(" older historical messages hidden. Showing ")?;
        let (visible, after_visible) = parse_leading_usize(showing)?;
        let after_visible = after_visible.strip_prefix(" of ")?;
        let (total, _) = parse_leading_usize(after_visible)?;
        return Some(CompactedHistoryLazyState {
            total_messages: total,
            visible_messages: visible,
            remaining_messages: first,
            hidden_user_prompts: 0,
            pending_request_visible: None,
        });
    }

    if after_first.starts_with(" historical messages hidden") {
        return Some(CompactedHistoryLazyState {
            total_messages: first,
            visible_messages: 0,
            remaining_messages: first,
            hidden_user_prompts: 0,
            pending_request_visible: None,
        });
    }

    None
}

fn parse_leading_usize(text: &str) -> Option<(usize, &str)> {
    let end = text
        .char_indices()
        .take_while(|(_, ch)| ch.is_ascii_digit())
        .map(|(idx, ch)| idx + ch.len_utf8())
        .last()?;
    let value = text[..end].parse().ok()?;
    Some((value, &text[end..]))
}
