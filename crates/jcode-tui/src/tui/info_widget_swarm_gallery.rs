//! Adapter from swarm member status into the inline gallery layout.
//!
//! Phase 1 feeds each member's status/detail into a [`SwarmTile`]; once the
//! server streams per-agent output tails, the body will carry live output.

use crate::protocol::SwarmMemberStatus;
use crate::tui::color_support::rgb;
use jcode_tui_render::swarm_tiles::{
    SwarmGalleryConfig, SwarmTile, render_swarm_gallery,
};
use ratatui::prelude::*;

/// Accent color for a member lifecycle status.
fn status_accent(status: &str) -> Color {
    match status {
        "spawned" => rgb(140, 140, 150),
        "ready" => rgb(120, 180, 120),
        "running" | "streaming" => rgb(255, 200, 100),
        "thinking" => rgb(140, 180, 255),
        "blocked" | "waiting_network" => rgb(255, 170, 80),
        "failed" | "crashed" => rgb(255, 100, 100),
        "completed" | "done" => rgb(100, 200, 100),
        "stopped" => rgb(140, 140, 150),
        _ => rgb(140, 140, 150),
    }
}

fn role_glyph(role: Option<&str>) -> Option<&'static str> {
    match role {
        Some("coordinator") => Some("★"),
        Some("worktree_manager") => Some("◆"),
        _ => None,
    }
}

fn member_label(member: &SwarmMemberStatus) -> String {
    member
        .friendly_name
        .clone()
        .unwrap_or_else(|| member.session_id.chars().take(8).collect())
}

/// Build the body lines shown inside a member's viewport. Until live output is
/// streamed, this surfaces the latest detail plus a couple of status hints.
fn member_body(member: &SwarmMemberStatus) -> Vec<String> {
    let mut body: Vec<String> = Vec::new();
    if let Some(detail) = member.detail.as_ref().filter(|d| !d.trim().is_empty()) {
        body.push(detail.clone());
    }
    if let Some(age) = member.status_age_secs {
        let age_text = if age < 2 {
            "now".to_string()
        } else if age < 60 {
            format!("{age}s")
        } else if age < 3600 {
            format!("{}m", age / 60)
        } else {
            format!("{}h", age / 3600)
        };
        body.push(format!("· {} ago", age_text));
    }
    body
}

/// Convert swarm members into gallery tiles, sorted for stable placement
/// (coordinator first, then by session id).
pub(super) fn members_to_tiles(members: &[SwarmMemberStatus]) -> Vec<SwarmTile> {
    let mut sorted: Vec<&SwarmMemberStatus> = members.iter().collect();
    sorted.sort_by(|a, b| {
        let rank = |m: &SwarmMemberStatus| match m.role.as_deref() {
            Some("coordinator") => 0,
            Some("worktree_manager") => 1,
            _ => 2,
        };
        rank(a)
            .cmp(&rank(b))
            .then_with(|| a.session_id.cmp(&b.session_id))
    });

    sorted
        .into_iter()
        .map(|member| {
            let mut tile = SwarmTile::new(
                member_label(member),
                member.status.clone(),
                status_accent(&member.status),
            )
            .with_body(member_body(member));
            if let Some(glyph) = role_glyph(member.role.as_deref()) {
                tile = tile.with_role_glyph(glyph);
            }
            tile
        })
        .collect()
}

/// Render the inline swarm gallery for the given members into `area`-width lines.
pub(super) fn render_swarm_gallery_lines(
    members: &[SwarmMemberStatus],
    width: usize,
    max_height: usize,
) -> Vec<Line<'static>> {
    if members.is_empty() {
        return Vec::new();
    }
    let tiles = members_to_tiles(members);
    let active = members
        .iter()
        .filter(|m| matches!(m.status.as_str(), "running" | "streaming" | "thinking"))
        .count();
    let header = Line::from(vec![
        Span::styled("🐝 ", Style::default().fg(rgb(255, 200, 100))),
        Span::styled(
            format!(
                "swarm · {} agent{}{}",
                members.len(),
                if members.len() == 1 { "" } else { "s" },
                if active > 0 {
                    format!(" · {active} active")
                } else {
                    String::new()
                }
            ),
            Style::default().fg(rgb(160, 160, 170)),
        ),
    ]);
    let cfg = SwarmGalleryConfig {
        max_height: max_height.saturating_sub(1).max(4),
        ..Default::default()
    };
    render_swarm_gallery(&tiles, width, &cfg, Some(header))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member(id: &str, status: &str, detail: Option<&str>, role: Option<&str>) -> SwarmMemberStatus {
        SwarmMemberStatus {
            session_id: id.to_string(),
            friendly_name: Some(id.to_string()),
            status: status.to_string(),
            detail: detail.map(str::to_string),
            role: role.map(str::to_string),
            is_headless: Some(true),
            live_attachments: None,
            status_age_secs: Some(3),
        }
    }

    #[test]
    fn coordinator_sorts_first() {
        let members = vec![
            member("zeta", "running", None, None),
            member("alpha", "running", None, Some("coordinator")),
        ];
        let tiles = members_to_tiles(&members);
        assert_eq!(tiles[0].title, "alpha");
        assert_eq!(tiles[0].role_glyph.as_deref(), Some("★"));
    }

    #[test]
    fn renders_header_and_boxes() {
        let members = vec![
            member("alpha", "running", Some("editing config.rs"), None),
            member("beta", "done", Some("reviewed"), None),
        ];
        let lines = render_swarm_gallery_lines(&members, 80, 12);
        assert!(!lines.is_empty());
        let header: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(header.contains("swarm · 2 agents"), "got: {header}");
        for line in &lines {
            assert!(line.width() <= 80);
        }
    }

    #[test]
    fn empty_members_render_nothing() {
        assert!(render_swarm_gallery_lines(&[], 80, 12).is_empty());
    }
}
