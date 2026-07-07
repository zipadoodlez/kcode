use super::{Tool, ToolContext, ToolOutput};
use crate::bus::{Bus, BusEvent, TodoEvent};
use crate::todo::{TodoItem, load_todos, save_todos};
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
    let Some(todos) = obj.get_mut("todos") else {
        return input;
    };

    // Whole array sent as a stringified JSON blob.
    if let Value::String(raw) = todos {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            *todos = Value::Null;
        } else if let Ok(parsed @ (Value::Array(_) | Value::Null)) =
            serde_json::from_str::<Value>(trimmed)
        {
            *todos = parsed;
        }
    }

    if let Value::Array(items) = todos {
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
        "Read or update the todo list. Include confidence for each item, update it as evidence accumulates while working, and include completion_confidence when marking an item completed. Rate each item's hill_climbability: how measurable and iterable progress on it is."
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
                            },
                            "hill_climbability": {
                                "type": "integer",
                                "minimum": 0,
                                "maximum": 100,
                                "description": "How hill-climbable this task is, 0-100: can progress be measured against a quantifiable, verifiable objective and iterated on? High for tasks with a clear metric (e.g. optimize grep latency, make failing tests pass), low for taste-driven or open-ended tasks (e.g. design an onboarding screen). High scores suggest building a measurement harness and iterating; low scores suggest checkpointing with the user or defining proxy metrics first."
                            }
                        }
                    }
                }
            }
        })
    }

    async fn execute(&self, input: Value, ctx: ToolContext) -> Result<ToolOutput> {
        let params: TodoInput = serde_json::from_value(normalize_todo_input(input))?;
        let operation = if params.todos.is_some() {
            "write"
        } else {
            "read"
        };
        match params.todos {
            Some(mut todos) => {
                let previous = load_todos(&ctx.session_id).unwrap_or_default();
                merge_confidence_history(&previous, &mut todos);
                save_todos(&ctx.session_id, &todos)?;

                Bus::global().publish(BusEvent::TodoUpdated(TodoEvent {
                    session_id: ctx.session_id.clone(),
                    todos: todos.clone(),
                }));

                let remaining = todos.iter().filter(|t| t.status != "completed").count();
                Ok(ToolOutput::new(serde_json::to_string_pretty(&todos)?)
                    .with_title(format!("{} todos", remaining))
                    .with_metadata(json!({"todos": todos})))
            }
            None => {
                let todos = load_todos(&ctx.session_id)?;
                let remaining = todos.iter().filter(|t| t.status != "completed").count();
                Ok(ToolOutput::new(serde_json::to_string_pretty(&todos)?)
                    .with_title(format!("{} todos", remaining))
                    .with_metadata(json!({"todos": todos})))
            }
        }
        .map_err(|err| {
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
        assert_eq!(props.len(), 2);
        assert!(props.contains_key("intent"));
        assert!(props.contains_key("todos"));

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
        assert!(item_props.contains_key("hill_climbability"));
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
    fn accepts_hill_climbability_including_string_coercion() {
        let input = json!({
            "todos": [
                {"content": "f", "status": "pending", "priority": "high", "id": "6", "confidence": 80, "hill_climbability": "95"},
                {"content": "g", "status": "pending", "priority": "low", "id": "7", "confidence": 60, "hill_climbability": 20}
            ]
        });
        let parsed = parse(input).expect("hill_climbability should parse");
        let todos = parsed.todos.expect("todos present");
        assert_eq!(todos[0].hill_climbability, Some(95));
        assert_eq!(todos[1].hill_climbability, Some(20));
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
