use std::collections::{BTreeSet, HashMap, HashSet};

pub mod kind;

/// A swarm plan item: the same task type the `todo` tool and the work list use.
///
/// The definition lives in `kcode-task-types`, because the plan, the `todo`
/// tool, and the list file all read and write the same entries.
pub use kcode_task_types::TaskItem;

/// What the dependency graph says about a set of rows.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlanGraphSummary {
    /// Rows a dependency keeps: a `blocked_by` entry naming no row in the list,
    /// one naming a row still open, or membership in a cycle.
    pub blocked_ids: Vec<String>,
    /// The `blocked_by` entries that name no row in the list: a hand-written id,
    /// or one whose row left the list without the close that removes it from
    /// every dependent.
    pub unresolved_dependency_ids: Vec<String>,
    pub cycle_ids: Vec<String>,
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

/// Read the graph off the rows: what a dependency keeps, which `blocked_by`
/// entries name nothing, and which rows sit on a cycle.
///
/// Readiness itself is not here: the file's rule is that a close removes its id
/// from every dependent, so a row with an empty `blocked_by` and a `kind` is
/// what the turn loop will work (`live_turn::row_is_ready`).
pub fn summarize_plan_graph(items: &[TaskItem]) -> PlanGraphSummary {
    let known_ids: HashSet<&str> = items.iter().map(|item| item.id.as_str()).collect();
    let cycle_ids = cycle_item_ids(items);
    let cycle_set: HashSet<&str> = cycle_ids.iter().map(String::as_str).collect();

    let mut blocked_ids = Vec::new();
    let mut unresolved = BTreeSet::new();

    for item in items {
        let missing = missing_dependencies(item, &known_ids);
        // A dependency still in the list is one that has not closed yet.
        let still_open = item
            .blocked_by
            .iter()
            .any(|dep| known_ids.contains(dep.as_str()));
        let is_cyclic = cycle_set.contains(item.id.as_str());

        unresolved.extend(missing.iter().cloned());
        if !missing.is_empty() || still_open || is_cyclic {
            blocked_ids.push(item.id.clone());
        }
    }

    blocked_ids.sort();
    PlanGraphSummary {
        blocked_ids,
        unresolved_dependency_ids: unresolved.into_iter().collect(),
        cycle_ids,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str, blocked_by: &[&str]) -> TaskItem {
        TaskItem {
            id: id.to_string(),
            content: id.to_string(),
            blocked_by: blocked_by.iter().map(|value| value.to_string()).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn summarize_plan_graph_reports_a_dependency_still_open() {
        let items = vec![item("a", &[]), item("b", &["a"]), item("c", &["b"])];

        let summary = summarize_plan_graph(&items);
        assert_eq!(summary.blocked_ids, vec!["b".to_string(), "c".to_string()]);
        assert_eq!(summary.unresolved_dependency_ids, Vec::<String>::new());
        assert_eq!(summary.cycle_ids, Vec::<String>::new());
    }

    #[test]
    fn summarize_plan_graph_reports_missing_dependencies() {
        let items = vec![item("a", &["missing-task"]), item("b", &[])];

        let summary = summarize_plan_graph(&items);
        assert_eq!(summary.blocked_ids, vec!["a".to_string()]);
        assert_eq!(
            summary.unresolved_dependency_ids,
            vec!["missing-task".to_string()]
        );
    }

    #[test]
    fn summarize_plan_graph_reports_cycles() {
        let items = vec![item("a", &["c"]), item("b", &["a"]), item("c", &["b"])];

        let summary = summarize_plan_graph(&items);
        assert_eq!(
            summary.blocked_ids,
            vec!["a".to_string(), "b".to_string(), "c".to_string()]
        );
        assert_eq!(
            summary.cycle_ids,
            vec!["a".to_string(), "b".to_string(), "c".to_string()]
        );
    }
}
