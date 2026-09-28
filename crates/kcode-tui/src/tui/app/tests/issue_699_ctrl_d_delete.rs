// Issue #699: Ctrl+D must forward-delete a character while the input line has
// text (readline/terminal convention), and only quit on an empty input line.

#[test]
fn test_ctrl_d_deletes_character_under_cursor() {
    let mut app = create_test_app();
    app.composer.input = "hello".to_string();
    app.composer.cursor_pos = 1;

    app.handle_key(KeyCode::Char('d'), KeyModifiers::CONTROL)
        .unwrap();

    assert_eq!(app.composer.input, "hllo");
    assert_eq!(app.composer.cursor_pos, 1);
    assert!(
        app.quit_pending.is_none(),
        "Ctrl+D with text in the input must not arm quit"
    );
}

#[test]
fn test_ctrl_d_at_end_of_non_empty_input_does_not_quit() {
    let mut app = create_test_app();
    app.composer.input = "hello".to_string();
    app.composer.cursor_pos = app.composer.input.len();

    app.handle_key(KeyCode::Char('d'), KeyModifiers::CONTROL)
        .unwrap();

    assert_eq!(app.composer.input, "hello", "nothing to delete forward");
    assert!(
        app.quit_pending.is_none(),
        "Ctrl+D must never quit while the input line has pending text"
    );
}

#[test]
fn test_ctrl_d_on_empty_input_still_requests_quit() {
    let mut app = create_test_app();
    assert!(app.composer.input.is_empty());

    app.handle_key(KeyCode::Char('d'), KeyModifiers::CONTROL)
        .unwrap();

    assert!(
        app.quit_pending.is_some() || app.cancel_requested,
        "Ctrl+D on an empty line keeps the existing interrupt/quit behavior"
    );
}

#[test]
fn test_ctrl_d_deletes_multibyte_character() {
    let mut app = create_test_app();
    app.composer.input = "héllo".to_string();
    app.composer.cursor_pos = 1;

    app.handle_key(KeyCode::Char('d'), KeyModifiers::CONTROL)
        .unwrap();

    assert_eq!(app.composer.input, "hllo");
}

// The normal `kcode` TUI runs as a remote client (`is_remote = true`), so the
// remote key path is the one real users hit. Cover it explicitly rather than
// trusting that it mirrors the local handler.

#[test]
fn test_remote_ctrl_d_deletes_character_under_cursor() {
    with_temp_kcode_home(|| {
        let mut app = create_test_app();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let _guard = rt.enter();
        let mut remote = crate::tui::backend::RemoteConnection::dummy();

        app.set_runtime_mode(crate::tui::app::AppRuntimeMode::RemoteClient);
        app.composer.input = "hello".to_string();
        app.composer.cursor_pos = 1;

        rt.block_on(app.handle_remote_key(
            KeyCode::Char('d'),
            KeyModifiers::CONTROL,
            &mut remote,
        ))
        .expect("Ctrl+D should be handled in the remote path");

        assert_eq!(app.composer.input, "hllo");
        assert!(
            app.quit_pending.is_none(),
            "Ctrl+D with text in the input must not arm quit in a remote session"
        );
    });
}

#[test]
fn test_remote_ctrl_d_on_empty_input_still_requests_quit() {
    with_temp_kcode_home(|| {
        let mut app = create_test_app();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let _guard = rt.enter();
        let mut remote = crate::tui::backend::RemoteConnection::dummy();

        app.set_runtime_mode(crate::tui::app::AppRuntimeMode::RemoteClient);
        assert!(app.composer.input.is_empty());

        rt.block_on(app.handle_remote_key(
            KeyCode::Char('d'),
            KeyModifiers::CONTROL,
            &mut remote,
        ))
        .expect("Ctrl+D should be handled in the remote path");

        assert!(
            app.quit_pending.is_some() || app.cancel_requested,
            "Ctrl+D on an empty remote line keeps interrupt/quit behavior"
        );
    });
}
