use jcode_message_types::ToolCall;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderedMessage {
    pub role: String,
    pub content: String,
    pub tool_calls: Vec<String>,
    pub tool_data: Option<ToolCall>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderedCompactedHistoryInfo {
    pub total_messages: usize,
    pub visible_messages: usize,
    pub remaining_messages: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RenderedImageSource {
    UserInput,
    ToolResult { tool_name: String },
    Other { role: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RenderedImage {
    pub media_type: String,
    pub data: String,
    pub label: Option<String>,
    pub source: RenderedImageSource,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum SessionStatus {
    #[default]
    Active,
    Closed,
    Crashed {
        message: Option<String>,
    },
    Reloaded,
    Compacted,
    RateLimited,
    Error {
        message: String,
    },
}

impl SessionStatus {
    pub fn display(&self) -> &'static str {
        match self {
            SessionStatus::Active => "active",
            SessionStatus::Closed => "closed",
            SessionStatus::Crashed { .. } => "crashed",
            SessionStatus::Reloaded => "reloaded",
            SessionStatus::Compacted => "compacted",
            SessionStatus::RateLimited => "rate limited",
            SessionStatus::Error { .. } => "error",
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            SessionStatus::Active => "▶",
            SessionStatus::Closed => "✓",
            SessionStatus::Crashed { .. } => "💥",
            SessionStatus::Reloaded => "🔄",
            SessionStatus::Compacted => "📦",
            SessionStatus::RateLimited => "⏳",
            SessionStatus::Error { .. } => "❌",
        }
    }

    pub fn detail(&self) -> Option<&str> {
        match self {
            SessionStatus::Crashed { message } => message.as_deref(),
            SessionStatus::Error { message } => Some(message.as_str()),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum SessionImproveMode {
    #[serde(rename = "improve_run", alias = "run")]
    ImproveRun,
    #[serde(rename = "improve_plan", alias = "plan")]
    ImprovePlan,
    #[serde(rename = "refactor_run")]
    RefactorRun,
    #[serde(rename = "refactor_plan")]
    RefactorPlan,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitState {
    pub root: String,
    pub head: Option<String>,
    pub branch: Option<String>,
    pub dirty: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvSnapshot {
    pub captured_at: chrono::DateTime<chrono::Utc>,
    pub reason: String,
    pub session_id: String,
    pub working_dir: Option<String>,
    pub provider: String,
    pub model: String,
    pub jcode_version: String,
    pub jcode_git_hash: Option<String>,
    pub jcode_git_dirty: Option<bool>,
    pub os: String,
    pub arch: String,
    pub pid: u32,
    pub is_selfdev: bool,
    pub is_debug: bool,
    pub is_canary: bool,
    pub testing_build: Option<String>,
    pub working_git: Option<GitState>,
}
