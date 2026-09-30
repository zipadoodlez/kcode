//! Server-initiated ("wake") turns for live sessions.
//!
//! Several server paths start a full conversation turn in a session without
//! that session's client sending a message: swarm DM/broadcast wake delivery,
//! background-task completion wakes, scheduled-task delivery, and post-reload
//! resume. Those turns must keep the same bookkeeping as client-initiated
//! turns, otherwise the swarm member status stays "ready/idle" while the agent
//! is actually streaming and attached TUIs never learn the turn finished.
//!
//! This module is the single shared implementation: it marks the member
//! `running` while the turn streams, flips it back to `ready` (with a
//! completion report) or `failed` at the end, and fans out a terminal
//! `Done`/`Error` event (id 0) so attached clients can settle the externally
//! started turn in their UI.

use super::client_lifecycle::process_locked_message_streaming_mpsc;
use super::{
    SessionAgents, SwarmEvent, SwarmMember, session_event_fanout_sender, truncate_detail,
    update_member_status, update_member_status_with_report,
};
use crate::agent::Agent;
use crate::protocol::ServerEvent;
use crate::protocol::SwarmLifecycleStatus;
use crate::todo::{TaskItem, load_tasks};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use tokio::sync::{OwnedMutexGuard, RwLock, broadcast};

/// Swarm bookkeeping handles needed to keep member status accurate around a
/// server-initiated turn.
#[derive(Clone)]
pub(super) struct LiveTurnSwarmContext {
    pub members: Arc<RwLock<HashMap<String, SwarmMember>>>,
    pub swarms_by_id: Arc<RwLock<HashMap<String, HashSet<String>>>>,
    pub event_history: Arc<RwLock<VecDeque<SwarmEvent>>>,
    pub event_counter: Arc<AtomicU64>,
    pub event_tx: broadcast::Sender<SwarmEvent>,
}

impl LiveTurnSwarmContext {
    pub(super) fn new(
        members: &Arc<RwLock<HashMap<String, SwarmMember>>>,
        swarms_by_id: &Arc<RwLock<HashMap<String, HashSet<String>>>>,
        event_history: &Arc<RwLock<VecDeque<SwarmEvent>>>,
        event_counter: &Arc<AtomicU64>,
        event_tx: &broadcast::Sender<SwarmEvent>,
    ) -> Self {
        Self {
            members: Arc::clone(members),
            swarms_by_id: Arc::clone(swarms_by_id),
            event_history: Arc::clone(event_history),
            event_counter: Arc::clone(event_counter),
            event_tx: event_tx.clone(),
        }
    }
}

/// One turn of a run: what the session is asked, how it is presented, and the row
/// it is working when it came from the list.
///
/// The row id is what lets a run tell a row it already worked from one it has not,
/// which is the run's whole bound: only a close removes a row, so a row left open
/// would otherwise be picked again forever.
pub(super) struct TurnSeed {
    pub message: String,
    pub system_reminder: Option<String>,
    pub display_role: Option<crate::session::StoredDisplayRole>,
    pub row_id: Option<String>,
    /// Short label for the member status while this turn runs.
    pub detail: Option<String>,
}

impl TurnSeed {
    /// A turn asked for in words: a client message, a wake, a system notice.
    pub(super) fn asked(
        message: &str,
        system_reminder: Option<String>,
        display_role: Option<crate::session::StoredDisplayRole>,
    ) -> Self {
        let detail = Some(truncate_detail(message, 120)).filter(|detail| !detail.is_empty());
        Self {
            message: message.to_string(),
            system_reminder,
            display_role,
            row_id: None,
            detail,
        }
    }

    /// The next row this run works, as the turn that asks for it. The payload is
    /// the row's own words.
    fn row(row: &TaskItem) -> Self {
        Self {
            message: row_turn_message(row),
            system_reminder: None,
            display_role: None,
            row_id: Some(row.id.clone()),
            detail: None,
        }
    }

