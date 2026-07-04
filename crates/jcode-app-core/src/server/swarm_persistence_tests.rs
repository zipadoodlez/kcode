use super::*;
use std::time::Instant;

struct EnvGuard {
    _lock: std::sync::MutexGuard<'static, ()>,
    runtime: Option<std::ffi::OsString>,
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        if let Some(value) = self.runtime.take() {
            crate::env::set_var("JCODE_RUNTIME_DIR", value);
        } else {
            crate::env::remove_var("JCODE_RUNTIME_DIR");
        }
    }
}

fn test_env(dir: &tempfile::TempDir) -> EnvGuard {
    let lock = storage::lock_test_env();
    let previous = std::env::var_os("JCODE_RUNTIME_DIR");
    crate::env::set_var("JCODE_RUNTIME_DIR", dir.path());
    EnvGuard {
        _lock: lock,
        runtime: previous,
    }
}

#[test]
fn persisted_swarm_state_round_trips_and_marks_running_stale() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let _env = test_env(&dir);

    let mut plans = HashMap::new();
    plans.insert(
        "swarm-alpha".to_string(),
        VersionedPlan {
            items: vec![crate::plan::PlanItem {
                content: "do thing".to_string(),
                status: "running".to_string(),
                priority: "high".to_string(),
                id: "task-1".to_string(),
                subsystem: None,
                file_scope: Vec::new(),
                blocked_by: Vec::new(),
                assigned_to: Some("session-1".to_string()),
            }],
            version: 3,
            participants: ["session-1".to_string(), "session-2".to_string()]
                .into_iter()
                .collect(),
            task_progress: HashMap::from([(
                "task-1".to_string(),
                SwarmTaskProgress {
                    assigned_session_id: Some("session-1".to_string()),
                    assignment_summary: Some("do thing".to_string()),
                    assigned_at_unix_ms: Some(10),
                    started_at_unix_ms: Some(20),
                    last_heartbeat_unix_ms: Some(30),
                    last_detail: Some("tool start: read".to_string()),
                    last_checkpoint_unix_ms: Some(40),
                    checkpoint_summary: Some("tool done: read".to_string()),
                    completed_at_unix_ms: None,
                    stale_since_unix_ms: None,
                    heartbeat_count: Some(2),
                    checkpoint_count: Some(1),
                    no_artifact_requeues: None,
                    dead_assignee_reclaims: None,
                },
            )]),
            mode: "light".to_string(),
            node_meta: HashMap::new(),
        },
    );
    let coordinators = HashMap::from([("swarm-alpha".to_string(), "session-2".to_string())]);
    let (event_tx, _event_rx) = tokio::sync::mpsc::unbounded_channel();
    let members = vec![SwarmMember {
        session_id: "session-1".to_string(),
        event_tx,
        event_txs: HashMap::new(),
        working_dir: Some(PathBuf::from("/tmp/swarm-alpha")),
        swarm_id: Some("swarm-alpha".to_string()),
        swarm_enabled: true,
        status: "running".to_string(),
        detail: Some("writing tests".to_string()),
        friendly_name: Some("fox".to_string()),
        report_back_to_session_id: Some("session-2".to_string()),
        latest_completion_report: None,
        role: "agent".to_string(),
        joined_at: Instant::now(),
        last_status_change: Instant::now(),
        is_headless: true,
        output_tail: None,
        todo_progress: None,
        todo_items: Vec::new(),
    }];

    persist_swarm_state(
        "swarm-alpha",
        plans.get("swarm-alpha"),
        coordinators.get("swarm-alpha").map(String::as_str),
        &members,
    );
    let loaded = load_runtime_state();

    let loaded_plan = loaded.plans.get("swarm-alpha").expect("loaded plan");
    assert_eq!(loaded_plan.version, 3);
    assert_eq!(loaded_plan.items.len(), 1);
    assert_eq!(loaded_plan.items[0].status, "running_stale");
    let progress = loaded_plan
        .task_progress
        .get("task-1")
        .expect("task progress");
    assert_eq!(progress.assigned_session_id.as_deref(), Some("session-1"));
    assert_eq!(
        progress.checkpoint_summary.as_deref(),
        Some("tool done: read")
    );
    assert!(progress.stale_since_unix_ms.is_some());
    assert_eq!(
        loaded.coordinators.get("swarm-alpha"),
        Some(&"session-2".to_string())
    );
    let recovered_member = loaded.members.get("session-1").expect("recovered member");
    assert_eq!(recovered_member.role, "agent");
    assert_eq!(
        recovered_member.report_back_to_session_id.as_deref(),
        Some("session-2")
    );
    assert_eq!(recovered_member.status, "crashed");
    assert_eq!(
        recovered_member.detail.as_deref(),
        Some("writing tests (recovered after reload while running)")
    );
    assert_eq!(
        loaded.swarms_by_id.get("swarm-alpha"),
        Some(&HashSet::from(["session-1".to_string()]))
    );
}

