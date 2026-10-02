#[tokio::test]
async fn await_members_includes_late_joiners_when_watching_swarm() {
    let (_env, _runtime) = RuntimeEnvGuard::new();
    let requester = "req";
    // The run is the requester's: nobody spawned it, so it reports back to
    // nobody, and every peer reports back to it.
    let swarm_id = requester;
    let initial_peer = "peer-1";
    let late_peer = "peer-2";
    let await_runtime = AwaitMembersRuntime::default();

    let (client_tx, mut client_rx) = mpsc::unbounded_channel();
    let swarm_members = Arc::new(RwLock::new(HashMap::from([
        (requester.to_string(), member(requester, swarm_id, "ready")),
        (
            initial_peer.to_string(),
            member(initial_peer, swarm_id, "running"),
        ),
    ])));
    let (swarm_event_tx, _swarm_event_rx) = broadcast::channel(32);

    handle_comm_await_members(
        1,
        requester.to_string(),
        vec!["completed".to_string()],
        vec![],
        None,
        Some(2),
        false,
        false,
        false,
        CommAwaitMembersContext {
            client_event_tx: &client_tx,
            swarm_members: &swarm_members,
            swarm_event_tx: &swarm_event_tx,
            await_members_runtime: &await_runtime,
        },
    )
    .await;

    {
        let mut members = swarm_members.write().await;
        members.insert(
            late_peer.to_string(),
            member(late_peer, swarm_id, "running"),
        );
    }
    let _ = swarm_event_tx.send(swarm_event(
        late_peer,
        swarm_id,
        SwarmEventType::MemberChange {
            action: "joined".to_string(),
        },
    ));

    {
        let mut members = swarm_members.write().await;
        members
            .get_mut(initial_peer)
            .expect("initial peer exists")
            .status = SwarmLifecycleStatus::Completed;
    }
    let _ = swarm_event_tx.send(swarm_event(
        initial_peer,
        swarm_id,
        SwarmEventType::StatusChange {
            old_status: SwarmLifecycleStatus::Running,
            new_status: SwarmLifecycleStatus::Completed,
        },
    ));

    {
        let mut members = swarm_members.write().await;
        members.get_mut(late_peer).expect("late peer exists").status = SwarmLifecycleStatus::Completed;
    }
    let _ = swarm_event_tx.send(swarm_event(
        late_peer,
        swarm_id,
        SwarmEventType::StatusChange {
            old_status: SwarmLifecycleStatus::Running,
            new_status: SwarmLifecycleStatus::Completed,
        },
    ));

    let response = tokio::time::timeout(std::time::Duration::from_secs(1), client_rx.recv())
        .await
        .expect("response should arrive")
        .expect("channel should stay open");

    match response {
        ServerEvent::CommAwaitMembersResponse {
            completed, members, ..
        } => {
            assert!(completed, "await should complete after both peers finish");
            let watched: HashSet<String> = members.into_iter().map(|m| m.session_id).collect();
            assert!(watched.contains(initial_peer));
            assert!(watched.contains(late_peer));
        }
        other => panic!("expected CommAwaitMembersResponse, got {other:?}"),
    }
}
