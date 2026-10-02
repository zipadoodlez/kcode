use super::state::{MAX_EVENT_HISTORY, fanout_session_event};
use super::{RunState, SwarmEvent, SwarmEventType, SwarmMember, SwarmState};

/// The rows a swarm works: the open rows its members hold, the rows nobody holds,
/// and the rows whose holder is no longer a member, in file order.
///
/// The list is the one place rows live, so a run reads them instead of keeping a
/// copy: its own rows come back because its members hold them; an unclaimed row is
/// anyone's to take, which is also how a row released by a salvage returns; and a row
/// whose holder has vanished is nobody's live work, which is what the sweep reclaims.
/// A row a live session of another swarm holds stays out.
pub(super) async fn swarm_rows(
    swarm_id: &str,
    requester: &str,
    swarm_members: &Arc<RwLock<HashMap<String, SwarmMember>>>,
) -> Vec<TaskItem> {
    let (working_dir, held_by, members) = {
        let members = swarm_members.read().await;
        let held_by: HashSet<String> = members
            .values()
            .filter(|member| member.swarm_id.as_deref() == Some(swarm_id))
            .map(|member| member.session_id.clone())
            .collect();
        let working_dir = members
            .get(requester)
            .and_then(|member| member.working_dir.clone())
            .or_else(|| {
                members
                    .values()
                    .find(|member| member.swarm_id.as_deref() == Some(swarm_id))
                    .and_then(|member| member.working_dir.clone())
            });
        (working_dir, held_by, members)
    };
    let Ok(rows) = crate::todo::load_tasks(working_dir.as_deref(), requester) else {
        return Vec::new();
    };
    rows.into_iter()
        .filter(|row| match row.assigned_to.as_deref() {
            None => true,
            Some(holder) => held_by.contains(holder) || !members.contains_key(holder),
        })
        .collect()
}

/// The rows as the run sees them: a row the run set a status for reads that status,
/// and a row it did not reads its own. Cheap because the run's map holds only the
/// rows a turn touched.
pub(super) fn rows_with_run_status(rows: &[TaskItem], run: &RunState) -> Vec<TaskItem> {
    rows.iter()
        .map(|row| {
            let mut row = row.clone();
            if let Some(state) = run.get(&row.id)
                && !state.status.is_empty()
            {
                row.status = state.status.clone();
            }
            row
        })
        .collect()
}

/// Set a row's run status for a swarm, creating the swarm's run state when it has
/// none yet: a row a dispatch just touched is work whatever the map held before.
pub(super) fn set_run_status_for(
    runs: &mut HashMap<String, RunState>,
    swarm_id: &str,
    row_id: &str,
    status: &str,
) {
    let run = runs.entry(swarm_id.to_string()).or_default();
    set_run_status(run, row_id, status);
}

/// The run's lifecycle for one row, in the run's own map.
pub(super) fn set_run_status(run: &mut RunState, id: &str, status: &str) {
    run.entry(id.to_string()).or_default().status = status.to_string();
}
use super::persist_swarm_state_for;
use crate::agent::Agent;
use crate::plan::{TaskItem, newly_ready_item_ids, summarize_plan_graph};
use crate::protocol::{NotificationType, ServerEvent, SwarmLifecycleStatus};
use crate::session::Session;
use anyhow::Result;
use futures::future::try_join_all;
use kcode_swarm_core::{completion_notification_message, normalize_completion_report};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Mutex as StdMutex, OnceLock};
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, RwLock, broadcast};

fn status_age_secs(last_status_change: Instant) -> u64 {
    last_status_change.elapsed().as_secs()
}

/// Maximum number of live members (agents) in a single swarm. Re-exported from
/// `kcode_swarm_core` so the server, tools, and prompts all agree on the one
/// runaway-prevention cap for the task-graph model. Normal and light swarms are
/// root-only, one-level fan-out. Deep-swarm roots may create recursive trees with
/// no depth limit, but both the configurable live-worker budget and this absolute
/// cap still apply.
pub(super) use kcode_swarm_core::MAX_SWARM_MEMBERS;

/// Walk the `report_back_to_session_id` chain upward from `session_id`,
/// returning the list of ancestor session ids (parent first, root last).
///
/// The spawner/parent edge is encoded by `report_back_to_session_id`: a child
/// spawned by `P` reports back to `P`. Walking that chain reconstructs the spawn
/// tree without persisting a separate parent field. Cycles (which should never
/// happen) are guarded against with a visited set.
pub(super) fn swarm_ancestors(
    members: &HashMap<String, SwarmMember>,
    session_id: &str,
) -> Vec<String> {
    let mut ancestors = Vec::new();
    let mut visited: HashSet<String> = HashSet::new();
    visited.insert(session_id.to_string());
    let mut current = session_id.to_string();
    while let Some(parent) = members
        .get(&current)
        .and_then(|member| member.report_back_to_session_id.clone())
    {
        if parent == current || !visited.insert(parent.clone()) {
            break;
        }
        ancestors.push(parent.clone());
        current = parent;
    }
    ancestors
}

/// Depth of `session_id` in the spawn tree: number of ancestors reachable via
/// the report-back chain. Root coordinators (no report-back owner) are depth 0.
///
/// Test-only: the spawn tree no longer enforces a depth cap, so production code
/// does not consult depth. Kept (behind `cfg(test)`) because the spawn-tree tests
/// assert ancestor-chain depth directly.
#[cfg(test)]
pub(super) fn swarm_spawn_depth(members: &HashMap<String, SwarmMember>, session_id: &str) -> u32 {
    swarm_ancestors(members, session_id).len() as u32
}

/// True when `ancestor` is `session_id` itself or any transitive spawner of it.
/// Used to decide whether a requester may manage (stop/control) a target: an
/// agent owns its entire spawned subtree.
pub(super) fn swarm_is_self_or_ancestor(
    members: &HashMap<String, SwarmMember>,
    ancestor: &str,
    session_id: &str,
) -> bool {
    ancestor == session_id
        || swarm_ancestors(members, session_id)
            .iter()
            .any(|candidate| candidate == ancestor)
}

const DEFAULT_SWARM_STATUS_DEBOUNCE_MEMBER_THRESHOLD: usize = 2;
const DEFAULT_SWARM_STATUS_DEBOUNCE_MS: u64 = 75;
const DEFAULT_SWARM_TASK_STALE_AFTER_SECS: u64 = 45;
const DEFAULT_SWARM_TASK_SWEEP_INTERVAL_SECS: u64 = 5;
const DEFAULT_SWARM_TERMINAL_MEMBER_RETENTION_SECS: u64 = 24 * 60 * 60;
const DEFAULT_SWARM_TERMINAL_MEMBER_GC_INTERVAL_SECS: u64 = 60;
/// How long terminal members stay in live SwarmStatus broadcasts. Terminal
/// members remain queryable for the full retention window above, but
/// re-sending hundreds of long-finished members to every attached client on
/// every status change dominates broadcast payloads (measured ~240 KB of
/// member JSON resident per client with ~700 mostly-stopped members). Keep
/// them in broadcasts briefly so done/failed transition notices still fire,
/// then drop them from the live fan-out.
const DEFAULT_SWARM_STATUS_BROADCAST_TERMINAL_SECS: u64 = 15 * 60;
#[derive(Default, Clone, Copy)]
struct PendingSwarmStatusBroadcast {
    scheduled: bool,
    dirty: bool,
}

fn pending_swarm_status_broadcasts()
-> &'static StdMutex<HashMap<String, PendingSwarmStatusBroadcast>> {
    static PENDING: OnceLock<StdMutex<HashMap<String, PendingSwarmStatusBroadcast>>> =
        OnceLock::new();
    PENDING.get_or_init(|| StdMutex::new(HashMap::new()))
}

fn swarm_status_debounce_member_threshold() -> usize {
    static CACHED: OnceLock<AtomicUsize> = OnceLock::new();
    CACHED
        .get_or_init(|| {
            let configured = std::env::var("KCODE_SWARM_STATUS_DEBOUNCE_MEMBER_THRESHOLD")
                .ok()
                .and_then(|value| value.trim().parse::<usize>().ok())
                .filter(|value| *value > 0)
                .unwrap_or(DEFAULT_SWARM_STATUS_DEBOUNCE_MEMBER_THRESHOLD);
            AtomicUsize::new(configured)
        })
        .load(Ordering::Relaxed)
}

fn swarm_status_debounce_ms() -> u64 {
    static CACHED: OnceLock<AtomicU64> = OnceLock::new();
    CACHED
        .get_or_init(|| {
            let configured = std::env::var("KCODE_SWARM_STATUS_DEBOUNCE_MS")
                .ok()
                .and_then(|value| value.trim().parse::<u64>().ok())
                .filter(|value| *value > 0)
                .unwrap_or(DEFAULT_SWARM_STATUS_DEBOUNCE_MS);
            AtomicU64::new(configured)
        })
        .load(Ordering::Relaxed)
}

fn configured_positive_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

fn log_swarm_lifecycle(phase: &str, fields: Vec<(&str, String)>) {
    crate::logging::event_info(
        "SWARM_LIFECYCLE",
        Vec::from([("phase", phase.to_string())])
            .into_iter()
            .chain(fields)
            .collect::<Vec<_>>(),
    );
}

pub(super) fn swarm_task_stale_after() -> Duration {
    Duration::from_secs(configured_positive_u64(
        "KCODE_SWARM_TASK_STALE_AFTER_SECS",
        DEFAULT_SWARM_TASK_STALE_AFTER_SECS,
    ))
}

pub(super) fn swarm_task_sweep_interval() -> Duration {
    Duration::from_secs(configured_positive_u64(
        "KCODE_SWARM_TASK_SWEEP_INTERVAL_SECS",
        DEFAULT_SWARM_TASK_SWEEP_INTERVAL_SECS,
    ))
}

/// How long terminal members remain visible in the active swarm listing. This
/// keeps completion reports available for inspection without allowing durable
/// history to grow forever.
pub(super) fn swarm_terminal_member_retention() -> Duration {
    Duration::from_secs(configured_positive_u64(
        "KCODE_SWARM_TERMINAL_MEMBER_RETENTION_SECS",
        DEFAULT_SWARM_TERMINAL_MEMBER_RETENTION_SECS,
    ))
}

/// How often the live server removes terminal members whose retention window
/// has elapsed. Startup loading performs the same pruning synchronously.
pub(super) fn swarm_terminal_member_gc_interval() -> Duration {
    Duration::from_secs(configured_positive_u64(
        "KCODE_SWARM_TERMINAL_MEMBER_GC_INTERVAL_SECS",
        DEFAULT_SWARM_TERMINAL_MEMBER_GC_INTERVAL_SECS,
    ))
}

/// How long terminal members remain included in live SwarmStatus broadcasts.
/// See [`DEFAULT_SWARM_STATUS_BROADCAST_TERMINAL_SECS`].
pub(super) fn swarm_status_broadcast_terminal_retention() -> Duration {
    Duration::from_secs(configured_positive_u64(
        "KCODE_SWARM_STATUS_BROADCAST_TERMINAL_SECS",
        DEFAULT_SWARM_STATUS_BROADCAST_TERMINAL_SECS,
    ))
}

/// Whether a member belongs in live SwarmStatus broadcasts: every live member,
/// plus terminal members whose status changed recently enough that clients may
/// still want to announce or display the transition.
pub(super) fn member_in_status_broadcast(member: &SwarmMember, retention: Duration) -> bool {
    !member.status.is_terminal() || member.last_status_change.elapsed() < retention
}

pub(super) fn member_consumes_swarm_capacity(member: &SwarmMember) -> bool {
    !member.status.is_terminal()
}

pub(super) fn expired_terminal_member_ids(
    members: &HashMap<String, SwarmMember>,
    retention: Duration,
) -> Vec<String> {
    members
        .values()
        .filter(|member| member.status.is_terminal())
        .filter(|member| member.last_status_change.elapsed() >= retention)
        .map(|member| member.session_id.clone())
        .collect()
}

/// How long a finished spawned worker may sit idle before the server reaps it
/// (closes its client and removes the member). `0` disables reaping.
///
/// Spawned workers (visible windows and headless sessions) rely on their
/// coordinator calling `cleanup`, but ad hoc spawns and interrupted plans
/// leave them behind, where each idle client holds ~80-150 MB indefinitely.
/// The reaper is the backstop that keeps them from stacking up.
const DEFAULT_SWARM_IDLE_WORKER_REAP_SECS: u64 = 30 * 60;

