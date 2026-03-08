use anyhow::Result;
use std::process::{Command as ProcessCommand, Stdio};

use super::args::{AmbientCommand, Args, Command, MemoryCommand};
use crate::{
    agent, auth, build, provider, provider_catalog, server, session, setup_hints, startup_profile,
    tui,
};

use super::{commands, debug, login, provider_init, selfdev, terminal, tui_launch};
use provider_init::ProviderChoice;

pub async fn run_main(mut args: Args) -> Result<()> {
    resolve_resume_arg(&mut args)?;

    match args.command {
        Some(Command::Serve) => {
            std::env::set_var("JCODE_NON_INTERACTIVE", "1");
            let provider =
                provider_init::init_provider(&args.provider, args.model.as_deref()).await?;
            let server = server::Server::new(provider);
            server.run().await?;
        }
        Some(Command::Connect) => {
            tui_launch::run_client().await?;
        }
        Some(Command::Run { message }) => {
            let (provider, registry) =
                provider_init::init_provider_and_registry(&args.provider, args.model.as_deref())
                    .await?;
            let mut agent = agent::Agent::new(provider, registry);
            agent.run_once(&message).await?;
        }
        Some(Command::Login { account }) => {
            login::run_login(&args.provider, account.as_deref()).await?;
        }
        Some(Command::Repl) => {
            let (provider, registry) =
                provider_init::init_provider_and_registry(&args.provider, args.model.as_deref())
                    .await?;
            let mut agent = agent::Agent::new(provider, registry);
            agent.repl().await?;
        }
        Some(Command::Update) => {
            tui_launch::run_update()?;
        }
        Some(Command::SelfDev { build }) => {
            selfdev::run_self_dev(build, args.resume).await?;
        }
        Some(Command::CanaryWrapper {
            session_id,
            binary,
            git_hash,
        }) => {
            selfdev::run_canary_wrapper(&session_id, &binary, &git_hash).await?;
        }
        Some(Command::Debug {
            command,
            arg,
            session,
            socket,
            wait,
        }) => {
            debug::run_debug_command(&command, &arg, session, socket, wait).await?;
        }
        Some(Command::Memory(subcmd)) => {
            commands::run_memory_command(map_memory_subcommand(subcmd))?;
        }
        Some(Command::Ambient(subcmd)) => {
            commands::run_ambient_command(map_ambient_subcommand(subcmd)).await?;
        }
        Some(Command::Pair { list, revoke }) => {
            commands::run_pair_command(list, revoke)?;
        }
        Some(Command::Permissions) => {
            tui::permissions::run_permissions()?;
        }
        Some(Command::SetupHotkey) => {
            setup_hints::run_setup_hotkey()?;
        }
        Some(Command::Browser { action }) => {
            commands::run_browser(&action).await?;
        }
        Some(Command::Replay {
            session,
            export,
            speed,
            timeline,
            auto_edit,
            video,
            cols,
            rows,
            fps,
            centered,
            no_centered,
        }) => {
            let centered_override = if centered {
                Some(true)
            } else if no_centered {
                Some(false)
            } else {
                None
            };
            tui_launch::run_replay_command(
                &session,
                export,
                auto_edit,
                speed,
                timeline.as_deref(),
                video.as_deref(),
                cols,
                rows,
                fps,
                centered_override,
            )
            .await?;
        }
        None => run_default_command(args).await?,
    }

    Ok(())
}

fn resolve_resume_arg(args: &mut Args) -> Result<()> {
    if let Some(ref resume_id) = args.resume {
        if resume_id.is_empty() {
            return tui_launch::list_sessions();
        }

        match session::find_session_by_name_or_id(resume_id) {
            Ok(full_id) => {
                args.resume = Some(full_id);
            }
            Err(e) => {
                eprintln!("Error: {}", e);
                eprintln!("\nUse `jcode --resume` to list available sessions.");
                std::process::exit(1);
            }
        }
    }

    Ok(())
}

fn map_memory_subcommand(subcmd: MemoryCommand) -> commands::MemorySubcommand {
    match subcmd {
        MemoryCommand::List { scope, tag } => commands::MemorySubcommand::List { scope, tag },
        MemoryCommand::Search { query, semantic } => {
            commands::MemorySubcommand::Search { query, semantic }
        }
        MemoryCommand::Export { output, scope } => {
            commands::MemorySubcommand::Export { output, scope }
        }
        MemoryCommand::Import {
            input,
            scope,
            overwrite,
        } => commands::MemorySubcommand::Import {
            input,
            scope,
            overwrite,
        },
        MemoryCommand::Stats => commands::MemorySubcommand::Stats,
        MemoryCommand::ClearTest => commands::MemorySubcommand::ClearTest,
    }
}

fn map_ambient_subcommand(subcmd: AmbientCommand) -> commands::AmbientSubcommand {
    match subcmd {
        AmbientCommand::Status => commands::AmbientSubcommand::Status,
        AmbientCommand::Log => commands::AmbientSubcommand::Log,
        AmbientCommand::Trigger => commands::AmbientSubcommand::Trigger,
        AmbientCommand::Stop => commands::AmbientSubcommand::Stop,
        AmbientCommand::RunVisible => commands::AmbientSubcommand::RunVisible,
    }
}

