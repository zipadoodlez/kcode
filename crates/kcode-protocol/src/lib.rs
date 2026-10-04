//! Client-server protocol for kcode
//!
//! Uses newline-delimited JSON over Unix socket.
//! Server streams events back to clients during message processing.
//!
//! Socket types:
//! - Main socket: TUI/client communication with agent
//! - Agent socket: Inter-agent communication (AI-to-AI)

use serde::{Deserialize, Serialize};

mod notifications;
mod side_panel;

pub use notifications::{FeatureToggle, NotificationType};

use kcode_message_types::BatchProgress;
use kcode_message_types::{InputShellResult, ToolCall};
use kcode_plan::TaskItem;
pub use kcode_session_types::SwarmLifecycleStatus;
pub use side_panel::{
    PersistedSidePanelPage, PersistedSidePanelState, SidePanelPage, SidePanelPageFormat,
    SidePanelPageSource, SidePanelSnapshot, snapshot_is_empty,
};

#[path = "protocol_memory.rs"]
mod memory_snapshots;

pub use memory_snapshots::{
    MemoryActivitySnapshot, MemoryPipelineSnapshot, MemoryStateSnapshot, MemoryStepResultSnapshot,
    MemoryStepStatusSnapshot,
};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum TranscriptMode {
    Insert,
    Append,
    Replace,
    #[default]
    Send,
}

/// A message in conversation history (for sync)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryMessage {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_stats: Option<kcode_session_types::ResponseStats>,
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_data: Option<ToolCall>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionActivitySnapshot {
    pub is_processing: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_tool_name: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TokenUsageTotals {
    pub messages_with_token_usage: usize,
    pub input_tokens: u64,
    pub output_tokens: u64,
    /// Input tokens from requests where the provider reported cache telemetry.
    /// This may be lower than `input_tokens` for providers or older sessions that
    /// did not expose cache-read/cache-write fields.
    pub cache_reported_input_tokens: u64,
    pub cache_read_input_tokens: u64,
    pub cache_creation_input_tokens: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(transparent)]
pub struct AuthProviderId(pub String);

impl AuthProviderId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(transparent)]
pub struct RuntimeProviderKey(pub String);

impl RuntimeProviderKey {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(transparent)]
pub struct CatalogNamespace(pub String);

