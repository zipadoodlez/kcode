#![cfg_attr(test, allow(clippy::await_holding_lock))]

use super::{
    CoordinatorSpawnIdentity, ensure_spawn_coordinator_swarm, prepare_visible_spawn_session,
    register_visible_spawned_member, resolve_coordinator_spawn_identity, resolve_spawn_working_dir,
    resolve_stop_target_session, resolve_swarm_spawn_selection, session_may_spawn,
    spawn_admission_lock, stop_targets,
};
use crate::agent::Agent;
use crate::message::{Message, ToolDefinition};
use crate::protocol::ServerEvent;
use crate::protocol::SwarmLifecycleStatus;
use crate::provider::{EventStream, Provider};
use crate::server::{RunState, SwarmEventType, SwarmMember};
use crate::tool::Registry;
use anyhow::Result;
use async_trait::async_trait;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::time::Instant;
use tokio::sync::{Mutex, RwLock, broadcast, mpsc};

struct MockProvider;

#[async_trait]
impl Provider for MockProvider {
    async fn complete(
        &self,
        _messages: &[Message],
        _tools: &[ToolDefinition],
        _system: &str,
        _resume_session_id: Option<&str>,
    ) -> Result<EventStream> {
        Err(anyhow::anyhow!("mock provider should not be called"))
    }

    fn name(&self) -> &str {
        "mock"
    }

    fn fork(&self) -> Arc<dyn Provider> {
        Arc::new(MockProvider)
    }
}

fn member(session_id: &str) -> (SwarmMember, mpsc::UnboundedReceiver<ServerEvent>) {
    let (event_tx, event_rx) = mpsc::unbounded_channel();
    (
        SwarmMember {
            session_id: session_id.to_string(),
            event_tx,
            event_txs: HashMap::new(),
            working_dir: None,
            status: SwarmLifecycleStatus::Ready,
            detail: None,
            friendly_name: Some(session_id.to_string()),
            report_back_to_session_id: None,
            joined_at: Instant::now(),
            last_status_change: Instant::now(),
            is_headless: false,
            output_tail: None,
            runtime: crate::protocol::SwarmMemberRuntime::default(),
            task_label: None,
        },
        event_rx,
    )
}

async fn test_agent_with_working_dir(session_id: &str, working_dir: &str) -> Arc<Mutex<Agent>> {
    let provider: Arc<dyn Provider> = Arc::new(MockProvider);
    let registry = Registry::new(provider.clone()).await;
    let mut session = crate::session::Session::create_with_id(session_id.to_string(), None, None);
    session.model = Some("mock".to_string());
    session.working_dir = Some(working_dir.to_string());
    let mut agent = Agent::new_with_session(provider, registry, session, None);
    agent.set_working_dir(working_dir);
    Arc::new(Mutex::new(agent))
}

#[tokio::test]
async fn resolve_spawn_working_dir_prefers_explicit_then_spawner_agent_dir() {
    let sessions = Arc::new(RwLock::new(HashMap::new()));
    sessions.write().await.insert(
        "req".to_string(),
        test_agent_with_working_dir("req", "/tmp/spawner-agent").await,
    );
    let swarm_members = Arc::new(RwLock::new(HashMap::new()));

    assert_eq!(
        resolve_spawn_working_dir(
            Some("/tmp/explicit".to_string()),
            "req",
            &sessions,
            &swarm_members,
        )
        .await
        .as_deref(),
        Some("/tmp/explicit")
    );
    assert_eq!(
        resolve_spawn_working_dir(None, "req", &sessions, &swarm_members)
            .await
            .as_deref(),
        Some("/tmp/spawner-agent")
    );
}

#[tokio::test]
async fn resolve_spawn_working_dir_falls_back_to_member_dir() {
    let sessions = Arc::new(RwLock::new(HashMap::new()));
    let swarm_members = Arc::new(RwLock::new(HashMap::new()));
    let (mut req_member, _rx) = member("req");
    req_member.working_dir = Some(std::path::PathBuf::from("/tmp/member-dir"));
    swarm_members
        .write()
        .await
        .insert("req".to_string(), req_member);

    assert_eq!(
        resolve_spawn_working_dir(None, "req", &sessions, &swarm_members)
            .await
            .as_deref(),
        Some("/tmp/member-dir")
    );
}

