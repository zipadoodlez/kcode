use super::*;
use crate::protocol::SwarmLifecycleStatus;
use std::time::{Duration, Instant};

struct EnvGuard {
    _lock: std::sync::MutexGuard<'static, ()>,
    runtime: Option<std::ffi::OsString>,
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        if let Some(value) = self.runtime.take() {
            crate::env::set_var("KCODE_RUNTIME_DIR", value);
        } else {
            crate::env::remove_var("KCODE_RUNTIME_DIR");
        }
    }
}

fn test_env(dir: &tempfile::TempDir) -> EnvGuard {
    let lock = storage::lock_test_env();
    let previous = std::env::var_os("KCODE_RUNTIME_DIR");
    crate::env::set_var("KCODE_RUNTIME_DIR", dir.path());
    EnvGuard {
        _lock: lock,
        runtime: previous,
    }
}

/// The smallest member a snapshot can hold: a session and nothing else.
fn persisted_member(session_id: &str) -> SwarmMember {
    let (event_tx, _event_rx) = tokio::sync::mpsc::unbounded_channel();
    SwarmMember {
        session_id: session_id.to_string(),
        event_tx,
        event_txs: HashMap::new(),
        working_dir: None,
        status: SwarmLifecycleStatus::Ready,
        detail: None,
        friendly_name: None,
        report_back_to_session_id: None,
        latest_completion_report: None,
        joined_at: Instant::now(),
        last_status_change: Instant::now(),
        is_headless: false,
        output_tail: None,
        todo_progress: None,
        todo_items: Vec::new(),
        runtime: crate::protocol::SwarmMemberRuntime::default(),
        task_label: None,
    }
}

/// A member entry in the shape a snapshot on disk holds. Tests that plant a file
/// the current writer would not produce use this instead of building a
/// `SwarmMember`; its keys mirror `SwarmMemberRecord`.
fn persisted_member_json(session_id: &str) -> serde_json::Value {
    serde_json::json!({
        "session_id": session_id,
        "working_dir": null,
        "status": "ready",
        "detail": null,
        "friendly_name": session_id,
        "report_back_to_session_id": null,
        "is_headless": true,
    })
}

#[test]
fn persisted_swarm_state_round_trips() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let _env = test_env(&dir);

    let (root_tx, _root_rx) = tokio::sync::mpsc::unbounded_channel();
    let (event_tx, _event_rx) = tokio::sync::mpsc::unbounded_channel();
    let members = vec![
        SwarmMember {
            session_id: "session-2".to_string(),
            event_tx: root_tx,
            event_txs: HashMap::new(),
            working_dir: Some(PathBuf::from("/tmp/swarm-alpha")),
            status: SwarmLifecycleStatus::Ready,
            detail: None,
            friendly_name: Some("owl".to_string()),
            report_back_to_session_id: None,
            latest_completion_report: None,
            joined_at: Instant::now(),
            last_status_change: Instant::now(),
            is_headless: false,
            output_tail: None,
            todo_progress: None,
            todo_items: Vec::new(),
            runtime: crate::protocol::SwarmMemberRuntime::default(),
            task_label: None,
        },
        SwarmMember {
            session_id: "session-1".to_string(),
            event_tx,
            event_txs: HashMap::new(),
            working_dir: Some(PathBuf::from("/tmp/swarm-alpha")),
            status: SwarmLifecycleStatus::Running,
            detail: Some("writing tests".to_string()),
            friendly_name: Some("fox".to_string()),
            report_back_to_session_id: Some("session-2".to_string()),
            latest_completion_report: None,
            joined_at: Instant::now(),
            last_status_change: Instant::now(),
            is_headless: true,
            output_tail: None,
            todo_progress: None,
            todo_items: Vec::new(),
            runtime: crate::protocol::SwarmMemberRuntime::default(),
            task_label: None,
        },
    ];

    persist_swarm_state("swarm-alpha", &members);
    let loaded = load_runtime_state();

    let recovered_member = loaded.members.get("session-1").expect("recovered member");
    assert_eq!(recovered_member.friendly_name.as_deref(), Some("fox"));
    assert_eq!(
        recovered_member.report_back_to_session_id.as_deref(),
        Some("session-2")
    );
    assert_eq!(recovered_member.status, SwarmLifecycleStatus::Crashed);
    assert_eq!(
        recovered_member.detail.as_deref(),
        Some("writing tests (recovered after reload while running)")
    );
    // The report-back edge is the membership, so the recovered worker still
    // belongs to the run its coordinator roots.
    assert_eq!(
        crate::server::swarm::swarm_root(&loaded.members, "session-1").as_deref(),
        Some("session-2")
    );
}

