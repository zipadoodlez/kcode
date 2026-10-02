use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap, HashSet};

/// Hard upper bound for one swarm's durable plan graph. A plan is coordination
/// state, not an append-only activity log; without a bound, repeated seed,
/// expand, and approve calls can retain and broadcast thousands of stale nodes
/// forever. This is four times the live swarm-member cap and well above normal
/// decomposed graphs while bounding server, disk, and per-client state.
pub const MAX_PLAN_ITEMS: usize = 1024;

pub mod artifact;
pub mod bridge;
pub mod dag;

/// A swarm plan item: the same task type the `todo` tool and the work list use.
///
/// The definition lives in `kcode-task-types`, because the plan, the `todo`
/// tool, and the list file all read and write the same entries.
pub use kcode_task_types::TaskItem;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SwarmPlanItemSpec {
    pub id: String,
    pub content: String,
    pub priority: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subsystem: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub file_scope: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blocked_by: Vec<String>,
}

/// Whether `id` is a row other rows belong to: a composite join/synthesis point
/// rather than a leaf. Derived, never stored: a child that is still a row names its
/// parent, and a close deletes the child and leaves its record on the parent
/// (`kcode_base::todo::close_row`), so a row with an open child or a nonempty
/// `records` was decomposed and still is.
pub fn is_composite(rows: &[TaskItem], id: &str) -> bool {
    rows.iter().any(|row| row.parent.as_deref() == Some(id))
        || rows
            .iter()
            .find(|row| row.id == id)
            .is_some_and(|row| !row.records.is_empty())
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlanGraphSummary {
    pub ready_ids: Vec<String>,
    pub blocked_ids: Vec<String>,
    pub active_ids: Vec<String>,
    pub completed_ids: Vec<String>,
    /// Terminal without completing: failed, stopped, or crashed items. These are
    /// finished from the scheduler's perspective but must not read as success.
    pub failed_ids: Vec<String>,
    pub terminal_ids: Vec<String>,
    pub unresolved_dependency_ids: Vec<String>,
    pub cycle_ids: Vec<String>,
}

pub fn is_completed_status(status: &str) -> bool {
    matches!(status, "completed" | "done")
}

pub fn is_terminal_status(status: &str) -> bool {
    matches!(
        status,
        "completed" | "done" | "failed" | "stopped" | "crashed"
    )
}

pub fn is_active_status(status: &str) -> bool {
    status == "running"
}

/// Terminal without completing: the item is finished from the scheduler's
/// perspective but did not succeed (failed, stopped, or crashed).
pub fn is_failed_status(status: &str) -> bool {
    is_terminal_status(status) && !is_completed_status(status)
}

pub fn is_runnable_status(status: &str) -> bool {
    matches!(status, "queued" | "ready" | "pending" | "todo")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskControlAction {
    Start,
    Wake,
    Resume,
    Retry,
    Reassign,
    Replace,
    Salvage,
}

impl TaskControlAction {
    pub fn parse(action: &str) -> Option<Self> {
        match action {
            "start" => Some(Self::Start),
            "wake" => Some(Self::Wake),
            "resume" => Some(Self::Resume),
            "retry" => Some(Self::Retry),
            "reassign" => Some(Self::Reassign),
            "replace" => Some(Self::Replace),
            "salvage" => Some(Self::Salvage),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Wake => "wake",
            Self::Resume => "resume",
            Self::Retry => "retry",
            Self::Reassign => "reassign",
            Self::Replace => "replace",
            Self::Salvage => "salvage",
        }
    }
}

pub fn combine_assignment_text(content: &str, message: Option<&str>) -> String {
    if let Some(extra) = message {
        format!(
            "{}\n\nAdditional coordinator instructions:\n{}",
            content, extra
        )
    } else {
        content.to_string()
    }
}

fn restart_instruction_prefix(action: TaskControlAction) -> Option<&'static str> {
    match action {
        TaskControlAction::Resume => Some(
            "Resume your assigned task from the current session context and continue the work.",
        ),
        TaskControlAction::Retry => {
            Some("Retry your assigned task. Fix any earlier issues and continue toward completion.")
        }
        _ => None,
    }
}

pub fn build_control_assignment_text(
    action: TaskControlAction,
    content: &str,
    message: Option<&str>,
) -> String {
    let mut parts = Vec::new();
    if let Some(prefix) = restart_instruction_prefix(action) {
        parts.push(prefix.to_string());
    }
    parts.push(content.to_string());
    if let Some(extra) = message {
        parts.push(format!("Additional coordinator instructions:\n{}", extra));
    }
    parts.join("\n\n")
}

pub fn task_control_action_allows_status(action: TaskControlAction, status: &str) -> bool {
    match action {
        TaskControlAction::Start | TaskControlAction::Wake => status == "queued",
        TaskControlAction::Resume => matches!(status, "queued" | "running"),
        TaskControlAction::Retry => status == "failed",
        // A completed node must never be reopened by handoff actions:
        // complete_node persists "completed" (not just "done"), and reassigning
        // it would re-queue finished work and clobber its artifact.
        TaskControlAction::Reassign | TaskControlAction::Replace | TaskControlAction::Salvage => {
            !is_completed_status(status)
        }
    }
}

pub fn task_control_status_error(action: TaskControlAction, status: &str, task_id: &str) -> String {
    match action {
        TaskControlAction::Start => format!(
            "Task '{}' is '{}' and cannot be started. Use start only for queued assignments.",
            task_id, status
        ),
        TaskControlAction::Wake => format!(
            "Task '{}' is '{}' and cannot be woken. Use wake only for queued assignments.",
            task_id, status
        ),
        TaskControlAction::Resume => format!(
            "Task '{}' is '{}' and cannot be resumed safely.",
            task_id, status
        ),
        TaskControlAction::Retry => format!(
            "Task '{}' is '{}' and cannot be retried. Retry is only for failed or stale work.",
            task_id, status
        ),
        TaskControlAction::Reassign => format!(
            "Task '{}' is already complete. Reassign unfinished work instead.",
            task_id
        ),
        TaskControlAction::Replace => format!(
            "Task '{}' is already complete. Replace is only for unfinished work.",
            task_id
        ),
        TaskControlAction::Salvage => format!(
            "Task '{}' is already complete. Salvage is only for unfinished or failed work.",
            task_id
        ),
    }
}

pub fn priority_rank(priority: &str) -> u8 {
    match priority {
        "high" | "urgent" | "p0" => 0,
        "medium" | "normal" | "p1" => 1,
        "low" | "p2" => 2,
        _ => 1,
    }
}

pub fn completed_item_ids(items: &[TaskItem]) -> HashSet<String> {
    items
        .iter()
        .filter(|item| is_completed_status(&item.status))
        .map(|item| item.id.clone())
        .collect()
}

pub fn unresolved_dependencies<'a>(
    item: &'a TaskItem,
    known_ids: &HashSet<&'a str>,
    completed_ids: &HashSet<&str>,
) -> Vec<String> {
    item.blocked_by
        .iter()
        .filter(|dep| known_ids.contains(dep.as_str()) && !completed_ids.contains(dep.as_str()))
        .cloned()
        .collect()
}

