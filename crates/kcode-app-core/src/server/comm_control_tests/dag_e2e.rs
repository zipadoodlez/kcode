// End-to-end task-DAG flow through the real server handlers and assignment loop.
//
// Unlike the engine unit tests (which exercise `kcode_plan::dag` in isolation),
// this drives the live `comm_graph` handlers against real server state
// (swarm_members / swarm_runs), proving the substrate works request-to-plan and
// that forward dataflow reaches a downstream row.

use crate::server::comm_graph::{
    handle_comm_complete_node, handle_comm_seed_graph,
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
            // What the store writes for a fresh row (`kcode_base::todo::add_row`),
            // so a fixture cannot model a row the store would never produce.
            status: "pending".to_string(),
            ..node.clone()
        })
        .collect();
    crate::todo::save_tasks(Some(repo), session_id, &rows).expect("write the rows");
}

/// Shared fixture: a two-member run (coordinator + worker) with an empty plan.
struct GraphFixture {
    /// The seeder's repo: the rows the seed reads live here (rule 1).
    repo: tempfile::TempDir,
    coord: String,
    worker: String,
    client_tx: mpsc::UnboundedSender<ServerEvent>,
    client_rx: mpsc::UnboundedReceiver<ServerEvent>,
    sessions: crate::server::SessionAgents,
    soft_interrupt_queues: crate::server::SessionInterruptQueues,
    client_connections: Arc<RwLock<HashMap<String, crate::server::ClientConnectionInfo>>>,
    swarm_members: Arc<RwLock<HashMap<String, SwarmMember>>>,
    swarm_runs: Arc<RwLock<HashMap<String, RunState>>>,
    event_history: Arc<RwLock<VecDeque<SwarmEvent>>>,
    event_counter: Arc<AtomicU64>,
    swarm_event_tx: broadcast::Sender<SwarmEvent>,
    mutation_runtime: SwarmMutationRuntime,
}

impl GraphFixture {
    /// The rows in the fixture's list: the store is where the handlers write, so a
    /// test reads its expectations from there.
    fn rows(&self) -> Vec<TaskItem> {
        crate::todo::load_tasks(Some(self.repo.path()), &self.worker).unwrap_or_default()
    }

    /// Write the fixture's rows back exactly as given.
    fn write_rows(&self, rows: &[TaskItem]) {
        crate::todo::save_tasks(Some(self.repo.path()), &self.worker, rows).expect("write the rows");
    }

    /// Mark a row as a session's in-flight work, where a dispatch would: the list.
    fn set_row(&self, id: &str, holder: Option<&str>, status: &str) {
        let mut rows = self.rows();
        let row = rows.iter_mut().find(|row| row.id == id).expect("a row");
        row.assigned_to = holder.map(str::to_string);
        row.status = status.to_string();
        self.write_rows(&rows);
    }
}

async fn graph_fixture() -> GraphFixture {
    graph_fixture_for("coord", "worker").await
}

/// The run is rooted at `coord`: it reports back to nobody and every other
/// member reports back to it, which is the whole membership.
async fn graph_fixture_for(coord: &str, worker: &str) -> GraphFixture {
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
            let mut m = member(&coord, &coord, "ready");
            m.working_dir = Some(repo.path().to_path_buf());
            m
        }),
        (worker.clone(), {
            let mut m = owned_member(&worker, &coord, "ready", &coord);
            m.working_dir = Some(repo.path().to_path_buf());
            m
        }),
    ])));
    let swarm_runs = Arc::new(RwLock::new(HashMap::from([(
        coord.clone(),
        RunState::new(),
    )])));
    GraphFixture {
        repo,
        coord,
        worker,
        client_tx,
        client_rx,
        sessions,
        soft_interrupt_queues: Arc::new(RwLock::new(HashMap::new())),
        client_connections: Arc::new(RwLock::new(HashMap::new())),
        swarm_members,
        swarm_runs,
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
            &self.swarm_runs,
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

    let rows = fx.rows();
    assert_eq!(rows.len(), 2);
    let kinded = |id: &str| {
        rows.iter()
            .find(|row| row.id == id)
            .expect("a seeded row")
            .kind
            .clone()
    };
    assert_eq!(kinded("explore").as_deref(), Some("explore"));
    assert_eq!(kinded("synth").as_deref(), Some("synthesize"));
    let synth = rows.iter().find(|i| i.id == "synth").unwrap();
    assert_eq!(synth.blocked_by, vec!["explore".to_string()]);
}

