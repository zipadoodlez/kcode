#![cfg_attr(test, allow(clippy::await_holding_lock))]

use super::{Tool, ToolContext, ToolOutput};
use crate::protocol::{
    AwaitedMemberStatus, CommDeliveryMode, ContextEntry, HistoryMessage, Request,
    ServerEvent, SwarmChannelInfo, default_comm_await_target_statuses,
    format_comm_awaited_members_with_reports, format_comm_channels, format_comm_context_entries,
    latest_assistant_comm_report,
};
use anyhow::Result;
use async_trait::async_trait;
use kcode_swarm_core::validate_swarm_tldr;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;

const REQUEST_ID: u64 = 1;

/// Every action this tool takes. One list, because the schema's `enum` and the
/// error a bad action gets both read it: a verb that goes cannot leave its name
/// behind in a message the model still reads.
const ACTIONS: &[&str] = &[
    "share",
    "share_append",
    "read",
    "message",
    "broadcast",
    "dm",
    "channel",
    "list_channels",
    "channel_members",
    "spawn",
    "stop",
    "subscribe_channel",
    "unsubscribe_channel",
    "await_members",
    "list_models",
];

mod transport;
use transport::{send_request, send_request_with_timeout};

fn check_error(response: &ServerEvent) -> Option<&str> {
    if let ServerEvent::Error { message, .. } = response {
        Some(message)
    } else {
        None
    }
}

fn ensure_success(response: &ServerEvent) -> Result<()> {
    if let Some(message) = check_error(response) {
        Err(anyhow::anyhow!(message.to_string()))
    } else {
        Ok(())
    }
}

fn format_context_entries(entries: &[ContextEntry]) -> ToolOutput {
    ToolOutput::new(format_comm_context_entries(entries))
}

#[cfg(test)]
fn format_awaited_members(
    completed: bool,
    summary: &str,
    members: &[AwaitedMemberStatus],
) -> ToolOutput {
    format_awaited_members_with_reports(completed, summary, members, &HashMap::new())
}

fn latest_assistant_report(messages: &[HistoryMessage]) -> Option<String> {
    latest_assistant_comm_report(messages)
}

fn format_awaited_members_with_reports(
    completed: bool,
    summary: &str,
    members: &[AwaitedMemberStatus],
    reports: &HashMap<String, String>,
) -> ToolOutput {
    ToolOutput::new(format_comm_awaited_members_with_reports(
        completed, summary, members, reports,
    ))
}

async fn fetch_awaited_member_reports(
    ctx: &ToolContext,
    members: &[AwaitedMemberStatus],
) -> HashMap<String, String> {
    let mut reports = HashMap::new();
    for member in members.iter().filter(|member| member.done) {
        let request = Request::CommReadContext {
            id: REQUEST_ID,
            session_id: ctx.session_id.clone(),
            target_session: member.session_id.clone(),
        };
        match send_request(request).await {
            Ok(ServerEvent::CommContextHistory { messages, .. }) => {
                if let Some(report) = latest_assistant_report(&messages) {
                    reports.insert(member.session_id.clone(), report);
                }
            }
            Ok(response) => {
                if check_error(&response).is_some() {
                    continue;
                }
            }
            Err(_) => continue,
        }
    }
    reports
}

fn default_await_target_statuses() -> Vec<String> {
    default_comm_await_target_statuses()
}

fn format_channels(channels: &[SwarmChannelInfo]) -> ToolOutput {
    ToolOutput::new(format_comm_channels(channels))
}

