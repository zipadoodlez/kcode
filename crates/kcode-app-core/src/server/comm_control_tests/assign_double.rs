// Taking a row over: an explicit `assign_task` naming a row another session holds
// moves the row to the named session and dispatches it. A claim is a lock for a run
// and not for the user's session, so there is no double-assignment refusal: the
// displaced holder finds out on its next write (a close by a non-holder is refused
// in the store). What is still refused is a member dispatching for the run; that is
// `require_run_root`, covered by the run-gate tests.

#[allow(clippy::type_complexity)]
async fn takeover_fixture(
    repo: &std::path::Path,
    requester: &str,
    holder: &str,
    taker: &str,
    contested: TaskItem,
) -> (
    Arc<RwLock<HashMap<String, Arc<Mutex<Agent>>>>>,
    Arc<RwLock<HashMap<String, SwarmMember>>>,
    Arc<RwLock<HashMap<String, RunState>>>,
) {
    let swarm_members = Arc::new(RwLock::new(HashMap::from([
        (requester.to_string(), member(requester, requester, "ready")),
        (
            holder.to_string(),
            owned_member(holder, requester, "running", requester),
        ),
        (
            taker.to_string(),
            owned_member(taker, requester, "ready", requester),
        ),
    ])));
    set_repo(&swarm_members, repo).await;
    let swarm_runs = seeded(repo, vec![contested]);
    let sessions = Arc::new(RwLock::new(HashMap::new()));
    (sessions, swarm_members, swarm_runs)
}

/// Live path: an explicit assign_task naming a claimed row moves the claim to the
/// named session, and the dispatch answers with the row it handed over.
#[tokio::test]
async fn assign_task_takes_a_claimed_task_over() {
    let (_env, _runtime) = RuntimeEnvGuard::new();
    let repo = scratch_repo();
    let (requester, holder, taker) = ("coord", "snail", "penguin");
    let mut contested = plan_item("contested", "running", "high", &[]);
    contested.assigned_to = Some(holder.to_string());
    let (sessions, swarm_members, swarm_runs) =
        takeover_fixture(repo.path(), requester, holder, taker, contested).await;
    sessions
        .write()
        .await
        .insert(taker.to_string(), test_agent().await);
    let (client_tx, mut client_rx) = mpsc::unbounded_channel();
    let soft_interrupt_queues = Arc::new(RwLock::new(HashMap::new()));
    let client_connections = Arc::new(RwLock::new(HashMap::new()));
    let event_history = Arc::new(RwLock::new(VecDeque::new()));
    let event_counter = Arc::new(AtomicU64::new(1));
    let (swarm_event_tx, _swarm_event_rx) = broadcast::channel(32);
    let mutation_runtime = SwarmMutationRuntime::default();

    handle_comm_assign_task(
        91,
        requester.to_string(),
        Some(taker.to_string()),
        Some("contested".to_string()),
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
        ServerEvent::CommAssignTaskResponse {
            task_id,
            target_session,
            ..
        } => {
            assert_eq!(task_id, "contested");
            assert_eq!(target_session, taker);
        }
        other => panic!("expected the takeover dispatch, got {other:?}"),
    }

    let rows = rows_in(repo.path());
    let item = rows
        .iter()
        .find(|item| item.id == "contested")
        .expect("contested task exists");
    assert_eq!(
        item.assigned_to.as_deref(),
        Some(taker),
        "the claim moves to the session the caller named"
    );
}

/// Live path: a holder that has gone quiet is taken over the same way. Nothing
/// reads the displaced holder's clock; the row's holder is a fact about the list,
/// and the write replaces it.
#[tokio::test]
async fn assign_task_takes_a_quietly_held_claim_over() {
    let (_env, _runtime) = RuntimeEnvGuard::new();
    let repo = scratch_repo();
    let (requester, holder, taker) = ("coord", "snail", "penguin");
    let mut stalled = plan_item("stalled", "queued", "high", &[]);
    stalled.assigned_to = Some(holder.to_string());
    let (sessions, swarm_members, swarm_runs) =
        takeover_fixture(repo.path(), requester, holder, taker, stalled).await;
    sessions
        .write()
        .await
        .insert(taker.to_string(), test_agent().await);
    let (client_tx, mut client_rx) = mpsc::unbounded_channel();
    let soft_interrupt_queues = Arc::new(RwLock::new(HashMap::new()));
    let client_connections = Arc::new(RwLock::new(HashMap::new()));
    let event_history = Arc::new(RwLock::new(VecDeque::new()));
    let event_counter = Arc::new(AtomicU64::new(1));
    let (swarm_event_tx, _swarm_event_rx) = broadcast::channel(32);
    let mutation_runtime = SwarmMutationRuntime::default();

    handle_comm_assign_task(
        92,
        requester.to_string(),
        Some(taker.to_string()),
        Some("stalled".to_string()),
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
        ServerEvent::CommAssignTaskResponse { target_session, .. } => {
            assert_eq!(target_session, taker);
        }
        other => panic!("expected the takeover dispatch, got {other:?}"),
    }

    let rows = rows_in(repo.path());
    let item = rows
        .iter()
        .find(|item| item.id == "stalled")
        .expect("stalled task exists");
    assert_eq!(
        item.assigned_to.as_deref(),
        Some(taker),
        "the claim moves even when the displaced holder is quiet"
    );
}
