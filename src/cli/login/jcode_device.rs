//! Jcode subscription device-code (magic-link) login flow.
//!
//! Contract (see subscription-worker/ for the live backend):
//! - `POST {auth_base}/v1/auth/device {"email": "..."}` ->
//!   `{device_code, verify_url, expires_in, interval}`
//! - `POST {auth_base}/v1/auth/token {"device_code": "..."}` ->
//!   HTTP 202 (or `{"status":"pending"}`) until approved, then
//!   `{api_key, account_id, email, tier}`
//!
//! `auth_base` is derived by stripping a trailing `/v1` from the configured
//! jcode API base (`JCODE_API_BASE` / `DEFAULT_JCODE_API_BASE`), since the
//! auth endpoints live at the service root rather than under the model API
//! `/v1` prefix.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::time::Duration;

#[derive(Debug, Clone, Deserialize)]
pub(super) struct DeviceAuthResponse {
    pub device_code: String,
    pub verify_url: String,
    #[serde(default = "default_expires_in")]
    pub expires_in: u64,
    #[serde(default = "default_interval")]
    pub interval: u64,
}

fn default_expires_in() -> u64 {
    900
}

fn default_interval() -> u64 {
    5
}

#[derive(Debug, Clone, Deserialize)]
pub(super) struct TokenApprovedResponse {
    pub api_key: String,
    #[serde(default)]
    pub account_id: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub tier: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct TokenErrorResponse {
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    error: Option<ErrorField>,
    #[serde(default)]
    error_description: Option<String>,
    #[serde(default)]
    message: Option<String>,
}

/// The backend nests errors as `{"error":{"code","message"}}`; also accept a
/// flat OAuth-style `{"error":"code","error_description":"..."}` shape.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum ErrorField {
    Code(String),
    Object {
        #[serde(default)]
        code: Option<String>,
        #[serde(default)]
        message: Option<String>,
    },
}

impl TokenErrorResponse {
    fn code(&self) -> Option<&str> {
        match &self.error {
            Some(ErrorField::Code(code)) => Some(code.as_str()),
            Some(ErrorField::Object { code, .. }) => code.as_deref(),
            None => self.status.as_deref(),
        }
    }

    fn description(&self) -> Option<String> {
        if let Some(ErrorField::Object {
            message: Some(message),
            ..
        }) = &self.error
        {
            return Some(message.clone());
        }
        self.error_description
            .clone()
            .or_else(|| self.message.clone())
    }
}

/// Outcome of a single `/v1/auth/token` poll attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum PollOutcome {
    Pending,
    SlowDown,
    Approved(TokenApprovedState),
    Expired,
    Denied(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TokenApprovedState {
    pub api_key: String,
    pub account_id: Option<String>,
    pub email: Option<String>,
    pub tier: Option<String>,
}

/// Derive the auth service base URL from the configured (or default) jcode
/// model API base by stripping a trailing `/v1` segment.
pub(super) fn auth_base_url() -> String {
    let base = crate::subscription_catalog::configured_api_base()
        .unwrap_or_else(|| crate::subscription_catalog::DEFAULT_JCODE_API_BASE.to_string());
    strip_v1_suffix(&base)
}

pub(super) fn strip_v1_suffix(base: &str) -> String {
    let trimmed = base.trim().trim_end_matches('/');
    trimmed
        .strip_suffix("/v1")
        .unwrap_or(trimmed)
        .trim_end_matches('/')
        .to_string()
}

pub(super) async fn request_device_code(
    client: &reqwest::Client,
    auth_base: &str,
    email: &str,
) -> Result<DeviceAuthResponse> {
    let url = format!("{}/v1/auth/device", auth_base.trim_end_matches('/'));
    let resp = client
        .post(&url)
        .json(&serde_json::json!({ "email": email }))
        .send()
        .await
        .with_context(|| format!("Failed to reach jcode auth service at {}", url))?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!(
            "jcode auth service rejected the device authorization request (HTTP {}): {}",
            status.as_u16(),
            body.trim()
        );
    }

    resp.json::<DeviceAuthResponse>()
        .await
        .context("Failed to parse device authorization response")
}

/// Perform one poll of `/v1/auth/token` and classify the response.
pub(super) async fn poll_token_once(
    client: &reqwest::Client,
    auth_base: &str,
    device_code: &str,
) -> Result<PollOutcome> {
    let url = format!("{}/v1/auth/token", auth_base.trim_end_matches('/'));
    let resp = client
        .post(&url)
        .json(&serde_json::json!({ "device_code": device_code }))
        .send()
        .await
        .with_context(|| format!("Failed to reach jcode auth service at {}", url))?;

    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();

    if status.as_u16() == 202 || status.as_u16() == 428 {
        return Ok(PollOutcome::Pending);
    }
    if status.as_u16() == 429 {
        return Ok(PollOutcome::SlowDown);
    }

    if status.is_success() {
        // Some backends signal pending with 200 + {"status":"pending"}.
        if let Ok(err_body) = serde_json::from_str::<TokenErrorResponse>(&body) {
            if matches!(
                err_body.status.as_deref(),
                Some("pending") | Some("authorization_pending")
            ) {
                return Ok(PollOutcome::Pending);
            }
        }
        let approved: TokenApprovedResponse = serde_json::from_str(&body)
            .context("Failed to parse approved token response from jcode auth service")?;
        if approved.api_key.trim().is_empty() {
            anyhow::bail!("jcode auth service returned an empty API key");
        }
        return Ok(PollOutcome::Approved(TokenApprovedState {
            api_key: approved.api_key,
            account_id: approved.account_id,
            email: approved.email,
            tier: approved.tier,
        }));
    }

    let parsed: TokenErrorResponse = serde_json::from_str(&body).unwrap_or_default();
    let code = parsed.code().unwrap_or("");
    match code {
        "authorization_pending" | "pending" => Ok(PollOutcome::Pending),
        "slow_down" => Ok(PollOutcome::SlowDown),
        "expired_token" | "expired" | "expired_device_code" => Ok(PollOutcome::Expired),
        "access_denied" | "denied" => Ok(PollOutcome::Denied(
            parsed
                .description()
                .unwrap_or_else(|| "Authorization was denied.".to_string()),
        )),
        _ if status.as_u16() == 404 || status.as_u16() == 410 => Ok(PollOutcome::Expired),
        _ => anyhow::bail!(
            "jcode auth service returned an unexpected error (HTTP {}): {}",
            status.as_u16(),
            body.trim()
        ),
    }
}

