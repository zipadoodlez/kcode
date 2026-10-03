/// Spawn a worker with `prompt` as its startup message and return its session id.
///
/// Membership is the spawn edge now: a session joins a run by being spawned by
/// that run's root, so spawning is how a test puts a peer into a run, and the
/// startup prompt is what puts that peer to work.
async fn spawn_worker(tool: &CommunicateTool, ctx: &ToolContext, label: &str, prompt: &str) -> String {
    let output = tool
        .execute(
            json!({"action": "spawn", "label": label, "prompt": prompt}),
            ctx.clone(),
        )
        .await
        .expect("spawn should succeed");
    output
        .output
        .strip_prefix("Spawned new agent: ")
        .unwrap_or_else(|| panic!("spawn output should include a session id: {}", output.output))
        .trim()
        .to_string()
}

/// Attach `client` to `target_session`'s event stream with a target-aware
/// subscribe, so the client observes what that session receives. A spawned
/// worker runs in-process with no client of its own, so this is how a test
/// watches the traffic the worker is sent.
async fn attach_to_session(
    client: &mut RawClient,
    target_session: &str,
    working_dir: &Path,
) -> Result<()> {
    let id = client.next_id;
    client.next_id += 1;
    client
        .send_request(Request::Subscribe {
            supports_pdf_panels: false,
            id,
            working_dir: Some(working_dir.display().to_string()),
            selfdev: None,
            target_session_id: Some(target_session.to_string()),
            client_instance_id: None,
            client_has_local_history: false,
            allow_session_takeover: false,
            crash_on_disconnect: false,
            continue_on_disconnect: false,
            terminal_env: Vec::new(),
        })
        .await?;
    client
        .read_until(Duration::from_secs(5), |event| {
            matches!(event, ServerEvent::Done { id: done_id } if *done_id == id)
        })
        .await?;
    Ok(())
}

#[tokio::test]
async fn communicate_list_and_await_members_work_end_to_end() {
    let _env_lock = crate::storage::lock_test_env();
    let runtime_dir = tempfile::TempDir::new().expect("runtime tempdir");
    let repo_dir = std::env::current_dir().expect("repo cwd");
    let socket_path = runtime_dir.path().join("kcode.sock");
    let _runtime = EnvGuard::set("KCODE_RUNTIME_DIR", runtime_dir.path());
    let _socket = EnvGuard::set("KCODE_SOCKET", &socket_path);
    let _debug = EnvGuard::set("KCODE_DEBUG_CONTROL", "1");

    let provider: Arc<dyn Provider> = Arc::new(DelayedTestProvider {
        delay: Duration::from_millis(500),
    });
    let server = Arc::new(Server::new(provider));
    let mut server_task = {
        let server = Arc::clone(&server);
        tokio::spawn(async move { server.run().await })
    };

    let socket_path = runtime_dir.path().join("kcode.sock");
    wait_for_server_socket(&socket_path, &mut server_task)
        .await
        .expect("server socket should be ready");

    let mut watcher = RawClient::connect(&socket_path)
        .await
        .expect("watcher should connect");
    watcher
        .subscribe(&repo_dir)
        .await
        .expect("watcher subscribe");

    let watcher_session = watcher.session_id().await.expect("watcher session id");

    let tool = CommunicateTool::new();
    let ctx = test_ctx(&watcher_session, &repo_dir);

    // The peer is a worker the watcher spawns, so it lands in the watcher's run
    // and its startup prompt puts it to work.
    let peer_session = spawn_worker(
        &tool,
        &ctx,
        "list-await worker",
        "Reply with a short acknowledgement.",
    )
    .await;

    let list_output = tool
        .execute(json!({"action": "list"}), ctx.clone())
        .await
        .expect("communicate list should succeed");
    assert!(
        list_output.output.contains("Status: ready"),
        "expected communicate list to render member status, got: {}",
        list_output.output
    );
    assert!(
        list_output.output.contains(&peer_session),
        "expected communicate list to list the spawned worker, got: {}",
        list_output.output
    );

    let running_members =
        wait_for_member_status(&mut watcher, &watcher_session, &peer_session, "running")
            .await
            .expect("spawned worker should enter running state");
    let running_peer = running_members
        .iter()
        .find(|member| member.session_id == peer_session)
        .expect("spawned worker should be listed while running");
    assert_eq!(running_peer.status, Some(SwarmLifecycleStatus::Running));

    // Legacy background=false input is upgraded to a durable asynchronous wait.
    let await_output = tokio::time::timeout(
        Duration::from_secs(5),
        tool.execute(
            json!({
                "action": "await_members",
                "session_ids": [peer_session.clone()],
                "timeout_minutes": 1,
                "background": false
            }),
            ctx.clone(),
        ),
    )
    .await
    .expect("legacy blocking request should return promptly")
    .expect("await_members should start");
    assert!(
        await_output.output.contains("no longer supported")
            && await_output.output.contains("asynchronously"),
        "expected compatibility hand-off output, got: {}",
        await_output.output
    );

    let event = watcher
        .read_until(Duration::from_secs(5), |event| {
            matches!(
                event,
                ServerEvent::Notification {
                    notification_type: NotificationType::Message { scope: Some(scope), .. },
                    ..
                } if scope == "swarm_await"
            )
        })
        .await
        .expect("upgraded asynchronous wait should notify on completion");
    let ServerEvent::Notification { message, .. } = event else {
        panic!("expected swarm_await notification, got: {event:?}");
    };
    assert!(
        message.contains("(ready)"),
        "expected await_members to treat ready as done, got: {}",
        message
    );

    let ready_members =
        wait_for_member_status(&mut watcher, &watcher_session, &peer_session, "ready")
            .await
            .expect("spawned worker should return to ready state");
    let ready_peer = ready_members
        .iter()
        .find(|member| member.session_id == peer_session)
        .expect("spawned worker should still be listed when ready");
    assert_eq!(ready_peer.status, Some(SwarmLifecycleStatus::Ready));

    server_task.abort();
}

