use super::text::truncate_smart;
use super::{GitInfo, InfoWidgetData};
use jcode_tui_style::theme::{
    border_color, error_color, header_name_color, pending_color, success_color, warning_color,
};
use ratatui::prelude::*;

pub(super) fn render_git_widget(data: &InfoWidgetData, inner: Rect) -> Vec<Line<'static>> {
    let Some(info) = &data.git_info else {
        return Vec::new();
    };
    if !info.is_interesting() {
        return Vec::new();
    }

    let w = inner.width as usize;
    let mut lines: Vec<Line> = Vec::new();

    let mut parts: Vec<Span> = Vec::new();
    parts.push(Span::styled(" ", Style::default().fg(warning_color())));

    let mut stats_len = 0usize;
    if info.ahead > 0 {
        stats_len += format!(" ↑{}", info.ahead).chars().count();
    }
    if info.behind > 0 {
        stats_len += format!(" ↓{}", info.behind).chars().count();
    }
    if info.modified > 0 {
        stats_len += format!(" ~{}", info.modified).chars().count();
    }
    if info.staged > 0 {
        stats_len += format!(" +{}", info.staged).chars().count();
    }
    if info.untracked > 0 {
        stats_len += format!(" ?{}", info.untracked).chars().count();
    }

    let branch_max = w.saturating_sub(2 + stats_len).max(4);
    let branch_display = truncate_smart(&info.branch, branch_max);
    parts.push(Span::styled(
        branch_display,
        Style::default()
            .fg(header_name_color())
            .add_modifier(Modifier::BOLD),
    ));

    if info.modified > 0 {
        parts.push(Span::styled(
            format!(" ~{}", info.modified),
            Style::default().fg(warning_color()),
        ));
    }
    if info.staged > 0 {
        parts.push(Span::styled(
            format!(" +{}", info.staged),
            Style::default().fg(success_color()),
        ));
    }
    if info.untracked > 0 {
        parts.push(Span::styled(
            format!(" ?{}", info.untracked),
            Style::default().fg(pending_color()),
        ));
    }
    if info.ahead > 0 {
        parts.push(Span::styled(
            format!(" ↑{}", info.ahead),
            Style::default().fg(success_color()),
        ));
    }
    if info.behind > 0 {
        parts.push(Span::styled(
            format!(" ↓{}", info.behind),
            Style::default().fg(error_color()),
        ));
    }

    lines.push(Line::from(parts));

    let max_files = inner.height.saturating_sub(lines.len() as u16).min(5) as usize;
    for file in info.dirty_files.iter().take(max_files) {
        let display = truncate_smart(file, w.saturating_sub(4));
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(display, Style::default().fg(pending_color())),
        ]));
    }
    if info.dirty_files.len() > max_files {
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                format!("+{} more", info.dirty_files.len() - max_files),
                Style::default().fg(border_color()),
            ),
        ]));
    }

    lines
}

pub(super) fn render_git_compact(info: &GitInfo, width: u16) -> Vec<Line<'static>> {
    let w = width as usize;
    let mut parts: Vec<Span> = Vec::new();

    let branch_display = truncate_smart(&info.branch, w.saturating_sub(12).max(6));
    parts.push(Span::styled(" ", Style::default().fg(warning_color())));
    parts.push(Span::styled(
        branch_display,
        Style::default().fg(pending_color()),
    ));

    if info.ahead > 0 {
        parts.push(Span::styled(
            format!(" ↑{}", info.ahead),
            Style::default().fg(success_color()),
        ));
    }
    if info.behind > 0 {
        parts.push(Span::styled(
            format!(" ↓{}", info.behind),
            Style::default().fg(error_color()),
        ));
    }
    if info.modified > 0 {
        parts.push(Span::styled(
            format!(" ~{}", info.modified),
            Style::default().fg(warning_color()),
        ));
    }
    if info.staged > 0 {
        parts.push(Span::styled(
            format!(" +{}", info.staged),
            Style::default().fg(success_color()),
        ));
    }
    if info.untracked > 0 {
        parts.push(Span::styled(
            format!(" ?{}", info.untracked),
            Style::default().fg(pending_color()),
        ));
    }

    vec![Line::from(parts)]
}
