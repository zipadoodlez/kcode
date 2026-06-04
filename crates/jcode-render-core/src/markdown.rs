//! Markdown -> backend-neutral [`Document`].
//!
//! This mirrors the *semantics* of the TUI markdown renderer
//! (`jcode-tui-markdown`) but emits the neutral [`crate::model`] types instead
//! of `ratatui` spans. Front-ends adapt the document to their backend and may
//! wrap it with [`crate::wrap`].
//!
//! Scope note: this is the shared foundation. It currently covers headings,
//! paragraphs, inline emphasis/strong/strike/code, fenced & indented code
//! blocks, blockquotes, ordered/unordered (incl. nested) lists, thematic
//! breaks, links, and raw HTML passthrough. Tables and math are tracked as
//! follow-ups; the TUI renderer remains authoritative until parity is proven.

use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};

use crate::model::{
    Alignment, Block, BlockKind, Document, FillRole, StyleRole, StyledLine, StyledSpan, TextAttrs,
};

#[derive(Clone, Copy, Default)]
struct InlineStyle {
    bold: bool,
    italic: bool,
    strike: bool,
}

impl InlineStyle {
    fn attrs(self) -> TextAttrs {
        TextAttrs {
            bold: self.bold,
            italic: self.italic,
            strikethrough: self.strike,
            underline: false,
        }
    }

    fn role(self) -> StyleRole {
        if self.bold {
            StyleRole::Strong
        } else {
            StyleRole::Text
        }
    }
}

struct ListFrame {
    ordered: bool,
    next_number: u64,
}

/// The block kind that inline content flushed in the current context belongs
/// to, based on enclosing blockquote/list nesting.
fn current_kind(blockquote_depth: usize, list_stack: &[ListFrame]) -> BlockKind {
    if blockquote_depth > 0 {
        BlockKind::BlockQuote
    } else if let Some(frame) = list_stack.last() {
        BlockKind::ListItem {
            ordered: frame.ordered,
            depth: list_stack.len().saturating_sub(1),
        }
    } else {
        BlockKind::Paragraph
    }
}

