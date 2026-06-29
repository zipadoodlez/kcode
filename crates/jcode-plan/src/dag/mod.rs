//! Task-DAG engine model.
//!
//! This is the DAG-first reframe of swarm described in `docs/SWARM_TASK_GRAPH.md`.
//! The graph is the primary object: nodes are tasks, edges are dependencies, and
//! agents are fungible workers that execute, decompose (composite nodes), and
//! verify (gate nodes) those tasks.
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

pub use ops::{ExpandOutcome, complete_node, expand_node, fail_node, inject_from_gate, seed};
pub use schedule::{
    LIGHT_MODE_SUGGESTED_WORKERS, assemble_input, dispatch, is_terminal, ready_nodes,
};

/// A node identifier. Stable string ids keep the model serializable and let the
/// auto-generated gate ids derive deterministically from their parent.
pub type NodeId = String;

/// Engine mode. One engine, two presets (see doc section 1a). The data model,
/// scheduler, and dataflow are identical; the mode only controls whether the
/// rigor machinery (mandatory gates + strict artifact validation) is engaged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mode {
    /// Comprehensive: composite nodes get an auto-inserted critique/verify gate
    /// before they can close, and completion artifacts are strictly validated.
    Deep,
    /// Fan-out: cheap parallelism. No mandatory gates, lightweight artifacts.
    Light,
}

impl Mode {
    pub fn requires_gates(self) -> bool {
        matches!(self, Mode::Deep)
    }
}

/// The terminal action a node represents. The DAG is task-type agnostic; only the
/// artifact contract and which gate kind is inserted vary by node kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeKind {
    /// Research/analysis. Artifact = findings. Gated by `Critique`.
    Explore,
    /// Code change. Artifact = diff/commit ref. Gated by `Verify`.
    Implement,
    /// Acceptance check (build/tests). A gate kind.
    Verify,
    /// Repair after a failed verify. Gated by `Verify`.
    Fix,
    /// Map-reduce rollup of a composite node's children. Gated by `Critique`.
    Synthesize,
    /// Adversarial gap-finder for exploration. A gate kind.
    Critique,
}

impl NodeKind {
    /// Whether this kind is itself a gate (auto-inserted, not user-seeded work).
    pub fn is_gate_kind(self) -> bool {
        matches!(self, NodeKind::Critique | NodeKind::Verify)
    }

    /// The gate kind that guards a composite node of `self` before it may close.
    /// Exploration-style work is guarded by a critique (gap-finding); code-style
    /// work is guarded by a verify (does it actually work).
    pub fn gate_kind(self) -> NodeKind {
        match self {
            NodeKind::Implement | NodeKind::Fix => NodeKind::Verify,
            _ => NodeKind::Critique,
        }
    }
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

/// The typed handoff artifact attached to a node on completion. This is the
/// dataflow payload that travels forward along edges to dependents.
///
/// In deep mode, `findings` and `what_i_did_not_check` are required: forcing an
/// agent to enumerate what it did *not* check is what makes thin work structurally
/// visible (doc section 6.3). In light mode any artifact is accepted.
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<String>,
    /// The cheat code: explicit unexplored surface. Gates convert these into new
    /// nodes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub what_i_did_not_check: Vec<String>,
}

impl HandoffArtifact {
    /// A minimal artifact for light mode or tests.
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
    /// a critique gate is explicitly instructed to read what each child did *not*
    /// check, so dropping those fields here would make the gate structurally unable
    /// to do its job (doc sections 5, 6.3).
    pub fn render_section(&self, id: &str, kind: &str) -> String {
        let mut body = format!("## {id} ({kind})\n");
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
    /// composite node re-runs as a synthesis/join once its children + gate close.
    #[serde(default)]
    pub expanded: bool,
    /// True if this node is an auto-inserted gate (critique/verify).
    #[serde(default)]
    pub is_gate: bool,
    /// The agent that planned this node's decomposition. Set when a node is
    /// expanded into a composite; used to prefer the same planner for the
    /// synthesis re-wake while leaving `owner` free for normal scheduling.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub planner: Option<String>,
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
    /// The completion artifact failed deep-mode validation.
    ThinArtifact { node: NodeId, reason: String },
    /// A gate kind was supplied as user work, or vice versa.
    GateMisuse(String),
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
            DagError::ThinArtifact { node, reason } => {
                write!(f, "node '{node}' artifact rejected: {reason}")
            }
            DagError::GateMisuse(msg) => write!(f, "gate misuse: {msg}"),
        }
    }
}

impl std::error::Error for DagError {}

/// The task DAG: a mode plus a set of nodes. Insertion order is preserved for
/// deterministic iteration; lookups are by id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskGraph {
    pub mode: Mode,
    nodes: Vec<TaskNode>,
}

impl TaskGraph {
    pub fn new(mode: Mode) -> Self {
        Self {
            mode,
            nodes: Vec::new(),
        }
    }

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

    /// Children of a composite node (excluding its gate).
    pub fn children_of(&self, id: &str) -> Vec<&TaskNode> {
        self.nodes
            .iter()
            .filter(|node| node.parent.as_deref() == Some(id) && !node.is_gate)
            .collect()
    }

    /// The gate node guarding a composite node, if any.
    pub fn gate_of(&self, id: &str) -> Option<&TaskNode> {
        self.nodes
            .iter()
            .find(|node| node.parent.as_deref() == Some(id) && node.is_gate)
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
            for dep in &node.depends_on {
                if known.contains(dep.as_str()) {
                    *indegree.entry(node.id.as_str()).or_insert(0) += 1;
                }
            }
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
