use crate::plan::PlanItem;
use crate::protocol::PlanGraphStatus;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct RemoteSwarmPlanSnapshot {
    pub swarm_id: String,
    pub version: u64,
    pub items: Vec<PlanItem>,
    pub participants: Vec<String>,
    pub reason: Option<String>,
    pub summary: Option<PlanGraphStatus>,
}

impl RemoteSwarmPlanSnapshot {
    pub fn status_notice(&self) -> String {
        let mut notice = format!(
            "Swarm plan synced (v{}, {} items)",
            self.version,
            self.items.len()
        );
        if let Some(summary) = &self.summary {
            // Task-DAG progress breakdown: how the graph currently partitions by
            // scheduling state. Only show segments that are non-empty so the line
            // stays compact.
            let mut segments = Vec::new();
            if !summary.completed_ids.is_empty() {
                segments.push(format!("{} done", summary.completed_ids.len()));
            }
            if !summary.active_ids.is_empty() {
                segments.push(format!("{} running", summary.active_ids.len()));
            }
            if !summary.ready_ids.is_empty() {
                segments.push(format!("{} ready", summary.ready_ids.len()));
            }
            if !summary.blocked_ids.is_empty() {
                segments.push(format!("{} blocked", summary.blocked_ids.len()));
            }
            if !summary.cycle_ids.is_empty() {
                segments.push(format!("{} in cycle", summary.cycle_ids.len()));
            }
            if !segments.is_empty() {
                notice.push_str(&format!(" · graph: {}", segments.join(", ")));
            }
            if !summary.next_ready_ids.is_empty() {
                notice.push_str(&format!(" · next: {}", summary.next_ready_ids.join(", ")));
            }
            if !summary.newly_ready_ids.is_empty() {
                notice.push_str(&format!(
                    " · newly ready: {}",
                    summary.newly_ready_ids.join(", ")
                ));
            }
        }
        notice
    }
}

#[cfg(test)]
mod tests {
    use super::RemoteSwarmPlanSnapshot;

    #[test]
    fn swarm_plan_status_notice_includes_graph_hints() {
        let snapshot = RemoteSwarmPlanSnapshot {
            swarm_id: "swarm-a".to_string(),
            version: 5,
            items: Vec::new(),
            participants: Vec::new(),
            reason: None,
            summary: Some(crate::protocol::PlanGraphStatus {
                swarm_id: Some("swarm-a".to_string()),
                version: 5,
                item_count: 2,
                ready_ids: vec!["task-2".to_string()],
                blocked_ids: vec!["task-4".to_string()],
                active_ids: Vec::new(),
                completed_ids: vec!["task-1".to_string()],
                cycle_ids: Vec::new(),
                unresolved_dependency_ids: Vec::new(),
                next_ready_ids: vec!["task-2".to_string()],
                newly_ready_ids: vec!["task-3".to_string()],
            }),
        };

        let notice = snapshot.status_notice();
        assert!(notice.contains("v5"));
        assert!(notice.contains("graph: 1 done, 1 ready, 1 blocked"));
        assert!(notice.contains("next: task-2"));
        assert!(notice.contains("newly ready: task-3"));
    }
}
