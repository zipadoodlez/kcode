//! Invariant tests for the task-DAG engine, including a full simulator run that
//! reproduces the worked example in `docs/SWARM_TASK_GRAPH.md` section 9.

use super::sim::{self, WorkerAction};
use super::*;

fn spec(id: &str, kind: NodeKind) -> NodeSpec {
    NodeSpec::new(id, format!("task {id}"), kind)
}

fn dag(mode: Mode, specs: Vec<NodeSpec>) -> TaskGraph {
    let mut g = TaskGraph::new(mode);
    seed(&mut g, specs).expect("seed should succeed");
    g
}

// ----- seed validation -----

#[test]
fn seed_rejects_duplicate_ids() {
    let mut g = TaskGraph::new(Mode::Light);
    let err = seed(
        &mut g,
        vec![spec("a", NodeKind::Explore), spec("a", NodeKind::Explore)],
    )
    .unwrap_err();
    assert_eq!(err, DagError::DuplicateNode("a".into()));
}

#[test]
fn seed_rejects_unknown_dependency() {
    let mut g = TaskGraph::new(Mode::Light);
    let err = seed(
        &mut g,
        vec![spec("a", NodeKind::Explore).depends_on(["ghost"])],
    )
    .unwrap_err();
    assert_eq!(
        err,
        DagError::UnknownDependency {
            node: "a".into(),
            dependency: "ghost".into()
        }
    );
}

#[test]
fn seed_rejects_cycle() {
    let mut g = TaskGraph::new(Mode::Light);
    let err = seed(
        &mut g,
        vec![
            spec("a", NodeKind::Explore).depends_on(["b"]),
            spec("b", NodeKind::Explore).depends_on(["a"]),
        ],
    )
    .unwrap_err();
    assert!(matches!(err, DagError::WouldCreateCycle(_)));
}

// ----- scheduling / ready set -----

#[test]
fn ready_set_respects_dependencies_and_priority() {
    let g = dag(
        Mode::Light,
        vec![
            spec("a", NodeKind::Explore).priority(1),
            spec("b", NodeKind::Explore).priority(0),
            spec("c", NodeKind::Explore).depends_on(["a"]),
        ],
    );
    // a and b are ready (no deps); c is blocked on a. b sorts first (priority 0).
    let ready: Vec<&str> = ready_nodes(&g).iter().map(|n| n.id.as_str()).collect();
    assert_eq!(ready, vec!["b", "a"]);
}

#[test]
fn dispatch_assigns_owner_and_blocks_dependents() {
    let mut g = dag(
        Mode::Light,
        vec![
            spec("a", NodeKind::Explore),
            spec("b", NodeKind::Implement).depends_on(["a"]),
        ],
    );
    assert!(dispatch(&mut g, "a", "w0"));
    assert_eq!(g.get("a").unwrap().owner.as_deref(), Some("w0"));
    assert_eq!(g.get("a").unwrap().status, NodeStatus::Running);
    // b is still blocked: a is not Done.
    assert!(ready_nodes(&g).iter().all(|n| n.id != "b"));
    // cannot dispatch b yet
    assert!(!dispatch(&mut g, "b", "w1"));
}

// ----- ownership enforcement -----

#[test]
fn complete_rejects_non_owner() {
    let mut g = dag(Mode::Light, vec![spec("a", NodeKind::Explore)]);
    dispatch(&mut g, "a", "w0");
    let err = complete_node(&mut g, "a", "intruder", HandoffArtifact::brief("x")).unwrap_err();
    assert_eq!(
        err,
        DagError::NotOwner {
            node: "a".into(),
            actor: "intruder".into()
        }
    );
}

#[test]
fn expand_rejects_non_owner() {
    let mut g = dag(Mode::Light, vec![spec("a", NodeKind::Explore)]);
    dispatch(&mut g, "a", "w0");
    let err = expand_node(
        &mut g,
        "a",
        "intruder",
        vec![spec("a.1", NodeKind::Explore)],
    )
    .unwrap_err();
    assert!(matches!(err, DagError::NotOwner { .. }));
}

// ----- dataflow on edges -----

