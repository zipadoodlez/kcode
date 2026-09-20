#[derive(Debug, Clone, Default)]
pub struct ProviderUsage {
    pub provider_name: String,
    pub limits: Vec<UsageLimit>,
    pub extra_info: Vec<(String, String)>,
    pub hard_limit_reached: bool,
    pub error: Option<String>,
    /// When jcode last successfully used this login/credential (unix seconds).
    /// Drives most-recently-used-first ordering in `/usage`. `None` sorts last.
    pub last_used_unix_secs: Option<u64>,
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

/// Local usage of one model route, aggregated across reasoning efforts.
///
/// `count` is the number of agent turns with at least one persisted assistant
/// response on this route. Tool continuations in the same turn count once.
/// Tracking is prospective, not an estimate of all-time usage. Picker selections
/// are a separate legacy signal and never contribute to the tracked-turn count.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct ModelUsage {
    pub count: u64,
    pub last_used_unix_secs: Option<u64>,
    pub tracking_started_unix_secs: Option<u64>,
    pub selection_count: u64,
    pub last_selected_unix_secs: Option<u64>,
}

impl ModelUsage {
    /// Merge a possibly delayed observation from another session. Counts cannot
    /// regress within one tracking epoch. A newer epoch represents a new ledger.
    pub fn merge_observation(&mut self, observed: &Self) {
        if observed.tracking_started_unix_secs > self.tracking_started_unix_secs {
            self.count = observed.count;
            self.last_used_unix_secs = observed.last_used_unix_secs;
            self.tracking_started_unix_secs = observed.tracking_started_unix_secs;
        } else if observed.tracking_started_unix_secs == self.tracking_started_unix_secs {
            self.count = self.count.max(observed.count);
            self.last_used_unix_secs = self.last_used_unix_secs.max(observed.last_used_unix_secs);
        }
        self.selection_count = self.selection_count.max(observed.selection_count);
        self.last_selected_unix_secs = self
            .last_selected_unix_secs
            .max(observed.last_selected_unix_secs);
    }
}

/// Compare usage best-first for `sort_by`. Callers should apply search relevance
/// first and a stable model/route identity tie-breaker afterwards. Missing usage
/// means unknown, not never used. Historical selections seed unused routes.
pub fn compare_model_usage(a: Option<&ModelUsage>, b: Option<&ModelUsage>) -> std::cmp::Ordering {
    let key = |usage: Option<&ModelUsage>| {
        usage.map(|u| {
            (
                u.count,
                u.last_used_unix_secs,
                u.selection_count,
                u.last_selected_unix_secs,
            )
        })
    };
    key(b).cmp(&key(a))
}

#[cfg(test)]
mod model_usage_tests {
    use super::*;
    #[test]
    fn observations_are_monotonic_until_a_new_tracking_epoch() {
        let mut usage = ModelUsage {
            count: 4,
            last_used_unix_secs: Some(50),
            tracking_started_unix_secs: Some(10),
            selection_count: 9,
            last_selected_unix_secs: Some(8),
        };
        usage.merge_observation(&ModelUsage {
            count: 2,
            last_used_unix_secs: Some(30),
            tracking_started_unix_secs: Some(10),
            ..Default::default()
        });
        assert_eq!(usage.count, 4);
        assert_eq!(usage.last_used_unix_secs, Some(50));
        usage.merge_observation(&ModelUsage {
            count: 1,
            last_used_unix_secs: Some(60),
            tracking_started_unix_secs: Some(55),
            ..Default::default()
        });
        assert_eq!(usage.count, 1);
        assert_eq!(usage.selection_count, 9);
        usage.merge_observation(&ModelUsage {
            count: 100,
            tracking_started_unix_secs: Some(10),
            ..Default::default()
        });
        assert_eq!(usage.count, 1);
        assert_eq!(usage.last_used_unix_secs, Some(60));
    }

    #[test]
    fn usage_order_prefers_turns_then_recency_then_historical_selections() {
        let popular = ModelUsage {
            count: 2,
            last_used_unix_secs: Some(10),
            ..Default::default()
        };
        let recent = ModelUsage {
            count: 1,
            last_used_unix_secs: Some(20),
            ..Default::default()
        };
        let legacy = ModelUsage {
            selection_count: 100,
            last_selected_unix_secs: Some(30),
            ..Default::default()
        };
        let unused = ModelUsage::default();
        let mut entries = vec![
            None,
            Some(&unused),
            Some(&legacy),
            Some(&recent),
            Some(&popular),
        ];
        entries.sort_by(|a, b| compare_model_usage(*a, *b));
        assert_eq!(
            entries,
            vec![
                Some(&popular),
                Some(&recent),
                Some(&legacy),
                Some(&unused),
                None
            ]
        );
        assert!(compare_model_usage(Some(&popular), Some(&popular)).is_eq());
        let newer = ModelUsage {
            last_used_unix_secs: Some(11),
            ..popular.clone()
        };
        assert!(compare_model_usage(Some(&newer), Some(&popular)).is_lt());
    }

    #[test]
    fn usage_dto_defaults_missing_metadata_without_inventing_history() {
        let usage: ModelUsage = serde_json::from_str("{}").unwrap();
        assert_eq!(usage, ModelUsage::default());
        assert_eq!(usage.tracking_started_unix_secs, None);
    }
}