#[test]
fn ready_headless_member_with_report_stops_without_losing_report() {
    // A headless worker that finished its task has no process after restart.
    // Preserve its report, but do not eagerly reconstruct the full Agent just
    // to keep an idle worker reusable indefinitely.
    let dir = tempfile::TempDir::new().expect("tempdir");
    let _env = test_env(&dir);

    let (event_tx, _event_rx) = tokio::sync::mpsc::unbounded_channel();
    let members = vec![SwarmMember {
        session_id: "session-ready".to_string(),
        event_tx,
        event_txs: HashMap::new(),
        working_dir: Some(PathBuf::from("/tmp/swarm-gamma")),
        status: SwarmLifecycleStatus::Ready,
        detail: None,
        friendly_name: Some("pig".to_string()),
        report_back_to_session_id: Some("session-coordinator".to_string()),
        latest_completion_report: Some("Done. Built the worker; all tests pass.".to_string()),
        joined_at: Instant::now(),
        last_status_change: Instant::now(),
        is_headless: true,
        output_tail: None,
        todo_progress: None,
        todo_items: Vec::new(),
        runtime: crate::protocol::SwarmMemberRuntime::default(),
        task_label: None,
    }];

    persist_swarm_state("swarm-gamma", &members);
    let loaded = load_runtime_state();

    let recovered = loaded.members.get("session-ready").expect("member");
    assert_eq!(recovered.status, SwarmLifecycleStatus::Stopped);
    assert_eq!(
        recovered.detail.as_deref(),
        Some("idle worker not restored after server restart")
    );
    assert_eq!(
        recovered.latest_completion_report.as_deref(),
        Some("Done. Built the worker; all tests pass.")
    );
}

#[test]
fn ready_detached_client_stops_on_reload_until_it_reattaches() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let _env = test_env(&dir);

    let (event_tx, _event_rx) = tokio::sync::mpsc::unbounded_channel();
    let members = vec![SwarmMember {
        session_id: "session-detached".to_string(),
        event_tx,
        event_txs: HashMap::new(),
        working_dir: Some(PathBuf::from("/tmp/swarm-client")),
        status: SwarmLifecycleStatus::Ready,
        detail: None,
        friendly_name: Some("finch".to_string()),
        report_back_to_session_id: None,
        latest_completion_report: None,
        joined_at: Instant::now(),
        last_status_change: Instant::now(),
        is_headless: false,
        output_tail: None,
        todo_progress: None,
        todo_items: Vec::new(),
        runtime: crate::protocol::SwarmMemberRuntime::default(),
        task_label: None,
    }];

    persist_swarm_state("swarm-client", &members);
    let loaded = load_runtime_state();
    let recovered = loaded.members.get("session-detached").expect("member");
    assert_eq!(recovered.status, SwarmLifecycleStatus::Stopped);
    assert_eq!(
        recovered.detail.as_deref(),
        Some("client not attached after server restart")
    );
}

