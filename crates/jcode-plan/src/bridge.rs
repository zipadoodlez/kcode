//! Bridge between the validated [`crate::dag`] engine and the live
//! [`VersionedPlan`] storage used by the swarm runtime.
//!
//! The `dag` engine is the brain: it owns validation (acyclicity, ownership,
//! gate insertion, artifact checks) and the reference simulator. `VersionedPlan`
//! is the live, persisted, broadcast storage. Rather than run two parallel
//! runtimes, server handlers lift the current plan into a `TaskGraph`, apply an
//! engine op, then lower the result back. This keeps a single source of truth and
//! reuses the existing persistence/broadcast/scheduler machinery.

use crate::dag::{
    HandoffArtifact, Mode, NodeKind, NodeStatus, TaskGraph, TaskNode,
};
use crate::{NodeMeta, PlanItem, VersionedPlan};

/// Parse a mode string ("deep"/"light"); unknown values fall back to light.
pub fn parse_mode(mode: &str) -> Mode {
    match mode.trim().to_ascii_lowercase().as_str() {
        "deep" => Mode::Deep,
        _ => Mode::Light,
    }
}

pub fn mode_str(mode: Mode) -> &'static str {
    match mode {
        Mode::Deep => "deep",
        Mode::Light => "light",
    }
}

