//! Validated graph mutations.
//!
//! Every mutation is append-style and server-validated. Writes are partitioned by
//! owner (you may only expand/complete a node you own), edges may only reference
//! existing nodes, and the result must stay acyclic. In deep mode, expanding a
//! node auto-inserts a critique/verify gate so a composite node cannot close
//! without surviving its gate (doc sections 2, 3, 6).

use super::{DagError, HandoffArtifact, Mode, NodeKind, NodeSpec, NodeStatus, TaskGraph, TaskNode};

/// Seed the initial DAG from a batch of specs (the first agent's draft). All
/// referenced dependencies must resolve within the supplied set, the ids must be
/// unique, and the result must be acyclic. The seed has no owner yet; ownership is
/// assigned on dispatch.
pub fn seed(graph: &mut TaskGraph, specs: Vec<NodeSpec>) -> Result<(), DagError> {
    // Validate ids: present, unique within the batch, and not already present.
    let mut seen = std::collections::HashSet::new();
    let mut ids = Vec::new();
    for spec in &specs {
        let id = validated_spec_id(spec, "seed")?;
        if graph.contains(&id) || !seen.insert(id.clone()) {
            return Err(DagError::DuplicateNode(id));
        }
        ids.push(id);
    }
    let known: std::collections::HashSet<&str> = ids.iter().map(String::as_str).collect();
    for spec in &specs {
        for dep in &spec.depends_on {
            if !known.contains(dep.as_str()) && !graph.contains(dep) {
                return Err(DagError::UnknownDependency {
                    node: spec.id.clone().unwrap_or_default(),
                    dependency: dep.clone(),
                });
            }
        }
    }

    // Apply onto a clone, verify acyclicity, then commit.
    let mut staged = graph.clone();
    for spec in specs {
        staged.push(spec_to_node(spec, None));
    }
    let cycle = staged.cycle_nodes();
    if !cycle.is_empty() {
        return Err(DagError::WouldCreateCycle(cycle));
    }
    *graph = staged;
    Ok(())
}

/// The result of expanding a node into children.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpandOutcome {
    /// Ids of the child nodes created.
    pub child_ids: Vec<String>,
    /// The id of the auto-inserted gate, if deep mode inserted one.
    pub gate_id: Option<String>,
}