    pub(super) fn with_detail(mut self, detail: Option<String>) -> Self {
        self.detail = detail;
        self
    }
}

/// Reserve the live agent for `session_id` when the session has at least one
/// live client attachment and its agent is currently idle.
///
/// The returned guard *is* the reservation: it stays held until the tracked
/// turn finishes, so two concurrent wakes cannot both observe the agent as
/// idle and then serialize behind each other (#1152).
pub(super) async fn idle_live_agent(
    session_id: &str,
    sessions: &SessionAgents,
    swarm_members: &Arc<RwLock<HashMap<String, SwarmMember>>>,
) -> Option<OwnedMutexGuard<Agent>> {
    let agent = {
        let guard = sessions.read().await;
        guard.get(session_id).cloned()
    }?;

    let has_live_attachments = {
        let members = swarm_members.read().await;
        members
            .get(session_id)
            .map(|member| !member.event_txs.is_empty() || !member.event_tx.is_closed())
            .unwrap_or(false)
    };
    if !has_live_attachments {
        return None;
    }

    agent.try_lock_owned().ok()
}

/// Whether `session_id` may take another turn without a user's message.
///
/// Holding a row says who owes the work, not that the session may keep going, so
/// this is a permission and it defaults off: the user turns it on for the session.
/// A member spawned for a run is driven by its plan rather than by the rows it
/// holds, so only attended sessions are continued here, until rows are what seeds a
/// run.
async fn may_continue_on_its_own(session_id: &str, swarm: &LiveTurnSwarmContext) -> bool {
    if !crate::config::config().features.auto_poke {
        return false;
    }
    let (headless, attached) = {
        let members = swarm.members.read().await;
        match members.get(session_id) {
            Some(member) => (member.is_headless, !member.event_txs.is_empty()),
            None => (false, false),
        }
    };
    !headless && attached
}

/// The next row this session holds and may work: file order, held by it, with every
/// blocker already gone and not already worked in this run.
///
/// Readiness is a join against the file rather than something a row says about
/// itself, because a blocker that closes is deleted and its id is left behind.
/// `worked` is the run's own record, so a row left open stops the run instead of
/// being picked again and again.
fn next_held_ready_row<'a>(
    rows: &'a [TaskItem],
    session_id: &str,
    worked: &HashSet<String>,
) -> Option<&'a TaskItem> {
    let present: HashSet<&str> = rows.iter().map(|row| row.id.as_str()).collect();
    rows.iter().find(|row| {
        row.assigned_to.as_deref() == Some(session_id)
            && !worked.contains(&row.id)
            && row
                .blocked_by
                .iter()
                .all(|blocker| !present.contains(blocker.as_str()))
    })
}

/// The whole payload of a continuation is the row's own words.
fn row_turn_message(row: &TaskItem) -> String {
    let mut message = format!(
        "Continue the work list. Next row: {}\ncontent: {}",
        row.id, row.content
    );
    if let Some(note) = row.note.as_deref().filter(|note| !note.trim().is_empty()) {
        message.push_str(&format!("\nnote: {note}"));
    }
    message.push_str("\nWork it and close it with the result.");
    message
}

/// The next row this session holds, as the turn that asks for it.
fn next_row_turn(agent: &Agent, session_id: &str, worked: &HashSet<String>) -> Option<TurnSeed> {
    let working_dir = agent.working_dir().map(PathBuf::from);
    let rows = load_tasks(working_dir.as_deref(), session_id).ok()?;
    next_held_ready_row(&rows, session_id, worked).map(TurnSeed::row)
}

