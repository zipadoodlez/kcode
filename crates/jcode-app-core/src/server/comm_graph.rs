//! Server handlers for the task-DAG mutation ops (seed/expand/complete/inject).
//!
//! These are the live counterparts of the validated engine ops in
//! `jcode_plan::dag`. Each handler lifts the swarm's current `VersionedPlan` into
//! a `TaskGraph` (via `jcode_plan::bridge`), applies the engine op (which enforces
//! acyclicity, ownership, gate insertion, and artifact validation), lowers the
//! result back into the plan, then persists and broadcasts using the existing
//! swarm machinery. This keeps a single source of truth and reuses the scheduler,
//! persistence, and TUI broadcast paths.

use super::{
    SwarmEvent, SwarmEventType, SwarmMember, SwarmState, VersionedPlan, broadcast_swarm_plan,
    persist_swarm_state_for, record_swarm_event,
};
use crate::protocol::ServerEvent;
use crate::protocol::TaskGraphNodeSpec;
use jcode_plan::bridge::{apply_task_graph, parse_kind, to_task_graph};
use jcode_plan::dag::{self, HandoffArtifact, NodeSpec};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::{RwLock, broadcast};
use tokio::sync::mpsc;

fn spec_from_wire(spec: TaskGraphNodeSpec) -> NodeSpec {
    NodeSpec {
        id: Some(spec.id),
        content: spec.content,
        kind: parse_kind(spec.kind.as_deref()),
        depends_on: spec.depends_on,
        priority: spec.priority,
    }
}

async fn swarm_id_for(
    session_id: &str,
    swarm_members: &Arc<RwLock<HashMap<String, SwarmMember>>>,
) -> Option<String> {
    swarm_members
        .read()
        .await
        .get(session_id)
        .and_then(|member| member.swarm_id.clone())
}

fn err(client_event_tx: &mpsc::UnboundedSender<ServerEvent>, id: u64, message: String) {
    let _ = client_event_tx.send(ServerEvent::Error {
        id,
        message,
        retry_after_secs: None,
    });
}

/// Shared finalize: persist, broadcast, record a plan-update event, and ack.
#[expect(
    clippy::too_many_arguments,
    reason = "finalize threads through swarm persistence, broadcast, and event-history handles"
)]
async fn finalize(
    id: u64,
    swarm_id: &str,
    req_session_id: &str,
    reason: &str,
    item_count: usize,
    client_event_tx: &mpsc::UnboundedSender<ServerEvent>,
    swarm_members: &Arc<RwLock<HashMap<String, SwarmMember>>>,
    swarms_by_id: &Arc<RwLock<HashMap<String, HashSet<String>>>>,
    swarm_plans: &Arc<RwLock<HashMap<String, VersionedPlan>>>,
    swarm_coordinators: &Arc<RwLock<HashMap<String, String>>>,
    event_history: &Arc<RwLock<std::collections::VecDeque<SwarmEvent>>>,
    event_counter: &Arc<std::sync::atomic::AtomicU64>,
    swarm_event_tx: &broadcast::Sender<SwarmEvent>,
) {
    let from_name = swarm_members
        .read()
        .await
        .get(req_session_id)
        .and_then(|member| member.friendly_name.clone());

    let swarm_state = SwarmState {
        members: Arc::clone(swarm_members),
        swarms_by_id: Arc::clone(swarms_by_id),
        plans: Arc::clone(swarm_plans),
        coordinators: Arc::clone(swarm_coordinators),
    };
    persist_swarm_state_for(swarm_id, &swarm_state).await;
    broadcast_swarm_plan(
        swarm_id,
        Some(reason.to_string()),
        swarm_plans,
        swarm_members,
        swarms_by_id,
    )
    .await;
    record_swarm_event(
        event_history,
        event_counter,
        swarm_event_tx,
        req_session_id.to_string(),
        from_name,
        Some(swarm_id.to_string()),
        SwarmEventType::PlanUpdate {
            swarm_id: swarm_id.to_string(),
            item_count,
        },
    )
    .await;
    let _ = client_event_tx.send(ServerEvent::Done { id });
}

