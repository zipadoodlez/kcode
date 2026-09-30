use super::*;
use crate::auth::{AuthState, AuthStatus, ProviderAuth};
use crate::message::{Message, StreamEvent, ToolDefinition};
use crate::provider::ModelRoute;
use crate::provider::{EventStream, Provider};
use crate::tool::Registry;
use async_trait::async_trait;
use std::io::{Read, Write};
use std::sync::Arc;
use tokio::sync::mpsc as tokio_mpsc;
use tokio_stream::wrappers::ReceiverStream;

struct SavedEnv {
    vars: Vec<(String, Option<String>)>,
}

impl SavedEnv {
    fn capture(keys: &[&str]) -> Self {
        Self {
            vars: keys
                .iter()
                .map(|key| (key.to_string(), std::env::var(key).ok()))
                .collect(),
        }
    }
}

impl Drop for SavedEnv {
    fn drop(&mut self) {
        for (key, value) in &self.vars {
            if let Some(value) = value {
                crate::env::set_var(key, value);
            } else {
                crate::env::remove_var(key);
            }
        }
    }
}

struct TestProvider;

#[async_trait]
impl Provider for TestProvider {
    async fn complete(
        &self,
        _messages: &[Message],
        _tools: &[ToolDefinition],
        _system: &str,
        _resume_session_id: Option<&str>,
    ) -> Result<EventStream> {
        let (tx, rx) = tokio_mpsc::channel::<Result<StreamEvent>>(4);
        tokio::spawn(async move {
            let _ = tx.send(Ok(StreamEvent::TextDelta("ok".to_string()))).await;
            let _ = tx
                .send(Ok(StreamEvent::MessageEnd {
                    stop_reason: Some("end_turn".to_string()),
                }))
                .await;
        });
        Ok(Box::pin(ReceiverStream::new(rx)))
    }

    fn name(&self) -> &str {
        "test"
    }

    fn fork(&self) -> Arc<dyn Provider> {
        Arc::new(Self)
    }
}

struct FailingTestProvider;

#[async_trait]
impl Provider for FailingTestProvider {
    async fn complete(
        &self,
        _messages: &[Message],
        _tools: &[ToolDefinition],
        _system: &str,
        _resume_session_id: Option<&str>,
    ) -> Result<EventStream> {
        Err(anyhow::anyhow!("one-shot sentinel failure"))
    }

    fn name(&self) -> &str {
        "failing-test"
    }

    fn fork(&self) -> Arc<dyn Provider> {
        Arc::new(Self)
    }
}

fn spawn_single_response_http_server(status: u16, body: &str) -> String {
    spawn_single_response_http_server_on_host("127.0.0.1", status, body)
}

fn spawn_single_response_http_server_on_host(host: &str, status: u16, body: &str) -> String {
    let listener = std::net::TcpListener::bind((host, 0)).expect("bind test server");
    let addr = listener.local_addr().expect("local addr");
    let body = body.to_string();
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept connection");
        let mut buf = [0u8; 2048];
        let _ = stream.read(&mut buf);
        let status_text = match status {
            200 => "OK",
            400 => "Bad Request",
            404 => "Not Found",
            500 => "Internal Server Error",
            _ => "OK",
        };
        let response = format!(
            "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            status,
            status_text,
            body.len(),
            body
        );
        stream
            .write_all(response.as_bytes())
            .expect("write response");
    });
    format!("http://{}:{}/v1", host, addr.port())
}

