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
            .filter(|member| swarm_root(&members, &member.session_id).as_deref() == Some(swarm_id))
            .map(|member| member.session_id.clone())
            .collect();
        let working_dir = members
            .get(requester)
            .and_then(|member| member.working_dir.clone())
            .or_else(|| {
                members
                    .values()
                    .find(|member| {
                        swarm_root(&members, &member.session_id).as_deref() == Some(swarm_id)
                    })
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

/// The sessions in a run: everything whose report-back chain roots at `swarm_id`.
pub(super) async fn swarm_session_ids(
    swarm_id: &str,
    swarm_members: &Arc<RwLock<HashMap<String, SwarmMember>>>,
) -> Vec<String> {
    let members = swarm_members.read().await;
    let mut ids: Vec<String> = members
        .values()
        .filter(|member| swarm_root(&members, &member.session_id).as_deref() == Some(swarm_id))
        .map(|member| member.session_id.clone())
        .collect();
    ids.sort();
    ids
}

/// The rows as the run sees them: a row the run set a status for reads that status,
/// and a row it did not reads its own. Cheap because the run's map holds only the
/// rows a turn touched.
/// The rows a session's client shows: a run's rows when the session is in a run,
/// the whole list when it stands alone. The list is one file, so the server reads
/// it and the client renders what it is sent (rules 1 and 7).
pub(super) async fn session_rows(
    session_id: &str,
    swarm_members: &Arc<RwLock<HashMap<String, SwarmMember>>>,
) -> Vec<TaskItem> {
    let (root, working_dir, in_run) = {
        let members = swarm_members.read().await;
        let root = swarm_root(&members, session_id);
        let working_dir = members
            .get(session_id)
            .and_then(|member| member.working_dir.clone());
        let in_run = root.as_deref().is_some_and(|root| {
            members.values().any(|member| {
                member.session_id != session_id
                    && swarm_root(&members, &member.session_id).as_deref() == Some(root)
            })
        });
        (root, working_dir, in_run)
    };
    match root {
        Some(root) if in_run => swarm_rows(&root, session_id, swarm_members).await,
        _ => crate::todo::load_tasks(working_dir.as_deref(), session_id).unwrap_or_default(),
    }
}

/// Send one session its rows.
pub(super) async fn send_todos_to_session(
    session_id: &str,
    swarm_members: &Arc<RwLock<HashMap<String, SwarmMember>>>,
) {
    let items = session_rows(session_id, swarm_members).await;
    let event = ServerEvent::SwarmPlan {
        swarm_id: session_id.to_string(),
        items,
    };
    fanout_session_event(swarm_members, session_id, event).await;
}

/// Refresh every session's list. Called on any write, because the file is shared:
/// a write in one session can unblock a row in another.
pub(super) async fn broadcast_todos(swarm_members: &Arc<RwLock<HashMap<String, SwarmMember>>>) {
    let session_ids: Vec<String> = {
        let members = swarm_members.read().await;
        members.keys().cloned().collect()
    };
    for session_id in session_ids {
        send_todos_to_session(&session_id, swarm_members).await;
    }
}

/// The run's lifecycle for one row, in the run's own map.
pub(super) fn set_run_status(run: &mut RunState, id: &str, status: &str) {
    run.entry(id.to_string()).or_default().status = status.to_string();
}
use super::persist_swarm_state_for;
use crate::agent::Agent;
use crate::plan::TaskItem;
use crate::protocol::{NotificationType, ServerEvent, SwarmLifecycleStatus};
use crate::session::Session;
use anyhow::Result;
use futures::future::try_join_all;
use kcode_swarm_core::completion_status_intro;
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

/// The run a session belongs to: the root of its report-back chain.
///
/// The spawn edge *is* the membership, so nothing stores which swarm a session is
/// in: a session that reports back to nobody roots its own run, and a session the
/// server does not know has none. Spawning is root-only (0.4b), so the root is also
/// the run's coordinator, the session holding the run's anchor row.
pub(super) fn swarm_root(
    members: &HashMap<String, SwarmMember>,
    session_id: &str,
) -> Option<String> {
    if !members.contains_key(session_id) {
        return None;
    }
    Some(
        swarm_ancestors(members, session_id)
            .into_iter()
            .rev()
            .find(|ancestor| members.contains_key(ancestor))
            .unwrap_or_else(|| session_id.to_string()),
    )
}

/// Whether a session roots its own run, which is what makes it the coordinator:
/// it is the session holding the run's anchor row.
pub(super) fn swarm_is_root(members: &HashMap<String, SwarmMember>, session_id: &str) -> bool {
    swarm_root(members, session_id).as_deref() == Some(session_id)
}

/// The role a member holds: the run's root coordinates, everything it spawned
/// works a row the run dispatches.
pub(super) fn swarm_role(members: &HashMap<String, SwarmMember>, session_id: &str) -> &'static str {
    if swarm_is_root(members, session_id) {
        "coordinator"
    } else {
        "agent"
    }
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
/// Only sessions spawned by another agent (`report_back_to_session_id` set) are
/// eligible; user-created sessions are never reaped.
pub(super) fn idle_spawned_worker_reap_candidates(
    members: &HashMap<String, SwarmMember>,
    idle_after: Duration,
) -> Vec<String> {
    members
        .values()
        .filter(|member| member.report_back_to_session_id.is_some())
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
            // Only the run's own map speaks for a row it has tracked; a row it
            // has not is still this holder's.
            run.get(id)
                .is_none_or(|state| !crate::plan::is_terminal_status(&state.status))
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
    swarm_runs: &Arc<RwLock<HashMap<String, RunState>>>,
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
                    .find(|member| {
                        swarm_root(&members, &member.session_id).as_deref() == Some(swarm_id)
                    })
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
        runs: Arc::clone(swarm_runs),
    };
    persist_swarm_state_for(swarm_id, &swarm_state).await;
    broadcast_todos(swarm_members).await;
    notify_coordinator_of_salvage(session_id, &outcome, swarm_members).await;
    outcome
}

/// Deliver a salvage notification to the swarm's current coordinator (when it
/// is not the dead session itself).
async fn notify_coordinator_of_salvage(
    session_id: &str,
    outcome: &DeadMemberSalvage,
    swarm_members: &Arc<RwLock<HashMap<String, SwarmMember>>>,
) {
    let coordinator_id = {
        let members = swarm_members.read().await;
        swarm_root(&members, session_id)
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
/// salvaging inside that window would double-assign their work. A member the run
/// still holds keeps its rows until this sweep frees them: `swarm_rows` shows a
/// dead holder's row as that holder's, so dispatch and the sweep never disagree
/// about who owns a row, and only this sweep moves it.
pub(super) async fn salvage_dead_assignees(
    swarm_members: &Arc<RwLock<HashMap<String, SwarmMember>>>,
    swarm_runs: &Arc<RwLock<HashMap<String, RunState>>>,
) {
    let salvage_grace = swarm_task_stale_after();
    let swarm_ids: Vec<String> = {
        let members = swarm_members.read().await;
        let mut ids: Vec<String> = members
            .values()
            .filter_map(|member| swarm_root(&members, &member.session_id))
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
                .find(|member| {
                    swarm_root(&members, &member.session_id).as_deref() == Some(swarm_id.as_str())
                })
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
        salvage_assignments_of_dead_member(&session_id, &swarm_id, swarm_members, swarm_runs).await;
    }
}

fn swarm_broadcast_key(
    swarm_id: &str,
    swarm_members: &Arc<RwLock<HashMap<String, SwarmMember>>>,
) -> String {
    format!("{:p}:{swarm_id}", Arc::as_ptr(swarm_members))
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
                    role: Some(swarm_role(&members_guard, &m.session_id).to_string()),
                    is_headless: Some(m.is_headless),
                    live_attachments: Some(m.event_txs.len()),
                    status_age_secs: Some(status_age_secs(m.last_status_change)),
                    report_back_to_session_id: m.report_back_to_session_id.clone(),
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
) {
    let session_ids = swarm_session_ids(swarm_id, swarm_members).await;
    if session_ids.is_empty() {
        return;
    }

    if session_ids.len() < swarm_status_debounce_member_threshold() {
        broadcast_swarm_status_now(session_ids, swarm_members).await;
        return;
    }

    let key = swarm_broadcast_key(swarm_id, swarm_members);
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
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(swarm_status_debounce_ms())).await;
            let session_ids = swarm_session_ids(&swarm_id, &swarm_members).await;
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

pub(super) async fn remove_session_from_swarm(
    session_id: &str,
    swarm_id: &str,
    swarm_members: &Arc<RwLock<HashMap<String, SwarmMember>>>,
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
    salvage_assignments_of_dead_member(session_id, swarm_id, swarm_members, swarm_runs).await;

    let was_coordinator = swarm_id == session_id;

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
                members.get(parent).is_some_and(|member| {
                    swarm_root(&members, &member.session_id).as_deref() == Some(swarm_id)
                })
            }
        } else {
            false
        };
        if grandparent_is_live {
            departing_parent.clone()
        } else {
            None
        }
    };
    let mut reparented: Vec<String> = Vec::new();
    {
        let mut members = swarm_members.write().await;
        for member in members.values_mut() {
            if member.report_back_to_session_id.as_deref() == Some(session_id) {
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
            runs: Arc::clone(swarm_runs),
        };
        persist_swarm_state_for(swarm_id, &swarm_state).await;
    }

    let remaining_member_count = swarm_session_ids(swarm_id, swarm_members).await.len();
    log_swarm_lifecycle(
        "member_remove_done",
        vec![
            ("session_id", session_id.to_string()),
            ("swarm_id", swarm_id.to_string()),
            ("was_coordinator", was_coordinator.to_string()),
            ("new_coordinator_session_id", session_id.to_string()),
            ("remaining_member_count", remaining_member_count.to_string()),
            ("elapsed_ms", started.elapsed().as_millis().to_string()),
        ],
    );
    broadcast_swarm_status(swarm_id, swarm_members).await;
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
            (
                member.friendly_name.clone(),
                swarm_root(&members, session_id),
            )
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

pub(super) async fn update_member_status(
    session_id: &str,
    status: SwarmLifecycleStatus,
    detail: Option<String>,
    swarm_members: &Arc<RwLock<HashMap<String, SwarmMember>>>,
    event_history: Option<&Arc<RwLock<std::collections::VecDeque<SwarmEvent>>>>,
    event_counter: Option<&Arc<std::sync::atomic::AtomicU64>>,
    swarm_event_tx: Option<&broadcast::Sender<SwarmEvent>>,
) {
    let detail_present = detail.is_some();
    let (
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
            let member_changed = status_changed || detail_changed;
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
            (
                name,
                member_changed,
                status_changed,
                previous_status,
                is_headless,
                report_back_to_session_id,
            )
        } else {
            (None, false, false, SwarmLifecycleStatus::Ready, false, None)
        }
    };
    let swarm_id = {
        let members = swarm_members.read().await;
        swarm_root(&members, session_id)
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

        broadcast_swarm_status(id, swarm_members).await;

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
            // The run's root is the coordinator, so the fallback owner is it.
            let fallback_coordinator_id = (id != session_id).then(|| id.clone());
            let recipient_session_id = report_back_to_session_id
                .clone()
                .filter(|owner_id| owner_id != session_id)
                .or(fallback_coordinator_id);
            if let Some(recipient_session_id) = recipient_session_id {
                let name = agent_name
                    .as_deref()
                    .unwrap_or(&session_id[..8.min(session_id.len())]);
                let msg = completion_status_intro(name, status.as_str());
                let _ = fanout_session_event(
                    swarm_members,
                    &recipient_session_id,
                    ServerEvent::Notification {
                        from_session: session_id.to_string(),
                        from_name: agent_name.clone(),
                        notification_type: NotificationType::Message {
                            scope: Some("swarm".to_string()),
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
        broadcast_swarm_status, member_in_status_broadcast, parse_swarm_tasks,
        remove_session_from_swarm, salvage_assignments_of_dead_member, salvage_dead_assignees,
        swarm_ancestors, swarm_is_root, swarm_is_self_or_ancestor, swarm_spawn_depth,
        update_member_status,
    };
    use crate::plan::TaskItem;
    use crate::protocol::SwarmLifecycleStatus;
    use crate::protocol::{NotificationType, ServerEvent};
    use crate::server::{RunState, SwarmMember};
    use kcode_swarm_core::{summarize_plan_items, truncate_detail};
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::{Duration, Instant};
    use tokio::sync::{RwLock, mpsc};

    fn plan_item(id: &str, content: &str) -> TaskItem {
        TaskItem {
            content: content.to_string(),
            id: id.to_string(),
            ..Default::default()
        }
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
    /// A member and the receiver of its event channel.
    ///
    /// `report_back_to` is the spawn edge, and the spawn edge is the membership: a
    /// member that reports back to nobody roots its own run (and is that run's
    /// coordinator), and everyone else belongs to the run its chain roots at.
    fn swarm_member(
        session_id: &str,
        report_back_to: Option<&str>,
        is_headless: bool,
    ) -> (SwarmMember, mpsc::UnboundedReceiver<ServerEvent>) {
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        (
            SwarmMember {
                session_id: session_id.to_string(),
                event_tx,
                event_txs: HashMap::new(),
                working_dir: None,
                status: SwarmLifecycleStatus::Ready,
                detail: None,
                task_label: None,
                friendly_name: Some(session_id.to_string()),
                report_back_to_session_id: report_back_to.map(str::to_string),
                joined_at: Instant::now(),
                last_status_change: Instant::now(),
                is_headless,
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
            ..Default::default()
        }
    }

    /// A member that works in `repo`.
    fn swarm_member_in(
        repo: &tempfile::TempDir,
        session_id: &str,
        report_back_to: Option<&str>,
        is_headless: bool,
    ) -> (SwarmMember, mpsc::UnboundedReceiver<ServerEvent>) {
        let (mut member, rx) = swarm_member(session_id, report_back_to, is_headless);
        member.working_dir = Some(repo.path().to_path_buf());
        (member, rx)
    }

    /// A member whose spawn edge is `parent` (a root when `parent` is `None`).
    fn member_with_parent(session_id: &str, parent: Option<&str>) -> SwarmMember {
        let (member, _rx) = swarm_member(session_id, parent, false);
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

        // Same shape but no spawner: a user-created session, and also a run's
        // root (its coordinator). Never reaped.
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

        let members: HashMap<String, SwarmMember> = [reapable, stopped, user_owned, running, fresh]
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

        let (live, _rx) = swarm_member("live", None, false);
        assert!(member_in_status_broadcast(&live, retention));

        let (mut fresh_terminal, _rx) = swarm_member("fresh", None, false);
        fresh_terminal.status = SwarmLifecycleStatus::Completed;
        assert!(member_in_status_broadcast(&fresh_terminal, retention));

        let (mut stale_terminal, _rx) = swarm_member("stale", None, false);
        stale_terminal.status = SwarmLifecycleStatus::Stopped;
        stale_terminal.last_status_change = Instant::now() - Duration::from_secs(901);
        assert!(!member_in_status_broadcast(&stale_terminal, retention));

        // A stale *live* status is never filtered, no matter how old.
        let (mut old_live, _rx) = swarm_member("old-live", None, false);
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
    async fn swarm_status_immediate_broadcasts_can_invert_on_one_member_channel() {
        let (worker, mut worker_rx) = swarm_member("worker", None, false);
        let swarm_members = Arc::new(RwLock::new(HashMap::from([("worker".to_string(), worker)])));

        // Broadcast A: snapshots status "ready", then is forced to yield at
        // the fanout write acquisition, before sending.
        let a = tokio::spawn({
            let swarm_members = Arc::clone(&swarm_members);
            async move {
                // Initial task budget is 128. Leave exactly 2 units so the two
                // read acquisitions (session-id list + status snapshot)
                // succeed and the fanout write acquisition forces a yield.
                for _ in 0..126 {
                    tokio::task::coop::consume_budget().await;
                }
                broadcast_swarm_status("worker", &swarm_members).await;
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
        broadcast_swarm_status("worker", &swarm_members).await;

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
    async fn remove_session_reparents_children_to_live_grandparent() {
        let swarm_members = Arc::new(RwLock::new(HashMap::new()));
        let swarm_runs = Arc::new(RwLock::new(HashMap::new()));

        let (root, _root_rx) = swarm_member("root", None, false);
        let (mid, _mid_rx) = swarm_member("mid", Some("root"), true);
        let (leaf, _leaf_rx) = swarm_member("leaf", Some("mid"), true);
        {
            let mut members = swarm_members.write().await;
            members.insert("root".to_string(), root);
            members.insert("mid".to_string(), mid);
            members.insert("leaf".to_string(), leaf);
        }

        remove_session_from_swarm("mid", "root", &swarm_members, &swarm_runs).await;

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

    /// A departing member's children whose parent has no live grandparent to
    /// inherit stop belonging to the departing run: with the spawn edge as the
    /// membership, each is promoted to a root of its own run rather than left on
    /// a dangling edge.
    #[tokio::test]
    async fn remove_session_promotes_orphans_to_roots_when_no_grandparent() {
        let swarm_members = Arc::new(RwLock::new(HashMap::new()));
        let swarm_runs = Arc::new(RwLock::new(HashMap::new()));

        // peer_root is itself a root (no parent), so its children have no
        // grandparent to inherit.
        let (peer_root, _peer_rx) = swarm_member("peer_root", None, false);
        let (child, _child_rx) = swarm_member("child", Some("peer_root"), true);
        {
            let mut members = swarm_members.write().await;
            members.insert("peer_root".to_string(), peer_root);
            members.insert("child".to_string(), child);
        }

        remove_session_from_swarm("peer_root", "peer_root", &swarm_members, &swarm_runs).await;

        let members = swarm_members.read().await;
        assert_eq!(
            members
                .get("child")
                .and_then(|member| member.report_back_to_session_id.as_deref()),
            None
        );
        assert!(swarm_is_root(&members, "child"));
    }

    #[tokio::test]
    async fn update_member_status_notifies_coordinator_when_headless_worker_returns_ready() {
        let swarm_members = Arc::new(RwLock::new(HashMap::new()));

        let (coord, mut coord_rx) = swarm_member("coord", None, false);
        let (mut worker, _worker_rx) = swarm_member("worker", Some("coord"), true);
        worker.status = SwarmLifecycleStatus::Running;
        worker.detail = Some("doing task".to_string());
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
        let (mut worker, _worker_rx) = swarm_member("worker", None, true);
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

        // The owner reports back to coord, so it is one of coord's workers, and
        // the worker reports back to the owner, not to the run's coordinator.
        let (coord, mut coord_rx) = swarm_member("coord", None, false);
        let (owner, mut owner_rx) = swarm_member("owner", Some("coord"), false);
        let (mut worker, _worker_rx) = swarm_member("worker", Some("owner"), true);
        worker.status = SwarmLifecycleStatus::Running;
        worker.detail = Some("doing task".to_string());
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
    async fn update_member_status_skips_noop_broadcasts() {
        let swarm_members = Arc::new(RwLock::new(HashMap::new()));

        let (worker, mut worker_rx) = swarm_member("worker", None, false);
        swarm_members
            .write()
            .await
            .insert("worker".to_string(), worker);

        update_member_status(
            "worker",
            SwarmLifecycleStatus::Ready,
            None,
            &swarm_members,
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
        swarm_id: &str,
        repo: &std::path::Path,
        row_id: &str,
    ) -> (Option<String>, Option<String>) {
        let run_status = swarm_runs
            .read()
            .await
            .get(swarm_id)
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
        let repo = list_repo(&[held_row("task-1", "worker")]);
        let swarm_runs = running_plan_assigned_to("worker");
        let (coord, mut coord_rx) = swarm_member_in(&repo, "coord", None, false);
        let (worker, _worker_rx) = swarm_member_in(&repo, "worker", Some("coord"), true);
        {
            let mut members = swarm_members.write().await;
            members.insert("coord".to_string(), coord);
            members.insert("worker".to_string(), worker);
        }

        let _outcome =
            salvage_assignments_of_dead_member("worker", "coord", &swarm_members, &swarm_runs)
                .await;

        let (status, holder) = salvage_state(&swarm_runs, "coord", repo.path(), "task-1").await;
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
        let repo = list_repo(&[held_row("task-1", "worker")]);
        let swarm_runs = running_plan_assigned_to("worker");
        let (coord, _coord_rx) = swarm_member_in(&repo, "coord", None, false);
        let (worker, _worker_rx) = swarm_member_in(&repo, "worker", Some("coord"), true);
        {
            let mut members = swarm_members.write().await;
            members.insert("coord".to_string(), coord);
            members.insert("worker".to_string(), worker);
        }

        remove_session_from_swarm("worker", "coord", &swarm_members, &swarm_runs).await;

        let (status, holder) = salvage_state(&swarm_runs, "coord", repo.path(), "task-1").await;
        assert_eq!(status.as_deref(), Some("queued"), "the row is work again");
        assert_eq!(holder, None, "and the claim is released in the list");
    }

    #[tokio::test]
    async fn salvage_reclaims_tasks_of_vanished_assignee() {
        // The assignee is not a swarm member at all (zombie left over from a
        // previous process): no grace period applies and the sweep must
        // requeue its running task.
        let swarm_members = Arc::new(RwLock::new(HashMap::new()));
        let repo = list_repo(&[held_row("task-1", "ghost")]);
        let swarm_runs = running_plan_assigned_to("ghost");
        let (coord, _coord_rx) = swarm_member_in(&repo, "coord", None, false);
        swarm_members
            .write()
            .await
            .insert("coord".to_string(), coord);

        salvage_dead_assignees(&swarm_members, &swarm_runs).await;

        let (status, holder) = salvage_state(&swarm_runs, "coord", repo.path(), "task-1").await;
        assert_eq!(status.as_deref(), Some("queued"), "the row is work again");
        assert_eq!(holder, None, "and the claim is released in the list");
    }

    #[tokio::test]
    async fn salvage_grants_grace_to_recently_crashed_member() {
        // A member marked crashed moments ago may be mid reload-recovery; the
        // sweep must not reclaim its work inside the grace window.
        let swarm_members = Arc::new(RwLock::new(HashMap::new()));
        let repo = list_repo(&[held_row("task-1", "grace-worker")]);
        let swarm_runs = running_plan_assigned_to("grace-worker");
        // The assignee is alive on its own clock, so the staleness phase leaves
        // the node alone; only the salvage phase's grace window is under test.
        crate::session_metrics::record_activity("grace-worker");
        let (mut worker, _worker_rx) = swarm_member_in(&repo, "grace-worker", None, true);
        worker.status = SwarmLifecycleStatus::Crashed;
        worker.last_status_change = Instant::now();
        swarm_members
            .write()
            .await
            .insert("grace-worker".to_string(), worker);

        salvage_dead_assignees(&swarm_members, &swarm_runs).await;

        let (status, holder) =
            salvage_state(&swarm_runs, "grace-worker", repo.path(), "task-1").await;
        assert_eq!(
            status, None,
            "a live holder keeps the row as the list has it"
        );
        assert_eq!(holder.as_deref(), Some("grace-worker"));
    }

    #[tokio::test]
    async fn update_member_status_notifies_owner_when_worker_crashes_mid_task() {
        let swarm_members = Arc::new(RwLock::new(HashMap::new()));
        let (owner, mut owner_rx) = swarm_member("owner", None, false);
        let (mut worker, _worker_rx) = swarm_member("worker", Some("owner"), true);
        worker.status = SwarmLifecycleStatus::Running;
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
