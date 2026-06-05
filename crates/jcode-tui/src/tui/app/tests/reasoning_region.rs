// Tests for the streaming reasoning region helpers.
//
// Reasoning text is rendered as dim, italic lines (no blockquote `│` gutter, no
// header, no footer). Each complete line is wrapped in `*…*` with an invisible
// REASONING_SENTINEL inside both ends that the markdown renderer strips and dims.
// (Both ends so whitespace at the line edges can't break CommonMark emphasis.) The
// region auto-closes when real output or a tool call begins so the final answer
// renders as normal (non-italic) text.
//
// The in-progress (not yet newline-terminated) line renders live as a partial
// `*…*` tail so reasoning trickles in token-by-token; that tail is rebuilt in
// place on each delta and promoted to a committed line when its newline arrives.

#[test]
fn reasoning_region_emits_dim_italic_lines_no_gutter_header_or_footer() {
    let mut app = create_test_app();

    app.open_reasoning_region();
    app.append_reasoning_text("Let me think.\nSecond thought.");
    // While streaming, reasoning is dim+italic markup in the live stream buffer.
    let streaming = app.streaming_text().to_string();
    assert!(
        !streaming.contains("Thinking"),
        "no header expected: {streaming:?}"
    );
    assert!(
        !streaming.contains('>'),
        "no blockquote gutter expected: {streaming:?}"
    );
    assert!(
        !streaming.contains("Thought for"),
        "no footer expected: {streaming:?}"
    );
    let sentinel = jcode_tui_markdown::REASONING_SENTINEL;
    assert!(
        streaming.contains(&format!("*{sentinel}Let me think.{sentinel}*")),
        "first line not dim+italic: {streaming:?}"
    );
    assert!(
        streaming.contains(&format!("*{sentinel}Second thought.{sentinel}*")),
        "second line not dim+italic: {streaming:?}"
    );

    // In `current` mode (the default), closing moves the block into a dedicated
    // collapsing `"reasoning"` display message and clears it from the stream.
    app.close_reasoning_region(None);
    assert!(
        app.streaming_text().is_empty(),
        "reasoning should leave the live stream once collapsed: {:?}",
        app.streaming_text()
    );
    let reasoning_msg = app
        .display_messages
        .iter()
        .find(|m| m.role == "reasoning")
        .expect("reasoning message present");
    assert!(
        reasoning_msg.content.contains(sentinel),
        "reasoning message keeps dim+italic markup: {:?}",
        reasoning_msg.content
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

    // The answer stays in the live stream and must never be styled as reasoning.
    let text = app.streaming_text();
    assert!(
        text.contains("Final answer."),
        "answer present in stream: {text:?}"
    );
    let answer_line = text
        .lines()
        .find(|l| l.contains("Final answer."))
        .expect("answer line present");
    assert!(
        !answer_line.contains(jcode_tui_markdown::REASONING_SENTINEL),
        "final answer must not be styled as reasoning: {answer_line:?}"
    );
    // The reasoning collapsed into its own message; it is no longer in the stream.
    assert!(
        !text.contains(jcode_tui_markdown::REASONING_SENTINEL),
        "reasoning must not remain in the answer stream: {text:?}"
    );
    assert!(
        app.display_messages.iter().any(|m| m.role == "reasoning"),
        "a collapsing reasoning message should exist"
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

    // The split-across-deltas line is committed as a single emphasis run in the
    // collapsed reasoning message.
    let content = app
        .display_messages
        .iter()
        .find(|m| m.role == "reasoning")
        .map(|m| m.content.clone())
        .expect("reasoning message present");
    let sentinel = jcode_tui_markdown::REASONING_SENTINEL;
    assert!(
        content.contains(&format!("*{sentinel}one two{sentinel}*")),
        "split line must be one emphasis run: {content:?}"
    );
}

#[test]
fn reasoning_region_renders_dim_italic_text_without_gutter() {
    use ratatui::style::Modifier;

    let mut app = create_test_app();

    app.open_reasoning_region();
    app.append_reasoning_text("considering options\n");
    app.close_reasoning_region(None);

    // In `current` mode the reasoning now lives in a dedicated collapsing message.
    let reasoning_content = app
        .display_messages
        .iter()
        .find(|m| m.role == "reasoning")
        .map(|m| m.content.clone())
        .expect("reasoning message present");

    let lines = crate::tui::markdown::render_markdown_with_width(&reasoning_content, Some(80));
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

#[test]
fn reasoning_partial_line_renders_live_before_newline() {
    // The in-progress line (no trailing newline) must render immediately as a
    // dim+italic partial tail so reasoning streams token-by-token.
    let mut app = create_test_app();
    let sentinel = jcode_tui_markdown::REASONING_SENTINEL;

    app.open_reasoning_region();
    app.append_reasoning_text("partial thou");

    let text = app.streaming_text();
    assert!(
        text.contains(&format!("*{sentinel}partial thou{sentinel}*")),
        "partial line should render live: {text:?}"
    );
}

#[test]
fn reasoning_partial_tail_grows_in_place_without_duplication() {
    // Successive deltas of the same line replace the live tail (truncate + rebuild)
    // rather than appending duplicate fragments.
    let mut app = create_test_app();
    let sentinel = jcode_tui_markdown::REASONING_SENTINEL;

    app.open_reasoning_region();
    app.append_reasoning_text("one ");
    app.append_reasoning_text("two ");
    app.append_reasoning_text("three");

    let text = app.streaming_text();
    assert!(
        text.contains(&format!("*{sentinel}one two three{sentinel}*")),
        "tail should grow in place: {text:?}"
    );
    // The earlier partial fragments must not linger as separate runs.
    assert!(
        !text.contains(&format!("*{sentinel}one {sentinel}*")),
        "stale partial tail should be replaced, not duplicated: {text:?}"
    );
    assert_eq!(
        text.matches(sentinel).count(),
        2,
        "exactly one live emphasis run (two sentinels) expected: {text:?}"
    );
}

#[test]
fn reasoning_partial_promotes_to_committed_line_on_newline() {
    // When the newline arrives, the live tail becomes a committed line and a fresh
    // (empty) tail follows; no duplicate copies of the completed line remain.
    let mut app = create_test_app();
    let sentinel = jcode_tui_markdown::REASONING_SENTINEL;

    app.open_reasoning_region();
    app.append_reasoning_text("growing line");
    app.append_reasoning_text("\nnext");

    let text = app.streaming_text();
    // Committed first line (hard-break terminated) and a live second-line tail.
    assert!(
        text.contains(&format!("*{sentinel}growing line{sentinel}*  \n")),
        "first line should be committed with a hard break: {text:?}"
    );
    assert!(
        text.contains(&format!("*{sentinel}next{sentinel}*")),
        "second line should render live: {text:?}"
    );
    // The completed line must appear exactly once (no partial+committed duplication).
    assert_eq!(
        text.matches(&format!("*{sentinel}growing line{sentinel}*"))
            .count(),
        1,
        "completed line must not be duplicated: {text:?}"
    );
}

#[test]
fn reasoning_close_promotes_pending_partial_line() {
    // Closing the region with an in-progress (no-newline) partial promotes it to a
    // committed line exactly once, then collapses into the reasoning message.
    let mut app = create_test_app();
    let sentinel = jcode_tui_markdown::REASONING_SENTINEL;

    app.open_reasoning_region();
    app.append_reasoning_text("final thought");
    app.close_reasoning_region(None);

    // The live stream no longer carries the reasoning; it moved into its message.
    assert!(
        app.streaming_text().is_empty(),
        "reasoning should leave the live stream once collapsed: {:?}",
        app.streaming_text()
    );
    let content = app
        .display_messages
        .iter()
        .find(|m| m.role == "reasoning")
        .map(|m| m.content.clone())
        .expect("reasoning message present");
    assert_eq!(
        content
            .matches(&format!("*{sentinel}final thought{sentinel}*"))
            .count(),
        1,
        "pending partial promoted exactly once on close: {content:?}"
    );
}

#[test]
fn reasoning_block_line_markups_keeps_only_sentinel_lines() {
    use crate::tui::app::input::{reasoning_block_line_markups, reasoning_message_content};

    let mut block = String::new();
    block.push_str(&jcode_tui_markdown::reasoning_line_markup("alpha"));
    block.push('\n'); // a blank separator line (no sentinel)
    block.push_str(&jcode_tui_markdown::reasoning_line_markup("beta"));

    let lines = reasoning_block_line_markups(&block);
    assert_eq!(lines.len(), 2, "blank separators are dropped: {lines:?}");
    let sentinel = jcode_tui_markdown::REASONING_SENTINEL;
    assert!(lines[0].contains(&format!("{sentinel}alpha{sentinel}")));
    assert!(lines[1].contains(&format!("{sentinel}beta{sentinel}")));

    // Full content shows every line; remaining==0 shows only the summary.
    let summary = jcode_tui_markdown::reasoning_line_markup("▸ thought");
    let full = reasoning_message_content(&summary, &lines, lines.len());
    assert!(full.contains("alpha") && full.contains("beta"));
    let collapsed = reasoning_message_content(&summary, &lines, 0);
    assert!(collapsed.contains("▸ thought"));
    assert!(!collapsed.contains("alpha") && !collapsed.contains("beta"));

    // A partial reveal keeps the *trailing* lines (oldest fold away first).
    let partial = reasoning_message_content(&summary, &lines, 1);
    assert!(partial.contains("beta"), "trailing line kept: {partial:?}");
    assert!(!partial.contains("alpha"), "leading line folded: {partial:?}");
}

#[test]
fn reasoning_summary_markup_uses_duration_when_known() {
    use crate::tui::app::input::reasoning_summary_markup;
    use std::time::Duration;

    let with_secs = reasoning_summary_markup(3, Some(Duration::from_secs(12)));
    assert!(with_secs.contains("▸ thought for 12s"), "{with_secs:?}");

    let no_time = reasoning_summary_markup(4, None);
    assert!(no_time.contains("▸ thought (4 lines)"), "{no_time:?}");
}

#[test]
fn reasoning_collapse_finalizes_to_single_summary_line() {
    let mut app = create_test_app();

    app.open_reasoning_region();
    app.append_reasoning_text("first\nsecond\nthird\n");
    app.close_reasoning_region(None);

    assert!(app.reasoning_collapse_active(), "collapse should start");

    // Snapping finalizes the message to just the summary line.
    app.finalize_reasoning_collapse();
    assert!(!app.reasoning_collapse_active(), "collapse cleared on finalize");

    let content = app
        .display_messages
        .iter()
        .find(|m| m.role == "reasoning")
        .map(|m| m.content.clone())
        .expect("reasoning message present");
    assert!(content.contains("▸ thought"), "summary present: {content:?}");
    assert!(!content.contains("first"), "lines folded away: {content:?}");
    assert!(!content.contains("third"), "lines folded away: {content:?}");
}

#[test]
fn reasoning_collapse_drops_when_target_message_replaced() {
    let mut app = create_test_app();

    app.open_reasoning_region();
    app.append_reasoning_text("thinking\n");
    app.close_reasoning_region(None);
    assert!(app.reasoning_collapse_active());

    // A transcript reset must invalidate the animation target safely.
    app.clear_display_messages();
    assert!(!app.reasoning_collapse_active());
    // Advancing now is a no-op and must not panic.
    assert!(!app.advance_reasoning_collapse());
}

#[test]
fn reasoning_collapse_visible_lines_shrink_monotonically_over_time() {
    use crate::tui::app::input::REASONING_COLLAPSE_DURATION;
    use std::time::Duration;

    let mut app = create_test_app();
    app.open_reasoning_region();
    app.append_reasoning_text("l1\nl2\nl3\nl4\nl5\nl6\n");
    app.close_reasoning_region(None);
    let sentinel = jcode_tui_markdown::REASONING_SENTINEL;

    let count_visible = |app: &App| -> usize {
        app.display_messages
            .iter()
            .find(|m| m.role == "reasoning")
            .map(|m| {
                m.content
                    .split_inclusive('\n')
                    .filter(|seg| seg.contains(sentinel))
                    .filter(|seg| !seg.contains('▸'))
                    .count()
            })
            .unwrap_or(0)
    };

    // Sample the eased timeline; visible reasoning lines must never increase and
    // must reach a single summary line (0 source lines) at/after the duration.
    let dur = REASONING_COLLAPSE_DURATION;
    let mut prev = usize::MAX;
    for frac in [0.0_f32, 0.25, 0.5, 0.75, 1.0] {
        let elapsed = Duration::from_secs_f32(dur.as_secs_f32() * frac);
        app.backdate_reasoning_collapse_for_test(elapsed)
            .expect("collapse active");
        app.advance_reasoning_collapse();
        let visible = count_visible(&app);
        assert!(
            visible <= prev,
            "visible lines must not increase: frac={frac} visible={visible} prev={prev}"
        );
        prev = visible;
    }

    // Past the duration the animation is finalized to the summary only.
    assert!(!app.reasoning_collapse_active(), "collapse should finish");
    assert_eq!(count_visible(&app), 0, "only the summary line remains");
}
