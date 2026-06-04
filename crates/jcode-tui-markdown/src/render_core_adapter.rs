//! Adapter: backend-neutral [`jcode_render_core::Document`] -> ratatui lines.
//!
//! This is the thin TUI-side translation layer for the shared render core. It
//! resolves the core's semantic [`StyleRole`]/[`FillRole`] to this crate's
//! concrete terminal palette (the same `*_color()` helpers the legacy renderer
//! uses) and turns [`StyledLine`]s into `ratatui::Line<'static>`.
//!
//! The legacy `render_markdown*` path remains authoritative; this adapter is
//! validated against it before any switchover.

use jcode_render_core::{
    Alignment as CoreAlignment, BlockKind, Document, FillRole, StyleRole, StyledLine, StyledSpan,
};
use ratatui::layout::Alignment;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::{
    bold_color, code_bg, code_fg, heading_h1_color, heading_h2_color, heading_h3_color,
    heading_color, html_fg, link_fg, math_fg, md_dim_color, text_color,
};

/// Convert a parsed neutral [`Document`] into ratatui lines using the TUI
/// palette. Blocks are separated by a blank line, matching document spacing.
/// Decorative framing (blockquote bars, code-block borders) is reproduced to
/// match the legacy renderer.
pub fn document_to_lines(doc: &Document) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    for (idx, block) in doc.blocks.iter().enumerate() {
        if idx > 0 {
            lines.push(Line::default());
        }
        match &block.kind {
            BlockKind::CodeBlock { language } => {
                push_code_block(&mut lines, block, language.as_deref());
            }
            BlockKind::MathDisplay => {
                push_math_display(&mut lines, block);
            }
            BlockKind::Table => {
                // Layout is width-dependent; defer to the legacy table renderer
                // for exact parity. Unbounded width here (callers that wrap pass
                // a width via the wrapped variant).
                lines.extend(crate::render_support::render_table(&block.table, None));
            }
            BlockKind::ThematicBreak => {
                lines.push(Line::from(Span::styled(
                    "─".repeat(crate::RULE_LEN),
                    Style::default().fg(md_dim_color()),
                )));
            }
            BlockKind::BlockQuote => {
                // The quote gutter (`│ ` per nesting level) is baked into the
                // stored lines by the parser, so render spans verbatim.
                for sl in &block.lines {
                    lines.push(styled_line_to_line(sl, &block.kind));
                }
            }
            _ => {
                for sl in &block.lines {
                    lines.push(styled_line_to_line(sl, &block.kind));
                }
            }
        }
    }
    lines
}

/// Render a code block with the legacy frame: `┌─ lang`, `│ ` gutter per line,
/// and a closing `└─`.
fn push_code_block(lines: &mut Vec<Line<'static>>, block: &jcode_render_core::Block, language: Option<&str>) {
    let dim = Style::default().fg(md_dim_color());
    let header = match language {
        Some(lang) if !lang.is_empty() => format!("┌─ {lang}"),
        _ => "┌─".to_string(),
    };
    lines.push(Line::from(Span::styled(header, dim)));
    for sl in &block.lines {
        let mut spans = vec![Span::styled("│ ".to_string(), dim)];
        spans.extend(sl.spans.iter().map(|s| styled_span_to_span(s, &block.kind)));
        lines.push(Line::from(spans));
    }
    lines.push(Line::from(Span::styled("└─".to_string(), dim)));
}

/// Render a display-math block with the legacy frame: `┌─ math `, `│ ` gutter
/// per source line, and a closing `└─`.
fn push_math_display(lines: &mut Vec<Line<'static>>, block: &jcode_render_core::Block) {
    let dim = Style::default().fg(md_dim_color());
    lines.push(Line::from(Span::styled("┌─ math ".to_string(), dim)).left_aligned());
    for sl in &block.lines {
        let text = sl.plain_text();
        lines.push(
            Line::from(vec![
                Span::styled("│ ".to_string(), dim),
                Span::styled(text, Style::default().fg(math_fg())),
            ])
            .left_aligned(),
        );
    }
    lines.push(Line::from(Span::styled("└─".to_string(), dim)).left_aligned());
}

