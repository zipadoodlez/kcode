use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Extra non-conversation UI/state events persisted for replay fidelity.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StoredReplayEvent {
    pub timestamp: DateTime<Utc>,
    #[serde(flatten)]
    pub kind: StoredReplayEventKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "event")]
pub enum StoredReplayEventKind {
    /// A non-provider display message shown in the UI (e.g. swarm/system notice).
    #[serde(rename = "display_message")]
    DisplayMessage {
        role: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        content: String,
    },
    /// Historical swarm member status snapshot. Nothing writes this any more
    /// (A8, 2026-10-03), but session files on disk carry it and session load
    /// rejects an unknown variant, so the shape and its redaction stay.
    #[serde(rename = "swarm_status")]
    SwarmStatus {
        members: Vec<crate::protocol::SwarmMemberStatus>,
    },
    /// Historical swarm plan snapshot. Read-only for the same reason as
    /// [`Self::SwarmStatus`].
    #[serde(rename = "swarm_plan")]
    SwarmPlan {
        swarm_id: String,
        items: Vec<crate::plan::TaskItem>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
}

pub(super) const SESSION_CONTEXT_PREFIX: &str = "<system-reminder>\n# Session Context";
