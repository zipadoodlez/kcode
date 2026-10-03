//! The two reads a request still needs, with the views they were written for gone:
//! `handle_comm_read_context` serves the await path's transcript read until S1e
//! deletes it, and `member_runtime_extras` enriches the shared-context view until
//! S1f deletes that. Everything else in this file was a read view of the run.

use super::swarm::{swarm_is_root, swarm_root};
use super::{ClientConnectionInfo, SessionAgents, SwarmMember};
use crate::protocol::{ServerEvent, SessionActivitySnapshot};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{RwLock, mpsc};

fn live_activity_snapshot(
    connections: &HashMap<String, ClientConnectionInfo>,
    session_id: &str,
    fallback_processing: bool,
) -> Option<SessionActivitySnapshot> {
    let mut processing_without_tool = false;
    let mut tool_name = None;
    for info in connections.values() {
        if info.session_id != session_id || !info.is_processing {
            continue;
        }
        if let Some(current_tool_name) = info.current_tool_name.clone() {
            tool_name = Some(current_tool_name);
            break;
        }
        processing_without_tool = true;
    }

    tool_name
        .map(|current_tool_name| SessionActivitySnapshot {
            is_processing: true,
            current_tool_name: Some(current_tool_name),
        })
        .or_else(|| {
            processing_without_tool.then_some(SessionActivitySnapshot {
                is_processing: true,
                current_tool_name: None,
            })
        })
        .or_else(|| {
            fallback_processing.then_some(SessionActivitySnapshot {
                is_processing: true,
                current_tool_name: None,
            })
        })
}

/// Recent-token lookback window used when reporting per-agent churn in
/// `swarm list`. Short enough to reflect "what is this agent doing right now".
pub(super) const SWARM_LIST_TOKEN_WINDOW_SECS: u64 = 10;

/// Runtime extras for a swarm member, gathered without holding the agent lock
/// for long. Used to enrich the `swarm list` roster with live activity,
/// provider/model, token churn, turn count, and todo progress.
#[derive(Default)]
pub(super) struct MemberRuntimeExtras {
    pub(super) activity: Option<SessionActivitySnapshot>,
    pub(super) provider_name: Option<String>,
    pub(super) provider_model: Option<String>,
    pub(super) provider_effort: Option<String>,
    pub(super) turn_count: Option<u64>,
    pub(super) recent_total_tokens: Option<u64>,
    pub(super) recent_output_tokens: Option<u64>,
    pub(super) recent_window_secs: Option<u64>,
    pub(super) cumulative_total_tokens: Option<u64>,
    pub(super) last_activity_age_secs: Option<u64>,
    pub(super) todos_completed: Option<usize>,
    pub(super) todos_total: Option<usize>,
}

