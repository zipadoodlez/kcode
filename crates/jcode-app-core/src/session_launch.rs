//! Launching jcode sessions in new terminal windows.
//!
//! These helpers spawn a fresh `jcode` process (resume or self-dev) inside a
//! new terminal window. They are pure process/terminal orchestration built on
//! the low-level `terminal_launch` facade and depend only on core modules
//! (`id`, `process_title`, `platform`), so
//! they live in the core layer rather than the CLI command layer. This lets
//! lower layers like `server`, `restart_snapshot`, and `tool` relaunch
//! sessions without depending on `cli`.

use anyhow::Result;

use crate::{id, server};

/// Map a persisted session/runtime provider key (e.g. `anthropic-api-key`,
/// `claude-oauth`) to the value the resumed process accepts for `--provider`
/// (the CLI `ProviderChoice` vocabulary, e.g. `anthropic-api`, `claude`).
///
/// The two vocabularies are not identical, so passing the raw runtime key
/// straight through makes clap reject it (`invalid value 'anthropic-api-key'`)
/// and the freshly spawned window exits immediately before the TUI starts.
/// Returns `None` when the key has no clean standalone CLI provider value; the
/// flag is then omitted and the persisted session reconstructs the route on
/// resume.
fn resume_provider_arg(provider_key: Option<&str>) -> Option<&'static str> {
    provider_key
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .and_then(crate::provider::cli_provider_arg_for_session_key)
}

/// Metadata describing why a session window is being spawned, exported to
/// spawn hooks and spawned terminals as `JCODE_SPAWN_*` env vars so external
/// programs (tmux, kitty remote, herd, window managers) can reroute or place
/// the window. See `[terminal] spawn_hook` in config.
#[derive(Debug, Clone, Default)]
pub struct SessionSpawnContext {
    /// Spawn kind override (e.g. "swarm-agent", "restart"). Defaults to
    /// "resume" or "selfdev" based on the launch helper used.
    pub kind: Option<String>,
    /// Extra `JCODE_SPAWN_*` env entries (e.g. swarm/coordinator ids).
    pub extra_env: Vec<(String, String)>,
    /// Terminal-identifying env vars captured from the client that requested
    /// the spawn (tmux/zellij/kitty/DISPLAY/...). Re-exported to spawn/focus
    /// hooks so the new window lands in the client's terminal instead of the
    /// server's stale startup env (#405).
    pub client_terminal_env: Vec<(String, String)>,
}

impl SessionSpawnContext {
    pub fn kind(kind: impl Into<String>) -> Self {
        Self {
            kind: Some(kind.into()),
            extra_env: Vec::new(),
            client_terminal_env: Vec::new(),
        }
    }

    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.extra_env.push((key.into(), value.into()));
        self
    }

    /// Attach the requesting client's terminal env snapshot (#405).
    pub fn with_client_terminal_env(mut self, env: Vec<(String, String)>) -> Self {
        self.client_terminal_env = env;
        self
    }

    fn apply(
        &self,
        mut command: crate::terminal_launch::TerminalCommand,
        default_kind: &str,
        session_id: &str,
    ) -> crate::terminal_launch::TerminalCommand {
        command = command
            .kind(self.kind.as_deref().unwrap_or(default_kind))
            .session_id(session_id);
        for (key, value) in &self.extra_env {
            command = command.spawn_env(key.clone(), value.clone());
        }
        if !self.client_terminal_env.is_empty() {
            command = command.client_terminal_env(self.client_terminal_env.clone());
        }
        command
    }
}

/// Compute the window/terminal title used when (re)launching a session.
pub fn resumed_window_title(session_id: &str) -> String {
    let session_name = crate::process_title::session_name(session_id);
    let icon = id::session_icon(&session_name);
    let display_title = crate::process_title::terminal_display_title_for_id(session_id);
    let session_label = crate::process_title::terminal_session_label(&session_name, None);
    let fallback_label = if let Some(server_info) =
        crate::registry::find_server_by_socket_sync(&server::socket_path())
    {
        format!("jcode/{} {}", server_info.name, session_label)
    } else {
        format!("jcode {}", session_label)
    };
    crate::process_title::terminal_window_title(
        icon,
        display_title.as_deref(),
        Some(&fallback_label),
        false,
    )
}

/// Focus/raise the window for `session_id` via the configured focus hook.
///
/// Returns `true` when a hook was configured and its process started (the
/// built-in wmctrl/xdotool fallback should then be skipped). The hook receives
/// `JCODE_FOCUS_SESSION_ID` and `JCODE_FOCUS_TITLE` env vars.
pub fn focus_session_via_hook(session_id: &str, title: &str) -> bool {
    focus_session_via_hook_with_env(session_id, title, &[])
}

/// Like [`focus_session_via_hook`] but also re-exports the requesting client's
/// terminal env (#405) so focus hooks (e.g. `zellij action go-to-tab-name`)
/// target the client's terminal session instead of the server's stale env. Each
/// var is exported natively and under a `JCODE_CLIENT_<NAME>` alias.
pub fn focus_session_via_hook_with_env(
    session_id: &str,
    title: &str,
    client_terminal_env: &[(String, String)],
) -> bool {
    let hook = {
        let config = &crate::config::config().terminal;
        config
            .focus_hook
            .as_deref()
            .map(str::trim)
            .filter(|hook| !hook.is_empty())
            .map(str::to_string)
    };
    let Some(hook) = hook else {
        return false;
    };

    let parts = match crate::terminal_launch::parse_hook_command(&hook) {
        Ok(parts) => parts,
        Err(error) => {
            crate::logging::warn(&format!("Focus hook '{hook}' failed to parse: {error}"));
            return false;
        }
    };
    let (program, args) = parts
        .split_first()
        .expect("parse_hook_command guarantees at least one part");

    let mut cmd = std::process::Command::new(crate::terminal_launch::expand_home(program));
    cmd.args(args)
        .env("JCODE_FOCUS_SESSION_ID", session_id)
        .env("JCODE_FOCUS_TITLE", title)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    for (key, value) in client_terminal_env {
        cmd.env(key, value);
        cmd.env(format!("JCODE_CLIENT_{key}"), value);
    }
    match crate::platform::spawn_detached(&mut cmd) {
        Ok(_) => true,
        Err(error) => {
            crate::logging::warn(&format!(
                "Focus hook '{hook}' failed to start ({error}); falling back to built-in focus"
            ));
            false
        }
    }
}