#[test]
fn stop_takes_the_target_and_its_subtree_deepest_first() {
    let run: HashMap<String, SwarmMember> = ["root", "a", "b", "a-child", "foreign"]
        .into_iter()
        .map(|id| {
            let (mut member, _rx) = member(id);
            member.report_back_to_session_id = match id {
                "root" => None,
                "a" | "b" => Some("root".to_string()),
                "a-child" => Some("a".to_string()),
                _ => Some("elsewhere".to_string()),
            };
            (id.to_string(), member)
        })
        .collect();

    // Naming the root ends the run: the root and everything it spawned, a child
    // before the session that spawned it, and nothing outside the run.
    assert_eq!(
        stop_targets(&run, "root"),
        vec!["a-child", "a", "b", "root"]
    );
    // Naming a member takes that member and its own subtree only.
    assert_eq!(stop_targets(&run, "a"), vec!["a-child", "a"]);
    assert_eq!(stop_targets(&run, "foreign"), vec!["foreign"]);
}

#[tokio::test]
async fn stop_target_resolves_unique_friendly_name_and_suffix() {
    let swarm_members = Arc::new(RwLock::new(HashMap::new()));
    // The run root is a registered session; workers reach it through their
    // report-back edge, which is the membership.
    let (root, _root_rx) = member("swarm-1");
    let (mut worker, _worker_rx) = member("session_jellyfish_1234_abcd");
    worker.report_back_to_session_id = Some("swarm-1".to_string());
    worker.friendly_name = Some("jellyfish".to_string());
    let mut members = swarm_members.write().await;
    members.insert(root.session_id.clone(), root);
    members.insert(worker.session_id.clone(), worker);
    drop(members);

    assert_eq!(
        resolve_stop_target_session("swarm-1", "jellyfish", &swarm_members)
            .await
            .as_deref(),
        Ok("session_jellyfish_1234_abcd")
    );
    assert_eq!(
        resolve_stop_target_session("swarm-1", "abcd", &swarm_members)
            .await
            .as_deref(),
        Ok("session_jellyfish_1234_abcd")
    );
}

#[tokio::test]
async fn stop_target_rejects_ambiguous_friendly_name() {
    let swarm_members = Arc::new(RwLock::new(HashMap::new()));
    let (root, _root_rx) = member("swarm-1");
    let (mut first, _first_rx) = member("session_bear_1");
    first.report_back_to_session_id = Some("swarm-1".to_string());
    first.friendly_name = Some("bear".to_string());
    let (mut second, _second_rx) = member("session_bear_2");
    second.report_back_to_session_id = Some("swarm-1".to_string());
    second.friendly_name = Some("bear".to_string());
    let mut members = swarm_members.write().await;
    members.insert(root.session_id.clone(), root);
    members.insert(first.session_id.clone(), first);
    members.insert(second.session_id.clone(), second);
    drop(members);

    let err = resolve_stop_target_session("swarm-1", "bear", &swarm_members)
        .await
        .expect_err("ambiguous friendly names should be rejected");
    assert!(err.contains("Ambiguous swarm session 'bear'"));
}

#[tokio::test]
async fn register_visible_spawned_member_marks_startup_as_running() {
    let swarm_members = Arc::new(RwLock::new(HashMap::new()));
    let event_history = Arc::new(RwLock::new(VecDeque::new()));
    let event_counter = Arc::new(AtomicU64::new(0));
    let (swarm_event_tx, _swarm_event_rx) = broadcast::channel(8);

    register_visible_spawned_member(
        "child-1",
        "swarm-1",
        Some("/tmp/worktree"),
        true,
        Some("owner"),
        &swarm_members,
        &event_history,
        &event_counter,
        &swarm_event_tx,
    )
    .await;

    let members = swarm_members.read().await;
    let member = members.get("child-1").expect("spawned member should exist");
    assert_eq!(member.status, SwarmLifecycleStatus::Running);
    assert_eq!(member.detail.as_deref(), Some("startup queued"));
    // The report-back edge is the membership: a visible spawn reports to the
    // session that asked for it.
    assert_eq!(member.report_back_to_session_id.as_deref(), Some("owner"));
    assert_eq!(
        member.working_dir.as_deref(),
        Some(std::path::Path::new("/tmp/worktree"))
    );
    drop(members);

    let history = event_history.read().await;
    assert!(history.iter().any(|event| {
            event.session_id == "child-1"
                && matches!(event.event, SwarmEventType::MemberChange { ref action } if action == "joined")
        }));
}

