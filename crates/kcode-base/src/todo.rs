use crate::storage;
use anyhow::{Result, bail};
use chrono::Utc;
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

pub use kcode_task_types::TaskItem;

/// The file that holds a repo's open work, one JSON object per line, at the repo
/// root. See `docs/plans/task-flow.md` (rule 7).
const WORK_LIST_FILE: &str = "tasks.jsonl";

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
/// grant typed. See `docs/plans/task-flow.md`, the model and rules 2 and 10.
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
/// session holds. See `docs/plans/task-flow.md`: for a run scoped to the whole
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

/// Add one open row and hand back its id. The caller brings the row's words and
/// its other fields; the store owns the id and what no row may be (empty), so the
/// `todo` tool and a grant's anchor are the same writer with the same invariants
/// (rule 2).
pub fn add_row(rows: &mut Vec<TaskItem>, mut row: TaskItem) -> Result<String> {
    let content = row.content.trim();
    if content.is_empty() {
        bail!("add needs content");
    }
    row.content = content.to_string();
    let id = next_id(rows);
    rows.push(TaskItem {
        id: id.clone(),
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
/// work belongs. See `docs/plans/task-flow.md`, rules 3 and 4.
///
/// The close states an outcome, so a nonempty `result` is required, and it names
/// the check that proves the row done. A row with a child still naming it cannot
/// close: rule 3 keeps a parent while a child does, which is what makes the
/// parent's result its children's results integrated. The record then goes onto
/// the row that owns the work, the parent for a row that has one, so a finished
/// row is not gone with nothing kept: the record is the close's own words,
/// `{id, result}`.
///
/// A run works the rows it holds, so a row that names another session as its holder
/// is refused: the user's session takes a row by writing it first, and a row that
/// moved mid-turn is no longer the moved-from turn's to finish.
///
/// The row is the last thing to go, and the close drops its id from every
/// dependent's `blocked_by`; an entry there always names an open row (rule 7).
pub fn close_row(rows: &mut Vec<TaskItem>, session_id: &str, id: &str, result: &str) -> Result<()> {
    let result = result.trim();
    if result.is_empty() {
        bail!(
            "close needs result: name the check that proves it, what it showed, and what you did not check"
        );
    }
    let row = rows
        .iter()
        .find(|row| row.id == id)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("no task {id:?}; open ids: {}", open_ids(rows)))?;
    if let Some(holder) = row.assigned_to.as_deref()
        && holder != session_id
    {
        bail!(
            "task {id:?} is held by {holder}, not {session_id}; a run closes only the rows it holds"
        );
    }
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
        parent
            .records
            .push(serde_json::json!({"id": id, "result": result}));
    }

    for row in rows.iter_mut() {
        row.blocked_by
            .retain(|dependency| dependency.as_str() != id);
    }
    rows.retain(|row| row.id != id);
    Ok(())
}

/// The rows a write changed, by id, so a caller can bring its own copy of the list
/// to the same state the store wrote. A close touches the parent that takes the
/// record and every row that no longer names the closed row as a blocker.
fn changed_rows(
    rows: &[TaskItem],
    before: &std::collections::HashMap<String, TaskItem>,
) -> Vec<TaskItem> {
    rows.iter()
        .filter(|row| before.get(&row.id) != Some(*row))
        .cloned()
        .collect()
}

/// Release one open row's claim in the file: a row whose holder can never come back goes
/// back to the list unclaimed so the next dispatch can seat it. This is what the stranded
/// reclaim and the salvage sweep write; a holder that is merely between turns is
/// left alone, because it still owes the row.
pub fn release_row_on_disk(
    working_dir: Option<&Path>,
    session_id: &str,
    id: &str,
) -> Result<TaskItem> {
    set_row_holder_on_disk(working_dir, session_id, id, None)
}

fn set_row_holder_on_disk(
    working_dir: Option<&Path>,
    session_id: &str,
    id: &str,
    holder: Option<&str>,
) -> Result<TaskItem> {
    let mut rows = load_tasks(working_dir, session_id)?;
    let index = rows
        .iter()
        .position(|row| row.id == id)
        .ok_or_else(|| anyhow::anyhow!("no task {id:?}; open ids: {}", open_ids(&rows)))?;
    rows[index].assigned_to = holder.map(str::to_string);
    let written = rows[index].clone();
    save_tasks(working_dir, session_id, &rows)?;
    Ok(written)
}

/// Rewrite every row's holder when a session's id changes.
///
/// The list is where a holder lives (rule 2), so a renamed session must be renamed
/// in the list too, or its rows read as somebody else's and it loses sight of its own
/// work. Read-modify-write, like every other write here.
pub fn rename_row_holder_on_disk(
    working_dir: Option<&Path>,
    session_id: &str,
    old_session_id: &str,
    new_session_id: &str,
) -> Result<Vec<TaskItem>> {
    let mut rows = load_tasks(working_dir, session_id)?;
    let before: std::collections::HashMap<String, TaskItem> = rows
        .iter()
        .map(|row| (row.id.clone(), row.clone()))
        .collect();
    for row in rows.iter_mut() {
        if row.assigned_to.as_deref() == Some(old_session_id) {
            row.assigned_to = Some(new_session_id.to_string());
        }
    }
    let touched = changed_rows(&rows, &before);
    save_tasks(working_dir, session_id, &rows)?;
    Ok(touched)
}

/// The next id: the row's creation minute as three base36 digits. The value comes
/// from the clock, so a closed row's id is not handed back the way a counter's was,
/// and the same value comes round again after 36^3 minutes (32 days). A value the
/// file still names is skipped, so the key the file reads stays unique.
///
/// The loop runs at most once per live row: every candidate it skips is a distinct
/// value some row holds.
fn next_id(rows: &[TaskItem]) -> String {
    let mut minute = Utc::now().timestamp() / 60;
    loop {
        let id = short_id(minute);
        if !rows.iter().any(|row| row.id == id) {
            return id;
        }
        minute += 1;
    }
}

/// The three base36 digits of a minute count, zero-padded, so every id is the same
/// width and the alphabet is stdlib's own (`char::from_digit`).
fn short_id(mut minute: i64) -> String {
    let mut id = String::with_capacity(3);
    for _ in 0..3 {
        id.insert(
            0,
            char::from_digit((minute % 36) as u32, 36).expect("a base36 digit"),
        );
        minute /= 36;
    }
    id
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

    /// A repo of its own, so the list a test writes is its own and never the
    /// machine's (rule 1), keyed by name so two tests never share one.
    fn scratch_repo(name: &str) -> PathBuf {
        let repo = std::env::temp_dir().join(format!("kcode-{name}-{}", std::process::id()));
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
        repo
    }

    #[test]
    fn a_list_round_trips_through_json_lines() {
        let tasks = vec![
            TaskItem {
                id: "a".to_string(),
                content: "first".to_string(),
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
        let repo = scratch_repo("anchor-run");
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
        assert_ne!(fresh.id, "t1", "a fresh anchor takes an id of its own");
        assert_eq!(fresh.id.len(), 3, "an id is three base36 digits");
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

    /// A run closes only the rows it holds: a row that moved to another session
    /// mid-turn is refused, and the row stays open for whoever holds it now. The
    /// user's session reaches one by taking it first, which is a write.
    #[test]
    fn a_close_refuses_a_row_held_by_another_session() {
        let mut rows = vec![TaskItem {
            id: "t1".to_string(),
            content: "work".to_string(),
            kind: Some("implement".to_string()),
            assigned_to: Some("worker".to_string()),
            ..Default::default()
        }];

        let err = close_row(&mut rows, "moved-away", "t1", "done: x").unwrap_err();
        assert!(
            err.to_string().contains("held by worker"),
            "unexpected error: {err}"
        );
        assert_eq!(rows.len(), 1, "a refused close leaves the row open");

        // Taking it is a write, and then it is anybody's to close.
        rows[0].assigned_to = None;
        close_row(&mut rows, "me", "t1", "done: the work is in commit abc")
            .expect("a row nobody holds is anybody's to close");
        assert!(rows.is_empty());
    }

    /// A run that typed no words gets its anchor from the rows it holds: the first
    /// row that belongs to nothing is the one at the top, and the rest of the rows
    /// the session holds that belong to nothing are changed to belong to it. A row
    /// that already belongs to something keeps it, and a second call writes nothing.
    #[test]
    fn a_run_that_typed_nothing_anchors_on_its_first_root_row() {
        let repo = scratch_repo("anchor-rows");
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
        let repo = scratch_repo("anchor-none");
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

    /// A release takes the holder back and leaves everything else as it was: the
    /// stranded reclaim and the salvage sweep write it, because a holder that can
    /// never come back should not keep its row.
    #[test]
    fn a_release_frees_the_row() {
        let repo = scratch_repo("row-holder");
        let row = TaskItem {
            id: "t1".to_string(),
            content: "work".to_string(),
            note: Some("kept".to_string()),
            assigned_to: Some("worker".to_string()),
            ..Default::default()
        };
        save_tasks(Some(&repo), "me", &[row]).expect("write the work list");

        let released = release_row_on_disk(Some(&repo), "me", "t1").expect("release");
        assert_eq!(released.assigned_to, None);
        assert_eq!(released.note.as_deref(), Some("kept"), "nothing else moved");
        assert_eq!(
            load_tasks(Some(&repo), "me").expect("read")[0].assigned_to,
            None
        );

        let _ = std::fs::remove_dir_all(&repo);
    }

    /// A close keeps its result on the row that owns the work: the parent for a row
    /// that has one, so a finished row is not gone with nothing kept (rule 4). The
    /// record is the close's own words, `{id, result}`.
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

        close_row(&mut rows, "me", "t2", "cargo test -p kcode-base: 7 passed").expect("close");

        assert_eq!(rows.len(), 1, "the closed row is gone");
        let record = &rows[0].records[0];
        assert_eq!(record["id"], "t2");
        assert_eq!(record["result"], "cargo test -p kcode-base: 7 passed");
        assert_eq!(
            record.as_object().map(|record| record.len()),
            Some(2),
            "a record is the close's own words: id and result, nothing else"
        );
    }

    /// The id is the creation minute, not a count of the file's own ids, so two rows
    /// made inside one minute still take different ones: the second skips the value
    /// the first holds, and the file's key stays unique.
    #[test]
    fn two_rows_in_one_minute_take_different_ids() {
        let mut rows = Vec::new();
        let first = add_row(
            &mut rows,
            TaskItem {
                content: "the run".to_string(),
                ..Default::default()
            },
        )
        .expect("first");
        let second = add_row(
            &mut rows,
            TaskItem {
                content: "the work".to_string(),
                ..Default::default()
            },
        )
        .expect("second");

        assert_ne!(
            first, second,
            "the second row skips the value the first one holds"
        );
        for id in [&first, &second] {
            assert_eq!(id.len(), 3, "an id is three base36 digits: {id}");
            assert!(
                id.chars()
                    .all(|digit| digit.is_ascii_digit() || digit.is_ascii_lowercase()),
                "an id is base36: {id}"
            );
        }
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
        close_row(&mut rows, "me", "t1", "done: nothing to run").expect("close");
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
        let err = close_row(&mut rows, "me", "t1", "done").unwrap_err();
        assert!(err.to_string().contains("still has open children"), "{err}");
        let err = close_row(&mut rows, "me", "t2", "   ").unwrap_err();
        assert!(err.to_string().contains("close needs result"), "{err}");
        assert_eq!(rows.len(), 2, "a refused close writes nothing");
    }
}
