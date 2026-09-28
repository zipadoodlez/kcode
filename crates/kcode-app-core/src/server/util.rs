use super::SwarmMember;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::{OnceCell, RwLock};

pub(crate) fn debug_control_allowed() -> bool {
    // Check config file setting
    if crate::config::config().display.debug_socket {
        return true;
    }
    if std::env::var("KCODE_DEBUG_CONTROL")
        .ok()
        .map(|v| matches!(v.as_str(), "1" | "true" | "yes" | "on"))
        .unwrap_or(false)
    {
        return true;
    }
    // Check for file-based toggle (allows enabling without restart)
    if let Ok(kcode_dir) = crate::storage::kcode_dir()
        && kcode_dir.join("debug_control").exists()
    {
        return true;
    }
    false
}

pub(crate) async fn get_shared_mcp_pool(
    cell: &OnceCell<Arc<crate::mcp::SharedMcpPool>>,
) -> Arc<crate::mcp::SharedMcpPool> {
    cell.get_or_init(|| async { Arc::new(crate::mcp::SharedMcpPool::from_default_config()) })
        .await
        .clone()
}

/// Resolve the binary a reload should exec into.
///
/// With self-install and update removed there is no channel to swap to, so a
/// reload is a plain restart in place: re-exec the currently running binary
/// with fresh process state and socket handoff.
pub(crate) fn reload_exec_target(_is_selfdev_session: bool) -> Option<(PathBuf, &'static str)> {
    let current = std::env::current_exe().ok()?;
    Some((strip_deleted_suffix(current), "current-exe"))
}

/// Strip the Linux `/proc/self/exe` " (deleted)" marker that appears when the
/// running binary has been unlinked or replaced in place.
fn strip_deleted_suffix(path: PathBuf) -> PathBuf {
    const DELETED_MARKER: &str = " (deleted)";
    if let Some(stripped) = path.to_str().and_then(|s| s.strip_suffix(DELETED_MARKER)) {
        return PathBuf::from(stripped);
    }
    path
}

pub(crate) fn git_common_dir_for(path: &Path) -> Option<PathBuf> {
    let mut current = Some(path);
    while let Some(dir) = current {
        let dotgit = dir.join(".git");
        if dotgit.is_dir() {
            return Some(canonicalize_or(dotgit));
        }
        if dotgit.is_file() {
            let content = std::fs::read_to_string(&dotgit).ok()?;
            let gitdir_line = content
                .lines()
                .find(|line| line.trim_start().starts_with("gitdir:"))?;
            let raw = gitdir_line
                .trim_start()
                .trim_start_matches("gitdir:")
                .trim();
            if raw.is_empty() {
                return None;
            }
            let gitdir = if Path::new(raw).is_absolute() {
                PathBuf::from(raw)
            } else {
                dir.join(raw)
            };
            let gitdir = canonicalize_or(gitdir);
            // Worktree gitdir looks like: <repo>/.git/worktrees/<name>
            if let Some(parent) = gitdir.parent()
                && parent.file_name().and_then(|s| s.to_str()) == Some("worktrees")
                && let Some(common) = parent.parent()
            {
                return Some(canonicalize_or(common.to_path_buf()));
            }
            return Some(gitdir);
        }
        current = dir.parent();
    }
    None
}

