use crate::provider_catalog;

pub const JCODE_API_KEY_ENV: &str = "JCODE_API_KEY";
pub const JCODE_API_BASE_ENV: &str = "JCODE_API_BASE";
pub const JCODE_ENV_FILE: &str = "jcode-subscription.env";
pub const JCODE_CACHE_NAMESPACE: &str = "jcode-subscription";
pub const JCODE_SUBSCRIPTION_ACTIVE_ENV: &str = "JCODE_SUBSCRIPTION_ACTIVE";
pub const DEFAULT_JCODE_API_BASE: &str = "https://subscription.jcode.invalid/v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JcodeTier {
    Starter20,
    Pro100,
}

impl JcodeTier {
    pub fn retail_price_usd(self) -> u32 {
        match self {
            Self::Starter20 => 20,
            Self::Pro100 => 100,
        }
    }

    pub fn usable_budget_usd(self) -> f64 {
        match self {
            Self::Starter20 => 18.12,
            Self::Pro100 => 91.75,
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Starter20 => "$20 Starter",
            Self::Pro100 => "$100 Pro",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpstreamRoutingPolicy {
    /// Routing is decided server-side by the jcode router (model -> provider +
    /// org key). The client does not pick upstreams; this is the only policy for
    /// the managed subscription.
    ServerManaged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CuratedModel {
    pub id: &'static str,
    pub display_name: &'static str,
    pub aliases: &'static [&'static str],
    pub default_enabled: bool,
    pub routing_policy: UpstreamRoutingPolicy,
    pub note: &'static str,
}

pub const CURATED_MODELS: &[CuratedModel] = &[
    CuratedModel {
        id: "claude-opus-4-8",
        display_name: "Claude Opus 4.8",
        aliases: &["claude-opus-4-8", "opus-4-8", "opus 4.8", "claude opus 4.8"],
        default_enabled: true,
        routing_policy: UpstreamRoutingPolicy::ServerManaged,
        note: "Frontier model; routed server-side to Anthropic by the jcode router.",
    },
    CuratedModel {
        id: "gpt-5.5",
        display_name: "GPT-5.5",
        aliases: &["gpt-5.5", "gpt-5-5", "gpt 5.5"],
        default_enabled: false,
        routing_policy: UpstreamRoutingPolicy::ServerManaged,
        note: "Frontier model; routed server-side to OpenAI by the jcode router.",
    },
];

pub fn curated_models() -> &'static [CuratedModel] {
    CURATED_MODELS
}

pub fn default_model() -> &'static CuratedModel {
    CURATED_MODELS
        .iter()
        .find(|model| model.default_enabled)
        .unwrap_or(&CURATED_MODELS[0])
}

/// Normalize a model id for curated-catalog matching: strips any `@provider`
/// routing suffix, the `[1m]` long-context suffix, and lowercases.
fn normalize_model_key(model: &str) -> String {
    let base = model.trim().split('@').next().unwrap_or("").trim();
    jcode_provider_core::model_id::canonical(base)
}

pub fn find_curated_model(model: &str) -> Option<&'static CuratedModel> {
    let normalized = normalize_model_key(model);
    CURATED_MODELS.iter().find(|candidate| {
        candidate.id.eq_ignore_ascii_case(&normalized)
            || candidate
                .aliases
                .iter()
                .any(|alias| alias.eq_ignore_ascii_case(&normalized))
    })
}

pub fn canonical_model_id(model: &str) -> Option<&'static str> {
    find_curated_model(model).map(|model| model.id)
}

pub fn is_curated_model(model: &str) -> bool {
    canonical_model_id(model).is_some()
}

pub fn routing_policy_detail(model: &CuratedModel) -> String {
    match model.routing_policy {
        UpstreamRoutingPolicy::ServerManaged => {
            "jcode subscription routing · managed server-side".to_string()
        }
    }
}

pub fn configured_api_key() -> Option<String> {
    provider_catalog::load_env_value_from_env_or_config(JCODE_API_KEY_ENV, JCODE_ENV_FILE)
}

