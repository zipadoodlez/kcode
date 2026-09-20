use anyhow::Result;
use async_trait::async_trait;
use jcode_agent_runtime::InterruptSignal;
use jcode_message_types::ToolDefinition;
use serde_json::Value;
use std::path::{Path, PathBuf};

pub const TOOL_INTENT_DESCRIPTION: &str =
    "Required short label shown in the UI: why this call is being made.";

/// Input key a caller sets to accept the token cost of an oversized result.
///
/// The context guard withholds any tool result too large for the remaining
/// context and states its token cost. Setting this repeats the call and spends
/// that cost deliberately. Kept in sync with the registry constant of the same
/// name, which reads the flag off raw tool input.
pub const ACCEPT_LARGE_OUTPUT_KEY: &str = "accept_large_output";

/// Deliberately terse: this rides on every tool schema on every request, so
/// each word is paid forever. The full explanation lives in the refusal
/// message, which is only ever shown when it is actually relevant.
pub const ACCEPT_LARGE_OUTPUT_DESCRIPTION: &str =
    "Re-run accepting the stated token cost of a withheld result.";

pub fn intent_schema_property() -> Value {
    serde_json::json!({
        "type": "string",
        "description": TOOL_INTENT_DESCRIPTION,
    })
}

pub fn accept_large_output_schema_property() -> Value {
    serde_json::json!({
        "type": "boolean",
        "description": ACCEPT_LARGE_OUTPUT_DESCRIPTION,
    })
}

/// Ensure a tool parameter schema declares the shared `intent` property and
/// marks it required. Applied centrally when converting tools to provider
/// definitions so every tool (including MCP proxies) asks the model for an
/// intent without each tool wiring it manually.
///
/// The optional `accept_large_output` escape hatch is added the same way. Any
/// tool can produce a result too large to return, so documenting it per tool
/// would mean editing dozens of schemas and missing MCP proxies entirely.
pub fn ensure_intent_in_schema(mut schema: Value) -> Value {
    let Some(object) = schema.as_object_mut() else {
        return schema;
    };
    // Only touch object-shaped parameter schemas.
    let is_object_schema = object
        .get("type")
        .and_then(|t| t.as_str())
        .map(|t| t == "object")
        .unwrap_or_else(|| object.contains_key("properties"));
    if !is_object_schema {
        return schema;
    }

    let properties = object
        .entry("properties")
        .or_insert_with(|| Value::Object(serde_json::Map::new()));
    if let Some(properties) = properties.as_object_mut() {
        properties
            .entry("intent")
            .or_insert_with(intent_schema_property);
        // Optional, so it is deliberately not added to `required`.
        properties
            .entry(ACCEPT_LARGE_OUTPUT_KEY)
            .or_insert_with(accept_large_output_schema_property);
    } else {
        return schema;
    }

    match object.get_mut("required") {
        Some(Value::Array(required)) => {
            if !required.iter().any(|v| v.as_str() == Some("intent")) {
                required.push(Value::String("intent".to_string()));
            }
        }
        _ => {
            object.insert(
                "required".to_string(),
                Value::Array(vec![Value::String("intent".to_string())]),
            );
        }
    }

    schema
}

/// A request for stdin input from a running command.
pub struct StdinInputRequest {
    pub request_id: String,
    pub prompt: String,
    pub is_password: bool,
    pub response_tx: tokio::sync::oneshot::Sender<String>,
}

#[derive(Clone)]
pub struct ToolContext {
    pub session_id: String,
    pub message_id: String,
    pub tool_call_id: String,
    pub working_dir: Option<PathBuf>,
    pub stdin_request_tx: Option<tokio::sync::mpsc::UnboundedSender<StdinInputRequest>>,
    pub graceful_shutdown_signal: Option<InterruptSignal>,
    pub execution_mode: ToolExecutionMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolExecutionMode {
    AgentTurn,
    Direct,
}

impl ToolContext {
    pub fn for_subcall(&self, tool_call_id: String) -> Self {
        Self {
            session_id: self.session_id.clone(),
            message_id: self.message_id.clone(),
            tool_call_id,
            working_dir: self.working_dir.clone(),
            stdin_request_tx: self.stdin_request_tx.clone(),
            graceful_shutdown_signal: self.graceful_shutdown_signal.clone(),
            execution_mode: self.execution_mode,
        }
    }

