#[tokio::test]
async fn assign_task_without_task_id_picks_highest_priority_runnable_task() {
    let (_env, _runtime) = RuntimeEnvGuard::new();
    let repo = scratch_repo();
    let swarm_id = "swarm-assign";
    let requester = "coord";
    let worker = "worker";
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
        (worker.to_string(), member(worker, swarm_id, "ready")),
    ])));
    set_repo(&swarm_members, repo.path()).await;
    let swarms_by_id = Arc::new(RwLock::new(HashMap::from([(
        swarm_id.to_string(),
        HashSet::from([requester.to_string(), worker.to_string()]),
    )])));
    let swarm_plans = Arc::new(RwLock::new(HashMap::from([(
        swarm_id.to_string(),
        VersionedPlan {
            items: vec![
                plan_item("done", "completed", "high", &[]),
                plan_item("blocked", "queued", "high", &["high-ready"]),
                plan_item("low-ready", "queued", "low", &["done"]),
                plan_item("high-ready", "queued", "high", &["done"]),
            ],
            task_progress: HashMap::new(),
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
        77,
        requester.to_string(),
        Some(worker.to_string()),
        None,
        Some("Pick the next task".to_string()),
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

    let response = client_rx.recv().await.expect("response");
    match response {
        ServerEvent::CommAssignTaskResponse {
            id,
            task_id,
            target_session,
        } => {
            assert_eq!(id, 77);
            assert_eq!(task_id, "high-ready");
            assert_eq!(target_session, worker);
        }
        other => panic!("expected CommAssignTaskResponse, got {other:?}"),
    }

    let plans = swarm_plans.read().await;
    let plan = plans.get(swarm_id).expect("plan exists");
    let selected = plan
        .items
        .iter()
        .find(|item| item.id == "high-ready")
        .expect("selected task exists");
    assert_eq!(selected.assigned_to.as_deref(), Some(worker));
    assert_eq!(selected.status, "queued");

    let blocked = plan
        .items
        .iter()
        .find(|item| item.id == "blocked")
        .expect("blocked task exists");
    assert!(
        blocked.assigned_to.is_none(),
        "blocked task should not be auto-assigned"
    );

    let members = swarm_members.read().await;
    let worker_member = members.get(worker).expect("worker member exists");
    assert_eq!(
        worker_member.status, SwarmLifecycleStatus::Queued,
        "assigned worker should stop looking completed/ready before async execution starts"
    );
}

#[tokio::test]
async fn assign_task_marks_completed_worker_queued_before_returning() {
    let (_env, _runtime) = RuntimeEnvGuard::new();
    let repo = scratch_repo();
    let swarm_id = "swarm-assign-completed-worker";
    let requester = "coord";
    let worker = "worker-completed";
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
        (worker.to_string(), member(worker, swarm_id, "completed")),
    ])));
    set_repo(&swarm_members, repo.path()).await;
    let swarms_by_id = Arc::new(RwLock::new(HashMap::from([(
        swarm_id.to_string(),
        HashSet::from([requester.to_string(), worker.to_string()]),
    )])));
    let swarm_plans = Arc::new(RwLock::new(HashMap::from([(
        swarm_id.to_string(),
        VersionedPlan {
            items: vec![plan_item("next", "queued", "high", &[])],
            task_progress: HashMap::new(),
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
        78,
        requester.to_string(),
        Some(worker.to_string()),
        Some("next".to_string()),
        None,
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
            assert_eq!(id, 78);
            assert_eq!(task_id, "next");
            assert_eq!(target_session, worker);
        }
        other => panic!("expected CommAssignTaskResponse, got {other:?}"),
    }

    let members = swarm_members.read().await;
    let worker_member = members.get(worker).expect("worker member exists");
    assert_eq!(worker_member.status, SwarmLifecycleStatus::Queued);
    assert!(
        worker_member
            .detail
            .as_deref()
            .is_some_and(|detail| detail.contains("task next")),
        "queued member should include the assigned task in its detail"
    );
}

/// A dispatch writes the holder where the list is, so a row the list does not have
/// is refused instead of dispatched: a plan item with no row is the plan holding
/// state the list does not, which is the shape this stage removes. Until the plan
/// is a view of the rows, this is the guard that catches the two disagreeing.
#[tokio::test]
async fn a_dispatch_refuses_a_row_the_list_does_not_have() {
    let (_env, _runtime) = RuntimeEnvGuard::new();
    let repo = scratch_repo();
    let swarm_id = "swarm-ghost-row";
    let requester = "coord";
    let worker = "worker";
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
            worker.to_string(),
            owned_member(worker, swarm_id, "ready", requester),
        ),
    ])));
    set_repo(&swarm_members, repo.path()).await;
    let swarms_by_id = Arc::new(RwLock::new(HashMap::from([(
        swarm_id.to_string(),
        HashSet::from([requester.to_string(), worker.to_string()]),
    )])));
    // Written empty on purpose: the plan below names a row no list has.
    write_list(repo.path(), requester, &[]);
    let swarm_plans = Arc::new(RwLock::new(HashMap::from([(
        swarm_id.to_string(),
        VersionedPlan {
            items: vec![plan_item("ghost", "queued", "high", &[])],
            task_progress: HashMap::new(),
        },
    )])));
    let swarm_coordinators = Arc::new(RwLock::new(HashMap::from([(
        swarm_id.to_string(),
        requester.to_string(),
    )])));
    let event_history = Arc::new(RwLock::new(VecDeque::new()));
    let event_counter = Arc::new(AtomicU64::new(1));
    let (swarm_event_tx, _swarm_event_rx) = broadcast::channel(32);
    let mutation_runtime = SwarmMutationRuntime::default();

    handle_comm_assign_task(
        79,
        requester.to_string(),
        Some(worker.to_string()),
        Some("ghost".to_string()),
        None,
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
        ServerEvent::Error { message, .. } => assert!(
            message.contains("could not be claimed in the list"),
            "expected the claim to be refused, got: {message}"
        ),
        other => panic!("expected a refusal, got {other:?}"),
    }
    let plans = swarm_plans.read().await;
    assert!(
        plans[swarm_id].items[0].assigned_to.is_none(),
        "a refused claim must leave the plan as it was"
    );
}

