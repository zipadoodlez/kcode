use super::*;
use crate::bus::{
    BusEvent, ClientMaintenanceAction, SessionUpdateStatus, UpdateStatus,
};
use crate::tui::TuiState;
use ratatui::backend::Backend;
use ratatui::layout::Rect;
use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc as StdArc, Mutex as StdMutex};
use std::time::{Duration, Instant};

fn cleanup_background_task_files(task_id: &str) {
    let task_dir = std::env::temp_dir().join("kcode-bg-tasks");
    let _ = std::fs::remove_file(task_dir.join(format!("{}.status.json", task_id)));
    let _ = std::fs::remove_file(task_dir.join(format!("{}.output", task_id)));
}

pub(super) fn cleanup_reload_context_file(session_id: &str) {
    if let Ok(path) = crate::session_recovery::ReloadContext::path_for_session(session_id) {
        let _ = std::fs::remove_file(path);
    }
}

// Mock provider for testing
struct MockProvider;

#[derive(Clone)]
struct NamedMockProvider {
    name: &'static str,
    model: &'static str,
}

#[derive(Clone)]
struct RefreshSummaryProvider {
    summary: crate::provider::ModelCatalogRefreshSummary,
}

#[derive(Clone)]
struct OpenRouterSpecCaptureProvider {
    set_model_calls: StdArc<StdMutex<Vec<String>>>,
}

#[async_trait::async_trait]
impl Provider for MockProvider {
    async fn complete(
        &self,
        _messages: &[Message],
        _tools: &[crate::message::ToolDefinition],
        _system: &str,
        _resume_session_id: Option<&str>,
    ) -> Result<crate::provider::EventStream> {
        unimplemented!("Mock provider")
    }

    fn name(&self) -> &str {
        "mock"
    }

    fn fork(&self) -> Arc<dyn Provider> {
        Arc::new(MockProvider)
    }
}

#[async_trait::async_trait]
impl Provider for NamedMockProvider {
    async fn complete(
        &self,
        _messages: &[Message],
        _tools: &[crate::message::ToolDefinition],
        _system: &str,
        _resume_session_id: Option<&str>,
    ) -> Result<crate::provider::EventStream> {
        unimplemented!("NamedMockProvider")
    }

    fn name(&self) -> &str {
        self.name
    }

    fn model(&self) -> String {
        self.model.to_string()
    }

    fn fork(&self) -> Arc<dyn Provider> {
        Arc::new(self.clone())
    }
}

#[async_trait::async_trait]
impl Provider for RefreshSummaryProvider {
    async fn complete(
        &self,
        _messages: &[Message],
        _tools: &[crate::message::ToolDefinition],
        _system: &str,
        _resume_session_id: Option<&str>,
    ) -> Result<crate::provider::EventStream> {
        unimplemented!("RefreshSummaryProvider")
    }

    fn name(&self) -> &str {
        "refresh-summary"
    }

    fn fork(&self) -> Arc<dyn Provider> {
        Arc::new(self.clone())
    }

    async fn refresh_model_catalog(&self) -> Result<crate::provider::ModelCatalogRefreshSummary> {
        Ok(self.summary.clone())
    }
}

#[async_trait::async_trait]
impl Provider for OpenRouterSpecCaptureProvider {
    async fn complete(
        &self,
        _messages: &[Message],
        _tools: &[crate::message::ToolDefinition],
        _system: &str,
        _resume_session_id: Option<&str>,
    ) -> Result<crate::provider::EventStream> {
        unimplemented!("OpenRouterSpecCaptureProvider")
    }

    fn name(&self) -> &str {
        "openrouter-spec-capture"
    }

    fn model(&self) -> String {
        "gpt-5.4".to_string()
    }

