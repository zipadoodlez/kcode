// End-to-end task-DAG flow through the real server handlers and assignment loop.
//
// Unlike the engine unit tests (which exercise `kcode_plan::dag` in isolation),
// this drives the live `comm_graph` handlers against real server state
// (swarm_members / swarms_by_id / swarm_plans / coordinators) and then the real
// `handle_comm_assign_task` path, proving the substrate works request-to-plan and
// that forward dataflow reaches a downstream assignment.

use crate::server::comm_graph::{
    handle_comm_complete_node, handle_comm_expand_node, handle_comm_seed_graph,
};

fn node_spec(id: &str, kind: &str, deps: &[&str]) -> TaskItem {
    TaskItem {
        id: id.to_string(),
        content: format!("task {id}"),
        kind: Some(kind.to_string()),
        blocked_by: deps.iter().map(|d| d.to_string()).collect(),
        ..Default::default()
    }
}

/// A repo of its own, so the list a seed reads is this test's and never the
/// machine's (rule 1).
fn scratch_repo() -> tempfile::TempDir {
    let repo = tempfile::TempDir::new().expect("tempdir");
    assert!(
        std::process::Command::new("git")
            .args(["init", "-q"])
            .current_dir(repo.path())
            .status()
            .expect("git init")
            .success(),
        "git init"
    );
    repo
}

/// Write `nodes` as `session_id`'s rows. The seed reads the seeder's list, so a
/// fixture puts the nodes it wants seeded there first; a node spec here is the
/// test's shorthand for a row's id, words, kind and blockers, and the position is
/// the order given.
fn write_rows(repo: &std::path::Path, session_id: &str, nodes: &[TaskItem]) {
    let rows: Vec<TaskItem> = nodes
        .iter()
        .map(|node| TaskItem {
            assigned_to: Some(session_id.to_string()),
            ..node.clone()
        })
        .collect();
    crate::todo::save_tasks(Some(repo), session_id, &rows).expect("write the rows");
}

/// Shared fixture: a two-member swarm (coordinator + worker) with an empty plan.
struct GraphFixture {
    /// The seeder's repo: the rows the seed reads live here (rule 1).
    repo: tempfile::TempDir,
    swarm_id: String,
    coord: String,
    worker: String,
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
}

async fn graph_fixture() -> GraphFixture {
    graph_fixture_named("swarm-dag", "coord", "worker").await
}

async fn graph_fixture_named(swarm_id: &str, coord: &str, worker: &str) -> GraphFixture {
    let swarm_id = swarm_id.to_string();
    let coord = coord.to_string();
    let worker = worker.to_string();
    let repo = scratch_repo();
    let (client_tx, client_rx) = mpsc::unbounded_channel();
    let sessions = Arc::new(RwLock::new(HashMap::from([
        (coord.clone(), test_agent().await),
        (worker.clone(), test_agent().await),
    ])));
    let swarm_members = Arc::new(RwLock::new(HashMap::from([
        (coord.clone(), {
            let mut m = member(&coord, &swarm_id, "ready");
            m.role = "coordinator".to_string();
            m.working_dir = Some(repo.path().to_path_buf());
            m
        }),
        (worker.clone(), {
            let mut m = member(&worker, &swarm_id, "ready");
            m.working_dir = Some(repo.path().to_path_buf());
            m
        }),
    ])));
    let swarms_by_id = Arc::new(RwLock::new(HashMap::from([(
        swarm_id.clone(),
        HashSet::from([coord.clone(), worker.clone()]),
    )])));
    let swarm_plans = Arc::new(RwLock::new(HashMap::from([(
        swarm_id.clone(),
        VersionedPlan::new(),
    )])));
    let swarm_coordinators = Arc::new(RwLock::new(HashMap::from([(
        swarm_id.clone(),
        coord.clone(),
    )])));
    GraphFixture {
        repo,
        swarm_id,
        coord,
        worker,
        client_tx,
        client_rx,
        sessions,
        soft_interrupt_queues: Arc::new(RwLock::new(HashMap::new())),
        client_connections: Arc::new(RwLock::new(HashMap::new())),
        swarm_members,
        swarms_by_id,
        swarm_plans,
        swarm_coordinators,
        event_history: Arc::new(RwLock::new(VecDeque::new())),
        event_counter: Arc::new(AtomicU64::new(1)),
        swarm_event_tx: broadcast::channel(64).0,
        mutation_runtime: SwarmMutationRuntime::default(),
    }
}