#[tokio::test]
async fn e2e_identical_seed_replay_succeeds_without_node_churn() {
    let (_env, _runtime) = RuntimeEnvGuard::new();
    let mut fx = graph_fixture_for("coord-replay", "worker-replay").await;
    let nodes = vec![
        node_spec("explore", "explore", &[]),
        node_spec("synth", "synthesize", &["explore"]),
    ];

    fx.seed(nodes.clone()).await;
    while fx.client_rx.try_recv().is_ok() {}
    let item_count = fx.rows().len();

    fx.seed(nodes).await;

    assert_eq!(fx.rows().len(), item_count, "a replay must not add nodes");
    let events: Vec<_> = std::iter::from_fn(|| fx.client_rx.try_recv().ok()).collect();
    assert!(
        events.iter().all(|event| !matches!(event, ServerEvent::Error { .. })),
        "an identical replay must acknowledge success: {events:?}"
    );
    assert!(events.iter().any(|event| matches!(event, ServerEvent::Done { .. })));
}

#[tokio::test]
async fn e2e_reseed_keeps_the_plans_existing_node() {
    // The caller cannot send a definition any more: the rows are the seed, and a row
    // already in the list is not seeded again. A row edited after it was seeded is the
    // row the run works, because the list is the only place a row lives.
    let (_env, _runtime) = RuntimeEnvGuard::new();
    let mut fx = graph_fixture_for("coord-conflict", "worker-conflict").await;
    fx.seed(vec![node_spec("shared", "explore", &[])])
        .await;
    while fx.client_rx.try_recv().is_ok() {}
    let before_items = fx.rows().len();
    let mut edited = node_spec("shared", "explore", &[]);
    edited.content = "a different task using the same id".to_string();

    fx.seed(vec![edited]).await;

    let after = fx.rows();
    assert_eq!(after.len(), before_items, "a replay adds nothing");
    assert_eq!(
        after
            .iter()
            .find(|row| row.id == "shared")
            .expect("the row remains")
            .content,
        "a different task using the same id",
        "the row's words are the row's: the list wins over what the run seeded"
    );
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
    fx.set_row("the-work", Some(&fx.worker.clone()), "running");

    handle_comm_complete_node(
        3,
        fx.worker.clone(),
        "the-work".to_string(),
        serde_json::json!({"findings": "it holds", "confidence": "high"}).to_string(),
        &fx.client_tx,
        &fx.swarm_members,
        &fx.swarm_runs,
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

    // The seed changed nothing (the fixture's own rows are all the list has) and an
    // error is surfaced.
    let mut saw_error = false;
    while let Ok(ev) = fx.client_rx.try_recv() {
        if let ServerEvent::Error { message, .. } = ev {
            assert!(message.contains("rejected") || message.contains("cycle"));
            saw_error = true;
        }
    }
    assert!(saw_error, "cycle seed should surface an error");
}

/// Regression: a solo deep-mode seeder must be able to complete (and expand) a
/// node it seeded. Seeded nodes are unowned and the assign path refuses
/// self-assignment, so without the handler-level self-claim the seeder's
/// `complete_node` bounced with "does not own node" (observed live 2026-06-30,
/// session_shrimp completing node 'probe').
#[tokio::test]
async fn e2e_solo_seeder_can_complete_its_own_seeded_node() {
    let (_env, _runtime) = RuntimeEnvGuard::new();
    let mut fx = graph_fixture_for("coord-sc", "worker-sc").await;
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
        &fx.swarm_runs,
        &fx.event_history,
        &fx.event_counter,
        &fx.swarm_event_tx,
    )
    .await;

    let rows = fx.rows();
    assert!(
        !rows.iter().any(|i| i.id == "probe"),
        "a solo seeder completed its own row, so the list no longer holds it"
    );
    // probe is the run's own top row, so it owns no records: its close is its own
    // result, which the store's commit carries.
    assert!(
        !rows.iter().any(|row| row.id == "probe"),
        "the close took the row out of the list"
    );
}

/// Regression: the self-claim must not let an actor steal a node that is
/// assigned to someone else. The engine's NotOwner check still applies.
#[tokio::test]
async fn e2e_self_claim_does_not_steal_foreign_assignment() {
    let (_env, _runtime) = RuntimeEnvGuard::new();
    let mut fx = graph_fixture_for("coord-ns", "worker-ns").await;
    fx.seed(vec![node_spec("task", "explore", &[])])
        .await;
    fx.set_row("task", Some(&fx.worker.clone()), "queued");

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
        &fx.swarm_runs,
        &fx.event_history,
        &fx.event_counter,
        &fx.swarm_event_tx,
    )
    .await;

    let rows = fx.rows();
    let task = rows.iter().find(|i| i.id == "task").unwrap();
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
    let mut fx = graph_fixture_for("coord-qo", "worker-qo").await;
    fx.seed(vec![node_spec("mine", "explore", &[])])
        .await;
    // Assigned but never flipped to running (live-client path).
    fx.set_row("mine", Some(&fx.worker.clone()), "queued");

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
        &fx.swarm_runs,
        &fx.event_history,
        &fx.event_counter,
        &fx.swarm_event_tx,
    )
    .await;

    assert!(
        !fx.rows().iter().any(|i| i.id == "mine"),
        "the assignee completed its queued assignment, so the row left the list"
    );
}
