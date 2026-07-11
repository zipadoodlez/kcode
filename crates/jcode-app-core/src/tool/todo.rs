use super::{Tool, ToolContext, ToolOutput};
use crate::bus::{Bus, BusEvent, TodoEvent};
use crate::todo::{
    LOW_HILL_CLIMBABILITY, TodoGoal, TodoItem, load_goals, load_todos, save_goals, save_todos,
};
use anyhow::Result;
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
/// The model reports `confidence` (and `completion_confidence` at completion)
/// as scalar fields; this keeps an append-only trail of every distinct value a
/// todo has carried so downstream consumers (auto-poke spike checks, analysis)
/// can distinguish a stepped, evidence-driven rise (75 -> 85 -> 95 -> 100)
/// from a bulk end-of-task stamp (75 -> 100). Model-supplied
/// `confidence_history` is ignored: the tool owns this field.
fn merge_confidence_history(previous: &[TodoItem], incoming: &mut [TodoItem]) {
    let prior: HashMap<&str, &TodoItem> = previous
        .iter()
        .map(|todo| (todo.id.as_str(), todo))
        .collect();
    for todo in incoming.iter_mut() {
        let mut history = prior
            .get(todo.id.as_str())
            .map(|prev| prev.confidence_history.clone())
            .unwrap_or_default();
        for value in [todo.confidence, todo.completion_confidence]
            .into_iter()
            .flatten()
        {
            if history.last() != Some(&value) {
                history.push(value);
            }
        }
        todo.confidence_history = history;
    }
}

#[derive(Deserialize)]
struct TodoInput {
    todos: Option<Vec<TodoItem>>,
    goals: Option<Vec<TodoGoal>>,
}

/// Normalize a goal's group label: trimmed, with empty/whitespace collapsed
/// to `None` (the implicit goal of an ungrouped list).
fn goal_group_key(group: Option<&str>) -> Option<String> {
    group
        .map(str::trim)
        .filter(|group| !group.is_empty())
        .map(str::to_string)
}

/// Merge incoming goal assessments with the stored ones.
///
/// Incoming goals win per group key; stored goals for groups the write does
/// not mention are retained (a todo update should not silently discard goal
/// assessments).
fn merge_goals(stored: &[TodoGoal], incoming: Option<Vec<TodoGoal>>) -> Vec<TodoGoal> {
    let Some(incoming) = incoming else {
        return stored.to_vec();
    };
    let mut merged: Vec<TodoGoal> = Vec::new();
    for mut goal in incoming {
        goal.group = goal_group_key(goal.group.as_deref());
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

/// Reframe nudges for goals that score low on hill-climbability.
///
/// A low score means there is no credible metric to iterate against, so the
/// objective must be reframed into something measurable. The nudge is
/// intentionally returned on every applicable todo write until the goal reaches
/// the threshold or its work closes.
fn take_reframe_nudges(goals: &[TodoGoal], todos: &[TodoItem]) -> Vec<String> {
    let mut nudges = Vec::new();
    for goal in goals {
        let Some(score) = goal.hill_climbability else {
            continue;
        };
        if score >= LOW_HILL_CLIMBABILITY {
            continue;
        }
        let group_open = todos.iter().any(|todo| {
            goal_group_key(todo.group.as_deref()) == goal.group
                && todo.status != "completed"
                && todo.status != "cancelled"
        });
        if !group_open {
            continue;
        }
        let label = goal.group.as_deref().unwrap_or("the current goal");
        nudges.push(format!(
            "Goal '{}' has low hill-climbability ({}). Reframe it into a quantifiable, \
             verifiable objective (set the goal's `objective`, e.g. a metric plus target, and \
             build a harness that measures it).",
            label, score
        ));
    }
    nudges
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
                for key in ["confidence", "completion_confidence", "hill_climbability"] {
                    if let Some(value) = fields.get_mut(key) {
                        coerce_value_to_integer(value);
                    }
                }
            }
        }
    }
    input
}

/// Coerce a numeric string (`"90"`) or whole float (`90.0`) to a JSON integer,
/// and an empty string to `null`. Leaves anything else untouched so strict
/// deserialization can report a precise error.
fn coerce_value_to_integer(value: &mut Value) {
    match value {
        Value::String(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                *value = Value::Null;
            } else if let Ok(parsed) = trimmed.parse::<u64>() {
                *value = Value::from(parsed);
            }
        }
        Value::Number(num) => {
            if num.as_u64().is_none()
                && let Some(float) = num.as_f64()
                && float.fract() == 0.0
                && (0.0..=u64::MAX as f64).contains(&float)
            {
                *value = Value::from(float as u64);
            }
        }
        _ => {}
    }
}

#[async_trait]
impl Tool for TodoTool {
    fn name(&self) -> &str {
        "todo"
    }

