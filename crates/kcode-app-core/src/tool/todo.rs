use super::{Tool, ToolContext, ToolOutput};
use crate::bus::{Bus, BusEvent, TodoEvent};
use crate::todo::{
    TodoGoal, TodoGoalChange, TodoGoalField, TodoItem, TodoPlan, TodoPlanChange, TodoPlanField,
    load_goals, load_plan, load_todos, save_goals, save_plan, save_todos,
};
use anyhow::{Result, bail};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;

pub struct TodoTool;

impl TodoTool {
    pub fn new() -> Self {
        Self
    }
}

/// Fold each incoming todo's confidence into its tool-maintained history.
///
/// The model reports `confidence` while working and `completion_confidence` at
/// completion. Each todo-tool write contributes at most one observation so a
/// single completion update cannot manufacture an apparent intermediate step.
/// The append-only trail lets downstream consumers distinguish an
/// evidence-driven rise (75 -> 85 -> 95 -> 100) from a bulk end-of-task stamp
/// (75 -> 100). Model-supplied `confidence_history` is ignored: the tool owns
/// this field.
fn merge_confidence_history(previous: &[TodoItem], incoming: &mut [TodoItem]) {
    let prior: HashMap<&str, &TodoItem> = previous
        .iter()
        .map(|todo| (todo.id.as_str(), todo))
        .collect();
    for todo in incoming.iter_mut() {
        let previous_todo = prior.get(todo.id.as_str()).copied();
        let mut history = previous_todo
            .map(|prev| prev.confidence_history.clone())
            .unwrap_or_default();
        if history.is_empty()
            && let Some(value) = previous_todo.and_then(|prev| {
                if prev.status == "completed" {
                    prev.completion_confidence.or(prev.confidence)
                } else {
                    prev.confidence
                }
            })
        {
            history.push(value);
        }
        let observation = if todo.status == "completed" {
            todo.completion_confidence.or(todo.confidence)
        } else {
            todo.confidence
        };
        if let Some(value) = observation
            && history.last() != Some(&value)
        {
            history.push(value);
        }
        todo.confidence_history = history;
    }
}

#[derive(Deserialize)]
struct TodoInput {
    todos: Option<Vec<TodoItem>>,
    goals: Option<Vec<TodoGoal>>,
    plan: Option<TodoPlan>,
}

fn parse_todo_input(input: Value) -> Result<TodoInput> {
    let params: TodoInput = serde_json::from_value(normalize_todo_input(input))?;
    if let Some(todo) = params.todos.as_ref().and_then(|todos| {
        todos
            .iter()
            .find(|todo| crate::todo::canonical_todo_status(&todo.status).is_none())
    }) {
        bail!(
            "invalid todo status {:?}; expected one of: pending, in_progress, completed, cancelled",
            todo.status
        );
    }
    Ok(params)
}

/// Normalize a goal's group label: trimmed, with empty/whitespace collapsed
/// to `None` (the implicit goal of an ungrouped list).
fn goal_group_key(group: Option<&str>) -> Option<String> {
    group
        .map(str::trim)
        .filter(|group| !group.is_empty())
        .map(str::to_string)
}

fn record_score_observation<T: Copy + PartialEq>(history: &mut Vec<T>, value: Option<T>) {
    if let Some(value) = value
        && history.last() != Some(&value)
    {
        history.push(value);
    }
}

/// Merge incoming goal assessments with the stored ones.
///
/// Incoming goals win per group key; stored goals for groups the write does
/// not mention are retained (a todo update should not silently discard goal
/// assessments). Score histories are tool-maintained: whatever the model sends
/// for them is discarded in favor of the stored trail plus this write's
/// observation.
fn merge_goals(stored: &[TodoGoal], incoming: Option<Vec<TodoGoal>>) -> Vec<TodoGoal> {
    let Some(incoming) = incoming else {
        return stored.to_vec();
    };
    let mut merged: Vec<TodoGoal> = Vec::new();
    for mut goal in incoming {
        goal.group = goal_group_key(goal.group.as_deref());
        let previous = stored
            .iter()
            .find(|prev| goal_group_key(prev.group.as_deref()) == goal.group);
        goal.closed_feedback_loop_history = previous
            .map(|prev| prev.closed_feedback_loop_history.clone())
            .unwrap_or_default();
        goal.feedback_loop_relevance_history = previous
            .map(|prev| prev.feedback_loop_relevance_history.clone())
            .unwrap_or_default();
        goal.feedback_loop_coverage_history = previous
            .map(|prev| prev.feedback_loop_coverage_history.clone())
            .unwrap_or_default();
        goal.feedback_loop_traceability_history = previous
            .map(|prev| prev.feedback_loop_traceability_history.clone())
            .unwrap_or_default();
        goal.delivery_state_history = previous
            .map(|prev| prev.delivery_state_history.clone())
            .unwrap_or_default();
        // Field-level merge, matching `merge_plan`: a write that revises one
        // assessment must not silently erase the others. Without this the
        // turn-end digest would read a stale `None` and re-raise a point the
        // agent had already resolved.
        if let Some(prev) = previous {
            if goal.closed_feedback_loop.is_none() {
                goal.closed_feedback_loop = prev.closed_feedback_loop;
            }
            if goal.delivery_state.is_none() {
                goal.delivery_state = prev.delivery_state;
            }
            if goal.feedback_loop_relevance.is_none() {
                goal.feedback_loop_relevance = prev.feedback_loop_relevance;
            }
            if goal.feedback_loop_coverage.is_none() {
                goal.feedback_loop_coverage = prev.feedback_loop_coverage;
            }
            if goal.feedback_loop_traceability.is_none() {
                goal.feedback_loop_traceability = prev.feedback_loop_traceability;
            }
            if goal.difficulty.is_none() {
                goal.difficulty = prev.difficulty;
            }
            if goal.autonomy.is_none() {
                goal.autonomy = prev.autonomy;
            }
            if goal.iteration_maturity.is_none() {
                goal.iteration_maturity = prev.iteration_maturity;
            }
            if goal.feedback_loop.is_none() {
                goal.feedback_loop = prev.feedback_loop.clone();
            }
            if goal.stopping_evidence.is_none() {
                goal.stopping_evidence = prev.stopping_evidence.clone();
            }
        }
        record_score_observation(
            &mut goal.closed_feedback_loop_history,
            goal.closed_feedback_loop,
        );
        record_score_observation(
            &mut goal.feedback_loop_relevance_history,
            goal.feedback_loop_relevance,
        );
        record_score_observation(
            &mut goal.feedback_loop_coverage_history,
            goal.feedback_loop_coverage,
        );
        record_score_observation(
            &mut goal.feedback_loop_traceability_history,
            goal.feedback_loop_traceability,
        );
        record_score_observation(&mut goal.delivery_state_history, goal.delivery_state);
        if let Some(slot) = merged
            .iter_mut()
            .find(|existing| existing.group == goal.group)
        {
            *slot = goal;
        } else {
            merged.push(goal);
        }
    }
    for prev in stored {
        let key = goal_group_key(prev.group.as_deref());
        if !merged.iter().any(|goal| goal.group == key) {
            merged.push(prev.clone());
        }
    }
    merged
}

