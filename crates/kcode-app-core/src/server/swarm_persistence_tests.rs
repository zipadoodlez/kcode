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

/// The smallest member a snapshot can hold: a session, its swarm, and nothing else.
fn persisted_member(session_id: &str, swarm_id: &str) -> SwarmMember {
    let (event_tx, _event_rx) = tokio::sync::mpsc::unbounded_channel();
    SwarmMember {
        session_id: session_id.to_string(),
        event_tx,
        event_txs: HashMap::new(),
        working_dir: None,
        swarm_id: Some(swarm_id.to_string()),
        swarm_enabled: true,
        status: SwarmLifecycleStatus::Ready,
        detail: None,
        friendly_name: None,
        report_back_to_session_id: None,
        latest_completion_report: None,
        role: "agent".to_string(),
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

#[test]
fn persisted_swarm_state_round_trips() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let _env = test_env(&dir);

    let coordinators = HashMap::from([("swarm-alpha".to_string(), "session-2".to_string())]);
    let (event_tx, _event_rx) = tokio::sync::mpsc::unbounded_channel();
    let members = vec![SwarmMember {
        session_id: "session-1".to_string(),
        event_tx,
        event_txs: HashMap::new(),
        working_dir: Some(PathBuf::from("/tmp/swarm-alpha")),
        swarm_id: Some("swarm-alpha".to_string()),
        swarm_enabled: true,
        status: SwarmLifecycleStatus::Running,
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
        runtime: crate::protocol::SwarmMemberRuntime::default(),
        task_label: None,
    }];

    persist_swarm_state(
        "swarm-alpha",
        coordinators.get("swarm-alpha").map(String::as_str),
        &members,
    );
    let loaded = load_runtime_state();

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
    assert_eq!(recovered_member.status, SwarmLifecycleStatus::Crashed);
    assert_eq!(
        recovered_member.detail.as_deref(),
        Some("writing tests (recovered after reload while running)")
    );
    assert_eq!(
        loaded.swarms_by_id.get("swarm-alpha"),
        Some(&HashSet::from(["session-1".to_string()]))
    );
}

/// A plan is not durable, so a loaded swarm rebuilds it from the list: the open rows
/// its members hold come back queued, and the run's own state starts fresh.
#[test]
fn a_loaded_swarm_seats_the_rows_its_members_hold() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let _env = test_env(&dir);
    let repo = dir.path().join("repo");
    std::fs::create_dir_all(&repo).expect("repo dir");
    let row = |id: &str, holder: Option<&str>| crate::plan::TaskItem {
        content: format!("task {id}"),
        status: "pending".to_string(),
        priority: "medium".to_string(),
        id: id.to_string(),
        assigned_to: holder.map(str::to_string),
        ..Default::default()
    };
    crate::todo::save_tasks(
        Some(&repo),
        "session-1",
        &[
            row("mine", Some("session-1")),
            row("theirs", Some("session-9")),
            row("free", None),
        ],
    )
    .expect("write list");

    let (event_tx, _event_rx) = tokio::sync::mpsc::unbounded_channel();
    let members = vec![SwarmMember {
        session_id: "session-1".to_string(),
        event_tx,
        event_txs: HashMap::new(),
        working_dir: Some(repo.clone()),
        swarm_id: Some("swarm-alpha".to_string()),
        swarm_enabled: true,
        status: SwarmLifecycleStatus::Ready,
        detail: None,
        friendly_name: None,
        report_back_to_session_id: None,
        latest_completion_report: None,
        role: "agent".to_string(),
        joined_at: Instant::now(),
        last_status_change: Instant::now(),
        is_headless: false,
        output_tail: None,
        todo_progress: None,
        todo_items: Vec::new(),
        runtime: crate::protocol::SwarmMemberRuntime::default(),
        task_label: None,
    }];
    persist_swarm_state("swarm-alpha", None, &members);

    let loaded = load_runtime_state();
    let plan = loaded.plans.get("swarm-alpha").expect("hydrated plan");
    assert_eq!(
        plan.items
            .iter()
            .map(|item| (item.id.as_str(), item.status.as_str()))
            .collect::<Vec<_>>(),
        vec![("mine", "queued")],
        "the rows the swarm holds, queued again"
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
        swarm_id: Some("swarm-gamma".to_string()),
        swarm_enabled: true,
        status: SwarmLifecycleStatus::Ready,
        detail: None,
        friendly_name: Some("pig".to_string()),
        report_back_to_session_id: Some("session-coordinator".to_string()),
        latest_completion_report: Some("Done. Built the worker; all tests pass.".to_string()),
        role: "agent".to_string(),
        joined_at: Instant::now(),
        last_status_change: Instant::now(),
        is_headless: true,
        output_tail: None,
        todo_progress: None,
        todo_items: Vec::new(),
        runtime: crate::protocol::SwarmMemberRuntime::default(),
        task_label: None,
    }];

    persist_swarm_state("swarm-gamma", None, &members);
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
        swarm_id: Some("swarm-client".to_string()),
        swarm_enabled: true,
        status: SwarmLifecycleStatus::Ready,
        detail: None,
        friendly_name: Some("finch".to_string()),
        report_back_to_session_id: None,
        latest_completion_report: None,
        role: "agent".to_string(),
        joined_at: Instant::now(),
        last_status_change: Instant::now(),
        is_headless: false,
        output_tail: None,
        todo_progress: None,
        todo_items: Vec::new(),
        runtime: crate::protocol::SwarmMemberRuntime::default(),
        task_label: None,
    }];

    persist_swarm_state("swarm-client", None, &members);
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
        swarm_id: Some("swarm-terminal".to_string()),
        swarm_enabled: true,
        status: SwarmLifecycleStatus::Completed,
        detail: Some("done".to_string()),
        friendly_name: Some("otter".to_string()),
        report_back_to_session_id: Some("session-coordinator".to_string()),
        latest_completion_report: Some("All targeted tests passed.".to_string()),
        role: "agent".to_string(),
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
        swarm_id: Some("swarm-legacy".to_string()),
        swarm_enabled: true,
        status: SwarmLifecycleStatus::Failed,
        detail: Some("old failure".to_string()),
        friendly_name: Some("badger".to_string()),
        report_back_to_session_id: None,
        latest_completion_report: Some("legacy report".to_string()),
        role: "agent".to_string(),
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
        swarm_id: Some("swarm-recovery".to_string()),
        swarm_enabled: true,
        status: SwarmLifecycleStatus::Ready,
        detail: None,
        friendly_name: Some("hare".to_string()),
        report_back_to_session_id: None,
        latest_completion_report: Some("finished just before restart".to_string()),
        role: "agent".to_string(),
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
        swarm_id: Some("swarm-expired".to_string()),
        swarm_enabled: true,
        status: SwarmLifecycleStatus::Completed,
        detail: None,
        friendly_name: Some("fox".to_string()),
        report_back_to_session_id: None,
        latest_completion_report: Some("report retained until expiry".to_string()),
        role: "agent".to_string(),
        joined_at: Instant::now(),
        last_status_change: Instant::now(),
        is_headless: true,
        output_tail: None,
        todo_progress: None,
        todo_items: Vec::new(),
        runtime: crate::protocol::SwarmMemberRuntime::default(),
        task_label: None,
    }];
    persist_swarm_state("swarm-expired", None, &members);

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

    // Members and the coordinator are what the snapshot holds, so a swarm with a
    // member is what leaves a file behind.
    persist_swarm_state(
        "swarm-beta",
        Some("session-beta"),
        &[persisted_member("session-beta", "swarm-beta")],
    );
    assert!(state_path("swarm-beta").exists());

    remove_swarm_state("swarm-beta");
    assert!(!state_path("swarm-beta").exists());
}

