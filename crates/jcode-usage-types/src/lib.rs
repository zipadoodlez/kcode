#[derive(Debug, Clone, Default)]
pub struct ProviderUsage {
    pub provider_name: String,
    pub limits: Vec<UsageLimit>,
    pub extra_info: Vec<(String, String)>,
    pub hard_limit_reached: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct UsageLimit {
    pub name: String,
    pub usage_percent: f32,
    pub resets_at: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ProviderUsageProgress {
    pub results: Vec<ProviderUsage>,
    pub completed: usize,
    pub total: usize,
    pub done: bool,
    pub from_cache: bool,
}

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CopilotUsageTracker {
    pub today: DayUsage,
    pub month: MonthUsage,
    pub all_time: AllTimeUsage,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DayUsage {
    pub date: String,
    pub requests: u64,
    pub premium_requests: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MonthUsage {
    pub month: String,
    pub requests: u64,
    pub premium_requests: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AllTimeUsage {
    pub requests: u64,
    pub premium_requests: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
}
