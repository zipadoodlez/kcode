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
