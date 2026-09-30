use crate::storage;
use anyhow::Result;
use std::path::PathBuf;

/// Generic mid-task reassessment prompt. The elapsed-time policy that triggers
/// it is intentionally private so the model reassesses from evidence rather
/// than targeting a timer or evaluator boundary.
pub const TODO_LONG_SESSION_REVIEW_MESSAGE: &str = "[auto] Re-read the request. Update the todo plan and goal assessments from the evidence gathered so far. Correct anything stale or overstated, then continue the work. Do not reply or wait for the user.";
const PRE_COMPACT_TODO_LONG_SESSION_REVIEW_MESSAGE: &str = "[automated todo assessment review - not a user message] Re-read the request. Update the todo plan and goal assessments from the evidence gathered so far. Correct anything stale or overstated, then continue the work. Do not reply or wait for the user.";
const PRE_BUDGET_TODO_LONG_SESSION_REVIEW_MESSAGE: &str = "[automated todo assessment review - not a user message] Re-read the original request and reconsider the current todo plan and every goal assessment using the evidence gathered during the work so far. Correct anything stale or overstated, including intent understanding, feedback-loop relevance and coverage, autonomy, difficulty, delivery, confidence, iteration maturity, and stopping evidence. Do not reply conversationally or wait for the user. Continue the work after saving an honest updated assessment.";

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

// Message texts produced by the removed enforcement tier.
//
// The checks that sent them are gone. `is_auto_poke_message` still lists every
// spelling so a session resumed from an older transcript renders them as system
// notices instead of as the user's own prompts. braid: this block is
// recognition-only and goes with that text matching.
//
/// Pre-plan-intent-rewrite alignment continuation. Kept only so persisted
/// transcripts still classify it as a synthetic gate message, not a user turn.
const LEGACY_TODO_ALIGNMENT_CONTINUATION_MESSAGE: &str = "Your alignment score is not high enough. Build a requirement inventory from the user's request, including outcomes, deliverables, constraints, prohibited actions, integration paths, edge cases, and necessary follow-through. Revise the plan and its stated user intention to represent every material item. Then map each item to an explicit observation or check in a feedback loop. Generic instructions to run tests, verify, or review count only for requirements those checks actually enforce; add separate checks for non-testable requirements. Reassess the weaker link before continuing the task.";

/// Model-facing continuation for the private intent-understanding check.
pub const TODO_INTENT_UNDERSTANDING_CONTINUATION_MESSAGE: &str = "[auto] Understand the user's intent better. Try to avoid asking the user. Make sure the todo is up to date.";
const PRE_COMPACT_TODO_INTENT_UNDERSTANDING_CONTINUATION_MESSAGE: &str = "Understand the user's intent better. Try to avoid asking the user. Make sure the todo is up to date.";

/// Previous verbose wording, retained so persisted sessions still classify it
/// as a hidden quality-gate message after the concise rewrite.
const PRE_CONCISE_TODO_INTENT_UNDERSTANDING_CONTINUATION_MESSAGE: &str = "Your understanding of the user's intent is not high enough. Re-read the request and think harder about what the user actually wants and left implicit, using the conversation and codebase as evidence. Form a requirement inventory covering outcomes, deliverables, constraints, prohibited actions, integration paths, edge cases, and necessary follow-through, and check the plan represents every material item. Do not ask the user; resolve the ambiguity yourself, then update the plan's user intention and understands_user_intent.";
const PRE_TODO_REMINDER_INTENT_UNDERSTANDING_CONTINUATION_MESSAGE: &str =
    "Understand the user's intent better. Try to avoid asking the user.";