pub fn missing_dependencies<'a>(item: &'a TaskItem, known_ids: &HashSet<&'a str>) -> Vec<String> {
    item.blocked_by
        .iter()
        .filter(|dep| !known_ids.contains(dep.as_str()))
        .cloned()
        .collect()
}

pub fn is_unblocked<'a>(
    item: &'a TaskItem,
    known_ids: &HashSet<&'a str>,
    completed_ids: &HashSet<&str>,
) -> bool {
    missing_dependencies(item, known_ids).is_empty()
        && unresolved_dependencies(item, known_ids, completed_ids).is_empty()
}

pub fn cycle_item_ids(items: &[TaskItem]) -> Vec<String> {
    let item_ids: HashSet<&str> = items.iter().map(|item| item.id.as_str()).collect();
    let mut indegree: HashMap<&str, usize> = HashMap::new();
    let mut dependents: HashMap<&str, Vec<&str>> = HashMap::new();

    for item in items {
        indegree.entry(item.id.as_str()).or_insert(0);
    }

    for item in items {
        for dependency in item
            .blocked_by
            .iter()
            .filter(|dependency| item_ids.contains(dependency.as_str()))
        {
            *indegree.entry(item.id.as_str()).or_insert(0) += 1;
            dependents
                .entry(dependency.as_str())
                .or_default()
                .push(item.id.as_str());
        }
    }

    let mut queue: Vec<&str> = indegree
        .iter()
        .filter_map(|(id, degree)| (*degree == 0).then_some(*id))
        .collect();
    let mut visited = HashSet::new();

    while let Some(id) = queue.pop() {
        if !visited.insert(id) {
            continue;
        }
        if let Some(children) = dependents.get(id) {
            for child in children {
                if let Some(degree) = indegree.get_mut(child) {
                    *degree = degree.saturating_sub(1);
                    if *degree == 0 {
                        queue.push(child);
                    }
                }
            }
        }
    }

    let mut cycle_ids: Vec<String> = indegree
        .into_iter()
        .filter_map(|(id, degree)| (degree > 0 && !visited.contains(id)).then_some(id.to_string()))
        .collect();
    cycle_ids.sort();
    cycle_ids
}

