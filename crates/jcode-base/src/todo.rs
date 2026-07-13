use crate::storage;
use anyhow::Result;
use std::path::PathBuf;

pub use jcode_task_types::{TodoGoal, TodoItem};

/// Minimum passing score for 0-100 quality assessments. Scores below this do
/// not provide enough evidence to clear their respective quality gate.
pub const QUALITY_GATE_THRESHOLD: u8 = 96;

/// Goals with a hill-climbability score strictly below this are considered
/// low: no credible metric to iterate against. The todo tool nudges the model
/// on every applicable write to reframe the objective into something
/// quantifiable and verifiable.
pub const LOW_HILL_CLIMBABILITY: u8 = QUALITY_GATE_THRESHOLD;

/// Model-facing continuation for the private hill-climbability check. Names the
/// assessment category without disclosing the score or threshold.
pub const TODO_HILL_CLIMBABILITY_CONTINUATION_MESSAGE: &str = "Your hill-climbability is not high enough. Think more about how to make this goal hill-climbable. The goal is to create a strong feedback loop you can iterate against.";

/// Model-facing continuation for the private end-to-end ownership check. Names
/// the assessment category without disclosing the score or threshold.
pub const TODO_OWNERSHIP_CONTINUATION_MESSAGE: &str = "Your end-to-end ownership is not high enough to complete this goal. Take ownership of the full user outcome, not just the immediate implementation. Follow the work through every relevant integration and runtime path, resolve consequential gaps, validate the complete workflow, and finish the necessary follow-through.";

/// Model-facing continuation for private completion-confidence checks. Names
/// the assessment category without disclosing scores, items, or thresholds.
pub const TODO_COMPLETION_CONTINUATION_MESSAGE: &str = "Your completion confidence is missing or not high enough. Validate the completed result more thoroughly, address any remaining issues, and then reassess whether the work is ready to finalize.";
const LEGACY_TODO_CONFIDENCE_SUMMARY_PREFIX: &str = "All todos are done. Todo confidence summary:";

fn normalized_group(group: Option<&str>) -> Option<String> {
    group
        .map(str::trim)
        .filter(|group| !group.is_empty())
        .map(str::to_string)
}

fn group_is_complete(todos: &[TodoItem], group: &Option<String>) -> bool {
    let mut matching = todos
        .iter()
        .filter(|todo| normalized_group(todo.group.as_deref()) == *group)
        .peekable();
    matching.peek().is_some() && matching.all(|todo| todo.status == "completed")
}