#[test]
fn assembled_input_includes_upstream_artifacts() {
    let mut g = dag(
        Mode::Light,
        vec![
            spec("a", NodeKind::Explore),
            spec("b", NodeKind::Implement).depends_on(["a"]),
        ],
    );
    dispatch(&mut g, "a", "w0");
    let mut artifact = HandoffArtifact::brief("API lives in foo.rs");
    artifact.evidence = vec!["crates/foo/api.rs:12".into()];
    complete_node(&mut g, "a", "w0", artifact).unwrap();

    let input = assemble_input(&g, "b");
    assert!(input.contains("task b"));
    assert!(input.contains("API lives in foo.rs"));
    assert!(input.contains("crates/foo/api.rs:12"));
}

// ----- deep-mode artifact validation -----

#[test]
fn deep_mode_rejects_thin_artifact() {
    let mut g = dag(Mode::Deep, vec![spec("a", NodeKind::Explore)]);
    dispatch(&mut g, "a", "w0");
    // empty what_i_did_not_check is rejected
    let err = complete_node(&mut g, "a", "w0", HandoffArtifact::brief("found stuff")).unwrap_err();
    assert!(matches!(err, DagError::ThinArtifact { .. }));

    // a complete artifact passes
    assert!(complete_node(&mut g, "a", "w0", sim::deep_artifact("found stuff")).is_ok());
}

#[test]
fn light_mode_accepts_thin_artifact() {
    let mut g = dag(Mode::Light, vec![spec("a", NodeKind::Explore)]);
    dispatch(&mut g, "a", "w0");
    assert!(complete_node(&mut g, "a", "w0", HandoffArtifact::brief("ok")).is_ok());
}

// ----- composite expansion + gate insertion -----

#[test]
fn deep_expand_inserts_gate_between_children_and_synthesis() {
    let mut g = dag(Mode::Deep, vec![spec("root", NodeKind::Explore)]);
    dispatch(&mut g, "root", "w0");
    let outcome = expand_node(
        &mut g,
        "root",
        "w0",
        vec![
            spec("root.1", NodeKind::Explore),
            spec("root.2", NodeKind::Explore),
        ],
    )
    .unwrap();

    // gate inserted, depends on both children
    let gate_id = outcome.gate_id.expect("deep mode inserts a gate");
    let gate = g.get(&gate_id).unwrap();
    assert!(gate.is_gate);
    assert_eq!(gate.kind, NodeKind::Critique);
    let mut gate_deps = gate.depends_on.clone();
    gate_deps.sort();
    assert_eq!(gate_deps, vec!["root.1", "root.2"]);

    // composite root now depends on the gate AND retains its child edges (so the
    // synthesis re-wake is hydrated with the children's artifacts) and is marked
    // expanded + re-queued.
    let root = g.get("root").unwrap();
    assert!(root.expanded);
    assert_eq!(root.status, NodeStatus::Queued);
    assert!(root.depends_on.contains(&gate_id));
    assert!(root.depends_on.contains(&"root.1".to_string()));
    assert!(root.depends_on.contains(&"root.2".to_string()));

    // root is NOT ready until children + gate complete.
    assert!(ready_nodes(&g).iter().all(|n| n.id != "root"));
}

#[test]
fn light_expand_inserts_no_gate() {
    let mut g = dag(Mode::Light, vec![spec("root", NodeKind::Explore)]);
    dispatch(&mut g, "root", "w0");
    let outcome = expand_node(
        &mut g,
        "root",
        "w0",
        vec![spec("root.1", NodeKind::Explore)],
    )
    .unwrap();
    assert!(outcome.gate_id.is_none());
    assert!(g.gate_of("root").is_none());
}

#[test]
fn expand_rejecting_cycle_leaves_graph_unchanged() {
    let mut g = dag(
        Mode::Light,
        vec![
            spec("root", NodeKind::Explore),
            spec("other", NodeKind::Explore),
        ],
    );
    dispatch(&mut g, "root", "w0");
    let before = g.clone();
    // child depends on a node that depends back on the child => cycle once the
    // synthesis edge is added. Construct a direct child self-cycle.
    let err = expand_node(
        &mut g,
        "root",
        "w0",
        vec![spec("root.1", NodeKind::Explore).depends_on(["root"])],
    );
    // root.1 depends on root, and root (synthesis) depends on root.1 => cycle.
    assert!(matches!(err, Err(DagError::WouldCreateCycle(_))));
    assert_eq!(g, before, "failed expand must not mutate the graph");
}

