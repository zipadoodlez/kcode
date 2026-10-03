use std::collections::{HashMap, HashSet};

use super::{AgentInfo, AwaitedMemberStatus, ContextEntry, HistoryMessage, SwarmChannelInfo};

pub fn default_comm_cleanup_target_statuses() -> Vec<String> {
    vec![
        "ready".to_string(),
        "completed".to_string(),
        "failed".to_string(),
        "stopped".to_string(),
        "crashed".to_string(),
    ]
}

pub fn default_comm_run_await_statuses() -> Vec<String> {
    vec![
        "ready".to_string(),
        "completed".to_string(),
        "failed".to_string(),
        "stopped".to_string(),
        "crashed".to_string(),
    ]
}

pub fn default_comm_await_target_statuses() -> Vec<String> {
    vec![
        "ready".to_string(),
        "completed".to_string(),
        "stopped".to_string(),
        "failed".to_string(),
        "crashed".to_string(),
    ]
}

pub fn comm_cleanup_candidate_session_ids(
    owner_session_id: &str,
    members: &[AgentInfo],
    target_status: &[String],
    requested_session_ids: &[String],
    force: bool,
) -> Vec<String> {
    let status_filter: HashSet<&str> = target_status.iter().map(String::as_str).collect();
    let requested: HashSet<&str> = requested_session_ids.iter().map(String::as_str).collect();
    let restrict_to_requested = !requested.is_empty();
    let mut ids = members
        .iter()
        .filter(|member| member.session_id != owner_session_id)
        .filter(|member| !restrict_to_requested || requested.contains(member.session_id.as_str()))
        .filter(|member| {
            member
                .status
                .as_ref()
                .is_some_and(|status| status_filter.contains(status.as_str()))
        })
        .filter(|member| {
            force || member.report_back_to_session_id.as_deref() == Some(owner_session_id)
        })
        .map(|member| member.session_id.clone())
        .collect::<Vec<_>>();
    ids.sort();
    ids
}

pub fn format_comm_context_entries(entries: &[ContextEntry]) -> String {
    if entries.is_empty() {
        "No shared context found.".to_string()
    } else {
        let mut output = String::from("Shared context from other agents:\n\n");
        for entry in entries {
            let from = entry.from_name.as_deref().unwrap_or(&entry.from_session);
            output.push_str(&format!(
                "  {} (from {}): {}\n",
                entry.key, from, entry.value
            ));
        }
        output
    }
}

pub fn duplicate_comm_friendly_names<'a>(
    names: impl IntoIterator<Item = Option<&'a str>>,
) -> HashSet<&'a str> {
    let mut counts = HashMap::<&'a str, usize>::new();
    for name in names.into_iter().flatten() {
        *counts.entry(name).or_default() += 1;
    }
    counts
        .into_iter()
        .filter_map(|(name, count)| (count > 1).then_some(name))
        .collect()
}

pub fn comm_session_display_suffix(session_id: &str) -> &str {
    let suffix = session_id.rsplit('_').next().unwrap_or(session_id);
    if suffix.len() > 6 {
        &suffix[suffix.len() - 6..]
    } else {
        suffix
    }
}

pub fn comm_display_friendly_name(
    friendly_name: Option<&str>,
    session_id: &str,
    duplicate_names: &HashSet<&str>,
) -> String {
    match friendly_name {
        Some(name) if duplicate_names.contains(name) => {
            format!("{} [{}]", name, comm_session_display_suffix(session_id))
        }
        Some(name) => name.to_string(),
        None => session_id.to_string(),
    }
}

pub fn truncate_comm_completion_report(report: &str) -> String {
    const MAX_REPORT_CHARS: usize = 4000;
    let report = report.trim();
    if report.chars().count() <= MAX_REPORT_CHARS {
        return report.to_string();
    }
    let suffix = "\n\n[Report truncated by kcode.]";
    let keep = MAX_REPORT_CHARS.saturating_sub(suffix.chars().count());
    let mut out: String = report.chars().take(keep).collect();
    out.push_str(suffix);
    out
}

pub fn latest_assistant_comm_report(messages: &[HistoryMessage]) -> Option<String> {
    messages.iter().rev().find_map(|message| {
        if message.role != "assistant" {
            return None;
        }
        let report = message.content.trim();
        (!report.is_empty()).then(|| truncate_comm_completion_report(report))
    })
}

pub fn format_comm_awaited_members_with_reports(
    completed: bool,
    summary: &str,
    members: &[AwaitedMemberStatus],
    reports: &HashMap<String, String>,
) -> String {
    // An any-mode wait can complete while some members are still pending, so
    // only claim "All members done" when every member actually matched.
    let all_done = members.iter().all(|member| member.done);
    let mut output = if completed && all_done {
        format!("All members done. {}\n", summary)
    } else if completed {
        format!("Await satisfied. {}\n", summary)
    } else {
        format!("Await incomplete. {}\n", summary)
    };

    if !members.is_empty() {
        let duplicate_names = duplicate_comm_friendly_names(
            members.iter().map(|member| member.friendly_name.as_deref()),
        );
        output.push_str("\nMember statuses:\n");
        for member in members {
            let name = comm_display_friendly_name(
                member.friendly_name.as_deref(),
                &member.session_id,
                &duplicate_names,
            );
            let icon = if member.done { "✓" } else { "✗" };
            output.push_str(&format!("  {} {} ({})\n", icon, name, member.status));
        }
    }

    let mut report_members: Vec<_> = members
        .iter()
        .filter_map(|member| {
            member
                .completion_report
                .as_ref()
                .or_else(|| reports.get(&member.session_id))
                .map(|report| (member, report))
        })
        .collect();
    report_members.sort_by(|(left, _), (right, _)| left.session_id.cmp(&right.session_id));
    if !report_members.is_empty() {
        let duplicate_names = duplicate_comm_friendly_names(
            members.iter().map(|member| member.friendly_name.as_deref()),
        );
        output.push_str("\nCompletion reports:\n");
        for (member, report) in report_members {
            let name = comm_display_friendly_name(
                member.friendly_name.as_deref(),
                &member.session_id,
                &duplicate_names,
            );
            output.push_str(&format!(
                "\n--- {} ({}) ---\n{}\n",
                name, member.status, report
            ));
        }
    }

    output
}

pub fn format_comm_channels(channels: &[SwarmChannelInfo]) -> String {
    if channels.is_empty() {
        "No swarm channels found.".to_string()
    } else {
        let mut output = String::from("Swarm channels:\n\n");
        for channel in channels {
            output.push_str(&format!(
                "  #{} — {} subscriber{}\n",
                channel.channel,
                channel.member_count,
                if channel.member_count == 1 { "" } else { "s" }
            ));
        }
        output
    }
}
