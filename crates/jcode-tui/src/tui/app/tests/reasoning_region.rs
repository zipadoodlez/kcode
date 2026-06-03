// Tests for the streaming reasoning blockquote region helpers.
//
// Reasoning text is rendered as a dim-gutter markdown blockquote whose body is
// styled dim + italic, with a `Thought for Xs` footer, instead of a single
// floating emoji prefix or a separate header line. The region must auto-close
// when real output or a tool call begins so the final answer renders as normal
// (non-quoted) text.

#[test]
fn reasoning_region_wraps_thinking_in_blockquote_with_footer_and_no_header() {
    let mut app = create_test_app();

    app.open_reasoning_region();
    app.append_reasoning_text("Let me think.\nSecond thought.");
    app.close_reasoning_region(Some("*Thought for 2.5s*".to_string()));

    let text = app.streaming_text();
    // No standalone "Thinking" header is emitted.
    assert!(
        !text.contains("Thinking"),
        "reasoning header should be gone: {text:?}"
    );
    // Body and footer all sit inside the blockquote (each line prefixed).
    assert!(text.contains("> Let me think."), "body line not quoted: {text:?}");
    assert!(text.contains("> Second thought."), "body line not quoted: {text:?}");
    assert!(
        text.contains("> *Thought for 2.5s*"),
        "footer not quoted: {text:?}"
    );
}

#[test]
fn reasoning_region_closes_before_normal_output() {
    let mut app = create_test_app();

    app.open_reasoning_region();
    app.append_reasoning_text("thinking about it");
    // Real output begins; region must close so output is not quoted.
    app.close_reasoning_region(None);
    app.append_streaming_text("Final answer.");

    let text = app.streaming_text();
    let answer_line = text
        .lines()
        .find(|l| l.contains("Final answer."))
        .expect("answer line present");
    assert!(
        !answer_line.trim_start().starts_with('>'),
        "final answer must not be inside the reasoning blockquote: {answer_line:?}"
    );
    // A blank line separates the quote from the answer so markdown ends the quote.
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
    // Both chunks are quoted into a single contiguous blockquote (no second
    // separator/header burst between them).
    assert!(text.contains("> a"), "first chunk not quoted: {text:?}");
    assert!(text.contains("> b"), "second chunk not quoted: {text:?}");
    assert!(
        !text.contains("\n\n> b"),
        "second chunk should not restart the quote: {text:?}"
    );
}

#[test]
fn reasoning_region_renders_dim_italic_body_with_gutter() {
    use ratatui::style::Modifier;

    let mut app = create_test_app();

    app.open_reasoning_region();
    app.append_reasoning_text("considering options");
    app.close_reasoning_region(Some("*Thought for 1.0s*".to_string()));

    let lines = crate::tui::markdown::render_markdown_with_width(app.streaming_text(), Some(80));
    // Find the body line and confirm dim gutter + dim/italic body text.
    let body = lines
        .iter()
        .find(|l| {
            l.spans
                .iter()
                .any(|s| s.content.as_ref().contains("considering options"))
        })
        .expect("reasoning body line present");

    let rendered: String = body.spans.iter().map(|s| s.content.as_ref()).collect();
    assert!(
        rendered.contains('│'),
        "expected dim blockquote gutter around reasoning, got: {rendered:?}"
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
