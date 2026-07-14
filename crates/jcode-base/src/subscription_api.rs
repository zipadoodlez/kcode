//! Typed client for the jcode subscription backend account endpoint.
//!
//! `GET /v1/me` is the source of truth for the account's tier and usage. The
//! last-known tier is persisted via
//! [`crate::subscription_catalog::store_cached_tier`] so model gating works
//! offline (unknown/absent tier behaves like Plus).

use crate::subscription_catalog::{self, JcodeTier};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Timeout for the short, non-blocking status fetch used by the TUI.
pub const ME_FETCH_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionUsage {
    pub used_usd: f64,
    pub budget_usd: f64,
    /// RFC 3339 timestamp for when the usage window resets.
    #[serde(default)]
    pub resets_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionMe {
    pub account_id: String,
    pub email: String,
    /// Stable wire tier value: "plus", "pro", "max", "ultra", or "flagship".
    pub tier: String,
    pub status: String,
    pub usage: SubscriptionUsage,
}

impl SubscriptionMe {
    pub fn parsed_tier(&self) -> Option<JcodeTier> {
        JcodeTier::parse(&self.tier)
    }
}

/// The `/v1/me` endpoint URL for the configured (or default) API base.
pub fn me_endpoint_url() -> String {
    let base = subscription_catalog::configured_api_base()
        .unwrap_or_else(|| subscription_catalog::DEFAULT_JCODE_API_BASE.to_string());
    format!("{}/me", base.trim_end_matches('/'))
}

/// Fetch the subscription account status from the backend using the
/// configured `JCODE_API_KEY` / `JCODE_API_BASE`.
///
/// On success, persists the reported tier as the last-known tier so offline
/// model gating stays accurate.
pub async fn fetch_subscription_me() -> Result<SubscriptionMe> {
    let api_key = subscription_catalog::configured_api_key()
        .context("no jcode subscription credential configured (run /login jcode)")?;

    let client = crate::provider::shared_http_client();
    let response = client
        .get(me_endpoint_url())
        .bearer_auth(api_key)
        .timeout(ME_FETCH_TIMEOUT)
        .send()
        .await
        .context("failed to reach the jcode subscription API")?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!(
            "jcode subscription API returned {}: {}",
            status,
            body.chars().take(200).collect::<String>()
        );
    }

    let me: SubscriptionMe = response
        .json()
        .await
        .context("failed to parse jcode subscription /me response")?;

    // Persist the backend-reported tier as the local source of truth for
    // offline gating. Best-effort; failure to persist should not fail the fetch.
    if let Some(tier) = me.parsed_tier() {
        let _ = subscription_catalog::store_cached_tier(Some(tier));
    }

    Ok(me)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subscription_me_parses_expected_shape() {
        let json = r#"{
            "account_id": "acct_123",
            "email": "dev@example.com",
            "tier": "flagship",
            "status": "active",
            "usage": {
                "used_usd": 12.5,
                "budget_usd": 3000.0,
                "resets_at": "2026-08-01T00:00:00Z"
            }
        }"#;
        let me: SubscriptionMe = serde_json::from_str(json).expect("parse SubscriptionMe");
        assert_eq!(me.account_id, "acct_123");
        assert_eq!(me.email, "dev@example.com");
        assert_eq!(me.parsed_tier(), Some(JcodeTier::Flagship));
        assert_eq!(me.status, "active");
        assert_eq!(me.usage.used_usd, 12.5);
        assert_eq!(me.usage.budget_usd, 3000.0);
        assert_eq!(me.usage.resets_at.as_deref(), Some("2026-08-01T00:00:00Z"));
    }

    #[test]
    fn subscription_me_tolerates_missing_resets_at_and_unknown_tier() {
        let json = r#"{
            "account_id": "acct_9",
            "email": "x@example.com",
            "tier": "mystery",
            "status": "active",
            "usage": { "used_usd": 0.0, "budget_usd": 18.0 }
        }"#;
        let me: SubscriptionMe = serde_json::from_str(json).expect("parse SubscriptionMe");
        assert_eq!(me.parsed_tier(), None);
        assert!(me.usage.resets_at.is_none());
    }

    #[test]
    fn me_endpoint_url_appends_me_to_configured_base() {
        let _guard = crate::storage::lock_test_env();
        crate::env::set_var(
            subscription_catalog::JCODE_API_BASE_ENV,
            "https://api.jcode.sh/v1/",
        );
        assert_eq!(me_endpoint_url(), "https://api.jcode.sh/v1/me");
        crate::env::remove_var(subscription_catalog::JCODE_API_BASE_ENV);
    }
}