#[tokio::test]
async fn communicate_await_members_background_returns_immediately_and_notifies() {
    let _env_lock = crate::storage::lock_test_env();
    let runtime_dir = tempfile::TempDir::new().expect("runtime tempdir");
    let repo_dir = std::env::current_dir().expect("repo cwd");
    let socket_path = runtime_dir.path().join("kcode.sock");
    let _runtime = EnvGuard::set("KCODE_RUNTIME_DIR", runtime_dir.path());
    let _socket = EnvGuard::set("KCODE_SOCKET", &socket_path);
    let _debug = EnvGuard::set("KCODE_DEBUG_CONTROL", "1");

    let provider: Arc<dyn Provider> = Arc::new(DelayedTestProvider {
        delay: Duration::from_millis(500),
    });
    let server = Arc::new(Server::new(provider));
    let mut server_task = {
        let server = Arc::clone(&server);
        tokio::spawn(async move { server.run().await })
    };

    let socket_path = runtime_dir.path().join("kcode.sock");
    wait_for_server_socket(&socket_path, &mut server_task)
        .await
        .expect("server socket should be ready");

    let mut watcher = RawClient::connect(&socket_path)
        .await
        .expect("watcher should connect");
    watcher
        .subscribe(&repo_dir)
        .await
        .expect("watcher subscribe");

    let watcher_session = watcher.session_id().await.expect("watcher session id");

    let tool = CommunicateTool::new();
    let ctx = test_ctx(&watcher_session, &repo_dir);

    // The peer is a worker the watcher spawns, so it is a member of the
    // watcher's run; its prompt puts it in a running state so the await
    // actually has to wait.
    let peer_session = spawn_worker(
        &tool,
        &ctx,
        "await-bg worker",
        "Reply with a short acknowledgement.",
    )
    .await;
    wait_for_member_status(&mut watcher, &watcher_session, &peer_session, "running")
        .await
        .expect("spawned worker should enter running state");

    // Background await (the default) must return promptly with a hand-off
    // message instead of blocking until the peer finishes.
    let await_output = tokio::time::timeout(
        Duration::from_secs(5),
        tool.execute(
            json!({
                "action": "await_members",
                "session_ids": [peer_session.clone()],
                "timeout_minutes": 1
            }),
            ctx.clone(),
        ),
    )
    .await
    .expect("background await should return promptly")
    .expect("await_members should succeed");
    assert!(
        await_output.output.contains("background"),
        "expected background hand-off message, got: {}",
        await_output.output
    );

    // The backgrounded watcher should deliver a swarm-await notification to the
    // requesting (watcher) session once the peer reaches ready.
    let event = watcher
        .read_until(Duration::from_secs(5), |event| {
            matches!(
                event,
                ServerEvent::Notification {
                    notification_type: NotificationType::Message { scope: Some(scope), .. },
                    ..
                } if scope == "swarm_await"
            )
        })
        .await
        .expect("background await should deliver a swarm_await notification");
    let ServerEvent::Notification { message, .. } = event else {
        panic!("expected swarm_await notification, got: {event:?}");
    };
    assert!(
        message.contains("Swarm await finished"),
        "expected swarm await completion body, got: {}",
        message
    );

    wait_for_member_status(&mut watcher, &watcher_session, &peer_session, "ready")
        .await
        .expect("spawned worker should return to ready state");

    server_task.abort();
}

