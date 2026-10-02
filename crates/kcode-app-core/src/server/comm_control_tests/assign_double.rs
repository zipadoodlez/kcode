// Double-assignment guard: a direct assign_task naming a node that is already
// assigned and actively worked must be rejected with an error naming the
// current assignee. Incident: run_plan dispatched a node to a spawned worker,
// and 16 seconds later an explicit `assign_task task_id=<same node>` silently
// re-assigned it to a fresh worker; both edited the same files for ~7 minutes.

/// Pure guard predicate: a claim is a claim, so assigned+in-flight -> conflict,
/// unassigned -> allow, and stale/terminal statuses -> allow.
#[test]
fn active_assignment_conflict_detects_any_claim_on_an_in_flight_item() {
    // Unassigned -> allow, whatever the status.
    for status in ["queued", "running"] {
        assert!(
            super::active_assignment_conflict(status, None, "coord").is_none(),
            "unassigned items are always assignable"
        );
    }

    // Assigned + in-flight -> reject, naming the assignee.
    for status in ["queued", "running"] {
        let conflict = super::active_assignment_conflict(status, Some("snail"), "coord")
            .unwrap_or_else(|| panic!("status '{status}' with a claim must conflict"));
        assert_eq!(conflict.assignee, "snail");
    }

    // The requester's own claim is the row it is handing over, not a conflict: the
    // claim moves from the run to the worker.
    assert!(
        super::active_assignment_conflict("queued", Some("coord"), "coord").is_none(),
        "a run hands over the rows it holds"
    );

    // The rejection names the task, the assignee, and the way out.
    let conflict =
        super::active_assignment_conflict("running", Some("snail"), "coord").expect("claim conflicts");
    let message = super::active_assignment_error("mem-impl-attribution", &conflict);
    assert!(
        message.contains("'mem-impl-attribution'")
            && message.contains("'snail'")
            && message.contains("reassign"),
        "error must name the task, the assignee, and the takeover path: {message}"
    );

    // Stale and terminal statuses -> allow (existing recovery paths).
    for status in ["failed", "stopped", "crashed", "completed", "done"] {
        assert!(
            super::active_assignment_conflict(status, Some("snail"), "coord").is_none(),
            "status '{status}' must not trigger the double-assignment guard"
        );
    }
}

#[allow(clippy::type_complexity)]
async fn double_assign_fixture(
    repo: &std::path::Path,
    swarm_id: &str,
    requester: &str,
    holder: &str,
    intruder: &str,
    contested: TaskItem,
) -> (
    Arc<RwLock<HashMap<String, Arc<Mutex<Agent>>>>>,
    Arc<RwLock<HashMap<String, SwarmMember>>>,
    Arc<RwLock<HashMap<String, HashSet<String>>>>,
    Arc<RwLock<HashMap<String, VersionedPlan>>>,
    Arc<RwLock<HashMap<String, String>>>,
) {
    let swarm_members = Arc::new(RwLock::new(HashMap::from([
        (requester.to_string(), {
            let mut member = member(requester, swarm_id, "ready");
            member.role = "coordinator".to_string();
            member
        }),
        (holder.to_string(), member(holder, swarm_id, "running")),
        (intruder.to_string(), member(intruder, swarm_id, "ready")),
    ])));
    set_repo(&swarm_members, repo).await;
    let swarms_by_id = Arc::new(RwLock::new(HashMap::from([(
        swarm_id.to_string(),
        HashSet::from([
            requester.to_string(),
            holder.to_string(),
            intruder.to_string(),
        ]),
    )])));
    let swarm_plans = Arc::new(RwLock::new(HashMap::from([(
        swarm_id.to_string(),
        VersionedPlan {
            items: vec![contested],
        },
    )])));
    // The plan needs rows behind it: the file is what a dispatch writes to,
    // and the repo root is that file's home.
    write_list(repo, "fixture", &swarm_plans.read().await[swarm_id].items);
    let swarm_coordinators = Arc::new(RwLock::new(HashMap::from([(
        swarm_id.to_string(),
        requester.to_string(),
    )])));
    let sessions = Arc::new(RwLock::new(HashMap::new()));
    (
        sessions,
        swarm_members,
        swarms_by_id,
        swarm_plans,
        swarm_coordinators,
    )
}