impl GraphFixture {
    async fn seed(&mut self, nodes: Vec<TaskItem>) {
        write_rows(self.repo.path(), &self.coord, &nodes);
        handle_comm_seed_graph(
            1,
            self.coord.clone(),
            &self.client_tx,
            &self.swarm_members,
            &self.swarms_by_id,
            &self.swarm_plans,
            &self.swarm_coordinators,
            &self.event_history,
            &self.event_counter,
            &self.swarm_event_tx,
        )
        .await;
    }
}

#[tokio::test]
async fn e2e_seed_creates_plan_with_kinds_and_edges() {
    let (_env, _runtime) = RuntimeEnvGuard::new();
    let mut fx = graph_fixture().await;
    fx.seed(
        vec![
            node_spec("explore", "explore", &[]),
            node_spec("synth", "synthesize", &["explore"]),
        ],
    )
    .await;

    let plans = fx.swarm_plans.read().await;
    let plan = &plans[&fx.swarm_id];
    assert_eq!(plan.items.len(), 2);
    let kinded = |id: &str| {
        plan.items
            .iter()
            .find(|item| item.id == id)
            .expect("a seeded row")
            .kind
            .clone()
    };
    assert_eq!(kinded("explore").as_deref(), Some("explore"));
    assert_eq!(kinded("synth").as_deref(), Some("synthesize"));
    let synth = plan.items.iter().find(|i| i.id == "synth").unwrap();
    assert_eq!(synth.blocked_by, vec!["explore".to_string()]);
}

#[tokio::test]
async fn e2e_identical_seed_replay_succeeds_without_version_or_node_churn() {
    let (_env, _runtime) = RuntimeEnvGuard::new();
    let mut fx = graph_fixture_named("swarm-seed-replay", "coord-replay", "worker-replay").await;
    let nodes = vec![
        node_spec("explore", "explore", &[]),
        node_spec("synth", "synthesize", &["explore"]),
    ];

    fx.seed(nodes.clone()).await;
    while fx.client_rx.try_recv().is_ok() {}
    let (version, item_count) = {
        let plans = fx.swarm_plans.read().await;
        let plan = &plans[&fx.swarm_id];
        (plan.version, plan.items.len())
    };

    fx.seed(nodes).await;

    let plans = fx.swarm_plans.read().await;
    let plan = &plans[&fx.swarm_id];
    assert_eq!(plan.version, version, "a replay must not bump plan version");
    assert_eq!(plan.items.len(), item_count, "a replay must not add nodes");
    drop(plans);
    let events: Vec<_> = std::iter::from_fn(|| fx.client_rx.try_recv().ok()).collect();
    assert!(
        events.iter().all(|event| !matches!(event, ServerEvent::Error { .. })),
        "an identical replay must acknowledge success: {events:?}"
    );
    assert!(events.iter().any(|event| matches!(event, ServerEvent::Done { .. })));
}

