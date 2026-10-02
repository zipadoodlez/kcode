//! Bridge between the validated [`crate::dag`] engine and the live
//! [`VersionedPlan`] storage used by the swarm runtime.
//!
//! The `dag` engine is the brain: it owns validation (acyclicity, ownership) and
//! the reference simulator. `VersionedPlan` is the live, persisted, broadcast
//! storage. Rather than run two parallel runtimes, server handlers lift the
//! current plan into a `TaskGraph`, apply an engine op, then lower the result
//! back. This keeps a single source of truth and reuses the existing
//! persistence/broadcast/scheduler machinery.

use crate::artifact::HandoffArtifact;
use crate::dag::{NodeKind, NodeSpec, NodeStatus, TaskGraph, TaskNode};
use crate::{TaskItem, VersionedPlan};
use std::collections::HashSet;

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

/// Map a plan status string to an engine [`NodeStatus`].
fn status_from_plan(status: &str) -> NodeStatus {
    match status {
        "running" => NodeStatus::Running,
        "completed" | "done" => NodeStatus::Done,
        "failed" | "stopped" | "crashed" => NodeStatus::Failed,
        _ => NodeStatus::Queued,
    }
}

/// The nodes one session's rows seed: the rows it holds, in file order, lifted
/// into the engine's node specs. See `docs/plans/work-list.md`, "Rows are the
/// run's seed source".
///
/// The row's id is the node's id, its words are the node's content, its kind is
/// the node's kind, and its position is its priority rank, so the engine's order
/// and the file's order agree. A row is seeded only when it is seedable at all:
/// the engine knows its kind (rule 8 forbids guessing one), and every row it is
/// blocked by is also this session's. A blocker held by someone else is not this
/// run's to start, and an edge naming a node the graph does not have is not a
/// graph, so the pair is left for whoever holds the blocker.
pub fn seed_specs(rows: &[TaskItem], session_id: &str) -> Vec<NodeSpec> {
    let held: Vec<(usize, &TaskItem, NodeKind)> = rows
        .iter()
        .enumerate()
        .filter(|(_, row)| row.assigned_to.as_deref() == Some(session_id))
        .filter_map(|(position, row)| {
            parse_kind(row.kind.as_deref()).map(|kind| (position, row, kind))
        })
        .collect();
    let in_scope: HashSet<&str> = held.iter().map(|(_, row, _)| row.id.as_str()).collect();
    held.into_iter()
        .filter(|(_, row, _)| {
            row.blocked_by
                .iter()
                .all(|blocker| in_scope.contains(blocker.as_str()))
        })
        .map(|(position, row, kind)| NodeSpec {
            id: Some(row.id.clone()),
            content: row.content.clone(),
            kind,
            depends_on: row.blocked_by.clone(),
            priority: position.min(u8::MAX as usize) as u8,
        })
        .collect()
}

/// Lift a [`VersionedPlan`] into a validated [`TaskGraph`] for engine ops.
pub fn to_task_graph(plan: &VersionedPlan) -> TaskGraph {
    let mut graph = TaskGraph::new();
    for item in &plan.items {
        graph.push_node(TaskNode {
            id: item.id.clone(),
            content: item.content.clone(),
            // A lifted node must have a kind, and `Explore` is what an unwritten
            // kind has always rendered as; a row with no word is refused by the
            // seed, not here.
            kind: parse_kind(item.kind.as_deref()).unwrap_or(NodeKind::Explore),
            status: status_from_plan(&item.status),
            owner: item.assigned_to.clone(),
            parent: item.parent.clone(),
            depends_on: item.blocked_by.clone(),
            expanded: plan.is_composite(&item.id),
            priority: crate::priority_rank(&item.priority),
        });
    }
    graph
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
pub fn upstream_context(plan: &VersionedPlan, task_id: &str) -> Option<String> {
    let item = plan.items.iter().find(|item| item.id == task_id)?;

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
pub fn hydrate_assignment(plan: &VersionedPlan, task_id: &str, content: &str) -> String {
    match upstream_context(plan, task_id) {
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

    fn kinded(id: &str, kind: &str, deps: &[&str]) -> TaskItem {
        TaskItem {
            kind: Some(kind.to_string()),
            blocked_by: deps.iter().map(|dep| (*dep).to_string()).collect(),
            assigned_to: Some("me".to_string()),
            ..plan_item(id, "queued")
        }
    }

    /// The seed is the rows the session holds: the id, the words, the kind, the
    /// blocker edges and the position all come from the row, and a row that cannot
    /// be seated is left out instead of being given a guessed kind or an edge to a
    /// node the graph will not have.
    #[test]
    fn the_seed_is_the_rows_the_session_holds() {
        let rows = vec![
            kinded("t1", "explore", &[]),
            kinded("t2", "verify", &["t1"]),
            kinded("t3", "", &[]),
            TaskItem {
                assigned_to: Some("someone-else".to_string()),
                ..kinded("t4", "fix", &[])
            },
            kinded("t5", "explore", &["t4"]),
        ];

        let specs = seed_specs(&rows, "me");

        let ids: Vec<&str> = specs
            .iter()
            .map(|spec| spec.id.as_deref().unwrap_or_default())
            .collect();
        assert_eq!(
            ids,
            ["t1", "t2"],
            "a kindless row, a foreign row, and a row blocked outside the run stay out"
        );
        assert_eq!(
            specs[0].content, "task t1",
            "the row's words are the node's"
        );
        assert_eq!(
            specs[1].kind,
            NodeKind::Verify,
            "the row's kind is the node's"
        );
        assert_eq!(
            specs[1].depends_on,
            vec!["t1".to_string()],
            "the row's blockers are the node's edges"
        );
        assert_eq!(specs[0].priority, 0, "position is priority");
        assert_eq!(specs[1].priority, 1);
    }

    #[test]
    fn round_trip_preserves_items_and_edges() {
        let mut plan = VersionedPlan::new();
        plan.items = vec![
            plan_item("a", "completed"),
            TaskItem {
                blocked_by: vec!["a".to_string()],
                ..plan_item("b", "queued")
            },
        ];

        let graph = to_task_graph(&plan);
        assert_eq!(graph.len(), 2);
        assert!(graph.get("a").unwrap().is_done());
        assert_eq!(graph.get("b").unwrap().depends_on, vec!["a".to_string()]);
    }

    /// The context of a row is the work that closed under it: a close leaves its
    /// record on the row that owns the work, and the machine-readable half of that
    /// record is what the row's own turn integrates.
    #[test]
    fn a_rows_context_is_the_work_that_closed_under_it() {
        let mut plan = VersionedPlan::new();
        plan.items = vec![
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

        let hydrated = hydrate_assignment(&plan, "parent", "integrate the children");
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
            hydrate_assignment(&plan, "leaf", "just do this"),
            "just do this"
        );
    }
}
