//! Render the one-line summary under a `todo` tool call saying what it did.
//!
//! The call names what happened (`add`, `update`, `close`), so nothing has to be
//! diffed against the transcript. A `close` names only an id, so its words come
//! from the previous list, which is where the row still was.

use super::*;
use crate::message::ToolCall;
use crate::todo::TaskItem;
use kcode_tui_style::theme::{pending_color, success_color, tool_color};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use serde_json::Value;

/// Find the todo list as it stood before `current_abs_idx`, for a `close` whose
/// call carries only an id.
pub(super) fn previous_todos(
    messages: &[DisplayMessage],
    current_abs_idx: usize,
) -> Option<Vec<TaskItem>> {
    let end = current_abs_idx.min(messages.len());
    messages[..end]
        .iter()
        .rev()
        .filter(|msg| msg.effective_role() == "tool")
        .filter(|msg| {
            msg.tool_data
                .as_ref()
                .is_some_and(|tc| tools_ui::canonical_tool_name(&tc.name) == "todo")
        })
        .find_map(|msg| serde_json::from_str::<Vec<TaskItem>>(&msg.content).ok())
}

pub(super) fn render_todo_change_lines(
    prev: Option<&[TaskItem]>,
    tc: &ToolCall,
    width: u16,
) -> Vec<Line<'static>> {
    let Some((label, label_color, text)) = action_line(prev, tc) else {
        return Vec::new();
    };
    let spans = vec![
        Span::styled("  ↳ ", Style::default().fg(dim_color())),
        Span::styled(format!("{label}: "), Style::default().fg(label_color)),
        Span::styled(text, Style::default().fg(tool_color())),
    ];
    vec![super::truncate_line_with_ellipsis_to_width(
        &Line::from(spans),
        width as usize,
    )]
}

fn action_line(prev: Option<&[TaskItem]>, tc: &ToolCall) -> Option<(&'static str, Color, String)> {
    let input = &tc.input;
    let field = |name: &str| input.get(name).and_then(Value::as_str);
    let id = field("id").unwrap_or_default().to_string();
    match field("action")? {
        "add" => Some(("added", pending_color(), field("content")?.to_string())),
        "update" => Some((
            "updated",
            tool_color(),
            field("content").map(str::to_string).unwrap_or(id),
        )),
        "close" => {
            let words = prev
                .and_then(|rows| rows.iter().find(|row| row.id == id))
                .map(|row| row.content.clone())
                .unwrap_or(id);
            Some(("closed", success_color(), words))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn call(input: Value) -> ToolCall {
        ToolCall {
            id: "call".to_string(),
            name: "todo".to_string(),
            input,
            ..Default::default()
        }
    }

    fn row(id: &str, content: &str) -> TaskItem {
        TaskItem {
            id: id.to_string(),
            content: content.to_string(),
            ..Default::default()
        }
    }

    fn text(lines: &[Line<'static>]) -> String {
        lines
            .iter()
            .flat_map(|line| line.spans.iter())
            .map(|span| span.content.as_ref())
            .collect()
    }

    #[test]
    fn add_shows_the_new_task() {
        let lines = render_todo_change_lines(
            None,
            &call(json!({"action": "add", "content": "write the store"})),
            80,
        );
        assert_eq!(lines.len(), 1);
        assert!(
            text(&lines).contains("added: write the store"),
            "{:?}",
            text(&lines)
        );
    }

    /// The call carries only the id, so the words must come from the old list.
    #[test]
    fn close_shows_the_words_of_the_task_it_closed() {
        let prev = vec![row("t1", "write the store")];
        let lines = render_todo_change_lines(
            Some(&prev),
            &call(json!({"action": "close", "id": "t1", "result": "cargo test"})),
            80,
        );
        assert!(
            text(&lines).contains("closed: write the store"),
            "{:?}",
            text(&lines)
        );
    }

    #[test]
    fn reading_renders_nothing() {
        let lines = render_todo_change_lines(None, &call(json!({"action": "list"})), 80);
        assert!(lines.is_empty());
    }
}
