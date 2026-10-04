use super::App;
use crate::side_panel::SidePanelPage;
use crate::todo::TaskItem;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

pub(super) const TODOS_VIEW_PAGE_ID: &str = "session_todos";
const TODOS_VIEW_TITLE: &str = "Todos";
/// Display-message role used by the inline chat todo card.
const TODO_CARD_ROLE: &str = "todos";

/// State behind the three todos surfaces: the side-panel page, the inline chat
/// card, and the pinned band. One home, so the refresh/pin/card logic reads one
/// struct instead of nine loose `todos_view_*` fields on `App`.
#[derive(Default)]
pub(super) struct TodosView {
    pub(super) enabled: bool,
    pub(super) markdown: String,
    pub(super) updated_at_ms: u64,
    pub(super) rendered_hash: u64,
    /// Hash of the payload rendered into the inline chat todo card, so the card
    /// stays live-updating while it sits in the transcript.
    pub(super) card_rendered_hash: u64,
    /// User-expanded state for the pinned band's `+N more` row.
    pub(super) pinned_expanded: bool,
}

impl TodosView {
    pub(super) fn enabled(&self) -> bool {
        self.enabled
    }

    fn clear_cache(&mut self) {
        self.markdown.clear();
        self.markdown.shrink_to_fit();
        self.updated_at_ms = now_ms();
        self.rendered_hash = 0;
    }

    pub(super) fn page(&self) -> SidePanelPage {
        SidePanelPage::ephemeral_markdown(
            TODOS_VIEW_PAGE_ID,
            TODOS_VIEW_TITLE,
            "todos://current-session",
            if self.markdown.trim().is_empty() {
                todos_view_placeholder_markdown()
            } else {
                self.markdown.clone()
            },
            self.updated_at_ms.max(1),
        )
    }
}

impl App {
    fn latest_todo_card_index(&self) -> Option<usize> {
        self.transcript
            .messages()
            .iter()
            .rposition(|message| message.role == TODO_CARD_ROLE)
    }

    /// Show the session todo list as an inline card in the chat transcript, or
    /// dismiss it when the card is already the trailing message.
    pub(super) fn toggle_todo_card(&mut self) {
        if let Some(idx) = self.latest_todo_card_index()
            && idx + 1 == self.transcript.messages().len()
        {
            self.remove_display_message(idx);
            self.todos_view.card_rendered_hash = 0;
            self.set_status_notice("Todos card dismissed");
            return;
        }
        self.show_todo_card();
    }

    /// Push (or move to the bottom) the inline todo card with fresh data. The
    /// transcript keeps at most one card so repeated toggles don't stack.
    pub(super) fn show_todo_card(&mut self) {
        if crate::tui::is_ssh_remote() {
            self.set_status_notice(
                "Local todo view unavailable for SSH; ask the remote agent for todos",
            );
            return;
        }
        let session_id = self.active_client_session_id().map(str::to_string);
        let todos = load_current_session_todos(
            self.session
                .working_dir
                .as_deref()
                .map(std::path::Path::new),
            session_id.as_deref(),
        );
        let content = todo_card_payload_json(&todos);
        self.todos_view.card_rendered_hash = hash_todos_payload(session_id.as_deref(), &todos);

        if let Some(idx) = self.latest_todo_card_index() {
            if idx + 1 == self.transcript.messages().len() {
                self.replace_display_message_content(idx, content);
                return;
            }
            self.remove_display_message(idx);
        }
        self.push_display_message(crate::tui::DisplayMessage::todos(content));
        self.set_status_notice("Todos card");
    }

    /// Live-refresh the inline todo card when the session todo list changed.
    /// Returns true when the transcript was updated.
    pub(super) fn refresh_todo_card_if_needed(&mut self) -> bool {
        if crate::tui::is_ssh_remote() {
            return false;
        }
        let Some(idx) = self.latest_todo_card_index() else {
            return false;
        };
        let session_id = self.active_client_session_id().map(str::to_string);
        let todos = load_current_session_todos(
            self.session
                .working_dir
                .as_deref()
                .map(std::path::Path::new),
            session_id.as_deref(),
        );
        let next_hash = hash_todos_payload(session_id.as_deref(), &todos);
        if next_hash == self.todos_view.card_rendered_hash {
            return false;
        }
        self.todos_view.card_rendered_hash = next_hash;
        let content = todo_card_payload_json(&todos);
        self.replace_display_message_content(idx, content)
    }

