#![cfg_attr(test, allow(clippy::items_after_test_module))]

use super::append_swarm_completion_report_instructions;
use super::swarm_mutation_state::{
    PersistedSwarmMutationResponse, begin_or_join_in_flight as begin_swarm_mutation_no_replay,
    begin_or_replay as begin_swarm_mutation_or_replay,
    finish_request as finish_swarm_mutation_request, request_key as swarm_mutation_request_key,
};
use super::{
    ClientConnectionInfo, RunState, SessionAgents, SwarmEvent, SwarmEventType, SwarmMember,
    SwarmMutationRuntime, SwarmState, broadcast_swarm_plan, broadcast_swarm_plan_with_previous,
    fanout_session_event, persist_swarm_state_for, queue_soft_interrupt_for_session,
    record_swarm_event, set_member_task_label, truncate_detail, update_member_status,
    update_member_status_with_report,
};
use crate::agent::Agent;
use crate::plan::{
    assignment_affinities_for_task, combine_assignment_text, explicit_task_blocked_reason,
    next_handover_runnable_item_id,
};
use crate::protocol::SwarmLifecycleStatus;
use crate::protocol::{NotificationType, ServerEvent};
use kcode_agent_runtime::SoftInterruptSource;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock, broadcast, mpsc};

/// Eligible auto-assignment targets for a swarm task.
///
/// Auto-pick must only land on sessions that will actually *execute* the work
/// without further human action. In a shared swarm there can be many foreign
/// members: independent human TUIs and stale "zombie" sessions left over from
/// other runs. Assigning to those silently strands the task (a human session is
/// never auto-driven; a zombie has no live agent at all) and stalls `run_plan`.
///
/// So a member is only a free worker for *automatic* selection when it is a
/// worker this run owns and can drive:
///
/// - `is_headless`: a spawned in-process worker (always auto-driven), or
/// - owned by the requester (`report_back_to_session_id == req`): a worker this
///   coordinator spawned, including reusable ones that already returned `ready`.
///
/// Everything else (foreign humans, zombies) must be addressed with an explicit
/// `target_session`, which bypasses this filter; this only governs auto-pick.
///
/// The status allowlist is also the whole busy rule: `ready` and `completed`
/// are the idle lifecycle states, and a member that is working or waiting on
/// work (`is_in_flight`, i.e. `queued`/`running`) is never idle. Busy is the
/// member's own in-flight work, so nothing here consults the plan.
///
/// Idleness is asked in one more place, the dispatch picker, which hands a ready
/// row back to a holder that can take it. Both read [`member_is_idle`].
fn filter_swarm_agent_candidates<'a>(
    members: &'a HashMap<String, SwarmMember>,
    req_session_id: &str,
    swarm_id: &str,
) -> Vec<&'a SwarmMember> {
    members
        .values()
        .filter(|member| {
            member.session_id != req_session_id
                && super::swarm::swarm_root(members, &member.session_id).as_deref()
                    == Some(swarm_id)
                && super::swarm::swarm_role(members, &member.session_id) == "agent"
                && member_is_idle(&member.status)
                && is_drivable_auto_worker(member, req_session_id)
        })
        .collect()
}

/// Whether a member can take a row right now: the idle lifecycle states. A
/// member that is `queued`/`running` is working, and one that is failed,
/// stopped or crashed can never come back (the salvage sweep frees its rows).
fn member_is_idle(status: &SwarmLifecycleStatus) -> bool {
    matches!(
        status,
        SwarmLifecycleStatus::Ready | SwarmLifecycleStatus::Completed
    )
}

/// Whether `member` can be auto-assigned a task and be relied on to run it.
/// See [`filter_swarm_agent_candidates`] for the rationale.
fn is_drivable_auto_worker(member: &SwarmMember, req_session_id: &str) -> bool {
    member.is_headless || member.report_back_to_session_id.as_deref() == Some(req_session_id)
}

/// Safety-net expiry for auto-pick claims that never reach an explicit
/// release (e.g. a request that dies between the pick and the plan write).
const AUTO_ASSIGN_CLAIM_TTL: std::time::Duration = std::time::Duration::from_secs(15);

/// In-process claims for auto-picked assignment targets, keyed by
/// `swarm_id\nsession_id`.
///
/// The plan records an assignment at a write that happens several awaits after
/// the target is picked, and until that write the member still looks idle.
/// Concurrent `assign_task`/`assign_next` requests resolve their targets inside
/// that window (observed live: three auto-picks within ~100ms all stacking onto
/// the same worker), so the pick itself must be a claim: the first resolver
/// wins the member, later ones skip it and fall back to another candidate or to
/// `spawn_if_needed`. Claims are released once the plan write records (or
/// abandons) the assignment; the TTL only reaps leaked claims.
fn auto_assign_claims() -> &'static std::sync::Mutex<HashMap<String, std::time::Instant>> {
    static CLAIMS: std::sync::OnceLock<std::sync::Mutex<HashMap<String, std::time::Instant>>> =
        std::sync::OnceLock::new();
    CLAIMS.get_or_init(|| std::sync::Mutex::new(HashMap::new()))
}

fn auto_assign_claim_key(swarm_id: &str, session_id: &str) -> String {
    format!("{swarm_id}\n{session_id}")
}

/// Claim `session_id` as an auto-pick target. Returns false when another
/// in-flight request already picked it (and that claim has not expired).
fn try_claim_auto_assign_target(swarm_id: &str, session_id: &str) -> bool {
    let mut claims = auto_assign_claims()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let now = std::time::Instant::now();
    claims.retain(|_, claimed_at| now.duration_since(*claimed_at) < AUTO_ASSIGN_CLAIM_TTL);
    match claims.entry(auto_assign_claim_key(swarm_id, session_id)) {
        std::collections::hash_map::Entry::Occupied(_) => false,
        std::collections::hash_map::Entry::Vacant(vacant) => {
            vacant.insert(now);
            true
        }
    }
}

fn release_auto_assign_claim(swarm_id: &str, session_id: &str) {
    let mut claims = auto_assign_claims()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    claims.remove(&auto_assign_claim_key(swarm_id, session_id));
}