    fn model_routes(&self) -> Vec<crate::provider::ModelRoute> {
        vec![crate::provider::ModelRoute {
            model: "gpt-5.4".to_string(),
            provider: "OpenAI".to_string(),
            api_method: "openrouter".to_string(),
            available: true,
            detail: "cached route".to_string(),
            usage: None,
            cheapness: None,
        }]
    }

    fn available_providers_for_model(&self, model: &str) -> Vec<String> {
        if model == "gpt-5.4" || model == "openai/gpt-5.4" {
            vec!["auto".to_string(), "OpenAI".to_string()]
        } else {
            Vec::new()
        }
    }

    fn available_efforts(&self) -> Vec<&'static str> {
        vec!["high"]
    }

    fn reasoning_effort(&self) -> Option<String> {
        Some("high".to_string())
    }

    fn set_reasoning_effort(&self, _effort: &str) -> Result<()> {
        Ok(())
    }

    fn set_model(&self, model: &str) -> Result<()> {
        self.set_model_calls.lock().unwrap().push(model.to_string());
        Ok(())
    }

    fn fork(&self) -> Arc<dyn Provider> {
        Arc::new(self.clone())
    }
}

pub(crate) fn create_test_app() -> App {
    clear_persisted_test_ui_state();
    // `clear_test_render_state_for_tests` wipes process-global render state
    // (flicker history, layout snapshots, copy targets) and internally takes
    // the shared render-state lock unless this thread already holds it. Do
    // not take `render_state_test_lock()` explicitly here: the mutex is not
    // reentrant, so tests that hold the lock and then build an app (e.g. the
    // pinned-todo-band render test) would self-deadlock, which hung the CI
    // TUI test step at its 35-minute job timeout.
    crate::tui::ui::clear_test_render_state_for_tests();

    let provider: Arc<dyn Provider> = Arc::new(MockProvider);
    let rt = tokio::runtime::Runtime::new().unwrap();
    let registry = rt.block_on(crate::tool::Registry::new(provider.clone()));
    let mut app = App::new_for_test_harness(provider, registry);
    app.queue_mode = false;
    app.diff_mode = crate::config::DiffDisplayMode::Inline;
    app
}

fn create_named_provider_test_app(name: &'static str, model: &'static str) -> App {
    clear_persisted_test_ui_state();
    crate::tui::ui::clear_test_render_state_for_tests();

    let provider: Arc<dyn Provider> = Arc::new(NamedMockProvider { name, model });
    let rt = tokio::runtime::Runtime::new().unwrap();
    let registry = rt.block_on(crate::tool::Registry::new(provider.clone()));
    let mut app = App::new_for_test_harness(provider, registry);
    app.queue_mode = false;
    app.diff_mode = crate::config::DiffDisplayMode::Inline;
    app
}

