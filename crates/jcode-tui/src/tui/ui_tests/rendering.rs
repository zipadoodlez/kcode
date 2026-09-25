use super::*;

#[test]
fn test_render_rounded_box_sides_aligned() {
    let content = vec![
        Line::from("short"),
        Line::from("a longer line of text here"),
        Line::from("mid"),
    ];
    let style = Style::default();
    let lines = render_rounded_box("title", content, 40, style);
    assert!(lines.len() >= 5);
    let top_width = lines[0].width();
    let bottom_width = lines[lines.len() - 1].width();
    assert_eq!(
        top_width, bottom_width,
        "top and bottom borders must be same width: top={}, bottom={}",
        top_width, bottom_width
    );
    for (i, line) in lines.iter().enumerate() {
        assert_eq!(
            line.width(),
            top_width,
            "line {} has width {} but expected {} (content: {:?})",
            i,
            line.width(),
            top_width,
            line.spans
                .iter()
                .map(|s| s.content.as_ref())
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn test_render_rounded_box_emoji_title_aligned() {
    let content = vec![
        Line::from("memory content line one"),
        Line::from("memory content line two"),
    ];
    let style = Style::default();
    let lines = render_rounded_box("🧠 recalled 2 memories", content, 50, style);
    assert!(lines.len() >= 4);
    let top_width = lines[0].width();
    let bottom_width = lines[lines.len() - 1].width();
    assert_eq!(
        top_width, bottom_width,
        "emoji title: top={}, bottom={}",
        top_width, bottom_width
    );
    for (i, line) in lines.iter().enumerate() {
        assert_eq!(
            line.width(),
            top_width,
            "emoji title: line {} width {} != expected {}",
            i,
            line.width(),
            top_width
        );
    }
}

#[test]
fn test_render_rounded_box_long_title_keeps_body_width_in_sync() {
    let content = vec![Line::from("tiny")];
    let style = Style::default();
    let lines = render_rounded_box("✓ bg bash completed · 6150794bik", content, 24, style);

    assert!(lines.len() >= 3);
    let top_width = lines[0].width();
    assert_eq!(top_width, 24, "box should respect max width");
    for (i, line) in lines.iter().enumerate() {
        assert_eq!(
            line.width(),
            top_width,
            "long title: line {} width {} != expected {}",
            i,
            line.width(),
            top_width
        );
    }
}

#[test]
fn test_render_direct_message_as_compact_agent_row() {
    crate::tui::markdown::set_center_code_blocks(false);
    let msg = DisplayMessage::swarm("DM from fox", "Can you take parser tests?");

    let lines = render_swarm_message(&msg, 80, crate::config::DiffDisplayMode::Off);
    let rendered: Vec<String> = lines.iter().map(extract_line_text).collect();

    assert_eq!(rendered, vec!["🦊 Can you take parser tests?"]);
    assert!(
        rendered
            .iter()
            .all(|line| !line.contains('│') && !line.contains('✉')),
        "direct messages should render without rails or type icons: {:?}",
        rendered
    );
}

#[test]
fn test_render_swarm_message_matches_exact_compact_snapshot() {
    crate::tui::markdown::set_center_code_blocks(false);
    let msg = DisplayMessage::swarm("Task · sheep", "Implement compaction asymptotic fixes");

    let lines = render_swarm_message(&msg, 80, crate::config::DiffDisplayMode::Off);
    let rendered: Vec<String> = lines.iter().map(extract_line_text).collect();

    assert_eq!(
        rendered,
        vec!["🐑 Implement compaction asymptotic fixes".to_string()]
    );
}

#[test]
fn test_render_swarm_await_as_compact_rail_free_summary() {
    crate::tui::markdown::set_center_code_blocks(false);
    let msg = DisplayMessage::swarm("🐝 Swarm await", "✓ 2/2");

    let lines = render_swarm_message(&msg, 80, crate::config::DiffDisplayMode::Off);
    let rendered: Vec<String> = lines.iter().map(extract_line_text).collect();

    assert_eq!(rendered, vec!["🐝 ✓ 2/2"]);
    assert!(rendered.iter().all(|line| !line.contains('│')));
}

#[test]
fn test_render_swarm_await_wake_message_as_compact_rail_free_summary() {
    crate::tui::markdown::set_center_code_blocks(false);
    let msg = DisplayMessage::background_task(
        "🐝 **Swarm await finished**\n\nAll members done. All 1 members are done: sabertooth\n\nMember statuses:\n  ✓ sabertooth (completed)\n\nCompletion reports:\n\n--- sabertooth (completed) ---\nAwait UI demo complete."
            .to_string(),
    );

    let lines = render_background_task_message(&msg, 80, crate::config::DiffDisplayMode::Off);
    let rendered: Vec<String> = lines.iter().map(extract_line_text).collect();

    assert_eq!(rendered, vec!["🐝 ✓ 1/1"]);
    assert!(rendered.iter().all(|line| !line.contains('│')));
    assert!(!rendered.join("\n").contains("sabertooth"));
}

#[test]
fn test_render_swarm_message_trims_extra_newlines() {
    crate::tui::markdown::set_center_code_blocks(false);
    let msg = DisplayMessage::swarm("Broadcast · coordinator", "\n\nPlan updated\n\n");

    let lines = render_swarm_message(&msg, 80, crate::config::DiffDisplayMode::Off);
    let rendered: Vec<String> = lines.iter().map(extract_line_text).collect();

    assert_eq!(rendered, vec!["💫 📣 Plan updated"]);
    assert!(rendered.iter().all(|line| !line.contains('│')));
}

#[test]
fn test_render_channel_and_shared_context_as_compact_agent_rows() {
    crate::tui::markdown::set_center_code_blocks(false);

    let channel = DisplayMessage::swarm("#dev · fox", "Can someone review this?");
    let context = DisplayMessage::swarm("Shared context · fox", "branch = feature/auth");

    let channel_lines = render_swarm_message(&channel, 80, crate::config::DiffDisplayMode::Off)
        .iter()
        .map(extract_line_text)
        .collect::<Vec<_>>();
    let context_lines = render_swarm_message(&context, 80, crate::config::DiffDisplayMode::Off)
        .iter()
        .map(extract_line_text)
        .collect::<Vec<_>>();

    assert_eq!(channel_lines, vec!["🦊 #dev · Can someone review this?"]);
    assert_eq!(context_lines, vec!["🦊 🧠 branch · feature/auth"]);
}

#[test]
fn test_render_file_activity_as_collapsible_compact_row() {
    crate::tui::markdown::set_center_code_blocks(false);
    let content = jcode_tui_messages::encode_collapsible_swarm_content(
        "src/auth.rs · modified",
        "```text\n-old\n+new\n```",
    );
    let msg = DisplayMessage::swarm("File activity · fox", content);

    let rendered = render_swarm_message(&msg, 80, crate::config::DiffDisplayMode::Off)
        .iter()
        .map(extract_line_text)
        .collect::<Vec<_>>();

    assert_eq!(rendered, vec!["🦊 ✎ src/auth.rs · modified  ▸ diff"]);
    assert!(rendered.iter().all(|line| !line.contains('│')));
}

#[test]
fn test_render_file_conflict_places_warning_before_agent() {
    crate::tui::markdown::set_center_code_blocks(false);
    let msg = DisplayMessage::swarm("File conflict · fox", "src/auth.rs · concurrent edits");

    let rendered = render_swarm_message(&msg, 80, crate::config::DiffDisplayMode::Off)
        .iter()
        .map(extract_line_text)
        .collect::<Vec<_>>();

    assert_eq!(rendered, vec!["⚠ 🦊 src/auth.rs · concurrent edits"]);
}

#[test]
fn test_render_swarm_message_uses_agent_emoji_for_assignments() {
    crate::tui::markdown::set_center_code_blocks(false);
    let msg = DisplayMessage::swarm("Task · sheep", "Implement compaction asymptotic fixes");

    let lines = render_swarm_message(&msg, 80, crate::config::DiffDisplayMode::Off);
    let rendered: Vec<String> = lines.iter().map(extract_line_text).collect();

    assert_eq!(rendered, vec!["🐑 Implement compaction asymptotic fixes"]);
}

#[test]
fn test_render_swarm_message_centered_mode_left_aligns_with_shared_padding() {
    let saved = crate::tui::markdown::center_code_blocks();
    crate::tui::markdown::set_center_code_blocks(true);

    let msg = DisplayMessage::swarm("Plan · sheep", "4 items · v1");
    let lines = render_swarm_message(&msg, 80, crate::config::DiffDisplayMode::Off);
    let rendered: Vec<String> = lines.iter().map(extract_line_text).collect();

    assert_eq!(rendered.len(), 1, "expected one compact plan row");

    let header_pad = rendered[0].chars().take_while(|c| *c == ' ').count();
    assert!(
        header_pad > 0,
        "centered swarm header should be padded: {rendered:?}"
    );
    assert_eq!(rendered[0].trim_start(), "🐝 Plan · 4 items · v1");
    assert!(!rendered[0].contains('│'));
    for line in &lines {
        assert_eq!(
            line.alignment,
            Some(ratatui::layout::Alignment::Left),
            "centered swarm lines should be left-aligned after padding"
        );
    }

    crate::tui::markdown::set_center_code_blocks(saved);
}

#[test]
fn test_render_swarm_message_centered_mode_keeps_task_icon_and_padding() {
    let saved = crate::tui::markdown::center_code_blocks();
    crate::tui::markdown::set_center_code_blocks(true);

    let msg = DisplayMessage::swarm("Task · sheep", "Implement compaction asymptotic fixes");
    let lines = render_swarm_message(&msg, 80, crate::config::DiffDisplayMode::Off);
    let rendered: Vec<String> = lines.iter().map(extract_line_text).collect();

    assert!(
        rendered[0].starts_with(' '),
        "centered task header should be padded: {rendered:?}"
    );
    assert_eq!(
        rendered[0].trim_start(),
        "🐑 Implement compaction asymptotic fixes"
    );

    crate::tui::markdown::set_center_code_blocks(saved);
}

#[test]
fn test_render_swarm_message_centered_mode_keeps_file_activity_preview_centered_when_diff_wraps() {
    let saved = crate::tui::markdown::center_code_blocks();
    crate::tui::markdown::set_center_code_blocks(true);

    let msg = DisplayMessage::swarm(
        "File activity · rose",
        "`…/jcode/src/server/comm_sync.rs`

Modified via apply_patch

```text
331-             persist_swarm_state_for(&swarm_id, swarm_state.clone()).await;
331+             persist_swarm_state_for(&swarm_id, swarm_state).await;
```",
    );

    let lines = render_swarm_message(&msg, 120, crate::config::DiffDisplayMode::Off);
    let rendered: Vec<String> = lines.iter().map(extract_line_text).collect();
    let first_pad = rendered[0].chars().take_while(|c| *c == ' ').count();

    assert!(
        first_pad >= 8,
        "centered file activity notification should preserve a visible left gutter: {rendered:?}"
    );
    assert!(
        rendered
            .iter()
            .all(|line| line.is_empty() || line.starts_with(&" ".repeat(first_pad))),
        "wrapped file activity preview should keep one shared left pad: {rendered:?}"
    );
    assert!(
        rendered
            .iter()
            .any(|line| line.contains("persist_swarm_state_for")),
        "expected diff preview to remain visible after wrapping: {rendered:?}"
    );

    crate::tui::markdown::set_center_code_blocks(saved);
}

#[test]
fn test_truncate_line_to_width_uses_display_width() {
    let line = Line::from(Span::raw("🧠 hello world"));
    let truncated = truncate_line_to_width(&line, 8);
    let w = truncated.width();
    assert!(w <= 8, "truncated line display width {} should be <= 8", w);
}

/// End-to-end proof that `[display.colors]` reaches a real rendered frame.
///
/// The unit tests in `jcode-tui-style` cover the substitution in isolation, but
/// the thing a user actually cares about is whether configuring a color changes
/// what the TUI paints. This renders a real frame through `ui::draw`, so it also
/// guards the hook's presence.
#[test]
fn test_configured_palette_recolors_a_real_rendered_frame() {
    fn render() -> ratatui::buffer::Buffer {
        let messages = vec![
            DisplayMessage {
                role: "user".into(),
                content: "hello there".into(),
                tool_calls: vec![],
                duration_secs: None,
                title: None,
                tool_data: None,
            },
            DisplayMessage {
                role: "assistant".into(),
                content: "hi! *bold* and `code`".into(),
                tool_calls: vec![],
                duration_secs: None,
                title: None,
                tool_data: None,
            },
        ];
        let state = TestState {
            display_messages: messages,
            input: "next question".into(),
            ..Default::default()
        };
        let backend = ratatui::backend::TestBackend::new(80, 24);
        let mut terminal = ratatui::Terminal::new(backend).expect("test terminal");
        terminal
            .draw(|frame| crate::tui::ui::draw(frame, &state))
            .expect("draw");
        terminal.backend().buffer().clone()
    }

    // The palette is process-global, so always restore it, even on failure.
    struct Restore;
    impl Drop for Restore {
        fn drop(&mut self) {
            jcode_tui_style::set_palette(jcode_tui_style::Palette::default());
        }
    }
    let _restore = Restore;

    jcode_tui_style::set_palette(jcode_tui_style::Palette::default());
    let baseline = render();

    // Recolor the user role to a color nothing in the default palette is near.
    let mut palette = jcode_tui_style::Palette::default();
    palette.set(jcode_tui_style::Role::User, (250, 40, 200));
    jcode_tui_style::set_palette(palette);
    let configured = render();

    assert_ne!(
        baseline
            .content
            .iter()
            .map(|cell| cell.fg)
            .collect::<Vec<_>>(),
        configured
            .content
            .iter()
            .map(|cell| cell.fg)
            .collect::<Vec<_>>(),
        "configuring a color role should change the rendered frame"
    );

    // Text content must be untouched: this is a recolor, not a relayout.
    let text_of = |buffer: &ratatui::buffer::Buffer| {
        buffer
            .content
            .iter()
            .map(|cell| cell.symbol().to_string())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        text_of(&baseline),
        text_of(&configured),
        "recoloring must not change any rendered text"
    );

    // And the default palette must render identically to no palette at all,
    // which is what keeps existing users' terminals looking the same.
    jcode_tui_style::set_palette(jcode_tui_style::Palette::default());
    assert_eq!(
        baseline,
        render(),
        "the default palette must be a no-op on the rendered frame"
    );
}
