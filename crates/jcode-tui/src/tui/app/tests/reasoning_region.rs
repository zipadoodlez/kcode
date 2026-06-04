// Tests for the streaming reasoning region helpers.
//
// Reasoning text is rendered as dim, italic lines (no blockquote `│` gutter, no
// header, no footer). Each complete line is wrapped in `*…*` with an invisible
// REASONING_SENTINEL inside both ends that the markdown renderer strips and dims.
// (Both ends so whitespace at the line edges can't break CommonMark emphasis.) The
// region auto-closes when real output or a tool call begins so the final answer
// renders as normal (non-italic) text.

#[test]
fn reasoning_region_emits_dim_italic_lines_no_gutter_header_or_footer() {
    let mut app = create_test_app();

    app.open_reasoning_region();
    app.append_reasoning_text("Let me think.\nSecond thought.");
    app.close_reasoning_region(None);

    let text = app.streaming_text();
    assert!(!text.contains("Thinking"), "no header expected: {text:?}");
    assert!(!text.contains('>'), "no blockquote gutter expected: {text:?}");
    assert!(!text.contains("Thought for"), "no footer expected: {text:?}");
    let sentinel = jcode_tui_markdown::REASONING_SENTINEL;
    assert!(
        text.contains(&format!("*{sentinel}Let me think.{sentinel}*")),
        "first line not dim+italic: {text:?}"
    );
    assert!(
        text.contains(&format!("*{sentinel}Second thought.{sentinel}*")),
        "second line not dim+italic: {text:?}"
    );
}

#[test]
fn reasoning_region_closes_before_normal_output() {
    let mut app = create_test_app();

    app.open_reasoning_region();
    app.append_reasoning_text("thinking about it\n");
    // Real output begins; region must close so output is not styled as reasoning.
    app.close_reasoning_region(None);
    app.append_streaming_text("Final answer.");

    let text = app.streaming_text();
    let answer_line = text
        .lines()
        .find(|l| l.contains("Final answer."))
        .expect("answer line present");
    assert!(
        !answer_line.contains(jcode_tui_markdown::REASONING_SENTINEL),
        "final answer must not be styled as reasoning: {answer_line:?}"
    );
    assert!(
        text.contains("\n\nFinal answer."),
        "missing blank-line separator before output: {text:?}"
    );
}

#[test]
fn reasoning_region_open_is_idempotent() {
    let mut app = create_test_app();

    app.open_reasoning_region();
    app.append_reasoning_text("a\n");
    app.open_reasoning_region(); // no-op while open
    app.append_reasoning_text("b\n");

    let text = app.streaming_text();
    let sentinel = jcode_tui_markdown::REASONING_SENTINEL;
    assert!(
        text.contains(&format!("*{sentinel}a{sentinel}*")),
        "first chunk: {text:?}"
    );
    assert!(
        text.contains(&format!("*{sentinel}b{sentinel}*")),
        "second chunk: {text:?}"
    );
    // No extra separator burst between the two chunks.
    assert!(
        !text.contains(&format!("*{sentinel}a{sentinel}*\n\n")),
        "second chunk should not restart the region: {text:?}"
    );
}

#[test]
fn reasoning_line_split_across_deltas_stays_one_run() {
    let mut app = create_test_app();

    app.open_reasoning_region();
    app.append_reasoning_text("one ");
    app.append_reasoning_text("two\n");
    app.close_reasoning_region(None);

    let text = app.streaming_text();
    let sentinel = jcode_tui_markdown::REASONING_SENTINEL;
    assert!(
        text.contains(&format!("*{sentinel}one two{sentinel}*")),
        "split line must be one emphasis run: {text:?}"
    );
}

#[test]
fn reasoning_region_renders_dim_italic_text_without_gutter() {
    use ratatui::style::Modifier;

    let mut app = create_test_app();

    app.open_reasoning_region();
    app.append_reasoning_text("considering options\n");
    app.close_reasoning_region(None);

    let lines = crate::tui::markdown::render_markdown_with_width(app.streaming_text(), Some(80));
    let body = lines
        .iter()
        .find(|l| {
            l.spans
                .iter()
                .any(|s| s.content.as_ref().contains("considering options"))
        })
        .expect("reasoning body line present");

    let rendered: String = body.spans.iter().map(|s| s.content.as_ref()).collect();
    // No blockquote gutter, and the sentinel is stripped from the visible text.
    assert!(!rendered.contains('│'), "no gutter expected: {rendered:?}");
    assert!(
        !rendered.contains(jcode_tui_markdown::REASONING_SENTINEL),
        "sentinel must be stripped: {rendered:?}"
    );

    let body_span = body
        .spans
        .iter()
        .find(|s| s.content.as_ref().contains("considering options"))
        .expect("body span present");
    assert!(
        body_span.style.add_modifier.contains(Modifier::ITALIC),
        "reasoning body should be italic: {:?}",
        body_span.style
    );
}

#[test]
fn strip_reasoning_lines_removes_reasoning_keeps_answer() {
    use crate::tui::app::input::strip_reasoning_lines;

    // Build content the way the streaming buffer would: reasoning lines wrapped
    // with the sentinel, then a normal answer paragraph.
    let mut content = String::new();
    content.push_str(&jcode_tui_markdown::reasoning_line_markup("thinking one"));
    content.push_str(&jcode_tui_markdown::reasoning_line_markup("thinking two"));
    content.push('\n');
    content.push_str("Here is the answer.\n");

    let stripped = strip_reasoning_lines(&content);
    assert_eq!(stripped, "Here is the answer.");
    assert!(!stripped.contains(jcode_tui_markdown::REASONING_SENTINEL));
}

#[test]
fn strip_reasoning_lines_reasoning_only_becomes_empty() {
    use crate::tui::app::input::strip_reasoning_lines;

    let mut content = String::new();
    content.push_str(&jcode_tui_markdown::reasoning_line_markup("only thinking"));
    let stripped = strip_reasoning_lines(&content);
    assert!(stripped.trim().is_empty(), "got: {stripped:?}");
}