/// Live path: explicit assign_task against a claimed node is rejected and the
/// plan keeps the original assignee.
#[tokio::test]
async fn assign_task_rejects_double_assignment_of_claimed_task() {
    let (_env, _runtime) = RuntimeEnvGuard::new();
    let repo = scratch_repo();
    let swarm_id = "swarm-double-assign";
    let (requester, holder, intruder) = ("coord", "snail", "penguin");
    let mut contested = plan_item("contested", "running", "high", &[]);
    contested.assigned_to = Some(holder.to_string());
    let (sessions, swarm_members, swarms_by_id, swarm_plans, swarm_coordinators) =
        double_assign_fixture(repo.path(), swarm_id, requester, holder, intruder, contested).await;
    sessions
        .write()
        .await
        .insert(intruder.to_string(), test_agent().await);
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
        Some(intruder.to_string()),
        Some("contested".to_string()),
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
        ServerEvent::Error { message, .. } => {
            assert!(
                message.contains("already assigned to 'snail'"),
                "error must name the current assignee: {message}"
            );
            assert!(
                message.contains("reassign"),
                "error must point at the explicit takeover path: {message}"
            );
        }
        other => panic!("expected double-assignment rejection, got {other:?}"),
    }

    let plans = swarm_plans.read().await;
    let item = plans[swarm_id]
        .items
        .iter()
        .find(|item| item.id == "contested")
        .expect("contested task exists");
    assert_eq!(
        item.assigned_to.as_deref(),
        Some(holder),
        "rejected double assignment must not steal the task"
    );
    assert_eq!(item.status, "running", "lifecycle status untouched");
}

/// Live path: a claim is not weakened by looking idle. Taking a task from its
/// holder goes through task_control, or the salvage sweep for a dead holder; a
/// plain assign_task is refused even when the holder has been quiet.
#[tokio::test]
async fn assign_task_refuses_to_take_a_quietly_held_claim() {
    let (_env, _runtime) = RuntimeEnvGuard::new();
    let repo = scratch_repo();
    let swarm_id = "swarm-double-assign-quiet";
    let (requester, holder, intruder) = ("coord", "snail", "penguin");
    let mut stalled = plan_item("stalled", "queued", "high", &[]);
    stalled.assigned_to = Some(holder.to_string());
    let (sessions, swarm_members, swarms_by_id, swarm_plans, swarm_coordinators) =
        double_assign_fixture(repo.path(), swarm_id, requester, holder, intruder, stalled).await;
    sessions
        .write()
        .await
        .insert(intruder.to_string(), test_agent().await);
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
        Some(intruder.to_string()),
        Some("stalled".to_string()),
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
        ServerEvent::Error { message, .. } => {
            assert!(
                message.contains("already assigned to 'snail'"),
                "a quietly held claim is still a claim: {message}"
            );
            assert!(
                message.contains("reassign"),
                "the refusal must point at the takeover path: {message}"
            );
        }
        other => panic!("expected the claim to be upheld, got {other:?}"),
    }

    let plans = swarm_plans.read().await;
    let item = plans[swarm_id]
        .items
        .iter()
        .find(|item| item.id == "stalled")
        .expect("stalled task exists");
    assert_eq!(
        item.assigned_to.as_deref(),
        Some(holder),
        "the holder keeps the task"
    );
}

