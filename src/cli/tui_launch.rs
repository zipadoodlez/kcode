#![cfg_attr(test, allow(clippy::await_holding_lock))]

use anyhow::Result;

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
    let native_ssh = std::env::var_os("KCODE_SSH_REMOTE").is_some();
    if !native_ssh {
        spawn_session_signal_watchers();
    }

    if native_ssh {
        let host = std::env::var("KCODE_SSH_REMOTE").unwrap_or_default();
        let label = resume_session.as_deref().unwrap_or("new session");
        crate::process_title::set_client_remote_display_title(
            &host,
            label,
            crate::client_mode::client_selfdev_requested(),
        );
        let _ = crossterm::execute!(
            std::io::stdout(),
            crossterm::terminal::SetTitle(format!("kcode SSH {host} {label}"))
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
        let _ = crossterm::execute!(std::io::stdout(), crossterm::terminal::SetTitle("kcode"));
    }
    startup_profile::mark("terminal_title");

    let mut app = tui::App::new_for_remote_with_options(resume_session.clone(), fresh_spawn);
    if should_show_server_spawning(server_spawning).await {
        app.set_server_spawning();
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

// Session-launching helpers live in the core `session_launch` module so that
// lower layers (server, restart_snapshot, tool) can relaunch sessions without
// depending on `cli`. Re-exported here for the CLI's own callers.
pub use crate::session_launch::{
    spawn_resume_in_new_terminal, spawn_resume_in_new_terminal_with_provider,
    spawn_selfdev_in_new_terminal, spawn_selfdev_in_new_terminal_with_provider,
};

pub fn list_sessions() -> Result<()> {
    fn session_cwd(session_id: &str) -> std::path::PathBuf {
        let mut cwd = std::env::current_dir().unwrap_or_default();
        if let Ok(sess) = session::Session::load(session_id)
            && let Some(dir) = sess.working_dir.as_deref()
            && std::path::Path::new(dir).is_dir()
        {
            cwd = std::path::PathBuf::from(dir);
        }
        cwd
    }

    fn spawn_many(exe: &std::path::Path, ids: &[String]) -> Result<()> {
        let mut spawned = 0usize;
        let mut warned_no_terminal = false;
        for session_id in ids {
            let cwd = session_cwd(session_id);
            match spawn_resume_in_new_terminal(exe, session_id, &cwd) {
                Ok(true) => spawned += 1,
                Ok(false) => {
                    if !warned_no_terminal {
                        eprintln!(
                            "No supported terminal emulator found. Run these commands manually:"
                        );
                        warned_no_terminal = true;
                    }
                    eprintln!("  kcode --resume {}", session_id);
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
            anyhow::bail!("Failed to spawn any selected sessions");
        }
        Ok(())
    }

    match tui::session_picker::pick_session()? {
        Some(
            tui::session_picker::PickerResult::Selected(ids)
            | tui::session_picker::PickerResult::SelectedInCurrentTerminal(ids),
        ) => {
            let exe = std::env::current_exe()?;
            if ids.len() == 1 {
                let session_id = &ids[0];
                let cwd = session_cwd(session_id);
                let err = crate::platform::replace_process(
                    ProcessCommand::new(&exe)
                        .arg("--resume")
                        .arg(session_id)
                        .current_dir(cwd),
                );
                Err(anyhow::anyhow!("Failed to exec {:?}: {}", exe, err))
            } else {
                spawn_many(&exe, &ids)
            }
        }
        Some(tui::session_picker::PickerResult::SelectedInNewTerminal(ids)) => {
            let exe = std::env::current_exe()?;
            spawn_many(&exe, &ids)
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
                        eprintln!("  kcode --resume {}", session_id);
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
        None => {
            eprintln!("No session selected.");
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests;