pub fn summarize_plan_graph(items: &[TaskItem]) -> PlanGraphSummary {
    let known_ids: HashSet<&str> = items.iter().map(|item| item.id.as_str()).collect();
    let completed_ids = completed_item_ids(items);
    let completed_refs: HashSet<&str> = completed_ids.iter().map(String::as_str).collect();
    let cycle_ids = cycle_item_ids(items);
    let cycle_set: HashSet<&str> = cycle_ids.iter().map(String::as_str).collect();

    let mut ready_ids = Vec::new();
    let mut blocked_ids = Vec::new();
    let mut active_ids = Vec::new();
    let mut completed = BTreeSet::new();
    let mut failed = BTreeSet::new();
    let mut terminal = BTreeSet::new();
    let mut unresolved = BTreeSet::new();

    for item in items {
        let missing = missing_dependencies(item, &known_ids);
        let unresolved_for_item = unresolved_dependencies(item, &known_ids, &completed_refs);
        let is_cyclic = cycle_set.contains(item.id.as_str());

        unresolved.extend(missing.iter().cloned());

        if is_active_status(&item.status) {
            active_ids.push(item.id.clone());
        }
        if is_completed_status(&item.status) {
            completed.insert(item.id.clone());
        }
        if is_failed_status(&item.status) {
            failed.insert(item.id.clone());
        }
        if is_terminal_status(&item.status) {
            terminal.insert(item.id.clone());
        }

        let has_dependency_blocker = !unresolved_for_item.is_empty() || is_cyclic;
        if is_runnable_status(&item.status) && missing.is_empty() && !has_dependency_blocker {
            ready_ids.push(item.id.clone());
        } else if !is_terminal_status(&item.status)
            && !is_active_status(&item.status)
            && (!missing.is_empty() || has_dependency_blocker || item.status == "blocked")
        {
            blocked_ids.push(item.id.clone());
        }
    }

    ready_ids.sort();
    blocked_ids.sort();
    active_ids.sort();

    PlanGraphSummary {
        ready_ids,
        blocked_ids,
        active_ids,
        completed_ids: completed.into_iter().collect(),
        failed_ids: failed.into_iter().collect(),
        terminal_ids: terminal.into_iter().collect(),
        unresolved_dependency_ids: unresolved.into_iter().collect(),
        cycle_ids,
    }
}

pub fn next_runnable_item_ids(items: &[TaskItem], limit: Option<usize>) -> Vec<String> {
    let ready_ids: HashSet<String> = summarize_plan_graph(items).ready_ids.into_iter().collect();
    let mut ready_items: Vec<&TaskItem> = items
        .iter()
        .filter(|item| ready_ids.contains(&item.id))
        .collect();

    ready_items.sort_by(|left, right| {
        priority_rank(&left.priority)
            .cmp(&priority_rank(&right.priority))
            .then_with(|| left.id.cmp(&right.id))
    });

    let iter = ready_items.into_iter().map(|item| item.id.clone());
    match limit {
        Some(limit) => iter.take(limit).collect(),
        None => iter.collect(),
    }
}