#[test]
fn configured_auth_test_targets_only_include_configured_supported_providers() {
    let _guard = crate::storage::lock_test_env();
    // OpenRouter has no OAuth state to set: its availability is read straight
    // from `OPENROUTER_API_KEY` (or `openrouter.env`). Setting it here is what
    // makes the expectation below independent of whoever runs the test having
    // a key exported.
    let saved_openrouter_key = std::env::var("OPENROUTER_API_KEY").ok();
    crate::env::set_var("OPENROUTER_API_KEY", "test-key");

    let status = AuthStatus {
        anthropic: ProviderAuth {
            state: AuthState::Available,
            has_oauth: true,
            oauth_state: AuthState::Available,
            has_api_key: false,
        },
        openai: AuthState::NotConfigured,
        gemini: AuthState::Available,
        copilot: AuthState::Available,
        cursor: AuthState::NotConfigured,
        openrouter: AuthState::Available,
        ..AuthStatus::default()
    };

    let targets = configured_auth_test_targets(&status);

    match saved_openrouter_key {
        Some(key) => crate::env::set_var("OPENROUTER_API_KEY", key),
        None => crate::env::remove_var("OPENROUTER_API_KEY"),
    }

    assert!(targets.contains(&ResolvedAuthTestTarget::Detailed(AuthTestTarget::Claude)));
    assert!(targets.contains(&ResolvedAuthTestTarget::Detailed(AuthTestTarget::Copilot)));
    assert!(targets.contains(&ResolvedAuthTestTarget::Detailed(AuthTestTarget::Gemini)));
    assert!(targets.contains(&ResolvedAuthTestTarget::Generic {
        provider: crate::provider_catalog::OPENROUTER_LOGIN_PROVIDER,
        choice: "openrouter".to_string(),
    }));

    assert!(!targets.contains(&ResolvedAuthTestTarget::Detailed(AuthTestTarget::Openai)));
    assert!(!targets.contains(&ResolvedAuthTestTarget::Detailed(AuthTestTarget::Cursor)));
}

#[test]
fn explicit_supported_provider_maps_to_single_auth_target() {
    let targets = resolve_auth_test_targets("gemini", false).expect("resolve target");
    assert_eq!(
        targets,
        vec![ResolvedAuthTestTarget::Detailed(AuthTestTarget::Gemini)]
    );
}

#[test]
fn explicit_generic_provider_maps_to_generic_auth_target() {
    let targets = resolve_auth_test_targets("openrouter", false).expect("resolve target");
    assert_eq!(
        targets,
        vec![ResolvedAuthTestTarget::Generic {
            provider: crate::provider_catalog::OPENROUTER_LOGIN_PROVIDER,
            choice: "openrouter".to_string(),
        }]
    );
}

#[test]
fn collect_cli_model_names_prefers_available_routes_and_dedupes() {
    let routes = vec![
        ModelRoute {
            model: "gpt-5.4".to_string(),
            provider: "OpenAI".to_string(),
            api_method: "openai-oauth".to_string(),
            available: true,
            detail: String::new(),
            cheapness: None,
            usage: None,
        },
        ModelRoute {
            model: "gpt-5.4".to_string(),
            provider: "auto".to_string(),
            api_method: "openrouter".to_string(),
            available: true,
            detail: String::new(),
            cheapness: None,
            usage: None,
        },
        ModelRoute {
            model: "openrouter models".to_string(),
            provider: "—".to_string(),
            api_method: "openrouter".to_string(),
            available: false,
            detail: "OPENROUTER_API_KEY not set".to_string(),
            cheapness: None,
            usage: None,
        },
    ];

    let models = collect_cli_model_names(
        &routes,
        vec!["gpt-5.4".to_string(), "claude-sonnet-4".to_string()],
    );

    assert_eq!(models, vec!["gpt-5.4", "claude-sonnet-4"]);
}

fn test_route(model: &str, provider: &str, api_method: &str) -> ModelRoute {
    ModelRoute {
        model: model.to_string(),
        provider: provider.to_string(),
        api_method: api_method.to_string(),
        available: true,
        detail: String::new(),
        cheapness: None,
        usage: None,
    }
}

#[test]
fn cli_route_display_uses_typed_api_methods() {
    assert_eq!(cli_api_method_display("openai-oauth"), "oauth");
    assert_eq!(cli_api_method_display("openai-api-key"), "api key");
    assert_eq!(
        cli_api_method_display("openai-compatible:cerebras"),
        "api key"
    );
    assert_eq!(cli_api_method_display("mock-auth:profile"), "mock-auth");
    assert_eq!(
        cli_route_provider_display("DeepSeek", "openrouter"),
        "OpenRouter/DeepSeek"
    );
}

