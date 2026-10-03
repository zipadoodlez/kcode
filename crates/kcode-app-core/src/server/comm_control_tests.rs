use crate::agent::Agent;
use crate::message::{Message, StreamEvent, ToolDefinition};
use crate::plan::TaskItem;
use crate::protocol::ServerEvent;
use crate::provider::{EventStream, Provider};
use crate::server::comm_await::{CommAwaitMembersContext, handle_comm_await_members};
use crate::server::{
    AwaitMembersRuntime, RunState, SwarmEvent, SwarmEventType, SwarmMember, SwarmMutationRuntime,
};
use crate::tool::Registry;
use anyhow::Result;
use async_trait::async_trait;
use futures::stream;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::{Mutex, RwLock, broadcast, mpsc};

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

/// A worker in the run rooted at `root`: spawned by `owner`, so it reports back
/// to it. Spawning is root-only, so the owner is the run's root and this edge is
/// the membership: auto-assignment only targets such drivable workers, so test
/// fixtures that model a spawned worker should use this rather than a bare
/// `member(x, x, ..)` (a foreign/independent session, its own root, and
/// intentionally not auto-assignable).
fn owned_member(session_id: &str, root: &str, status: &str, owner: &str) -> SwarmMember {
    let mut m = member(session_id, root, status);
    m.report_back_to_session_id = Some(owner.to_string());
    m
}

/// Put every member of a fixture in `repo`, so the list a dispatch writes is this
/// test's own (rule 1: the rows live where the session lives).

/// Write `rows` as `session_id`'s list, exactly as given, so a fixture's plan has
/// rows behind it: the file is the list, and a plan row with no row is exactly the
/// state the plan is being cut down to. `write_rows` (dag_e2e) is this plus
/// holding every row for the session.

/// A fixture's rows go where the rows live: the list in the repo. The run map starts
/// empty, since a row with no run entry reads its own status.
/// The rows in a fixture's list: the store is where the handlers write, so a test
/// reads its expectations from there.
/// Put every member of a fixture in `repo`, so the list a dispatch writes is this
/// test's own (rule 1: the rows live where the session lives).
async fn set_repo(members: &Arc<RwLock<HashMap<String, SwarmMember>>>, repo: &std::path::Path) {
    for member in members.write().await.values_mut() {
        member.working_dir = Some(repo.to_path_buf());
    }
}

/// Write `rows` as `session_id`'s list, exactly as given, so a fixture's plan has
/// rows behind it: the file is the list, and a plan row with no row is exactly the
/// state the plan is being cut down to. `write_rows` (dag_e2e) is this plus
/// holding every row for the session.
fn write_list(repo: &std::path::Path, session_id: &str, rows: &[TaskItem]) {
    crate::todo::save_tasks(Some(repo), session_id, rows).expect("write the list");
}

/// Rows are the list's, so a fixture's rows are written there and the run map starts
/// empty: a row with no run entry reads its own status.
fn empty_run_state() -> Arc<RwLock<HashMap<String, RunState>>> {
    Arc::new(RwLock::new(HashMap::new()))
}

fn rows_in(repo: &std::path::Path) -> Vec<TaskItem> {
    crate::todo::load_tasks(Some(repo), "fixture").expect("read the list")
}

fn seeded(repo: &std::path::Path, rows: Vec<TaskItem>) -> Arc<RwLock<HashMap<String, RunState>>> {
    write_list(repo, "fixture", &rows);
    empty_run_state()
}

fn plan_item(id: &str, status: &str, priority: &str, blocked_by: &[&str]) -> TaskItem {
    TaskItem {
        content: format!("task {id}"),
        status: status.to_string(),
        priority: priority.to_string(),
        id: id.to_string(),
        blocked_by: blocked_by.iter().map(|value| value.to_string()).collect(),
        ..Default::default()
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

#[derive(Default)]
struct TestProvider;

#[async_trait]
impl Provider for TestProvider {
    async fn complete(
        &self,
        _messages: &[Message],
        _tools: &[ToolDefinition],
        _system: &str,
        _resume_session_id: Option<&str>,
    ) -> Result<EventStream> {
        Ok(Box::pin(stream::iter(vec![Ok(StreamEvent::MessageEnd {
            stop_reason: None,
        })])))
    }

    fn name(&self) -> &str {
        "test"
    }

    fn fork(&self) -> Arc<dyn Provider> {
        Arc::new(Self)
    }
}

async fn test_agent() -> Arc<Mutex<Agent>> {
    let provider: Arc<dyn Provider> = Arc::new(TestProvider);
    let registry = Registry::new(provider.clone()).await;
    Arc::new(Mutex::new(Agent::new(provider, registry)))
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
include!("comm_control_tests/dag_e2e.rs");