/// Whether every group newly closed by this update has a sufficient assessment
/// of ownership over its full outcome. Groups completed before this check was
/// introduced are intentionally grandfathered so existing sessions stay writable.
pub fn newly_completed_groups_have_sufficient_ownership(
    previous: &[TodoItem],
    incoming: &[TodoItem],
    goals: &[TodoGoal],
) -> bool {
    let mut groups: Vec<Option<String>> = Vec::new();
    for todo in incoming {
        let group = normalized_group(todo.group.as_deref());
        if !groups.contains(&group) {
            groups.push(group);
        }
    }

    groups.into_iter().all(|group| {
        if !group_is_complete(incoming, &group) || group_is_complete(previous, &group) {
            return true;
        }
        goals
            .iter()
            .find(|goal| normalized_group(goal.group.as_deref()) == group)
            .and_then(|goal| goal.end_to_end_ownership)
            .is_some_and(|score| score >= QUALITY_GATE_THRESHOLD)
    })
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
        || trimmed.starts_with(TODO_HILL_CLIMBABILITY_CONTINUATION_MESSAGE)
        || trimmed.starts_with(TODO_OWNERSHIP_CONTINUATION_MESSAGE)
        || trimmed.starts_with(TODO_COMPLETION_CONTINUATION_MESSAGE)
        || trimmed.starts_with(LEGACY_TODO_CONFIDENCE_SUMMARY_PREFIX)
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
        assert!(is_auto_poke_message(
            TODO_HILL_CLIMBABILITY_CONTINUATION_MESSAGE
        ));
        assert!(is_auto_poke_message(TODO_OWNERSHIP_CONTINUATION_MESSAGE));
        assert!(is_auto_poke_message(TODO_COMPLETION_CONTINUATION_MESSAGE));
        assert!(is_auto_poke_message(LEGACY_TODO_CONFIDENCE_SUMMARY_PREFIX));
    }

    #[test]
    fn quality_continuations_are_actionable_without_private_calibration() {
        for (message, category) in [
            (
                TODO_HILL_CLIMBABILITY_CONTINUATION_MESSAGE,
                "hill-climbability",
            ),
            (TODO_OWNERSHIP_CONTINUATION_MESSAGE, "end-to-end ownership"),
            (
                TODO_COMPLETION_CONTINUATION_MESSAGE,
                "completion confidence",
            ),
        ] {
            let lower = message.to_ascii_lowercase();
            assert!(lower.contains(category));
            assert!(!message.chars().any(|ch| ch.is_ascii_digit()));
            for disclosure in ["threshold", "score", "percent", "below", "quality gate"] {
                assert!(
                    !lower.contains(disclosure),
                    "category-only continuation disclosed {disclosure}: {message}"
                );
            }
        }

        assert!(TODO_HILL_CLIMBABILITY_CONTINUATION_MESSAGE.contains("strong feedback loop"));
        assert!(TODO_OWNERSHIP_CONTINUATION_MESSAGE.contains("full user outcome"));
        assert!(TODO_OWNERSHIP_CONTINUATION_MESSAGE.contains("complete workflow"));
        assert!(TODO_OWNERSHIP_CONTINUATION_MESSAGE.contains("necessary follow-through"));
        assert!(TODO_COMPLETION_CONTINUATION_MESSAGE.contains("Validate the completed result"));
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

    fn ownership_goal(group: Option<&str>, ownership: Option<u8>) -> TodoGoal {
        TodoGoal {
            group: group.map(str::to_string),
            end_to_end_ownership: ownership,
            ..Default::default()
        }
    }

    #[test]
    fn newly_completed_group_requires_sufficient_end_to_end_ownership() {
        let previous = vec![todo("work", "in_progress", Some("ship"))];
        let completed = vec![todo("work", "completed", Some("ship"))];

        for ownership in [None, Some(0), Some(95)] {
            assert!(!newly_completed_groups_have_sufficient_ownership(
                &previous,
                &completed,
                &[ownership_goal(Some("ship"), ownership)],
            ));
        }
        assert!(newly_completed_groups_have_sufficient_ownership(
            &previous,
            &completed,
            &[ownership_goal(Some("ship"), Some(96))],
        ));
    }

    #[test]
    fn ownership_is_not_required_before_group_completion() {
        let previous = vec![todo("work", "pending", Some("ship"))];
        let in_progress = vec![todo("work", "in_progress", Some("ship"))];

        assert!(newly_completed_groups_have_sufficient_ownership(
            &previous,
            &in_progress,
            &[],
        ));
    }

    #[test]
    fn ownership_gate_normalizes_groups_and_supports_ungrouped_work() {
        let previous = vec![todo("work", "in_progress", Some(" ship "))];
        let completed = vec![todo("work", "completed", Some("ship"))];
        assert!(newly_completed_groups_have_sufficient_ownership(
            &previous,
            &completed,
            &[ownership_goal(Some(" ship"), Some(96))],
        ));

        let previous = vec![todo("work", "in_progress", None)];
        let completed = vec![todo("work", "completed", None)];
        assert!(newly_completed_groups_have_sufficient_ownership(
            &previous,
            &completed,
            &[ownership_goal(None, Some(96))],
        ));
    }

    #[test]
    fn ownership_gate_grandfathers_preexisting_completed_groups() {
        let completed = vec![todo("legacy", "completed", Some("legacy"))];
        assert!(newly_completed_groups_have_sufficient_ownership(
            &completed,
            &completed,
            &[],
        ));
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
            ..Default::default()
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