#[tokio::test]
async fn e2e_reseed_keeps_the_plans_existing_node() {
    // The caller cannot send a definition any more: the rows are the seed, and a
    // row already in the plan is not seeded again. A row edited after it was seeded
    // therefore leaves the plan's node as the run had it; 0.4 makes the plan a view
    // of the file, where the file wins.
    let (_env, _runtime) = RuntimeEnvGuard::new();
    let mut fx = graph_fixture_named("swarm-seed-conflict", "coord-conflict", "worker-conflict").await;
    fx.seed(vec![node_spec("shared", "explore", &[])])
        .await;
    while fx.client_rx.try_recv().is_ok() {}
    let (before_version, before_items, before_content) = {
        let plans = fx.swarm_plans.read().await;
        let plan = &plans[&fx.swarm_id];
        (
            plan.version,
            plan.items.len(),
            plan.items
                .iter()
                .find(|item| item.id == "shared")
                .expect("seeded node")
                .content
                .clone(),
        )
    };
    let mut edited = node_spec("shared", "explore", &[]);
    edited.content = "a different task using the same id".to_string();

    fx.seed(vec![edited]).await;

    let plans = fx.swarm_plans.read().await;
    let after = &plans[&fx.swarm_id];
    assert_eq!(after.version, before_version, "a replay bumps nothing");
    assert_eq!(after.items.len(), before_items, "a replay adds nothing");
    assert_eq!(
        after
            .items
            .iter()
            .find(|item| item.id == "shared")
            .expect("original node remains")
            .content,
        before_content,
        "the plan's node keeps the words the run seeded it with"
    );
    drop(plans);
    let events: Vec<_> = std::iter::from_fn(|| fx.client_rx.try_recv().ok()).collect();
    assert!(
        events
            .iter()
            .all(|event| !matches!(event, ServerEvent::Error { .. })),
        "a row already in the plan is a no-op, not an error: {events:?}"
    );
    assert!(events.iter().any(|event| matches!(event, ServerEvent::Done { .. })));
}

/// The engine's close is the row's close: a completed node's row leaves the list
/// and its record lands on the row that owns the work (S5b). The fixture's seed made
/// the first row the run's anchor, so the finished row's record lands there, which is
/// what a resumed run and a gate read.
#[tokio::test]
async fn e2e_complete_closes_the_row_and_keeps_its_record() {
    let (_env, _runtime) = RuntimeEnvGuard::new();
    let mut fx = graph_fixture().await;
    fx.seed(
        vec![
            node_spec("the-run", "synthesize", &[]),
            node_spec("the-work", "implement", &[]),
        ],
    )
    .await;
    {
        let mut plans = fx.swarm_plans.write().await;
        let plan = plans.get_mut(&fx.swarm_id).expect("plan");
        let work = plan
            .items
            .iter_mut()
            .find(|item| item.id == "the-work")
            .expect("the node");
        work.status = "running".to_string();
        work.assigned_to = Some(fx.worker.clone());
    }

    handle_comm_complete_node(
        3,
        fx.worker.clone(),
        "the-work".to_string(),
        serde_json::json!({"findings": "it holds", "confidence": "high"}).to_string(),
        &fx.client_tx,
        &fx.swarm_members,
        &fx.swarms_by_id,
        &fx.swarm_plans,
        &fx.swarm_coordinators,
        &fx.event_history,
        &fx.event_counter,
        &fx.swarm_event_tx,
    )
    .await;
    while fx.client_rx.try_recv().is_ok() {}

    let rows = crate::todo::load_tasks(Some(fx.repo.path()), &fx.coord).expect("read the list");
    assert!(
        !rows.iter().any(|row| row.id == "the-work"),
        "the completed node's row is gone: {:?}",
        rows.iter().map(|row| row.id.as_str()).collect::<Vec<_>>()
    );
    let anchor = rows
        .iter()
        .find(|row| row.id == "the-run")
        .expect("the run's anchor row");
    assert_eq!(anchor.records[0]["id"], "the-work");
    assert_eq!(anchor.records[0]["result"], "it holds");
    assert_eq!(anchor.records[0]["artifact"]["confidence"], "high");
}

#[tokio::test]
async fn e2e_seed_rejects_cycle_without_mutating_plan() {
    let (_env, _runtime) = RuntimeEnvGuard::new();
    let mut fx = graph_fixture().await;
    fx.seed(
        vec![
            node_spec("a", "explore", &["b"]),
            node_spec("b", "explore", &["a"]),
        ],
    )
    .await;

    // Plan stays empty and an error is surfaced.
    let plans = fx.swarm_plans.read().await;
    assert!(plans[&fx.swarm_id].items.is_empty());
    drop(plans);
    let mut saw_error = false;
    while let Ok(ev) = fx.client_rx.try_recv() {
        if let ServerEvent::Error { message, .. } = ev {
            assert!(message.contains("rejected") || message.contains("cycle"));
            saw_error = true;
        }
    }
    assert!(saw_error, "cycle seed should surface an error");
}