#[test]
fn terminal_member_retention_preserves_recent_reports_and_prunes_expired_records() {
    let (event_tx, _event_rx) = tokio::sync::mpsc::unbounded_channel();
    let member = SwarmMember {
        session_id: "session-terminal".to_string(),
        event_tx,
        event_txs: HashMap::new(),
        working_dir: Some(PathBuf::from("/tmp/swarm-terminal")),
        status: SwarmLifecycleStatus::Completed,
        detail: Some("done".to_string()),
        friendly_name: Some("otter".to_string()),
        report_back_to_session_id: Some("session-coordinator".to_string()),
        latest_completion_report: Some("All targeted tests passed.".to_string()),
        joined_at: Instant::now(),
        last_status_change: Instant::now(),
        is_headless: true,
        output_tail: None,
        todo_progress: None,
        todo_items: Vec::new(),
        runtime: crate::protocol::SwarmMemberRuntime::default(),
        task_label: Some("retention test".to_string()),
    };
    let loaded_at = 10_000_000;
    let mut persisted = to_persisted_member(&member, loaded_at);
    persisted.terminal_since_unix_ms = Some(loaded_at - 30_000);

    let recent = from_persisted_member(
        persisted.clone(),
        loaded_at,
        loaded_at,
        Duration::from_secs(60),
    )
    .expect("recent terminal member remains inspectable");
    assert_eq!(
        recent.latest_completion_report.as_deref(),
        Some("All targeted tests passed.")
    );
    assert!(recent.last_status_change.elapsed() >= Duration::from_secs(30));

    assert!(
        from_persisted_member(persisted, loaded_at, loaded_at, Duration::from_secs(10),).is_none(),
        "expired terminal member should be pruned"
    );
}

#[test]
fn legacy_terminal_member_uses_snapshot_time_as_retention_fallback() {
    let (event_tx, _event_rx) = tokio::sync::mpsc::unbounded_channel();
    let member = SwarmMember {
        session_id: "session-legacy-terminal".to_string(),
        event_tx,
        event_txs: HashMap::new(),
        working_dir: None,
        status: SwarmLifecycleStatus::Failed,
        detail: Some("old failure".to_string()),
        friendly_name: Some("badger".to_string()),
        report_back_to_session_id: None,
        latest_completion_report: Some("legacy report".to_string()),
        joined_at: Instant::now(),
        last_status_change: Instant::now(),
        is_headless: true,
        output_tail: None,
        todo_progress: None,
        todo_items: Vec::new(),
        runtime: crate::protocol::SwarmMemberRuntime::default(),
        task_label: None,
    };
    let loaded_at = 20_000_000;
    let mut persisted = to_persisted_member(&member, loaded_at);
    persisted.terminal_since_unix_ms = None;

    assert!(
        from_persisted_member(
            persisted,
            loaded_at - 20_000,
            loaded_at,
            Duration::from_secs(10),
        )
        .is_none(),
        "legacy records should age from their containing snapshot"
    );
}

#[test]
fn recovery_induced_terminal_status_starts_retention_at_load_time() {
    let (event_tx, _event_rx) = tokio::sync::mpsc::unbounded_channel();
    let member = SwarmMember {
        session_id: "session-ready-recovery".to_string(),
        event_tx,
        event_txs: HashMap::new(),
        working_dir: None,
        status: SwarmLifecycleStatus::Ready,
        detail: None,
        friendly_name: Some("hare".to_string()),
        report_back_to_session_id: None,
        latest_completion_report: Some("finished just before restart".to_string()),
        joined_at: Instant::now(),
        last_status_change: Instant::now(),
        is_headless: true,
        output_tail: None,
        todo_progress: None,
        todo_items: Vec::new(),
        runtime: crate::protocol::SwarmMemberRuntime::default(),
        task_label: None,
    };
    let loaded_at = 300_000_000;
    let mut persisted = to_persisted_member(&member, loaded_at);
    persisted.terminal_since_unix_ms = None;

    let recovered = from_persisted_member(
        persisted,
        loaded_at - Duration::from_secs(48 * 60 * 60).as_millis() as u64,
        loaded_at,
        Duration::from_secs(24 * 60 * 60),
    )
    .expect("recovery-induced terminal status should receive a fresh retention window");
    assert_eq!(recovered.status, SwarmLifecycleStatus::Stopped);
    assert!(recovered.last_status_change.elapsed() < Duration::from_secs(1));
}