    pub fn resolve_path(&self, path: &Path) -> PathBuf {
        if path.is_absolute() {
            path.to_path_buf()
        } else if let Some(ref base) = self.working_dir {
            base.join(path)
        } else {
            path.to_path_buf()
        }
    }
}

/// A tool that can be executed by the agent.
#[async_trait]
pub trait Tool: Send + Sync {
    /// Tool name (must match what's sent to the API).
    fn name(&self) -> &str;

    /// Original MCP identity, independent of the provider-facing registry alias.
    fn mcp_identity(&self) -> Option<(&str, &str)> {
        None
    }

    /// Human-readable description.
    fn description(&self) -> &str;

    /// JSON Schema for the input parameters.
    fn parameters_schema(&self) -> Value;

    /// Execute the tool with the given input.
    async fn execute(&self, input: Value, ctx: ToolContext) -> Result<ToolOutput>;

    /// Convert to API tool definition.
    fn to_definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: self.name().to_string(),
            description: self.description().to_string(),
            input_schema: ensure_intent_in_schema(self.parameters_schema()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ensure_intent_adds_property_and_required() {
        let schema = serde_json::json!({
            "type": "object",
            "required": ["command"],
            "properties": {
                "command": {"type": "string"}
            }
        });
        let out = ensure_intent_in_schema(schema);
        assert!(out["properties"]["intent"].is_object());
        let required: Vec<_> = out["required"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str())
            .collect();
        assert!(required.contains(&"command"));
        assert!(required.contains(&"intent"));
    }

    #[test]
    fn ensure_intent_creates_required_array_when_missing() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {}
        });
        let out = ensure_intent_in_schema(schema);
        assert_eq!(out["required"], serde_json::json!(["intent"]));
    }

    #[test]
    fn ensure_intent_preserves_existing_intent_property() {
        let schema = serde_json::json!({
            "type": "object",
            "required": ["intent"],
            "properties": {
                "intent": {"type": "string", "description": "custom"}
            }
        });
        let out = ensure_intent_in_schema(schema);
        assert_eq!(out["properties"]["intent"]["description"], "custom");
        assert_eq!(
            out["required"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|v| v.as_str() == Some("intent"))
                .count(),
            1
        );
    }

    #[test]
    fn ensure_intent_skips_non_object_schemas() {
        let schema = serde_json::json!({"type": "string"});
        let out = ensure_intent_in_schema(schema.clone());
        assert_eq!(out, schema);
    }
}

#[cfg(test)]
mod escape_hatch_tests {
    use super::*;

    #[test]
    fn injects_the_escape_hatch_into_any_object_schema() {
        // MCP tools are built from remote definitions and never edit their own
        // schemas, so they can only advertise the flag if injection is central.
        // A schema shaped like an MCP proxy's proves the mechanism.
        let mcp_shaped = serde_json::json!({
            "type": "object",
            "required": ["path"],
            "properties": { "path": { "type": "string" } }
        });
        let out = ensure_intent_in_schema(mcp_shaped);
        assert_eq!(
            out["properties"][ACCEPT_LARGE_OUTPUT_KEY]["type"], "boolean",
            "every object schema must advertise the escape hatch"
        );
        // Optional by design: requiring it would make the model answer a token
        // budget question on every call.
        let required: Vec<&str> = out["required"]
            .as_array()
            .expect("required array")
            .iter()
            .filter_map(|v| v.as_str())
            .collect();
        assert!(required.contains(&"intent"));
        assert!(!required.contains(&ACCEPT_LARGE_OUTPUT_KEY));
    }

    #[test]
    fn never_overwrites_a_schema_that_declares_the_flag_itself() {
        let custom = serde_json::json!({
            "type": "object",
            "properties": {
                ACCEPT_LARGE_OUTPUT_KEY: { "type": "boolean", "description": "custom" }
            }
        });
        let out = ensure_intent_in_schema(custom);
        assert_eq!(
            out["properties"][ACCEPT_LARGE_OUTPUT_KEY]["description"], "custom",
            "a tool's own declaration must survive injection"
        );
    }

    #[test]
    fn the_schema_key_matches_what_the_guard_reads() {
        // The registry reads this exact constant off raw tool input. If the two
        // ever diverge, the flag would be advertised but never honored, which is
        // worse than not offering it at all.
        assert_eq!(ACCEPT_LARGE_OUTPUT_KEY, "accept_large_output");
    }
}

// --- folded in from jcode-tool-types: tool output shapes and name aliasing ---
#[derive(Debug, Clone)]
pub struct ToolOutput {
    pub output: String,
    pub title: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub images: Vec<ToolImage>,
}

#[derive(Debug, Clone)]
pub struct ToolImage {
    pub media_type: String,
    pub data: String,
    pub label: Option<String>,
}