pub(super) fn swarm_idle_worker_reap_after() -> Option<Duration> {
    let secs = std::env::var("KCODE_SWARM_IDLE_WORKER_REAP_SECS")
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .unwrap_or(DEFAULT_SWARM_IDLE_WORKER_REAP_SECS);
    (secs > 0).then(|| Duration::from_secs(secs))
}

/// Spawned workers whose work is finished (`ready` report-back or a terminal
/// status) and whose status has not changed for at least `idle_after`.
/// Only sessions spawned by another agent (`report_back_to_session_id` set)
/// and not holding the coordinator role are eligible; user-created sessions
/// are never reaped.
pub(super) fn idle_spawned_worker_reap_candidates(
    members: &HashMap<String, SwarmMember>,
    idle_after: Duration,
) -> Vec<String> {
    members
        .values()
        .filter(|member| member.report_back_to_session_id.is_some())
        .filter(|member| member.role != "coordinator")
        .filter(|member| {
            member.status == SwarmLifecycleStatus::Ready || member.status.is_terminal()
        })
        .filter(|member| member.last_status_change.elapsed() >= idle_after)
        .map(|member| member.session_id.clone())
        .collect()
}

/// Outcome of salvaging one dead member's rows.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct DeadMemberSalvage {
    /// Rows released back to the list for automatic re-dispatch.
    pub released_task_ids: Vec<String>,
}

impl DeadMemberSalvage {
    pub(super) fn is_empty(&self) -> bool {
        self.released_task_ids.is_empty()
    }

    /// Human-readable notification body for the coordinator/owner.
    fn describe(&self, worker_label: &str) -> String {
        let mut parts = vec![format!(
            "⚠ Worker {} died while holding swarm task assignment(s).",
            worker_label
        )];
        parts.push(format!(
            "Released for re-dispatch: {}.",
            self.released_task_ids.join(", ")
        ));
        parts.push(
            "Assign them again with assign_next/run_plan, or hand them to someone; they stay open either way."
                .to_string(),
        );
        parts.join(" ")
    }
}

/// Every non-terminal plan item assigned to `session_id`, back to `queued`.
///
/// This is the eager counterpart to the assign-time stranded-row reclaim: a worker
/// that crashes, stops, or leaves the swarm mid-task leaves its rows `running`/
/// `queued` and assigned to a corpse, where nothing picks them and a driving
/// `run_plan` stalls into its transient-stall error. Salvaging at the moment the
/// member dies converts that silent strand into work the list offers again. The
/// claim is released in the list by the caller; nothing counts attempts, because
/// the bound belongs to the loop that repeats the work.
fn held_row_ids(rows: &[TaskItem], run: &RunState, session_id: &str) -> Vec<String> {
    rows.iter()
        .filter(|row| row.assigned_to.as_deref() == Some(session_id))
        .map(|row| row.id.clone())
        .filter(|id| {
            let status = run
                .get(id)
                .map(|state| state.status.as_str())
                .unwrap_or(&rows[0].status);
            !crate::plan::is_terminal_status(status)
        })
        .collect()
}

/// Salvage `session_id`'s plan assignments in `swarm_id`, then persist,
/// broadcast the plan change, and notify the swarm coordinator so the death is
/// visible instead of silent. No-ops (and skips all I/O) when the member held
/// no non-terminal assignments.
pub(super) async fn salvage_assignments_of_dead_member(
    session_id: &str,
    swarm_id: &str,
    swarm_members: &Arc<RwLock<HashMap<String, SwarmMember>>>,
    swarms_by_id: &Arc<RwLock<HashMap<String, HashSet<String>>>>,
    swarm_runs: &Arc<RwLock<HashMap<String, RunState>>>,
    swarm_coordinators: &Arc<RwLock<HashMap<String, String>>>,
) -> DeadMemberSalvage {
    let rows = swarm_rows(swarm_id, session_id, swarm_members).await;
    let outcome = {
        let mut runs = swarm_runs.write().await;
        let run = runs.entry(swarm_id.to_string()).or_default();
        let mut outcome = DeadMemberSalvage::default();
        for task_id in held_row_ids(&rows, run, session_id) {
            set_run_status(run, &task_id, "queued");
            outcome.released_task_ids.push(task_id);
        }
        outcome
    };
    if outcome.is_empty() {
        return outcome;
    }

    // The dead holder does not owe these rows anymore, so the file stops naming it.
    // The rows live where that session lived, and a failed write is logged.
    let working_dir = {
        let members = swarm_members.read().await;
        members
            .get(session_id)
            .and_then(|member| member.working_dir.clone())
            .or_else(|| {
                // The dead session's own record may already be gone (it left the
                // swarm), and the rows live where the swarm's sessions live, so any
                // member of it names the same list.
                members
                    .values()
                    .find(|member| member.swarm_id.as_deref() == Some(swarm_id))
                    .and_then(|member| member.working_dir.clone())
            })
    };
    let mut released = Vec::with_capacity(outcome.released_task_ids.len());
    for task_id in outcome.released_task_ids.iter() {
        match crate::todo::release_row_on_disk(working_dir.as_deref(), session_id, task_id) {
            Ok(row) => released.push(row),
            Err(error) => crate::logging::warn(&format!(
                "swarm {swarm_id}: salvaged task '{task_id}' in the plan but not in the list: {error}"
            )),
        }
    }
    log_swarm_lifecycle(
        "dead_member_tasks_salvaged",
        vec![
            ("session_id", session_id.to_string()),
            ("swarm_id", swarm_id.to_string()),
            ("released_task_ids", outcome.released_task_ids.join(",")),
        ],
    );

    let swarm_state = SwarmState {
        members: Arc::clone(swarm_members),
        swarms_by_id: Arc::clone(swarms_by_id),
        runs: Arc::clone(swarm_runs),
        coordinators: Arc::clone(swarm_coordinators),
    };
    persist_swarm_state_for(swarm_id, &swarm_state).await;
    broadcast_swarm_plan(
        swarm_id,
        Some("task_salvaged_dead_worker".to_string()),
        swarm_runs,
        swarm_members,
        swarms_by_id,
    )
    .await;
    notify_coordinator_of_salvage(
        session_id,
        swarm_id,
        &outcome,
        swarm_members,
        swarm_coordinators,
    )
    .await;
    outcome
}

/// Deliver a salvage notification to the swarm's current coordinator (when it
/// is not the dead session itself).
async fn notify_coordinator_of_salvage(
    session_id: &str,
    swarm_id: &str,
    outcome: &DeadMemberSalvage,
    swarm_members: &Arc<RwLock<HashMap<String, SwarmMember>>>,
    swarm_coordinators: &Arc<RwLock<HashMap<String, String>>>,
) {
    let coordinator_id = {
        let coordinators = swarm_coordinators.read().await;
        coordinators.get(swarm_id).cloned()
    };
    let Some(coordinator_id) = coordinator_id.filter(|id| id != session_id) else {
        return;
    };
    let label = {
        let members = swarm_members.read().await;
        members
            .get(session_id)
            .and_then(|member| member.friendly_name.clone())
    }
    .unwrap_or_else(|| session_id[..8.min(session_id.len())].to_string());
    let _ = fanout_session_event(
        swarm_members,
        &coordinator_id,
        ServerEvent::Notification {
            from_session: session_id.to_string(),
            from_name: Some(label.clone()),
            notification_type: NotificationType::Message {
                scope: Some("swarm".to_string()),
                channel: None,
                tldr: None,
            },
            message: outcome.describe(&label),
        },
    )
    .await;
}

/// Requeue (or fail, at the reclaim cap) in-flight rows whose assignee is dead.
///
/// Nothing here marks a row stale: liveness is the member's own clock, so a row
/// whose holder has gone quiet is still that holder's. What this resolves is a
/// holder that can never come back, because no turn-end will ever arrive for it.
/// A terminal-status member gets a grace period before salvage: reload recovery
/// briefly marks resumable members `crashed` before restoring them, and
/// salvaging inside that window would double-assign their work.
pub(super) async fn salvage_dead_assignees(
    swarm_members: &Arc<RwLock<HashMap<String, SwarmMember>>>,
    swarms_by_id: &Arc<RwLock<HashMap<String, HashSet<String>>>>,
    swarm_runs: &Arc<RwLock<HashMap<String, RunState>>>,
    swarm_coordinators: &Arc<RwLock<HashMap<String, String>>>,
) {
    let salvage_grace = swarm_task_stale_after();
    let swarm_ids: Vec<String> = {
        let members = swarm_members.read().await;
        let mut ids: Vec<String> = members
            .values()
            .filter_map(|member| member.swarm_id.clone())
            .collect();
        ids.sort();
        ids.dedup();
        ids
    };
    let mut salvage_candidates: std::collections::BTreeSet<(String, String)> =
        std::collections::BTreeSet::new();
    for swarm_id in swarm_ids {
        let Some(requester) = ({
            let members = swarm_members.read().await;
            members
                .values()
                .find(|member| member.swarm_id.as_deref() == Some(swarm_id.as_str()))
                .map(|member| member.session_id.clone())
        }) else {
            continue;
        };
        let rows = swarm_rows(&swarm_id, &requester, swarm_members).await;
        let members = swarm_members.read().await;
        for row in rows {
            let Some(assignee) = row.assigned_to.as_deref() else {
                continue;
            };
            let assignee_is_dead = match members.get(assignee) {
                None => true,
                Some(member) => {
                    member.status.is_dead() && member.last_status_change.elapsed() >= salvage_grace
                }
            };
            if assignee_is_dead {
                salvage_candidates.insert((swarm_id.clone(), assignee.to_string()));
            }
        }
    }
    for (swarm_id, session_id) in salvage_candidates {
        salvage_assignments_of_dead_member(
            &session_id,
            &swarm_id,
            swarm_members,
            swarms_by_id,
            swarm_runs,
            swarm_coordinators,
        )
        .await;
    }
}

/// The last status detail of every member, keyed by session id.
///
/// The plan keeps no per-node history, so a node's failure reason is its
/// assignee's own detail: this is the projection the plan snapshots need.
pub(super) async fn member_details(
    swarm_members: &Arc<RwLock<HashMap<String, SwarmMember>>>,
) -> HashMap<String, String> {
    swarm_members
        .read()
        .await
        .iter()
        .filter_map(|(session_id, member)| {
            member
                .detail
                .clone()
                .map(|detail| (session_id.clone(), detail))
        })
        .collect()
}

/// Failure reason per failed plan item id, read from [`member_details`].
pub(super) fn failed_reasons_for(
    items: &[TaskItem],
    member_details: &HashMap<String, String>,
) -> std::collections::BTreeMap<String, String> {
    summarize_plan_graph(items)
        .failed_ids
        .into_iter()
        .filter_map(|id| {
            let assignee = items
                .iter()
                .find(|item| item.id == id)?
                .assigned_to
                .as_deref()?;
            Some((id, member_details.get(assignee)?.clone()))
        })
        .collect()
}

fn swarm_broadcast_key(
    swarm_id: &str,
    swarm_members: &Arc<RwLock<HashMap<String, SwarmMember>>>,
    swarms_by_id: &Arc<RwLock<HashMap<String, HashSet<String>>>>,
) -> String {
    format!(
        "{:p}:{:p}:{swarm_id}",
        Arc::as_ptr(swarm_members),
        Arc::as_ptr(swarms_by_id)
    )
}

async fn broadcast_swarm_status_now(
    session_ids: Vec<String>,
    swarm_members: &Arc<RwLock<HashMap<String, SwarmMember>>>,
) {
    if session_ids.is_empty() {
        return;
    }

    let members_guard = swarm_members.read().await;
    let broadcast_terminal_retention = swarm_status_broadcast_terminal_retention();
    let members_list: Vec<crate::protocol::SwarmMemberStatus> = session_ids
        .iter()
        .filter_map(|sid| {
            members_guard
                .get(sid)
                .filter(|m| member_in_status_broadcast(m, broadcast_terminal_retention))
                .map(|m| crate::protocol::SwarmMemberStatus {
                    session_id: m.session_id.clone(),
                    friendly_name: m.friendly_name.clone(),
                    status: m.status.clone(),
                    detail: m.detail.clone(),
                    task_label: m.task_label.clone(),
                    role: Some(m.role.clone()),
                    is_headless: Some(m.is_headless),
                    live_attachments: Some(m.event_txs.len()),
                    status_age_secs: Some(status_age_secs(m.last_status_change)),
                    output_tail: m.output_tail.clone(),
                    report_back_to_session_id: m.report_back_to_session_id.clone(),
                    todo_progress: m.todo_progress,
                    todo_items: m.todo_items.clone(),
                    runtime: crate::protocol::SwarmMemberRuntime {
                        model: m.runtime.model.clone(),
                        provider: m.runtime.provider.clone(),
                        auth_method: m.runtime.auth_method.clone(),
                        effort: m.runtime.effort.clone(),
                        elapsed_secs: if matches!(
                            m.status.as_str(),
                            "running" | "streaming" | "thinking"
                        ) {
                            Some(m.joined_at.elapsed().as_secs())
                        } else {
                            Some(m.runtime.elapsed_secs.unwrap_or(0))
                        },
                    },
                })
        })
        .collect();

    drop(members_guard);
    let event = ServerEvent::SwarmStatus {
        members: members_list,
    };
    for sid in session_ids {
        let _ = fanout_session_event(swarm_members, &sid, event.clone()).await;
    }
}