/// Decompose a node the actor owns into a child sub-DAG (the composite path). The
/// node flips to composite and becomes a join/synthesis point that depends on its
/// children. In deep mode a critique/verify gate is auto-inserted between the
/// children and the synthesis, so the composite cannot close without surviving it.
///
/// Children may depend on each other and on the parent's own upstream
/// dependencies (already-existing nodes), preserving acyclicity by construction.
pub fn expand_node(
    graph: &mut TaskGraph,
    node_id: &str,
    actor: &str,
    children: Vec<NodeSpec>,
) -> Result<ExpandOutcome, DagError> {
    {
        let node = graph
            .get(node_id)
            .ok_or_else(|| DagError::UnknownNode(node_id.to_string()))?;
        if node.owner.as_deref() != Some(actor) {
            return Err(DagError::NotOwner {
                node: node_id.to_string(),
                actor: actor.to_string(),
            });
        }
        // Only a running, not-yet-expanded, non-gate node may be decomposed.
        if node.is_gate {
            return Err(DagError::GateMisuse(format!(
                "gate node '{node_id}' cannot be decomposed"
            )));
        }
        if node.expanded || node.status != NodeStatus::Running {
            return Err(DagError::InvalidState {
                node: node_id.to_string(),
                status: node.status,
            });
        }
        if children.is_empty() {
            return Err(DagError::GateMisuse(
                "expand requires at least one child".into(),
            ));
        }
    }

    // Validate child ids and dependency references.
    let mut seen = std::collections::HashSet::new();
    for spec in &children {
        let id = validated_spec_id(spec, "expand")?;
        if graph.contains(&id) || !seen.insert(id.clone()) {
            return Err(DagError::DuplicateNode(id));
        }
    }
    let child_ids: Vec<String> = children
        .iter()
        .map(|spec| spec.id.clone().unwrap())
        .collect();
    let child_set: std::collections::HashSet<&str> = child_ids.iter().map(String::as_str).collect();
    for spec in &children {
        for dep in &spec.depends_on {
            // A child may depend on a sibling or any already-existing node.
            if !child_set.contains(dep.as_str()) && !graph.contains(dep) {
                return Err(DagError::UnknownDependency {
                    node: spec.id.clone().unwrap(),
                    dependency: dep.clone(),
                });
            }
        }
    }

    // Stage onto a clone so a cycle rejects the whole expansion.
    let mut staged = graph.clone();

    // Insert children, parented to this node.
    for spec in children {
        staged.push(spec_to_node(spec, Some(node_id.to_string())));
    }

    // The synthesis (parent) must wait for every child. In deep mode it must also
    // wait for the gate. We keep the child edges even in deep mode: the gate
    // already depends on every child, so "gate done" implies "children done" for
    // *scheduling*, but the forward-dataflow hydration only reads a node's *direct*
    // dependencies. Dropping the child edges would mean the map-reduce synthesis
    // re-wake never receives its children's artifacts (doc section 5).
    let mut synth_deps = child_ids.clone();

    // Deep mode: insert a gate that depends on all children; the synthesis then
    // additionally depends on the gate so it cannot close until the gate passes.
    let gate_id = if staged.mode.requires_gates() {
        let parent_kind = staged
            .get(node_id)
            .map(|n| n.kind)
            .unwrap_or(NodeKind::Explore);
        let gate_kind = parent_kind.gate_kind();
        let gate_id = unique_gate_id(&staged, node_id);
        let gate = TaskNode {
            id: gate_id.clone(),
            content: gate_content(gate_kind, node_id),
            kind: gate_kind,
            status: NodeStatus::Queued,
            owner: None,
            parent: Some(node_id.to_string()),
            depends_on: child_ids.clone(),
            expanded: false,
            is_gate: true,
            planner: None,
            priority: 0,
            output: None,
        };
        staged.push(gate);
        synth_deps.push(gate_id.clone());
        Some(gate_id)
    } else {
        None
    };

    // Flip the parent into a composite join: it re-queues, depends on the
    // gate/children, and is marked expanded. Its prior upstream deps are retained
    // so the synthesis still waits on the original dependencies too.
    {
        let node = staged.get_mut(node_id).unwrap();
        node.expanded = true;
        node.status = NodeStatus::Queued;
        // Record the planner (current owner) for synthesis re-wake affinity, then
        // free `owner` so the re-queued composite is eligible for normal
        // scheduling once its children + gate complete.
        if node.planner.is_none() {
            node.planner = node.owner.clone();
        }
        node.owner = None;
        // Keep its original upstream deps and add the join deps.
        for dep in synth_deps {
            if !node.depends_on.contains(&dep) {
                node.depends_on.push(dep);
            }
        }
    }

    let cycle = staged.cycle_nodes();
    if !cycle.is_empty() {
        return Err(DagError::WouldCreateCycle(cycle));
    }
    *graph = staged;
    Ok(ExpandOutcome { child_ids, gate_id })
}

/// Complete a node the actor owns with a typed handoff artifact. In deep mode the
/// artifact is validated for thinness (findings + an honest "what I did not check"
/// on substantive work) and must carry a parseable confidence rung. A gate
/// additionally may not pass while a sibling under the same composite completed
/// with low confidence, unless the gate's artifact explicitly addresses that node
/// by id — the intended escape hatch is `inject_from_gate`, which converts the
/// doubt into new breadth. The artifact becomes the dataflow payload for
/// dependents.
pub fn complete_node(
    graph: &mut TaskGraph,
    node_id: &str,
    actor: &str,
    artifact: HandoffArtifact,
) -> Result<(), DagError> {
    let mode = graph.mode;
    let node = graph
        .get(node_id)
        .ok_or_else(|| DagError::UnknownNode(node_id.to_string()))?;
    if node.owner.as_deref() != Some(actor) {
        return Err(DagError::NotOwner {
            node: node_id.to_string(),
            actor: actor.to_string(),
        });
    }
    if node.status != NodeStatus::Running {
        return Err(DagError::InvalidState {
            node: node_id.to_string(),
            status: node.status,
        });
    }
    let is_gate = node.is_gate;
    validate_artifact(mode, node_id, is_gate, &artifact)?;
    if is_gate && mode.requires_gates() {
        validate_gate_confidence_debts(graph, node_id, &artifact)?;
    }

    let node = graph.get_mut(node_id).unwrap();
    node.status = NodeStatus::Done;
    node.output = Some(artifact);
    Ok(())
}

