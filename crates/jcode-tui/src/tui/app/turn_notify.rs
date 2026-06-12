//! Desktop notification for completed long agent turns.
//!
//! When a turn finishes after a configurable duration (lower threshold when
//! the session has todos, since those indicate task-style work), the user gets
//! a compact desktop notification: session name + duration in the title, todo
//! progress and a short snippet of the final assistant text in the body. By
//! default it fires only while the terminal window is unfocused.

use super::App;
use crate::todo::TodoItem;

/// Maximum characters of assistant text shown in the notification body.
/// Notification banners truncate aggressively; keep the payload tight.
const SNIPPET_MAX_CHARS: usize = 120;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TurnNotification {
    pub title: String,
    pub body: String,
}

impl App {
    /// Send a desktop notification for a just-completed turn when warranted.
    /// Call at turn completion, after the final assistant message is committed.
    pub(super) fn maybe_notify_turn_complete(&self, duration_secs: Option<f32>) {
        if !self.runtime_mode_allows_turn_notifications() {
            return;
        }
        let cfg = &crate::config::config().notifications;
        if !cfg.turn_complete {
            return;
        }
        if cfg.turn_complete_only_when_unfocused && self.client_focused() {
            return;
        }
        let Some(duration) = duration_secs else {
            return;
        };

        let todos = self
            .active_client_session_id()
            .map(load_session_todos)
            .unwrap_or_default();
        let threshold = if todos.is_empty() {
            cfg.turn_complete_min_secs
        } else {
            cfg.turn_complete_todo_min_secs.min(cfg.turn_complete_min_secs)
        };
        if (duration as u64) < threshold.max(1) {
            return;
        }

        let notification = build_turn_notification(
            self.active_client_session_id()
                .and_then(crate::id::extract_session_name),
            duration,
            &todos,
            self.last_assistant_text_for_notification().as_deref(),
        );
        crate::notifications::send_desktop_notification(&notification.title, &notification.body);
    }

    fn runtime_mode_allows_turn_notifications(&self) -> bool {
        matches!(self.runtime_mode(), super::AppRuntimeMode::RemoteClient) && !self.is_replay
    }

    /// Final assistant text of the turn, used for the notification snippet.
    fn last_assistant_text_for_notification(&self) -> Option<String> {
        self.display_messages
            .iter()
            .rev()
            .find(|m| m.role == "assistant" && !m.content.trim().is_empty())
            .map(|m| m.content.clone())
    }
}

fn load_session_todos(session_id: &str) -> Vec<TodoItem> {
    crate::todo::load_todos(session_id).unwrap_or_default()
}

/// Build the compact notification. Kept free of `App` for testability.
pub(super) fn build_turn_notification(
    session_name: Option<&str>,
    duration_secs: f32,
    todos: &[TodoItem],
    last_assistant_text: Option<&str>,
) -> TurnNotification {
    let mut title = String::from("jcode");
    if let Some(name) = session_name {
        title.push_str(" · ");
        title.push_str(name);
    }
    title.push_str(" · done in ");
    title.push_str(&format_duration_compact(duration_secs));

    let mut body = String::new();
    if let Some(progress) = todo_progress_line(todos) {
        body.push_str(&progress);
    }
    if let Some(snippet) = last_assistant_text.map(summary_snippet).filter(|s| !s.is_empty()) {
        if !body.is_empty() {
            body.push_str(" — ");
        }
        body.push_str(&snippet);
    }
    if body.is_empty() {
        body.push_str("Turn finished");
    }

    TurnNotification { title, body }
}

/// "3/5 todos" plus "· 1 blocked" when relevant; None when no todos exist.
fn todo_progress_line(todos: &[TodoItem]) -> Option<String> {
    if todos.is_empty() {
        return None;
    }
    let total = todos.len();
    let completed = todos.iter().filter(|t| t.status == "completed").count();
    let blocked = todos
        .iter()
        .filter(|t| t.status != "completed" && !t.blocked_by.is_empty())
        .count();
    let mut line = if completed == total {
        format!("✓ all {} todos", total)
    } else {
        format!("{}/{} todos", completed, total)
    };
    if blocked > 0 {
        line.push_str(&format!(" · {} blocked", blocked));
    }
    Some(line)
}