#[test]
fn prepare_visible_spawn_session_persists_startup_before_launch() {
    let _guard = crate::storage::lock_test_env();
    let temp_home = tempfile::TempDir::new().expect("temp home");
    crate::env::set_var("KCODE_HOME", temp_home.path());

    let worktree = tempfile::TempDir::new().expect("temp worktree");
    let startup = "Please start by auditing prompt delivery.";

    let (session_id, launched) = prepare_visible_spawn_session(
        Some(worktree.path().to_str().expect("utf8 worktree path")),
        None,
        None,
        None,
        None,
        false,
        Some(startup),
        |session_id, _cwd: &std::path::Path, _selfdev, provider_key| {
            assert_eq!(provider_key, None);
            let path = crate::storage::kcode_dir()
                .expect("kcode dir")
                .join(format!("client-input-{}", session_id));
            let data = std::fs::read_to_string(&path).expect("startup file should exist");
            assert!(
                data.contains(startup),
                "startup payload should be written before launch"
            );
            assert!(
                data.contains(r#""submit_on_restore":true"#),
                "startup payload should auto-submit on restore"
            );
            Ok(true)
        },
    )
    .expect("visible spawn preparation should succeed");

    assert!(launched);
    let path = crate::storage::kcode_dir()
        .expect("kcode dir")
        .join(format!("client-input-{}", session_id));
    assert!(
        path.exists(),
        "startup file should remain for launched visible session"
    );

    crate::env::remove_var("KCODE_HOME");
}

#[test]
fn prepare_visible_spawn_session_cleans_startup_when_launch_not_started() {
    let _guard = crate::storage::lock_test_env();
    let temp_home = tempfile::TempDir::new().expect("temp home");
    crate::env::set_var("KCODE_HOME", temp_home.path());

    let worktree = tempfile::TempDir::new().expect("temp worktree");

    let (session_id, launched) = prepare_visible_spawn_session(
        Some(worktree.path().to_str().expect("utf8 worktree path")),
        None,
        None,
        None,
        None,
        false,
        Some("Do the thing."),
        |_session_id, _cwd: &std::path::Path, _selfdev, _provider_key| Ok(false),
    )
    .expect("visible spawn preparation should succeed even when launch is skipped");

    assert!(!launched);
    let path = crate::storage::kcode_dir()
        .expect("kcode dir")
        .join(format!("client-input-{}", session_id));
    assert!(
        !path.exists(),
        "startup file should be removed when visible launch does not start"
    );
    assert!(
        !crate::session::session_exists(&session_id),
        "prepared session should be cleaned up when visible launch does not start"
    );

    crate::env::remove_var("KCODE_HOME");
}

#[test]
fn prepare_visible_spawn_session_cleans_session_when_launch_errors() {
    let _guard = crate::storage::lock_test_env();
    let temp_home = tempfile::TempDir::new().expect("temp home");
    crate::env::set_var("KCODE_HOME", temp_home.path());

    let worktree = tempfile::TempDir::new().expect("temp worktree");

    let error = prepare_visible_spawn_session(
        Some(worktree.path().to_str().expect("utf8 worktree path")),
        None,
        None,
        None,
        None,
        false,
        Some("Do the thing."),
        |_session_id, _cwd: &std::path::Path, _selfdev, _provider_key| {
            Err(anyhow::anyhow!("launch failed"))
        },
    )
    .expect_err("visible spawn preparation should surface launch error");

    assert!(error.to_string().contains("launch failed"));
    let sessions_dir = crate::storage::kcode_dir()
        .expect("kcode dir")
        .join("sessions");
    let remaining_sessions = std::fs::read_dir(&sessions_dir)
        .map(|entries| entries.count())
        .unwrap_or(0);
    assert_eq!(
        remaining_sessions, 0,
        "failed visible launch should not leave orphan prepared sessions"
    );

    crate::env::remove_var("KCODE_HOME");
}

#[test]
fn prepare_visible_spawn_session_persists_and_launches_provider_key_for_openrouter_model() {
    let _guard = crate::storage::lock_test_env();
    let temp_home = tempfile::TempDir::new().expect("temp home");
    crate::env::set_var("KCODE_HOME", temp_home.path());

    let worktree = tempfile::TempDir::new().expect("temp worktree");
    let (session_id, launched) = prepare_visible_spawn_session(
        Some(worktree.path().to_str().expect("utf8 worktree path")),
        Some("openai/gpt-5.4@OpenAI"),
        None,
        None,
        None,
        false,
        None,
        |_session_id, _cwd: &std::path::Path, _selfdev, provider_key| {
            assert_eq!(provider_key, Some("openrouter"));
            Ok(true)
        },
    )
    .expect("visible spawn preparation should succeed");

    assert!(launched);
    let session = crate::session::Session::load(&session_id).expect("prepared session should save");
    assert_eq!(session.model.as_deref(), Some("openai/gpt-5.4@OpenAI"));
    assert_eq!(session.provider_key.as_deref(), Some("openrouter"));

    crate::env::remove_var("KCODE_HOME");
}

#[test]
fn prepare_visible_spawn_session_persists_inherited_effort() {
    let _guard = crate::storage::lock_test_env();
    let temp_home = tempfile::TempDir::new().expect("temp home");
    crate::env::set_var("KCODE_HOME", temp_home.path());

    let worktree = tempfile::TempDir::new().expect("temp worktree");
    let (session_id, launched) = prepare_visible_spawn_session(
        Some(worktree.path().to_str().expect("utf8 worktree path")),
        Some("gpt-5.5"),
        None,
        None,
        Some("low"),
        false,
        None,
        |_session_id, _cwd: &std::path::Path, _selfdev, _provider_key| Ok(true),
    )
    .expect("visible spawn preparation should succeed");

    assert!(launched);
    let session = crate::session::Session::load(&session_id).expect("prepared session should save");
    assert_eq!(session.model.as_deref(), Some("gpt-5.5"));
    assert_eq!(
        session.reasoning_effort.as_deref(),
        Some("low"),
        "the inherited level should persist so the headed client restores it"
    );

    crate::env::remove_var("KCODE_HOME");
}

#[test]
fn prepare_visible_spawn_session_prefers_parent_provider_key_over_model_guess() {
    let _guard = crate::storage::lock_test_env();
    let temp_home = tempfile::TempDir::new().expect("temp home");
    crate::env::set_var("KCODE_HOME", temp_home.path());

    let worktree = tempfile::TempDir::new().expect("temp worktree");
    let (session_id, launched) = prepare_visible_spawn_session(
        Some(worktree.path().to_str().expect("utf8 worktree path")),
        Some("gpt-5.4"),
        Some("ollama"),
        None,
        None,
        false,
        None,
        |_session_id, _cwd: &std::path::Path, _selfdev, provider_key| {
            assert_eq!(provider_key, Some("ollama"));
            Ok(true)
        },
    )
    .expect("visible spawn preparation should succeed");

    assert!(launched);
    let session = crate::session::Session::load(&session_id).expect("prepared session should save");
    assert_eq!(session.model.as_deref(), Some("gpt-5.4"));
    assert_eq!(session.provider_key.as_deref(), Some("ollama"));

    crate::env::remove_var("KCODE_HOME");
}

fn coordinator_identity(
    model: Option<&str>,
    provider_key: Option<&str>,
    route_api_method: Option<&str>,
) -> CoordinatorSpawnIdentity {
    CoordinatorSpawnIdentity {
        model: model.map(str::to_string),
        provider_key: provider_key.map(str::to_string),
        route_api_method: route_api_method.map(str::to_string),
        effort: None,
        is_canary: false,
    }
}

#[test]
fn resolve_swarm_spawn_model_prefers_configured_model_over_coordinator_model() {
    let selection = resolve_swarm_spawn_selection(
        None,
        Some("openai/gpt-5.4@OpenAI".to_string()),
        &coordinator_identity(
            Some("nvidia/llama-3.3-nemotron-super-49b-v1"),
            Some("nvidia"),
            Some("openai-compatible:nvidia-nim"),
        ),
    );

    assert_eq!(selection.model.as_deref(), Some("openai/gpt-5.4@OpenAI"));
    assert_eq!(selection.provider_key.as_deref(), Some("openrouter"));
    // A different configured model must not inherit the coordinator's route.
    assert_eq!(selection.route_api_method, None);
}

#[test]
fn resolve_swarm_spawn_model_inherits_coordinator_when_unconfigured() {
    let selection = resolve_swarm_spawn_selection(
        None,
        None,
        &coordinator_identity(
            Some("nvidia/llama-3.3-nemotron-super-49b-v1"),
            Some("nvidia"),
            Some("openai-compatible:nvidia-nim"),
        ),
    );

    assert_eq!(
        selection.model.as_deref(),
        Some("nvidia/llama-3.3-nemotron-super-49b-v1")
    );
    assert_eq!(selection.provider_key.as_deref(), Some("nvidia"));
    assert_eq!(
        selection.route_api_method.as_deref(),
        Some("openai-compatible:nvidia-nim")
    );
}

#[test]
fn resolve_swarm_spawn_model_inherits_coordinator_auth_route_for_oauth_vs_api() {
    // Regression: a coordinator on the Claude API route must spawn agents on
    // the same API route, not Claude OAuth (the config default).
    let selection = resolve_swarm_spawn_selection(
        None,
        None,
        &coordinator_identity(
            Some("claude-opus-4-6"),
            Some("claude-api"),
            Some("claude-api"),
        ),
    );

    assert_eq!(selection.model.as_deref(), Some("claude-opus-4-6"));
    assert_eq!(selection.provider_key.as_deref(), Some("claude-api"));
    assert_eq!(selection.route_api_method.as_deref(), Some("claude-api"));
}

#[test]
fn resolve_swarm_spawn_model_keeps_provider_key_when_config_matches_coordinator() {
    let selection = resolve_swarm_spawn_selection(
        None,
        Some("custom-model".to_string()),
        &coordinator_identity(
            Some("custom-model"),
            Some("custom-provider"),
            Some("custom-route"),
        ),
    );

    assert_eq!(selection.model.as_deref(), Some("custom-model"));
    assert_eq!(selection.provider_key.as_deref(), Some("custom-provider"));
    assert_eq!(selection.route_api_method.as_deref(), Some("custom-route"));
}

#[test]
fn resolve_swarm_spawn_model_openai_api_prefix_pins_api_route_over_coordinator() {
    // `agents.swarm_model = "openai-api:gpt-5.5"` must spawn agents on GPT-5.5
    // via the OpenAI API key route, regardless of the coordinator's model/auth.
    let selection = resolve_swarm_spawn_selection(
        None,
        Some("openai-api:gpt-5.5".to_string()),
        &coordinator_identity(
            Some("claude-opus-4-8"),
            Some("claude-oauth"),
            Some("claude-oauth"),
        ),
    );

    assert_eq!(selection.model.as_deref(), Some("gpt-5.5"));
    assert_eq!(selection.provider_key.as_deref(), Some("openai-api-key"));
    assert_eq!(
        selection.route_api_method.as_deref(),
        Some("openai-api-key")
    );
}

#[test]
fn resolve_swarm_spawn_model_auth_route_prefixes_pin_expected_routes() {
    for (configured, expected_model, expected_key) in [
        ("openai-api:gpt-5.5", "gpt-5.5", "openai-api-key"),
        ("openai-oauth:gpt-5.5", "gpt-5.5", "openai-oauth"),
        (
            "claude-api:claude-opus-4-8",
            "claude-opus-4-8",
            "anthropic-api-key",
        ),
        (
            "claude-oauth:claude-opus-4-8",
            "claude-opus-4-8",
            "claude-oauth",
        ),
    ] {
        let selection = resolve_swarm_spawn_selection(
            None,
            Some(configured.to_string()),
            &coordinator_identity(
                Some("some-other-model"),
                Some("some-key"),
                Some("some-route"),
            ),
        );
        assert_eq!(
            selection.model.as_deref(),
            Some(expected_model),
            "configured {configured:?} model",
        );
        assert_eq!(
            selection.provider_key.as_deref(),
            Some(expected_key),
            "configured {configured:?} provider_key",
        );
        assert_eq!(
            selection.route_api_method.as_deref(),
            Some(expected_key),
            "configured {configured:?} route_api_method",
        );
    }
}

#[test]
fn resolve_swarm_spawn_model_inherit_sentinel_uses_coordinator_model() {
    for sentinel in ["inherit", "INHERIT", "coordinator", " inherit ", ""] {
        let selection = resolve_swarm_spawn_selection(
            None,
            Some(sentinel.to_string()),
            &coordinator_identity(
                Some("nvidia/llama-3.3-nemotron-super-49b-v1"),
                Some("nvidia"),
                Some("openai-compatible:nvidia-nim"),
            ),
        );

        assert_eq!(
            selection.model.as_deref(),
            Some("nvidia/llama-3.3-nemotron-super-49b-v1"),
            "sentinel {sentinel:?} should inherit coordinator model",
        );
        assert_eq!(
            selection.provider_key.as_deref(),
            Some("nvidia"),
            "sentinel {sentinel:?} should inherit coordinator provider key",
        );
        assert_eq!(
            selection.route_api_method.as_deref(),
            Some("openai-compatible:nvidia-nim"),
            "sentinel {sentinel:?} should inherit coordinator auth route",
        );
    }
}

#[test]
fn resolve_swarm_spawn_model_requested_model_overrides_configured_pin() {
    for requested in ["openai-api:gpt-5.5", "  openai-api:gpt-5.5 \t"] {
        let selection = resolve_swarm_spawn_selection(
            Some(requested.to_string()),
            Some("claude-oauth:claude-opus-4-8".to_string()),
            &coordinator_identity(
                Some("claude-fable-5"),
                Some("claude-oauth"),
                Some("claude-oauth"),
            ),
        );

        assert_eq!(selection.model.as_deref(), Some("gpt-5.5"));
        assert_eq!(selection.provider_key.as_deref(), Some("openai-api-key"));
        assert_eq!(
            selection.route_api_method.as_deref(),
            Some("openai-api-key")
        );
    }
}

#[test]
fn resolve_swarm_spawn_model_requested_inherit_overrides_configured_pin() {
    for requested in [
        "inherit",
        "INHERIT",
        "coordinator",
        " COORDINATOR ",
        " inherit ",
    ] {
        let selection = resolve_swarm_spawn_selection(
            Some(requested.to_string()),
            Some("openai-api:gpt-5.5".to_string()),
            &coordinator_identity(
                Some("claude-fable-5"),
                Some("claude-api"),
                Some("claude-api"),
            ),
        );

        assert_eq!(selection.model.as_deref(), Some("claude-fable-5"));
        assert_eq!(selection.provider_key.as_deref(), Some("claude-api"));
        assert_eq!(selection.route_api_method.as_deref(), Some("claude-api"));
    }
}

#[test]
fn resolve_swarm_spawn_model_requested_matching_coordinator_model_keeps_route() {
    let selection = resolve_swarm_spawn_selection(
        Some(" custom-model ".to_string()),
        Some("openai-api:gpt-5.5".to_string()),
        &coordinator_identity(
            Some("custom-model"),
            Some("custom-provider"),
            Some("custom-route"),
        ),
    );

    assert_eq!(selection.model.as_deref(), Some("custom-model"));
    assert_eq!(selection.provider_key.as_deref(), Some("custom-provider"));
    assert_eq!(selection.route_api_method.as_deref(), Some("custom-route"));
}

#[test]
fn resolve_swarm_spawn_model_blank_requested_model_falls_back_to_config() {
    for requested in ["", "   ", "\t\n"] {
        let selection = resolve_swarm_spawn_selection(
            Some(requested.to_string()),
            Some("openai-api:gpt-5.5".to_string()),
            &coordinator_identity(
                Some("claude-fable-5"),
                Some("claude-oauth"),
                Some("claude-oauth"),
            ),
        );

        assert_eq!(selection.model.as_deref(), Some("gpt-5.5"));
        assert_eq!(selection.provider_key.as_deref(), Some("openai-api-key"));
        assert_eq!(
            selection.route_api_method.as_deref(),
            Some("openai-api-key")
        );
    }
}

#[test]
fn resolve_swarm_spawn_model_omitted_request_trims_configured_model() {
    let selection = resolve_swarm_spawn_selection(
        None,
        Some(" \topenai-api:gpt-5.5 \n".to_string()),
        &coordinator_identity(
            Some("claude-fable-5"),
            Some("claude-oauth"),
            Some("claude-oauth"),
        ),
    );

    assert_eq!(selection.model.as_deref(), Some("gpt-5.5"));
    assert_eq!(selection.provider_key.as_deref(), Some("openai-api-key"));
    assert_eq!(
        selection.route_api_method.as_deref(),
        Some("openai-api-key")
    );
}

#[test]
fn resolve_swarm_spawn_model_blank_requested_model_inherits_when_unconfigured() {
    let selection = resolve_swarm_spawn_selection(
        Some(" \t\n".to_string()),
        None,
        &coordinator_identity(
            Some("custom-model"),
            Some("custom-provider"),
            Some("custom-route"),
        ),
    );

    assert_eq!(selection.model.as_deref(), Some("custom-model"));
    assert_eq!(selection.provider_key.as_deref(), Some("custom-provider"));
    assert_eq!(selection.route_api_method.as_deref(), Some("custom-route"));
}

#[tokio::test]
async fn coordinator_identity_uses_live_agent_when_lock_is_available() {
    let agent = test_agent_with_working_dir("coord", "/tmp/coord").await;
    let live_model = agent.lock().await.provider_model();
    let sessions = Arc::new(RwLock::new(HashMap::new()));
    sessions
        .write()
        .await
        .insert("coord".to_string(), Arc::clone(&agent));

    let identity = resolve_coordinator_spawn_identity("coord", &sessions).await;
    assert_eq!(identity.model.as_deref(), Some(live_model.as_str()));
}

#[tokio::test]
async fn coordinator_identity_falls_back_to_persisted_session_when_agent_busy() {
    let _guard = crate::storage::lock_test_env();
    let temp_home = tempfile::TempDir::new().expect("temp home");
    crate::env::set_var("KCODE_HOME", temp_home.path());

    let agent = test_agent_with_working_dir("coord_busy", "/tmp/coord").await;

    // Persist a coordinator session that records a concrete model + auth route.
    // Persist after the agent is built so it reflects the authoritative on-disk
    // snapshot the spawn path will read when the agent lock is unavailable.
    let mut session = crate::session::Session::create_with_id(
        "coord_busy".to_string(),
        None,
        Some("coordinator identity test".to_string()),
    );
    session.model = Some("claude-opus-4-6".to_string());
    session.provider_key = Some("claude-api".to_string());
    session.route_api_method = Some("claude-api".to_string());
    session.reasoning_effort = Some("high".to_string());
    session.save().expect("persist coordinator session");

    // Hold the agent lock to simulate a coordinator mid-turn: the spawn path
    // must not block and must read the persisted identity instead of defaults.
    let _held = agent.lock().await;
    let sessions = Arc::new(RwLock::new(HashMap::new()));
    sessions
        .write()
        .await
        .insert("coord_busy".to_string(), Arc::clone(&agent));

    let identity = resolve_coordinator_spawn_identity("coord_busy", &sessions).await;
    assert_eq!(identity.model.as_deref(), Some("claude-opus-4-6"));
    assert_eq!(identity.provider_key.as_deref(), Some("claude-api"));
    assert_eq!(identity.route_api_method.as_deref(), Some("claude-api"));
    assert_eq!(
        identity.effort.as_deref(),
        Some("high"),
        "a spawned worker inherits the level its creator's session holds"
    );

    crate::env::remove_var("KCODE_HOME");
}

#[tokio::test]
async fn only_the_root_session_may_spawn() {
    // A member with a parent never starts agents; only the parentless root does.
    // The root's effort no longer keys this: a member's deeper work is rows the
    // run dispatches, not a worker starting a worker.
    let mut members = HashMap::new();
    let (root, _root_rx) = member("root");
    members.insert("root".to_string(), root);
    let (mut child, _child_rx) = member("child");
    child.report_back_to_session_id = Some("root".to_string());
    members.insert("child".to_string(), child);

    assert!(session_may_spawn(&members, "root"));
    assert!(!session_may_spawn(&members, "child"));
    assert!(session_may_spawn(&members, "stranger"));
}

#[tokio::test]
async fn spawn_resolves_the_requesting_session_as_run_root() {
    let swarm_members = Arc::new(RwLock::new(HashMap::new()));
    let swarm_runs = Arc::new(RwLock::new(HashMap::<String, RunState>::new()));
    let (req_member, _req_rx) = member("req");
    swarm_members
        .write()
        .await
        .insert("req".to_string(), req_member);
    let (client_event_tx, mut client_event_rx) = mpsc::unbounded_channel();

    let run_id =
        ensure_spawn_coordinator_swarm(1, "req", &client_event_tx, &swarm_members, &swarm_runs, 32)
            .await;

    // A session that reports back to nobody roots its own run; there is no
    // coordinator slot to claim and no bootstrap message to send.
    assert_eq!(run_id.as_deref(), Some("req"));
    assert!(
        client_event_rx.try_recv().is_err(),
        "resolving an existing run must not emit anything to the requester"
    );
}

#[tokio::test]
async fn terminal_members_do_not_consume_spawn_capacity() {
    let swarm_members = Arc::new(RwLock::new(HashMap::new()));
    let swarm_runs = Arc::new(RwLock::new(HashMap::<String, RunState>::new()));
    {
        let mut members = swarm_members.write().await;
        let (root, _rx) = member("root");
        members.insert("root".to_string(), root);
        // More finished members than the configured limit below.
        for idx in 0..64 {
            let id = format!("historical-{idx}");
            let (mut historical, _rx) = member(&id);
            historical.status = if idx % 2 == 0 {
                SwarmLifecycleStatus::Completed
            } else {
                SwarmLifecycleStatus::Stopped
            };
            historical.report_back_to_session_id = Some("root".to_string());
            members.insert(id, historical);
        }
    }
    let (client_event_tx, _client_event_rx) = mpsc::unbounded_channel();

    let allowed = ensure_spawn_coordinator_swarm(
        7,
        "root",
        &client_event_tx,
        &swarm_members,
        &swarm_runs,
        32,
    )
    .await;

    assert_eq!(allowed.as_deref(), Some("root"));
}

#[tokio::test]
async fn spawn_rejected_at_configured_live_agent_limit() {
    let swarm_members = Arc::new(RwLock::new(HashMap::new()));
    let swarm_runs = Arc::new(RwLock::new(HashMap::<String, RunState>::new()));
    {
        let mut members = swarm_members.write().await;
        let (root, _rx) = member("root");
        members.insert("root".to_string(), root);
        for idx in 0..2 {
            let id = format!("agent-{idx}");
            let (mut worker, _rx) = member(&id);
            worker.report_back_to_session_id = Some("root".to_string());
            members.insert(id, worker);
        }
    }
    let (client_event_tx, mut client_event_rx) = mpsc::unbounded_channel();

    let refused =
        ensure_spawn_coordinator_swarm(7, "root", &client_event_tx, &swarm_members, &swarm_runs, 2)
            .await;

    assert!(refused.is_none());
    assert!(matches!(
        client_event_rx.recv().await,
        Some(ServerEvent::Error { message, .. })
            if message.contains("Swarm live-agent limit reached (max 2")
    ));
}

#[tokio::test]
async fn spawn_admission_lock_serializes_per_swarm_only() {
    use std::time::Duration;

    let key = format!("lock-test-{}", std::process::id());
    let same_a = spawn_admission_lock(&key);
    let same_b = spawn_admission_lock(&key);
    let other = spawn_admission_lock(&format!("{key}-other"));

    let held = same_a.lock().await;
    assert!(
        tokio::time::timeout(Duration::from_millis(10), same_b.lock())
            .await
            .is_err()
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(100), other.lock())
            .await
            .is_ok()
    );
    drop(held);
    assert!(
        tokio::time::timeout(Duration::from_millis(100), same_b.lock())
            .await
            .is_ok()
    );
}