/// Error for an auto-pick that found no assignable worker. The leading
/// sentence is a stable contract: `spawn_if_needed`/`run_plan` match on it to
/// decide to spawn a fresh agent instead of failing the assignment.
fn no_auto_target_error(claim_skipped: usize) -> String {
    let mut message =
        "No ready or completed swarm agents are available for automatic task assignment."
            .to_string();
    if claim_skipped > 0 {
        message.push_str(&format!(
            " Skipped {claim_skipped} worker(s) another in-flight assignment request just \
             picked; spawn a fresh agent (spawn_if_needed/prefer_spawn) or retry."
        ));
    }
    message
}

/// Pick the first unclaimed candidate from `candidates` (already ranked by the
/// caller) and claim it.
///
/// Idleness is the candidate filter's job (see
/// [`filter_swarm_agent_candidates`]), so all that is left here is the claim:
/// the plan records an assignment several awaits after the target is picked, so
/// two requests resolving inside that window must not choose the same member.
fn select_and_claim_auto_target(
    swarm_id: &str,
    candidates: &[&SwarmMember],
) -> Result<String, String> {
    let mut claim_skipped = 0usize;
    for member in candidates {
        if try_claim_auto_assign_target(swarm_id, &member.session_id) {
            return Ok(member.session_id.clone());
        }
        claim_skipped += 1;
    }
    Err(no_auto_target_error(claim_skipped))
}

/// Decide whether a worker's just-finished turn should auto-mark its assigned
/// node `done`.
///
/// A turn must NOT force-complete a node when the worker decomposed it into a
/// composite (`expanded`): that node is now a synthesis/join point that has to
/// wait for its children before it can close, and it will be re-woken to
/// synthesize. Likewise a node the worker already drove to a terminal status
/// (e.g. via `complete_node`, or that failed) must not be reopened/reclosed. A
/// node that is `queued` at turn end was re-queued mid-turn by someone else (a
/// reassign, a requeue): it is no longer this worker's to close. Only a plain,
/// still-running atomic turn auto-completes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TurnEndDisposition {
    /// The worker just ran an atomic node; mark it done.
    AutoComplete,
    /// The node is already terminal, queued (expanded this turn), or otherwise
    /// not this turn's responsibility.
    LeaveAlone,
}

/// Decide the turn-end disposition for a node.
///
/// A running atomic node auto-completes; an expanded composite stays open for
/// synthesis.
fn turn_end_disposition(status: &str, expanded: bool) -> TurnEndDisposition {
    if status != "running" {
        return TurnEndDisposition::LeaveAlone;
    }
    if expanded {
        // The worker decomposed the node; it must stay open to synthesize later.
        return TurnEndDisposition::LeaveAlone;
    }
    TurnEndDisposition::AutoComplete
}

#[cfg(test)]
fn turn_end_should_auto_complete(status: &str, expanded: bool) -> bool {
    turn_end_disposition(status, expanded) == TurnEndDisposition::AutoComplete
}

/// Assignment content for a (re-)dispatched node.
///
/// For a re-woken composite (`is_composite_synthesis`), the node's original
/// content is the now-stale decomposition brief, so replace it with an explicit
/// synthesis instruction that tells the planner to integrate its children and
/// finish with `complete_node`. Otherwise the original content is used verbatim.
fn composite_synthesis_content(
    item_id: &str,
    raw_content: &str,
    is_composite_synthesis: bool,
) -> String {
    if is_composite_synthesis {
        format!(
            "Synthesis turn for composite node '{item_id}'. Its children are complete; their \
             outputs are provided below. Read them, \
             write one synthesized result, and finish by calling `swarm complete_node` with \
             node_id=\"{item_id}\" and an artifact summarizing the integrated findings. Do NOT \
             call expand_node again. Original brief: {raw_content}"
        )
    } else {
        raw_content.to_string()
    }
}

async fn resolve_assignment_target_session(
    req_session_id: &str,
    swarm_id: &str,
    requested_target: Option<&str>,
    swarm_members: &Arc<RwLock<HashMap<String, SwarmMember>>>,
) -> Result<String, String> {
    let members = swarm_members.read().await;

    if let Some(target) = requested_target {
        if target == req_session_id {
            return Err("Coordinator cannot assign a swarm task to itself.".to_string());
        }
        if !members.contains_key(target) {
            return Err(format!("Unknown session '{target}'"));
        }
        if super::swarm::swarm_root(&members, target).as_deref() != Some(swarm_id) {
            return Err(format!(
                "Session '{}' is not in swarm '{}' and cannot receive this task.",
                target, swarm_id
            ));
        }
        return Ok(target.to_string());
    }

    let mut candidates = filter_swarm_agent_candidates(&members, req_session_id, swarm_id);

    candidates.sort_by(|left, right| {
        let left_rank = if left.status == SwarmLifecycleStatus::Ready {
            0
        } else {
            1
        };
        let right_rank = if right.status == SwarmLifecycleStatus::Ready {
            0
        } else {
            1
        };
        left_rank
            .cmp(&right_rank)
            .then_with(|| left.session_id.cmp(&right.session_id))
    });

    select_and_claim_auto_target(swarm_id, &candidates)
}

async fn next_handover_runnable_task_id(
    swarm_id: &str,
    req_session_id: &str,
    swarm_members: &Arc<RwLock<HashMap<String, SwarmMember>>>,
    swarm_runs: &Arc<RwLock<HashMap<String, RunState>>>,
) -> Option<String> {
    let run = swarm_runs
        .read()
        .await
        .get(swarm_id)
        .cloned()
        .unwrap_or_default();
    let rows = super::swarm::swarm_rows(swarm_id, req_session_id, swarm_members).await;
    let items = super::swarm::rows_with_run_status(&rows, &run);
    next_handover_runnable_item_id(&items, req_session_id)
}

/// Which row a dispatch takes, and how the row is already held.
enum Dispatch {
    /// Nobody holds the row, or the run itself does: either way the resolver picks
    /// a free worker and the claim moves to it.
    HandOver(String),
    /// A holder that can work the row already claims it, so the row goes back
    /// to that holder.
    HandBack { task_id: String, holder: String },
}