/// Seed (or re-seed) the swarm task DAG from a batch of node specs.
#[expect(clippy::too_many_arguments, reason = "swarm op threads runtime handles")]
pub(super) async fn handle_comm_seed_graph(
    id: u64,
    req_session_id: String,
    mode: Option<String>,
    nodes: Vec<TaskGraphNodeSpec>,
    client_event_tx: &mpsc::UnboundedSender<ServerEvent>,
    swarm_members: &Arc<RwLock<HashMap<String, SwarmMember>>>,
    swarms_by_id: &Arc<RwLock<HashMap<String, HashSet<String>>>>,
    swarm_plans: &Arc<RwLock<HashMap<String, VersionedPlan>>>,
    swarm_coordinators: &Arc<RwLock<HashMap<String, String>>>,
    event_history: &Arc<RwLock<std::collections::VecDeque<SwarmEvent>>>,
    event_counter: &Arc<std::sync::atomic::AtomicU64>,
    swarm_event_tx: &broadcast::Sender<SwarmEvent>,
) {
    let Some(swarm_id) = swarm_id_for(&req_session_id, swarm_members).await else {
        err(client_event_tx, id, "Not in a swarm.".to_string());
        return;
    };

    let specs: Vec<NodeSpec> = nodes.into_iter().map(spec_from_wire).collect();
    let count = specs.len();

    let result = {
        let mut plans = swarm_plans.write().await;
        let plan = plans
            .entry(swarm_id.clone())
            .or_insert_with(VersionedPlan::new);
        if let Some(mode) = mode {
            plan.mode = mode;
        }
        plan.participants.insert(req_session_id.clone());
        let mut graph = to_task_graph(plan);
        match dag::seed(&mut graph, specs) {
            Ok(()) => {
                apply_task_graph(plan, &graph);
                plan.version += 1;
                Ok(())
            }
            Err(e) => Err(e),
        }
    };

    match result {
        Ok(()) => {
            finalize(
                id,
                &swarm_id,
                &req_session_id,
                "task_graph_seed",
                count,
                client_event_tx,
                swarm_members,
                swarms_by_id,
                swarm_plans,
                swarm_coordinators,
                event_history,
                event_counter,
                swarm_event_tx,
            )
            .await;
        }
        Err(e) => err(client_event_tx, id, format!("Seed rejected: {e}")),
    }
}

/// Decompose a node the caller owns into a child sub-DAG.
#[expect(clippy::too_many_arguments, reason = "swarm op threads runtime handles")]
pub(super) async fn handle_comm_expand_node(
    id: u64,
    req_session_id: String,
    node_id: String,
    children: Vec<TaskGraphNodeSpec>,
    client_event_tx: &mpsc::UnboundedSender<ServerEvent>,
    swarm_members: &Arc<RwLock<HashMap<String, SwarmMember>>>,
    swarms_by_id: &Arc<RwLock<HashMap<String, HashSet<String>>>>,
    swarm_plans: &Arc<RwLock<HashMap<String, VersionedPlan>>>,
    swarm_coordinators: &Arc<RwLock<HashMap<String, String>>>,
    event_history: &Arc<RwLock<std::collections::VecDeque<SwarmEvent>>>,
    event_counter: &Arc<std::sync::atomic::AtomicU64>,
    swarm_event_tx: &broadcast::Sender<SwarmEvent>,
) {
    let Some(swarm_id) = swarm_id_for(&req_session_id, swarm_members).await else {
        err(client_event_tx, id, "Not in a swarm.".to_string());
        return;
    };
    let specs: Vec<NodeSpec> = children.into_iter().map(spec_from_wire).collect();
    let count = specs.len();

    let result = {
        let mut plans = swarm_plans.write().await;
        let Some(plan) = plans.get_mut(&swarm_id) else {
            err(client_event_tx, id, "No plan for this swarm.".to_string());
            return;
        };
        let mut graph = to_task_graph(plan);
        match dag::expand_node(&mut graph, &node_id, &req_session_id, specs) {
            Ok(_) => {
                apply_task_graph(plan, &graph);
                plan.version += 1;
                Ok(())
            }
            Err(e) => Err(e.to_string()),
        }
    };

    match result {
        Ok(()) => {
            finalize(
                id,
                &swarm_id,
                &req_session_id,
                "task_graph_expand",
                count,
                client_event_tx,
                swarm_members,
                swarms_by_id,
                swarm_plans,
                swarm_coordinators,
                event_history,
                event_counter,
                swarm_event_tx,
            )
            .await;
        }
        Err(e) => err(client_event_tx, id, format!("Expand rejected: {e}")),
    }
}

