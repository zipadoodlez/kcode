use super::{
    ACTIONS, CommunicateInput, CommunicateTool, canonical_swarm_action, format_swarm_model_list,
};
use crate::message::{Message, StreamEvent, ToolDefinition};
use crate::protocol::{NotificationType, Request, ServerEvent};
use crate::provider::{EventStream, Provider};
use crate::server::Server;
use crate::tool::{Tool, ToolContext, ToolExecutionMode};
use crate::transport::{ReadHalf, Stream, WriteHalf};
use anyhow::Result;
use async_trait::async_trait;
use futures::StreamExt;
use serde_json::json;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

#[test]
fn tool_is_named_swarm() {
    assert_eq!(CommunicateTool::new().name(), "swarm");
}
#[test]
fn canonical_swarm_action_maps_common_synonyms() {
    assert_eq!(canonical_swarm_action("send"), "message");
    assert_eq!(canonical_swarm_action("msg"), "message");
    assert_eq!(canonical_swarm_action("direct_message"), "message");
    assert_eq!(canonical_swarm_action("models"), "list_models");
    assert_eq!(canonical_swarm_action("kill"), "stop");
}

#[test]
fn canonical_swarm_action_is_case_insensitive_and_trims() {
    assert_eq!(canonical_swarm_action("  Kill  "), "stop");
    assert_eq!(canonical_swarm_action("SEND"), "message");
}

#[test]
fn canonical_swarm_action_passes_through_known_and_unknown_actions() {
    // Real actions are unchanged.
    assert_eq!(canonical_swarm_action("spawn"), "spawn");
    assert_eq!(canonical_swarm_action("message"), "message");
    // Genuinely unknown actions are returned unchanged for normal validation.
    assert_eq!(canonical_swarm_action("totally_made_up"), "totally_made_up");
}

#[test]
fn communicate_input_aliases_to_session_and_target_session() {
    // Either field name should be accepted; the execute() normalization mirrors them.
    let from_target: CommunicateInput = serde_json::from_value(
        json!({ "action": "dm", "message": "hi", "target_session": "worker-1" }),
    )
    .expect("parse target_session input");
    assert_eq!(from_target.target_session.as_deref(), Some("worker-1"));
    assert_eq!(from_target.to_session, None);

    let from_to: CommunicateInput =
        serde_json::from_value(json!({ "action": "summary", "to_session": "worker-2" }))
            .expect("parse to_session input");
    assert_eq!(from_to.to_session.as_deref(), Some("worker-2"));
    assert_eq!(from_to.target_session, None);
}

#[test]
fn schema_still_requires_action() {
    let schema = CommunicateTool::new().parameters_schema();
    assert_eq!(schema["required"], json!(["action"]));
}

#[test]
fn schema_advertises_model_spawn_override() {
    let schema = CommunicateTool::new().parameters_schema();
    let props = schema["properties"]
        .as_object()
        .expect("swarm schema should have properties");

    assert_eq!(props["model"]["type"], json!("string"));
    assert!(
        props["model"]["description"]
            .as_str()
            .expect("model description")
            .contains("list_models"),
        "model param should point at the list_models action"
    );
    assert!(
        !props.contains_key("effort"),
        "effort is the session's level, not a spawn argument"
    );
    assert!(
        schema["properties"]["action"]["enum"]
            .as_array()
            .expect("action enum")
            .contains(&json!("list_models"))
    );
}

#[test]
fn schema_requires_a_nonblank_label_for_spawn() {
    let schema = CommunicateTool::new().parameters_schema();
    assert_eq!(schema["properties"]["label"]["minLength"], json!(1));
    assert!(
        schema["properties"]["label"]["description"]
            .as_str()
            .expect("label description")
            .contains("Required for spawn")
    );
    assert!(
        schema["properties"]["action"]["description"]
            .as_str()
            .expect("action description")
            .contains("spawn requires label")
    );

    let branches = schema["anyOf"]
        .as_array()
        .expect("swarm schema should declare action-specific branches");
    let spawn_branch = branches
        .iter()
        .find(|branch| branch["properties"]["action"]["enum"] == json!(["spawn"]))
        .expect("spawn schema branch");
    assert_eq!(spawn_branch["required"], json!(["action", "label"]));

    let non_spawn_branch = branches
        .iter()
        .find(|branch| {
            branch["properties"]["action"]["enum"]
                .as_array()
                .is_some_and(|actions| !actions.contains(&json!("spawn")))
        })
        .expect("non-spawn schema branch");
    assert_eq!(non_spawn_branch["required"], json!(["action"]));
}

