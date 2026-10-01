//! Live soft-interrupt placement (e2e).
//!
//! An injected message must land after the `tool_result`s of the turn it
//! interrupts, never between a `tool_use` and its `tool_result`. The unit tests
//! cover the placement predicate (`messages_end_with_tool_result_*` in
//! `kcode-app-core/src/agent/turn_loops.rs`); this covers the live turn, which
//! nothing else does.
//!
//! Runs against a private server socket with the mock provider, so it needs no
//! credentials and does not touch the installed daemon (`docs/dev/testing.md`).
//!
//! This is the only driver ported from the Python trio. The timing-based and
//! real-provider cases in `test_soft_interrupt.py` / `test_injection_*.py` were
//! dropped rather than ported (see `docs/todo.md` §4).

use crate::test_support::*;

fn nanos() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos()
}

/// A private server on its own socket, so the test never touches the installed
/// daemon and two runs never share state.
struct PrivateServer {
    provider: Arc<MockProvider>,
    socket_path: std::path::PathBuf,
    debug_socket_path: std::path::PathBuf,
    handle: tokio::task::JoinHandle<Result<()>>,
}

async fn start_private_server(name: &str) -> Result<PrivateServer> {
    let runtime_dir = short_runtime_dir(format!("kcode-{name}-{}", nanos()));
    std::fs::create_dir_all(&runtime_dir)?;
    let socket_path = runtime_dir.join("kcode.sock");
    let debug_socket_path = runtime_dir.join("kcode-debug.sock");

    let provider = Arc::new(MockProvider::new());
    let provider_dyn: Arc<dyn Provider> = provider.clone();
    let server_instance = server::Server::new_with_paths(
        provider_dyn,
        socket_path.clone(),
        debug_socket_path.clone(),
    );
    let handle = tokio::spawn(async move { server_instance.run().await });

    wait_for_server_ready(&socket_path, &debug_socket_path).await?;
    Ok(PrivateServer {
        provider,
        socket_path,
        debug_socket_path,
        handle,
    })
}

impl PrivateServer {
    fn stop(self) {
        abort_server_and_cleanup(&self.handle, &self.socket_path, &self.debug_socket_path);
    }
}

/// The text of each message in a `history` debug response, in file order. Tool
/// blocks carry no `text`, so a `tool_use` message is empty and a `tool_result`
/// message shows the tool's output.
fn history_texts(history: &serde_json::Value) -> Vec<String> {
    let Some(messages) = history.as_array() else {
        return Vec::new();
    };
    messages
        .iter()
        .map(|message| match message.get("content") {
            Some(serde_json::Value::String(text)) => text.clone(),
            Some(serde_json::Value::Array(blocks)) => blocks
                .iter()
                .filter_map(|block| block.get("text").and_then(|v| v.as_str()))
                .collect::<Vec<_>>()
                .join(""),
            _ => String::new(),
        })
        .collect()
}

/// Poll `history` until any message contains `needle`, or the deadline passes.
async fn wait_for_history_containing(
    debug_socket_path: &std::path::Path,
    session_id: &str,
    needle: &str,
) -> Result<serde_json::Value> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let history =
            debug_run_command_json(debug_socket_path.to_path_buf(), "history", Some(session_id))
                .await?;
        if history_texts(&history)
            .iter()
            .any(|text| text.contains(needle))
        {
            return Ok(history);
        }
        if Instant::now() >= deadline {
            anyhow::bail!("timed out waiting for {needle:?} in history; got {history:?}");
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// The placement case: an interrupt is queued before the turn runs, the turn
/// calls a tool, and the injected message must land *after* the tool result.
/// Queuing while idle (rather than racing the turn) keeps it deterministic.
#[tokio::test]
async fn injected_message_lands_after_tool_results() -> Result<()> {
    let _env = setup_test_env()?;
    let server = start_private_server("soft-interrupt-order").await?;
    let session_id = debug_create_headless_session(server.debug_socket_path.clone()).await?;

    // Turn 1 calls a tool that prints a marker, so the tool_result is findable in
    // history; turn 2 ends the message.
    server.provider.queue_response(vec![
        StreamEvent::ToolUseStart {
            id: "call-1".to_string(),
            name: "bash".to_string(),
        },
        StreamEvent::ToolInputDelta("{\"command\":\"echo TOOLRESULT\"}".to_string()),
        StreamEvent::ToolUseEnd,
        StreamEvent::MessageEnd {
            stop_reason: Some("tool_use".to_string()),
        },
    ]);
    server.provider.queue_response(vec![
        StreamEvent::TextDelta("finished".to_string()),
        StreamEvent::MessageEnd {
            stop_reason: Some("end_turn".to_string()),
        },
    ]);

    // Queue the interrupt before the turn so it is pending when the turn runs.
    let _ = debug_run_command(
        server.debug_socket_path.clone(),
        "queue_interrupt:INJECTED",
        Some(&session_id),
    )
    .await?;

    let mut client = server::Client::connect_with_path(server.socket_path.clone()).await?;
    let subscribe_id = client.subscribe().await?;
    let _ = collect_until_done_unix(&mut client, subscribe_id).await?;
    let _ = client.resume_session(&session_id).await?;
    let _ = client.send_message("run the tool").await?;

    let history =
        wait_for_history_containing(&server.debug_socket_path, &session_id, "finished").await?;
    let texts = history_texts(&history);
    server.stop();

    let tool_result = texts
        .iter()
        .position(|text| text.contains("TOOLRESULT"))
        .expect("the tool result must be in history");
    let injected = texts
        .iter()
        .position(|text| text.contains("INJECTED"))
        .unwrap_or_else(|| panic!("no INJECTED in history; texts: {texts:?}"));
    assert!(
        injected > tool_result,
        "an injected message must land after the tool_result, not between tool_use and tool_result; \
         history texts: {texts:?}"
    );
    Ok(())
}

/// Two interrupts queued while idle must land in the order they were queued.
#[tokio::test]
async fn queued_interrupts_keep_their_order() -> Result<()> {
    let _env = setup_test_env()?;
    let server = start_private_server("soft-interrupt-order").await?;
    let session_id = debug_create_headless_session(server.debug_socket_path.clone()).await?;

    server.provider.queue_response(vec![
        StreamEvent::TextDelta("done".to_string()),
        StreamEvent::MessageEnd {
            stop_reason: Some("end_turn".to_string()),
        },
    ]);

    for text in ["ONE", "TWO"] {
        let _ = debug_run_command(
            server.debug_socket_path.clone(),
            &format!("queue_interrupt:{text}"),
            Some(&session_id),
        )
        .await?;
    }

    let mut client = server::Client::connect_with_path(server.socket_path.clone()).await?;
    let subscribe_id = client.subscribe().await?;
    let _ = collect_until_done_unix(&mut client, subscribe_id).await?;
    let _ = client.resume_session(&session_id).await?;
    let _ = client.send_message("go").await?;

    let history =
        wait_for_history_containing(&server.debug_socket_path, &session_id, "done").await?;
    let texts = history_texts(&history);
    server.stop();

    let flat = texts.join("\n");
    let one = flat.find("ONE");
    let two = flat.find("TWO");
    assert!(
        matches!((one, two), (Some(a), Some(b)) if a < b),
        "queued interrupts must keep their order; history texts: {texts:?}"
    );
    Ok(())
}
