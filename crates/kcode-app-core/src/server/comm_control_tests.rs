use crate::protocol::ServerEvent;
use crate::server::comm_await::{CommAwaitMembersContext, handle_comm_await_members};
use crate::server::{AwaitMembersRuntime, SwarmEvent, SwarmEventType, SwarmMember};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::{RwLock, broadcast, mpsc};

struct RuntimeEnvGuard {
    _guard: std::sync::MutexGuard<'static, ()>,
    prev_runtime: Option<std::ffi::OsString>,
}

impl RuntimeEnvGuard {
    fn new() -> (Self, tempfile::TempDir) {
        let guard = crate::storage::lock_test_env();
        let temp = tempfile::TempDir::new().expect("create runtime dir");
        let prev_runtime = std::env::var_os("KCODE_RUNTIME_DIR");
        crate::env::set_var("KCODE_RUNTIME_DIR", temp.path());
        (
            Self {
                _guard: guard,
                prev_runtime,
            },
            temp,
        )
    }
}

impl Drop for RuntimeEnvGuard {
    fn drop(&mut self) {
        if let Some(prev_runtime) = self.prev_runtime.take() {
            crate::env::set_var("KCODE_RUNTIME_DIR", prev_runtime);
        } else {
            crate::env::remove_var("KCODE_RUNTIME_DIR");
        }
    }
}

/// A member of the run rooted at `root`: the root reports back to nobody, which
/// is what makes it the run's coordinator, and every other session reports back
/// to the root, which is the only membership edge there is.
fn member(session_id: &str, root: &str, status: &str) -> SwarmMember {
    let (event_tx, _event_rx) = mpsc::unbounded_channel();
    SwarmMember {
        session_id: session_id.to_string(),
        event_tx,
        event_txs: HashMap::new(),
        working_dir: None,
        status: status.into(),
        detail: None,
        friendly_name: Some(session_id.to_string()),
        report_back_to_session_id: (session_id != root).then(|| root.to_string()),
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

fn swarm_event(session_id: &str, swarm_id: &str, event: SwarmEventType) -> SwarmEvent {
    SwarmEvent {
        id: 1,
        session_id: session_id.to_string(),
        session_name: Some(session_id.to_string()),
        swarm_id: Some(swarm_id.to_string()),
        event,
        timestamp: Instant::now(),
        absolute_time: SystemTime::now(),
    }
}

use crate::protocol::SwarmLifecycleStatus;
include!("comm_control_tests/await_late_joiners.rs");
include!("comm_control_tests/await_disconnect.rs");
include!("comm_control_tests/await_any.rs");
include!("comm_control_tests/await_reload_deadline.rs");
include!("comm_control_tests/await_reload_final.rs");
include!("comm_control_tests/await_lagged.rs");
include!("comm_control_tests/await_resume_expired.rs");
include!("comm_control_tests/await_background_expired.rs");
include!("comm_control_tests/await_upgrade_background.rs");
