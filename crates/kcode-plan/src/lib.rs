use std::collections::{BTreeSet, HashMap, HashSet};

/// Hard upper bound for one swarm's durable plan graph. A plan is coordination
/// state, not an append-only activity log; without a bound, repeated seed,
/// expand, and approve calls can retain and broadcast thousands of stale nodes
/// forever. This is four times the live swarm-member cap and well above normal
/// decomposed graphs while bounding server, disk, and per-client state.
pub const MAX_PLAN_ITEMS: usize = 1024;

pub mod kind;

/// A swarm plan item: the same task type the `todo` tool and the work list use.
///
/// The definition lives in `kcode-task-types`, because the plan, the `todo`
/// tool, and the list file all read and write the same entries.
pub use kcode_task_types::TaskItem;

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
}