/// Focus a session window: configured focus hook first, then the built-in
/// wmctrl/xdotool title search (Linux only) as a best-effort fallback.
pub fn focus_session_window_best_effort(session_id: &str, title: &str) {
    focus_session_window_best_effort_with_env(session_id, title, &[]);
}

/// Like [`focus_session_window_best_effort`] but forwards the requesting
/// client's terminal env to the focus hook (#405).
pub fn focus_session_window_best_effort_with_env(
    session_id: &str,
    title: &str,
    client_terminal_env: &[(String, String)],
) {
    if focus_session_via_hook_with_env(session_id, title, client_terminal_env) {
        return;
    }
    focus_title_best_effort(title);
}

#[cfg(all(unix, not(target_os = "macos")))]
fn focus_title_best_effort(title: &str) {
    use std::process::{Command, Stdio};

    let mut cmd = Command::new("sh");
    cmd.arg("-c")
        .arg(
            "sleep 0.4; \
             if command -v wmctrl >/dev/null 2>&1; then wmctrl -a \"$JCODE_WINDOW_TITLE\" >/dev/null 2>&1 && exit 0; fi; \
             if command -v xdotool >/dev/null 2>&1; then xdotool search --name \"$JCODE_WINDOW_TITLE\" windowactivate >/dev/null 2>&1 && exit 0; fi; \
             exit 0",
        )
        .env("JCODE_WINDOW_TITLE", title)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    let _ = crate::platform::spawn_detached(&mut cmd);
}

#[cfg(target_os = "macos")]
fn focus_title_best_effort(_title: &str) {}

pub fn spawn_resume_in_new_terminal(
    exe: &std::path::Path,
    session_id: &str,
    cwd: &std::path::Path,
) -> Result<bool> {
    spawn_resume_in_new_terminal_with_provider(exe, session_id, cwd, None)
}

pub fn spawn_resume_in_new_terminal_with_provider(
    exe: &std::path::Path,
    session_id: &str,
    cwd: &std::path::Path,
    provider_key: Option<&str>,
) -> Result<bool> {
    spawn_resume_in_new_terminal_with_context(
        exe,
        session_id,
        cwd,
        provider_key,
        &SessionSpawnContext::default(),
    )
}

pub fn spawn_resume_in_new_terminal_with_context(
    exe: &std::path::Path,
    session_id: &str,
    cwd: &std::path::Path,
    provider_key: Option<&str>,
    context: &SessionSpawnContext,
) -> Result<bool> {
    let title = resumed_window_title(session_id);
    let mut args = vec!["--fresh-spawn".to_string()];
    if let Some(provider_arg) = resume_provider_arg(provider_key) {
        args.push("--provider".to_string());
        args.push(provider_arg.to_string());
    }
    args.extend(["--resume".to_string(), session_id.to_string()]);
    let command = crate::terminal_launch::TerminalCommand::new(exe, args)
        .title(title)
        .fresh_spawn();
    let command = context.apply(command, "resume", session_id);
    crate::terminal_launch::spawn_command_in_new_terminal(&command, cwd)
}

pub fn spawn_selfdev_in_new_terminal(
    exe: &std::path::Path,
    session_id: &str,
    cwd: &std::path::Path,
) -> Result<bool> {
    spawn_selfdev_in_new_terminal_with_provider(exe, session_id, cwd, None)
}

pub fn spawn_selfdev_in_new_terminal_with_provider(
    exe: &std::path::Path,
    session_id: &str,
    cwd: &std::path::Path,
    provider_key: Option<&str>,
) -> Result<bool> {
    spawn_selfdev_in_new_terminal_with_context(
        exe,
        session_id,
        cwd,
        provider_key,
        &SessionSpawnContext::default(),
    )
}

pub fn spawn_selfdev_in_new_terminal_with_context(
    exe: &std::path::Path,
    session_id: &str,
    cwd: &std::path::Path,
    provider_key: Option<&str>,
    context: &SessionSpawnContext,
) -> Result<bool> {
    let selfdev_title = format!("{} [self-dev]", resumed_window_title(session_id));
    let mut args = vec!["--fresh-spawn".to_string()];
    if let Some(provider_arg) = resume_provider_arg(provider_key) {
        args.push("--provider".to_string());
        args.push(provider_arg.to_string());
    }
    args.extend([
        "--resume".to_string(),
        session_id.to_string(),
        "self-dev".to_string(),
    ]);
    let command = crate::terminal_launch::TerminalCommand::new(exe, args)
        .title(selfdev_title.clone())
        .fresh_spawn();
    let command = context.apply(command, "selfdev", session_id);
    let spawned = crate::terminal_launch::spawn_command_in_new_terminal(&command, cwd)?;
    if spawned {
        focus_session_window_best_effort_with_env(
            session_id,
            &selfdev_title,
            &context.client_terminal_env,
        );
    }
    Ok(spawned)
}
