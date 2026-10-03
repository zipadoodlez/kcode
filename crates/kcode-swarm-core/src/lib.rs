use kcode_plan::TaskItem;
use std::collections::{HashMap, HashSet};

pub use kcode_session_types::{SwarmLifecycleStatus, SwarmMemberRecord, SwarmRole};

/// Message/report bodies longer than this require a sender-provided `tldr`
/// so receiving UIs can render them collapsed to one line with an expand
/// control instead of dumping the full body into the transcript.
pub const SWARM_TLDR_REQUIRED_OVER_CHARS: usize = 240;

/// Upper bound for a sender-provided `tldr`. Anything longer defeats the
/// purpose of a one-line collapsed summary.
pub const MAX_SWARM_TLDR_CHARS: usize = 200;

/// Validate a sender-provided `tldr` against the message body it summarizes.
///
/// Returns the normalized (trimmed, whitespace-collapsed) tldr when present,
/// `Ok(None)` when the body is short enough to not need one, and a
/// human/model-actionable error when a long body is missing a tldr or the
/// tldr itself is malformed (too long or multi-line).
pub fn validate_swarm_tldr(
    tldr: Option<&str>,
    body: &str,
    context: &str,
) -> Result<Option<String>, String> {
    let normalized = tldr
        .map(|t| t.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|t| !t.is_empty());

    if let Some(ref tldr) = normalized {
        let chars = tldr.chars().count();
        if chars > MAX_SWARM_TLDR_CHARS {
            return Err(format!(
                "'tldr' for {context} is too long ({chars} chars, max {MAX_SWARM_TLDR_CHARS}). \
                 Provide a single short line summarizing the message."
            ));
        }
        return Ok(normalized);
    }

    let body_chars = body.chars().count();
    if body_chars > SWARM_TLDR_REQUIRED_OVER_CHARS {
        return Err(format!(
            "'tldr' is required for {context} because the body is {body_chars} chars \
             (over {SWARM_TLDR_REQUIRED_OVER_CHARS}). Add a one-line 'tldr' (under \
             {MAX_SWARM_TLDR_CHARS} chars) summarizing it; recipients see the tldr \
             collapsed with an expand control."
        ));
    }

    Ok(None)
}

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

/// Bidirectional index for swarm channel subscriptions.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChannelIndex {
    pub by_swarm_channel: HashMap<String, HashMap<String, HashSet<String>>>,
    pub by_session: HashMap<String, HashMap<String, HashSet<String>>>,
}

impl ChannelIndex {
    pub fn subscribe(&mut self, session_id: &str, swarm_id: &str, channel: &str) {
        self.by_swarm_channel
            .entry(swarm_id.to_string())
            .or_default()
            .entry(channel.to_string())
            .or_default()
            .insert(session_id.to_string());
        self.by_session
            .entry(session_id.to_string())
            .or_default()
            .entry(swarm_id.to_string())
            .or_default()
            .insert(channel.to_string());
    }

    pub fn unsubscribe(&mut self, session_id: &str, swarm_id: &str, channel: &str) {
        let mut remove_swarm = false;
        if let Some(swarm_subs) = self.by_swarm_channel.get_mut(swarm_id) {
            if let Some(members) = swarm_subs.get_mut(channel) {
                members.remove(session_id);
                if members.is_empty() {
                    swarm_subs.remove(channel);
                }
            }
            remove_swarm = swarm_subs.is_empty();
        }
        if remove_swarm {
            self.by_swarm_channel.remove(swarm_id);
        }

        let mut remove_session_entry = false;
        if let Some(session_subs) = self.by_session.get_mut(session_id) {
            let mut remove_swarm_entry = false;
            if let Some(channels) = session_subs.get_mut(swarm_id) {
                channels.remove(channel);
                remove_swarm_entry = channels.is_empty();
            }
            if remove_swarm_entry {
                session_subs.remove(swarm_id);
            }
            remove_session_entry = session_subs.is_empty();
        }
        if remove_session_entry {
            self.by_session.remove(session_id);
        }
    }