fn test_todo(id: &str, status: &str, priority: &str) -> crate::todo::TodoItem {
    crate::todo::TodoItem {
        id: id.to_string(),
        content: format!("todo {id}"),
        status: status.to_string(),
        priority: priority.to_string(),
        ..Default::default()
    }
}

#[test]
fn incomplete_poke_todos_treats_status_synonyms_and_case_as_finished() {
    for status in [
        "done",
        "finished",
        "complete",
        "Completed",
        " DONE ",
        "cancelled",
        "canceled",
    ] {
        assert!(
            incomplete_poke_todos(vec![test_todo("a", status, "high")]).is_empty(),
            "status {status:?} must not count as open work"
        );
    }
    for status in ["pending", "in_progress", "in progress", "blocked"] {
        assert_eq!(
            incomplete_poke_todos(vec![test_todo("a", status, "high")]).len(),
            1,
            "status {status:?} must count as open work"
        );
    }
}

/// Headless `kcode run` is what the benchmarks and scripted use go through, so
/// the deferred quality review must reach that path too, not only the TUI.
/// Open todos mean the agent is still working, so the review must wait for the
/// turn to actually end rather than interrupting mid-flight.
/// Regression: the digest is consumed from the log before the follow-up is
/// chosen, so a turn with open todos must not destroy it. Auto-poke iterates
/// many times with open todos on a long run, and each pass used to silently
/// discard the observations, meaning the reminder never survived to delivery.
/// The log must be consumed on delivery, or one turn's observations would leak
/// into the next turn and be raised again against work they never described.
#[test]
fn cli_provider_choice_filter_uses_typed_api_methods() {
    let routes = vec![
        test_route("claude-opus-4-6", "Anthropic", "claude-oauth"),
        test_route("claude-opus-4-6", "Anthropic", "claude-api"),
        test_route("gpt-5.5", "OpenAI", "openai-oauth"),
        test_route("gpt-5.5", "OpenAI", "openai-api-key"),
        test_route("gpt-5.6-pro[web]", "OpenAI", "chatgpt-web"),
        test_route("deepseek/deepseek-v4-pro", "auto", "openrouter"),
        test_route("grok-code-fast-1", "Copilot", "copilot"),
    ];

    let openai = filter_cli_model_routes_for_choice("openai", &routes);
    assert_eq!(openai.len(), 2);
    assert!(openai.iter().any(|route| matches!(
        route.api_method_kind(),
        crate::provider::ModelRouteApiMethod::OpenAIOAuth
    )));
    assert!(openai.iter().any(|route| {
        matches!(
            route.api_method_kind(),
            crate::provider::ModelRouteApiMethod::Other(ref value) if value == "chatgpt-web"
        )
    }));

    let claude = filter_cli_model_routes_for_choice("claude", &routes);
    assert_eq!(claude.len(), 2);
    assert!(
        claude
            .iter()
            .all(|route| route.api_method_kind().is_anthropic_credential_route())
    );
}

#[test]
fn auth_test_retryable_error_detection_handles_rate_limits() {
    let err = anyhow::anyhow!(
        "Gemini request generateContent failed (HTTP 429 Too Many Requests): RESOURCE_EXHAUSTED"
    );
    assert!(auth_test_error_is_retryable(&err));
}

#[test]
fn auth_test_retryable_error_detection_rejects_hard_usage_limit_exhaustion() {
    // Regression for #1148: the text contains "rate limit" but the quota
    // resets in weeks, so retrying is pointless.
    let err = anyhow::anyhow!(
        "Rate limited: The usage limit has been reached. Plan: free. Resets in 28d 19h 47m (2026-09-30 18:44 UTC)."
    );
    assert!(!auth_test_error_is_retryable(&err));
    let err = anyhow::anyhow!("OpenAI request failed (HTTP 429): insufficient_quota");
    assert!(!auth_test_error_is_retryable(&err));
}