/// Drop retained goals whose todo group no longer exists in `todos`.
///
/// `merge_goals` deliberately keeps goals a write does not mention, so a
/// goals-only or partial update cannot erase assessments. But when the agent
/// moves on to a new task and replaces the todo list wholesale, goals from the
/// finished task have no todos left to describe and were shown indefinitely in
/// the todos panel (issue #695). Goals whose group still has a todo, and the
/// ungrouped goal for a flat list, are always kept.
fn prune_orphaned_goals(goals: Vec<TodoGoal>, todos: &[TodoItem]) -> Vec<TodoGoal> {
    if todos.is_empty() {
        return goals;
    }
    let live_groups: std::collections::HashSet<Option<String>> = todos
        .iter()
        .map(|todo| goal_group_key(todo.group.as_deref()))
        .collect();
    goals
        .into_iter()
        .filter(|goal| live_groups.contains(&goal_group_key(goal.group.as_deref())))
        .collect()
}

fn changed_goal_fields(before: Option<&TodoGoal>, after: Option<&TodoGoal>) -> Vec<TodoGoalField> {
    let mut fields = Vec::new();
    if before.and_then(|goal| goal.closed_feedback_loop)
        != after.and_then(|goal| goal.closed_feedback_loop)
    {
        fields.push(TodoGoalField::ClosedFeedbackLoop);
    }
    if before.and_then(|goal| goal.feedback_loop.as_ref())
        != after.and_then(|goal| goal.feedback_loop.as_ref())
    {
        fields.push(TodoGoalField::FeedbackLoop);
    }
    if before.and_then(|goal| goal.feedback_loop_relevance)
        != after.and_then(|goal| goal.feedback_loop_relevance)
    {
        fields.push(TodoGoalField::FeedbackLoopRelevance);
    }
    if before.and_then(|goal| goal.feedback_loop_coverage)
        != after.and_then(|goal| goal.feedback_loop_coverage)
    {
        fields.push(TodoGoalField::FeedbackLoopCoverage);
    }
    if before.and_then(|goal| goal.feedback_loop_traceability)
        != after.and_then(|goal| goal.feedback_loop_traceability)
    {
        fields.push(TodoGoalField::FeedbackLoopTraceability);
    }
    if before.and_then(|goal| goal.delivery_state) != after.and_then(|goal| goal.delivery_state) {
        fields.push(TodoGoalField::DeliveryState);
    }
    if before.and_then(|goal| goal.autonomy) != after.and_then(|goal| goal.autonomy) {
        fields.push(TodoGoalField::Autonomy);
    }
    if before.and_then(|goal| goal.iteration_maturity)
        != after.and_then(|goal| goal.iteration_maturity)
    {
        fields.push(TodoGoalField::IterationMaturity);
    }
    if before.and_then(|goal| goal.stopping_evidence.as_ref())
        != after.and_then(|goal| goal.stopping_evidence.as_ref())
    {
        fields.push(TodoGoalField::StoppingEvidence);
    }
    fields
}

/// Merge the incoming plan-level intent assessment with the stored one.
///
/// User intention describes why the user asked for the work and should remain
/// stable while the agent revises its steps, so an omitted intention inherits
/// the stored value. Sending an empty string clears it.
fn merge_plan(stored: &TodoPlan, incoming: Option<TodoPlan>) -> TodoPlan {
    let Some(mut plan) = incoming else {
        return stored.clone();
    };
    if plan.user_intention.is_none() {
        plan.user_intention = stored.user_intention.clone();
    }
    if plan.understands_user_intent.is_none() {
        plan.understands_user_intent = stored.understands_user_intent;
    }
    plan
}

fn plan_change(before: &TodoPlan, after: &TodoPlan) -> Option<TodoPlanChange> {
    let mut fields = Vec::new();
    if before.user_intention != after.user_intention {
        fields.push(TodoPlanField::UserIntention);
    }
    if before.understands_user_intent != after.understands_user_intent {
        fields.push(TodoPlanField::UnderstandsUserIntent);
    }
    (!fields.is_empty()).then(|| TodoPlanChange {
        before: Some(before.clone()),
        after: Some(after.clone()),
        fields,
    })
}

fn goal_changes(before: &[TodoGoal], after: &[TodoGoal]) -> Vec<TodoGoalChange> {
    let mut changes = Vec::new();
    for current in after {
        let key = goal_group_key(current.group.as_deref());
        let previous = before
            .iter()
            .find(|goal| goal_group_key(goal.group.as_deref()) == key);
        let fields = changed_goal_fields(previous, Some(current));
        if !fields.is_empty() {
            changes.push(TodoGoalChange {
                before: previous.cloned(),
                after: Some(current.clone()),
                fields,
            });
        }
    }
    for previous in before {
        let key = goal_group_key(previous.group.as_deref());
        if after
            .iter()
            .any(|goal| goal_group_key(goal.group.as_deref()) == key)
        {
            continue;
        }
        let fields = changed_goal_fields(Some(previous), None);
        if !fields.is_empty() {
            changes.push(TodoGoalChange {
                before: Some(previous.clone()),
                after: None,
                fields,
            });
        }
    }
    changes
}

fn build_todo_output(
    todos: Vec<TodoItem>,
    plan: TodoPlan,
    goals: Vec<TodoGoal>,
    plan_change: Option<TodoPlanChange>,
    goal_changes: Option<Vec<TodoGoalChange>>,
) -> Result<ToolOutput> {
    let remaining = todos
        .iter()
        .filter(|todo| todo.status != "completed")
        .count();
    let mut text = serde_json::to_string_pretty(&todos)?;
    if plan != TodoPlan::default() {
        text.push_str("\n\nPlan:\n");
        text.push_str(&serde_json::to_string_pretty(&plan)?);
    }
    if !goals.is_empty() {
        text.push_str("\n\nGoals:\n");
        text.push_str(&serde_json::to_string_pretty(&goals)?);
    }
    if let Some(plan_change) = plan_change.as_ref() {
        text.push_str("\n\nPlan updates:\n");
        text.push_str(&serde_json::to_string_pretty(plan_change)?);
    }
    if let Some(goal_changes) = goal_changes.as_ref().filter(|changes| !changes.is_empty()) {
        text.push_str("\n\nGoal updates:\n");
        text.push_str(&serde_json::to_string_pretty(goal_changes)?);
    }
    let mut metadata = json!({"todos": todos, "plan": plan, "goals": goals});
    if let Some(plan_change) = plan_change {
        metadata["plan_update"] = serde_json::to_value(plan_change)?;
    }
    if let Some(goal_changes) = goal_changes.filter(|changes| !changes.is_empty()) {
        metadata["goal_updates"] = serde_json::to_value(goal_changes)?;
    }
    Ok(ToolOutput::new(text)
        .with_title(format!("{} todos", remaining))
        .with_metadata(metadata))
}

