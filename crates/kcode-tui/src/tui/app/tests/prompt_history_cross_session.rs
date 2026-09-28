// Cross-session prompt history + Ctrl+R reverse search tests
// ====================================================================

#[test]
fn test_prompt_history_records_only_new_prompts_and_moves_repeats_to_front() {
    let mut app = create_test_app();

    app.record_prompt_history("first");
    app.record_prompt_history("second");
    app.record_prompt_history("first"); // repeat: moves to front, no duplicate

    let history = app.prompt_history.persisted.clone().unwrap();
    assert_eq!(history, vec!["second".to_string(), "first".to_string()]);
}

#[test]
fn test_prompt_history_skips_slash_shell_and_empty_inputs() {
    let mut app = create_test_app();

    app.record_prompt_history("/help");
    app.record_prompt_history("!ls -la");
    app.record_prompt_history("   ");
    app.record_prompt_history("");

    assert_eq!(
        app.prompt_history.persisted.as_deref().unwrap_or_default(),
        &[] as &[String]
    );
}

#[test]
fn test_prompt_history_skips_pending_login_input() {
    let mut app = create_test_app();
    app.pending_login = Some(PendingLogin::Gemini {
        verifier: "v".to_string(),
        expected_state: None,
        redirect_uri: "http://localhost".to_string(),
    });

    app.record_prompt_history("sk-secret-value");

    assert_eq!(
        app.prompt_history.persisted.as_deref().unwrap_or_default(),
        &[] as &[String]
    );
}

#[test]
fn test_up_arrow_recalls_prompts_from_previous_sessions() {
    let mut app = create_test_app();
    // Persisted history from earlier sessions, oldest first.
    app.prompt_history.persisted = Some(vec![
        "old session prompt".to_string(),
        "newer old prompt".to_string(),
    ]);
    // Current session has one prompt.
    app.transcript.set_all(vec![DisplayMessage::user("current prompt")]);
    app.bump_display_messages_version();

    app.handle_key(KeyCode::Up, KeyModifiers::empty()).unwrap();
    assert_eq!(app.composer.input, "current prompt");

    app.handle_key(KeyCode::Up, KeyModifiers::empty()).unwrap();
    assert_eq!(app.composer.input, "newer old prompt");

    app.handle_key(KeyCode::Up, KeyModifiers::empty()).unwrap();
    assert_eq!(app.composer.input, "old session prompt");

    app.handle_key(KeyCode::Down, KeyModifiers::empty())
        .unwrap();
    assert_eq!(app.composer.input, "newer old prompt");
}

#[test]
fn test_merged_prompt_history_dedupes_across_sessions() {
    let mut app = create_test_app();
    app.prompt_history.persisted =
        Some(vec!["shared prompt".to_string(), "unique old".to_string()]);
    app.transcript.set_all(vec![DisplayMessage::user("shared prompt")]);
    app.bump_display_messages_version();

    let merged = app.merged_prompt_history();
    assert_eq!(
        merged,
        vec!["unique old".to_string(), "shared prompt".to_string()]
    );
}

#[test]
fn test_ctrl_r_opens_history_search_and_enter_inserts_selection() {
    let mut app = create_test_app();
    app.prompt_history.persisted = Some(vec![
        "fix the login bug".to_string(),
        "write more tests".to_string(),
    ]);

    app.handle_key(KeyCode::Char('r'), KeyModifiers::CONTROL)
        .unwrap();
    assert!(app.prompt_history.search.is_some());
    // Readline-style: no results until the user types a query.
    assert!(app
        .prompt_history.search
        .as_ref()
        .unwrap()
        .matches
        .is_empty());

    // Type a query that matches only the older prompt.
    for c in "login".chars() {
        app.handle_key(KeyCode::Char(c), KeyModifiers::empty())
            .unwrap();
    }
    let state = app.prompt_history.search.as_ref().unwrap();
    assert_eq!(state.matches, vec!["fix the login bug".to_string()]);
    // The selected match previews live in the input line.
    assert_eq!(app.composer.input, "fix the login bug");

    app.handle_key(KeyCode::Enter, KeyModifiers::empty())
        .unwrap();
    assert!(app.prompt_history.search.is_none());
    assert_eq!(app.composer.input, "fix the login bug");
    assert_eq!(app.composer.cursor_pos, app.composer.input.len());
}

#[test]
fn test_history_search_esc_cancels_without_touching_input() {
    let mut app = create_test_app();
    app.prompt_history.persisted = Some(vec!["some prompt".to_string()]);
    app.composer.input = "draft".to_string();
    app.composer.cursor_pos = app.composer.input.len();

    app.handle_key(KeyCode::Char('r'), KeyModifiers::CONTROL)
        .unwrap();
    assert!(app.prompt_history.search.is_some());

    // Typing a matching query previews the match in the input line...
    for c in "some".chars() {
        app.handle_key(KeyCode::Char(c), KeyModifiers::empty())
            .unwrap();
    }
    assert_eq!(app.composer.input, "some prompt");

    // ...but Esc restores the original draft.
    app.handle_key(KeyCode::Esc, KeyModifiers::empty()).unwrap();
    assert!(app.prompt_history.search.is_none());
    assert_eq!(app.composer.input, "draft");
    assert_eq!(app.composer.cursor_pos, "draft".len());
}