#[tokio::test]
async fn communicate_status_returns_busy_snapshot_for_running_member() {
    let _env_lock = crate::storage::lock_test_env();
    let runtime_dir = tempfile::TempDir::new().expect("runtime tempdir");
    let repo_dir = std::env::current_dir().expect("repo cwd");
    let socket_path = runtime_dir.path().join("kcode.sock");
    let _runtime = EnvGuard::set("KCODE_RUNTIME_DIR", runtime_dir.path());
    let _socket = EnvGuard::set("KCODE_SOCKET", &socket_path);
    let _debug = EnvGuard::set("KCODE_DEBUG_CONTROL", "1");

    let provider: Arc<dyn Provider> = Arc::new(DelayedTestProvider {
        delay: Duration::from_millis(1500),
    });
    let server = Arc::new(Server::new(provider));
    let mut server_task = {
        let server = Arc::clone(&server);
        tokio::spawn(async move { server.run().await })
    };

    wait_for_server_socket(&socket_path, &mut server_task)
        .await
        .expect("server socket should be ready");

    let mut watcher = RawClient::connect(&socket_path)
        .await
        .expect("watcher should connect");
    watcher
        .subscribe(&repo_dir)
        .await
        .expect("watcher subscribe");

    let watcher_session = watcher.session_id().await.expect("watcher session id");
    let tool = CommunicateTool::new();
    let ctx = test_ctx(&watcher_session, &repo_dir);

    // A worker the watcher spawned is a member of the watcher's run; the long
    // provider delay keeps it running while the snapshot is taken.
    let peer_session = spawn_worker(
        &tool,
        &ctx,
        "status-busy worker",
        "Reply with a short acknowledgement.",
    )
    .await;

    wait_for_member_status(&mut watcher, &watcher_session, &peer_session, "running")
        .await
        .expect("spawned worker should enter running state");

    let snapshot = watcher
        .comm_status(&watcher_session, &peer_session)
        .await
        .expect("comm_status should succeed while the worker is busy");
    assert_eq!(snapshot.session_id, peer_session);
    assert_eq!(snapshot.status, Some(SwarmLifecycleStatus::Running));
    assert!(
        snapshot
            .activity
            .as_ref()
            .is_some_and(|activity| activity.is_processing)
    );

    let output = tool
        .execute(
            json!({
                "action": "status",
                "target_session": peer_session.clone()
            }),
            ctx,
        )
        .await
        .expect("status action should succeed");
    assert!(output.output.contains("Lifecycle: running"));
    assert!(output.output.contains("Activity: busy"));

    server_task.abort();
}

