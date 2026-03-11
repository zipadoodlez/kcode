use anyhow::Result;
use std::process::{Command as ProcessCommand, Stdio};

use super::args::{AmbientCommand, Args, Command, MemoryCommand};
use crate::{
    agent, auth, build, provider, provider_catalog, server, session, setup_hints, startup_profile,
    tui,
};

use super::{commands, debug, hot_exec, login, provider_init, selfdev, terminal, tui_launch};
use provider_init::ProviderChoice;

pub(crate) async fn run_main(mut args: Args) -> Result<()> {
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
            hot_exec::run_update()?;
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
    let already_in_selfdev = crate::cli::selfdev::client_selfdev_requested();

    if in_jcode_repo && !already_in_selfdev && !args.standalone && !args.no_selfdev {
        eprintln!("📍 Detected jcode repository - enabling self-dev mode");
        eprintln!("   Using shared server with self-dev session mode");
        eprintln!("   (use --no-selfdev to disable auto-detection)\n");

        std::env::set_var(selfdev::CLIENT_SELFDEV_ENV, "1");
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

pub(crate) async fn server_is_running() -> bool {
    server_is_running_at(&server::socket_path()).await
}

async fn server_is_running_at(path: &std::path::Path) -> bool {
    crate::transport::is_socket_path(path) && crate::transport::Stream::connect(path).await.is_ok()
}

#[cfg(unix)]
fn spawn_lock_path(socket_path: &std::path::Path) -> std::path::PathBuf {
    std::path::PathBuf::from(format!("{}.spawning", socket_path.display()))
}

#[cfg(unix)]
struct SpawnLockGuard {
    _file: std::fs::File,
    path: std::path::PathBuf,
}

#[cfg(unix)]
impl Drop for SpawnLockGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

#[cfg(unix)]
fn try_acquire_spawn_lock(path: &std::path::Path) -> Result<Option<SpawnLockGuard>> {
    use std::fs::OpenOptions;
    use std::os::fd::AsRawFd;

    let file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(path)?;
    let fd = file.as_raw_fd();
    let ret = unsafe { libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB) };
    if ret == 0 {
        Ok(Some(SpawnLockGuard {
            _file: file,
            path: path.to_path_buf(),
        }))
    } else {
        Ok(None)
    }
}

#[cfg(unix)]
async fn acquire_spawn_lock_or_wait(
    socket_path: &std::path::Path,
) -> Result<Option<SpawnLockGuard>> {
    let lock_path = spawn_lock_path(socket_path);
    let wait_start = std::time::Instant::now();
    let wait_timeout = std::time::Duration::from_secs(10);
    let mut announced_wait = false;

    loop {
        if let Some(lock) = try_acquire_spawn_lock(&lock_path)? {
            return Ok(Some(lock));
        }

        if server_is_running_at(socket_path).await {
            return Ok(None);
        }

        if !announced_wait {
            eprintln!("Another client is starting the server, waiting...");
            announced_wait = true;
        }

        if wait_start.elapsed() >= wait_timeout {
            anyhow::bail!(
                "Timed out waiting for another client to start server at {}",
                socket_path.display()
            );
        }

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}

pub(crate) async fn maybe_prompt_server_bootstrap_login(
    provider_choice: &ProviderChoice,
) -> Result<()> {
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

pub(crate) async fn spawn_server(
    provider_choice: &ProviderChoice,
    model: Option<&str>,
) -> Result<()> {
    let socket_path = server::socket_path();
    let debug_socket_path = server::debug_socket_path();

    #[cfg(unix)]
    let _spawn_lock = acquire_spawn_lock_or_wait(&socket_path).await?;

    if server_is_running_at(&socket_path).await {
        startup_profile::mark("server_ready");
        return Ok(());
    }

    let _ = std::fs::remove_file(&socket_path);
    let _ = std::fs::remove_file(&debug_socket_path);

    startup_profile::mark("server_spawn_start");
    eprintln!("Starting server...");
    let exe = std::env::current_exe()?;
    let mut cmd = ProcessCommand::new(&exe);
    let client_requested_selfdev = selfdev::client_selfdev_requested();
    cmd.env_remove(selfdev::CLIENT_SELFDEV_ENV);
    if client_requested_selfdev {
        cmd.env("JCODE_DEBUG_CONTROL", "1");
    }
    cmd.arg("--provider").arg(provider_choice.as_arg_value());
    if let Some(model) = model {
        cmd.arg("--model").arg(model);
    }
    cmd.arg("serve")
        .stdout(Stdio::null())
        .stderr(Stdio::piped());

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

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn spawn_lock_serializes_shared_server_bootstrap() {
        let temp = tempfile::tempdir().expect("tempdir");
        let socket_path = temp.path().join("jcode.sock");
        let lock_path = spawn_lock_path(&socket_path);

        let first = try_acquire_spawn_lock(&lock_path)
            .expect("acquire first lock")
            .expect("first lock should succeed");
        let second = try_acquire_spawn_lock(&lock_path).expect("acquire second lock");
        assert!(second.is_none(), "second lock should be held by first guard");

        drop(first);

        let third = try_acquire_spawn_lock(&lock_path)
            .expect("acquire third lock")
            .expect("third lock should succeed after release");
        drop(third);

        assert!(
            !lock_path.exists(),
            "lock file should be cleaned up when the guard drops"
        );
    }
}