/// The highest-priority runnable item a hand-over may pick: one nobody holds, or
/// one `requester` itself holds, because a run hands over the rows it works and the
/// assignment moves the claim from the run to the worker. A row another live
/// session holds is not this run's to give away (the holder takes it back, see
/// [`next_held_runnable_item_id`]), and a row whose holder can never come back is
/// [`next_stranded_runnable_item_id`].
pub fn next_handover_runnable_item_id(items: &[TaskItem], requester: &str) -> Option<String> {
    next_runnable_item_ids(items, None)
        .into_iter()
        .find(|candidate_id| {
            items
                .iter()
                .find(|item| item.id == *candidate_id)
                .is_some_and(|item| {
                    item.assigned_to
                        .as_deref()
                        .is_none_or(|holder| holder == requester)
                })
        })
}

/// The highest-priority runnable (ready) item that a holder which can work it
/// already claims, together with that holder.
///
/// A row that names a holder is not granted afresh by anyone: the assignment is
/// the record of who owes the work, so a ready row held by a session that can
/// take it is handed back to that session rather than given to a free worker. A
/// holder the caller cannot drive is not returned here; the dead case is
/// [`next_stranded_runnable_item_id`], and a holder that is merely busy is left
/// alone until it finishes. `requester`'s own rows are excluded: it works those
/// itself, and the assign path refuses to assign a task to the asking session.
pub fn next_held_runnable_item_id(
    items: &[TaskItem],
    holder_can_work: &dyn Fn(&str) -> bool,
    requester: &str,
) -> Option<(String, String)> {
    next_runnable_item_ids(items, None)
        .into_iter()
        .find_map(|candidate_id| {
            let item = items.iter().find(|item| item.id == candidate_id)?;
            let holder = item.assigned_to.as_deref()?;
            if holder == requester || !holder_can_work(holder) {
                return None;
            }
            Some((candidate_id, holder.to_string()))
        })
}

/// The highest-priority runnable (ready) item that is *stranded*: it carries an
/// assignment, but the assignee is dead per `assignee_is_dead` (terminal
/// lifecycle status or no longer a swarm member). Such items are invisible to
/// [`next_handover_runnable_item_id`] (which requires nobody else to hold it),
/// which is how `task_control retry` against a dead worker used to strand a
/// Ready node: retry keeps the assignee, the re-dispatch dies with the session,
/// and automatic assignment skips the node forever. A row that keeps killing its
/// workers is not bounded here: releasing the claim is the whole recovery, and the
/// loop that repeats the work owns the bound.
pub fn next_stranded_runnable_item_id(
    items: &[TaskItem],
    assignee_is_dead: &dyn Fn(&str) -> bool,
) -> Option<String> {
    next_runnable_item_ids(items, None)
        .into_iter()
        .find(|candidate_id| {
            let Some(item) = items.iter().find(|item| item.id == *candidate_id) else {
                return false;
            };
            let Some(assignee) = item.assigned_to.as_deref() else {
                return false;
            };
            assignee_is_dead(assignee)
        })
}

