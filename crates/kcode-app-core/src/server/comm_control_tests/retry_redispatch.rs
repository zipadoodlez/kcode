// An explicit retry must actually dispatch. The durable mutation layer replays a
// persisted final response for an identical request within the final-state TTL,
// which is right for a resend and wrong for a deliberate act: a worker that failed
// quickly would make the coordinator's second retry a silent no-op, a success
// response with the task still failed. The tool's `retry`/`wake` therefore set
// `redispatch`, and this pins that the flag is what keeps the second retry real.

#[tokio::test]
async fn retry_re_dispatches_after_a_recent_identical_retry() {
    let (_env, _runtime) = RuntimeEnvGuard::new();
    let repo = scratch_repo();
    let (requester, worker) = ("coord", "worker");
    let (client_tx, mut client_rx) = mpsc::unbounded_channel();
    let sessions = Arc::new(RwLock::new(HashMap::from([(
        worker.to_string(),
        test_agent().await,
    )])));
    let soft_interrupt_queues = Arc::new(RwLock::new(HashMap::new()));
    let client_connections = Arc::new(RwLock::new(HashMap::new()));
    let swarm_members = Arc::new(RwLock::new(HashMap::from([
        (requester.to_string(), member(requester, requester, "ready")),
        (
            worker.to_string(),
            owned_member(worker, requester, "ready", requester),
        ),
    ])));
    set_repo(&swarm_members, repo.path()).await;
    let mut assigned = plan_item("flaky-task", "failed", "high", &[]);
    assigned.assigned_to = Some(worker.to_string());
    let swarm_runs = seeded(repo.path(), vec![assigned]);
    let event_history = Arc::new(RwLock::new(VecDeque::new()));
    let event_counter = Arc::new(AtomicU64::new(1));
    let (swarm_event_tx, _swarm_event_rx) = broadcast::channel(32);
    let mutation_runtime = SwarmMutationRuntime::default();

    let repo_path = repo.path().to_path_buf();
    let status_leaving_failed = || {
        let repo_path = repo_path.clone();
        async move {
            for _ in 0..200 {
                let status = rows_in(&repo_path)
                    .iter()
                    .find(|item| item.id == "flaky-task")
                    .map(|item| item.status.clone())
                    .unwrap_or_default();
                if status != "failed" {
                    return status;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            "failed".to_string()
        }
    };

    for id in [201u64, 202] {
        handle_comm_assign_task(
            id,
            requester.to_string(),
            Some(worker.to_string()),
            Some("flaky-task".to_string()),
            Some("Retry your assigned task.".to_string()),
            true,
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
        match client_rx.recv().await.expect("retry response") {
            ServerEvent::CommAssignTaskResponse { id: answered, .. } => assert_eq!(answered, id),
            other => panic!("expected CommAssignTaskResponse, got {other:?}"),
        }
        assert_ne!(
            status_leaving_failed().await,
            "failed",
            "retry {id} must dispatch the task"
        );

        // The worker fails the row again, well inside the final-state TTL, so the
        // next identical retry is the case the replay layer would swallow.
        let mut failed = plan_item("flaky-task", "failed", "high", &[]);
        failed.assigned_to = Some(worker.to_string());
        crate::todo::save_tasks(Some(&repo_path), "fixture", &[failed]).expect("write the list");
    }
}