/// The row a dispatch should take next, asked in this order: a runnable row nobody
/// holds, or one this run holds and is handing over; then a runnable row a live
/// holder can take back (the assignment is the record of who owes it); then a
/// runnable row stranded on a holder that can never come back, whose claim is
/// cleared first.
///
/// The stranded rung is the requeue pickup: a row re-dispatched to a session that
/// then died (e.g. an auth-failure wave) sits `queued` and assigned to a corpse,
/// invisible to `next_handover_runnable_item_id`, and a still-running `run_plan`
/// driver reports "No runnable unassigned tasks" forever. Releasing the claim is
/// the whole recovery; nothing counts attempts here, because the bound belongs to
/// the loop that repeats the work.
async fn next_dispatch(
    swarm_id: &str,
    req_session_id: &str,
    swarm_runs: &Arc<RwLock<HashMap<String, RunState>>>,
    swarm_members: &Arc<RwLock<HashMap<String, SwarmMember>>>,
) -> Option<Dispatch> {
    if let Some(task_id) =
        next_handover_runnable_task_id(swarm_id, req_session_id, swarm_members, swarm_runs).await
    {
        return Some(Dispatch::HandOver(task_id));
    }

    // Snapshot member liveness first so the plans locks are not held across the
    // members read lock (avoids lock-order inversions with paths that lock
    // members before plans).
    let (member_statuses, working_dir): (
        HashMap<String, SwarmLifecycleStatus>,
        Option<std::path::PathBuf>,
    ) = {
        let members = swarm_members.read().await;
        let statuses = members
            .values()
            .filter(|member| {
                super::swarm::swarm_root(&members, &member.session_id).as_deref() == Some(swarm_id)
            })
            .map(|member| (member.session_id.clone(), member.status.clone()))
            .collect();
        let working_dir = members
            .get(req_session_id)
            .and_then(|member| member.working_dir.clone());
        (statuses, working_dir)
    };
    let holder_can_work =
        |session_id: &str| member_statuses.get(session_id).is_some_and(member_is_idle);
    let assignee_is_dead = |session_id: &str| -> bool {
        match member_statuses.get(session_id) {
            Some(status) => status.is_dead(),
            // Not a member of this swarm anymore: nothing can drive it.
            None => true,
        }
    };

    let run = swarm_runs
        .read()
        .await
        .get(swarm_id)
        .cloned()
        .unwrap_or_default();
    let rows = super::swarm::swarm_rows(swarm_id, req_session_id, swarm_members).await;
    let items = super::swarm::rows_with_run_status(&rows, &run);

    if let Some((task_id, holder)) =
        crate::plan::next_held_runnable_item_id(&items, &holder_can_work, req_session_id)
    {
        return Some(Dispatch::HandBack { task_id, holder });
    }

    let stranded_id = crate::plan::next_stranded_runnable_item_id(&items, &assignee_is_dead)?;
    // braid: one of the two reclaim clocks; the periodic sweep
    // (`swarm.rs::salvage_dead_assignees`) is the other and it waits a grace period
    // this one does not. Unify when a liveness change is in scope.
    // The claim is released where the list is: a holder that can never come back
    // does not owe the row, so the next dispatch can pick it up. Nothing counts
    // attempts, because the bound belongs to the loop that repeats the work.
    if let Err(error) =
        crate::todo::release_row_on_disk(working_dir.as_deref(), req_session_id, &stranded_id)
    {
        crate::logging::warn(&format!(
            "swarm {swarm_id}: could not release stranded task '{stranded_id}' in the list: {error}"
        ));
        return None;
    }
    crate::logging::info(&format!(
        "swarm {}: reclaimed stranded task '{}' from dead assignee for re-dispatch",
        swarm_id, stranded_id
    ));
    Some(Dispatch::HandOver(stranded_id))
}

async fn resolve_assignment_target_for_task(
    req_session_id: &str,
    swarm_id: &str,
    task_id: &str,
    requested_target: Option<&str>,
    swarm_members: &Arc<RwLock<HashMap<String, SwarmMember>>>,
    swarm_runs: &Arc<RwLock<HashMap<String, RunState>>>,
) -> Result<String, String> {
    if requested_target.is_some() {
        return resolve_assignment_target_session(
            req_session_id,
            swarm_id,
            requested_target,
            swarm_members,
        )
        .await;
    }

    // A composite's synthesis re-wake needs no affinity here: the row keeps its
    // holder when it is decomposed, so the dispatch picker hands it straight back
    // (see [`next_dispatch`]). This resolver only ever chooses a worker for a row
    // nobody holds.
    let affinities = {
        let run = swarm_runs
            .read()
            .await
            .get(swarm_id)
            .cloned()
            .unwrap_or_default();
        let rows = super::swarm::swarm_rows(swarm_id, req_session_id, swarm_members).await;
        let items = super::swarm::rows_with_run_status(&rows, &run);
        if items.is_empty() {
            return Err("No runnable unassigned tasks are available in the swarm plan".to_string());
        }
        assignment_affinities_for_task(&items, task_id)?
    };

    let members = swarm_members.read().await;
    let mut candidates = filter_swarm_agent_candidates(&members, req_session_id, swarm_id);

    candidates.sort_by(|left, right| {
        let left_carry = affinities
            .dependency_carryover
            .get(&left.session_id)
            .copied()
            .unwrap_or(0);
        let right_carry = affinities
            .dependency_carryover
            .get(&right.session_id)
            .copied()
            .unwrap_or(0);
        let left_meta = affinities
            .metadata_carryover
            .get(&left.session_id)
            .copied()
            .unwrap_or(0);
        let right_meta = affinities
            .metadata_carryover
            .get(&right.session_id)
            .copied()
            .unwrap_or(0);
        let left_rank = if left.status == SwarmLifecycleStatus::Ready {
            0
        } else {
            1
        };
        let right_rank = if right.status == SwarmLifecycleStatus::Ready {
            0
        } else {
            1
        };
        right_carry
            .cmp(&left_carry)
            .then_with(|| right_meta.cmp(&left_meta))
            .then_with(|| left_rank.cmp(&right_rank))
            .then_with(|| left.session_id.cmp(&right.session_id))
    });

    select_and_claim_auto_target(swarm_id, &candidates)
}