#[test]
fn schema_branches_only_require_properties_they_declare() {
    // Gemini rejects the entire request when a `required` entry names a property
    // the same object does not define, which made every tool-enabled Gemini call
    // fail on this tool's spawn branch (issue #655).
    let schema = CommunicateTool::new().parameters_schema();
    for branch in schema["anyOf"].as_array().expect("schema branches") {
        let declared = branch["properties"]
            .as_object()
            .expect("branch properties")
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        for required in branch["required"].as_array().expect("branch required") {
            let name = required.as_str().expect("required name");
            assert!(
                declared.iter().any(|known| known == name),
                "branch requires '{name}' without declaring it: {branch}"
            );
        }
    }
}

#[test]
fn spawn_label_validation_rejects_missing_or_blank_labels() {
    let missing: CommunicateInput =
        serde_json::from_value(json!({"action": "spawn"})).expect("spawn input");
    assert_eq!(
        missing
            .required_spawn_label()
            .expect_err("missing label must fail")
            .to_string(),
        "'label' is required for spawn action"
    );

    let blank: CommunicateInput = serde_json::from_value(json!({
        "action": "spawn",
        "label": "  \n\t "
    }))
    .expect("spawn input");
    assert_eq!(
        blank
            .required_spawn_label()
            .expect_err("blank label must fail")
            .to_string(),
        "'label' must not be blank for spawn action"
    );
}

#[test]
fn spawn_label_validation_trims_valid_labels() {
    let params: CommunicateInput = serde_json::from_value(json!({
        "action": "spawn",
        "label": "  api reviewer  "
    }))
    .expect("spawn input");
    assert_eq!(
        params.required_spawn_label().expect("valid label"),
        "api reviewer"
    );
}

#[tokio::test]
async fn spawn_execute_rejects_missing_label_before_sending_request() {
    let working_dir = tempfile::tempdir().expect("working dir");
    let error = CommunicateTool::new()
        .execute(
            json!({"action": "spawn", "prompt": "review the API"}),
            test_ctx("session-parent", working_dir.path()),
        )
        .await
        .expect_err("missing spawn label must fail locally");

    assert_eq!(error.to_string(), "'label' is required for spawn action");
}

#[test]
fn description_includes_swarm_prompt_guidance() {
    let tool = CommunicateTool::new();
    let description = tool.description();
    assert!(
        description.starts_with("Coordinate agents"),
        "description should lead with the short coordination summary"
    );
    assert!(
        description.contains("Swarm prompt"),
        "description should embed the swarm prompt section"
    );
}

#[test]
fn existing_tool_keeps_prompt_while_new_tool_loads_edit() {
    let project = tempfile::tempdir().unwrap();
    let prompt_dir = project.path().join(".kcode");
    std::fs::create_dir_all(&prompt_dir).unwrap();
    let prompt_path = prompt_dir.join("swarm-prompt.md");
    std::fs::write(&prompt_path, "first routing version").unwrap();

    let existing = CommunicateTool::new_for_working_dir(Some(project.path()));
    std::fs::write(&prompt_path, "second routing version").unwrap();
    let newly_created = CommunicateTool::new_for_working_dir(Some(project.path()));

    assert!(existing.description().contains("first routing version"));
    assert!(!existing.description().contains("second routing version"));
    assert!(
        newly_created
            .description()
            .contains("second routing version")
    );
}

#[test]
fn spawning_action_inputs_preserve_requested_model() {
    for action in ["spawn"] {
        for model in [
            "z-ai/glm-5.2:free",
            "openai-api:gpt-5.5",
            "inherit",
            "coordinator",
            "  ",
        ] {
            let input: CommunicateInput = serde_json::from_value(json!({
                "action": action,
                "label": "reviewer",
                "model": model
            }))
            .unwrap();
            assert_eq!(input.model.as_deref(), Some(model));
        }
    }
}

#[test]
fn spawning_action_inputs_allow_omitted_or_null_model() {
    let without_model: CommunicateInput =
        serde_json::from_value(json!({"action": "spawn", "label": "reviewer"})).unwrap();
    assert!(without_model.model.is_none());
    let null_model: CommunicateInput = serde_json::from_value(json!({
        "action": "spawn",
        "label": "reviewer",
        "model": null
    }))
    .unwrap();
    assert!(null_model.model.is_none());
}

