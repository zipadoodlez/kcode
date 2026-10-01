use super::{Tool, ToolContext, ToolOutput};
use crate::bus::{Bus, BusEvent, TodoEvent};
use crate::todo::{TaskItem, load_tasks, save_tasks};
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

/// What a call does. Omitted means read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Action {
    List,
    Add,
    Update,
    Close,
}

#[derive(Debug, Deserialize)]
struct TodoInput {
    #[serde(default)]
    action: Option<Action>,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    parent: Option<String>,
    #[serde(default)]
    blocked_by: Option<Vec<String>>,
    #[serde(default)]
    assigned_to: Option<String>,
    #[serde(default)]
    note: Option<String>,
    #[serde(default)]
    result: Option<String>,
}

/// Apply one call to the open rows.
///
/// The write rules live here rather than in the schema: a close needs a result
/// that says what proves the task done, and a parent's row cannot go while a
/// child names it. See `docs/plans/work-list.md`, rules 3 and 4.
fn apply(input: &TodoInput, rows: &mut Vec<TaskItem>, session_id: &str) -> Result<()> {
    match input.action.unwrap_or(Action::List) {
        Action::List => {}
        Action::Add => {
            let content = nonempty(input.content.as_deref(), "add needs content")?;
            check_references(rows, input)?;
            let id = next_id(rows);
            rows.push(TaskItem {
                id: id.clone(),
                content: content.to_string(),
                status: "pending".to_string(),
                priority: String::new(),
                parent: input.parent.clone(),
                blocked_by: input.blocked_by.clone().unwrap_or_default(),
                assigned_to: Some(
                    input
                        .assigned_to
                        .clone()
                        .unwrap_or_else(|| session_id.to_string()),
                ),
                note: input.note.clone(),
                ..Default::default()
            });
            return Ok(());
        }
        Action::Update => {
            let id = nonempty(input.id.as_deref(), "update needs id")?;
            let row = row_mut(rows, id)?;
            if let Some(content) = input.content.as_deref() {
                row.content = nonempty(Some(content), "content cannot be empty")?.to_string();
            }
            if let Some(parent) = &input.parent {
                row.parent = Some(parent.clone());
            }
            if let Some(blocked_by) = &input.blocked_by {
                row.blocked_by = blocked_by.clone();
            }
            if let Some(assigned_to) = &input.assigned_to {
                row.assigned_to = Some(assigned_to.clone());
            }
            if let Some(note) = &input.note {
                row.note = Some(note.clone());
            }
            check_references(rows, input)?;
            return Ok(());
        }
        Action::Close => {
            let id = nonempty(input.id.as_deref(), "close needs id")?;
            nonempty(
                input.result.as_deref(),
                "close needs result: name the check that proves it, and what it showed",
            )?;
            row_mut(rows, id)?;
            if let Some(child) = rows.iter().find(|row| row.parent.as_deref() == Some(id)) {
                bail!(
                    "{id} still has open children: {} {}",
                    child.id,
                    child.content
                );
            }
            // The blocker is gone, so drop it from every dependent: an id left
            // behind would name no open row, which the plan engine reads as an
            // unsatisfied dependency (rule 7).
            for row in rows.iter_mut() {
                row.blocked_by
                    .retain(|dependency| dependency.as_str() != id);
            }
            rows.retain(|row| row.id != id);
            // The result is the task's durable record; it travels in the commit
            // with this delete (rule 4), so nothing keeps it here.
            return Ok(());
        }
    }
    Ok(())
}

fn nonempty<'a>(value: Option<&'a str>, message: &str) -> Result<&'a str> {
    match value.map(str::trim) {
        Some(value) if !value.is_empty() => Ok(value),
        _ => bail!("{message}"),
    }
}

