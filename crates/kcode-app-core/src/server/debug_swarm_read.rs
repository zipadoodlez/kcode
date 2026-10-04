use super::{FileTouchService, ServerIdentity, SessionAgents, SwarmMember};
use crate::plan::summarize_plan_graph;
use crate::protocol::SwarmLifecycleStatus;
use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

pub(super) async fn maybe_handle_swarm_read_command(
    cmd: &str,
    sessions: &SessionAgents,
    swarm_members: &Arc<RwLock<HashMap<String, SwarmMember>>>,
    file_touch: &FileTouchService,
    server_identity: &ServerIdentity,
) -> Result<Option<String>> {
    if cmd == "swarm" || cmd == "swarm_status" || cmd == "swarm:members" {
        let members = swarm_members.read().await;
        let sessions_guard = sessions.read().await;
        let mut out: Vec<serde_json::Value> = Vec::new();
        for member in members.values() {
            let (provider, model) = if let Some(agent_arc) = sessions_guard.get(&member.session_id)
            {
                if let Ok(agent) = agent_arc.try_lock() {
                    (Some(agent.provider_name()), Some(agent.provider_model()))
                } else {
                    (None, None)
                }
            } else {
                (None, None)
            };
            out.push(serde_json::json!({
                "session_id": member.session_id,
                "friendly_name": member.friendly_name,
                "swarm_id": super::swarm::swarm_root(&members, &member.session_id),
                "working_dir": member.working_dir,
                "status": member.status,
                "detail": member.detail,
                "role": super::swarm::swarm_role(&members, &member.session_id),
                "is_headless": member.is_headless,
                "live_attachments": member.event_txs.len(),
                "joined_secs_ago": member.joined_at.elapsed().as_secs(),
                "status_changed_secs_ago": member.last_status_change.elapsed().as_secs(),
                "provider": provider,
                "model": model,
                "server_name": server_identity.name,
                "server_icon": server_identity.icon,
            }));
        }
        return Ok(Some(
            serde_json::to_string_pretty(&out).unwrap_or_else(|_| "[]".to_string()),
        ));
    }

    if cmd == "swarm:list" {
        let members = swarm_members.read().await;
        // A run is what its members' report-back chains root at, so the runs are
        // the distinct roots and a run's coordinator is the root itself.
        let mut swarms: HashMap<String, Vec<String>> = HashMap::new();
        for member in members.values() {
            if let Some(root) = super::swarm::swarm_root(&members, &member.session_id) {
                swarms
                    .entry(root)
                    .or_default()
                    .push(member.session_id.clone());
            }
        }
        let mut swarm_ids: Vec<&String> = swarms.keys().collect();
        swarm_ids.sort();
        let mut out: Vec<serde_json::Value> = Vec::new();
        for swarm_id in swarm_ids {
            let session_ids = &swarms[swarm_id];
            let coordinator_name = members.get(swarm_id).and_then(|m| m.friendly_name.clone());
            let mut status_counts: HashMap<String, usize> = HashMap::new();
            let mut headless_count = 0usize;
            let mut attached_member_count = 0usize;
            let mut live_attachment_count = 0usize;
            let member_details: Vec<serde_json::Value> = session_ids
                .iter()
                .filter_map(|session_id| members.get(session_id))
                .map(|member| {
                    *status_counts.entry(member.status.to_string()).or_default() += 1;
                    if member.is_headless {
                        headless_count += 1;
                    }
                    if !member.event_txs.is_empty() {
                        attached_member_count += 1;
                    }
                    live_attachment_count += member.event_txs.len();
                    serde_json::json!({
                        "session_id": member.session_id,
                        "friendly_name": member.friendly_name,
                        "status": member.status,
                        "detail": member.detail,
                        "role": super::swarm::swarm_role(&members, &member.session_id),
                        "is_headless": member.is_headless,
                        "live_attachments": member.event_txs.len(),
                    })
                })
                .collect();
            out.push(serde_json::json!({
                "swarm_id": swarm_id,
                "member_count": session_ids.len(),
                "members": session_ids,
                "coordinator": swarm_id,
                "coordinator_name": coordinator_name,
                "headless_count": headless_count,
                "attached_member_count": attached_member_count,
                "live_attachment_count": live_attachment_count,
                "status_counts": status_counts,
                "member_details": member_details,
            }));
        }
        return Ok(Some(
            serde_json::to_string_pretty(&out).unwrap_or_else(|_| "[]".to_string()),
        ));
    }

    if cmd == "swarm:coordinators" {
        let members = swarm_members.read().await;
        let mut swarm_ids: Vec<String> = members
            .values()
            .filter_map(|member| super::swarm::swarm_root(&members, &member.session_id))
            .collect();
        swarm_ids.sort();
        swarm_ids.dedup();
        let mut out: Vec<serde_json::Value> = Vec::new();
        for swarm_id in swarm_ids {
            let name = members.get(&swarm_id).and_then(|m| m.friendly_name.clone());
            out.push(serde_json::json!({
                "swarm_id": swarm_id,
                "coordinator_session": swarm_id,
                "coordinator_name": name,
            }));
        }
        return Ok(Some(
            serde_json::to_string_pretty(&out).unwrap_or_else(|_| "[]".to_string()),
        ));
    }

    if cmd.starts_with("swarm:coordinator:") {
        let swarm_id = cmd.strip_prefix("swarm:coordinator:").unwrap_or("").trim();
        let members = swarm_members.read().await;
        let exists = members.values().any(|member| {
            super::swarm::swarm_root(&members, &member.session_id).as_deref() == Some(swarm_id)
        });
        if !exists {
            return Err(anyhow::anyhow!("No coordinator for swarm '{}'", swarm_id));
        }
        // The run's root holds the coordinator role, so the coordinator of a
        // swarm id is that id itself.
        let name = members.get(swarm_id).and_then(|m| m.friendly_name.clone());
        return Ok(Some(
            serde_json::json!({
                "swarm_id": swarm_id,
                "coordinator_session": swarm_id,
                "coordinator_name": name,
            })
            .to_string(),
        ));
    }

    if cmd == "swarm:roles" {
        let members = swarm_members.read().await;
        let mut out: Vec<serde_json::Value> = Vec::new();
        for (sid, member) in members.iter() {
            let is_coordinator = super::swarm::swarm_is_root(&members, sid);
            out.push(serde_json::json!({
                "session_id": sid,
                "friendly_name": member.friendly_name,
                "role": super::swarm::swarm_role(&members, sid),
                "swarm_id": super::swarm::swarm_root(&members, sid),
                "status": member.status,
                "is_coordinator": is_coordinator,
            }));
        }
        return Ok(Some(
            serde_json::to_string_pretty(&out).unwrap_or_else(|_| "[]".to_string()),
        ));
    }

    if cmd.starts_with("swarm:plan:") {
        let swarm_id = cmd.strip_prefix("swarm:plan:").unwrap_or("").trim();
        let members = super::swarm::swarm_members_of(swarm_id, swarm_members).await;
        let requester = members
            .first()
            .map(|member| member.session_id.as_str())
            .unwrap_or(swarm_id);
        let items = super::swarm::swarm_rows(swarm_id, requester, swarm_members).await;
        if items.is_empty() {
            return Ok(Some("[]".to_string()));
        }
        let summary = summarize_plan_graph(&items);
        let output = serde_json::json!({
            "swarm_id": swarm_id,
            "member_count": members.len(),
            "coordinator": swarm_id,
            "rows": &items,
            "blocked_ids": summary.blocked_ids,
            "cycle_ids": summary.cycle_ids,
            "unresolved_dependency_ids": summary.unresolved_dependency_ids,
        })
        .to_string();
        return Ok(Some(output));
    }

    if cmd == "swarm:touches" {
        let touches = file_touch.snapshot().await;
        let members = swarm_members.read().await;
        let mut out: Vec<serde_json::Value> = Vec::new();
        for (path, accesses) in touches.iter() {
            for access in accesses.iter() {
                let name = members
                    .get(&access.session_id)
                    .and_then(|m| m.friendly_name.clone());
                let timestamp_unix = access
                    .absolute_time
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                out.push(serde_json::json!({
                    "path": path.to_string_lossy(),
                    "session_id": access.session_id,
                    "session_name": name,
                    "op": access.op.as_str(),
                    "summary": access.summary,
                    "age_secs": access.timestamp.elapsed().as_secs(),
                    "timestamp_unix": timestamp_unix,
                }));
            }
        }
        return Ok(Some(
            serde_json::to_string_pretty(&out).unwrap_or_else(|_| "[]".to_string()),
        ));
    }

    if cmd.starts_with("swarm:touches:") {
        let arg = cmd.strip_prefix("swarm:touches:").unwrap_or("").trim();
        let touches = file_touch.snapshot().await;
        let members = swarm_members.read().await;
        let output = if arg.starts_with("swarm:") {
            let swarm_id = arg.strip_prefix("swarm:").unwrap_or("");
            let swarm_sessions: HashSet<String> = members
                .iter()
                .filter(|(_, m)| {
                    super::swarm::swarm_root(&members, &m.session_id).as_deref() == Some(swarm_id)
                })
                .map(|(id, _)| id.clone())
                .collect();

            let mut out: Vec<serde_json::Value> = Vec::new();
            for (path, accesses) in touches.iter() {
                for access in accesses.iter() {
                    if swarm_sessions.contains(&access.session_id) {
                        let name = members
                            .get(&access.session_id)
                            .and_then(|m| m.friendly_name.clone());
                        let timestamp_unix = access
                            .absolute_time
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_secs())
                            .unwrap_or(0);
                        out.push(serde_json::json!({
                            "path": path.to_string_lossy(),
                            "session_id": access.session_id,
                            "session_name": name,
                            "op": access.op.as_str(),
                            "summary": access.summary,
                            "age_secs": access.timestamp.elapsed().as_secs(),
                            "timestamp_unix": timestamp_unix,
                        }));
                    }
                }
            }
            serde_json::to_string_pretty(&out).unwrap_or_else(|_| "[]".to_string())
        } else {
            let path = PathBuf::from(arg);
            if let Some(accesses) = touches.get(&path) {
                let mut out: Vec<serde_json::Value> = Vec::new();
                for access in accesses.iter() {
                    let name = members
                        .get(&access.session_id)
                        .and_then(|m| m.friendly_name.clone());
                    let timestamp_unix = access
                        .absolute_time
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(0);
                    out.push(serde_json::json!({
                        "session_id": access.session_id,
                        "session_name": name,
                        "op": access.op.as_str(),
                        "summary": access.summary,
                        "age_secs": access.timestamp.elapsed().as_secs(),
                        "timestamp_unix": timestamp_unix,
                    }));
                }
                serde_json::to_string_pretty(&out).unwrap_or_else(|_| "[]".to_string())
            } else {
                "[]".to_string()
            }
        };
        return Ok(Some(output));
    }

    if cmd == "swarm:conflicts" {
        let touches = file_touch.snapshot().await;
        let members = swarm_members.read().await;
        let mut out: Vec<serde_json::Value> = Vec::new();
        for (path, accesses) in touches.iter() {
            let unique_sessions: HashSet<_> = accesses.iter().map(|a| &a.session_id).collect();
            if unique_sessions.len() > 1 {
                let access_history: Vec<_> = accesses
                    .iter()
                    .map(|access| {
                        let name = members
                            .get(&access.session_id)
                            .and_then(|m| m.friendly_name.clone());
                        let timestamp_unix = access
                            .absolute_time
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_secs())
                            .unwrap_or(0);
                        serde_json::json!({
                            "session_id": access.session_id,
                            "session_name": name,
                            "op": access.op.as_str(),
                            "summary": access.summary,
                            "age_secs": access.timestamp.elapsed().as_secs(),
                            "timestamp_unix": timestamp_unix,
                        })
                    })
                    .collect();
                out.push(serde_json::json!({
                    "path": path.to_string_lossy(),
                    "session_count": unique_sessions.len(),
                    "accesses": access_history,
                }));
            }
        }
        return Ok(Some(
            serde_json::to_string_pretty(&out).unwrap_or_else(|_| "[]".to_string()),
        ));
    }

    if cmd.starts_with("swarm:info:") {
        let swarm_id = cmd.strip_prefix("swarm:info:").unwrap_or("").trim();
        let session_ids = super::swarm::swarm_session_ids(swarm_id, swarm_members).await;
        if session_ids.is_empty() {
            return Err(anyhow::anyhow!("No swarm with id '{}'", swarm_id));
        }
        let members = swarm_members.read().await;
        let touches = file_touch.snapshot().await;

        let coordinator_name = members.get(swarm_id).and_then(|m| m.friendly_name.clone());

        let member_details: Vec<_> = session_ids
            .iter()
            .filter_map(|sid| {
                members.get(sid).map(|m| {
                    serde_json::json!({
                        "session_id": m.session_id,
                        "friendly_name": m.friendly_name,
                        "status": m.status,
                        "detail": m.detail,
                        "working_dir": m.working_dir,
                    })
                })
            })
            .collect();

        let items = super::swarm::swarm_rows(swarm_id, swarm_id, swarm_members).await;

        let conflicts: Vec<_> = touches
            .iter()
            .filter_map(|(path, accesses)| {
                let swarm_accesses: Vec<_> = accesses
                    .iter()
                    .filter(|a| session_ids.contains(&a.session_id))
                    .collect();
                let unique: HashSet<_> = swarm_accesses.iter().map(|a| &a.session_id).collect();
                if unique.len() > 1 {
                    Some(path.to_string_lossy().to_string())
                } else {
                    None
                }
            })
            .collect();

        let output = serde_json::json!({
            "swarm_id": swarm_id,
            "member_count": session_ids.len(),
            "members": member_details,
            "coordinator": swarm_id,
            "coordinator_name": coordinator_name,
            "plan": { "items": items },
            "conflict_files": conflicts,
        })
        .to_string();
        return Ok(Some(output));
    }

    if cmd.starts_with("swarm:session:") {
        let target_session = cmd.strip_prefix("swarm:session:").unwrap_or("").trim();
        if target_session.is_empty() {
            return Err(anyhow::anyhow!("swarm:session requires a session_id"));
        }
        let sessions_guard = sessions.read().await;
        let members = swarm_members.read().await;

        let output = if let Some(agent_arc) = sessions_guard.get(target_session) {
            let member_info = members.get(target_session);
            let agent_state = if let Ok(agent) = agent_arc.try_lock() {
                Some(serde_json::json!({
                    "provider": agent.provider_name(),
                    "model": agent.provider_model(),
                    "message_count": agent.message_count(),
                    "pending_alert_count": agent.pending_alert_count(),
                    "pending_alerts": agent.pending_alerts_preview(),
                    "soft_interrupt_count": agent.soft_interrupt_count(),
                    "soft_interrupts": agent.soft_interrupts_preview(),
                    "has_urgent_interrupt": agent.has_urgent_interrupt(),
                    "last_usage": agent.last_usage(),
                }))
            } else {
                None
            };

            let is_processing = member_info
                .map(|m| m.status == SwarmLifecycleStatus::Running)
                .unwrap_or(agent_state.is_none());

            serde_json::json!({
                "session_id": target_session,
                "friendly_name": member_info.and_then(|m| m.friendly_name.clone()),
                "swarm_id": super::swarm::swarm_root(&members, target_session),
                "status": member_info.map(|m| m.status.clone()),
                "detail": member_info.and_then(|m| m.detail.clone()),
                "joined_secs_ago": member_info.map(|m| m.joined_at.elapsed().as_secs()),
                "status_changed_secs_ago": member_info.map(|m| m.last_status_change.elapsed().as_secs()),
                "is_processing": is_processing,
                "agent_state": agent_state,
            })
            .to_string()
        } else {
            return Err(anyhow::anyhow!("Unknown session '{}'", target_session));
        };
        return Ok(Some(output));
    }

    if cmd == "swarm:interrupts" {
        let sessions_guard = sessions.read().await;
        let members = swarm_members.read().await;
        let mut out: Vec<serde_json::Value> = Vec::new();

        for (session_id, agent_arc) in sessions_guard.iter() {
            if let Ok(agent) = agent_arc.try_lock() {
                let alert_count = agent.pending_alert_count();
                let interrupt_count = agent.soft_interrupt_count();

                if alert_count > 0 || interrupt_count > 0 {
                    let name = members
                        .get(session_id)
                        .and_then(|m| m.friendly_name.clone());
                    out.push(serde_json::json!({
                        "session_id": session_id,
                        "session_name": name,
                        "pending_alert_count": alert_count,
                        "pending_alerts": agent.pending_alerts_preview(),
                        "soft_interrupt_count": interrupt_count,
                        "soft_interrupts": agent.soft_interrupts_preview(),
                        "has_urgent": agent.has_urgent_interrupt(),
                    }));
                }
            }
        }
        return Ok(Some(
            serde_json::to_string_pretty(&out).unwrap_or_else(|_| "[]".to_string()),
        ));
    }

    Ok(None)
}