    pub(super) fn set_todos_view_enabled(&mut self, enabled: bool, focus: bool) {
        self.todos_view.enabled = enabled;
        let page = if enabled {
            self.refresh_todos_view_cache(true);
            Some(self.todos_view.page())
        } else {
            self.todos_view.clear_cache();
            None
        };
        self.apply_mirror_page(TODOS_VIEW_PAGE_ID, page, focus);
    }

    pub(super) fn refresh_todos_view_if_needed(&mut self) -> bool {
        if !self.todos_view.enabled {
            return false;
        }
        let changed = self.refresh_todos_view_cache(false);
        if !changed {
            return false;
        }
        self.refresh_todos_view_page();
        true
    }

    fn refresh_todos_view_page(&mut self) {
        if !self.todos_view.enabled {
            return;
        }

        let focus_todos = self.side_panel.focused_page_id.as_deref() == Some(TODOS_VIEW_PAGE_ID);
        let snapshot = self.decorate_side_panel_with_page(
            self.snapshot_without_page(TODOS_VIEW_PAGE_ID),
            self.todos_view.page(),
            focus_todos,
        );
        self.apply_side_panel_snapshot(snapshot);
    }

    fn refresh_todos_view_cache(&mut self, force: bool) -> bool {
        if crate::tui::is_ssh_remote() {
            return false;
        }
        let session_id = self.active_client_session_id();
        let todos = load_current_session_todos(
            self.session
                .working_dir
                .as_deref()
                .map(std::path::Path::new),
            session_id,
        );
        let next_hash = hash_todos_payload(session_id, &todos);
        if !force && self.todos_view.rendered_hash == next_hash {
            return false;
        }

        self.todos_view.markdown = build_todos_view_markdown(session_id, &todos);
        self.todos_view.updated_at_ms = now_ms();
        self.todos_view.rendered_hash = next_hash;
        true
    }
}

pub(super) fn todos_view_status_message(app: &App) -> String {
    format!(
        "Todo card: shown inline in the chat with /todos or {}.\n\nTodo side-panel screen: {}\n\nPinned todo band: {}\n\nWhen the panel screen is enabled (/todos panel), the side panel shows a transient Todos page dedicated to the current session's todo list and refreshes as the list changes. It is not persisted to session side-panel storage.\n\nWhen the pinned band is enabled (/todos pin), the full todo list stays pinned to the top of the chat transcript while it scrolls, like the previous-prompt preview.",
        crate::tui::keybind::todo_card_key_label(),
        if app.todos_view.enabled() {
            "enabled"
        } else {
            "disabled"
        },
        if crate::config::config().display.pin_todos {
            "enabled"
        } else {
            "disabled"
        }
    )
}

pub(super) fn handle_todos_view_command(app: &mut App, trimmed: &str) -> bool {
    let arg = if trimmed == "/todo" {
        ""
    } else if let Some(rest) = trimmed.strip_prefix("/todos") {
        rest.trim()
    } else {
        return false;
    };

    match arg {
        // Default: show the todo list as an inline chat card (toggles off when
        // the card is already the latest message).
        "" | "card" => {
            app.toggle_todo_card();
        }
        // Legacy side-panel screen, now behind an explicit subcommand.
        "panel" => {
            let enabled = !app.todos_view.enabled();
            app.set_todos_view_enabled(enabled, true);
            if enabled {
                app.set_status_notice("Todos panel: ON");
                app.push_display_message(crate::tui::DisplayMessage::system(
                    "Todo screen enabled. The side panel now shows only this session's todo list."
                        .to_string(),
                ));
            } else {
                app.set_status_notice("Todos panel: OFF");
                app.push_display_message(crate::tui::DisplayMessage::system(
                    "Todo screen disabled.".to_string(),
                ));
            }
        }
        "on" | "panel on" => {
            app.set_todos_view_enabled(true, true);
            app.set_status_notice("Todos panel: ON");
            app.push_display_message(crate::tui::DisplayMessage::system(
                "Todo screen enabled. The side panel now shows only this session's todo list."
                    .to_string(),
            ));
        }
        "off" | "panel off" => {
            app.set_todos_view_enabled(false, false);
            app.set_status_notice("Todos panel: OFF");
            app.push_display_message(crate::tui::DisplayMessage::system(
                "Todo screen disabled.".to_string(),
            ));
        }
        "status" => {
            app.push_display_message(crate::tui::DisplayMessage::system(
                todos_view_status_message(app),
            ));
        }
        _ => {
            app.push_display_message(crate::tui::DisplayMessage::error(
                "Usage: /todos [card|panel|on|off|status]".to_string(),
            ));
        }
    }

    true
}

