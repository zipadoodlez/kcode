#[test]
fn test_remote_done_shows_footer_after_final_tool_result_without_trailing_text() {
    let mut app = create_test_app();
    let rt = tokio::runtime::Runtime::new().unwrap();
    let _guard = rt.enter();
    let mut remote = crate::tui::backend::RemoteConnection::dummy();

    app.is_processing = true;
    app.status = ProcessingStatus::Streaming;
    app.current_message_id = Some(42);
    app.processing_started = Some(Instant::now());
    app.visible_turn_started = Some(Instant::now());

    app.handle_server_event(
        crate::protocol::ServerEvent::ToolStart {
            id: "tool_read".to_string(),
            name: "read".to_string(),
        },
        &mut remote,
    );
    app.handle_server_event(
        crate::protocol::ServerEvent::ToolInput {
            delta: r#"{"file_path":"src/main.rs","start_line":1,"end_line":2}"#.to_string(),
        },
        &mut remote,
    );
    app.handle_server_event(
        crate::protocol::ServerEvent::ToolExec {
            id: "tool_read".to_string(),
            name: "read".to_string(),
        },
        &mut remote,
    );
    app.handle_server_event(
        crate::protocol::ServerEvent::TokenUsage {
            input: 123,
            output: 45,
            cache_read_input: None,
            cache_creation_input: None,
        },
        &mut remote,
    );
    app.handle_server_event(
        crate::protocol::ServerEvent::ToolDone {
            id: "tool_read".to_string(),
            name: "read".to_string(),
            output: "1 fn main() {}".to_string(),
            error: None,
        },
        &mut remote,
    );

    let needs_redraw =
        app.handle_server_event(crate::protocol::ServerEvent::Done { id: 42 }, &mut remote);

    assert!(
        needs_redraw,
        "remote Done must redraw after finalizing the response"
    );

    let footers: Vec<&DisplayMessage> = app
        .display_messages()
        .iter()
        .filter(|msg| msg.role == "meta")
        .collect();
    assert!(
        footers.iter().any(|msg| msg.content.contains("↑123 ↓45")),
        "footer not found"
    );
}
#[test]
fn test_remote_rewind_lists_display_history_when_session_transcript_is_empty() {
    let mut app = create_test_app();
    let rt = tokio::runtime::Runtime::new().unwrap();
    let _guard = rt.enter();
    let mut remote = crate::tui::backend::RemoteConnection::dummy();

    app.set_runtime_mode(crate::tui::app::AppRuntimeMode::RemoteClient);
    app.session.messages.clear();
    app.push_display_message(DisplayMessage::user("hello"));
    app.push_display_message(DisplayMessage::assistant("hi there"));

    app.composer.input = "/rewind".to_string();
    app.composer.cursor_pos = app.composer.input.len();
    rt.block_on(app.handle_remote_key(KeyCode::Enter, KeyModifiers::empty(), &mut remote))
        .expect("/rewind should be handled remotely");

    let last = app.display_messages().last().expect("history message");
    assert!(last.content.contains("Conversation history:"));
    assert!(last.content.contains("1 👤 User - hello"));
    assert!(last.content.contains("2 🤖 Assistant - hi there"));
    assert!(!last.content.contains("No messages in conversation"));
}

#[test]
fn test_remote_rewind_completion_shows_undo_hint_after_history_refresh() {
    let mut app = create_test_app();
    let rt = tokio::runtime::Runtime::new().unwrap();
    let _guard = rt.enter();
    let mut remote = crate::tui::backend::RemoteConnection::dummy();

    app.set_runtime_mode(crate::tui::app::AppRuntimeMode::RemoteClient);
    app.push_display_message(DisplayMessage::user("hello"));
    app.push_display_message(DisplayMessage::assistant("hi there"));

    app.composer.input = "/rewind 1".to_string();
    app.composer.cursor_pos = app.composer.input.len();
    rt.block_on(app.handle_remote_key(KeyCode::Enter, KeyModifiers::empty(), &mut remote))
        .expect("/rewind N should be sent remotely");

    app.handle_server_event(
        crate::protocol::ServerEvent::History {
            id: 1,
            session_id: "session_rewind_remote".to_string(),
            messages: vec![crate::protocol::HistoryMessage {
                response_stats: None,
                role: "user".to_string(),
                content: "hello".to_string(),
                tool_calls: None,
                tool_data: None,
            }],
            images: vec![],
            provider_name: Some("mock".to_string()),
            provider_model: Some("mock-model".to_string()),
            subagent_model: None,
            autoreview_enabled: None,
            autojudge_enabled: None,
            available_models: vec![],
            available_model_routes: vec![],
            mcp_servers: vec![],
            skills: vec![],
            total_tokens: None,
            token_usage_totals: None,
            all_sessions: vec![],
            client_count: None,
            is_canary: None,
            reload_recovery: None,
            server_version: None,
            server_name: None,
            server_icon: None,
            server_has_update: None,
            was_interrupted: None,
            connection_type: None,
            status_detail: None,
            upstream_provider: None,
            resolved_credential: None,
            reasoning_effort: None,
            service_tier: None,
            compaction_mode: crate::config::CompactionMode::Reactive,
            activity: None,
            side_panel: crate::side_panel::SidePanelSnapshot::default(),
        },
        &mut remote,
    );

    let last = app
        .display_messages()
        .last()
        .expect("rewind completion notice");
    assert!(last.content.contains("✓ Rewound to message 1"));
    assert!(last.content.contains("Undo anytime with /rewind undo"));
}