/// Leniently normalize raw todo-tool arguments before strict deserialization.
///
/// Some providers (notably Claude tool calling) intermittently emit tool
/// arguments as JSON *strings* instead of native types: the whole `todos`
/// array as one stringified JSON blob, individual items as stringified
/// objects, or numeric fields like `confidence` as `"90"`. Strict
/// `serde_json::from_value` rejects these with `invalid type: string ...`,
/// failing the entire call (issue #357; same provider quirk as #106).
fn normalize_todo_input(mut input: Value) -> Value {
    let Some(obj) = input.as_object_mut() else {
        return input;
    };
    if let Some(plan) = obj.get_mut("plan") {
        if let Value::String(raw) = plan {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                *plan = Value::Null;
            } else if let Ok(parsed @ (Value::Object(_) | Value::Null)) =
                serde_json::from_str::<Value>(trimmed)
            {
                *plan = parsed;
            }
        }
        if let Some(fields) = plan.as_object_mut() {
            for key in [
                "alignment_score",
                "user_intention_alignment",
                "understands_user_intent",
            ] {
                if let Some(value) = fields.get_mut(key) {
                    coerce_empty_string_to_null(value);
                }
            }
        }
    }
    for key in ["todos", "goals"] {
        let Some(entries) = obj.get_mut(key) else {
            continue;
        };

        // Whole array sent as a stringified JSON blob.
        if let Value::String(raw) = entries {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                *entries = Value::Null;
            } else if let Ok(parsed @ (Value::Array(_) | Value::Null)) =
                serde_json::from_str::<Value>(trimmed)
            {
                *entries = parsed;
            }
        }

        if let Value::Array(items) = entries {
            for item in items.iter_mut() {
                // Individual item sent as a stringified JSON object.
                if let Value::String(raw) = item
                    && let Ok(parsed @ Value::Object(_)) = serde_json::from_str::<Value>(raw.trim())
                {
                    *item = parsed;
                }
                let Some(fields) = item.as_object_mut() else {
                    continue;
                };
                if key == "todos"
                    && let Some(Value::String(status)) = fields.get_mut("status")
                    && let Some(canonical) = crate::todo::canonical_todo_status(status)
                {
                    *status = canonical.to_string();
                }
                for key in [
                    "confidence",
                    "completion_confidence",
                    "alignment_score",
                    "user_intention_alignment",
                    "closed_feedback_loop",
                    // Pre-rename aliases; some prompts and replayed transcripts
                    // still carry the old keys.
                    "hill_climbability",
                    "end_to_end_ownership",
                    "delivery_state",
                    "feedback_loop_relevance",
                    "feedback_loop_coverage",
                    "feedback_loop_traceability",
                    "difficulty",
                    "autonomy",
                ] {
                    if let Some(value) = fields.get_mut(key) {
                        coerce_empty_string_to_null(value);
                    }
                }
            }
        }
    }
    input
}

/// Coerce an empty or whitespace-only string to `null` so an omitted-but-sent
/// assessment reads as absent. Numeric legacy scores (ints, floats, numeric
/// strings) are handled by the semantic-state deserializers themselves, so no
/// numeric coercion is needed here anymore.
fn coerce_empty_string_to_null(value: &mut Value) {
    if let Value::String(raw) = value
        && raw.trim().is_empty()
    {
        *value = Value::Null;
    }
}

#[async_trait]
impl Tool for TodoTool {
    fn name(&self) -> &str {
        "todo"
    }

