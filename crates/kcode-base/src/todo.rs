use crate::storage;
use anyhow::{Result, bail};
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

/// The row a granted run is scoped to (its anchor), resolved from the words the
/// grant typed. See `docs/plans/work-list.md`, the model and rules 2 and 10.
///
/// Words that name an open row - its id, or its content as a user says it - make
/// that row the run's anchor and claim it for `session_id`, so the run's scope is
/// its subtree. Words that name no row are the instruction: they become a fresh
/// anchor, so every run has one row at its top for its records and its end-of-run
/// result. The read-modify-write is the list's one write path (rule 2), shared
/// with the `todo` tool, so a hand edit between the grant and this call is an
/// input rather than a conflict.
///
/// `fresh_kind` types a freshly written anchor. The caller brings the word, so the
/// store still learns no engine vocabulary (rule 5); a row the words name keeps
/// its own kind, because the grant does not get to guess what that work is.
pub fn anchor_from_words(
    working_dir: Option<&Path>,
    session_id: &str,
    words: &str,
    fresh_kind: Option<&str>,
) -> Result<TaskItem> {
    let words = words.trim();
    let mut rows = load_tasks(working_dir, session_id)?;
    let anchor = match rows.iter().position(|row| names_row(row, words)) {
        Some(index) => {
            let anchor = &mut rows[index];
            anchor.assigned_to = Some(session_id.to_string());
            anchor.clone()
        }
        None => {
            let id = add_row(
                &mut rows,
                TaskItem {
                    content: words.to_string(),
                    kind: fresh_kind.map(str::to_string),
                    assigned_to: Some(session_id.to_string()),
                    ..Default::default()
                },
            )?;
            rows.iter()
                .find(|row| row.id == id)
                .cloned()
                .expect("the row just added")
        }
    };
    save_tasks(working_dir, session_id, &rows)?;
    Ok(anchor)
}

/// The anchor of a run that typed none: the one row at the top of the rows a
/// session holds. See `docs/plans/work-list.md`: for a run scoped to the whole
/// list, the run's first row is the anchor, and the rest name it as their `parent`.
///
/// The rule, in full. If the rows this session holds include one that belongs to
/// nothing, the first such row is the anchor and the other rows it holds that
/// belong to nothing are changed to belong to it. If none of them belongs to
/// nothing, nothing is written: those rows already have a row that owns their
/// records. Running it again changes nothing, and a row that already has a parent
/// keeps it, so a run never adopts work that belongs to another run's structure.
///
/// Returns the run's anchor, whether this call promoted it or found it already
/// there. `None` means the session holds no row that belongs to nothing, so its
/// rows already have a row that owns their records and there is nothing to make.
pub fn anchor_from_rows(working_dir: Option<&Path>, session_id: &str) -> Result<Option<String>> {
    let mut rows = load_tasks(working_dir, session_id)?;
    let held = |row: &TaskItem| row.assigned_to.as_deref() == Some(session_id);
    let Some(anchor) = rows
        .iter()
        .find(|row| held(row) && row.parent.is_none())
        .map(|row| row.id.clone())
    else {
        return Ok(None);
    };
    let mut adopted = false;
    for row in rows.iter_mut() {
        if held(row) && row.parent.is_none() && row.id != anchor {
            row.parent = Some(anchor.clone());
            adopted = true;
        }
    }
    if adopted {
        save_tasks(working_dir, session_id, &rows)?;
    }
    Ok(Some(anchor))
}

/// Whether these words name `row`: its id, or its content as the user says it.
/// The file keys on `id`, the conversation on words (rule 10).
fn names_row(row: &TaskItem, words: &str) -> bool {
    row.id == words || row.content.trim().eq_ignore_ascii_case(words)
}

/// Add one open row and hand back its id. The caller brings the row's words; the
/// store owns what every new row is (an id, `pending`, no priority rank) and what
/// no row may be (empty), so the `todo` tool and a grant's anchor are the same
/// writer with the same invariants (rule 2).
pub fn add_row(rows: &mut Vec<TaskItem>, mut row: TaskItem) -> Result<String> {
    let content = row.content.trim();
    if content.is_empty() {
        bail!("add needs content");
    }
    row.content = content.to_string();
    let id = next_id(rows);
    rows.push(TaskItem {
        id: id.clone(),
        status: "pending".to_string(),
        priority: String::new(),
        ..row
    });
    Ok(id)
}

/// The open ids, for a message that has to name what a caller could have written
/// instead of the id it wrote.
pub fn open_ids(rows: &[TaskItem]) -> String {
    match rows.is_empty() {
        true => "none".to_string(),
        false => rows
            .iter()
            .map(|row| row.id.as_str())
            .collect::<Vec<_>>()
            .join(", "),
    }
}