#[expect(
    clippy::too_many_arguments,
    reason = "task execution restart needs session state, plan state, and event sinks together"
)]
fn spawn_assigned_task_run(
    agent_arc: Arc<Mutex<Agent>>,
    target_session: String,
    swarm_id: String,
    task_id: String,
    assignment_text: String,
    swarm_members: Arc<RwLock<HashMap<String, SwarmMember>>>,
    swarm_runs: Arc<RwLock<HashMap<String, RunState>>>,
    event_history: Arc<RwLock<std::collections::VecDeque<SwarmEvent>>>,
    event_counter: Arc<std::sync::atomic::AtomicU64>,
    swarm_event_tx: broadcast::Sender<SwarmEvent>,
) {
    let assignment_text = append_swarm_completion_report_instructions(&assignment_text);
    tokio::spawn(async move {
        {
            let mut runs = swarm_runs.write().await;
            super::swarm::set_run_status_for(&mut runs, &swarm_id, &task_id, "running");
        }
        let swarm_state = SwarmState {
            members: Arc::clone(&swarm_members),
            runs: Arc::clone(&swarm_runs),
        };
        persist_swarm_state_for(&swarm_id, &swarm_state).await;
        broadcast_swarm_plan(
            &swarm_id,
            Some("task_running".to_string()),
            &swarm_runs,
            &swarm_members,
        )
        .await;
        set_member_task_label(&target_session, &assignment_text, &swarm_members).await;
        update_member_status(
            &target_session,
            SwarmLifecycleStatus::Running,
            Some(truncate_detail(&assignment_text, 120)),
            &swarm_members,
            Some(&event_history),
            Some(&event_counter),
            Some(&swarm_event_tx),
        )
        .await;

        let event_tx = task_progress_event_sender(
            target_session.clone(),
            Arc::clone(&swarm_members),
            Arc::clone(&event_history),
            Arc::clone(&event_counter),
            swarm_event_tx.clone(),
        );
        let start_message_index = {
            let agent = agent_arc.lock().await;
            agent.message_count()
        };
        let result = super::client_lifecycle::process_message_streaming_mpsc(
            Arc::clone(&agent_arc),
            &assignment_text,
            vec![],
            None,
            event_tx,
        )
        .await;
        let completion_report = if result.is_ok() {
            let agent = agent_arc.lock().await;
            agent.latest_assistant_text_after(start_message_index)
        } else {
            None
        };
        match result {
            Ok(_) => {
                // The list is the row storage: this turn's view of the run's rows,
                // read once for the whole tail.
                let rows =
                    super::swarm::swarm_rows(&swarm_id, &target_session, &swarm_members).await;
                let run = swarm_runs
                    .read()
                    .await
                    .get(&swarm_id)
                    .cloned()
                    .unwrap_or_default();
                let items = super::swarm::rows_with_run_status(&rows, &run);
                let previous_items = items.clone();
                // The row the turn was handed, and the list it lives in: the acting
                // session is the worker this turn ran as.
                let working_dir = swarm_members
                    .read()
                    .await
                    .get(&target_session)
                    .and_then(|member| member.working_dir.clone());
                let mut applied_disposition = TurnEndDisposition::LeaveAlone;
                // A worker turn ends in one of three ways for its row:
                //  1. it decomposed the row via `expand_node` -> the row is now a
                //     composite synthesis/join point that must stay open until its
                //     children finish; it is re-woken later to synthesize.
                //  2. it already closed the row via `complete_node` -> the row is
                //     gone from the list and the plan, and this turn is done.
                //  3. it just ran and the row is still in flight.
                // Case 3 closes the row: a turn that ended without a close states its
                // own outcome, which is the turn's report, or the fact that the turn
                // reported nothing. The store writes that close, exactly as
                // `complete_node` does, and the record lands where the row's work is
                // owned.
                let closes_its_row = {
                    let composite = crate::plan::is_composite(&rows, &task_id);
                    items
                        .iter()
                        .find(|item| item.id == task_id)
                        .is_some_and(|item| {
                            turn_end_disposition(&item.status, composite)
                                == TurnEndDisposition::AutoComplete
                        })
                };
                if closes_its_row {
                    applied_disposition = TurnEndDisposition::AutoComplete;
                    let report = completion_report
                        .as_deref()
                        .map(str::trim)
                        .filter(|report| !report.is_empty());
                    let result = report.unwrap_or("the turn ended without a report");
                    // The report is the artifact's findings too, so the row that
                    // integrates this work can read what this turn found.
                    let artifact = report.map(|report| serde_json::json!({ "findings": report }));
                    match crate::todo::close_row_on_disk(
                        working_dir.as_deref(),
                        &target_session,
                        &task_id,
                        result,
                        artifact,
                    ) {
                        Ok(_) => {
                            // The row is gone from the list, so the run's own entry for
                            // it goes too.
                            if let Some(run) = swarm_runs.write().await.get_mut(&swarm_id) {
                                run.remove(&task_id);
                            }
                        }
                        Err(error) => crate::logging::warn(&format!(
                            "swarm {swarm_id}: the turn for '{task_id}' ended but its row could not be closed: {error}"
                        )),
                    }
                }
                let swarm_state = SwarmState {
                    members: Arc::clone(&swarm_members),
                    runs: Arc::clone(&swarm_runs),
                };
                persist_swarm_state_for(&swarm_id, &swarm_state).await;
                let plan_reason = match applied_disposition {
                    TurnEndDisposition::AutoComplete => "task_closed",
                    TurnEndDisposition::LeaveAlone => "task_completed",
                };
                broadcast_swarm_plan_with_previous(
                    &swarm_id,
                    Some(plan_reason.to_string()),
                    Some(&previous_items),
                    &swarm_runs,
                    &swarm_members,
                )
                .await;
                // The worker's member status reflects its own turn (it ran to
                // completion); lifecycle and node state are separate axes.
                update_member_status_with_report(
                    &target_session,
                    SwarmLifecycleStatus::Completed,
                    None,
                    completion_report,
                    &swarm_members,
                    Some(&event_history),
                    Some(&event_counter),
                    Some(&swarm_event_tx),
                )
                .await;
            }
            Err(error) => {
                {
                    let mut runs = swarm_runs.write().await;
                    super::swarm::set_run_status_for(&mut runs, &swarm_id, &task_id, "failed");
                }
                let swarm_state = SwarmState {
                    members: Arc::clone(&swarm_members),
                    runs: Arc::clone(&swarm_runs),
                };
                persist_swarm_state_for(&swarm_id, &swarm_state).await;
                broadcast_swarm_plan(
                    &swarm_id,
                    Some("task_failed".to_string()),
                    &swarm_runs,
                    &swarm_members,
                )
                .await;
                update_member_status(
                    &target_session,
                    SwarmLifecycleStatus::Failed,
                    Some(truncate_detail(&error.to_string(), 120)),
                    &swarm_members,
                    Some(&event_history),
                    Some(&event_counter),
                    Some(&swarm_event_tx),
                )
                .await;
            }
        }
    });
}

