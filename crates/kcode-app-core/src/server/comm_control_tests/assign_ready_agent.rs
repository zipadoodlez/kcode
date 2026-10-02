#[tokio::test]
async fn assign_task_without_target_picks_ready_agent() {
    let (_env, _runtime) = RuntimeEnvGuard::new();
    let repo = scratch_repo();
    let swarm_id = "swarm-auto-target";
    let requester = "coord";
    let ready_worker = "worker-ready";
    let completed_worker = "worker-completed";
    let running_worker = "worker-running";
    let (client_tx, mut client_rx) = mpsc::unbounded_channel();
    let sessions = Arc::new(RwLock::new(HashMap::new()));
    let soft_interrupt_queues = Arc::new(RwLock::new(HashMap::new()));
    let client_connections = Arc::new(RwLock::new(HashMap::new()));
    let swarm_members = Arc::new(RwLock::new(HashMap::from([
        (requester.to_string(), {
            let mut member = member(requester, swarm_id, "ready");
            member.role = "coordinator".to_string();
            member
        }),
        (
            ready_worker.to_string(),
            owned_member(ready_worker, swarm_id, "ready", requester),
        ),
        (
            completed_worker.to_string(),
            owned_member(completed_worker, swarm_id, "completed", requester),
        ),
        (
            running_worker.to_string(),
            owned_member(running_worker, swarm_id, "running", requester),
        ),
    ])));
    set_repo(&swarm_members, repo.path()).await;
    let swarms_by_id = Arc::new(RwLock::new(HashMap::from([(
        swarm_id.to_string(),
        HashSet::from([
            requester.to_string(),
            ready_worker.to_string(),
            completed_worker.to_string(),
            running_worker.to_string(),
        ]),
    )])));
    let swarm_plans = Arc::new(RwLock::new(HashMap::from([(
        swarm_id.to_string(),
        VersionedPlan {
            items: vec![
                plan_item("setup", "completed", "high", &[]),
                plan_item("next", "queued", "high", &["setup"]),
            ],
            version: 1,
            participants: HashSet::from([
                requester.to_string(),
                ready_worker.to_string(),
                completed_worker.to_string(),
                running_worker.to_string(),
            ]),
            task_progress: HashMap::new(),
            node_meta: HashMap::new(),
        },
    )])));
    // The plan needs rows behind it: the file is what a dispatch writes to,
    // and the repo root is that file's home.
    write_list(repo.path(), "fixture", &swarm_plans.read().await[swarm_id].items);
    let swarm_coordinators = Arc::new(RwLock::new(HashMap::from([(
        swarm_id.to_string(),
        requester.to_string(),
    )])));
    let event_history = Arc::new(RwLock::new(VecDeque::new()));
    let event_counter = Arc::new(AtomicU64::new(1));
    let (swarm_event_tx, _swarm_event_rx) = broadcast::channel(32);
    let mutation_runtime = SwarmMutationRuntime::default();

    handle_comm_assign_task(
        99,
        requester.to_string(),
        None,
        None,
        Some("Pick a task and worker".to_string()),
        &client_tx,
        &sessions,
        &soft_interrupt_queues,
        &client_connections,
        &swarm_members,
        &swarms_by_id,
        &swarm_plans,
        &swarm_coordinators,
        &event_history,
        &event_counter,
        &swarm_event_tx,
        &mutation_runtime,
    )
    .await;

    match client_rx.recv().await.expect("response") {
        ServerEvent::CommAssignTaskResponse {
            id,
            task_id,
            target_session,
        } => {
            assert_eq!(id, 99);
            assert_eq!(task_id, "next");
            assert_eq!(target_session, ready_worker);
        }
        other => panic!("expected CommAssignTaskResponse, got {other:?}"),
    }

    // The holder is a fact about the list, so the row the dispatch handed out
    // names the worker it handed it to.
    let rows = crate::todo::load_tasks(Some(repo.path()), requester).expect("read the list");
    let handed = rows.iter().find(|row| row.id == "next").expect("the row");
    assert_eq!(handed.assigned_to.as_deref(), Some(ready_worker));
}