/// Mark a node the actor owns as failed. A downstream verify/fix path may then
/// supersede it.
pub fn fail_node(graph: &mut TaskGraph, node_id: &str, actor: &str) -> Result<(), DagError> {
    let node = graph
        .get(node_id)
        .ok_or_else(|| DagError::UnknownNode(node_id.to_string()))?;
    if node.owner.as_deref() != Some(actor) {
        return Err(DagError::NotOwner {
            node: node_id.to_string(),
            actor: actor.to_string(),
        });
    }
    if node.status != NodeStatus::Running {
        return Err(DagError::InvalidState {
            node: node_id.to_string(),
            status: node.status,
        });
    }
    graph.get_mut(node_id).unwrap().status = NodeStatus::Failed;
    Ok(())
}

/// Inject new gap/fix nodes from a gate that found a problem (the adversarial
/// path). The gate does not decompose itself; instead it adds new sibling nodes
/// under the same composite parent and re-queues itself to depend on them. This is
/// the "re-critique"/"re-verify" loop: the gate cannot pass, and the composite
/// parent (which depends on the gate) cannot close, until the new nodes drain and
/// the gate re-runs cleanly (doc section 6.2).
pub fn inject_from_gate(
    graph: &mut TaskGraph,
    gate_id: &str,
    actor: &str,
    new_nodes: Vec<NodeSpec>,
) -> Result<Vec<String>, DagError> {
    let parent = {
        let gate = graph
            .get(gate_id)
            .ok_or_else(|| DagError::UnknownNode(gate_id.to_string()))?;
        if gate.owner.as_deref() != Some(actor) {
            return Err(DagError::NotOwner {
                node: gate_id.to_string(),
                actor: actor.to_string(),
            });
        }
        if !gate.is_gate {
            return Err(DagError::GateMisuse(format!(
                "node '{gate_id}' is not a gate; use expand_node to decompose work"
            )));
        }
        if gate.status != NodeStatus::Running {
            return Err(DagError::InvalidState {
                node: gate_id.to_string(),
                status: gate.status,
            });
        }
        if new_nodes.is_empty() {
            return Err(DagError::GateMisuse(
                "inject_from_gate requires at least one new node".into(),
            ));
        }
        gate.parent.clone()
    };

    // Validate new node ids/deps.
    let mut seen = std::collections::HashSet::new();
    for spec in &new_nodes {
        let id = validated_spec_id(spec, "inject_from_gate")?;
        if graph.contains(&id) || !seen.insert(id.clone()) {
            return Err(DagError::DuplicateNode(id));
        }
    }
    let new_ids: Vec<String> = new_nodes.iter().map(|s| s.id.clone().unwrap()).collect();
    let new_set: std::collections::HashSet<&str> = new_ids.iter().map(String::as_str).collect();
    for spec in &new_nodes {
        for dep in &spec.depends_on {
            if !new_set.contains(dep.as_str()) && !graph.contains(dep) {
                return Err(DagError::UnknownDependency {
                    node: spec.id.clone().unwrap(),
                    dependency: dep.clone(),
                });
            }
        }
    }

    let mut staged = graph.clone();
    for spec in new_nodes {
        staged.push(spec_to_node(spec, parent.clone()));
    }
    // Re-queue the gate, now depending on the new nodes (re-critique/re-verify).
    {
        let gate = staged.get_mut(gate_id).unwrap();
        gate.status = NodeStatus::Queued;
        gate.owner = None;
        for id in &new_ids {
            if !gate.depends_on.contains(id) {
                gate.depends_on.push(id.clone());
            }
        }
    }
    // The composite parent must also depend on the gap nodes directly. Scheduling
    // alone would not need this (the gate already gates the parent), but forward
    // dataflow hydration reads only a node's *direct* dependencies, so without
    // these edges the synthesis re-wake would never receive the gap nodes'
    // artifacts — the same reason expand_node keeps child edges (doc section 5).
    if let Some(parent_id) = &parent
        && let Some(parent_node) = staged.get_mut(parent_id)
    {
        for id in &new_ids {
            if !parent_node.depends_on.contains(id) {
                parent_node.depends_on.push(id.clone());
            }
        }
    }
    let cycle = staged.cycle_nodes();
    if !cycle.is_empty() {
        return Err(DagError::WouldCreateCycle(cycle));
    }
    *graph = staged;
    Ok(new_ids)
}

