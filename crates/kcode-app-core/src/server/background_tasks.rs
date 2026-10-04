use super::live_turn::{LiveTurnSwarmContext, RunGrant, TurnSeed, run_live_turn_if_idle};
use super::state::SwarmEvent;
use super::{
    SessionAgents, SessionInterruptQueues, SwarmMember, fanout_session_event,
    queue_soft_interrupt_for_session,
};
use crate::message::{
    format_background_task_notification_markdown, format_background_task_progress_markdown,
};
use crate::protocol::{NotificationType, ServerEvent};
use kcode_agent_runtime::SoftInterruptSource;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use tokio::sync::{RwLock, broadcast};

async fn emit_external_wake(
    session_id: &str,
    reason: &str,
    notification: &str,
    swarm_members: &Arc<RwLock<HashMap<String, SwarmMember>>>,
) -> bool {
    if crate::config::config().server.wake_mode != crate::config::WakeMode::External {
        return false;
    }
    let _ = fanout_session_event(
        swarm_members,
        session_id,
        ServerEvent::WakeRequested {
            session_id: session_id.to_string(),
            reason: reason.to_string(),
            notification: notification.to_string(),
        },
    )
    .await;
    true
}

pub(super) async fn dispatch_background_task_completion(
    task: &crate::bus::BackgroundTaskCompleted,
    sessions: &SessionAgents,
    soft_interrupt_queues: &SessionInterruptQueues,
    swarm_members: &Arc<RwLock<HashMap<String, SwarmMember>>>,
    event_history: &Arc<RwLock<VecDeque<SwarmEvent>>>,
    event_counter: &Arc<AtomicU64>,
    swarm_event_tx: &broadcast::Sender<SwarmEvent>,
) {
    let notification = format_background_task_notification_markdown(task);

    if task.notify
        && fanout_session_event(
            swarm_members,
            &task.session_id,
            ServerEvent::Notification {
                from_session: "background_task".to_string(),
                from_name: Some("background task".to_string()),
                notification_type: NotificationType::Message {
                    scope: Some("background_task".to_string()),
                },
                message: notification.clone(),
            },
        )
        .await
            == 0
    {
        crate::logging::warn(&format!(
            "Failed to notify attached clients for background task completion on session {}",
            task.session_id
        ));
    }

    if task.wake
        && !emit_external_wake(
            &task.session_id,
            "background_task_completed",
            &notification,
            swarm_members,
        )
        .await
        && !run_live_turn_if_idle(
            &task.session_id,
            TurnSeed::asked(
                &notification,
                Some(
                    "A background task for this session just finished. Review the completion message and continue if useful."
                        .to_string(),
                ),
                None,
            ),
            RunGrant::denied(),
            sessions,
            LiveTurnSwarmContext::new(
                swarm_members,
                event_history,
                event_counter,
                swarm_event_tx,
            ),
        )
        .await
        && !queue_soft_interrupt_for_session(
            &task.session_id,
            notification.clone(),
            false,
            SoftInterruptSource::BackgroundTask,
            soft_interrupt_queues,
            sessions,
        )
        .await
    {
        crate::logging::warn(&format!(
            "Failed to deliver background task completion to session {}",
            task.session_id
        ));
    }
}