pub fn configured_api_base() -> Option<String> {
    provider_catalog::load_env_value_from_env_or_config(JCODE_API_BASE_ENV, JCODE_ENV_FILE)
}

pub fn has_credentials() -> bool {
    configured_api_key().is_some()
}

pub fn has_router_base() -> bool {
    configured_api_base().is_some()
}

pub fn is_runtime_mode_enabled() -> bool {
    std::env::var(JCODE_SUBSCRIPTION_ACTIVE_ENV)
        .ok()
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes"
            )
        })
        .unwrap_or(false)
}

pub fn apply_runtime_env() {
    crate::env::set_var(JCODE_SUBSCRIPTION_ACTIVE_ENV, "1");
    crate::env::set_var(
        "JCODE_OPENROUTER_API_BASE",
        configured_api_base().unwrap_or_else(|| DEFAULT_JCODE_API_BASE.to_string()),
    );
    crate::env::set_var("JCODE_OPENROUTER_API_KEY_NAME", JCODE_API_KEY_ENV);
    crate::env::set_var("JCODE_OPENROUTER_ENV_FILE", JCODE_ENV_FILE);
    crate::env::set_var("JCODE_OPENROUTER_CACHE_NAMESPACE", JCODE_CACHE_NAMESPACE);
    crate::env::set_var("JCODE_OPENROUTER_PROVIDER_FEATURES", "0");
    crate::env::set_var("JCODE_OPENROUTER_TRANSPORT_STATE", "jcode-subscription");
    crate::env::remove_var("JCODE_OPENROUTER_ALLOW_NO_AUTH");
    crate::env::remove_var("JCODE_OPENROUTER_PROVIDER");
    crate::env::remove_var("JCODE_OPENROUTER_NO_FALLBACK");
}

pub fn clear_runtime_env() {
    crate::env::remove_var(JCODE_SUBSCRIPTION_ACTIVE_ENV);
    crate::env::remove_var("JCODE_OPENROUTER_API_BASE");
    crate::env::remove_var("JCODE_OPENROUTER_API_KEY_NAME");
    crate::env::remove_var("JCODE_OPENROUTER_ENV_FILE");
    crate::env::remove_var("JCODE_OPENROUTER_CACHE_NAMESPACE");
    crate::env::remove_var("JCODE_OPENROUTER_PROVIDER_FEATURES");
    crate::env::remove_var("JCODE_OPENROUTER_TRANSPORT_STATE");
    crate::env::remove_var("JCODE_OPENROUTER_ALLOW_NO_AUTH");
    crate::env::remove_var("JCODE_OPENROUTER_PROVIDER");
    crate::env::remove_var("JCODE_OPENROUTER_NO_FALLBACK");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curated_model_aliases_resolve_to_canonical_ids() {
        assert_eq!(canonical_model_id("opus 4.8"), Some("claude-opus-4-8"));
        assert_eq!(canonical_model_id("Claude Opus 4.8"), Some("claude-opus-4-8"));
        assert_eq!(canonical_model_id("gpt-5.5"), Some("gpt-5.5"));
        assert_eq!(canonical_model_id("GPT 5.5"), Some("gpt-5.5"));
        assert_eq!(canonical_model_id("unknown-model"), None);
    }

    #[test]
    fn curated_model_lookup_ignores_provider_pin_suffix() {
        assert_eq!(
            canonical_model_id("claude-opus-4-8@anthropic"),
            Some("claude-opus-4-8")
        );
        assert_eq!(canonical_model_id("gpt-5.5@openai"), Some("gpt-5.5"));
    }

    #[test]
    fn default_model_is_opus() {
        assert_eq!(default_model().id, "claude-opus-4-8");
    }

    #[test]
    fn runtime_mode_flag_tracks_subscription_activation() {
        let _guard = crate::storage::lock_test_env();
        clear_runtime_env();
        assert!(!is_runtime_mode_enabled());

        apply_runtime_env();
        assert!(is_runtime_mode_enabled());

        clear_runtime_env();
        assert!(!is_runtime_mode_enabled());
    }
}