/// First meaningful line of the assistant text, markdown-stripped and clipped.
fn summary_snippet(text: &str) -> String {
    let line = text
        .lines()
        .map(str::trim)
        .find(|l| {
            !l.is_empty()
                && !l.starts_with("```")
                && !l.starts_with('|')
                && !l.starts_with("---")
        })
        .unwrap_or("");
    let cleaned = strip_markdown_inline(line);
    truncate_chars(cleaned.trim(), SNIPPET_MAX_CHARS)
}

fn strip_markdown_inline(line: &str) -> String {
    let line = line.trim_start_matches('#').trim_start();
    // List/quote markers.
    let line = line
        .strip_prefix("- ")
        .or_else(|| line.strip_prefix("* "))
        .or_else(|| line.strip_prefix("> "))
        .unwrap_or(line);
    line.replace("**", "").replace('`', "")
}

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

fn format_duration_compact(secs: f32) -> String {
    let secs = secs.max(0.0) as u64;
    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        let m = secs / 60;
        let s = secs % 60;
        if s == 0 {
            format!("{}m", m)
        } else {
            format!("{}m {}s", m, s)
        }
    } else {
        let h = secs / 3600;
        let m = (secs % 3600) / 60;
        if m == 0 {
            format!("{}h", h)
        } else {
            format!("{}h {}m", h, m)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn todo(status: &str, blocked: bool) -> TodoItem {
        TodoItem {
            content: "x".to_string(),
            status: status.to_string(),
            priority: "medium".to_string(),
            id: "t".to_string(),
            group: None,
            confidence: None,
            completion_confidence: None,
            blocked_by: if blocked {
                vec!["other".to_string()]
            } else {
                Vec::new()
            },
            assigned_to: None,
        }
    }

    #[test]
    fn title_includes_session_and_compact_duration() {
        let n = build_turn_notification(Some("fox"), 754.0, &[], Some("All done."));
        assert_eq!(n.title, "jcode · fox · done in 12m 34s");
        assert_eq!(n.body, "All done.");
    }

    #[test]
    fn body_combines_todo_progress_and_snippet() {
        let todos = vec![todo("completed", false), todo("pending", false)];
        let n = build_turn_notification(None, 200.0, &todos, Some("Fixed the parser bug."));
        assert_eq!(n.title, "jcode · done in 3m 20s");
        assert_eq!(n.body, "1/2 todos — Fixed the parser bug.");
    }

    #[test]
    fn body_celebrates_all_todos_complete_and_counts_blocked() {
        let done = vec![todo("completed", false), todo("completed", false)];
        let n = build_turn_notification(None, 200.0, &done, None);
        assert_eq!(n.body, "✓ all 2 todos");

        let blocked = vec![todo("completed", false), todo("pending", true)];
        let n = build_turn_notification(None, 200.0, &blocked, None);
        assert_eq!(n.body, "1/2 todos · 1 blocked");
    }

    #[test]
    fn snippet_skips_markdown_noise_and_truncates() {
        let text = "```rust\ncode\n```\n\n## **Results** are `good`\nmore detail";
        assert_eq!(summary_snippet(text), "code");

        let text = "\n\n- **Fixed** the `frobnicator`\nrest";
        assert_eq!(summary_snippet(text), "Fixed the frobnicator");

        let long = "a".repeat(300);
        let s = summary_snippet(&long);
        assert_eq!(s.chars().count(), SNIPPET_MAX_CHARS);
        assert!(s.ends_with('…'));
    }

    #[test]
    fn empty_inputs_fall_back_to_minimal_body() {
        let n = build_turn_notification(None, 65.0, &[], None);
        assert_eq!(n.title, "jcode · done in 1m 5s");
        assert_eq!(n.body, "Turn finished");
    }

    #[test]
    fn duration_formats_hours() {
        assert_eq!(format_duration_compact(59.0), "59s");
        assert_eq!(format_duration_compact(60.0), "1m");
        assert_eq!(format_duration_compact(3600.0), "1h");
        assert_eq!(format_duration_compact(3725.0), "1h 2m");
    }
}
