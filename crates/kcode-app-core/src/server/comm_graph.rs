//! Server handlers for the task-DAG mutation ops (seed/expand/complete).
//!
//! Seeding and completing lift the swarm's current `VersionedPlan` into a
//! `TaskGraph` (via `kcode_plan::bridge`), apply the engine op (which enforces
//! acyclicity and ownership), and lower the result back. Decomposing is rows: the
//! handler writes child rows through the file store (`kcode-base`'s `todo`), and the
//! plan follows what the store wrote. Every path then persists and broadcasts using
//! the existing swarm machinery, so there is one source of truth per fact.

use super::{
    SwarmEvent, SwarmEventType, SwarmMember, SwarmState, VersionedPlan, broadcast_swarm_plan,
    persist_swarm_state_for, record_swarm_event,
};
use crate::protocol::ServerEvent;
use kcode_plan::MAX_PLAN_ITEMS;
use kcode_plan::bridge::{apply_task_graph, to_task_graph};
use kcode_plan::dag::{self, HandoffArtifact, NodeSpec, NodeStatus, TaskGraph};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::sync::{RwLock, broadcast};

fn graph_size_error(graph: &TaskGraph) -> Option<String> {
    (graph.len() > MAX_PLAN_ITEMS).then(|| {
        format!(
            "plan would contain {} items, exceeding the per-swarm limit of {}; finish or clear stale plan nodes before adding more",
            graph.len(),
            MAX_PLAN_ITEMS
        )
    })
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

/// Ensure the seeding session can actually drive the graph it just created.
///
/// Seeding sessions are frequently solo `agent`s with no coordinator elected,
/// yet `assign_task` / `assign_next` / `run_plan` are coordinator-gated. Without
/// this, a fresh agent can seed a task graph but then cannot dispatch any of it.
/// We elect the seeder as coordinator when the swarm has no *live*
/// coordinator, mirroring the self-promote rule used by `assign_role`. A live,
/// non-headless coordinator is left untouched so a real coordinator is never
/// displaced by a worker that happens to seed.
///
/// Returns true when the seeder was (or already is) the coordinator afterwards.
async fn ensure_seeder_can_coordinate(
    swarm_id: &str,
    seeder_session_id: &str,
    swarm_members: &Arc<RwLock<HashMap<String, SwarmMember>>>,
    swarm_coordinators: &Arc<RwLock<HashMap<String, String>>>,
) -> bool {
    // 1. Read the current coordinator id without holding the lock across the
    //    liveness check (matches the non-nested lock pattern used elsewhere).
    let current = swarm_coordinators.read().await.get(swarm_id).cloned();
    match &current {
        Some(coord) if coord == seeder_session_id => return true,
        _ => {}
    }

    // 2. Decide whether the existing coordinator is still a live driver.
    let coordinator_is_live = match &current {
        Some(coord) => {
            let members = swarm_members.read().await;
            members
                .get(coord)
                .map(|member| !member.event_tx.is_closed() && !member.is_headless)
                .unwrap_or(false)
        }
        None => false,
    };
    if coordinator_is_live {
        return false;
    }

    // 3. Promote the seeder; demote any prior (stale) coordinator member. Re-check
    //    under the write lock that the coordinator is still the one we inspected
    //    (compare-and-swap): two concurrent seeders race here, and the loser must
    //    not silently displace the winner it never liveness-checked.
    let prior = {
        let mut coordinators = swarm_coordinators.write().await;
        if coordinators.get(swarm_id) != current.as_ref() {
            // Someone else changed the coordinator between our read and write.
            return coordinators.get(swarm_id).map(String::as_str) == Some(seeder_session_id);
        }
        coordinators.insert(swarm_id.to_string(), seeder_session_id.to_string())
    };
    {
        let mut members = swarm_members.write().await;
        if let Some(member) = members.get_mut(seeder_session_id) {
            member.role = "coordinator".to_string();
        }
        if let Some(prior) = prior
            && prior != seeder_session_id
            && let Some(member) = members.get_mut(&prior)
        {
            member.role = "agent".to_string();
        }
    }
    true
}

/// Auto-claim a queued node for the participant that is trying to mutate it.
///
/// Seeded nodes are unowned until dispatch, but the run's contract tells the
/// seeding agent to `expand_node`/`complete_node` its own nodes, and the assign
/// path refuses self-assignment — so without this a solo deep seeder could never
/// legally touch any node it seeded (observed live as "Complete rejected: actor
/// does not own node"). Similarly, assignment to a client-attached worker leaves
/// the item `queued` (the server-run flip to `running` is skipped when a live
/// client owns the turn), so the assignee's own complete/expand would bounce with
/// "invalid state Queued".
///
/// Claiming is safe only when the node is genuinely available to this actor:
/// queued, with every dependency done (enforced by `dispatch`), and either
/// unowned or already assigned to this same actor. A node owned by someone else
/// is never touched — the engine's `NotOwner` check still applies.
fn claim_queued_node_for_actor(graph: &mut TaskGraph, node_id: &str, actor: &str) {
    let claimable = graph.get(node_id).is_some_and(|node| {
        node.status == NodeStatus::Queued
            && node.owner.as_deref().is_none_or(|owner| owner == actor)
    });
    if claimable {
        // `dispatch` re-validates queued status and dependency satisfaction; if
        // deps are not done the claim is skipped and the engine op reports the
        // real error.
        let _ = dag::dispatch(graph, node_id, actor);
    }
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
#[expect(
    clippy::too_many_arguments,
    reason = "swarm op threads runtime handles"
)]
pub(super) async fn handle_comm_seed_graph(
    id: u64,
    req_session_id: String,
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
    // The rows live where the seeder's session lives: its member record, the same
    // session root a busy agent's attach reads.
    let working_dir = swarm_members
        .read()
        .await
        .get(&req_session_id)
        .and_then(|member| member.working_dir.clone());

    // A solo seeder needs the coordinator slot to dispatch the graph it seeds via
    // the coordinator-gated assign/run_plan paths.
    ensure_seeder_can_coordinate(
        &swarm_id,
        &req_session_id,
        swarm_members,
        swarm_coordinators,
    )
    .await;

    // The seed is the rows this session holds (the run's own scope), not a list the
    // caller types: the row's id is the node's id, so a row never becomes two nodes
    // and a re-seed is a no-op. What the plan already has is not seeded again.
    // The run gets its anchor first, when it holds rows that belong to nothing, so
    // the row the run's records land on is one of the nodes it seeds.
    if let Err(error) = crate::todo::anchor_from_rows(working_dir.as_deref(), &req_session_id) {
        crate::logging::warn(&format!(
            "seed for {req_session_id} could not make the run's anchor ({error})"
        ));
    }
    let rows = crate::todo::load_tasks(working_dir.as_deref(), &req_session_id).unwrap_or_default();
    let seedable: Vec<NodeSpec> = kcode_plan::bridge::seed_specs(&rows, &req_session_id);
    if seedable.is_empty() {
        err(
            client_event_tx,
            id,
            "Seed rejected: this session holds no row the engine can seat. A row seeds a run when the todo tool gave it a kind and every row it is blocked by is this session's too."
                .to_string(),
        );
        return;
    }
    let count = seedable.len();

    let result = {
        let mut plans = swarm_plans.write().await;
        let plan = plans
            .entry(swarm_id.clone())
            .or_insert_with(VersionedPlan::new);
        let specs: Vec<NodeSpec> = seedable
            .into_iter()
            .filter(|spec| {
                let id = spec.id.as_deref().unwrap_or_default();
                !plan.items.iter().any(|item| item.id == id)
            })
            .collect();
        plan.participants.insert(req_session_id.clone());
        let mut graph = to_task_graph(plan);
        let before = graph.clone();
        match dag::seed(&mut graph, specs) {
            Ok(()) => match graph_size_error(&graph) {
                Some(message) => Err(message),
                None => {
                    if graph != before {
                        apply_task_graph(plan, &graph);
                        plan.version += 1;
                    }
                    Ok(count)
                }
            },
            Err(e) => Err(e.to_string()),
        }
    };

    match result {
        Ok(count) => {
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

/// Decompose a row the caller holds into child rows.
#[expect(
    clippy::too_many_arguments,
    reason = "swarm op threads runtime handles"
)]
pub(super) async fn handle_comm_expand_node(
    id: u64,
    req_session_id: String,
    node_id: String,
    children: Vec<kcode_plan::TaskItem>,
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
    let working_dir = swarm_members
        .read()
        .await
        .get(&req_session_id)
        .and_then(|member| member.working_dir.clone());
    let count = children.len();

    // Only the row's holder (or an unclaimed row) may decompose it. The plan holds
    // the dispatch, so the check lives here while the file is not yet its view.
    {
        let plans = swarm_plans.read().await;
        let Some(plan) = plans.get(&swarm_id) else {
            err(client_event_tx, id, "No plan for this swarm.".to_string());
            return;
        };
        let Some(item) = plan.items.iter().find(|item| item.id == node_id) else {
            err(
                client_event_tx,
                id,
                format!("Expand rejected: no node '{node_id}'"),
            );
            return;
        };
        if let Some(owner) = item.assigned_to.as_deref()
            && owner != req_session_id
        {
            err(
                client_event_tx,
                id,
                format!("Expand rejected: actor '{req_session_id}' does not hold '{node_id}'"),
            );
            return;
        }
    }

    // The store is the writer (rule 2): it owns the child ids and the file, and the
    // plan follows the rows it wrote.
    let result = match crate::todo::expand_row_on_disk(
        working_dir.as_deref(),
        &req_session_id,
        &node_id,
        children,
    ) {
        Ok(added) => {
            let mut plans = swarm_plans.write().await;
            match plans.get_mut(&swarm_id) {
                Some(plan) => {
                    let child_ids: Vec<String> = added.iter().map(|row| row.id.clone()).collect();
                    for row in added {
                        if !plan.items.iter().any(|item| item.id == row.id) {
                            plan.items.push(row);
                        }
                    }
                    if let Some(parent) = plan.items.iter_mut().find(|item| item.id == node_id) {
                        // The row is a join now: it waits for its children, and it
                        // keeps its holder, who is the one that integrates them.
                        parent.status = "queued".to_string();
                        for child_id in child_ids {
                            if !parent.blocked_by.contains(&child_id) {
                                parent.blocked_by.push(child_id);
                            }
                        }
                    }
                    plan.version += 1;
                    Ok(())
                }
                None => Err("No plan for this swarm.".to_string()),
            }
        }
        Err(error) => Err(error.to_string()),
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
#[expect(
    clippy::too_many_arguments,
    reason = "swarm op threads runtime handles"
)]
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

    // The row's half of this close, kept before the engine op takes the artifact:
    // the row's words are the artifact's findings, and the machine-readable half is
    // the artifact as written.
    let findings = artifact.findings.clone();
    let record_artifact = serde_json::to_value(&artifact).ok();

    let result = {
        let mut plans = swarm_plans.write().await;
        let Some(plan) = plans.get_mut(&swarm_id) else {
            err(client_event_tx, id, "No plan for this swarm.".to_string());
            return;
        };
        let mut graph = to_task_graph(plan);
        claim_queued_node_for_actor(&mut graph, &node_id, &req_session_id);
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
            // The engine's close is the row's close too, through the same writer the
            // `todo` tool uses: the node's id is the row's id, so the row goes with
            // its record onto the row that owns the work, and a re-seed does not
            // lift finished work again.
            let working_dir = swarm_members
                .read()
                .await
                .get(&req_session_id)
                .and_then(|member| member.working_dir.clone());
            if let Err(error) = crate::todo::close_row_on_disk(
                working_dir.as_deref(),
                &req_session_id,
                &node_id,
                &findings,
                record_artifact,
            ) {
                crate::logging::warn(&format!(
                    "node {node_id} closed in the plan but not in the list: {error}"
                ));
            }
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
