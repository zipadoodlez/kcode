#[test]
fn test_parse_provider_hint_supports_known_values() {
    assert_eq!(
        MultiProvider::parse_provider_hint("claude"),
        Some(ActiveProvider::Claude)
    );
    assert_eq!(
        MultiProvider::parse_provider_hint("Anthropic"),
        Some(ActiveProvider::Claude)
    );
    assert_eq!(
        MultiProvider::parse_provider_hint("openai"),
        Some(ActiveProvider::OpenAI)
    );
    assert_eq!(
        MultiProvider::parse_provider_hint("copilot"),
        Some(ActiveProvider::Copilot)
    );
    assert_eq!(
        MultiProvider::parse_provider_hint("gemini"),
        Some(ActiveProvider::Gemini)
    );
    assert_eq!(
        MultiProvider::parse_provider_hint("openrouter"),
        Some(ActiveProvider::OpenRouter)
    );
    assert_eq!(
        MultiProvider::parse_provider_hint("cursor"),
        Some(ActiveProvider::Cursor)
    );
}

#[test]
fn test_active_provider_env_only_seeds_sessions_when_explicitly_selected() {
    with_clean_provider_test_env(|| {
        crate::env::set_var("KCODE_ACTIVE_PROVIDER", "openai");
        assert_eq!(MultiProvider::initial_provider_from_env(), None);

        crate::provider::activation::select_initial_runtime_provider_key("openai");
        assert_eq!(
            MultiProvider::initial_provider_from_env(),
            Some(ActiveProvider::OpenAI)
        );

        crate::provider::activation::clear_initial_runtime_provider();
        assert_eq!(MultiProvider::initial_provider_from_env(), None);
    });
}

#[test]
fn test_cursor_models_are_included_in_available_models_display_when_configured() {
    with_clean_provider_test_env(|| {
        let provider = test_multi_provider_with_cursor();
        let models = provider.available_models_display();
        assert!(models.iter().any(|model| model == "composer-2-fast"));
        assert!(models.iter().any(|model| model == "composer-2"));
    });
}

#[test]
fn test_cursor_models_are_included_in_model_routes_when_configured() {
    with_clean_provider_test_env(|| {
        let provider = test_multi_provider_with_cursor();
        let routes = provider.model_routes();
        assert!(routes.iter().any(|route| {
            route.model == "composer-2-fast"
                && route.provider == "Cursor"
                && route.api_method == "cursor"
                && route.available
        }));
    });
}

#[test]
fn test_set_model_switches_to_cursor_for_cursor_models() {
    with_clean_provider_test_env(|| {
        let provider = test_multi_provider_with_cursor();
        *provider.active.write().unwrap() = ActiveProvider::Claude;

        provider
            .set_model("composer-2-fast")
            .expect("cursor model should route to Cursor");

        assert_eq!(provider.active_provider(), ActiveProvider::Cursor);
        assert_eq!(provider.model(), "composer-2-fast");
    });
}

#[test]
fn test_set_model_supports_explicit_cursor_prefix() {
    with_clean_provider_test_env(|| {
        let provider = test_multi_provider_with_cursor();
        *provider.active.write().unwrap() = ActiveProvider::OpenAI;

        provider
            .set_model("cursor:gpt-5")
            .expect("explicit cursor prefix should force Cursor route");

        assert_eq!(provider.active_provider(), ActiveProvider::Cursor);
        assert_eq!(provider.model(), "gpt-5");
    });
}

#[test]
fn test_initial_provider_allows_cross_provider_switch_and_reports_target_credentials() {
    with_clean_provider_test_env(|| {
        let runtime = enter_test_runtime();
        let _enter = runtime.enter();
        let provider = MultiProvider {
            claude: RwLock::new(None),
            anthropic: RwLock::new(None),
            openai: RwLock::new(None),
            copilot_api: RwLock::new(None),
            antigravity: RwLock::new(None),
            gemini: RwLock::new(None),
            cursor: RwLock::new(None),
            bedrock: RwLock::new(None),
            openrouter: RwLock::new(None),
            openai_compatible_profiles: RwLock::new(std::collections::HashMap::new()),
            active_openai_compatible_profile: RwLock::new(None),
            active: RwLock::new(ActiveProvider::OpenAI),
            use_claude_cli: false,
            startup_notices: RwLock::new(Vec::new()),
            initial_provider: Some(ActiveProvider::OpenAI),
            routes_memo: std::sync::Mutex::new(None),
            post_auth_refreshes_pending: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        };

        let err = provider
            .set_model("claude:claude-sonnet-4-6")
            .expect_err("the target provider should report its missing credentials");
        assert!(
            err.to_string().contains("Claude credentials not available"),
            "expected target-provider credential error, got: {}",
            err
        );
    });
}

