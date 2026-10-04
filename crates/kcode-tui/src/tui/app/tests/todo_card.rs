/// Simple todo used by the pinned-band tests.
fn pinned_band_todo(id: &str, content: &str, status: &str) -> crate::todo::TaskItem {
    crate::todo::TaskItem {
        id: id.to_string(),
        content: content.to_string(),
        status: status.to_string(),
        priority: "high".to_string(),
        ..Default::default()
    }
}

#[test]
fn pinned_todos_hide_todo_tool_messages_from_the_transcript() {
    let _env_lock = crate::storage::lock_test_env();
    let mut app = create_test_app();
    app.swarm.plan_items = vec![pinned_band_todo("pinned", "PINNED_ONLY", "in_progress")];
    app.transcript.set_all(vec![
        DisplayMessage::tool(
            "duplicate todo transcript card",
            crate::message::ToolCall {
                id: "todo-tool".to_string(),
                name: "todo".to_string(),
                input: serde_json::json!({"todos": []}),
                intent: None,
                thought_signature: None,
            },
        ),
        DisplayMessage::tool(
            "ordinary tool remains visible",
            crate::message::ToolCall {
                id: "read-tool".to_string(),
                name: "read".to_string(),
                input: serde_json::json!({"file_path": "README.md"}),
                intent: None,
                thought_signature: None,
            },
        ),
    ]);
    app.bump_display_messages_version();
    app.session.short_name = Some("test".to_string());
    let backend = ratatui::backend::TestBackend::new(80, 40);
    let mut terminal = ratatui::Terminal::new(backend).expect("failed to create test terminal");
    let transcript = render_and_snap(&app, &mut terminal);
    assert!(!transcript.contains("duplicate todo transcript card"));
    assert!(transcript.contains("PINNED_ONLY"), "{transcript}");
}

#[test]
fn pinned_todo_band_renders_below_sticky_prompt_without_separator() {
    let _env_lock = crate::storage::lock_test_env();
    let _render_lock = crate::tui::ui::render_state_test_lock();
    let mut app = create_test_app();
    app.swarm.plan_items = vec![pinned_band_todo("t1", "pinned band item", "in_progress")];

    app.transcript.set_all(vec![
        DisplayMessage {
            role: "user".to_string(),
            content: "kick off the work".to_string(),
            tool_calls: vec![],
            duration_secs: None,
            title: None,
            tool_data: None,
        },
        DisplayMessage {
            role: "assistant".to_string(),
            content: App::build_scroll_test_content(0, 40, None),
            tool_calls: vec![],
            duration_secs: None,
            title: None,
            tool_data: None,
        },
    ]);
    app.bump_display_messages_version();
    app.viewport.scroll_offset = 0;
    app.viewport.auto_scroll_paused = false;
    app.is_processing = false;
    app.streaming.streaming_text.clear();
    app.status = ProcessingStatus::Idle;
    app.session.short_name = Some("test".to_string());

    let backend = ratatui::backend::TestBackend::new(60, 16);
    let mut terminal = ratatui::Terminal::new(backend).expect("failed to create test terminal");

    app.viewport.auto_scroll_paused = true;
    let top_text = render_and_snap(&app, &mut terminal);
    assert!(
        top_text.lines().take(6).any(|row| row.contains("pinned band item")),
        "pinned todo should remain visible at the top of scrollback, got:\n{}",
        top_text
    );

    app.viewport.auto_scroll_paused = false;
    let text = render_and_snap(&app, &mut terminal);

    let first_rows = text.lines().take(6).collect::<Vec<_>>();
    let prompt_row = first_rows
        .iter()
        .position(|row| row.contains("kick off the work"))
        .expect("sticky prompt should be visible");
    let todo_row = first_rows
        .iter()
        .position(|row| row.contains("pinned band item"))
        .expect("pinned todo should be visible");
    assert!(
        prompt_row < todo_row,
        "pinned todo band should render below the sticky prompt, got:\n{}",
        text
    );
    assert!(
        !first_rows.iter().any(|row| row.contains("────")),
        "pinned todo band should not render a horizontal separator, got:\n{}",
        text
    );
}

#[test]
fn background_task_rows_render_without_todos_or_transcript_cards() {
    let _env_lock = crate::storage::lock_test_env();
    let _render_lock = crate::tui::ui::render_state_test_lock();
    let mut app = create_test_app();
    app.session.short_name = Some("test".to_string());
    app.push_display_message(DisplayMessage::assistant("ordinary transcript content"));
    app.background_tasks.upsert_running(
        "running".to_string(),
        "cargo test".to_string(),
        Some(42.0),
    );
    app.background_tasks.finish(
        "done".to_string(),
        "release build".to_string(),
        crate::tui::BackgroundTaskRowStatus::Completed,
    );
    app.background_tasks.finish(
        "failed".to_string(),
        "integration tests".to_string(),
        crate::tui::BackgroundTaskRowStatus::Failed,
    );

    let backend = ratatui::backend::TestBackend::new(80, 20);
    let mut terminal = ratatui::Terminal::new(backend).expect("failed to create test terminal");
    let rendered = render_and_snap(&app, &mut terminal);

    assert!(
        rendered.contains("✓ bg release build  ━━━━━━ 100%"),
        "missing completed task row:\n{rendered}"
    );
    assert!(
        rendered.contains("× bg integration tests  ────── failed"),
        "missing failed task row:\n{rendered}"
    );
    assert!(
        !rendered.contains("◌ bg cargo test"),
        "only the two most recent task rows should render:\n{rendered}"
    );
    assert!(!rendered.contains("Background tasks"));
    assert!(!rendered.contains("Background task started"));
    assert!(!rendered.contains("Background task progress"));
    assert!(!rendered.contains("Background task completed"));
}