/// Complete a node the caller owns with a typed handoff artifact.
#[expect(clippy::too_many_arguments, reason = "swarm op threads runtime handles")]
pub(super) async fn handle_comm_complete_node(
    id: u64,
    req_session_id: String,
    node_id: String,
    artifact_json: String,
    client_event_tx: &mpsc::UnboundedSender<ServerEvent>,
    swarm_members: &Arc<RwLock<HashMap<String, SwarmMember>>>,
    swarms_by_id: &Arc<RwLock<HashMap<String, HashSet<String>>>>,
    swarm_plans: &Arc<RwLock<HashMap<String, VersionedPlan>>>,
    swarm_coordinators: &Arc<RwLock<HashMap<String, String>>>,
    event_history: &Arc<RwLock<std::collections::VecDeque<SwarmEvent>>>,
    event_counter: &Arc<std::sync::atomic::AtomicU64>,
    swarm_event_tx: &broadcast::Sender<SwarmEvent>,
) {
    let Some(swarm_id) = swarm_id_for(&req_session_id, swarm_members).await else {
        err(client_event_tx, id, "Not in a swarm.".to_string());
        return;
    };

    let artifact: HandoffArtifact = match serde_json::from_str(&artifact_json) {
        Ok(artifact) => artifact,
        Err(e) => {
            err(client_event_tx, id, format!("Invalid artifact JSON: {e}"));
            return;
        }
    };

    let result = {
        let mut plans = swarm_plans.write().await;
        let Some(plan) = plans.get_mut(&swarm_id) else {
            err(client_event_tx, id, "No plan for this swarm.".to_string());
            return;
        };
        let mut graph = to_task_graph(plan);
        match dag::complete_node(&mut graph, &node_id, &req_session_id, artifact) {
            Ok(()) => {
                apply_task_graph(plan, &graph);
                plan.version += 1;
                Ok(())
            }
            Err(e) => Err(e.to_string()),
        }
    };

    match result {
        Ok(()) => {
            finalize(
                id,
                &swarm_id,
                &req_session_id,
                "task_graph_complete",
                1,
                client_event_tx,
                swarm_members,
                swarms_by_id,
                swarm_plans,
                swarm_coordinators,
                event_history,
                event_counter,
                swarm_event_tx,
            )
            .await;
        }
        Err(e) => err(client_event_tx, id, format!("Complete rejected: {e}")),
    }
}

/// Inject gap/fix nodes from a gate the caller owns.
#[expect(clippy::too_many_arguments, reason = "swarm op threads runtime handles")]
pub(super) async fn handle_comm_inject_gap(
    id: u64,
    req_session_id: String,
    gate_id: String,
    nodes: Vec<TaskGraphNodeSpec>,
    client_event_tx: &mpsc::UnboundedSender<ServerEvent>,
    swarm_members: &Arc<RwLock<HashMap<String, SwarmMember>>>,
    swarms_by_id: &Arc<RwLock<HashMap<String, HashSet<String>>>>,
    swarm_plans: &Arc<RwLock<HashMap<String, VersionedPlan>>>,
    swarm_coordinators: &Arc<RwLock<HashMap<String, String>>>,
    event_history: &Arc<RwLock<std::collections::VecDeque<SwarmEvent>>>,
    event_counter: &Arc<std::sync::atomic::AtomicU64>,
    swarm_event_tx: &broadcast::Sender<SwarmEvent>,
) {
    let Some(swarm_id) = swarm_id_for(&req_session_id, swarm_members).await else {
        err(client_event_tx, id, "Not in a swarm.".to_string());
        return;
    };
    let specs: Vec<NodeSpec> = nodes.into_iter().map(spec_from_wire).collect();
    let count = specs.len();

    let result = {
        let mut plans = swarm_plans.write().await;
        let Some(plan) = plans.get_mut(&swarm_id) else {
            err(client_event_tx, id, "No plan for this swarm.".to_string());
            return;
        };
        let mut graph = to_task_graph(plan);
        match dag::inject_from_gate(&mut graph, &gate_id, &req_session_id, specs) {
            Ok(_) => {
                apply_task_graph(plan, &graph);
                plan.version += 1;
                Ok(())
            }
            Err(e) => Err(e.to_string()),
        }
    };

    match result {
        Ok(()) => {
            finalize(
                id,
                &swarm_id,
                &req_session_id,
                "task_graph_inject_gap",
                count,
                client_event_tx,
                swarm_members,
                swarms_by_id,
                swarm_plans,
                swarm_coordinators,
                event_history,
                event_counter,
                swarm_event_tx,
            )
            .await;
        }
        Err(e) => err(client_event_tx, id, format!("Inject rejected: {e}")),
    }
}
