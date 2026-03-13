use anyhow::Result;
use clap::Parser;
use std::process::Command as ProcessCommand;

use crate::{build, logging, perf, server, startup_profile, storage, telemetry, update};

use super::{
    args::{Args, Command},
    dispatch, hot_exec, terminal,
};

pub async fn run() -> Result<()> {
    startup_profile::init();

    terminal::install_panic_hook();
    startup_profile::mark("panic_hook");

    logging::init();
    startup_profile::mark("logging_init");
    logging::cleanup_old_logs();
    startup_profile::mark("log_cleanup");
    logging::info("jcode starting");

    storage::harden_user_config_permissions();
    startup_profile::mark("perm_harden");

    perf::init_background();
    startup_profile::mark("perf_init");

    telemetry::record_install_if_first_run();
    startup_profile::mark("telemetry_check");

    let args = parse_and_prepare_args()?;
    spawn_background_update_check(&args);

    if let Err(e) = dispatch::run_main(args).await {
        report_main_error(&e);
        return Err(e);
    }

    Ok(())
}

fn parse_and_prepare_args() -> Result<Args> {
    let args = Args::parse();
    startup_profile::mark("args_parse");

    if let Some(cwd) = &args.cwd {
        std::env::set_current_dir(cwd)?;
        logging::info(&format!("Changed working directory to: {}", cwd));
    }

    if args.trace {
        crate::env::set_var("JCODE_TRACE", "1");
    }

    if let Some(ref socket) = args.socket {
        server::set_socket_path(socket);
    }

    Ok(args)
}

fn spawn_background_update_check(args: &Args) {
    let check_updates =
        !args.no_update && !matches!(args.command, Some(Command::Update)) && args.resume.is_none();
    let auto_update = args.auto_update;

    if !check_updates {
        return;
    }

    if update::is_release_build() {
        std::thread::spawn(move || match update::check_and_maybe_update(auto_update) {
            update::UpdateCheckResult::UpdateAvailable {
                current, latest, ..
            } => {
                logging::info(&format!("Update available: {} -> {}", current, latest));
            }
            update::UpdateCheckResult::UpdateInstalled { version, path } => {
                update::print_centered(&format!("✅ Updated to {}. Restarting...", version));
                let args: Vec<String> = std::env::args().skip(1).collect();
                let exec_path = build::client_update_candidate(false)
                    .map(|(p, _)| p)
                    .unwrap_or(path);
                let err = crate::platform::replace_process(
                    ProcessCommand::new(&exec_path)
                        .args(&args)
                        .arg("--no-update"),
                );
                eprintln!("Failed to exec new binary: {}", err);
            }
            update::UpdateCheckResult::Error(e) => {
                logging::info(&format!("Update check failed: {}", e));
            }
            update::UpdateCheckResult::NoUpdate => {}
        });
    } else {
        std::thread::spawn(move || {
            if let Some(update_available) = hot_exec::check_for_updates() {
                if update_available {
                    if auto_update {
                        logging::info("Update available - auto-updating...");
                        if let Err(e) = hot_exec::run_auto_update() {
                            logging::error(&format!(
                                "Auto-update failed: {}. Continuing with current version.",
                                e
                            ));
                        }
                    } else {
                        logging::info(
                            "Update available! Run `jcode update` or `/reload` to update.",
                        );
                    }
                }
            }
        });
    }
}

fn report_main_error(error: &anyhow::Error) {
    let error_str = format!("{:?}", error);
    logging::error(&error_str);

    if let Some(session_id) = terminal::get_current_session() {
        eprintln!();
        eprintln!("\x1b[33mTo restore this session, run:\x1b[0m");
        eprintln!("  jcode --resume {}", session_id);
        eprintln!();
    }
}
