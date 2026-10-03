//! Task-DAG engine model.
//!
//! This is the DAG-first reframe of swarm described in `docs/internals/swarm.md`.
//! The graph is the primary object: nodes are tasks, edges are dependencies, and
//! agents are fungible workers that execute and decompose (composite nodes)
//! those tasks.
//!
//! The model here is deliberately decoupled from the server/runtime wiring so it
//! can be exercised end-to-end by the deterministic simulator in `dag::sim`, which
//! is compiled for tests only, before being attached to live swarm sessions.

use serde::{Deserialize, Serialize};

mod ops;
mod schedule;
#[cfg(test)]
pub mod sim;

#[cfg(test)]
mod tests;

pub use ops::{complete_node, expand_node, fail_node, seed};
pub use schedule::{dispatch, is_terminal, ready_nodes};

/// A node identifier. Stable string ids keep the model serializable.
pub type NodeId = String;

/// The terminal action a node represents. The DAG is task-type agnostic.
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

/// Node lifecycle status. "Blocked" is intentionally not stored: it is computed
/// from dependency state by the scheduler, so there is a single source of truth.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeStatus {
    /// Not yet dispatched. Becomes runnable once all dependencies are `Done`.
    Queued,
    /// Dispatched to a worker and actively executing.
    Running,
    /// Finished successfully. The close's own words and its machine-readable half
    /// are the row's record, not the node's.
    Done,
    /// Unrecoverable failure. A `Fix`/re-verify path may supersede it.
    Failed,
}

/// A single task node in the DAG.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskNode {
    pub id: NodeId,
    /// The task prompt/instructions for the worker.
    pub content: String,
    pub kind: NodeKind,
    pub status: NodeStatus,
    /// The worker that owns this node (assigned on dispatch). Only the owner may
    /// expand or complete it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    /// The composite node this was decomposed from, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<NodeId>,
    /// Upstream node ids that must be `Done` before this node is runnable. This is
    /// both the dependency relation and the dataflow channel.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<NodeId>,
    /// True once this node has been decomposed into children (composite). A
    /// composite node re-runs as a synthesis/join once its children close.
    #[serde(default)]
    pub expanded: bool,
    /// Priority used to order the ready set. Lower rank runs first.
    #[serde(default)]
    pub priority: u8,
}

impl TaskNode {
    pub fn is_composite(&self) -> bool {
        self.expanded
    }

    pub fn is_done(&self) -> bool {
        matches!(self.status, NodeStatus::Done)
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self.status, NodeStatus::Done | NodeStatus::Failed)
    }
}

/// A declarative spec for a node to add (seed or expand). Ids may be omitted to be
/// auto-assigned by the engine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeSpec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<NodeId>,
    pub content: String,
    pub kind: NodeKind,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<NodeId>,
    #[serde(default)]
    pub priority: u8,
}

impl NodeSpec {
    pub fn new(id: impl Into<String>, content: impl Into<String>, kind: NodeKind) -> Self {
        Self {
            id: Some(id.into()),
            content: content.into(),
            kind,
            depends_on: Vec::new(),
            priority: 0,
        }
    }

    pub fn depends_on(mut self, deps: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.depends_on = deps.into_iter().map(Into::into).collect();
        self
    }

    pub fn priority(mut self, priority: u8) -> Self {
        self.priority = priority;
        self
    }
}

/// Errors produced by validated graph mutations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DagError {
    /// A referenced node id does not exist.
    UnknownNode(NodeId),
    /// A node id collides with an existing one.
    DuplicateNode(NodeId),
    /// An edge references a node id that exists nowhere in the operation.
    UnknownDependency { node: NodeId, dependency: NodeId },
    /// The mutation would introduce a cycle.
    WouldCreateCycle(Vec<NodeId>),
    /// The actor is not the owner of the node it tried to mutate.
    NotOwner { node: NodeId, actor: String },
    /// The node is not in a state where the operation is valid.
    InvalidState { node: NodeId, status: NodeStatus },
    /// A node spec was malformed (missing/empty id, empty child batch).
    InvalidSpec(String),
}

