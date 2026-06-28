//! Scheduler: ready-set computation, dispatch, and dataflow hydration.
//!
//! The scheduler walks the DAG. A node becomes runnable when all its dependencies
//! are `Done`. On dispatch it is assigned to a worker (ownership) and its input is
//! hydrated from the merged artifacts of its upstream dependencies, which is the
//! forward dataflow along edges (doc section 5).

use super::{NodeStatus, TaskGraph, TaskNode};

/// Suggested default worker ceiling for light mode (doc section 1a). Deep mode is
/// bounded by the swarm-level `MAX_SWARM_MEMBERS` cap instead.
pub const LIGHT_MODE_SUGGESTED_WORKERS: usize = 16;

/// Whether a node has reached a terminal status.
pub fn is_terminal(node: &TaskNode) -> bool {
    node.is_terminal()
}

/// The set of nodes that are runnable right now: queued, with every dependency
/// `Done`. Returned in scheduling order (priority asc, then id) for determinism.
pub fn ready_nodes(graph: &TaskGraph) -> Vec<&TaskNode> {
    let mut ready: Vec<&TaskNode> = graph
        .nodes()
        .iter()
        .filter(|node| node.status == NodeStatus::Queued && deps_satisfied(graph, node))
        .collect();
    ready.sort_by(|a, b| a.priority.cmp(&b.priority).then_with(|| a.id.cmp(&b.id)));
    ready
}

fn deps_satisfied(graph: &TaskGraph, node: &TaskNode) -> bool {
    node.depends_on.iter().all(|dep| {
        graph
            .get(dep)
            .map(TaskNode::is_done)
            // A dependency that does not exist is treated as unsatisfiable; this
            // should never happen because edges are validated on insertion.
            .unwrap_or(false)
    })
}

/// Dispatch a ready node to `worker`: assign ownership and flip it to `Running`.
/// Returns false if the node is not currently dispatchable.
pub fn dispatch(graph: &mut TaskGraph, node_id: &str, worker: &str) -> bool {
    let dispatchable = graph
        .get(node_id)
        .map(|node| node.status == NodeStatus::Queued && deps_satisfied(graph, node))
        .unwrap_or(false);
    if !dispatchable {
        return false;
    }
    let node = graph.get_mut(node_id).unwrap();
    node.owner = Some(worker.to_string());
    node.status = NodeStatus::Running;
    true
}

/// Assemble the worker input for a node: its own prompt plus the merged handoff
/// artifacts of all its upstream dependencies. Artifacts are passed by reference
/// (findings + evidence), keeping context small (doc section 5).
pub fn assemble_input(graph: &TaskGraph, node_id: &str) -> String {
    let Some(node) = graph.get(node_id) else {
        return String::new();
    };
    let mut out = String::new();
    out.push_str(&node.content);

    let upstream: Vec<&TaskNode> = node
        .depends_on
        .iter()
        .filter_map(|dep| graph.get(dep))
        .filter(|dep| dep.is_done())
        .collect();

    if upstream.is_empty() {
        return out;
    }

    out.push_str("\n\n# Inputs from completed dependencies\n");
    for dep in upstream {
        out.push_str(&format!("\n## {} ({:?})\n", dep.id, dep.kind));
        if let Some(artifact) = &dep.output {
            if !artifact.findings.trim().is_empty() {
                out.push_str(&artifact.findings);
                out.push('\n');
            }
            if !artifact.evidence.is_empty() {
                out.push_str("Evidence: ");
                out.push_str(&artifact.evidence.join("; "));
                out.push('\n');
            }
            if let Some(validation) = &artifact.validation {
                out.push_str(&format!("Validation: {validation}\n"));
            }
            if !artifact.open_questions.is_empty() {
                out.push_str("Open questions: ");
                out.push_str(&artifact.open_questions.join("; "));
                out.push('\n');
            }
        }
    }
    out
}