#[test]
fn format_swarm_model_list_renders_routes_and_default() {
    let routes = vec![
        kcode_provider_core::ModelRoute {
            model: "gpt-5.5".to_string(),
            provider: "OpenAI".to_string(),
            api_method: "openai-api-key".to_string(),
            available: true,
            detail: "API key".to_string(),
            usage: None,
            cheapness: None,
        },
        kcode_provider_core::ModelRoute {
            model: "claude-fable-5".to_string(),
            provider: "Anthropic".to_string(),
            api_method: "anthropic-api-key".to_string(),
            available: false,
            detail: String::new(),
            usage: None,
            cheapness: None,
        },
    ];
    let output =
        format_swarm_model_list(Some("claude-fable-5"), Some("openai-api:gpt-5.5"), &routes);
    assert!(output.contains("Current coordinator model: claude-fable-5"));
    assert!(output.contains("Configured agents.swarm_model default: openai-api:gpt-5.5"));
    assert!(output.contains("gpt-5.5 via OpenAI [openai-api-key] (API key)"));
    assert!(output.contains("claude-fable-5 via Anthropic [anthropic-api-key] [unavailable]"));
}

#[test]
fn format_swarm_model_list_handles_empty_catalog() {
    let output = format_swarm_model_list(None, None, &[]);
    assert!(output.contains("Current coordinator model: unknown"));
    assert!(output.contains("No agents.swarm_model default configured"));
    assert!(output.contains("unless model is passed"));
    assert!(output.contains("No model routes reported"));
}

#[test]
fn schema_advertises_supported_swarm_fields() {
    let schema = CommunicateTool::new().parameters_schema();
    let props = schema["properties"]
        .as_object()
        .expect("swarm schema should have properties");

    assert!(props.contains_key("action"));
    assert!(props.contains_key("message"));
    assert!(props.contains_key("to_session"));
    assert_eq!(
        props["to_session"]["description"],
        json!("Session ID or unique friendly name of one agent. Alias of target_session.")
    );
    assert!(props.contains_key("target_session"));
    assert_eq!(
        props["target_session"]["description"],
        json!("Session ID or unique friendly name for management actions. Alias of to_session.")
    );
    assert!(props.contains_key("prompt"));
    assert!(props.contains_key("working_dir"));
    assert!(props.contains_key("wake"));
    assert!(props.contains_key("initial_message"));
    // The schema's enum is the one action list, so the choices the model is given
    // and the actions the dispatch accepts cannot drift apart.
    assert_eq!(
        props["action"]["enum"],
        json!(ACTIONS),
        "the schema must advertise exactly the actions the tool takes"
    );
}

struct EnvGuard {
    key: &'static str,
    original: Option<std::ffi::OsString>,
}

impl EnvGuard {
    fn set(key: &'static str, value: impl AsRef<std::ffi::OsStr>) -> Self {
        let original = std::env::var_os(key);
        crate::env::set_var(key, value);
        Self { key, original }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        if let Some(value) = self.original.take() {
            crate::env::set_var(self.key, value);
        } else {
            crate::env::remove_var(self.key);
        }
    }
}

struct DelayedTestProvider {
    delay: Duration,
}

#[async_trait]
impl Provider for DelayedTestProvider {
    async fn complete(
        &self,
        _messages: &[Message],
        _tools: &[ToolDefinition],
        _system: &str,
        _resume_session_id: Option<&str>,
    ) -> Result<EventStream> {
        let delay = self.delay;
        let stream = futures::stream::once(async move {
            tokio::time::sleep(delay).await;
            Ok(StreamEvent::TextDelta("ok".to_string()))
        })
        .chain(futures::stream::once(async {
            Ok(StreamEvent::MessageEnd { stop_reason: None })
        }));
        Ok(Box::pin(stream))
    }

    fn name(&self) -> &str {
        "test"
    }

    fn fork(&self) -> Arc<dyn Provider> {
        Arc::new(Self { delay: self.delay })
    }
}

struct RawClient {
    reader: BufReader<ReadHalf>,
    writer: WriteHalf,
    next_id: u64,
}