#[test]
fn startup_gc_removes_expired_terminal_members_from_durable_snapshot() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let _env = test_env(&dir);
    let (event_tx, _event_rx) = tokio::sync::mpsc::unbounded_channel();
    let members = vec![SwarmMember {
        session_id: "session-expired".to_string(),
        event_tx,
        event_txs: HashMap::new(),
        working_dir: None,
        status: SwarmLifecycleStatus::Completed,
        detail: None,
        friendly_name: Some("fox".to_string()),
        report_back_to_session_id: None,
        latest_completion_report: Some("report retained until expiry".to_string()),
        joined_at: Instant::now(),
        last_status_change: Instant::now(),
        is_headless: true,
        output_tail: None,
        todo_progress: None,
        todo_items: Vec::new(),
        runtime: crate::protocol::SwarmMemberRuntime::default(),
        task_label: None,
    }];
    persist_swarm_state("swarm-expired", &members);

    let path = state_path("swarm-expired");
    let mut persisted = storage::read_json::<PersistedSwarmState>(&path).expect("snapshot");
    persisted.members[0].terminal_since_unix_ms =
        Some(now_unix_ms().saturating_sub(Duration::from_secs(48 * 60 * 60).as_millis() as u64));
    storage::write_json_fast(&path, &persisted).expect("age terminal member");

    let loaded = load_runtime_state();
    assert!(!loaded.members.contains_key("session-expired"));
    assert!(
        !path.exists(),
        "empty snapshot should be deleted after startup collection"
    );
}

#[test]
fn remove_swarm_state_deletes_persisted_snapshot() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let _env = test_env(&dir);

    // Members are what the snapshot holds, so a swarm with a member is what
    // leaves a file behind.
    persist_swarm_state("swarm-beta", &[persisted_member("session-beta")]);
    assert!(state_path("swarm-beta").exists());

    remove_swarm_state("swarm-beta");
    assert!(!state_path("swarm-beta").exists());
}

#[test]
fn load_migrates_legacy_runtime_dir_state() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let _env = test_env(&dir);

    // A snapshot from the legacy runtime dir, still carrying the old
    // coordinator key a previous release wrote.
    let legacy = serde_json::json!({
        "swarm_id": "swarm-migrate",
        "coordinator_session_id": "coord-legacy",
        "members": [persisted_member_json("session-legacy")],
        "updated_at_unix_ms": 1u64
    });
    std::fs::create_dir_all(legacy_state_dir()).expect("legacy state dir");
    std::fs::write(
        legacy_state_dir().join("swarm-migrate.json"),
        serde_json::to_vec(&legacy).unwrap(),
    )
    .expect("write legacy snapshot");

    let loaded = load_runtime_state();
    assert!(
        loaded.members.contains_key("session-legacy"),
        "the migrated snapshot's members should be restored"
    );
    // Migrated copy lives in the durable dir now.
    assert!(state_path("swarm-migrate").exists());
}

#[test]
fn migration_does_not_clobber_existing_durable_state() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let _env = test_env(&dir);

    // Durable dir already has state for this swarm.
    persist_swarm_state("swarm-both", &[persisted_member("session-new")]);

    // Legacy dir has a stale snapshot for the same swarm.
    let legacy = serde_json::json!({
        "swarm_id": "swarm-both",
        "coordinator_session_id": "coord-old",
        "members": [persisted_member_json("session-old")],
        "updated_at_unix_ms": 1u64
    });
    std::fs::create_dir_all(legacy_state_dir()).expect("legacy state dir");
    std::fs::write(
        legacy_state_dir().join("swarm-both.json"),
        serde_json::to_vec(&legacy).unwrap(),
    )
    .expect("write legacy snapshot");

    let loaded = load_runtime_state();
    assert!(
        loaded.members.contains_key("session-new"),
        "the durable snapshot must survive a legacy copy of the same swarm"
    );
    assert!(
        !loaded.members.contains_key("session-old"),
        "the stale legacy copy must not be merged in"
    );
}

