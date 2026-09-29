use chrono::{DateTime, Utc};
use kcode_message_types::ToolCall;
use kcode_session_types::SessionStatus;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum SessionFilterMode {
    All,
    /// Sessions whose working directory matches the directory `/resume` was
    /// opened from.
    CurrentDir,
    CatchUp,
    Saved,
    /// Sessions with a live process right now (from the active-pid registry),
    /// annotated with whether each is still streaming a response or is ready
    /// for input. Backs the opt-in "active sessions manager" view.
    Active,
}

impl SessionFilterMode {
    pub fn next(self) -> Self {
        match self {
            Self::All => Self::CurrentDir,
            Self::CurrentDir => Self::CatchUp,
            Self::CatchUp => Self::Saved,
            Self::Saved => Self::Active,
            Self::Active => Self::All,
        }
    }

    pub fn previous(self) -> Self {
        match self {
            Self::All => Self::Active,
            Self::CurrentDir => Self::All,
            Self::CatchUp => Self::CurrentDir,
            Self::Saved => Self::CatchUp,
            Self::Active => Self::Saved,
        }
    }

    pub fn label(self) -> Option<&'static str> {
        match self {
            Self::All => None,
            Self::CurrentDir => Some("📁 current dir"),
            Self::CatchUp => Some("⏭ catch up"),
            Self::Saved => Some("📌 saved"),
            Self::Active => Some("⚡ active"),
        }
    }
}

/// Session info for display in the interactive session picker.
#[derive(Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SessionInfo {
    pub id: String,
    pub parent_id: Option<String>,
    pub short_name: String,
    pub icon: String,
    pub title: String,
    pub message_count: usize,
    pub user_message_count: usize,
    pub assistant_message_count: usize,
    pub created_at: DateTime<Utc>,
    pub last_message_time: DateTime<Utc>,
    pub last_active_at: Option<DateTime<Utc>>,
    pub working_dir: Option<String>,
    pub model: Option<String>,
    pub provider_key: Option<String>,
    pub is_canary: bool,
    pub is_debug: bool,
    pub saved: bool,
    pub save_label: Option<String>,
    pub status: SessionStatus,
    pub needs_catchup: bool,
    pub estimated_tokens: usize,
    /// First visible user prompt in the session, shown in compact list rows.
    pub first_user_prompt: Option<String>,
    pub messages_preview: Vec<PreviewMessage>,
    /// Lowercased searchable text used by picker filtering.
    pub search_index: String,
    /// Server name this session belongs to (if running).
    pub server_name: Option<String>,
    /// Server icon.
    pub server_icon: Option<String>,
}

/// A group of sessions under a server.
#[derive(Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ServerGroup {
    pub name: String,
    pub icon: String,
    pub version: String,
    pub git_hash: String,
    pub is_running: bool,
    pub sessions: Vec<SessionInfo>,
}

#[derive(Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PreviewMessage {
    pub role: String,
    pub content: String,
    pub tool_calls: Vec<String>,
    pub tool_data: Option<ToolCall>,
    pub timestamp: Option<DateTime<Utc>>,
}

/// An item in the picker list, either a server/header row or a session row.
#[derive(Clone)]
pub enum PickerItem {
    ServerHeader {
        name: String,
        icon: String,
        version: String,
        session_count: usize,
    },
    Session,
    OrphanHeader {
        session_count: usize,
    },
    SavedHeader {
        session_count: usize,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_mode_cycles_through_local_views() {
        let mut mode = SessionFilterMode::All;
        for expected in [
            SessionFilterMode::CurrentDir,
            SessionFilterMode::CatchUp,
            SessionFilterMode::Saved,
            SessionFilterMode::Active,
            SessionFilterMode::All,
        ] {
            mode = mode.next();
            assert_eq!(mode, expected);
        }
        for expected in [
            SessionFilterMode::Active,
            SessionFilterMode::Saved,
            SessionFilterMode::CatchUp,
            SessionFilterMode::CurrentDir,
            SessionFilterMode::All,
        ] {
            mode = mode.previous();
            assert_eq!(mode, expected);
        }
    }
}