/// Render the swarm model catalog for the `list_models` action: the current
/// (spawn-default) model, any config pin, and one line per route with
/// availability, auth method, and a relative cost estimate.
fn format_swarm_model_list(
    current_model: Option<&str>,
    configured_swarm_model: Option<&str>,
    model_routes: &[kcode_provider_core::ModelRoute],
) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "Current coordinator model: {}\n",
        current_model.unwrap_or("unknown")
    ));
    match configured_swarm_model {
        Some(pin) if !pin.trim().is_empty() => {
            out.push_str(&format!("Configured agents.swarm_model default: {pin}\n"));
        }
        _ => out.push_str(
            "No agents.swarm_model default configured (workers inherit the coordinator's model unless model is passed).\n",
        ),
    }
    if model_routes.is_empty() {
        out.push_str("\nNo model routes reported. Omit model to use the configured default, or pass inherit to use the coordinator.");
        return out;
    }
    out.push_str("\nAvailable model routes (pass model with a bare model or route-pinned value to override the configured default):\n");
    for route in model_routes {
        let availability = if route.available {
            ""
        } else {
            " [unavailable]"
        };
        let cost = route
            .estimated_reference_cost_micros()
            .map(|micros| format!(" ~${:.2}/ref-task", micros as f64 / 1_000_000.0))
            .unwrap_or_default();
        let detail = if route.detail.is_empty() {
            String::new()
        } else {
            format!(" ({})", route.detail)
        };
        out.push_str(&format!(
            "- {} via {} [{}]{}{}{}\n",
            route.model, route.provider, route.api_method, availability, cost, detail
        ));
    }
    out
}

pub struct CommunicateTool {
    /// Full tool description including the user-tunable swarm prompt
    /// (model-routing guidance loaded from `swarm-prompt.md`). Computed once at
    /// registry construction so `description()` can hand out a borrowed str.
    description: String,
}

impl CommunicateTool {
    pub fn new() -> Self {
        Self::new_for_working_dir(None)
    }

    fn new_for_working_dir(working_dir: Option<&std::path::Path>) -> Self {
        const BASE_DESCRIPTION: &str =
            "Coordinate agents: spawn workers with a prompt, message them, and manage swarm plans.";
        let swarm_prompt = crate::prompt::load_swarm_prompt(working_dir);
        let description = if swarm_prompt.is_empty() {
            BASE_DESCRIPTION.to_string()
        } else {
            format!(
                "{BASE_DESCRIPTION}\n\nSwarm prompt (user-tunable via ~/.kcode/swarm-prompt.md):\n{swarm_prompt}"
            )
        };
        Self { description }
    }
}

#[derive(Clone, Deserialize)]
struct CommunicateInput {
    action: String,
    key: Option<String>,
    value: Option<String>,
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    to_session: Option<String>,
    #[serde(default)]
    channel: Option<String>,
    #[serde(default)]
    target_session: Option<String>,
    #[serde(default)]
    working_dir: Option<String>,
    #[serde(default)]
    initial_message: Option<String>,
    #[serde(default)]
    prompt: Option<String>,
    #[serde(default)]
    target_status: Option<Vec<String>>,
    #[serde(default)]
    session_ids: Option<Vec<String>>,
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    timeout_minutes: Option<u64>,
    #[serde(default)]
    wake: Option<bool>,
    #[serde(default)]
    background: Option<bool>,
    #[serde(default)]
    notify: Option<bool>,
    #[serde(default)]
    delivery: Option<CommDeliveryMode>,
    #[serde(default)]
    spawn_mode: Option<String>,
    /// One-line summary shown collapsed in the recipient's UI for long
    /// message/report bodies. Required when the body exceeds the collapse
    /// threshold.
    #[serde(default)]
    tldr: Option<String>,
    /// Per-worker model override for spawn and assignment-created workers.
    /// Takes precedence over agents.swarm_model; see list_models for routes.
    #[serde(default)]
    model: Option<String>,
    /// Short human-readable label for a spawned agent shown in swarm UI.
    /// Required and nonblank for the explicit `spawn` action.
    #[serde(default)]
    label: Option<String>,
}

impl CommunicateInput {
    fn spawn_initial_message(&self) -> Option<String> {
        self.initial_message
            .as_ref()
            .filter(|message| !message.trim().is_empty())
            .cloned()
            .or_else(|| {
                self.prompt
                    .as_ref()
                    .filter(|prompt| !prompt.trim().is_empty())
                    .cloned()
            })
    }

    fn required_spawn_label(&self) -> anyhow::Result<String> {
        let label = self
            .label
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("'label' is required for spawn action"))?
            .trim();
        if label.is_empty() {
            return Err(anyhow::anyhow!(
                "'label' must not be blank for spawn action"
            ));
        }
        Ok(label.to_string())
    }
}