pub fn task_control_target_item_id(
    items: &[TaskItem],
    target_session: &str,
    action: TaskControlAction,
) -> Result<String, String> {
    let mut candidates: Vec<&TaskItem> = items
        .iter()
        .filter(|item| item.assigned_to.as_deref() == Some(target_session))
        .filter(|item| task_control_action_allows_status(action, &item.status))
        .collect();

    candidates.sort_by_key(|item| match item.status.as_str() {
        "running" => 0,
        "queued" | "ready" | "pending" | "todo" => 1,
        "failed" | "stopped" | "crashed" => 2,
        "completed" | "done" => 3,
        _ => 4,
    });

    match candidates.as_slice() {
        [] => Err(format!(
            "No task assigned to '{}' can be {}. Provide task_id explicitly, or assign a task first.",
            target_session,
            action.as_str()
        )),
        [item] => Ok(item.id.clone()),
        [first, second, ..] if first.status != second.status => Ok(first.id.clone()),
        _ => Err(format!(
            "Multiple tasks assigned to '{}' can be {}: {}. Provide task_id explicitly.",
            target_session,
            action.as_str(),
            candidates
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

pub fn explicit_task_blocked_reason(items: &[TaskItem], task_id: &str) -> Option<String> {
    let known_ids: HashSet<&str> = items.iter().map(|item| item.id.as_str()).collect();
    let completed_ids = completed_item_ids(items);
    let completed_refs: HashSet<&str> = completed_ids.iter().map(String::as_str).collect();
    let cycle_ids: HashSet<String> = cycle_item_ids(items).into_iter().collect();

    let item = items.iter().find(|item| item.id == task_id)?;
    let missing = missing_dependencies(item, &known_ids);
    if !missing.is_empty() {
        return Some(format!(
            "Task '{}' has missing dependencies: {}",
            item.id,
            missing.join(", ")
        ));
    }

    let unresolved = unresolved_dependencies(item, &known_ids, &completed_refs);
    if !unresolved.is_empty() {
        return Some(format!(
            "Task '{}' is still blocked by: {}",
            item.id,
            unresolved.join(", ")
        ));
    }

    if cycle_ids.contains(&item.id) {
        return Some(format!(
            "Task '{}' is part of a dependency cycle and is not runnable",
            item.id
        ));
    }

    None
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AssignmentAffinities {
    pub dependency_carryover: HashMap<String, usize>,
    pub metadata_carryover: HashMap<String, usize>,
}

pub fn assignment_affinities_for_task(
    items: &[TaskItem],
    task_id: &str,
) -> Result<AssignmentAffinities, String> {
    let Some(task) = items.iter().find(|item| item.id == task_id) else {
        return Err(format!("Task '{}' not found in swarm plan", task_id));
    };

    let mut dependency_carryover = HashMap::<String, usize>::new();
    let mut metadata_carryover = HashMap::<String, usize>::new();
    for dependency_id in &task.blocked_by {
        if let Some(dep_item) = items.iter().find(|item| item.id == *dependency_id)
            && let Some(owner) = dep_item.assigned_to.as_ref()
        {
            *dependency_carryover.entry(owner.clone()).or_default() += 1;
        }
    }

    for item in items {
        let Some(owner) = item.assigned_to.as_ref() else {
            continue;
        };
        if item.id == task.id {
            continue;
        }
        if task
            .subsystem
            .as_ref()
            .zip(item.subsystem.as_ref())
            .is_some_and(|(left, right)| left == right)
        {
            *metadata_carryover.entry(owner.clone()).or_default() += 2;
        }
        if !task.file_scope.is_empty() && !item.file_scope.is_empty() {
            let overlap = task
                .file_scope
                .iter()
                .filter(|path| item.file_scope.contains(*path))
                .count();
            if overlap > 0 {
                *metadata_carryover.entry(owner.clone()).or_default() += overlap;
            }
        }
    }

    Ok(AssignmentAffinities {
        dependency_carryover,
        metadata_carryover,
    })
}

pub fn newly_ready_item_ids(before: &[TaskItem], after: &[TaskItem]) -> Vec<String> {
    let before_ready: HashSet<String> =
        summarize_plan_graph(before).ready_ids.into_iter().collect();
    let mut after_ready = summarize_plan_graph(after).ready_ids;
    after_ready.retain(|item_id| !before_ready.contains(item_id));
    after_ready
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str, status: &str, blocked_by: &[&str]) -> TaskItem {
        TaskItem {
            id: id.to_string(),
            content: id.to_string(),
            status: status.to_string(),
            priority: "high".to_string(),
            blocked_by: blocked_by.iter().map(|value| value.to_string()).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn summarize_plan_graph_reports_ready_and_blocked_items() {
        let items = vec![
            item("a", "completed", &[]),
            item("b", "queued", &["a"]),
            item("c", "queued", &["b"]),
        ];

        let summary = summarize_plan_graph(&items);
        assert_eq!(summary.ready_ids, vec!["b".to_string()]);
        assert_eq!(summary.blocked_ids, vec!["c".to_string()]);
        assert_eq!(summary.completed_ids, vec!["a".to_string()]);
        assert_eq!(summary.cycle_ids, Vec::<String>::new());
    }

    #[test]
    fn summarize_plan_graph_reports_missing_dependencies() {
        let items = vec![
            item("a", "queued", &["missing-task"]),
            item("b", "running", &[]),
        ];

        let summary = summarize_plan_graph(&items);
        assert_eq!(summary.ready_ids, Vec::<String>::new());
        assert_eq!(summary.blocked_ids, vec!["a".to_string()]);
        assert_eq!(summary.active_ids, vec!["b".to_string()]);
        assert_eq!(
            summary.unresolved_dependency_ids,
            vec!["missing-task".to_string()]
        );
    }

    #[test]
    fn newly_ready_item_ids_reports_tasks_unblocked_by_completion() {
        let before = vec![
            item("setup", "running", &[]),
            item("follow-up", "queued", &["setup"]),
            item("later", "queued", &["follow-up"]),
        ];
        let after = vec![
            item("setup", "completed", &[]),
            item("follow-up", "queued", &["setup"]),
            item("later", "queued", &["follow-up"]),
        ];

        assert_eq!(newly_ready_item_ids(&before, &after), vec!["follow-up"]);
    }

    #[test]
    fn summarize_plan_graph_reports_failed_items_separately_from_completed() {
        let items = vec![
            item("ok", "completed", &[]),
            item("boom", "failed", &[]),
            item("halted", "stopped", &[]),
            item("crashed-task", "crashed", &[]),
            item("pending-task", "queued", &[]),
        ];

        let summary = summarize_plan_graph(&items);
        assert_eq!(summary.completed_ids, vec!["ok".to_string()]);
        assert_eq!(
            summary.failed_ids,
            vec![
                "boom".to_string(),
                "crashed-task".to_string(),
                "halted".to_string()
            ]
        );
        // Terminal covers both success and failure; failed is the non-success subset.
        assert_eq!(
            summary.terminal_ids,
            vec![
                "boom".to_string(),
                "crashed-task".to_string(),
                "halted".to_string(),
                "ok".to_string()
            ]
        );
        assert_eq!(summary.ready_ids, vec!["pending-task".to_string()]);
    }

    #[test]
    fn summarize_plan_graph_reports_cycles() {
        let items = vec![
            item("a", "queued", &["c"]),
            item("b", "queued", &["a"]),
            item("c", "queued", &["b"]),
        ];

        let summary = summarize_plan_graph(&items);
        assert_eq!(summary.ready_ids, Vec::<String>::new());
        assert_eq!(
            summary.blocked_ids,
            vec!["a".to_string(), "b".to_string(), "c".to_string()]
        );
        assert_eq!(
            summary.cycle_ids,
            vec!["a".to_string(), "b".to_string(), "c".to_string()]
        );
    }

    #[test]
    fn status_helpers_match_runtime_expectations() {
        assert!(is_completed_status("completed"));
        assert!(is_terminal_status("failed"));
        assert!(is_active_status("running"));
        assert!(is_runnable_status("queued"));
        assert!(!is_terminal_status("queued"));
    }

    #[test]
    fn next_runnable_items_prefers_higher_priority() {
        let items = vec![
            item("done", "completed", &[]),
            item("b", "queued", &["done"]),
            TaskItem {
                priority: "low".to_string(),
                ..item("c", "queued", &["done"])
            },
            TaskItem {
                priority: "high".to_string(),
                ..item("a", "queued", &["done"])
            },
        ];

        assert_eq!(next_runnable_item_ids(&items, None), vec!["a", "b", "c"]);
        assert_eq!(next_runnable_item_ids(&items, Some(2)), vec!["a", "b"]);
    }

    #[test]
    fn task_control_target_prefers_active_assignment_and_rejects_ambiguous_matches() {
        let items = vec![
            TaskItem {
                assigned_to: Some("agent-a".to_string()),
                ..item("queued", "queued", &[])
            },
            TaskItem {
                assigned_to: Some("agent-a".to_string()),
                ..item("running", "running", &[])
            },
        ];

        assert_eq!(
            task_control_target_item_id(&items, "agent-a", TaskControlAction::Resume),
            Ok("running".to_string())
        );

        let ambiguous = vec![
            TaskItem {
                assigned_to: Some("agent-a".to_string()),
                ..item("one", "queued", &[])
            },
            TaskItem {
                assigned_to: Some("agent-a".to_string()),
                ..item("two", "queued", &[])
            },
        ];
        assert!(
            task_control_target_item_id(&ambiguous, "agent-a", TaskControlAction::Start)
                .unwrap_err()
                .contains("Multiple tasks")
        );
    }

    #[test]
    fn assignment_helpers_report_blocked_and_next_unassigned_tasks() {
        let items = vec![
            item("done", "completed", &[]),
            TaskItem {
                assigned_to: Some("agent-a".to_string()),
                ..item("assigned", "queued", &["done"])
            },
            item("ready", "queued", &["done"]),
            item("blocked", "queued", &["ready"]),
        ];

        assert_eq!(
            next_handover_runnable_item_id(&items, "me"),
            Some("ready".to_string())
        );
        assert_eq!(
            explicit_task_blocked_reason(&items, "blocked"),
            Some("Task 'blocked' is still blocked by: ready".to_string())
        );
    }

    #[test]
    fn a_held_runnable_row_names_the_holder_that_can_take_it_back() {
        let items = vec![
            TaskItem {
                assigned_to: Some("worker-busy".to_string()),
                ..item("busy", "queued", &[])
            },
            TaskItem {
                assigned_to: Some("worker-free".to_string()),
                ..item("held", "queued", &[])
            },
            TaskItem {
                assigned_to: Some("requester".to_string()),
                ..item("own", "queued", &[])
            },
            item("unowned", "queued", &[]),
        ];
        let free = |session: &str| session == "worker-free" || session == "requester";

        // The first runnable row whose holder can take it, skipping a busy holder
        // and the asker's own rows.
        assert_eq!(
            next_held_runnable_item_id(&items, &free, "requester"),
            Some(("held".to_string(), "worker-free".to_string()))
        );
        // Nobody can take anything: no hand-back, and an unowned row is not one.
        let nobody = |_: &str| false;
        assert_eq!(
            next_held_runnable_item_id(&items, &nobody, "requester"),
            None
        );
    }

    #[test]
    fn assignment_affinities_count_dependency_and_metadata_carryover() {
        let items = vec![
            TaskItem {
                assigned_to: Some("agent-a".to_string()),
                subsystem: Some("ui".to_string()),
                file_scope: vec!["src/tui.rs".to_string()],
                ..item("dep", "completed", &[])
            },
            TaskItem {
                assigned_to: Some("agent-b".to_string()),
                subsystem: Some("ui".to_string()),
                file_scope: vec!["src/tui.rs".to_string()],
                ..item("sibling", "queued", &[])
            },
            TaskItem {
                subsystem: Some("ui".to_string()),
                file_scope: vec!["src/tui.rs".to_string()],
                ..item("target", "queued", &["dep"])
            },
        ];
        let affinities = assignment_affinities_for_task(&items, "target").unwrap();
        assert_eq!(affinities.dependency_carryover.get("agent-a"), Some(&1));
        assert_eq!(affinities.metadata_carryover.get("agent-b"), Some(&3));
    }

    #[test]
    fn stranded_runnable_item_requires_a_dead_assignee() {
        let dead = |session: &str| session == "dead-session";

        // Ready but unassigned: not stranded (normal path handles it).
        let mut items = vec![item("a", "queued", &[])];
        assert_eq!(next_stranded_runnable_item_id(&items, &dead), None);

        // Assigned to a live session: not stranded.
        items[0].assigned_to = Some("live-session".to_string());
        assert_eq!(next_stranded_runnable_item_id(&items, &dead), None);

        // Assigned to a dead session: stranded.
        items[0].assigned_to = Some("dead-session".to_string());
        assert_eq!(
            next_stranded_runnable_item_id(&items, &dead),
            Some("a".to_string())
        );

        // Blocked items never count even with a dead assignee.
        let blocked = vec![item("gate", "queued", &[]), {
            let mut blocked = item("b", "queued", &["gate"]);
            blocked.assigned_to = Some("dead-session".to_string());
            blocked
        }];
        assert_eq!(next_stranded_runnable_item_id(&blocked, &dead), None);
    }
}
