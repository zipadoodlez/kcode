use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;

pub const OVERNIGHT_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OvernightDuration {
    pub minutes: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum OvernightCommand {
    Start {
        duration: OvernightDuration,
        mission: Option<String>,
    },
    Status,
    Log,
    Review,
    Cancel,
    Help,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OvernightRunStatus {
    Running,
    CancelRequested,
    Completed,
    Failed,
}

impl OvernightRunStatus {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::CancelRequested => "cancel requested",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OvernightManifest {
    pub version: u32,
    pub run_id: String,
    pub parent_session_id: String,
    pub coordinator_session_id: String,
    pub coordinator_session_name: String,
    pub started_at: DateTime<Utc>,
    pub target_wake_at: DateTime<Utc>,
    pub handoff_ready_at: DateTime<Utc>,
    pub post_wake_grace_until: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub morning_report_posted_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cancel_requested_at: Option<DateTime<Utc>>,
    pub status: OvernightRunStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mission: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_dir: Option<String>,
    pub provider_name: String,
    pub model: String,
    pub max_agents_guidance: u8,
    pub process_id: u32,
    pub run_dir: PathBuf,
    pub events_path: PathBuf,
    pub human_log_path: PathBuf,
    pub review_path: PathBuf,
    pub review_notes_path: PathBuf,
    pub preflight_path: PathBuf,
    pub task_cards_dir: PathBuf,
    pub issue_drafts_dir: PathBuf,
    pub validation_dir: PathBuf,
    pub last_activity_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OvernightEvent {
    pub timestamp: DateTime<Utc>,
    pub run_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    pub kind: String,
    pub summary: String,
    #[serde(default)]
    pub details: Value,
    #[serde(default)]
    pub meaningful: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ResourceSnapshot {
    pub captured_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_total_mb: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_available_mb: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_used_percent: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub swap_total_mb: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub swap_free_mb: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub load_one: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_count: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub battery_percent: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub battery_status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disk_available_gb: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageProviderSnapshot {
    pub provider_name: String,
    pub hard_limit_reached: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub limits: Vec<UsageLimitSnapshot>,
    pub extra_info: Vec<(String, String)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageLimitSnapshot {
    pub name: String,
    pub usage_percent: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resets_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageProjection {
    pub captured_at: DateTime<Utc>,
    pub risk: String,
    pub confidence: String,
    pub projected_delta_min_percent: Option<f32>,
    pub projected_delta_max_percent: Option<f32>,
    pub projected_end_min_percent: Option<f32>,
    pub projected_end_max_percent: Option<f32>,
    pub providers: Vec<UsageProviderSnapshot>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitSnapshot {
    pub captured_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dirty_count: Option<usize>,
    #[serde(default)]
    pub dirty_summary: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OvernightPreflight {
    pub captured_at: DateTime<Utc>,
    pub usage: UsageProjection,
    pub resources: ResourceSnapshot,
    pub git: GitSnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OvernightTaskCardBefore {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
    #[serde(default)]
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OvernightTaskCardAfter {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub change: Option<String>,
    #[serde(default)]
    pub files_changed: Vec<String>,
    #[serde(default)]
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OvernightTaskCardValidation {
    #[serde(default)]
    pub commands: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    #[serde(default)]
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OvernightTaskCard {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub why_selected: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verifiability: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub risk: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<String>,
    #[serde(default)]
    pub before: OvernightTaskCardBefore,
    #[serde(default)]
    pub after: OvernightTaskCardAfter,
    #[serde(default)]
    pub validation: OvernightTaskCardValidation,
    #[serde(default)]
    pub followups: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct OvernightTaskStatusCounts {
    pub completed: usize,
    pub active: usize,
    pub blocked: usize,
    pub deferred: usize,
    pub failed: usize,
    pub skipped: usize,
    pub unknown: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct OvernightTaskCardSummary {
    pub total: usize,
    pub counts: OvernightTaskStatusCounts,
    pub validated: usize,
    pub high_risk: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OvernightProgressCard {
    pub run_id: String,
    pub status: String,
    pub phase: String,
    pub coordinator_session_id: String,
    pub coordinator_session_name: String,
    pub elapsed_label: String,
    pub target_duration_label: String,
    pub progress_percent: f32,
    pub target_wake_at: String,
    pub time_relation: String,
    pub last_activity_label: String,
    pub next_prompt_label: String,
    pub usage_risk: String,
    pub usage_confidence: String,
    pub usage_projection: String,
    pub resources_summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_event_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_event_summary: Option<String>,
    pub task_summary: OvernightTaskCardSummary,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_task_title: Option<String>,
    pub review_path: String,
    pub log_path: String,
    pub run_dir: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
}

pub fn parse_overnight_command(trimmed: &str) -> Option<Result<OvernightCommand, String>> {
    let rest = trimmed.strip_prefix("/overnight")?.trim();
    if rest.is_empty() || rest == "help" || rest == "--help" || rest == "-h" {
        return Some(Ok(OvernightCommand::Help));
    }

    match rest {
        "status" => return Some(Ok(OvernightCommand::Status)),
        "log" => return Some(Ok(OvernightCommand::Log)),
        "review" | "open" => return Some(Ok(OvernightCommand::Review)),
        "cancel" | "stop" => return Some(Ok(OvernightCommand::Cancel)),
        _ => {}
    }

    if rest.starts_with("status ")
        || rest.starts_with("log ")
        || rest.starts_with("review ")
        || rest.starts_with("cancel ")
    {
        return Some(Err(overnight_usage().to_string()));
    }

    let mut parts = rest.splitn(2, char::is_whitespace);
    let duration_raw = parts.next().unwrap_or_default();
    let mission = parts
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    let duration = match parse_duration(duration_raw) {
        Ok(duration) => duration,
        Err(error) => return Some(Err(error)),
    };

    Some(Ok(OvernightCommand::Start { duration, mission }))
}

pub fn overnight_usage() -> &'static str {
    "Usage: `/overnight <hours>[h|m] [mission]`, `/overnight status`, `/overnight log`, `/overnight review`, or `/overnight cancel`"
}

pub fn parse_duration(input: &str) -> std::result::Result<OvernightDuration, String> {
    let raw = input.trim();
    if raw.is_empty() {
        return Err(overnight_usage().to_string());
    }

    let (number, multiplier) = if let Some(hours) = raw.strip_suffix('h') {
        (hours, 60.0)
    } else if let Some(minutes) = raw.strip_suffix('m') {
        (minutes, 1.0)
    } else {
        (raw, 60.0)
    };

    let value: f64 = number.parse().map_err(|_| {
        format!(
            "Invalid overnight duration `{}`. {}",
            raw,
            overnight_usage()
        )
    })?;
    if !value.is_finite() || value <= 0.0 {
        return Err(format!(
            "Invalid overnight duration `{}`. Duration must be greater than zero.",
            raw
        ));
    }
    let minutes = (value * multiplier).round() as u32;
    if minutes == 0 || minutes > 72 * 60 {
        return Err("Overnight duration must be between 1 minute and 72 hours.".to_string());
    }
    Ok(OvernightDuration { minutes })
}