/// Poll `/v1/auth/token` until approval, denial, or expiry.
///
/// `interval` and `expires_in` come from the device authorization response.
pub(super) async fn poll_for_api_key(
    client: &reqwest::Client,
    auth_base: &str,
    device_code: &str,
    interval: u64,
    expires_in: u64,
) -> Result<TokenApprovedState> {
    let interval = interval.max(1);
    let deadline = std::time::Instant::now() + Duration::from_secs(expires_in.max(interval));
    let mut wait = Duration::from_secs(interval);

    loop {
        tokio::time::sleep(wait).await;
        if std::time::Instant::now() >= deadline {
            anyhow::bail!(
                "The sign-in link expired before it was approved. Run `jcode login jcode` again."
            );
        }
        match poll_token_once(client, auth_base, device_code).await? {
            PollOutcome::Pending => {}
            PollOutcome::SlowDown => {
                wait += Duration::from_secs(5);
            }
            PollOutcome::Approved(state) => return Ok(state),
            PollOutcome::Expired => {
                anyhow::bail!(
                    "The sign-in link expired before it was approved. Run `jcode login jcode` again."
                );
            }
            PollOutcome::Denied(reason) => {
                anyhow::bail!("jcode sign-in was denied: {}", reason);
            }
        }
    }
}

/// Persist an approved subscription credential set to the jcode-subscription
/// env file and process environment, preserving any configured API base.
pub(super) fn persist_subscription_credentials(state: &TokenApprovedState) -> Result<()> {
    use crate::subscription_catalog as cat;

    crate::provider_catalog::save_env_value_to_env_file(
        cat::JCODE_API_KEY_ENV,
        cat::JCODE_ENV_FILE,
        Some(state.api_key.trim()),
    )?;
    crate::provider_catalog::save_env_value_to_env_file(
        cat::JCODE_ACCOUNT_ID_ENV,
        cat::JCODE_ENV_FILE,
        state
            .account_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty()),
    )?;
    crate::provider_catalog::save_env_value_to_env_file(
        cat::JCODE_ACCOUNT_EMAIL_ENV,
        cat::JCODE_ENV_FILE,
        state
            .email
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty()),
    )?;
    crate::provider_catalog::save_env_value_to_env_file(
        cat::JCODE_TIER_ENV,
        cat::JCODE_ENV_FILE,
        state
            .tier
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty()),
    )?;
    Ok(())
}

/// Full interactive device-code login flow for the jcode subscription.
pub(super) async fn login_jcode_device_flow(email: &str, no_browser: bool) -> Result<()> {
    let client = crate::provider::shared_http_client();
    let auth_base = auth_base_url();

    let device = request_device_code(&client, &auth_base, email).await?;

    eprintln!();
    eprintln!("  Check your email ({}) for a sign-in link.", email);
    eprintln!("  If it does not arrive, open this URL to approve the sign-in:");
    eprintln!("    {}", device.verify_url);
    eprintln!();
    eprintln!("  Waiting for approval...");

    super::maybe_open_browser(&device.verify_url, no_browser);

    let approved = poll_for_api_key(
        &client,
        &auth_base,
        &device.device_code,
        device.interval,
        device.expires_in,
    )
    .await?;

    persist_subscription_credentials(&approved)?;
    crate::auth::AuthStatus::invalidate_cache();

    let config_dir = crate::storage::app_config_dir()?;
    eprintln!();
    eprintln!(
        "  ✓ Signed in to the jcode subscription{}{}",
        approved
            .email
            .as_deref()
            .map(|value| format!(" as {}", value))
            .unwrap_or_default(),
        approved
            .tier
            .as_deref()
            .map(|value| format!(" ({} tier)", value))
            .unwrap_or_default(),
    );
    eprintln!(
        "  Credentials stored at {}",
        config_dir
            .join(crate::subscription_catalog::JCODE_ENV_FILE)
            .display()
    );

    crate::telemetry::record_auth_success("jcode-subscription", "device_code_magic_link");
    // TODO(telemetry): emit a dedicated `account_linked` event carrying
    // `account_id` once jcode-telemetry-core grows a generic event with an
    // account field; the current AuthEvent schema has no account_id slot and
    // adding one is out of scope for this change.

    Ok(())
}

#[cfg(test)]
mod tests;
