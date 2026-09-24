#![cfg_attr(test, allow(clippy::await_holding_lock))]

use anyhow::{Context, Result};

const MAX_INTERACTIVE_SWARM_REPLAY_PANES: usize = 16;
use std::io::{self, Write};
use std::process::Command as ProcessCommand;

use crate::{id, logging, server, session, startup_profile, tui};

use super::hot_exec::{execute_requested_action, has_requested_action};

use super::terminal::{
    init_tui_runtime, print_session_resume_hint, set_current_session, spawn_session_signal_watchers,
};

pub(crate) use crate::session_launch::resumed_window_title;

pub async fn run_client() -> Result<()> {
    let mut client = server::Client::connect().await?;

    if !client.ping().await? {
        anyhow::bail!("Failed to ping server");
    }

    println!("Connected to Kcode server");
    println!("Type your message, or 'quit' to exit.\n");

    loop {
        print!("> ");
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        let input = input.trim();
        if input.is_empty() {
            continue;
        }

        if input == "quit" || input == "exit" {
            break;
        }

        match client.send_message(input).await {
            Ok(msg_id) => loop {
                match client.read_event().await {
                    Ok(event) => {
                        use crate::protocol::ServerEvent;
                        match event {
                            ServerEvent::TextDelta { text } => {
                                print!("{}", crate::output_style::terminal_text(&text));
                                std::io::stdout().flush()?;
                            }
                            ServerEvent::Done { id } if id == msg_id => {
                                break;
                            }
                            ServerEvent::Error { message, .. } => {
                                eprintln!("Error: {}", message);
                                break;
                            }
                            _ => {}
                        }
                    }
                    Err(e) => {
                        eprintln!("Event error: {}", e);
                        break;
                    }
                }
            },
            Err(e) => {
                eprintln!("Error: {}", e);
            }
        }

        println!();
    }

    Ok(())
}

pub async fn run_tui_client(
    resume_session: Option<String>,
    server_spawning: bool,
    fresh_spawn: bool,
    remote_working_dir: Option<String>,
    onboarding_sim: bool,
    update_sim: bool,
) -> Result<()> {
    startup_profile::mark("tui_client_enter");
    let (terminal, tui_runtime) = init_tui_runtime()?;
    startup_profile::mark("tui_terminal_init");
    startup_profile::mark("mermaid_picker");
    startup_profile::mark("config_load");
    startup_profile::mark("keyboard_enhancement");
    startup_profile::mark("terminal_modes");

    if let Some(ref session_id) = resume_session {
        set_current_session(session_id);
    }
    let native_ssh = std::env::var_os("JCODE_SSH_REMOTE").is_some();
    if !native_ssh {
        spawn_session_signal_watchers();
    }

    if native_ssh {
        let host = std::env::var("JCODE_SSH_REMOTE").unwrap_or_default();
        let label = resume_session.as_deref().unwrap_or("new session");
        crate::process_title::set_client_remote_display_title(
            &host,
            label,
            crate::client_mode::client_selfdev_requested(),
        );
        let _ = crossterm::execute!(
            std::io::stdout(),
            crossterm::terminal::SetTitle(format!("jcode SSH {host} {label}"))
        );
    } else if let Some(ref session_id) = resume_session {
        let session_name = id::extract_session_name(session_id)
            .map(|s| s.to_string())
            .unwrap_or_else(|| session_id.clone());
        let is_selfdev = crate::client_mode::client_selfdev_requested();
        if let Some(server_info) =
            crate::registry::find_server_by_socket_sync(&server::socket_path())
        {
            crate::process_title::set_client_remote_display_title(
                &server_info.name,
                &session_name,
                is_selfdev,
            );
        } else {
            crate::process_title::set_client_display_title(&session_name, is_selfdev);
        }
        let _ = crossterm::execute!(
            std::io::stdout(),
            crossterm::terminal::SetTitle(resumed_window_title(session_id))
        );
    } else {
        crate::process_title::set_client_generic_title(
            crate::client_mode::client_selfdev_requested(),
        );
        let _ = crossterm::execute!(std::io::stdout(), crossterm::terminal::SetTitle("jcode"));
    }
    startup_profile::mark("terminal_title");

    let mut app = tui::App::new_for_remote_with_options(resume_session.clone(), fresh_spawn);
    if should_show_server_spawning(server_spawning).await {
        app.set_server_spawning();
    }
    if onboarding_sim {
        app.start_onboarding_simulator_on_launch();
    }
    if update_sim {
        app.start_update_simulator_on_launch();
    }
    startup_profile::mark("app_new_for_remote");

    startup_profile::mark("pre_run_remote");
    startup_profile::report_to_log();

    let result = app.run_remote(terminal, remote_working_dir).await;

    // On the error path, `?` returns here while `tui_runtime` is still alive, so
    // its `Drop` guarantees the terminal is restored (issue #214). On the happy
    // path we hand the run result to the guard so it can skip the restore when
    // we are about to exec a follow-up process.
    let run_result = result?;

    if native_ssh {
        // No local exec/reload may escape the SSH lifetime guard or inherit
        // remote session IDs as if they referred to laptop session files.
        tui_runtime.finish(true);
        if has_requested_action(&run_result) {
            anyhow::bail!(
                "local reload/update actions are unavailable during SSH attach; reconnect after updating explicitly"
            );
        }
        if run_result.exit_code.is_some_and(|code| code != 0) {
            anyhow::bail!(
                "SSH client exited with code {}",
                run_result.exit_code.unwrap()
            );
        }
        if let Some(ref session_id) = run_result.session_id {
            print_session_resume_hint(session_id);
        }
        return Ok(());
    }

    tui_runtime.finish_for_run_result(&run_result, false);

    if let Some(code) = run_result.exit_code {
        std::process::exit(code);
    }

    execute_requested_action(&run_result)?;

    if !has_requested_action(&run_result)
        && let Some(ref session_id) = run_result.session_id
    {
        print_session_resume_hint(session_id);
    }

    Ok(())
}