fn load_current_session_todos(
    working_dir: Option<&std::path::Path>,
    session_id: Option<&str>,
) -> Vec<TaskItem> {
    let Some(session_id) = session_id else {
        return Vec::new();
    };
    crate::todo::load_tasks(working_dir, session_id).unwrap_or_default()
}

fn todo_card_payload_json(todos: &[TaskItem]) -> String {
    serde_json::to_string(&serde_json::json!({ "todos": todos }))
        .unwrap_or_else(|_| r#"{"todos":[]}"#.to_string())
}

fn build_todos_view_markdown(_session_id: Option<&str>, todos: &[TaskItem]) -> String {
    if todos.is_empty() {
        return "# Todos\n\nNo todos saved yet for this session.\n".to_string();
    }

    let total = todos.len();
    let completed = todos
        .iter()
        .filter(|todo| todo.status == "completed")
        .count();
    let in_progress = todos
        .iter()
        .filter(|todo| todo.status == "in_progress")
        .count();
    let pending = todos.iter().filter(|todo| todo.status == "pending").count();
    let cancelled = todos
        .iter()
        .filter(|todo| todo.status == "cancelled")
        .count();
    let blocked = todos
        .iter()
        .filter(|todo| todo.status != "completed" && !todo.blocked_by.is_empty())
        .count();
    let percent = ((completed as f64 / total as f64) * 100.0).round() as u64;
    let mut markdown = format!(
        "# Todos\n\n**{}/{} completed** ({}%) · {} doing · {} pending · {} blocked{}\n",
        completed,
        total,
        percent,
        in_progress,
        pending,
        blocked,
        if cancelled > 0 {
            format!(" · {cancelled} cancelled")
        } else {
            String::new()
        },
    );

    let sections = [
        ("in_progress", "In progress"),
        ("pending", "Pending"),
        ("completed", "Completed"),
        ("cancelled", "Cancelled"),
    ];
    const MAX_VISIBLE_TODOS: usize = 8;
    let mut visible = 0;
    for (status, heading) in sections {
        let items = sorted_todos_for_status(todos, status);
        if items.is_empty() {
            continue;
        }
        markdown.push_str(&format!("\n## {}\n\n", heading));
        for todo in items {
            if visible == MAX_VISIBLE_TODOS {
                break;
            }
            markdown.push_str(&format_todo_compact_markdown(todo));
            visible += 1;
        }
        if visible == MAX_VISIBLE_TODOS {
            break;
        }
    }

    let hidden = total.saturating_sub(visible);
    if hidden > 0 {
        markdown.push_str(&format!("\n_… and {hidden} more._\n"));
    }
    markdown.push_str("\nUse `/todos` for full details.\n");

    markdown
}

fn format_todo_compact_markdown(todo: &TaskItem) -> String {
    let group = todo
        .group
        .as_deref()
        .map(str::trim)
        .filter(|group| !group.is_empty())
        .map(|group| format!(" _({group})_"))
        .unwrap_or_default();
    format!(
        "- {} {}{}\n",
        status_badge(&todo.status, !todo.blocked_by.is_empty()),
        todo.content,
        group
    )
}

fn sorted_todos_for_status<'a>(todos: &'a [TaskItem], status: &str) -> Vec<&'a TaskItem> {
    let mut items: Vec<&TaskItem> = todos.iter().filter(|todo| todo.status == status).collect();
    items.sort_by(|a, b| {
        priority_rank(&a.priority)
            .cmp(&priority_rank(&b.priority))
            .then_with(|| a.content.cmp(&b.content))
            .then_with(|| a.id.cmp(&b.id))
    });
    items
}

