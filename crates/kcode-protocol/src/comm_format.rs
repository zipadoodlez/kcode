use super::{ContextEntry, SwarmChannelInfo};

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
