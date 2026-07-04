//! OpenRouter / OpenAI-compatible provider shared helpers (compatibility shim).
//!
//! The OpenRouter provider *runtime* (`OpenRouterProvider`) now lives in the
//! downstream `jcode-provider-openrouter-runtime` crate so provider edits do
//! not rebuild the base -> app-core -> tui spine. The binary's composition
//! root registers a parameterized factory via
//! [`crate::provider::external::register_openrouter_factory`].
//!
//! Base keeps what its own routing/auth/TUI surfaces share with the runtime:
//! - the env-derived endpoint/key-name/auth-mode configuration helpers,
//! - [`OpenRouterTransportState`] (used by the TUI header and auth lifecycle),
//! - the credential probe (`has_credentials`), and
//! - re-exports of the pure catalog/cache types from
//!   `jcode-provider-openrouter`.

use crate::provider_catalog::{
    OPENAI_COMPAT_PROFILE, is_safe_env_file_name, is_safe_env_key_name,
    load_api_key_from_env_or_config, normalize_api_base, openai_compatible_profile_is_configured,
    openai_compatible_profiles, resolve_openai_compatible_profile,
};
pub use jcode_provider_openrouter::{
    EndpointInfo, ModelInfo, ModelPricing, ModelTimestampIndex, ProviderRouting,
    all_model_timestamps, load_endpoints_disk_cache_public, load_model_pricing_disk_cache_public,
    load_model_timestamp_index, model_created_timestamp, model_created_timestamp_from_index,
};
use reqwest::header::HeaderName;

/// Schedule a background catalog refresh for a direct OpenAI-compatible
/// profile through the composition-root hook (implemented by the runtime
/// crate). Kept at its historical path for callers.
pub(crate) fn maybe_schedule_openai_compatible_profile_catalog_refresh(
    profile: crate::provider_catalog::OpenAiCompatibleProfile,
    context: &'static str,
) -> bool {
    super::external::maybe_schedule_profile_catalog_refresh(profile, context)
}

/// Schedule a background refresh of the standard public OpenRouter catalog
/// through the composition-root hook. Kept at its historical path.
pub(crate) fn maybe_schedule_standard_openrouter_catalog_refresh(context: &'static str) -> bool {
    super::external::maybe_schedule_standard_openrouter_catalog_refresh(context)
}

/// Whether OpenRouter/OpenAI-compatible credentials are available.
pub fn has_credentials() -> bool {
    if matches!(
        configured_dynamic_bearer_provider().as_deref(),
        Some("azure")
    ) {
        return crate::auth::azure::has_configuration();
    }
    if configured_allow_no_auth() {
        return true;
    }
    get_api_key().is_some()
}

/// Resolve the configured API key for the OpenRouter/OpenAI-compatible slot.
pub fn get_api_key() -> Option<String> {
    let key_name = configured_api_key_name();
    let env_file = configured_env_file_name();
    load_api_key_from_env_or_config(&key_name, &env_file)
}

/// OpenRouter API base URL
const DEFAULT_API_BASE: &str = "https://openrouter.ai/api/v1";
const DEFAULT_API_KEY_NAME: &str = "OPENROUTER_API_KEY";
const DEFAULT_ENV_FILE: &str = "openrouter.env";
const OPENROUTER_TRANSPORT_STATE_ENV: &str = "JCODE_OPENROUTER_TRANSPORT_STATE";
const KIMI_CODING_USER_AGENT: &str = "claude-cli/1.0.0";
const KIMI_CODING_X_APP: &str = "cli";

/// Default model (Claude Sonnet via OpenRouter)
const DEFAULT_MODEL: &str = "anthropic/claude-sonnet-4";

/// Soft refresh TTL for the model catalog.
///
/// We keep the 24h disk cache for resilience/offline startup, but after this
/// shorter interval we refresh in the background so new models appear quickly
/// without blocking the picker UI.
const MODEL_CATALOG_SOFT_REFRESH_SECS: u64 = 15 * 60;
/// Minimum delay between background refresh attempts.
const MODEL_CATALOG_REFRESH_RETRY_SECS: u64 = 60;
/// Standard OpenRouter catalog freshness window for the inactive-slot refresh
/// path. Matches the shared on-disk model-catalog TTL (24h).
const STANDARD_OPENROUTER_CATALOG_TTL_SECS: u64 = 24 * 60 * 60;

/// Endpoints cache TTL (1 hour) - per-model provider endpoint data
const ENDPOINTS_CACHE_TTL_SECS: u64 = 60 * 60;
const MAX_BACKGROUND_ENDPOINT_REFRESHES: usize = 8;

fn explicit_openrouter_runtime_configured() -> bool {
    [
        "JCODE_OPENROUTER_API_BASE",
        "JCODE_OPENROUTER_API_KEY_NAME",
        "JCODE_OPENROUTER_ENV_FILE",
        "JCODE_OPENROUTER_DYNAMIC_BEARER_PROVIDER",
    ]
    .iter()
    .any(|var| std::env::var_os(var).is_some())
}

