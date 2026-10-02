use super::{RunState, SharedContext, SwarmMember, SwarmState, persist_swarm_state_for};
use crate::protocol::{NotificationType, ServerEvent};
use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;

pub(super) struct DebugSwarmWriteContext<'a> {
    pub(super) session_id: &'a Arc<RwLock<String>>,
    pub(super) swarm_members: &'a Arc<RwLock<HashMap<String, SwarmMember>>>,
    pub(super) shared_context: &'a Arc<RwLock<HashMap<String, HashMap<String, SharedContext>>>>,
    pub(super) swarm_runs: &'a Arc<RwLock<HashMap<String, RunState>>>,
}

pub(super) async fn maybe_handle_swarm_write_command(
    cmd: &str,
    ctx: &DebugSwarmWriteContext<'_>,
) -> Result<Option<String>> {
    if cmd.starts_with("swarm:clear_plan:") {
        let swarm_id = cmd.strip_prefix("swarm:clear_plan:").unwrap_or("").trim();
        if swarm_id.is_empty() {
            return Err(anyhow::anyhow!(
                "swarm:clear_plan requires a swarm_id: swarm:clear_plan:<swarm_id>"
            ));
        }
        let removed = {
            let mut plans = ctx.swarm_runs.write().await;
            plans.remove(swarm_id)
        };
        let Some(removed) = removed else {
            return Err(anyhow::anyhow!(
                "No run state found for swarm '{}'",
                swarm_id
            ));
        };
        // The plan is in memory only, so clearing it clears it; the members are
        // what the state file holds.
        let swarm_state = SwarmState {
            members: Arc::clone(ctx.swarm_members),
            runs: Arc::clone(ctx.swarm_runs),
        };
        persist_swarm_state_for(swarm_id, &swarm_state).await;
        // Push the cleared state to attached clients. Without this, every
        // connected TUI keeps rendering (and holding resident) the old item
        // graph until its next reconnect; a 1.5k-item stale plan is ~650 KB
        // of JSON pinned per client.
        let clear_event = ServerEvent::SwarmPlan {
            swarm_id: swarm_id.to_string(),
            items: Vec::new(),
            reason: Some("plan_cleared".to_string()),
            summary: None,
        };
        let session_ids = super::swarm::swarm_session_ids(swarm_id, ctx.swarm_members).await;
        {
            let members = ctx.swarm_members.read().await;
            for sid in session_ids {
                if let Some(member) = members.get(&sid) {
                    let _ = member.event_tx.send(clear_event.clone());
                    for tx in member.event_txs.values() {
                        let _ = tx.send(clear_event.clone());
                    }
                }
            }
        }
        return Ok(Some(
            serde_json::json!({
                "swarm_id": swarm_id,
                "cleared_row_state_count": removed.len(),
            })
            .to_string(),
        ));
    }

    if cmd.starts_with("swarm:broadcast:") {
        let rest = cmd.strip_prefix("swarm:broadcast:").unwrap_or("").trim();
        let (target_swarm_id, message) = if let Some(space_idx) = rest.find(' ') {
            let potential_id = &rest[..space_idx];
            let msg = rest[space_idx + 1..].trim();
            if potential_id.contains('/') {
                (Some(potential_id.to_string()), msg.to_string())
            } else {
                (None, rest.to_string())
            }
        } else {
            (None, rest.to_string())
        };

        if message.is_empty() {
            return Err(anyhow::anyhow!("swarm:broadcast requires a message"));
        }

        let swarm_id = if let Some(id) = target_swarm_id {
            Some(id)
        } else {
            let members = ctx.swarm_members.read().await;
            let current_session = ctx.session_id.read().await;
            super::swarm::swarm_root(&members, &current_session)
        };

        if let Some(swarm_id) = swarm_id {
            let member_ids = super::swarm::swarm_session_ids(&swarm_id, ctx.swarm_members).await;
            let members = ctx.swarm_members.read().await;
            let current_session = ctx.session_id.read().await;
            let from_name = members
                .get(&*current_session)
                .and_then(|member| member.friendly_name.clone());

            if !member_ids.is_empty() {
                let mut sent_count = 0;
                for member_id in &member_ids {
                    if let Some(member) = members.get(member_id) {
                        let notification = ServerEvent::Notification {
                            from_session: current_session.clone(),
                            from_name: from_name.clone(),
                            notification_type: NotificationType::Message {
                                scope: Some("broadcast".to_string()),
                                channel: None,
                                tldr: None,
                            },
                            message: message.clone(),
                        };
                        if member.event_tx.send(notification).is_ok() {
                            sent_count += 1;
                        }
                    }
                }
                return Ok(Some(
                    serde_json::json!({
                        "swarm_id": swarm_id,
                        "message": message,
                        "sent_to": sent_count,
                    })
                    .to_string(),
                ));
            }

            return Err(anyhow::anyhow!("No members in swarm '{}'", swarm_id));
        }

        return Err(anyhow::anyhow!(
            "No swarm found. Specify swarm_id: swarm:broadcast:<swarm_id> <message>"
        ));
    }

    if cmd.starts_with("swarm:notify:") {
        let rest = cmd.strip_prefix("swarm:notify:").unwrap_or("").trim();
        if let Some(space_idx) = rest.find(' ') {
            let target_session = &rest[..space_idx];
            let message = rest[space_idx + 1..].trim();

            if message.is_empty() {
                return Err(anyhow::anyhow!("swarm:notify requires a message"));
            }

            let members = ctx.swarm_members.read().await;
            let current_session = ctx.session_id.read().await;
            let from_name = members
                .get(&*current_session)
                .and_then(|member| member.friendly_name.clone());

            if let Some(target) = members.get(target_session) {
                let notification = ServerEvent::Notification {
                    from_session: current_session.clone(),
                    from_name: from_name.clone(),
                    notification_type: NotificationType::Message {
                        scope: Some("dm".to_string()),
                        channel: None,
                        tldr: None,
                    },
                    message: message.to_string(),
                };
                if target.event_tx.send(notification).is_ok() {
                    return Ok(Some(
                        serde_json::json!({
                            "sent_to": target_session,
                            "sent_to_name": target.friendly_name.clone(),
                            "message": message,
                        })
                        .to_string(),
                    ));
                }
                return Err(anyhow::anyhow!("Failed to send notification"));
            }

            return Err(anyhow::anyhow!("Unknown session '{}'", target_session));
        }

        return Err(anyhow::anyhow!(
            "Usage: swarm:notify:<session_id> <message>"
        ));
    }

    if cmd.starts_with("swarm:set_context:") {
        let rest = cmd.strip_prefix("swarm:set_context:").unwrap_or("").trim();
        let parts: Vec<&str> = rest.splitn(3, ' ').collect();
        if parts.len() < 3 {
            return Err(anyhow::anyhow!(
                "Usage: swarm:set_context:<session_id> <key> <value>"
            ));
        }

        let acting_session = parts[0];
        let key = parts[1].to_string();
        let value = parts[2].to_string();

        let (swarm_id, friendly_name) = {
            let members = ctx.swarm_members.read().await;
            let swarm_id = super::swarm::swarm_root(&members, acting_session);
            let name = members
                .get(acting_session)
                .and_then(|member| member.friendly_name.clone());
            (swarm_id, name)
        };

        if let Some(swarm_id) = swarm_id {
            {
                let mut shared_ctx = ctx.shared_context.write().await;
                let swarm_ctx = shared_ctx
                    .entry(swarm_id.clone())
                    .or_insert_with(HashMap::new);
                let now = Instant::now();
                let created_at = swarm_ctx
                    .get(&key)
                    .map(|context| context.created_at)
                    .unwrap_or(now);
                swarm_ctx.insert(
                    key.clone(),
                    SharedContext {
                        key: key.clone(),
                        value: value.clone(),
                        from_session: acting_session.to_string(),
                        from_name: friendly_name.clone(),
                        created_at,
                        updated_at: now,
                    },
                );
            }

            let swarm_session_ids =
                super::swarm::swarm_session_ids(&swarm_id, ctx.swarm_members).await;
            let members = ctx.swarm_members.read().await;
            for sid in &swarm_session_ids {
                if sid != acting_session
                    && let Some(member) = members.get(sid)
                {
                    let _ = member.event_tx.send(ServerEvent::Notification {
                        from_session: acting_session.to_string(),
                        from_name: friendly_name.clone(),
                        notification_type: NotificationType::SharedContext {
                            key: key.clone(),
                            value: value.clone(),
                        },
                        message: format!("Shared context: {} = {}", key, value),
                    });
                }
            }

            return Ok(Some(
                serde_json::json!({
                    "swarm_id": swarm_id,
                    "key": key,
                    "value": value,
                    "from_session": acting_session,
                })
                .to_string(),
            ));
        }

        return Err(anyhow::anyhow!(
            "Session '{}' is not in a swarm",
            acting_session
        ));
    }

    Ok(None)
}
