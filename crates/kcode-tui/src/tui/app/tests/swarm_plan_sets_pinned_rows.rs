#[test]
fn swarm_plan_event_sets_the_pinned_rows_without_a_transcript_message() {
    let mut app = create_test_app();
    let rt = tokio::runtime::Runtime::new().unwrap();
    let _guard = rt.enter();
    let mut remote = crate::tui::backend::RemoteConnection::dummy();
    remote.mark_history_loaded();
    let message_count = app.display_messages().len();

    let item = crate::plan::TaskItem {
        content: "write a haiku".to_string(),
        id: "haiku-1".to_string(),
        assigned_to: Some("worker-fox".to_string()),
        ..Default::default()
    };

    app.handle_server_event(
        crate::protocol::ServerEvent::SwarmPlan {
            swarm_id: "test-swarm".to_string(),
            items: vec![item.clone()],
        },
        &mut remote,
    );

    assert_eq!(app.swarm.plan_items, vec![item]);
    assert_eq!(
        app.display_messages().len(),
        message_count,
        "a rows push should not add transcript messages"
    );
}