/// Re-queue a failed node so it can be dispatched again (the retry path). The
/// owner is cleared: the retry may go to any worker. This is the engine-level
/// counterpart of the live `task_control retry` action; without it a failed
/// deep-mode gate would wedge its composite forever, because `deps_satisfied`
/// requires `Done` and every other mutation requires `Running`.
pub fn requeue_failed(graph: &mut TaskGraph, node_id: &str) -> Result<(), DagError> {
    let node = graph
        .get(node_id)
        .ok_or_else(|| DagError::UnknownNode(node_id.to_string()))?;
    if node.status != NodeStatus::Failed {
        return Err(DagError::InvalidState {
            node: node_id.to_string(),
            status: node.status,
        });
    }
    let node = graph.get_mut(node_id).unwrap();
    node.status = NodeStatus::Queued;
    node.owner = None;
    Ok(())
}

/// Derive a gate id for a composite node that does not collide with an existing
/// node id. The natural choice is `{node}::gate`; if a user happened to seed a
/// node by that exact id we suffix a counter so the engine never silently creates
/// a duplicate id (which would corrupt id-based lookups).
fn unique_gate_id(graph: &TaskGraph, node_id: &str) -> String {
    let base = format!("{node_id}::gate");
    if !graph.contains(&base) {
        return base;
    }
    let mut n = 2u32;
    loop {
        let candidate = format!("{base}{n}");
        if !graph.contains(&candidate) {
            return candidate;
        }
        n += 1;
    }
}

/// Whether free text mentions a node id as a standalone token (not merely as a
/// substring of a longer word). The confidence-debt rule turns on this: with
/// bare `contains`, a short child id like "a" or "fix" would match nearly any
/// English sentence and let a gate rubber-stamp an unaddressed low-confidence
/// sibling. Boundaries are any non-id characters; ids themselves may contain
/// alphanumerics plus `-_.:`/`::` (matching the gate-id convention).
fn mentions_node_id(text: &str, id: &str) -> bool {
    if id.is_empty() {
        return false;
    }
    let is_id_char = |c: char| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ':');
    let mut start = 0;
    while let Some(pos) = text[start..].find(id) {
        let begin = start + pos;
        let end = begin + id.len();
        let before_ok = begin == 0 || !text[..begin].chars().next_back().is_some_and(is_id_char);
        let after_ok = end == text.len() || !text[end..].chars().next().is_some_and(is_id_char);
        if before_ok && after_ok {
            return true;
        }
        // Advance past the first char of this match (char-boundary safe).
        let step = text[begin..].chars().next().map_or(1, char::len_utf8);
        start = begin + step;
        if start >= text.len() {
            break;
        }
    }
    false
}

/// Validate that a spec carries an explicit, non-blank id and return it. A
/// missing id is a misuse; an empty/whitespace id would corrupt id-based
/// lookups and edge references just like a duplicate would.
fn validated_spec_id(spec: &NodeSpec, op: &str) -> Result<String, DagError> {
    let id = spec
        .id
        .clone()
        .ok_or_else(|| DagError::GateMisuse(format!("{op} specs must carry explicit ids")))?;
    if id.trim().is_empty() {
        return Err(DagError::GateMisuse(format!(
            "{op} specs must carry non-empty ids"
        )));
    }
    Ok(id)
}

fn spec_to_node(spec: NodeSpec, parent: Option<String>) -> TaskNode {
    // Dedup dependencies (order-preserving). Agent-supplied specs sometimes
    // repeat a dep; duplicates carry no meaning and used to trip the cycle
    // detector's indegree accounting.
    let mut seen = std::collections::HashSet::new();
    let depends_on: Vec<String> = spec
        .depends_on
        .into_iter()
        .filter(|dep| seen.insert(dep.clone()))
        .collect();
    TaskNode {
        id: spec.id.unwrap_or_default(),
        content: spec.content,
        kind: spec.kind,
        status: NodeStatus::Queued,
        owner: None,
        parent,
        depends_on,
        expanded: false,
        is_gate: false,
        planner: None,
        priority: spec.priority,
        output: None,
    }
}

