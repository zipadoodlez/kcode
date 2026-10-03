//! The kind a row's work is: the one vocabulary the words, the parser that reads
//! them back, and the option list a schema shows all share.
//!
//! A kind is stored as the word the writer wrote (`TaskItem::kind`), so the store
//! learns no engine type (rule 5) and this module is the only place that turns a
//! word into the engine's kind and back.

use crate::TaskItem;
use crate::artifact::HandoffArtifact;
use serde::{Deserialize, Serialize};

/// The terminal action a row represents. The work list is kind-agnostic: the
/// vocabulary is the engine's, and a row writes one of these words or none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeKind {
    /// Research/analysis. Artifact = findings.
    Explore,
    /// Code change. Artifact = diff/commit ref.
    Implement,
    /// Acceptance check (build/tests).
    Verify,
    /// Repair after a failed verify.
    Fix,
    /// Map-reduce rollup of a composite node's children.
    Synthesize,
    /// Adversarial gap-finder for exploration.
    Critique,
}

/// Every kind the engine knows, in the order its words are listed.
pub const KINDS: [NodeKind; 6] = [
    NodeKind::Explore,
    NodeKind::Implement,
    NodeKind::Verify,
    NodeKind::Fix,
    NodeKind::Synthesize,
    NodeKind::Critique,
];

/// A kind's word, or `None` for a kind the engine has no word for.
pub fn parse_kind(kind: Option<&str>) -> Option<NodeKind> {
    match kind.map(|k| k.trim().to_ascii_lowercase()).as_deref() {
        Some("explore") => Some(NodeKind::Explore),
        Some("implement") => Some(NodeKind::Implement),
        Some("verify") => Some(NodeKind::Verify),
        Some("fix") => Some(NodeKind::Fix),
        Some("synthesize") => Some(NodeKind::Synthesize),
        Some("critique") => Some(NodeKind::Critique),
        _ => None,
    }
}

pub fn kind_str(kind: NodeKind) -> &'static str {
    match kind {
        NodeKind::Explore => "explore",
        NodeKind::Implement => "implement",
        NodeKind::Verify => "verify",
        NodeKind::Fix => "fix",
        NodeKind::Synthesize => "synthesize",
        NodeKind::Critique => "critique",
    }
}

/// The kinds as the words a caller writes: a schema's `enum`, or an error message
/// for a word the engine does not know.
pub fn kind_words() -> String {
    KINDS.map(kind_str).join(", ")
}

/// Build the forward-dataflow context for a task: the handoff artifacts of the work
/// that closed under it, formatted for injection into the assigned worker's prompt.
/// Returns `None` when that work left no artifact, so callers can skip appending
/// anything.
///
/// The results of earlier work live on the row that owns it, so a row's own
/// `records` are what it integrates: a row that was split gets its children's
/// artifacts for its synthesis turn, and a run's top row gets the run's results when
/// it closes. A row whose dependencies closed before it gets nothing here, because
/// the list keeps no edge to a closed row (rule 7): carrying the artifacts a second
/// time to keep that path would be the duplicate this replaces.The artifact is the
/// machine-readable half, and a close that left only its result words contributes
/// nothing, exactly as before.
pub fn upstream_context(rows: &[TaskItem], task_id: &str) -> Option<String> {
    let item = rows.iter().find(|item| item.id == task_id)?;

    let mut sections = Vec::new();
    for record in &item.records {
        let Some(id) = record.get("id").and_then(|value| value.as_str()) else {
            continue;
        };
        let Some(artifact) = record.get("artifact") else {
            continue;
        };
        let Ok(artifact) = serde_json::from_value::<HandoffArtifact>(artifact.clone()) else {
            continue;
        };
        sections.push(artifact.render_section(id, None));
    }

    if sections.is_empty() {
        None
    } else {
        Some(format!(
            "# Results of the work under this row\n\n{}",
            sections.join("\n")
        ))
    }
}

/// Prepend upstream dependency context (if any) to a task's assignment content.
pub fn hydrate_assignment(rows: &[TaskItem], task_id: &str, content: &str) -> String {
    match upstream_context(rows, task_id) {
        Some(context) => format!("{content}\n\n{context}"),
        None => content.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan_item(id: &str, status: &str) -> TaskItem {
        TaskItem {
            content: format!("task {id}"),
            status: status.to_string(),
            priority: "medium".to_string(),
            id: id.to_string(),
            ..Default::default()
        }
    }

    /// The words, the parser and the writer are one vocabulary: a schema and an
    /// error message both read `kind_words`, so a kind added to `KINDS` without a
    /// word, or a word without a kind, has to fail here.
    #[test]
    fn the_words_the_parser_and_the_writer_agree() {
        for kind in KINDS {
            assert_eq!(parse_kind(Some(kind_str(kind))), Some(kind));
        }
        assert_eq!(parse_kind(None), None, "no word is no kind, not a guess");
        assert_eq!(
            parse_kind(Some("implment")),
            None,
            "an unknown word is no kind"
        );
        assert_eq!(parse_kind(Some(" Explore ")), Some(NodeKind::Explore));
        assert_eq!(
            kind_words(),
            "explore, implement, verify, fix, synthesize, critique"
        );
    }

    /// The context of a row is the work that closed under it: a close leaves its
    /// record on the row that owns the work, and the machine-readable half of that
    /// record is what the row's own turn integrates.
    #[test]
    fn a_rows_context_is_the_work_that_closed_under_it() {
        let rows = vec![
            TaskItem {
                records: vec![
                    serde_json::json!({
                        "id": "child-a",
                        "result": "cargo test: 12 passed",
                        "artifact": serde_json::to_value(HandoffArtifact {
                            findings: "API in foo.rs".to_string(),
                            evidence: vec!["crates/foo/api.rs:12".to_string()],
                            ..HandoffArtifact::default()
                        })
                        .unwrap(),
                    }),
                    // A close that left only its words contributes nothing.
                    serde_json::json!({"id": "child-b", "result": "done"}),
                ],
                ..plan_item("parent", "queued")
            },
            plan_item("leaf", "queued"),
        ];

        let hydrated = hydrate_assignment(&rows, "parent", "integrate the children");
        assert!(hydrated.contains("integrate the children"));
        assert!(hydrated.contains("Results of the work under this row"));
        assert!(hydrated.contains("## child-a"));
        assert!(hydrated.contains("API in foo.rs"));
        assert!(hydrated.contains("crates/foo/api.rs:12"));
        assert!(
            !hydrated.contains("child-b"),
            "a record with no artifact adds no section"
        );

        // A row nothing closed under has no context, so its content is unchanged.
        assert_eq!(
            hydrate_assignment(&rows, "leaf", "just do this"),
            "just do this"
        );
    }
}
