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

pub fn summarize_task_cards_slice(cards: &[OvernightTaskCard]) -> OvernightTaskCardSummary {
    let mut summary = OvernightTaskCardSummary {
        total: cards.len(),
        ..Default::default()
    };
    for card in cards {
        match task_status_bucket(&card.status) {
            "completed" => summary.counts.completed += 1,
            "active" => summary.counts.active += 1,
            "blocked" => summary.counts.blocked += 1,
            "deferred" => summary.counts.deferred += 1,
            "failed" => summary.counts.failed += 1,
            "skipped" => summary.counts.skipped += 1,
            _ => summary.counts.unknown += 1,
        }
        if task_card_validated(card) {
            summary.validated += 1;
        }
        if card
            .risk
            .as_deref()
            .map(|risk| risk.to_ascii_lowercase().contains("high"))
            .unwrap_or(false)
        {
            summary.high_risk += 1;
        }
    }
    if let Some(latest) = cards.last() {
        summary.latest_title = Some(task_card_title(latest));
        summary.latest_status = Some(if latest.status.trim().is_empty() {
            "unknown".to_string()
        } else {
            latest.status.clone()
        });
    }
    summary
}

pub fn task_card_title(card: &OvernightTaskCard) -> String {
    if !card.title.trim().is_empty() {
        card.title.clone()
    } else if !card.id.trim().is_empty() {
        card.id.clone()
    } else {
        "untitled task".to_string()
    }
}

pub fn task_status_bucket(status: &str) -> &'static str {
    let normalized = status
        .trim()
        .to_ascii_lowercase()
        .replace('-', "_")
        .replace(' ', "_");
    match normalized.as_str() {
        "done" | "complete" | "completed" | "fixed" | "validated" | "merged" => "completed",
        "active" | "running" | "in_progress" | "working" | "verifying" | "planned" => "active",
        "blocked" | "needs_user" | "waiting" => "blocked",
        "deferred" | "queued" | "backlog" | "todo" => "deferred",
        "failed" | "error" | "abandoned" => "failed",
        "skipped" | "rejected" | "not_started" => "skipped",
        _ => "unknown",
    }
}

pub fn task_card_validated(card: &OvernightTaskCard) -> bool {
    let result = card
        .validation
        .result
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();
    result.contains("pass")
        || result.contains("success")
        || result.contains("validated")
        || result == "ok"
}

pub fn event_class(kind: &str) -> &'static str {
    if kind.contains("failed") || kind.contains("cancel") {
        "bad"
    } else if kind.contains("warning") || kind.contains("requested") || kind.contains("handoff") {
        "warn"
    } else if kind.contains("completed") || kind.contains("started") {
        "ok"
    } else {
        "info"
    }
}

pub fn html_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

pub fn resource_summary(snapshot: &ResourceSnapshot) -> String {
    let memory = snapshot
        .memory_used_percent
        .map(|pct| format!("RAM {:.0}%", pct))
        .unwrap_or_else(|| "RAM unknown".to_string());
    let load = snapshot
        .load_one
        .zip(snapshot.cpu_count)
        .map(|(load, cpus)| format!("load {:.1}/{}", load, cpus))
        .unwrap_or_else(|| "load unknown".to_string());
    let battery = snapshot
        .battery_percent
        .map(|pct| {
            format!(
                "battery {}%{}",
                pct,
                snapshot
                    .battery_status
                    .as_ref()
                    .map(|status| format!(" {}", status))
                    .unwrap_or_default()
            )
        })
        .unwrap_or_else(|| "battery unknown".to_string());
    format!("{}, {}, {}", memory, load, battery)
}

pub fn git_summary(snapshot: &GitSnapshot) -> String {
    if let Some(error) = snapshot.error.as_ref() {
        return format!("git unavailable ({})", error);
    }
    let dirty = snapshot.dirty_count.unwrap_or(0);
    let branch = snapshot.branch.as_deref().unwrap_or("unknown branch");
    if dirty == 0 {
        format!("{} clean", branch)
    } else {
        format!(
            "{} with {} dirty file{}",
            branch,
            dirty,
            if dirty == 1 { "" } else { "s" }
        )
    }
}

pub fn format_minutes(minutes: u32) -> String {
    if minutes < 60 {
        return format!("{}m", minutes);
    }
    let hours = minutes / 60;
    let mins = minutes % 60;
    if mins == 0 {
        format!("{}h", hours)
    } else {
        format!("{}h {}m", hours, mins)
    }
}

#[cfg(test)]
mod helper_tests {
    use super::*;
    use chrono::Utc;

    fn task_card(id: &str, title: &str, status: &str) -> OvernightTaskCard {
        OvernightTaskCard {
            id: id.to_string(),
            title: title.to_string(),
            status: status.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn summarizes_task_card_statuses_and_validation() {
        let mut completed = task_card("1", "Done", "validated");
        completed.validation.result = Some("passed".to_string());
        completed.risk = Some("high".to_string());
        let active = task_card("2", "Active", "in progress");
        let blocked = task_card("3", "Blocked", "needs user");
        let summary = summarize_task_cards_slice(&[completed, active, blocked]);
        assert_eq!(summary.total, 3);
        assert_eq!(summary.counts.completed, 1);
        assert_eq!(summary.counts.active, 1);
        assert_eq!(summary.counts.blocked, 1);
        assert_eq!(summary.validated, 1);
        assert_eq!(summary.high_risk, 1);
        assert_eq!(summary.latest_title.as_deref(), Some("Blocked"));
    }

    #[test]
    fn task_status_bucket_normalizes_common_labels() {
        assert_eq!(task_status_bucket("in-progress"), "active");
        assert_eq!(task_status_bucket("needs user"), "blocked");
        assert_eq!(task_status_bucket("not started"), "skipped");
    }

    #[test]
    fn escape_and_event_class_helpers_are_stable() {
        assert_eq!(
            html_escape("<tag & 'quote'>"),
            "&lt;tag &amp; &#39;quote&#39;&gt;"
        );
        assert_eq!(event_class("task_failed"), "bad");
        assert_eq!(event_class("handoff_requested"), "warn");
        assert_eq!(event_class("run_completed"), "ok");
    }

    #[test]
    fn resource_and_git_summaries_are_compact() {
        let resources = ResourceSnapshot {
            captured_at: Utc::now(),
            memory_used_percent: Some(42.0),
            load_one: Some(1.5),
            cpu_count: Some(8),
            battery_percent: Some(77),
            battery_status: Some("Discharging".to_string()),
            ..Default::default()
        };
        assert_eq!(
            resource_summary(&resources),
            "RAM 42%, load 1.5/8, battery 77% Discharging"
        );

        let git = GitSnapshot {
            captured_at: Utc::now(),
            branch: Some("master".to_string()),
            dirty_count: Some(2),
            dirty_summary: Vec::new(),
            error: None,
        };
        assert_eq!(git_summary(&git), "master with 2 dirty files");
    }

    #[test]
    fn format_minutes_is_human_compact() {
        assert_eq!(format_minutes(45), "45m");
        assert_eq!(format_minutes(120), "2h");
        assert_eq!(format_minutes(125), "2h 5m");
    }
}
