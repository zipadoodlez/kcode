use crate::storage;
use anyhow::Result;
use std::path::PathBuf;

pub use jcode_task_types::{TodoGoal, TodoItem};

/// Goals with a hill-climbability score strictly below this are considered
/// low: no credible metric to iterate against. The todo tool nudges the model
/// on every applicable write to reframe the objective into something
/// quantifiable and verifiable.
pub const LOW_HILL_CLIMBABILITY: u8 = 90;

/// Prefix of the synthetic "all todos done" confidence summary follow-up that
/// auto-poke queues once every todo is complete.
pub const TODO_CONFIDENCE_SUMMARY_PREFIX: &str = "All todos are done. Todo confidence summary:";

/// A completed todo is "spike-finished" when its confidence jumped at least
/// this many points in its final step. Benchmark analysis (TB2.1 k=5) showed
/// planning confidence correctly flags the riskiest step, but a bulk
/// end-of-task stamp to 100 erases that signal: every wrong 100%-confidence
/// completion ended in such a spike, while stepped, evidence-backed rises
/// were always right.
pub const TODO_CONFIDENCE_SPIKE: u8 = 15;

/// Completed todos whose confidence trail ends in an unearned jump: a final
/// step of [`TODO_CONFIDENCE_SPIKE`]+ points in the tool-maintained
/// `confidence_history`, or, for todos without a recorded trail, an equally
/// large gap between planning `confidence` and `completion_confidence`.
pub fn spike_completed_todos(todos: &[TodoItem]) -> Vec<&TodoItem> {
    todos
        .iter()
        .filter(|todo| todo.status == "completed")
        .filter(|todo| {
            let history = &todo.confidence_history;
            match history.len() {
                0 => {
                    todo.confidence
                        .zip(todo.completion_confidence)
                        .is_some_and(|(first, last)| {
                            last.saturating_sub(first) >= TODO_CONFIDENCE_SPIKE
                        })
                }
                1 => false,
                n => history[n - 1].saturating_sub(history[n - 2]) >= TODO_CONFIDENCE_SPIKE,
            }
        })
        .collect()
}

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

/// Goal-level assessments live beside the todo list in a separate file so the
/// todo list format (a bare `Vec<TodoItem>` array) stays readable by every
/// existing consumer.
pub fn load_goals(session_id: &str) -> Result<Vec<TodoGoal>> {
    let path = goals_path(session_id)?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    storage::read_json(&path).or_else(|_| Ok(Vec::new()))
}

/// Derive a concise session-title hint from the todo tool's persisted plan.
///
/// Todo groups are intended to name coherent goals, so the group containing the
/// current (or latest incomplete) item is the strongest signal. Ungrouped plans
/// fall back to their measurable objective, then the item text itself.
pub fn derive_session_title(todos: &[TodoItem], goals: &[TodoGoal]) -> Option<String> {
    fn non_empty(value: Option<&str>) -> Option<String> {
        value
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    }

    let current = todos
        .iter()
        .rev()
        .find(|todo| todo.status.eq_ignore_ascii_case("in_progress"))
        .or_else(|| {
            todos
                .iter()
                .rev()
                .find(|todo| !todo.status.eq_ignore_ascii_case("completed"))
        })
        .or_else(|| todos.last());

    if let Some(todo) = current {
        if let Some(group) = non_empty(todo.group.as_deref()) {
            return Some(group);
        }

        if let Some(objective) = goals
            .iter()
            .rev()
            .find(|goal| goal.group.is_none())
            .and_then(|goal| non_empty(goal.objective.as_deref()))
        {
            return Some(objective);
        }

        return non_empty(Some(&todo.content));
    }

    goals.iter().rev().find_map(|goal| {
        non_empty(goal.group.as_deref()).or_else(|| non_empty(goal.objective.as_deref()))
    })
}

/// Load todo state for a session and derive its best title hint.
pub fn load_session_title(session_id: &str) -> Option<String> {
    let todos = load_todos(session_id).ok()?;
    let goals = load_goals(session_id).unwrap_or_default();
    derive_session_title(&todos, &goals)
}

pub fn save_goals(session_id: &str, goals: &[TodoGoal]) -> Result<()> {
    let path = goals_path(session_id)?;
    storage::write_json_fast(&path, goals)
}

fn goals_path(session_id: &str) -> Result<PathBuf> {
    let base = storage::jcode_dir()?;
    Ok(base
        .join("todos")
        .join(format!("{}-goals.json", session_id)))
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

    fn todo(content: &str, status: &str, group: Option<&str>) -> TodoItem {
        TodoItem {
            content: content.to_string(),
            status: status.to_string(),
            priority: "high".to_string(),
            id: content.to_ascii_lowercase().replace(' ', "-"),
            group: group.map(str::to_string),
            confidence: None,
            completion_confidence: None,
            confidence_history: Vec::new(),
            blocked_by: Vec::new(),
            assigned_to: None,
        }
    }

    #[test]
    fn session_title_prefers_in_progress_todo_group() {
        let todos = vec![
            todo("old task", "pending", Some("Older goal")),
            todo("current task", "in_progress", Some("Fix resume names")),
            todo("later task", "pending", Some("Later goal")),
        ];

        assert_eq!(
            derive_session_title(&todos, &[]).as_deref(),
            Some("Fix resume names")
        );
    }

    #[test]
    fn session_title_uses_latest_incomplete_group_when_nothing_is_active() {
        let todos = vec![
            todo("finished", "completed", Some("Old goal")),
            todo("next", "pending", Some("Current goal")),
        ];

        assert_eq!(
            derive_session_title(&todos, &[]).as_deref(),
            Some("Current goal")
        );
    }

    #[test]
    fn ungrouped_session_title_prefers_goal_objective_then_item_content() {
        let todos = vec![todo("Run targeted tests", "in_progress", None)];
        let goals = vec![TodoGoal {
            group: None,
            hill_climbability: Some(90),
            objective: Some("All resume naming tests pass".to_string()),
        }];

        assert_eq!(
            derive_session_title(&todos, &goals).as_deref(),
            Some("All resume naming tests pass")
        );
        assert_eq!(
            derive_session_title(&todos, &[]).as_deref(),
            Some("Run targeted tests")
        );
    }
}