/// Model-facing continuation for the private closed-feedback-loop check. Names
/// the assessment category without disclosing the score or threshold.
pub const TODO_CLOSED_FEEDBACK_LOOP_CONTINUATION_MESSAGE: &str = "[auto] Your feedback loop isn't good enough. Think about what feedback loops you need. Make sure the todo is up to date.";
const PRE_COMPACT_TODO_CLOSED_FEEDBACK_LOOP_CONTINUATION_MESSAGE: &str = "Improve the goal's feedback loop. Name a concrete check for each requirement and what result will show it passed. Update the todo, then continue the work.";
const PRE_TODO_REMINDER_CLOSED_FEEDBACK_LOOP_CONTINUATION_MESSAGE: &str = "Improve the goal's feedback loop. Name a concrete check for each requirement and what result will show it passed. Update the goal, then continue the work.";
const PRE_BUDGET_TODO_CLOSED_FEEDBACK_LOOP_CONTINUATION_MESSAGE: &str = "Your feedback loop is not closed. First, improve the goal's objective and name the observation that reports back on each requirement, so progress can be measured across iterations. Generic phrases such as run tests, verify, or review count only for requirements those named checks demonstrably enforce; add separate explicit checks for non-testable requirements. Then call the todo tool again with the revised goal before continuing the task. The goal is to create a strong feedback loop you can iterate against.";

/// Pre-rename ("hill-climbability") version of the closed-feedback-loop
/// continuation. Kept only so persisted transcripts still classify it as a
/// synthetic gate message rather than a user turn.
const LEGACY_TODO_HILL_CLIMBABILITY_CONTINUATION_MESSAGE: &str = "Your hill-climbability is not high enough. First, improve the goal's objective and feedback loop so progress can be measured across iterations. Then call the todo tool again with the revised goal before continuing the task. The goal is to create a strong feedback loop you can iterate against.";

/// Model-facing continuation for the private end-to-end ownership check. It
/// asks for more work without revealing that an evaluator triggered it.
pub const TODO_OWNERSHIP_CONTINUATION_MESSAGE: &str =
    "[auto] Continue the work below. Keep the todo up to date; do not reply or wait for the user.";
const PRE_COMPACT_TODO_OWNERSHIP_CONTINUATION_MESSAGE: &str = "[automated follow-up - not a user message] Continue the work below. Keep the todo up to date; do not reply or wait for the user.";

/// Legacy ownership-gate wording (pre delivery_state rename). Kept only so
/// persisted transcripts still classify it as a synthetic gate message.
const LEGACY_TODO_OWNERSHIP_CONTINUATION_MESSAGE: &str = "[automated todo completion gate - not a user message] Your end-to-end ownership is not high enough to finish this goal.";

/// Model-facing continuation for private completion-confidence checks.
pub const TODO_COMPLETION_CONTINUATION_MESSAGE: &str = "[auto] Do more validation on the work below. Keep the todo up to date; do not reply or wait for the user.";
const PRE_COMPACT_TODO_COMPLETION_CONTINUATION_MESSAGE: &str = "[automated follow-up - not a user message] Do more validation on the work below. Keep the todo up to date; do not reply or wait for the user.";

/// Model-facing continuation identifying the items whose confidence jumped and
/// asking for one explicit double-check without exposing scores or thresholds.
pub const TODO_CONFIDENCE_SPIKE_CONTINUATION_MESSAGE: &str = "[auto] You had a confidence jump in the items below. Double-check that these are correct. Keep the todo up to date; do not reply or wait for the user.";
const PRE_COMPACT_TODO_CONFIDENCE_SPIKE_CONTINUATION_MESSAGE: &str = "[automated follow-up - not a user message] You had a confidence jump in the items below. Double-check that these are correct. Keep the todo up to date; do not reply or wait for the user.";

/// Final synthetic turn after every todo completion check has passed. Gate
/// continuations tell the model not to reply, so without this handoff a cycle
/// can end on a bare tool call or an internal-looking validation response.
pub const TODO_FINAL_RESPONSE_CONTINUATION_MESSAGE: &str = "[auto] Quality checks passed. Give the user a concise final response now. Do not call the todo tool or do more work.";
const PRE_COMPACT_TODO_FINAL_RESPONSE_CONTINUATION_MESSAGE: &str = "[automated follow-up - not a user message] Quality checks passed. Give the user a concise final response now. Do not call the todo tool or do more work.";
const PRE_BUDGET_TODO_FINAL_RESPONSE_CONTINUATION_MESSAGE: &str = "[automated follow-up - not a user message] All work and quality checks are complete. Give the user the final response now. Default to fewer than 5 lines unless the user's request requires more detail. Summarize the outcome clearly; do not call the todo tool or perform more work.";