    fn description(&self) -> &str {
        // SECURITY/EVAL: This is model-visible calibration text. Keep it
        // deliberately handwritten. Never generate it from gate constants or
        // interpolate private thresholds, because that would teach the model
        // how to target the evaluator instead of reporting an honest assessment.
        "Read or update structured todo items and optional goal-level assessments."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "intent": super::intent_schema_property(),
                "todos": {
                    "type": "array",
                    "description": "Todo list to save.",
                    "items": {
                        "type": "object",
                        "required": ["content", "status", "priority", "id", "confidence"],
                        "properties": {
                            "content": {
                                "type": "string",
                                "description": "Task."
                            },
                            "status": {
                                "type": "string",
                                "enum": ["pending", "in_progress", "completed", "cancelled"],
                                "description": "Status. Use completed when the task is done."
                            },
                            "priority": {
                                "type": "string",
                                "description": "Priority."
                            },
                            "id": {
                                "type": "string",
                                "description": "ID."
                            },
                            "group": {
                                "type": "string",
                                "description": "Optional group label; one group per coherent goal, new direction = new group. Omit for flat list."
                            },
                            "confidence": {
                                "type": "string",
                                "enum": ["speculative", "plausible", "validated", "verified"],
                                "description": "Evidence state that this todo can be completed correctly; reassess as evidence accumulates."
                            },
                            "completion_confidence": {
                                "type": "string",
                                "enum": ["speculative", "plausible", "validated", "verified"],
                                "description": "Evidence state behind this todo's completion. Use only for completed items."
                            },
                        }
                    }
                },
                "plan": {
                    "type": "object",
                    "description": "Plan-level understanding of the request. Send on first write and whenever understanding changes.",
                    "required": ["user_intention", "understands_user_intent"],
                    "properties": {
                        "user_intention": {
                            "type": "string",
                            "description": "What the user actually wants: underlying reason and desired end state. Omit later to retain."
                        },
                        "understands_user_intent": {
                            "type": "string",
                            "enum": ["uncertain", "partial", "clear", "complete"],
                            "description": "How well you understand what the user wants. Report uncertain or partial when guessing."
                        }
                    }
                },
                "goals": {
                    "type": "array",
                    "description": "Goal-level assessments, one per todo group (null = ungrouped). Omitted groups are retained.",
                    "items": {
                        "type": "object",
                        "required": ["closed_feedback_loop", "feedback_loop", "feedback_loop_relevance", "feedback_loop_coverage", "feedback_loop_traceability"],
                        "properties": {
                            "group": {
                                "type": "string",
                                "description": "Group label this goal describes. Omit or null for the ungrouped list."
                            },
                            "closed_feedback_loop": {
                                "type": "string",
                                "enum": ["absent", "weak", "usable", "strong", "closed"],
                                "description": "How much of this goal's correctness the feedback_loop can verify on its own."
                            },
                            "feedback_loop": {
                                "type": "string",
                                "description": "Requirement-to-check process: an explicit observation or check for each requirement of this goal."
                            },
                            "feedback_loop_relevance": {
                                "type": "string",
                                "enum": ["indirect", "synthetic", "representative", "acceptance_blocked", "acceptance_aligned"],
                                "description": "How directly the checks represent acceptance behavior."
                            },
                            "feedback_loop_coverage": {
                                "type": "string",
                                "enum": ["narrow", "main_paths", "edge_and_integration_paths"],
                                "description": "How broadly the checks exercise main workflows, integration boundaries, edge cases, packaging, and likely failure modes."
                            },
                            "feedback_loop_traceability": {
                                "type": "string",
                                "enum": ["unmapped", "partial", "complete"],
                                "description": "How completely requirements map to evidence."
                            },
                            "delivery_state": {
                                "type": "string",
                                "enum": ["change_made", "integrated", "workflow_validated", "outcome_delivered"],
                                "description": "Completion-time: how far the result actually traveled toward the user's outcome."
                            },
                            "difficulty": {
                                "type": "string",
                                "enum": ["trivial", "routine", "involved", "complex", "hard", "expert", "research", "open_ended"],
                                "description": "Honest intrinsic difficulty of this goal. Descriptive only."
                            },
                            "autonomy": {
                                "type": "string",
                                "enum": ["requested_only", "necessary_followthrough", "proactive", "stewardship"],
                                "description": "How far beyond the literal request the work went. Assess from what was completed."
                            },
                            "iteration_maturity": {
                                "type": "string",
                                "enum": ["not_started", "exploring", "improving", "plateau_unproven", "outcome_reached", "constraints_exhausted", "plateau_confirmed", "budget_exhausted"],
                                "description": "How far the feedback loop was actually exercised, and any evidence-based reason to stop iterating."
                            },
                            "stopping_evidence": {
                                "type": "string",
                                "description": "Evidence for the reported iteration_maturity: attempts, observations, or a real budget limit."
                            }
                        }
                    }
                }
            }
        })
    }

    async fn execute(&self, input: Value, ctx: ToolContext) -> Result<ToolOutput> {
        let params = parse_todo_input(input)?;
        let is_write = params.todos.is_some() || params.goals.is_some() || params.plan.is_some();
        let operation = if is_write { "write" } else { "read" };
        let result = if is_write {
            // Goals/plan-only writes keep the stored todo list.
            let previous = load_todos(&ctx.session_id).unwrap_or_default();
            let mut todos = params.todos.unwrap_or_else(|| previous.clone());
            merge_confidence_history(&previous, &mut todos);
            (|| {
                let stored_goals = load_goals(&ctx.session_id).unwrap_or_default();
                let stored_plan = load_plan(&ctx.session_id).unwrap_or_default();
                let goals = prune_orphaned_goals(merge_goals(&stored_goals, params.goals), &todos);
                let plan = merge_plan(&stored_plan, params.plan);
                // Assessment-only writes should render the fields that changed
                // instead of repeating an otherwise identical todo plan.
                let assessment_only = todos == previous;
                let concise_goal_changes = (assessment_only && !stored_goals.is_empty())
                    .then(|| goal_changes(&stored_goals, &goals));
                let concise_plan_change = assessment_only
                    .then(|| plan_change(&stored_plan, &plan))
                    .flatten();
                save_todos(&ctx.session_id, &todos)?;
                save_goals(&ctx.session_id, &goals)?;
                save_plan(&ctx.session_id, &plan)?;
                Bus::global().publish(BusEvent::TodoUpdated(TodoEvent {
                    session_id: ctx.session_id.clone(),
                    todos: todos.clone(),
                }));

                build_todo_output(
                    todos,
                    plan,
                    goals,
                    concise_plan_change,
                    concise_goal_changes,
                )
            })()
        } else {
            (|| {
                let todos = load_todos(&ctx.session_id)?;
                let goals = load_goals(&ctx.session_id).unwrap_or_default();
                let plan = load_plan(&ctx.session_id).unwrap_or_default();
                build_todo_output(todos, plan, goals, None, None)
            })()
        };
        result.map_err(|err| {
            crate::logging::warn(&format!(
                "[tool:todo] operation failed operation={} session_id={} error={}",
                operation, ctx.session_id, err
            ));
            err
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_is_named_todo() {
        assert_eq!(TodoTool::new().name(), "todo");
    }

    #[test]
    fn schema_advertises_intent_and_todos() {
        let schema = TodoTool::new().parameters_schema();
        let props = schema
            .get("properties")
            .and_then(|v| v.as_object())
            .expect("todo schema should have properties");
        assert_eq!(props.len(), 4);
        assert!(props.contains_key("intent"));
        assert!(props.contains_key("todos"));
        assert!(props.contains_key("plan"));
        assert!(props.contains_key("goals"));

        let item = props["todos"]
            .get("items")
            .and_then(|v| v.as_object())
            .expect("todos should describe item objects");
        let required = item
            .get("required")
            .and_then(|v| v.as_array())
            .expect("todo item should advertise required fields");
        assert!(required.iter().any(|v| v == "confidence"));
        let item_props = item
            .get("properties")
            .and_then(|v| v.as_object())
            .expect("todo item should advertise properties");
        assert!(item_props.contains_key("confidence"));
        assert!(item_props.contains_key("completion_confidence"));
        assert!(!item_props.contains_key("closed_feedback_loop"));
        assert_eq!(
            item_props["confidence"]["description"],
            "Evidence state that this todo can be completed correctly; reassess as evidence accumulates."
        );

        let plan_props = props["plan"]
            .get("properties")
            .and_then(|v| v.as_object())
            .expect("plan should describe properties");
        assert!(plan_props.contains_key("user_intention"));
        assert!(plan_props.contains_key("understands_user_intent"));
        assert!(!plan_props.contains_key("alignment_score"));
        assert!(!plan_props.contains_key("user_intention_alignment"));
        assert_eq!(plan_props.len(), 2);
        let plan_required = props["plan"]["required"]
            .as_array()
            .expect("plan should advertise required fields");
        assert!(plan_required.iter().any(|value| value == "user_intention"));
        assert!(
            plan_required
                .iter()
                .any(|value| value == "understands_user_intent")
        );

        let goal_props = props["goals"]
            .get("items")
            .and_then(|v| v.get("properties"))
            .and_then(|v| v.as_object())
            .expect("goals should describe item objects");
        assert!(goal_props.contains_key("group"));
        assert!(goal_props.contains_key("closed_feedback_loop"));
        assert!(goal_props.contains_key("feedback_loop"));
        assert!(goal_props.contains_key("feedback_loop_relevance"));
        assert!(goal_props.contains_key("feedback_loop_coverage"));
        assert!(goal_props.contains_key("feedback_loop_traceability"));
        assert!(goal_props.contains_key("delivery_state"));
        assert!(goal_props.contains_key("difficulty"));
        assert!(goal_props.contains_key("autonomy"));
        assert!(goal_props.contains_key("iteration_maturity"));
        assert!(goal_props.contains_key("stopping_evidence"));
        assert!(!goal_props.contains_key("end_to_end_ownership"));
        // Intent lives on the plan, not per goal.
        assert!(!goal_props.contains_key("user_intention"));
        assert!(!goal_props.contains_key("alignment_score"));
        assert!(!goal_props.contains_key("objective"));
        assert_eq!(goal_props.len(), 11);
        assert_eq!(
            goal_props["feedback_loop_relevance"]["enum"],
            json!([
                "indirect",
                "synthetic",
                "representative",
                "acceptance_blocked",
                "acceptance_aligned"
            ])
        );
        let relevance_description = goal_props["feedback_loop_relevance"]["description"]
            .as_str()
            .expect("feedback-loop relevance should describe the field");
        assert!(
            relevance_description.contains("acceptance behavior"),
            "relevance description should name what it assesses: {relevance_description}"
        );

        let goal_required = props["goals"]["items"]["required"]
            .as_array()
            .expect("goals should advertise required fields");
        assert!(
            goal_required
                .iter()
                .any(|value| value == "closed_feedback_loop")
        );
        assert!(goal_required.iter().any(|value| value == "feedback_loop"));
        assert!(
            goal_required
                .iter()
                .any(|value| value == "feedback_loop_relevance")
        );
        assert!(
            goal_required
                .iter()
                .any(|value| value == "feedback_loop_coverage")
        );
        assert!(
            goal_required
                .iter()
                .any(|value| value == "feedback_loop_traceability")
        );

        let alignment_description = plan_props["understands_user_intent"]
            .get("description")
            .and_then(Value::as_str)
            .expect("alignment score should describe representation coverage");
        assert!(alignment_description.contains("what the user wants"));
        assert!(alignment_description.contains("when guessing"));
        // The detailed calibration rubric lives in `docs/internals/todo-calibration.md`
        // (retrievable via `kcode_docs`), not in the always-on schema and not in
        // the category-only gate messages, which have their own token budget.
        let feedback_description = goal_props["feedback_loop"]
            .get("description")
            .and_then(Value::as_str)
            .expect("feedback loop should describe requirement-to-check coverage");
        // Case-insensitive: the description opens the sentence with
        // "Requirement-to-check", so a case-sensitive match broke when the
        // wording moved to the front of the string (issue #730).
        let feedback_description_lower = feedback_description.to_ascii_lowercase();
        assert!(
            feedback_description_lower.contains("requirement-to-check"),
            "feedback_loop description omitted the requirement-to-check framing: {feedback_description}"
        );
        assert!(
            feedback_description_lower.contains("explicit observation or check"),
            "feedback_loop description omitted per-requirement check coverage: {feedback_description}"
        );
        // Per-requirement check guidance lives in the calibration doc's
        // feedback-loop sections, not in the category-only gate message.
        assert!(
            !alignment_description
                .to_ascii_lowercase()
                .contains("threshold")
        );

        let ownership_description = goal_props["delivery_state"]
            .get("description")
            .and_then(Value::as_str)
            .expect("delivery state should have a neutral description");
        assert!(ownership_description.contains("toward the user's outcome"));
        assert!(!ownership_description.contains("90"));
        assert!(
            !ownership_description
                .to_ascii_lowercase()
                .contains("threshold")
        );

        let loop_description = goal_props["closed_feedback_loop"]
            .get("description")
            .and_then(Value::as_str)
            .expect("closed feedback loop should describe the assessment neutrally");
        assert!(!loop_description.to_ascii_lowercase().contains("threshold"));

        let model_visible_schema = serde_json::to_string(&schema)
            .expect("todo schema should serialize")
            .to_ascii_lowercase();
        for disclosure in [
            "threshold",
            "quality gate",
            "internal quality check",
            "not jump",
            "test that passes",
            "isn't high enough",
        ] {
            assert!(
                !model_visible_schema.contains(disclosure),
                "model-visible todo schema disclosed calibration wording: {disclosure}"
            );
        }
        for required_guidance in [
            "integration boundaries",
            "edge cases",
            "packaging",
            "likely failure modes",
        ] {
            assert!(
                model_visible_schema.contains(required_guidance),
                "todo schema omitted generic validation guidance: {required_guidance}"
            );
        }
        for domain_hint in [
            "visual quality",
            "screenshot",
            "browser",
            "viewport",
            "console error",
        ] {
            assert!(
                !model_visible_schema.contains(domain_hint),
                "model-visible todo schema biased visual-work feedback: {domain_hint}"
            );
        }
    }

    fn parse(input: Value) -> Result<TodoInput> {
        parse_todo_input(input)
    }

    #[test]
    fn accepts_stringified_todos_array() {
        let input = json!({
            "todos": "[{\"content\":\"a\",\"status\":\"pending\",\"priority\":\"high\",\"id\":\"1\",\"confidence\":90}]"
        });
        let parsed = parse(input).expect("stringified todos array should parse");
        let todos = parsed.todos.expect("todos present");
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].content, "a");
        assert_eq!(
            todos[0].confidence,
            Some(crate::todo::ConfidenceState::Plausible)
        );
    }

    #[test]
    fn accepts_stringified_todo_items_and_string_confidence() {
        let input = json!({
            "todos": [
                "{\"content\":\"b\",\"status\":\"completed\",\"priority\":\"low\",\"id\":\"2\",\"confidence\":\"85\",\"completion_confidence\":\"95\"}",
                {"content": "c", "status": "pending", "priority": "high", "id": "3", "confidence": "70"}
            ]
        });
        let parsed = parse(input).expect("string-coerced items should parse");
        let todos = parsed.todos.expect("todos present");
        assert_eq!(todos.len(), 2);
        assert_eq!(
            todos[0].confidence,
            Some(crate::todo::ConfidenceState::Plausible)
        );
        assert_eq!(
            todos[0].completion_confidence,
            Some(crate::todo::ConfidenceState::Plausible)
        );
        assert_eq!(
            todos[1].confidence,
            Some(crate::todo::ConfidenceState::Plausible)
        );
    }

    #[test]
    fn normalizes_natural_and_case_varied_todo_statuses() {
        let parsed = parse(json!({
            "todos": [
                {"content": "a", "status": "done", "priority": "high", "id": "1", "confidence": "verified"},
                {"content": "b", "status": " Finished ", "priority": "low", "id": "2", "confidence": "validated"},
                {"content": "c", "status": "Canceled", "priority": "low", "id": "3", "confidence": "plausible"}
            ]
        }))
        .expect("status synonyms should parse");
        let statuses: Vec<_> = parsed
            .todos
            .expect("todos present")
            .into_iter()
            .map(|todo| todo.status)
            .collect();
        assert_eq!(statuses, ["completed", "completed", "cancelled"]);
    }

    #[test]
    fn rejects_unknown_todo_statuses_with_valid_vocabulary() {
        let error = parse(json!({
            "todos": [
                {"content": "a", "status": "blocked", "priority": "high", "id": "1", "confidence": "plausible"}
            ]
        }))
        .err()
        .expect("unknown status should be rejected");
        let message = error.to_string();
        assert!(message.contains("invalid todo status \"blocked\""));
        assert!(message.contains("pending, in_progress, completed, cancelled"));
    }

    #[test]
    fn accepts_float_confidence_and_empty_string_as_none() {
        let input = json!({
            "todos": [
                {"content": "d", "status": "pending", "priority": "high", "id": "4", "confidence": 90.0, "completion_confidence": ""}
            ]
        });
        let parsed = parse(input).expect("float confidence should parse");
        let todos = parsed.todos.expect("todos present");
        assert_eq!(
            todos[0].confidence,
            Some(crate::todo::ConfidenceState::Plausible)
        );
        assert_eq!(todos[0].completion_confidence, None);
    }

    #[test]
    fn empty_string_todos_means_read() {
        let parsed = parse(json!({"todos": ""})).expect("empty string should parse");
        assert!(parsed.todos.is_none());
    }

    #[test]
    fn native_input_still_parses() {
        let input = json!({
            "todos": [
                {"content": "e", "status": "pending", "priority": "high", "id": "5", "confidence": 80}
            ]
        });
        let parsed = parse(input).expect("native input should parse");
        assert_eq!(
            parsed.todos.expect("todos present")[0].confidence,
            Some(crate::todo::ConfidenceState::Plausible)
        );
    }

    #[test]
    fn accepts_goals_and_plan_including_string_coercion() {
        let input = json!({
            "plan": {"user_intention": "make repository search feel instant", "understands_user_intent": "97"},
            "goals": [
                {"group": "optimize grep", "closed_feedback_loop": "95", "feedback_loop": "run the grep benchmark and compare p50"},
                {"closed_feedback_loop": 20}
            ]
        });
        let parsed = parse(input).expect("goals and plan should parse");
        let plan = parsed.plan.expect("plan present");
        assert_eq!(
            plan.understands_user_intent,
            Some(crate::todo::IntentUnderstanding::Clear)
        );
        assert_eq!(
            plan.user_intention.as_deref(),
            Some("make repository search feel instant")
        );
        let goals = parsed.goals.expect("goals present");
        assert_eq!(
            goals[0].closed_feedback_loop,
            Some(crate::todo::FeedbackLoopState::Strong)
        );
        assert_eq!(
            goals[0].feedback_loop.as_deref(),
            Some("run the grep benchmark and compare p50")
        );
        // Runtime parsing remains backward-compatible with stored or older
        // provider payloads even though the advertised schema requires the field.
        assert_eq!(goals[1].feedback_loop, None);
        assert_eq!(goals[1].group, None);
    }

    #[test]
    fn stringified_plan_object_is_accepted() {
        let parsed = parse(json!({
            "plan": "{\"user_intention\":\"ship it\",\"understands_user_intent\":\"96\"}"
        }))
        .expect("stringified plan should parse");
        let plan = parsed.plan.expect("plan present");
        assert_eq!(plan.user_intention.as_deref(), Some("ship it"));
        assert_eq!(
            plan.understands_user_intent,
            Some(crate::todo::IntentUnderstanding::Clear)
        );
    }

    #[test]
    fn accepts_legacy_plan_alignment_key_but_serializes_the_new_name() {
        let parsed = parse(json!({
            "plan": {"user_intention_alignment": "97"}
        }))
        .expect("legacy alignment key should remain readable");
        let plan = parsed.plan.expect("plan present");
        assert_eq!(
            plan.understands_user_intent,
            Some(crate::todo::IntentUnderstanding::Clear)
        );

        let serialized = serde_json::to_value(plan).expect("plan should serialize");
        assert_eq!(serialized["understands_user_intent"], "clear");
        assert!(serialized.get("user_intention_alignment").is_none());

        let legacy_field: TodoPlanField = serde_json::from_str("\"user_intention_alignment\"")
            .expect("legacy plan-change field should deserialize");
        assert_eq!(legacy_field, TodoPlanField::UnderstandsUserIntent);
        assert_eq!(
            serde_json::to_string(&legacy_field).expect("plan field should serialize"),
            "\"understands_user_intent\""
        );
    }

    fn goal(group: Option<&str>, state: crate::todo::FeedbackLoopState) -> TodoGoal {
        TodoGoal {
            group: group.map(str::to_string),
            closed_feedback_loop: Some(state),
            feedback_loop_relevance: Some(crate::todo::FeedbackLoopRelevance::Representative),
            feedback_loop_coverage: Some(crate::todo::FeedbackLoopCoverage::MainPaths),
            feedback_loop_traceability: Some(crate::todo::FeedbackLoopTraceability::Complete),
            ..Default::default()
        }
    }

    /// A plan whose intent assessment clears the private gate, so goal-level
    /// tests observe only closed feedback loop behavior.
    fn aligned_plan() -> TodoPlan {
        TodoPlan {
            user_intention: Some("understood".to_string()),
            understands_user_intent: Some(crate::todo::IntentUnderstanding::Complete),
            understands_user_intent_history: vec![crate::todo::IntentUnderstanding::Complete],
        }
    }

    fn todo_in_group(group: Option<&str>, id: &str) -> TodoItem {
        TodoItem {
            content: format!("task {id}"),
            status: "pending".to_string(),
            priority: "medium".to_string(),
            id: id.to_string(),
            group: group.map(str::to_string),
            ..Default::default()
        }
    }

    #[test]
    fn prune_orphaned_goals_drops_goals_without_live_todos() {
        let goals = vec![
            goal(Some("old task"), crate::todo::FeedbackLoopState::Weak),
            goal(Some("new task"), crate::todo::FeedbackLoopState::Strong),
        ];
        let todos = vec![todo_in_group(Some("new task"), "1")];

        let pruned = prune_orphaned_goals(goals, &todos);

        assert_eq!(pruned.len(), 1);
        assert_eq!(pruned[0].group.as_deref(), Some("new task"));
    }

    #[test]
    fn prune_orphaned_goals_keeps_ungrouped_goal_for_flat_list() {
        let goals = vec![goal(None, crate::todo::FeedbackLoopState::Usable)];
        let todos = vec![todo_in_group(None, "1")];

        assert_eq!(prune_orphaned_goals(goals, &todos).len(), 1);
    }

    #[test]
    fn prune_orphaned_goals_keeps_everything_when_todo_list_is_empty() {
        // A goals-only write with no stored todos must not lose assessments.
        let goals = vec![
            goal(Some("a"), crate::todo::FeedbackLoopState::Absent),
            goal(None, crate::todo::FeedbackLoopState::Weak),
        ];
        assert_eq!(prune_orphaned_goals(goals, &[]).len(), 2);
    }

    #[test]
    fn merge_goals_retains_unmentioned_goals() {
        let stored = vec![
            goal(Some("a"), crate::todo::FeedbackLoopState::Weak),
            goal(Some("b"), crate::todo::FeedbackLoopState::Strong),
        ];
        // Rewrite goal 'a', leave 'b' alone.
        let merged = merge_goals(
            &stored,
            Some(vec![goal(
                Some(" a "),
                crate::todo::FeedbackLoopState::Weak,
            )]),
        );
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].group.as_deref(), Some("a"));
        assert_eq!(
            merged[0].closed_feedback_loop,
            Some(crate::todo::FeedbackLoopState::Weak)
        );
        assert_eq!(merged[1].group.as_deref(), Some("b"));
        // No incoming goals: stored goals unchanged.
        assert_eq!(merge_goals(&stored, None).len(), 2);
    }

    #[test]
    fn merge_plan_retains_stored_intent_when_update_omits_fields() {
        let stored = TodoPlan {
            user_intention: Some("make search feel instant".to_string()),
            understands_user_intent: Some(crate::todo::IntentUnderstanding::Partial),
            understands_user_intent_history: vec![crate::todo::IntentUnderstanding::Partial],
        };

        let merged = merge_plan(
            &stored,
            Some(TodoPlan {
                user_intention: None,
                understands_user_intent: Some(crate::todo::IntentUnderstanding::Partial),
                ..Default::default()
            }),
        );
        assert_eq!(
            merged.user_intention.as_deref(),
            Some("make search feel instant")
        );
        assert_eq!(
            merged.understands_user_intent,
            Some(crate::todo::IntentUnderstanding::Partial)
        );

        // An omitted plan leaves the stored assessment untouched.
        assert_eq!(merge_plan(&stored, None), stored);
    }

    #[test]
    fn plan_change_reports_only_updated_intent_fields() {
        let before = aligned_plan();
        let after = TodoPlan {
            user_intention: Some("understood better".to_string()),
            ..before.clone()
        };

        let change = plan_change(&before, &after).expect("intent change should be reported");
        assert_eq!(change.fields, vec![TodoPlanField::UserIntention]);
        assert_eq!(change.before.as_ref(), Some(&before));
        assert_eq!(change.after.as_ref(), Some(&after));
        assert!(plan_change(&before, &before).is_none());
    }

    fn test_ctx(session_id: &str) -> ToolContext {
        ToolContext {
            session_id: session_id.to_string(),
            message_id: session_id.to_string(),
            tool_call_id: "call".to_string(),
            working_dir: None,
            stdin_request_tx: None,
            graceful_shutdown_signal: None,
            execution_mode: crate::tool::ToolExecutionMode::Direct,
        }
    }

    /// Issue #695, the visibly-stale case. The todos panel renders the
    /// ungrouped goal unconditionally (not only as a group header), so an
    /// ungrouped goal left over from a previous flat todo list is exactly what
    /// the reporter saw frozen in the panel.
    #[tokio::test]
    async fn an_ungrouped_goal_does_not_survive_into_a_grouped_next_task() {
        let _guard = crate::storage::lock_test_env();
        let previous_home = std::env::var_os("KCODE_HOME");
        let dir = tempfile::TempDir::new().expect("tempdir");
        crate::env::set_var("KCODE_HOME", dir.path());
        let session = "issue-695-ungrouped";
        let tool = TodoTool::new();

        // Task one: a flat (ungrouped) list, so its goal is the ungrouped one.
        tool.execute(
            json!({
                "todos": [{
                    "content": "flat task one", "status": "in_progress",
                    "priority": "high", "id": "t1", "confidence": 70,
                }],
                "plan": {"user_intention": "do task one", "understands_user_intent": 97},
                "goals": [{"closed_feedback_loop": 97, "feedback_loop": "ran the checks"}],
            }),
            test_ctx(session),
        )
        .await
        .expect("first write");
        let stored = load_goals(session).expect("goals");
        assert_eq!(stored.len(), 1);
        assert!(
            stored[0].group.is_none(),
            "task one goal is the ungrouped one"
        );

        // Task two: a grouped list. The ungrouped goal now describes nothing.
        tool.execute(
            json!({
                "todos": [{
                    "content": "task two", "status": "in_progress", "priority": "high",
                    "id": "t2", "group": "second task", "confidence": 70,
                }],
                "goals": [{"group": "second task", "closed_feedback_loop": 80,
                           "feedback_loop": "run the new checks"}],
            }),
            test_ctx(session),
        )
        .await
        .expect("second write");

        let goals = load_goals(session).expect("goals");
        assert!(
            !goals.iter().any(|goal| goal.group.is_none()),
            "the stale ungrouped goal must not stay in the panel: {goals:?}"
        );
        assert_eq!(goals.len(), 1);
        assert_eq!(goals[0].group.as_deref(), Some("second task"));

        if let Some(home) = previous_home {
            crate::env::set_var("KCODE_HOME", home);
        } else {
            crate::env::remove_var("KCODE_HOME");
        }
    }

    /// Issue #695, end to end through the real tool: finish task one, then
    /// start task two. What the todos panel renders (stored todos + goals) must
    /// describe task two only, with no leftovers from task one.
    #[tokio::test]
    async fn moving_to_a_new_task_replaces_what_the_todos_panel_shows() {
        let _guard = crate::storage::lock_test_env();
        let previous_home = std::env::var_os("KCODE_HOME");
        let dir = tempfile::TempDir::new().expect("tempdir");
        crate::env::set_var("KCODE_HOME", dir.path());
        let session = "issue-695-new-task";
        let tool = TodoTool::new();

        // Task one, completed. `end_to_end_ownership` clears the completion
        // gate so the write is actually stored.
        tool.execute(
            json!({
                "todos": [{
                    "content": "task one",
                    "status": "completed",
                    "priority": "high",
                    "id": "t1",
                    "group": "first task",
                    "confidence": 90,
                    "completion_confidence": 97,
                }],
                "plan": {"user_intention": "do task one", "understands_user_intent": 97},
                "goals": [{
                    "group": "first task",
                    "closed_feedback_loop": 97,
                    "end_to_end_ownership": 97,
                    "feedback_loop": "ran the checks",
                }],
            }),
            test_ctx(session),
        )
        .await
        .expect("first task write should succeed");
        assert_eq!(load_goals(session).expect("goals").len(), 1);

        // Task two: a fresh todo list in a new group.
        tool.execute(
            json!({
                "todos": [{
                    "content": "task two",
                    "status": "in_progress",
                    "priority": "high",
                    "id": "t2",
                    "group": "second task",
                    "confidence": 70,
                }],
                "goals": [{
                    "group": "second task",
                    "closed_feedback_loop": 80,
                    "feedback_loop": "run the new checks",
                }],
            }),
            test_ctx(session),
        )
        .await
        .expect("second task write should succeed");

        let todos = load_todos(session).expect("todos");
        assert_eq!(todos.len(), 1, "panel must show only the current task");
        assert_eq!(todos[0].group.as_deref(), Some("second task"));

        let goals = load_goals(session).expect("goals");
        assert_eq!(
            goals.len(),
            1,
            "the finished task's goal must not linger in the panel: {goals:?}"
        );
        assert_eq!(goals[0].group.as_deref(), Some("second task"));

        if let Some(home) = previous_home {
            crate::env::set_var("KCODE_HOME", home);
        } else {
            crate::env::remove_var("KCODE_HOME");
        }
    }

    #[tokio::test]
    async fn low_ownership_completion_is_saved_without_mid_write_rejection() {
        let _guard = crate::storage::lock_test_env();
        let previous_home = std::env::var_os("KCODE_HOME");
        let dir = tempfile::TempDir::new().expect("tempdir");
        crate::env::set_var("KCODE_HOME", dir.path());
        let session = "ownership-save-before-turn-gate";

        TodoTool::new()
            .execute(
                json!({
                    "todos": [{
                        "content": "ship the complete workflow",
                        "status": "completed",
                        "priority": "high",
                        "id": "ship",
                        "group": "release",
                        "confidence": 100,
                        "completion_confidence": 100,
                    }],
                    "goals": [{
                        "group": "release",
                        "closed_feedback_loop": 100,
                        "feedback_loop": "run the end-to-end release check",
                        "feedback_loop_relevance": "indirect",
                        "feedback_loop_coverage": "narrow",
                        "end_to_end_ownership": 95,
                    }],
                }),
                test_ctx(session),
            )
            .await
            .expect("low ownership must not reject the todo write");

        let saved = load_todos(session).expect("completed todo should be persisted");
        assert_eq!(saved.len(), 1);
        assert_eq!(saved[0].status, "completed");
        let saved_goals = load_goals(session).expect("goal should be persisted");
        let saved_goal = &saved_goals[0];
        assert_eq!(
            saved_goal.delivery_state,
            Some(crate::todo::DeliveryState::WorkflowValidated)
        );
        assert_eq!(
            saved_goal.feedback_loop_relevance,
            Some(crate::todo::FeedbackLoopRelevance::Indirect)
        );
        assert_eq!(
            saved_goal.feedback_loop_coverage,
            Some(crate::todo::FeedbackLoopCoverage::Narrow)
        );

        match previous_home {
            Some(value) => crate::env::set_var("KCODE_HOME", value),
            None => crate::env::remove_var("KCODE_HOME"),
        }
    }

    #[test]
    fn goal_changes_include_only_updated_quality_fields() {
        let before = TodoGoal {
            group: Some("search".to_string()),
            closed_feedback_loop: Some(crate::todo::FeedbackLoopState::Strong),
            feedback_loop: Some("Run one benchmark".to_string()),
            feedback_loop_relevance: Some(crate::todo::FeedbackLoopRelevance::Indirect),
            feedback_loop_coverage: Some(crate::todo::FeedbackLoopCoverage::Narrow),
            delivery_state: None,
            ..Default::default()
        };
        let after = TodoGoal {
            closed_feedback_loop: Some(crate::todo::FeedbackLoopState::Closed),
            feedback_loop: Some("Run five benchmarks and compare p50".to_string()),
            feedback_loop_relevance: Some(crate::todo::FeedbackLoopRelevance::Representative),
            feedback_loop_coverage: Some(crate::todo::FeedbackLoopCoverage::MainPaths),
            ..before.clone()
        };

        let changes = goal_changes(std::slice::from_ref(&before), std::slice::from_ref(&after));

        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].before.as_ref(), Some(&before));
        assert_eq!(changes[0].after.as_ref(), Some(&after));
        assert_eq!(
            changes[0].fields,
            vec![
                TodoGoalField::ClosedFeedbackLoop,
                TodoGoalField::FeedbackLoop,
                TodoGoalField::FeedbackLoopRelevance,
                TodoGoalField::FeedbackLoopCoverage,
            ]
        );
    }

    /// Tool-owned histories are the substrate the turn-end digest reasons over,
    /// so a model-supplied trail must not be able to fabricate a climb.
    #[test]
    fn goal_score_histories_are_tool_maintained() {
        let stored_goals = merge_goals(
            &[],
            Some(vec![TodoGoal {
                group: Some("perf".to_string()),
                closed_feedback_loop: Some(crate::todo::FeedbackLoopState::Usable),
                feedback_loop_relevance: Some(crate::todo::FeedbackLoopRelevance::Indirect),
                feedback_loop_coverage: Some(crate::todo::FeedbackLoopCoverage::Narrow),
                ..Default::default()
            }]),
        );
        assert_eq!(
            stored_goals[0].closed_feedback_loop_history,
            vec![crate::todo::FeedbackLoopState::Usable]
        );
        let merged_goals = merge_goals(
            &stored_goals,
            Some(vec![TodoGoal {
                group: Some("perf".to_string()),
                closed_feedback_loop: Some(crate::todo::FeedbackLoopState::Strong),
                feedback_loop_relevance: Some(
                    crate::todo::FeedbackLoopRelevance::AcceptanceAligned,
                ),
                feedback_loop_relevance_history: vec![
                    crate::todo::FeedbackLoopRelevance::AcceptanceAligned,
                ],
                feedback_loop_coverage: Some(
                    crate::todo::FeedbackLoopCoverage::EdgeAndIntegrationPaths,
                ),
                feedback_loop_coverage_history: vec![
                    crate::todo::FeedbackLoopCoverage::EdgeAndIntegrationPaths,
                ],
                ..Default::default()
            }]),
        );
        assert_eq!(
            merged_goals[0].closed_feedback_loop_history,
            vec![
                crate::todo::FeedbackLoopState::Usable,
                crate::todo::FeedbackLoopState::Strong
            ]
        );
        assert_eq!(
            merged_goals[0].feedback_loop_relevance_history,
            vec![
                crate::todo::FeedbackLoopRelevance::Indirect,
                crate::todo::FeedbackLoopRelevance::AcceptanceAligned,
            ]
        );
        assert_eq!(
            merged_goals[0].feedback_loop_coverage_history,
            vec![
                crate::todo::FeedbackLoopCoverage::Narrow,
                crate::todo::FeedbackLoopCoverage::EdgeAndIntegrationPaths,
            ]
        );
    }

    /// A write that revises one assessment must not erase the others, or the
    /// digest would read a stale `None` and re-raise a resolved point.
    #[test]
    fn omitted_goal_fields_inherit_the_stored_assessment() {
        let stored = merge_goals(
            &[],
            Some(vec![TodoGoal {
                group: Some("perf".to_string()),
                closed_feedback_loop: Some(crate::todo::FeedbackLoopState::Closed),
                feedback_loop: Some("cargo bench".to_string()),
                feedback_loop_relevance: Some(crate::todo::FeedbackLoopRelevance::Representative),
                feedback_loop_coverage: Some(crate::todo::FeedbackLoopCoverage::MainPaths),
                delivery_state: Some(crate::todo::DeliveryState::OutcomeDelivered),
                ..Default::default()
            }]),
        );
        let merged = merge_goals(
            &stored,
            Some(vec![TodoGoal {
                group: Some("perf".to_string()),
                ..Default::default()
            }]),
        );
        assert_eq!(
            merged[0].closed_feedback_loop,
            Some(crate::todo::FeedbackLoopState::Closed)
        );
        assert_eq!(merged[0].feedback_loop.as_deref(), Some("cargo bench"));
        assert_eq!(
            merged[0].feedback_loop_relevance,
            Some(crate::todo::FeedbackLoopRelevance::Representative)
        );
        assert_eq!(
            merged[0].feedback_loop_coverage,
            Some(crate::todo::FeedbackLoopCoverage::MainPaths)
        );
        assert_eq!(
            merged[0].delivery_state,
            Some(crate::todo::DeliveryState::OutcomeDelivered)
        );
    }

    #[test]
    fn garbage_string_still_errors() {
        assert!(parse(json!({"todos": "not json at all"})).is_err());
    }

    /// Sessions and model calls written before the rename carry
    /// `hill_climbability`. Those must keep loading, or resuming an old session
    /// silently drops its goal assessments and re-raises resolved gate points.
    #[test]
    fn pre_rename_hill_climbability_keys_still_load() {
        let goal: crate::todo::TodoGoal = serde_json::from_value(json!({
            "group": "optimize grep",
            "hill_climbability": 91,
            "hill_climbability_history": [70, 91],
            "feedback_loop": "cargo bench grep"
        }))
        .expect("the pre-rename key must still deserialize");
        assert_eq!(
            goal.closed_feedback_loop,
            Some(crate::todo::FeedbackLoopState::Strong)
        );
        assert_eq!(
            goal.closed_feedback_loop_history,
            vec![
                crate::todo::FeedbackLoopState::Usable,
                crate::todo::FeedbackLoopState::Strong
            ]
        );

        let goals = parse(json!({
            "goals": [{"group": "optimize grep", "hill_climbability": "88", "feedback_loop": "bench"}]
        }))
        .expect("a pre-rename tool call must still parse")
        .goals
        .expect("goals should be present");
        assert_eq!(
            goals[0].closed_feedback_loop,
            Some(crate::todo::FeedbackLoopState::Strong)
        );
    }

    use crate::todo::ConfidenceState as CS;

    fn history_todo(id: &str, confidence: Option<CS>, history: Vec<CS>) -> TodoItem {
        TodoItem {
            id: id.to_string(),
            content: format!("todo {id}"),
            status: "in_progress".to_string(),
            priority: "high".to_string(),
            confidence,
            confidence_history: history,
            ..Default::default()
        }
    }

    #[test]
    fn confidence_history_appends_changes_and_skips_repeats() {
        let previous = vec![history_todo("1", Some(CS::Plausible), vec![CS::Plausible])];
        // Same confidence again: no new entry.
        let mut incoming = vec![history_todo("1", Some(CS::Plausible), Vec::new())];
        merge_confidence_history(&previous, &mut incoming);
        assert_eq!(incoming[0].confidence_history, vec![CS::Plausible]);
        // Raised confidence: appended.
        let mut incoming = vec![history_todo("1", Some(CS::Validated), Vec::new())];
        merge_confidence_history(&previous, &mut incoming);
        assert_eq!(
            incoming[0].confidence_history,
            vec![CS::Plausible, CS::Validated]
        );
    }

    #[test]
    fn confidence_history_records_completion_confidence() {
        let previous = vec![history_todo("1", Some(CS::Plausible), vec![CS::Plausible])];
        let mut done = history_todo("1", Some(CS::Verified), Vec::new());
        done.status = "completed".to_string();
        done.completion_confidence = Some(CS::Verified);
        let mut incoming = vec![done];
        merge_confidence_history(&previous, &mut incoming);
        // 75 (planning) -> 100 (final bulk stamp): the spike stays visible.
        assert_eq!(
            incoming[0].confidence_history,
            vec![CS::Plausible, CS::Verified]
        );
    }

    #[test]
    fn completion_write_contributes_only_one_final_confidence_observation() {
        let previous = vec![history_todo("1", Some(CS::Plausible), vec![CS::Plausible])];
        let mut done = history_todo("1", Some(CS::Plausible), Vec::new());
        done.status = "completed".to_string();
        done.completion_confidence = Some(CS::Verified);

        let mut incoming = vec![done];
        merge_confidence_history(&previous, &mut incoming);

        assert_eq!(
            incoming[0].confidence_history,
            vec![CS::Plausible, CS::Verified]
        );
    }

    #[test]
    fn confidence_history_seeds_legacy_todos_before_completion() {
        let previous = vec![history_todo("1", Some(CS::Plausible), Vec::new())];
        let mut done = history_todo("1", Some(CS::Plausible), Vec::new());
        done.status = "completed".to_string();
        done.completion_confidence = Some(CS::Verified);

        let mut incoming = vec![done];
        merge_confidence_history(&previous, &mut incoming);

        assert_eq!(
            incoming[0].confidence_history,
            vec![CS::Plausible, CS::Verified]
        );
    }

    #[test]
    fn confidence_history_ignores_model_supplied_history_for_new_todos() {
        let mut incoming = vec![history_todo(
            "9",
            Some(CS::Plausible),
            vec![CS::Speculative, CS::Verified],
        )];
        merge_confidence_history(&[], &mut incoming);
        assert_eq!(incoming[0].confidence_history, vec![CS::Plausible]);
    }
}
