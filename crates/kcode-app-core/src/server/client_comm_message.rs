use super::live_turn::{LiveTurnSwarmContext, RunGrant, TurnSeed, run_live_turn_if_idle};
use super::{
    SessionAgents, SessionInterruptQueues, SwarmEvent, SwarmEventType, SwarmMember,
    fanout_session_event, queue_soft_interrupt_for_session, record_swarm_event, truncate_detail,
    util::{member_friendly_name, member_swarm_id},
};
use crate::protocol::{NotificationType, ServerEvent};
use kcode_agent_runtime::SoftInterruptSource;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{RwLock, broadcast, mpsc};

/// Resolve a message address to a swarm member's session id: an exact session
/// id, or a friendly name that is unique within the run.
async fn resolve_target_session(
    target: &str,
    swarm_session_ids: &[String],
    swarm_members: &Arc<RwLock<HashMap<String, SwarmMember>>>,
) -> anyhow::Result<String> {
    if swarm_session_ids
        .iter()
        .any(|session_id| session_id == target)
    {
        return Ok(target.to_string());
    }

    let members = swarm_members.read().await;
    let mut matches: Vec<(String, String)> = swarm_session_ids
        .iter()
        .filter_map(|session_id| {
            let member = members.get(session_id)?;
            member
                .friendly_name
                .as_deref()
                .filter(|friendly_name| *friendly_name == target)
                .map(|friendly_name| (session_id.clone(), friendly_name.to_string()))
        })
        .collect();
    matches.sort_by(|(left_session, _), (right_session, _)| left_session.cmp(right_session));
    matches.dedup_by(|(left_session, _), (right_session, _)| left_session == right_session);
    match matches.len() {
        1 => Ok(matches.remove(0).0),
        0 => Err(anyhow::anyhow!(
            "Unknown target '{}' - use an exact session_id or a unique friendly name within the swarm.",
            target
        )),
        _ => {
            let match_list = matches
                .iter()
                .map(|(session_id, friendly_name)| format!("{} [{}]", friendly_name, session_id))
                .collect::<Vec<_>>()
                .join(", ");
            Err(anyhow::anyhow!(
                "Friendly name '{}' is ambiguous in swarm. Use an exact session id instead. Matches: {}",
                target,
                match_list
            ))
        }
    }
}

/// Hand a message to one session: a notification the client renders, and, when
/// the holder is headless and idle, the turn that works it. This is the run's
/// only way to hand work over, so the address is a session, never a group.
#[expect(
    clippy::too_many_arguments,
    reason = "the message path joins the swarm, the turn runner and the event log"
)]
pub(super) async fn handle_comm_message(
    id: u64,
    from_session: String,
    message: String,
    to_session: String,
    wake: Option<bool>,
    client_event_tx: &mpsc::UnboundedSender<ServerEvent>,
    sessions: &SessionAgents,
    soft_interrupt_queues: &SessionInterruptQueues,
    swarm_members: &Arc<RwLock<HashMap<String, SwarmMember>>>,
    event_history: &Arc<RwLock<std::collections::VecDeque<SwarmEvent>>>,
    event_counter: &Arc<std::sync::atomic::AtomicU64>,
    swarm_event_tx: &broadcast::Sender<SwarmEvent>,
) {
    let started = std::time::Instant::now();
    let Some(swarm_id) = member_swarm_id(&from_session, swarm_members).await else {
        let _ = client_event_tx.send(ServerEvent::Error {
            id,
            message: "Not in a swarm. Use a git repository to enable swarm features.".to_string(),
            retry_after_secs: None,
        });
        return;
    };

    let friendly_name = member_friendly_name(&from_session, swarm_members).await;
    let swarm_session_ids: Vec<String> =
        super::swarm::swarm_session_ids(&swarm_id, swarm_members).await;

    let target = match resolve_target_session(&to_session, &swarm_session_ids, swarm_members).await
    {
        Ok(session_id) => session_id,
        Err(message) => {
            let _ = client_event_tx.send(ServerEvent::Error {
                id,
                message: message.to_string(),
                retry_after_secs: None,
            });
            return;
        }
    };

    if !swarm_session_ids.contains(&target) {
        let _ = client_event_tx.send(ServerEvent::Error {
            id,
            message: format!("Message failed: session '{}' not in swarm", target),
            retry_after_secs: None,
        });
        return;
    }

    let from_label = friendly_name
        .clone()
        .unwrap_or_else(|| from_session[..8.min(from_session.len())].to_string());
    let notification_msg = format!("Message from {}: {}", from_label, message);
    let _ = fanout_session_event(
        swarm_members,
        &target,
        ServerEvent::Notification {
            from_session: from_session.clone(),
            from_name: friendly_name.clone(),
            notification_type: NotificationType::Message {
                scope: Some("dm".to_string()),
            },
            message: notification_msg.clone(),
        },
    )
    .await;

    if wake.unwrap_or(true) {
        let sender_name = friendly_name
            .clone()
            .unwrap_or_else(|| from_session.clone());
        let reminder = Some(format!(
            "You just received a direct swarm message from {}. Review it and respond or act if useful.",
            sender_name
        ));
        if crate::config::config().server.wake_mode == crate::config::WakeMode::External {
            let _ = fanout_session_event(
                swarm_members,
                &target,
                ServerEvent::WakeRequested {
                    session_id: target.clone(),
                    reason: "communication_delivery".to_string(),
                    notification: notification_msg.clone(),
                },
            )
            .await;
        } else {
            let woke_immediately = run_live_turn_if_idle(
                &target,
                TurnSeed::asked(&notification_msg, reminder, None),
                RunGrant::denied(),
                sessions,
                LiveTurnSwarmContext::new(
                    swarm_members,
                    event_history,
                    event_counter,
                    swarm_event_tx,
                ),
            )
            .await;
            if !woke_immediately {
                let _ = queue_soft_interrupt_for_session(
                    &target,
                    notification_msg.clone(),
                    false,
                    SoftInterruptSource::System,
                    soft_interrupt_queues,
                    sessions,
                )
                .await;
            }
        }
    }

    record_swarm_event(
        event_history,
        event_counter,
        swarm_event_tx,
        from_session.clone(),
        friendly_name.clone(),
        Some(swarm_id.clone()),
        SwarmEventType::Notification {
            notification_type: "dm".to_string(),
            message: truncate_detail(&message, 220),
        },
    )
    .await;

    let _ = client_event_tx.send(ServerEvent::Done { id });
    crate::logging::event_info(
        "COMM_LIFECYCLE",
        vec![
            ("phase", "message_done".to_string()),
            ("request_id", id.to_string()),
            ("from_session", from_session),
            ("swarm_id", swarm_id),
            ("target_session", target),
            ("elapsed_ms", started.elapsed().as_millis().to_string()),
        ],
    );
}