pub(super) async fn broadcast_swarm_status(
    swarm_id: &str,
    swarm_members: &Arc<RwLock<HashMap<String, SwarmMember>>>,
    swarms_by_id: &Arc<RwLock<HashMap<String, HashSet<String>>>>,
) {
    let session_ids: Vec<String> = {
        let swarms = swarms_by_id.read().await;
        swarms
            .get(swarm_id)
            .map(|s| s.iter().cloned().collect())
            .unwrap_or_default()
    };
    if session_ids.is_empty() {
        return;
    }

    if session_ids.len() < swarm_status_debounce_member_threshold() {
        broadcast_swarm_status_now(session_ids, swarm_members).await;
        return;
    }

    let key = swarm_broadcast_key(swarm_id, swarm_members, swarms_by_id);
    let should_spawn = {
        let mut pending = pending_swarm_status_broadcasts()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let entry = pending.entry(key.clone()).or_default();
        if entry.scheduled {
            entry.dirty = true;
            false
        } else {
            entry.scheduled = true;
            entry.dirty = false;
            true
        }
    };

    if !should_spawn {
        return;
    }

    let swarm_id = swarm_id.to_string();
    let swarm_members = Arc::clone(swarm_members);
    let swarms_by_id = Arc::clone(swarms_by_id);
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(swarm_status_debounce_ms())).await;
            let session_ids: Vec<String> = {
                let swarms = swarms_by_id.read().await;
                swarms
                    .get(&swarm_id)
                    .map(|s| s.iter().cloned().collect())
                    .unwrap_or_default()
            };
            broadcast_swarm_status_now(session_ids, &swarm_members).await;

            let mut pending = pending_swarm_status_broadcasts()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let Some(entry) = pending.get_mut(&key) else {
                break;
            };
            if entry.dirty {
                entry.dirty = false;
                continue;
            }
            pending.remove(&key);
            break;
        }
    });
}

/// Broadcast the authoritative swarm plan snapshot to the swarm's sessions.
///
/// The plan lock is held until every event is queued. A mutation cannot take the
/// write lock until this broadcast has sent, so two racing mutations cannot deliver
/// out of order; the sender is an unbounded queue, so holding the lock cannot block.
pub(super) async fn broadcast_swarm_plan(
    swarm_id: &str,
    reason: Option<String>,
    swarm_runs: &Arc<RwLock<HashMap<String, RunState>>>,
    swarm_members: &Arc<RwLock<HashMap<String, SwarmMember>>>,
    swarms_by_id: &Arc<RwLock<HashMap<String, HashSet<String>>>>,
) {
    broadcast_swarm_plan_with_previous(
        swarm_id,
        reason,
        None,
        swarm_runs,
        swarm_members,
        swarms_by_id,
    )
    .await;
}

pub(super) async fn broadcast_swarm_plan_with_previous(
    swarm_id: &str,
    reason: Option<String>,
    previous_items: Option<&[TaskItem]>,
    swarm_runs: &Arc<RwLock<HashMap<String, RunState>>>,
    swarm_members: &Arc<RwLock<HashMap<String, SwarmMember>>>,
    swarms_by_id: &Arc<RwLock<HashMap<String, HashSet<String>>>>,
) {
    // One per-swarm ordering domain, the same one the persist path takes: the read
    // and the send happen together, so two mutations cannot deliver out of order.
    let _ordering = crate::server::swarm_operation_lock(swarm_id)
        .lock_owned()
        .await;
    let assignee_details = member_details(swarm_members).await;
    let rows = swarm_rows(swarm_id, swarm_id, swarm_members).await;
    let run = swarm_runs
        .read()
        .await
        .get(swarm_id)
        .cloned()
        .unwrap_or_default();
    let items = rows_with_run_status(&rows, &run);
    let newly_ready_ids = previous_items
        .map(|before| newly_ready_item_ids(before, &items))
        .unwrap_or_default();
    let summary = crate::protocol::PlanGraphStatus::from_rows(
        swarm_id,
        &items,
        Some(3),
        newly_ready_ids,
        failed_reasons_for(&items, &assignee_details),
    );

    let mut participants: Vec<String> = {
        let swarms = swarms_by_id.read().await;
        swarms
            .get(swarm_id)
            .map(|s| s.iter().cloned().collect())
            .unwrap_or_default()
    };
    participants.sort();
    if participants.is_empty() {
        return;
    }

    let item_count = items.len();
    let reason_label = reason.clone().unwrap_or_else(|| "unspecified".to_string());
    let event = ServerEvent::SwarmPlan {
        swarm_id: swarm_id.to_string(),
        items,
        reason,
        summary: Some(summary),
    };

    let members = swarm_members.read().await;
    let participant_count = participants.len();
    let mut delivered_count = 0usize;
    for sid in participants {
        if let Some(member) = members.get(&sid)
            && member.event_tx.send(event.clone()).is_ok()
        {
            delivered_count += 1;
        }
    }
    log_swarm_lifecycle(
        "plan_broadcast",
        vec![
            ("swarm_id", swarm_id.to_string()),
            ("item_count", item_count.to_string()),
            ("participant_count", participant_count.to_string()),
            ("delivered_count", delivered_count.to_string()),
            ("reason", reason_label),
        ],
    );
}

/// Send the current swarm plan snapshot to ONE session (subscribe/resume
/// refresh). Unlike [`broadcast_swarm_plan`] this does not fan out to all
/// participants: reconnecting clients would otherwise show no plan graph
/// until the next plan mutation happens to broadcast.
pub(super) async fn send_swarm_plan_to_session(
    session_id: &str,
    swarm_members: &Arc<RwLock<HashMap<String, SwarmMember>>>,
    swarm_runs: &Arc<RwLock<HashMap<String, RunState>>>,
) {
    let swarm_id = {
        let members = swarm_members.read().await;
        members
            .get(session_id)
            .and_then(|member| member.swarm_id.clone())
    };
    let Some(swarm_id) = swarm_id else {
        return;
    };

    let assignee_details = member_details(swarm_members).await;
    let rows = swarm_rows(&swarm_id, session_id, swarm_members).await;
    let run = swarm_runs
        .read()
        .await
        .get(&swarm_id)
        .cloned()
        .unwrap_or_default();
    let items = rows_with_run_status(&rows, &run);
    if items.is_empty() {
        return;
    }
    let event = ServerEvent::SwarmPlan {
        swarm_id: swarm_id.clone(),
        items: items.clone(),
        reason: Some("reconnect".to_string()),
        summary: Some(crate::protocol::PlanGraphStatus::from_rows(
            &swarm_id,
            &items,
            Some(3),
            Vec::new(),
            failed_reasons_for(&items, &assignee_details),
        )),
    };

    let members = swarm_members.read().await;
    if let Some(member) = members.get(session_id) {
        let _ = member.event_tx.send(event);
    }
}

pub(super) async fn remove_session_from_swarm(
    session_id: &str,
    swarm_id: &str,
    swarm_members: &Arc<RwLock<HashMap<String, SwarmMember>>>,
    swarms_by_id: &Arc<RwLock<HashMap<String, HashSet<String>>>>,
    swarm_coordinators: &Arc<RwLock<HashMap<String, String>>>,
    swarm_runs: &Arc<RwLock<HashMap<String, RunState>>>,
) {
    let started = Instant::now();
    log_swarm_lifecycle(
        "member_remove_start",
        vec![
            ("session_id", session_id.to_string()),
            ("swarm_id", swarm_id.to_string()),
        ],
    );
    // Capture the departing member's own spawner before any teardown. Some
    // callers remove the member from the map before calling us, so this is
    // best-effort: when unavailable the orphan-reparenting below falls back to
    // the swarm coordinator.
    let departing_parent: Option<String> = {
        let members = swarm_members.read().await;
        members
            .get(session_id)
            .and_then(|member| member.report_back_to_session_id.clone())
    };
    // A leaving member can no longer drive its plan assignments (crash, stop,
    // disconnect, feature-off all funnel through here). Salvage before any
    // membership state is torn down so the coordinator notification can still
    // resolve names and fan out.
    salvage_assignments_of_dead_member(
        session_id,
        swarm_id,
        swarm_members,
        swarms_by_id,
        swarm_runs,
        swarm_coordinators,
    )
    .await;

    {
        let mut swarms = swarms_by_id.write().await;
        if let Some(swarm) = swarms.get_mut(swarm_id) {
            swarm.remove(session_id);
            if swarm.is_empty() {
                swarms.remove(swarm_id);
            }
        }
    }

    let was_coordinator = {
        let coordinators = swarm_coordinators.read().await;
        coordinators
            .get(swarm_id)
            .map(|id| id == session_id)
            .unwrap_or(false)
    };

    let mut elected_coordinator = None;
    if was_coordinator {
        let new_coordinator = {
            let swarms = swarms_by_id.read().await;
            let members = swarm_members.read().await;
            swarms.get(swarm_id).and_then(|swarm| {
                swarm
                    .iter()
                    .filter_map(|id| {
                        members
                            .get(id)
                            .filter(|member| !member.is_headless)
                            .map(|_| id.clone())
                    })
                    .min()
            })
        };

        {
            let mut coordinators = swarm_coordinators.write().await;
            coordinators.remove(swarm_id);
            if let Some(ref new_id) = new_coordinator {
                coordinators.insert(swarm_id.to_string(), new_id.clone());
            }
        }

        if let Some(new_id) = new_coordinator {
            elected_coordinator = Some(new_id.clone());
            {
                let mut members = swarm_members.write().await;
                if let Some(member) = members.get_mut(&new_id) {
                    member.role = "coordinator".to_string();
                }
            }
            let members = swarm_members.read().await;
            if let Some(member) = members.get(&new_id) {
                let _ = member.event_tx.send(ServerEvent::Notification {
                    from_session: new_id.clone(),
                    from_name: member.friendly_name.clone(),
                    notification_type: NotificationType::Message {
                        scope: Some("swarm".to_string()),
                        channel: None,
                        tldr: None,
                    },
                    message: "You are now the coordinator for this swarm.".to_string(),
                });
            }
        }
    }

    {
        let mut members = swarm_members.write().await;
        if let Some(member) = members.get_mut(session_id) {
            member.role = "agent".to_string();
        }
    }

    // Reparent the departing member's direct children so the spawn tree never
    // holds dangling report-back edges. Orphaned subtrees would otherwise
    // silently change ownership semantics: stop permissions, subtree broadcast
    // scope, and completion report-back all walk this chain. Children are
    // attached to their grandparent when it is still a live member of this
    // swarm, otherwise to the current coordinator, otherwise they become
    // roots (report_back_to_session_id = None).
    let fallback_parent: Option<String> = {
        let grandparent_is_live = if let Some(ref parent) = departing_parent {
            parent != session_id && {
                let members = swarm_members.read().await;
                members
                    .get(parent)
                    .is_some_and(|member| member.swarm_id.as_deref() == Some(swarm_id))
            }
        } else {
            false
        };
        if grandparent_is_live {
            departing_parent.clone()
        } else {
            let coordinators = swarm_coordinators.read().await;
            coordinators
                .get(swarm_id)
                .filter(|coordinator| coordinator.as_str() != session_id)
                .cloned()
        }
    };
    let mut reparented: Vec<String> = Vec::new();
    {
        let mut members = swarm_members.write().await;
        for member in members.values_mut() {
            if member.swarm_id.as_deref() == Some(swarm_id)
                && member.report_back_to_session_id.as_deref() == Some(session_id)
            {
                member.report_back_to_session_id = fallback_parent
                    .clone()
                    .filter(|parent| parent != &member.session_id);
                reparented.push(member.session_id.clone());
            }
        }
    }
    if !reparented.is_empty() {
        log_swarm_lifecycle(
            "member_remove_reparent",
            vec![
                ("session_id", session_id.to_string()),
                ("swarm_id", swarm_id.to_string()),
                (
                    "new_parent",
                    fallback_parent
                        .clone()
                        .unwrap_or_else(|| "none (promoted to root)".to_string()),
                ),
                ("reparented_children", reparented.join(",")),
            ],
        );
    }

    {
        let swarm_state = SwarmState {
            members: Arc::clone(swarm_members),
            swarms_by_id: Arc::clone(swarms_by_id),
            runs: Arc::clone(swarm_runs),
            coordinators: Arc::clone(swarm_coordinators),
        };
        persist_swarm_state_for(swarm_id, &swarm_state).await;
    }

    let remaining_member_count = swarms_by_id
        .read()
        .await
        .get(swarm_id)
        .map(|members| members.len())
        .unwrap_or_default();
    log_swarm_lifecycle(
        "member_remove_done",
        vec![
            ("session_id", session_id.to_string()),
            ("swarm_id", swarm_id.to_string()),
            ("was_coordinator", was_coordinator.to_string()),
            (
                "new_coordinator_session_id",
                elected_coordinator.unwrap_or_else(|| "none".to_string()),
            ),
            ("remaining_member_count", remaining_member_count.to_string()),
            ("elapsed_ms", started.elapsed().as_millis().to_string()),
        ],
    );
    broadcast_swarm_status(swarm_id, swarm_members, swarms_by_id).await;
}

