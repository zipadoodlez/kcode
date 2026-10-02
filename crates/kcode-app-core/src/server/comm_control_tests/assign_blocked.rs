#[tokio::test]
async fn assign_task_rejects_explicit_blocked_task() {
    let (_env, _runtime) = RuntimeEnvGuard::new();
    let repo = scratch_repo();
    let requester = "coord";
    let worker = "worker";
    let (client_tx, mut client_rx) = mpsc::unbounded_channel();
    let worker_agent = test_agent().await;
    let sessions = Arc::new(RwLock::new(HashMap::from([(
        worker.to_string(),
        worker_agent,
    )])));
    let soft_interrupt_queues = Arc::new(RwLock::new(HashMap::new()));
    let client_connections = Arc::new(RwLock::new(HashMap::new()));
    let swarm_members = Arc::new(RwLock::new(HashMap::from([
        (requester.to_string(), member(requester, requester, "ready")),
        (worker.to_string(), owned_member(worker, requester, "ready", requester)),
    ])));
    set_repo(&swarm_members, repo.path()).await;
    let swarm_runs = seeded(repo.path(), vec![
        plan_item("setup", "completed", "high", &[]),
        plan_item("blocked", "queued", "high", &["missing-prereq"]),
    ]);
    let event_history = Arc::new(RwLock::new(VecDeque::new()));
    let event_counter = Arc::new(AtomicU64::new(1));
    let (swarm_event_tx, _swarm_event_rx) = broadcast::channel(32);
    let mutation_runtime = SwarmMutationRuntime::default();

    handle_comm_assign_task(
        88,
        requester.to_string(),
        Some(worker.to_string()),
        Some("blocked".to_string()),
        None,
        false,
        &client_tx,
        &sessions,
        &soft_interrupt_queues,
        &client_connections,
        &swarm_members,
        &swarm_runs,
        &event_history,
        &event_counter,
        &swarm_event_tx,
        &mutation_runtime)
    .await;

    match client_rx.recv().await.expect("response") {
        ServerEvent::Error { message, .. } => {
            assert!(message.contains("missing dependencies") || message.contains("blocked"));
        }
        other => panic!("expected error for blocked task assignment, got {other:?}"),
    }

    let rows = rows_in(repo.path());
    let blocked = rows
        .iter()
        .find(|item| item.id == "blocked")
        .expect("blocked task exists");
    assert!(
        blocked.assigned_to.is_none(),
        "blocked task should stay unassigned"
    );
}