impl CatalogNamespace {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuthCredentialSource {
    ApiKeyFile,
    ProcessEnv,
    OAuthTokenStore,
    ExternalImport,
    Unknown,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuthMethod {
    TuiPasteApiKey,
    RemoteTuiPasteApiKey,
    CliLogin,
    EnvFilePreseeded,
    ProcessEnvPreseeded,
    OAuthBrowser,
    DeviceCode,
    ExternalImport,
    Unknown,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthChanged {
    pub provider: AuthProviderId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_source: Option<AuthCredentialSource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_method: Option<AuthMethod>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_runtime: Option<RuntimeProviderKey>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_catalog_namespace: Option<CatalogNamespace>,
}

impl AuthChanged {
    pub fn new(provider: impl Into<String>) -> Self {
        Self {
            provider: AuthProviderId::new(provider),
            credential_source: None,
            auth_method: None,
            expected_runtime: None,
            expected_catalog_namespace: None,
        }
    }
}

/// Server-owned reload recovery intent for a session: an optional reconnect
/// notice plus the continuation message the client should queue after a reload
/// or an interruption.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReloadRecoveryDirective {
    pub reconnect_notice: Option<String>,
    pub continuation_message: String,
}

pub type ReloadRecoverySnapshot = ReloadRecoveryDirective;

mod wire;
pub use wire::{Request, ServerEvent};

/// Swarm member status for lifecycle updates
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SwarmMemberStatus {
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub friendly_name: Option<String>,
    /// Lifecycle status (ready, running, completed, failed, stopped, etc.)
    pub status: SwarmLifecycleStatus,
    /// Optional detail (task, error, etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// Stable label of the task/role this member was spawned or assigned for.
    /// Unlike `detail`, it is not overwritten by transient status updates.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_label: Option<String>,
    /// Role: "agent" or "coordinator"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// Whether this member is a headless spawned session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_headless: Option<bool>,
    /// Number of currently attached live client connections.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub live_attachments: Option<usize>,
    /// Seconds since the last status change.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_age_secs: Option<u64>,
    /// Session id this member reports back to (its spawner/parent in the swarm
    /// tree). Walking this chain reconstructs the spawn tree, which is also the
    /// membership: a session belongs to the run rooted at the end of its chain.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub report_back_to_session_id: Option<String>,
    /// Ephemeral runtime metadata for the pinned list's holder line.
    #[serde(default, skip_serializing_if = "SwarmMemberRuntime::is_empty")]
    pub runtime: SwarmMemberRuntime,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SwarmMemberRuntime {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    /// Human-facing credential route, such as "OAuth" or "API key".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_method: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elapsed_secs: Option<u64>,
}

impl SwarmMemberRuntime {
    fn is_empty(&self) -> bool {
        self.model.is_none()
            && self.provider.is_none()
            && self.auth_method.is_none()
            && self.effort.is_none()
            && self.elapsed_secs.is_none()
    }
}

impl Request {
    pub fn id(&self) -> u64 {
        match self {
            Request::Message { id, .. } => *id,
            Request::Cancel { id } => *id,
            Request::BackgroundTool { id } => *id,
            Request::SoftInterrupt { id, .. } => *id,
            Request::CancelSoftInterrupts { id } => *id,
            Request::Clear { id } => *id,
            Request::Rewind { id, .. } => *id,
            Request::RewindUndo { id } => *id,
            Request::Ping { id } => *id,
            Request::GetState { id } => *id,
            Request::DebugCommand { id, .. } => *id,
            Request::ClientDebugCommand { id, .. } => *id,
            Request::ClientDebugResponse { id, .. } => *id,
            Request::Subscribe { id, .. } | Request::PrepareDisconnect { id } => *id,
            Request::GetHistory { id } => *id,
            Request::GetModelCatalog { id, .. } => *id,
            Request::GetCompactedHistory { id, .. } => *id,
            Request::Reload { id, .. } => *id,
            Request::ResumeSession { id, .. } => *id,
            Request::ResumeAllSessions { id } => *id,
            Request::NotifySession { id, .. } => *id,
            Request::Transcript { id, .. } => *id,
            Request::InputShell { id, .. } => *id,
            Request::CycleModel { id, .. } => *id,
            Request::RefreshModels { id } => *id,
            Request::SetModel { id, .. } => *id,
            Request::SetRoute { id, .. } => *id,
            Request::SetSubagentModel { id, .. } => *id,
            Request::RunSubagent { id, .. } => *id,
            Request::SetReasoningEffort { id, .. } => *id,
            Request::SetServiceTier { id, .. } => *id,
            Request::SetTransport { id, .. } => *id,
            Request::SetPremiumMode { id, .. } => *id,
            Request::SetFeature { id, .. } => *id,
            Request::SetCompactionMode { id, .. } => *id,
            Request::Split { id } => *id,
            Request::Transfer { id } => *id,
            Request::Compact { id } => *id,
            Request::NotifyAuthChanged { id, .. } => *id,
            Request::SwitchAnthropicAccount { id, .. } => *id,
            Request::SwitchOpenAiAccount { id, .. } => *id,
            Request::StdinResponse { id, .. } => *id,
            Request::AgentRegister { id, .. } => *id,
            Request::AgentTask { id, .. } => *id,
            Request::AgentCapabilities { id } => *id,
            Request::AgentContext { id } => *id,
            Request::CommMessage { id, .. } => *id,
            Request::CommSpawn { id, .. } => *id,
            Request::CommListModels { id, .. } => *id,
            Request::CommStop { id, .. } => *id,
        }
    }

    pub fn is_lightweight_control_request(&self) -> bool {
        matches!(
            self,
            Request::Ping { .. }
                | Request::NotifySession { .. }
                | Request::CommMessage { .. }
                | Request::CommSpawn { .. }
                | Request::CommListModels { .. }
                | Request::CommStop { .. }
        )
    }
}

fn default_model_direction() -> i8 {
    1
}

/// Encode an event as a newline-terminated JSON string
pub fn encode_event(event: &ServerEvent) -> String {
    let mut json = serde_json::to_string(event).unwrap_or_else(|_| "{}".to_string());
    json.push('\n');
    json
}

/// Decode a request from a JSON string.
///
/// Handles a legacy/desktop compatibility shape where a model switch was sent as
/// `{"type":"set_route","model":"..."}` (a bare model string under the
/// `set_route` tag). The current protocol reserves the `set_route` tag for the
/// structured [`Request::SetRoute`] variant (which carries a `selection`
/// object), so this older shape is normalized into [`Request::SetModel`] here
/// instead of via a serde `alias`. Using an alias would make `SetModel` also
/// claim the `set_route` tag and, because serde dispatches internally-tagged
/// enums by tag rather than by fields, shadow the structured variant entirely
/// (every real route switch would then fail with `missing field \`model\``).
pub fn decode_request(line: &str) -> Result<Request, serde_json::Error> {
    match serde_json::from_str::<Request>(line) {
        Ok(request) => Ok(request),
        Err(error) => {
            if let Some(request) = decode_legacy_set_route_model(line) {
                Ok(request)
            } else {
                Err(error)
            }
        }
    }
}

/// Recognize the legacy `{"type":"set_route","id":N,"model":"..."}` shape and
/// translate it into [`Request::SetModel`]. Returns `None` for anything else
/// (including the current structured `set_route` payload that carries a
/// `selection` object) so the original decode error is surfaced unchanged.
fn decode_legacy_set_route_model(line: &str) -> Option<Request> {
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    let obj = value.as_object()?;
    if obj.get("type")?.as_str()? != "set_route" {
        return None;
    }
    // The structured route switch carries `selection`; never reinterpret it.
    if obj.contains_key("selection") {
        return None;
    }
    let model = obj.get("model")?.as_str()?.to_string();
    let id = obj.get("id").and_then(|v| v.as_u64()).unwrap_or(0);
    Some(Request::SetModel { id, model })
}

#[cfg(test)]
#[path = "protocol_tests.rs"]
mod tests;