/// Set a member's stable task label, derived from its spawn prompt or task
/// assignment. Unlike `detail` (transient status text), the label survives
/// status churn so UIs can always answer "what was this agent for?". A later
/// assignment overwrites the label: the member is now doing that task.
pub(super) async fn set_member_task_label(
    session_id: &str,
    task_text: &str,
    swarm_members: &Arc<RwLock<HashMap<String, SwarmMember>>>,
) {
    let Some(label) = kcode_swarm_core::derive_swarm_task_label(task_text) else {
        return;
    };
    let mut members = swarm_members.write().await;
    if let Some(member) = members.get_mut(session_id) {
        member.task_label = Some(label);
    }
}

pub(super) async fn record_swarm_event(
    event_history: &Arc<RwLock<std::collections::VecDeque<SwarmEvent>>>,
    event_counter: &Arc<std::sync::atomic::AtomicU64>,
    swarm_event_tx: &broadcast::Sender<SwarmEvent>,
    session_id: String,
    session_name: Option<String>,
    swarm_id: Option<String>,
    event: SwarmEventType,
) {
    let swarm_event = SwarmEvent {
        id: event_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst),
        session_id,
        session_name,
        swarm_id,
        event,
        timestamp: Instant::now(),
        absolute_time: std::time::SystemTime::now(),
    };
    let _ = swarm_event_tx.send(swarm_event.clone());
    let mut history = event_history.write().await;
    history.push_back(swarm_event);
    if history.len() > MAX_EVENT_HISTORY {
        history.pop_front();
    }
}

pub(super) async fn record_swarm_event_for_session(
    session_id: &str,
    event: SwarmEventType,
    swarm_members: &Arc<RwLock<HashMap<String, SwarmMember>>>,
    event_history: &Arc<RwLock<std::collections::VecDeque<SwarmEvent>>>,
    event_counter: &Arc<std::sync::atomic::AtomicU64>,
    swarm_event_tx: &broadcast::Sender<SwarmEvent>,
) {
    let (session_name, swarm_id) = {
        let members = swarm_members.read().await;
        if let Some(member) = members.get(session_id) {
            (member.friendly_name.clone(), member.swarm_id.clone())
        } else {
            (None, None)
        }
    };
    record_swarm_event(
        event_history,
        event_counter,
        swarm_event_tx,
        session_id.to_string(),
        session_name,
        swarm_id,
        event,
    )
    .await;
}

#[expect(
    clippy::too_many_arguments,
    reason = "member status updates need swarm membership, broadcast state, and optional event history sinks"
)]
pub(super) async fn update_member_status(
    session_id: &str,
    status: SwarmLifecycleStatus,
    detail: Option<String>,
    swarm_members: &Arc<RwLock<HashMap<String, SwarmMember>>>,
    swarms_by_id: &Arc<RwLock<HashMap<String, HashSet<String>>>>,
    event_history: Option<&Arc<RwLock<std::collections::VecDeque<SwarmEvent>>>>,
    event_counter: Option<&Arc<std::sync::atomic::AtomicU64>>,
    swarm_event_tx: Option<&broadcast::Sender<SwarmEvent>>,
) {
    update_member_status_with_report(
        session_id,
        status,
        detail,
        None,
        swarm_members,
        swarms_by_id,
        event_history,
        event_counter,
        swarm_event_tx,
    )
    .await;
}

#[expect(
    clippy::too_many_arguments,
    reason = "member status updates need swarm membership, broadcast state, optional report text, and event history sinks"
)]
pub(super) async fn update_member_status_with_report(
    session_id: &str,
    status: SwarmLifecycleStatus,
    detail: Option<String>,
    completion_report: Option<String>,
    swarm_members: &Arc<RwLock<HashMap<String, SwarmMember>>>,
    swarms_by_id: &Arc<RwLock<HashMap<String, HashSet<String>>>>,
    event_history: Option<&Arc<RwLock<std::collections::VecDeque<SwarmEvent>>>>,
    event_counter: Option<&Arc<std::sync::atomic::AtomicU64>>,
    swarm_event_tx: Option<&broadcast::Sender<SwarmEvent>>,
) {
    update_member_status_with_report_tldr(
        session_id,
        status,
        detail,
        completion_report,
        None,
        swarm_members,
        swarms_by_id,
        event_history,
        event_counter,
        swarm_event_tx,
    )
    .await
}

#[expect(
    clippy::too_many_arguments,
    reason = "member status updates need swarm membership, broadcast state, optional report text, and event history sinks"
)]
pub(super) async fn update_member_status_with_report_tldr(
    session_id: &str,
    status: SwarmLifecycleStatus,
    detail: Option<String>,
    completion_report: Option<String>,
    report_tldr: Option<String>,
    swarm_members: &Arc<RwLock<HashMap<String, SwarmMember>>>,
    swarms_by_id: &Arc<RwLock<HashMap<String, HashSet<String>>>>,
    event_history: Option<&Arc<RwLock<std::collections::VecDeque<SwarmEvent>>>>,
    event_counter: Option<&Arc<std::sync::atomic::AtomicU64>>,
    swarm_event_tx: Option<&broadcast::Sender<SwarmEvent>>,
) {
    let completion_report = normalize_completion_report(completion_report);
    let detail_present = detail.is_some();
    let (
        swarm_id,
        agent_name,
        member_changed,
        status_changed,
        old_status,
        _is_headless,
        report_back_to_session_id,
    ) = {
        let mut members = swarm_members.write().await;
        if let Some(member) = members.get_mut(session_id) {
            let previous_status = member.status.clone();
            let status_changed = member.status != status;
            let detail_changed = member.detail != detail;
            let report_changed =
                completion_report.is_some() && member.latest_completion_report != completion_report;
            let member_changed = status_changed || detail_changed || report_changed;
            if status_changed {
                member.last_status_change = Instant::now();
                if status.is_active() {
                    member.runtime.elapsed_secs = None;
                } else if previous_status.is_active() {
                    member.runtime.elapsed_secs = Some(member.joined_at.elapsed().as_secs());
                }
            }
            let name = member.friendly_name.clone();
            let is_headless = member.is_headless;
            let report_back_to_session_id = member.report_back_to_session_id.clone();
            member.status = status.clone();
            member.detail = detail;
            // Clear any live output tail when the worker reaches a terminal or
            // idle state so the inline gallery viewport doesn't keep showing
            // stale in-progress text after the turn finishes.
            if status.is_terminal() || matches!(status, SwarmLifecycleStatus::Ready) {
                member.output_tail = None;
            }
            if completion_report.is_some() {
                member.latest_completion_report = completion_report.clone();
            }
            (
                member.swarm_id.clone(),
                name,
                member_changed,
                status_changed,
                previous_status,
                is_headless,
                report_back_to_session_id,
            )
        } else {
            (
                None,
                None,
                false,
                false,
                SwarmLifecycleStatus::Ready,
                false,
                None,
            )
        }
    };
    if let Some(ref id) = swarm_id {
        if !member_changed {
            return;
        }

        log_swarm_lifecycle(
            "member_status_updated",
            vec![
                ("session_id", session_id.to_string()),
                ("swarm_id", id.clone()),
                ("old_status", old_status.to_string()),
                ("new_status", status.to_string()),
                ("status_changed", status_changed.to_string()),
                ("detail_present", detail_present.to_string()),
                (
                    "completion_report_present",
                    completion_report.is_some().to_string(),
                ),
                (
                    "report_back_to_session_id",
                    report_back_to_session_id
                        .clone()
                        .unwrap_or_else(|| "none".to_string()),
                ),
            ],
        );

        if status_changed
            && let (Some(history), Some(counter), Some(tx)) =
                (event_history, event_counter, swarm_event_tx)
        {
            record_swarm_event(
                history,
                counter,
                tx,
                session_id.to_string(),
                agent_name.clone(),
                Some(id.clone()),
                SwarmEventType::StatusChange {
                    old_status: old_status.clone(),
                    new_status: status.clone(),
                },
            )
            .await;
        }

        broadcast_swarm_status(id, swarm_members, swarms_by_id).await;

        let should_notify_coordinator = status_changed
            && ((status == SwarmLifecycleStatus::Completed)
                || (report_back_to_session_id.is_some()
                    && old_status == SwarmLifecycleStatus::Running
                    && matches!(
                        status,
                        SwarmLifecycleStatus::Ready
                            | SwarmLifecycleStatus::Failed
                            | SwarmLifecycleStatus::Stopped
                    ))
                // A crash is never routine: notify whoever is responsible
                // (owner, else coordinator) whenever a member dies while it
                // was doing or holding work, so worker deaths cannot pass
                // silently.
                || (status == SwarmLifecycleStatus::Crashed && old_status.is_in_flight()));
        if should_notify_coordinator {
            let fallback_coordinator_id =
                if report_back_to_session_id.as_deref() == Some(session_id) {
                    None
                } else {
                    let members = swarm_members.read().await;
                    members
                        .values()
                        .find(|m| {
                            m.swarm_id.as_deref() == Some(id)
                                && m.role == "coordinator"
                                && m.session_id != session_id
                        })
                        .map(|m| m.session_id.clone())
                };
            let recipient_session_id = report_back_to_session_id
                .clone()
                .filter(|owner_id| owner_id != session_id)
                .or(fallback_coordinator_id);
            if let Some(recipient_session_id) = recipient_session_id {
                let name = agent_name
                    .as_deref()
                    .unwrap_or(&session_id[..8.min(session_id.len())]);
                let msg = completion_notification_message(
                    name,
                    status.as_str(),
                    completion_report.as_deref(),
                );
                let _ = fanout_session_event(
                    swarm_members,
                    &recipient_session_id,
                    ServerEvent::Notification {
                        from_session: session_id.to_string(),
                        from_name: agent_name.clone(),
                        notification_type: NotificationType::Message {
                            scope: Some("swarm".to_string()),
                            channel: None,
                            tldr: report_tldr.clone(),
                        },
                        message: msg,
                    },
                )
                .await;
            }
        }
    }
}

