use super::{
    SharedContext, SwarmEvent, SwarmEventType, SwarmMember, fanout_session_event,
    record_swarm_event,
    util::{member_friendly_name, member_swarm_id},
};
use crate::protocol::{ContextEntry, NotificationType, ServerEvent};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{RwLock, broadcast, mpsc};

#[expect(
    clippy::too_many_arguments,
    reason = "shared-context writes join swarm membership, context state, and event sinks"
)]
pub(super) async fn handle_comm_share(
    id: u64,
    req_session_id: String,
    key: String,
    value: String,
    append: bool,
    client_event_tx: &mpsc::UnboundedSender<ServerEvent>,
    swarm_members: &Arc<RwLock<HashMap<String, SwarmMember>>>,
    shared_context: &Arc<RwLock<HashMap<String, HashMap<String, SharedContext>>>>,
    event_history: &Arc<RwLock<std::collections::VecDeque<SwarmEvent>>>,
    event_counter: &Arc<std::sync::atomic::AtomicU64>,
    swarm_event_tx: &broadcast::Sender<SwarmEvent>,
) {
    let swarm_id = member_swarm_id(&req_session_id, swarm_members).await;

    if let Some(swarm_id) = swarm_id {
        let friendly_name = member_friendly_name(&req_session_id, swarm_members).await;

        {
            let mut ctx = shared_context.write().await;
            let swarm_ctx = ctx.entry(swarm_id.clone()).or_insert_with(HashMap::new);
            let now = Instant::now();
            let created_at = swarm_ctx.get(&key).map(|c| c.created_at).unwrap_or(now);
            let stored_value = if append {
                swarm_ctx
                    .get(&key)
                    .map(|existing| {
                        if existing.value.is_empty() {
                            value.clone()
                        } else {
                            format!("{}\n{}", existing.value, value)
                        }
                    })
                    .unwrap_or_else(|| value.clone())
            } else {
                value.clone()
            };
            swarm_ctx.insert(
                key.clone(),
                SharedContext {
                    key: key.clone(),
                    value: stored_value.clone(),
                    from_session: req_session_id.clone(),
                    from_name: friendly_name.clone(),
                    created_at,
                    updated_at: now,
                },
            );
        }

        let swarm_session_ids: Vec<String> =
            super::swarm::swarm_session_ids(&swarm_id, swarm_members).await;

        // Shared-context updates are subtree-scoped like broadcasts: notify only
        // the sessions the writer (transitively) spawned, so a share cannot
        // become a member-cap-sized notification storm. The coordinator keeps
        // whole-swarm reach. Everyone can still `read` the key on demand.
        //
        // Compute the target set up front and drop the read guard before the
        // fanout loop: `fanout_session_event` takes a write lock on members.
        let notify_targets: Vec<String> = {
            let members = swarm_members.read().await;
            let sender_is_coordinator = super::swarm::swarm_is_root(&members, &req_session_id);
            swarm_session_ids
                .iter()
                .filter(|sid| *sid != &req_session_id)
                .filter(|sid| {
                    sender_is_coordinator
                        || super::swarm_is_self_or_ancestor(&members, &req_session_id, sid)
                })
                .cloned()
                .collect()
        };
        for sid in &notify_targets {
            {
                let _ = fanout_session_event(
                    swarm_members,
                    sid,
                    ServerEvent::Notification {
                        from_session: req_session_id.clone(),
                        from_name: friendly_name.clone(),
                        notification_type: NotificationType::SharedContext {
                            key: key.clone(),
                            value: if append {
                                format!("(appended) {}", value)
                            } else {
                                value.clone()
                            },
                        },
                        message: if append {
                            format!("Appended shared context: {} += {}", key, value)
                        } else {
                            format!("Shared context: {} = {}", key, value)
                        },
                    },
                )
                .await;
            }
        }

        record_swarm_event(
            event_history,
            event_counter,
            swarm_event_tx,
            req_session_id.clone(),
            friendly_name.clone(),
            Some(swarm_id.clone()),
            SwarmEventType::ContextUpdate {
                swarm_id: swarm_id.clone(),
                key: key.clone(),
            },
        )
        .await;

        let _ = client_event_tx.send(ServerEvent::Done { id });
    } else {
        let _ = client_event_tx.send(ServerEvent::Error {
            id,
            message: "Not in a swarm. Use a git repository to enable swarm features.".to_string(),
            retry_after_secs: None,
        });
    }
}

pub(super) async fn handle_comm_read(
    id: u64,
    req_session_id: String,
    key: Option<String>,
    client_event_tx: &mpsc::UnboundedSender<ServerEvent>,
    swarm_members: &Arc<RwLock<HashMap<String, SwarmMember>>>,
    shared_context: &Arc<RwLock<HashMap<String, HashMap<String, SharedContext>>>>,
) {
    let swarm_id = member_swarm_id(&req_session_id, swarm_members).await;

    let entries = if let Some(swarm_id) = swarm_id {
        let ctx = shared_context.read().await;
        if let Some(swarm_ctx) = ctx.get(&swarm_id) {
            if let Some(k) = key {
                swarm_ctx
                    .get(&k)
                    .map(|c| {
                        vec![ContextEntry {
                            key: c.key.clone(),
                            value: c.value.clone(),
                            from_session: c.from_session.clone(),
                            from_name: c.from_name.clone(),
                        }]
                    })
                    .unwrap_or_default()
            } else {
                swarm_ctx
                    .values()
                    .map(|c| ContextEntry {
                        key: c.key.clone(),
                        value: c.value.clone(),
                        from_session: c.from_session.clone(),
                        from_name: c.from_name.clone(),
                    })
                    .collect()
            }
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    let _ = client_event_tx.send(ServerEvent::CommContext { id, entries });
}
