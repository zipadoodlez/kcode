use crate::storage;
use anyhow::Result;
use std::path::PathBuf;

pub use kcode_task_types::{
    Autonomy, ConfidenceState, DeliveryState, Difficulty, FeedbackLoopCoverage,
    FeedbackLoopRelevance, FeedbackLoopState, FeedbackLoopTraceability, IntentUnderstanding,
    IterationMaturity, TodoGoal, TodoGoalChange, TodoGoalField, TodoItem, TodoPlan, TodoPlanChange,
    TodoPlanField,
};

/// Return the canonical todo status for model-written status vocabulary.
///
/// The todo tool historically accepted any string, so persisted sessions can
/// contain natural completion synonyms such as `done` or `finished`. Keep this
/// helper tolerant for those sessions even though new tool calls advertise a
/// constrained vocabulary.
pub fn canonical_todo_status(status: &str) -> Option<&'static str> {
    let status = status.trim();
    if status.eq_ignore_ascii_case("pending") {
        Some("pending")
    } else if status.eq_ignore_ascii_case("in_progress")
        || status.eq_ignore_ascii_case("in progress")
        || status.eq_ignore_ascii_case("in-progress")
    {
        Some("in_progress")
    } else if status.eq_ignore_ascii_case("completed")
        || status.eq_ignore_ascii_case("complete")
        || status.eq_ignore_ascii_case("done")
        || status.eq_ignore_ascii_case("finished")
    {
        Some("completed")
    } else if status.eq_ignore_ascii_case("cancelled") || status.eq_ignore_ascii_case("canceled") {
        Some("cancelled")
    } else {
        None
    }
}

pub fn todo_status_is_completed(status: &str) -> bool {
    canonical_todo_status(status) == Some("completed")
}

pub fn todo_status_is_cancelled(status: &str) -> bool {
    canonical_todo_status(status) == Some("cancelled")
}

/// Whether the plan's intent understanding is solid enough to work against.
pub fn intent_understanding_passes(state: Option<IntentUnderstanding>) -> bool {
    state.is_some_and(|state| state >= IntentUnderstanding::Clear)
}

/// Whether a goal's feedback loop reports back on the requirements by itself.
pub fn feedback_loop_passes(state: Option<FeedbackLoopState>) -> bool {
    state.is_some_and(|state| state >= FeedbackLoopState::Closed)
}

/// Minimum directness expected from a completion check. More involved goals
/// need checks aligned with acceptance behavior rather than a representative
/// proxy alone.
pub fn required_feedback_loop_relevance(difficulty: Option<Difficulty>) -> FeedbackLoopRelevance {
    if difficulty.is_some_and(|difficulty| difficulty >= Difficulty::Involved) {
        FeedbackLoopRelevance::AcceptanceAligned
    } else {
        FeedbackLoopRelevance::Representative
    }
}

/// Minimum breadth expected from a completion check. More involved goals must
/// include edge cases and integration boundaries as well as their main paths.
pub fn required_feedback_loop_coverage(difficulty: Option<Difficulty>) -> FeedbackLoopCoverage {
    if difficulty.is_some_and(|difficulty| difficulty >= Difficulty::Involved) {
        FeedbackLoopCoverage::EdgeAndIntegrationPaths
    } else {
        FeedbackLoopCoverage::MainPaths
    }
}

pub fn feedback_loop_relevance_passes(goal: &TodoGoal) -> bool {
    goal.feedback_loop_relevance
        .is_some_and(|state| state >= required_feedback_loop_relevance(goal.difficulty))
}

pub fn feedback_loop_coverage_passes(goal: &TodoGoal) -> bool {
    goal.feedback_loop_coverage
        .is_some_and(|state| state >= required_feedback_loop_coverage(goal.difficulty))
}

pub fn required_feedback_loop_traceability(
    difficulty: Option<Difficulty>,
) -> FeedbackLoopTraceability {
    if difficulty.is_some_and(|difficulty| difficulty >= Difficulty::Involved) {
        FeedbackLoopTraceability::Complete
    } else {
        FeedbackLoopTraceability::Partial
    }
}

pub fn feedback_loop_traceability_passes(goal: &TodoGoal) -> bool {
    goal.feedback_loop_traceability
        .is_some_and(|state| state >= required_feedback_loop_traceability(goal.difficulty))
}

