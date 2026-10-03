use kcode_plan::TaskItem;

pub use kcode_session_types::{SwarmLifecycleStatus, SwarmMemberRecord, SwarmRole};

/// Absolute maximum number of live members in a single swarm. Servers also apply
/// the lower configurable live-worker RAM budget before reaching this hard stop.
pub const MAX_SWARM_MEMBERS: usize = 1000;

/// Upper bound for a member's derived task label, sized for one-line UI chips.
pub const MAX_SWARM_TASK_LABEL_CHARS: usize = 48;

/// Derive a short, stable task label from a spawn prompt or task assignment.
///
/// Takes the first non-empty line, strips common markdown/list prefixes,
/// collapses whitespace, and truncates on a char boundary with an ellipsis.
/// Returns `None` when the text has no usable content.
pub fn derive_swarm_task_label(text: &str) -> Option<String> {
    let line = text.lines().map(str::trim).find(|line| !line.is_empty())?;
    let line = line
        .trim_start_matches(['#', '-', '*', '>', ' '])
        .trim_end_matches(':')
        .trim();
    let collapsed = line.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        return None;
    }
    if collapsed.chars().count() <= MAX_SWARM_TASK_LABEL_CHARS {
        return Some(collapsed);
    }
    let truncated: String = collapsed
        .chars()
        .take(MAX_SWARM_TASK_LABEL_CHARS.saturating_sub(1))
        .collect();
    Some(format!("{}…", truncated.trim_end()))
}

/// The one line a member's status change notifies its coordinator with. The
/// report it used to carry, and the advice it used to append, are the row's now:
/// a closer's words are the close, and the coordinator reads them there.
pub fn completion_status_intro(name: &str, status: &str) -> String {
    match status {
        "ready" => format!("Agent {} finished their work and is ready for more.", name),
        "failed" => format!("Agent {} finished with status failed.", name),
        "stopped" => format!("Agent {} stopped.", name),
        "crashed" => format!("Agent {} crashed while working.", name),
        _ => format!("Agent {} completed their work.", name),
    }
}

pub fn truncate_detail(text: &str, max_len: usize) -> String {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = collapsed.trim();
    let max_len = max_len.max(1);
    if trimmed.chars().count() <= max_len {
        return trimmed.to_string();
    }
    if max_len <= 3 {
        return trimmed.chars().take(max_len).collect();
    }
    let mut out: String = trimmed.chars().take(max_len - 3).collect();
    out.push_str("...");
    out
}

pub fn summarize_plan_items(items: &[TaskItem], max_items: usize) -> String {
    if items.is_empty() {
        return "no items".to_string();
    }
    let mut parts: Vec<String> = Vec::new();
    for item in items.iter().take(max_items.max(1)) {
        parts.push(item.content.clone());
    }
    let mut summary = parts.join("; ");
    if items.len() > max_items.max(1) {
        summary.push_str(&format!(" (+{} more)", items.len() - max_items.max(1)));
    }
    summary
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan_item(id: &str, content: &str) -> TaskItem {
        TaskItem {
            id: id.to_string(),
            content: content.to_string(),
            status: "queued".to_string(),
            priority: "normal".to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn truncate_detail_collapses_whitespace_and_ellipsizes() {
        assert_eq!(truncate_detail("hello   there\nworld", 11), "hello th...");
    }

    #[test]
    fn summarize_plan_items_limits_output() {
        let items = vec![
            plan_item("a", "first"),
            plan_item("b", "second"),
            plan_item("c", "third"),
        ];
        assert_eq!(summarize_plan_items(&items, 2), "first; second (+1 more)");
    }
    #[test]
    fn task_label_takes_first_line_strips_prefixes_and_collapses_whitespace() {
        assert_eq!(
            derive_swarm_task_label("Fix the   parser\n\nMore detail here"),
            Some("Fix the parser".to_string())
        );
        assert_eq!(
            derive_swarm_task_label("\n\n  ## Investigate flaky test:  \nbody"),
            Some("Investigate flaky test".to_string())
        );
        assert_eq!(
            derive_swarm_task_label("- review PR #42"),
            Some("review PR #42".to_string())
        );
    }

    #[test]
    fn task_label_truncates_long_prompts_with_ellipsis() {
        let long = "implement the entire authentication subsystem including oauth flows";
        let label = derive_swarm_task_label(long).unwrap();
        assert!(label.chars().count() <= MAX_SWARM_TASK_LABEL_CHARS);
        assert!(label.ends_with('…'), "got: {label}");
    }

    #[test]
    fn task_label_rejects_empty_or_marker_only_text() {
        assert_eq!(derive_swarm_task_label(""), None);
        assert_eq!(derive_swarm_task_label("   \n\t\n"), None);
        assert_eq!(derive_swarm_task_label("###"), None);
    }
}