#[tokio::test]
async fn e2e_complete_flows_artifact_to_downstream_assignment() {
    let (_env, _runtime) = RuntimeEnvGuard::new();
    let mut fx = graph_fixture().await;
    fx.seed(
        vec![
            node_spec("api", "implement", &[]),
            node_spec("ui", "implement", &["api"]),
        ],
    )
    .await;

    // Assign "api" to the worker, mark running, then complete with an artifact.
    handle_comm_assign_task(
        2,
        fx.coord.clone(),
        Some(fx.worker.clone()),
        Some("api".to_string()),
        None,
        &fx.client_tx,
        &fx.sessions,
        &fx.soft_interrupt_queues,
        &fx.client_connections,
        &fx.swarm_members,
        &fx.swarms_by_id,
        &fx.swarm_plans,
        &fx.swarm_coordinators,
        &fx.event_history,
        &fx.event_counter,
        &fx.swarm_event_tx,
        &fx.mutation_runtime,
    )
    .await;
    {
        let mut plans = fx.swarm_plans.write().await;
        let plan = plans.get_mut(&fx.swarm_id).unwrap();
        let api = plan.items.iter_mut().find(|i| i.id == "api").unwrap();
        api.status = "running".to_string();
        api.assigned_to = Some(fx.worker.clone());
    }

    let artifact = serde_json::json!({
        "findings": "API built in crates/foo/api.rs with types Req/Resp",
        "evidence": ["crates/foo/api.rs:1"],
    })
    .to_string();
    handle_comm_complete_node(
        4,
        fx.worker.clone(),
        "api".to_string(),
        artifact,
        &fx.client_tx,
        &fx.swarm_members,
        &fx.swarms_by_id,
        &fx.swarm_plans,
        &fx.swarm_coordinators,
        &fx.event_history,
        &fx.event_counter,
        &fx.swarm_event_tx,
    )
    .await;

    // api is now completed; ui should be runnable.
    {
        let plans = fx.swarm_plans.read().await;
        let plan = &plans[&fx.swarm_id];
        let api = plan.items.iter().find(|i| i.id == "api").unwrap();
        assert_eq!(api.status, "completed");
        assert!(plan.node_meta["api"].artifact_json.is_some());
        let ready = kcode_plan::next_runnable_item_ids(&plan.items, None);
        assert!(
            ready.contains(&"ui".to_string()),
            "ui should be ready: {ready:?}"
        );
    }

    // Assign "ui": its prompt must be hydrated with api's artifact.
    handle_comm_assign_task(
        5,
        fx.coord.clone(),
        Some(fx.worker.clone()),
        Some("ui".to_string()),
        None,
        &fx.client_tx,
        &fx.sessions,
        &fx.soft_interrupt_queues,
        &fx.client_connections,
        &fx.swarm_members,
        &fx.swarms_by_id,
        &fx.swarm_plans,
        &fx.swarm_coordinators,
        &fx.event_history,
        &fx.event_counter,
        &fx.swarm_event_tx,
        &fx.mutation_runtime,
    )
    .await;

    // The assignment summary stored in task_progress should reflect hydration.
    let plans = fx.swarm_plans.read().await;
    let plan = &plans[&fx.swarm_id];
    let ui = plan.items.iter().find(|i| i.id == "ui").unwrap();
    assert_eq!(ui.assigned_to.as_deref(), Some(fx.worker.as_str()));
}

