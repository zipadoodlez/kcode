use super::{
    effort_display_label, extract_bracketed_system_message, inferred_reasoning_efforts,
    partition_queued_messages, resume_invocation_args, resumed_window_title,
};
use crate::terminal_launch::{detected_resume_terminal, shell_command};

struct EnvVarGuard {
    key: &'static str,
    prev: Option<std::ffi::OsString>,
}

impl EnvVarGuard {
    fn set_value(key: &'static str, value: &str) -> Self {
        let prev = std::env::var_os(key);
        crate::env::set_var(key, value);
        Self { key, prev }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        if let Some(prev) = self.prev.take() {
            crate::env::set_var(self.key, prev);
        } else {
            crate::env::remove_var(self.key);
        }
    }
}

#[test]
fn extract_bracketed_system_message_strips_wrapper() {
    let parsed = extract_bracketed_system_message(
        "[SYSTEM: Your session was interrupted. Continue immediately.]",
    );
    assert_eq!(
        parsed.as_deref(),
        Some("Your session was interrupted. Continue immediately.")
    );
}

#[test]
fn partition_queued_messages_moves_system_messages_into_reminders() {
    let (user_messages, reminder) = partition_queued_messages(
        vec![
            "[SYSTEM: Continue where you left off.]".to_string(),
            "normal user input".to_string(),
        ],
        vec!["hidden reminder".to_string()],
    );

    assert_eq!(
        user_messages,
        vec!["normal user input"],
        "a queued system message must not travel as user text"
    );
    assert_eq!(
        reminder.as_deref(),
        Some("hidden reminder\n\nContinue where you left off.")
    );
}

#[test]
fn inferred_reasoning_efforts_use_provider_specific_order_and_max_semantics() {
    assert_eq!(
        inferred_reasoning_efforts(Some("openai"), Some("gpt-5.4")),
        vec!["none", "minimal", "low", "medium", "high", "xhigh", "max",],
        "OpenAI exposes max as a real Responses API effort level"
    );
    assert_eq!(
        inferred_reasoning_efforts(Some("openai-compatible:custom"), Some("o5-mini")),
        kcode_provider_core::OPENAI_SELECTABLE_EFFORTS,
        "direct compatible routes must preserve OpenAI max instead of aliasing it to xhigh"
    );
    assert_eq!(
        inferred_reasoning_efforts(Some("anthropic"), Some("claude-sonnet-4-6")),
        vec!["none", "low", "medium", "high", "max",]
    );
    assert_eq!(
        inferred_reasoning_efforts(Some("anthropic"), Some("claude-opus-4-7")),
        vec!["none", "low", "medium", "high", "xhigh", "max",]
    );
    assert_eq!(
        inferred_reasoning_efforts(Some("openrouter"), Some("anthropic/claude-sonnet-4.6")),
        vec!["none", "minimal", "low", "medium", "high", "xhigh",]
    );
    assert_eq!(
        inferred_reasoning_efforts(Some("openrouter"), Some("deepseek/deepseek-r1")),
        vec!["none", "minimal", "low", "medium", "high", "xhigh",],
        "OpenRouter uses unified reasoning where max is only an alias, not a cycle level"
    );
    assert_eq!(
        inferred_reasoning_efforts(Some("deepseek"), Some("deepseek-v4-pro")),
        vec!["none", "low", "medium", "high", "max",],
        "DeepSeek direct keeps max as a real provider level"
    );
    assert!(inferred_reasoning_efforts(Some("ollama"), Some("llama3")).is_empty());
}

#[test]
fn effort_display_labels_are_static_and_validated() {
    assert_eq!(effort_display_label("high"), "High");
    assert_eq!(effort_display_label("future"), "future");
}

#[test]
fn detected_resume_terminal_recognizes_handterm_term_program() {
    let _env_lock = crate::storage::lock_test_env();
    let _guard = EnvVarGuard::set_value("TERM_PROGRAM", "handterm");
    assert_eq!(detected_resume_terminal().as_deref(), Some("handterm"));
}

#[test]
fn shell_command_quotes_single_quotes_for_handterm_exec() {
    let command = shell_command(&[
        "/tmp/kcode binary".to_string(),
        "--resume".to_string(),
        "session'quote".to_string(),
    ]);
    assert_eq!(
        command,
        "'/tmp/kcode binary' '--resume' 'session'\"'\"'quote'"
    );
}