/// Parse markdown into a backend-neutral [`Document`].
pub fn parse_markdown(text: &str) -> Document {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_FOOTNOTES);
    options.insert(Options::ENABLE_GFM);
    options.insert(Options::ENABLE_SMART_PUNCTUATION);

    let parser = Parser::new_ext(text, options);

    let mut doc = Document::default();

    // Inline accumulation for the block currently being built.
    let mut spans: Vec<StyledSpan> = Vec::new();
    let mut style = InlineStyle::default();

    // Block context.
    let mut heading_level: Option<u8> = None;
    let mut blockquote_depth = 0usize;
    let mut list_stack: Vec<ListFrame> = Vec::new();

    // Code block accumulation.
    let mut in_code_block = false;
    let mut code_lang: Option<String> = None;
    let mut code_buf = String::new();

    // Pending list-item marker prefix to emit when the item's first inline
    // text arrives.
    let mut pending_item_marker: Option<String> = None;

    let push_block = |doc: &mut Document, kind: BlockKind, lines: Vec<StyledLine>| {
        if !lines.is_empty() {
            doc.blocks.push(Block::new(kind, lines));
        }
    };

    let flush_paragraph = |doc: &mut Document,
                           spans: &mut Vec<StyledSpan>,
                           kind: BlockKind,
                           alignment: Alignment| {
        if spans.is_empty() {
            return;
        }
        let line = StyledLine::aligned(std::mem::take(spans), alignment);
        push_block(doc, kind, vec![line]);
    };

    for event in parser {
        match event {
            // ---- block starts ----
            Event::Start(Tag::Heading { level, .. }) => {
                flush_paragraph(&mut doc, &mut spans, BlockKind::Paragraph, Alignment::Left);
                heading_level = Some(level as u8);
            }
            Event::Start(Tag::Paragraph) => {
                // marker (if any) is emitted lazily on first text
            }
            Event::Start(Tag::BlockQuote(_)) => {
                flush_paragraph(&mut doc, &mut spans, current_kind(blockquote_depth, &list_stack), Alignment::Left);
                blockquote_depth += 1;
            }
            Event::Start(Tag::List(first)) => {
                flush_paragraph(&mut doc, &mut spans, current_kind(blockquote_depth, &list_stack), Alignment::Left);
                list_stack.push(ListFrame {
                    ordered: first.is_some(),
                    next_number: first.unwrap_or(1),
                });
            }
            Event::Start(Tag::Item) => {
                let depth = list_stack.len().saturating_sub(1);
                let indent = "  ".repeat(depth);
                let marker = if let Some(frame) = list_stack.last_mut() {
                    if frame.ordered {
                        let n = frame.next_number;
                        frame.next_number += 1;
                        format!("{indent}{n}. ")
                    } else {
                        format!("{indent}• ")
                    }
                } else {
                    String::new()
                };
                pending_item_marker = Some(marker);
            }
            Event::Start(Tag::CodeBlock(kind)) => {
                flush_paragraph(&mut doc, &mut spans, BlockKind::Paragraph, Alignment::Left);
                in_code_block = true;
                code_buf.clear();
                code_lang = match kind {
                    CodeBlockKind::Fenced(lang) if !lang.is_empty() => Some(lang.to_string()),
                    _ => None,
                };
            }
            Event::Start(Tag::Emphasis) => style.italic = true,
            Event::Start(Tag::Strong) => style.bold = true,
            Event::Start(Tag::Strikethrough) => style.strike = true,
            Event::Start(Tag::Link { .. }) => {}
            Event::Start(Tag::Image { .. }) => {}

            // ---- inline content ----
            Event::Text(t) => {
                if in_code_block {
                    code_buf.push_str(&t);
                } else {
                    if let Some(marker) = pending_item_marker.take() {
                        spans.push(StyledSpan::new(marker, StyleRole::Dim));
                    }
                    spans.push(StyledSpan {
                        text: t.to_string(),
                        role: style.role(),
                        fill: FillRole::None,
                        attrs: style.attrs(),
                    });
                }
            }
            Event::Code(t) => {
                if let Some(marker) = pending_item_marker.take() {
                    spans.push(StyledSpan::new(marker, StyleRole::Dim));
                }
                spans.push(StyledSpan {
                    text: t.to_string(),
                    role: StyleRole::Code,
                    fill: FillRole::Code,
                    attrs: TextAttrs::none(),
                });
            }
            Event::SoftBreak | Event::HardBreak => {
                spans.push(StyledSpan::plain(" "));
            }
            Event::Html(raw) | Event::InlineHtml(raw) => {
                spans.push(StyledSpan {
                    text: raw.to_string(),
                    role: StyleRole::Html,
                    fill: FillRole::None,
                    attrs: TextAttrs {
                        italic: true,
                        ..TextAttrs::none()
                    },
                });
            }
            Event::Rule => {
                flush_paragraph(&mut doc, &mut spans, BlockKind::Paragraph, Alignment::Left);
                push_block(
                    &mut doc,
                    BlockKind::ThematicBreak,
                    vec![StyledLine::from_spans(vec![StyledSpan::new(
                        "─".repeat(3),
                        StyleRole::Dim,
                    )])],
                );
            }

            // ---- block ends ----
            Event::End(TagEnd::Heading(_)) => {
                let level = heading_level.take().unwrap_or(1);
                // Headings render with strong role + bold across the line.
                for s in spans.iter_mut() {
                    s.role = StyleRole::Strong;
                    s.attrs.bold = true;
                }
                flush_paragraph(
                    &mut doc,
                    &mut spans,
                    BlockKind::Heading { level },
                    Alignment::Left,
                );
            }
            Event::End(TagEnd::Paragraph) => {
                let kind = current_kind(blockquote_depth, &list_stack);
                flush_paragraph(&mut doc, &mut spans, kind, Alignment::Left);
            }
            Event::End(TagEnd::Item) => {
                // Item with no paragraph child (tight list): flush inline buffer.
                if !spans.is_empty() {
                    let kind = current_kind(blockquote_depth, &list_stack);
                    flush_paragraph(&mut doc, &mut spans, kind, Alignment::Left);
                }
                pending_item_marker = None;
            }
            Event::End(TagEnd::List(_)) => {
                list_stack.pop();
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                blockquote_depth = blockquote_depth.saturating_sub(1);
            }
            Event::End(TagEnd::CodeBlock) => {
                let lines: Vec<StyledLine> = code_buf
                    .trim_end_matches('\n')
                    .split('\n')
                    .map(|l| {
                        StyledLine::from_spans(vec![StyledSpan {
                            text: l.to_string(),
                            role: StyleRole::Code,
                            fill: FillRole::Code,
                            attrs: TextAttrs::none(),
                        }])
                    })
                    .collect();
                push_block(
                    &mut doc,
                    BlockKind::CodeBlock {
                        language: code_lang.take(),
                    },
                    lines,
                );
                in_code_block = false;
                code_buf.clear();
            }
            Event::End(TagEnd::Emphasis) => style.italic = false,
            Event::End(TagEnd::Strong) => style.bold = false,
            Event::End(TagEnd::Strikethrough) => style.strike = false,

            _ => {}
        }
    }

    // Trailing inline buffer.
    flush_paragraph(&mut doc, &mut spans, BlockKind::Paragraph, Alignment::Left);

    doc
}