#[allow(deprecated)]
async fn run_default_command(args: Args) -> Result<()> {
    startup_profile::mark("run_main_none_branch");

    let startup_message = setup_hints::maybe_show_setup_hints();
    startup_profile::mark("setup_hints");

    if args.resume.is_none() {
        terminal::show_crash_resume_hint();
    }
    startup_profile::mark("crash_resume_hint");

    let cwd = std::env::current_dir()?;
    let in_jcode_repo = build::is_jcode_repo(&cwd);
    startup_profile::mark("is_jcode_repo");
    let already_in_selfdev = std::env::var("JCODE_SELFDEV_MODE").is_ok();

    if in_jcode_repo && !already_in_selfdev && !args.standalone && !args.no_selfdev {
        eprintln!("📍 Detected jcode repository - enabling self-dev mode");
        eprintln!("   (use --no-selfdev to disable auto-detection)\n");

        std::env::set_var("JCODE_SELFDEV_MODE", "1");
        return selfdev::run_self_dev(false, args.resume).await;
    }

    if args.standalone {
        eprintln!("\x1b[33m⚠️  Warning: --standalone is deprecated and will be removed in a future version.\x1b[0m");
        eprintln!("\x1b[33m   The default server/client mode now handles all use cases including self-dev.\x1b[0m\n");
        let (provider, registry) =
            provider_init::init_provider_and_registry(&args.provider, args.model.as_deref())
                .await?;
        tui_launch::run_tui(
            provider,
            registry,
            args.resume,
            args.debug_socket,
            startup_message,
        )
        .await?;
        return Ok(());
    }

    startup_profile::mark("client_mode_start");
    let server_running = server_is_running().await;
    startup_profile::mark("server_check");

    if server_running && (args.provider != ProviderChoice::Auto || args.model.is_some()) {
        eprintln!(
            "Server already running; provider/model flags only apply when starting a new server."
        );
        eprintln!(
            "Current server settings control `/model`. Restart server to apply: --provider {}{}",
            args.provider.as_arg_value(),
            args.model
                .as_ref()
                .map(|m| format!(" --model {}", m))
                .unwrap_or_default()
        );
    }

    if !server_running {
        maybe_prompt_server_bootstrap_login(&args.provider).await?;
        spawn_server(&args.provider, args.model.as_deref()).await?;
    }

    startup_profile::mark("pre_tui_client");
    if std::env::var("JCODE_RESUMING").is_err() && server_running {
        eprintln!("Connecting to server...");
    }
    tui_launch::run_tui_client(args.resume, startup_message, !server_running).await?;

    Ok(())
}

async fn server_is_running() -> bool {
    if crate::transport::is_socket_path(&server::socket_path()) {
        crate::transport::Stream::connect(server::socket_path())
            .await
            .is_ok()
    } else {
        false
    }
}

async fn maybe_prompt_server_bootstrap_login(provider_choice: &ProviderChoice) -> Result<()> {
    startup_profile::mark("cred_check_start");
    let (has_claude, has_openai) = tokio::join!(
        tokio::task::spawn_blocking(|| auth::claude::load_credentials().is_ok()),
        tokio::task::spawn_blocking(|| auth::codex::load_credentials().is_ok()),
    );
    let has_claude = has_claude.unwrap_or(false);
    let has_openai = has_openai.unwrap_or(false);
    let has_openrouter = provider::openrouter::OpenRouterProvider::has_credentials();
    let has_copilot = auth::copilot::has_copilot_credentials();
    let has_api_key = std::env::var("ANTHROPIC_API_KEY").is_ok();
    startup_profile::mark("cred_check_done");

    if !has_claude
        && !has_openai
        && !has_openrouter
        && !has_copilot
        && !has_api_key
        && *provider_choice == ProviderChoice::Auto
    {
        let provider = provider_init::prompt_login_provider_selection(
            &provider_catalog::server_bootstrap_login_providers(),
            "No credentials found. Let's log in!\n\nChoose a provider:",
        )?;
        login::run_login_provider(provider, Some("default")).await?;
        provider_init::apply_login_provider_profile_env(provider);
        eprintln!();
    }

    Ok(())
}

async fn spawn_server(provider_choice: &ProviderChoice, model: Option<&str>) -> Result<()> {
    let _ = std::fs::remove_file(server::socket_path());
    let _ = std::fs::remove_file(server::debug_socket_path());

    startup_profile::mark("server_spawn_start");
    eprintln!("Starting server...");
    let exe = std::env::current_exe()?;
    let mut cmd = ProcessCommand::new(&exe);
    cmd.arg("--provider").arg(provider_choice.as_arg_value());
    if let Some(model) = model {
        cmd.arg("--model").arg(model);
    }
    cmd.arg("serve")
        .stdout(Stdio::null())
        .stderr(Stdio::null());

    #[cfg(unix)]
    {
        let _child = server::spawn_server_notify(&mut cmd).await?;
        startup_profile::mark("server_ready");
    }
    #[cfg(not(unix))]
    {
        cmd.spawn()?;
        let start = std::time::Instant::now();
        while start.elapsed() < std::time::Duration::from_millis(500) {
            if crate::transport::is_socket_path(&server::socket_path()) {
                if crate::transport::Stream::connect(server::socket_path())
                    .await
                    .is_ok()
                {
                    startup_profile::mark("server_ready");
                    break;
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    }

    Ok(())
}