#[tokio::test]
async fn communicate_spawn_reports_completion_back_to_spawner() {
    let _env_lock = crate::storage::lock_test_env();
    let runtime_dir = tempfile::TempDir::new().expect("runtime tempdir");
    let repo_dir = std::env::current_dir().expect("repo cwd");
    let socket_path = runtime_dir.path().join("kcode.sock");
    let _runtime = EnvGuard::set("KCODE_RUNTIME_DIR", runtime_dir.path());
    let _socket = EnvGuard::set("KCODE_SOCKET", &socket_path);
    let _debug = EnvGuard::set("KCODE_DEBUG_CONTROL", "1");

    let provider: Arc<dyn Provider> = Arc::new(DelayedTestProvider {
        delay: Duration::from_millis(100),
    });
    let server = Arc::new(Server::new(provider));
    let mut server_task = {
        let server = Arc::clone(&server);
        tokio::spawn(async move { server.run().await })
    };

    let socket_path = runtime_dir.path().join("kcode.sock");
    wait_for_server_socket(&socket_path, &mut server_task)
        .await
        .expect("server socket should be ready");

    let mut watcher = RawClient::connect(&socket_path)
        .await
        .expect("watcher should connect");
    watcher
        .subscribe(&repo_dir)
        .await
        .expect("watcher subscribe");

    let watcher_session = watcher.session_id().await.expect("watcher session id");
    let tool = CommunicateTool::new();
    let ctx = test_ctx(&watcher_session, &repo_dir);

    let spawn_output = tool
        .execute(
            json!({
                "action": "spawn",
                "label": "report-back worker",
                "prompt": "Reply with exactly AUTH_TEST_OK and nothing else."
            }),
            ctx,
        )
        .await
        .expect("spawn with prompt should succeed");
    let spawned_session = spawn_output
        .output
        .strip_prefix("Spawned new agent: ")
        .expect("spawn output should include session id")
        .trim()
        .to_string();

    watcher
        .read_until(Duration::from_secs(15), |event| {
            matches!(
                event,
                ServerEvent::Notification {
                    from_session,
                    notification_type: crate::protocol::NotificationType::Message {
                        scope: Some(scope),
                        channel: None,
                        tldr: None,
                    },
                    message,
                    ..
                } if from_session == &spawned_session
                    && scope == "swarm"
                    && message.contains("finished their work and is ready for more")
            )
        })
        .await
        .expect("spawner should receive completion report-back notification");

    server_task.abort();
}

#[tokio::test]
async fn communicate_spawn_with_prompt_and_summary_work_end_to_end() {
    let _env_lock = crate::storage::lock_test_env();
    let runtime_dir = tempfile::TempDir::new().expect("runtime tempdir");
    let repo_dir = std::env::current_dir().expect("repo cwd");
    let socket_path = runtime_dir.path().join("kcode.sock");
    let _runtime = EnvGuard::set("KCODE_RUNTIME_DIR", runtime_dir.path());
    let _socket = EnvGuard::set("KCODE_SOCKET", &socket_path);
    let _debug = EnvGuard::set("KCODE_DEBUG_CONTROL", "1");

    let provider: Arc<dyn Provider> = Arc::new(DelayedTestProvider {
        delay: Duration::from_millis(100),
    });
    let server = Arc::new(Server::new(provider));
    let mut server_task = {
        let server = Arc::clone(&server);
        tokio::spawn(async move { server.run().await })
    };

    let socket_path = runtime_dir.path().join("kcode.sock");
    wait_for_server_socket(&socket_path, &mut server_task)
        .await
        .expect("server socket should be ready");

    let mut watcher = RawClient::connect(&socket_path)
        .await
        .expect("watcher should connect");
    watcher
        .subscribe(&repo_dir)
        .await
        .expect("watcher subscribe");

    let watcher_session = watcher.session_id().await.expect("watcher session id");
    let tool = CommunicateTool::new();
    let ctx = test_ctx(&watcher_session, &repo_dir);

    let spawn_output = tool
        .execute(
            json!({
                "action": "spawn",
                "label": "summary worker",
                "prompt": "Reply with a short acknowledgement."
            }),
            ctx.clone(),
        )
        .await
        .expect("spawn with prompt should succeed");
    let spawned_session = spawn_output
        .output
        .strip_prefix("Spawned new agent: ")
        .expect("spawn output should include session id")
        .trim()
        .to_string();
    assert!(
        !spawned_session.is_empty(),
        "spawned session id should not be empty"
    );

    wait_for_member_presence(&mut watcher, &watcher_session, &spawned_session)
        .await
        .expect("spawned member should appear in swarm list");

    let summary_output = {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        loop {
            match tool
                .execute(
                    json!({
                        "action": "summary",
                        "target_session": spawned_session
                    }),
                    ctx.clone(),
                )
                .await
            {
                Ok(output) => break output,
                Err(err)
                    if (err.to_string().contains("Unknown session")
                        || err.to_string().contains(" is busy;"))
                        && tokio::time::Instant::now() < deadline =>
                {
                    tokio::time::sleep(Duration::from_millis(25)).await;
                }
                Err(err) => panic!("summary for spawned agent should succeed: {err}"),
            }
        }
    };
    assert!(
        summary_output.output.contains("Tool call summary for")
            || summary_output.output.contains("No tool calls found for"),
        "unexpected summary output: {}",
        summary_output.output
    );

    server_task.abort();
}