#[test]
fn test_history_search_up_down_moves_selection() {
    let mut app = create_test_app();
    app.prompt_history.persisted = Some(vec![
        "prompt alpha".to_string(),
        "prompt beta".to_string(),
        "prompt gamma".to_string(),
    ]);

    app.handle_key(KeyCode::Char('r'), KeyModifiers::CONTROL)
        .unwrap();
    // Type a query matching all three prompts; newest-first order is
    // [gamma, beta, alpha].
    for c in "prompt".chars() {
        app.handle_key(KeyCode::Char(c), KeyModifiers::empty())
            .unwrap();
    }
    assert_eq!(app.prompt_history.search.as_ref().unwrap().selected, 0);
    assert_eq!(app.composer.input, "prompt gamma");

    app.handle_key(KeyCode::Up, KeyModifiers::empty()).unwrap();
    assert_eq!(app.prompt_history.search.as_ref().unwrap().selected, 1);
    assert_eq!(app.composer.input, "prompt beta");

    // Ctrl+R again also steps older (readline muscle memory).
    app.handle_key(KeyCode::Char('r'), KeyModifiers::CONTROL)
        .unwrap();
    assert_eq!(app.prompt_history.search.as_ref().unwrap().selected, 2);
    assert_eq!(app.composer.input, "prompt alpha");

    app.handle_key(KeyCode::Down, KeyModifiers::empty())
        .unwrap();
    assert_eq!(app.prompt_history.search.as_ref().unwrap().selected, 1);

    // Enter keeps the selected (middle) match in the input line.
    app.handle_key(KeyCode::Enter, KeyModifiers::empty())
        .unwrap();
    assert_eq!(app.composer.input, "prompt beta");
}

#[test]
fn test_prompt_history_file_roundtrip_dedupes_and_caps() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("prompt-history.jsonl");

    crate::tui::app::prompt_history::append_to_path(&path, "one");
    crate::tui::app::prompt_history::append_to_path(&path, "two");
    crate::tui::app::prompt_history::append_to_path(&path, "one"); // repeat

    let loaded = crate::tui::app::prompt_history::load_from_path(&path);
    // Dedupe keeps the most recent occurrence.
    assert_eq!(loaded, vec!["two".to_string(), "one".to_string()]);

    // Multiline prompts survive the JSONL roundtrip.
    crate::tui::app::prompt_history::append_to_path(&path, "multi\nline\nprompt");
    let loaded = crate::tui::app::prompt_history::load_from_path(&path);
    assert_eq!(loaded.last().unwrap(), "multi\nline\nprompt");
}

#[test]
fn test_submit_input_records_prompt_history() {
    let mut app = create_test_app();
    app.composer.input = "hello world".to_string();
    app.composer.cursor_pos = app.composer.input.len();

    app.submit_input();

    assert_eq!(
        app.prompt_history.persisted.as_deref().unwrap_or_default(),
        &["hello world".to_string()]
    );
}

#[test]
fn test_history_search_overlay_renders_matches_in_frame() {
    // Create the app before taking the render lock: create_test_app acquires
    // the same non-reentrant lock internally to clear render state.
    let mut app = create_test_app();
    let _lock = crate::tui::ui::render_state_test_lock();
    app.prompt_history.persisted = Some(vec![
        "refactor the parser".to_string(),
        "add prompt history".to_string(),
    ]);

    app.handle_key(KeyCode::Char('r'), KeyModifiers::CONTROL)
        .unwrap();
    assert!(app.prompt_history.search.is_some());

    let backend = ratatui::backend::TestBackend::new(80, 16);
    let mut terminal = ratatui::Terminal::new(backend).expect("failed to create test terminal");
    terminal
        .draw(|f| crate::tui::ui::draw(f, &app))
        .expect("draw failed");
    let rendered = buffer_to_text(&terminal);

    assert!(
        rendered.contains("history search"),
        "overlay header missing from frame:\n{rendered}"
    );
    // No results are shown until the user types a query.
    assert!(
        rendered.contains("type to search history"),
        "empty-query hint missing from frame:\n{rendered}"
    );
    assert!(
        !rendered.contains("add prompt history"),
        "matches should be hidden before a query is typed:\n{rendered}"
    );

    // Filter down to one match and re-render.
    for c in "parser".chars() {
        app.handle_key(KeyCode::Char(c), KeyModifiers::empty())
            .unwrap();
    }
    terminal
        .draw(|f| crate::tui::ui::draw(f, &app))
        .expect("draw failed");
    let rendered = buffer_to_text(&terminal);
    assert!(
        rendered.contains("refactor the parser"),
        "filtered match missing from frame:\n{rendered}"
    );
    assert!(
        !rendered.contains("add prompt history"),
        "non-matching prompt should be filtered out:\n{rendered}"
    );
}
