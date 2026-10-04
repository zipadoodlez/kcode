use serde::{Deserialize, Serialize};

/// One entry of a repo's work list: the single task type behind the `todo` tool,
/// the swarm plan, and the list file.
///
/// There used to be two of these, `TodoItem` and `PlanItem`, with the same
/// shape and a comment claiming they were separate. This is them once.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskItem {
    pub content: String,
    pub id: String,
    /// The run's word for this row's work: "explore" | "implement" | "verify" |
    /// "fix" | "synthesize" | "critique". The vocabulary is the plan engine's
    /// (`kcode_plan::kind`: `parse_kind` reads it, `kind_str` writes it) and the
    /// word is stored as written, so the store learns no engine type and takes no
    /// dependency on the engine (rule 5). Absent is a real state, not a default:
    /// rule 8 forbids guessing a kind for a row that has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// The task this one is part of, if any. A parent is work: its result is its
    /// children's results integrated, and its row stays until that is done.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    /// One line on where the work got to, for whoever picks it up next.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// The closes of the work done under this row: one entry per row that was
    /// closed while it named this one as its `parent`, so a row with children
    /// carries their results after they are gone (rule 3, and rule 4's "a parent's
    /// result is its children's results integrated").
    ///
    /// The store keeps each entry as the closer wrote it and reads none of it: the
    /// shape is `{"id", "result"}`, the closer's own words. Kept as JSON so the
    /// store learns no engine type (rule 5): it writes what a closer gave it and
    /// reads none of it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub records: Vec<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blocked_by: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assigned_to: Option<String>,
}