    fn description(&self) -> &str {
        "Read or update the todo list. Include confidence for each item, update it as evidence accumulates while working, and include completion_confidence when marking an item completed. Rate each goal's hill_climbability via the goals param: how measurable and iterable progress toward it is."
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
                                "description": "Status."
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
                                "description": "Optional group label. Todos sharing a group render together under one header. Use one group per coherent goal (e.g. 'optimize rendering'). When the user steers into new work, start a new group instead of renaming the existing one. Omit for an ungrouped flat list."
                            },
                            "confidence": {
                                "type": "integer",
                                "minimum": 0,
                                "maximum": 100,
                                "description": "Current confidence, 0-100, that this todo can be completed correctly. Set when creating the todo, and update it as evidence accumulates while working (each validation or test that passes justifies a step up). Confidence should rise in evidence-backed steps, not jump to 100 at the end."
                            },
                            "completion_confidence": {
                                "type": "integer",
                                "minimum": 0,
                                "maximum": 100,
                                "description": "Confidence, 0-100, that this todo is correctly completed. Set when marking the todo completed; omit until then."
                            }
                        }
                    }
                },
                "goals": {
                    "type": "array",
                    "description": "Goal-level assessments, one per todo group (use group: null for an ungrouped flat list, which is one implicit goal). Rate how hill-climbable each goal is and state its measurable objective when one exists. Stored goals for groups not mentioned in a write are retained.",
                    "items": {
                        "type": "object",
                        "required": ["hill_climbability"],
                        "properties": {
                            "group": {
                                "type": "string",
                                "description": "Group label this goal describes. Omit or null for the ungrouped list."
                            },
                            "hill_climbability": {
                                "type": "integer",
                                "minimum": 0,
                                "maximum": 100,
                                "description": "How hill-climbable this goal is, 0-100: can progress be measured against a quantifiable, verifiable objective and iterated on? Scores below 90 trigger recurring reframe guidance on every applicable todo write. High scores should have a clear metric and stated objective (e.g. p50 grep latency under 50ms, all targeted tests pass)."
                            },
                            "objective": {
                                "type": "string",
                                "description": "The measurable objective progress climbs toward, e.g. 'p50 grep latency under 50ms on the repo corpus'. State one whenever it exists; a high hill_climbability without an objective is not credible."
                            }
                        }
                    }
                }
            }
        })
    }

    async fn execute(&self, input: Value, ctx: ToolContext) -> Result<ToolOutput> {
        let params: TodoInput = serde_json::from_value(normalize_todo_input(input))?;
        let operation = if params.todos.is_some() || params.goals.is_some() {
            "write"
        } else {
            "read"
        };
        let result = if params.todos.is_some() || params.goals.is_some() {
            // Goals-only writes keep the stored todo list.
            let previous = load_todos(&ctx.session_id).unwrap_or_default();
            let mut todos = params.todos.unwrap_or_else(|| previous.clone());
            merge_confidence_history(&previous, &mut todos);
            save_todos(&ctx.session_id, &todos).and_then(|_| {
                let stored_goals = load_goals(&ctx.session_id).unwrap_or_default();
                let goals = merge_goals(&stored_goals, params.goals);
                let nudges = take_reframe_nudges(&goals, &todos);
                save_goals(&ctx.session_id, &goals)?;

                Bus::global().publish(BusEvent::TodoUpdated(TodoEvent {
                    session_id: ctx.session_id.clone(),
                    todos: todos.clone(),
                }));

                let remaining = todos.iter().filter(|t| t.status != "completed").count();
                let mut text = serde_json::to_string_pretty(&todos)?;
                if !goals.is_empty() {
                    text.push_str("\n\nGoals:\n");
                    text.push_str(&serde_json::to_string_pretty(&goals)?);
                }
                for nudge in &nudges {
                    text.push_str("\n\n");
                    text.push_str(nudge);
                }
                Ok(ToolOutput::new(text)
                    .with_title(format!("{} todos", remaining))
                    .with_metadata(json!({"todos": todos, "goals": goals})))
            })
        } else {
            (|| {
                let todos = load_todos(&ctx.session_id)?;
                let goals = load_goals(&ctx.session_id).unwrap_or_default();
                let remaining = todos.iter().filter(|t| t.status != "completed").count();
                let mut text = serde_json::to_string_pretty(&todos)?;
                if !goals.is_empty() {
                    text.push_str("\n\nGoals:\n");
                    text.push_str(&serde_json::to_string_pretty(&goals)?);
                }
                Ok(ToolOutput::new(text)
                    .with_title(format!("{} todos", remaining))
                    .with_metadata(json!({"todos": todos, "goals": goals})))
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
        assert_eq!(props.len(), 3);
        assert!(props.contains_key("intent"));
        assert!(props.contains_key("todos"));
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
        assert!(!item_props.contains_key("hill_climbability"));

        let goal_props = props["goals"]
            .get("items")
            .and_then(|v| v.get("properties"))
            .and_then(|v| v.as_object())
            .expect("goals should describe item objects");
        assert!(goal_props.contains_key("group"));
        assert!(goal_props.contains_key("hill_climbability"));
        assert!(goal_props.contains_key("objective"));
        assert_eq!(goal_props.len(), 3);
    }

    fn parse(input: Value) -> Result<TodoInput, serde_json::Error> {
        serde_json::from_value(normalize_todo_input(input))
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
        assert_eq!(todos[0].confidence, Some(90));
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
        assert_eq!(todos[0].confidence, Some(85));
        assert_eq!(todos[0].completion_confidence, Some(95));
        assert_eq!(todos[1].confidence, Some(70));
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
        assert_eq!(todos[0].confidence, Some(90));
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
        assert_eq!(parsed.todos.expect("todos present")[0].confidence, Some(80));
    }

    #[test]
    fn accepts_goals_including_string_coercion() {
        let input = json!({
            "goals": [
                {"group": "optimize grep", "hill_climbability": "95", "objective": "p50 under 50ms"},
                {"hill_climbability": 20}
            ]
        });
        let parsed = parse(input).expect("goals should parse");
        let goals = parsed.goals.expect("goals present");
        assert_eq!(goals[0].hill_climbability, Some(95));
        assert_eq!(goals[0].objective.as_deref(), Some("p50 under 50ms"));
        assert_eq!(goals[1].group, None);
    }

    fn goal(group: Option<&str>, score: u8) -> TodoGoal {
        TodoGoal {
            group: group.map(str::to_string),
            hill_climbability: Some(score),
            ..Default::default()
        }
    }

    #[test]
    fn merge_goals_retains_unmentioned_goals() {
        let stored = vec![goal(Some("a"), 20), goal(Some("b"), 90)];
        // Rewrite goal 'a', leave 'b' alone.
        let merged = merge_goals(&stored, Some(vec![goal(Some(" a "), 30)]));
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].group.as_deref(), Some("a"));
        assert_eq!(merged[0].hill_climbability, Some(30));
        assert_eq!(merged[1].group.as_deref(), Some("b"));
        // No incoming goals: stored goals unchanged.
        assert_eq!(merge_goals(&stored, None).len(), 2);
    }

    fn open_todo(group: Option<&str>) -> TodoItem {
        TodoItem {
            id: "t1".to_string(),
            content: "work".to_string(),
            status: "in_progress".to_string(),
            priority: "high".to_string(),
            group: group.map(str::to_string),
            ..Default::default()
        }
    }

    #[test]
    fn reframe_nudge_recurs_for_every_low_open_goal_write() {
        let todos = vec![open_todo(Some("design"))];
        let goals = vec![goal(Some("design"), 89), goal(Some("perf"), 90)];
        let nudges = take_reframe_nudges(&goals, &todos);
        assert_eq!(nudges.len(), 1);
        assert!(nudges[0].contains("design"));
        assert!(!nudges[0].contains("open-ended"));
        assert!(!nudges[0].contains("checkpoints"));
        // A subsequent write receives the same guidance until the score reaches 90.
        assert_eq!(take_reframe_nudges(&goals, &todos).len(), 1);
    }

    #[test]
    fn reframe_nudge_skips_closed_goals() {
        // Low goal whose todos are all completed: nothing to reframe.
        let mut done = open_todo(Some("legacy"));
        done.status = "completed".to_string();
        let goals = vec![goal(Some("legacy"), 10)];
        assert!(take_reframe_nudges(&goals, &[done]).is_empty());
    }

    #[test]
    fn reframe_nudge_covers_ungrouped_implicit_goal() {
        let todos = vec![open_todo(None)];
        let goals = vec![goal(None, 15)];
        let nudges = take_reframe_nudges(&goals, &todos);
        assert_eq!(nudges.len(), 1);
        assert!(nudges[0].contains("the current goal"));
    }

    #[test]
    fn garbage_string_still_errors() {
        assert!(parse(json!({"todos": "not json at all"})).is_err());
    }

    fn history_todo(id: &str, confidence: Option<u8>, history: Vec<u8>) -> TodoItem {
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
        let previous = vec![history_todo("1", Some(75), vec![75])];
        // Same confidence again: no new entry.
        let mut incoming = vec![history_todo("1", Some(75), Vec::new())];
        merge_confidence_history(&previous, &mut incoming);
        assert_eq!(incoming[0].confidence_history, vec![75]);
        // Raised confidence: appended.
        let mut incoming = vec![history_todo("1", Some(90), Vec::new())];
        merge_confidence_history(&previous, &mut incoming);
        assert_eq!(incoming[0].confidence_history, vec![75, 90]);
    }

    #[test]
    fn confidence_history_records_completion_confidence() {
        let previous = vec![history_todo("1", Some(75), vec![75])];
        let mut done = history_todo("1", Some(100), Vec::new());
        done.status = "completed".to_string();
        done.completion_confidence = Some(100);
        let mut incoming = vec![done];
        merge_confidence_history(&previous, &mut incoming);
        // 75 (planning) -> 100 (final bulk stamp): the spike stays visible.
        assert_eq!(incoming[0].confidence_history, vec![75, 100]);
    }

    #[test]
    fn confidence_history_ignores_model_supplied_history_for_new_todos() {
        let mut incoming = vec![history_todo("9", Some(80), vec![1, 2, 3])];
        merge_confidence_history(&[], &mut incoming);
        assert_eq!(incoming[0].confidence_history, vec![80]);
    }
}
