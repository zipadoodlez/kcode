//! Validated graph mutations.
//!
//! Every mutation is append-style and server-validated. Writes are partitioned by
//! owner (you may only expand/complete a node you own), edges may only reference
//! existing nodes, and the result must stay acyclic (doc sections 2, 3, 6).

use super::{DagError, HandoffArtifact, NodeSpec, NodeStatus, TaskGraph, TaskNode};

/// Seed the initial DAG from a batch of specs (the first agent's draft). All
/// referenced dependencies must resolve within the supplied set and the result
/// must be acyclic. Replaying an identical seed definition is a no-op, which makes
/// transport/tool retries safe. Reusing an id for a different definition remains
/// an error. The seed has no owner yet; ownership is assigned on dispatch.
pub fn seed(graph: &mut TaskGraph, specs: Vec<NodeSpec>) -> Result<(), DagError> {
    // Validate ids and collapse exact replays within the same request. A repeated
    // id with different declarative fields is still ambiguous and rejected.
    let mut unique_specs: Vec<NodeSpec> = Vec::with_capacity(specs.len());
    let mut indexes = std::collections::HashMap::<String, usize>::new();
    for spec in specs {
        let id = validated_spec_id(&spec, "seed")?;
        if let Some(existing_index) = indexes.get(&id).copied() {
            if !seed_specs_equivalent(&unique_specs[existing_index], &spec) {
                return Err(DagError::DuplicateNode(id));
            }
            continue;
        }
        indexes.insert(id, unique_specs.len());
        unique_specs.push(spec);
    }

    let known: std::collections::HashSet<&str> = indexes.keys().map(String::as_str).collect();
    for spec in &unique_specs {
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
    for spec in unique_specs {
        let id = spec.id.as_deref().expect("seed ids were validated above");
        if let Some(existing) = graph.get(id) {
            if !seed_spec_matches_existing(graph, existing, &spec) {
                return Err(DagError::DuplicateNode(id.to_string()));
            }
            continue;
        }
        staged.push(spec_to_node(spec, None));
    }
    let cycle = staged.cycle_nodes();
    if !cycle.is_empty() {
        return Err(DagError::WouldCreateCycle(cycle));
    }
    *graph = staged;
    Ok(())
}

fn seed_specs_equivalent(left: &NodeSpec, right: &NodeSpec) -> bool {
    left.id == right.id
        && left.content == right.content
        && left.kind == right.kind
        && left.priority == right.priority
        && dependency_sets_equal(&left.depends_on, &right.depends_on)
}

fn seed_spec_matches_existing(graph: &TaskGraph, node: &TaskNode, spec: &NodeSpec) -> bool {
    // Only top-level seeded work can be replayed. A collision with an expanded
    // child must never be silently treated as the same declaration.
    if node.parent.is_some()
        || node.content != spec.content
        || node.kind != spec.kind
        || node.priority != spec.priority
    {
        return false;
    }

    // Expanding a seeded node appends its children to the parent's dependency
    // list. Filter those machinery-owned join edges so a later replay can still
    // be recognized from the original upstream dependencies.
    let declared_dependencies: Vec<&str> = node
        .depends_on
        .iter()
        .filter(|dependency| {
            graph
                .get(dependency)
                .is_none_or(|candidate| candidate.parent.as_deref() != Some(node.id.as_str()))
        })
        .map(String::as_str)
        .collect();
    dependency_sets_equal_iter(
        declared_dependencies,
        spec.depends_on.iter().map(String::as_str),
    )
}

fn dependency_sets_equal(left: &[String], right: &[String]) -> bool {
    dependency_sets_equal_iter(
        left.iter().map(String::as_str),
        right.iter().map(String::as_str),
    )
}

fn dependency_sets_equal_iter<'a>(
    left: impl IntoIterator<Item = &'a str>,
    right: impl IntoIterator<Item = &'a str>,
) -> bool {
    let left: std::collections::HashSet<&str> = left.into_iter().collect();
    let right: std::collections::HashSet<&str> = right.into_iter().collect();
    left == right
}