#[test]
fn auth_test_retryable_error_detection_rejects_schema_errors() {
    let err = anyhow::anyhow!(
        "Gemini request generateContent failed (HTTP 400 Bad Request): invalid argument"
    );
    assert!(!auth_test_error_is_retryable(&err));
}

#[tokio::test]
async fn auth_test_choice_plan_preserves_explicit_model_for_local_provider() {
    let plan = auth_test_choice_plan("ollama", Some("llama3.2"))
        .await
        .expect("choice plan");

    match plan {
        AuthTestChoicePlan::Run { model } => assert_eq!(model.as_deref(), Some("llama3.2")),
        AuthTestChoicePlan::Skip(detail) => panic!("unexpected skip: {detail}"),
    }
}

#[tokio::test]
async fn auth_test_choice_plan_leaves_non_compat_provider_unchanged() {
    let plan = auth_test_choice_plan("openrouter", None)
        .await
        .expect("choice plan");

    match plan {
        AuthTestChoicePlan::Run { model } => assert!(model.is_none()),
        AuthTestChoicePlan::Skip(detail) => panic!("unexpected skip: {detail}"),
    }
}

#[tokio::test]
async fn auth_test_choice_plan_discovers_model_for_local_custom_compat_endpoint() {
    let _env_guard = crate::storage::lock_test_env();
    let _saved = SavedEnv::capture(&[
        "KCODE_OPENAI_COMPAT_API_BASE",
        "KCODE_OPENAI_COMPAT_API_KEY_NAME",
        "KCODE_OPENAI_COMPAT_ENV_FILE",
        "KCODE_OPENAI_COMPAT_DEFAULT_MODEL",
        "KCODE_OPENAI_COMPAT_LOCAL_ENABLED",
        "KCODE_OPENROUTER_API_BASE",
        "KCODE_OPENROUTER_API_KEY_NAME",
        "KCODE_OPENROUTER_ENV_FILE",
        "KCODE_OPENROUTER_ALLOW_NO_AUTH",
    ]);
    let api_base = spawn_single_response_http_server(200, r#"{"data":[{"id":"llama3.2"}]}"#);
    crate::env::set_var("KCODE_OPENAI_COMPAT_API_BASE", &api_base);
    crate::env::remove_var("KCODE_OPENAI_COMPAT_DEFAULT_MODEL");
    crate::env::remove_var("KCODE_OPENAI_COMPAT_LOCAL_ENABLED");
    crate::provider_catalog::apply_openai_compatible_profile_env(None);

    let plan = auth_test_choice_plan("openai-compatible", None)
        .await
        .expect("choice plan");

    match plan {
        AuthTestChoicePlan::Run { model } => assert_eq!(model.as_deref(), Some("llama3.2")),
        AuthTestChoicePlan::Skip(detail) => panic!("unexpected skip: {detail}"),
    }
}

#[tokio::test]
async fn auth_test_choice_plan_discovers_model_for_hosted_custom_compat_endpoint_with_api_key() {
    let _env_guard = crate::storage::lock_test_env();
    let _saved = SavedEnv::capture(&[
        "KCODE_OPENAI_COMPAT_API_BASE",
        "KCODE_OPENAI_COMPAT_API_KEY_NAME",
        "KCODE_OPENAI_COMPAT_ENV_FILE",
        "KCODE_OPENAI_COMPAT_DEFAULT_MODEL",
        "KCODE_OPENAI_COMPAT_LOCAL_ENABLED",
        "KCODE_OPENROUTER_API_BASE",
        "KCODE_OPENROUTER_API_KEY_NAME",
        "KCODE_OPENROUTER_ENV_FILE",
        "KCODE_OPENROUTER_ALLOW_NO_AUTH",
        "OPENAI_COMPAT_API_KEY",
        "NO_PROXY",
        "no_proxy",
    ]);
    // 0.0.0.0 is accepted as an insecure HTTP test host but is not treated as
    // localhost by resolve_openai_compatible_profile, so this exercises the
    // hosted/API-key code path while still serving the response locally.
    let api_base = spawn_single_response_http_server_on_host(
        "0.0.0.0",
        200,
        r#"{"data":[{"id":"hosted-compatible-model"}]}"#,
    );
    crate::env::set_var("KCODE_OPENAI_COMPAT_API_BASE", &api_base);
    crate::env::set_var("OPENAI_COMPAT_API_KEY", "test-key");
    crate::env::set_var("NO_PROXY", "0.0.0.0,127.0.0.1,localhost");
    crate::env::set_var("no_proxy", "0.0.0.0,127.0.0.1,localhost");
    crate::env::remove_var("KCODE_OPENAI_COMPAT_DEFAULT_MODEL");
    crate::env::remove_var("KCODE_OPENAI_COMPAT_LOCAL_ENABLED");
    crate::provider_catalog::apply_openai_compatible_profile_env(None);

    let resolved = crate::provider_catalog::resolve_openai_compatible_profile(
        crate::provider_catalog::OPENAI_COMPAT_PROFILE,
    );
    assert!(resolved.requires_api_key);

    let plan = auth_test_choice_plan("openai-compatible", None)
        .await
        .expect("choice plan");

    match plan {
        AuthTestChoicePlan::Run { model } => {
            assert_eq!(model.as_deref(), Some("hosted-compatible-model"))
        }
        AuthTestChoicePlan::Skip(detail) => panic!("unexpected skip: {detail}"),
    }
}

#[tokio::test]
async fn auth_test_choice_plan_skips_local_custom_compat_endpoint_without_models() {
    let _env_guard = crate::storage::lock_test_env();
    let _saved = SavedEnv::capture(&[
        "KCODE_OPENAI_COMPAT_API_BASE",
        "KCODE_OPENAI_COMPAT_API_KEY_NAME",
        "KCODE_OPENAI_COMPAT_ENV_FILE",
        "KCODE_OPENAI_COMPAT_DEFAULT_MODEL",
        "KCODE_OPENAI_COMPAT_LOCAL_ENABLED",
        "KCODE_OPENROUTER_API_BASE",
        "KCODE_OPENROUTER_API_KEY_NAME",
        "KCODE_OPENROUTER_ENV_FILE",
        "KCODE_OPENROUTER_ALLOW_NO_AUTH",
    ]);
    let api_base = spawn_single_response_http_server(200, r#"{"data":[]}"#);
    crate::env::set_var("KCODE_OPENAI_COMPAT_API_BASE", &api_base);
    crate::env::remove_var("KCODE_OPENAI_COMPAT_DEFAULT_MODEL");
    crate::env::remove_var("KCODE_OPENAI_COMPAT_LOCAL_ENABLED");
    crate::provider_catalog::apply_openai_compatible_profile_env(None);

    let plan = auth_test_choice_plan("openai-compatible", None)
        .await
        .expect("choice plan");

    match plan {
        AuthTestChoicePlan::Run { model } => panic!("unexpected run plan: {model:?}"),
        AuthTestChoicePlan::Skip(detail) => {
            assert!(detail.contains("reported no models"));
            assert!(detail.contains("openai-compatible"));
        }
    }
}

#[test]
fn collect_cli_model_names_falls_back_when_no_routes_are_available() {
    let routes = vec![ModelRoute {
        model: "claude-opus-4-6".to_string(),
        provider: "Anthropic".to_string(),
        api_method: "claude-oauth".to_string(),
        available: false,
        detail: "no credentials".to_string(),
        cheapness: None,
        usage: None,
    }];

    let models = collect_cli_model_names(&routes, vec!["gpt-5.4".to_string()]);

    assert_eq!(models, vec!["claude-opus-4-6", "gpt-5.4"]);
}

#[test]
fn list_cli_providers_includes_auto_and_openai() {
    let providers = super::report_info::list_cli_providers();
    assert!(providers.iter().any(|provider| provider.id == "auto"));
    assert!(providers.iter().any(|provider| {
        provider.id == "openai"
            && provider.display_name == "OpenAI"
            && provider.auth_kind.as_deref() == Some("OAuth")
    }));
    assert!(providers.iter().any(|provider| provider.id == "groq"));
    assert!(providers.iter().any(|provider| provider.id == "xai"));
}

#[test]
fn list_cli_providers_includes_conifer() {
    let providers = super::report_info::list_cli_providers();
    let conifer = providers
        .iter()
        .find(|provider| provider.id == "conifer")
        .expect("Conifer should be discoverable through `provider list`");

    assert_eq!(conifer.display_name, "Conifer");
    assert_eq!(conifer.auth_kind.as_deref(), Some("API key"));
}

#[test]
fn version_command_plain_output_includes_core_fields() {
    let report = super::report_info::VersionReport {
        version: "v1.2.3 (abc1234)".to_string(),
        semver: "1.2.3".to_string(),
        base_semver: "1.2.0".to_string(),
        update_semver: "1.2.0".to_string(),
        git_hash: "abc1234".to_string(),
        git_tag: "v1.2.3".to_string(),
        build_time: "2026-03-18 18:00:00 +0000".to_string(),
        git_date: "2026-03-18 17:59:00 +0000".to_string(),
        release_build: false,
    };
    let text = format!(
        "version\t{}\nsemver\t{}\nbase_semver\t{}\nupdate_semver\t{}\ngit_hash\t{}\ngit_tag\t{}\nbuild_time\t{}\ngit_date\t{}\nrelease_build\t{}\n",
        report.version,
        report.semver,
        report.base_semver,
        report.update_semver,
        report.git_hash,
        report.git_tag,
        report.build_time,
        report.git_date,
        report.release_build
    );

    assert!(text.contains("version\tv1.2.3 (abc1234)"));
    assert!(text.contains("semver\t1.2.3"));
    assert!(text.contains("git_hash\tabc1234"));
    assert!(text.contains("release_build\tfalse"));
}

#[tokio::test]
async fn restore_agent_session_if_requested_restores_resumed_session() {
    let _guard = crate::storage::lock_test_env();

    let provider: Arc<dyn Provider> = Arc::new(TestProvider);
    let registry = Registry::new(provider.clone()).await;
    let mut original = crate::agent::Agent::new(provider.clone(), registry);
    let original_session_id = original.session_id().to_string();
    original
        .run_once_capture("seed session for resume test")
        .await
        .expect("seed session");

    let registry = Registry::new(provider.clone()).await;
    let mut resumed = crate::agent::Agent::new(provider, registry);
    let fresh_session_id = resumed.session_id().to_string();
    assert_ne!(fresh_session_id, original_session_id);

    restore_agent_session_if_requested(&mut resumed, Some(&original_session_id))
        .expect("restore session");

    assert_eq!(resumed.session_id(), original_session_id);
}

#[tokio::test]
async fn one_shot_output_modes_close_sessions_and_clear_active_pid_markers() {
    let _guard = crate::storage::lock_test_env();
    let _saved = SavedEnv::capture(&["KCODE_HOME", "KCODE_RUN_AUTO_POKE"]);
    let temp = tempfile::tempdir().expect("tempdir");
    crate::env::set_var("KCODE_HOME", temp.path());
    crate::env::set_var("KCODE_RUN_AUTO_POKE", "0");

    for (mode, emit_json, emit_ndjson) in [
        ("plain", false, false),
        ("json", true, false),
        ("ndjson", false, true),
    ] {
        let provider: Arc<dyn Provider> = Arc::new(TestProvider);
        let registry = Registry::new(provider.clone()).await;
        let mut agent = crate::agent::Agent::new(provider.clone(), registry);
        let session_id = agent.session_id().to_string();
        let marker = crate::storage::active_pids_dir()
            .expect("active PID directory")
            .join(&session_id);
        assert!(marker.exists(), "{mode} session should start active");

        run_single_message_with_agent(
            &mut agent,
            provider,
            "Return the test response.",
            emit_json,
            emit_ndjson,
        )
        .await
        .unwrap_or_else(|error| panic!("{mode} run failed: {error:#}"));

        assert!(
            !marker.exists(),
            "{mode} run left an active PID marker behind"
        );
        let persisted = crate::session::Session::load(&session_id)
            .unwrap_or_else(|error| panic!("load {mode} session: {error:#}"));
        assert!(
            matches!(persisted.status, crate::session::SessionStatus::Closed),
            "{mode} run persisted status {:?}",
            persisted.status
        );
        assert!(
            !crate::session::find_recent_crashed_sessions()
                .iter()
                .any(|(id, _)| id == &session_id),
            "{mode} run was rediscovered as a stale-PID crash"
        );
    }
}

#[tokio::test]
async fn resumed_one_shot_closes_the_restored_session() {
    let _guard = crate::storage::lock_test_env();
    let _saved = SavedEnv::capture(&["KCODE_HOME", "KCODE_RUN_AUTO_POKE"]);
    let temp = tempfile::tempdir().expect("tempdir");
    crate::env::set_var("KCODE_HOME", temp.path());
    crate::env::set_var("KCODE_RUN_AUTO_POKE", "0");

    let provider: Arc<dyn Provider> = Arc::new(TestProvider);
    let registry = Registry::new(provider.clone()).await;
    let mut original = crate::agent::Agent::new(provider.clone(), registry);
    original
        .run_once_capture("Seed the resumed one-shot session.")
        .await
        .expect("seed resumed session");
    let session_id = original.session_id().to_string();
    original.mark_closed();

    let registry = Registry::new(provider.clone()).await;
    let mut resumed = crate::agent::Agent::new(provider.clone(), registry);
    restore_agent_session_if_requested(&mut resumed, Some(&session_id))
        .expect("restore one-shot session");
    let marker = crate::storage::active_pids_dir()
        .expect("active PID directory")
        .join(&session_id);
    assert!(
        marker.exists(),
        "restored session should be active while running"
    );

    run_single_message_with_agent(
        &mut resumed,
        provider,
        "Finish the resumed one-shot session.",
        true,
        false,
    )
    .await
    .expect("run resumed one-shot session");

    assert!(!marker.exists(), "resumed run left its active PID marker");
    let persisted = crate::session::Session::load(&session_id).expect("load resumed session");
    assert!(matches!(
        persisted.status,
        crate::session::SessionStatus::Closed
    ));
    assert!(
        !crate::session::find_recent_crashed_sessions()
            .iter()
            .any(|(id, _)| id == &session_id),
        "resumed run was rediscovered as a stale-PID crash"
    );
}

#[tokio::test]
async fn one_shot_cleanup_preserves_the_original_command_error() {
    let _guard = crate::storage::lock_test_env();
    let _saved = SavedEnv::capture(&["KCODE_HOME", "KCODE_RUN_AUTO_POKE"]);
    let temp = tempfile::tempdir().expect("tempdir");
    crate::env::set_var("KCODE_HOME", temp.path());
    crate::env::set_var("KCODE_RUN_AUTO_POKE", "0");

    for (mode, emit_json, emit_ndjson) in [
        ("plain", false, false),
        ("json", true, false),
        ("ndjson", false, true),
    ] {
        let provider: Arc<dyn Provider> = Arc::new(FailingTestProvider);
        let registry = Registry::new(provider.clone()).await;
        let mut agent = crate::agent::Agent::new(provider.clone(), registry);
        let session_id = agent.session_id().to_string();
        let marker = crate::storage::active_pids_dir()
            .expect("active PID directory")
            .join(&session_id);

        let error = match run_single_message_with_agent(
            &mut agent,
            provider,
            "Fail this run.",
            emit_json,
            emit_ndjson,
        )
        .await
        {
            Ok(()) => panic!("{mode} provider failure should remain the command result"),
            Err(error) => error,
        };

        assert!(
            error.to_string().contains("one-shot sentinel failure"),
            "{mode} changed the original error: {error:#}"
        );
        assert!(
            !marker.exists(),
            "failed {mode} run left an active PID marker"
        );
        let persisted = crate::session::Session::load(&session_id)
            .unwrap_or_else(|error| panic!("load failed {mode} session: {error:#}"));
        assert!(matches!(
            persisted.status,
            crate::session::SessionStatus::Closed
        ));
    }
}