#[test]
fn remove_swarm_state_deletes_persisted_snapshot() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let _env = test_env(&dir);

    let plans = HashMap::from([(
        "swarm-beta".to_string(),
        VersionedPlan {
            items: Vec::new(),
            version: 1,
            participants: Default::default(),
            task_progress: HashMap::new(),
            mode: "light".to_string(),
            node_meta: HashMap::new(),
        },
    )]);
    persist_swarm_state("swarm-beta", plans.get("swarm-beta"), None, &[]);
    assert!(state_path("swarm-beta").exists());

    remove_swarm_state("swarm-beta");
    assert!(!state_path("swarm-beta").exists());
}

#[test]
fn deep_plan_mode_and_node_meta_round_trip() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let _env = test_env(&dir);

    let mut node_meta = HashMap::new();
    node_meta.insert(
        "root".to_string(),
        crate::plan::NodeMeta {
            kind: Some("explore".to_string()),
            parent: None,
            expanded: true,
            is_gate: false,
            planner: Some("session-1".to_string()),
            artifact_json: Some(r#"{"findings":"found it","confidence":"high"}"#.to_string()),
            origin: Some("seed".to_string()),
        },
    );
    node_meta.insert(
        "root.gate".to_string(),
        crate::plan::NodeMeta {
            kind: Some("critique".to_string()),
            parent: Some("root".to_string()),
            expanded: false,
            is_gate: true,
            planner: None,
            artifact_json: None,
            origin: Some("gate".to_string()),
        },
    );

    let plan = VersionedPlan {
        items: vec![
            crate::plan::PlanItem {
                content: "explore X".to_string(),
                status: "completed".to_string(),
                priority: "high".to_string(),
                id: "root".to_string(),
                subsystem: None,
                file_scope: Vec::new(),
                blocked_by: Vec::new(),
                assigned_to: Some("session-1".to_string()),
            },
            crate::plan::PlanItem {
                content: "gate".to_string(),
                status: "queued".to_string(),
                priority: "medium".to_string(),
                id: "root.gate".to_string(),
                subsystem: None,
                file_scope: Vec::new(),
                blocked_by: vec!["root".to_string()],
                assigned_to: None,
            },
        ],
        version: 7,
        participants: ["session-1".to_string()].into_iter().collect(),
        task_progress: HashMap::new(),
        mode: "deep".to_string(),
        node_meta,
    };

    persist_swarm_state("swarm-deep", Some(&plan), None, &[]);
    let loaded = load_runtime_state();

    let loaded_plan = loaded.plans.get("swarm-deep").expect("loaded plan");
    assert_eq!(loaded_plan.mode, "deep");
    assert_eq!(loaded_plan.version, 7);

    // Edges survive on the item itself.
    let gate_item = loaded_plan
        .items
        .iter()
        .find(|item| item.id == "root.gate")
        .expect("gate item");
    assert_eq!(gate_item.blocked_by, vec!["root".to_string()]);

    // Node kinds, gate flags, expansion, planner, and artifacts survive in node_meta.
    let root_meta = loaded_plan.node_meta.get("root").expect("root meta");
    assert_eq!(root_meta.kind.as_deref(), Some("explore"));
    assert!(root_meta.expanded);
    assert!(!root_meta.is_gate);
    assert_eq!(root_meta.planner.as_deref(), Some("session-1"));
    assert!(
        root_meta
            .artifact_json
            .as_deref()
            .is_some_and(|json| json.contains("found it"))
    );
    let gate_meta = loaded_plan.node_meta.get("root.gate").expect("gate meta");
    assert_eq!(gate_meta.kind.as_deref(), Some("critique"));
    assert!(gate_meta.is_gate);
    assert_eq!(gate_meta.parent.as_deref(), Some("root"));
}