/// Convert one neutral [`StyledLine`] to a ratatui [`Line`], given the block it
/// belongs to (used to pick heading-level color).
pub fn styled_line_to_line(sl: &StyledLine, kind: &BlockKind) -> Line<'static> {
    let spans: Vec<Span<'static>> = sl
        .spans
        .iter()
        .map(|s| styled_span_to_span(s, kind))
        .collect();
    let mut line = Line::from(spans);
    line.alignment = Some(match sl.alignment {
        CoreAlignment::Left => Alignment::Left,
        CoreAlignment::Center => Alignment::Center,
        CoreAlignment::Right => Alignment::Right,
    });
    line
}

fn styled_span_to_span(span: &StyledSpan, kind: &BlockKind) -> Span<'static> {
    let mut style = Style::default().fg(role_color(span.role, kind));

    if span.fill == FillRole::Code {
        style = style.bg(code_bg());
    }
    if span.attrs.bold {
        style = style.add_modifier(Modifier::BOLD);
    }
    if span.attrs.italic {
        style = style.add_modifier(Modifier::ITALIC);
    }
    if span.attrs.strikethrough {
        style = style.add_modifier(Modifier::CROSSED_OUT);
    }
    if span.attrs.underline {
        style = style.add_modifier(Modifier::UNDERLINED);
    }

    Span::styled(span.text.clone(), style)
}

fn role_color(role: StyleRole, kind: &BlockKind) -> ratatui::style::Color {
    match role {
        StyleRole::Text => text_color(),
        StyleRole::Dim => md_dim_color(),
        StyleRole::Code => code_fg(),
        StyleRole::Link => link_fg(),
        StyleRole::Html => html_fg(),
        StyleRole::Reasoning => md_dim_color(),
        StyleRole::Math => math_fg(),
        StyleRole::Strong => match kind {
            BlockKind::Heading { level } => match level {
                1 => heading_h1_color(),
                2 => heading_h2_color(),
                3 => heading_h3_color(),
                _ => heading_color(),
            },
            _ => bold_color(),
        },
    }
}

/// Parse markdown and render it to ratatui lines through the shared core.
pub fn render_markdown_via_core(text: &str) -> Vec<Line<'static>> {
    document_to_lines(&jcode_render_core::parse_markdown(text))
}

/// Like [`render_markdown_via_core`] but wraps each block's lines to `width`
/// columns using the shared wrapper. Decorated blocks (code, math, tables,
/// blockquotes) use their decoration-aware rendering; tables additionally get
/// the width constraint.
pub fn render_markdown_via_core_wrapped(text: &str, width: usize) -> Vec<Line<'static>> {
    use jcode_render_core::{ColumnWidth, wrap_lines};
    let doc = jcode_render_core::parse_markdown(text);
    let mut out: Vec<Line<'static>> = Vec::new();
    for (idx, block) in doc.blocks.iter().enumerate() {
        if idx > 0 {
            out.push(Line::default());
        }
        match &block.kind {
            // Source layout preserved (not reflowed).
            BlockKind::CodeBlock { language } => {
                push_code_block(&mut out, block, language.as_deref());
            }
            BlockKind::MathDisplay => {
                push_math_display(&mut out, block);
            }
            BlockKind::Table => {
                out.extend(crate::render_support::render_table(&block.table, Some(width)));
            }
            BlockKind::ThematicBreak => {
                // Legacy fills the available width when one is known.
                out.push(Line::from(Span::styled(
                    "─".repeat(width),
                    Style::default().fg(md_dim_color()),
                )));
            }
            BlockKind::BlockQuote => {
                // Gutter prefix is baked in; render verbatim (no reflow) to keep
                // it aligned, matching the legacy line-per-source-line behavior.
                for sl in &block.lines {
                    out.push(styled_line_to_line(sl, &block.kind));
                }
            }
            _ => {
                let wrapped = wrap_lines(&block.lines, width, &ColumnWidth);
                for sl in &wrapped {
                    out.push(styled_line_to_line(sl, &block.kind));
                }
            }
        }
    }
    out
}

#[cfg(test)]
#[path = "render_core_adapter_tests.rs"]
mod tests;