fn row_mut<'a>(rows: &'a mut [TaskItem], id: &str) -> Result<&'a mut TaskItem> {
    if !rows.iter().any(|row| row.id == id) {
        bail!("no task {id:?}; open ids: {}", open_ids(rows));
    }
    Ok(rows
        .iter_mut()
        .find(|row| row.id == id)
        .expect("checked above"))
}

fn open_ids(rows: &[TaskItem]) -> String {
    match rows.is_empty() {
        true => "none".to_string(),
        false => rows
            .iter()
            .map(|row| row.id.as_str())
            .collect::<Vec<_>>()
            .join(", "),
    }
}

/// A parent or blocker that names no open row is a typo, not a plan.
fn check_references(rows: &[TaskItem], input: &TodoInput) -> Result<()> {
    for referenced in input
        .parent
        .iter()
        .chain(input.blocked_by.iter().flatten())
        .chain(input.id.iter().filter(|_| input.parent.is_some()))
    {
        if !rows.iter().any(|row| row.id == *referenced) {
            bail!(
                "{referenced:?} names no open task; open ids: {}",
                open_ids(rows)
            );
        }
    }
    Ok(())
}

fn next_id(rows: &[TaskItem]) -> String {
    let highest = rows
        .iter()
        .filter_map(|row| row.id.strip_prefix('t'))
        .filter_map(|number| number.parse::<u32>().ok())
        .max()
        .unwrap_or(0);
    format!("t{}", highest + 1)
}

fn build_todo_output(rows: Vec<TaskItem>) -> Result<ToolOutput> {
    let text = serde_json::to_string_pretty(&rows)?;
    Ok(ToolOutput::new(text)
        .with_title(format!("{} todos", rows.len()))
        .with_metadata(json!({"todos": rows})))
}

#[async_trait]
impl Tool for TodoTool {
    fn name(&self) -> &str {
        "todo"
    }