/// Parse a node-kind string; unknown/absent values default to `Explore`.
pub fn parse_kind(kind: Option<&str>) -> NodeKind {
    match kind.map(|k| k.trim().to_ascii_lowercase()).as_deref() {
        Some("implement") => NodeKind::Implement,
        Some("verify") => NodeKind::Verify,
        Some("fix") => NodeKind::Fix,
        Some("synthesize") => NodeKind::Synthesize,
        Some("critique") => NodeKind::Critique,
        _ => NodeKind::Explore,
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

/// Map a plan status string to an engine [`NodeStatus`].
fn status_from_plan(status: &str) -> NodeStatus {
    match status {
        "running" | "running_stale" => NodeStatus::Running,
        "completed" | "done" => NodeStatus::Done,
        "failed" | "stopped" | "crashed" => NodeStatus::Failed,
        _ => NodeStatus::Queued,
    }
}

/// Map an engine [`NodeStatus`] back to the canonical plan status string.
fn status_to_plan(status: NodeStatus) -> &'static str {
    match status {
        NodeStatus::Queued => "queued",
        NodeStatus::Running => "running",
        NodeStatus::Done => "completed",
        NodeStatus::Failed => "failed",
    }
}

/// Lift a [`VersionedPlan`] into a validated [`TaskGraph`] for engine ops.
pub fn to_task_graph(plan: &VersionedPlan) -> TaskGraph {
    let mut graph = TaskGraph::new(parse_mode(&plan.mode));
    for item in &plan.items {
        let meta = plan.node_meta.get(&item.id).cloned().unwrap_or_default();
        let artifact = meta
            .artifact_json
            .as_deref()
            .and_then(|json| serde_json::from_str::<HandoffArtifact>(json).ok());
        graph.push_node(TaskNode {
            id: item.id.clone(),
            content: item.content.clone(),
            kind: parse_kind(meta.kind.as_deref()),
            status: status_from_plan(&item.status),
            owner: item.assigned_to.clone(),
            parent: meta.parent.clone(),
            depends_on: item.blocked_by.clone(),
            expanded: meta.expanded,
            is_gate: meta.is_gate,
            priority: crate::priority_rank(&item.priority),
            output: artifact,
        });
    }
    graph
}

/// Lower a [`TaskGraph`] back into the plan's items + node_meta, preserving the
/// fields the engine does not own (subsystem, file_scope, original priority
/// string) from the prior plan where ids still match.
pub fn apply_task_graph(plan: &mut VersionedPlan, graph: &TaskGraph) {
    plan.mode = mode_str(graph.mode).to_string();

    // Index prior items to retain non-engine fields.
    let prior: std::collections::HashMap<String, PlanItem> = plan
        .items
        .iter()
        .map(|item| (item.id.clone(), item.clone()))
        .collect();

    let mut items = Vec::with_capacity(graph.nodes().len());
    let mut node_meta = std::collections::HashMap::new();

    for node in graph.nodes() {
        let prev = prior.get(&node.id);
        items.push(PlanItem {
            content: node.content.clone(),
            status: status_to_plan(node.status).to_string(),
            priority: prev
                .map(|p| p.priority.clone())
                .unwrap_or_else(|| priority_string(node.priority)),
            id: node.id.clone(),
            subsystem: prev.and_then(|p| p.subsystem.clone()),
            file_scope: prev.map(|p| p.file_scope.clone()).unwrap_or_default(),
            blocked_by: node.depends_on.clone(),
            assigned_to: node.owner.clone(),
        });
        node_meta.insert(
            node.id.clone(),
            NodeMeta {
                kind: Some(kind_str(node.kind).to_string()),
                parent: node.parent.clone(),
                expanded: node.expanded,
                is_gate: node.is_gate,
                artifact_json: node
                    .output
                    .as_ref()
                    .and_then(|a| serde_json::to_string(a).ok()),
            },
        );
    }

    plan.items = items;
    plan.node_meta = node_meta;
}

fn priority_string(rank: u8) -> String {
    match rank {
        0 => "high".to_string(),
        2 => "low".to_string(),
        _ => "medium".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dag::{NodeSpec, complete_node, dispatch, expand_node, seed};

    fn plan_item(id: &str, status: &str) -> PlanItem {
        PlanItem {
            content: format!("task {id}"),
            status: status.to_string(),
            priority: "medium".to_string(),
            id: id.to_string(),
            subsystem: None,
            file_scope: Vec::new(),
            blocked_by: Vec::new(),
            assigned_to: None,
        }
    }

    #[test]
    fn round_trip_preserves_items_and_edges() {
        let mut plan = VersionedPlan::new();
        plan.mode = "deep".to_string();
        plan.items = vec![
            plan_item("a", "completed"),
            PlanItem {
                blocked_by: vec!["a".to_string()],
                ..plan_item("b", "queued")
            },
        ];

        let graph = to_task_graph(&plan);
        assert_eq!(graph.mode, Mode::Deep);
        assert_eq!(graph.len(), 2);
        assert!(graph.get("a").unwrap().is_done());
        assert_eq!(graph.get("b").unwrap().depends_on, vec!["a".to_string()]);

        let mut plan2 = plan.clone();
        apply_task_graph(&mut plan2, &graph);
        assert_eq!(plan2.items.len(), 2);
        let b = plan2.items.iter().find(|i| i.id == "b").unwrap();
        assert_eq!(b.blocked_by, vec!["a".to_string()]);
        assert_eq!(b.status, "queued");
    }

    #[test]
    fn engine_op_through_bridge_updates_plan() {
        let mut plan = VersionedPlan::new();
        plan.mode = "deep".to_string();

        // Seed via engine, lower back into the plan.
        let mut graph = to_task_graph(&plan);
        seed(
            &mut graph,
            vec![NodeSpec::new("root", "explore X", NodeKind::Explore)],
        )
        .unwrap();
        apply_task_graph(&mut plan, &graph);
        assert_eq!(plan.items.len(), 1);
        assert_eq!(plan.node_meta["root"].kind.as_deref(), Some("explore"));

        // Dispatch + expand via engine, lower back; the gate must appear in the
        // plan with the composite parent marked expanded.
        let mut graph = to_task_graph(&plan);
        dispatch(&mut graph, "root", "w0");
        expand_node(
            &mut graph,
            "root",
            "w0",
            vec![NodeSpec::new("root.1", "facet", NodeKind::Explore)],
        )
        .unwrap();
        apply_task_graph(&mut plan, &graph);

        assert!(plan.node_meta["root"].expanded);
        let gate = plan
            .items
            .iter()
            .find(|i| plan.node_meta.get(&i.id).map(|m| m.is_gate).unwrap_or(false))
            .expect("gate should exist in lowered plan");
        assert_eq!(plan.node_meta[&gate.id].kind.as_deref(), Some("critique"));

        // Complete the child + gate + synthesis end to end through the bridge.
        let mut graph = to_task_graph(&plan);
        dispatch(&mut graph, "root.1", "w0");
        complete_node(
            &mut graph,
            "root.1",
            "w0",
            HandoffArtifact {
                findings: "found".into(),
                what_i_did_not_check: vec!["nothing".into()],
                ..HandoffArtifact::default()
            },
        )
        .unwrap();
        apply_task_graph(&mut plan, &graph);
        // The child's artifact round-trips through node_meta JSON.
        let stored = &plan.node_meta["root.1"].artifact_json;
        assert!(stored.as_ref().unwrap().contains("found"));
    }
}
