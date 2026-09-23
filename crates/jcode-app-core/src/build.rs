//! Repository discovery and source-state helpers.
//!
//! The self-dev build/install/update/launcher machinery that used to live in
//! the `jcode-build-support` crate is gone: the operating system package
//! manager owns installing and updating the binary, so jcode no longer builds
//! or installs itself. What remains here is the small set of helpers the kernel
//! still needs to locate the jcode source checkout (the bash tool's repo
//! detection and the agent's repo source state).

use anyhow::Result;
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn binary_stem() -> &'static str {
    "jcode"
}

pub fn binary_name() -> &'static str {
    binary_stem()
}

/// Get the jcode repository directory.
pub fn get_repo_dir() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("JCODE_REPO_DIR") {
        let path = PathBuf::from(path);
        if is_jcode_repo(&path) {
            return Some(path);
        }
    }

    // First try: compile-time directory.
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let path = PathBuf::from(manifest_dir);
    if let Some(repo) = find_repo_in_ancestors(&path) {
        return Some(repo);
    }

    // Fallback: check relative to the executable.
    // Assume structure: repo/target/<profile>/<binary>.
    if let Ok(exe) = std::env::current_exe()
        && let Some(repo) = exe
            .parent()
            .and_then(|p| p.parent())
            .and_then(|p| p.parent())
        && is_jcode_repo(repo)
    {
        return Some(repo.to_path_buf());
    }

    // Final fallback: search upward from the current working directory.
    if let Ok(cwd) = std::env::current_dir()
        && let Some(repo) = find_repo_in_ancestors(&cwd)
    {
        return Some(repo);
    }

    None
}

pub fn find_repo_in_ancestors(start: &Path) -> Option<PathBuf> {
    for dir in start.ancestors() {
        if is_jcode_repo(dir) {
            return Some(dir.to_path_buf());
        }
    }
    None
}

/// Check if a directory is the jcode repository.
pub fn is_jcode_repo(dir: &Path) -> bool {
    let cargo_toml = dir.join("Cargo.toml");
    if !cargo_toml.exists() {
        return false;
    }

    // A `.git` directory or gitdir file (worktrees use a file).
    if !dir.join(".git").exists() {
        return false;
    }

    if let Ok(content) = std::fs::read_to_string(&cargo_toml)
        && content.contains("name = \"jcode\"")
    {
        return true;
    }

    false
}

/// Get the current short git hash for a repository.
pub fn current_git_hash(repo_dir: &Path) -> Result<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .current_dir(repo_dir)
        .output()?;

    if !output.status.success() {
        anyhow::bail!("Failed to get git hash");
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Check if a repository's working tree is dirty.
pub fn is_working_tree_dirty(repo_dir: &Path) -> Result<bool> {
    let output = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(repo_dir)
        .output()?;

    Ok(!output.stdout.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo_fixture(git_file: bool) -> tempfile::TempDir {
        let temp = tempfile::TempDir::new().expect("temp repo");
        if git_file {
            std::fs::write(temp.path().join(".git"), "gitdir: /tmp/jcode-test-git\n")
                .expect("git file");
        } else {
            std::fs::create_dir_all(temp.path().join(".git")).expect("git dir");
        }
        std::fs::write(
            temp.path().join("Cargo.toml"),
            "[package]\nname = \"jcode\"\nversion = \"0.1.0\"\n",
        )
        .expect("Cargo.toml");
        temp
    }

    #[test]
    fn find_repo_in_ancestors_finds_workspace_from_crate_dir() {
        let repo = repo_fixture(false);
        let crate_dir = repo.path().join("crates").join("jcode-app-core");
        std::fs::create_dir_all(&crate_dir).expect("crate dir");

        assert_eq!(
            find_repo_in_ancestors(&crate_dir).as_deref(),
            Some(repo.path())
        );
    }

    #[test]
    fn is_jcode_repo_accepts_git_file_for_worktree() {
        let repo = repo_fixture(true);
        assert!(is_jcode_repo(repo.path()));
    }
}