/// Wording of the removed turn-end digest, kept only so persisted transcripts
/// still classify it as a synthetic gate message rather than a user prompt.
pub const TODO_GATE_DIGEST_PREFIX: &str = "[auto] Before you treat this turn as finished, double-check the weak points it surfaced. Keep the todo up to date. Do not reply or wait for the user.";
const PRE_COMPACT_TODO_GATE_DIGEST_PREFIX: &str = "Before you treat this turn as finished, double-check the weak points it surfaced. Keep the todo up to date. Do not reply or wait for the user.";
const LABELED_TODO_GATE_DIGEST_PREFIX: &str = "[automated todo quality review - not a user message] Before you treat this turn as finished, double-check the weak points it surfaced. Do not reply conversationally or wait for the user.";

const LEGACY_TODO_CONFIDENCE_SUMMARY_PREFIX: &str = "All todos are done. Todo confidence summary:";
/// Pre-gate-rewrite texts (before the "[automated todo completion gate" prefix)
/// still exist in persisted transcripts; keep detecting them so reload/resume
/// does not re-render them as user prompts.
const LEGACY_TODO_COMPLETION_CONTINUATION_MESSAGE: &str =
    "Your completion confidence is missing or not high enough.";
const LEGACY_TODO_CONFIDENCE_SPIKE_CONTINUATION_MESSAGE: &str =
    "Your completion confidence rose too sharply to count as independently validated.";
/// Wording used immediately before the evidence-backed framing. Persisted
/// sessions can still contain it and must keep treating it as a hidden gate.
const PRE_EVIDENCE_TODO_CONFIDENCE_SPIKE_CONTINUATION_MESSAGE: &str = "[automated follow-up - not a user message] Independently recheck the work below. Keep the todo up to date; do not reply or wait for the user.";

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
        || trimmed.starts_with(TODO_CLOSED_FEEDBACK_LOOP_CONTINUATION_MESSAGE)
        || trimmed.starts_with(PRE_COMPACT_TODO_CLOSED_FEEDBACK_LOOP_CONTINUATION_MESSAGE)
        || trimmed.starts_with(PRE_TODO_REMINDER_CLOSED_FEEDBACK_LOOP_CONTINUATION_MESSAGE)
        || trimmed.starts_with(PRE_BUDGET_TODO_CLOSED_FEEDBACK_LOOP_CONTINUATION_MESSAGE)
        || trimmed.starts_with(LEGACY_TODO_HILL_CLIMBABILITY_CONTINUATION_MESSAGE)
        || trimmed.starts_with(LEGACY_TODO_ALIGNMENT_CONTINUATION_MESSAGE)
        || trimmed.starts_with(TODO_INTENT_UNDERSTANDING_CONTINUATION_MESSAGE)
        || trimmed.starts_with(PRE_COMPACT_TODO_INTENT_UNDERSTANDING_CONTINUATION_MESSAGE)
        || trimmed.starts_with(PRE_TODO_REMINDER_INTENT_UNDERSTANDING_CONTINUATION_MESSAGE)
        || trimmed.starts_with(PRE_CONCISE_TODO_INTENT_UNDERSTANDING_CONTINUATION_MESSAGE)
        || trimmed.starts_with(TODO_OWNERSHIP_CONTINUATION_MESSAGE)
        || trimmed.starts_with(PRE_COMPACT_TODO_OWNERSHIP_CONTINUATION_MESSAGE)
        || trimmed.starts_with(LEGACY_TODO_OWNERSHIP_CONTINUATION_MESSAGE)
        || trimmed.starts_with(TODO_COMPLETION_CONTINUATION_MESSAGE)
        || trimmed.starts_with(PRE_COMPACT_TODO_COMPLETION_CONTINUATION_MESSAGE)
        || trimmed.starts_with(TODO_CONFIDENCE_SPIKE_CONTINUATION_MESSAGE)
        || trimmed.starts_with(PRE_COMPACT_TODO_CONFIDENCE_SPIKE_CONTINUATION_MESSAGE)
        || trimmed.starts_with(TODO_FINAL_RESPONSE_CONTINUATION_MESSAGE)
        || trimmed.starts_with(PRE_COMPACT_TODO_FINAL_RESPONSE_CONTINUATION_MESSAGE)
        || trimmed.starts_with(PRE_BUDGET_TODO_FINAL_RESPONSE_CONTINUATION_MESSAGE)
        || trimmed.starts_with(LEGACY_TODO_COMPLETION_CONTINUATION_MESSAGE)
        || trimmed.starts_with(LEGACY_TODO_CONFIDENCE_SPIKE_CONTINUATION_MESSAGE)
        || trimmed.starts_with(PRE_EVIDENCE_TODO_CONFIDENCE_SPIKE_CONTINUATION_MESSAGE)
        || trimmed.starts_with(LEGACY_TODO_CONFIDENCE_SUMMARY_PREFIX)
        || trimmed.starts_with(TODO_GATE_DIGEST_PREFIX)
        || trimmed.starts_with(PRE_COMPACT_TODO_GATE_DIGEST_PREFIX)
        || trimmed.starts_with(LABELED_TODO_GATE_DIGEST_PREFIX)
        || trimmed.starts_with(TODO_LONG_SESSION_REVIEW_MESSAGE)
        || trimmed.starts_with(PRE_COMPACT_TODO_LONG_SESSION_REVIEW_MESSAGE)
        || trimmed.starts_with(PRE_BUDGET_TODO_LONG_SESSION_REVIEW_MESSAGE)
}