/// Continue a session with the next row it holds, when it is allowed to keep going
/// on its own. Returns whether a turn was started.
///
/// This is what replaces the client-side poke: the session's own turns are the
/// loop, and the loop ends by itself, because a session holding nothing ready is
/// never continued.
pub(super) async fn continue_with_next_row(
    session_id: &str,
    sessions: &SessionAgents,
    swarm: LiveTurnSwarmContext,
) -> bool {
    if !may_continue_on_its_own(session_id, &swarm).await {
        return false;
    }
    let working_dir = {
        let agents = sessions.read().await;
        agents
            .get(session_id)
            .and_then(|agent| agent.try_lock().ok())
            .and_then(|agent| agent.working_dir().map(PathBuf::from))
    };
    let Ok(rows) = load_tasks(working_dir.as_deref(), session_id) else {
        return false;
    };
    let Some(row) = next_held_ready_row(&rows, session_id, &HashSet::new()) else {
        return false;
    };
    let seed = TurnSeed::row(row);
    run_live_turn_if_idle(session_id, seed, sessions, swarm).await
}

/// Spawn `seed` as a full tracked turn in a live session.
///
/// Mirrors the client-initiated turn lifecycle: the swarm member is marked
/// `running` before the turn starts and `ready` (with a completion report) or
/// `failed` when it finishes. A synthetic terminal `Done { id: 0 }` (or
/// `Error { id: 0, .. }`) is fanned out to attached clients so their UI can
/// finish rendering the externally started turn.
pub(super) async fn spawn_tracked_live_turn(
    session_id: &str,
    mut agent: OwnedMutexGuard<Agent>,
    seed: TurnSeed,
    swarm: LiveTurnSwarmContext,
) {
    update_member_status(
        session_id,
        SwarmLifecycleStatus::Running,
        seed.detail.clone(),
        &swarm.members,
        &swarm.swarms_by_id,
        Some(&swarm.event_history),
        Some(&swarm.event_counter),
        Some(&swarm.event_tx),
    )
    .await;

    let event_tx = session_event_fanout_sender(session_id.to_string(), Arc::clone(&swarm.members));
    let session_id = session_id.to_string();
    tokio::spawn(async move {
        // A session's own turns are the loop: after each one, take the next row it
        // holds while it may keep going on its own. The reservation is held across
        // the whole loop, so a wake cannot interleave, and each turn's terminal
        // status is published before the loop asks for the next row.
        //
        // `worked` is the loop's stop: a row worked once in this run is not picked
        // again, so a row left open ends the run instead of spinning it.
        let mut worked: HashSet<String> = HashSet::new();
        let mut next = Some(seed);
        while let Some(seed) = next {
            if let Some(row_id) = seed.row_id.clone() {
                worked.insert(row_id);
            }
            let TurnSeed {
                message,
                system_reminder,
                display_role,
                ..
            } = seed;
            let start_message_index = agent.message_count();
            let result = if let Some(display_role) = display_role {
                agent
                    .run_once_streaming_mpsc_with_display_role(
                        &message,
                        vec![],
                        system_reminder,
                        event_tx.clone(),
                        Some(display_role),
                    )
                    .await
            } else {
                process_locked_message_streaming_mpsc(
                    &mut agent,
                    &message,
                    vec![],
                    system_reminder,
                    event_tx.clone(),
                )
                .await
            };
            let completion_report = result
                .is_ok()
                .then(|| agent.latest_assistant_text_after(start_message_index))
                .flatten();
            match result {
                Ok(()) => {
                    update_member_status_with_report(
                        &session_id,
                        SwarmLifecycleStatus::Ready,
                        None,
                        completion_report,
                        &swarm.members,
                        &swarm.swarms_by_id,
                        Some(&swarm.event_history),
                        Some(&swarm.event_counter),
                        Some(&swarm.event_tx),
                    )
                    .await;
                    let _ = event_tx.send(ServerEvent::Done { id: 0 });
                }
                Err(error) => {
                    crate::logging::error(&format!(
                        "Server-initiated turn failed for live session {}: {}",
                        session_id, error
                    ));
                    update_member_status(
                        &session_id,
                        SwarmLifecycleStatus::Failed,
                        Some(truncate_detail(&error.to_string(), 120)),
                        &swarm.members,
                        &swarm.swarms_by_id,
                        Some(&swarm.event_history),
                        Some(&swarm.event_counter),
                        Some(&swarm.event_tx),
                    )
                    .await;
                    let _ = event_tx.send(ServerEvent::Error {
                        id: 0,
                        message: crate::util::format_error_chain(&error),
                        retry_after_secs: None,
                    });
                    break;
                }
            }
            if !may_continue_on_its_own(&session_id, &swarm).await {
                break;
            }
            next = next_row_turn(&agent, &session_id, &worked);
        }
        drop(agent);
    });
}

