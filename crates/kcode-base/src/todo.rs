use crate::storage;
use anyhow::Result;
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

pub use kcode_task_types::TaskItem;

/// The file that holds a repo's open work, one JSON object per line, at the repo
/// root. See `docs/plans/work-list.md` (rule 7).
const WORK_LIST_FILE: &str = "tasks.jsonl";

/// Return the canonical todo status for model-written status vocabulary.
///
/// The todo tool historically accepted any string, so persisted sessions can
/// contain natural completion synonyms such as `done` or `finished`. Keep this
/// helper tolerant for those sessions even though new tool calls advertise a
/// constrained vocabulary.
pub fn canonical_todo_status(status: &str) -> Option<&'static str> {
    let status = status.trim();
    if status.eq_ignore_ascii_case("pending") {
        Some("pending")
    } else if status.eq_ignore_ascii_case("in_progress")
        || status.eq_ignore_ascii_case("in progress")
        || status.eq_ignore_ascii_case("in-progress")
    {
        Some("in_progress")
    } else if status.eq_ignore_ascii_case("completed")
        || status.eq_ignore_ascii_case("complete")
        || status.eq_ignore_ascii_case("done")
        || status.eq_ignore_ascii_case("finished")
    {
        Some("completed")
    } else if status.eq_ignore_ascii_case("cancelled") || status.eq_ignore_ascii_case("canceled") {
        Some("cancelled")
    } else {
        None
    }
}

pub fn todo_status_is_completed(status: &str) -> bool {
    canonical_todo_status(status) == Some("completed")
}

pub fn todo_status_is_cancelled(status: &str) -> bool {
    canonical_todo_status(status) == Some("cancelled")
}

/// Build the synthetic auto-poke continuation prompt sent when the model
/// stops with incomplete todos. Kept here so every remaining producer (the
/// `kcode run` auto-poke) and the transcript renderer agree on the exact text.
pub fn build_auto_poke_message(incomplete_count: usize) -> String {
    format!(
        "You have {} incomplete todo{}. Continue working, or update the todo tool.",
        incomplete_count,
        if incomplete_count == 1 { "" } else { "s" },
    )
}

/// Where a session's work list lives: `tasks.jsonl` at the repo root of
/// `working_dir`, or, with no repo, a session-scoped file under the kcode dir.
///
/// Found from git rather than from the working directory, so a session started
/// in `crates/foo` reads the same list as one started at the root.
fn work_list_path(working_dir: Option<&Path>, session_id: &str) -> Result<PathBuf> {
    if let Some(dir) = working_dir
        && let Some(root) = repo_root(dir)
    {
        return Ok(root.join(WORK_LIST_FILE));
    }
    Ok(storage::kcode_dir()?
        .join("work-lists")
        .join(format!("{session_id}.jsonl")))
}

/// The root of the repo `dir` is inside, if any. Only an explicit directory
/// counts, so a caller with no working directory never writes into whatever repo
/// the process happens to be started in.
///
/// Memoized per directory: one `git` call per distinct working directory, not
/// one per read. A `git init` inside a directory first looked up as a non-repo is
/// only noticed on restart.
fn repo_root(dir: &Path) -> Option<PathBuf> {
    static ROOTS: LazyLock<Mutex<HashMap<PathBuf, Option<PathBuf>>>> =
        LazyLock::new(|| Mutex::new(HashMap::new()));
    if let Ok(roots) = ROOTS.lock()
        && let Some(root) = roots.get(dir)
    {
        return root.clone();
    }
    let root = lookup_repo_root(dir);
    if let Ok(mut roots) = ROOTS.lock() {
        roots.insert(dir.to_path_buf(), root.clone());
    }
    root
}

fn lookup_repo_root(dir: &Path) -> Option<PathBuf> {
    let output = std::process::Command::new("git")
        .current_dir(dir)
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let root = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!root.is_empty()).then(|| PathBuf::from(root))
}

pub fn load_tasks(working_dir: Option<&Path>, session_id: &str) -> Result<Vec<TaskItem>> {
    let path = work_list_path(working_dir, session_id)?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    read_json_lines(&std::fs::read_to_string(&path)?)
}

pub fn save_tasks(working_dir: Option<&Path>, session_id: &str, tasks: &[TaskItem]) -> Result<()> {
    let path = work_list_path(working_dir, session_id)?;
    storage::write_bytes(&path, write_json_lines(tasks)?.as_bytes())
}

/// One task per line, blank lines skipped, so an insert, a claim, or a close is a
/// one-line diff in git.
fn read_json_lines<T: DeserializeOwned>(text: &str) -> Result<Vec<T>> {
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| Ok(serde_json::from_str(line)?))
        .collect()
}

fn write_json_lines<T: Serialize>(tasks: &[T]) -> Result<String> {
    let mut out = String::new();
    for task in tasks {
        out.push_str(&serde_json::to_string(task)?);
        out.push('\n');
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The turn-finish gate must tell the model how to clear it without implying
    /// that the todo write which triggered the check was discarded.
    #[test]
    fn the_poke_names_the_count_and_invites_an_update() {
        let message = build_auto_poke_message(3);
        assert!(message.contains("3 incomplete todos"), "{message}");
        assert!(message.contains("update the todo tool"), "{message}");
        assert!(
            build_auto_poke_message(1).contains("1 incomplete todo."),
            "the singular form must not gain an s"
        );
    }

    #[test]
    fn a_list_round_trips_through_json_lines() {
        let tasks = vec![
            TaskItem {
                id: "a".to_string(),
                content: "first".to_string(),
                status: "pending".to_string(),
                priority: "high".to_string(),
                ..Default::default()
            },
            TaskItem {
                id: "b".to_string(),
                content: "second".to_string(),
                blocked_by: vec!["a".to_string()],
                ..Default::default()
            },
        ];
        let text = write_json_lines(&tasks).expect("write");
        assert_eq!(text.lines().count(), 2, "one task per line");
        assert_eq!(read_json_lines::<TaskItem>(&text).expect("read"), tasks);
    }

    #[test]
    fn blank_lines_are_skipped() {
        let text = "{\"id\":\"a\",\"content\":\"x\",\"status\":\"\",\"priority\":\"\"}\n\n";
        let tasks = read_json_lines::<TaskItem>(text).expect("read");
        assert_eq!(tasks.len(), 1);
    }

    /// A repo home is the root, not the directory the session started in.
    #[test]
    fn the_home_is_the_repo_root() {
        let root = repo_root(Path::new(env!("CARGO_MANIFEST_DIR")));
        assert_eq!(
            root.as_deref(),
            Some(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .parent()
                    .unwrap()
                    .parent()
                    .unwrap()
            )
        );
        let path =
            work_list_path(Some(Path::new(env!("CARGO_MANIFEST_DIR"))), "ignored").expect("path");
        assert!(path.ends_with(WORK_LIST_FILE), "{path:?}");
    }
}
