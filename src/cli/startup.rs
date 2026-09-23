use anyhow::Result;
use clap::Parser;

use crate::{logging, perf, server, startup_profile, storage};

use super::{
    args::{Args, Command},
    dispatch, output, terminal,
};

fn sync_output_style_from_config() {
    crate::output_style::set_emoji_enabled(crate::config::config().display.emoji);
}

pub async fn run() -> Result<()> {
    // Parse once, before startup side effects. Invalid arguments and --help
    // must not harden credential files or create configuration state.
    let args = Args::parse();
    // Credential import must refuse existing stores without normal startup
    // hardening, migrations, or provider discovery touching them.
    if args.ssh.is_none()
        && matches!(
            args.command,
            Some(Command::Auth(super::args::AuthCommand::Import { .. }))
        )
    {
        if let Some(cwd) = &args.cwd {
            std::env::set_current_dir(cwd)?;
        }
        return dispatch::run_main(args).await;
    }
    startup_profile::init();

    terminal::install_panic_hook();
    startup_profile::mark("panic_hook");

    logging::init();
    startup_profile::mark("logging_init");
    // Old log pruning now runs on a background thread inside logging::init(),
    // so it no longer blocks startup. Memory-event logs have a separate,
    // longer (14-day) retention, so prune them on their own background thread.
    std::thread::Builder::new()
        .name("jcode-memlog-cleanup".to_string())
        .spawn(crate::memory_log::cleanup_old_memory_logs)
        .ok();
    // Prune stale per-session `.bak` recovery copies (never the transcripts
    // themselves) so the sessions directory does not grow without bound.
    std::thread::Builder::new()
        .name("jcode-session-bak-prune".to_string())
        .spawn(crate::session::prune_old_session_backups)
        .ok();
    logging::info("jcode starting");

    // Wire config-reload reactions without making config depend on auth/bus:
    // when the config cache reloads, invalidate the auth-status cache and
    // broadcast a models-updated event.
    sync_output_style_from_config();
    crate::config::on_config_reloaded(sync_output_style_from_config);
    crate::config::on_config_reloaded(crate::auth::AuthStatus::invalidate_cache);
    crate::config::on_config_reloaded(|| crate::bus::Bus::global().publish_models_updated());

    // Invert the legacy provider_catalog -> auth dependency: provider_catalog
    // consults registered fallback resolvers, and auth (the higher layer)
    // registers its external-CLI credential scan here.
    crate::provider_catalog::register_api_key_fallback_resolver(
        crate::auth::external::load_api_key_for_env,
    );

    // Register externally-implemented provider runtimes with the base
    // provider registry. These crates sit downstream of jcode-base (so
    // provider edits do not rebuild the app spine), which means base cannot
    // name their concrete types; this composition root wires them up instead.
    register_external_provider_runtimes();

    // Invert the legacy memory -> skill dependency: memory collects synthetic
    // entries from registered providers, and skill (the higher layer that
    // depends on MemoryEntry) registers its registry->memory adapter here.
    // The shared snapshot holds global skills only; memory retrieval is
    // process-scoped, so compose the project overlay from the process cwd
    // (issue #457 keeps session overlays out of the shared registry).
    crate::memory::register_synthetic_entry_provider(|| {
        let global = crate::skill::SkillRegistry::shared_snapshot();
        crate::skill::SkillRegistry::effective_for_working_dir(&global, None)
            .list()
            .into_iter()
            .map(|skill| skill.as_memory_entry())
            .collect()
    });

    // Invert the legacy server -> tui dependency: the TUI session picker owns
    // the session-list cache and registers its invalidator here, so the server
    // can drop the cache (e.g. after a rename) without referencing tui.
    crate::session_list_cache::register_invalidator(
        crate::tui::session_picker::invalidate_session_list_cache,
    );

    // Invert the legacy tui -> cli dependency for shared-server spawning: the
    // CLI owns the provider-bootstrap spawn logic and registers it here, so the
    // TUI reconnect loop can request a replacement server via server_spawn
    // without referencing cli.
    crate::server_spawn::register_default_server_spawner(Box::new(|| {
        Box::pin(async {
            dispatch::spawn_server(&crate::cli::provider_init::ProviderChoice::Auto, None, None)
                .await
        })
    }));

    crate::tui::keybind::log_keybinding_default_warnings();
    crate::platform::raise_nofile_limit_best_effort(8_192);
    startup_profile::mark("nofile_limit");

    storage::harden_user_config_permissions();
    startup_profile::mark("perm_harden");

    perf::init_background();
    startup_profile::mark("perf_init");

    let args = parse_and_prepare_args(args)?;

    if let Err(e) = dispatch::run_main(args).await {
        report_main_error(&e);
        return Err(e);
    }

    Ok(())
}

