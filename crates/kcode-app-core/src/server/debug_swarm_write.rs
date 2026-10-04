use super::SwarmMember;
use crate::protocol::{NotificationType, ServerEvent};
use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

pub(super) struct DebugSwarmWriteContext<'a> {
    pub(super) session_id: &'a Arc<RwLock<String>>,
    pub(super) swarm_members: &'a Arc<RwLock<HashMap<String, SwarmMember>>>,
}

pub(super) async fn maybe_handle_swarm_write_command(
    cmd: &str,
    ctx: &DebugSwarmWriteContext<'_>,
) -> Result<Option<String>> {
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

    Ok(None)
}