fn task_progress_event_sender(
    session_id: String,
    swarm_members: Arc<RwLock<HashMap<String, SwarmMember>>>,
    event_history: Arc<RwLock<std::collections::VecDeque<SwarmEvent>>>,
    event_counter: Arc<std::sync::atomic::AtomicU64>,
    swarm_event_tx: broadcast::Sender<SwarmEvent>,
) -> mpsc::UnboundedSender<ServerEvent> {
    let (tx, mut rx) = mpsc::unbounded_channel::<ServerEvent>();
    tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            // A busy worker shows through its member status, which this keeps
            // current from the turn's own events; the turn loop marks the
            // activity clock, so no per-task record is needed for liveness.
            let detail = match &event {
                ServerEvent::StatusDetail { detail } => Some(detail.clone()),
                ServerEvent::ToolStart { name, .. } => Some(format!("tool start: {name}")),
                ServerEvent::ToolDone { name, error, .. } => Some(if error.is_some() {
                    format!("tool error: {name}")
                } else {
                    format!("tool done: {name}")
                }),
                _ => None,
            };

            if let Some(detail) = detail {
                update_member_status(
                    &session_id,
                    SwarmLifecycleStatus::Running,
                    Some(truncate_detail(&detail, 120)),
                    &swarm_members,
                    Some(&event_history),
                    Some(&event_counter),
                    Some(&swarm_event_tx),
                )
                .await;
            }

            let _ = fanout_session_event(&swarm_members, &session_id, event).await;
        }
    });
    tx
}

/// How an assign_task request interacts with the durable mutation dedup layer.
#[derive(Clone, Copy, PartialEq, Eq)]
enum AssignDedupMode {
    /// Direct client requests: an identical request within the final-state
    /// TTL replays the persisted response instead of re-dispatching. This
    /// absorbs client-side retries of the same logical request.
    ReplayFinal,
    /// Task-control-driven dispatches (retry/reassign/replace/salvage): each
    /// invocation is a deliberate new attempt, so a persisted success from a
    /// previous identical attempt must not swallow the re-dispatch (a worker
    /// failing within the TTL would otherwise make the coordinator's retry a
    /// silent no-op). Concurrent in-flight duplicates still coalesce.
    AlwaysDispatch,
}

#[expect(
    clippy::too_many_arguments,
    reason = "task assignment coordinates sessions, interrupts, connections, swarm plan state, and event history"
)]
pub(super) async fn handle_comm_assign_task(
    id: u64,
    req_session_id: String,
    target_session: Option<String>,
    task_id: Option<String>,
    message: Option<String>,
    redispatch: bool,
    client_event_tx: &mpsc::UnboundedSender<ServerEvent>,
    sessions: &SessionAgents,
    soft_interrupt_queues: &super::SessionInterruptQueues,
    client_connections: &Arc<RwLock<HashMap<String, ClientConnectionInfo>>>,
    swarm_members: &Arc<RwLock<HashMap<String, SwarmMember>>>,
    swarm_runs: &Arc<RwLock<HashMap<String, RunState>>>,
    event_history: &Arc<RwLock<std::collections::VecDeque<SwarmEvent>>>,
    event_counter: &Arc<std::sync::atomic::AtomicU64>,
    swarm_event_tx: &broadcast::Sender<SwarmEvent>,
    swarm_mutation_runtime: &SwarmMutationRuntime,
) {
    handle_comm_assign_task_with_mode(
        id,
        req_session_id,
        target_session,
        task_id,
        message,
        if redispatch {
            AssignDedupMode::AlwaysDispatch
        } else {
            AssignDedupMode::ReplayFinal
        },
        client_event_tx,
        sessions,
        soft_interrupt_queues,
        client_connections,
        swarm_members,
        swarm_runs,
        event_history,
        event_counter,
        swarm_event_tx,
        swarm_mutation_runtime,
    )
    .await;
}