fn autodetected_openai_compatible_profile()
-> Option<crate::provider_catalog::ResolvedOpenAiCompatibleProfile> {
    if explicit_openrouter_runtime_configured() {
        return None;
    }

    if load_api_key_from_env_or_config(DEFAULT_API_KEY_NAME, DEFAULT_ENV_FILE).is_some() {
        return None;
    }

    let compat = resolve_openai_compatible_profile(OPENAI_COMPAT_PROFILE);
    if load_api_key_from_env_or_config(&compat.api_key_env, &compat.env_file).is_some() {
        return Some(compat);
    }

    let mut matches = openai_compatible_profiles()
        .iter()
        .filter(|profile| profile.id != OPENAI_COMPAT_PROFILE.id)
        .filter_map(|profile| {
            let resolved = resolve_openai_compatible_profile(*profile);
            if crate::provider_catalog::openai_compatible_profile_is_configured(*profile) {
                Some(resolved)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    if matches.len() == 1 {
        matches.pop()
    } else {
        None
    }
}

fn configured_api_base() -> String {
    let raw = std::env::var("JCODE_OPENROUTER_API_BASE")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .or_else(|| autodetected_openai_compatible_profile().map(|profile| profile.api_base))
        .unwrap_or_else(|| DEFAULT_API_BASE.to_string());
    normalize_api_base(&raw).unwrap_or_else(|| {
        crate::logging::warn(&format!(
            "Ignoring invalid JCODE_OPENROUTER_API_BASE '{}'; using {}",
            raw, DEFAULT_API_BASE
        ));
        DEFAULT_API_BASE.to_string()
    })
}

fn configured_api_key_name() -> String {
    let raw = std::env::var("JCODE_OPENROUTER_API_KEY_NAME")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .or_else(|| autodetected_openai_compatible_profile().map(|profile| profile.api_key_env))
        .unwrap_or_else(|| DEFAULT_API_KEY_NAME.to_string());
    if is_safe_env_key_name(&raw) {
        raw
    } else {
        crate::logging::warn(&format!(
            "Ignoring invalid JCODE_OPENROUTER_API_KEY_NAME '{}'; using {}",
            raw, DEFAULT_API_KEY_NAME
        ));
        DEFAULT_API_KEY_NAME.to_string()
    }
}

fn configured_env_file_name() -> String {
    let raw = std::env::var("JCODE_OPENROUTER_ENV_FILE")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .or_else(|| autodetected_openai_compatible_profile().map(|profile| profile.env_file))
        .unwrap_or_else(|| DEFAULT_ENV_FILE.to_string());
    if is_safe_env_file_name(&raw) {
        raw
    } else {
        crate::logging::warn(&format!(
            "Ignoring invalid JCODE_OPENROUTER_ENV_FILE '{}'; using {}",
            raw, DEFAULT_ENV_FILE
        ));
        DEFAULT_ENV_FILE.to_string()
    }
}

fn load_named_profile_api_key(
    env_key: &str,
    profile: &crate::config::NamedProviderConfig,
) -> Option<String> {
    if let Some(env_file) = profile
        .env_file
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return load_api_key_from_env_or_config(env_key, env_file);
    }

    std::env::var(env_key)
        .ok()
        .map(|key| key.trim().to_string())
        .filter(|key| !key.is_empty())
}

fn parse_env_bool(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

fn provider_features_enabled(api_base: &str) -> bool {
    if let Ok(raw) = std::env::var("JCODE_OPENROUTER_PROVIDER_FEATURES") {
        if let Some(value) = parse_env_bool(&raw) {
            return value;
        }
        crate::logging::warn(&format!(
            "Ignoring invalid JCODE_OPENROUTER_PROVIDER_FEATURES '{}'; expected true/false",
            raw
        ));
    }
    api_base.contains("openrouter.ai")
}

fn model_catalog_enabled() -> bool {
    if let Ok(raw) = std::env::var("JCODE_OPENROUTER_MODEL_CATALOG") {
        if let Some(value) = parse_env_bool(&raw) {
            return value;
        }
        crate::logging::warn(&format!(
            "Ignoring invalid JCODE_OPENROUTER_MODEL_CATALOG '{}'; expected true/false",
            raw
        ));
    }
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AuthHeaderMode {
    AuthorizationBearer,
    ApiKey,
}

fn configured_auth_header_mode() -> AuthHeaderMode {
    let Some(raw) = std::env::var("JCODE_OPENROUTER_AUTH_HEADER")
        .ok()
        .map(|v| v.trim().to_ascii_lowercase())
        .filter(|v| !v.is_empty())
    else {
        return AuthHeaderMode::AuthorizationBearer;
    };

    match raw.as_str() {
        "authorization" | "authorization-bearer" | "bearer" => AuthHeaderMode::AuthorizationBearer,
        "api-key" | "apikey" => AuthHeaderMode::ApiKey,
        other => {
            crate::logging::warn(&format!(
                "Ignoring invalid JCODE_OPENROUTER_AUTH_HEADER '{}'; expected authorization-bearer or api-key",
                other
            ));
            AuthHeaderMode::AuthorizationBearer
        }
    }
}

fn configured_auth_header_name() -> HeaderName {
    let raw = std::env::var("JCODE_OPENROUTER_AUTH_HEADER_NAME")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "api-key".to_string());
    HeaderName::from_bytes(raw.as_bytes()).unwrap_or_else(|_| {
        crate::logging::warn(&format!(
            "Ignoring invalid JCODE_OPENROUTER_AUTH_HEADER_NAME '{}'; using api-key",
            raw
        ));
        HeaderName::from_static("api-key")
    })
}

fn configured_dynamic_bearer_provider() -> Option<String> {
    std::env::var("JCODE_OPENROUTER_DYNAMIC_BEARER_PROVIDER")
        .ok()
        .map(|v| v.trim().to_ascii_lowercase())
        .filter(|v| !v.is_empty())
}

fn configured_allow_no_auth() -> bool {
    std::env::var("JCODE_OPENROUTER_ALLOW_NO_AUTH")
        .ok()
        .and_then(|raw| parse_env_bool(&raw))
        .or_else(|| {
            autodetected_openai_compatible_profile().and_then(|profile| {
                if profile.requires_api_key {
                    None
                } else {
                    Some(true)
                }
            })
        })
        .unwrap_or(false)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenRouterTransportState {
    /// Real OpenRouter BYOK. The provider implementation is both the runtime identity
    /// and the HTTP transport.
    OpenRouterApiKey,
    /// Jcode subscription access currently reuses the OpenRouter HTTP slot, but is
    /// not user BYOK/OpenRouter billing.
    JcodeSubscription,
    /// A direct OpenAI-compatible endpoint that needs a user key, Azure credential,
    /// or provider-profile secret while reusing the OpenRouter-compatible transport.
    DirectApiKey,
    /// A direct local/no-auth OpenAI-compatible endpoint, for example Ollama or LM Studio.
    DirectNoAuth,
}

impl OpenRouterTransportState {
    pub fn from_current_env(runtime_provider: Option<&str>) -> Self {
        if let Some(state) = Self::from_env_marker() {
            return state;
        }

        let runtime_provider = runtime_provider
            .map(|value| value.trim().to_ascii_lowercase())
            .filter(|value| !value.is_empty());

        if matches!(runtime_provider.as_deref(), Some("jcode")) {
            return Self::JcodeSubscription;
        }

        if matches!(runtime_provider.as_deref(), Some("openrouter")) {
            return Self::OpenRouterApiKey;
        }

        if configured_allow_no_auth() {
            return Self::DirectNoAuth;
        }

        if Self::runtime_provider_is_direct_compatible(runtime_provider.as_deref())
            || std::env::var_os("JCODE_NAMED_PROVIDER_PROFILE").is_some()
        {
            return Self::DirectApiKey;
        }

        let api_base = configured_api_base();
        if provider_features_enabled(&api_base) {
            Self::OpenRouterApiKey
        } else {
            Self::DirectApiKey
        }
    }

    fn from_env_marker() -> Option<Self> {
        let raw = std::env::var(OPENROUTER_TRANSPORT_STATE_ENV).ok()?;
        let value = raw.trim().to_ascii_lowercase();
        if value.is_empty() {
            return None;
        }

        match value.as_str() {
            "openrouter" | "openrouter-api-key" | "openrouter_byok" | "openrouter-byok" => {
                Some(Self::OpenRouterApiKey)
            }
            "jcode" | "jcode-subscription" | "subscription" => Some(Self::JcodeSubscription),
            "direct" | "direct-api-key" | "openai-compatible" | "compatible-api-key" => {
                Some(Self::DirectApiKey)
            }
            "direct-no-auth" | "no-auth" | "local" => Some(Self::DirectNoAuth),
            other => {
                crate::logging::warn(&format!(
                    "Ignoring invalid {} '{}'; expected openrouter-api-key, jcode-subscription, direct-api-key, or direct-no-auth",
                    OPENROUTER_TRANSPORT_STATE_ENV, other
                ));
                None
            }
        }
    }

    fn runtime_provider_is_direct_compatible(runtime_provider: Option<&str>) -> bool {
        matches!(runtime_provider, Some("openai-compatible" | "azure-openai"))
            || runtime_provider
                .and_then(crate::provider_catalog::openai_compatible_profile_by_id)
                .is_some()
    }

    pub fn accrues_user_api_key_cost(self) -> bool {
        matches!(self, Self::OpenRouterApiKey | Self::DirectApiKey)
    }

    pub fn is_real_openrouter(self) -> bool {
        matches!(self, Self::OpenRouterApiKey)
    }
}