#[test]
fn gate_injection_reblocks_composite_until_gap_drains() {
    let mut g = dag(Mode::Deep, vec![spec("root", NodeKind::Explore)]);
    dispatch(&mut g, "root", "w0");
    let outcome = expand_node(
        &mut g,
        "root",
        "w0",
        vec![spec("root.1", NodeKind::Explore)],
    )
    .unwrap();
    let gate_id = outcome.gate_id.unwrap();

    // Finish the single child so the gate becomes runnable.
    dispatch(&mut g, "root.1", "w0");
    complete_node(&mut g, "root.1", "w0", sim::deep_artifact("child done")).unwrap();
    assert!(ready_nodes(&g).iter().any(|n| n.id == gate_id));

    // Gate runs and finds a gap, injecting a new node and re-queuing itself.
    dispatch(&mut g, &gate_id, "w0");
    let gaps = inject_from_gate(
        &mut g,
        &gate_id,
        "w0",
        vec![NodeSpec::new("root.gap", "missed thing", NodeKind::Explore)],
    )
    .unwrap();
    assert_eq!(gaps, vec!["root.gap".to_string()]);

    // Gate is re-queued and now blocked on the gap; root (composite) still blocked.
    assert_eq!(g.get(&gate_id).unwrap().status, NodeStatus::Queued);
    assert!(
        g.get(&gate_id)
            .unwrap()
            .depends_on
            .contains(&"root.gap".to_string())
    );
    assert!(!ready_nodes(&g).iter().any(|n| n.id == "root"));
    // The gap node is the only newly-ready work.
    assert!(ready_nodes(&g).iter().any(|n| n.id == "root.gap"));

    // Drain the gap, gate passes, root finally closes.
    dispatch(&mut g, "root.gap", "w0");
    complete_node(&mut g, "root.gap", "w0", sim::deep_artifact("gap covered")).unwrap();
    dispatch(&mut g, &gate_id, "w0");
    complete_node(&mut g, &gate_id, "w0", HandoffArtifact::brief("passed")).unwrap();
    assert!(ready_nodes(&g).iter().any(|n| n.id == "root"));
}

#[test]
fn inject_from_gate_rejects_non_gate_node() {
    let mut g = dag(Mode::Deep, vec![spec("a", NodeKind::Explore)]);
    dispatch(&mut g, "a", "w0");
    let err =
        inject_from_gate(&mut g, "a", "w0", vec![spec("a.gap", NodeKind::Explore)]).unwrap_err();
    assert!(matches!(err, DagError::GateMisuse(_)));
}

#[test]
fn expand_records_planner_and_frees_owner_for_rescheduling() {
    let mut g = dag(Mode::Light, vec![spec("root", NodeKind::Explore)]);
    dispatch(&mut g, "root", "w0");
    expand_node(
        &mut g,
        "root",
        "w0",
        vec![spec("root.1", NodeKind::Explore)],
    )
    .unwrap();

    let root = g.get("root").unwrap();
    // Owner is freed so the re-queued composite can be auto-scheduled, but the
    // planner is recorded for synthesis affinity.
    assert_eq!(root.owner, None);
    assert_eq!(root.planner.as_deref(), Some("w0"));
    assert!(root.expanded);

    // Once the child completes, the composite is runnable again (no owner gate).
    dispatch(&mut g, "root.1", "w0");
    complete_node(&mut g, "root.1", "w0", HandoffArtifact::brief("done")).unwrap();
    assert!(ready_nodes(&g).iter().any(|n| n.id == "root"));
}

// ----- full simulator: explore-then-act with gate-spawned gap -----