/// Gather live runtime extras for a single member session.
///
/// `member_is_running` is used as a fallback "processing" hint when no live
/// client connection is reporting activity (e.g. headless sessions).
pub(super) async fn member_runtime_extras(
    session_id: &str,
    member_is_running: bool,
    sessions: &SessionAgents,
    client_connections: &Arc<RwLock<HashMap<String, ClientConnectionInfo>>>,
) -> MemberRuntimeExtras {
    let activity = {
        let connections = client_connections.read().await;
        live_activity_snapshot(&connections, session_id, member_is_running)
    };

    let (provider_name, provider_model, provider_effort) = {
        let agent_sessions = sessions.read().await;
        if let Some(agent) = agent_sessions.get(session_id) {
            // Never block on a busy agent: token churn and turns come from the
            // lock-free metrics registry, so a missing provider name here just
            // means the agent is mid-turn.
            if let Ok(agent) = agent.try_lock() {
                (
                    Some(agent.provider_name()),
                    Some(agent.provider_model()),
                    agent.provider_reasoning_effort(),
                )
            } else {
                (None, None, None)
            }
        } else {
            (None, None, None)
        }
    };

    let metrics = crate::session_metrics::snapshot(
        session_id,
        std::time::Duration::from_secs(SWARM_LIST_TOKEN_WINDOW_SECS),
    );

    // The list is per repo now, so a member's progress comes from the repo the
    // member works in. `Agent::working_dir` is where the tool already got it.
    let working_dir = {
        let agent_sessions = sessions.read().await;
        agent_sessions
            .get(session_id)
            .and_then(|agent| agent.try_lock().ok())
            .and_then(|agent| agent.working_dir().map(std::path::PathBuf::from))
    };

    let (todos_completed, todos_total) =
        match crate::todo::load_tasks(working_dir.as_deref(), session_id) {
            Ok(todos) if !todos.is_empty() => {
                let completed = todos.iter().filter(|t| t.status == "completed").count();
                (Some(completed), Some(todos.len()))
            }
            _ => (None, None),
        };

    MemberRuntimeExtras {
        activity,
        provider_name,
        provider_model,
        provider_effort,
        turn_count: metrics.map(|m| m.turns),
        recent_total_tokens: metrics.map(|m| m.recent_total_tokens),
        recent_output_tokens: metrics.map(|m| m.recent_output_tokens),
        recent_window_secs: metrics.map(|_| SWARM_LIST_TOKEN_WINDOW_SECS),
        cumulative_total_tokens: metrics.map(|m| m.cumulative_total_tokens),
        last_activity_age_secs: metrics.and_then(|m| m.last_activity_age_secs),
        todos_completed,
        todos_total,
    }
}

async fn ensure_same_swarm_access(
    id: u64,
    req_session_id: &str,
    target_session: &str,
    swarm_members: &Arc<RwLock<HashMap<String, SwarmMember>>>,
    client_event_tx: &mpsc::UnboundedSender<ServerEvent>,
) -> bool {
    let (req_swarm, target_swarm) = {
        let members = swarm_members.read().await;
        (
            swarm_root(&members, req_session_id),
            swarm_root(&members, target_session),
        )
    };

    if req_swarm.is_some() && req_swarm == target_swarm {
        true
    } else {
        let _ = client_event_tx.send(ServerEvent::Error {
            id,
            message: format!(
                "Session '{}' is not in the same swarm as requester '{}'",
                target_session, req_session_id
            ),
            retry_after_secs: None,
        });
        false
    }
}

async fn can_read_full_context(
    req_session_id: &str,
    target_session: &str,
    swarm_members: &Arc<RwLock<HashMap<String, SwarmMember>>>,
) -> bool {
    if req_session_id == target_session {
        return true;
    }

    let members = swarm_members.read().await;
    swarm_is_root(&members, req_session_id)
}

pub(super) async fn handle_comm_read_context(
    id: u64,
    req_session_id: String,
    target_session: String,
    sessions: &SessionAgents,
    swarm_members: &Arc<RwLock<HashMap<String, SwarmMember>>>,
    client_event_tx: &mpsc::UnboundedSender<ServerEvent>,
) {
    if !ensure_same_swarm_access(
        id,
        &req_session_id,
        &target_session,
        swarm_members,
        client_event_tx,
    )
    .await
    {
        return;
    }

    if !can_read_full_context(&req_session_id, &target_session, swarm_members).await {
        let _ = client_event_tx.send(ServerEvent::Error {
            id,
            message: "Only the coordinator, worktree manager, or the target session may read full context.".to_string(),
            retry_after_secs: None,
        });
        return;
    }

    let agent_sessions = sessions.read().await;
    if let Some(agent) = agent_sessions.get(&target_session) {
        let messages = if let Ok(agent) = agent.try_lock() {
            agent.get_history()
        } else {
            let _ = client_event_tx.send(ServerEvent::Error {
                id,
                message: format!(
                    "Session '{}' is busy; try read_context again shortly",
                    target_session
                ),
                retry_after_secs: Some(1),
            });
            return;
        };
        let _ = client_event_tx.send(ServerEvent::CommContextHistory {
            id,
            session_id: target_session,
            messages,
        });
    } else {
        let _ = client_event_tx.send(ServerEvent::Error {
            id,
            message: format!("Unknown session '{target_session}'"),
            retry_after_secs: None,
        });
    }
}
