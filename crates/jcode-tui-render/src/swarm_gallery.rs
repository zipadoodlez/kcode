//! Shared presentation logic for the inline swarm gallery.
//!
//! This is the single source of truth for how swarm-agent viewports look:
//! status accent colors, role glyphs, age formatting, the header line, member
//! sorting, and the gallery [`SwarmGalleryConfig`]. Both the live TUI adapter
//! (`jcode-tui`) and the `swarm_gallery_live` demo map their own data into
//! [`GalleryMember`] and call [`render_gallery`], so the demo renders identical
//! output to production and the two cannot drift.

use ratatui::prelude::*;

use jcode_tui_style::color::rgb;

use crate::swarm_tiles::{SwarmGalleryConfig, SwarmTile, render_swarm_gallery};

/// Accent color for a member lifecycle status.
pub fn status_accent(status: &str) -> Color {
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

/// Optional glyph prefixed to a member's title based on its swarm role.
pub fn role_glyph(role: Option<&str>) -> Option<&'static str> {
    match role {
        Some("coordinator") => Some("★"),
        Some("worktree_manager") => Some("◆"),
        _ => None,
    }
}

/// Compact age formatting for member viewports (now/Ns/Nm/Nh).
pub fn humanize_age(age: u64) -> String {
    if age < 2 {
        "now".to_string()
    } else if age < 60 {
        format!("{age}s")
    } else if age < 3600 {
        format!("{}m", age / 60)
    } else {
        format!("{}h", age / 3600)
    }
}

/// Whether a status counts as "active" for the header's active-agent tally.
pub fn is_active_status(status: &str) -> bool {
    matches!(status, "running" | "streaming" | "thinking")
}

/// Sort rank for stable placement: coordinator first, then worktree manager,
/// then everything else.
fn role_rank(role: Option<&str>) -> u8 {
    match role {
        Some("coordinator") => 0,
        Some("worktree_manager") => 1,
        _ => 2,
    }
}

/// The header line shown above the gallery grid.
pub fn gallery_header(total: usize, active: usize) -> Line<'static> {
    Line::from(vec![
        Span::styled("🐝 ", Style::default().fg(rgb(255, 200, 100))),
        Span::styled(
            format!(
                "swarm · {} agent{}{}",
                total,
                if total == 1 { "" } else { "s" },
                if active > 0 {
                    format!(" · {active} active")
                } else {
                    String::new()
                }
            ),
            Style::default().fg(rgb(160, 160, 170)),
        ),
    ])
}

/// A renderer-agnostic view of one swarm member, ready for layout.
///
/// Callers are responsible for building the `body` lines (e.g. choosing live
/// output tail vs. status detail); everything else about how the tile looks is
/// handled here.
#[derive(Clone, Debug)]
pub struct GalleryMember {
    /// Display title (friendly name or short id).
    pub label: String,
    /// Lifecycle status string (drives the badge text and accent color).
    pub status: String,
    /// Swarm role, if any (drives the title glyph and sort order).
    pub role: Option<String>,
    /// Pre-rendered body lines shown inside the tile.
    pub body: Vec<String>,
    /// Stable tiebreaker for sorting members with equal role rank (e.g. id).
    pub sort_key: String,
}

/// Convert members into gallery tiles, sorted for stable placement
/// (coordinator first, worktree manager next, then by `sort_key`).
pub fn members_to_tiles(members: &[GalleryMember]) -> Vec<SwarmTile> {
    let mut sorted: Vec<&GalleryMember> = members.iter().collect();
    sorted.sort_by(|a, b| {
        role_rank(a.role.as_deref())
            .cmp(&role_rank(b.role.as_deref()))
            .then_with(|| a.sort_key.cmp(&b.sort_key))
    });

    sorted
        .into_iter()
        .map(|m| {
            let mut tile =
                SwarmTile::new(m.label.clone(), m.status.clone(), status_accent(&m.status))
                    .with_body(m.body.clone());
            if let Some(glyph) = role_glyph(m.role.as_deref()) {
                tile = tile.with_role_glyph(glyph);
            }
            tile
        })
        .collect()
}

/// Render the inline swarm gallery for `members` into `width`-bounded lines.
///
/// `max_height` is the total height budget for the band (including the header);
/// the gallery grid gets `max_height - 1` rows. Returns an empty vec when there
/// are no members.
pub fn render_gallery(
    members: &[GalleryMember],
    width: usize,
    max_height: usize,
) -> Vec<Line<'static>> {
    if members.is_empty() {
        return Vec::new();
    }
    let tiles = members_to_tiles(members);
    let active = members
        .iter()
        .filter(|m| is_active_status(&m.status))
        .count();
    let header = gallery_header(members.len(), active);
    let cfg = SwarmGalleryConfig {
        max_height: max_height.saturating_sub(1).max(4),
        ..Default::default()
    };
    render_swarm_gallery(&tiles, width, &cfg, Some(header))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member(id: &str, status: &str, role: Option<&str>, body: &[&str]) -> GalleryMember {
        GalleryMember {
            label: id.to_string(),
            status: status.to_string(),
            role: role.map(str::to_string),
            body: body.iter().map(|s| s.to_string()).collect(),
            sort_key: id.to_string(),
        }
    }

    #[test]
    fn coordinator_sorts_first() {
        let members = vec![
            member("zeta", "running", None, &[]),
            member("alpha", "running", Some("coordinator"), &[]),
        ];
        let tiles = members_to_tiles(&members);
        assert_eq!(tiles[0].title, "alpha");
        assert_eq!(tiles[0].role_glyph.as_deref(), Some("★"));
    }

    #[test]
    fn renders_header_and_is_width_bounded() {
        let members = vec![
            member("alpha", "running", None, &["editing config.rs"]),
            member("beta", "done", None, &["reviewed"]),
        ];
        let lines = render_gallery(&members, 80, 12);
        assert!(!lines.is_empty());
        let header: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(header.contains("swarm · 2 agents"), "got: {header}");
        for line in &lines {
            assert!(line.width() <= 80);
        }
    }

    #[test]
    fn active_count_in_header() {
        let members = vec![
            member("a", "running", None, &[]),
            member("b", "thinking", None, &[]),
            member("c", "done", None, &[]),
        ];
        let lines = render_gallery(&members, 100, 12);
        let header: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(header.contains("2 active"), "got: {header}");
    }

    #[test]
    fn empty_members_render_nothing() {
        assert!(render_gallery(&[], 80, 12).is_empty());
    }

    #[test]
    fn humanize_age_buckets() {
        assert_eq!(humanize_age(0), "now");
        assert_eq!(humanize_age(5), "5s");
        assert_eq!(humanize_age(120), "2m");
        assert_eq!(humanize_age(7200), "2h");
    }
}