#[test]
fn state_dir_is_durable_not_runtime() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let _env = test_env(&dir);

    // With KCODE_RUNTIME_DIR pinned, the state dir stays sandboxed but must
    // not be the legacy runtime-dir location.
    assert_ne!(state_dir(), legacy_state_dir());
    assert!(state_dir().starts_with(dir.path()));
}

#[tokio::test]
async fn persistence_operations_serialize_per_swarm_but_not_globally() {
    let alpha = swarm_operation_lock("swarm-lock-alpha");
    let same_alpha = swarm_operation_lock("swarm-lock-alpha");
    let beta = swarm_operation_lock("swarm-lock-beta");
    assert!(
        Arc::ptr_eq(&alpha, &same_alpha),
        "the same swarm must share one operation lock"
    );
    assert!(
        !Arc::ptr_eq(&alpha, &beta),
        "unrelated swarms must not share a global serialization lock"
    );

    let alpha_guard = alpha.lock().await;
    assert!(
        tokio::time::timeout(Duration::from_millis(20), same_alpha.lock())
            .await
            .is_err(),
        "a second operation for the same swarm must wait"
    );
    let _beta_guard = tokio::time::timeout(Duration::from_millis(100), beta.lock())
        .await
        .expect("an unrelated swarm operation was unnecessarily blocked");
    drop(alpha_guard);
}

/// A `.bak` sibling is a corruption fallback, not a co-equal snapshot: the loader
/// ingests one only when the matching `.json` is absent. This pins that a `.bak`
/// with no primary at all is still loaded, so a backup is the sole copy when the
/// primary is gone.
#[test]
fn load_runtime_state_reads_bak_files_as_snapshots() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let _env = test_env(&dir);

    let snapshot = serde_json::json!({
        "swarm_id": "swarm-bak-only",
        "coordinator_session_id": "coord-from-bak",
        "members": [persisted_member_json("session-from-bak")],
        "updated_at_unix_ms": 1u64
    });
    std::fs::create_dir_all(state_dir()).expect("state dir");
    std::fs::write(
        state_dir().join("swarm-bak-only.bak"),
        serde_json::to_vec(&snapshot).unwrap(),
    )
    .expect("write bak snapshot");

    let loaded = load_runtime_state();
    assert!(
        loaded.members.contains_key("session-from-bak"),
        "a .bak with no primary is the only copy and must still load"
    );
}

/// A `.bak` sibling must NOT be loaded when the primary `.json` exists: the write
/// path rotates the previous snapshot to `.bak`, so after startup GC prunes an
/// expired terminal member the `.bak` still holds it. Union-loading both would
/// resurrect every pruned member on every server restart.
#[test]
fn load_runtime_state_ignores_bak_when_primary_json_exists() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let _env = test_env(&dir);

    std::fs::create_dir_all(state_dir()).expect("state dir");
    let stale_with_dropped_member = serde_json::json!({
        "swarm_id": "swarm-cleared",
        "members": [persisted_member_json("session-stale")],
        // Keys an older format wrote: a snapshot must keep loading, because the
        // file is not a durable artifact this server owns alone.
        "plan": {
            "items": [{
                "id": "stale-task",
                "content": "stale",
                "status": "queued",
                "assigned_to": null
            }],
            "version": 42u64,
            "mode": "light",
        },
        "coordinator_session_id": "coord-stale",
        "updated_at_unix_ms": 1u64
    });
    let current = serde_json::json!({
        "swarm_id": "swarm-cleared",
        "members": [persisted_member_json("session-current")],
        "coordinator_session_id": "coord-current",
        "updated_at_unix_ms": 2u64
    });
    std::fs::write(
        state_dir().join("swarm-cleared.bak"),
        serde_json::to_vec(&stale_with_dropped_member).unwrap(),
    )
    .expect("write bak snapshot");
    std::fs::write(
        state_dir().join("swarm-cleared.json"),
        serde_json::to_vec(&current).unwrap(),
    )
    .expect("write primary snapshot");

    let loaded = load_runtime_state();
    assert!(
        loaded.members.contains_key("session-current"),
        "the primary snapshot's members must be restored"
    );
    assert!(
        !loaded.members.contains_key("session-stale"),
        "the primary snapshot must win over its .bak sibling"
    );
}

