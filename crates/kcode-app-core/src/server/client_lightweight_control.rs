use super::client_actions::{NotifySessionContext, handle_notify_session};
use super::client_comm::handle_comm_message;
use super::client_writer::write_direct_event;
use super::comm_session::{handle_comm_list_models, handle_comm_spawn, handle_comm_stop};
use super::{
    ClientConnectionInfo, RunState, SessionAgents, SessionInterruptQueues, SwarmEvent, SwarmMember,
    SwarmMutationRuntime,
};
use crate::config::SwarmSpawnMode;
use crate::protocol::{Request, ServerEvent};
use crate::provider::Provider;
use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock, broadcast, mpsc};

pub(super) fn parse_swarm_spawn_mode(
    id: u64,
    spawn_mode: Option<String>,
    client_event_tx: &mpsc::UnboundedSender<ServerEvent>,
) -> Option<Option<SwarmSpawnMode>> {
    match spawn_mode {
        Some(value) => match SwarmSpawnMode::parse(&value) {
            Some(mode) => Some(Some(mode)),
            None => {
                let _ = client_event_tx.send(ServerEvent::Error {
                    id,
                    message: format!(
                        "Invalid spawn_mode '{value}'. Expected one of: visible, headless, inline, auto"
                    ),
                    retry_after_secs: None,
                });
                None
            }
        },
        None => Some(None),
    }
}

pub(super) struct LightweightControlContext<'a> {
    pub(super) sessions: &'a SessionAgents,
    pub(super) global_session_id: &'a Arc<RwLock<String>>,
    pub(super) provider_template: &'a Arc<dyn Provider>,
    pub(super) swarm_members: &'a Arc<RwLock<HashMap<String, SwarmMember>>>,
    pub(super) swarm_runs: &'a Arc<RwLock<HashMap<String, RunState>>>,
    pub(super) client_connections: &'a Arc<RwLock<HashMap<String, ClientConnectionInfo>>>,
    pub(super) event_history: &'a Arc<RwLock<std::collections::VecDeque<SwarmEvent>>>,
    pub(super) event_counter: &'a Arc<std::sync::atomic::AtomicU64>,
    pub(super) swarm_event_tx: &'a broadcast::Sender<SwarmEvent>,
    pub(super) mcp_pool: &'a Arc<crate::mcp::SharedMcpPool>,
    pub(super) soft_interrupt_queues: &'a SessionInterruptQueues,
    pub(super) swarm_mutation_runtime: &'a SwarmMutationRuntime,
}

pub(super) async fn handle_lightweight_control_request(
    request: Request,
    writer: Arc<Mutex<crate::transport::WriteHalf>>,
    context: LightweightControlContext<'_>,
) -> Result<()> {
    let LightweightControlContext {
        sessions,
        global_session_id,
        provider_template,
        swarm_members,
        swarm_runs,
        client_connections,
        event_history,
        event_counter,
        swarm_event_tx,
        mcp_pool,
        soft_interrupt_queues,
        swarm_mutation_runtime,
    } = context;
    if let Request::Ping { id } = request {
        write_direct_event(
            &writer,
            &ServerEvent::Pong {
                id,
                native_ssh_protocol: Some(1),
            },
        )
        .await?;
        return Ok(());
    }

    write_direct_event(&writer, &ServerEvent::Ack { id: request.id() }).await?;

    let (client_event_tx, mut client_event_rx) = mpsc::unbounded_channel::<ServerEvent>();
    let writer_clone = Arc::clone(&writer);
    let event_handle = tokio::spawn(async move {
        while let Some(event) = client_event_rx.recv().await {
            if let Err(error) = write_direct_event(&writer_clone, &event).await {
                // Routine on client reload/disconnect; avoid dumping the full
                // event (an await response can embed whole completion reports).
                let event_desc = crate::logging::truncate_for_log(&format!("{:?}", event), 200);
                crate::logging::warn(&format!(
                    "lightweight control writer failed while sending {}: {}",
                    event_desc, error
                ));
                break;
            }
        }
    });

    match request {
        // Scheduled delivery opens a one-shot connection and names the target
        // session explicitly. Reuse its live agent, not a new subscribed agent.
        Request::NotifySession {
            id,
            session_id,
            message,
        } => {
            handle_notify_session(
                id,
                session_id,
                message,
                NotifySessionContext {
                    sessions,
                    soft_interrupt_queues,
                    client_connections,
                    swarm_members,
                    event_history,
                    event_counter,
                    swarm_event_tx,
                    client_event_tx: &client_event_tx,
                },
            )
            .await;
        }
        Request::CommMessage {
            id,
            from_session,
            message,
            to_session,
            wake,
        } => {
            handle_comm_message(
                id,
                from_session,
                message,
                to_session,
                wake,
                &client_event_tx,
                sessions,
                soft_interrupt_queues,
                swarm_members,
                event_history,
                event_counter,
                swarm_event_tx,
            )
            .await;
        }
        Request::CommSpawn {
            id,
            session_id: req_session_id,
            working_dir,
            initial_message,
            request_nonce,
            spawn_mode,
            model,
            label,
        } => {
            let spawn_mode = match parse_swarm_spawn_mode(id, spawn_mode, &client_event_tx) {
                Some(spawn_mode) => spawn_mode,
                None => return Ok(()),
            };
            handle_comm_spawn(
                id,
                req_session_id,
                working_dir,
                initial_message,
                request_nonce,
                spawn_mode,
                model,
                label,
                &client_event_tx,
                sessions,
                global_session_id,
                provider_template,
                swarm_members,
                swarm_runs,
                event_history,
                event_counter,
                swarm_event_tx,
                mcp_pool,
                soft_interrupt_queues,
                swarm_mutation_runtime,
                client_connections,
            )
            .await;
        }
        Request::CommListModels {
            id,
            session_id: req_session_id,
        } => {
            handle_comm_list_models(id, &req_session_id, sessions, provider_template, |event| {
                let _ = client_event_tx.send(event);
            })
            .await;
        }
        Request::CommStop {
            id,
            session_id: req_session_id,
            target_session,
        } => {
            handle_comm_stop(
                id,
                req_session_id,
                target_session,
                &client_event_tx,
                sessions,
                swarm_members,
                swarm_runs,
                event_history,
                event_counter,
                swarm_event_tx,
                soft_interrupt_queues,
                swarm_mutation_runtime,
            )
            .await;
        }

        other => {
            let _ = client_event_tx.send(ServerEvent::Error {
                id: other.id(),
                message: "unsupported lightweight control request".to_string(),
                retry_after_secs: None,
            });
        }
    }

    drop(client_event_tx);
    let _ = event_handle.await;
    Ok(())
}
