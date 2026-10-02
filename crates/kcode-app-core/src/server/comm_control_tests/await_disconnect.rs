#[tokio::test]
async fn await_members_stops_when_requesting_client_disconnects() {
    let (_env, _runtime) = RuntimeEnvGuard::new();
    let requester = "req";
    // The run is the requester's: nobody spawned it, so it reports back to
    // nobody, and every peer reports back to it.
    let swarm_id = requester;
    let peer = "peer-1";
    let await_runtime = AwaitMembersRuntime::default();

    let (client_tx, client_rx) = mpsc::unbounded_channel();
    let swarm_members = Arc::new(RwLock::new(HashMap::from([
        (requester.to_string(), member(requester, swarm_id, "ready")),
        (peer.to_string(), member(peer, swarm_id, "running")),
    ])));
    let (swarm_event_tx, swarm_event_rx) = broadcast::channel(32);
    drop(swarm_event_rx);
    let baseline_receivers = swarm_event_tx.receiver_count();

    handle_comm_await_members(
        1,
        requester.to_string(),
        vec!["completed".to_string()],
        vec![],
        None,
        Some(60),
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

    drop(client_rx);

    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        loop {
            if swarm_event_tx.receiver_count() == baseline_receivers {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("await task should unsubscribe promptly after client disconnect");
}