#[expect(
    clippy::too_many_arguments,
    reason = "task assignment coordinates sessions, interrupts, connections, swarm plan state, and event history"
)]
async fn handle_comm_assign_task_with_mode(
    id: u64,
    req_session_id: String,
    target_session: Option<String>,
    task_id: Option<String>,
    message: Option<String>,
    dedup_mode: AssignDedupMode,
    client_event_tx: &mpsc::UnboundedSender<ServerEvent>,
    sessions: &SessionAgents,
    soft_interrupt_queues: &super::SessionInterruptQueues,
    client_connections: &Arc<RwLock<HashMap<String, ClientConnectionInfo>>>,
    swarm_members: &Arc<RwLock<HashMap<String, SwarmMember>>>,
    swarm_runs: &Arc<RwLock<HashMap<String, RunState>>>,
    event_history: &Arc<RwLock<std::collections::VecDeque<SwarmEvent>>>,
    event_counter: &Arc<std::sync::atomic::AtomicU64>,
    swarm_event_tx: &broadcast::Sender<SwarmEvent>,
    swarm_mutation_runtime: &SwarmMutationRuntime,
) {
    let requested_target_session = target_session.and_then(|target| {
        let trimmed = target.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    });
    let requested_task_id = task_id.and_then(|task_id| {
        let trimmed = task_id.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    });

    let swarm_id = match require_run_root(id, &req_session_id, client_event_tx, swarm_members).await
    {
        Some(swarm_id) => swarm_id,
        None => return,
    };

    let mutation_key = swarm_mutation_request_key(
        &req_session_id,
        "assign_task",
        &[
            swarm_id.clone(),
            requested_target_session
                .clone()
                .unwrap_or_else(|| "__next_available__".to_string()),
            requested_task_id
                .clone()
                .unwrap_or_else(|| "__next_runnable__".to_string()),
            message.clone().unwrap_or_default(),
        ],
    );
    let mutation_state = match dedup_mode {
        AssignDedupMode::ReplayFinal => {
            begin_swarm_mutation_or_replay(
                swarm_mutation_runtime,
                &mutation_key,
                "assign_task",
                &req_session_id,
                id,
                client_event_tx,
            )
            .await
        }
        AssignDedupMode::AlwaysDispatch => {
            begin_swarm_mutation_no_replay(
                swarm_mutation_runtime,
                &mutation_key,
                "assign_task",
                &req_session_id,
                id,
                client_event_tx,
            )
            .await
        }
    };
    let Some(mutation_state) = mutation_state else {
        return;
    };

    let target_session = match resolve_assignment_target_session(
        &req_session_id,
        &swarm_id,
        requested_target_session.as_deref(),
        swarm_members,
    )
    .await
    {
        Ok(target_session) => target_session,
        Err(message) => {
            finish_swarm_mutation_request(
                swarm_mutation_runtime,
                &mutation_state,
                PersistedSwarmMutationResponse::Error {
                    message,
                    retry_after_secs: None,
                },
            )
            .await;
            return;
        }
    };

    // The list a dispatch writes lives in the run's repo, the same root a seed
    // reads, and the acting session's member record names it.
    let working_dir = swarm_members
        .read()
        .await
        .get(&req_session_id)
        .and_then(|member| member.working_dir.clone());

    // The list is where the rows are, so a dispatch reads it: the run's own rows and
    // the unclaimed ones are what it may hand out.
    let rows = super::swarm::swarm_rows(&swarm_id, &req_session_id, swarm_members).await;
    let (selected_task_id, task_content, plan_item_count, blocked_reason) = {
        let run = swarm_runs
            .read()
            .await
            .get(&swarm_id)
            .cloned()
            .unwrap_or_default();
        let items = super::swarm::rows_with_run_status(&rows, &run);
        let selected_task_id = requested_task_id
            .clone()
            .or_else(|| next_handover_runnable_item_id(&items, &req_session_id));
        let blocked_reason = {
            requested_task_id
                .as_deref()
                .and_then(|task_id| explicit_task_blocked_reason(&items, task_id))
        };
        // The holder is a fact about the list (rule 2), so the claim is written
        // where the list is, before the plan records it: a failed write fails the
        // dispatch instead of leaving the plan holding a row the list does not.
        // This is the shape the expand path already has.
        let blocked_reason = match (&blocked_reason, selected_task_id.as_deref()) {
            (None, Some(task_id)) => {
                match crate::todo::claim_row_on_disk(
                    working_dir.as_deref(),
                    &req_session_id,
                    task_id,
                    &target_session,
                ) {
                    // The row the store wrote is the row: nothing is copied.
                    Ok(_) => None,
                    Err(error) => Some(format!(
                        "Task '{task_id}' could not be claimed in the list: {error}"
                    )),
                }
            }
            _ => blocked_reason,
        };
        let found = if blocked_reason.is_some() {
            None
        } else {
            selected_task_id
                .as_ref()
                .and_then(|id| items.iter().find(|item| item.id == *id).cloned())
        };
        if let Some(item) = found {
            let item_id = item.id.clone();
            // A re-woken composite is the synthesis/join step: its original content
            // was the (now-stale) decomposition brief, so replace it with an explicit
            // synthesis instruction. Without this the planner replays the old "expand
            // me" prompt and reports instead of calling `complete_node`, leaving the
            // composite open forever.
            let is_composite_synthesis = crate::plan::is_composite(&rows, &item_id);
            let effective_content =
                composite_synthesis_content(&item_id, &item.content, is_composite_synthesis);
            let content =
                kcode_plan::bridge::hydrate_assignment(&items, &item_id, &effective_content);
            (Some(item_id), Some(content), items.len(), None)
        } else {
            (None, None, 0, blocked_reason)
        }
    };
    if let Some(task_id) = selected_task_id.as_ref() {
        let mut runs = swarm_runs.write().await;
        super::swarm::set_run_status_for(&mut runs, &swarm_id, task_id, "queued");
    }

    // The plan write above either recorded the assignment (which puts the
    // target's own status in flight) or abandoned it (no runnable task /
    // blocked), so an auto-picked target's in-flight claim is released in both
    // cases. Explicit targets never claimed; `assign_next` releases its own
    // pre-claimed pick after this handler returns.
    if requested_target_session.is_none() {
        release_auto_assign_claim(&swarm_id, &target_session);
    }

    let Some(selected_task_id) = selected_task_id else {
        let message = blocked_reason.unwrap_or_else(|| {
            requested_task_id.as_ref().map_or_else(
                || "No runnable unassigned tasks are available in the swarm plan".to_string(),
                |task_id| format!("Task '{}' not found in swarm plan", task_id),
            )
        });
        finish_swarm_mutation_request(
            swarm_mutation_runtime,
            &mutation_state,
            PersistedSwarmMutationResponse::Error {
                message,
                retry_after_secs: None,
            },
        )
        .await;
        return;
    };
    let Some(content) = task_content else {
        finish_swarm_mutation_request(
            swarm_mutation_runtime,
            &mutation_state,
            PersistedSwarmMutationResponse::Error {
                message: format!(
                    "Task '{}' could not be assigned because its content was unavailable.",
                    selected_task_id
                ),
                retry_after_secs: None,
            },
        )
        .await;
        return;
    };

    let swarm_state = SwarmState {
        members: Arc::clone(swarm_members),
        runs: Arc::clone(swarm_runs),
    };
    persist_swarm_state_for(&swarm_id, &swarm_state).await;

    broadcast_swarm_plan(
        &swarm_id,
        Some("task_assigned".to_string()),
        swarm_runs,
        swarm_members,
    )
    .await;
    record_swarm_event(
        event_history,
        event_counter,
        swarm_event_tx,
        req_session_id.clone(),
        None,
        Some(swarm_id.clone()),
        SwarmEventType::PlanUpdate {
            swarm_id: swarm_id.clone(),
            item_count: plan_item_count,
        },
    )
    .await;

    let coordinator_name = {
        let members = swarm_members.read().await;
        members
            .get(&req_session_id)
            .and_then(|member| member.friendly_name.clone())
    };
    let notification = if let Some(ref extra) = message {
        format!(
            "Task assigned to you by coordinator: {} — {}",
            content, extra
        )
    } else {
        format!("Task assigned to you by coordinator: {}", content)
    };
    let queued_task_prompt = append_swarm_completion_report_instructions(&notification);
    let assignment_text = combine_assignment_text(&content, message.as_deref());
    set_member_task_label(&target_session, &assignment_text, swarm_members).await;
    update_member_status(
        &target_session,
        SwarmLifecycleStatus::Queued,
        Some(truncate_detail(&assignment_text, 120)),
        swarm_members,
        Some(event_history),
        Some(event_counter),
        Some(swarm_event_tx),
    )
    .await;

    let target_agent = {
        let agent_sessions = sessions.read().await;
        agent_sessions.get(&target_session).cloned()
    };
    let _ = queue_soft_interrupt_for_session(
        &target_session,
        queued_task_prompt,
        false,
        SoftInterruptSource::System,
        soft_interrupt_queues,
        sessions,
    )
    .await;
    if let Some(member) = swarm_members.read().await.get(&target_session) {
        let _ = member.event_tx.send(ServerEvent::Notification {
            from_session: req_session_id.clone(),
            from_name: coordinator_name.clone(),
            notification_type: NotificationType::Message {
                scope: Some("dm".to_string()),
                channel: None,
                tldr: None,
            },
            message: notification,
        });
    }

    let target_has_client = {
        let connections = client_connections.read().await;
        connections
            .values()
            .any(|connection| connection.session_id == target_session)
    };
    if !target_has_client && let Some(agent_arc) = target_agent {
        let target_session_for_run = target_session.clone();
        let swarm_members_for_run = Arc::clone(swarm_members);
        let swarm_runs_for_run = Arc::clone(swarm_runs);
        let swarm_id_for_run = swarm_id.clone();
        let task_id_for_run = selected_task_id.clone();
        let event_history_for_run = Arc::clone(event_history);
        let event_counter_for_run = Arc::clone(event_counter);
        let swarm_event_tx_for_run = swarm_event_tx.clone();
        spawn_assigned_task_run(
            agent_arc,
            target_session_for_run,
            swarm_id_for_run,
            task_id_for_run,
            assignment_text,
            swarm_members_for_run,
            swarm_runs_for_run,
            event_history_for_run,
            event_counter_for_run,
            swarm_event_tx_for_run,
        );
    }

    let plan_msg = format!(
        "Plan updated: task '{}' assigned to {}.",
        selected_task_id, target_session
    );
    let swarm_members_for_notice = super::swarm::swarm_session_ids(&swarm_id, swarm_members).await;
    let members = swarm_members.read().await;
    for sid in swarm_members_for_notice {
        if sid == target_session || sid == req_session_id {
            continue;
        }
        if let Some(member) = members.get(&sid) {
            let _ = member.event_tx.send(ServerEvent::Notification {
                from_session: req_session_id.clone(),
                from_name: coordinator_name.clone(),
                notification_type: NotificationType::Message {
                    scope: Some("plan".to_string()),
                    channel: None,
                    tldr: None,
                },
                message: plan_msg.clone(),
            });
        }
    }

    finish_swarm_mutation_request(
        swarm_mutation_runtime,
        &mutation_state,
        PersistedSwarmMutationResponse::AssignTask {
            task_id: selected_task_id,
            target_session,
        },
    )
    .await;
}