#[test]
fn persisted_swarm_state_without_plan_still_restores_members() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let _env = test_env(&dir);

    let (event_tx, _event_rx) = tokio::sync::mpsc::unbounded_channel();
    let members = vec![SwarmMember {
        session_id: "coord-1".to_string(),
        event_tx,
        event_txs: HashMap::new(),
        working_dir: Some(PathBuf::from("/tmp/swarm-gamma")),
        status: SwarmLifecycleStatus::Ready,
        detail: None,
        friendly_name: Some("owl".to_string()),
        report_back_to_session_id: None,
        latest_completion_report: None,
        joined_at: Instant::now(),
        last_status_change: Instant::now(),
        is_headless: false,
        output_tail: None,
        todo_progress: None,
        todo_items: Vec::new(),
        runtime: crate::protocol::SwarmMemberRuntime::default(),
        task_label: None,
    }];

    persist_swarm_state("swarm-gamma", &members);

    let loaded = load_runtime_state();
    assert_eq!(
        loaded
            .members
            .get("coord-1")
            .and_then(|member| member.friendly_name.as_deref()),
        Some("owl")
    );
    // A session that reports back to nobody roots its own run, so the restored
    // snapshot derives that membership instead of storing a swarm id.
    assert_eq!(
        crate::server::swarm::swarm_root(&loaded.members, "coord-1").as_deref(),
        Some("coord-1")
    );
}

#[test]
fn remove_swarm_state_removes_backup_and_cannot_resurrect() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let _env = test_env(&dir);

    // First persist creates the primary; the second overwrite makes
    // write_json_fast hard-link the previous snapshot to `.bak`.
    persist_swarm_state("swarm-zombie", &[persisted_member("session-zombie")]);
    persist_swarm_state("swarm-zombie", &[persisted_member("session-zombie")]);
    let bak_path = state_path("swarm-zombie").with_extension("bak");
    assert!(bak_path.exists(), "write_json_fast leaves a .bak hard link");

    remove_swarm_state("swarm-zombie");
    assert!(!state_path("swarm-zombie").exists());
    assert!(
        !bak_path.exists(),
        "logical deletion must remove the recovery backup too"
    );

    let loaded = load_runtime_state();
    assert!(
        !loaded.members.contains_key("session-zombie"),
        "a deleted swarm's members must not be restored on the next load"
    );
}

#[test]
fn empty_persist_dissolution_removes_backup_and_cannot_resurrect() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let _env = test_env(&dir);

    persist_swarm_state("swarm-dissolve", &[persisted_member("session-dissolve")]);
    persist_swarm_state("swarm-dissolve", &[persisted_member("session-dissolve")]);
    let bak_path = state_path("swarm-dissolve").with_extension("bak");
    assert!(bak_path.exists(), "write_json_fast leaves a .bak hard link");

    // Dissolution: no members hits the remove_file branch instead of writing a
    // snapshot.
    persist_swarm_state("swarm-dissolve", &[]);
    assert!(!state_path("swarm-dissolve").exists());
    assert!(
        !bak_path.exists(),
        "empty-state persistence must remove the recovery backup too"
    );

    let loaded = load_runtime_state();
    assert!(
        !loaded.members.contains_key("session-dissolve"),
        "a dissolved swarm must not be restored on the next load"
    );
}
