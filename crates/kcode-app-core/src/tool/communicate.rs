#![cfg_attr(test, allow(clippy::await_holding_lock))]

use super::{Tool, ToolContext, ToolOutput};
use crate::protocol::{Request, ServerEvent};
use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};

const REQUEST_ID: u64 = 1;

/// Every action this tool takes. One list, because the schema's `enum` and the
/// error a bad action gets both read it: a verb that goes cannot leave its name
/// behind in a message the model still reads.
const ACTIONS: &[&str] = &["message", "spawn", "stop", "list_models"];

mod transport;
use transport::send_request;

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
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    to_session: Option<String>,
    #[serde(default)]
    target_session: Option<String>,
    #[serde(default)]
    working_dir: Option<String>,
    #[serde(default)]
    initial_message: Option<String>,
    #[serde(default)]
    prompt: Option<String>,
    #[serde(default)]
    wake: Option<bool>,
    #[serde(default)]
    spawn_mode: Option<String>,
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
        "send" | "msg" | "send_message" | "dm_session" | "direct_message" | "whisper" => "message",
        "models" | "model_list" | "list_model" | "list_providers" | "list_routes" => "list_models",
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
                    "description": "Action. message hands work to one agent. spawn requires label and should include prompt. list_models shows available models/routes."
                },
                "message": {
                    "type": "string",
                    "description": "Message body, addressed with to_session/target_session."
                },
                "to_session": {
                    "type": "string",
                    "description": "Session id or unique friendly name of one agent. Alias of target_session."
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
                "wake": {
                    "type": "boolean",
                    "description": "Start a turn for the recipient now if it is idle (default true). Set false to leave the message as a notification."
                }
            }
        });

        // `swarm` is a multi-action tool, so putting `label` in the top-level
        // `required` array would incorrectly require it for message/list_models
        // and every other action. Use mutually exclusive action branches
        // instead: the spawn branch requires label, the rest do not.
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
            "message" => {
                // `message` hands work to exactly one session: the address is a
                // session (or a unique friendly name), never a group.
                let message = params
                    .message
                    .ok_or_else(|| anyhow::anyhow!("'message' is required for message action"))?;
                let to_session = params.to_session.clone().ok_or_else(|| {
                    anyhow::anyhow!(
                        "'to_session' (or 'target_session') is required for message action"
                    )
                })?;

                let request = Request::CommMessage {
                    id: REQUEST_ID,
                    from_session: ctx.session_id.clone(),
                    message: message.clone(),
                    to_session: to_session.clone(),
                    wake: params.wake,
                };

                match send_request(request).await {
                    Ok(response) => {
                        ensure_success(&response)?;
                        Ok(ToolOutput::new(format!(
                            "Message sent to {}: {}",
                            to_session, message
                        )))
                    }
                    Err(e) => Err(anyhow::anyhow!("Failed to send message: {}", e)),
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

            _ => Err(anyhow::anyhow!(
                "Unknown action '{}'. Valid actions: {}.",
                params.action,
                ACTIONS.join(", ")
            )),
        }
    }
}

#[cfg(test)]
#[path = "communicate_tests.rs"]
mod tests;