impl std::fmt::Display for DagError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DagError::UnknownNode(id) => write!(f, "unknown node '{id}'"),
            DagError::DuplicateNode(id) => write!(f, "duplicate node id '{id}'"),
            DagError::UnknownDependency { node, dependency } => {
                write!(f, "node '{node}' depends on unknown node '{dependency}'")
            }
            DagError::WouldCreateCycle(ids) => {
                write!(
                    f,
                    "operation would create a cycle among: {}",
                    ids.join(", ")
                )
            }
            DagError::NotOwner { node, actor } => {
                write!(f, "actor '{actor}' does not own node '{node}'")
            }
            DagError::InvalidState { node, status } => {
                write!(
                    f,
                    "node '{node}' is in invalid state {status:?} for this operation"
                )
            }
            DagError::InvalidSpec(msg) => write!(f, "invalid spec: {msg}"),
        }
    }
}

impl std::error::Error for DagError {}

/// The task DAG: a set of nodes. Insertion order is preserved for deterministic
/// iteration; lookups are by id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskGraph {
    nodes: Vec<TaskNode>,
}

impl TaskGraph {
    pub fn new() -> Self {
        Self { nodes: Vec::new() }
    }
}

impl Default for TaskGraph {
    fn default() -> Self {
        Self::new()
    }
}

impl TaskGraph {
    pub fn nodes(&self) -> &[TaskNode] {
        &self.nodes
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn get(&self, id: &str) -> Option<&TaskNode> {
        self.nodes.iter().find(|node| node.id == id)
    }

    pub(crate) fn get_mut(&mut self, id: &str) -> Option<&mut TaskNode> {
        self.nodes.iter_mut().find(|node| node.id == id)
    }

    pub fn contains(&self, id: &str) -> bool {
        self.nodes.iter().any(|node| node.id == id)
    }

    pub(crate) fn push(&mut self, node: TaskNode) {
        self.nodes.push(node);
    }

    /// Push a fully-formed node. Used by the bridge to lift the run's rows into
    /// a `TaskGraph`. Callers are responsible for keeping ids unique; the
    /// validated ops (`seed`/`expand_node`) enforce uniqueness on the write path.
    pub fn push_node(&mut self, node: TaskNode) {
        self.nodes.push(node);
    }

    /// Children of a composite node.
    pub fn children_of(&self, id: &str) -> Vec<&TaskNode> {
        self.nodes
            .iter()
            .filter(|node| node.parent.as_deref() == Some(id))
            .collect()
    }

    /// Whether every node has reached a terminal status.
    pub fn all_terminal(&self) -> bool {
        self.nodes.iter().all(TaskNode::is_terminal)
    }

    /// Detect a cycle over the current `depends_on` edges, returning the node ids
    /// that participate in (or are downstream of) a cycle. Empty when acyclic.
    pub fn cycle_nodes(&self) -> Vec<NodeId> {
        // Kahn's algorithm: repeatedly remove zero-indegree nodes. Anything left
        // is part of, or fed by, a cycle.
        use std::collections::HashMap;
        let known: std::collections::HashSet<&str> =
            self.nodes.iter().map(|n| n.id.as_str()).collect();
        let mut indegree: HashMap<&str, usize> = HashMap::new();
        for node in &self.nodes {
            indegree.entry(node.id.as_str()).or_insert(0);
        }
        for node in &self.nodes {
            // Count each unique in-graph dependency once. `depends_on` can carry
            // duplicates (agent-supplied specs are not deduped), and the
            // relaxation below decrements once per unique (dep, dependent) pair,
            // so counting occurrences here would strand acyclic nodes at
            // indegree > 0 and falsely report a cycle.
            let unique_deps: std::collections::HashSet<&str> = node
                .depends_on
                .iter()
                .map(String::as_str)
                .filter(|dep| known.contains(dep))
                .collect();
            *indegree.entry(node.id.as_str()).or_insert(0) += unique_deps.len();
        }
        let mut queue: Vec<&str> = indegree
            .iter()
            .filter_map(|(id, deg)| (*deg == 0).then_some(*id))
            .collect();
        queue.sort_unstable();
        let mut visited = std::collections::HashSet::new();
        while let Some(id) = queue.pop() {
            if !visited.insert(id) {
                continue;
            }
            for node in &self.nodes {
                if node.depends_on.iter().any(|dep| dep == id)
                    && let Some(deg) = indegree.get_mut(node.id.as_str())
                {
                    *deg = deg.saturating_sub(1);
                    if *deg == 0 {
                        queue.push(node.id.as_str());
                    }
                }
            }
        }
        let mut leftover: Vec<NodeId> = self
            .nodes
            .iter()
            .map(|n| n.id.clone())
            .filter(|id| !visited.contains(id.as_str()))
            .collect();
        leftover.sort();
        leftover
    }
}