#[expect(
    clippy::too_many_arguments,
    reason = "assign_next reuses task assignment orchestration and forwards the same runtime dependencies"
)]
pub(super) async fn handle_comm_assign_next(
    id: u64,
    req_session_id: String,
    target_session: Option<String>,
    working_dir: Option<String>,
    prefer_spawn: Option<bool>,
    spawn_if_needed: Option<bool>,
    message: Option<String>,
    model: Option<String>,
    effort: Option<String>,
    client_event_tx: &mpsc::UnboundedSender<ServerEvent>,
    sessions: &SessionAgents,
    global_session_id: &Arc<RwLock<String>>,
    provider_template: &Arc<dyn crate::provider::Provider>,
    soft_interrupt_queues: &super::SessionInterruptQueues,
    client_connections: &Arc<RwLock<HashMap<String, ClientConnectionInfo>>>,
    swarm_members: &Arc<RwLock<HashMap<String, SwarmMember>>>,
    swarm_runs: &Arc<RwLock<HashMap<String, RunState>>>,
    event_history: &Arc<RwLock<std::collections::VecDeque<SwarmEvent>>>,
    event_counter: &Arc<std::sync::atomic::AtomicU64>,
    swarm_event_tx: &broadcast::Sender<SwarmEvent>,
    mcp_pool: &Arc<crate::mcp::SharedMcpPool>,
    swarm_mutation_runtime: &SwarmMutationRuntime,
) {
    if target_session.is_none() {
        let swarm_id =
            match require_run_root(id, &req_session_id, client_event_tx, swarm_members).await {
                Some(swarm_id) => swarm_id,
                None => return,
            };

        let Some(dispatch) =
            next_dispatch(&swarm_id, &req_session_id, swarm_runs, swarm_members).await
        else {
            let _ = client_event_tx.send(ServerEvent::Error {
                id,
                message: "No runnable unassigned tasks are available in the swarm plan".to_string(),
                retry_after_secs: None,
            });
            return;
        };

        // A row that already names a holder goes back to that holder, and no
        // fresh agent is spawned for it: the assignment is the record of who owes
        // the work, so a caller's fresh-agent preference governs rows nobody
        // holds. The re-dispatch is deliberate, like `retry`, so a stored answer
        // to an earlier identical request must not swallow it.
        if let Dispatch::HandBack { task_id, holder } = dispatch {
            handle_comm_assign_task_with_mode(
                id,
                req_session_id,
                Some(holder),
                Some(task_id),
                message,
                AssignDedupMode::AlwaysDispatch,
                client_event_tx,
                sessions,
                soft_interrupt_queues,
                client_connections,
                swarm_members,
                swarm_runs,
                event_history,
                event_counter,
                swarm_event_tx,
                swarm_mutation_runtime,
            )
            .await;
            return;
        }
        let Dispatch::HandOver(selected_task_id) = dispatch else {
            unreachable!("the hand-back case returned above")
        };

        let preferred_target = resolve_assignment_target_for_task(
            &req_session_id,
            &swarm_id,
            &selected_task_id,
            None,
            swarm_members,
            swarm_runs,
        )
        .await;

        if (prefer_spawn.unwrap_or(false) || spawn_if_needed.unwrap_or(false))
            && (prefer_spawn.unwrap_or(false) || preferred_target.is_err())
        {
            // A prefer_spawn run can reach here with a successfully resolved
            // (and therefore claimed) reuse target it will never use; free it
            // for concurrent picks.
            if let Ok(unused_target) = &preferred_target {
                release_auto_assign_claim(&swarm_id, unused_target);
            }
            match super::comm_session::spawn_swarm_agent(
                &req_session_id,
                &swarm_id,
                working_dir.clone(),
                None,
                None,
                model.clone(),
                effort.clone(),
                None,
                sessions,
                global_session_id,
                provider_template,
                swarm_members,
                swarm_runs,
                event_history,
                event_counter,
                swarm_event_tx,
                mcp_pool,
                soft_interrupt_queues,
                client_connections,
            )
            .await
            {
                Ok(spawned_session) => {
                    handle_comm_assign_task(
                        id,
                        req_session_id,
                        Some(spawned_session),
                        Some(selected_task_id),
                        message,
                        false,
                        client_event_tx,
                        sessions,
                        soft_interrupt_queues,
                        client_connections,
                        swarm_members,
                        swarm_runs,
                        event_history,
                        event_counter,
                        swarm_event_tx,
                        swarm_mutation_runtime,
                    )
                    .await;
                    return;
                }
                Err(error) => {
                    let _ = client_event_tx.send(ServerEvent::Error {
                        id,
                        message: format!("Failed to spawn preferred worker: {error}"),
                        retry_after_secs: None,
                    });
                    return;
                }
            }
        }

        match preferred_target {
            Ok(target_session) => {
                handle_comm_assign_task(
                    id,
                    req_session_id,
                    Some(target_session.clone()),
                    Some(selected_task_id),
                    message,
                    false,
                    client_event_tx,
                    sessions,
                    soft_interrupt_queues,
                    client_connections,
                    swarm_members,
                    swarm_runs,
                    event_history,
                    event_counter,
                    swarm_event_tx,
                    swarm_mutation_runtime,
                )
                .await;
                // The pick was claimed at resolve time; by now the assignment
                // either landed in the plan or was rejected, so the in-flight
                // claim is spent either way.
                release_auto_assign_claim(&swarm_id, &target_session);
            }
            Err(message) => {
                let _ = client_event_tx.send(ServerEvent::Error {
                    id,
                    message,
                    retry_after_secs: None,
                });
            }
        }
        return;
    }

    handle_comm_assign_task(
        id,
        req_session_id,
        target_session,
        None,
        message,
        false,
        client_event_tx,
        sessions,
        soft_interrupt_queues,
        client_connections,
        swarm_members,
        swarm_runs,
        event_history,
        event_counter,
        swarm_event_tx,
        swarm_mutation_runtime,
    )
    .await;
}