/// Build the synthetic auto-poke continuation prompt sent when the model
/// stops with incomplete todos. Kept here so every producer (TUI auto-poke,
/// `kcode run` auto-poke) and the transcript renderer agree on the exact text.
pub fn build_auto_poke_message(incomplete_count: usize) -> String {
    format!(
        "You have {} incomplete todo{}. Continue working, or update the todo tool.",
        incomplete_count,
        if incomplete_count == 1 { "" } else { "s" },
    )
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
    storage::write_json_fast(&path, todos)?;
    Ok(())
}

fn todo_path(session_id: &str) -> Result<PathBuf> {
    let base = storage::kcode_dir()?;
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

pub fn save_goals(session_id: &str, goals: &[TodoGoal]) -> Result<()> {
    let path = goals_path(session_id)?;
    storage::write_json_fast(&path, goals)
}

fn goals_path(session_id: &str) -> Result<PathBuf> {
    let base = storage::kcode_dir()?;
    Ok(base
        .join("todos")
        .join(format!("{}-goals.json", session_id)))
}

/// The plan-level intent assessment lives in its own file beside the todo list
/// and per-group goals, so each format stays independently readable.
pub fn load_plan(session_id: &str) -> Result<TodoPlan> {
    let path = plan_path(session_id)?;
    if !path.exists() {
        return Ok(TodoPlan::default());
    }
    storage::read_json(&path).or_else(|_| Ok(TodoPlan::default()))
}

pub fn save_plan(session_id: &str, plan: &TodoPlan) -> Result<()> {
    let path = plan_path(session_id)?;
    storage::write_json_fast(&path, plan)?;
    Ok(())
}

fn plan_path(session_id: &str) -> Result<PathBuf> {
    let base = storage::kcode_dir()?;
    Ok(base.join("todos").join(format!("{}-plan.json", session_id)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn substitute_and_blocked_checks_do_not_pass_involved_acceptance_gate() {
        let goal = |relevance| TodoGoal {
            difficulty: Some(Difficulty::Involved),
            feedback_loop_relevance: Some(relevance),
            ..Default::default()
        };

        assert!(!feedback_loop_relevance_passes(&goal(
            FeedbackLoopRelevance::Synthetic
        )));
        assert!(!feedback_loop_relevance_passes(&goal(
            FeedbackLoopRelevance::Representative
        )));
        assert!(!feedback_loop_relevance_passes(&goal(
            FeedbackLoopRelevance::AcceptanceBlocked
        )));
        assert!(feedback_loop_relevance_passes(&goal(
            FeedbackLoopRelevance::AcceptanceAligned
        )));
    }

    /// The turn-finish gate must tell the model how to clear it without implying
    /// that the todo write which triggered the check was discarded.
    #[test]
    fn the_poke_names_the_count_and_invites_an_update() {
        let message = build_auto_poke_message(3);
        assert!(message.contains("3 incomplete todos"), "{message}");
        assert!(message.contains("update the todo tool"), "{message}");
        assert!(
            build_auto_poke_message(1).contains("1 incomplete todo."),
            "the singular form must not gain an s"
        );
    }

    #[test]
    fn plan_intent_fields_round_trip_through_storage() {
        let _guard = crate::storage::lock_test_env();
        let previous_home = std::env::var_os("KCODE_HOME");
        let dir = tempfile::TempDir::new().expect("tempdir");
        crate::env::set_var("KCODE_HOME", dir.path());

        let plan = TodoPlan {
            user_intention: Some("Preserve why the user requested the work".to_string()),
            understands_user_intent: Some(IntentUnderstanding::Clear),
            ..Default::default()
        };
        save_plan("user-intention-round-trip", &plan).expect("save plan");
        let stored =
            std::fs::read_to_string(plan_path("user-intention-round-trip").expect("plan path"))
                .expect("read stored plan");
        assert!(stored.contains("\"understands_user_intent\""));
        assert!(!stored.contains("\"alignment_score\""));
        assert!(!stored.contains("\"user_intention_alignment\""));

        let loaded = load_plan("user-intention-round-trip").expect("load plan");
        assert_eq!(loaded, plan);

        match previous_home {
            Some(value) => crate::env::set_var("KCODE_HOME", value),
            None => crate::env::remove_var("KCODE_HOME"),
        }
    }
}