/// A dispatched turn that ends without an explicit close closes its own row: the
/// store writes the turn's report as the result (or says the turn reported nothing),
/// the record lands on the row that owns the work, and the plan stops holding the
/// row. This is the fallback for a worker that just ran, so a run cannot re-dispatch
/// a row whose turn already ended.
#[tokio::test]
async fn a_dispatched_turn_closes_its_row() {
    let (_env, _runtime) = RuntimeEnvGuard::new();
    let repo = scratch_repo();
    let swarm_id = "swarm-turn-close";
    let requester = "coord";
    let worker = "worker";
    let (client_tx, mut client_rx) = mpsc::unbounded_channel();
    let sessions: crate::server::SessionAgents = Arc::new(RwLock::new(HashMap::from([(
        worker.to_string(),
        test_agent().await,
    )])));
    let soft_interrupt_queues = Arc::new(RwLock::new(HashMap::new()));
    let client_connections = Arc::new(RwLock::new(HashMap::new()));
    let swarm_members = Arc::new(RwLock::new(HashMap::from([
        (requester.to_string(), {
            let mut member = member(requester, swarm_id, "ready");
            member.role = "coordinator".to_string();
            member
        }),
        (
            worker.to_string(),
            owned_member(worker, swarm_id, "ready", requester),
        ),
    ])));
    set_repo(&swarm_members, repo.path()).await;
    let swarms_by_id = Arc::new(RwLock::new(HashMap::from([(
        swarm_id.to_string(),
        HashSet::from([requester.to_string(), worker.to_string()]),
    )])));
    // The run's own row (the anchor) and the row this turn will work under it.
    let mut run_row = plan_item("run", "queued", "high", &[]);
    run_row.kind = Some("synthesize".to_string());
    run_row.assigned_to = Some(requester.to_string());
    let mut work_row = plan_item("work", "queued", "high", &[]);
    work_row.kind = Some("implement".to_string());
    work_row.assigned_to = Some(requester.to_string());
    // The row that owns the work takes the close's record.
    work_row.parent = Some("run".to_string());
    write_list(repo.path(), requester, &[run_row.clone(), work_row.clone()]);
    let swarm_plans = Arc::new(RwLock::new(HashMap::from([(
        swarm_id.to_string(),
        VersionedPlan {
            items: vec![run_row, work_row],
            task_progress: HashMap::new(),
        },
    )])));
    let swarm_coordinators = Arc::new(RwLock::new(HashMap::from([(
        swarm_id.to_string(),
        requester.to_string(),
    )])));
    let event_history = Arc::new(RwLock::new(VecDeque::new()));
    let event_counter = Arc::new(AtomicU64::new(1));
    let (swarm_event_tx, _swarm_event_rx) = broadcast::channel(32);
    let mutation_runtime = SwarmMutationRuntime::default();

    handle_comm_assign_task(
        81,
        requester.to_string(),
        Some(worker.to_string()),
        Some("work".to_string()),
        None,
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
    // The dispatch answers before its turn has finished; drain it so the turn's own
    // events do not accumulate.
    let _ = client_rx.try_recv();

    // The turn is spawned, so wait for its close to land (bounded).
    for _ in 0..200 {
        let closed = {
            let plans = swarm_plans.read().await;
            !plans[swarm_id].items.iter().any(|item| item.id == "work")
        };
        if closed {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    let plans = swarm_plans.read().await;
    assert!(
        !plans[swarm_id].items.iter().any(|item| item.id == "work"),
        "the turn's close takes the row out of the plan"
    );
    drop(plans);

    let rows = crate::todo::load_tasks(Some(repo.path()), requester).expect("read the list");
    assert!(
        !rows.iter().any(|row| row.id == "work"),
        "and out of the list"
    );
    let run = rows.iter().find(|row| row.id == "run").expect("the run row");
    let record = run
        .records
        .iter()
        .find(|record| record["id"] == "work")
        .expect("the close left its record on the row that owns the work");
    assert!(
        record["result"]
            .as_str()
            .is_some_and(|result| result.contains("the turn ended without a report")),
        "the turn stated its own outcome: {record}"
    );
}
