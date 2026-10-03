#[test]
fn test_stdin_response_roundtrip() -> Result<()> {
    let req = Request::StdinResponse {
        id: 99,
        request_id: "stdin-call_abc-1".to_string(),
        input: "my_password".to_string(),
    };
    let json = serde_json::to_string(&req)?;
    assert!(json.contains("\"type\":\"stdin_response\""));
    assert!(json.contains("\"request_id\":\"stdin-call_abc-1\""));
    assert!(json.contains("\"input\":\"my_password\""));

    let decoded = parse_request_json(&json)?;
    assert_eq!(decoded.id(), 99);
    let Request::StdinResponse {
        request_id, input, ..
    } = decoded
    else {
        return Err(anyhow!("expected StdinResponse"));
    };
    assert_eq!(request_id, "stdin-call_abc-1");
    assert_eq!(input, "my_password");
    Ok(())
}

#[test]
fn test_stdin_response_deserialize_from_json() -> Result<()> {
    let json = r#"{"type":"stdin_response","id":5,"request_id":"req-42","input":"hello world"}"#;
    let decoded = parse_request_json(json)?;
    assert_eq!(decoded.id(), 5);
    let Request::StdinResponse {
        request_id, input, ..
    } = decoded
    else {
        return Err(anyhow!("expected StdinResponse"));
    };
    assert_eq!(request_id, "req-42");
    assert_eq!(input, "hello world");
    Ok(())
}

#[test]
fn test_stdin_request_event_roundtrip() -> Result<()> {
    let event = ServerEvent::StdinRequest {
        request_id: "stdin-xyz-1".to_string(),
        prompt: "Password: ".to_string(),
        is_password: true,
        tool_call_id: "call_abc".to_string(),
    };
    let json = encode_event(&event);
    assert!(json.contains("\"type\":\"stdin_request\""));
    assert!(json.contains("\"is_password\":true"));

    let decoded = parse_event_json(json.trim())?;
    let ServerEvent::StdinRequest {
        request_id,
        prompt,
        is_password,
        tool_call_id,
    } = decoded
    else {
        return Err(anyhow!("expected StdinRequest"));
    };
    assert_eq!(request_id, "stdin-xyz-1");
    assert_eq!(prompt, "Password: ");
    assert!(is_password);
    assert_eq!(tool_call_id, "call_abc");
    Ok(())
}

#[test]
fn test_stdin_request_event_defaults() -> Result<()> {
    // is_password defaults to false when not present
    let json = r#"{"type":"stdin_request","request_id":"r1","prompt":"","tool_call_id":"tc1"}"#;
    let decoded = parse_event_json(json)?;
    let ServerEvent::StdinRequest { is_password, .. } = decoded else {
        return Err(anyhow!("expected StdinRequest"));
    };
    assert!(!is_password, "is_password should default to false");
    Ok(())
}

#[test]
fn test_comm_stop_roundtrip() -> Result<()> {
    let req = Request::CommStop {
        id: 61,
        session_id: "sess_coord".to_string(),
        target_session: "sess_worker".to_string(),
    };
    let json = serde_json::to_string(&req)?;
    assert!(json.contains("\"type\":\"comm_stop\""));
    let decoded = parse_request_json(&json)?;
    assert_eq!(decoded.id(), 61);
    let Request::CommStop {
        session_id,
        target_session,
        ..
    } = decoded
    else {
        return Err(anyhow!("expected CommStop"));
    };
    assert_eq!(session_id, "sess_coord");
    assert_eq!(target_session, "sess_worker");
    Ok(())
}

#[test]
fn test_comm_spawn_roundtrip_with_optional_nonce() -> Result<()> {
    let req = Request::CommSpawn {
        id: 59,
        session_id: "sess_coord".to_string(),
        working_dir: Some("/tmp/project".to_string()),
        initial_message: Some("Start here".to_string()),
        request_nonce: Some("planner-fresh-123".to_string()),
        spawn_mode: Some("headless".to_string()),
        model: Some("openai-api:gpt-5.5".to_string()),
        label: Some("review auth flow".to_string()),
    };
    let json = serde_json::to_string(&req)?;
    assert!(json.contains("\"type\":\"comm_spawn\""));
    assert!(json.contains("\"request_nonce\":\"planner-fresh-123\""));
    assert!(json.contains("\"spawn_mode\":\"headless\""));
    assert!(json.contains("\"label\":\"review auth flow\""));
    assert!(json.contains("\"model\":\"openai-api:gpt-5.5\""));
    let decoded = parse_request_json(&json)?;
    assert_eq!(decoded.id(), 59);
    let Request::CommSpawn {
        session_id,
        working_dir,
        initial_message,
        request_nonce,
        spawn_mode,
        model,
        label,
        ..
    } = decoded
    else {
        return Err(anyhow!("expected CommSpawn"));
    };
    assert_eq!(session_id, "sess_coord");
    assert_eq!(working_dir.as_deref(), Some("/tmp/project"));
    assert_eq!(initial_message.as_deref(), Some("Start here"));
    assert_eq!(request_nonce.as_deref(), Some("planner-fresh-123"));
    assert_eq!(spawn_mode.as_deref(), Some("headless"));
    assert_eq!(model.as_deref(), Some("openai-api:gpt-5.5"));
    assert_eq!(label.as_deref(), Some("review auth flow"));
    Ok(())
}