#[cfg(test)]
#[path = "comm_control_tests.rs"]
mod tests;

pub(super) async fn handle_client_debug_command(
    id: u64,
    client_event_tx: &mpsc::UnboundedSender<ServerEvent>,
) {
    let _ = client_event_tx.send(ServerEvent::Error {
        id,
        message: "ClientDebugCommand is for internal use only".to_string(),
        retry_after_secs: None,
    });
}

pub(super) fn handle_client_debug_response(
    id: u64,
    output: String,
    client_debug_response_tx: &broadcast::Sender<(u64, String)>,
) {
    let _ = client_debug_response_tx.send((id, output));
}

/// The run's root may dispatch its run's rows.
///
/// Membership is the whole authorization to work a run's rows, and a member's
/// deeper work is rows the run hands it: only the session that roots the run drives
/// the list, so a worker cannot hand its row to anyone, itself included.
async fn require_run_root(
    id: u64,
    req_session_id: &str,
    client_event_tx: &mpsc::UnboundedSender<ServerEvent>,
    swarm_members: &Arc<RwLock<HashMap<String, SwarmMember>>>,
) -> Option<String> {
    let (root, is_root) = {
        let members = swarm_members.read().await;
        (
            super::swarm::swarm_root(&members, req_session_id),
            super::swarm::swarm_is_root(&members, req_session_id),
        )
    };
    let Some(root) = root else {
        let _ = client_event_tx.send(ServerEvent::Error {
            id,
            message: "Not in a swarm.".to_string(),
            retry_after_secs: None,
        });
        return None;
    };
    if !is_root {
        let _ = client_event_tx.send(ServerEvent::Error {
            id,
            message: "Only the session that roots this run may dispatch its work; a member works the rows the run hands it.".to_string(),
            retry_after_secs: None,
        });
        return None;
    }
    Some(root)
}