#[test]
fn load_migrates_legacy_runtime_dir_state() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let _env = test_env(&dir);

    let legacy = serde_json::json!({
        "swarm_id": "swarm-migrate",
        "coordinator_session_id": "coord-legacy",
        "updated_at_unix_ms": 1u64
    });
    std::fs::create_dir_all(legacy_state_dir()).expect("legacy state dir");
    std::fs::write(
        legacy_state_dir().join("swarm-migrate.json"),
        serde_json::to_vec(&legacy).unwrap(),
    )
    .expect("write legacy snapshot");

    let loaded = load_runtime_state();
    assert_eq!(
        loaded.coordinators.get("swarm-migrate"),
        Some(&"coord-legacy".to_string())
    );
    // Migrated copy lives in the durable dir now.
    assert!(state_path("swarm-migrate").exists());
}

#[test]
fn migration_does_not_clobber_existing_durable_state() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let _env = test_env(&dir);

    // Durable dir already has state for this swarm.
    persist_swarm_state("swarm-both", Some("coord-new"), &[]);

    // Legacy dir has a stale snapshot for the same swarm.
    let legacy = serde_json::json!({
        "swarm_id": "swarm-both",
        "coordinator_session_id": "coord-old",
        "updated_at_unix_ms": 1u64
    });
    std::fs::create_dir_all(legacy_state_dir()).expect("legacy state dir");
    std::fs::write(
        legacy_state_dir().join("swarm-both.json"),
        serde_json::to_vec(&legacy).unwrap(),
    )
    .expect("write legacy snapshot");

    let loaded = load_runtime_state();
    assert_eq!(
        loaded.coordinators.get("swarm-both"),
        Some(&"coord-new".to_string())
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

/// Companion finding discovered while writing the regression test above:
/// `load_runtime_state` filters entries only with `path.is_file()`, with no
/// `.json` extension check (unlike `migrate_legacy_state`, which does check).
/// Since `storage::write_json_fast` leaves a `<swarm>.bak` hard link of the
/// PREVIOUS snapshot next to the primary, startup restore parses both files
/// and inserts them into the same maps keyed by `state.swarm_id`, so
/// whichever the directory iterator yields last wins. After a regressed
/// primary (v5) with a newer backup (v6), restart restore is therefore
/// nondeterministic between the two. This test pins the underlying behavior
/// deterministically: a `.bak` file with no primary at all is still loaded
/// as a live snapshot.
#[test]
fn load_runtime_state_reads_bak_files_as_snapshots() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let _env = test_env(&dir);

    let snapshot = serde_json::json!({
        "swarm_id": "swarm-bak-only",
        "coordinator_session_id": "coord-from-bak",
        "updated_at_unix_ms": 1u64
    });
    std::fs::create_dir_all(state_dir()).expect("state dir");
    std::fs::write(
        state_dir().join("swarm-bak-only.bak"),
        serde_json::to_vec(&snapshot).unwrap(),
    )
    .expect("write bak snapshot");

    let loaded = load_runtime_state();
    assert_eq!(
        loaded.coordinators.get("swarm-bak-only"),
        Some(&"coord-from-bak".to_string()),
        "load_runtime_state currently ingests .bak files as snapshots; if \
         this fails the loader gained a .json extension filter (update the \
         wiring audit and the primary-file assertions in \
         stale_persist_cannot_regress_newer_plan_version)"
    );
}