#[tokio::test]
async fn e2e_composite_rewake_prefers_planner_via_assign_next() {
    let (_env, _runtime) = RuntimeEnvGuard::new();
    let mut fx = graph_fixture().await;
    // Two workers so auto-assignment has a choice; the planner should still win
    // the composite synthesis re-wake.
    let planner = "planner".to_string();
    let other = "other".to_string();
    {
        let mut members = fx.swarm_members.write().await;
        let mut planner_member = owned_member(&planner, &fx.swarm_id, "ready", &fx.coord);
        planner_member.working_dir = Some(fx.repo.path().to_path_buf());
        members.insert(planner.clone(), planner_member);
        let mut other_member = owned_member(&other, &fx.swarm_id, "ready", &fx.coord);
        other_member.working_dir = Some(fx.repo.path().to_path_buf());
        members.insert(other.clone(), other_member);
        let mut by_id = fx.swarms_by_id.write().await;
        by_id
            .get_mut(&fx.swarm_id)
            .unwrap()
            .extend([planner.clone(), other.clone()]);
        let mut sessions = fx.sessions.write().await;
        sessions.insert(planner.clone(), test_agent().await);
        sessions.insert(other.clone(), test_agent().await);
    }

    fx.seed(vec![node_spec("root", "explore", &[])])
        .await;

    // planner owns root and decomposes it into one child.
    {
        let mut plans = fx.swarm_plans.write().await;
        let plan = plans.get_mut(&fx.swarm_id).unwrap();
        let root = plan.items.iter_mut().find(|i| i.id == "root").unwrap();
        root.status = "running".to_string();
        root.assigned_to = Some(planner.clone());
    }
    handle_comm_expand_node(
        3,
        planner.clone(),
        "root".to_string(),
        vec![node_spec("root.1", "explore", &[])],
        &fx.client_tx,
        &fx.swarm_members,
        &fx.swarms_by_id,
        &fx.swarm_plans,
        &fx.swarm_coordinators,
        &fx.event_history,
        &fx.event_counter,
        &fx.swarm_event_tx,
    )
    .await;

    // The holder stays: it is the record of who integrates the children, so the
    // re-queued composite goes back to it with no side table.
    {
        let plans = fx.swarm_plans.read().await;
        let plan = &plans[&fx.swarm_id];
        let root = plan.items.iter().find(|i| i.id == "root").unwrap();
        assert_eq!(root.assigned_to.as_deref(), Some(planner.as_str()));
    }

    // Complete the child so the composite root becomes runnable again.
    // The store owned the child's id, so read it back rather than assuming one.
    let child_id = {
        let plans = fx.swarm_plans.read().await;
        let plan = &plans[&fx.swarm_id];
        plan.items
            .iter()
            .find(|i| i.parent.as_deref() == Some("root"))
            .expect("the decomposition wrote a child row")
            .id
            .clone()
    };
    {
        let mut plans = fx.swarm_plans.write().await;
        let plan = plans.get_mut(&fx.swarm_id).unwrap();
        let child = plan.items.iter_mut().find(|i| i.id == child_id).unwrap();
        child.status = "running".to_string();
        child.assigned_to = Some(other.clone());
    }
    handle_comm_complete_node(
        4,
        other.clone(),
        child_id,
        serde_json::json!({"findings": "child done"}).to_string(),
        &fx.client_tx,
        &fx.swarm_members,
        &fx.swarms_by_id,
        &fx.swarm_plans,
        &fx.swarm_coordinators,
        &fx.event_history,
        &fx.event_counter,
        &fx.swarm_event_tx,
    )
    .await;

    // The composite is runnable again and still names the planner, so the next
    // dispatch hands the synthesis back to it rather than choosing a worker.
    let mut client_rx = fx.client_rx;
    let global_session_id = Arc::new(RwLock::new(String::new()));
    let provider: Arc<dyn Provider> = Arc::new(TestProvider);
    let mcp_pool = Arc::new(crate::mcp::SharedMcpPool::from_default_config());
    handle_comm_assign_next(
        4,
        fx.coord.clone(),
        None,
        None,
        Some(true),
        Some(true),
        None,
        None,
        None,
        &fx.client_tx,
        &fx.sessions,
        &global_session_id,
        &provider,
        &fx.soft_interrupt_queues,
        &fx.client_connections,
        &fx.swarm_members,
        &fx.swarms_by_id,
        &fx.swarm_plans,
        &fx.swarm_coordinators,
        &fx.event_history,
        &fx.event_counter,
        &fx.swarm_event_tx,
        &mcp_pool,
        &fx.mutation_runtime,
    )
    .await;
    // The seed, expand and complete calls above left their acks in this channel.
    while let Some(event) = client_rx.recv().await {
        match event {
            ServerEvent::CommAssignTaskResponse {
                task_id,
                target_session,
                ..
            } => {
                assert_eq!(task_id, "root");
                assert_eq!(target_session, planner);
                return;
            }
            ServerEvent::Error { message, .. } => panic!("assign_next failed: {message}"),
            _ => {}
        }
    }
    panic!("assign_next sent no dispatch response");
}