fn gate_content(kind: NodeKind, parent: &str) -> String {
    match kind {
        NodeKind::Verify => format!(
            "Verify the work of '{parent}': run the declared acceptance checks (build, tests, lint). \
             If anything fails, emit fix nodes back into the graph; do not pass until they drain."
        ),
        _ => format!(
            "Critique the work of '{parent}' adversarially. Read every child's 'what_i_did_not_check' \
             and find unexplored gaps given this task's stated scope. For each gap, emit a new child node; \
             do not pass until no gaps remain."
        ),
    }
}

fn validate_artifact(
    mode: Mode,
    node_id: &str,
    is_gate: bool,
    artifact: &HandoffArtifact,
) -> Result<(), DagError> {
    if !mode.requires_gates() {
        // Light mode accepts any artifact.
        return Ok(());
    }
    if is_gate {
        // Gate artifacts are pass/fail records; thinness rules don't apply, and
        // their confidence is about the *gate's* judgement, not the work.
        return Ok(());
    }
    if artifact.findings.trim().is_empty() {
        return Err(DagError::ThinArtifact {
            node: node_id.to_string(),
            reason: "deep-mode artifact requires non-empty findings".into(),
        });
    }
    if artifact.what_i_did_not_check.is_empty() {
        return Err(DagError::ThinArtifact {
            node: node_id.to_string(),
            reason: "deep-mode artifact must list 'what_i_did_not_check' (use an explicit \
                     'nothing, fully covered' entry only when truly exhaustive)"
                .into(),
        });
    }
    // Confidence is the breadth signal: gates prioritize probing low-confidence
    // siblings and cannot pass over unaddressed ones, and status surfaces report
    // them. That machinery only works if every substantive artifact carries a
    // parseable rung, so an absent/unparseable confidence is rejected the same
    // way thin findings are.
    if artifact.confidence_level().is_none() {
        return Err(DagError::ThinArtifact {
            node: node_id.to_string(),
            reason: "deep-mode artifact must state a confidence of low, medium, or high \
                     (honest 'low' is welcome: it routes follow-up work instead of \
                     penalizing you)"
                .into(),
        });
    }
    Ok(())
}

/// The gate confidence-debt rule (deep mode).
///
/// A gate exists to convert doubt into breadth. When a sibling under the same
/// composite finished with LOW confidence, that doubt is on the record, and the
/// gate may not simply pass over it: it must either have injected follow-up
/// nodes (`inject_from_gate`, after which the gate re-runs behind them) or
/// explicitly mention the shaky node's id in its `findings` or
/// `open_questions`, accepting the low confidence with a stated reason. The
/// gate's own `what_i_did_not_check` deliberately does NOT count: declaring "I
/// did not check X" is the opposite of addressing X. This keeps confidence
/// honest — admitting low confidence buys the work a second look instead of
/// nothing.
fn validate_gate_confidence_debts(
    graph: &TaskGraph,
    gate_id: &str,
    artifact: &HandoffArtifact,
) -> Result<(), DagError> {
    let Some(parent) = graph.get(gate_id).and_then(|gate| gate.parent.clone()) else {
        return Ok(());
    };
    let addressed = |id: &str| {
        mentions_node_id(&artifact.findings, id)
            || artifact
                .open_questions
                .iter()
                .any(|q| mentions_node_id(q, id))
    };
    let debts: Vec<String> = graph
        .children_of(&parent)
        .into_iter()
        .filter(|child| child.is_done())
        .filter(|child| {
            child
                .output
                .as_ref()
                .and_then(HandoffArtifact::confidence_level)
                == Some(super::ConfidenceLevel::Low)
        })
        .filter(|child| !addressed(&child.id))
        .map(|child| child.id.clone())
        .collect();
    if debts.is_empty() {
        Ok(())
    } else {
        Err(DagError::UnaddressedLowConfidence {
            gate: gate_id.to_string(),
            nodes: debts,
        })
    }
}
