#![cfg_attr(test, allow(clippy::await_holding_lock))]

use anyhow::Result;
use serde::Serialize;
use std::collections::BTreeSet;
use std::io::{Read, Write};

use crate::{browser, memory, session, storage, tui};

use super::{output::terminal_title, terminal::init_tui_runtime};

mod menubar;
mod provider_setup;
mod report_info;
mod restart;

pub(crate) use super::auth_test::run_post_login_validation;
#[cfg(test)]
pub(crate) use super::auth_test::{
    AuthTestChoicePlan, AuthTestTarget, ResolvedAuthTestTarget, auth_test_choice_plan,
    auth_test_error_is_retryable, configured_auth_test_targets, resolve_auth_test_targets,
};
pub use super::auth_test::{
    run_auth_test_command, run_auth_test_context_audit_command, run_auth_test_coverage_command,
};
pub use menubar::{ensure_menubar_helper_running, run_menubar_command};
pub(crate) use provider_setup::{ProviderAddOptions, run_provider_add_command};
pub use restart::{
    maybe_run_pending_restart_restore_on_startup, run_restart_clear_command,
    run_restart_restore_command, run_restart_save_command, run_restart_status_command,
};

pub enum AmbientSubcommand {
    Status,
    Log,
    Trigger,
    Stop,
    RunVisible,
}

pub async fn run_ambient_command(cmd: AmbientSubcommand) -> Result<()> {
    if let AmbientSubcommand::RunVisible = cmd {
        return run_ambient_visible().await;
    }

    let debug_cmd = match cmd {
        AmbientSubcommand::Status => "ambient:status",
        AmbientSubcommand::Log => "ambient:log",
        AmbientSubcommand::Trigger => "ambient:trigger",
        AmbientSubcommand::Stop => "ambient:stop",
        AmbientSubcommand::RunVisible => unreachable!(),
    };

    super::debug::run_debug_command(debug_cmd, "", None, None, false).await
}

pub async fn run_transcript_command(
    text: Option<String>,
    mode: crate::protocol::TranscriptMode,
    session: Option<String>,
) -> Result<()> {
    let text = if let Some(text) = text {
        text
    } else {
        let mut stdin = String::new();
        std::io::stdin().read_to_string(&mut stdin)?;
        let trimmed = stdin.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            anyhow::bail!("Provide transcript text as an argument or pipe it via stdin")
        }
        trimmed.to_string()
    };

    let mut client = crate::server::Client::connect_debug().await?;
    let request_id = client.send_transcript(&text, mode, session).await?;

    loop {
        match client.read_event().await? {
            crate::protocol::ServerEvent::Ack { id } if id == request_id => {}
            crate::protocol::ServerEvent::Done { id } if id == request_id => return Ok(()),
            crate::protocol::ServerEvent::Error { id, message, .. } if id == request_id => {
                anyhow::bail!(message)
            }
            _ => {}
        }
    }
}

pub async fn run_dictate_command(type_output: bool) -> Result<()> {
    let run = crate::dictation::run_configured().await?;

    if type_output {
        crate::dictation::type_text(&run.text)
    } else {
        run_transcript_command(Some(run.text), run.mode, None).await
    }
}

#[derive(Serialize)]
struct SessionRenameOutput {
    session_id: String,
    display_name: String,
    title: Option<String>,
    cleared: bool,
}