#[test]
fn background_task_rows_retain_the_two_most_recently_active_tasks() {
    let mut app = create_test_app();
    app.background_tasks.upsert_running("first".to_string(), "first task".to_string(), None);
    app.background_tasks.upsert_running("second".to_string(), "second task".to_string(), None);
    app.background_tasks.upsert_running(
        "first".to_string(),
        "first task updated".to_string(),
        Some(50.0),
    );
    app.background_tasks.upsert_running("third".to_string(), "third task".to_string(), None);

    assert_eq!(
        app.background_tasks.rows()
            .iter()
            .map(|row| row.task_id.as_str())
            .collect::<Vec<_>>(),
        vec!["first", "third"]
    );
}

#[test]
fn indeterminate_background_update_preserves_last_known_percent() {
    let _env_lock = crate::storage::lock_test_env();
    let _render_lock = crate::tui::ui::render_state_test_lock();
    let mut app = create_test_app();
    app.session.short_name = Some("test".to_string());
    app.push_display_message(DisplayMessage::assistant("ordinary transcript content"));
    app.background_tasks.upsert_running(
        "build".to_string(),
        "cargo build".to_string(),
        Some(42.0),
    );

    // A later phase-only parser update carries no percentage. It should update
    // activity/label state without resetting the visible bar to zero.
    app.background_tasks.upsert_running("build".to_string(), "Compiling kcode".to_string(), None);

    let row = app
        .background_tasks.rows()
        .iter()
        .find(|row| row.task_id == "build")
        .expect("background row should remain present");
    assert_eq!(row.percent, Some(42.0));
    assert_eq!(row.label, "Compiling kcode");

    let backend = ratatui::backend::TestBackend::new(80, 20);
    let mut terminal = ratatui::Terminal::new(backend).expect("failed to create test terminal");
    let rendered = render_and_snap(&app, &mut terminal);
    assert!(
        rendered.contains("Compiling kcode") && rendered.contains("42%"),
        "phase-only update reset or hid the visible percentage:\n{rendered}"
    );
}

#[test]
fn completed_background_tasks_clear_after_they_stop_being_relevant() {
    let mut app = create_test_app();
    app.background_tasks.finish(
        "failed".to_string(),
        "integration tests".to_string(),
        crate::tui::BackgroundTaskRowStatus::Failed,
    );
    app.background_tasks.upsert_running("running".to_string(), "cargo test".to_string(), None);
    app.background_tasks.finish(
        "done".to_string(),
        "release build".to_string(),
        crate::tui::BackgroundTaskRowStatus::Completed,
    );

    app.background_tasks.rows_mut()
        .iter_mut()
        .find(|row| row.task_id == "done")
        .unwrap()
        .completed_at = Some(std::time::Instant::now() - std::time::Duration::from_secs(13));

    assert!(app.background_tasks.prune_irrelevant());
    assert_eq!(
        app.background_tasks.rows()
            .iter()
            .map(|row| row.task_id.as_str())
            .collect::<Vec<_>>(),
        vec!["running"]
    );
    assert!(!app.background_tasks.prune_irrelevant());
}

#[test]
fn clicking_pinned_todo_more_row_expands_the_band() {
    use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};

    let _env_lock = crate::storage::lock_test_env();
    let _render_lock = crate::tui::ui::render_state_test_lock();
    let mut app = create_test_app();
    app.pinned_todos_expanded = false;
    crate::tui::ui::viewport::set_pinned_todo_more_area_for_test(Some(ratatui::layout::Rect {
        x: 2,
        y: 4,
        width: 20,
        height: 1,
    }));

    app.handle_mouse_event(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 8,
        row: 4,
        modifiers: KeyModifiers::NONE,
    });

    assert!(app.pinned_todos_expanded);
    crate::tui::ui::viewport::set_pinned_todo_more_area_for_test(None);
}

#[test]
fn pinned_todo_card_shows_tasks_without_expanding() {
    let _env_lock = crate::storage::lock_test_env();
    let _render_lock = crate::tui::ui::render_state_test_lock();
    let mut app = create_test_app();
    app.session.short_name = Some("test".to_string());
    let todos = vec![
        pinned_band_todo("done", "Finished task", "completed"),
        pinned_band_todo("active", "Current task", "in_progress"),
        pinned_band_todo("next", "Queued task", "pending"),
    ];
    app.swarm.plan_items = todos;
    app.push_display_message(DisplayMessage::assistant("ordinary transcript content"));
    for width in [40, 80, 120] {
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, 40)).unwrap();
        let rendered = render_and_snap(&app, &mut terminal);
        for label in ["Finished task", "Current task", "Queued task"] {
            assert!(rendered.contains(label), "missing {label}:\n{rendered}");
        }
        assert!(!rendered.contains("▸ todo"), "{rendered}");
        assert!(!rendered.contains("▾ todo"), "{rendered}");
        assert!(crate::tui::ui::viewport::pinned_todo_more_area().is_none());
        println!(
            "{width}x40: all 3 todo states visible on first render, no expansion required or summary toggle"
        );
    }
}