#[test]
fn simulator_runs_deep_graph_with_composite_and_gap_to_completion() {
    let mut g = dag(
        Mode::Deep,
        vec![
            spec("explore", NodeKind::Explore),
            spec("synth", NodeKind::Synthesize).depends_on(["explore"]),
        ],
    );

    // Scripted behavior:
    //  - "explore" decomposes once into two facets.
    //  - facet "explore.hot" decomposes once into a sub-child, then synthesizes.
    //  - the critique gate "explore::gate" spawns one gap node the first time.
    //  - everything else just completes.
    let mut expanded: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut gate_fired: std::collections::HashSet<String> = std::collections::HashSet::new();

    let mut worker = move |id: &str, kind: NodeKind, _input: &str| -> WorkerAction {
        // Gate nodes: first time, find a gap and inject a node; second time, pass.
        if kind == NodeKind::Critique || kind == NodeKind::Verify {
            if id == "explore::gate" && gate_fired.insert(id.to_string()) {
                return WorkerAction::InjectGap(vec![NodeSpec::new(
                    "explore.gap",
                    "cover the missed facet",
                    NodeKind::Explore,
                )]);
            }
            return WorkerAction::Complete(HandoffArtifact::brief("gate passed"));
        }

        match id {
            "explore" if expanded.insert(id.to_string()) => WorkerAction::Expand(vec![
                spec("explore.geo", NodeKind::Explore),
                spec("explore.hot", NodeKind::Explore),
            ]),
            "explore.hot" if expanded.insert(id.to_string()) => {
                WorkerAction::Expand(vec![spec("explore.hot.udev", NodeKind::Explore)])
            }
            _ => WorkerAction::Complete(sim::deep_artifact(&format!("did {id}"))),
        }
    };

    let report = sim::run(&mut g, 8, 200, &mut worker).unwrap();

    assert!(!report.stalled, "graph should not stall: {report:?}");
    assert_eq!(report.failed, 0);
    assert!(g.all_terminal());

    // The gate-spawned gap node must exist and be done (comprehensiveness gate
    // converted a miss into graph).
    assert!(
        g.get("explore.gap").is_some(),
        "gate should have spawned a gap node"
    );
    assert!(g.get("explore.gap").unwrap().is_done());

    // The composite nodes are expanded and completed via synthesis.
    assert!(g.get("explore").unwrap().expanded);
    assert!(g.get("explore").unwrap().is_done());
    assert!(g.get("explore.hot").unwrap().expanded);
    assert!(g.get("explore.hot").unwrap().is_done());

    // Downstream synthesis ran after explore completed.
    assert!(g.get("synth").unwrap().is_done());
}

#[test]
fn simulator_light_mode_flat_fanout_completes_fast() {
    let mut g = dag(
        Mode::Light,
        vec![
            spec("a", NodeKind::Implement),
            spec("b", NodeKind::Implement),
            spec("c", NodeKind::Implement),
            spec("merge", NodeKind::Synthesize).depends_on(["a", "b", "c"]),
        ],
    );
    let mut worker =
        |id: &str, _k: NodeKind, _i: &str| WorkerAction::Complete(HandoffArtifact::brief(id));
    let report = sim::run(&mut g, 4, 50, &mut worker).unwrap();
    assert!(!report.stalled);
    assert_eq!(report.failed, 0);
    assert_eq!(report.completed, 4);
    assert!(g.all_terminal());
}

#[test]
fn simulator_stalls_when_failed_node_blocks_dependents() {
    let mut g = dag(
        Mode::Light,
        vec![
            spec("a", NodeKind::Implement),
            spec("b", NodeKind::Implement).depends_on(["a"]),
        ],
    );
    let mut worker = |id: &str, _k: NodeKind, _i: &str| {
        if id == "a" {
            WorkerAction::Fail
        } else {
            WorkerAction::Complete(HandoffArtifact::brief(id))
        }
    };
    let report = sim::run(&mut g, 2, 50, &mut worker).unwrap();
    assert!(
        report.stalled,
        "a failed dependency must stall its dependent"
    );
    assert_eq!(report.failed, 1);
    assert!(!g.get("b").unwrap().is_terminal());
}

// ----- dataflow surfaces every artifact field (the critique gate cheat code) -----