/// #715, the behavioral half. The compile-time guard above proves
/// `spawn_in_new_terminal` exists on every target; this proves the invocation
/// it builds actually reaches a launcher and gets spawned.
///
/// Drives `spawn_command_in_new_terminal_with`, the injectable seam the real
/// path bottoms out in, and records what the spawner was handed. Using the
/// injected closure rather than the configured spawn hook keeps this
/// deterministic: the hook is read through the process-wide cached config, so a
/// hook-based test passes or fails depending on whether config was already
/// loaded by an earlier test in the same binary.
#[test]
fn resume_invocation_reaches_the_launcher_and_reports_success() {
    let args = resume_invocation_args("ses_715_behavioral", None);
    let command = crate::terminal_launch::TerminalCommand::new(
        std::path::Path::new("/usr/bin/true"),
        args.clone(),
    )
    .title(resumed_window_title("ses_715_behavioral"));

    let mut spawned: Vec<String> = Vec::new();
    let result = crate::terminal_launch::spawn_command_in_new_terminal_with(
        &command,
        std::path::Path::new("/tmp"),
        |cmd| {
            spawned.push(cmd.get_program().to_string_lossy().into_owned());
            for arg in cmd.get_args() {
                spawned.push(arg.to_string_lossy().into_owned());
            }
            Ok(())
        },
    );

    assert!(
        matches!(result, Ok(true)),
        "launcher reported no terminal for the resume invocation: {result:?}"
    );
    let joined = spawned.join(" ");
    assert!(
        joined.contains("--resume") && joined.contains("ses_715_behavioral"),
        "launcher never received the resume invocation, got: {joined:?}"
    );
}

#[test]
fn resume_invocation_args_includes_socket_when_present() {
    let args = resume_invocation_args("ses_123", Some("/tmp/kcode-test.sock"));
    assert_eq!(
        args,
        vec![
            "--fresh-spawn".to_string(),
            "--resume".to_string(),
            "ses_123".to_string(),
            "--socket".to_string(),
            "/tmp/kcode-test.sock".to_string()
        ]
    );
}

#[test]
fn resume_invocation_args_omits_blank_socket() {
    let args = resume_invocation_args("ses_123", Some("   "));
    assert_eq!(
        args,
        vec![
            "--fresh-spawn".to_string(),
            "--resume".to_string(),
            "ses_123".to_string()
        ]
    );
}

#[test]
fn fresh_session_command_includes_fresh_spawn_and_socket() {
    let command = super::build_fresh_session_command(Some("/tmp/test.sock"));
    assert!(command.fresh_spawn, "must hand off as a fresh spawn");
    assert_eq!(command.kind.as_deref(), Some("new-terminal"));
    assert_eq!(command.title.as_deref(), Some("kcode · new session"));
    assert_eq!(
        command.args,
        vec![
            "--fresh-spawn".to_string(),
            "--socket".to_string(),
            "/tmp/test.sock".to_string(),
        ]
    );
}

#[test]
fn fresh_session_command_omits_blank_socket() {
    let command = super::build_fresh_session_command(Some("   "));
    assert_eq!(command.args, vec!["--fresh-spawn".to_string()]);
    let command = super::build_fresh_session_command(None);
    assert_eq!(command.args, vec!["--fresh-spawn".to_string()]);
}

/// Regression for issue #424: `Instant::now() - Duration` panics with
/// "overflow when subtracting duration from instant" when the monotonic clock
/// epoch (boot time) is more recent than the backdate amount. `backdated_now`
/// must saturate instead of panicking, and still return a value in the past
/// when possible so TTL checks treat the entry as expired.
#[test]
fn backdated_now_never_panics_and_prefers_past_instants() {
    use std::time::{Duration, Instant};

    let now = Instant::now();

    // Typical case: small backdate should land in the past.
    let recent = super::backdated_now(Duration::from_millis(10));
    assert!(recent <= now, "backdated instant must not be in the future");

    // Huge backdate (longer than any plausible uptime) must not panic and
    // must still return something no later than now.
    let ancient = super::backdated_now(Duration::from_secs(60 * 60 * 24 * 365 * 100));
    assert!(
        ancient <= now,
        "saturated backdate must not be in the future"
    );

    // Zero backdate is a no-op.
    let zero = super::backdated_now(Duration::ZERO);
    assert!(zero <= Instant::now());
}