/// Register provider runtimes that live downstream of `jcode-base` with the
/// base crate's external provider registry. Keep every downstream runtime
/// registration in this one function so the composition-root wiring stays
/// discoverable as more providers move out of the base crate.
pub fn register_external_provider_runtimes() {
    crate::provider::external::register_external_provider(
        crate::provider::external::GROK_BUILD_RUNTIME,
        || {
            let mut process = jcode_provider_grok_build_runtime::GrokBuildProcess::from_env();
            process.command = crate::auth::grok_build::cli_path();
            std::sync::Arc::new(
                jcode_provider_grok_build_runtime::GrokBuildProvider::with_process(process),
            )
        },
    );
    crate::provider::external::register_external_provider(
        crate::provider::external::GEMINI_RUNTIME,
        || std::sync::Arc::new(jcode_provider_gemini_runtime::GeminiProvider::new()),
    );
    crate::provider::external::register_external_provider(
        crate::provider::external::CURSOR_RUNTIME,
        || std::sync::Arc::new(jcode_provider_cursor_runtime::CursorCliProvider::new()),
    );
    crate::provider::external::register_external_provider(
        crate::provider::external::ANTIGRAVITY_RUNTIME,
        || std::sync::Arc::new(jcode_provider_antigravity_runtime::AntigravityProvider::new()),
    );
    crate::provider::external::register_external_provider(
        crate::provider::external::CLAUDE_CLI_RUNTIME,
        || std::sync::Arc::new(jcode_provider_claude_cli_runtime::ClaudeProvider::new()),
    );
    crate::provider::external::register_external_provider(
        crate::provider::external::ANTHROPIC_RUNTIME,
        || std::sync::Arc::new(jcode_provider_anthropic_runtime::AnthropicProvider::new()),
    );
    // OpenRouter serves several identities (aggregator, pinned API-key
    // runtime, direct OpenAI-compatible profiles, named config profiles)
    // through one concrete type, so it registers a parameterized factory.
    crate::provider::external::register_openrouter_factory(|spec| {
        use crate::provider::external::OpenRouterRuntimeSpec;
        use jcode_provider_openrouter_runtime::OpenRouterProvider;
        let provider: std::sync::Arc<dyn crate::provider::Provider> = match spec {
            OpenRouterRuntimeSpec::Default => std::sync::Arc::new(OpenRouterProvider::new()?),
            OpenRouterRuntimeSpec::OpenRouterApiKey => {
                std::sync::Arc::new(OpenRouterProvider::new_openrouter_api_key_runtime()?)
            }
            OpenRouterRuntimeSpec::CompatibleProfile(profile) => std::sync::Arc::new(
                OpenRouterProvider::new_openai_compatible_profile_runtime(profile)?,
            ),
            OpenRouterRuntimeSpec::NamedProfile { name, config } => std::sync::Arc::new(
                OpenRouterProvider::new_named_openai_compatible(&name, &config)?,
            ),
        };
        Ok(provider)
    });
    crate::provider::external::register_profile_catalog_refresh(
        jcode_provider_openrouter_runtime::maybe_schedule_openai_compatible_profile_catalog_refresh,
    );
    crate::provider::external::register_standard_openrouter_catalog_refresh(
        jcode_provider_openrouter_runtime::maybe_schedule_standard_openrouter_catalog_refresh,
    );
    // API-backed OpenAI routes use Codex/platform credentials. The runtime is
    // still registered without them so browser-backed ChatGPT models remain
    // usable through the logged-in Firefox session.
    crate::provider::external::register_external_provider_fallible(
        crate::provider::external::OPENAI_RUNTIME,
        || {
            let provider = match crate::auth::codex::load_credentials() {
                Ok(credentials) => jcode_provider_openai_runtime::OpenAIProvider::new(credentials),
                Err(_) => jcode_provider_openai_runtime::OpenAIProvider::new_browser_only(),
            };
            Some(std::sync::Arc::new(provider) as std::sync::Arc<dyn crate::provider::Provider>)
        },
    );
    // Copilot's constructor is fallible (needs a GitHub token) and the runtime
    // wants tier detection scheduled right after construction, eagerly for
    // interactive sessions and deferred for non-interactive ones. That policy
    // lives here in the composition root so base stays provider-agnostic.
    crate::provider::external::register_external_provider_fallible(
        crate::provider::external::COPILOT_RUNTIME,
        || {
            let provider = std::sync::Arc::new(
                jcode_provider_copilot_runtime::CopilotApiProvider::new().ok()?,
            );
            let eager_tier_detection = std::env::var("JCODE_NON_INTERACTIVE").is_err();
            if eager_tier_detection && tokio::runtime::Handle::try_current().is_ok() {
                let p_clone = std::sync::Arc::clone(&provider);
                tokio::spawn(async move {
                    p_clone.detect_tier_and_set_default().await;
                });
            } else {
                provider.complete_init_without_tier_detection();
            }
            Some(provider as std::sync::Arc<dyn crate::provider::Provider>)
        },
    );
}