async fn should_show_server_spawning(server_spawning: bool) -> bool {
    if !server_spawning {
        return false;
    }

    let socket_path = server::socket_path();
    if server::has_live_listener(&socket_path).await {
        logging::info(&format!(
            "Skipping stale startup phase: server already listening at {}",
            socket_path.display()
        ));
        return false;
    }

    true
}

#[expect(
    clippy::too_many_arguments,
    reason = "Replay command maps directly from CLI flags and transport options"
)]

// Session-launching helpers live in the core `session_launch` module so that
// lower layers (server, restart_snapshot, tool) can relaunch sessions without
// depending on `cli`. Re-exported here for the CLI's own callers.
pub use crate::session_launch::{
    spawn_resume_in_new_terminal, spawn_resume_in_new_terminal_with_provider,
    spawn_selfdev_in_new_terminal, spawn_selfdev_in_new_terminal_with_provider,
};

pub fn list_sessions() -> Result<()> {
    fn build_resume_target_command(
        exe: &std::path::Path,
        target: &jcode_tui_session_picker::ResumeTarget,
    ) -> (std::path::PathBuf, Vec<String>) {
        match target {
            jcode_tui_session_picker::ResumeTarget::JcodeSession { session_id } => (
                exe.to_path_buf(),
                vec!["--resume".to_string(), session_id.clone()],
            ),
            jcode_tui_session_picker::ResumeTarget::ClaudeCodeSession { session_id, .. } => (
                exe.to_path_buf(),
                vec![
                    "--resume".to_string(),
                    crate::import::imported_claude_code_session_id(session_id),
                ],
            ),
            jcode_tui_session_picker::ResumeTarget::CodexSession { session_id, .. } => (
                exe.to_path_buf(),
                vec![
                    "--resume".to_string(),
                    crate::import::imported_codex_session_id(session_id),
                ],
            ),
            jcode_tui_session_picker::ResumeTarget::PiSession { session_path } => (
                exe.to_path_buf(),
                vec![
                    "--resume".to_string(),
                    crate::import::imported_pi_session_id(session_path),
                ],
            ),
            jcode_tui_session_picker::ResumeTarget::OpenCodeSession { session_id, .. } => (
                exe.to_path_buf(),
                vec![
                    "--resume".to_string(),
                    crate::import::imported_opencode_session_id(session_id),
                ],
            ),
            jcode_tui_session_picker::ResumeTarget::CursorSession { session_id, .. } => (
                exe.to_path_buf(),
                vec![
                    "--resume".to_string(),
                    crate::import::imported_cursor_session_id(session_id),
                ],
            ),
        }
    }

    fn command_display(program: &std::path::Path, args: &[String]) -> String {
        std::iter::once(program.to_string_lossy().to_string())
            .chain(args.iter().cloned())
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn spawn_target_in_new_terminal(
        target: &jcode_tui_session_picker::ResumeTarget,
        exe: &std::path::Path,
        cwd: &std::path::Path,
    ) -> Result<bool> {
        let (program, args) = build_resume_target_command(exe, target);
        let title = match target {
            jcode_tui_session_picker::ResumeTarget::JcodeSession { session_id } => {
                resumed_window_title(session_id)
            }
            jcode_tui_session_picker::ResumeTarget::ClaudeCodeSession { session_id, .. } => {
                format!("🧵 Claude Code {}", &session_id[..session_id.len().min(8)])
            }
            jcode_tui_session_picker::ResumeTarget::CodexSession { session_id, .. } => {
                format!("🧠 Codex {}", &session_id[..session_id.len().min(8)])
            }
            jcode_tui_session_picker::ResumeTarget::PiSession { session_path } => {
                format!(
                    "π Pi {}",
                    std::path::Path::new(session_path)
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("session")
                )
            }
            jcode_tui_session_picker::ResumeTarget::OpenCodeSession { session_id, .. } => {
                format!("◌ OpenCode {}", &session_id[..session_id.len().min(8)])
            }
            jcode_tui_session_picker::ResumeTarget::CursorSession { session_id, .. } => {
                format!("▮ Cursor {}", &session_id[..session_id.len().min(8)])
            }
        };
        let title = crate::output_style::terminal_text(&title).into_owned();
        let command = crate::terminal_launch::TerminalCommand::new(program, args).title(title);
        crate::terminal_launch::spawn_command_in_new_terminal(&command, cwd)
    }

    match tui::session_picker::pick_session()? {
        Some(tui::session_picker::PickerResult::TakeOverClaude(target)) => {
            let resolved_target = crate::import::take_over_live_claude_session(&target)?;
            let jcode_tui_session_picker::ResumeTarget::JcodeSession { session_id } =
                &resolved_target
            else {
                anyhow::bail!("Claude takeover did not produce a Jcode session");
            };
            let exe = std::env::current_exe()?;
            let mut session_cwd = std::env::current_dir()?;
            if let Ok(sess) = session::Session::load(session_id)
                && let Some(dir) = sess.working_dir.as_deref()
                && std::path::Path::new(dir).is_dir()
            {
                session_cwd = std::path::PathBuf::from(dir);
            }
            let (program, args) = build_resume_target_command(&exe, &resolved_target);
            let err = crate::platform::replace_process(
                ProcessCommand::new(&program)
                    .args(&args)
                    .current_dir(session_cwd),
            );
            Err(anyhow::anyhow!("Failed to exec {:?}: {}", program, err))
        }
        Some(
            tui::session_picker::PickerResult::Selected(targets)
            | tui::session_picker::PickerResult::SelectedInCurrentTerminal(targets),
        ) => {
            let exe = std::env::current_exe()?;
            let cwd = std::env::current_dir()?;

            if targets.len() == 1 {
                let target = &targets[0];
                let resolved_target = crate::import::resolve_resume_target_to_jcode(target)?;
                let mut session_cwd = cwd.clone();
                if let jcode_tui_session_picker::ResumeTarget::JcodeSession { session_id } =
                    &resolved_target
                    && let Ok(sess) = session::Session::load(session_id)
                    && let Some(dir) = sess.working_dir.as_deref()
                    && std::path::Path::new(dir).is_dir()
                {
                    session_cwd = std::path::PathBuf::from(dir);
                }
                let (program, args) = build_resume_target_command(&exe, &resolved_target);
                let err = crate::platform::replace_process(
                    ProcessCommand::new(&program)
                        .args(&args)
                        .current_dir(session_cwd),
                );

                Err(anyhow::anyhow!("Failed to exec {:?}: {}", program, err))
            } else {
                let mut spawned = 0usize;
                let mut warned_no_terminal = false;

                for target in targets {
                    let resolved_target =
                        match crate::import::resolve_resume_target_to_jcode(&target) {
                            Ok(target) => target,
                            Err(e) => {
                                eprintln!("Failed to import selected session: {}", e);
                                continue;
                            }
                        };
                    let mut session_cwd = cwd.clone();
                    if let jcode_tui_session_picker::ResumeTarget::JcodeSession { session_id } =
                        &resolved_target
                        && let Ok(sess) = session::Session::load(session_id)
                        && let Some(dir) = sess.working_dir.as_deref()
                        && std::path::Path::new(dir).is_dir()
                    {
                        session_cwd = std::path::PathBuf::from(dir);
                    }

                    match spawn_target_in_new_terminal(&resolved_target, &exe, &session_cwd) {
                        Ok(true) => spawned += 1,
                        Ok(false) => {
                            if !warned_no_terminal {
                                eprintln!(
                                    "No supported terminal emulator found. Run these commands manually:"
                                );
                                warned_no_terminal = true;
                            }
                            let (program, args) =
                                build_resume_target_command(&exe, &resolved_target);
                            eprintln!("  {}", command_display(&program, &args));
                        }
                        Err(e) => {
                            eprintln!("Failed to spawn selected session: {}", e);
                        }
                    }
                }

                if spawned == 0 && warned_no_terminal {
                    return Ok(());
                }

                if spawned == 0 {
                    anyhow::bail!("Failed to spawn any selected sessions");
                }

                Ok(())
            }
        }
        Some(tui::session_picker::PickerResult::SelectedInNewTerminal(targets)) => {
            let exe = std::env::current_exe()?;
            let cwd = std::env::current_dir()?;
            let mut spawned = 0usize;
            let mut warned_no_terminal = false;

            for target in targets {
                let resolved_target = match crate::import::resolve_resume_target_to_jcode(&target) {
                    Ok(target) => target,
                    Err(e) => {
                        eprintln!("Failed to import selected session: {}", e);
                        continue;
                    }
                };
                let mut session_cwd = cwd.clone();
                if let jcode_tui_session_picker::ResumeTarget::JcodeSession { session_id } =
                    &resolved_target
                    && let Ok(sess) = session::Session::load(session_id)
                    && let Some(dir) = sess.working_dir.as_deref()
                    && std::path::Path::new(dir).is_dir()
                {
                    session_cwd = std::path::PathBuf::from(dir);
                }

                match spawn_target_in_new_terminal(&resolved_target, &exe, &session_cwd) {
                    Ok(true) => spawned += 1,
                    Ok(false) => {
                        if !warned_no_terminal {
                            eprintln!(
                                "No supported terminal emulator found. Run these commands manually:"
                            );
                            warned_no_terminal = true;
                        }
                        let (program, args) = build_resume_target_command(&exe, &resolved_target);
                        eprintln!("  {}", command_display(&program, &args));
                    }
                    Err(e) => {
                        eprintln!("Failed to spawn selected session: {}", e);
                    }
                }
            }

            if spawned == 0 && warned_no_terminal {
                return Ok(());
            }

            if spawned == 0 {
                anyhow::bail!("Failed to spawn any selected sessions");
            }

            Ok(())
        }
        Some(tui::session_picker::PickerResult::RestoreCrashedGroup(session_ids)) => {
            let recovered = session::recover_crashed_sessions_by_ids(&session_ids)?;
            if recovered.is_empty() {
                eprintln!("No crashed sessions found in the selected restore group.");
                return Ok(());
            }

            eprintln!(
                "Recovered {} crashed session(s) from the selected restore group.",
                recovered.len()
            );

            let exe = std::env::current_exe()?;
            let cwd = std::env::current_dir()?;
            let mut spawned = 0usize;
            let mut warned_no_terminal = false;

            for session_id in recovered {
                let mut session_cwd = cwd.clone();
                if let Ok(sess) = session::Session::load(&session_id)
                    && let Some(dir) = sess.working_dir.as_deref()
                    && std::path::Path::new(dir).is_dir()
                {
                    session_cwd = std::path::PathBuf::from(dir);
                }

                match spawn_resume_in_new_terminal(&exe, &session_id, &session_cwd) {
                    Ok(true) => {
                        spawned += 1;
                    }
                    Ok(false) => {
                        if !warned_no_terminal {
                            eprintln!(
                                "No supported terminal emulator found. Run these commands manually:"
                            );
                            warned_no_terminal = true;
                        }
                        eprintln!("  jcode --resume {}", session_id);
                    }
                    Err(e) => {
                        eprintln!("Failed to spawn session {}: {}", session_id, e);
                    }
                }
            }

            if spawned == 0 && warned_no_terminal {
                return Ok(());
            }

            if spawned == 0 {
                anyhow::bail!("Failed to spawn any recovered sessions");
            }

            Ok(())
        }
        None
        | Some(tui::session_picker::PickerResult::StartNewSession)
        | Some(tui::session_picker::PickerResult::ReviewRecentProject) => {
            eprintln!("No session selected.");
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests;
