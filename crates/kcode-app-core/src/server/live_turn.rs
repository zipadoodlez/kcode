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
    RunState, SessionAgents, SwarmEvent, SwarmMember, session_event_fanout_sender, truncate_detail,
    update_member_status, update_member_status_with_report,
};
use crate::agent::Agent;
use crate::protocol::ServerEvent;
use crate::protocol::SwarmLifecycleStatus;
use crate::todo::{TaskItem, load_tasks};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use tokio::sync::{OwnedMutexGuard, RwLock, broadcast};

/// Swarm bookkeeping handles needed to keep member status accurate around a
/// server-initiated turn.
#[derive(Clone)]
pub(super) struct LiveTurnSwarmContext {
    pub members: Arc<RwLock<HashMap<String, SwarmMember>>>,
    pub event_history: Arc<RwLock<VecDeque<SwarmEvent>>>,
    pub event_counter: Arc<AtomicU64>,
    pub event_tx: broadcast::Sender<SwarmEvent>,
}

impl LiveTurnSwarmContext {
    pub(super) fn new(
        members: &Arc<RwLock<HashMap<String, SwarmMember>>>,
        event_history: &Arc<RwLock<VecDeque<SwarmEvent>>>,
        event_counter: &Arc<AtomicU64>,
        event_tx: &broadcast::Sender<SwarmEvent>,
    ) -> Self {
        Self {
            members: Arc::clone(members),
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

/// What a granted turn carries into its run: whether the run may take another turn
/// when this one ends, and the row whose subtree it works.
///
/// Both are run properties, stored nowhere and read where the next turn is decided
/// (rule 11). `scope` is the run's anchor: `todo::anchor_run` resolves it from the
/// grant's words, and `None` is a run scoped to everything the session holds, which
/// is where an untyped grant starts.
#[derive(Clone, Debug)]
pub(super) struct RunGrant {
    pub may_continue: bool,
    pub scope: Option<String>,
}

impl RunGrant {
    /// No permission: the turn ends where it ends (rule 11's default).
    pub(super) fn denied() -> Self {
        Self {
            may_continue: false,
            scope: None,
        }
    }

    /// Permission with no typed scope: the run works whatever it holds.
    pub(super) fn whole_list() -> Self {
        Self {
            may_continue: true,
            scope: None,
        }
    }

    /// Permission scoped to one row's subtree: the grant's own anchor.
    pub(super) fn scoped(anchor: String) -> Self {
        Self {
            may_continue: true,
            scope: Some(anchor),
        }
    }
}

/// The grant a message carries, resolved before its turn starts: the permission,
/// and the scope its words name or become (`todo::anchor_from_words`).
///
/// A write that fails costs the scope and never the turn, so the run is warned
/// about and left unscoped rather than silently losing the permission it was given.
pub(super) fn resolve_grant(
    words: &str,
    working_dir: Option<&Path>,
    session_id: &str,
) -> (RunGrant, String) {
    // A fresh anchor is the run's own row: its result is the rows under it
    // integrated, so the engine's word for it is `synthesize`, and it needs one at
    // all because rule 8 forbids guessing a kind for a row that has none.
    let fresh_kind = kcode_plan::bridge::kind_str(kcode_plan::bridge::NodeKind::Synthesize);
    match crate::todo::anchor_from_words(working_dir, session_id, words, Some(fresh_kind)) {
        Ok(anchor) => (RunGrant::scoped(anchor.id), anchor.content),
        Err(error) => {
            crate::logging::warn(&format!(
                "a granted turn could not resolve its scope ({error}); \
                 the run works everything the session holds"
            ));
            (RunGrant::whole_list(), words.to_string())
        }
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

    let attended = {
        let members = swarm_members.read().await;
        members
            .get(session_id)
            .is_some_and(SwarmMember::is_attended)
    };
    if !attended {
        return None;
    }

    agent.try_lock_owned().ok()
}

/// A live session's working directory, whether or not its agent is mid-turn: the
/// agent's own when it is free, and the member's record otherwise, which is the
/// same session root (`resolve_target_subscribe_working_dir` reads it the same
/// way). Never waits on the agent: a caller on a client's read loop must not stall
/// behind a run, or that client could not even cancel it.
pub(super) async fn session_working_dir(
    session_id: &str,
    sessions: &SessionAgents,
    members: &Arc<RwLock<HashMap<String, SwarmMember>>>,
) -> Option<PathBuf> {
    let agent = {
        let guard = sessions.read().await;
        guard.get(session_id).cloned()
    }?;
    if let Some(dir) = agent
        .try_lock()
        .ok()
        .and_then(|agent| agent.working_dir().map(PathBuf::from))
    {
        return Some(dir);
    }
    members
        .read()
        .await
        .get(session_id)
        .and_then(|member| member.working_dir.clone())
}

/// The next row this session holds and may work: file order, in this run's scope,
/// held by it, with no blocker and not already worked in this run.
///
/// Readiness is a join against the file rather than something a row says about
/// itself: every `blocked_by` entry is an open row that must close first, so an
/// empty list is exactly "unblocked". The close removes its id from every
/// dependent, which is what empties it, and a hand-written id that names nothing
/// blocks the row, as the plan engine's `missing_dependencies` says too. `worked`
/// is the run's own record, so a row left open stops the run instead of being
/// picked again and again.
///
/// A row other rows belong to waits for them as well: its result is theirs
/// integrated (rule 3), and a close is refused while a child is open, so picking
/// it first would burn its one turn in the run and leave the run no turn to close
/// it with. This is what makes a run's anchor, which every root-level row of the
/// run belongs to, the last row the run closes.
///
/// The scope row is picked last: it is a run's anchor, its close is the run's end,
/// and it stays open while a child names it (rule 3), so working it first would
/// only ask the model to close a row that cannot close yet.
fn next_held_ready_row<'a>(
    rows: &'a [TaskItem],
    session_id: &str,
    worked: &HashSet<String>,
    scope: Option<&str>,
) -> Option<&'a TaskItem> {
    let ready = |row: &TaskItem| {
        row.assigned_to.as_deref() == Some(session_id)
            && !worked.contains(&row.id)
            && row_is_ready(rows, row)
            && scope.is_none_or(|anchor| descends_from(rows, &row.id, anchor))
    };
    rows.iter()
        .find(|row| ready(row) && Some(row.id.as_str()) != scope)
        .or_else(|| rows.iter().find(|row| ready(row)))
}

/// Whether this row is ready: nothing blocks it, and no open row belongs to it.
///
/// Readiness is a join against the file, never a field. An empty `blocked_by` means
/// every id it named was closed, because a close removes its id from every dependent;
/// a row other rows belong to waits for them, because its result is theirs (rule 3).
fn row_is_ready(rows: &[TaskItem], row: &TaskItem) -> bool {
    row.blocked_by.is_empty()
        && !rows
            .iter()
            .any(|child| child.parent.as_deref() == Some(row.id.as_str()))
}

/// Whether `id` is `anchor` or descends from it, by walking `parent`. A parent
/// that names no open row, or a hand-written cycle, ends the walk: neither can be
/// a descendant of the anchor.
fn descends_from(rows: &[TaskItem], id: &str, anchor: &str) -> bool {
    let mut current = Some(id);
    for _ in 0..=rows.len() {
        let Some(candidate) = current else {
            return false;
        };
        if candidate == anchor {
            return true;
        }
        current = rows
            .iter()
            .find(|row| row.id == candidate)
            .and_then(|row| row.parent.as_deref());
    }
    false
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
fn next_row_turn(
    working_dir: Option<&Path>,
    session_id: &str,
    worked: &HashSet<String>,
    scope: Option<&str>,
) -> Option<TurnSeed> {
    let rows = load_tasks(working_dir, session_id).ok()?;
    next_held_ready_row(&rows, session_id, worked, scope).map(TurnSeed::row)
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
    grant: RunGrant,
) -> bool {
    let working_dir = session_working_dir(session_id, sessions, &swarm.members).await;
    let Ok(rows) = load_tasks(working_dir.as_deref(), session_id) else {
        return false;
    };
    // A run that typed no scope has no row at its top yet: the first row it holds
    // that belongs to nothing becomes it, and the rest of its rows are changed to
    // belong to it, so the run's records and its end-of-run result have a home
    // (`todo::anchor_from_rows`). A scoped run already has one, the row its grant
    // named or became, and a run whose rows all belong to something needs none.
    if grant.scope.is_none()
        && let Err(error) = crate::todo::anchor_from_rows(working_dir.as_deref(), session_id)
    {
        crate::logging::warn(&format!(
            "run {session_id} could not make its anchor ({error}); \
             its records live in the commits alone"
        ));
    }
    let Some(row) = next_held_ready_row(&rows, session_id, &HashSet::new(), grant.scope.as_deref())
    else {
        return false;
    };
    let seed = TurnSeed::row(row);
    run_live_turn_if_idle(session_id, seed, grant, sessions, swarm).await
}

/// Wake the holder of every row this write made ready.
///
/// The write is the only thing that starts work. The verbs that used to poke a worker
/// are gone, so a row that becomes ready wakes the session that owes it, and the turn
/// that follows carries the row's own words (`next_row_turn`). Only a headless member
/// is woken: a session with a human owns its own turns (rule 11). A row the run's own
/// map already carries a status for is work it took, so a row left open does not
/// restart the run that left it.
///
/// A ready row *nobody* holds wakes nobody: the pick takes only what the session
/// holds, so the model's other half ("held by me or nobody") is the loop's dispatch,
/// and it lands with the verbs that dispatch today.
/// # braid: unheld rows wake nobody, surpassed by the pick taking "mine or nobody".
///
/// Returns how many sessions were woken.
pub(super) async fn wake_ready_owners(
    rows: &[TaskItem],
    sessions: &SessionAgents,
    swarm: LiveTurnSwarmContext,
    runs: &Arc<RwLock<HashMap<String, RunState>>>,
) -> usize {
    let mut owners: Vec<String> = {
        let members = swarm.members.read().await;
        let runs = runs.read().await;
        rows.iter()
            .filter(|row| row_is_ready(rows, row))
            .filter_map(|row| {
                row.assigned_to
                    .as_deref()
                    .map(|owner| (owner, row.id.as_str()))
            })
            .filter(|(owner, _)| members.get(*owner).is_some_and(|member| member.is_headless))
            .filter(
                |(owner, row_id)| match super::swarm::swarm_root(&members, owner) {
                    Some(root) => runs
                        .get(&root)
                        .and_then(|run| run.get(*row_id))
                        .is_none_or(|state| state.status.is_empty()),
                    None => true,
                },
            )
            .map(|(owner, _)| owner.to_string())
            .collect()
    };
    owners.sort();
    owners.dedup();
    let mut woken = 0;
    for owner in owners {
        // A headless member holds the continuation permission inherently: it was
        // spawned with no human, so the run supplies every turn (rule 11).
        if continue_with_next_row(&owner, sessions, swarm.clone(), RunGrant::whole_list()).await {
            woken += 1;
        }
    }
    woken
}

/// Spawn `seed` as a full tracked turn in a live session.
///
/// Mirrors the client-initiated turn lifecycle: the swarm member is marked
/// `running` before the turn starts and `ready` (with a completion report) or
/// `failed` when it finishes. A synthetic terminal `Done { id: 0 }` (or
/// `Error { id: 0, .. }`) is fanned out to attached clients so their UI can
/// finish rendering the externally started turn. When the session holds a ready
/// row it may keep going on its own, the turn is one of a run: the next row is
/// taken after this one, until none is left.
pub(super) async fn spawn_tracked_live_turn(
    session_id: &str,
    sessions: &SessionAgents,
    agent: OwnedMutexGuard<Agent>,
    seed: TurnSeed,
    grant: RunGrant,
    swarm: LiveTurnSwarmContext,
) {
    update_member_status(
        session_id,
        SwarmLifecycleStatus::Running,
        seed.detail.clone(),
        &swarm.members,
        Some(&swarm.event_history),
        Some(&swarm.event_counter),
        Some(&swarm.event_tx),
    )
    .await;

    let event_tx = session_event_fanout_sender(session_id.to_string(), Arc::clone(&swarm.members));
    let sessions = Arc::clone(sessions);
    let session_id = session_id.to_string();
    tokio::spawn(async move {
        // A session's own turns are the loop: after each one, take the next row it
        // holds while it may keep going on its own.
        //
        // The reservation is given up between turns, before the next row is even
        // looked for, on purpose: releasing it hands the permit to a turn that is
        // already waiting for it, and the file read in between yields, so a person
        // typing takes the agent and this run ends instead of their message
        // queueing behind it. Each turn's terminal status is published before the
        // release, so a later turn's `running` cannot be overwritten by this one's
        // `ready`.
        //
        // `worked` is the loop's stop: a row worked once in this run is not picked
        // again, so a row left open ends the run instead of spinning it.
        let mut worked: HashSet<String> = HashSet::new();
        // The working directory is fixed for the run, so the pick does not need the
        // agent, which it cannot hold between turns.
        let working_dir = agent.working_dir().map(PathBuf::from);
        let mut held = Some(agent);
        let mut seed = Some(seed);
        while let (Some(agent), Some(next)) = (held.take(), seed.take()) {
            let mut agent = agent;
            if let Some(row_id) = next.row_id.clone() {
                worked.insert(row_id);
            }
            let TurnSeed {
                message,
                system_reminder,
                display_role,
                ..
            } = next;
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
            // The permission is the run's, decided where the run started, so a
            // turn that was not granted ends after its one turn.
            if !grant.may_continue {
                break;
            }
            // Give the agent up before looking for more work: if a person or another
            // turn wants it, they get it and this run ends here.
            drop(agent);
            let Some(next_seed) = next_row_turn(
                working_dir.as_deref(),
                &session_id,
                &worked,
                grant.scope.as_deref(),
            ) else {
                break;
            };
            seed = Some(next_seed);
            held = idle_live_agent(&session_id, &sessions, &swarm.members).await;
        }
    });
}

/// Run the turn `seed` describes immediately if the session is live and idle.
/// Returns `true` when the turn was started.
pub(super) async fn run_live_turn_if_idle(
    session_id: &str,
    seed: TurnSeed,
    grant: RunGrant,
    sessions: &SessionAgents,
    swarm: LiveTurnSwarmContext,
) -> bool {
    let Some(agent) = idle_live_agent(session_id, sessions, &swarm.members).await else {
        return false;
    };
    spawn_tracked_live_turn(session_id, sessions, agent, seed, grant, swarm).await;
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
    spawn_tracked_live_turn(session_id, sessions, agent, seed, RunGrant::denied(), swarm).await;
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

    fn child(id: &str, holder: &str, parent: &str) -> TaskItem {
        TaskItem {
            parent: Some(parent.to_string()),
            ..row(id, Some(holder), &[])
        }
    }

    /// A repo of its own, so the list is this test's and never the machine's.
    fn scratch_repo() -> tempfile::TempDir {
        let repo = tempfile::tempdir().expect("temp dir");
        assert!(
            std::process::Command::new("git")
                .args(["init", "-q"])
                .current_dir(repo.path())
                .status()
                .expect("git init")
                .success(),
            "git init"
        );
        repo
    }

    #[test]
    fn readiness_is_the_blockers_and_the_open_children() {
        let mut rows = vec![
            row("t1", Some("me"), &["t2"]),
            row("t2", None, &[]),
            child("t3", "me", "t4"),
            row("t4", Some("me"), &[]),
        ];
        assert!(!row_is_ready(&rows, &rows[0]), "a blocker keeps it unready");
        assert!(
            row_is_ready(&rows, &rows[1]),
            "a row with no blocker is ready"
        );
        assert!(row_is_ready(&rows, &rows[2]), "a held leaf is ready");
        assert!(
            !row_is_ready(&rows, &rows[3]),
            "an open child keeps it unready"
        );
        rows[0].blocked_by.clear();
        assert!(
            row_is_ready(&rows, &rows[0]),
            "the close empties `blocked_by`, which is what makes it ready"
        );
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
        let picked =
            next_held_ready_row(&rows, "me", &HashSet::new(), None).expect("a held ready row");
        assert_eq!(picked.id, "t3");
    }

    /// A blocker id that names no open row blocks too. A close removes its id from
    /// dependents instead, so a dangling one is a hand-written typo, and the plan
    /// engine's `missing_dependencies` treats it the same way.
    #[test]
    fn a_dangling_blocker_keeps_the_row_unready() {
        let rows = vec![row("t2", Some("me"), &["t1"])];
        assert!(next_held_ready_row(&rows, "me", &HashSet::new(), None).is_none());
    }

    #[test]
    fn holding_nothing_ready_continues_nothing() {
        let rows = vec![row("t1", None, &[]), row("t2", Some("me"), &["t1"])];
        assert!(next_held_ready_row(&rows, "me", &HashSet::new(), None).is_none());
    }

    /// Only a close removes a row, so without this the loop picks the same row
    /// again on every turn until something else stops it.
    #[test]
    fn a_row_the_run_already_worked_ends_it() {
        let rows = vec![row("t3", Some("me"), &[])];
        let worked: HashSet<String> = ["t3".to_string()].into_iter().collect();
        assert!(next_held_ready_row(&rows, "me", &worked, None).is_none());
    }

    #[test]
    fn a_worked_row_steps_aside_for_the_next_one() {
        let rows = vec![row("t3", Some("me"), &[]), row("t4", Some("me"), &[])];
        let worked: HashSet<String> = ["t3".to_string()].into_iter().collect();
        let picked = next_held_ready_row(&rows, "me", &worked, None).expect("the row after t3");
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

    /// A run's scope is its anchor's subtree: a row the session holds but that
    /// descends from no anchor row is not this run's work, which is what makes
    /// `/auto <row>` a scope rather than a starting point.
    #[test]
    fn a_held_row_outside_the_anchor_subtree_is_not_picked() {
        let rows = vec![
            row("t1", Some("me"), &[]),
            child("t2", "me", "t1"),
            row("t3", Some("me"), &[]),
        ];
        let picked = next_held_ready_row(&rows, "me", &HashSet::new(), Some("t1"));
        assert_eq!(picked.map(|row| row.id.as_str()), Some("t2"));
    }

    /// A row other rows belong to waits for them: its result is theirs integrated,
    /// and a close is refused while a child is open (rule 3). Without this the run
    /// would spend its one turn on it, could not close it, and never come back, so a
    /// run's anchor would never write the run's end-of-run result.
    #[test]
    fn a_row_other_rows_belong_to_waits_for_them() {
        let rows = vec![row("t1", Some("me"), &[]), child("t2", "me", "t1")];
        let picked = next_held_ready_row(&rows, "me", &HashSet::new(), Some("t1"));
        assert_eq!(picked.map(|row| row.id.as_str()), Some("t2"));

        // Taking a turn on the child is not closing it: the parent waits for the
        // close, which is what a run's anchor needs.
        let worked: HashSet<String> = ["t2".to_string()].into_iter().collect();
        assert!(
            next_held_ready_row(&rows, "me", &worked, Some("t1")).is_none(),
            "an open child keeps its parent out of the run"
        );

        let closed = vec![rows[0].clone()];
        let picked = next_held_ready_row(&closed, "me", &worked, Some("t1"));
        assert_eq!(picked.map(|row| row.id.as_str()), Some("t1"));
    }

    /// The grant's words are both the scope and the run's first turn, and a named
    /// row is claimed: a run only works what it holds, so an unclaimed anchor would
    /// leave the session scoped to a row it could never pick.
    #[test]
    fn a_grant_scopes_the_run_to_the_row_its_words_name() {
        let repo = scratch_repo();
        let mut named = row("t1", Some("someone-else"), &[]);
        named.content = "fix the docs".to_string();
        crate::todo::save_tasks(Some(repo.path()), "me", &[named]).expect("write the work list");

        let (grant, words) = resolve_grant("fix the docs", Some(repo.path()), "me");
        assert_eq!(grant.scope.as_deref(), Some("t1"));
        assert!(grant.may_continue, "the grant is this turn's permission");
        assert_eq!(
            words, "fix the docs",
            "the row's words are the run's first turn"
        );
        assert_eq!(
            crate::todo::load_tasks(Some(repo.path()), "me").expect("read")[0]
                .assigned_to
                .as_deref(),
            Some("me"),
            "the grant claims the row the words name"
        );

        let (grant, words) =
            resolve_grant("work the list until it is done", Some(repo.path()), "me");
        assert_eq!(
            grant.scope.as_deref(),
            Some("t2"),
            "words that name no row become the run's anchor"
        );
        assert_eq!(words, "work the list until it is done");
    }
}
