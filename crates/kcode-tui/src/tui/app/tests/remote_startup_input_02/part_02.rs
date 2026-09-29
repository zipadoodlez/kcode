#[test]
fn test_background_task_started_activity_creates_running_row_without_card() {
    let mut app = create_test_app();
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut remote = crate::tui::backend::RemoteConnection::dummy();
    let event = BusEvent::UiActivity(crate::bus::UiActivity::background(
        Some(app.session.id.clone()),
        "**Background task started** `bgstarted` · `cargo test`\n\nKcode is running this in the background. Progress, checkpoints, and completion will appear here.",
        Some("Background task started · cargo test"),
    ));

    let _ = rt.block_on(super::remote::handle_bus_event(&mut app, &mut remote, Ok(event)));

    assert!(app.display_messages().is_empty());
    assert_eq!(
        app.background_tasks.rows(),
        &[crate::tui::BackgroundTaskRow {
            task_id: "bgstarted".to_string(),
            label: "cargo test".to_string(),
            percent: None,
            status: crate::tui::BackgroundTaskRowStatus::Running,
            completed_at: None,
        }]
    );
}

#[test]
fn test_handle_server_event_input_shell_result_renders_markdown_blocks() {
    let mut app = create_test_app();
    let rt = tokio::runtime::Runtime::new().unwrap();
    let _guard = rt.enter();
    let mut remote = crate::tui::backend::RemoteConnection::dummy();

    app.handle_server_event(
        crate::protocol::ServerEvent::InputShellResult {
            result: crate::message::InputShellResult {
                command: "pwd".to_string(),
                cwd: Some("/tmp/project".to_string()),
                output: "/tmp/project\n".to_string(),
                exit_code: Some(0),
                duration_ms: 5,
                truncated: false,
                failed_to_start: false,
            },
        },
        &mut remote,
    );

    let rendered = app.display_messages().last().expect("shell result message");
    assert_eq!(rendered.role, "system");
    assert!(rendered.content.contains("Shell command"));
    assert!(rendered.content.contains("pwd"));
    assert!(rendered.content.contains("/tmp/project"));
    assert_eq!(
        app.status_notice(),
        Some("Shell command completed".to_string())
    );
}

#[test]
fn test_streaming_tokens() {
    let mut app = create_test_app();

    assert_eq!(app.streaming_tokens(), (0, 0));

    app.streaming.streaming_input_tokens = 100;
    app.streaming.streaming_output_tokens = 50;

    assert_eq!(app.streaming_tokens(), (100, 50));
}

#[test]
fn test_build_turn_footer_uses_compact_duration_labels() {
    let app = create_test_app();

    assert_eq!(
        app.build_turn_footer(Some(316.1)),
        Some("5m 16s".to_string())
    );
    assert_eq!(app.build_turn_footer(Some(9.2)), Some("9.2s".to_string()));
}