/// A `.bak` sibling must NOT be loaded when the primary `.json` exists:
/// the write path rotates the previous snapshot to `.bak`, so after an
/// intentional state drop (e.g. `swarm:clear_plan`) the `.bak` still holds
/// the dropped plan. Union-loading both would resurrect the cleared plan on
/// every server restart.
#[test]
fn load_runtime_state_ignores_bak_when_primary_json_exists() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let _env = test_env(&dir);

    std::fs::create_dir_all(state_dir()).expect("state dir");
    let stale_with_plan = serde_json::json!({
        "swarm_id": "swarm-cleared",
        "plan": {
            "items": [{
                "id": "stale-task",
                "content": "stale",
                "status": "queued",
                "assigned_to": null
            }],
            "version": 42u64,
            "participants": [],
            "task_progress": {},
            // Keys an older format wrote: a snapshot must keep loading, because the
            // plan is not a durable artifact this server owns alone.
            "mode": "light",
            "node_meta": {}
        },
        "coordinator_session_id": "coord-stale",
        "updated_at_unix_ms": 1u64
    });
    let current_without_plan = serde_json::json!({
        "swarm_id": "swarm-cleared",
        "coordinator_session_id": "coord-current",
        "updated_at_unix_ms": 2u64
    });
    std::fs::write(
        state_dir().join("swarm-cleared.bak"),
        serde_json::to_vec(&stale_with_plan).unwrap(),
    )
    .expect("write bak snapshot");
    std::fs::write(
        state_dir().join("swarm-cleared.json"),
        serde_json::to_vec(&current_without_plan).unwrap(),
    )
    .expect("write primary snapshot");

    let loaded = load_runtime_state();
    assert!(
        !loaded.plans.contains_key("swarm-cleared"),
        "plan cleared from the primary snapshot must not be resurrected from .bak"
    );
    assert_eq!(
        loaded.coordinators.get("swarm-cleared"),
        Some(&"coord-current".to_string()),
        "primary snapshot must win over its .bak sibling"
    );
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
        status: SwarmLifecycleStatus::Ready,
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
        runtime: crate::protocol::SwarmMemberRuntime::default(),
        task_label: None,
    }];

    persist_swarm_state("swarm-gamma", Some("coord-1"), &members);

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

#[test]
fn remove_swarm_state_removes_backup_and_cannot_resurrect() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let _env = test_env(&dir);

    // First persist creates the primary; the second overwrite makes
    // write_json_fast hard-link the previous (coord-v1) snapshot to `.bak`.
    persist_swarm_state("swarm-zombie", Some("coord-v1"), &[]);
    persist_swarm_state("swarm-zombie", Some("coord-v2"), &[]);
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
        !loaded.coordinators.contains_key("swarm-zombie"),
        "a deleted swarm must not be restored on the next load"
    );
}