/// Map common action synonyms/typos to the canonical swarm action name. Models
/// frequently invent near-miss verbs (e.g. `inbox` for reading messages, `send`
/// for `message`), which previously produced an "Unknown action" error. Unknown
/// inputs are returned unchanged so the normal validation path still reports them.
fn canonical_swarm_action(action: &str) -> &str {
    match action.trim().to_ascii_lowercase().as_str() {
        "inbox" | "messages" | "check_messages" | "read_messages" | "read_inbox" => "read",
        "send" | "msg" | "send_message" => "message",
        "dm_session" | "direct_message" | "whisper" => "dm",
        "broadcast_all" | "announce" => "broadcast",
        "agents" | "members" | "list_agents" | "list_members" | "roster" => "list",
        "models" | "model_list" | "list_model" | "list_providers" | "list_routes" => "list_models",
        "plan" | "status_plan" => "plan_status",
        "kill" | "terminate" => "stop",
        _ => action,
    }
}

#[async_trait]
impl Tool for CommunicateTool {
    fn name(&self) -> &str {
        "swarm"
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn parameters_schema(&self) -> Value {
        let mut schema = json!({
            "type": "object",
            "required": ["action"],
            "properties": {
                "intent": super::intent_schema_property(),
                "action": {
                    "type": "string",
                    "enum": ACTIONS,
                    "description": "Action. spawn requires label and should include prompt. list_models shows available models/routes."
                },
                "key": {
                    "type": "string",
                    "description": "Shared-context key for share/share_append/read. Discouraged: prefer the repo and node artifacts."
                },
                "value": {
                    "type": "string"
                },
                "message": {
                    "type": "string",
                    "description": "Message body: DM with to_session, channel post with channel, else broadcast."
                },
                "tldr": {
                    "type": "string",
                    "description": "One-line summary under ~120 chars. Required for message bodies longer than 240 chars."
                },
                "to_session": {
                    "type": "string",
                    "description": "Session ID or unique friendly name of one agent. Alias of target_session."
                },
                "channel": {
                    "type": "string",
                    "description": "Channel name for channel actions. Discouraged: prefer DMs and task-graph artifacts."
                },
                "target_session": {
                    "type": "string",
                    "description": "Session id or unique friendly name. For stop: naming a run's root ends the run; naming a member takes that member and everything it spawned. Alias of to_session."
                },
                "label": {
                    "type": "string",
                    "minLength": 1,
                    "description": "Required for spawn. Short label shown on the agent's chip (e.g. 'api reviewer')."
                },
                "working_dir": {
                    "type": "string",
                    "description": "Optional working directory for spawn."
                },
                "prompt": {
                    "type": "string",
                    "description": "Initial task/instructions for spawn. Spawning without it creates an idle agent."
                },
                "initial_message": {
                    "type": "string",
                    "description": "Alias of prompt for spawn; wins when both are set."
                },
                "spawn_mode": {
                    "type": "string",
                    "enum": ["visible", "headless", "inline", "auto"],
                    "description": "Spawn UI mode: visible terminal, headless, inline gallery, or auto. Defaults to inline."
                },
                "model": {
                    "type": "string",
                    "description": "Model for a spawned worker, e.g. 'gpt-6-astra' or 'openai-api:gpt-5.6-luna'. Overrides agents.swarm_model. Omit to use that default or inherit the coordinator if unset. Use 'inherit' to force the coordinator's model and route. See list_models."
                },
                "session_ids": {
                    "type": "array",
                    "items": {"type": "string"}
                },
                "mode": {
                    "type": "string",
                    "enum": ["all", "any"],
                    "description": "await_members: all or any."
                },
                "target_status": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Optional completion statuses for await_members. Defaults to ready/completed/stopped/failed."
                },
                "timeout_minutes": {
                    "type": "integer",
                    "minimum": 1,
                    "description": "Optional timeout for await_members."
                },
                "notify": {
                    "type": "boolean",
                    "description": "For await_members/run_plan: show a notification when resolved. Defaults to true."
                },
                "wake": {
                    "type": "boolean",
                    "description": "Wake this agent when a message or awaited background task resolves (default true)."
                },
                "delivery": {
                    "type": "string",
                    "enum": ["notify", "interrupt", "wake"],
                    "description": "Optional delivery mode for dm/channel messaging."
                }
            }
        });

        // Task-DAG properties are added after the macro to keep `json!` nesting
        // depth under the macro recursion limit.
        // `swarm` is a multi-action tool, so putting `label` in the top-level
        // `required` array would incorrectly require it for read/list/message and
        // every other action. Use mutually exclusive action branches instead:
        // the spawn branch requires label, while the non-spawn branch does not.
        // `anyOf` object branches are supported by our provider schema adapters
        // and avoid the less-portable JSON Schema `if`/`then` keywords.
        let non_spawn_actions: Vec<Value> = schema["properties"]["action"]["enum"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|action| action.as_str() != Some("spawn"))
            .cloned()
            .collect();
        schema["anyOf"] = json!([
            {
                "type": "object",
                "required": ["action", "label"],
                "properties": {
                    "action": { "type": "string", "enum": ["spawn"] },
                    // Gemini validates that every `required` name is defined in
                    // the same object's `properties` and rejects the whole
                    // request otherwise (issue #655), so declare `label` here
                    // instead of relying on the parent schema's declaration.
                    "label": { "type": "string", "minLength": 1 }
                }
            },
            {
                "type": "object",
                "required": ["action"],
                "properties": {
                    "action": { "type": "string", "enum": non_spawn_actions }
                }
            }
        ]);

        schema
    }

