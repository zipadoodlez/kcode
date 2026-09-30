use super::{Tool, ToolContext, ToolOutput};
use crate::bus::{Bus, BusEvent, TodoEvent};
use crate::todo::{TodoItem, load_todos, save_todos};
use anyhow::{Result, bail};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};

pub struct TodoTool;

impl TodoTool {
    pub fn new() -> Self {
        Self
    }
}

#[derive(Debug, Deserialize)]
struct TodoInput {
    todos: Option<Vec<TodoItem>>,
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

fn build_todo_output(todos: Vec<TodoItem>) -> Result<ToolOutput> {
    let remaining = todos
        .iter()
        .filter(|todo| todo.status != "completed")
        .count();
    let text = serde_json::to_string_pretty(&todos)?;
    Ok(ToolOutput::new(text)
        .with_title(format!("{} todos", remaining))
        .with_metadata(json!({"todos": todos})))
}

/// Leniently normalize raw todo-tool arguments before strict deserialization.
///
/// Some providers (notably Claude tool calling) intermittently emit tool
/// arguments as JSON *strings* instead of native types: the whole `todos`
/// array as one stringified JSON blob, individual items as stringified
/// objects, or numeric fields like `confidence` as `"90"`. Strict
/// `serde_json::from_value` rejects these with `invalid type: string ...`,
/// failing the entire call (issue #357; same provider quirk as #106).
/// Leniently normalize raw todo-tool arguments before strict deserialization.
///
/// Some providers (notably Claude tool calling) intermittently emit tool
/// arguments as JSON *strings* instead of native types: the whole `todos` array
/// as one stringified JSON blob, or individual items as stringified objects.
/// Strict `serde_json::from_value` rejects these with `invalid type: string
/// ...`, failing the entire call (issue #357; same provider quirk as #106).
fn normalize_todo_input(mut input: Value) -> Value {
    if let Some(entries) = input.get_mut("todos") {
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
                // Persisted sessions and prompts still carry natural spellings.
                if let Some(fields) = item.as_object_mut()
                    && let Some(Value::String(status)) = fields.get_mut("status")
                    && let Some(canonical) = crate::todo::canonical_todo_status(status)
                {
                    *status = canonical.to_string();
                }
            }
        }
    }
    input
}

#[async_trait]
impl Tool for TodoTool {
    fn name(&self) -> &str {
        "todo"
    }