#[test]
fn empty_persist_dissolution_removes_backup_and_cannot_resurrect() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let _env = test_env(&dir);

    persist_swarm_state("swarm-dissolve", Some("coord-v1"), &[]);
    persist_swarm_state("swarm-dissolve", Some("coord-v2"), &[]);
    let bak_path = state_path("swarm-dissolve").with_extension("bak");
    assert!(bak_path.exists(), "write_json_fast leaves a .bak hard link");

    // Dissolution: no plan, no coordinator, no members hits the
    // remove_file branch instead of writing a snapshot.
    persist_swarm_state("swarm-dissolve", None, &[]);
    assert!(!state_path("swarm-dissolve").exists());
    assert!(
        !bak_path.exists(),
        "empty-state persistence must remove the recovery backup too"
    );

    let loaded = load_runtime_state();
    assert!(
        !loaded.coordinators.contains_key("swarm-dissolve"),
        "a dissolved swarm must not be restored on the next load"
    );
}

/// Delete-vs-write interleaving between `remove_persisted_swarm_state_for`
/// and a concurrent persist (wiring-audit.bak-resurrection, part b).
///
/// `remove_persisted_swarm_state_for` (server.rs:120) is `load_runtime()
/// .await` followed by an unserialized `remove_swarm_state`. Like the
/// persist inversion race above, `load_runtime` observes the four state
/// maps across multiple await points, so a remover that saw an all-empty
/// (dissolved) runtime can park, lose the race to a swarm re-creation plus
/// persist, then resume and delete the FRESH snapshot the re-creation just
/// wrote. Two failures compound:
///   1. Orphaned live swarm: the recreated swarm (coordinator registered
///      in memory) has no primary snapshot, so a clean restart loses it.
///   2. Zombie resurrection: the persist that the remover clobbered
///      hard-linked the PRE-dissolution snapshot to `.bak`, and
///      `load_runtime_state` reads `.bak` files, so restart restores the
///      stale pre-dissolution state instead.
///
/// Same gate technique as
/// `stale_persist_cannot_regress_newer_plan_version`:
/// park A inside `load_runtime` at the contended `members.read()`, run
/// mutator B's re-creation and persist while A is parked, release A.
#[tokio::test]
#[allow(clippy::await_holding_lock)]
async fn stale_remove_cannot_delete_fresh_snapshot_or_restore_backup() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let _env = test_env(&dir);

    // The previous incarnation's snapshot is on disk; the swarm has since
    // been dissolved, so the in-memory runtime is empty.
    persist_swarm_state("swarm-del-race", Some("coord-stale"), &[]);
    let swarm_state = crate::server::SwarmState::new(
        HashMap::new(),
        HashMap::new(),
        HashMap::new(),
        HashMap::new(),
    );

    // Gate: hold members.write() so remover A parks inside load_runtime at
    // the final members.read(), AFTER it has already observed the
    // dissolved (all-empty) plans/coordinators/swarms_by_id state.
    let gate = swarm_state.members.write().await;

    let a = tokio::spawn({
        let swarm_state = swarm_state.clone();
        async move {
            crate::server::remove_persisted_swarm_state_for("swarm-del-race", &swarm_state).await;
        }
    });
    // Current-thread test runtime: yielding runs A until it parks on the
    // contended members.read().await.
    for _ in 0..16 {
        tokio::task::yield_now().await;
    }

    // Mutator B: the swarm is recreated while A is parked. B registers a
    // new coordinator in memory ...
    {
        let mut coordinators = swarm_state.coordinators.write().await;
        coordinators.insert("swarm-del-race".to_string(), "coord-new".to_string());
    }
    // ... and B's persist half runs to completion (in production this is
    // B's own persist_swarm_state_for on another worker thread, whose
    // uncontended lock reads resolve without suspending). This overwrite
    // also hard-links the stale pre-dissolution snapshot to `.bak`.
    persist_swarm_state("swarm-del-race", Some("coord-new"), &[]);
    let on_disk = storage::read_json::<PersistedSwarmState>(&state_path("swarm-del-race"))
        .expect("fresh snapshot");
    assert_eq!(
        on_disk.coordinator_session_id.as_deref(),
        Some("coord-new"),
        "fresh snapshot must be durably on disk before A resumes"
    );

    // Release A: its stale all-empty runtime passes has_any_state(), but the
    // compare-and-delete guard must notice that the durable snapshot changed.
    drop(gate);
    a.await.expect("remove task");

    assert!(
        state_path("swarm-del-race").exists(),
        "a stale remove must not delete a freshly persisted snapshot"
    );
    let loaded = load_runtime_state();
    assert_eq!(
        loaded.coordinators.get("swarm-del-race"),
        Some(&"coord-new".to_string()),
        "restart must restore the fresh incarnation, not its stale backup"
    );
}