    async fn execute(&self, input: Value, ctx: ToolContext) -> Result<ToolOutput> {
        let mut params: CommunicateInput = serde_json::from_value(input)?;

        // `to_session` and `target_session` both name a single session id. Historically
        // different actions required different field names (e.g. `dm` wanted `to_session`
        // while `summary`/`status`/`start`/`resume` wanted `target_session`),
        // which models frequently confuse, producing repeated "'to_session' is required" /
        // "'target_session' is required" errors. Treat the two fields as interchangeable
        // aliases so either name works for any action that targets a session.
        match (params.to_session.is_some(), params.target_session.is_some()) {
            (true, false) => params.target_session = params.to_session.clone(),
            (false, true) => params.to_session = params.target_session.clone(),
            _ => {}
        }

        // Normalize common action synonyms that models invent (e.g. `inbox`, `send`,
        // `msg`) so a near-miss verb maps to the real action instead of erroring out.
        params.action = canonical_swarm_action(&params.action).to_string();

        match params.action.as_str() {
            "share" | "share_append" => {
                let key = params
                    .key
                    .ok_or_else(|| anyhow::anyhow!("'key' is required for share action"))?;
                let value = params
                    .value
                    .ok_or_else(|| anyhow::anyhow!("'value' is required for share action"))?;

                let request = Request::CommShare {
                    id: REQUEST_ID,
                    session_id: ctx.session_id.clone(),
                    key: key.clone(),
                    value: value.clone(),
                    append: params.action == "share_append",
                };

                match send_request(request).await {
                    Ok(response) => {
                        ensure_success(&response)?;
                        let verb = if params.action == "share_append" {
                            "Appended shared context"
                        } else {
                            "Shared with other agents"
                        };
                        Ok(ToolOutput::new(format!("{}: {} = {}", verb, key, value)))
                    }
                    Err(e) => Err(anyhow::anyhow!("Failed to share: {}", e)),
                }
            }

            "read" => {
                let request = Request::CommRead {
                    id: REQUEST_ID,
                    session_id: ctx.session_id.clone(),
                    key: params.key.clone(),
                };

                match send_request(request).await {
                    Ok(ServerEvent::CommContext { entries, .. }) => {
                        Ok(format_context_entries(&entries))
                    }
                    Ok(response) => {
                        ensure_success(&response)?;
                        Ok(ToolOutput::new("No shared context found."))
                    }
                    Err(e) => Err(anyhow::anyhow!("Failed to read shared context: {}", e)),
                }
            }

            "message" => {
                // `message` is the general-purpose send: it routes by the fields
                // provided. With `to_session` it acts as a DM, with `channel` it
                // posts to that channel, and with neither it broadcasts to the
                // sender's spawned subtree (whole swarm only for the coordinator).
                let message = params
                    .message
                    .ok_or_else(|| anyhow::anyhow!("'message' is required for message action"))?;
                let tldr = validate_swarm_tldr(params.tldr.as_deref(), &message, "this message")
                    .map_err(|e| anyhow::anyhow!(e))?;
                let to_session = params.to_session.clone();
                let channel = params.channel.clone();

                let request = Request::CommMessage {
                    id: REQUEST_ID,
                    from_session: ctx.session_id.clone(),
                    message: message.clone(),
                    to_session: to_session.clone(),
                    channel: channel.clone(),
                    wake: params.wake,
                    delivery: params.delivery,
                    tldr,
                };

                match send_request(request).await {
                    Ok(response) => {
                        ensure_success(&response)?;
                        let confirmation = match (to_session, channel) {
                            (Some(target), _) => {
                                format!("Direct message sent to {}: {}", target, message)
                            }
                            (None, Some(channel)) => {
                                format!("Channel message sent to #{}: {}", channel, message)
                            }
                            (None, None) => {
                                format!("Broadcast sent to your spawned subtree: {}", message)
                            }
                        };
                        Ok(ToolOutput::new(confirmation))
                    }
                    Err(e) => Err(anyhow::anyhow!("Failed to send message: {}", e)),
                }
            }

            "broadcast" => {
                // `broadcast` targets the sender's spawned subtree (the swarm
                // coordinator reaches the whole swarm). Any `to_session`/
                // `channel` is intentionally ignored so the action stays an
                // unambiguous group send; use `message`/`dm`/`channel` to target.
                // Prefer DMs or task-graph artifacts; group sends are for rare
                // coordination moments, not routine status updates.
                let message = params
                    .message
                    .ok_or_else(|| anyhow::anyhow!("'message' is required for broadcast action"))?;
                let tldr = validate_swarm_tldr(params.tldr.as_deref(), &message, "this broadcast")
                    .map_err(|e| anyhow::anyhow!(e))?;

                let request = Request::CommMessage {
                    id: REQUEST_ID,
                    from_session: ctx.session_id.clone(),
                    message: message.clone(),
                    to_session: None,
                    channel: None,
                    wake: params.wake,
                    delivery: params.delivery,
                    tldr,
                };

                match send_request(request).await {
                    Ok(response) => {
                        ensure_success(&response)?;
                        Ok(ToolOutput::new(format!(
                            "Broadcast sent to your spawned subtree: {}",
                            message
                        )))
                    }
                    Err(e) => Err(anyhow::anyhow!("Failed to broadcast message: {}", e)),
                }
            }

            "dm" => {
                let message = params
                    .message
                    .ok_or_else(|| anyhow::anyhow!("'message' is required for dm action"))?;
                let tldr = validate_swarm_tldr(params.tldr.as_deref(), &message, "this DM")
                    .map_err(|e| anyhow::anyhow!(e))?;
                let to_session = params.to_session.ok_or_else(|| {
                    anyhow::anyhow!("'to_session' (or 'target_session') is required for dm action")
                })?;

                let request = Request::CommMessage {
                    id: REQUEST_ID,
                    from_session: ctx.session_id.clone(),
                    message: message.clone(),
                    to_session: Some(to_session.clone()),
                    channel: None,
                    delivery: params.delivery,
                    wake: params.wake,
                    tldr,
                };

                match send_request(request).await {
                    Ok(response) => {
                        ensure_success(&response)?;
                        Ok(ToolOutput::new(format!(
                            "Direct message sent to {}: {}",
                            to_session, message
                        )))
                    }
                    Err(e) => Err(anyhow::anyhow!("Failed to send DM: {}", e)),
                }
            }

            "channel" => {
                let message = params
                    .message
                    .ok_or_else(|| anyhow::anyhow!("'message' is required for channel action"))?;
                let tldr =
                    validate_swarm_tldr(params.tldr.as_deref(), &message, "this channel message")
                        .map_err(|e| anyhow::anyhow!(e))?;
                let channel = params
                    .channel
                    .ok_or_else(|| anyhow::anyhow!("'channel' is required for channel action"))?;

                let request = Request::CommMessage {
                    id: REQUEST_ID,
                    from_session: ctx.session_id.clone(),
                    message: message.clone(),
                    to_session: None,
                    channel: Some(channel.clone()),
                    delivery: params.delivery,
                    wake: params.wake,
                    tldr,
                };

                match send_request(request).await {
                    Ok(response) => {
                        ensure_success(&response)?;
                        Ok(ToolOutput::new(format!(
                            "Channel message sent to #{}: {}",
                            channel, message
                        )))
                    }
                    Err(e) => Err(anyhow::anyhow!("Failed to send channel message: {}", e)),
                }
            }

            "list_channels" => {
                let request = Request::CommListChannels {
                    id: REQUEST_ID,
                    session_id: ctx.session_id.clone(),
                };

                match send_request(request).await {
                    Ok(ServerEvent::CommChannels { channels, .. }) => {
                        Ok(format_channels(&channels))
                    }
                    Ok(response) => {
                        ensure_success(&response)?;
                        Ok(ToolOutput::new("No channels found."))
                    }
                    Err(e) => Err(anyhow::anyhow!("Failed to list channels: {}", e)),
                }
            }

            "channel_members" => {
                let channel = params.channel.ok_or_else(|| {
                    anyhow::anyhow!("'channel' is required for channel_members action")
                })?;
                let request = Request::CommChannelMembers {
                    id: REQUEST_ID,
                    session_id: ctx.session_id.clone(),
                    channel: channel.clone(),
                };

                match send_request(request).await {
                    Ok(ServerEvent::CommMembers { members, .. }) => {
                        let mut output = format!("Members subscribed to #{}:\n\n", channel);
                        if members.is_empty() {
                            output.push_str("  (none)\n");
                        } else {
                            for member in members {
                                let name = member.friendly_name.unwrap_or(member.session_id);
                                let status = member
                                    .status
                                    .as_ref()
                                    .map_or("unknown", |s| s.as_str())
                                    .to_string();
                                output.push_str(&format!("  {} ({})\n", name, status));
                            }
                        }
                        Ok(ToolOutput::new(output))
                    }
                    Ok(response) => {
                        ensure_success(&response)?;
                        Ok(ToolOutput::new("No channel members found."))
                    }
                    Err(e) => Err(anyhow::anyhow!("Failed to list channel members: {}", e)),
                }
            }

            "spawn" => {
                let label = params.required_spawn_label()?;
                let request = Request::CommSpawn {
                    id: REQUEST_ID,
                    session_id: ctx.session_id.clone(),
                    working_dir: params.working_dir.clone(),
                    initial_message: params.spawn_initial_message(),
                    request_nonce: None,
                    spawn_mode: params.spawn_mode.clone(),
                    model: params.model.clone(),
                    label: Some(label),
                };

                match send_request(request).await {
                    Ok(ServerEvent::CommSpawnResponse { new_session_id, .. })
                        if !new_session_id.is_empty() =>
                    {
                        Ok(ToolOutput::new(format!(
                            "Spawned new agent: {}",
                            new_session_id
                        )))
                    }
                    Ok(response) => {
                        ensure_success(&response)?;
                        Err(anyhow::anyhow!(
                            "Spawn succeeded but new session ID was not returned."
                        ))
                    }
                    Err(e) => Err(anyhow::anyhow!("Failed to spawn agent: {}", e)),
                }
            }

            "list_models" => {
                let request = Request::CommListModels {
                    id: REQUEST_ID,
                    session_id: ctx.session_id.clone(),
                };
                match send_request(request).await {
                    Ok(ServerEvent::CommListModelsResponse {
                        current_model,
                        configured_swarm_model,
                        model_routes,
                        ..
                    }) => Ok(ToolOutput::new(format_swarm_model_list(
                        current_model.as_deref(),
                        configured_swarm_model.as_deref(),
                        &model_routes,
                    ))),
                    Ok(response) => {
                        ensure_success(&response)?;
                        Ok(ToolOutput::new("No model catalog returned."))
                    }
                    Err(e) => Err(anyhow::anyhow!("Failed to list models: {}", e)),
                }
            }

            "stop" => {
                let target = params.target_session.ok_or_else(|| {
                    anyhow::anyhow!("'target_session' is required for stop action")
                })?;

                let request = Request::CommStop {
                    id: REQUEST_ID,
                    session_id: ctx.session_id.clone(),
                    target_session: target.clone(),
                };

                match send_request(request).await {
                    Ok(response) => {
                        ensure_success(&response)?;
                        Ok(ToolOutput::new(format!("Stopped {}.", target)))
                    }
                    Err(e) => Err(anyhow::anyhow!("Failed to stop agent: {}", e)),
                }
            }

            "subscribe_channel" => {
                let channel = params.channel.ok_or_else(|| {
                    anyhow::anyhow!("'channel' is required for subscribe_channel action")
                })?;

                let request = Request::CommSubscribeChannel {
                    id: REQUEST_ID,
                    session_id: ctx.session_id.clone(),
                    channel: channel.clone(),
                };

                match send_request(request).await {
                    Ok(response) => {
                        ensure_success(&response)?;
                        Ok(ToolOutput::new(format!("Subscribed to #{}", channel)))
                    }
                    Err(e) => Err(anyhow::anyhow!("Failed to subscribe: {}", e)),
                }
            }

            "unsubscribe_channel" => {
                let channel = params.channel.ok_or_else(|| {
                    anyhow::anyhow!("'channel' is required for unsubscribe_channel action")
                })?;

                let request = Request::CommUnsubscribeChannel {
                    id: REQUEST_ID,
                    session_id: ctx.session_id.clone(),
                    channel: channel.clone(),
                };

                match send_request(request).await {
                    Ok(response) => {
                        ensure_success(&response)?;
                        Ok(ToolOutput::new(format!("Unsubscribed from #{}", channel)))
                    }
                    Err(e) => Err(anyhow::anyhow!("Failed to unsubscribe: {}", e)),
                }
            }

            "await_members" => {
                let target_status = params
                    .target_status
                    .unwrap_or_else(default_await_target_statuses);
                let mut session_ids = params.session_ids.unwrap_or_default();
                if let Some(target_session) = params.target_session.clone()
                    && !session_ids.iter().any(|id| id == &target_session)
                {
                    session_ids.push(target_session);
                }
                let timeout_minutes = params.timeout_minutes.unwrap_or(60);
                let timeout_secs = timeout_minutes * 60;
                // Public member waits are always asynchronous. The blocking
                // CommAwaitMembers protocol remains available internally for the
                // run_plan coordination loop, but agents must not park an entire
                // turn waiting on a worker or a long-lived socket.
                let blocking_was_requested = params.background == Some(false);
                let background = true;
                let notify = params.notify.unwrap_or(true);
                let wake = params.wake.unwrap_or(true);

                let request = Request::CommAwaitMembers {
                    id: REQUEST_ID,
                    session_id: ctx.session_id.clone(),
                    target_status,
                    session_ids,
                    mode: params.mode.clone(),
                    timeout_secs: Some(timeout_secs),
                    background,
                    notify,
                    wake,
                };

                // Background waits return promptly with a snapshot; only blocking
                // waits need the long socket timeout that covers the full wait.
                let socket_timeout = if background {
                    std::time::Duration::from_secs(30)
                } else {
                    std::time::Duration::from_secs(timeout_secs + 30)
                };

                match send_request_with_timeout(request, Some(socket_timeout)).await {
                    Ok(ServerEvent::CommAwaitMembersResponse {
                        completed,
                        members,
                        summary,
                        background_started,
                        ..
                    }) => {
                        if background_started {
                            let compatibility_note = if blocking_was_requested {
                                "\n\n(Blocking member waits are no longer supported; this wait was started asynchronously.)"
                            } else {
                                "\n\n(You can keep working; this wait runs in the background.)"
                            };
                            return Ok(ToolOutput::new(format!(
                                "{}{}",
                                summary, compatibility_note
                            )));
                        }
                        let reports = fetch_awaited_member_reports(&ctx, &members).await;
                        Ok(format_awaited_members_with_reports(
                            completed, &summary, &members, &reports,
                        ))
                    }
                    Ok(response) => {
                        ensure_success(&response)?;
                        Ok(ToolOutput::new("Await completed."))
                    }
                    Err(e) => Err(anyhow::anyhow!("Failed to await members: {}", e)),
                }
            }

            _ => Err(anyhow::anyhow!(
                "Unknown action '{}'. Valid actions: {}. \
                 To read messages addressed to you, use action='read'.",
                params.action,
                ACTIONS.join(", ")
            )),
        }
    }
}

#[cfg(test)]
#[path = "communicate_tests.rs"]
mod tests;