    fn description(&self) -> &str {
        // Model-visible and always on, so every word is paid on every request.
        // Handwritten, not generated.
        "Read or update the todo list. For each item, name the check that proves it is done and report the actual result."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "intent": super::intent_schema_property(),
                "todos": {
                    "type": "array",
                    "description": "The full list. Any item not sent is dropped.",
                    "items": {
                        "type": "object",
                        "required": ["id", "content", "status", "priority"],
                        "properties": {
                            "id": {
                                "type": "string",
                                "description": "Stable id; reuse it to update an item."
                            },
                            "content": {
                                "type": "string",
                                "description": "Task."
                            },
                            "status": {
                                "type": "string",
                                "enum": ["pending", "in_progress", "completed", "cancelled"],
                                "description": "Status. Use completed when the task is done, cancelled when it should not be done."
                            },
                            "priority": {
                                "type": "string",
                                "description": "Priority."
                            },
                            "group": {
                                "type": "string",
                                "description": "Optional group label; one group per coherent goal, new direction = new group. Omit for a flat list."
                            }
                        }
                    }
                }
            }
        })
    }

    async fn execute(&self, input: Value, ctx: ToolContext) -> Result<ToolOutput> {
        let params = parse_todo_input(input)?;
        let is_write = params.todos.is_some();
        let operation = if is_write { "write" } else { "read" };
        let result = match params.todos {
            Some(todos) => save_todos(&ctx.session_id, &todos).and_then(|()| {
                Bus::global().publish(BusEvent::TodoUpdated(TodoEvent {
                    session_id: ctx.session_id.clone(),
                    todos: todos.clone(),
                }));
                build_todo_output(todos)
            }),
            None => load_todos(&ctx.session_id).and_then(build_todo_output),
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

    fn parse(input: Value) -> Result<TodoInput> {
        parse_todo_input(input)
    }

    #[test]
    fn empty_string_todos_means_read() {
        let parsed = parse(json!({"todos": ""})).expect("empty string should parse");
        assert!(parsed.todos.is_none());
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

    #[test]
    fn garbage_string_still_errors() {
        assert!(parse(json!({"todos": "not json at all"})).is_err());
    }

    #[test]
    fn schema_advertises_the_display_shape() {
        let schema = TodoTool::new().parameters_schema();
        let props = schema["properties"].as_object().expect("properties");
        assert!(props.contains_key("intent"));
        assert!(props.contains_key("todos"));
        for gone in ["plan", "goals"] {
            assert!(!props.contains_key(gone), "{gone} must be gone");
        }

        let items = &props["todos"]["items"];
        let fields = items["properties"].as_object().expect("item properties");
        for field in ["id", "content", "status", "priority", "group"] {
            assert!(fields.contains_key(field), "missing {field}");
        }
        for gone in ["confidence", "completion_confidence"] {
            assert!(!fields.contains_key(gone), "{gone} must be gone");
        }

        let required: Vec<&str> = items["required"]
            .as_array()
            .expect("required")
            .iter()
            .filter_map(|value| value.as_str())
            .collect();
        assert_eq!(required, vec!["id", "content", "status", "priority"]);
    }

    /// The one thing the deleted rubric was worth is now a sentence: name the
    /// check that proves an item done, and report its actual result.
    #[test]
    fn description_asks_for_the_proving_check() {
        let tool = TodoTool::new();
        let description = tool.description();
        assert!(
            description.contains("check that proves it is done"),
            "{description}"
        );
        assert!(
            description.contains("report the actual result"),
            "{description}"
        );
    }

    #[test]
    fn accepts_stringified_todos_array() {
        let parsed = parse(json!({
            "todos": "[{\"id\":\"a\",\"content\":\"c\",\"status\":\"pending\",\"priority\":\"high\"}]"
        }))
        .expect("a stringified array should parse");
        assert_eq!(parsed.todos.expect("todos").len(), 1);
    }

    #[test]
    fn accepts_stringified_todo_items() {
        let parsed = parse(json!({
            "todos": ["{\"id\":\"a\",\"content\":\"c\",\"status\":\"pending\",\"priority\":\"high\"}"]
        }))
        .expect("a stringified item should parse");
        assert_eq!(parsed.todos.expect("todos")[0].id, "a");
    }

    #[test]
    fn normalizes_natural_and_case_varied_todo_statuses() {
        for (sent, expected) in [
            ("done", "completed"),
            ("Finished", "completed"),
            ("IN PROGRESS", "in_progress"),
            ("cancelled", "cancelled"),
        ] {
            let parsed = parse(json!({
                "todos": [{"id": "a", "content": "c", "status": sent, "priority": "high"}]
            }))
            .expect("a natural status should parse");
            assert_eq!(
                parsed.todos.expect("todos")[0].status,
                expected,
                "sent {sent}"
            );
        }
    }

    #[test]
    fn rejects_unknown_todo_statuses_with_valid_vocabulary() {
        let error = parse(json!({
            "todos": [{"id": "a", "content": "c", "status": "welp", "priority": "high"}]
        }))
        .expect_err("an unknown status must be rejected")
        .to_string();
        assert!(
            error.contains("pending, in_progress, completed, cancelled"),
            "{error}"
        );
    }

    #[test]
    fn native_input_still_parses() {
        let parsed = parse(json!({
            "todos": [{"id": "a", "content": "c", "status": "pending", "priority": "high", "group": "g"}]
        }))
        .expect("native items should parse");
        assert_eq!(parsed.todos.expect("todos")[0].group.as_deref(), Some("g"));
    }

    /// A model that still sends the retired assessment fields, and a transcript
    /// replayed from before the schema shrank, must both keep working: those
    /// fields are ignored, never rejected.
    #[tokio::test]
    async fn retired_assessment_fields_are_ignored() {
        let _guard = crate::storage::lock_test_env();
        let previous_home = std::env::var_os("KCODE_HOME");
        let dir = tempfile::TempDir::new().expect("tempdir");
        crate::env::set_var("KCODE_HOME", dir.path());
        let session = "retired-assessment-fields";

        TodoTool::new()
            .execute(
                json!({
                    "plan": {"user_intention": "ship it", "understands_user_intent": "clear"},
                    "goals": [{"group": "release", "closed_feedback_loop": "closed"}],
                    "todos": [{
                        "id": "a",
                        "content": "c",
                        "status": "pending",
                        "priority": "high",
                        "confidence": "verified",
                        "completion_confidence": "verified",
                    }],
                }),
                test_ctx(session),
            )
            .await
            .expect("a write carrying retired fields must still succeed");

        let stored = load_todos(session).expect("todos");
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].status, "pending");

        match previous_home {
            Some(value) => crate::env::set_var("KCODE_HOME", value),
            None => crate::env::remove_var("KCODE_HOME"),
        }
    }
}