#[test]
fn test_comm_spawn_decode_model_alone() -> Result<()> {
    let json = serde_json::json!({
        "type": "comm_spawn",
        "id": 60,
        "session_id": "sess_coord",
        "model": "gpt-5.5"
    });
    let decoded = parse_request_json(&json.to_string())?;
    match decoded {
        Request::CommSpawn {
            model, label, ..
        } => {
            assert_eq!(model.as_deref(), Some("gpt-5.5"));
            assert_eq!(label, None);
        }
        _ => return Err(anyhow!("expected spawn")),
    }
    Ok(())
}

#[test]
fn test_comm_spawn_roundtrip_omitted_or_null_model() -> Result<()> {
    for explicit_null in [false, true] {
        // Older clients omit the optional field. Explicit null must also work.
        let mut json = serde_json::json!({
            "type": "comm_spawn",
            "id": 60,
            "session_id": "sess_coord"
        });
        if explicit_null {
            json["model"] = serde_json::Value::Null;
        }
        let decoded = parse_request_json(&json.to_string())?;
        let encoded = serde_json::to_string(&decoded)?;
        let roundtripped = parse_request_json(&encoded)?;
        for request in [decoded, roundtripped] {
            assert_eq!(request.id(), 60);
            match request {
                Request::CommSpawn {
                    model, label, ..
                } => {
                    assert_eq!(model, None);
                    assert_eq!(label, None);
                }
                _ => return Err(anyhow!("expected spawn")),
            }
        }
    }
    Ok(())
}

#[test]
fn test_comm_list_models_roundtrip() -> Result<()> {
    let req = Request::CommListModels {
        id: 61,
        session_id: "sess_coord".to_string(),
    };
    let json = serde_json::to_string(&req)?;
    assert!(json.contains("\"type\":\"comm_list_models\""));
    let decoded = parse_request_json(&json)?;
    assert_eq!(decoded.id(), 61);
    assert!(decoded.is_lightweight_control_request());
    let Request::CommListModels { session_id, .. } = decoded else {
        return Err(anyhow!("expected CommListModels"));
    };
    assert_eq!(session_id, "sess_coord");
    Ok(())
}

#[test]
fn targeted_notification_does_not_require_subscription() -> Result<()> {
    let request = Request::NotifySession {
        id: 92,
        session_id: "existing-session".to_string(),
        message: "scheduled reminder".to_string(),
    };
    let decoded = parse_request_json(&serde_json::to_string(&request)?)?;
    assert_eq!(decoded.id(), 92);
    assert!(decoded.is_lightweight_control_request());
    assert!(
        matches!(decoded, Request::NotifySession { session_id, message, .. }
        if session_id == "existing-session" && message == "scheduled reminder")
    );
    Ok(())
}

#[test]
fn test_reload_force_defaults_true_for_legacy_clients() -> Result<()> {
    // Old clients (and the desktop Swift enum, which has no reload case) send a
    // reload request with no `force` field. It must default to true so their
    // behavior stays unconditional, matching the pre-#291 protocol.
    let json = r#"{"type":"reload","id":7}"#;
    let decoded = parse_request_json(json)?;
    let Request::Reload { id, force } = decoded else {
        return Err(anyhow!("expected Reload"));
    };
    assert_eq!(id, 7);
    assert!(force, "missing force must default to true");
    Ok(())
}

#[test]
fn test_reload_force_roundtrip() -> Result<()> {
    for force in [false, true] {
        let req = Request::Reload { id: 9, force };
        let json = serde_json::to_string(&req)?;
        assert!(json.contains("\"type\":\"reload\""));
        let decoded = parse_request_json(&json)?;
        let Request::Reload {
            id,
            force: decoded_force,
        } = decoded
        else {
            return Err(anyhow!("expected Reload"));
        };
        assert_eq!(id, 9);
        assert_eq!(decoded_force, force);
    }
    Ok(())
}