/// The behavioral counterpart of `deep_plan_mode_and_node_meta_round_trip`:
/// after a persist -> load cycle (server restart), the reloaded plan must still
/// drive the deep-mode machinery that reads `node_meta`:
///
/// 1. `low_confidence_completed_ids` still reports completed nodes whose stored
///    artifact self-reported low confidence (gate confidence-debt tracking).
/// 2. `hydrate_assignment` still injects completed upstream artifacts
///    (forward dataflow) into assignment content.
/// 3. Lifting the reloaded plan into the DAG engine still enforces the gate
///    debt rule: a gate cannot rubber-stamp past an unaddressed low-confidence
///    sibling, but passes once it addresses that sibling by id.
#[test]
fn gate_debt_and_artifact_hydration_survive_reload() {
    use crate::plan::dag::{DagError, HandoffArtifact, complete_node, dispatch};

    let dir = tempfile::TempDir::new().expect("tempdir");
    let _env = test_env(&dir);

    let solid_artifact = serde_json::to_string(&HandoffArtifact {
        findings: "solid scope fully mapped".to_string(),
        evidence: vec!["crates/foo/api.rs:12".to_string()],
        confidence: Some("high".to_string()),
        what_i_did_not_check: vec!["nothing, fully covered".to_string()],
        ..HandoffArtifact::default()
    })
    .unwrap();
    let shaky_artifact = serde_json::to_string(&HandoffArtifact {
        findings: "unsure about the edge cases here".to_string(),
        confidence: Some("low".to_string()),
        what_i_did_not_check: vec!["error paths".to_string()],
        ..HandoffArtifact::default()
    })
    .unwrap();

    let item = |id: &str, status: &str, blocked_by: Vec<String>| crate::plan::PlanItem {
        content: format!("work on {id}"),
        status: status.to_string(),
        priority: "medium".to_string(),
        id: id.to_string(),
        subsystem: None,
        file_scope: Vec::new(),
        blocked_by,
        assigned_to: None,
    };
    let meta = |kind: &str, parent: Option<&str>, is_gate: bool, artifact: Option<&str>| {
        crate::plan::NodeMeta {
            kind: Some(kind.to_string()),
            parent: parent.map(str::to_string),
            expanded: false,
            is_gate,
            planner: None,
            artifact_json: artifact.map(str::to_string),
            origin: None,
        }
    };

    let mut plan = VersionedPlan::new();
    plan.mode = "deep".to_string();
    plan.version = 4;
    plan.items = vec![
        {
            let mut root = item("root", "running", Vec::new());
            root.assigned_to = Some("planner-1".to_string());
            root
        },
        item("root.solid", "completed", Vec::new()),
        item("root.shaky", "completed", Vec::new()),
        item(
            "root.gate",
            "queued",
            vec!["root.solid".to_string(), "root.shaky".to_string()],
        ),
    ];
    plan.node_meta = HashMap::from([
        ("root".to_string(), {
            let mut m = meta("explore", None, false, None);
            m.expanded = true;
            m.planner = Some("planner-1".to_string());
            m
        }),
        (
            "root.solid".to_string(),
            meta("explore", Some("root"), false, Some(&solid_artifact)),
        ),
        (
            "root.shaky".to_string(),
            meta("explore", Some("root"), false, Some(&shaky_artifact)),
        ),
        (
            "root.gate".to_string(),
            meta("critique", Some("root"), true, None),
        ),
    ]);

    persist_swarm_state("swarm-debt", Some(&plan), None, &[]);
    let loaded = load_runtime_state();
    let loaded_plan = loaded.plans.get("swarm-debt").expect("loaded plan");

    // 1. Confidence-debt tracking: the reloaded plan still flags the shaky node.
    assert_eq!(
        crate::plan::bridge::low_confidence_completed_ids(loaded_plan),
        vec!["root.shaky".to_string()]
    );

    // 2. Upstream artifact hydration: the gate's assignment content still gets
    // both completed dependency artifacts, including what_i_did_not_check.
    let hydrated = crate::plan::bridge::hydrate_assignment(loaded_plan, "root.gate", "gate prompt");
    assert!(hydrated.contains("gate prompt"));
    assert!(hydrated.contains("Inputs from completed dependencies"));
    assert!(hydrated.contains("solid scope fully mapped"));
    assert!(hydrated.contains("crates/foo/api.rs:12"));
    assert!(hydrated.contains("unsure about the edge cases here"));
    assert!(hydrated.contains("error paths"));

    // 3. The DAG engine, lifted from the reloaded plan, still enforces the gate
    // debt rule end to end.
    let mut graph = crate::plan::bridge::to_task_graph(loaded_plan);
    assert!(dispatch(&mut graph, "root.gate", "gate-worker"));
    let err = complete_node(
        &mut graph,
        "root.gate",
        "gate-worker",
        HandoffArtifact::brief("all good, no gaps"),
    )
    .unwrap_err();
    match &err {
        DagError::UnaddressedLowConfidence { gate, nodes } => {
            assert_eq!(gate, "root.gate");
            assert_eq!(nodes, &vec!["root.shaky".to_string()]);
        }
        other => panic!("expected UnaddressedLowConfidence after reload, got {other:?}"),
    }
    complete_node(
        &mut graph,
        "root.gate",
        "gate-worker",
        HandoffArtifact::brief(
            "root.shaky's low confidence is acceptable: its scope was re-derived and \
             cross-checked; root.solid audited clean",
        ),
    )
    .expect("gate passes once every audited node is addressed by id");
}

