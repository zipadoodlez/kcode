//! Adapter from swarm member status into the inline gallery layout.
//!
//! All presentation logic (status colors, role glyphs, age formatting, header,
//! sorting, layout config) lives in the shared
//! [`jcode_tui_render::swarm_gallery`] module so the live TUI and the
//! `swarm_gallery_live` demo render identically. This adapter only handles
//! turning a [`SwarmMemberStatus`] into a renderer-agnostic
//! [`GalleryMember`] (label + body lines).

use crate::protocol::SwarmMemberStatus;
use jcode_tui_render::swarm_gallery::{
    GalleryMember, SwarmStripHint, display_order, humanize_age, render_gallery, render_swarm_dock,
    render_swarm_panel, render_swarm_strip,
};
use ratatui::prelude::*;

fn member_label(member: &SwarmMemberStatus) -> String {
    member
        .friendly_name
        .clone()
        .unwrap_or_else(|| member.session_id.chars().take(8).collect())
}

/// Age marker appended to member bodies, e.g. "· 7s ago" or "· now".
/// `humanize_age` already yields "now" for fresh updates, which reads wrong
/// with an "ago" suffix.
fn age_marker(age: u64) -> String {
    let human = humanize_age(age);
    if human == "now" {
        "· now".to_string()
    } else {
        format!("· {human} ago")
    }
}

/// Build the body lines shown inside a member's viewport. Prefers live streamed
/// output (the tail) when present; otherwise surfaces the latest detail plus a
/// status-age hint.
fn member_body(member: &SwarmMemberStatus) -> Vec<String> {
    // Live streamed output wins: show the worker's in-progress assistant text.
    if let Some(tail) = member.output_tail.as_ref().filter(|t| !t.trim().is_empty()) {
        let mut body: Vec<String> = tail.lines().map(|l| l.to_string()).collect();
        if let Some(age) = member.status_age_secs {
            body.push(age_marker(age));
        }
        return body;
    }
    let mut body: Vec<String> = Vec::new();
    if let Some(detail) = member.detail.as_ref().filter(|d| !d.trim().is_empty()) {
        body.push(detail.clone());
    }
    if let Some(age) = member.status_age_secs {
        body.push(age_marker(age));
    }
    body
}

/// Convert swarm members into renderer-agnostic gallery members.
fn members_to_gallery(members: &[SwarmMemberStatus]) -> Vec<GalleryMember> {
    members
        .iter()
        .map(|member| GalleryMember {
            label: member_label(member),
            status: member.status.clone(),
            task: member.task_label.clone(),
            role: member.role.clone(),
            body: member_body(member),
            sort_key: member.session_id.clone(),
            todo: member.todo_progress,
            todo_items: member
                .todo_items
                .iter()
                .map(|t| jcode_tui_render::swarm_gallery::GalleryTodo {
                    content: t.content.clone(),
                    status: t.status.clone(),
                })
                .collect(),
        })
        .collect()
}

/// Render the inline swarm gallery for the given members into `area`-width lines.
#[allow(dead_code)]
pub(crate) fn render_swarm_gallery_lines(
    members: &[SwarmMemberStatus],
    width: usize,
    max_height: usize,
) -> Vec<Line<'static>> {
    if members.is_empty() {
        return Vec::new();
    }
    render_gallery(&members_to_gallery(members), width, max_height)
}

/// Render the list+detail swarm panel: a compact list of managed agents plus a
/// detail viewport for the `selected` one. `focused` adds an interaction hint.
#[allow(dead_code)]
pub(crate) fn render_swarm_panel_lines(
    members: &[SwarmMemberStatus],
    selected: usize,
    focused: bool,
    width: usize,
    max_height: usize,
) -> Vec<Line<'static>> {
    if members.is_empty() {
        return Vec::new();
    }
    render_swarm_panel(
        &members_to_gallery(members),
        selected,
        focused,
        width,
        max_height,
    )
}

/// Render the compact swarm strip (agent chips + status glyphs + todo counts)
/// shown directly above the status line.
///
/// `focus_key` is the configured chord to enter the controls (e.g. "ctrl+t"),
/// used both for the unfocused enter-hint and as the first focused hint.
/// `spinner_frame` animates active agents' glyphs. `max_height` bounds the
/// focused strip (chips + expanded hovered-agent detail + hints).
pub(crate) fn render_swarm_strip_lines(
    members: &[SwarmMemberStatus],
    selected: usize,
    focused: bool,
    focus_key: &str,
    spinner_frame: usize,
    width: usize,
    max_height: usize,
) -> Vec<Line<'static>> {
    if members.is_empty() {
        return Vec::new();
    }
    let enter_hint = format!("{focus_key} controls");
    let hints = vec![
        SwarmStripHint {
            key: "↑/↓".into(),
            label: "select".into(),
        },
        SwarmStripHint {
            key: "enter".into(),
            label: "pop out".into(),
        },
        SwarmStripHint {
            key: "esc".into(),
            label: "exit".into(),
        },
    ];
    render_swarm_strip(
        &members_to_gallery(members),
        selected,
        focused,
        &hints,
        if focused {
            None
        } else {
            Some(enter_hint.as_str())
        },
        spinner_frame,
        width,
        max_height,
    )
}

