#[test]
fn test_swarm_plan_event_roundtrip() -> Result<()> {
    let event = ServerEvent::SwarmPlan {
        swarm_id: "swarm_123".to_string(),
        items: vec![TaskItem {
            content: "Investigate planner state".to_string(),
            status: "queued".to_string(),
            priority: "high".to_string(),
            id: "task-1".to_string(),
            ..Default::default()
        }],
    };
    let json = encode_event(&event);
    assert!(json.contains("\"type\":\"swarm_plan\""));
    let decoded = parse_event_json(json.trim())?;
    let ServerEvent::SwarmPlan { swarm_id, items } = decoded else {
        return Err(anyhow!("expected SwarmPlan event"));
    };
    assert_eq!(swarm_id, "swarm_123");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].id, "task-1");
    Ok(())
}
#[test]
fn test_session_close_requested_roundtrip() -> Result<()> {
    let event = ServerEvent::SessionCloseRequested {
        reason: "Stopped by coordinator coord".to_string(),
    };
    let json = encode_event(&event);
    assert!(json.contains("\"type\":\"session_close_requested\""));
    let decoded = parse_event_json(json.trim())?;
    let ServerEvent::SessionCloseRequested { reason } = decoded else {
        return Err(anyhow!("expected SessionCloseRequested"));
    };
    assert_eq!(reason, "Stopped by coordinator coord");
    Ok(())
}