    fn description(&self) -> &str {
        // Model-visible and always on, so every word is paid on every request.
        // Handwritten, not generated.
        "Read or change the open work of this repo: add a task, update one, or close one with the result that proves it was done."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "intent": super::intent_schema_property(),
                "action": {
                    "type": "string",
                    "enum": ["list", "add", "update", "close"],
                    "description": "Omit to read the list."
                },
                "id": {
                    "type": "string",
                    "description": "Task id. Required for update and close."
                },
                "content": {
                    "type": "string",
                    "description": "The task in words."
                },
                "parent": {
                    "type": "string",
                    "description": "Id of the task this one is part of. A parent's row stays until its children are closed and it is integrated."
                },
                "blocked_by": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Ids that must be closed before this one starts."
                },
                "assigned_to": {
                    "type": "string",
                    "description": "Session id that holds the task. Defaults to you when adding."
                },
                "note": {
                    "type": "string",
                    "description": "One line on where the work got to."
                },
                "result": {
                    "type": "string",
                    "description": "Required to close: name the check that proves the task done, and report what it showed."
                }
            }
        })
    }

    async fn execute(&self, input: Value, ctx: ToolContext) -> Result<ToolOutput> {
        if input.get("todos").is_some() {
            bail!(
                "the todo tool takes an action now: add, update, or close (there is no whole-list write)"
            );
        }
        let input: TodoInput = serde_json::from_value(input)?;
        let dir = ctx.working_dir.as_deref();
        let mut rows = load_tasks(dir, &ctx.session_id)?;
        apply(&input, &mut rows, &ctx.session_id)?;
        if input.action.unwrap_or(Action::List) != Action::List {
            save_tasks(dir, &ctx.session_id, &rows)?;
            Bus::global().publish(BusEvent::TodoUpdated(TodoEvent {
                session_id: ctx.session_id.clone(),
                todos: rows.clone(),
            }));
        }
        build_todo_output(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(json: Value) -> TodoInput {
        serde_json::from_value(json).expect("input")
    }

    fn row(id: &str, content: &str) -> TaskItem {
        TaskItem {
            id: id.to_string(),
            content: content.to_string(),
            status: "pending".to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn add_claims_the_task_and_hands_out_the_next_id() {
        let mut rows = vec![row("t1", "first")];
        apply(
            &input(json!({"action": "add", "content": "second"})),
            &mut rows,
            "session-a",
        )
        .expect("add");
        assert_eq!(rows[1].id, "t2");
        assert_eq!(rows[1].assigned_to.as_deref(), Some("session-a"));
    }

    #[test]
    fn add_without_content_is_refused() {
        let mut rows = Vec::new();
        let err = apply(&input(json!({"action": "add"})), &mut rows, "s").unwrap_err();
        assert!(err.to_string().contains("content"), "{err}");
    }

    #[test]
    fn a_parent_that_names_nothing_is_refused() {
        let mut rows = Vec::new();
        let err = apply(
            &input(json!({"action": "add", "content": "child", "parent": "t9"})),
            &mut rows,
            "s",
        )
        .unwrap_err();
        assert!(err.to_string().contains("names no open task"), "{err}");
    }

    #[test]
    fn close_removes_the_row_and_needs_a_result() {
        let mut rows = vec![row("t1", "first")];
        let err = apply(
            &input(json!({"action": "close", "id": "t1"})),
            &mut rows,
            "s",
        )
        .unwrap_err();
        assert!(err.to_string().contains("close needs result"), "{err}");

        apply(
            &input(json!({"action": "close", "id": "t1", "result": "cargo test: 12 passed"})),
            &mut rows,
            "s",
        )
        .expect("close");
        assert!(rows.is_empty());
    }

    #[test]
    fn closing_a_blocker_frees_its_dependents() {
        let mut rows = vec![row("t1", "blocker")];
        let mut dependent = row("t2", "dependent");
        dependent.blocked_by = vec!["t1".to_string()];
        rows.push(dependent);

        apply(
            &input(json!({"action": "close", "id": "t1", "result": "cargo test: 12 passed"})),
            &mut rows,
            "s",
        )
        .expect("close");

        assert_eq!(rows.len(), 1);
        assert!(
            rows[0].blocked_by.is_empty(),
            "a closed blocker's id must not strand its dependent: {:?}",
            rows[0].blocked_by
        );
    }

    #[test]
    fn a_row_with_an_open_child_cannot_close() {
        let mut rows = vec![row("t1", "parent")];
        rows[0].parent = None;
        let mut child = row("t2", "child");
        child.parent = Some("t1".to_string());
        rows.push(child);
        let err = apply(
            &input(json!({"action": "close", "id": "t1", "result": "done"})),
            &mut rows,
            "s",
        )
        .unwrap_err();
        assert!(err.to_string().contains("still has open children"), "{err}");
    }

    #[test]
    fn update_changes_only_what_it_names() {
        let mut rows = vec![row("t1", "first")];
        rows[0].note = Some("old note".to_string());
        apply(
            &input(json!({"action": "update", "id": "t1", "content": "reworded"})),
            &mut rows,
            "s",
        )
        .expect("update");
        assert_eq!(rows[0].content, "reworded");
        assert_eq!(rows[0].note.as_deref(), Some("old note"));
    }

    #[test]
    fn listing_changes_nothing() {
        let mut rows = vec![row("t1", "first")];
        apply(&input(json!({})), &mut rows, "s").expect("list");
        assert_eq!(rows.len(), 1);
    }

    #[test]
    fn the_description_asks_for_the_proving_check() {
        let tool = TodoTool::new();
        let description = tool.description();
        assert!(description.contains("result that proves"), "{description}");
    }

    #[test]
    fn the_schema_names_the_actions_and_the_result() {
        let schema = TodoTool::new().parameters_schema();
        let props = &schema["properties"];
        assert_eq!(props["action"]["enum"][0], "list");
        assert!(
            props["result"]["description"]
                .as_str()
                .unwrap()
                .contains("check")
        );
        assert!(props.get("todos").is_none(), "the whole-list write is gone");
    }
}
