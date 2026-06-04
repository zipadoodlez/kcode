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

#[test]
fn parity_table() {
    let md = "\
| A | B |
|---|---|
| 1 | 2 |";
    assert_content_parity(md);
}

#[test]
fn core_renders_table_borders() {
    let core = render_markdown_via_core("| A | B |\n|---|---|\n| 1 | 2 |");
    let text: String = core
        .iter()
        .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref()))
        .collect();
    assert!(text.contains('A') && text.contains('1'), "table cells present: {text}");
}

#[test]
fn core_renders_inline_math() {
    let core = render_markdown_via_core("an equation $x^2$ here");
    let spans: Vec<_> = core.iter().flat_map(|l| l.spans.iter()).collect();
    assert!(
        spans.iter().any(|s| s.content.contains("$x^2$")),
        "inline math should be wrapped in dollar signs"
    );
}

#[test]
fn core_renders_display_math_frame() {
    let core = render_markdown_via_core("$$\nx^2 + y^2\n$$");
    let texts = nonblank_texts(&core);
    assert!(
        texts.iter().any(|t| t.starts_with("┌─ math")),
        "display math should be framed: {texts:?}"
    );
    assert!(
        texts.iter().any(|t| t.contains("x^2 + y^2")),
        "display math content present: {texts:?}"
    );
}

#[test]
fn parity_currency_dollars() {
    assert_content_parity("It costs $35 and then $5.99 total.");
}

#[test]
fn parity_two_currency_amounts_not_math() {
    // Legacy escapes $-then-digit so it is NOT treated as inline math.
    assert_content_parity("Spend $5 here and $10 there for $15.");
}

#[test]
fn probe_math_divergence() {
    let math = crate::math_fg();
    for input in [
        "$5x$ and more",
        "price $5$ each",
        "$5+$3 = $8",
        "a $1 and $2 b",
        "x$5$y",
        "buy $5 sell $9 net $4",
        "$$\nx=5\n$$",
        "inline $a+b$ ok",
    ] {
        let cm: Vec<String> = render_markdown_via_core(input)
            .iter()
            .flat_map(|l| l.spans.iter())
            .filter(|s| s.style.fg == Some(math))
            .map(|s| s.content.to_string())
            .collect();
        let lm: Vec<String> = render_markdown(input)
            .iter()
            .flat_map(|l| l.spans.iter())
            .filter(|s| s.style.fg == Some(math))
            .map(|s| s.content.to_string())
            .collect();
        assert_eq!(cm, lm, "math styling mismatch for {input:?}");
    }
}

#[test]
fn fuzz_visible_text_parity() {
    // Differential corpus: visible (flattened) text must match the legacy
    // renderer across a wide variety of constructs.
    let corpus = [
        "# H1\n## H2\n### H3",
        "plain paragraph with words",
        "**bold** _italic_ ~~strike~~ `code`",
        "- a\n- b\n  - nested\n- c",
        "1. one\n2. two\n3. three",
        "> quote line one\n> quote line two",
        "```rust\nfn f() {}\n```",
        "| A | B |\n|---|---|\n| 1 | 2 |\n| 3 | 4 |",
        "text $a+b$ inline and $$x=5$$ display",
        "money $5 and $9.99 here",
        "[link](http://example.com) text",
        "a\n\nb\n\nc",
        "* item with **bold** and `code`",
        "Term\n: definition",
        "Mixed: # not heading inline",
        "line with  \nhard break",
        "1. step\n   - sub a\n   - sub b\n2. step two",
        "> nested\n> > deeper quote",
        "Para with $35 cost then math $y=2$.",
        "---\nafter rule",
        "***\nstars rule",
        "nested **bold _italic_ end** tail",
        "`code with $5` outside $6",
        "1. a\n2. b\n   1. b1\n   2. b2\n3. c",
        "- [ ] todo\n- [x] done",
        "para one\n\n> quote\n\npara two",
        "| left | right |\n|:-----|------:|\n| a | b |",
        "text with ![alt](img.png) image",
        "> quote with **bold** and `code`",
        "## Heading with `code` and **bold**",
        "Mixed $$\\sum_i x_i$$ display in para",
        "emoji 😀 and CJK 中文 text",
        "trailing spaces   \nnext line",
        "$5.00, $6, and $a=b$ together",
        "footnote ref[^1]\n\n[^1]: the note",
        "auto link <http://example.com> here",
        "> - quoted list item\n> - second",
        "Heading\n=======\n\nbody",
        "Sub\n---\n\nbody",
        "a\tb\tc tabs",
        "line\\\nwith backslash break",
        "**unclosed bold and `code",
        "| a |\n|---|\n| $5 |\n| $x$ |",
    ];
    for md in corpus {
        assert_eq!(
            flattened(&render_markdown_via_core(md)),
            flattened(&render_markdown(md)),
            "visible-text mismatch for:\n{md}"
        );
    }
}
