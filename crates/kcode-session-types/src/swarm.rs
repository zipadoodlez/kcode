use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::path::PathBuf;

// --- Swarm member vocabulary -------------------------------------------------
//
// The swarm-member data contract: role, lifecycle status, and the durable
// record. These live here (not in `kcode-swarm-core`) so the wire crate and the
// server share one definition without depending on swarm behavior.

/// A member's role in a swarm.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SwarmRole {
    Agent,
    Coordinator,
    Other(String),
}

impl SwarmRole {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Agent => "agent",
            Self::Coordinator => "coordinator",
            Self::Other(value) => value.as_str(),
        }
    }
}

impl From<String> for SwarmRole {
    fn from(value: String) -> Self {
        match value.as_str() {
            "agent" => Self::Agent,
            "coordinator" => Self::Coordinator,
            _ => Self::Other(value),
        }
    }
}

impl Serialize for SwarmRole {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for SwarmRole {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(Self::from(String::deserialize(deserializer)?))
    }
}

/// A swarm member's lifecycle status. Serializes to the same snake_case strings
/// the server has always written; unknown values round-trip as `Other`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SwarmLifecycleStatus {
    Spawned,
    Ready,
    Running,
    Completed,
    Done,
    Failed,
    Stopped,
    Crashed,
    Queued,
    Blocked,
    Pending,
    Todo,
    Other(String),
}

impl SwarmLifecycleStatus {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Spawned => "spawned",
            Self::Ready => "ready",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Done => "done",
            Self::Failed => "failed",
            Self::Stopped => "stopped",
            Self::Crashed => "crashed",
            Self::Queued => "queued",
            Self::Blocked => "blocked",
            Self::Pending => "pending",
            Self::Todo => "todo",
            Self::Other(value) => value.as_str(),
        }
    }

    /// The agent loop is gone, so no heartbeat or turn end will ever arrive for
    /// tasks this member holds.
    pub fn is_dead(&self) -> bool {
        matches!(self, Self::Failed | Self::Stopped | Self::Crashed)
    }

    /// A historical record, not a live agent: visible for reports and
    /// diagnostics but not consuming the runaway-prevention spawn budget.
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Done | Self::Failed | Self::Stopped | Self::Crashed
        )
    }

    /// Actively working right now.
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Running)
    }

    /// Holds or is waiting on an assignment.
    pub fn is_in_flight(&self) -> bool {
        matches!(self, Self::Queued | Self::Running)
    }
}

impl From<String> for SwarmLifecycleStatus {
    fn from(value: String) -> Self {
        match value.as_str() {
            "spawned" => Self::Spawned,
            "ready" => Self::Ready,
            "running" => Self::Running,
            "completed" => Self::Completed,
            "done" => Self::Done,
            "failed" => Self::Failed,
            "stopped" => Self::Stopped,
            "crashed" => Self::Crashed,
            "queued" => Self::Queued,
            "blocked" => Self::Blocked,
            "pending" => Self::Pending,
            "todo" => Self::Todo,
            _ => Self::Other(value),
        }
    }
}

impl From<&str> for SwarmLifecycleStatus {
    fn from(value: &str) -> Self {
        Self::from(value.to_string())
    }
}

impl std::fmt::Display for SwarmLifecycleStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for SwarmLifecycleStatus {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for SwarmLifecycleStatus {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(Self::from(String::deserialize(deserializer)?))
    }
}

/// Durable, persistable portion of a swarm member.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SwarmMemberRecord {
    pub session_id: String,
    pub working_dir: Option<PathBuf>,
    pub status: SwarmLifecycleStatus,
    pub detail: Option<String>,
    /// Stable label of the task/role this member was spawned or assigned for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_label: Option<String>,
    pub friendly_name: Option<String>,
    pub report_back_to_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_completion_report: Option<String>,
    pub is_headless: bool,
}

#[cfg(test)]
mod swarm_status_tests {
    use super::*;

    #[test]
    fn status_predicates_group_the_vocabulary() {
        assert!(SwarmLifecycleStatus::Running.is_active());
        assert!(!SwarmLifecycleStatus::Ready.is_active());
        assert!(SwarmLifecycleStatus::Failed.is_dead());
        assert!(!SwarmLifecycleStatus::Running.is_dead());
        assert!(SwarmLifecycleStatus::Completed.is_terminal());
        assert!(SwarmLifecycleStatus::Crashed.is_terminal());
        assert!(!SwarmLifecycleStatus::Running.is_terminal());
        assert!(SwarmLifecycleStatus::Queued.is_in_flight());
        assert!(SwarmLifecycleStatus::Running.is_in_flight());
        assert!(!SwarmLifecycleStatus::Ready.is_in_flight());
    }

    #[test]
    fn status_serializes_to_the_historical_strings() {
        let json = serde_json::to_string(&SwarmLifecycleStatus::Running).unwrap();
        assert_eq!(json, "\"running\"");
        let parsed: SwarmLifecycleStatus = serde_json::from_str("\"running\"").unwrap();
        assert_eq!(parsed, SwarmLifecycleStatus::Running);
    }

    #[test]
    fn unknown_status_round_trips_as_other() {
        let parsed: SwarmLifecycleStatus = serde_json::from_str("\"thinking\"").unwrap();
        assert_eq!(parsed, SwarmLifecycleStatus::Other("thinking".to_string()));
        assert_eq!(parsed.as_str(), "thinking");
        assert!(!parsed.is_active());
        let json = serde_json::to_string(&parsed).unwrap();
        assert_eq!(json, "\"thinking\"");
    }

    #[test]
    fn role_round_trips() {
        let coordinator: SwarmRole = serde_json::from_str("\"coordinator\"").unwrap();
        assert_eq!(coordinator, SwarmRole::Coordinator);
        let other: SwarmRole = serde_json::from_str("\"reviewer\"").unwrap();
        assert_eq!(other.as_str(), "reviewer");
    }
}
