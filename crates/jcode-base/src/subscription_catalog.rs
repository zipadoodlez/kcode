use crate::provider_catalog;

pub const JCODE_API_KEY_ENV: &str = "JCODE_API_KEY";
pub const JCODE_API_BASE_ENV: &str = "JCODE_API_BASE";
pub const JCODE_ACCOUNT_ID_ENV: &str = "JCODE_ACCOUNT_ID";
pub const JCODE_ACCOUNT_EMAIL_ENV: &str = "JCODE_ACCOUNT_EMAIL";
pub const JCODE_TIER_ENV: &str = "JCODE_TIER";
pub const JCODE_ENV_FILE: &str = "jcode-subscription.env";
pub const JCODE_CACHE_NAMESPACE: &str = "jcode-subscription";
pub const JCODE_SUBSCRIPTION_ACTIVE_ENV: &str = "JCODE_SUBSCRIPTION_ACTIVE";
pub const DEFAULT_JCODE_API_BASE: &str = "https://api.jcode.sh/v1";
pub const JCODE_PRICING_URL: &str = "https://jcode.sh/pricing";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum JcodeTier {
    Plus,
    Flagship,
}

impl JcodeTier {
    pub const ALL: &'static [JcodeTier] = &[JcodeTier::Plus, JcodeTier::Flagship];

    pub fn retail_price_usd(self) -> u32 {
        match self {
            Self::Plus => 10,
            Self::Flagship => 1000,
        }
    }

    pub fn usable_budget_usd(self) -> f64 {
        match self {
            Self::Plus => 18.00,
            Self::Flagship => 3000.00,
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Plus => "Plus",
            Self::Flagship => "Flagship",
        }
    }

    /// Stable machine identifier used for wire values and local persistence.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Plus => "plus",
            Self::Flagship => "flagship",
        }
    }

    /// Parse a tier from a wire/persisted value (case-insensitive).
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "plus" => Some(Self::Plus),
            "flagship" => Some(Self::Flagship),
            _ => None,
        }
    }

    /// Whether an account on this tier may use a model gated at `required`.
    pub fn allows(self, required: JcodeTier) -> bool {
        self >= required
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
    /// Minimum subscription tier that may use this model.
    pub min_tier: JcodeTier,
    pub note: &'static str,
}