/// Takeover path: task_control reassign moves the task AND tells the displaced
/// worker to stand down (soft interrupt + DM), so it stops editing the same
/// files as its replacement.
#[tokio::test]
async fn task_control_reassign_tells_displaced_worker_to_stand_down() {
    let (_env, _runtime) = RuntimeEnvGuard::new();
    let repo = scratch_repo();
    let swarm_id = "swarm-reassign-stand-down";
    let (requester, holder, intruder) = ("coord", "snail", "penguin");
    let mut contested = plan_item("contested", "running", "high", &[]);
    contested.assigned_to = Some(holder.to_string());
    let (sessions, swarm_members, swarms_by_id, swarm_plans, swarm_coordinators) =
        double_assign_fixture(repo.path(), swarm_id, requester, holder, intruder, contested).await;
    // Capture the displaced worker's server-event stream to observe the DM.
    let (holder_tx, mut holder_rx) = mpsc::unbounded_channel();
    swarm_members
        .write()
        .await
        .get_mut(holder)
        .expect("holder member")
        .event_tx = holder_tx;
    sessions
        .write()
        .await
        .insert(holder.to_string(), test_agent().await);
    sessions
        .write()
        .await
        .insert(intruder.to_string(), test_agent().await);
    let (client_tx, mut client_rx) = mpsc::unbounded_channel();
    let soft_interrupt_queues = Arc::new(RwLock::new(HashMap::new()));
    let client_connections = Arc::new(RwLock::new(HashMap::new()));
    let event_history = Arc::new(RwLock::new(VecDeque::new()));
    let event_counter = Arc::new(AtomicU64::new(1));
    let (swarm_event_tx, _swarm_event_rx) = broadcast::channel(32);
    let mutation_runtime = SwarmMutationRuntime::default();

    handle_comm_task_control(
        93,
        requester.to_string(),
        "reassign".to_string(),
        "contested".to_string(),
        Some(intruder.to_string()),
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
            task_id,
            target_session,
            ..
        } => {
            assert_eq!(task_id, "contested");
            assert_eq!(target_session, intruder);
        }
        other => panic!("reassign should re-dispatch the task, got {other:?}"),
    }

    {
        let plans = swarm_plans.read().await;
        let item = plans[swarm_id]
            .items
            .iter()
            .find(|item| item.id == "contested")
            .expect("contested task exists");
        assert_eq!(item.assigned_to.as_deref(), Some(intruder));
    }

    // The displaced worker's soft-interrupt queue carries the stand-down order.
    let stand_down = {
        let queues = soft_interrupt_queues.read().await;
        queues.get(holder).and_then(|queue| {
            queue.lock().ok().and_then(|pending| {
                pending
                    .iter()
                    .map(|msg| msg.content.clone())
                    .find(|content| content.contains("handed off"))
            })
        })
    };
    let stand_down =
        stand_down.expect("displaced worker must receive a stand-down soft interrupt");
    assert!(
        stand_down.contains("'contested'") && stand_down.contains("'penguin'"),
        "stand-down order must name the task and the new assignee: {stand_down}"
    );
    assert!(
        stand_down.contains("Stop working"),
        "stand-down order must tell the worker to stop: {stand_down}"
    );

    // And the DM notification reaches its event stream.
    let mut saw_dm = false;
    while let Ok(event) = holder_rx.try_recv() {
        if let ServerEvent::Notification { message, .. } = event
            && message.contains("handed off")
        {
            saw_dm = true;
        }
    }
    assert!(saw_dm, "displaced worker must receive a stand-down DM");
}

/// Live path: the handoff verbs refuse while the holder is still reporting
/// activity, so an explicit hand-over cannot clobber work in flight. The
/// staleness sweep used to write this fact down as `running_stale`; it is read
/// from the member's own clock now, so the holder's activity decides.
#[tokio::test]
async fn task_control_reassign_refuses_while_the_holder_is_live() {
    let (_env, _runtime) = RuntimeEnvGuard::new();
    let repo = scratch_repo();
    let swarm_id = "swarm-handoff-live";
    let (requester, holder, intruder) = ("coord", "live-holder", "penguin");
    let mut contested = plan_item("contested", "running", "high", &[]);
    contested.assigned_to = Some(holder.to_string());
    let (sessions, swarm_members, swarms_by_id, swarm_plans, swarm_coordinators) =
        double_assign_fixture(repo.path(), swarm_id, requester, holder, intruder, contested).await;
    // The holder is reporting activity, so the row is in flight.
    crate::session_metrics::record_activity(holder);
    sessions
        .write()
        .await
        .insert(holder.to_string(), test_agent().await);
    sessions
        .write()
        .await
        .insert(intruder.to_string(), test_agent().await);
    let (client_tx, mut client_rx) = mpsc::unbounded_channel();
    let soft_interrupt_queues = Arc::new(RwLock::new(HashMap::new()));
    let client_connections = Arc::new(RwLock::new(HashMap::new()));
    let event_history = Arc::new(RwLock::new(VecDeque::new()));
    let event_counter = Arc::new(AtomicU64::new(1));
    let (swarm_event_tx, _swarm_event_rx) = broadcast::channel(32);
    let mutation_runtime = SwarmMutationRuntime::default();

    handle_comm_task_control(
        94,
        requester.to_string(),
        "reassign".to_string(),
        "contested".to_string(),
        Some(intruder.to_string()),
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
        ServerEvent::Error { message, .. } => {
            assert!(
                message.contains("actively running on 'live-holder'"),
                "a live holder keeps the row: {message}"
            );
        }
        other => panic!("expected the live holder to keep the row, got {other:?}"),
    }

    let plans = swarm_plans.read().await;
    let item = plans[swarm_id]
        .items
        .iter()
        .find(|item| item.id == "contested")
        .expect("contested task exists");
    assert_eq!(
        item.assigned_to.as_deref(),
        Some(holder),
        "the live holder keeps the task"
    );
}
