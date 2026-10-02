//! Scheduler: ready-set computation and dispatch.
//!
//! The scheduler walks the DAG. A node becomes runnable when all its dependencies
//! are `Done`, and on dispatch it is assigned to a worker (ownership). What a
//! worker reads before its turn is the plan's business, not the graph's:
//! `bridge::upstream_context` builds it from the row's own records.

use super::{NodeStatus, TaskGraph, TaskNode};

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
    // `dispatchable` proved the node exists under this same borrow of `graph`.
    let Some(node) = graph.get_mut(node_id) else {
        return false;
    };
    node.owner = Some(worker.to_string());
    node.status = NodeStatus::Running;
    true
}