/// Deliver a stall-watchdog wake for a background task that has gone quiet.
///
/// Mirrors completion delivery: optionally notify attached clients, then wake
/// an idle agent or queue a soft interrupt for a busy one. The task is still
/// running; the message tells the agent to inspect and decide.
pub(super) async fn dispatch_background_task_stalled(
    task: &crate::bus::BackgroundTaskStalled,
    sessions: &SessionAgents,
    soft_interrupt_queues: &SessionInterruptQueues,
    swarm_members: &Arc<RwLock<HashMap<String, SwarmMember>>>,
    event_history: &Arc<RwLock<VecDeque<SwarmEvent>>>,
    event_counter: &Arc<AtomicU64>,
    swarm_event_tx: &broadcast::Sender<SwarmEvent>,
) {
    let notification = crate::message::format_background_task_stalled_markdown(task);

    if task.notify
        && fanout_session_event(
            swarm_members,
            &task.session_id,
            ServerEvent::Notification {
                from_session: "background_task".to_string(),
                from_name: Some("background task".to_string()),
                notification_type: NotificationType::Message {
                    scope: Some("background_task".to_string()),
                },
                message: notification.clone(),
            },
        )
        .await
            == 0
    {
        crate::logging::warn(&format!(
            "Failed to notify attached clients for background task stall on session {}",
            task.session_id
        ));
    }

    if task.wake
        && !emit_external_wake(
            &task.session_id,
            "background_task_stalled",
            &notification,
            swarm_members,
        )
        .await
        && !run_live_turn_if_idle(
            &task.session_id,
            TurnSeed::asked(
                &notification,
                Some(
                    "A background task for this session has produced no output or progress for its stall window. Inspect it and decide whether to keep waiting, fix it, or cancel it."
                        .to_string(),
                ),
                None,
            ),
            RunGrant::denied(),
            sessions,
            LiveTurnSwarmContext::new(
                swarm_members,
                event_history,
                event_counter,
                swarm_event_tx,
            ),
        )
        .await
        && !queue_soft_interrupt_for_session(
            &task.session_id,
            notification.clone(),
            false,
            SoftInterruptSource::BackgroundTask,
            soft_interrupt_queues,
            sessions,
        )
        .await
    {
        crate::logging::warn(&format!(
            "Failed to deliver background task stall to session {}",
            task.session_id
        ));
    }
}

pub(super) async fn dispatch_background_task_progress(
    task: &crate::bus::BackgroundTaskProgressEvent,
    swarm_members: &Arc<RwLock<HashMap<String, SwarmMember>>>,
) {
    let notification = format_background_task_progress_markdown(task);
    if fanout_session_event(
        swarm_members,
        &task.session_id,
        ServerEvent::Notification {
            from_session: "background_task".to_string(),
            from_name: Some("background task".to_string()),
            notification_type: NotificationType::Message {
                scope: Some("background_task".to_string()),
            },
            message: notification,
        },
    )
    .await
        == 0
    {
        crate::logging::warn(&format!(
            "Failed to notify attached clients for background task progress on session {}",
            task.session_id
        ));
    }
}

/// Update a swarm worker's cached output tail and rebroadcast swarm status so
/// the coordinator's inline gallery can render the live viewport. The tail is
/// already capped by the producer; we only store and fan it out.
pub(super) async fn dispatch_swarm_output_tail(
    tail: &crate::bus::SwarmOutputTail,
    swarm_members: &Arc<RwLock<HashMap<String, SwarmMember>>>,
) {
    let swarm_id = {
        let mut members = swarm_members.write().await;
        let Some(member) = members.get_mut(&tail.session_id) else {
            return;
        };
        member.output_tail = Some(tail.tail.clone());
        super::swarm::swarm_root(&members, &tail.session_id)
    };
    if let Some(swarm_id) = swarm_id {
        super::swarm::broadcast_swarm_status(&swarm_id, swarm_members).await;
    }
}

pub(super) async fn dispatch_swarm_runtime_status(
    event: &crate::bus::SubagentStatus,
    swarm_members: &Arc<RwLock<HashMap<String, SwarmMember>>>,
) {
    let Some(model) = event
        .model
        .as_ref()
        .filter(|model| !model.trim().is_empty())
    else {
        return;
    };
    let swarm_id = {
        let mut members = swarm_members.write().await;
        let Some(member) = members.get_mut(&event.session_id) else {
            return;
        };
        if member.runtime.model.as_ref() == Some(model) {
            return;
        }
        member.runtime.model = Some(model.clone());
        super::swarm::swarm_root(&members, &event.session_id)
    };
    if let Some(swarm_id) = swarm_id {
        super::swarm::broadcast_swarm_status(&swarm_id, swarm_members).await;
    }
}

pub(super) async fn dispatch_ui_activity(
    activity: &crate::bus::UiActivity,
    swarm_members: &Arc<RwLock<HashMap<String, SwarmMember>>>,
) {
    if activity.message.trim().is_empty() {
        return;
    }
    let Some(session_id) = activity.session_id.as_deref() else {
        return;
    };

    if fanout_session_event(
        swarm_members,
        session_id,
        ServerEvent::Notification {
            from_session: "kcode".to_string(),
            from_name: Some("Kcode".to_string()),
            notification_type: NotificationType::Message {
                scope: Some(activity.kind.scope().to_string()),
            },
            message: activity.message.clone(),
        },
    )
    .await
        == 0
    {
        crate::logging::warn(&format!(
            "Failed to notify attached clients for UI activity on session {}",
            session_id
        ));
    }
}
