// A row that names a holder goes back to that holder.
//
// The assignment is the record of who owes the work, so a runnable row a live
// holder can take is not granted afresh to a free worker, and a caller's
// fresh-agent preference governs only rows nobody holds. The asking session's own
// rows are excluded, because the assign path refuses to assign a task to the
// session that asked.
//
// Included into the `comm_control::tests` module.

struct HandbackFixture {
    swarm_id: &'static str,
    coord: &'static str,
    holder: &'static str,
    other: &'static str,
    client_tx: mpsc::UnboundedSender<ServerEvent>,
    client_rx: mpsc::UnboundedReceiver<ServerEvent>,
    sessions: crate::server::SessionAgents,
    soft_interrupt_queues: crate::server::SessionInterruptQueues,
    client_connections: Arc<RwLock<HashMap<String, crate::server::ClientConnectionInfo>>>,
    swarm_members: Arc<RwLock<HashMap<String, SwarmMember>>>,
    swarms_by_id: Arc<RwLock<HashMap<String, HashSet<String>>>>,
    swarm_plans: Arc<RwLock<HashMap<String, VersionedPlan>>>,
    swarm_coordinators: Arc<RwLock<HashMap<String, String>>>,
    event_history: Arc<RwLock<VecDeque<SwarmEvent>>>,
    event_counter: Arc<AtomicU64>,
    swarm_event_tx: broadcast::Sender<SwarmEvent>,
    mutation_runtime: SwarmMutationRuntime,
    provider: Arc<dyn Provider>,
    global_session_id: Arc<RwLock<String>>,
    mcp_pool: Arc<crate::mcp::SharedMcpPool>,
}

async fn handback_fixture(rows: Vec<TaskItem>) -> HandbackFixture {
    let swarm_id = "swarm-handback";
    let coord = "coord";
    let holder = "worker-holder";
    let other = "worker-other";
    let (client_tx, client_rx) = mpsc::unbounded_channel();
    let swarm_members = Arc::new(RwLock::new(HashMap::from([
        (coord.to_string(), {
            let mut member = member(coord, swarm_id, "ready");
            member.role = "coordinator".to_string();
            member
        }),
        (
            holder.to_string(),
            owned_member(holder, swarm_id, "ready", coord),
        ),
        (
            other.to_string(),
            owned_member(other, swarm_id, "ready", coord),
        ),
    ])));
    HandbackFixture {
        swarm_id,
        coord,
        holder,
        other,
        client_tx,
        client_rx,
        sessions: Arc::new(RwLock::new(HashMap::new())),
        soft_interrupt_queues: Arc::new(RwLock::new(HashMap::new())),
        client_connections: Arc::new(RwLock::new(HashMap::new())),
        swarm_members,
        swarms_by_id: Arc::new(RwLock::new(HashMap::from([(
            swarm_id.to_string(),
            HashSet::from([coord.to_string(), holder.to_string(), other.to_string()]),
        )]))),
        swarm_plans: Arc::new(RwLock::new(HashMap::from([(
            swarm_id.to_string(),
            VersionedPlan {
                items: rows,
                version: 1,
                participants: HashSet::from([
                    coord.to_string(),
                    holder.to_string(),
                    other.to_string(),
                ]),
                task_progress: HashMap::new(),
                node_meta: HashMap::new(),
            },
        )]))),
        swarm_coordinators: Arc::new(RwLock::new(HashMap::from([(
            swarm_id.to_string(),
            coord.to_string(),
        )]))),
        event_history: Arc::new(RwLock::new(VecDeque::new())),
        event_counter: Arc::new(AtomicU64::new(1)),
        swarm_event_tx: broadcast::channel(32).0,
        mutation_runtime: SwarmMutationRuntime::default(),
        provider: Arc::new(TestProvider),
        global_session_id: Arc::new(RwLock::new(String::new())),
        mcp_pool: Arc::new(crate::mcp::SharedMcpPool::from_default_config()),
    }
}

