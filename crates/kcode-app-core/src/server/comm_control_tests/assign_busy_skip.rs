// Worker-stacking regression tests for auto-assignment target selection.
//
// Observed live: three `assign_task spawn_if_needed=true` calls within ~100ms
// all auto-picked the SAME reusable worker, queueing three large tasks
// serially on one agent while the swarm had spawn capacity. Two rules pin the
// fix:
//  1. busy is the member's own in-flight work: the candidate filter admits
//     only idle members (`ready`/`completed`), never a working one;
//  2. the pick itself is an in-process claim, so concurrent picks that race
//     ahead of the plan write cannot select the same member twice.
//
// Included into the `comm_control::tests` module, so the parent's private
// selection helpers are in scope.

use super::{
    release_auto_assign_claim, select_and_claim_auto_target, try_claim_auto_assign_target,
};

#[test]
fn auto_pick_skips_a_member_another_request_already_claimed() {
    let swarm_id = "swarm-claim-skip";
    let a = owned_member("worker-a", swarm_id, "ready", "coord");
    let b = owned_member("worker-b", swarm_id, "ready", "coord");
    let candidates = vec![&a, &b];
    // Another in-flight request already picked worker-a.
    assert!(try_claim_auto_assign_target(swarm_id, "worker-a"));

    let picked = select_and_claim_auto_target(swarm_id, &candidates).expect("pick worker-b");
    assert_eq!(picked, "worker-b");

    release_auto_assign_claim(swarm_id, "worker-a");
    release_auto_assign_claim(swarm_id, &picked);
}

#[test]
fn auto_pick_with_every_worker_claimed_reports_no_target_for_spawn_fallback() {
    let swarm_id = "swarm-claimed-only";
    let only = owned_member("worker-a", swarm_id, "ready", "coord");
    let candidates = vec![&only];
    assert!(try_claim_auto_assign_target(swarm_id, "worker-a"));

    let err = select_and_claim_auto_target(swarm_id, &candidates).unwrap_err();
    // The leading sentence is the stable contract that spawn_if_needed /
    // run_plan match on to spawn a fresh agent instead of failing.
    assert!(
        err.starts_with("No ready or completed swarm agents are available"),
        "unexpected error: {err}"
    );
    assert!(err.contains("Skipped 1 worker(s)"), "unexpected error: {err}");

    release_auto_assign_claim(swarm_id, "worker-a");
}

#[test]
fn concurrent_auto_picks_do_not_stack_on_one_member() {
    let swarm_id = "swarm-race-claim";
    let a = owned_member("worker-a", swarm_id, "ready", "coord");
    let b = owned_member("worker-b", swarm_id, "ready", "coord");
    let candidates = vec![&a, &b];

    let first = select_and_claim_auto_target(swarm_id, &candidates).expect("first pick");
    let second = select_and_claim_auto_target(swarm_id, &candidates).expect("second pick");
    assert_ne!(first, second, "two racing picks must not share a member");

    let third = select_and_claim_auto_target(swarm_id, &candidates).unwrap_err();
    assert!(
        third.starts_with("No ready or completed swarm agents are available"),
        "third racing pick should demand a spawn, got: {third}"
    );

    release_auto_assign_claim(swarm_id, &first);
    release_auto_assign_claim(swarm_id, &second);
}

#[test]
fn released_claim_makes_member_pickable_again() {
    let swarm_id = "swarm-claim-release";
    let a = owned_member("worker-a", swarm_id, "ready", "coord");
    let candidates = vec![&a];

    let first = select_and_claim_auto_target(swarm_id, &candidates).expect("first pick");
    assert_eq!(first, "worker-a");
    // The assign path releases the claim once the plan write records (or
    // abandons) the assignment; the member is then genuinely reusable again.
    release_auto_assign_claim(swarm_id, &first);
    let again = select_and_claim_auto_target(swarm_id, &candidates).expect("re-pick");
    assert_eq!(again, "worker-a");
    release_auto_assign_claim(swarm_id, &again);
}

/// Handler-level regression for the rule this stage changed: an idle member is
/// reusable even while it still holds an open row. A plan-derived load count
/// used to refuse this (the stacked worker it was written for was `ready` with
/// an unfinished assignment); busy is now the member's own in-flight work, so
/// the row it already holds does not block a second one. The in-flight claim
/// still covers picks that race ahead of the plan write.
#[tokio::test]
async fn assign_task_reuses_an_idle_worker_that_still_holds_a_row() {
    let (_env, _runtime) = RuntimeEnvGuard::new();
    let repo = scratch_repo();
    let swarm_id = "swarm-reuse-holder";
    let requester = "coord";
    let holder = "worker-holder";
    let (client_tx, mut client_rx) = mpsc::unbounded_channel();
    let sessions = Arc::new(RwLock::new(HashMap::new()));
    let soft_interrupt_queues = Arc::new(RwLock::new(HashMap::new()));
    let client_connections = Arc::new(RwLock::new(HashMap::new()));
    let swarm_members = Arc::new(RwLock::new(HashMap::from([
        (requester.to_string(), member(requester, swarm_id, "ready")),
        // "ready" lifecycle status but still holding an unfinished assignment:
        // the state the load count treated as busy.
        (
            holder.to_string(),
            owned_member(holder, swarm_id, "ready", requester),
        ),
    ])));
    set_repo(&swarm_members, repo.path()).await;
    let mut in_flight = plan_item("in-flight", "queued", "high", &[]);
    in_flight.assigned_to = Some(holder.to_string());
    let swarm_runs = seeded(repo.path(), vec![in_flight, plan_item("next", "queued", "high", &[])]);
    let event_history = Arc::new(RwLock::new(VecDeque::new()));
    let event_counter = Arc::new(AtomicU64::new(1));
    let (swarm_event_tx, _swarm_event_rx) = broadcast::channel(32);
    let mutation_runtime = SwarmMutationRuntime::default();

    handle_comm_assign_task(
        104,
        requester.to_string(),
        None,
        None,
        Some("Keep the holder working".to_string()),
        &client_tx,
        &sessions,
        &soft_interrupt_queues,
        &client_connections,
        &swarm_members,
        &swarm_runs,
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
            assert_eq!(id, 104);
            assert_eq!(task_id, "next");
            assert_eq!(target_session, holder);
        }
        other => panic!("expected the worker to be reused, got {other:?}"),
    }

    let rows = rows_in(repo.path());
    let assigned: Vec<&str> = rows
        .iter()
        .filter(|item| item.assigned_to.as_deref() == Some(holder))
        .map(|item| item.id.as_str())
        .collect();
    assert_eq!(assigned, vec!["in-flight", "next"]);
}
