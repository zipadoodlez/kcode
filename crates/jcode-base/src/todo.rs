use crate::storage;
use anyhow::Result;
use std::path::PathBuf;

pub use jcode_task_types::TodoItem;

/// Prefix of the synthetic "all todos done" confidence summary follow-up that
/// auto-poke queues once every todo is complete.
pub const TODO_CONFIDENCE_SUMMARY_PREFIX: &str = "All todos are done. Todo confidence summary:";

/// Build the synthetic auto-poke continuation prompt sent when the model
/// stops with incomplete todos. Kept here so every producer (TUI auto-poke,
/// `jcode run` auto-poke) and the transcript renderer agree on the exact text.
pub fn build_auto_poke_message(incomplete_count: usize) -> String {
    format!(
        "You have {} incomplete todo{}. Continue working, or update the todo tool.",
        incomplete_count,
        if incomplete_count == 1 { "" } else { "s" },
    )
}

/// True when `message` is a synthetic auto-poke continuation (the
/// incomplete-todos poke or the todo confidence summary) rather than a real
/// user prompt.
///
/// These are persisted as `Role::User` so the model treats them as a normal
/// continuation turn, but they are not something the user typed. The live UI
/// hides them (showing an "Auto-poking..." notice instead), and the session
/// renderer uses this to avoid re-rendering them as user prompts on
/// reload/resume/remote attach.
pub fn is_auto_poke_message(message: &str) -> bool {
    let trimmed = message.trim();
    (trimmed.starts_with("You have ")
        && trimmed.contains(" incomplete todo")
        && trimmed.ends_with("update the todo tool."))
        || trimmed.starts_with(TODO_CONFIDENCE_SUMMARY_PREFIX)
}

pub fn load_todos(session_id: &str) -> Result<Vec<TodoItem>> {
    let path = todo_path(session_id)?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    storage::read_json(&path).or_else(|_| Ok(Vec::new()))
}

pub fn todos_exist(session_id: &str) -> Result<bool> {
    Ok(todo_path(session_id)?.exists())
}

pub fn save_todos(session_id: &str, todos: &[TodoItem]) -> Result<()> {
    let path = todo_path(session_id)?;
    storage::write_json_fast(&path, todos)
}

fn todo_path(session_id: &str) -> Result<PathBuf> {
    let base = storage::jcode_dir()?;
    Ok(base.join("todos").join(format!("{}.json", session_id)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_auto_poke_messages_are_detected() {
        assert!(is_auto_poke_message(&build_auto_poke_message(1)));
        assert!(is_auto_poke_message(&build_auto_poke_message(3)));
        assert!(is_auto_poke_message(&format!(
            "{} core work 95%",
            TODO_CONFIDENCE_SUMMARY_PREFIX
        )));
    }

    #[test]
    fn real_user_prompts_are_not_detected_as_pokes() {
        assert!(!is_auto_poke_message("fix the login bug"));
        assert!(!is_auto_poke_message(
            "You have 2 incomplete todos. Continue working, or update the todo tool.\n\nalso please fix the tests"
        ));
        assert!(!is_auto_poke_message(""));
    }
}