#[test]
fn test_auto_default_prefers_claude_over_openai_when_both_available() {
    let active = MultiProvider::auto_default_provider(ProviderAvailability {
        openai: true,
        claude: true,
        copilot: false,
        antigravity: false,
        gemini: false,
        cursor: false,
        bedrock: false,
        openrouter: false,
        copilot_premium_zero: false,
    });
    assert_eq!(active, ActiveProvider::Claude);
}

#[test]
fn test_auto_default_prefers_copilot_when_zero_premium_mode_enabled() {
    let active = MultiProvider::auto_default_provider(ProviderAvailability {
        openai: true,
        claude: true,
        copilot: true,
        antigravity: true,
        gemini: true,
        cursor: true,
        bedrock: false,
        openrouter: true,
        copilot_premium_zero: true,
    });
    assert_eq!(active, ActiveProvider::Copilot);
}

#[test]
fn test_no_provider_error_mentions_tokens_and_details() {
    let provider = MultiProvider {
        claude: RwLock::new(None),
        anthropic: RwLock::new(None),
        openai: RwLock::new(None),
        copilot_api: RwLock::new(None),
        antigravity: RwLock::new(None),
        gemini: RwLock::new(None),
        cursor: RwLock::new(None),
        bedrock: RwLock::new(None),
        openrouter: RwLock::new(None),
        openai_compatible_profiles: RwLock::new(std::collections::HashMap::new()),
        active_openai_compatible_profile: RwLock::new(None),
        active: RwLock::new(ActiveProvider::OpenAI),
        use_claude_cli: false,
        startup_notices: RwLock::new(Vec::new()),
        initial_provider: None,
        routes_memo: std::sync::Mutex::new(None),
        post_auth_refreshes_pending: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
    };
    let err = provider.no_provider_available_error(&[
        "OpenAI: rate limited".to_string(),
        "GitHub Copilot: not configured".to_string(),
    ]);
    let text = err.to_string();
    assert!(text.contains("Provider unavailable"));
    assert!(text.contains("OpenAI: rate limited"));
    assert!(text.contains("GitHub Copilot: not configured"));
    assert!(text.contains("Switch with `/model`"));
}

/// Regression for issue #358: after switching to a direct OpenAI-compatible
/// profile (e.g. `minimax:MiniMax-M3`), the OpenRouter slot's configured check
/// must see the *active profile runtime*, not just the real-OpenRouter slot.
/// With no OPENROUTER_API_KEY, the old check reported "not configured" and the
/// request was rerouted to another provider (the user saw an OpenAI token
/// refresh against api.openai.com).
#[test]
fn test_active_compat_profile_counts_as_configured_openrouter_slot() {
    with_clean_provider_test_env(|| {
        with_env_var("DEEPSEEK_API_KEY", "test-deepseek-key", || {
            crate::env::remove_var("OPENROUTER_API_KEY");
            let provider = MultiProvider {
                claude: RwLock::new(None),
                anthropic: RwLock::new(None),
                openai: RwLock::new(None),
                copilot_api: RwLock::new(None),
                antigravity: RwLock::new(None),
                gemini: RwLock::new(None),
                cursor: RwLock::new(None),
                bedrock: RwLock::new(None),
                openrouter: RwLock::new(None),
                openai_compatible_profiles: RwLock::new(std::collections::HashMap::new()),
                active_openai_compatible_profile: RwLock::new(None),
                active: RwLock::new(ActiveProvider::OpenRouter),
                use_claude_cli: false,
                startup_notices: RwLock::new(Vec::new()),
                initial_provider: None,
                routes_memo: std::sync::Mutex::new(None),
                post_auth_refreshes_pending: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            };

            // Activate a direct compat profile exactly like
            // `set_model("deepseek:<model>")` does.
            provider
                .set_model("deepseek:deepseek-v4-flash")
                .expect("compat profile switch should succeed with profile key set");
            assert_eq!(provider.active_provider(), ActiveProvider::OpenRouter);
            assert_eq!(provider.model(), "deepseek-v4-flash");

            // The real OpenRouter slot is still empty...
            assert!(provider.openrouter_provider().is_none());
            // ...but the slot check (used by the dispatch "not configured"
            // precheck) must consider the slot available through the active
            // compat profile runtime. `provider_slot_available` is asserted
            // directly because `provider_is_configured` would reconcile auth
            // from disk and could hot-install a real OpenRouter runtime from
            // ambient developer credentials, masking the regression.
            assert!(
                provider.provider_slot_available(ActiveProvider::OpenRouter),
                "active OpenAI-compatible profile must count as a configured OpenRouter slot"
            );
        })
    });
}
