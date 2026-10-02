//! Deterministic task-DAG simulator.
//!
//! This drives the engine end-to-end with scripted mock workers so the scheduler and
//! ops can be verified without any live agents. It is the executable analogue of the
//! DAG model in `docs/internals/swarm.md`.
//!
//! A worker is a closure that, given the node it was handed, returns a
//! [`WorkerAction`]. The driver loops: dispatch all ready nodes round-robin to a
//! bounded worker pool, run each one step, apply the resulting mutation, and
//! repeat until the graph is fully terminal or it stalls.

use super::{
    DagError, NodeKind, NodeSpec, TaskGraph, complete_node, dispatch, expand_node, fail_node,
    ready_nodes,
};

/// What a mock worker decides to do with the node it was handed.
#[derive(Debug, Clone)]
pub enum WorkerAction {
    /// Execute the node directly and complete it.
    Complete,
    /// Decompose the node into these children (composite path).
    Expand(Vec<NodeSpec>),
    /// Fail the node.
    Fail,
}

/// A scripted worker. Receives the node id and kind; returns an action. The closure
/// may capture mutable state (e.g. to expand only once).
pub type Worker<'a> = dyn FnMut(&str, NodeKind) -> WorkerAction + 'a;

/// Outcome of a simulation run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimReport {
    pub steps: usize,
    pub completed: usize,
    pub failed: usize,
    pub stalled: bool,
}

/// Run the simulation to completion (or stall). `max_workers` bounds how many
/// nodes run concurrently per step; `max_steps` guards against runaway loops.
pub fn run(
    graph: &mut TaskGraph,
    max_workers: usize,
    max_steps: usize,
    worker: &mut Worker<'_>,
) -> Result<SimReport, DagError> {
    let mut steps = 0usize;
    loop {
        if graph.all_terminal() {
            break;
        }
        if steps >= max_steps {
            return Ok(report(graph, steps, true));
        }

        // Dispatch up to `max_workers` ready nodes this step. We collect ids first
        // to avoid borrowing the graph while mutating it.
        let ready: Vec<(String, NodeKind)> = ready_nodes(graph)
            .into_iter()
            .take(max_workers)
            .map(|node| (node.id.clone(), node.kind))
            .collect();

        if ready.is_empty() {
            // Nothing runnable and not all terminal => stall (e.g. a Failed node
            // blocking its dependents with no fix path).
            return Ok(report(graph, steps, true));
        }

        for (idx, (node_id, kind)) in ready.into_iter().enumerate() {
            let worker_name = format!("w{}", idx % max_workers);
            if !dispatch(graph, &node_id, &worker_name) {
                continue;
            }
            let action = worker(&node_id, kind);
            match action {
                WorkerAction::Complete => {
                    complete_node(graph, &node_id, &worker_name)?;
                }
                WorkerAction::Expand(children) => {
                    expand_node(graph, &node_id, &worker_name, children)?;
                }
                WorkerAction::Fail => {
                    fail_node(graph, &node_id, &worker_name)?;
                }
            }
            steps += 1;
        }
    }
    Ok(report(graph, steps, false))
}

fn report(graph: &TaskGraph, steps: usize, stalled: bool) -> SimReport {
    let completed = graph.nodes().iter().filter(|node| node.is_done()).count();
    let failed = graph
        .nodes()
        .iter()
        .filter(|node| matches!(node.status, super::NodeStatus::Failed))
        .count();
    SimReport {
        steps,
        completed,
        failed,
        stalled,
    }
}
