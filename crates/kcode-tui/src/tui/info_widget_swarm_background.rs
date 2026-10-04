use super::{BackgroundInfo, InfoWidgetData, truncate_smart};
use kcode_tui_style::theme::{
    accent_color, border_color, header_name_color, pending_color, tool_color,
};
use ratatui::prelude::*;

pub(super) fn render_background_widget(data: &InfoWidgetData, inner: Rect) -> Vec<Line<'static>> {
    let Some(info) = &data.background_info else {
        return Vec::new();
    };

    render_background_lines(info, inner.width as usize)
}

pub(super) fn render_background_compact(info: &BackgroundInfo) -> Vec<Line<'static>> {
    render_background_lines(info, 40)
}

fn render_background_lines(info: &BackgroundInfo, width: usize) -> Vec<Line<'static>> {
    let Some(summary) = background_summary(info) else {
        return Vec::new();
    };
    let mut lines = vec![Line::from(vec![
        Span::styled("⏳ ", Style::default().fg(accent_color())),
        Span::styled(summary, Style::default().fg(pending_color())),
    ])];

    let row_width = width.saturating_sub(4).max(12);
    for (index, task) in info.running_tasks.iter().take(3).enumerate() {
        let detail = if index == 0 {
            info.progress_detail.as_deref()
        } else {
            None
        };
        let row_text = if let Some(detail) = detail {
            truncate_smart(&format!("{} · {}", task, detail), row_width)
        } else {
            truncate_smart(task, row_width)
        };
        lines.push(Line::from(vec![
            Span::styled("  • ", Style::default().fg(tool_color())),
            Span::styled(row_text, Style::default().fg(header_name_color())),
        ]));
    }

    let hidden = info.running_tasks.len().saturating_sub(3);
    if hidden > 0 {
        lines.push(Line::from(vec![
            Span::styled("   ", Style::default().fg(border_color())),
            Span::styled(
                format!("+{} more", hidden),
                Style::default().fg(pending_color()),
            ),
        ]));
    }

    lines
}

fn background_summary(info: &BackgroundInfo) -> Option<String> {
    if info.running_count == 0 {
        return None;
    }

    Some(format!("Background · {} running", info.running_count))
}
