use crate::agent::Agent;
use crate::protocol::ServerEvent;
use crate::provider::Provider;
use crate::server::{
    SessionInterruptQueues, SwarmMember, VersionedPlan, broadcast_swarm_status,
    register_background_tool_signal, register_session_interrupt_queue, swarm_id_for_dir,
};
use crate::tool::Registry;
use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{Mutex, RwLock};

type SessionAgents = Arc<RwLock<HashMap<String, Arc<Mutex<Agent>>>>>;

#[expect(
    clippy::too_many_arguments,
    reason = "headless session creation wires provider, global session, swarm state, interrupts, and MCP pool together"
)]
pub(super) async fn create_headless_session(
    sessions: &SessionAgents,
    global_session_id: &Arc<RwLock<String>>,
    provider_template: &Arc<dyn Provider>,
    command: &str,
    swarm_members: &Arc<RwLock<HashMap<String, SwarmMember>>>,
    swarms_by_id: &Arc<RwLock<HashMap<String, HashSet<String>>>>,
    swarm_coordinators: &Arc<RwLock<HashMap<String, String>>>,
    _swarm_plans: &Arc<RwLock<HashMap<String, VersionedPlan>>>,
    soft_interrupt_queues: &SessionInterruptQueues,
    selfdev_requested: bool,
    model_override: Option<String>,
    provider_key_override: Option<String>,
    route_api_method_override: Option<String>,
    effort_override: Option<String>,
    mcp_pool: Option<Arc<crate::mcp::SharedMcpPool>>,
    report_back_to_session_id: Option<String>,
) -> Result<String> {
    let memory_enabled = crate::config::config().features.memory;
    let swarm_enabled = crate::config::config().features.swarm;

    let working_dir = if let Some(path_str) = command.strip_prefix("create_session:") {
        let path_str = path_str.trim();
        if !path_str.is_empty() {
            Some(std::path::PathBuf::from(path_str))
        } else {
            None
        }
    } else {
        None
    };

    let provider = provider_template.fork();
    let registry = Registry::new(provider.clone()).await;

    registry.enable_memory_test_mode().await;

    if selfdev_requested {
        registry.register_selfdev_tools().await;
    }

    registry
        .register_mcp_tools_for_dir(
            None,
            mcp_pool,
            Some("headless".to_string()),
            working_dir.clone(),
        )
        .await;

    let working_dir_string = working_dir
        .as_ref()
        .map(|dir| dir.to_string_lossy().into_owned());
    let mut new_agent = Agent::new_with_initial_working_dir(
        Arc::clone(&provider),
        registry,
        working_dir_string.as_deref(),
    );
    new_agent.set_memory_enabled(memory_enabled);
    // Inline swarm mode renders a live gallery of worker viewports in the
    // coordinator TUI; enable the per-agent output tap so this worker streams a
    // throttled output tail onto the bus.
    if matches!(
        crate::config::config().agents.swarm_spawn_mode,
        crate::config::SwarmSpawnMode::Inline
    ) {
        new_agent.set_inline_output_tap(true);
    }
    if provider_key_override.is_some() {
        new_agent.set_session_provider_key(provider_key_override.clone());
    }
    let client_session_id = new_agent.session_id().to_string();

    if let Some(model) = model_override {
        // Build a model-switch request that preserves the coordinator's auth
        // route (e.g. claude-api vs claude-oauth, or an openai-compatible
        // profile) so the spawned headless agent reconstructs the exact
        // provider/auth the coordinator was using instead of a config default.
        let model_request = crate::provider::MultiProvider::model_switch_request_for_session_route(
            &model,
            provider_key_override.as_deref(),
            route_api_method_override.as_deref(),
        );
        if let Err(e) = new_agent.set_model(&model_request) {
            crate::logging::warn(&format!(
                "Failed to set headless session model override '{}' (request '{}'): {}",
                model, model_request, e
            ));
        }
    }

    if let Some(effort) = effort_override
        .as_deref()
        .map(str::trim)
        .filter(|effort| !effort.is_empty())
        && let Err(e) = new_agent.set_reasoning_effort(effort)
    {
        crate::logging::warn(&format!(
            "Failed to set headless session reasoning effort override '{}': {}",
            effort, e
        ));
    }

    new_agent.set_debug(true);

    if selfdev_requested {
        new_agent.set_canary("self-dev");
    }

    {
        let mut current = global_session_id.write().await;
        if current.is_empty() {
            *current = client_session_id.clone();
        }
    }

    let agent = Arc::new(Mutex::new(new_agent));
    {
        let mut sessions_guard = sessions.write().await;
        sessions_guard.insert(client_session_id.clone(), Arc::clone(&agent));
    }
    let (provider_model, provider_name, auth_method, effort) = {
        let agent_guard = agent.lock().await;
        register_session_interrupt_queue(
            soft_interrupt_queues,
            &client_session_id,
            agent_guard.soft_interrupt_queue(),
        )
        .await;
        register_background_tool_signal(&client_session_id, agent_guard.background_tool_signal());
        let route_api_method = agent_guard.session_route_api_method();
        let auth_method = agent_guard
            .active_resolved_credential()
            .map(|credential| credential.auth_method_label().to_string())
            .or_else(|| {
                route_api_method.as_deref().and_then(|route| {
                    let route = route.to_ascii_lowercase();
                    if route.contains("oauth") {
                        Some("OAuth".to_string())
                    } else if route.contains("api") || route.contains("compatible") {
                        Some("API key".to_string())
                    } else {
                        None
                    }
                })
            });
        (
            agent_guard.provider_model(),
            agent_guard.provider_name(),
            auth_method,
            crate::session_effort::session_effort(&client_session_id),
        )
    };

    let swarm_id = if swarm_enabled {
        swarm_id_for_dir(working_dir.clone())
    } else {
        None
    };
    let friendly_name = crate::id::extract_session_name(&client_session_id)
        .map(|s| s.to_string())
        .unwrap_or_else(|| client_session_id[..8.min(client_session_id.len())].to_string());

    let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel::<ServerEvent>();
    tokio::spawn(async move {
        while event_rx.recv().await.is_some() {
            // Drain events to keep channel alive
        }
    });

    {
        let now = Instant::now();
        let mut members = swarm_members.write().await;
        members.insert(
            client_session_id.clone(),
            SwarmMember {
                session_id: client_session_id.clone(),
                event_tx: event_tx.clone(),
                event_txs: HashMap::new(),
                working_dir: working_dir.clone(),
                swarm_id: swarm_id.clone(),
                swarm_enabled,
                status: "ready".to_string(),
                detail: None,
                task_label: None,
                friendly_name: Some(friendly_name.clone()),
                report_back_to_session_id: report_back_to_session_id.clone(),
                latest_completion_report: None,
                role: "agent".to_string(),
                joined_at: now,
                last_status_change: now,
                is_headless: true,
                output_tail: None,
                todo_progress: None,
                todo_items: Vec::new(),
                runtime: crate::protocol::SwarmMemberRuntime {
                    model: Some(provider_model),
                    provider: Some(provider_name),
                    auth_method,
                    effort,
                    elapsed_secs: Some(0),
                },
            },
        );
    }

    if let Some(ref id) = swarm_id {
        let mut swarms = swarms_by_id.write().await;
        swarms
            .entry(id.clone())
            .or_insert_with(HashSet::new)
            .insert(client_session_id.clone());
    }

    // Headless sessions never auto-claim coordinator; only TUI-connected sessions do.
    let is_new_coordinator = false;
    let _ = swarm_coordinators;
    if is_new_coordinator {
        let mut members = swarm_members.write().await;
        if let Some(m) = members.get_mut(&client_session_id) {
            m.role = "coordinator".to_string();
        }
    }

    if let Some(ref id) = swarm_id {
        broadcast_swarm_status(id, swarm_members, swarms_by_id).await;
    }

    Ok(serde_json::json!({
        "session_id": client_session_id,
        "working_dir": working_dir,
        "swarm_id": swarm_id,
        "friendly_name": friendly_name,
        "is_canary": selfdev_requested,
    })
    .to_string())
}