/// Short, user-facing stand-in for a synthetic auto-poke/gate continuation.
///
/// The continuations themselves are written for the model and name specific
/// todos and required fields. Showing that wall of instructions in the
/// transcript (on reload/resume, where the live short notice is gone) buries the
/// conversation, so the UI renders this one-liner instead.
pub fn auto_poke_display_summary(message: &str) -> Option<&'static str> {
    let trimmed = message.trim();
    if !is_auto_poke_message(trimmed) {
        return None;
    }
    if trimmed.starts_with(TODO_CONFIDENCE_SPIKE_CONTINUATION_MESSAGE)
        || trimmed.starts_with(PRE_COMPACT_TODO_CONFIDENCE_SPIKE_CONTINUATION_MESSAGE)
        || trimmed.starts_with(LEGACY_TODO_CONFIDENCE_SPIKE_CONTINUATION_MESSAGE)
        || trimmed.starts_with(PRE_EVIDENCE_TODO_CONFIDENCE_SPIKE_CONTINUATION_MESSAGE)
    {
        return Some("🔍 Double-checking confidence jumps...");
    }
    if trimmed.starts_with(TODO_FINAL_RESPONSE_CONTINUATION_MESSAGE)
        || trimmed.starts_with(PRE_COMPACT_TODO_FINAL_RESPONSE_CONTINUATION_MESSAGE)
        || trimmed.starts_with(PRE_BUDGET_TODO_FINAL_RESPONSE_CONTINUATION_MESSAGE)
    {
        return Some("✅ Preparing the final response...");
    }
    if trimmed.starts_with(TODO_COMPLETION_CONTINUATION_MESSAGE)
        || trimmed.starts_with(PRE_COMPACT_TODO_COMPLETION_CONTINUATION_MESSAGE)
        || trimmed.starts_with(LEGACY_TODO_COMPLETION_CONTINUATION_MESSAGE)
        || trimmed.starts_with(LEGACY_TODO_CONFIDENCE_SUMMARY_PREFIX)
    {
        return Some("🔍 Double-checking confidence for you...");
    }
    if trimmed.starts_with(TODO_GATE_DIGEST_PREFIX)
        || trimmed.starts_with(PRE_COMPACT_TODO_GATE_DIGEST_PREFIX)
        || trimmed.starts_with(LABELED_TODO_GATE_DIGEST_PREFIX)
    {
        return Some("🔍 Reviewing the weak points of this turn for you...");
    }
    if trimmed.starts_with(TODO_LONG_SESSION_REVIEW_MESSAGE)
        || trimmed.starts_with(PRE_COMPACT_TODO_LONG_SESSION_REVIEW_MESSAGE)
        || trimmed.starts_with(PRE_BUDGET_TODO_LONG_SESSION_REVIEW_MESSAGE)
    {
        return Some("🔍 Rechecking the plan and assessments after extended work...");
    }
    if trimmed.starts_with(TODO_OWNERSHIP_CONTINUATION_MESSAGE)
        || trimmed.starts_with(PRE_COMPACT_TODO_OWNERSHIP_CONTINUATION_MESSAGE)
        || trimmed.starts_with(LEGACY_TODO_OWNERSHIP_CONTINUATION_MESSAGE)
    {
        return Some("🔍 Checking the delivery state of the finished work...");
    }
    if trimmed.starts_with(TODO_INTENT_UNDERSTANDING_CONTINUATION_MESSAGE)
        || trimmed.starts_with(PRE_COMPACT_TODO_INTENT_UNDERSTANDING_CONTINUATION_MESSAGE)
        || trimmed.starts_with(PRE_TODO_REMINDER_INTENT_UNDERSTANDING_CONTINUATION_MESSAGE)
        || trimmed.starts_with(PRE_CONCISE_TODO_INTENT_UNDERSTANDING_CONTINUATION_MESSAGE)
    {
        return Some("🔍 Re-checking the request was understood...");
    }
    if trimmed.starts_with(TODO_CLOSED_FEEDBACK_LOOP_CONTINUATION_MESSAGE)
        || trimmed.starts_with(PRE_COMPACT_TODO_CLOSED_FEEDBACK_LOOP_CONTINUATION_MESSAGE)
        || trimmed.starts_with(PRE_TODO_REMINDER_CLOSED_FEEDBACK_LOOP_CONTINUATION_MESSAGE)
        || trimmed.starts_with(PRE_BUDGET_TODO_CLOSED_FEEDBACK_LOOP_CONTINUATION_MESSAGE)
        || trimmed.starts_with(LEGACY_TODO_HILL_CLIMBABILITY_CONTINUATION_MESSAGE)
        || trimmed.starts_with(LEGACY_TODO_ALIGNMENT_CONTINUATION_MESSAGE)
    {
        return Some("🔍 Asking for a stronger way to verify this work...");
    }
    // Incomplete-todos poke: the count is genuinely useful, and it is already
    // short, so it keeps its own text.
    None
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

    #[test]
    fn built_auto_poke_messages_are_detected() {
        assert!(is_auto_poke_message(&build_auto_poke_message(1)));
        assert!(is_auto_poke_message(&build_auto_poke_message(3)));
        assert!(is_auto_poke_message(
            TODO_CLOSED_FEEDBACK_LOOP_CONTINUATION_MESSAGE
        ));
        assert!(is_auto_poke_message(
            LEGACY_TODO_ALIGNMENT_CONTINUATION_MESSAGE
        ));
        assert!(is_auto_poke_message(
            TODO_INTENT_UNDERSTANDING_CONTINUATION_MESSAGE
        ));
        assert!(is_auto_poke_message(TODO_OWNERSHIP_CONTINUATION_MESSAGE));
        assert!(is_auto_poke_message(TODO_COMPLETION_CONTINUATION_MESSAGE));
        assert!(is_auto_poke_message(
            TODO_CONFIDENCE_SPIKE_CONTINUATION_MESSAGE
        ));
        assert!(is_auto_poke_message(
            TODO_FINAL_RESPONSE_CONTINUATION_MESSAGE
        ));
        assert_eq!(
            auto_poke_display_summary(TODO_FINAL_RESPONSE_CONTINUATION_MESSAGE),
            Some("✅ Preparing the final response...")
        );
        assert!(is_auto_poke_message(LEGACY_TODO_CONFIDENCE_SUMMARY_PREFIX));
        assert!(is_auto_poke_message(LABELED_TODO_GATE_DIGEST_PREFIX));
    }

    #[test]
    fn quality_continuations_are_actionable_without_private_calibration() {
        for (message, category) in [
            (
                TODO_CLOSED_FEEDBACK_LOOP_CONTINUATION_MESSAGE,
                "feedback loop isn't good enough",
            ),
            (
                TODO_INTENT_UNDERSTANDING_CONTINUATION_MESSAGE,
                "understand the user's intent better",
            ),
            (TODO_OWNERSHIP_CONTINUATION_MESSAGE, "continue the work"),
            (TODO_COMPLETION_CONTINUATION_MESSAGE, "more validation"),
            (
                TODO_CONFIDENCE_SPIKE_CONTINUATION_MESSAGE,
                "confidence jump",
            ),
        ] {
            let lower = message.to_ascii_lowercase();
            assert!(lower.contains(category));
            assert!(!message.chars().any(|ch| ch.is_ascii_digit()));
            for disclosure in ["threshold", "percent", "quality gate"] {
                assert!(
                    !lower.contains(disclosure),
                    "category-only continuation disclosed {disclosure}: {message}"
                );
            }
            if category != "alignment score" {
                assert!(
                    !lower.contains("score"),
                    "category-only continuation disclosed score: {message}"
                );
            }
        }

        assert!(TODO_CLOSED_FEEDBACK_LOOP_CONTINUATION_MESSAGE.contains("Think about"));
        assert!(TODO_INTENT_UNDERSTANDING_CONTINUATION_MESSAGE.contains("Try to avoid asking"));
        for message in [
            TODO_OWNERSHIP_CONTINUATION_MESSAGE,
            TODO_COMPLETION_CONTINUATION_MESSAGE,
        ] {
            let lower = message.to_ascii_lowercase();
            for evaluator_term in ["gate", "flagged", "failed", "threshold", "confidence"] {
                assert!(
                    !lower.contains(evaluator_term),
                    "disclosed {evaluator_term}: {message}"
                );
            }
        }
        let spike = TODO_CONFIDENCE_SPIKE_CONTINUATION_MESSAGE.to_ascii_lowercase();
        for evaluator_term in ["gate", "flagged", "failed", "threshold", "score"] {
            assert!(
                !spike.contains(evaluator_term),
                "disclosed {evaluator_term}"
            );
        }
    }

    #[test]
    fn working_quality_gates_remind_the_model_to_update_todos() {
        for (name, message) in [
            ("long session review", TODO_LONG_SESSION_REVIEW_MESSAGE),
            (
                "intent understanding",
                TODO_INTENT_UNDERSTANDING_CONTINUATION_MESSAGE,
            ),
            (
                "closed feedback loop",
                TODO_CLOSED_FEEDBACK_LOOP_CONTINUATION_MESSAGE,
            ),
            ("ownership", TODO_OWNERSHIP_CONTINUATION_MESSAGE),
            ("completion", TODO_COMPLETION_CONTINUATION_MESSAGE),
            (
                "confidence jump",
                TODO_CONFIDENCE_SPIKE_CONTINUATION_MESSAGE,
            ),
        ] {
            assert!(
                message.to_ascii_lowercase().contains("todo"),
                "{name} quality gate does not remind the model to update the todo: {message}"
            );
        }
    }

    #[test]
    fn real_user_prompts_are_not_detected_as_pokes() {
        assert!(!is_auto_poke_message("fix the login bug"));
        assert!(!is_auto_poke_message(
            "You have 2 incomplete todos. Continue working, or update the todo tool.\n\nalso please fix the tests"
        ));
        assert!(!is_auto_poke_message(""));
    }

    /// The turn-finish gate must tell the model how to clear it without implying
    /// that the todo write which triggered the check was discarded.
    #[test]
    fn ownership_message_names_the_field_that_must_be_raised() {
        assert!(TODO_OWNERSHIP_CONTINUATION_MESSAGE.contains("Continue the work below"));
        for private_calibration in [
            "necessary_followthrough",
            "outcome_reached",
            "plateau_confirmed",
            "budget_exhausted",
        ] {
            assert!(!TODO_OWNERSHIP_CONTINUATION_MESSAGE.contains(private_calibration));
        }
        assert!(
            TODO_OWNERSHIP_CONTINUATION_MESSAGE.contains("Keep the todo up to date"),
            "the ownership nudge must say how to update the assessment"
        );
        assert!(
            !TODO_OWNERSHIP_CONTINUATION_MESSAGE.contains("rejected")
                && !TODO_OWNERSHIP_CONTINUATION_MESSAGE.contains("unchanged"),
            "the turn-finish nudge must not claim the already-saved write was discarded"
        );
        assert!(TODO_COMPLETION_CONTINUATION_MESSAGE.contains("more validation"));
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