/// Run the turn `seed` describes immediately if the session is live and idle.
/// Returns `true` when the turn was started.
pub(super) async fn run_live_turn_if_idle(
    session_id: &str,
    seed: TurnSeed,
    sessions: &SessionAgents,
    swarm: LiveTurnSwarmContext,
) -> bool {
    let Some(agent) = idle_live_agent(session_id, sessions, &swarm.members).await else {
        return false;
    };
    spawn_tracked_live_turn(session_id, agent, seed, swarm).await;
    true
}

pub(super) async fn run_live_system_turn_if_idle(
    session_id: &str,
    message: &str,
    sessions: &SessionAgents,
    swarm: LiveTurnSwarmContext,
) -> bool {
    let Some(agent) = idle_live_agent(session_id, sessions, &swarm.members).await else {
        return false;
    };
    let seed = TurnSeed::asked(
        message,
        None,
        Some(crate::session::StoredDisplayRole::System),
    );
    spawn_tracked_live_turn(session_id, agent, seed, swarm).await;
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: &str, holder: Option<&str>, blocked: &[&str]) -> TaskItem {
        TaskItem {
            id: id.to_string(),
            content: format!("row {id}"),
            assigned_to: holder.map(str::to_string),
            blocked_by: blocked.iter().map(|b| b.to_string()).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn takes_the_first_held_row_whose_blockers_are_gone() {
        let rows = vec![
            row("t1", None, &[]),
            row("t2", Some("me"), &["t1"]),
            row("t3", Some("me"), &[]),
            row("t4", Some("other"), &[]),
        ];
        // t2 is held but still blocked by an open t1, and t4 is somebody else's.
        let picked = next_held_ready_row(&rows, "me", &HashSet::new()).expect("a held ready row");
        assert_eq!(picked.id, "t3");
    }

    #[test]
    fn a_closed_blocker_is_gone_and_its_row_becomes_ready() {
        let rows = vec![row("t2", Some("me"), &["t1"])];
        assert_eq!(
            next_held_ready_row(&rows, "me", &HashSet::new()).map(|row| row.id.as_str()),
            Some("t2")
        );
    }

    #[test]
    fn holding_nothing_ready_continues_nothing() {
        let rows = vec![row("t1", None, &[]), row("t2", Some("me"), &["t1"])];
        assert!(next_held_ready_row(&rows, "me", &HashSet::new()).is_none());
    }

    /// Only a close removes a row, so without this the loop picks the same row
    /// again on every turn until something else stops it.
    #[test]
    fn a_row_the_run_already_worked_ends_it() {
        let rows = vec![row("t3", Some("me"), &[])];
        let worked: HashSet<String> = ["t3".to_string()].into_iter().collect();
        assert!(next_held_ready_row(&rows, "me", &worked).is_none());
    }

    #[test]
    fn a_worked_row_steps_aside_for_the_next_one() {
        let rows = vec![row("t3", Some("me"), &[]), row("t4", Some("me"), &[])];
        let worked: HashSet<String> = ["t3".to_string()].into_iter().collect();
        let picked = next_held_ready_row(&rows, "me", &worked).expect("the row after t3");
        assert_eq!(picked.id, "t4");
    }

    #[test]
    fn the_message_carries_the_rows_own_words() {
        let mut held = row("t7", Some("me"), &[]);
        held.note = Some("paths first".to_string());
        let message = row_turn_message(&held);
        assert!(message.contains("t7"));
        assert!(message.contains("row t7"));
        assert!(message.contains("paths first"));
    }
}
