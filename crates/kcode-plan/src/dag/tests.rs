//! Invariant tests for the task-DAG engine, including a full simulator run that
//! reproduces the DAG engine mechanics in `docs/internals/swarm.md`.

use super::sim::{self, WorkerAction};
use super::*;

fn spec(id: &str, kind: NodeKind) -> NodeSpec {
    NodeSpec::new(id, format!("task {id}"), kind)
}

fn dag(specs: Vec<NodeSpec>) -> TaskGraph {
    let mut g = TaskGraph::new();
    seed(&mut g, specs).expect("seed should succeed");
    g
}

// ----- seed validation -----

#[test]
fn seed_rejects_duplicate_ids_with_conflicting_definitions() {
    let mut g = TaskGraph::new();
    let mut conflicting = spec("a", NodeKind::Explore);
    conflicting.content = "different task".to_string();
    let err = seed(&mut g, vec![spec("a", NodeKind::Explore), conflicting]).unwrap_err();
    assert_eq!(err, DagError::DuplicateNode("a".into()));
}

#[test]
fn seed_collapses_identical_duplicate_specs_in_one_batch() {
    let mut g = TaskGraph::new();
    let repeated = spec("a", NodeKind::Explore).depends_on(["upstream", "upstream"]);
    seed(
        &mut g,
        vec![
            spec("upstream", NodeKind::Explore),
            repeated.clone(),
            repeated,
        ],
    )
    .unwrap();
    assert_eq!(g.nodes().iter().filter(|node| node.id == "a").count(), 1);
}

#[test]
fn seed_replay_is_idempotent_and_preserves_runtime_state() {
    let original = spec("a", NodeKind::Explore).depends_on(["upstream"]);
    let mut g = dag(vec![spec("upstream", NodeKind::Explore), original.clone()]);
    dispatch(&mut g, "upstream", "worker");
    let before = g.clone();

    seed(&mut g, vec![original]).unwrap();

    assert_eq!(g, before, "an identical replay must be a complete no-op");
}

#[test]
fn seed_replay_accepts_dependency_order_and_duplicate_differences() {
    let mut g = dag(vec![
        spec("a", NodeKind::Explore),
        spec("b", NodeKind::Explore),
        spec("join", NodeKind::Synthesize).depends_on(["a", "b"]),
    ]);
    let before = g.clone();

    seed(
        &mut g,
        vec![spec("join", NodeKind::Synthesize).depends_on(["b", "a", "a"])],
    )
    .unwrap();

    assert_eq!(g, before);
}

#[test]
fn partial_seed_replay_adds_only_new_nodes() {
    let existing = spec("a", NodeKind::Explore);
    let mut g = dag(vec![existing.clone()]);

    seed(
        &mut g,
        vec![existing, spec("b", NodeKind::Implement).depends_on(["a"])],
    )
    .unwrap();

    assert_eq!(g.nodes().iter().filter(|node| node.id == "a").count(), 1);
    assert_eq!(g.nodes().iter().filter(|node| node.id == "b").count(), 1);
}

#[test]
fn seed_replay_rejects_changed_existing_definition() {
    let mut g = dag(vec![spec("a", NodeKind::Explore)]);
    let before = g.clone();
    let mut changed = spec("a", NodeKind::Explore);
    changed.priority = 7;

    let err = seed(&mut g, vec![changed]).unwrap_err();

    assert_eq!(err, DagError::DuplicateNode("a".into()));
    assert_eq!(g, before, "a conflict must not partially mutate the graph");
}

#[test]
fn seed_rejects_unknown_dependency() {
    let mut g = TaskGraph::new();
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
    let mut g = TaskGraph::new();
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
    let g = dag(vec![
        spec("a", NodeKind::Explore).priority(1),
        spec("b", NodeKind::Explore).priority(0),
        spec("c", NodeKind::Explore).depends_on(["a"]),
    ]);
    // a and b are ready (no deps); c is blocked on a. b sorts first (priority 0).
    let ready: Vec<&str> = ready_nodes(&g).iter().map(|n| n.id.as_str()).collect();
    assert_eq!(ready, vec!["b", "a"]);
}

#[test]
fn dispatch_assigns_owner_and_blocks_dependents() {
    let mut g = dag(vec![
        spec("a", NodeKind::Explore),
        spec("b", NodeKind::Implement).depends_on(["a"]),
    ]);
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
    let mut g = dag(vec![spec("a", NodeKind::Explore)]);
    dispatch(&mut g, "a", "w0");
    let err = complete_node(&mut g, "a", "intruder").unwrap_err();
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
    let mut g = dag(vec![spec("a", NodeKind::Explore)]);
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

// ----- composite expansion + gate insertion -----

#[test]
fn expand_rejecting_cycle_leaves_graph_unchanged() {
    let mut g = dag(vec![
        spec("root", NodeKind::Explore),
        spec("other", NodeKind::Explore),
    ]);
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
fn expand_keeps_the_owner_who_integrates_the_children() {
    let mut g = dag(vec![spec("root", NodeKind::Explore)]);
    dispatch(&mut g, "root", "w0");
    expand_node(
        &mut g,
        "root",
        "w0",
        vec![spec("root.1", NodeKind::Explore)],
    )
    .unwrap();

    let root = g.get("root").unwrap();
    // The owner stays: it is the record of who integrates the children, and
    // ready_nodes never reads ownership, so the composite stays runnable.
    assert_eq!(root.owner.as_deref(), Some("w0"));
    assert!(root.expanded);

    // Once the child completes, the composite is runnable again (no owner gate).
    dispatch(&mut g, "root.1", "w0");
    complete_node(&mut g, "root.1", "w0").unwrap();
    assert!(ready_nodes(&g).iter().any(|n| n.id == "root"));
}

// ----- full simulator -----

#[test]
fn simulator_stalls_when_failed_node_blocks_dependents() {
    let mut g = dag(vec![
        spec("a", NodeKind::Implement),
        spec("b", NodeKind::Implement).depends_on(["a"]),
    ]);
    let mut worker = |id: &str, _k: NodeKind| {
        if id == "a" {
            WorkerAction::Fail
        } else {
            WorkerAction::Complete
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

#[test]
fn seed_accepts_duplicate_dependencies_without_false_cycle() {
    // A repeated dep in agent JSON must not be misread as a cycle: indegree used
    // to count occurrences while relaxation decremented unique pairs.
    let mut g = TaskGraph::new();
    let mut a = spec("a", NodeKind::Explore);
    a.depends_on = vec!["b".into(), "b".into()];
    seed(&mut g, vec![a, spec("b", NodeKind::Explore)]).expect("duplicate deps are not a cycle");
    // Deps are deduped on insertion.
    assert_eq!(g.get("a").unwrap().depends_on, vec!["b".to_string()]);
    // And the graph drains normally.
    dispatch(&mut g, "b", "w0");
    complete_node(&mut g, "b", "w0").unwrap();
    assert_eq!(ready_nodes(&g).len(), 1);
}

#[test]
fn seed_rejects_blank_ids() {
    let mut g = TaskGraph::new();
    let blank = NodeSpec::new("", "task", NodeKind::Explore);
    let err = seed(&mut g, vec![blank]).unwrap_err();
    assert!(matches!(err, DagError::InvalidSpec(_)));
    let mut ws = TaskGraph::new();
    let white = NodeSpec::new("   ", "task", NodeKind::Explore);
    assert!(seed(&mut ws, vec![white]).is_err());
}