    pub fn remove_session(&mut self, session_id: &str) {
        if let Some(session_subscriptions) = self.by_session.remove(session_id) {
            for (swarm_id, channels) in session_subscriptions {
                let mut remove_swarm = false;
                if let Some(swarm_subs) = self.by_swarm_channel.get_mut(&swarm_id) {
                    for channel_name in channels {
                        if let Some(members) = swarm_subs.get_mut(&channel_name) {
                            members.remove(session_id);
                            if members.is_empty() {
                                swarm_subs.remove(&channel_name);
                            }
                        }
                    }
                    remove_swarm = swarm_subs.is_empty();
                }
                if remove_swarm {
                    self.by_swarm_channel.remove(&swarm_id);
                }
            }
            return;
        }

        let swarm_ids: Vec<String> = self.by_swarm_channel.keys().cloned().collect();
        for swarm_id in swarm_ids {
            let mut remove_swarm = false;
            if let Some(swarm_subs) = self.by_swarm_channel.get_mut(&swarm_id) {
                let channel_names: Vec<String> = swarm_subs.keys().cloned().collect();
                for channel_name in channel_names {
                    if let Some(members) = swarm_subs.get_mut(&channel_name) {
                        members.remove(session_id);
                        if members.is_empty() {
                            swarm_subs.remove(&channel_name);
                        }
                    }
                }
                remove_swarm = swarm_subs.is_empty();
            }
            if remove_swarm {
                self.by_swarm_channel.remove(&swarm_id);
            }
        }
    }

    pub fn members(&self, swarm_id: &str, channel: &str) -> Vec<String> {
        let mut members = self
            .by_swarm_channel
            .get(swarm_id)
            .and_then(|swarm_subs| swarm_subs.get(channel))
            .map(|members| members.iter().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        members.sort();
        members
    }

    #[cfg(test)]
    pub fn channels_for_session(&self, session_id: &str, swarm_id: &str) -> Vec<String> {
        let mut channels = self
            .by_session
            .get(session_id)
            .and_then(|session_subs| session_subs.get(swarm_id))
            .map(|channels| channels.iter().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        channels.sort();
        channels
    }
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
    fn validate_swarm_tldr_allows_short_body_without_tldr() {
        assert_eq!(validate_swarm_tldr(None, "quick note", "this DM"), Ok(None));
    }

    #[test]
    fn validate_swarm_tldr_requires_tldr_for_long_body() {
        let body = "x".repeat(SWARM_TLDR_REQUIRED_OVER_CHARS + 1);
        let err = validate_swarm_tldr(None, &body, "this DM").unwrap_err();
        assert!(err.contains("'tldr' is required"), "{err}");
        assert!(err.contains("this DM"), "{err}");
    }

    #[test]
    fn validate_swarm_tldr_normalizes_whitespace() {
        let body = "x".repeat(SWARM_TLDR_REQUIRED_OVER_CHARS + 1);
        assert_eq!(
            validate_swarm_tldr(Some("  did\nthe   thing  "), &body, "this report"),
            Ok(Some("did the thing".to_string()))
        );
    }

    #[test]
    fn validate_swarm_tldr_rejects_overlong_tldr() {
        let tldr = "y".repeat(MAX_SWARM_TLDR_CHARS + 1);
        let err = validate_swarm_tldr(Some(&tldr), "body", "this message").unwrap_err();
        assert!(err.contains("too long"), "{err}");
    }

    #[test]
    fn validate_swarm_tldr_blank_tldr_counts_as_missing() {
        let body = "x".repeat(SWARM_TLDR_REQUIRED_OVER_CHARS + 1);
        assert!(validate_swarm_tldr(Some("   "), &body, "this DM").is_err());
        assert_eq!(
            validate_swarm_tldr(Some("   "), "short", "this DM"),
            Ok(None)
        );
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
    fn channel_index_keeps_bidirectional_maps_in_sync() {
        let mut index = ChannelIndex::default();
        index.subscribe("worker-1", "swarm-a", "build");
        index.subscribe("worker-1", "swarm-a", "tests");
        index.subscribe("worker-2", "swarm-a", "build");

        assert_eq!(
            index.members("swarm-a", "build"),
            vec!["worker-1", "worker-2"]
        );
        assert_eq!(
            index.channels_for_session("worker-1", "swarm-a"),
            vec!["build", "tests"]
        );

        index.unsubscribe("worker-1", "swarm-a", "build");
        assert_eq!(index.members("swarm-a", "build"), vec!["worker-2"]);

        index.remove_session("worker-1");
        assert!(index.channels_for_session("worker-1", "swarm-a").is_empty());
        assert_eq!(index.members("swarm-a", "tests"), Vec::<String>::new());
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
