#[test]
fn spawn_initial_message_accepts_prompt_alias_and_prefers_explicit_initial_message() {
    let from_prompt: CommunicateInput = serde_json::from_value(serde_json::json!({
        "action": "spawn",
        "prompt": "review the diff"
    }))
    .expect("prompt alias should deserialize");
    assert_eq!(
        from_prompt.spawn_initial_message().as_deref(),
        Some("review the diff")
    );

    let preferred: CommunicateInput = serde_json::from_value(serde_json::json!({
        "action": "spawn",
        "initial_message": "preferred",
        "prompt": "fallback"
    }))
    .expect("spawn payload should deserialize");
    assert_eq!(
        preferred.spawn_initial_message().as_deref(),
        Some("preferred")
    );

    for blank_initial_message in ["", " \t\n"] {
        let from_prompt: CommunicateInput = serde_json::from_value(serde_json::json!({
            "action": "spawn",
            "initial_message": blank_initial_message,
            "prompt": "fallback"
        }))
        .expect("spawn payload should deserialize");
        assert_eq!(
            from_prompt.spawn_initial_message().as_deref(),
            Some("fallback")
        );
    }

    let blank_messages: CommunicateInput = serde_json::from_value(serde_json::json!({
        "action": "spawn",
        "initial_message": "",
        "prompt": "  "
    }))
    .expect("spawn payload should deserialize");
    assert_eq!(blank_messages.spawn_initial_message(), None);
}

#[test]
fn communicate_input_accepts_delivery_and_share_append() {
    let delivery: CommunicateInput = serde_json::from_value(serde_json::json!({
        "action": "dm",
        "message": "ping",
        "to_session": "sess-2",
        "delivery": "wake"
    }))
    .expect("delivery mode should deserialize");
    assert_eq!(
        delivery.delivery,
        Some(crate::protocol::CommDeliveryMode::Wake)
    );

    let append: CommunicateInput = serde_json::from_value(serde_json::json!({
        "action": "share_append",
        "key": "task/123/notes",
        "value": "new line"
    }))
    .expect("share_append should deserialize");
    assert_eq!(append.action, "share_append");
}

#[test]
fn cleanup_candidates_default_to_owned_terminal_workers() {
    let members = vec![
        AgentInfo {
            session_id: "coord".to_string(),
            friendly_name: Some("coord".to_string()),
            files_touched: vec![],
            status: Some(SwarmLifecycleStatus::Ready),
            detail: None,
            role: Some("coordinator".to_string()),
            is_headless: None,
            report_back_to_session_id: None,
            latest_completion_report: None,
            live_attachments: None,
            status_age_secs: None,
            ..Default::default()
        },
        AgentInfo {
            session_id: "owned-done".to_string(),
            friendly_name: Some("owned".to_string()),
            files_touched: vec![],
            status: Some(SwarmLifecycleStatus::Completed),
            detail: None,
            role: Some("agent".to_string()),
            is_headless: Some(true),
            report_back_to_session_id: Some("coord".to_string()),
            latest_completion_report: None,
            live_attachments: None,
            status_age_secs: None,
            ..Default::default()
        },
        AgentInfo {
            session_id: "user-created".to_string(),
            friendly_name: Some("user".to_string()),
            files_touched: vec![],
            status: Some(SwarmLifecycleStatus::Completed),
            detail: None,
            role: Some("agent".to_string()),
            is_headless: None,
            report_back_to_session_id: None,
            latest_completion_report: None,
            live_attachments: None,
            status_age_secs: None,
            ..Default::default()
        },
        AgentInfo {
            session_id: "owned-running".to_string(),
            friendly_name: Some("running".to_string()),
            files_touched: vec![],
            status: Some(SwarmLifecycleStatus::Running),
            detail: None,
            role: Some("agent".to_string()),
            is_headless: Some(true),
            report_back_to_session_id: Some("coord".to_string()),
            latest_completion_report: None,
            live_attachments: None,
            status_age_secs: None,
            ..Default::default()
        },
    ];
    let statuses = default_cleanup_target_statuses();
    assert_eq!(
        cleanup_candidate_session_ids("coord", &members, &statuses, &[], false),
        vec!["owned-done".to_string()]
    );
    assert_eq!(
        cleanup_candidate_session_ids("coord", &members, &statuses, &[], true),
        vec!["owned-done".to_string(), "user-created".to_string()]
    );
}
#[test]
fn format_awaited_members_disambiguates_duplicate_friendly_names() {
    let output = format_awaited_members(
        true,
        "done",
        &[
            AwaitedMemberStatus {
                session_id: "session_shark_1234567890_aaaaaaaaaaaa0001".to_string(),
                friendly_name: Some("shark".to_string()),
                status: SwarmLifecycleStatus::Ready,
                done: true,
                completion_report: None,
            },
            AwaitedMemberStatus {
                session_id: "session_shark_1234567890_bbbbbbbbbbbb0002".to_string(),
                friendly_name: Some("shark".to_string()),
                status: SwarmLifecycleStatus::Ready,
                done: true,
                completion_report: None,
            },
        ],
    );

    assert!(output.output.contains("✓ shark [aa0001] (ready)"));
    assert!(output.output.contains("✓ shark [bb0002] (ready)"));
}