/// Close one row with the result that proves it, and keep that result where the
/// work belongs. See `docs/plans/work-list.md`, rules 3 and 4.
///
/// The close states an outcome, so a nonempty `result` is required, and it names
/// the check that proves the row done. A row with a child still naming it cannot
/// close: rule 3 keeps a parent while a child does, which is what makes the
/// parent's result its children's results integrated. The record then goes onto
/// the row that owns the work, the parent for a row that has one, so a finished
/// row is not gone with nothing kept: `artifact` is the machine-readable half a
/// closer may bring (the words of the result stay the result's).
///
/// The row is the last thing to go, and the close drops its id from every
/// dependent's `blocked_by`; an entry there always names an open row (rule 7).
pub fn close_row(
    rows: &mut Vec<TaskItem>,
    id: &str,
    result: &str,
    artifact: Option<serde_json::Value>,
) -> Result<()> {
    let result = result.trim();
    if result.is_empty() {
        bail!("close needs result: name the check that proves it, and what it showed");
    }
    let row = rows
        .iter()
        .find(|row| row.id == id)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("no task {id:?}; open ids: {}", open_ids(rows)))?;
    if let Some(child) = rows.iter().find(|row| row.parent.as_deref() == Some(id)) {
        bail!(
            "{id} still has open children: {} {}",
            child.id,
            child.content
        );
    }

    // The record lands on the row that owns this work. A row that owns nothing
    // keeps none: its result is its own close, which the caller's commit carries.
    if let Some(parent_id) = row.parent.as_deref()
        && let Some(parent) = rows.iter_mut().find(|row| row.id == parent_id)
    {
        let mut record = serde_json::Map::new();
        record.insert("id".to_string(), serde_json::Value::from(id));
        record.insert("result".to_string(), serde_json::Value::from(result));
        if let Some(artifact) = artifact {
            record.insert("artifact".to_string(), artifact);
        }
        parent.records.push(serde_json::Value::Object(record));
    }

    for row in rows.iter_mut() {
        row.blocked_by
            .retain(|dependency| dependency.as_str() != id);
    }
    rows.retain(|row| row.id != id);
    Ok(())
}

