//! Parity checks: the shared-core adapter vs. the legacy renderer.
//!
//! The two renderers differ in spacing details and some decorative styling, so
//! these tests assert *content* parity (the visible text, modulo blank-line
//! padding and decorative markers) plus key styling invariants, rather than
//! byte-identical `Line` equality. The goal is to prove the shared core
//! reproduces the legacy renderer's meaning before any switchover.

use crate::{render_markdown, render_markdown_via_core};
use ratatui::text::Line;

/// Visible text of each non-blank line, trimmed, for loose comparison.
fn nonblank_texts(lines: &[Line<'static>]) -> Vec<String> {
    lines
        .iter()
        .map(|l| {
            l.spans
                .iter()
                .map(|s| s.content.as_ref())
                .collect::<String>()
        })
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Concatenated visible text with whitespace collapsed, for content-equality
/// checks that ignore layout/spacing differences.
fn flattened(lines: &[Line<'static>]) -> String {
    let joined = nonblank_texts(lines).join(" ");
    joined.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn assert_content_parity(md: &str) {
    let legacy = render_markdown(md);
    let core = render_markdown_via_core(md);
    assert_eq!(
        flattened(&core),
        flattened(&legacy),
        "content mismatch for input:\n{md}\n--- legacy ---\n{:?}\n--- core ---\n{:?}",
        nonblank_texts(&legacy),
        nonblank_texts(&core),
    );
}

#[test]
fn parity_plain_paragraph() {
    assert_content_parity("Hello world, this is a paragraph.");
}

#[test]
fn parity_heading_and_paragraph() {
    assert_content_parity("# Title\n\nSome body text here.");
}

#[test]
fn parity_emphasis() {
    assert_content_parity("This is *italic* and **bold** and `code`.");
}

#[test]
fn parity_unordered_list() {
    assert_content_parity("- alpha\n- beta\n- gamma");
}

#[test]
fn parity_ordered_list() {
    assert_content_parity("1. first\n2. second\n3. third");
}

#[test]
fn parity_code_block() {
    assert_content_parity("```rust\nfn main() {\n    println!(\"hi\");\n}\n```");
}

#[test]
fn parity_blockquote() {
    assert_content_parity("> a quoted line");
}

#[test]
fn parity_mixed_document() {
    let md = "\
# Heading

Intro paragraph with **bold** and a `snippet`.

- one
- two

Closing line.";
    assert_content_parity(md);
}

#[test]
fn core_marks_bold_and_code_styling() {
    let core = render_markdown_via_core("text **bold** and `code`");
    let spans: Vec<_> = core.iter().flat_map(|l| l.spans.iter()).collect();
    assert!(
        spans
            .iter()
            .any(|s| s.content.contains("bold")
                && s.style.add_modifier.contains(ratatui::style::Modifier::BOLD)),
        "bold word should carry BOLD modifier"
    );
    assert!(
        spans.iter().any(|s| s.content.contains("code") && s.style.bg.is_some()),
        "inline code should carry a background fill"
    );
}