/// A solo deep-mode agent (no coordinator registered) seeds a graph. It must be
/// elected coordinator so it can then drive the coordinator-gated assign path it
/// just created work for.
#[tokio::test]
async fn e2e_solo_seeder_is_elected_coordinator_and_can_assign() {
    let (_env, _runtime) = RuntimeEnvGuard::new();
    let swarm_id = "swarm-solo".to_string();
    let seeder = "seeder".to_string();
    let worker = "worker".to_string();
    let (client_tx, _client_rx) = mpsc::unbounded_channel();
    let sessions: crate::server::SessionAgents = Arc::new(RwLock::new(HashMap::from([
        (seeder.clone(), test_agent().await),
        (worker.clone(), test_agent().await),
    ])));
    let repo = scratch_repo();
    write_rows(
        repo.path(),
        &seeder,
        &[
            node_spec("explore", "explore", &[]),
            node_spec("synth", "synthesize", &["explore"]),
        ],
    );
    let swarm_members = Arc::new(RwLock::new(HashMap::from([
        (seeder.clone(), {
            let mut m = member(&seeder, &swarm_id, "ready");
            m.working_dir = Some(repo.path().to_path_buf());
            m
        }),
        (worker.clone(), member(&worker, &swarm_id, "ready")),
    ])));
    let swarms_by_id = Arc::new(RwLock::new(HashMap::from([(
        swarm_id.clone(),
        HashSet::from([seeder.clone(), worker.clone()]),
    )])));
    let swarm_plans = Arc::new(RwLock::new(HashMap::from([(
        swarm_id.clone(),
        VersionedPlan::new(),
    )])));
    // No coordinator registered: this is the deep-mode solo-agent starting state.
    let swarm_coordinators: Arc<RwLock<HashMap<String, String>>> =
        Arc::new(RwLock::new(HashMap::new()));
    let event_history = Arc::new(RwLock::new(VecDeque::new()));
    let event_counter = Arc::new(AtomicU64::new(1));
    let swarm_event_tx = broadcast::channel(64).0;
    let mutation_runtime = SwarmMutationRuntime::default();
    let soft_interrupt_queues: crate::server::SessionInterruptQueues =
        Arc::new(RwLock::new(HashMap::new()));
    let client_connections: Arc<RwLock<HashMap<String, crate::server::ClientConnectionInfo>>> =
        Arc::new(RwLock::new(HashMap::new()));

    handle_comm_seed_graph(
        1,
        seeder.clone(),
        &client_tx,
        &swarm_members,
        &swarms_by_id,
        &swarm_plans,
        &swarm_coordinators,
        &event_history,
        &event_counter,
        &swarm_event_tx,
    )
    .await;

    // The seeder is now the coordinator of its swarm.
    assert_eq!(
        swarm_coordinators.read().await.get(&swarm_id).cloned(),
        Some(seeder.clone()),
        "solo seeder should be elected coordinator"
    );
    assert_eq!(
        swarm_members.read().await.get(&seeder).unwrap().role,
        "coordinator"
    );

    // And it can now drive the graph: assign the ready node to the worker.
    handle_comm_assign_task(
        2,
        seeder.clone(),
        Some(worker.clone()),
        Some("explore".to_string()),
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

    let plans = swarm_plans.read().await;
    let explore = plans[&swarm_id]
        .items
        .iter()
        .find(|i| i.id == "explore")
        .unwrap();
    assert_eq!(
        explore.assigned_to.as_deref(),
        Some(worker.as_str()),
        "elected coordinator should be able to assign the seeded task"
    );
}

/// A live, non-headless coordinator must not be displaced by a different member
/// that happens to seed a graph.
#[tokio::test]
async fn e2e_seed_does_not_displace_live_coordinator() {
    let (_env, _runtime) = RuntimeEnvGuard::new();
    let swarm_id = "swarm-live-coord".to_string();
    let coord = "coord".to_string();
    let worker = "worker".to_string();
    let (client_tx, _client_rx) = mpsc::unbounded_channel();

    // Build the coordinator with a *retained* receiver so its event channel is
    // genuinely open (the shared `member()` helper drops the receiver, which would
    // make the channel look closed and the coordinator look dead).
    let (coord_tx, _coord_rx) = mpsc::unbounded_channel();
    let mut coord_member = member(&coord, &swarm_id, "ready");
    coord_member.event_tx = coord_tx;
    coord_member.role = "coordinator".to_string();

    let repo = scratch_repo();
    write_rows(repo.path(), &worker, &[node_spec("root", "explore", &[])]);
    let swarm_members = Arc::new(RwLock::new(HashMap::from([
        (coord.clone(), coord_member),
        (worker.clone(), {
            let mut m = member(&worker, &swarm_id, "ready");
            m.working_dir = Some(repo.path().to_path_buf());
            m
        }),
    ])));
    let swarms_by_id = Arc::new(RwLock::new(HashMap::from([(
        swarm_id.clone(),
        HashSet::from([coord.clone(), worker.clone()]),
    )])));
    let swarm_plans = Arc::new(RwLock::new(HashMap::from([(
        swarm_id.clone(),
        VersionedPlan::new(),
    )])));
    let swarm_coordinators = Arc::new(RwLock::new(HashMap::from([(
        swarm_id.clone(),
        coord.clone(),
    )])));
    let event_history = Arc::new(RwLock::new(VecDeque::new()));
    let event_counter = Arc::new(AtomicU64::new(1));
    let swarm_event_tx = broadcast::channel(64).0;

    // The non-coordinator worker seeds the graph.
    handle_comm_seed_graph(
        1,
        worker.clone(),
        &client_tx,
        &swarm_members,
        &swarms_by_id,
        &swarm_plans,
        &swarm_coordinators,
        &event_history,
        &event_counter,
        &swarm_event_tx,
    )
    .await;

    assert_eq!(
        swarm_coordinators.read().await.get(&swarm_id).cloned(),
        Some(coord.clone()),
        "a live coordinator must not be displaced by a seeding worker"
    );
    assert_eq!(
        swarm_members.read().await.get(&worker).unwrap().role,
        "agent",
        "the seeding worker should remain an agent"
    );
}

/// Regression: a solo deep-mode seeder must be able to complete (and expand) a
/// node it seeded. Seeded nodes are unowned and the assign path refuses
/// self-assignment, so without the handler-level self-claim the seeder's
/// `complete_node` bounced with "does not own node" (observed live 2026-06-30,
/// session_shrimp completing node 'probe').
#[tokio::test]
async fn e2e_solo_seeder_can_complete_its_own_seeded_node() {
    let (_env, _runtime) = RuntimeEnvGuard::new();
    let mut fx = graph_fixture_named("swarm-self-claim", "coord-sc", "worker-sc").await;
    fx.seed(vec![node_spec("probe", "explore", &[])])
        .await;

    // The seeder completes its own seeded node directly: the handler must
    // auto-claim the unowned queued node instead of rejecting with NotOwner.
    handle_comm_complete_node(
        2,
        fx.coord.clone(),
        "probe".to_string(),
        serde_json::json!({
            "findings": "probe complete",
            "confidence": "high",
            "what_i_did_not_check": ["nothing; probe only"],
        })
        .to_string(),
        &fx.client_tx,
        &fx.swarm_members,
        &fx.swarms_by_id,
        &fx.swarm_plans,
        &fx.swarm_coordinators,
        &fx.event_history,
        &fx.event_counter,
        &fx.swarm_event_tx,
    )
    .await;

    let plans = fx.swarm_plans.read().await;
    let plan = &plans[&fx.swarm_id];
    let probe = plan.items.iter().find(|i| i.id == "probe").unwrap();
    assert_eq!(
        probe.status, "completed",
        "solo seeder must be able to complete its own seeded node"
    );
    assert!(
        plan.node_meta["probe"].artifact_json.is_some(),
        "artifact must be recorded"
    );
}

/// Regression: the self-claim must not let an actor steal a node that is
/// assigned to someone else. The engine's NotOwner check still applies.
#[tokio::test]
async fn e2e_self_claim_does_not_steal_foreign_assignment() {
    let (_env, _runtime) = RuntimeEnvGuard::new();
    let mut fx = graph_fixture_named("swarm-no-steal", "coord-ns", "worker-ns").await;
    fx.seed(vec![node_spec("task", "explore", &[])])
        .await;
    {
        let mut plans = fx.swarm_plans.write().await;
        let plan = plans.get_mut(&fx.swarm_id).unwrap();
        let item = plan.items.iter_mut().find(|i| i.id == "task").unwrap();
        item.assigned_to = Some(fx.worker.clone());
        item.status = "queued".to_string();
    }

    // The coordinator (not the assignee) tries to complete it -> rejected.
    handle_comm_complete_node(
        2,
        fx.coord.clone(),
        "task".to_string(),
        serde_json::json!({
            "findings": "hijack",
            "confidence": "high",
            "what_i_did_not_check": ["everything"],
        })
        .to_string(),
        &fx.client_tx,
        &fx.swarm_members,
        &fx.swarms_by_id,
        &fx.swarm_plans,
        &fx.swarm_coordinators,
        &fx.event_history,
        &fx.event_counter,
        &fx.swarm_event_tx,
    )
    .await;

    let plans = fx.swarm_plans.read().await;
    let plan = &plans[&fx.swarm_id];
    let task = plan.items.iter().find(|i| i.id == "task").unwrap();
    assert_eq!(
        task.status, "queued",
        "a foreign actor must not complete someone else's assignment"
    );
    assert_eq!(task.assigned_to.as_deref(), Some(fx.worker.as_str()));
}

/// Regression: an assignee whose node was left `queued` (client-attached worker
/// path skips the server-side flip to running) must still be able to
/// complete/expand its own assignment.
#[tokio::test]
async fn e2e_assignee_can_complete_queued_assignment() {
    let (_env, _runtime) = RuntimeEnvGuard::new();
    let mut fx = graph_fixture_named("swarm-queued-own", "coord-qo", "worker-qo").await;
    fx.seed(vec![node_spec("mine", "explore", &[])])
        .await;
    {
        let mut plans = fx.swarm_plans.write().await;
        let plan = plans.get_mut(&fx.swarm_id).unwrap();
        let item = plan.items.iter_mut().find(|i| i.id == "mine").unwrap();
        // Assigned but never flipped to running (live-client path).
        item.assigned_to = Some(fx.worker.clone());
        item.status = "queued".to_string();
    }

    handle_comm_complete_node(
        2,
        fx.worker.clone(),
        "mine".to_string(),
        serde_json::json!({
            "findings": "did the work",
            "confidence": "high",
            "what_i_did_not_check": ["nothing"],
        })
        .to_string(),
        &fx.client_tx,
        &fx.swarm_members,
        &fx.swarms_by_id,
        &fx.swarm_plans,
        &fx.swarm_coordinators,
        &fx.event_history,
        &fx.event_counter,
        &fx.swarm_event_tx,
    )
    .await;

    let plans = fx.swarm_plans.read().await;
    let plan = &plans[&fx.swarm_id];
    let mine = plan.items.iter().find(|i| i.id == "mine").unwrap();
    assert_eq!(
        mine.status, "completed",
        "the assignee must be able to complete its queued assignment"
    );
}