#[test]
fn legacy_snapshot_without_mode_defaults_to_light() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let _env = test_env(&dir);

    // Simulate a pre-deep-mode snapshot on disk: no `mode`, no `node_meta`.
    let legacy = serde_json::json!({
        "swarm_id": "swarm-legacy",
        "plan": {
            "items": [{
                "content": "old task",
                "status": "queued",
                "priority": "medium",
                "id": "t1"
            }],
            "version": 2,
            "participants": ["session-1"]
        },
        "updated_at_unix_ms": 1u64
    });
    std::fs::create_dir_all(state_dir()).expect("state dir");
    std::fs::write(
        state_path("swarm-legacy"),
        serde_json::to_vec(&legacy).unwrap(),
    )
    .expect("write legacy snapshot");

    let loaded = load_runtime_state();
    let plan = loaded.plans.get("swarm-legacy").expect("legacy plan");
    assert_eq!(plan.mode, "light");
    assert!(plan.node_meta.is_empty());
    assert_eq!(plan.version, 2);
}

#[test]
fn persisted_swarm_state_without_plan_still_restores_coordinator_and_members() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let _env = test_env(&dir);

    let (event_tx, _event_rx) = tokio::sync::mpsc::unbounded_channel();
    let members = vec![SwarmMember {
        session_id: "coord-1".to_string(),
        event_tx,
        event_txs: HashMap::new(),
        working_dir: Some(PathBuf::from("/tmp/swarm-gamma")),
        swarm_id: Some("swarm-gamma".to_string()),
        swarm_enabled: true,
        status: "ready".to_string(),
        detail: None,
        friendly_name: Some("owl".to_string()),
        report_back_to_session_id: None,
        latest_completion_report: None,
        role: "coordinator".to_string(),
        joined_at: Instant::now(),
        last_status_change: Instant::now(),
        is_headless: false,
        output_tail: None,
        todo_progress: None,
        todo_items: Vec::new(),
    }];

    persist_swarm_state("swarm-gamma", None, Some("coord-1"), &members);

    let loaded = load_runtime_state();
    assert!(!loaded.plans.contains_key("swarm-gamma"));
    assert_eq!(
        loaded.coordinators.get("swarm-gamma"),
        Some(&"coord-1".to_string())
    );
    assert_eq!(
        loaded
            .members
            .get("coord-1")
            .and_then(|member| member.friendly_name.as_deref()),
        Some("owl")
    );
    assert_eq!(
        loaded.swarms_by_id.get("swarm-gamma"),
        Some(&HashSet::from(["coord-1".to_string()]))
    );
}
