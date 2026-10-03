#[test]
fn test_swarm_plan_event_roundtrip_with_summary() -> Result<()> {
    let event = ServerEvent::SwarmPlan {
        swarm_id: "swarm_123".to_string(),
        items: vec![TaskItem {
            content: "Investigate planner state".to_string(),
            status: "queued".to_string(),
            priority: "high".to_string(),
            id: "task-1".to_string(),
            ..Default::default()
        }],
        reason: Some("task_completed".to_string()),
        summary: Some(PlanGraphStatus {
            swarm_id: Some("swarm_123".to_string()),
            item_count: 1,
            ready_ids: vec!["task-1".to_string()],
            blocked_ids: Vec::new(),
            active_ids: Vec::new(),
            failed_ids: Vec::new(),
            failed_reasons: Default::default(),
            cycle_ids: Vec::new(),
            unresolved_dependency_ids: Vec::new(),
            next_ready_ids: vec!["task-1".to_string()],
            newly_ready_ids: Vec::new(),
        }),
    };
    let json = encode_event(&event);
    assert!(json.contains("\"type\":\"swarm_plan\""));
    assert!(json.contains("\"summary\""));
    let decoded = parse_event_json(json.trim())?;
    let ServerEvent::SwarmPlan {
        swarm_id,
        items,
        reason,
        summary,
    } = decoded
    else {
        return Err(anyhow!("expected SwarmPlan event"));
    };
    assert_eq!(swarm_id, "swarm_123");
    assert_eq!(reason.as_deref(), Some("task_completed"));
    assert_eq!(items.len(), 1);
    let summary = summary.ok_or_else(|| anyhow!("expected plan summary"))?;
    assert_eq!(summary.ready_ids, vec!["task-1"]);
    assert_eq!(summary.next_ready_ids, vec!["task-1"]);
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
