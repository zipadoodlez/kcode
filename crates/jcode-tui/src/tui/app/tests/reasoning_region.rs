// Tests for the streaming reasoning blockquote region helpers.
//
// Reasoning text is rendered as a dim-gutter markdown blockquote with an italic
// `Thinking` header and a `Thought for Xs` footer, instead of a single floating
// emoji prefix. The region must auto-close when real output or a tool call
// begins so the final answer renders as normal (non-quoted) text.

#[test]
fn reasoning_region_wraps_thinking_in_blockquote_with_header_and_footer() {
    let mut app = create_test_app();

    app.open_reasoning_region();
    app.append_reasoning_text("Let me think.\nSecond thought.");
    app.close_reasoning_region(Some("*Thought for 2.5s*".to_string()));

    let text = app.streaming_text();
    // Header, body, and footer all sit inside the blockquote (each line prefixed).
    assert!(text.contains("> *Thinking…*"), "missing italic header: {text:?}");
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
    app.append_reasoning_text("a");
    app.open_reasoning_region(); // no-op while open
    app.append_reasoning_text("b");

    let text = app.streaming_text();
    let header_count = text.matches("*Thinking…*").count();
    assert_eq!(header_count, 1, "header emitted more than once: {text:?}");
}

#[test]
fn reasoning_region_renders_with_dim_gutter() {
    let mut app = create_test_app();

    app.open_reasoning_region();
    app.append_reasoning_text("considering options");
    app.close_reasoning_region(Some("*Thought for 1.0s*".to_string()));

    let lines = crate::tui::markdown::render_markdown_with_width(app.streaming_text(), Some(80));
    let rendered: Vec<String> = lines
        .iter()
        .map(|l| l.spans.iter().map(|s| s.content.as_ref()).collect::<String>())
        .collect();
    assert!(
        rendered
            .iter()
            .any(|l| l.contains('│') && l.contains("considering options")),
        "expected dim blockquote gutter around reasoning, got: {rendered:?}"
    );
}