/// `message` routes by the fields supplied (DM when `to_session` is set,
/// broadcast otherwise), while `broadcast` is a group send scoped to the
/// sender's spawned subtree (whole swarm when the sender is the coordinator).
/// Regression test for the bug where `message` and `broadcast` were identical
/// because the tool discarded `to_session`/`channel` for both.
#[tokio::test]
async fn communicate_message_routes_as_dm_while_broadcast_targets_swarm() {
    let _env_lock = crate::storage::lock_test_env();
    let runtime_dir = tempfile::TempDir::new().expect("runtime tempdir");
    let repo_dir = std::env::current_dir().expect("repo cwd");
    let socket_path = runtime_dir.path().join("kcode.sock");
    let _runtime = EnvGuard::set("KCODE_RUNTIME_DIR", runtime_dir.path());
    let _socket = EnvGuard::set("KCODE_SOCKET", &socket_path);
    let _debug = EnvGuard::set("KCODE_DEBUG_CONTROL", "1");

    let provider: Arc<dyn Provider> = Arc::new(DelayedTestProvider {
        delay: Duration::from_millis(100),
    });
    let server = Arc::new(Server::new(provider));
    let mut server_task = {
        let server = Arc::clone(&server);
        tokio::spawn(async move { server.run().await })
    };

    wait_for_server_socket(&socket_path, &mut server_task)
        .await
        .expect("server socket should be ready");

    let mut sender = RawClient::connect(&socket_path)
        .await
        .expect("sender should connect");
    sender.subscribe(&repo_dir).await.expect("sender subscribe");

    let sender_session = sender.session_id().await.expect("sender session id");

    let tool = CommunicateTool::new();
    let ctx = test_ctx(&sender_session, &repo_dir);

    // The peer is a worker the sender spawned: the sender roots its own run, so
    // the worker is inside the subtree a broadcast from the sender reaches.
    let peer_session = spawn_worker(
        &tool,
        &ctx,
        "dm-broadcast worker",
        "Reply with a short acknowledgement.",
    )
    .await;
    wait_for_member_presence(&mut sender, &sender_session, &peer_session)
        .await
        .expect("spawned worker should join the sender's run");

    // The worker runs in-process with no client of its own; attach a viewer so
    // the test observes what the worker receives.
    let mut peer = RawClient::connect(&socket_path)
        .await
        .expect("peer viewer should connect");
    attach_to_session(&mut peer, &peer_session, &repo_dir)
        .await
        .expect("attach to the spawned worker");

    // `message` with a `to_session` should arrive at the peer scoped as a DM.
    let dm_output = tool
        .execute(
            json!({
                "action": "message",
                "message": "ping-dm",
                "to_session": peer_session.clone()
            }),
            ctx.clone(),
        )
        .await
        .expect("message with to_session should succeed");
    assert!(
        dm_output.output.contains("Direct message sent to"),
        "message with to_session should report a DM, got: {}",
        dm_output.output
    );
    let dm_scope = peer
        .next_message_notification(Duration::from_secs(5))
        .await
        .expect("spawned worker should receive the targeted message");
    assert_eq!(
        dm_scope.as_deref(),
        Some("dm"),
        "message with to_session should be delivered with dm scope"
    );

    // `broadcast` should reach the peer scoped as a broadcast even though no
    // explicit target is supplied: the peer is in the sender's spawned subtree.
    let broadcast_output = tool
        .execute(
            json!({
                "action": "broadcast",
                "message": "ping-all"
            }),
            ctx.clone(),
        )
        .await
        .expect("broadcast should succeed");
    assert!(
        broadcast_output
            .output
            .contains("Broadcast sent to your spawned subtree"),
        "broadcast should report a subtree-scoped group send, got: {}",
        broadcast_output.output
    );
    let broadcast_scope = peer
        .next_message_notification(Duration::from_secs(5))
        .await
        .expect("spawned worker should receive the broadcast");
    assert_eq!(
        broadcast_scope.as_deref(),
        Some("broadcast"),
        "broadcast should be delivered with broadcast scope"
    );

    server_task.abort();
}