#[test]
fn assembled_input_surfaces_what_i_did_not_check_for_gate() {
    // A deep critique gate is told to read each child's `what_i_did_not_check`.
    // It can only do that if hydration actually forwards that field.
    let mut g = dag(Mode::Deep, vec![spec("root", NodeKind::Explore)]);
    dispatch(&mut g, "root", "w0");
    let outcome = expand_node(
        &mut g,
        "root",
        "w0",
        vec![spec("root.1", NodeKind::Explore)],
    )
    .unwrap();
    let gate_id = outcome.gate_id.unwrap();

    dispatch(&mut g, "root.1", "w0");
    let mut artifact = HandoffArtifact::brief("explored the easy path");
    artifact.edge_cases_considered = vec!["empty input".into()];
    artifact.what_i_did_not_check = vec!["the concurrent hotplug path".into()];
    artifact.confidence = Some("medium".into());
    complete_node(&mut g, "root.1", "w0", artifact).unwrap();

    let gate_input = assemble_input(&g, &gate_id);
    assert!(
        gate_input.contains("the concurrent hotplug path"),
        "gate must see what_i_did_not_check: {gate_input}"
    );
    assert!(
        gate_input.contains("empty input"),
        "gate must see edge_cases_considered: {gate_input}"
    );
    assert!(
        gate_input.contains("medium"),
        "gate must see confidence: {gate_input}"
    );
}

#[test]
fn composite_synthesis_rewake_is_hydrated_with_child_artifacts() {
    // The map-reduce synthesis re-wake must receive its children's findings, not
    // just a thin "gate passed" token (doc section 5). The composite retains its
    // child edges precisely so direct-dependency hydration covers the children.
    let mut g = dag(Mode::Deep, vec![spec("root", NodeKind::Explore)]);
    dispatch(&mut g, "root", "w0");
    let outcome = expand_node(
        &mut g,
        "root",
        "w0",
        vec![spec("root.1", NodeKind::Explore)],
    )
    .unwrap();
    let gate_id = outcome.gate_id.unwrap();

    dispatch(&mut g, "root.1", "w0");
    complete_node(
        &mut g,
        "root.1",
        "w0",
        sim::deep_artifact("child found the answer in foo.rs"),
    )
    .unwrap();
    dispatch(&mut g, &gate_id, "w0");
    complete_node(
        &mut g,
        &gate_id,
        "w0",
        HandoffArtifact::brief("gate passed"),
    )
    .unwrap();

    // root is now runnable; its assembled synthesis input must include the child.
    assert!(ready_nodes(&g).iter().any(|n| n.id == "root"));
    let synth_input = assemble_input(&g, "root");
    assert!(
        synth_input.contains("child found the answer in foo.rs"),
        "synthesis re-wake must be hydrated with child artifacts: {synth_input}"
    );
}

// ----- gate id never collides with a user-seeded node id -----

#[test]
fn expand_gate_id_avoids_collision_with_seeded_node() {
    // A user seeds a node whose id is exactly the natural gate id. The auto gate
    // must pick a non-colliding id so id-based lookups are never corrupted.
    let mut g = dag(
        Mode::Deep,
        vec![
            spec("root", NodeKind::Explore),
            spec("root::gate", NodeKind::Explore),
        ],
    );
    dispatch(&mut g, "root", "w0");
    let outcome = expand_node(
        &mut g,
        "root",
        "w0",
        vec![spec("root.1", NodeKind::Explore)],
    )
    .unwrap();
    let gate_id = outcome.gate_id.unwrap();
    assert_ne!(
        gate_id, "root::gate",
        "gate id must not collide with the seeded node"
    );
    assert!(g.get(&gate_id).unwrap().is_gate);
    // The pre-existing user node is still a non-gate node, intact.
    assert!(!g.get("root::gate").unwrap().is_gate);
    // No duplicate ids in the graph.
    let mut ids: Vec<&str> = g.nodes().iter().map(|n| n.id.as_str()).collect();
    ids.sort_unstable();
    let count = ids.len();
    ids.dedup();
    assert_eq!(ids.len(), count, "graph must not contain duplicate ids");
}