fn wait_for_model_picker_load(app: &mut App) {
    let start = Instant::now();
    while app.model_picker.pending.is_some() {
        app.poll_model_picker_load();
        assert!(
            start.elapsed() < Duration::from_secs(2),
            "timed out waiting for async model picker load"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn create_refresh_summary_test_app(summary: crate::provider::ModelCatalogRefreshSummary) -> App {
    clear_persisted_test_ui_state();
    crate::tui::ui::clear_test_render_state_for_tests();

    let provider: Arc<dyn Provider> = Arc::new(RefreshSummaryProvider { summary });
    let rt = tokio::runtime::Runtime::new().unwrap();
    let registry = rt.block_on(crate::tool::Registry::new(provider.clone()));
    let mut app = App::new_for_test_harness(provider, registry);
    app.queue_mode = false;
    app.diff_mode = crate::config::DiffDisplayMode::Inline;
    app
}

fn create_openrouter_spec_capture_test_app() -> (App, StdArc<StdMutex<Vec<String>>>) {
    clear_persisted_test_ui_state();
    crate::tui::ui::clear_test_render_state_for_tests();

    let set_model_calls = StdArc::new(StdMutex::new(Vec::new()));
    let provider: Arc<dyn Provider> = Arc::new(OpenRouterSpecCaptureProvider {
        set_model_calls: set_model_calls.clone(),
    });
    let rt = tokio::runtime::Runtime::new().unwrap();
    let registry = rt.block_on(crate::tool::Registry::new(provider.clone()));
    let mut app = App::new_for_test_harness(provider, registry);
    app.queue_mode = false;
    app.diff_mode = crate::config::DiffDisplayMode::Inline;
    (app, set_model_calls)
}

#[test]
fn local_add_provider_message_does_not_retain_local_provider_copy() {
    let mut app = create_test_app();
    app.add_provider_message(Message::user("hello"));
    assert!(app.messages.is_empty());
}

#[test]
fn remote_add_provider_message_retains_remote_provider_copy() {
    let mut app = create_test_app();
    app.set_runtime_mode(crate::tui::app::AppRuntimeMode::RemoteClient);
    app.ensure_provider_messages_hydrated();
    let before = app.messages.len();
    app.add_provider_message(Message::user("hello"));
    assert_eq!(app.messages.len(), before + 1);
}

fn test_side_panel_snapshot(page_id: &str, title: &str) -> crate::side_panel::SidePanelSnapshot {
    crate::side_panel::SidePanelSnapshot {
        focus_revision: 0,
        focused_page_id: Some(page_id.to_string()),
        pages: vec![crate::side_panel::SidePanelPage {
            id: page_id.to_string(),
            title: title.to_string(),
            file_path: format!("/tmp/{page_id}.md"),
            format: crate::side_panel::SidePanelPageFormat::Markdown,
            pdf_data: None,
            source: crate::side_panel::SidePanelPageSource::Managed,
            content: format!("# {title}"),
            updated_at_ms: 1,
        }],
    }
}

/// Point `KCODE_HOME` at a per-process scratch directory if nothing set one.
///
/// This runs from `create_test_app`, so roughly 570 tests call it, and it
/// mutates a process-global that Rust's parallel test threads all share. Tests
/// that scope their own `KCODE_HOME` restore it by *removing* the variable, so
/// without serialization this function would observe the gap and repoint
/// `KCODE_HOME` at the shared scratch home while that test was still running.
/// That is the race behind kcode-tui's intermittent failures in unrelated
/// ambient, header, and model-picker tests, which all read files under
/// `KCODE_HOME` mid-assertion.
///
/// Taking the same lock those tests use makes the check-then-set atomic with
/// respect to them. The lock is released on return, which is correct: it only
/// needs to cover this read-modify-write, not the caller's whole test.
///
/// The lock is acquired with `try_lock`, never blocking: tests like
/// `with_temp_kcode_home` hold this same non-reentrant mutex for their whole
/// body and may call `create_*_test_app` inside it, so a blocking lock here
/// self-deadlocks (this hung CI's TUI test step at the job timeout). When
/// `try_lock` fails because this thread holds the lock, the caller's own
/// exclusion already covers the transition; a cross-thread `try_lock` miss
/// falls back to the pre-serialization benign race for that one call.
/// The shared per-process test home that `create_test_app` installs when no
/// test has scoped its own `KCODE_HOME`.
fn shared_test_kcode_home() -> &'static std::path::Path {
    static TEST_HOME: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
    TEST_HOME.get_or_init(|| {
        let path = std::env::temp_dir().join(format!("kcode-test-home-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&path);
        path
    })
}

/// `kcode-base`'s `test-support` install must have run before any test here;
/// without it `KCODE_HOME` falls back to the developer's real `~/.kcode`. Pins
/// the dev-feature dependency so dropping it fails loudly here.
#[test]
fn the_process_home_is_a_temp_dir() {
    let home = std::env::var_os("KCODE_HOME")
        .expect("kcode-base's test-support install must set KCODE_HOME");
    assert!(
        std::path::Path::new(&home).starts_with(std::env::temp_dir()),
        "KCODE_HOME = {home:?}, outside the temp dir"
    );
}

fn clear_persisted_test_ui_state() {
    // Only wipe ambient files in the *shared* per-process test home, and only
    // while `KCODE_HOME` still points at it. `create_test_app` runs from ~800
    // sites without the env lock, so following whatever `KCODE_HOME` happens
    // to be set to deleted the ambient queue of a concurrently running test
    // that scoped its own temporary home (#1142). A scoped home starts from an
    // empty tempdir and has no stale ambient state to clear anyway.
    let home_is_shared = std::env::var_os("KCODE_HOME")
        .is_some_and(|home| std::path::Path::new(&home) == shared_test_kcode_home());
    if home_is_shared {
        let ambient_dir = shared_test_kcode_home().join("ambient");
        let _ = std::fs::remove_file(ambient_dir.join("queue.json"));
        let _ = std::fs::remove_file(ambient_dir.join("state.json"));
        let _ = std::fs::remove_file(ambient_dir.join("directives.json"));
        let _ = std::fs::remove_file(ambient_dir.join("visible_cycle.json"));
    }
    crate::auth::AuthStatus::invalidate_cache();
}

fn with_temp_kcode_home<T>(f: impl FnOnce() -> T) -> T {
    let _guard = crate::storage::lock_test_env();
    let temp = tempfile::tempdir().expect("tempdir");
    let prev_home = std::env::var_os("KCODE_HOME");
    crate::env::set_var("KCODE_HOME", temp.path());
    crate::auth::claude::set_active_account_override(None);
    crate::auth::codex::set_active_account_override(None);
    crate::auth::AuthStatus::invalidate_cache();
    // The config cache is keyed by content, not by home, so a config written
    // under the previous home would otherwise be read inside this one.
    crate::config::invalidate_config_cache();
    clear_persisted_test_ui_state();

    let result = f();

    crate::auth::claude::set_active_account_override(None);
    crate::auth::codex::set_active_account_override(None);
    crate::auth::AuthStatus::invalidate_cache();
    if let Some(prev_home) = prev_home {
        crate::env::set_var("KCODE_HOME", prev_home);
    } else {
        crate::env::remove_var("KCODE_HOME");
    }
    // Drop any config loaded from the temp home so it cannot leak into the next
    // test, which is process-global state shared across this suite.
    crate::config::invalidate_config_cache();
    result
}

/// Run `f` in a hermetic `KCODE_HOME` with reasoning display pinned to
/// `current`.
///
/// The reasoning-region tests assert live-then-anchored ("current") behaviour, but
/// the *default* display mode became `Off` when `show_thinking` was defaulted off
/// for new users (166e4444f). A temp home alone therefore no longer produces the
/// mode these tests describe: it produces the new default. Pin the mode
/// explicitly so the tests exercise the behaviour they document instead of
/// silently following a config default they do not control.
fn with_reasoning_current_home<T>(f: impl FnOnce() -> T) -> T {
    with_temp_kcode_home(|| {
        crate::config::Config::set_reasoning_display(
            crate::config::ReasoningDisplayMode::Current,
        )
        .expect("pin reasoning display to current for the test config");
        crate::config::invalidate_config_cache();
        f()
    })
}

fn create_real_git_repo_fixture() -> tempfile::TempDir {
    let temp = tempfile::tempdir().expect("tempdir");
    std::process::Command::new("git")
        .args(["init"])
        .current_dir(temp.path())
        .output()
        .expect("git init");
    std::process::Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(temp.path())
        .output()
        .expect("git config email");
    std::process::Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(temp.path())
        .output()
        .expect("git config name");
    std::fs::write(temp.path().join("tracked.txt"), "before\n").expect("write tracked file");
    std::process::Command::new("git")
        .args(["add", "tracked.txt"])
        .current_dir(temp.path())
        .output()
        .expect("git add");
    std::process::Command::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(temp.path())
        .output()
        .expect("git commit");
    temp
}