/// Decompose a node the actor owns into a child sub-DAG (the composite path). The
/// node flips to composite and becomes a join/synthesis point that depends on its
/// children.
///
/// Children may depend on each other and on the parent's own upstream
/// dependencies (already-existing nodes), preserving acyclicity by construction.
pub fn expand_node(
    graph: &mut TaskGraph,
    node_id: &str,
    actor: &str,
    children: Vec<NodeSpec>,
) -> Result<Vec<String>, DagError> {
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
        // Only a running, not-yet-expanded node may be decomposed.
        if node.expanded || node.status != NodeStatus::Running {
            return Err(DagError::InvalidState {
                node: node_id.to_string(),
                status: node.status,
            });
        }
        if children.is_empty() {
            return Err(DagError::InvalidSpec(
                "expand requires at least one child".into(),
            ));
        }
    }

    // Validate child ids and dependency references. Collect the validated ids
    // once so later steps never re-unwrap `spec.id`.
    let mut seen = std::collections::HashSet::new();
    let mut child_ids: Vec<String> = Vec::with_capacity(children.len());
    for spec in &children {
        let id = validated_spec_id(spec, "expand")?;
        if graph.contains(&id) || !seen.insert(id.clone()) {
            return Err(DagError::DuplicateNode(id));
        }
        child_ids.push(id);
    }
    let child_set: std::collections::HashSet<&str> = child_ids.iter().map(String::as_str).collect();
    for (spec, child_id) in children.iter().zip(child_ids.iter()) {
        for dep in &spec.depends_on {
            // A child may depend on a sibling or any already-existing node.
            if !child_set.contains(dep.as_str()) && !graph.contains(dep) {
                return Err(DagError::UnknownDependency {
                    node: child_id.clone(),
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

    // The synthesis (parent) must wait for every child. The forward-dataflow
    // hydration reads a node's *direct* dependencies, so the child edges carry
    // the map-reduce synthesis re-wake its children's artifacts (doc section 5).
    let synth_deps = child_ids.clone();

    // Flip the parent into a composite join: it re-queues, depends on the
    // children, and is marked expanded. Its prior upstream deps are retained so
    // the synthesis still waits on the original dependencies too.
    {
        let node = staged
            .get_mut(node_id)
            .ok_or_else(|| DagError::UnknownNode(node_id.to_string()))?;
        node.expanded = true;
        node.status = NodeStatus::Queued;
        // The owner stays: it is the record of who integrates the children, so the
        // re-queued composite is scheduled back to the same worker once they
        // complete. The row's assignment carries this in the plan, and readiness
        // never reads ownership (`ready_nodes`), so nothing needs freeing to make
        // the composite runnable again.
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
    Ok(child_ids)
}

/// Complete a node the actor owns with a typed handoff artifact. The artifact
/// becomes the dataflow payload for dependents.
pub fn complete_node(
    graph: &mut TaskGraph,
    node_id: &str,
    actor: &str,
    artifact: HandoffArtifact,
) -> Result<(), DagError> {
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

    let node = graph
        .get_mut(node_id)
        .ok_or_else(|| DagError::UnknownNode(node_id.to_string()))?;
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
    graph
        .get_mut(node_id)
        .ok_or_else(|| DagError::UnknownNode(node_id.to_string()))?
        .status = NodeStatus::Failed;
    Ok(())
}

/// Re-queue a failed node so it can be dispatched again (the retry path). The
/// owner is cleared: the retry may go to any worker. This is the engine-level
/// counterpart of the live `task_control retry` action, because `deps_satisfied`
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
    let node = graph
        .get_mut(node_id)
        .ok_or_else(|| DagError::UnknownNode(node_id.to_string()))?;
    node.status = NodeStatus::Queued;
    node.owner = None;
    Ok(())
}

/// Validate that a spec carries an explicit, non-blank id and return it. A
/// missing id is a misuse; an empty/whitespace id would corrupt id-based
/// lookups and edge references just like a duplicate would.
fn validated_spec_id(spec: &NodeSpec, op: &str) -> Result<String, DagError> {
    let id = spec
        .id
        .clone()
        .ok_or_else(|| DagError::InvalidSpec(format!("{op} specs must carry explicit ids")))?;
    if id.trim().is_empty() {
        return Err(DagError::InvalidSpec(format!(
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
        priority: spec.priority,
        output: None,
    }
}