/// Render the swarm dock widget body: a narrow vertical agent list for the
/// info-widget margins. `plan` is the coordinator's swarm plan progress
/// (completed, total), shown in the header when present.
pub(crate) fn render_swarm_dock_lines(
    members: &[SwarmMemberStatus],
    selected: usize,
    focused: bool,
    plan: Option<(u32, u32)>,
    spinner_frame: usize,
    width: usize,
    max_height: usize,
) -> Vec<Line<'static>> {
    if members.is_empty() {
        return Vec::new();
    }
    render_swarm_dock(
        &members_to_gallery(members),
        selected,
        focused,
        plan,
        spinner_frame,
        width,
        max_height,
    )
}

/// Session ids of `members` in the same order the panel/gallery displays them
/// (coordinator first, then worktree manager, then by session id). Lets the TUI
/// map a selected panel index back to a concrete session for pop-out.
///
/// Delegates to the renderer's [`display_order`] on the exact same
/// [`GalleryMember`] conversion used for rendering, so the pop-out index can
/// never drift from what is on screen.
pub(crate) fn members_display_order(members: &[SwarmMemberStatus]) -> Vec<String> {
    display_order(&members_to_gallery(members))
        .into_iter()
        .map(|i| members[i].session_id.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use jcode_tui_render::swarm_gallery::members_to_tiles;

    fn member(
        id: &str,
        status: &str,
        detail: Option<&str>,
        role: Option<&str>,
    ) -> SwarmMemberStatus {
        SwarmMemberStatus {
            session_id: id.to_string(),
            friendly_name: Some(id.to_string()),
            status: status.to_string(),
            detail: detail.map(str::to_string),
            task_label: None,
            role: role.map(str::to_string),
            is_headless: Some(true),
            live_attachments: None,
            status_age_secs: Some(3),
            output_tail: None,
            report_back_to_session_id: None,
            todo_progress: None,
            todo_items: Vec::new(),
        }
    }

    #[test]
    fn coordinator_sorts_first() {
        let members = vec![
            member("zeta", "running", None, None),
            member("alpha", "running", None, Some("coordinator")),
        ];
        let tiles = members_to_tiles(&members_to_gallery(&members));
        assert_eq!(tiles[0].title, "alpha");
        assert_eq!(tiles[0].role_glyph.as_deref(), Some("★"));
    }

    /// Regression: pop-out selection resolves `swarm_panel_selected` through
    /// `members_display_order`, so its order must match what the renderer
    /// actually draws (tile order) for mixed roles, ties, and unnamed
    /// sessions. If this ever diverges, pop-out opens the wrong agent.
    #[test]
    fn members_display_order_matches_rendered_tile_order() {
        let mut members = vec![
            member("zeta-session", "running", None, None),
            member("wt-session", "done", None, Some("worktree_manager")),
            member("coord-session", "running", None, Some("coordinator")),
            member("mystery-session", "thinking", None, Some("mystery_role")),
            member("alpha-session", "failed", None, None),
        ];
        // Unnamed session: label falls back to a session-id prefix.
        let mut unnamed = member("beta-session-long-id", "ready", None, None);
        unnamed.friendly_name = None;
        members.push(unnamed);

        let order = members_display_order(&members);
        assert_eq!(order.len(), members.len());

        // Map each ordered session id to the label the renderer would show.
        let ordered_labels: Vec<String> = order
            .iter()
            .map(|id| {
                let m = members.iter().find(|m| &m.session_id == id).unwrap();
                member_label(m)
            })
            .collect();
        let tile_titles: Vec<String> = members_to_tiles(&members_to_gallery(&members))
            .into_iter()
            .map(|t| t.title)
            .collect();
        assert_eq!(
            ordered_labels, tile_titles,
            "pop-out order must match rendered tile order"
        );

        // Sanity: coordinator first, worktree manager second, then the rest
        // active-first (thinking/running), then failed, then idle, ties by id.
        assert_eq!(order[0], "coord-session");
        assert_eq!(order[1], "wt-session");
        assert_eq!(
            &order[2..],
            &[
                "mystery-session".to_string(),
                "zeta-session".to_string(),
                "alpha-session".to_string(),
                "beta-session-long-id".to_string(),
            ]
        );
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

    #[test]
    fn output_tail_takes_priority_over_detail() {
        let mut m = member("alpha", "running", Some("the detail line"), None);
        m.output_tail = Some("line one\nline two".to_string());
        let body = member_body(&m);
        assert_eq!(body[0], "line one");
        assert_eq!(body[1], "line two");
        assert!(!body.iter().any(|l| l.contains("the detail line")));
    }
}