impl RawClient {
    async fn connect(path: &Path) -> Result<Self> {
        let stream = Stream::connect(path).await?;
        let (reader, writer) = stream.into_split();
        Ok(Self {
            reader: BufReader::new(reader),
            writer,
            next_id: 1,
        })
    }

    async fn send_request(&mut self, request: Request) -> Result<u64> {
        let id = request.id();
        let json = serde_json::to_string(&request)? + "\n";
        self.writer.write_all(json.as_bytes()).await?;
        Ok(id)
    }

    async fn read_event(&mut self) -> Result<ServerEvent> {
        let mut line = String::new();
        let n = self.reader.read_line(&mut line).await?;
        if n == 0 {
            anyhow::bail!("server disconnected")
        }
        Ok(serde_json::from_str(&line)?)
    }

    async fn read_until<F>(&mut self, timeout: Duration, mut predicate: F) -> Result<ServerEvent>
    where
        F: FnMut(&ServerEvent) -> bool,
    {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            let event = tokio::time::timeout(remaining, self.read_event()).await??;
            if predicate(&event) {
                return Ok(event);
            }
        }
    }

    async fn subscribe(&mut self, working_dir: &Path) -> Result<()> {
        let id = self.next_id;
        self.next_id += 1;
        self.send_request(Request::Subscribe {
            supports_pdf_panels: false,
            id,
            working_dir: Some(working_dir.display().to_string()),
            selfdev: None,
            target_session_id: None,
            client_instance_id: None,
            client_has_local_history: false,
            allow_session_takeover: false,
            crash_on_disconnect: false,
            continue_on_disconnect: false,
            terminal_env: Vec::new(),
        })
        .await?;
        self.read_until(
            Duration::from_secs(5),
            |event| matches!(event, ServerEvent::Done { id: done_id } if *done_id == id),
        )
        .await?;
        Ok(())
    }

    async fn session_id(&mut self) -> Result<String> {
        let id = self.next_id;
        self.next_id += 1;
        self.send_request(Request::GetState { id }).await?;
        match self
            .read_until(
                Duration::from_secs(5),
                |event| matches!(event, ServerEvent::State { id: event_id, .. } if *event_id == id),
            )
            .await?
        {
            ServerEvent::State { session_id, .. } => Ok(session_id),
            other => anyhow::bail!("unexpected state response: {other:?}"),
        }
    }

    async fn next_message_notification(&mut self, timeout: Duration) -> Result<Option<String>> {
        match self
            .read_until(timeout, |event| {
                matches!(
                    event,
                    ServerEvent::Notification {
                        notification_type: NotificationType::Message { .. },
                        ..
                    }
                )
            })
            .await?
        {
            ServerEvent::Notification {
                notification_type: NotificationType::Message { scope, .. },
                ..
            } => Ok(scope),
            other => anyhow::bail!("unexpected notification response: {other:?}"),
        }
    }
}

async fn wait_for_server_socket(
    path: &Path,
    server_task: &mut tokio::task::JoinHandle<Result<()>>,
) -> Result<()> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        if server_task.is_finished() {
            let result = server_task.await?;
            return Err(anyhow::anyhow!(
                "server exited before socket became ready: {:?}",
                result
            ));
        }
        match Stream::connect(path).await {
            Ok(stream) => {
                drop(stream);
                return Ok(());
            }
            Err(err) => {
                if tokio::time::Instant::now() >= deadline {
                    return Err(err.into());
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        }
    }
}

fn test_ctx(session_id: &str, working_dir: &Path) -> ToolContext {
    ToolContext {
        session_id: session_id.to_string(),
        message_id: "msg-1".to_string(),
        tool_call_id: "call-1".to_string(),
        working_dir: Some(working_dir.to_path_buf()),
        stdin_request_tx: None,
        graceful_shutdown_signal: None,
        execution_mode: ToolExecutionMode::Direct,
    }
}

/// Wait until `target_session` appears in a status event at all.
async fn wait_for_member_presence(client: &mut RawClient, target_session: &str) -> Result<()> {
    client
        .read_until(Duration::from_secs(5), |event| {
            matches!(
                event,
                ServerEvent::SwarmStatus { members }
                    if members.iter().any(|member| member.session_id == target_session)
            )
        })
        .await?;
    Ok(())
}

include!("communicate_tests/input_format.rs");
include!("communicate_tests/end_to_end.rs");