pub(super) async fn run_swarm_task(
    agent: Arc<Mutex<Agent>>,
    description: &str,
    subagent_type: &str,
    prompt: &str,
) -> Result<String> {
    let started = Instant::now();
    let (provider, registry, session_id, working_dir, coordinator_model, provider_key, route) = {
        let agent = agent.lock().await;
        (
            agent.provider_fork(),
            agent.registry(),
            agent.session_id().to_string(),
            agent.working_dir().map(PathBuf::from),
            agent.provider_model(),
            agent.session_provider_key(),
            agent.session_route_api_method(),
        )
    };
    let parent_session_id = session_id.clone();
    let mut session = Session::create(
        Some(session_id),
        Some(format!("{} (@{} swarm)", description, subagent_type)),
    );
    let child_session_id = session.id.clone();
    session.model = Some(coordinator_model);
    // Inherit the coordinator's exact auth identity so the forked worker keeps
    // the same provider/auth route (OAuth vs API, openai-compatible profile)
    // instead of silently falling back to the config default on persistence.
    session.provider_key = provider_key;
    session.route_api_method = route;
    if let Some(dir) = working_dir {
        session.working_dir = Some(dir.display().to_string());
    }
    session.save()?;

    log_swarm_lifecycle(
        "task_start",
        vec![
            ("parent_session_id", parent_session_id.clone()),
            ("child_session_id", child_session_id.clone()),
            ("subagent_type", subagent_type.to_string()),
            ("description_chars", description.chars().count().to_string()),
            ("prompt_chars", prompt.chars().count().to_string()),
        ],
    );

    let mut allowed: HashSet<String> = registry.tool_names().await.into_iter().collect();
    for blocked in ["subagent", "task", "todo", "todowrite", "todoread"] {
        allowed.remove(blocked);
    }
    crate::config::config()
        .tools
        .apply_to_allowed_set(&mut allowed);

    let mut worker = Agent::new_with_session(provider, registry, session, Some(allowed));
    match worker.run_once_capture(prompt).await {
        Ok(output) => {
            log_swarm_lifecycle(
                "task_done",
                vec![
                    ("parent_session_id", parent_session_id),
                    ("child_session_id", child_session_id),
                    ("subagent_type", subagent_type.to_string()),
                    ("output_chars", output.chars().count().to_string()),
                    ("elapsed_ms", started.elapsed().as_millis().to_string()),
                ],
            );
            Ok(output)
        }
        Err(error) => {
            crate::logging::event_warn(
                "SWARM_LIFECYCLE",
                vec![
                    ("phase", "task_error".to_string()),
                    ("parent_session_id", parent_session_id),
                    ("child_session_id", child_session_id),
                    ("subagent_type", subagent_type.to_string()),
                    ("error", error.to_string()),
                    ("elapsed_ms", started.elapsed().as_millis().to_string()),
                ],
            );
            Err(error)
        }
    }
}

pub(super) async fn run_swarm_message(agent: Arc<Mutex<Agent>>, message: &str) -> Result<String> {
    let started = Instant::now();
    log_swarm_lifecycle(
        "message_start",
        vec![("message_chars", message.chars().count().to_string())],
    );
    let working_dir = {
        let agent = agent.lock().await;
        agent.working_dir().map(|dir| dir.to_string())
    };
    let working_dir_hint = working_dir
        .as_deref()
        .map(|dir| format!("Working directory: {}\n", dir))
        .unwrap_or_default();

    let planner_prompt = format!(
        "{working_dir_hint}You are a task planner. Break the request into 2-4 subtasks. \
Return ONLY a JSON array of objects with keys: description, prompt, subagent_type. \
No extra text.\n\nRequest:\n{message}"
    );

    let plan_text = {
        let mut agent = agent.lock().await;
        agent.run_once_capture(&planner_prompt).await?
    };

    let mut tasks = parse_swarm_tasks(&plan_text);
    if tasks.is_empty() {
        tasks.push(SwarmTaskSpec {
            description: "Main task".to_string(),
            prompt: message.to_string(),
            subagent_type: Some("general".to_string()),
        });
    }
    log_swarm_lifecycle(
        "message_plan_done",
        vec![
            ("task_count", tasks.len().to_string()),
            ("plan_chars", plan_text.chars().count().to_string()),
        ],
    );

    let task_futures = tasks.iter().map(|task| {
        let agent = agent.clone();
        let working_dir_hint = working_dir_hint.clone();
        let description = task.description.clone();
        let prompt = format!("{working_dir_hint}{}", task.prompt);
        let subagent_type = task
            .subagent_type
            .clone()
            .unwrap_or_else(|| "general".to_string());
        async move {
            let output = run_swarm_task(agent, &description, &subagent_type, &prompt).await?;
            Ok::<(String, String), anyhow::Error>((description, output))
        }
    });
    let task_outputs = try_join_all(task_futures).await?;

    let mut integration_prompt = String::new();
    integration_prompt.push_str(
        "You are the coordinator. Complete the original request using the subagent outputs below. ",
    );
    integration_prompt.push_str("Do not stop early; run any requested tests and fix failures.\n\n");
    integration_prompt.push_str("Original request:\n");
    integration_prompt.push_str(message);
    integration_prompt.push_str("\n\nSubagent outputs:\n");
    for (desc, output) in &task_outputs {
        integration_prompt.push_str(&format!("\n--- {} ---\n{}\n", desc, output));
    }
    integration_prompt.push_str("\nNow complete the task.\n");

    let final_output = {
        let mut agent = agent.lock().await;
        agent.run_once_capture(&integration_prompt).await?
    };

    log_swarm_lifecycle(
        "message_done",
        vec![
            ("task_count", task_outputs.len().to_string()),
            ("output_chars", final_output.chars().count().to_string()),
            ("elapsed_ms", started.elapsed().as_millis().to_string()),
        ],
    );

    Ok(final_output)
}

#[derive(Debug, Deserialize)]
struct SwarmTaskSpec {
    description: String,
    prompt: String,
    #[serde(default)]
    subagent_type: Option<String>,
}

