use super::live_turn::{LiveTurnSwarmContext, run_live_turn_if_idle};
use super::state::SwarmEvent;
use super::{
    SessionAgents, SessionInterruptQueues, SwarmMember, fanout_session_event,
    queue_soft_interrupt_for_session,
};
use crate::message::{
    format_background_task_notification_markdown, format_background_task_progress_markdown,
};
use crate::protocol::{NotificationType, ServerEvent};
use jcode_agent_runtime::SoftInterruptSource;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use tokio::sync::{RwLock, broadcast};

#[expect(
    clippy::too_many_arguments,
    reason = "background task completion needs session, interrupt, and swarm status state"
)]
pub(super) async fn dispatch_background_task_completion(
    task: &crate::bus::BackgroundTaskCompleted,
    sessions: &SessionAgents,
    soft_interrupt_queues: &SessionInterruptQueues,
    swarm_members: &Arc<RwLock<HashMap<String, SwarmMember>>>,
    swarms_by_id: &Arc<RwLock<HashMap<String, HashSet<String>>>>,
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
                    channel: None,
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
        && !run_live_turn_if_idle(
            &task.session_id,
            &notification,
            Some(
                "A background task for this session just finished. Review the completion message and continue if useful."
                    .to_string(),
            ),
            sessions,
            LiveTurnSwarmContext::new(
                swarm_members,
                swarms_by_id,
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
                channel: None,
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
    swarms_by_id: &Arc<RwLock<HashMap<String, HashSet<String>>>>,
) {
    let swarm_id = {
        let mut members = swarm_members.write().await;
        let Some(member) = members.get_mut(&tail.session_id) else {
            return;
        };
        member.output_tail = Some(tail.tail.clone());
        member.swarm_id.clone()
    };
    if let Some(swarm_id) = swarm_id {
        super::swarm::broadcast_swarm_status(&swarm_id, swarm_members, swarms_by_id).await;
    }
}

/// Update a swarm member's aggregate todo progress (completed/total) from a
/// `TodoUpdated` bus event and rebroadcast swarm status so coordinators see the
/// counter move on the inline strip. Only the counts cross the swarm boundary,
/// never the todo items themselves.
pub(super) async fn dispatch_swarm_todo_progress(
    event: &crate::bus::TodoEvent,
    swarm_members: &Arc<RwLock<HashMap<String, SwarmMember>>>,
    swarms_by_id: &Arc<RwLock<HashMap<String, HashSet<String>>>>,
) {
    let total = event.todos.len() as u32;
    let completed = event
        .todos
        .iter()
        .filter(|t| t.status == "completed")
        .count() as u32;
    let progress = if total == 0 { None } else { Some((completed, total)) };

    let swarm_id = {
        let mut members = swarm_members.write().await;
        let Some(member) = members.get_mut(&event.session_id) else {
            return;
        };
        if member.todo_progress == progress {
            return; // no change, skip the broadcast
        }
        member.todo_progress = progress;
        member.swarm_id.clone()
    };
    if let Some(swarm_id) = swarm_id {
        super::swarm::broadcast_swarm_status(&swarm_id, swarm_members, swarms_by_id).await;
    }
}

pub(super) async fn dispatch_ui_activity(
    activity: &crate::bus::UiActivity,
    swarm_members: &Arc<RwLock<HashMap<String, SwarmMember>>>,
) {
    let Some(session_id) = activity.session_id.as_deref() else {
        return;
    };

    if fanout_session_event(
        swarm_members,
        session_id,
        ServerEvent::Notification {
            from_session: "jcode".to_string(),
            from_name: Some("Jcode".to_string()),
            notification_type: NotificationType::Message {
                scope: Some(activity.kind.scope().to_string()),
                channel: None,
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