pub(crate) fn swarm_id_for_dir(dir: Option<PathBuf>) -> Option<String> {
    if let Ok(sw_id) = std::env::var("KCODE_SWARM_ID") {
        let trimmed = sw_id.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    let dir = dir?;
    if let Some(git_common) = git_common_dir_for(&dir) {
        return Some(git_common.to_string_lossy().to_string());
    }
    Some(dir.to_string_lossy().to_string())
}

/// Return the swarm identity for an independently-created root session.
///
/// Swarm plans are keyed by swarm id. Deriving that id from the working
/// directory made every session opened in one repository share one plan, even
/// when those sessions were unrelated. Root sessions therefore own a swarm by
/// default. `KCODE_SWARM_ID` remains an explicit opt-in to a shared swarm.
pub(crate) fn swarm_id_for_session(session_id: &str) -> Option<String> {
    if let Ok(sw_id) = std::env::var("KCODE_SWARM_ID") {
        let trimmed = sw_id.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    default_swarm_id_for_session(session_id)
}

fn default_swarm_id_for_session(session_id: &str) -> Option<String> {
    if session_id.trim().is_empty() {
        None
    } else {
        Some(format!("session:{session_id}"))
    }
}

/// Swarm id of the swarm `session_id` is currently a member of, if any.
pub(crate) async fn member_swarm_id(
    session_id: &str,
    swarm_members: &Arc<RwLock<HashMap<String, SwarmMember>>>,
) -> Option<String> {
    let members = swarm_members.read().await;
    members.get(session_id).and_then(|m| m.swarm_id.clone())
}

/// Display name of `session_id`'s swarm membership, if any.
pub(crate) async fn member_friendly_name(
    session_id: &str,
    swarm_members: &Arc<RwLock<HashMap<String, SwarmMember>>>,
) -> Option<String> {
    let members = swarm_members.read().await;
    members
        .get(session_id)
        .and_then(|member| member.friendly_name.clone())
}

/// Make a session id safe to use as a single path component: every character
/// that is not ASCII alphanumeric, `-`, or `_` becomes `_`.
pub(crate) fn sanitize_session_id(session_id: &str) -> String {
    session_id
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod swarm_identity_tests {
    use super::{default_swarm_id_for_session, sanitize_session_id};

    #[test]
    fn sanitize_session_id_strips_path_traversal_and_separators() {
        // A malicious or merely unusual session id must never be able to escape
        // the recovery directory or collide with sibling paths.
        assert_eq!(sanitize_session_id("../../etc/passwd"), "______etc_passwd");
        assert_eq!(sanitize_session_id("a/b\\c"), "a_b_c");
        assert_eq!(sanitize_session_id("sess.with space"), "sess_with_space");
        // Already-safe ids are preserved verbatim.
        assert_eq!(sanitize_session_id("session-abc_123"), "session-abc_123");
    }

    #[test]
    fn independent_root_sessions_have_distinct_swarm_ids() {
        assert_eq!(
            default_swarm_id_for_session("session-one").as_deref(),
            Some("session:session-one")
        );
        assert_eq!(
            default_swarm_id_for_session("session-two").as_deref(),
            Some("session:session-two")
        );
        assert_ne!(
            default_swarm_id_for_session("session-one"),
            default_swarm_id_for_session("session-two")
        );
    }

    #[test]
    fn empty_session_cannot_own_a_swarm() {
        assert_eq!(default_swarm_id_for_session("  "), None);
    }
}

/// The server never has an in-band update available: the operating system
/// package manager is the source of truth for the installed version.
pub(crate) fn server_has_newer_binary() -> bool {
    false
}

fn canonicalize_or(path: PathBuf) -> PathBuf {
    std::fs::canonicalize(&path).unwrap_or(path)
}

/// Server identity for multi-server support
#[derive(Debug, Clone)]
pub struct ServerIdentity {
    /// Full server ID (e.g., "server_blazing_1705012345678")
    pub id: String,
    /// Short name (e.g., "blazing")
    pub name: String,
    /// Icon for display (e.g., "🔥")
    pub icon: String,
    /// Git hash of the binary
    pub git_hash: String,
    /// Version string (e.g., "v0.1.123")
    pub version: String,
}

impl ServerIdentity {
    /// Display name with icon (e.g., "🔥 blazing")
    pub fn display_name(&self) -> String {
        format!("{} {}", self.icon, self.name)
    }
}

pub(crate) fn startup_headless_recovery_test_delay() -> Option<std::time::Duration> {
    let raw = std::env::var("KCODE_TEST_HEADLESS_STARTUP_RECOVERY_DELAY_MS").ok()?;
    let delay_ms = raw.trim().parse::<u64>().ok()?;
    (delay_ms > 0).then(|| std::time::Duration::from_millis(delay_ms))
}