pub const CURATED_MODELS: &[CuratedModel] = &[
    CuratedModel {
        id: "claude-opus-4-8",
        display_name: "Claude Opus 4.8",
        aliases: &["claude-opus-4-8", "opus-4-8", "opus 4.8", "claude opus 4.8"],
        default_enabled: true,
        routing_policy: UpstreamRoutingPolicy::ServerManaged,
        min_tier: JcodeTier::Plus,
        note: "Frontier model; routed server-side to Anthropic by the jcode router.",
    },
    CuratedModel {
        id: "gpt-5.5",
        display_name: "GPT-5.5",
        aliases: &["gpt-5.5", "gpt-5-5", "gpt 5.5"],
        default_enabled: false,
        routing_policy: UpstreamRoutingPolicy::ServerManaged,
        min_tier: JcodeTier::Plus,
        note: "Frontier model; routed server-side to OpenAI by the jcode router.",
    },
    CuratedModel {
        id: "claude-fable-5",
        display_name: "Claude Fable 5",
        aliases: &["claude-fable-5", "fable-5", "fable 5", "claude fable 5"],
        default_enabled: false,
        routing_policy: UpstreamRoutingPolicy::ServerManaged,
        min_tier: JcodeTier::Flagship,
        note: "Flagship-tier model; routed server-side to Anthropic by the jcode router.",
    },
    CuratedModel {
        id: "gpt-5.6-sol",
        display_name: "GPT-5.6 Sol",
        aliases: &["gpt-5.6-sol", "gpt 5.6 sol", "sol"],
        default_enabled: false,
        routing_policy: UpstreamRoutingPolicy::ServerManaged,
        min_tier: JcodeTier::Flagship,
        note: "Flagship-tier model; routed server-side to OpenAI by the jcode router.",
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

/// The effective subscription tier for gating decisions.
///
/// `/v1/me` is the source of truth; the last-known tier is persisted to
/// `jcode-subscription.env` (`JCODE_TIER`). Unknown/absent tier behaves like
/// Plus for backward compatibility.
pub fn effective_tier() -> JcodeTier {
    cached_tier().unwrap_or(JcodeTier::Plus)
}

/// The last tier reported by the backend, if any was persisted.
pub fn cached_tier() -> Option<JcodeTier> {
    provider_catalog::load_env_value_from_env_or_config(JCODE_TIER_ENV, JCODE_ENV_FILE)
        .as_deref()
        .and_then(JcodeTier::parse)
}

/// Persist the last-known tier reported by the backend (`None` clears it).
pub fn store_cached_tier(tier: Option<JcodeTier>) -> anyhow::Result<()> {
    provider_catalog::save_env_value_to_env_file(
        JCODE_TIER_ENV,
        JCODE_ENV_FILE,
        tier.map(JcodeTier::as_str),
    )
}

/// Whether the current (cached) tier is allowed to use `model`.
/// Non-curated models return `false`.
pub fn is_model_allowed_for_current_tier(model: &str) -> bool {
    find_curated_model(model)
        .map(|curated| effective_tier().allows(curated.min_tier))
        .unwrap_or(false)
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
        assert_eq!(
            canonical_model_id("Claude Opus 4.8"),
            Some("claude-opus-4-8")
        );
        assert_eq!(canonical_model_id("gpt-5.5"), Some("gpt-5.5"));
        assert_eq!(canonical_model_id("GPT 5.5"), Some("gpt-5.5"));
        assert_eq!(canonical_model_id("fable-5"), Some("claude-fable-5"));
        assert_eq!(canonical_model_id("Claude Fable 5"), Some("claude-fable-5"));
        assert_eq!(canonical_model_id("sol"), Some("gpt-5.6-sol"));
        assert_eq!(canonical_model_id("GPT 5.6 Sol"), Some("gpt-5.6-sol"));
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
    fn tier_pricing_matches_launched_plans() {
        assert_eq!(JcodeTier::Plus.retail_price_usd(), 10);
        assert_eq!(JcodeTier::Plus.usable_budget_usd(), 18.00);
        assert_eq!(JcodeTier::Plus.display_name(), "Plus");
        assert_eq!(JcodeTier::Flagship.retail_price_usd(), 1000);
        assert_eq!(JcodeTier::Flagship.usable_budget_usd(), 3000.00);
        assert_eq!(JcodeTier::Flagship.display_name(), "Flagship");
    }

    #[test]
    fn tier_parse_round_trips() {
        for tier in JcodeTier::ALL {
            assert_eq!(JcodeTier::parse(tier.as_str()), Some(*tier));
        }
        assert_eq!(JcodeTier::parse("PLUS"), Some(JcodeTier::Plus));
        assert_eq!(JcodeTier::parse(" Flagship "), Some(JcodeTier::Flagship));
        assert_eq!(JcodeTier::parse("starter"), None);
    }

    #[test]
    fn tier_gating_orders_plus_below_flagship() {
        assert!(JcodeTier::Plus.allows(JcodeTier::Plus));
        assert!(!JcodeTier::Plus.allows(JcodeTier::Flagship));
        assert!(JcodeTier::Flagship.allows(JcodeTier::Plus));
        assert!(JcodeTier::Flagship.allows(JcodeTier::Flagship));
    }

    #[test]
    fn flagship_models_are_gated_above_plus() {
        for model in CURATED_MODELS {
            match model.id {
                "claude-fable-5" | "gpt-5.6-sol" => {
                    assert_eq!(model.min_tier, JcodeTier::Flagship)
                }
                _ => assert_eq!(model.min_tier, JcodeTier::Plus),
            }
        }
    }

    #[test]
    fn effective_tier_defaults_to_plus_when_unknown() {
        let _guard = crate::storage::lock_test_env();
        crate::env::remove_var(JCODE_TIER_ENV);
        let temp = tempfile::tempdir().expect("temp home");
        crate::env::set_var("JCODE_HOME", temp.path().to_string_lossy().to_string());

        assert_eq!(cached_tier(), None);
        assert_eq!(effective_tier(), JcodeTier::Plus);
        assert!(is_model_allowed_for_current_tier("claude-opus-4-8"));
        assert!(!is_model_allowed_for_current_tier("claude-fable-5"));

        store_cached_tier(Some(JcodeTier::Flagship)).expect("persist tier");
        assert_eq!(cached_tier(), Some(JcodeTier::Flagship));
        assert!(is_model_allowed_for_current_tier("claude-fable-5"));
        assert!(is_model_allowed_for_current_tier("gpt-5.6-sol"));

        store_cached_tier(None).expect("clear tier");
        assert_eq!(cached_tier(), None);

        crate::env::remove_var("JCODE_HOME");
        crate::env::remove_var(JCODE_TIER_ENV);
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
