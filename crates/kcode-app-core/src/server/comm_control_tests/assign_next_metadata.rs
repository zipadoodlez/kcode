#[tokio::test]
async fn assign_next_prefers_worker_with_matching_subsystem_metadata() {
    let (_env, _runtime) = RuntimeEnvGuard::new();
    let repo = scratch_repo();
    let requester = "coord";
    let metadata_worker = "worker-metadata";
    let other_worker = "worker-other";
    let (client_tx, mut client_rx) = mpsc::unbounded_channel();
    let sessions = Arc::new(RwLock::new(HashMap::new()));
    let soft_interrupt_queues = Arc::new(RwLock::new(HashMap::new()));
    let client_connections = Arc::new(RwLock::new(HashMap::new()));
    let swarm_members = Arc::new(RwLock::new(HashMap::from([
        (requester.to_string(), member(requester, requester, "ready")),
        (
            metadata_worker.to_string(),
            owned_member(metadata_worker, requester, "ready", requester),
        ),
        (
            other_worker.to_string(),
            owned_member(other_worker, requester, "ready", requester),
        ),
    ])));
    set_repo(&swarm_members, repo.path()).await;
    let mut prior = plan_item("prior", "completed", "high", &[]);
    prior.subsystem = Some("parser".to_string());
    prior.file_scope = vec!["src/parser.rs".to_string()];
    prior.assigned_to = Some(metadata_worker.to_string());
    let mut next = plan_item("next", "queued", "high", &[]);
    next.subsystem = Some("parser".to_string());
    next.file_scope = vec!["src/parser.rs".to_string()];
    let swarm_runs = seeded(repo.path(), vec![prior, next]);
    let event_history = Arc::new(RwLock::new(VecDeque::new()));
    let event_counter = Arc::new(AtomicU64::new(1));
    let (swarm_event_tx, _swarm_event_rx) = broadcast::channel(32);
    let mutation_runtime = SwarmMutationRuntime::default();
    let provider: Arc<dyn Provider> = Arc::new(TestProvider);
    let global_session_id = Arc::new(RwLock::new(String::new()));
    let mcp_pool = Arc::new(crate::mcp::SharedMcpPool::from_default_config());

    handle_comm_assign_next(
        103,
        requester.to_string(),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        &client_tx,
        &sessions,
        &global_session_id,
        &provider,
        &soft_interrupt_queues,
        &client_connections,
        &swarm_members,
        &swarm_runs,
        &event_history,
        &event_counter,
        &swarm_event_tx,
        &mcp_pool,
        &mutation_runtime,
    )
    .await;

    match client_rx.recv().await.expect("response") {
        ServerEvent::CommAssignTaskResponse {
            id,
            task_id,
            target_session,
        } => {
            assert_eq!(id, 103);
            assert_eq!(task_id, "next");
            assert_eq!(target_session, metadata_worker);
        }
        other => panic!("expected CommAssignTaskResponse, got {other:?}"),
    }
}