fn parse_and_prepare_args(args: Args) -> Result<Args> {
    startup_profile::mark("args_parse");

    output::set_quiet_enabled(args.quiet);

    if let Some(cwd) = &args.cwd {
        std::env::set_current_dir(cwd)?;
        logging::info(&format!("Changed working directory to: {}", cwd));
    }

    validate_remote_working_dir(args.remote_working_dir.as_deref())?;

    if args.trace {
        crate::env::set_var("JCODE_TRACE", "1");
    }

    if let Some(ref socket) = args.socket {
        server::set_socket_path(socket);
    }

    crate::cli::proctitle::set_initial_title(&args);

    Ok(args)
}

fn validate_remote_working_dir(remote_working_dir: Option<&str>) -> Result<()> {
    if let Some(remote_working_dir) = remote_working_dir
        && !remote_working_dir_is_absolute(remote_working_dir)
    {
        anyhow::bail!("--remote-working-dir must be an absolute path");
    }
    Ok(())
}

fn remote_working_dir_is_absolute(path: &str) -> bool {
    if path.starts_with('/') || path.starts_with('\\') {
        return true;
    }

    let bytes = path.as_bytes();
    bytes.len() >= 3
        && bytes[1] == b':'
        && (bytes[2] == b'/' || bytes[2] == b'\\')
        && bytes[0].is_ascii_alphabetic()
}

fn report_main_error(error: &anyhow::Error) {
    let error_str = format!("{:?}", error);
    logging::error(&error_str);

    if let Some(session_id) = terminal::get_current_session() {
        output::stderr_blank_line();
        output::stderr_info("\x1b[33mTo restore this session, run:\x1b[0m");
        output::stderr_info(format!("  jcode --resume {}", session_id));
        output::stderr_blank_line();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::args::Args;
    use clap::Parser;

    fn parse_args(argv: &[&str]) -> Args {
        Args::parse_from(argv)
    }

    #[test]
    fn parses_mcp_tool_exposure_flags() {
        let args = parse_args(&[
            "jcode",
            "--mcp-tools",
            "deferred",
            "--mcp-tools-token-threshold",
            "4321",
            "run",
            "hello",
        ]);
        assert_eq!(args.mcp_tools.as_deref(), Some("deferred"));
        assert_eq!(args.mcp_tools_token_threshold, Some(4_321));
    }

    #[test]
    fn remote_working_dir_validation_requires_absolute_path() {
        assert!(validate_remote_working_dir(Some("/home/agent/project")).is_ok());
        assert!(validate_remote_working_dir(Some("C:\\Users\\agent\\project")).is_ok());
        assert!(validate_remote_working_dir(None).is_ok());

        let error = validate_remote_working_dir(Some("relative/project")).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("--remote-working-dir must be an absolute path")
        );
    }

    #[test]
    fn external_provider_runtimes_register_and_instantiate() {
        register_external_provider_runtimes();
        for (key, expected_name) in [
            (crate::provider::external::GEMINI_RUNTIME, "gemini"),
            (crate::provider::external::CURSOR_RUNTIME, "cursor"),
            (
                crate::provider::external::ANTIGRAVITY_RUNTIME,
                "antigravity",
            ),
        ] {
            assert!(
                crate::provider::external::external_provider_registered(key),
                "{key} runtime should be registered"
            );
            let provider = crate::provider::external::instantiate_external_provider(key)
                .unwrap_or_else(|| panic!("{key} runtime factory should instantiate"));
            assert_eq!(provider.name(), expected_name);
            assert!(!provider.model().is_empty());
        }

        // Copilot's factory is fallible (requires a GitHub token), so only
        // assert registration; instantiation legitimately returns None when no
        // Copilot credentials exist on the machine running the tests.
        assert!(crate::provider::external::external_provider_registered(
            crate::provider::external::COPILOT_RUNTIME
        ));
    }
}
