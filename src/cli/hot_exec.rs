use anyhow::Result;
use std::process::Command as ProcessCommand;

use crate::{build, update};

pub fn hot_reload(session_id: &str) -> Result<()> {
    let cwd = std::env::current_dir()?;

    std::env::set_var("JCODE_RESUMING", "1");

    if let Ok(migrate_binary) = std::env::var("JCODE_MIGRATE_BINARY") {
        let binary_path = std::path::PathBuf::from(&migrate_binary);
        if binary_path.exists() {
            crate::logging::info("Migrating to stable binary...");
            let err = crate::platform::replace_process(
                ProcessCommand::new(&binary_path)
                    .arg("--resume")
                    .arg(session_id)
                    .arg("--no-update")
                    .current_dir(cwd),
            );
            return Err(anyhow::anyhow!("Failed to exec {:?}: {}", binary_path, err));
        } else {
            crate::logging::warn(&format!(
                "Migration binary not found at {:?}, falling back to local binary",
                binary_path
            ));
        }
    }

    let is_selfdev = std::env::var("JCODE_SELFDEV_MODE").is_ok();
    let (exe, _label) = build::client_update_candidate(is_selfdev)
        .ok_or_else(|| anyhow::anyhow!("No reloadable binary found"))?;

    if let Ok(metadata) = std::fs::metadata(&exe) {
        let age = metadata
            .modified()
            .ok()
            .and_then(|m| m.elapsed().ok())
            .map(|d| {
                let secs = d.as_secs();
                if secs < 60 {
                    format!("{} seconds ago", secs)
                } else if secs < 3600 {
                    format!("{} minutes ago", secs / 60)
                } else {
                    format!("{} hours ago", secs / 3600)
                }
            })
            .unwrap_or_else(|| "unknown".to_string());
        crate::logging::info(&format!("Reloading with binary built {}...", age));
    }

    for attempt in 0..3 {
        if attempt > 0 {
            std::thread::sleep(std::time::Duration::from_millis(200));
            if !exe.exists() {
                continue;
            }
        }
        let mut cmd = ProcessCommand::new(&exe);
        if is_selfdev {
            cmd.arg("self-dev");
        }
        cmd.arg("--resume").arg(session_id).current_dir(&cwd);
        let err = crate::platform::replace_process(&mut cmd);

        if err.kind() == std::io::ErrorKind::NotFound && attempt < 2 {
            crate::logging::warn(&format!(
                "exec attempt {} failed (ENOENT) for {:?}, retrying...",
                attempt + 1,
                exe
            ));
            continue;
        }
        return Err(anyhow::anyhow!("Failed to exec {:?}: {}", exe, err));
    }
    Err(anyhow::anyhow!(
        "Failed to exec {:?}: binary not found after retries",
        exe
    ))
}

pub fn hot_rebuild(session_id: &str) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let repo_dir =
        build::get_repo_dir().ok_or_else(|| anyhow::anyhow!("Could not find jcode repository"))?;

    eprintln!("Rebuilding jcode with session {}...", session_id);

    eprintln!("Pulling latest changes...");
    if let Err(e) = update::run_git_pull_ff_only(&repo_dir, true) {
        eprintln!("Warning: {}. Continuing with current version.", e);
    }

    eprintln!("Building...");
    let build_status = ProcessCommand::new("cargo")
        .args(["build", "--release"])
        .current_dir(&repo_dir)
        .status()?;

    if !build_status.success() {
        anyhow::bail!("Build failed - staying on current version");
    }

    eprintln!("Running tests...");
    let test = ProcessCommand::new("cargo")
        .args(["test", "--release", "--", "--test-threads=1"])
        .current_dir(&repo_dir)
        .status()?;

    if !test.success() {
        eprintln!("\n⚠️  Tests failed! Aborting reload to protect your session.");
        eprintln!("Fix the failing tests and try /rebuild again.");
        anyhow::bail!("Tests failed - staying on current version");
    }

    eprintln!("✓ All tests passed");

    if let Err(e) = build::install_local_release(&repo_dir) {
        eprintln!("Warning: install failed: {}", e);
    }

    let is_selfdev = std::env::var("JCODE_SELFDEV_MODE").is_ok();
    let exe = build::client_update_candidate(is_selfdev)
        .map(|(path, _)| path)
        .unwrap_or_else(|| build::release_binary_path(&repo_dir));
    if !exe.exists() {
        anyhow::bail!("Binary not found at {:?}", exe);
    }

    update::print_centered(&format!("Restarting with session {}...", session_id));

    std::env::set_var("JCODE_RESUMING", "1");

    let mut cmd = ProcessCommand::new(&exe);
    if is_selfdev {
        cmd.arg("self-dev");
    }
    cmd.arg("--resume").arg(session_id).current_dir(&cwd);
    let err = crate::platform::replace_process(&mut cmd);

    Err(anyhow::anyhow!("Failed to exec {:?}: {}", exe, err))
}

pub fn hot_update(session_id: &str) -> Result<()> {
    let cwd = std::env::current_dir()?;

    update::print_centered("Checking for updates...");

    match update::check_for_update_blocking() {
        Ok(Some(release)) => {
            let current = env!("JCODE_VERSION");
            update::print_centered(&format!(
                "Update available: {} -> {}",
                current, release.tag_name
            ));
            update::print_centered(&format!("Downloading {}...", release.tag_name));

            match update::download_and_install_blocking(&release) {
                Ok(path) => {
                    update::print_centered(&format!("✓ Installed {}", release.tag_name));

                    let is_selfdev = std::env::var("JCODE_SELFDEV_MODE").is_ok();
                    let exe = build::client_update_candidate(is_selfdev)
                        .map(|(p, _)| p)
                        .unwrap_or(path);

                    update::print_centered(&format!("Restarting with session {}...", session_id));

                    std::env::set_var("JCODE_RESUMING", "1");

                    let mut cmd = ProcessCommand::new(&exe);
                    if is_selfdev {
                        cmd.arg("self-dev");
                    }
                    cmd.arg("--resume")
                        .arg(session_id)
                        .arg("--no-update")
                        .current_dir(&cwd);
                    let err = crate::platform::replace_process(&mut cmd);
                    return Err(anyhow::anyhow!("Failed to exec {:?}: {}", exe, err));
                }
                Err(e) => {
                    update::print_centered(&format!("✗ Download failed: {}", e));
                    update::print_centered("Resuming session with current version...");
                }
            }
        }
        Ok(None) => {
            update::print_centered(&format!("Already up to date ({})", env!("JCODE_VERSION")));
        }
        Err(e) => {
            update::print_centered(&format!("✗ Update check failed: {}", e));
            update::print_centered("Resuming session with current version...");
        }
    }

    std::env::set_var("JCODE_RESUMING", "1");
    let exe = std::env::current_exe()?;
    let is_selfdev = std::env::var("JCODE_SELFDEV_MODE").is_ok();
    let mut cmd = ProcessCommand::new(&exe);
    if is_selfdev {
        cmd.arg("self-dev");
    }
    cmd.arg("--resume")
        .arg(session_id)
        .arg("--no-update")
        .current_dir(&cwd);
    let err = crate::platform::replace_process(&mut cmd);
    Err(anyhow::anyhow!("Failed to exec {:?}: {}", exe, err))
}
