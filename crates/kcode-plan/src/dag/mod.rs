//! Task-DAG engine model.
//!
//! This is the DAG-first reframe of swarm described in `docs/internals/swarm.md`.
//! The graph is the primary object: nodes are tasks, edges are dependencies, and
//! agents are fungible workers that execute and decompose (composite nodes)
//! those tasks.
//!
//! The model here is deliberately decoupled from the server/runtime wiring so it
//! can be exercised end-to-end by the deterministic simulator in [`crate::dag::sim`]
//! before being attached to live swarm sessions.

use serde::{Deserialize, Serialize};

mod ops;
mod schedule;
pub mod sim;

#[cfg(test)]
mod tests;

pub use ops::{complete_node, expand_node, fail_node, requeue_failed, seed};
pub use schedule::{assemble_input, dispatch, is_terminal, ready_nodes};

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
    /// Finished successfully; `output` artifact is attached.
    Done,
    /// Unrecoverable failure. A `Fix`/re-verify path may supersede it.
    Failed,
}

/// Deserialize `confidence` from either a JSON string or a bare number.
/// Agents frequently emit `"confidence": 0.8` instead of `"0.8"`; rejecting
/// that with a serde type error is pointless friction, so numbers are
/// stringified.
fn de_confidence_scalar<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Scalar {
        Text(String),
        Number(f64),
        Bool(bool),
    }
    Ok(
        Option::<Scalar>::deserialize(deserializer)?.map(|scalar| match scalar {
            Scalar::Text(text) => text,
            Scalar::Number(number) => number.to_string(),
            Scalar::Bool(flag) => flag.to_string(),
        }),
    )
}

/// The typed handoff artifact attached to a node on completion. This is the
/// dataflow payload that travels forward along edges to dependents.
///
/// Forcing an agent to enumerate what it did *not* check is what makes thin work
/// structurally visible, so that field is rendered forward with the rest.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HandoffArtifact {
    /// The deliverable summary (findings for explore, what shipped for implement).
    #[serde(default)]
    pub findings: String,
    /// References, not claims: file:line, commit refs, paths.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edge_cases_considered: Vec<String>,
    /// Verify results for code-style nodes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub open_questions: Vec<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "de_confidence_scalar"
    )]
    pub confidence: Option<String>,
    /// Explicit unexplored surface, rendered forward for downstream workers.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub what_i_did_not_check: Vec<String>,
}

impl HandoffArtifact {
    /// A minimal artifact for tests.
    pub fn brief(findings: impl Into<String>) -> Self {
        Self {
            findings: findings.into(),
            ..Self::default()
        }
    }

    /// Render this artifact as a forward-dataflow section for a downstream worker
    /// (or a gate). This is the single source of truth for how an artifact is
    /// surfaced on a dependency edge, so the engine scheduler and the live bridge
    /// stay in lockstep.
    ///
    /// Critically this includes `edge_cases_considered` and `what_i_did_not_check`:
    /// a downstream worker reads what its dependencies did *not* check, so
    /// dropping those fields here would hide that surface (doc sections 5, 6.3).
    /// Render this artifact as one section of a prompt's context. `kind` is the
    /// node's word for the work when the caller still has the node; a section built
    /// from a closed row's record has no kind, because the store keeps no engine
    /// vocabulary and the row is gone.
    pub fn render_section(&self, id: &str, kind: Option<&str>) -> String {
        let mut body = match kind {
            Some(kind) => format!("## {id} ({kind})\n"),
            None => format!("## {id}\n"),
        };
        if !self.findings.trim().is_empty() {
            body.push_str(&self.findings);
            body.push('\n');
        }
        if !self.evidence.is_empty() {
            body.push_str(&format!("Evidence: {}\n", self.evidence.join("; ")));
        }
        if !self.edge_cases_considered.is_empty() {
            body.push_str(&format!(
                "Edge cases considered: {}\n",
                self.edge_cases_considered.join("; ")
            ));
        }
        if let Some(validation) = &self.validation {
            body.push_str(&format!("Validation: {validation}\n"));
        }
        if !self.open_questions.is_empty() {
            body.push_str(&format!(
                "Open questions: {}\n",
                self.open_questions.join("; ")
            ));
        }
        if let Some(confidence) = &self.confidence {
            body.push_str(&format!("Confidence: {confidence}\n"));
        }
        if !self.what_i_did_not_check.is_empty() {
            body.push_str(&format!(
                "What was not checked: {}\n",
                self.what_i_did_not_check.join("; ")
            ));
        }
        body
    }
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
    /// The typed handoff artifact, present once `Done`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<HandoffArtifact>,
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

    /// Push a fully-formed node. Used by the bridge to lift a `VersionedPlan` into
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