impl ToolOutput {
    pub fn new(output: impl Into<String>) -> Self {
        Self {
            output: output.into(),
            title: None,
            metadata: None,
            images: Vec::new(),
        }
    }

    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = Some(metadata);
        self
    }

    pub fn with_image(mut self, media_type: impl Into<String>, data: impl Into<String>) -> Self {
        self.images.push(ToolImage {
            media_type: media_type.into(),
            data: data.into(),
            label: None,
        });
        self
    }

    pub fn with_labeled_image(
        mut self,
        media_type: impl Into<String>,
        data: impl Into<String>,
        label: impl Into<String>,
    ) -> Self {
        self.images.push(ToolImage {
            media_type: media_type.into(),
            data: data.into(),
            label: Some(label.into()),
        });
        self
    }
}

/// Resolve tool name aliases to their canonical internal names.
///
/// Providers can present tools with Claude Code aliases (e.g. `file_grep`,
/// `shell_exec`) or API namespace prefixes (e.g. `functions.bash`). Models can
/// repeat those names in sub-tool calls such as `batch`, while our registry
/// uses canonical internal names (`agentgrep`, `bash`). This mapping ensures
/// all of those forms resolve correctly.
///
/// This lives in `jcode-tool-core` (rather than the tool `Registry`) so that
/// low-level crates such as config can normalize tool names without depending
/// on the full tool subsystem.
pub fn resolve_tool_name(name: &str) -> &str {
    // Some function-calling APIs expose a recipient such as `functions.bash`.
    // Models occasionally preserve that transport namespace when constructing
    // a nested tool call, especially inside `batch`.
    let name = name.strip_prefix("functions.").unwrap_or(name);

    match name {
        "communicate" => "swarm",
        "task" | "task_runner" => "subagent",
        "launch" => "open",
        "shell" => "bash",
        "shell_exec" => "bash",
        "read_file" => "read",
        "file_read" => "read",
        "write_file" => "write",
        "file_write" => "write",
        "edit_file" => "edit",
        "file_edit" => "edit",
        // The native grep tool was removed in favor of agentgrep, but models
        // still frequently call `grep` (and OAuth's `file_grep`). agentgrep's
        // grep mode accepts `pattern` as an alias for `query`, so these calls
        // work as-is.
        "grep" | "file_grep" => "agentgrep",
        "skill" | "Skill" => "skill_manage",
        "todoread" | "todowrite" | "todo_read" | "todo_write" | "todos" => "todo",
        // The Anthropic OAuth surface advertises PascalCase tool names and
        // reverse-maps them provider-side for top-level calls, but nested
        // `batch` subcall names bypass that mapping and resolve here (issue
        // #486). Keep these in sync with anthropic_map_tool_name_from_oauth.
        "Bash" => "bash",
        "Read" => "read",
        "Write" => "write",
        "Edit" => "edit",
        "Grep" => "agentgrep",
        "Agent" => "subagent",
        "ScheduleWakeup" => "schedule",
        other => other,
    }
}

#[cfg(test)]
mod tool_types_tests {
    use super::resolve_tool_name;

    #[test]
    fn resolve_tool_name_strips_function_namespace_before_alias_resolution() {
        assert_eq!(resolve_tool_name("functions.bash"), "bash");
        assert_eq!(resolve_tool_name("functions.shell_exec"), "bash");
        assert_eq!(resolve_tool_name("functions.file_grep"), "agentgrep");
    }

    #[test]
    fn resolve_tool_name_does_not_strip_unrecognized_namespaces() {
        assert_eq!(
            resolve_tool_name("mcp.functions.bash"),
            "mcp.functions.bash"
        );
    }

    #[test]
    fn resolve_tool_name_maps_pascalcase_oauth_aliases() {
        // Anthropic OAuth advertises PascalCase names; batch subcalls resolve
        // through here rather than the provider-side reverse map (issue #486).
        assert_eq!(resolve_tool_name("Read"), "read");
        assert_eq!(resolve_tool_name("Bash"), "bash");
        assert_eq!(resolve_tool_name("Write"), "write");
        assert_eq!(resolve_tool_name("Edit"), "edit");
        assert_eq!(resolve_tool_name("Grep"), "agentgrep");
        assert_eq!(resolve_tool_name("Agent"), "subagent");
        assert_eq!(resolve_tool_name("ScheduleWakeup"), "schedule");
        assert_eq!(resolve_tool_name("Skill"), "skill_manage");
        assert_eq!(resolve_tool_name("functions.Read"), "read");
    }
}