/// Ask for the next row with the fresh-agent preference ON, which is what
/// `run_plan` uses by default. A held row must still go to its holder.
async fn assign_next_preferring_spawn(fx: &HandbackFixture) {
    handle_comm_assign_next(
        102,
        fx.coord.to_string(),
        None,
        None,
        Some(true),
        Some(true),
        None,
        None,
        None,
        &fx.client_tx,
        &fx.sessions,
        &fx.global_session_id,
        &fx.provider,
        &fx.soft_interrupt_queues,
        &fx.client_connections,
        &fx.swarm_members,
        &fx.swarms_by_id,
        &fx.swarm_plans,
        &fx.swarm_coordinators,
        &fx.event_history,
        &fx.event_counter,
        &fx.swarm_event_tx,
        &fx.mcp_pool,
        &fx.mutation_runtime,
    )
    .await;
}

#[tokio::test]
async fn a_ready_row_held_by_a_free_holder_goes_back_to_it() {
    let (_env, _runtime) = RuntimeEnvGuard::new();
    let mut held = plan_item("held", "queued", "high", &[]);
    held.assigned_to = Some("worker-holder".to_string());
    let mut fx = handback_fixture(vec![held]).await;

    assign_next_preferring_spawn(&fx).await;

    match fx.client_rx.recv().await.expect("response") {
        ServerEvent::CommAssignTaskResponse {
            task_id,
            target_session,
            ..
        } => {
            assert_eq!(task_id, "held");
            assert_eq!(target_session, fx.holder);
        }
        other => panic!("expected the held row to go back to its holder, got {other:?}"),
    }
}

#[tokio::test]
async fn a_held_row_is_not_granted_to_a_free_worker() {
    let (_env, _runtime) = RuntimeEnvGuard::new();
    let mut held = plan_item("held", "queued", "high", &[]);
    held.assigned_to = Some("worker-holder".to_string());
    let fx = handback_fixture(vec![held]).await;

    assign_next_preferring_spawn(&fx).await;

    let plans = fx.swarm_plans.read().await;
    let row = plans[fx.swarm_id]
        .items
        .iter()
        .find(|item| item.id == "held")
        .expect("the held row");
    assert_eq!(row.assigned_to.as_deref(), Some(fx.holder));
    assert_ne!(row.assigned_to.as_deref(), Some(fx.other));
}

#[tokio::test]
async fn a_held_row_whose_holder_is_working_is_left_alone() {
    let (_env, _runtime) = RuntimeEnvGuard::new();
    let mut held = plan_item("held", "queued", "high", &[]);
    held.assigned_to = Some("worker-holder".to_string());
    let mut fx = handback_fixture(vec![held]).await;
    // The holder is mid-turn: it cannot take the row back yet, and the row is
    // not free for anyone else either.
    {
        let mut members = fx.swarm_members.write().await;
        members.get_mut(fx.holder).unwrap().status = SwarmLifecycleStatus::Running;
    }

    assign_next_preferring_spawn(&fx).await;

    match fx.client_rx.recv().await.expect("response") {
        ServerEvent::Error { message, .. } => {
            assert!(
                message.starts_with("No runnable unassigned tasks"),
                "a busy holder must leave the row alone, got: {message}"
            );
        }
        other => panic!("expected no dispatch, got {other:?}"),
    }
}

#[tokio::test]
async fn the_asking_sessions_own_held_row_is_not_handed_back_to_it() {
    let (_env, _runtime) = RuntimeEnvGuard::new();
    // The coordinator holds a ready row. Handing it to itself would be refused by
    // the assign path ("cannot assign a swarm task to itself") and would fail the
    // whole run, so the picker excludes the asker's own rows.
    let mut own = plan_item("own", "queued", "high", &[]);
    own.assigned_to = Some("coord".to_string());
    let mut fx = handback_fixture(vec![own]).await;

    assign_next_preferring_spawn(&fx).await;

    match fx.client_rx.recv().await.expect("response") {
        ServerEvent::Error { message, .. } => {
            assert!(
                message.starts_with("No runnable unassigned tasks"),
                "the asker's own row must stay with it, got: {message}"
            );
        }
        other => panic!("expected no self hand-back, got {other:?}"),
    }
}