/// The next free `t<n>` id.
fn next_id(rows: &[TaskItem]) -> String {
    let highest = rows
        .iter()
        .filter_map(|row| row.id.strip_prefix('t'))
        .filter_map(|number| number.parse::<u32>().ok())
        .max()
        .unwrap_or(0);
    format!("t{}", highest + 1)
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

    /// `/auto <words>` scopes a run to the row the words name - claiming it, since
    /// a run only works what it holds - and words that name no row become the run's
    /// anchor, so every run has one row at its top for its records.
    #[test]
    fn a_grant_resolves_its_anchor_from_its_words() {
        let repo = std::env::temp_dir().join(format!("kcode-anchor-run-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&repo);
        std::fs::create_dir_all(&repo).expect("scratch repo");
        assert!(
            std::process::Command::new("git")
                .args(["init", "-q"])
                .current_dir(&repo)
                .status()
                .expect("git init")
                .success(),
            "git init"
        );
        save_tasks(
            Some(&repo),
            "me",
            &[TaskItem {
                id: "t1".to_string(),
                content: "fix the docs".to_string(),
                assigned_to: Some("someone-else".to_string()),
                ..Default::default()
            }],
        )
        .expect("write the work list");

        let named =
            anchor_from_words(Some(&repo), "me", "fix the docs", None).expect("named anchor");
        assert_eq!(named.id, "t1");
        assert_eq!(
            load_tasks(Some(&repo), "me").expect("read")[0]
                .assigned_to
                .as_deref(),
            Some("me"),
            "the grant claims the row the words name"
        );

        let fresh = anchor_from_words(
            Some(&repo),
            "me",
            "work the list until it is done",
            Some("synthesize"),
        )
        .expect("fresh anchor");
        assert_eq!(fresh.id, "t2");
        assert_eq!(fresh.content, "work the list until it is done");
        assert_eq!(fresh.parent, None, "a run's anchor has no parent");
        assert_eq!(
            fresh.kind.as_deref(),
            Some("synthesize"),
            "a fresh anchor is the run's own row, and the caller's word types it"
        );
        assert_eq!(fresh.assigned_to.as_deref(), Some("me"));
        assert_eq!(load_tasks(Some(&repo), "me").expect("read").len(), 2);

        let _ = std::fs::remove_dir_all(&repo);
    }

    /// A run that typed no words gets its anchor from the rows it holds: the first
    /// row that belongs to nothing is the one at the top, and the rest of the rows
    /// the session holds that belong to nothing are changed to belong to it. A row
    /// that already belongs to something keeps it, and a second call writes nothing.
    #[test]
    fn a_run_that_typed_nothing_anchors_on_its_first_root_row() {
        let repo = std::env::temp_dir().join(format!("kcode-anchor-rows-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&repo);
        std::fs::create_dir_all(&repo).expect("scratch repo");
        assert!(
            std::process::Command::new("git")
                .args(["init", "-q"])
                .current_dir(&repo)
                .status()
                .expect("git init")
                .success(),
            "git init"
        );
        let mine = |id: &str| TaskItem {
            id: id.to_string(),
            content: format!("row {id}"),
            assigned_to: Some("me".to_string()),
            ..Default::default()
        };
        let mut nested = mine("t4");
        nested.parent = Some("elsewhere".to_string());
        let mut theirs = mine("t5");
        theirs.assigned_to = Some("someone-else".to_string());
        save_tasks(
            Some(&repo),
            "me",
            &[mine("t1"), mine("t2"), nested, theirs, mine("t3")],
        )
        .expect("write the work list");

        let anchor = anchor_from_rows(Some(&repo), "me").expect("anchor");
        assert_eq!(
            anchor.as_deref(),
            Some("t1"),
            "the first root row is the top"
        );

        let rows = load_tasks(Some(&repo), "me").expect("read");
        let parent = |id: &str| {
            rows.iter()
                .find(|row| row.id == id)
                .and_then(|row| row.parent.clone())
        };
        assert_eq!(parent("t2").as_deref(), Some("t1"));
        assert_eq!(parent("t3").as_deref(), Some("t1"));
        assert_eq!(
            parent("t4").as_deref(),
            Some("elsewhere"),
            "a row that already belongs to something keeps it"
        );
        assert_eq!(parent("t1"), None, "the anchor itself belongs to nothing");
        assert_eq!(
            parent("t5"),
            None,
            "another session's row is not this run's"
        );

        let _ = std::fs::remove_dir_all(&repo);
    }

    /// A session whose rows all belong to something needs no anchor: their records
    /// already have a row that owns them, and nothing may be written.
    #[test]
    fn rows_that_all_belong_to_something_need_no_anchor() {
        let repo = std::env::temp_dir().join(format!("kcode-anchor-none-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&repo);
        std::fs::create_dir_all(&repo).expect("scratch repo");
        assert!(
            std::process::Command::new("git")
                .args(["init", "-q"])
                .current_dir(&repo)
                .status()
                .expect("git init")
                .success(),
            "git init"
        );
        let mut child = TaskItem {
            id: "t1".to_string(),
            content: "row t1".to_string(),
            assigned_to: Some("me".to_string()),
            ..Default::default()
        };
        child.parent = Some("someone-elses-anchor".to_string());
        save_tasks(Some(&repo), "me", &[child]).expect("write the work list");

        assert_eq!(anchor_from_rows(Some(&repo), "me").expect("anchor"), None);
        assert_eq!(
            load_tasks(Some(&repo), "me").expect("read")[0]
                .parent
                .as_deref(),
            Some("someone-elses-anchor"),
            "the file is left as it was"
        );

        let _ = std::fs::remove_dir_all(&repo);
    }

    /// A close keeps its result on the row that owns the work: the parent for a row
    /// that has one, so a finished row is not gone with nothing kept (rule 4). The
    /// machine-readable half a closer brings travels with it.
    #[test]
    fn a_close_keeps_its_record_on_the_row_that_owns_the_work() {
        let mut parent = TaskItem {
            id: "t1".to_string(),
            content: "the run".to_string(),
            assigned_to: Some("me".to_string()),
            ..Default::default()
        };
        parent.kind = Some("synthesize".to_string());
        let mut child = TaskItem {
            id: "t2".to_string(),
            content: "the work".to_string(),
            assigned_to: Some("me".to_string()),
            parent: Some("t1".to_string()),
            ..Default::default()
        };
        child.kind = Some("implement".to_string());
        let mut rows = vec![parent, child];

        close_row(
            &mut rows,
            "t2",
            "cargo test -p kcode-base: 7 passed",
            Some(serde_json::json!({"findings": "the store owns it", "confidence": "high"})),
        )
        .expect("close");

        assert_eq!(rows.len(), 1, "the closed row is gone");
        let record = &rows[0].records[0];
        assert_eq!(record["id"], "t2");
        assert_eq!(record["result"], "cargo test -p kcode-base: 7 passed");
        assert_eq!(record["artifact"]["confidence"], "high");
    }

    /// A row that owns nothing keeps no record: there is no parent row to hold it,
    /// and the close's own words are the commit's (rule 4).
    #[test]
    fn a_close_with_no_parent_keeps_no_record() {
        let mut rows = vec![TaskItem {
            id: "t1".to_string(),
            content: "root work".to_string(),
            assigned_to: Some("me".to_string()),
            ..Default::default()
        }];
        close_row(&mut rows, "t1", "done: nothing to run", None).expect("close");
        assert!(rows.is_empty());
    }

    /// The close still refuses what it always refused: no result, and a row a child
    /// still names (rule 3).
    #[test]
    fn a_close_needs_a_result_and_no_open_child() {
        let mut rows = vec![
            TaskItem {
                id: "t1".to_string(),
                content: "parent".to_string(),
                ..Default::default()
            },
            TaskItem {
                id: "t2".to_string(),
                content: "child".to_string(),
                parent: Some("t1".to_string()),
                ..Default::default()
            },
        ];
        let err = close_row(&mut rows, "t1", "done", None).unwrap_err();
        assert!(err.to_string().contains("still has open children"), "{err}");
        let err = close_row(&mut rows, "t2", "   ", None).unwrap_err();
        assert!(err.to_string().contains("close needs result"), "{err}");
        assert_eq!(rows.len(), 2, "a refused close writes nothing");
    }
}
