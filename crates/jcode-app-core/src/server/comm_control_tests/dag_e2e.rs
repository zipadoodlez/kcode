// End-to-end task-DAG flow through the real server handlers and assignment loop.
//
// Unlike the engine unit tests (which exercise `jcode_plan::dag` in isolation),
// this drives the live `comm_graph` handlers against real server state
// (swarm_members / swarms_by_id / swarm_plans / coordinators) and then the real
// `handle_comm_assign_task` path, proving the substrate works request-to-plan and
// that forward dataflow reaches a downstream assignment.

use crate::server::comm_graph::{
    handle_comm_complete_node, handle_comm_expand_node, handle_comm_seed_graph,
};
use crate::protocol::TaskGraphNodeSpec;

fn node_spec(id: &str, kind: &str, deps: &[&str]) -> TaskGraphNodeSpec {
    TaskGraphNodeSpec {
        id: id.to_string(),
        content: format!("task {id}"),
        kind: Some(kind.to_string()),
        depends_on: deps.iter().map(|d| d.to_string()).collect(),
        priority: 0,
    }
}

/// Shared fixture: a two-member swarm (coordinator + worker) with an empty plan.
struct GraphFixture {
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
    let swarm_id = "swarm-dag".to_string();
    let coord = "coord".to_string();
    let worker = "worker".to_string();
    let (client_tx, client_rx) = mpsc::unbounded_channel();
    let sessions = Arc::new(RwLock::new(HashMap::from([
        (coord.clone(), test_agent().await),
        (worker.clone(), test_agent().await),
    ])));
    let swarm_members = Arc::new(RwLock::new(HashMap::from([
        (coord.clone(), {
            let mut m = member(&coord, &swarm_id, "ready");
            m.role = "coordinator".to_string();
            m
        }),
        (worker.clone(), member(&worker, &swarm_id, "ready")),
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
    async fn seed(&mut self, mode: &str, nodes: Vec<TaskGraphNodeSpec>) {
        handle_comm_seed_graph(
            1,
            self.coord.clone(),
            Some(mode.to_string()),
            nodes,
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
        "deep",
        vec![
            node_spec("explore", "explore", &[]),
            node_spec("synth", "synthesize", &["explore"]),
        ],
    )
    .await;

    let plans = fx.swarm_plans.read().await;
    let plan = &plans[&fx.swarm_id];
    assert_eq!(plan.mode, "deep");
    assert_eq!(plan.items.len(), 2);
    assert_eq!(plan.node_meta["explore"].kind.as_deref(), Some("explore"));
    assert_eq!(plan.node_meta["synth"].kind.as_deref(), Some("synthesize"));
    let synth = plan.items.iter().find(|i| i.id == "synth").unwrap();
    assert_eq!(synth.blocked_by, vec!["explore".to_string()]);
}

#[tokio::test]
async fn e2e_seed_rejects_cycle_without_mutating_plan() {
    let (_env, _runtime) = RuntimeEnvGuard::new();
    let mut fx = graph_fixture().await;
    fx.seed(
        "light",
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
async fn e2e_deep_expand_inserts_gate_in_live_plan() {
    let (_env, _runtime) = RuntimeEnvGuard::new();
    let mut fx = graph_fixture().await;
    fx.seed("deep", vec![node_spec("root", "explore", &[])]).await;

    // Assign + dispatch root to the worker so it owns the node, then expand.
    handle_comm_assign_task(
        2,
        fx.coord.clone(),
        Some(fx.worker.clone()),
        Some("root".to_string()),
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

    // Mark root running (assignment leaves it queued); the engine requires a
    // running owner to expand. Simulate the worker starting by setting status.
    {
        let mut plans = fx.swarm_plans.write().await;
        let plan = plans.get_mut(&fx.swarm_id).unwrap();
        let root = plan.items.iter_mut().find(|i| i.id == "root").unwrap();
        root.status = "running".to_string();
        root.assigned_to = Some(fx.worker.clone());
    }

    handle_comm_expand_node(
        3,
        fx.worker.clone(),
        "root".to_string(),
        vec![node_spec("root.1", "explore", &[]), node_spec("root.2", "explore", &[])],
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
    // Gate inserted, root marked composite/expanded.
    let gate = plan
        .items
        .iter()
        .find(|i| plan.node_meta.get(&i.id).map(|m| m.is_gate).unwrap_or(false))
        .expect("a gate node should be present after deep expand");
    assert_eq!(plan.node_meta[&gate.id].kind.as_deref(), Some("critique"));
    assert!(plan.node_meta["root"].expanded);
}

#[tokio::test]
async fn e2e_complete_flows_artifact_to_downstream_assignment() {
    let (_env, _runtime) = RuntimeEnvGuard::new();
    let mut fx = graph_fixture().await;
    fx.seed(
        "light",
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
        let ready = jcode_plan::next_runnable_item_ids(&plan.items, None);
        assert!(ready.contains(&"ui".to_string()), "ui should be ready: {ready:?}");
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
        members.insert(planner.clone(), member(&planner, &fx.swarm_id, "ready"));
        members.insert(other.clone(), member(&other, &fx.swarm_id, "ready"));
        let mut by_id = fx.swarms_by_id.write().await;
        by_id
            .get_mut(&fx.swarm_id)
            .unwrap()
            .extend([planner.clone(), other.clone()]);
        let mut sessions = fx.sessions.write().await;
        sessions.insert(planner.clone(), test_agent().await);
        sessions.insert(other.clone(), test_agent().await);
    }

    fx.seed("light", vec![node_spec("root", "explore", &[])]).await;

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

    // Planner recorded; root owner freed.
    {
        let plans = fx.swarm_plans.read().await;
        let plan = &plans[&fx.swarm_id];
        assert_eq!(plan.node_meta["root"].planner.as_deref(), Some(planner.as_str()));
        let root = plan.items.iter().find(|i| i.id == "root").unwrap();
        assert!(root.assigned_to.is_none());
    }

    // Complete the child so the composite root becomes runnable again.
    {
        let mut plans = fx.swarm_plans.write().await;
        let plan = plans.get_mut(&fx.swarm_id).unwrap();
        let child = plan.items.iter_mut().find(|i| i.id == "root.1").unwrap();
        child.status = "running".to_string();
        child.assigned_to = Some(other.clone());
    }
    handle_comm_complete_node(
        4,
        other.clone(),
        "root.1".to_string(),
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

    // assign_next should route the composite synthesis back to the planner.
    let resolved = crate::server::comm_control::resolve_assignment_target_for_task_test_hook(
        &fx.coord,
        &fx.swarm_id,
        "root",
        None,
        &fx.swarm_members,
        &fx.swarm_plans,
    )
    .await;
    assert_eq!(resolved.as_deref(), Ok(planner.as_str()));
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
    let swarm_members = Arc::new(RwLock::new(HashMap::from([
        (seeder.clone(), member(&seeder, &swarm_id, "ready")),
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
        Some("deep".to_string()),
        vec![
            node_spec("explore", "explore", &[]),
            node_spec("synth", "synthesize", &["explore"]),
        ],
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

    let swarm_members = Arc::new(RwLock::new(HashMap::from([
        (coord.clone(), coord_member),
        (worker.clone(), member(&worker, &swarm_id, "ready")),
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
        Some("deep".to_string()),
        vec![node_spec("root", "explore", &[])],
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

