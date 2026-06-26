//! Pick the "next best" model route to fall back to after a provider error.
//!
//! When the active model/route fails (auth error, rate limit, provider outage,
//! a broken API key while an OAuth login still works, ...), the UI offers the
//! user a one-keypress switch to a usable alternative and resends the turn. This
//! module is the single, pure source of truth for *which* alternative to offer.
//!
//! It is intentionally provider-agnostic and side-effect free so it can be unit
//! tested exhaustively and reused by both the local and remote TUI paths.

use crate::{ModelRoute, ModelRouteApiMethod, model_route_provider_labels_match};

/// Whether an api_method authenticates via an OAuth / subscription login rather
/// than a metered API key. Subscription logins are preferred as fallbacks: they
/// are the most likely path to "just work" when an API key is broken/unfunded,
/// and they bill against a flat subscription instead of per-token cost.
fn api_method_is_oauth(api_method: &ModelRouteApiMethod) -> bool {
    matches!(
        api_method,
        ModelRouteApiMethod::ClaudeOAuth
            | ModelRouteApiMethod::OpenAIOAuth
            | ModelRouteApiMethod::CodeAssistOAuth
    )
}

fn models_match(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

fn api_methods_match(a: &str, b: &str) -> bool {
    ModelRouteApiMethod::parse(a) == ModelRouteApiMethod::parse(b)
}

/// Pick the index of the best alternative route to fall back to, given the
/// currently-selected route that just failed.
///
/// Ranking (lower tier wins; ties broken to keep the result stable and to
/// prefer subscription logins, then original catalog order):
///
/// 1. **Same model, different auth method** - e.g. the active `claude-api`
///    route's key is broken but a `claude-oauth` login for the *same* model is
///    available. This is the least disruptive switch (identical model, just a
///    different credential) and is exactly the case cross-provider failover
///    cannot currently handle.
/// 2. **Same provider, different model** - stay on the provider the user chose
///    when only a sibling model is usable.
/// 3. **Different provider** - last resort cross-provider hop.
///
/// Returns `None` when no *available* route other than the current one exists.
pub fn pick_next_fallback_route(
    routes: &[ModelRoute],
    current_model: &str,
    current_provider: &str,
    current_api_method: &str,
) -> Option<usize> {
    routes
        .iter()
        .enumerate()
        .filter(|(_, route)| route.available)
        .filter_map(|(index, route)| {
            let same_model = models_match(&route.model, current_model);
            let same_method = api_methods_match(&route.api_method, current_api_method);
            let same_provider =
                model_route_provider_labels_match(&route.provider, current_provider)
                    || models_match(&route.provider, current_provider);

            // Never offer the exact route that just failed.
            if same_model && same_method && same_provider {
                return None;
            }

            let tier = if same_model && !same_method {
                0
            } else if same_provider {
                1
            } else {
                2
            };

            // Within a tier, prefer subscription (OAuth) logins, then preserve
            // the catalog's original ordering for determinism.
            let prefers_oauth = u8::from(!api_method_is_oauth(&ModelRouteApiMethod::parse(
                &route.api_method,
            )));

            Some((tier, prefers_oauth, index))
        })
        .min()
        .map(|(_, _, index)| index)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn route(model: &str, provider: &str, api_method: &str, available: bool) -> ModelRoute {
        ModelRoute {
            model: model.to_string(),
            provider: provider.to_string(),
            api_method: api_method.to_string(),
            available,
            detail: String::new(),
            cheapness: None,
        }
    }

    #[test]
    fn prefers_same_model_oauth_when_api_key_broken() {
        // The user's exact case: active Claude API-key route is broken, but the
        // same Claude model is reachable via an OAuth login.
        let routes = vec![
            route("claude-sonnet-4", "Anthropic", "claude-api", true),
            route("claude-sonnet-4", "Anthropic", "claude-oauth", true),
            route("gpt-5", "OpenAI", "openai-oauth", true),
        ];
        let pick = pick_next_fallback_route(&routes, "claude-sonnet-4", "Anthropic", "claude-api")
            .expect("a fallback should exist");
        assert_eq!(routes[pick].api_method, "claude-oauth");
        assert_eq!(routes[pick].model, "claude-sonnet-4");
    }

    #[test]
    fn falls_back_to_same_provider_sibling_model() {
        let routes = vec![
            route("claude-opus-4", "Anthropic", "claude-oauth", true),
            route("claude-sonnet-4", "Anthropic", "claude-oauth", true),
        ];
        // Active opus route failed and has no alternate method; offer the
        // sibling Anthropic model rather than jumping providers.
        let pick = pick_next_fallback_route(&routes, "claude-opus-4", "Anthropic", "claude-oauth")
            .expect("a fallback should exist");
        assert_eq!(routes[pick].model, "claude-sonnet-4");
        assert_eq!(routes[pick].provider, "Anthropic");
    }

    #[test]
    fn falls_back_cross_provider_as_last_resort() {
        let routes = vec![
            route("claude-sonnet-4", "Anthropic", "claude-oauth", true),
            route("gpt-5", "OpenAI", "openai-oauth", true),
        ];
        let pick =
            pick_next_fallback_route(&routes, "claude-sonnet-4", "Anthropic", "claude-oauth")
                .expect("a fallback should exist");
        assert_eq!(routes[pick].provider, "OpenAI");
    }

    #[test]
    fn skips_unavailable_routes() {
        let routes = vec![
            route("claude-sonnet-4", "Anthropic", "claude-api", true),
            route("claude-sonnet-4", "Anthropic", "claude-oauth", false),
            route("gpt-5", "OpenAI", "openai-oauth", false),
        ];
        // The only same-model alt is unavailable and the cross-provider option
        // is unavailable too, so there is nothing to offer.
        assert!(
            pick_next_fallback_route(&routes, "claude-sonnet-4", "Anthropic", "claude-api")
                .is_none()
        );
    }

    #[test]
    fn returns_none_when_only_current_route_exists() {
        let routes = vec![route("gpt-5", "OpenAI", "openai-oauth", true)];
        assert!(pick_next_fallback_route(&routes, "gpt-5", "OpenAI", "openai-oauth").is_none());
    }

    #[test]
    fn cross_provider_prefers_oauth_over_api_key() {
        let routes = vec![
            route("claude-sonnet-4", "Anthropic", "claude-oauth", true),
            route("gpt-5", "OpenAI", "openai-api", true),
            route("gpt-5", "OpenAI", "openai-oauth", true),
        ];
        let pick =
            pick_next_fallback_route(&routes, "claude-sonnet-4", "Anthropic", "claude-oauth")
                .expect("a fallback should exist");
        assert_eq!(routes[pick].provider, "OpenAI");
        assert_eq!(routes[pick].api_method, "openai-oauth");
    }
}