pub fn run_session_rename_command(
    session_ref: &str,
    name: Option<&str>,
    clear: bool,
    json: bool,
) -> Result<()> {
    let resolved_id = session::find_session_by_name_or_id(session_ref)?;
    let mut session = session::Session::load(&resolved_id)?;

    if clear {
        session.rename_title(None);
    } else {
        let Some(name) = name.map(str::trim).filter(|name| !name.is_empty()) else {
            anyhow::bail!("Provide a session name or use --clear");
        };
        session.rename_title(Some(name.to_string()));
    }

    session.save()?;
    crate::tui::session_picker::invalidate_session_list_cache();

    let output = SessionRenameOutput {
        session_id: session.id.clone(),
        display_name: session.display_name().to_string(),
        title: session.display_title().map(ToOwned::to_owned),
        cleared: clear,
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else if clear {
        println!(
            "Cleared custom name for session {} ({}).",
            output.display_name, output.session_id
        );
    } else if let Some(title) = output.title.as_deref() {
        println!(
            "Renamed session {} ({}) to \"{}\".",
            output.display_name, output.session_id, title
        );
    }

    Ok(())
}

async fn run_ambient_visible() -> Result<()> {
    use crate::ambient::VisibleCycleContext;

    let context = VisibleCycleContext::load().map_err(|e| {
        anyhow::anyhow!(
            "Failed to load visible cycle context: {}\nIs the ambient runner running?",
            e
        )
    })?;

    let (provider, registry) = super::provider_init::init_provider_and_registry(
        &super::provider_init::ProviderChoice::Auto,
        None,
    )
    .await?;

    registry.register_ambient_tools().await;

    let safety = std::sync::Arc::new(crate::safety::SafetySystem::new());
    crate::tool::ambient::init_safety_system(safety);

    let (terminal, tui_runtime) = init_tui_runtime()?;

    let mut app = tui::App::new(provider, registry);
    app.set_ambient_mode(context.system_prompt, context.initial_message);

    let _ = crossterm::execute!(
        std::io::stdout(),
        crossterm::terminal::SetTitle(terminal_title("🤖 jcode ambient cycle"))
    );

    let result = app.run(terminal).await;

    tui_runtime.finish(true);

    if let Some(cycle_result) = crate::tool::ambient::take_cycle_result() {
        let result_path = VisibleCycleContext::result_path()?;
        crate::storage::write_json(&result_path, &cycle_result)?;
        eprintln!("Ambient cycle result saved.");
    }

    result?;
    Ok(())
}

pub enum MemorySubcommand {
    List {
        scope: String,
        tag: Option<String>,
    },
    Search {
        query: String,
        semantic: bool,
    },
    Export {
        output: String,
        scope: String,
    },
    Import {
        input: String,
        scope: String,
        overwrite: bool,
    },
    Stats,
    ClearTest,
}

pub fn run_memory_command(cmd: MemorySubcommand) -> Result<()> {
    run_memory_command_for_dir(cmd, std::env::current_dir().ok())
}

fn run_memory_command_for_dir(
    cmd: MemorySubcommand,
    project_dir: Option<std::path::PathBuf>,
) -> Result<()> {
    use memory::{MemoryEntry, MemoryManager};

    let project_dir = project_dir.filter(|dir| !dir.as_os_str().is_empty());
    // Match agent-side memory resolution: project memory is available only when
    // the caller supplied a concrete working directory.
    let manager = match project_dir.as_ref() {
        Some(dir) => MemoryManager::new().with_project_dir(dir),
        _ => MemoryManager::new(),
    };

    match cmd {
        MemorySubcommand::List { scope, tag } => {
            let mut all_memories: Vec<MemoryEntry> = Vec::new();

            if (scope == "all" || scope == "project")
                && let Ok(graph) = manager.load_project_graph()
            {
                all_memories.extend(graph.all_memories().cloned());
            }
            if (scope == "all" || scope == "global")
                && let Ok(graph) = manager.load_global_graph()
            {
                all_memories.extend(graph.all_memories().cloned());
            }

            if let Some(tag_filter) = tag {
                all_memories.retain(|m| m.tags.contains(&tag_filter));
            }

            all_memories.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

            if all_memories.is_empty() {
                println!("No memories found.");
            } else {
                println!("Found {} memories:\n", all_memories.len());
                for entry in &all_memories {
                    let tags_str = if entry.tags.is_empty() {
                        String::new()
                    } else {
                        format!(" [{}]", entry.tags.join(", "))
                    };
                    let conf = entry.effective_confidence();
                    println!(
                        "- [{}] {}{}\n  id: {} (conf: {:.0}%, accessed: {}x)",
                        entry.category,
                        entry.content,
                        tags_str,
                        entry.id,
                        conf * 100.0,
                        entry.access_count
                    );
                    println!();
                }
            }
        }

        MemorySubcommand::Search { query, semantic } => {
            if semantic {
                match manager.find_similar(&query, 0.3, 20) {
                    Ok(results) => {
                        if results.is_empty() {
                            println!("No memories found matching '{}'", query);
                        } else {
                            println!(
                                "Found {} memories matching '{}' (semantic):\n",
                                results.len(),
                                query
                            );
                            for (entry, score) in results {
                                let tags_str = if entry.tags.is_empty() {
                                    String::new()
                                } else {
                                    format!(" [{}]", entry.tags.join(", "))
                                };
                                println!(
                                    "- [{}] {}{}\n  id: {} (score: {:.0}%)",
                                    entry.category,
                                    entry.content,
                                    tags_str,
                                    entry.id,
                                    score * 100.0
                                );
                                println!();
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Search failed: {}", e);
                    }
                }
            } else {
                match manager.search(&query) {
                    Ok(results) => {
                        if results.is_empty() {
                            println!("No memories found matching '{}'", query);
                        } else {
                            println!(
                                "Found {} memories matching '{}' (keyword):\n",
                                results.len(),
                                query
                            );
                            for entry in results {
                                let tags_str = if entry.tags.is_empty() {
                                    String::new()
                                } else {
                                    format!(" [{}]", entry.tags.join(", "))
                                };
                                println!(
                                    "- [{}] {}{}\n  id: {}",
                                    entry.category, entry.content, tags_str, entry.id
                                );
                                println!();
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Search failed: {}", e);
                    }
                }
            }
        }

        MemorySubcommand::Export { output, scope } => {
            let mut all_memories: Vec<memory::MemoryEntry> = Vec::new();

            if (scope == "all" || scope == "project")
                && let Ok(graph) = manager.load_project_graph()
            {
                all_memories.extend(graph.all_memories().cloned());
            }
            if (scope == "all" || scope == "global")
                && let Ok(graph) = manager.load_global_graph()
            {
                all_memories.extend(graph.all_memories().cloned());
            }

            let json = serde_json::to_string_pretty(&all_memories)?;
            std::fs::write(&output, json)?;
            println!("Exported {} memories to {}", all_memories.len(), output);
        }

        MemorySubcommand::Import {
            input,
            scope,
            overwrite,
        } => {
            if scope != "global" && project_dir.is_none() {
                anyhow::bail!("cannot import project memories without a project directory");
            }
            let content = std::fs::read_to_string(&input)?;
            let memories: Vec<memory::MemoryEntry> = serde_json::from_str(&content)?;

            let mut imported = 0;
            let mut skipped = 0;

            for entry in memories {
                let entry_id = entry.id.clone();
                let result = if scope == "global" {
                    if !overwrite
                        && let Ok(graph) = manager.load_global_graph()
                        && graph.get_memory(&entry.id).is_some()
                    {
                        skipped += 1;
                        continue;
                    }
                    manager.remember_global(entry)
                } else {
                    if !overwrite
                        && let Ok(graph) = manager.load_project_graph()
                        && graph.get_memory(&entry.id).is_some()
                    {
                        skipped += 1;
                        continue;
                    }
                    manager.remember_project(entry)
                };

                result.map_err(|error| {
                    anyhow::anyhow!(
                        "failed to durably import memory {entry_id} into {scope} scope: {error}"
                    )
                })?;
                imported += 1;
            }

            println!("Imported {} memories ({} skipped)", imported, skipped);
        }

        MemorySubcommand::Stats => {
            let mut project_count = 0;
            let mut global_count = 0;
            let mut total_tags = std::collections::HashSet::new();
            let mut categories: std::collections::HashMap<String, usize> =
                std::collections::HashMap::new();

            if let Ok(graph) = manager.load_project_graph() {
                project_count = graph.memory_count();
                for entry in graph.all_memories() {
                    for tag in &entry.tags {
                        total_tags.insert(tag.clone());
                    }
                    *categories.entry(entry.category.to_string()).or_default() += 1;
                }
            }

            if let Ok(graph) = manager.load_global_graph() {
                global_count = graph.memory_count();
                for entry in graph.all_memories() {
                    for tag in &entry.tags {
                        total_tags.insert(tag.clone());
                    }
                    *categories.entry(entry.category.to_string()).or_default() += 1;
                }
            }

            println!("Memory Statistics:");
            println!("  Project memories: {}", project_count);
            println!("  Global memories:  {}", global_count);
            println!("  Total:            {}", project_count + global_count);
            println!("  Unique tags:      {}", total_tags.len());
            println!("\nBy category:");
            for (cat, count) in &categories {
                println!("  {}: {}", cat, count);
            }
        }

        MemorySubcommand::ClearTest => {
            let test_dir = storage::jcode_dir()?.join("memory").join("test");
            if test_dir.exists() {
                let count = std::fs::read_dir(&test_dir)?.count();
                std::fs::remove_dir_all(&test_dir)?;
                println!("Cleared test memory storage ({} files)", count);
            } else {
                println!("Test memory storage is already empty");
            }
        }
    }

    Ok(())
}

pub async fn run_browser(action: &str) -> Result<()> {
    match action {
        "setup" => browser::run_setup_command().await?,
        "status" => {
            let status = browser::ensure_browser_ready_noninteractive().await?;
            println!("Browser automation");
            println!("  backend: {}", status.backend);
            println!("  browser: {}", status.browser);
            println!(
                "  binary: {}",
                if status.binary_installed {
                    "installed"
                } else {
                    "missing"
                }
            );
            println!(
                "  setup: {}",
                if status.setup_complete {
                    "complete"
                } else {
                    "not complete"
                }
            );
            println!(
                "  bridge: {}",
                if status.responding {
                    "responding"
                } else {
                    "not responding"
                }
            );
            println!(
                "  compatibility: {}",
                if status.compatible {
                    "ok"
                } else {
                    "extension/bridge mismatch"
                }
            );
            if !status.missing_actions.is_empty() {
                println!("  missing actions: {}", status.missing_actions.join(", "));
            }

            if status.ready {
                println!("\nBuilt-in browser tool is ready.");
            } else if status.responding && !status.compatible {
                println!(
                    "\nThe browser bridge is connected, but the installed Firefox extension is out of date for this jcode build. Run `jcode browser setup` to repair or update it."
                );
            } else if status.binary_installed && !browser::is_firefox_running() {
                println!(
                    "\nFirefox is not running, so the bridge cannot respond. Start Firefox (or run a browser tool action, which launches it automatically), then re-check status. Setup is one-time and does not need to be re-run."
                );
            } else if status.binary_installed {
                println!(
                    "\nFirefox is running, but the bridge is not responding. Check that the Browser Agent Bridge extension is enabled in the running profile. Run `jcode browser setup` only to repair the install."
                );
            } else {
                println!("\nRun `jcode browser setup` to install or repair it.");
            }
        }
        other => {
            eprintln!("Unknown browser action: {}", other);
            eprintln!("Available: setup, status");
            std::process::exit(1);
        }
    }
    Ok(())
}

#[derive(Debug, Serialize)]
struct ModelListReport {
    provider: String,
    selected_model: String,
    models: Vec<String>,
    routes: Vec<ModelListRouteReport>,
}

#[derive(Debug, Serialize)]
struct ModelListRouteReport {
    provider: String,
    model: String,
    method: String,
    available: bool,
}

#[derive(Debug, Serialize)]
struct RunCommandReport {
    session_id: String,
    provider: String,
    model: String,
    text: String,
    usage: crate::agent::TokenUsage,
}

#[derive(Debug, Default)]
struct NdjsonRunState {
    text: String,
    session_id: Option<String>,
    upstream_provider: Option<String>,
    connection_type: Option<String>,
    connection_phase: Option<String>,
    status_detail: Option<String>,
    usage: crate::agent::TokenUsage,
}

pub fn run_auth_status_command(emit_json: bool) -> Result<()> {
    report_info::run_auth_status_command(emit_json)
}

pub async fn run_auth_doctor_command(
    provider_arg: Option<&str>,
    validate: bool,
    emit_json: bool,
) -> Result<()> {
    report_info::run_auth_doctor_command(provider_arg, validate, emit_json).await
}

pub fn run_provider_list_command(emit_json: bool) -> Result<()> {
    report_info::run_provider_list_command(emit_json)
}

pub async fn run_provider_current_command(
    choice: &super::provider_init::ProviderChoice,
    model: Option<&str>,
    emit_json: bool,
) -> Result<()> {
    report_info::run_provider_current_command(choice, model, emit_json).await
}

pub fn run_version_command(emit_json: bool) -> Result<()> {
    report_info::run_version_command(emit_json)
}

pub async fn run_usage_command(emit_json: bool) -> Result<()> {
    report_info::run_usage_command(emit_json).await
}

/// Explicitly pin the daemon's shared-server channel to an installed build.
/// Promotion and reload intentionally remain separate operations: updates may
/// advance a shared server that tracks stable, but must not overwrite a build
/// the user deliberately promoted here.
pub fn run_server_promote_command(version: Option<&str>, emit_json: bool) -> Result<()> {
    #[derive(Serialize)]
    struct ServerPromoteReport {
        version: String,
        previous: Option<String>,
        binary: String,
        promoted: bool,
        detail: String,
    }

    let version = match version {
        Some(version) => version.to_string(),
        None => crate::build::read_current_version()?.ok_or_else(|| {
            anyhow::anyhow!("No current version is installed; pass an installed VERSION explicitly")
        })?,
    };
    let previous = crate::build::promote_version_to_shared_server(&version)?;
    let binary = crate::build::version_binary_path(&version)?;
    let promoted = previous.as_deref() != Some(version.as_str());
    let detail = if promoted {
        format!(
            "shared-server channel {} -> {}. Run `jcode server reload` to apply it to the running daemon.",
            previous.as_deref().unwrap_or("<unset>"),
            version
        )
    } else {
        format!(
            "shared-server channel already points to {}. Run `jcode server reload` if the running daemon has not applied it.",
            version
        )
    };
    let report = ServerPromoteReport {
        version,
        previous,
        binary: binary.display().to_string(),
        promoted,
        detail,
    };

    if emit_json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("{}", report.detail);
    }
    Ok(())
}

/// Gracefully reload the running background server onto the newest binary.
///
/// This is the preferred upgrade path (issue #291): instead of killing the
/// daemon and dropping live headless/swarm sessions, we ask it to hand its
/// sessions off to a freshly exec'd server (the same path `/reload` uses).
///
/// Behavior:
/// - With `force == false` (the default), the server only reloads when it is
///   provably running older code than an available reload candidate. A server
///   already on the newest binary reports "already up to date" and does
///   nothing, which keeps an installer from downgrading a newer/dev daemon or
///   re-entering the reload-loop family (#277).
/// - With `force == true`, the server reloads unconditionally.
/// - If no server is running, this is a successful no-op so installers can call
///   it unconditionally.
pub async fn run_server_reload_command(force: bool, emit_json: bool) -> Result<()> {
    use crate::protocol::ServerEvent;
    use std::time::Duration;

    let socket = crate::server::socket_path();

    #[derive(Serialize)]
    struct ServerReloadReport {
        socket: String,
        had_listener: bool,
        forced: bool,
        reloaded: bool,
        already_current: bool,
        handoff_ready: bool,
        detail: String,
    }

    let emit = |report: ServerReloadReport| -> Result<()> {
        if emit_json {
            println!("{}", serde_json::to_string_pretty(&report)?);
        } else if !report.detail.is_empty() {
            println!("{}", report.detail);
        }
        Ok(())
    };

    // No server? Nothing to reload. This is a success so an installer can call
    // `jcode server reload` unconditionally after swapping the binary.
    if !crate::server::has_live_listener(&socket).await {
        // Reap a stale socket left by a crashed daemon so the next launch binds
        // cleanly instead of wedging in a connect-retry loop.
        let reaped = crate::server::reap_stale_socket_if_dead(&socket).await;
        let detail = if reaped {
            "No running jcode server found; cleared a stale socket.".to_string()
        } else {
            "No running jcode server found; nothing to reload.".to_string()
        };
        return emit(ServerReloadReport {
            socket: socket.display().to_string(),
            had_listener: false,
            forced: force,
            reloaded: false,
            already_current: false,
            handoff_ready: false,
            detail,
        });
    }

    let mut client = crate::server::Client::connect().await?;

    // The server requires `Subscribe` as the first frame and rejects any other
    // opening request with "Client must Subscribe with a working_dir before
    // sending stateful requests", so `reload` alone always failed (issue #648).
    // `subscribe()` defaults `working_dir` to the current directory.
    client.subscribe().await?;

    // Before asking the (possibly older) daemon to reload, repair a stale
    // `shared-server` channel from the client side. The running server resolves
    // its reload target from that channel; if it still points at the server's
    // own old binary (the "current client, stale server" state, e.g. after a
    // no-op `/update`), a forced reload would just re-exec the same old binary.
    // Repointing shared-server -> stable when stable is strictly newer gives the
    // reload a newer binary to exec into. Never downgrades; preserves a fresher
    // self-dev pin. Best-effort: a failure here must not block the reload.
    match crate::build::repair_stale_shared_server_channel() {
        Ok(crate::build::SharedServerRepair::Repaired {
            repaired_to,
            previous,
        }) => {
            crate::logging::info(&format!(
                "server reload: repaired stale shared-server channel {:?} -> {} before reload",
                previous, repaired_to
            ));
        }
        Ok(crate::build::SharedServerRepair::AlreadyCurrent) => {}
        Err(err) => {
            crate::logging::warn(&format!(
                "server reload: shared-server channel repair failed (continuing): {}",
                err
            ));
        }
    }

    let request_id = client.reload_with_force(force).await?;

    let mut reloading = false;
    let mut skipped = false;

    // Drive the request to a terminal state. On a real reload the old server
    // exec's a new process, which drops this connection after it sends Done;
    // we treat a disconnect after observing Reloading as the expected handoff.
    loop {
        match client.read_event().await {
            Ok(ServerEvent::Ack { id }) if id == request_id => {}
            Ok(ServerEvent::Reloading { .. }) => {
                reloading = true;
            }
            Ok(ServerEvent::ReloadProgress { step, .. }) if step == "skip" => {
                skipped = true;
            }
            Ok(ServerEvent::ReloadProgress { .. }) => {}
            Ok(ServerEvent::Done { id }) if id == request_id => break,
            Ok(ServerEvent::Error { id, message, .. }) if id == request_id => {
                anyhow::bail!("server reload failed: {message}");
            }
            Ok(_) => {}
            Err(e) => {
                // A disconnect mid-reload is the expected handoff; otherwise it
                // is a genuine failure.
                if reloading {
                    break;
                }
                return Err(e);
            }
        }
    }

    if skipped && !reloading {
        return emit(ServerReloadReport {
            socket: socket.display().to_string(),
            had_listener: true,
            forced: force,
            reloaded: false,
            already_current: true,
            handoff_ready: true,
            detail: "jcode server is already running the newest binary; no reload needed."
                .to_string(),
        });
    }

    // Wait (bounded) for the freshly exec'd server to take over the socket so
    // callers know the upgrade actually landed.
    let handoff_ready = matches!(
        crate::server::await_reload_handoff(&socket, Duration::from_secs(30)).await,
        crate::server::ReloadWaitStatus::Ready
    );

    let detail = if handoff_ready {
        "jcode server reloaded onto the newest binary.".to_string()
    } else {
        "jcode server reload requested; the new server is still coming up.".to_string()
    };

    emit(ServerReloadReport {
        socket: socket.display().to_string(),
        had_listener: true,
        forced: force,
        reloaded: true,
        already_current: false,
        handoff_ready,
        detail,
    })
}

/// Stop the running background server gracefully and clear its socket.
///
/// Intended for use after an upgrade so the next launch starts the freshly
/// installed binary instead of a surviving daemon running old code (issue #291).
///
/// Steps:
/// 1. Look up the daemon owning the active socket in the server registry and
///    send it SIGTERM (the daemon has a graceful SIGTERM handler).
/// 2. Wait for the listener to go away (bounded), escalating to SIGKILL only if
///    the process refuses to exit.
/// 3. Reap any leftover stale socket so a later launch binds cleanly.
pub async fn run_server_stop_command(force: bool, emit_json: bool) -> Result<()> {
    use std::time::{Duration, Instant};

    if !force {
        let msg = "`jcode server stop` terminates the daemon and drops any live headless/swarm sessions. \
Prefer `jcode server reload` to pick up an upgrade gracefully. \
Re-run with `--force` if you really want to stop the server.";
        if emit_json {
            println!(
                "{}",
                serde_json::json!({
                    "stopped": false,
                    "force_required": true,
                    "detail": msg,
                })
            );
        } else {
            eprintln!("{msg}");
        }
        return Ok(());
    }

    let socket = crate::server::socket_path();
    let had_listener = crate::server::has_live_listener(&socket).await;
    let server_info = crate::registry::find_server_by_socket_sync(&socket);

    #[derive(Serialize)]
    struct ServerStopReport {
        socket: String,
        had_listener: bool,
        signaled_pid: Option<u32>,
        stopped: bool,
        reaped_socket: bool,
        detail: String,
    }

    let mut signaled_pid: Option<u32> = None;
    let mut stopped = false;
    let detail: String;

    if let Some(info) = server_info.as_ref() {
        let pid = info.pid;
        if crate::platform::is_process_running(pid) {
            {
                // The daemon spawns detached with setsid(), so it leads its own
                // process group. Signal the group so any helper children exit too.
                match crate::platform::signal_detached_process_group(pid, libc::SIGTERM) {
                    Ok(()) => {
                        signaled_pid = Some(pid);
                        detail = format!("Sent SIGTERM to jcode server (pid {pid}).");
                    }
                    Err(e) => {
                        detail = format!("Failed to signal jcode server (pid {pid}): {e}");
                    }
                }
            }
        } else {
            detail = format!("Registered jcode server (pid {pid}) is not running.");
        }
    } else if had_listener {
        // A listener answers but no registry entry maps to it. We deliberately
        // do not guess a pid; just reap the socket below once the listener is
        // gone. (This is rare: a daemon that bound the socket but never wrote a
        // registry entry.)
        detail = "Found a live server socket with no registry entry.".to_string();
    } else {
        detail = "No running jcode server found.".to_string();
    }

    // Wait for the listener to disappear after signalling. Escalate to SIGKILL
    // once if the daemon does not exit within the graceful window.
    if signaled_pid.is_some() || had_listener {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut escalated = false;
        loop {
            let listener_gone = !crate::server::has_live_listener(&socket).await;
            let process_gone = signaled_pid
                .map(|pid| !crate::platform::is_process_running(pid))
                .unwrap_or(true);
            if listener_gone && process_gone {
                stopped = true;
                break;
            }
            if Instant::now() >= deadline {
                break;
            }
            if !escalated
                && Instant::now() + Duration::from_secs(2) >= deadline
                && let Some(pid) = signaled_pid
                && crate::platform::is_process_running(pid)
            {
                let _ = crate::platform::signal_detached_process_group(pid, libc::SIGKILL);
                escalated = true;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    } else {
        stopped = true;
    }

    // Reap any stale socket the (now-dead) daemon left behind so the next launch
    // binds cleanly instead of wedging in a connect-retry loop.
    let reaped = crate::server::reap_stale_socket_if_dead(&socket).await;

    if emit_json {
        let report = ServerStopReport {
            socket: socket.display().to_string(),
            had_listener,
            signaled_pid,
            stopped,
            reaped_socket: reaped,
            detail: detail.clone(),
        };
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        if !detail.is_empty() {
            println!("{detail}");
        }
        if stopped && signaled_pid.is_some() {
            println!("jcode server stopped.");
        } else if stopped && !had_listener && signaled_pid.is_none() {
            // Nothing was running; this is still a success for an installer.
        } else if !stopped {
            println!(
                "jcode server did not exit cleanly; it may still be shutting down. Re-run if needed."
            );
        }
        if reaped {
            println!("Cleared a stale jcode socket.");
        }
    }

    Ok(())
}

pub async fn run_single_message_command(
    choice: &super::provider_init::ProviderChoice,
    model: Option<&str>,
    resume_session: Option<&str>,
    message: &str,
    emit_json: bool,
    emit_ndjson: bool,
) -> Result<()> {
    let provider = if emit_json || emit_ndjson {
        super::provider_init::init_provider_quiet(choice, model).await?
    } else {
        super::provider_init::init_provider_for_validation(choice, model).await?
    };
    let registry = crate::tool::Registry::new(provider.clone()).await;
    // Load MCP servers from ~/.jcode/mcp.json so headless `jcode run` has the
    // same `mcp__*` tools as interactive/server sessions. This is non-blocking:
    // `register_mcp_tools` advertises cached tool schemas synchronously (so the
    // first locked tool snapshot already contains MCP tools, for zero
    // prompt-cache miss) and connects in the background (connect-on-first-call).
    // For a short single-message run, startup latency is unchanged.
    // (#390, #206 Phase 2)
    if run_command_mcp_enabled() {
        registry.register_mcp_tools(None, None, None).await;
        // Cold-cache gap: when a configured MCP server has no cached schema yet
        // (first ever use, or reconfigured), advertise-early registers nothing
        // for it, and a single-turn `jcode run` locks its tool snapshot before
        // the background connection finishes, so the model would never see those
        // tools. Long-lived sessions recover on a later turn, but `jcode run`
        // has no later turn. So, only when the cache is cold for some configured
        // server, briefly wait for the first connection to register tools before
        // the agent runs. Warm runs skip this entirely and stay instant. (#390)
        wait_for_cold_cache_mcp_tools(&registry).await;
    }
    let mut agent = crate::agent::Agent::new(provider.clone(), registry);
    if let Err(error) = restore_agent_session_if_requested(&mut agent, resume_session) {
        agent.mark_closed();
        return Err(error);
    }

    run_single_message_with_agent(&mut agent, provider, message, emit_json, emit_ndjson).await
}

async fn run_single_message_with_agent(
    agent: &mut crate::agent::Agent,
    provider: std::sync::Arc<dyn crate::provider::Provider>,
    message: &str,
    emit_json: bool,
    emit_ndjson: bool,
) -> Result<()> {
    let result: Result<()> = async {
        if emit_json {
            let text = run_single_message_command_capture_with_auto_poke(agent, message).await?;
            let report = RunCommandReport {
                session_id: agent.session_id().to_string(),
                provider: provider.name().to_string(),
                model: provider.model(),
                text,
                usage: agent.last_usage().clone(),
            };
            println!("{}", serde_json::to_string_pretty(&report)?);
        } else if emit_ndjson {
            run_single_message_command_ndjson(agent, provider, message).await?;
        } else {
            run_single_message_command_plain_with_auto_poke(agent, message).await?;
        }
        Ok(())
    }
    .await;

    // `Agent::new` and session restore both register this process as the active
    // owner. Unlike the interactive lifecycle, `jcode run` has no later quit
    // path to close the session. Finalize after output has been emitted, while
    // returning the original command result unchanged. This prevents a normal
    // one-shot exit from looking like a stale-PID crash on the next startup
    // (issue #988).
    agent.mark_closed();
    result
}

fn run_command_auto_poke_enabled() -> bool {
    std::env::var("JCODE_RUN_AUTO_POKE")
        .ok()
        .map(|value| {
            let value = value.trim().to_ascii_lowercase();
            !matches!(value.as_str(), "0" | "false" | "off" | "no")
        })
        .unwrap_or_else(|| crate::config::config().features.auto_poke)
}

/// Whether headless `jcode run` should load MCP servers from `~/.jcode/mcp.json`.
/// Enabled by default; set `JCODE_RUN_MCP=0` (or `false`/`off`/`no`) to skip MCP
/// registration for latency-sensitive scripting. (#390)
fn run_command_mcp_enabled() -> bool {
    std::env::var("JCODE_RUN_MCP")
        .ok()
        .map(|value| {
            let value = value.trim().to_ascii_lowercase();
            !matches!(value.as_str(), "0" | "false" | "off" | "no")
        })
        .unwrap_or(true)
}

/// Max time `jcode run` waits for cold-cache MCP servers to register their
/// tools before running the single turn. Override with `JCODE_RUN_MCP_WAIT_MS`
/// (0 disables the wait).
fn run_command_mcp_cold_wait() -> std::time::Duration {
    let ms = std::env::var("JCODE_RUN_MCP_WAIT_MS")
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .unwrap_or(5000);
    std::time::Duration::from_millis(ms)
}

/// Returns the set of MCP servers configured for this run that have no usable
/// cached schema yet (cold cache). Advertise-early can only pre-register tools
/// for servers whose schemas are cached, so these are the servers whose tools
/// would otherwise miss the single-turn snapshot.
fn cold_cache_mcp_servers() -> Vec<String> {
    let config = crate::mcp::McpConfig::load();
    if config.servers.is_empty() {
        return Vec::new();
    }
    let cache = crate::mcp::McpSchemaCache::load();
    config
        .servers
        .iter()
        .filter(|(name, cfg)| cache.tools_for(name, cfg).is_none())
        .map(|(name, _)| name.clone())
        .collect()
}

/// Bridge the cold-cache gap for `jcode run`: if any configured MCP server has
/// no cached schema, briefly poll the registry until its `mcp__*` tools appear
/// (or the budget elapses) so the single turn's locked tool snapshot includes
/// them. Warm caches return immediately because `cold_cache_mcp_servers` is
/// empty. (#390)
async fn wait_for_cold_cache_mcp_tools(registry: &crate::tool::Registry) {
    let cold_servers = cold_cache_mcp_servers();
    if cold_servers.is_empty() {
        return;
    }
    let budget = run_command_mcp_cold_wait();
    if budget.is_zero() {
        return;
    }
    crate::logging::info(&format!(
        "jcode run: waiting up to {}ms for cold-cache MCP server(s) to register tools: {}",
        budget.as_millis(),
        cold_servers.join(", ")
    ));
    let deadline = std::time::Instant::now() + budget;
    loop {
        let names = registry.tool_names().await;
        let covered = cold_servers.iter().all(|server| {
            let prefix = format!("mcp__{}__", server);
            names.iter().any(|name| name.starts_with(&prefix))
        });
        if covered {
            crate::logging::info(
                "jcode run: cold-cache MCP server(s) registered tools; proceeding",
            );
            return;
        }
        if std::time::Instant::now() >= deadline {
            crate::logging::warn(
                "jcode run: timed out waiting for cold-cache MCP server(s); \
                 their tools may be missing from this run",
            );
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
}

fn run_command_auto_poke_max_turns() -> Option<usize> {
    std::env::var("JCODE_RUN_AUTO_POKE_MAX_TURNS")
        .ok()
        .and_then(|value| value.trim().parse::<usize>().ok())
        .filter(|value| *value > 0)
}

fn run_command_auto_poke_limit_reached(turns_completed: usize, max_turns: Option<usize>) -> bool {
    max_turns
        .map(|max_turns| turns_completed >= max_turns)
        .unwrap_or(false)
}

#[derive(Debug)]
enum RunAutoPokeFollowUp {
    Incomplete {
        count: usize,
        message: String,
    },
    ConfidenceSummary {
        total_todos: usize,
        message: String,
        confidence_spike_challenge: bool,
    },
    /// Deferred quality-check reminder for the points this turn flagged and
    /// never resolved. Delivered once, ahead of the confidence summary.
    GateDigest {
        message: String,
    },
}

fn run_todos(session_id: &str) -> Vec<crate::todo::TodoItem> {
    crate::todo::load_todos(session_id).unwrap_or_default()
}

/// Build the deferred quality-check reminder for a headless run, consuming the
/// turn's observation log.
///
/// The log is cleared whether or not a reminder results, so one turn's points
/// cannot be raised again against the next turn's work. Returns `None` only when
/// the turn recorded nothing.
fn take_run_gate_digest(session_id: &str, already_delivered: bool) -> Option<String> {
    if already_delivered {
        return None;
    }
    let observations = crate::todo::load_gate_observations(session_id).unwrap_or_default();
    if observations.is_empty() {
        return None;
    }
    let plan = crate::todo::load_plan(session_id).unwrap_or_default();
    let goals = crate::todo::load_goals(session_id).unwrap_or_default();
    let digest = crate::todo::build_gate_digest(&observations, &plan, &goals);
    let _ = crate::todo::clear_gate_observations(session_id);
    digest
}

/// Consume the observation log only once the turn has actually ended.
///
/// `take_run_gate_digest` clears the log, so calling it while todos are still
/// open would destroy the reminder: auto-poke iterates many times with open work
/// on a long run, and the incomplete-todo follow-up takes precedence, so the
/// digest string would be dropped on the floor with the log already emptied.
fn take_run_gate_digest_if_turn_ended(
    session_id: &str,
    already_delivered: bool,
    todos: &[crate::todo::TodoItem],
) -> Option<String> {
    let work_remains = todos.iter().any(|todo| {
        !crate::todo::todo_status_is_completed(&todo.status)
            && !crate::todo::todo_status_is_cancelled(&todo.status)
    });
    if work_remains {
        return None;
    }
    take_run_gate_digest(session_id, already_delivered)
}

fn build_run_auto_poke_follow_up_from_todos(
    todos: &[crate::todo::TodoItem],
    confidence_spike_challenged: bool,
    gate_digest: Option<String>,
) -> Option<RunAutoPokeFollowUp> {
    let incomplete: Vec<_> = todos
        .iter()
        .filter(|todo| {
            !crate::todo::todo_status_is_completed(&todo.status)
                && !crate::todo::todo_status_is_cancelled(&todo.status)
        })
        .cloned()
        .collect();
    if !incomplete.is_empty() {
        return Some(RunAutoPokeFollowUp::Incomplete {
            count: incomplete.len(),
            message: build_run_poke_message(&incomplete),
        });
    }
    // Verify the weak points before judging completion confidence: the digest
    // may prompt work that changes those very assessments.
    if let Some(message) = gate_digest {
        return Some(RunAutoPokeFollowUp::GateDigest { message });
    }
    if !todos.is_empty()
        && let Some((message, confidence_spike_challenge)) =
            build_run_todo_validation_message(todos, !confidence_spike_challenged)
    {
        return Some(RunAutoPokeFollowUp::ConfidenceSummary {
            total_todos: todos.len(),
            message,
            confidence_spike_challenge,
        });
    }
    None
}

fn build_run_poke_message(incomplete: &[crate::todo::TodoItem]) -> String {
    crate::todo::build_auto_poke_message(incomplete.len())
}

fn build_run_todo_validation_message(
    todos: &[crate::todo::TodoItem],
    allow_confidence_spike_challenge: bool,
) -> Option<(String, bool)> {
    let completed: Vec<&crate::todo::TodoItem> = todos
        .iter()
        .filter(|todo| crate::todo::todo_status_is_completed(&todo.status))
        .collect();
    if completed.is_empty() {
        return None;
    }

    let completion_confidence_needs_validation = completed
        .iter()
        .any(|todo| !crate::todo::completion_confidence_passes(todo.completion_confidence));
    let confidence_spike_detected =
        allow_confidence_spike_challenge && !crate::todo::spike_completed_todos(todos).is_empty();

    if !completion_confidence_needs_validation && !confidence_spike_detected {
        // Nothing actionable: completing the loop with a generic summary just
        // spends tokens on "all good" theater, so send nothing and end the run.
        return None;
    }

    if completion_confidence_needs_validation {
        Some((
            crate::todo::build_todo_completion_continuation_message(todos),
            false,
        ))
    } else {
        Some((
            crate::todo::build_todo_confidence_spike_continuation_message(todos),
            true,
        ))
    }
}

async fn run_single_message_command_plain_with_auto_poke(
    agent: &mut crate::agent::Agent,
    message: &str,
) -> Result<()> {
    let mut next_message = message.to_string();
    let max_turns = run_command_auto_poke_max_turns();
    let mut turns_completed = 0usize;
    let mut confidence_spike_challenged = false;
    let mut gate_digest_delivered = false;
    loop {
        agent.run_once(&next_message).await?;
        turns_completed += 1;
        if !run_command_auto_poke_enabled() {
            break;
        }
        let todos = run_todos(agent.session_id());
        let gate_digest =
            take_run_gate_digest_if_turn_ended(agent.session_id(), gate_digest_delivered, &todos);
        match build_run_auto_poke_follow_up_from_todos(
            &todos,
            confidence_spike_challenged,
            gate_digest,
        ) {
            Some(RunAutoPokeFollowUp::GateDigest { message }) => {
                if run_command_auto_poke_limit_reached(turns_completed, max_turns) {
                    if let Some(max_turns) = max_turns {
                        eprintln!(
                            "We stopped poking after {max_turns} turn(s); some quality-review points are still open."
                        );
                    }
                    break;
                }
                gate_digest_delivered = true;
                next_message = message;
                eprintln!(
                    "We asked the agent to double-check this turn's weak points. Set JCODE_RUN_AUTO_POKE=0 to disable."
                );
                continue;
            }
            Some(RunAutoPokeFollowUp::ConfidenceSummary {
                message,
                confidence_spike_challenge,
                ..
            }) => {
                if run_command_auto_poke_limit_reached(turns_completed, max_turns) {
                    if let Some(max_turns) = max_turns {
                        eprintln!(
                            "We stopped poking after {max_turns} turn(s); the agent's completion confidence still needs validation."
                        );
                    }
                    break;
                }
                confidence_spike_challenged |= confidence_spike_challenge;
                next_message = message;
                eprintln!(
                    "Todos are done. Asking the agent for a final confidence check. Set JCODE_RUN_AUTO_POKE=0 to disable."
                );
                continue;
            }
            Some(RunAutoPokeFollowUp::Incomplete { count, message }) => {
                if run_command_auto_poke_limit_reached(turns_completed, max_turns) {
                    if let Some(max_turns) = max_turns {
                        eprintln!(
                            "We stopped poking after {max_turns} turn(s); {} todo(s) are still unfinished.",
                            count
                        );
                    }
                    break;
                }
                next_message = message;
                eprintln!(
                    "{} incomplete todo(s). We poked the agent for you. Set JCODE_RUN_AUTO_POKE=0 to disable.",
                    count
                );
            }
            None => break,
        }
    }
    Ok(())
}

async fn run_single_message_command_capture_with_auto_poke(
    agent: &mut crate::agent::Agent,
    message: &str,
) -> Result<String> {
    let mut next_message = message.to_string();
    let max_turns = run_command_auto_poke_max_turns();
    let mut outputs = Vec::new();
    let mut turns_completed = 0usize;
    let mut confidence_spike_challenged = false;
    let mut gate_digest_delivered = false;
    loop {
        outputs.push(agent.run_once_capture(&next_message).await?);
        turns_completed += 1;
        if !run_command_auto_poke_enabled() {
            break;
        }
        let todos = run_todos(agent.session_id());
        let gate_digest =
            take_run_gate_digest_if_turn_ended(agent.session_id(), gate_digest_delivered, &todos);
        match build_run_auto_poke_follow_up_from_todos(
            &todos,
            confidence_spike_challenged,
            gate_digest,
        ) {
            Some(RunAutoPokeFollowUp::GateDigest { message }) => {
                if run_command_auto_poke_limit_reached(turns_completed, max_turns) {
                    if let Some(max_turns) = max_turns {
                        eprintln!(
                            "We stopped poking after {max_turns} turn(s); some quality-review points are still open."
                        );
                    }
                    break;
                }
                gate_digest_delivered = true;
                next_message = message;
                eprintln!(
                    "We asked the agent to double-check this turn's weak points. Set JCODE_RUN_AUTO_POKE=0 to disable."
                );
                continue;
            }
            Some(RunAutoPokeFollowUp::ConfidenceSummary {
                message,
                confidence_spike_challenge,
                ..
            }) => {
                if run_command_auto_poke_limit_reached(turns_completed, max_turns) {
                    if let Some(max_turns) = max_turns {
                        outputs.push(format!(
                            "We stopped poking after {max_turns} turn(s); the agent's completion confidence still needs validation."
                        ));
                    }
                    break;
                }
                confidence_spike_challenged |= confidence_spike_challenge;
                next_message = message;
                continue;
            }
            Some(RunAutoPokeFollowUp::Incomplete { count, message }) => {
                if run_command_auto_poke_limit_reached(turns_completed, max_turns) {
                    if let Some(max_turns) = max_turns {
                        outputs.push(format!(
                            "We stopped poking after {max_turns} turn(s); {} todo(s) are still unfinished.",
                            count
                        ));
                    }
                    break;
                }
                next_message = message;
            }
            None => break,
        }
    }
    Ok(outputs.join("\n\n"))
}

fn restore_agent_session_if_requested(
    agent: &mut crate::agent::Agent,
    resume_session: Option<&str>,
) -> Result<()> {
    if let Some(session_id) = resume_session {
        agent.restore_session(session_id)?;
    }
    Ok(())
}

async fn run_single_message_command_ndjson(
    agent: &mut crate::agent::Agent,
    provider: std::sync::Arc<dyn crate::provider::Provider>,
    message: &str,
) -> Result<()> {
    let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel();
    let session_id = agent.session_id().to_string();
    let mut stdout = std::io::stdout().lock();
    let mut state = NdjsonRunState {
        session_id: Some(session_id.clone()),
        ..NdjsonRunState::default()
    };
    write_json_line(
        &mut stdout,
        &serde_json::json!({
            "type": "start",
            "session_id": session_id,
            "provider": provider.name(),
            "model": provider.model(),
        }),
    )?;

    let max_turns = run_command_auto_poke_max_turns();
    let mut next_message = message.to_string();
    let mut result: Result<()> = Ok(());
    let mut turns_completed = 0usize;
    let mut confidence_spike_challenged = false;
    let mut gate_digest_delivered = false;
    loop {
        let turn_result = {
            let mut run_future = std::pin::pin!(agent.run_once_streaming_mpsc(
                &next_message,
                Vec::new(),
                None,
                event_tx.clone(),
            ));
            let mut run_result: Option<Result<()>> = None;
            loop {
                tokio::select! {
                    result = &mut run_future, if run_result.is_none() => {
                        run_result = Some(result);
                    }
                    event = event_rx.recv() => {
                        match event {
                            Some(event) => emit_ndjson_event(&mut stdout, &mut state, event)?,
                            None => break,
                        }
                    }
                }
                if run_result.is_some() {
                    while let Ok(event) = event_rx.try_recv() {
                        emit_ndjson_event(&mut stdout, &mut state, event)?;
                    }
                    break;
                }
            }
            run_result.unwrap_or(Ok(()))
        };

        if let Err(err) = turn_result {
            result = Err(err);
            break;
        }
        turns_completed += 1;
        if !run_command_auto_poke_enabled() {
            break;
        }
        let todos = run_todos(&session_id);
        let gate_digest =
            take_run_gate_digest_if_turn_ended(agent.session_id(), gate_digest_delivered, &todos);
        match build_run_auto_poke_follow_up_from_todos(
            &todos,
            confidence_spike_challenged,
            gate_digest,
        ) {
            Some(RunAutoPokeFollowUp::GateDigest { message }) => {
                if run_command_auto_poke_limit_reached(turns_completed, max_turns) {
                    if let Some(max_turns) = max_turns {
                        eprintln!(
                            "We stopped poking after {max_turns} turn(s); some quality-review points are still open."
                        );
                    }
                    break;
                }
                gate_digest_delivered = true;
                next_message = message;
                eprintln!(
                    "We asked the agent to double-check this turn's weak points. Set JCODE_RUN_AUTO_POKE=0 to disable."
                );
                continue;
            }
            Some(RunAutoPokeFollowUp::ConfidenceSummary {
                total_todos,
                message,
                confidence_spike_challenge,
            }) => {
                if run_command_auto_poke_limit_reached(turns_completed, max_turns) {
                    if let Some(max_turns) = max_turns {
                        write_json_line(
                            &mut stdout,
                            &serde_json::json!({
                                "type": "auto_poke_stopped",
                                "session_id": session_id,
                                "completion_confidence_needs_validation": true,
                                "max_turns": max_turns,
                            }),
                        )?;
                    }
                    break;
                }
                confidence_spike_challenged |= confidence_spike_challenge;
                next_message = message;
                write_json_line(
                    &mut stdout,
                    &serde_json::json!({
                        "type": "auto_poke_confidence_summary",
                        "session_id": session_id,
                        "todos": total_todos,
                        "confidence_spike_challenge": confidence_spike_challenge,
                        "message": next_message,
                    }),
                )?;
                continue;
            }
            Some(RunAutoPokeFollowUp::Incomplete { count, message }) => {
                if run_command_auto_poke_limit_reached(turns_completed, max_turns) {
                    if let Some(max_turns) = max_turns {
                        write_json_line(
                            &mut stdout,
                            &serde_json::json!({
                                "type": "auto_poke_stopped",
                                "session_id": session_id,
                                "incomplete_todos": count,
                                "max_turns": max_turns,
                            }),
                        )?;
                    }
                    break;
                }
                next_message = message;
                write_json_line(
                    &mut stdout,
                    &serde_json::json!({
                        "type": "auto_poke",
                        "session_id": session_id,
                        "incomplete_todos": count,
                        "message": next_message,
                    }),
                )?;
            }
            None => break,
        }
    }

    match result {
        Ok(()) => {
            write_json_line(
                &mut stdout,
                &serde_json::json!({
                    "type": "done",
                    "session_id": session_id,
                    "provider": provider.name(),
                    "model": provider.model(),
                    "text": state.text,
                    "usage": state.usage,
                    "upstream_provider": state.upstream_provider,
                    "connection_type": state.connection_type,
                    "connection_phase": state.connection_phase,
                    "status_detail": state.status_detail,
                }),
            )?;
            Ok(())
        }
        Err(err) => {
            write_json_line(
                &mut stdout,
                &serde_json::json!({
                    "type": "error",
                    "session_id": session_id,
                    "provider": provider.name(),
                    "model": provider.model(),
                    "message": format!("{err:#}"),
                }),
            )?;
            Err(err)
        }
    }
}

fn emit_ndjson_event(
    stdout: &mut impl Write,
    state: &mut NdjsonRunState,
    event: crate::protocol::ServerEvent,
) -> Result<()> {
    use crate::protocol::ServerEvent;

    match event {
        ServerEvent::TextDelta { text } => {
            state.text.push_str(&text);
            write_json_line(
                stdout,
                &serde_json::json!({ "type": "text_delta", "text": text }),
            )
        }
        ServerEvent::TextReplace { text } => {
            state.text = text.clone();
            write_json_line(
                stdout,
                &serde_json::json!({ "type": "text_replace", "text": text }),
            )
        }
        ServerEvent::ToolStart { id, name } => write_json_line(
            stdout,
            &serde_json::json!({ "type": "tool_start", "id": id, "name": name }),
        ),
        ServerEvent::ToolInput { delta } => write_json_line(
            stdout,
            &serde_json::json!({ "type": "tool_input", "delta": delta }),
        ),
        ServerEvent::ToolExec { id, name } => write_json_line(
            stdout,
            &serde_json::json!({ "type": "tool_exec", "id": id, "name": name }),
        ),
        ServerEvent::ToolDone {
            id,
            name,
            output,
            error,
        } => write_json_line(
            stdout,
            &serde_json::json!({
                "type": "tool_done",
                "id": id,
                "name": name,
                "output": output,
                "error": error,
            }),
        ),
        ServerEvent::TokenUsage {
            input,
            output,
            cache_read_input,
            cache_creation_input,
        } => {
            state.usage = crate::agent::TokenUsage {
                input_tokens: input,
                output_tokens: output,
                cache_read_input_tokens: cache_read_input,
                cache_creation_input_tokens: cache_creation_input,
            };
            write_json_line(
                stdout,
                &serde_json::json!({
                    "type": "tokens",
                    "input": input,
                    "output": output,
                    "cache_read_input": cache_read_input,
                    "cache_creation_input": cache_creation_input,
                }),
            )
        }
        ServerEvent::ConnectionType { connection } => {
            state.connection_type = Some(connection.clone());
            write_json_line(
                stdout,
                &serde_json::json!({ "type": "connection_type", "connection": connection }),
            )
        }
        ServerEvent::ConnectionPhase { phase } => {
            state.connection_phase = Some(phase.clone());
            write_json_line(
                stdout,
                &serde_json::json!({ "type": "connection_phase", "phase": phase }),
            )
        }
        ServerEvent::StatusDetail { detail } => {
            state.status_detail = Some(detail.clone());
            write_json_line(
                stdout,
                &serde_json::json!({ "type": "status_detail", "detail": detail }),
            )
        }
        ServerEvent::MessageEnd { stop_reason } => write_json_line(
            stdout,
            &serde_json::json!({ "type": "message_end", "stop_reason": stop_reason }),
        ),
        ServerEvent::UpstreamProvider { provider } => {
            state.upstream_provider = Some(provider.clone());
            write_json_line(
                stdout,
                &serde_json::json!({ "type": "upstream_provider", "provider": provider }),
            )
        }
        ServerEvent::SessionId { session_id } => {
            state.session_id = Some(session_id.clone());
            write_json_line(
                stdout,
                &serde_json::json!({ "type": "session", "session_id": session_id }),
            )
        }
        ServerEvent::Compaction {
            trigger,
            pre_tokens,
            messages_dropped,
            post_tokens,
            tokens_saved,
            duration_ms,
            messages_compacted,
            summary_chars,
            active_messages,
        } => write_json_line(
            stdout,
            &serde_json::json!({
                "type": "compaction",
                "trigger": trigger,
                "pre_tokens": pre_tokens,
                "messages_dropped": messages_dropped,
                "post_tokens": post_tokens,
                "tokens_saved": tokens_saved,
                "duration_ms": duration_ms,
                "messages_compacted": messages_compacted,
                "summary_chars": summary_chars,
                "active_messages": active_messages,
            }),
        ),
        ServerEvent::MemoryInjected {
            count,
            prompt_chars,
            computed_age_ms,
            ..
        } => write_json_line(
            stdout,
            &serde_json::json!({
                "type": "memory_injected",
                "count": count,
                "prompt_chars": prompt_chars,
                "computed_age_ms": computed_age_ms,
            }),
        ),
        ServerEvent::Interrupted => {
            write_json_line(stdout, &serde_json::json!({ "type": "interrupted" }))
        }
        ServerEvent::SoftInterruptInjected {
            content,
            display_role,
            point,
            tools_skipped,
        } => write_json_line(
            stdout,
            &serde_json::json!({
                "type": "soft_interrupt_injected",
                "content": content,
                "display_role": display_role,
                "point": point,
                "tools_skipped": tools_skipped,
            }),
        ),
        ServerEvent::BatchProgress { progress } => write_json_line(
            stdout,
            &serde_json::json!({ "type": "batch_progress", "progress": progress }),
        ),
        ServerEvent::Error {
            message,
            retry_after_secs,
            ..
        } => write_json_line(
            stdout,
            &serde_json::json!({
                "type": "error",
                "message": message,
                "retry_after_secs": retry_after_secs,
            }),
        ),
        ServerEvent::Ack { .. } | ServerEvent::Done { .. } | ServerEvent::Pong { .. } => Ok(()),
        _ => Ok(()),
    }
}

fn write_json_line(stdout: &mut impl Write, value: &impl Serialize) -> Result<()> {
    serde_json::to_writer(&mut *stdout, value)?;
    stdout.write_all(b"\n")?;
    stdout.flush()?;
    Ok(())
}

pub async fn run_model_command(
    choice: &super::provider_init::ProviderChoice,
    model: Option<&str>,
    emit_json: bool,
    verbose: bool,
) -> Result<()> {
    let provider = super::provider_init::init_provider_quiet(choice, model).await?;

    if let Err(err) = provider.prefetch_models().await
        && !super::output::quiet_enabled()
    {
        eprintln!("Warning: failed to refresh dynamic model list: {}", err);
    }

    let routes = provider.model_routes();
    let filtered_routes = filter_cli_model_routes_for_choice(choice, &routes);
    let models = if filtered_routes.len() == routes.len() {
        collect_cli_model_names(&routes, provider.available_models_display())
    } else {
        collect_cli_model_names(&filtered_routes, Vec::new())
    };

    if models.is_empty() {
        anyhow::bail!(
            "No models found for provider '{}'. Check credentials or try a different --provider.",
            provider.name()
        );
    }

    if emit_json {
        let provider_label = super::provider_init::login_provider_for_choice(choice)
            .map(|provider| provider.display_name.to_string())
            .unwrap_or_else(|| {
                crate::provider_catalog::runtime_provider_display_name(provider.name())
            });
        let report = ModelListReport {
            provider: provider_label,
            selected_model: provider.model(),
            models,
            routes: filtered_routes
                .iter()
                .map(|route| ModelListRouteReport {
                    provider: cli_route_provider_display(&route.provider, &route.api_method),
                    model: route.model.clone(),
                    method: cli_api_method_display(&route.api_method),
                    available: route.available,
                })
                .collect(),
        };
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        if verbose {
            println!(
                "Provider: {}",
                crate::provider_catalog::runtime_provider_display_name(provider.name())
            );
            println!("Selected model: {}", provider.model());
            println!("Available models: {}", models.len());
            println!();
        }
        for model in models {
            println!("{}", model);
        }
    }

    Ok(())
}

fn cli_api_method_display(raw: &str) -> String {
    crate::provider::ModelRouteApiMethod::parse(raw).display_label()
}

fn cli_route_provider_display(provider: &str, api_method: &str) -> String {
    if crate::provider::ModelRouteApiMethod::parse(api_method).is_openrouter()
        && provider != "auto"
        && !provider.contains("OpenRouter")
    {
        format!("OpenRouter/{}", provider)
    } else {
        provider.to_string()
    }
}

fn collect_cli_model_names(
    routes: &[crate::provider::ModelRoute],
    display_models: Vec<String>,
) -> Vec<String> {
    let mut deduped = Vec::new();
    let mut seen = BTreeSet::new();

    fn push_model(deduped: &mut Vec<String>, seen: &mut BTreeSet<String>, model: &str) {
        let trimmed = model.trim();
        if !crate::provider::is_listable_model_name(trimmed) {
            return;
        }
        if seen.insert(trimmed.to_string()) {
            deduped.push(trimmed.to_string());
        }
    }

    for route in routes.iter().filter(|route| route.available) {
        push_model(&mut deduped, &mut seen, &route.model);
    }

    if deduped.is_empty() {
        for route in routes {
            push_model(&mut deduped, &mut seen, &route.model);
        }
    }

    for model in display_models {
        push_model(&mut deduped, &mut seen, &model);
    }

    deduped
}

#[allow(deprecated)]
fn filter_cli_model_routes_for_choice(
    choice: &super::provider_init::ProviderChoice,
    routes: &[crate::provider::ModelRoute],
) -> Vec<crate::provider::ModelRoute> {
    use super::provider_init::ProviderChoice;

    let keep = |route: &&crate::provider::ModelRoute| match choice {
        ProviderChoice::Claude | ProviderChoice::ClaudeSubprocess => {
            route.api_method_kind().is_anthropic_credential_route()
        }
        ProviderChoice::Openai => {
            let method = route.api_method_kind();
            matches!(method, crate::provider::ModelRouteApiMethod::OpenAIOAuth)
                || matches!(method, crate::provider::ModelRouteApiMethod::Other(ref value) if value == "chatgpt-web")
        }
        ProviderChoice::OpenaiApi => matches!(
            route.api_method_kind(),
            crate::provider::ModelRouteApiMethod::OpenAIApiKey
        ),
        ProviderChoice::Openrouter | ProviderChoice::Azure => {
            route.api_method_kind().is_openrouter()
        }
        ProviderChoice::Copilot => route.api_method_kind().is_copilot(),
        _ => true,
    };

    let filtered: Vec<_> = routes.iter().filter(keep).cloned().collect();
    if filtered.is_empty() {
        routes.to_vec()
    } else {
        filtered
    }
}
#[cfg(test)]
#[path = "commands_tests.rs"]
mod tests;