fn parse_swarm_tasks(text: &str) -> Vec<SwarmTaskSpec> {
    if let Ok(tasks) = serde_json::from_str::<Vec<SwarmTaskSpec>>(text) {
        return tasks;
    }

    if let (Some(start), Some(end)) = (text.find('['), text.rfind(']'))
        && start < end
        && let Ok(tasks) = serde_json::from_str::<Vec<SwarmTaskSpec>>(&text[start..=end])
    {
        return tasks;
    }

    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::{
        broadcast_swarm_plan, broadcast_swarm_plan_with_previous, broadcast_swarm_status,
        member_in_status_broadcast, parse_swarm_tasks, remove_session_from_swarm,
        salvage_assignments_of_dead_member, salvage_dead_assignees, swarm_ancestors,
        swarm_is_self_or_ancestor, swarm_spawn_depth, update_member_status,
        update_member_status_with_report,
    };
    use crate::plan::TaskItem;
    use crate::protocol::SwarmLifecycleStatus;
    use crate::protocol::{NotificationType, ServerEvent};
    use crate::server::{RunState, SwarmMember};
    use kcode_swarm_core::{
        append_swarm_completion_report_instructions, summarize_plan_items, truncate_detail,
    };
    use std::collections::{HashMap, HashSet};
    use std::sync::Arc;
    use std::time::{Duration, Instant};
    use tokio::sync::{RwLock, mpsc};

    fn plan_item(id: &str, content: &str) -> TaskItem {
        TaskItem {
            content: content.to_string(),
            status: "pending".to_string(),
            priority: "medium".to_string(),
            id: id.to_string(),
            ..Default::default()
        }
    }

    /// A row as the store would write it: words, a status, and a holder.
    fn row(id: &str, status: &str, holder: Option<&str>) -> TaskItem {
        TaskItem {
            content: format!("task {id}"),
            status: status.to_string(),
            priority: "medium".to_string(),
            id: id.to_string(),
            assigned_to: holder.map(str::to_string),
            ..Default::default()
        }
    }

    /// Write the fixture's list back, exactly as given.
    fn write_list(repo: &std::path::Path, rows: &[TaskItem]) {
        crate::todo::save_tasks(Some(repo), "worker", rows).expect("write the list");
    }

    #[test]
    fn truncate_detail_collapses_whitespace_and_ellipsizes() {
        assert_eq!(truncate_detail("hello   there\nworld", 11), "hello th...");
    }

    #[test]
    fn summarize_plan_items_limits_output() {
        let items = vec![
            plan_item("1", "inspect"),
            plan_item("2", "refactor"),
            plan_item("3", "test"),
        ];

        assert_eq!(
            summarize_plan_items(&items, 2),
            "inspect; refactor (+1 more)"
        );
    }

    #[test]
    fn parse_swarm_tasks_accepts_wrapped_json() {
        let text =
            "Plan:\n[{\"description\":\"A\",\"prompt\":\"B\",\"subagent_type\":\"general\"}]";
        let tasks = parse_swarm_tasks(text);

        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].description, "A");
        assert_eq!(tasks[0].prompt, "B");
        assert_eq!(tasks[0].subagent_type.as_deref(), Some("general"));
    }

    #[test]
    fn append_swarm_completion_report_instructions_is_idempotent() {
        let prompt = "Implement the task.";
        let with_instructions = append_swarm_completion_report_instructions(prompt);

        assert!(with_instructions.starts_with(prompt));
        assert!(with_instructions.contains("SWARM COMPLETION REPORT REQUIRED"));
        assert!(with_instructions.contains("swarm tool with action=\"report\""));
        assert_eq!(
            append_swarm_completion_report_instructions(&with_instructions),
            with_instructions
        );
    }

    fn swarm_member(
        session_id: &str,
        role: &str,
        is_headless: bool,
    ) -> (SwarmMember, mpsc::UnboundedReceiver<ServerEvent>) {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        (
            SwarmMember {
                session_id: session_id.to_string(),
                event_tx,
                event_txs: HashMap::new(),
                working_dir: None,
                swarm_id: Some("swarm-1".to_string()),
                swarm_enabled: true,
                status: SwarmLifecycleStatus::Ready,
                detail: None,
                task_label: None,
                friendly_name: Some(session_id.to_string()),
                report_back_to_session_id: None,
                latest_completion_report: None,
                role: role.to_string(),
                joined_at: Instant::now(),
                last_status_change: Instant::now(),
                is_headless,
                output_tail: None,
                todo_progress: None,
                todo_items: Vec::new(),
                runtime: crate::protocol::SwarmMemberRuntime::default(),
            },
            event_rx,
        )
    }

    /// A repo whose list holds `rows`, so a store write in these tests has somewhere
    /// to go (rule 1: the rows live where the session lives). The caller keeps the
    /// returned dir alive.
    fn list_repo(rows: &[TaskItem]) -> tempfile::TempDir {
        let repo = tempfile::TempDir::new().expect("tempdir");
        assert!(
            std::process::Command::new("git")
                .args(["init", "-q"])
                .current_dir(repo.path())
                .status()
                .expect("git init")
                .success(),
            "git init"
        );
        crate::todo::save_tasks(Some(repo.path()), "worker", rows).expect("write the list");
        repo
    }

    /// A row the store would write for a plan item of the same id, held by `holder`.
    fn held_row(id: &str, holder: &str) -> TaskItem {
        TaskItem {
            id: id.to_string(),
            assigned_to: Some(holder.to_string()),
            status: "pending".to_string(),
            ..Default::default()
        }
    }

    /// A member that works in `repo`.
    fn swarm_member_in(
        repo: &tempfile::TempDir,
        session_id: &str,
        role: &str,
        is_headless: bool,
    ) -> (SwarmMember, mpsc::UnboundedReceiver<ServerEvent>) {
        let (mut member, rx) = swarm_member(session_id, role, is_headless);
        member.working_dir = Some(repo.path().to_path_buf());
        (member, rx)
    }

    fn member_with_parent(session_id: &str, parent: Option<&str>) -> SwarmMember {
        let (mut member, _rx) = swarm_member(session_id, "agent", false);
        member.report_back_to_session_id = parent.map(str::to_string);
        member
    }

    #[test]
    fn idle_spawned_worker_reap_selects_only_finished_idle_spawned_agents() {
        use super::idle_spawned_worker_reap_candidates;

        let idle_after = Duration::from_secs(60);
        let old = Instant::now() - Duration::from_secs(120);

        // Finished spawned worker, idle past the window: reapable.
        let mut reapable = member_with_parent("reapable", Some("coord"));
        reapable.status = SwarmLifecycleStatus::Ready;
        reapable.last_status_change = old;

        // Terminal-status spawned worker: reapable.
        let mut stopped = member_with_parent("stopped", Some("coord"));
        stopped.status = SwarmLifecycleStatus::Completed;
        stopped.last_status_change = old;

        // Same shape but user-created (no spawner): never reaped.
        let mut user_owned = member_with_parent("user-owned", None);
        user_owned.status = SwarmLifecycleStatus::Ready;
        user_owned.last_status_change = old;

        // Spawned but still running: not reaped.
        let mut running = member_with_parent("running", Some("coord"));
        running.status = SwarmLifecycleStatus::Running;
        running.last_status_change = old;

        // Spawned and finished, but recently: not reaped yet.
        let mut fresh = member_with_parent("fresh", Some("coord"));
        fresh.status = SwarmLifecycleStatus::Ready;

        // Spawned coordinator (sub-swarm manager): never reaped by role.
        let mut sub_coordinator = member_with_parent("sub-coord", Some("coord"));
        sub_coordinator.role = "coordinator".to_string();
        sub_coordinator.status = SwarmLifecycleStatus::Ready;
        sub_coordinator.last_status_change = old;

        let members: HashMap<String, SwarmMember> = [
            reapable,
            stopped,
            user_owned,
            running,
            fresh,
            sub_coordinator,
        ]
        .into_iter()
        .map(|member| (member.session_id.clone(), member))
        .collect();

        let mut candidates = idle_spawned_worker_reap_candidates(&members, idle_after);
        candidates.sort();
        assert_eq!(
            candidates,
            vec!["reapable".to_string(), "stopped".to_string()]
        );
    }

    #[test]
    fn idle_worker_reap_window_env_zero_disables() {
        // Note: mutating the process env in tests is racy in general, but this
        // env var is read on every call (not cached), and no other test touches
        // it.
        unsafe {
            std::env::set_var("KCODE_SWARM_IDLE_WORKER_REAP_SECS", "0");
        }
        assert_eq!(super::swarm_idle_worker_reap_after(), None);
        unsafe {
            std::env::set_var("KCODE_SWARM_IDLE_WORKER_REAP_SECS", "90");
        }
        assert_eq!(
            super::swarm_idle_worker_reap_after(),
            Some(Duration::from_secs(90))
        );
        unsafe {
            std::env::remove_var("KCODE_SWARM_IDLE_WORKER_REAP_SECS");
        }
        assert!(super::swarm_idle_worker_reap_after().is_some());
    }

    #[test]
    fn status_broadcast_keeps_live_and_recently_terminal_members_only() {
        let retention = Duration::from_secs(900);

        let (live, _rx) = swarm_member("live", "agent", false);
        assert!(member_in_status_broadcast(&live, retention));

        let (mut fresh_terminal, _rx) = swarm_member("fresh", "agent", false);
        fresh_terminal.status = SwarmLifecycleStatus::Completed;
        assert!(member_in_status_broadcast(&fresh_terminal, retention));

        let (mut stale_terminal, _rx) = swarm_member("stale", "agent", false);
        stale_terminal.status = SwarmLifecycleStatus::Stopped;
        stale_terminal.last_status_change = Instant::now() - Duration::from_secs(901);
        assert!(!member_in_status_broadcast(&stale_terminal, retention));

        // A stale *live* status is never filtered, no matter how old.
        let (mut old_live, _rx) = swarm_member("old-live", "agent", false);
        old_live.last_status_change = Instant::now() - Duration::from_secs(100_000);
        assert!(member_in_status_broadcast(&old_live, retention));
    }

    #[test]
    fn swarm_depth_and_ancestry_follow_report_back_chain() {
        let mut members: HashMap<String, SwarmMember> = HashMap::new();
        for (id, parent) in [
            ("root", None),
            ("a", Some("root")),
            ("b", Some("a")),
            ("c", Some("b")),
        ] {
            members.insert(id.to_string(), member_with_parent(id, parent));
        }

        assert_eq!(swarm_spawn_depth(&members, "root"), 0);
        assert_eq!(swarm_spawn_depth(&members, "a"), 1);
        assert_eq!(swarm_spawn_depth(&members, "c"), 3);
        assert_eq!(swarm_ancestors(&members, "c"), vec!["b", "a", "root"]);

        // Ownership: an ancestor (or self) owns the subtree.
        assert!(swarm_is_self_or_ancestor(&members, "a", "c"));
        assert!(swarm_is_self_or_ancestor(&members, "root", "c"));
        assert!(swarm_is_self_or_ancestor(&members, "c", "c"));
        // A sibling/descendant is not an ancestor.
        assert!(!swarm_is_self_or_ancestor(&members, "c", "a"));
        assert!(!swarm_is_self_or_ancestor(&members, "b", "a"));
    }

    #[test]
    fn swarm_ancestry_guards_against_cycles() {
        let mut members: HashMap<String, SwarmMember> = HashMap::new();
        // x -> y -> x is a (pathological) cycle; depth must terminate.
        members.insert("x".to_string(), member_with_parent("x", Some("y")));
        members.insert("y".to_string(), member_with_parent("y", Some("x")));
        assert_eq!(swarm_spawn_depth(&members, "x"), 1);
        assert_eq!(swarm_ancestors(&members, "x"), vec!["y"]);
    }

    #[tokio::test]
    async fn broadcast_swarm_plan_with_previous_includes_newly_ready_ids() {
        let repo = list_repo(&[
            row("setup", "running", Some("worker")),
            row("follow-up", "queued", Some("worker")),
        ]);
        let swarm_runs = Arc::new(RwLock::new(HashMap::<String, RunState>::new()));
        let (worker, mut worker_rx) = swarm_member_in(&repo, "worker", "agent", false);
        let swarm_members = Arc::new(RwLock::new(HashMap::from([("worker".to_string(), worker)])));
        let swarms_by_id = Arc::new(RwLock::new(HashMap::from([(
            "swarm-1".to_string(),
            HashSet::from(["worker".to_string()]),
        )])));
        let previous_items = vec![
            TaskItem {
                content: "setup".to_string(),
                status: "running".to_string(),
                priority: "high".to_string(),
                id: "setup".to_string(),
                assigned_to: Some("worker".to_string()),
                ..Default::default()
            },
            TaskItem {
                content: "follow-up".to_string(),
                status: "queued".to_string(),
                priority: "high".to_string(),
                id: "follow-up".to_string(),
                blocked_by: vec!["setup".to_string()],
                ..Default::default()
            },
        ];

        broadcast_swarm_plan_with_previous(
            "swarm-1",
            Some("task_completed".to_string()),
            Some(&previous_items),
            &swarm_runs,
            &swarm_members,
            &swarms_by_id,
        )
        .await;

        match worker_rx.recv().await.expect("swarm plan event") {
            ServerEvent::SwarmPlan {
                reason,
                summary: Some(summary),
                ..
            } => {
                assert_eq!(reason.as_deref(), Some("task_completed"));
                assert_eq!(summary.newly_ready_ids, vec!["follow-up".to_string()]);
                assert_eq!(summary.next_ready_ids, vec!["follow-up".to_string()]);
            }
            other => panic!("expected SwarmPlan event, got {other:?}"),
        }
    }

    /// A mutation cannot reach a member before a broadcast that started earlier.
    ///
    /// `broadcast_swarm_plan_with_previous` holds the plan read lock until every
    /// event is queued, so a second mutator cannot take the write lock, and cannot
    /// send, until the first broadcast has sent. This test parks broadcast A behind a
    /// held `swarms_by_id.write()` guard and lets mutator B queue behind A's plan
    /// lock; releasing the guard delivers one item then two, in order.
    #[tokio::test]
    async fn swarm_plan_broadcasts_cannot_invert_on_one_member_channel() {
        let repo = std::sync::Arc::new(list_repo(&[plan_item("t1", "task one")]));
        let swarm_runs = Arc::new(RwLock::new(HashMap::<String, RunState>::new()));
        let (worker, mut worker_rx) = swarm_member_in(&repo, "worker", "agent", false);
        let swarm_members = Arc::new(RwLock::new(HashMap::from([("worker".to_string(), worker)])));
        let swarms_by_id = Arc::new(RwLock::new(HashMap::from([(
            "swarm-1".to_string(),
            HashSet::from(["worker".to_string()]),
        )])));

        // Hold the membership lock so broadcast A parks on it, already holding the
        // plan lock it took first.
        let gate = swarms_by_id.write().await;

        let a = tokio::spawn({
            let swarm_runs = Arc::clone(&swarm_runs);
            let swarm_members = Arc::clone(&swarm_members);
            let swarms_by_id = Arc::clone(&swarms_by_id);
            async move {
                broadcast_swarm_plan(
                    "swarm-1",
                    Some("mutator_1".to_string()),
                    &swarm_runs,
                    &swarm_members,
                    &swarms_by_id,
                )
                .await;
            }
        });
        for _ in 0..16 {
            tokio::task::yield_now().await;
        }

        // Mutator B writes the plan and broadcasts it. It queues behind A's plan
        // lock rather than interleaving with it.
        let b = tokio::spawn({
            let swarm_runs = Arc::clone(&swarm_runs);
            let swarm_members = Arc::clone(&swarm_members);
            let swarms_by_id = Arc::clone(&swarms_by_id);
            let repo = std::sync::Arc::clone(&repo);
            async move {
                // The mutation is a write to the list, which is where rows live.
                write_list(
                    repo.path(),
                    &[
                        row("t1", "pending", Some("worker")),
                        row("t2", "pending", Some("worker")),
                    ],
                );
                broadcast_swarm_plan(
                    "swarm-1",
                    Some("mutator_2".to_string()),
                    &swarm_runs,
                    &swarm_members,
                    &swarms_by_id,
                )
                .await;
            }
        });
        for _ in 0..16 {
            tokio::task::yield_now().await;
        }

        drop(gate);
        a.await.expect("broadcast task");
        b.await.expect("mutator task");

        let mut item_counts = Vec::new();
        while let Ok(event) = worker_rx.try_recv() {
            if let ServerEvent::SwarmPlan { items, .. } = event {
                item_counts.push(items.len());
            }
        }
        assert_eq!(
            item_counts,
            vec![1, 2],
            "the broadcast that started first must reach the member first"
        );
    }

    /// Deterministic demonstration of the SwarmStatus immediate-path
    /// snapshot-vs-send inversion (wiring-audit.status-proposal-ordering).
    ///
    /// `broadcast_swarm_status_now` snapshots member statuses under
    /// `swarm_members.read()`, drops the guard, then awaits
    /// `fanout_session_event` (a `swarm_members.write()` acquisition) before
    /// sending. Swarms below `KCODE_SWARM_STATUS_DEBOUNCE_MEMBER_THRESHOLD`
    /// (default 2) take this immediate, non-debounced path on every status
    /// change, so two concurrent broadcasts can deliver an old snapshot after
    /// a newer one on the same ordered mpsc channel. A last-write-wins
    /// consumer (the TUI SwarmStatus handler) is then left showing the stale
    /// status until the next unrelated broadcast.
    ///
    /// Unlike the SwarmPlan path, whose broadcast now holds the plan lock through
    /// the send, there is no second lock we can gate on here: the status path snapshots from the same `swarm_members`
    /// lock it later writes, so holding any guard also blocks the mutator.
    /// Instead this test uses tokio's cooperative budget (128 units per task
    /// poll on a current-thread runtime; every RwLock acquisition consumes
    /// exactly one). Draining 126 units leaves broadcast A exactly enough for
    /// `swarms_by_id.read()` and the `swarm_members.read()` snapshot, forcing
    /// a yield at the (uncontended) `swarm_members.write()` inside
    /// `fanout_session_event`, i.e. precisely inside the race window between
    /// snapshot and send.
    ///
    /// If this test starts failing with `["running", "running"]` or
    /// `["ready", "running"]`, the race has been fixed (e.g. by holding the
    /// read lock through the send, or by stamping a monotonic sequence on
    /// SwarmStatus and dropping stale ones consumer-side); update the wiring
    /// audit. If it fails because broadcast A parks somewhere else, the tokio
    /// coop budget constants changed: re-derive the `128 - 2` drain count.
    #[tokio::test]
    async fn swarm_status_immediate_broadcasts_can_invert_on_one_member_channel() {
        let (worker, mut worker_rx) = swarm_member("worker", "agent", false);
        let swarm_members = Arc::new(RwLock::new(HashMap::from([("worker".to_string(), worker)])));
        let swarms_by_id = Arc::new(RwLock::new(HashMap::from([(
            "swarm-1".to_string(),
            HashSet::from(["worker".to_string()]),
        )])));

        // Broadcast A: snapshots status "ready", then is forced to yield at
        // the fanout write acquisition, before sending.
        let a = tokio::spawn({
            let swarm_members = Arc::clone(&swarm_members);
            let swarms_by_id = Arc::clone(&swarms_by_id);
            async move {
                // Initial task budget is 128. Leave exactly 2 units so the two
                // read acquisitions (session-id list + status snapshot)
                // succeed and the fanout write acquisition forces a yield.
                for _ in 0..126 {
                    tokio::task::coop::consume_budget().await;
                }
                broadcast_swarm_status("swarm-1", &swarm_members, &swarms_by_id).await;
            }
        });
        // Single yield on the current-thread runtime: A runs its entire first
        // poll (budget drain + both reads) and parks after snapshotting
        // "ready". Its coop yield happens *before* joining the lock queue, so
        // every acquisition below is uncontended and the mutator finishes
        // within one poll, before A is re-polled.
        tokio::task::yield_now().await;

        // Concurrent mutator: flips the status and completes its own
        // immediate broadcast while A is parked between snapshot and send.
        {
            let mut members = swarm_members.write().await;
            members.get_mut("worker").expect("worker member").status =
                SwarmLifecycleStatus::Running;
        }
        broadcast_swarm_status("swarm-1", &swarm_members, &swarms_by_id).await;

        // Release A: it resumes with a fresh budget and sends its stale
        // "ready" snapshot after "running" on the same ordered channel.
        a.await.expect("broadcast task");

        let mut statuses = Vec::new();
        while let Ok(event) = worker_rx.try_recv() {
            if let ServerEvent::SwarmStatus { members } = event {
                assert_eq!(members.len(), 1);
                assert_eq!(members[0].session_id, "worker");
                statuses.push(members[0].status.clone());
            }
        }
        assert_eq!(
            statuses,
            vec![SwarmLifecycleStatus::Running, SwarmLifecycleStatus::Ready],
            "expected status inversion (new-then-old) on one member channel; \
             if this fails with the correct order, the snapshot-vs-send race \
             may have been fixed (update the wiring audit)"
        );
    }

    /// A restored member with a dead channel does not starve the live ones: the
    /// recipients are the swarm's sessions, not a hand-kept participant list, so a
    /// session whose channel has closed simply fails to receive.
    #[tokio::test]
    async fn a_closed_member_channel_does_not_starve_live_members_of_plan_broadcasts() {
        let repo = list_repo(&[row("t1", "pending", Some("live"))]);
        let swarm_runs = Arc::new(RwLock::new(HashMap::<String, RunState>::new()));
        // Ghost member as produced by swarm_persistence restore: present in
        // the member map but with a closed event channel.
        let (ghost, ghost_rx) = swarm_member_in(&repo, "ghost", "agent", true);
        drop(ghost_rx);
        let (live, mut live_rx) = swarm_member_in(&repo, "live", "agent", false);
        let swarm_members = Arc::new(RwLock::new(HashMap::from([
            ("ghost".to_string(), ghost),
            ("live".to_string(), live),
        ])));
        let swarms_by_id = Arc::new(RwLock::new(HashMap::from([(
            "swarm-1".to_string(),
            HashSet::from(["ghost".to_string(), "live".to_string()]),
        )])));

        broadcast_swarm_plan(
            "swarm-1",
            Some("test".to_string()),
            &swarm_runs,
            &swarm_members,
            &swarms_by_id,
        )
        .await;

        assert!(
            live_rx.try_recv().is_ok(),
            "every live member of the swarm receives the plan broadcast"
        );
    }

    #[tokio::test]
    async fn remove_session_from_swarm_reassigns_to_non_headless_member() {
        let swarm_members = Arc::new(RwLock::new(HashMap::new()));
        let swarms_by_id = Arc::new(RwLock::new(HashMap::from([(
            "swarm-1".to_string(),
            HashSet::from([
                "coord".to_string(),
                "headless".to_string(),
                "worker".to_string(),
            ]),
        )])));
        let swarm_coordinators = Arc::new(RwLock::new(HashMap::from([(
            "swarm-1".to_string(),
            "coord".to_string(),
        )])));
        let _repo = list_repo(&[row("1", "pending", Some("coord"))]);
        let swarm_runs = Arc::new(RwLock::new(HashMap::<String, RunState>::new()));

        let (coord, _coord_rx) = swarm_member("coord", "coordinator", false);
        let (headless, mut headless_rx) = swarm_member("headless", "agent", true);
        let (worker, mut worker_rx) = swarm_member("worker", "agent", false);
        {
            let mut members = swarm_members.write().await;
            members.insert("coord".to_string(), coord);
            members.insert("headless".to_string(), headless);
            members.insert("worker".to_string(), worker);
            members.remove("coord");
        }

        remove_session_from_swarm(
            "coord",
            "swarm-1",
            &swarm_members,
            &swarms_by_id,
            &swarm_coordinators,
            &swarm_runs,
        )
        .await;

        assert_eq!(
            swarm_coordinators
                .read()
                .await
                .get("swarm-1")
                .map(String::as_str),
            Some("worker")
        );
        assert_eq!(
            swarm_members
                .read()
                .await
                .get("worker")
                .map(|member| member.role.as_str()),
            Some("coordinator")
        );
        assert_eq!(
            swarm_members
                .read()
                .await
                .get("headless")
                .map(|member| member.role.as_str()),
            Some("agent")
        );

        let headless_events: Vec<_> = std::iter::from_fn(|| headless_rx.try_recv().ok()).collect();
        assert!(headless_events.iter().all(|event| {
            !matches!(
                event,
                ServerEvent::Notification {
                    notification_type: NotificationType::Message { .. },
                    message,
                    ..
                } if message == "You are now the coordinator for this swarm."
            )
        }));

        let worker_events: Vec<_> = std::iter::from_fn(|| worker_rx.try_recv().ok()).collect();
        assert!(worker_events.iter().any(|event| {
            matches!(
                event,
                ServerEvent::Notification {
                    notification_type: NotificationType::Message { .. },
                    message,
                    ..
                } if message == "You are now the coordinator for this swarm."
            )
        }));
    }

    #[tokio::test]
    async fn remove_session_reparents_children_to_live_grandparent() {
        let swarm_members = Arc::new(RwLock::new(HashMap::new()));
        let swarms_by_id = Arc::new(RwLock::new(HashMap::from([(
            "swarm-1".to_string(),
            HashSet::from(["root".to_string(), "mid".to_string(), "leaf".to_string()]),
        )])));
        let swarm_coordinators = Arc::new(RwLock::new(HashMap::from([(
            "swarm-1".to_string(),
            "root".to_string(),
        )])));
        let swarm_runs = Arc::new(RwLock::new(HashMap::new()));

        let (root, _root_rx) = swarm_member("root", "coordinator", false);
        let (mut mid, _mid_rx) = swarm_member("mid", "agent", true);
        mid.report_back_to_session_id = Some("root".to_string());
        let (mut leaf, _leaf_rx) = swarm_member("leaf", "agent", true);
        leaf.report_back_to_session_id = Some("mid".to_string());
        {
            let mut members = swarm_members.write().await;
            members.insert("root".to_string(), root);
            members.insert("mid".to_string(), mid);
            members.insert("leaf".to_string(), leaf);
        }

        remove_session_from_swarm(
            "mid",
            "swarm-1",
            &swarm_members,
            &swarms_by_id,
            &swarm_coordinators,
            &swarm_runs,
        )
        .await;

        // Leaf follows the report-back chain up to its grandparent instead of
        // dangling on the removed session.
        let members = swarm_members.read().await;
        assert_eq!(
            members
                .get("leaf")
                .and_then(|member| member.report_back_to_session_id.as_deref()),
            Some("root")
        );
        assert!(swarm_is_self_or_ancestor(&members, "root", "leaf"));
    }

    #[tokio::test]
    async fn remove_session_reparents_children_to_coordinator_when_no_grandparent() {
        let swarm_members = Arc::new(RwLock::new(HashMap::new()));
        let swarms_by_id = Arc::new(RwLock::new(HashMap::from([(
            "swarm-1".to_string(),
            HashSet::from([
                "coord".to_string(),
                "peer_root".to_string(),
                "child".to_string(),
            ]),
        )])));
        let swarm_coordinators = Arc::new(RwLock::new(HashMap::from([(
            "swarm-1".to_string(),
            "coord".to_string(),
        )])));
        let swarm_runs = Arc::new(RwLock::new(HashMap::new()));

        // peer_root is itself a root (no parent), so its children have no
        // grandparent to inherit; they should fall back to the coordinator.
        let (coord, _coord_rx) = swarm_member("coord", "coordinator", false);
        let (peer_root, _peer_rx) = swarm_member("peer_root", "agent", false);
        let (mut child, _child_rx) = swarm_member("child", "agent", true);
        child.report_back_to_session_id = Some("peer_root".to_string());
        {
            let mut members = swarm_members.write().await;
            members.insert("coord".to_string(), coord);
            members.insert("peer_root".to_string(), peer_root);
            members.insert("child".to_string(), child);
        }

        remove_session_from_swarm(
            "peer_root",
            "swarm-1",
            &swarm_members,
            &swarms_by_id,
            &swarm_coordinators,
            &swarm_runs,
        )
        .await;

        let members = swarm_members.read().await;
        assert_eq!(
            members
                .get("child")
                .and_then(|member| member.report_back_to_session_id.as_deref()),
            Some("coord")
        );
    }

    #[tokio::test]
    async fn update_member_status_notifies_coordinator_when_headless_worker_returns_ready() {
        let swarm_members = Arc::new(RwLock::new(HashMap::new()));
        let swarms_by_id = Arc::new(RwLock::new(HashMap::from([(
            "swarm-1".to_string(),
            HashSet::from(["coord".to_string(), "worker".to_string()]),
        )])));

        let (coord, mut coord_rx) = swarm_member("coord", "coordinator", false);
        let (mut worker, _worker_rx) = swarm_member("worker", "agent", true);
        worker.status = SwarmLifecycleStatus::Running;
        worker.detail = Some("doing task".to_string());
        worker.report_back_to_session_id = Some("coord".to_string());
        {
            let mut members = swarm_members.write().await;
            members.insert("coord".to_string(), coord);
            members.insert("worker".to_string(), worker);
        }

        update_member_status(
            "worker",
            SwarmLifecycleStatus::Ready,
            None,
            &swarm_members,
            &swarms_by_id,
            None,
            None,
            None,
        )
        .await;

        let events: Vec<_> = std::iter::from_fn(|| coord_rx.try_recv().ok()).collect();
        assert!(events.iter().any(|event| {
            matches!(
                event,
                ServerEvent::Notification {
                    notification_type: NotificationType::Message { .. },
                    message,
                    ..
                } if message.contains("finished their work and is ready for more")
            )
        }));
    }

    #[tokio::test]
    async fn member_elapsed_time_runs_only_while_active_and_freezes_afterward() {
        let swarm_members = Arc::new(RwLock::new(HashMap::new()));
        let swarms_by_id = Arc::new(RwLock::new(HashMap::from([(
            "swarm-1".to_string(),
            HashSet::from(["worker".to_string()]),
        )])));
        let (mut worker, _worker_rx) = swarm_member("worker", "agent", true);
        worker.runtime.elapsed_secs = Some(12);
        swarm_members
            .write()
            .await
            .insert("worker".to_string(), worker);

        update_member_status(
            "worker",
            SwarmLifecycleStatus::Running,
            None,
            &swarm_members,
            &swarms_by_id,
            None,
            None,
            None,
        )
        .await;
        assert_eq!(
            swarm_members
                .read()
                .await
                .get("worker")
                .and_then(|member| member.runtime.elapsed_secs),
            None,
            "active members should derive elapsed time from joined_at"
        );

        {
            let mut members = swarm_members.write().await;
            members.get_mut("worker").unwrap().joined_at = Instant::now() - Duration::from_secs(37);
        }
        update_member_status(
            "worker",
            SwarmLifecycleStatus::Completed,
            None,
            &swarm_members,
            &swarms_by_id,
            None,
            None,
            None,
        )
        .await;

        let frozen = swarm_members
            .read()
            .await
            .get("worker")
            .and_then(|member| member.runtime.elapsed_secs)
            .expect("terminal member should retain frozen elapsed time");
        assert!((37..=38).contains(&frozen), "frozen={frozen}");
        tokio::time::sleep(Duration::from_millis(20)).await;
        assert_eq!(
            swarm_members
                .read()
                .await
                .get("worker")
                .and_then(|member| member.runtime.elapsed_secs),
            Some(frozen)
        );
    }

    #[tokio::test]
    async fn update_member_status_prefers_explicit_report_back_owner_over_coordinator() {
        let swarm_members = Arc::new(RwLock::new(HashMap::new()));
        let swarms_by_id = Arc::new(RwLock::new(HashMap::from([(
            "swarm-1".to_string(),
            HashSet::from([
                "coord".to_string(),
                "owner".to_string(),
                "worker".to_string(),
            ]),
        )])));

        let (coord, mut coord_rx) = swarm_member("coord", "coordinator", false);
        let (owner, mut owner_rx) = swarm_member("owner", "agent", false);
        let (mut worker, _worker_rx) = swarm_member("worker", "agent", true);
        worker.status = SwarmLifecycleStatus::Running;
        worker.detail = Some("doing task".to_string());
        worker.report_back_to_session_id = Some("owner".to_string());
        {
            let mut members = swarm_members.write().await;
            members.insert("coord".to_string(), coord);
            members.insert("owner".to_string(), owner);
            members.insert("worker".to_string(), worker);
        }

        update_member_status(
            "worker",
            SwarmLifecycleStatus::Ready,
            None,
            &swarm_members,
            &swarms_by_id,
            None,
            None,
            None,
        )
        .await;

        let owner_events: Vec<_> = std::iter::from_fn(|| owner_rx.try_recv().ok()).collect();
        assert!(owner_events.iter().any(|event| {
            matches!(
                event,
                ServerEvent::Notification {
                    notification_type: NotificationType::Message { .. },
                    message,
                    ..
                } if message.contains("finished their work and is ready for more")
            )
        }));
        let coord_events: Vec<_> = std::iter::from_fn(|| coord_rx.try_recv().ok()).collect();
        assert!(coord_events.iter().all(|event| {
            !matches!(
                event,
                ServerEvent::Notification {
                    notification_type: NotificationType::Message { .. },
                    message,
                    ..
                } if message.contains("finished their work and is ready for more")
            )
        }));
    }

    #[tokio::test]
    async fn update_member_status_includes_completion_report_in_owner_notification() {
        let swarm_members = Arc::new(RwLock::new(HashMap::new()));
        let swarms_by_id = Arc::new(RwLock::new(HashMap::from([(
            "swarm-1".to_string(),
            HashSet::from(["coord".to_string(), "worker".to_string()]),
        )])));

        let (coord, mut coord_rx) = swarm_member("coord", "coordinator", false);
        let (mut worker, _worker_rx) = swarm_member("worker", "agent", true);
        worker.status = SwarmLifecycleStatus::Running;
        worker.report_back_to_session_id = Some("coord".to_string());
        {
            let mut members = swarm_members.write().await;
            members.insert("coord".to_string(), coord);
            members.insert("worker".to_string(), worker);
        }

        update_member_status_with_report(
            "worker",
            SwarmLifecycleStatus::Ready,
            None,
            Some("Validated the parser and all tests passed.".to_string()),
            &swarm_members,
            &swarms_by_id,
            None,
            None,
            None,
        )
        .await;

        let events: Vec<_> = std::iter::from_fn(|| coord_rx.try_recv().ok()).collect();
        assert!(events.iter().any(|event| {
            matches!(
                event,
                ServerEvent::Notification {
                    notification_type: NotificationType::Message { .. },
                    message,
                    ..
                } if message.contains("Report:\nValidated the parser")
                    && !message.contains("No final textual report")
            )
        }));
    }

    #[tokio::test]
    async fn update_member_status_skips_noop_broadcasts() {
        let swarm_members = Arc::new(RwLock::new(HashMap::new()));
        let swarms_by_id = Arc::new(RwLock::new(HashMap::from([(
            "swarm-1".to_string(),
            HashSet::from(["worker".to_string()]),
        )])));

        let (worker, mut worker_rx) = swarm_member("worker", "agent", false);
        swarm_members
            .write()
            .await
            .insert("worker".to_string(), worker);

        update_member_status(
            "worker",
            SwarmLifecycleStatus::Ready,
            None,
            &swarm_members,
            &swarms_by_id,
            None,
            None,
            None,
        )
        .await;

        assert!(worker_rx.try_recv().is_err());

        update_member_status(
            "worker",
            SwarmLifecycleStatus::Other("busy".to_string()),
            Some("working".to_string()),
            &swarm_members,
            &swarms_by_id,
            None,
            None,
            None,
        )
        .await;

        assert!(matches!(
            worker_rx.try_recv(),
            Ok(ServerEvent::SwarmStatus { members }) if members.len() == 1
                && members[0].session_id == "worker"
                && members[0].status == SwarmLifecycleStatus::Other("busy".to_string())
                && members[0].detail.as_deref() == Some("working")
        ));
    }

    fn running_plan_assigned_to(_assignee: &str) -> Arc<RwLock<HashMap<String, RunState>>> {
        Arc::new(RwLock::new(HashMap::<String, RunState>::new()))
    }

    /// The row's status as the run sees it: the run's own entry when it set one, and
    /// the list's row otherwise. The holder is the row's, read from the list.
    async fn salvage_state(
        swarm_runs: &Arc<RwLock<HashMap<String, RunState>>>,
        repo: &std::path::Path,
        row_id: &str,
    ) -> (Option<String>, Option<String>) {
        let run_status = swarm_runs
            .read()
            .await
            .get("swarm-1")
            .and_then(|run| run.get(row_id))
            .map(|state| state.status.clone());
        let holder = crate::todo::load_tasks(Some(repo), "worker")
            .expect("read the list")
            .into_iter()
            .find(|row| row.id == row_id)
            .and_then(|row| row.assigned_to);
        (run_status, holder)
    }

    #[tokio::test]
    async fn salvage_releases_dead_members_rows_and_notifies_coordinator() {
        let swarm_members = Arc::new(RwLock::new(HashMap::new()));
        let swarms_by_id = Arc::new(RwLock::new(HashMap::from([(
            "swarm-1".to_string(),
            HashSet::from(["coord".to_string(), "worker".to_string()]),
        )])));
        let swarm_coordinators = Arc::new(RwLock::new(HashMap::from([(
            "swarm-1".to_string(),
            "coord".to_string(),
        )])));
        let repo = list_repo(&[held_row("task-1", "worker")]);
        let swarm_runs = running_plan_assigned_to("worker");
        let (coord, mut coord_rx) = swarm_member_in(&repo, "coord", "coordinator", false);
        let (worker, _worker_rx) = swarm_member_in(&repo, "worker", "agent", true);
        {
            let mut members = swarm_members.write().await;
            members.insert("coord".to_string(), coord);
            members.insert("worker".to_string(), worker);
        }

        let _outcome = salvage_assignments_of_dead_member(
            "worker",
            "swarm-1",
            &swarm_members,
            &swarms_by_id,
            &swarm_runs,
            &swarm_coordinators,
        )
        .await;

        let (status, holder) = salvage_state(&swarm_runs, repo.path(), "task-1").await;
        assert_eq!(status.as_deref(), Some("queued"), "the row is work again");
        assert_eq!(holder, None, "and the claim is released in the list");

        let coord_events: Vec<_> = std::iter::from_fn(|| coord_rx.try_recv().ok()).collect();
        assert!(
            coord_events.iter().any(|event| matches!(
                event,
                ServerEvent::Notification { message, .. }
                    if message.contains("died") && message.contains("task-1")
            )),
            "coordinator should be told about the salvage, got {coord_events:?}"
        );
    }

    #[tokio::test]
    async fn remove_session_from_swarm_salvages_running_assignments() {
        let swarm_members = Arc::new(RwLock::new(HashMap::new()));
        let swarms_by_id = Arc::new(RwLock::new(HashMap::from([(
            "swarm-1".to_string(),
            HashSet::from(["coord".to_string(), "worker".to_string()]),
        )])));
        let swarm_coordinators = Arc::new(RwLock::new(HashMap::from([(
            "swarm-1".to_string(),
            "coord".to_string(),
        )])));
        let repo = list_repo(&[held_row("task-1", "worker")]);
        let swarm_runs = running_plan_assigned_to("worker");
        let (coord, _coord_rx) = swarm_member_in(&repo, "coord", "coordinator", false);
        let (worker, _worker_rx) = swarm_member_in(&repo, "worker", "agent", true);
        {
            let mut members = swarm_members.write().await;
            members.insert("coord".to_string(), coord);
            members.insert("worker".to_string(), worker);
        }

        remove_session_from_swarm(
            "worker",
            "swarm-1",
            &swarm_members,
            &swarms_by_id,
            &swarm_coordinators,
            &swarm_runs,
        )
        .await;

        let (status, holder) = salvage_state(&swarm_runs, repo.path(), "task-1").await;
        assert_eq!(status.as_deref(), Some("queued"), "the row is work again");
        assert_eq!(holder, None, "and the claim is released in the list");
    }

    #[tokio::test]
    async fn salvage_reclaims_tasks_of_vanished_assignee() {
        // The assignee is not a swarm member at all (zombie left over from a
        // previous process): no grace period applies and the sweep must
        // requeue its running task.
        let swarm_members = Arc::new(RwLock::new(HashMap::new()));
        let swarms_by_id = Arc::new(RwLock::new(HashMap::from([(
            "swarm-1".to_string(),
            HashSet::from(["coord".to_string()]),
        )])));
        let swarm_coordinators = Arc::new(RwLock::new(HashMap::from([(
            "swarm-1".to_string(),
            "coord".to_string(),
        )])));
        let repo = list_repo(&[held_row("task-1", "ghost")]);
        let swarm_runs = running_plan_assigned_to("ghost");
        let (coord, _coord_rx) = swarm_member_in(&repo, "coord", "coordinator", false);
        swarm_members
            .write()
            .await
            .insert("coord".to_string(), coord);

        salvage_dead_assignees(
            &swarm_members,
            &swarms_by_id,
            &swarm_runs,
            &swarm_coordinators,
        )
        .await;

        let (status, holder) = salvage_state(&swarm_runs, repo.path(), "task-1").await;
        assert_eq!(status.as_deref(), Some("queued"), "the row is work again");
        assert_eq!(holder, None, "and the claim is released in the list");
    }

    #[tokio::test]
    async fn salvage_grants_grace_to_recently_crashed_member() {
        // A member marked crashed moments ago may be mid reload-recovery; the
        // sweep must not reclaim its work inside the grace window.
        let swarm_members = Arc::new(RwLock::new(HashMap::new()));
        let swarms_by_id = Arc::new(RwLock::new(HashMap::from([(
            "swarm-1".to_string(),
            HashSet::from(["worker".to_string()]),
        )])));
        let swarm_coordinators = Arc::new(RwLock::new(HashMap::new()));
        let repo = list_repo(&[held_row("task-1", "grace-worker")]);
        let swarm_runs = running_plan_assigned_to("grace-worker");
        // The assignee is alive on its own clock, so the staleness phase leaves
        // the node alone; only the salvage phase's grace window is under test.
        crate::session_metrics::record_activity("grace-worker");
        let (mut worker, _worker_rx) = swarm_member_in(&repo, "grace-worker", "agent", true);
        worker.status = SwarmLifecycleStatus::Crashed;
        worker.last_status_change = Instant::now();
        swarm_members
            .write()
            .await
            .insert("grace-worker".to_string(), worker);

        salvage_dead_assignees(
            &swarm_members,
            &swarms_by_id,
            &swarm_runs,
            &swarm_coordinators,
        )
        .await;

        let (status, holder) = salvage_state(&swarm_runs, repo.path(), "task-1").await;
        assert_eq!(
            status, None,
            "a live holder keeps the row as the list has it"
        );
        assert_eq!(holder.as_deref(), Some("grace-worker"));
    }

    #[tokio::test]
    async fn update_member_status_notifies_owner_when_worker_crashes_mid_task() {
        let swarm_members = Arc::new(RwLock::new(HashMap::new()));
        let swarms_by_id = Arc::new(RwLock::new(HashMap::from([(
            "swarm-1".to_string(),
            HashSet::from(["owner".to_string(), "worker".to_string()]),
        )])));
        let (owner, mut owner_rx) = swarm_member("owner", "coordinator", false);
        let (mut worker, _worker_rx) = swarm_member("worker", "agent", true);
        worker.status = SwarmLifecycleStatus::Running;
        worker.report_back_to_session_id = Some("owner".to_string());
        {
            let mut members = swarm_members.write().await;
            members.insert("owner".to_string(), owner);
            members.insert("worker".to_string(), worker);
        }

        update_member_status(
            "worker",
            SwarmLifecycleStatus::Crashed,
            Some("client disconnected while processing".to_string()),
            &swarm_members,
            &swarms_by_id,
            None,
            None,
            None,
        )
        .await;

        let owner_events: Vec<_> = std::iter::from_fn(|| owner_rx.try_recv().ok()).collect();
        assert!(
            owner_events.iter().any(|event| matches!(
                event,
                ServerEvent::Notification { message, .. }
                    if message.contains("crashed while working")
            )),
            "owner should be notified of the crash, got {owner_events:?}"
        );
    }
}