fn status_badge(status: &str, blocked: bool) -> &'static str {
    if blocked && status != "completed" {
        return "[blocked]";
    }
    match status {
        "completed" => "[done]",
        "in_progress" => "[doing]",
        "cancelled" => "[x]",
        _ => "[todo]",
    }
}

fn priority_rank(priority: &str) -> u8 {
    match priority {
        "high" => 0,
        "medium" => 1,
        _ => 2,
    }
}

fn hash_todos_payload(session_id: Option<&str>, todos: &[TaskItem]) -> u64 {
    let mut hasher = DefaultHasher::new();
    session_id.hash(&mut hasher);
    for todo in todos {
        todo.id.hash(&mut hasher);
        todo.content.hash(&mut hasher);
        todo.status.hash(&mut hasher);
        todo.priority.hash(&mut hasher);
        todo.group.hash(&mut hasher);
        todo.blocked_by.hash(&mut hasher);
    }
    hasher.finish()
}

fn todos_view_placeholder_markdown() -> String {
    "# Todos\n\nWaiting for a session todo list.\n".to_string()
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|dur| dur.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn todo(id: &str, content: &str, status: &str, priority: &str) -> TaskItem {
        TaskItem {
            id: id.to_string(),
            content: content.to_string(),
            status: status.to_string(),
            priority: priority.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn todos_view_markdown_keeps_status_but_omits_internal_metadata() {
        let todos = vec![
            todo(
                "todo-1",
                "Validate confidence side panel",
                "in_progress",
                "high",
            ),
            todo("todo-2", "Finish completed item", "completed", "medium"),
        ];

        let markdown = build_todos_view_markdown(Some("session_test"), &todos);

        assert!(markdown.contains("**1/2 completed** (50%) · 1 doing · 0 pending"));
        assert!(markdown.contains("- [doing] Validate confidence side panel"));
        assert!(markdown.contains("- [done] Finish completed item"));
        assert!(!markdown.contains("Session ID"));
        assert!(markdown.contains("Use `/todos` for full details."));
    }

    #[test]
    fn todos_view_markdown_is_bounded_and_keeps_group_context() {
        let mut grouped_a = todo("g1", "Cut frame allocs", "in_progress", "high");
        grouped_a.group = Some("optimize rendering".to_string());
        let mut grouped_b = todo("g2", "Batch draw calls", "completed", "medium");
        grouped_b.group = Some("optimize rendering".to_string());
        let mut other = todo("o1", "Fix scrollback", "pending", "low");
        other.group = Some("scrollback".to_string());
        let ungrouped = todo("u1", "Misc cleanup", "pending", "low");

        let mut todos = vec![grouped_a, grouped_b, other, ungrouped];
        for index in 0..8 {
            todos.push(todo(
                &format!("extra-{index}"),
                &format!("Extra item {index}"),
                "pending",
                "low",
            ));
        }
        let markdown = build_todos_view_markdown(Some("session_test"), &todos);

        assert!(markdown.contains("_(optimize rendering)_"), "{markdown}");
        assert!(!markdown.contains("Feedback loop:"), "{markdown}");
        assert!(markdown.contains("_… and 4 more._"), "{markdown}");
        assert_eq!(markdown.matches("- [").count(), 8, "{markdown}");
    }

    #[test]
    fn todos_view_hash_changes_when_group_changes() {
        let mut todos = vec![todo("g", "Group hash", "pending", "high")];
        let before = hash_todos_payload(Some("session_test"), &todos);
        todos[0].group = Some("rendering".to_string());
        let after = hash_todos_payload(Some("session_test"), &todos);
        assert_ne!(before, after);
    }
}
