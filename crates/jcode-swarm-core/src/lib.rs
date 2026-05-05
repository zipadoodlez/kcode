use jcode_plan::PlanItem;

pub const MAX_SWARM_COMPLETION_REPORT_CHARS: usize = 4000;
pub const SWARM_COMPLETION_REPORT_MARKER: &str = "SWARM COMPLETION REPORT REQUIRED";

pub fn append_swarm_completion_report_instructions(message: &str) -> String {
    if message.contains(SWARM_COMPLETION_REPORT_MARKER) {
        return message.to_string();
    }

    let mut out = message.trim_end().to_string();
    if !out.is_empty() {
        out.push_str("\n\n");
    }
    out.push_str("<system-reminder>\n");
    out.push_str(SWARM_COMPLETION_REPORT_MARKER);
    out.push_str(
        "\nBefore finishing, call the swarm tool with action=\"report\" to submit your completion report. \
Include a concise message, validation/tests performed, and blockers or follow-ups. \
After the report tool succeeds, also write a brief final assistant response. \
Do not finish with only tool output, a lifecycle status change, or no final response. \
Do not send a separate DM for the final report unless you need interactive coordination before finishing.\n",
    );
    out.push_str("</system-reminder>");
    out
}

pub fn format_structured_completion_report(
    message: &str,
    validation: Option<&str>,
    follow_up: Option<&str>,
) -> String {
    let mut report = message.trim().to_string();
    if let Some(validation) = validation.map(str::trim).filter(|value| !value.is_empty()) {
        if !report.is_empty() {
            report.push_str("\n\n");
        }
        report.push_str("Validation:\n");
        report.push_str(validation);
    }
    if let Some(follow_up) = follow_up.map(str::trim).filter(|value| !value.is_empty()) {
        if !report.is_empty() {
            report.push_str("\n\n");
        }
        report.push_str("Follow-ups/blockers:\n");
        report.push_str(follow_up);
    }
    report
}

pub fn normalize_completion_report(report: Option<String>) -> Option<String> {
    let report = report?.trim().to_string();
    if report.is_empty() {
        return None;
    }

    let char_count = report.chars().count();
    if char_count <= MAX_SWARM_COMPLETION_REPORT_CHARS {
        return Some(report);
    }

    let suffix = "\n\n[Report truncated by jcode before delivery.]";
    let keep_chars = MAX_SWARM_COMPLETION_REPORT_CHARS.saturating_sub(suffix.chars().count());
    let mut truncated: String = report.chars().take(keep_chars).collect();
    truncated.push_str(suffix);
    Some(truncated)
}

fn completion_status_intro(name: &str, status: &str) -> String {
    match status {
        "ready" => format!("Agent {} finished their work and is ready for more.", name),
        "failed" => format!("Agent {} finished with status failed.", name),
        "stopped" => format!("Agent {} stopped.", name),
        _ => format!("Agent {} completed their work.", name),
    }
}

fn completion_followup(status: &str, has_report: bool) -> &'static str {
    match (status, has_report) {
        ("ready", true) => {
            "Use assign_task to give them more work, stop to remove them, or summary/read_context for full context."
        }
        ("ready", false) => {
            "Use summary/read_context to inspect results, assign_task for more work, or stop to remove them."
        }
        ("failed", true) => {
            "Use summary/read_context for full context, retry with guidance, or stop to remove them."
        }
        ("failed", false) => {
            "Use summary/read_context to inspect results, assign_task to retry with guidance, or stop to remove them."
        }
        ("stopped", _) => "Use summary/read_context to inspect results or stop to remove them.",
        (_, true) => {
            "Use assign_task to give them new work, stop to remove them, or summary/read_context for full context."
        }
        (_, false) => "Use assign_task to give them new work, or stop to remove them.",
    }
}

pub fn completion_notification_message(name: &str, status: &str, report: Option<&str>) -> String {
    let intro = completion_status_intro(name, status);
    let followup = completion_followup(status, report.is_some());
    match report {
        Some(report) => format!("{intro}\n\nReport:\n{report}\n\n{followup}"),
        None => format!("{intro}\n\nNo final textual report was produced. {followup}"),
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

pub fn summarize_plan_items(items: &[PlanItem], max_items: usize) -> String {
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

    fn plan_item(id: &str, content: &str) -> PlanItem {
        PlanItem {
            id: id.to_string(),
            content: content.to_string(),
            status: "queued".to_string(),
            priority: "normal".to_string(),
            subsystem: None,
            file_scope: Vec::new(),
            blocked_by: Vec::new(),
            assigned_to: None,
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
    fn append_swarm_completion_report_instructions_is_idempotent() {
        let prompt = "Do work";
        let with_instructions = append_swarm_completion_report_instructions(prompt);
        assert!(with_instructions.contains(SWARM_COMPLETION_REPORT_MARKER));
        assert_eq!(
            append_swarm_completion_report_instructions(&with_instructions),
            with_instructions
        );
    }

    #[test]
    fn completion_report_normalization_trims_and_truncates() {
        assert_eq!(
            normalize_completion_report(Some("  done  ".to_string())),
            Some("done".to_string())
        );
        assert_eq!(normalize_completion_report(Some("   ".to_string())), None);
        let long = "x".repeat(MAX_SWARM_COMPLETION_REPORT_CHARS + 100);
        let normalized = normalize_completion_report(Some(long)).unwrap();
        assert_eq!(
            normalized.chars().count(),
            MAX_SWARM_COMPLETION_REPORT_CHARS
        );
        assert!(normalized.ends_with("[Report truncated by jcode before delivery.]"));
    }
}
