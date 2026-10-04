use super::*;
use crate::tui::color_support::rgb;

fn extract_line_text(line: &Line<'_>) -> String {
    line.spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect::<String>()
}

fn leading_spaces(text: &str) -> usize {
    text.chars().take_while(|c| *c == ' ').count()
}

fn system_glyph_env_lock() -> std::sync::MutexGuard<'static, ()> {
    use std::sync::{Mutex, OnceLock};

    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[test]
fn render_system_message_forces_system_color_on_all_spans() {
    let msg = DisplayMessage::system("**Reload complete** - continuing.");

    let lines = render_system_message(&msg, 80, crate::config::DiffDisplayMode::Off);

    assert!(!lines.is_empty(), "expected rendered system message lines");
    for line in lines {
        for span in line.spans {
            assert_eq!(span.style.fg, Some(system_message_color()));
        }
    }
}

#[test]
fn render_cold_cache_warning_is_always_one_width_bounded_line() {
    let saved = crate::tui::markdown::center_code_blocks();
    let msg = DisplayMessage::system(
        "🧊 Prompt cache went cold · next turn may resend ~96K tok · /cache extends",
    );

    for centered in [false, true] {
        crate::tui::markdown::set_center_code_blocks(centered);
        for width in [80_u16, 50, 30] {
            let lines = render_system_message(&msg, width, crate::config::DiffDisplayMode::Off);
            assert_eq!(
                lines.len(),
                1,
                "cold-cache notice wrapped at width {width} (centered={centered}): {lines:?}"
            );
            let text = extract_line_text(&lines[0]);
            assert!(
                !text.contains('\n'),
                "cold-cache notice contains a newline: {text:?}"
            );
            assert!(
                lines[0].width() <= width as usize,
                "cold-cache notice width {} exceeds {width}: {text:?}",
                lines[0].width()
            );
            assert!(
                text.contains("Prompt cache went cold"),
                "cold-cache identity was truncated away: {text:?}"
            );
            if width < 80 {
                assert!(
                    text.ends_with('…'),
                    "narrow cold-cache notice should end in an ellipsis: {text:?}"
                );
            }
            for span in &lines[0].spans {
                assert_eq!(span.style.fg, Some(system_message_color()));
            }
        }
    }

    crate::tui::markdown::set_center_code_blocks(saved);
}

#[test]
fn render_compact_divergence_notice_as_one_line() {
    let saved = crate::tui::markdown::center_code_blocks();
    let notices = [DisplayMessage::system(
        "Update diverged. Press Ctrl+Y to let a kcode agent merge local and upstream (or run `git pull` / `git rebase` yourself).",
    )
    .with_title("Update")];

    for centered in [false, true] {
        crate::tui::markdown::set_center_code_blocks(centered);
        for msg in &notices {
            for width in [80_u16, 50, 30] {
                let lines = render_system_message(msg, width, crate::config::DiffDisplayMode::Off);
                assert_eq!(
                    lines.len(),
                    1,
                    "compact notice wrapped at width {width} (centered={centered}): {lines:?}"
                );
                let text = extract_line_text(&lines[0]);
                assert!(!text.contains('\n'), "notice contains a newline: {text:?}");
                assert!(
                    lines[0].width() <= width as usize,
                    "notice width {} exceeds {width}: {text:?}",
                    lines[0].width()
                );
                if width < 80 {
                    assert!(
                        text.ends_with('…'),
                        "narrow compact notice should end in an ellipsis: {text:?}"
                    );
                }
            }
        }
    }

    crate::tui::markdown::set_center_code_blocks(saved);
}

#[test]
fn render_system_message_renders_markdown_formatting() {
    let msg = DisplayMessage::system(
        "**bold** and `code` and # heading\n- bullet item\n[link](http://example.com)",
    );

    let lines = render_system_message(&msg, 80, crate::config::DiffDisplayMode::Off);
    let plain = lines
        .iter()
        .map(extract_line_text)
        .collect::<Vec<_>>()
        .join("\n");

    // System messages now render markdown: the inline markers are consumed and
    // the underlying text survives. Bold/code markers should no longer appear
    // literally, while the text content and a bullet glyph remain.
    assert!(plain.contains("bold"), "keeps bold text: {plain:?}");
    assert!(
        !plain.contains("**bold**"),
        "strips bold markers: {plain:?}"
    );
    assert!(plain.contains("code"), "keeps code text: {plain:?}");
    assert!(plain.contains("heading"), "keeps heading text: {plain:?}");
    assert!(
        plain.contains("bullet item"),
        "keeps bullet text: {plain:?}"
    );
    // The link text renders without the raw markdown link syntax.
    assert!(plain.contains("link"), "keeps link text: {plain:?}");
    assert!(
        !plain.contains("[link](http://example.com)"),
        "strips raw link syntax: {plain:?}"
    );

    // Color is still forced to the system color over every span.
    for line in &lines {
        for span in &line.spans {
            assert_eq!(span.style.fg, Some(system_message_color()));
        }
    }
}

#[test]
fn render_system_message_preserves_indentation_and_newlines() {
    let msg = DisplayMessage::system("Header line\n  indented detail\n\nNext block");

    let lines = render_system_message(&msg, 80, crate::config::DiffDisplayMode::Off);
    let rendered = lines.iter().map(extract_line_text).collect::<Vec<_>>();

    // Centered mode may add uniform left padding; compare relative structure.
    assert_eq!(rendered.len(), 4, "got: {rendered:?}");
    assert!(
        rendered[0].trim_end().ends_with("Header line"),
        "got: {rendered:?}"
    );
    assert!(
        rendered[1].trim_end().ends_with("indented detail"),
        "got: {rendered:?}"
    );
    assert!(
        rendered[2].trim().is_empty(),
        "blank line preserved, got: {rendered:?}"
    );
    assert!(
        rendered[3].trim_end().ends_with("Next block"),
        "got: {rendered:?}"
    );

    // The detail line keeps exactly two more leading spaces than the header.
    assert_eq!(
        leading_spaces(&rendered[1]),
        leading_spaces(&rendered[0]) + 2,
        "indentation should be preserved, got: {rendered:?}"
    );
}

#[test]
fn render_plaintext_lines_hang_indents_wrapped_continuations() {
    // An indented line longer than the wrap width keeps its indent on the wrap.
    let lines = render_plaintext_lines("  alpha beta gamma delta", 12);
    let rendered = lines.iter().map(extract_line_text).collect::<Vec<_>>();

    assert!(rendered.len() >= 2, "expected wrapping, got: {rendered:?}");
    for line in &rendered {
        assert!(
            line.is_empty() || line.starts_with("  "),
            "continuation lines should keep indent, got: {rendered:?}"
        );
        assert!(line.width() <= 12, "line too wide: {line:?}");
    }
}

#[test]
fn render_system_message_centered_mode_left_aligns_with_padding() {
    let saved = crate::tui::markdown::center_code_blocks();
    crate::tui::markdown::set_center_code_blocks(true);
    let msg = DisplayMessage::system("Reload complete - continuing.");

    let lines = render_system_message(&msg, 80, crate::config::DiffDisplayMode::Off);

    assert!(!lines.is_empty(), "expected rendered system message lines");
    for line in &lines {
        assert_eq!(
            line.alignment,
            Some(ratatui::layout::Alignment::Left),
            "centered system lines should be left-aligned with padding"
        );
        assert!(
            line.spans
                .first()
                .is_some_and(|span| span.content.starts_with(' ')),
            "centered system lines should start with padding"
        );
    }
    crate::tui::markdown::set_center_code_blocks(saved);
}

#[test]
fn render_system_message_uses_width_stable_titles_on_kitty() {
    let _guard = system_glyph_env_lock();
    let prev_term_program = std::env::var("TERM_PROGRAM").ok();
    let prev_term = std::env::var("TERM").ok();
    crate::env::set_var("TERM_PROGRAM", "kitty");
    crate::env::set_var("TERM", "xterm-kitty");

    let msg = DisplayMessage::system(
        "⚡ Connection lost - retrying (attempt 2, 7s) - connection reset by server",
    )
    .with_title("Connection");

    let lines = render_system_message(&msg, 80, crate::config::DiffDisplayMode::Off);
    let plain = lines
        .iter()
        .map(extract_line_text)
        .collect::<Vec<_>>()
        .join("\n");

    assert!(plain.contains("reconnecting"));
    assert!(!plain.contains("⚡ reconnecting"));

    match prev_term_program {
        Some(value) => crate::env::set_var("TERM_PROGRAM", value),
        None => crate::env::remove_var("TERM_PROGRAM"),
    }
    match prev_term {
        Some(value) => crate::env::set_var("TERM", value),
        None => crate::env::remove_var("TERM"),
    }
}

#[test]
fn render_background_task_message_uses_box_and_truncates_preview_lines() {
    let msg = DisplayMessage::background_task(
        "**Background task** `bg123` · `bash` · ✓ completed · 7.1s · exit 0\n\n```text\nline 1\nline 2\nline 3\nline 4\nline 5\n```\n\n_Full output:_ `bg action=\"output\" task_id=\"bg123\"`",
    );

    let lines = render_background_task_message(&msg, 80, crate::config::DiffDisplayMode::Off);
    let plain = lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");

    assert!(plain.contains("✓ bg bash completed · bg123"));
    assert!(plain.contains("exit 0 · 7.1s"));
    assert!(plain.contains("line 1"));
    assert!(plain.contains("… +1 more line"));
    assert!(!plain.contains("task bg123 · bash"));
    assert!(!plain.contains("Preview"));
    assert!(!plain.contains("Full output"));
    assert!(!plain.contains("bg action=\"output\" task_id=\"bg123\""));
}

#[test]
fn render_background_task_message_strips_ansi_from_existing_preview() {
    let msg = DisplayMessage::background_task(
        "**Background task** `bg123` · `bash` · ✓ completed · 0.1s · exit 0\n\n```text\n\u{1b}[32m✓\u{1b}[39m passes \u{1b}[2m12ms\u{1b}[22m\n```\n\n_Full output:_ `bg action=\"output\" task_id=\"bg123\"`",
    );

    let plain = render_background_task_message(&msg, 80, crate::config::DiffDisplayMode::Off)
        .iter()
        .map(extract_line_text)
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        plain.contains("✓ passes 12ms"),
        "rendered preview:\n{plain}"
    );
    assert!(!plain.contains('\u{1b}'));
    assert!(!plain.contains("[32m"));
    assert!(!plain.contains("[2m"));
}

#[test]
fn render_system_message_strips_ansi_from_existing_inline_command_preview() {
    let msg = DisplayMessage::system(
        "Shell command · ✓ exit 0 · 12ms\n\n  cargo test\n\n  \u{1b}[32m✓\u{1b}[39m passes \u{1b}[2m12ms\u{1b}[22m",
    );

    let plain = render_system_message(&msg, 80, crate::config::DiffDisplayMode::Off)
        .iter()
        .map(extract_line_text)
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        plain.contains("✓ passes 12ms"),
        "rendered preview:\n{plain}"
    );
    assert!(!plain.contains('\u{1b}'));
    assert!(!plain.contains("[32m"));
    assert!(!plain.contains("[2m"));
}

#[test]
fn render_background_task_message_uses_swarm_flavor_for_swarm_tool() {
    crate::tui::markdown::set_center_code_blocks(false);
    let msg = DisplayMessage::background_task(
        "**Background task** `bg777` · `run_plan (6 nodes, deep mode)` (`swarm`) · ✓ completed · 92.4s · exit 0\n\n```text\nSwarm plan reached terminal/blocked state after 9 loop(s). completed=6 blocked=0 cycles=0 active=0 assignments=8\n```\n\n_Full output:_ `bg action=\"output\" task_id=\"bg777\"`",
    );

    let lines = render_background_task_message(&msg, 100, crate::config::DiffDisplayMode::Off);
    let plain = lines
        .iter()
        .map(extract_line_text)
        .collect::<Vec<_>>()
        .join("\n");

    assert_eq!(plain, "🐝 ✓ run plan · 92.4s");
    assert!(!plain.contains("bg777"));
    assert!(!plain.contains("Swarm plan reached terminal/blocked state"));
}

#[test]
fn render_background_task_progress_message_uses_swarm_flavor_for_swarm_tool() {
    crate::tui::markdown::set_center_code_blocks(false);
    let msg = DisplayMessage::background_task(
        "**Background task progress** `bg777` · `run_plan (6 nodes, deep mode)` (`swarm`)\n\n[####--------] 33% · 2/6 nodes · completed 2 · blocked 0 · active 3 · assignments 5 (reported)",
    );

    let lines = render_background_task_message(&msg, 100, crate::config::DiffDisplayMode::Off);
    let plain = lines
        .iter()
        .map(extract_line_text)
        .collect::<Vec<_>>()
        .join("\n");

    assert_eq!(plain, "🐝 ● run plan · 2/6");
    assert!(!plain.contains("bg777"));
}

#[test]
fn render_background_task_progress_message_uses_box_with_progress_bar() {
    let msg = DisplayMessage::background_task(
        "**Background task progress** `bg123` · `bash`\n\n[#####-------] 42% · Running tests (reported)",
    );

    let lines = render_background_task_message(&msg, 80, crate::config::DiffDisplayMode::Off);
    let plain = lines
        .iter()
        .map(extract_line_text)
        .collect::<Vec<_>>()
        .join("\n");

    assert!(plain.contains("◌ bg bash · bg123"));
    assert!(plain.contains("█"));
    assert!(plain.contains("░"));
    assert!(plain.contains("42%"));
    assert!(plain.contains("Running tests"));
    assert!(plain.contains("Latest status: bg action=\"status\" task_id=\"bg123\""));
    assert_eq!(
        plain.matches('│').count(),
        4,
        "expected compact progress row plus status hint:\n{plain}"
    );
    assert!(!plain.contains("Latest update"));
    assert!(!plain.contains("Source: reported"));
    assert!(!plain.contains("**Background task progress**"));
}


#[test]
fn render_background_task_messages_prefer_display_name() {
    let completion = DisplayMessage::background_task(
        "**Background task** `bg123` · `Run integration tests` (`bash`) · ✓ completed · 7.1s · exit 0\n\n_No output captured._\n\n_Full output:_ `bg action=\"output\" task_id=\"bg123\"`",
    );
    let completion_plain =
        render_background_task_message(&completion, 100, crate::config::DiffDisplayMode::Off)
            .iter()
            .map(extract_line_text)
            .collect::<Vec<_>>()
            .join("\n");
    assert!(completion_plain.contains("✓ bg Run integration tests completed · bg123"));

    let progress = DisplayMessage::background_task(
        "**Background task progress** `bg123` · `Run integration tests` (`bash`)\n\n[#####-------] 42% · Running tests (reported)",
    );
    let progress_plain =
        render_background_task_message(&progress, 100, crate::config::DiffDisplayMode::Off)
            .iter()
            .map(extract_line_text)
            .collect::<Vec<_>>()
            .join("\n");
    assert!(progress_plain.contains("◌ bg Run integration tests · bg123"));
}

#[test]
fn render_system_message_uses_scheduled_task_card() {
    let msg = DisplayMessage::system(
        "[Scheduled task]\nA scheduled task for this session is now due.\n\nTask: Follow up on the scheduler test\nWorking directory: /home/jeremy/kcode\nRelevant files: src/tui/ui_messages.rs\nBranch: master\n\nBackground: Verify the scheduled task card styling\nSuccess criteria: The due task renders clearly\nScheduled by session: session_test",
    );

    let lines = render_system_message(&msg, 100, crate::config::DiffDisplayMode::Off);
    let plain = lines
        .iter()
        .map(extract_line_text)
        .collect::<Vec<_>>()
        .join("\n");

    assert!(plain.contains(width_stable_system_title(
        "⏰ scheduled task due",
        "scheduled task due"
    )));
    assert!(plain.contains("This scheduled task is now active in this session."));
    assert!(plain.contains("Follow up on the scheduler test"));
    assert!(plain.contains("Verify the scheduled task card styling"));
    assert!(!plain.contains("[Scheduled task]"));
    assert!(!plain.contains("A scheduled task for this session is now due."));
}

#[test]
fn render_tool_message_uses_scheduled_card() {
    let msg = DisplayMessage {
        role: "tool".to_string(),
        content: "Scheduled task 'Follow up on the scheduler test' for in 1m (id: sched_abc123)\nWorking directory: /home/jeremy/kcode\nRelevant files: src/tui/ui_messages.rs\nTarget: resume session session_test".to_string(),
        tool_calls: Vec::new(),
        duration_secs: None,
        title: Some("scheduled: Follow up on the scheduler test".to_string()),
        tool_data: Some(crate::message::ToolCall {
            id: "call_schedule_card".to_string(),
            name: "schedule".to_string(),
            input: serde_json::json!({
                "task": "Follow up on the scheduler test",
                "wake_in_minutes": 1,
                "target": "resume"
            }),
            intent: None, thought_signature: None, }),
    };

    let lines = render_tool_message(&msg, 100, crate::config::DiffDisplayMode::Off);
    let plain = lines
        .iter()
        .map(extract_line_text)
        .collect::<Vec<_>>()
        .join("\n");

    assert!(plain.contains(width_stable_system_title("⏰ scheduled", "scheduled")));
    assert!(plain.contains("Will run in 1m."));
    assert!(plain.contains("Follow up on the scheduler test"));
    assert!(plain.contains("session session_test"));
    assert!(plain.contains("sched_abc123"));
    assert!(!plain.contains("✓ schedule"));
}

#[test]
fn render_assistant_message_renders_plan_block_as_card() {
    let saved = crate::tui::markdown::center_code_blocks();
    crate::tui::markdown::set_center_code_blocks(false);
    let msg = DisplayMessage::assistant(
        "Here is the plan:\n\n```plan\n# Ship compact mode\n\n## Goal\nAdd a compact message mode.\n\n## Approach\n1. Add config flag\n2. Wire renderer\n```\n\nLet me know if this works.",
    );

    let lines = render_assistant_message(&msg, 100, crate::config::DiffDisplayMode::Off);
    crate::tui::markdown::set_center_code_blocks(saved);
    let plain = lines
        .iter()
        .map(extract_line_text)
        .collect::<Vec<_>>()
        .join("\n");

    assert!(plain.contains("Here is the plan:"), "plain: {plain}");
    assert!(plain.contains("⛭ Ship compact mode"), "plain: {plain}");
    assert!(plain.contains('╭'), "expected card border: {plain}");
    assert!(plain.contains('╰'), "expected card border: {plain}");
    assert!(plain.contains("Add a compact message mode."));
    assert!(plain.contains("Let me know if this works."));
    assert!(
        !plain.contains("```"),
        "plan fence markers should not render: {plain}"
    );
}

#[test]
fn render_assistant_message_plan_card_survives_unterminated_fence() {
    let saved = crate::tui::markdown::center_code_blocks();
    crate::tui::markdown::set_center_code_blocks(false);
    let msg = DisplayMessage::assistant("```plan\n# Streaming plan\n\n- step one");

    let lines = render_assistant_message(&msg, 80, crate::config::DiffDisplayMode::Off);
    crate::tui::markdown::set_center_code_blocks(saved);
    let plain = lines
        .iter()
        .map(extract_line_text)
        .collect::<Vec<_>>()
        .join("\n");

    assert!(plain.contains("⛭ Streaming plan"), "plain: {plain}");
    assert!(plain.contains("step one"), "plain: {plain}");
}

#[test]
fn render_assistant_message_plan_card_keeps_nested_fences_inside() {
    let saved = crate::tui::markdown::center_code_blocks();
    crate::tui::markdown::set_center_code_blocks(false);
    let msg = DisplayMessage::assistant(
        "```plan\n# Validation plan\n\n```bash\ncargo test -p kcode-tui\n```\n\nAfter the block.\n```\n\nOutside text.",
    );

    let lines = render_assistant_message(&msg, 100, crate::config::DiffDisplayMode::Off);
    crate::tui::markdown::set_center_code_blocks(saved);
    let plain = lines
        .iter()
        .map(extract_line_text)
        .collect::<Vec<_>>()
        .join("\n");

    assert!(plain.contains("⛭ Validation plan"), "plain: {plain}");
    assert!(plain.contains("cargo test -p kcode-tui"), "plain: {plain}");
    assert!(plain.contains("After the block."), "plain: {plain}");
    assert!(plain.contains("Outside text."), "plain: {plain}");
    // The nested bash content stays inside the card borders.
    let bash_line = lines
        .iter()
        .map(extract_line_text)
        .find(|line| line.contains("cargo test -p kcode-tui"))
        .expect("missing bash line");
    assert!(
        bash_line.trim_start().starts_with('│'),
        "nested fence content should be inside the card: {bash_line}"
    );
}

#[test]
fn split_plan_segments_returns_none_without_plan_block() {
    assert!(split_plan_segments("Just some text\n\n```rust\nfn main() {}\n```").is_none());
    assert!(split_plan_segments("mentions plan but no fence").is_none());
}

#[test]
fn render_assistant_message_truncates_tool_calls_to_single_line() {
    let saved = crate::tui::markdown::center_code_blocks();
    crate::tui::markdown::set_center_code_blocks(false);
    let msg = DisplayMessage {
        role: "assistant".to_string(),
        content: "Done.".to_string(),
        tool_calls: vec![
            "read".to_string(),
            "grep".to_string(),
            "apply_patch".to_string(),
            "batch".to_string(),
        ],
        duration_secs: None,
        title: None,
        tool_data: None,
    };

    let lines = render_assistant_message(&msg, 20, crate::config::DiffDisplayMode::Off);
    assert_eq!(extract_line_text(&lines[1]), "");
    let tool_lines: Vec<String> = lines
        .iter()
        .skip(2)
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect()
        })
        .collect();

    assert!(
        tool_lines.len() == 1,
        "expected single-line tool-call summary: {tool_lines:?}"
    );
    assert!(
        tool_lines[0].contains("tools:"),
        "expected tool summary label on first line: {tool_lines:?}"
    );
    assert!(
        tool_lines.iter().all(|line| line.width() <= 20),
        "tool-call summary line should respect available width: {tool_lines:?}"
    );
    crate::tui::markdown::set_center_code_blocks(saved);
}

#[test]
fn render_assistant_message_centers_single_line_tool_summary() {
    let saved = crate::tui::markdown::center_code_blocks();
    crate::tui::markdown::set_center_code_blocks(true);
    let msg = DisplayMessage {
        role: "assistant".to_string(),
        content: "Done.".to_string(),
        tool_calls: vec![
            "read".to_string(),
            "grep".to_string(),
            "apply_patch".to_string(),
            "batch".to_string(),
        ],
        duration_secs: None,
        title: None,
        tool_data: None,
    };

    let lines = render_assistant_message(&msg, 28, crate::config::DiffDisplayMode::Off);
    assert_eq!(extract_line_text(&lines[1]), "");
    let tool_lines: Vec<String> = lines
        .iter()
        .skip(2)
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect()
        })
        .collect();

    assert!(
        tool_lines.len() == 1,
        "expected single-line tool-call summary: {tool_lines:?}"
    );
    let first_pad = tool_lines[0].chars().take_while(|c| *c == ' ').count();
    assert!(
        first_pad > 0,
        "tool summary should still be padded/centered as a block: {tool_lines:?}"
    );
    assert!(
        lines
            .iter()
            .skip(2)
            .all(|line| line.alignment == Some(ratatui::layout::Alignment::Left)),
        "centered tool summary should use a shared left-aligned block pad"
    );

    crate::tui::markdown::set_center_code_blocks(saved);
}

#[test]
fn render_assistant_message_without_body_does_not_add_extra_blank_line_before_tool_summary() {
    let saved = crate::tui::markdown::center_code_blocks();
    crate::tui::markdown::set_center_code_blocks(false);
    let msg = DisplayMessage {
        role: "assistant".to_string(),
        content: String::new(),
        tool_calls: vec!["read".to_string()],
        duration_secs: None,
        title: None,
        tool_data: None,
    };

    let lines = render_assistant_message(&msg, 28, crate::config::DiffDisplayMode::Off);
    let rendered: Vec<String> = lines.iter().map(extract_line_text).collect();

    assert_eq!(rendered.len(), 1, "rendered={rendered:?}");
    assert!(rendered[0].contains("tool:"), "rendered={rendered:?}");

    crate::tui::markdown::set_center_code_blocks(saved);
}

#[test]
fn render_assistant_message_centered_mode_keeps_markdown_unpadded_for_center_alignment() {
    let saved = crate::tui::markdown::center_code_blocks();
    crate::tui::markdown::set_center_code_blocks(true);
    let msg = DisplayMessage::assistant(
        "streaming-block streaming-block streaming-block streaming-block",
    );

    let lines = render_assistant_message(&msg, 120, crate::config::DiffDisplayMode::Off);
    let content_line = lines
        .iter()
        .find(|line| extract_line_text(line).contains("streaming-block"))
        .expect("expected assistant markdown line");

    let first_pad = extract_line_text(content_line)
        .chars()
        .take_while(|c| *c == ' ')
        .count();
    assert_eq!(
        first_pad, 0,
        "centered assistant markdown should not inject left padding: {lines:?}"
    );
    assert_eq!(
        content_line.alignment, None,
        "assistant render should leave centered prose alignment unset for outer centering"
    );

    crate::tui::markdown::set_center_code_blocks(saved);
}

#[test]
fn render_assistant_message_recenters_structured_markdown_to_actual_width() {
    let saved = crate::tui::markdown::center_code_blocks();
    crate::tui::markdown::set_center_code_blocks(true);
    let msg = DisplayMessage::assistant("- one\n- two");

    let lines = render_assistant_message(&msg, 140, crate::config::DiffDisplayMode::Off);
    let rendered: Vec<String> = lines.iter().map(extract_line_text).collect();
    let bullets: Vec<&String> = rendered.iter().filter(|line| line.contains("• ")).collect();

    assert_eq!(
        bullets.len(),
        2,
        "expected two rendered bullet lines: {rendered:?}"
    );
    let first_pad = leading_spaces(bullets[0]);
    let second_pad = leading_spaces(bullets[1]);
    assert_eq!(
        first_pad, second_pad,
        "simple list should share a block pad: {rendered:?}"
    );
    assert!(
        first_pad > 45,
        "list should be re-centered to the full display width: {rendered:?}"
    );
    assert!(
        bullets
            .iter()
            .all(|line| line[leading_spaces(line)..].starts_with("• ")),
        "bullet markers should remain flush-left within the centered block: {rendered:?}"
    );

    crate::tui::markdown::set_center_code_blocks(saved);
}

#[test]
fn render_system_message_centered_mode_caps_wrap_width_for_visible_gutters() {
    let saved = crate::tui::markdown::center_code_blocks();
    crate::tui::markdown::set_center_code_blocks(true);
    let msg = DisplayMessage::system(
        "This is a long centered-mode system notification that should keep visible side gutters instead of stretching nearly edge to edge in a wide terminal.",
    );

    let lines = render_system_message(&msg, 120, crate::config::DiffDisplayMode::Off);
    let rendered: Vec<String> = lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect()
        })
        .collect();

    assert!(
        rendered.iter().all(|line| line.starts_with("          ")),
        "centered system message should retain visible left padding in wide layouts: {rendered:?}"
    );

    crate::tui::markdown::set_center_code_blocks(saved);
}

#[test]
fn render_system_message_uses_minimal_inline_style_for_reload_title() {
    let msg = DisplayMessage::system("Reloading server with newer binary...").with_title("Reload");

    let lines = render_system_message(&msg, 80, crate::config::DiffDisplayMode::Off);
    let plain = lines
        .iter()
        .map(extract_line_text)
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        !plain.contains('╭'),
        "unexpected reload card border: {plain}"
    );
    assert!(
        !plain.contains('╰'),
        "unexpected reload card border: {plain}"
    );
    assert!(
        !plain.contains("⚡ reload"),
        "unexpected reload card title: {plain}"
    );
    assert!(plain.contains("Reloading server with newer binary"));
}

#[test]
fn render_system_message_uses_connection_card_for_reconnect_status() {
    let msg = DisplayMessage::system(
        "⚡ Connection lost - retrying (attempt 2, 7s) - connection reset by server · resume: kcode --resume koala",
    )
    .with_title("Connection");

    let lines = render_system_message(&msg, 80, crate::config::DiffDisplayMode::Off);
    let plain = lines
        .iter()
        .map(extract_line_text)
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        plain.contains("reconnecting"),
        "expected reconnect card title: {plain}"
    );
    assert!(plain.contains("Retrying · attempt 2 · 7s"));
    assert!(plain.contains("connection reset by server"));
    assert!(plain.contains("kcode --resume koala"));
}

#[test]
fn render_swarm_message_centered_mode_caps_wrap_width_for_long_notifications() {
    let saved = crate::tui::markdown::center_code_blocks();
    crate::tui::markdown::set_center_code_blocks(true);
    let msg = DisplayMessage::swarm(
        "File activity",
        "/home/jeremy/kcode/src/tui/ui_messages.rs - moss just edited this file while you were working nearby, so the notification should still read as centered in wide layouts.",
    );

    let lines = render_swarm_message(&msg, 120, crate::config::DiffDisplayMode::Off);
    let rendered: Vec<String> = lines.iter().map(extract_line_text).collect();
    let first_pad = rendered[0].chars().take_while(|c| *c == ' ').count();

    assert!(
        first_pad >= 8,
        "centered swarm notification should keep a clearly visible left gutter: {rendered:?}"
    );
    assert!(
        rendered
            .iter()
            .all(|line| line.is_empty() || line.starts_with(&" ".repeat(first_pad))),
        "centered swarm notification should share one left pad across wrapped lines: {rendered:?}"
    );

    crate::tui::markdown::set_center_code_blocks(saved);
}

#[test]
fn render_swarm_message_collapsed_shows_tldr_and_expand_badge_only() {
    let saved = crate::tui::markdown::center_code_blocks();
    crate::tui::markdown::set_center_code_blocks(false);
    let content = kcode_tui_messages::encode_collapsible_swarm_content(
        "fixed the flaky test",
        "The flaky test was caused by a race in the setup helper.\n\nI rewrote it to use a barrier.",
    );
    let msg = DisplayMessage::swarm("DM from sheep", content);

    let lines = render_swarm_message(&msg, 100, crate::config::DiffDisplayMode::Off);
    let plain = lines
        .iter()
        .map(extract_line_text)
        .collect::<Vec<_>>()
        .join("\n");

    assert!(plain.contains("fixed the flaky test"), "{plain}");
    assert!(plain.contains(super::SWARM_EXPAND_BADGE), "{plain}");
    assert!(
        !plain.contains("race in the setup helper"),
        "collapsed card must hide the full body: {plain}"
    );
    crate::tui::markdown::set_center_code_blocks(saved);
}

#[test]
fn render_swarm_message_expanded_shows_body_and_collapse_badge() {
    let saved = crate::tui::markdown::center_code_blocks();
    crate::tui::markdown::set_center_code_blocks(false);
    let collapsed = kcode_tui_messages::encode_collapsible_swarm_content(
        "fixed the flaky test",
        "The flaky test was caused by a race in the setup helper.",
    );
    let expanded =
        kcode_tui_messages::toggle_collapsible_swarm_content(&collapsed).expect("toggle");
    let msg = DisplayMessage::swarm("DM from sheep", expanded);

    let lines = render_swarm_message(&msg, 100, crate::config::DiffDisplayMode::Off);
    let plain = lines
        .iter()
        .map(extract_line_text)
        .collect::<Vec<_>>()
        .join("\n");

    assert!(plain.contains("fixed the flaky test"), "{plain}");
    assert!(plain.contains(super::SWARM_COLLAPSE_BADGE), "{plain}");
    assert!(plain.contains("race in the setup helper"), "{plain}");
    crate::tui::markdown::set_center_code_blocks(saved);
}

#[test]
fn render_tool_message_prefers_subagent_title_with_model() {
    let msg = DisplayMessage {
        role: "tool".to_string(),
        content: "done".to_string(),
        tool_calls: Vec::new(),
        duration_secs: None,
        title: Some("Verify subagent model (general · gpt-5.4)".to_string()),
        tool_data: Some(crate::message::ToolCall {
            id: "call_1".to_string(),
            name: "subagent".to_string(),
            input: serde_json::json!({
                "description": "Verify subagent model",
                "subagent_type": "general"
            }),
            intent: None,
            thought_signature: None,
        }),
    };

    let lines = render_tool_message(&msg, 80, crate::config::DiffDisplayMode::Off);
    let rendered: String = lines[0]
        .spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect();

    assert!(rendered.contains("subagent Verify subagent model (general · gpt-5.4)"));
}

#[test]
fn render_tool_message_shows_intent_and_technical_preview_on_one_line() {
    crate::tui::ui::tools_ui::tests_tool_call_details_override::set(true);
    let msg = DisplayMessage {
        role: "tool".to_string(),
        content: "ok".to_string(),
        tool_calls: Vec::new(),
        duration_secs: None,
        title: None,
        tool_data: Some(crate::message::ToolCall {
            id: "call_intent".to_string(),
            name: "bash".to_string(),
            input: serde_json::json!({
                "command": "cargo test -p kcode render_background_task --lib",
                "intent": "Verify compact progress card"
            }),
            intent: Some("Verify compact progress card".to_string()),
            thought_signature: None,
        }),
    };

    let lines = render_tool_message(&msg, 120, crate::config::DiffDisplayMode::Off);
    let rendered = extract_line_text(&lines[0]);

    assert!(rendered.contains("bash · Verify compact progress card · $ cargo test"));
    assert_eq!(lines.len(), 1, "Bash output is hidden by default");
    crate::tui::ui::tools_ui::tests_tool_call_details_override::set(false);
}

/// Default (tool_call_details off): a row with an intent renders only the
/// intent; the dimmed technical preview is dropped and no fallback command
/// line is added.
#[test]
fn render_tool_message_hides_technical_preview_by_default() {
    let msg = DisplayMessage {
        role: "tool".to_string(),
        content: "ok".to_string(),
        tool_calls: Vec::new(),
        duration_secs: None,
        title: None,
        tool_data: Some(crate::message::ToolCall {
            id: "call_intent".to_string(),
            name: "bash".to_string(),
            input: serde_json::json!({
                "command": "cargo test -p kcode render_background_task --lib",
                "intent": "Verify compact progress card"
            }),
            intent: Some("Verify compact progress card".to_string()),
            thought_signature: None,
        }),
    };

    let lines = render_tool_message(&msg, 120, crate::config::DiffDisplayMode::Off);
    let rendered = extract_line_text(&lines[0]);

    assert!(
        rendered.contains("bash · Verify compact progress card"),
        "rendered={rendered}"
    );
    assert!(
        !rendered.contains("cargo test"),
        "technical detail should be hidden by default: {rendered}"
    );
    assert_eq!(lines.len(), 1, "Bash output is hidden by default");
}

/// Even with details off, a failed tool row keeps its error summary so
/// failures stay diagnosable.
#[test]
fn render_tool_message_keeps_error_summary_when_details_hidden() {
    let msg = DisplayMessage {
        role: "tool".to_string(),
        content: "Error: command not found: cargoo".to_string(),
        tool_calls: Vec::new(),
        duration_secs: None,
        title: None,
        tool_data: Some(crate::message::ToolCall {
            id: "call_intent_err".to_string(),
            name: "bash".to_string(),
            input: serde_json::json!({
                "command": "cargoo test",
                "intent": "Run the test suite"
            }),
            intent: Some("Run the test suite".to_string()),
            thought_signature: None,
        }),
    };

    let lines = render_tool_message(&msg, 120, crate::config::DiffDisplayMode::Off);
    let rendered = extract_line_text(&lines[0]);

    assert!(
        rendered.contains("Run the test suite ·"),
        "error summary should still render after the intent: {rendered}"
    );
}

#[test]
fn render_tool_message_shows_token_badge() {
    let msg = DisplayMessage {
        role: "tool".to_string(),
        content: "x".repeat(7_600),
        tool_calls: Vec::new(),
        duration_secs: None,
        title: None,
        tool_data: Some(crate::message::ToolCall {
            id: "call_2".to_string(),
            name: "read".to_string(),
            input: serde_json::json!({"file_path": "src/main.rs"}),
            intent: None,
            thought_signature: None,
        }),
    };

    let lines = render_tool_message(&msg, 120, crate::config::DiffDisplayMode::Off);
    let badge_span = lines[0]
        .spans
        .iter()
        .find(|span| span.content.contains("1.9k tok"))
        .expect("missing token badge");

    assert_eq!(badge_span.style.fg, Some(rgb(120, 120, 120)));
}

#[test]
fn render_tool_message_hides_bash_output() {
    let msg = DisplayMessage {
        role: "tool".to_string(),
        content: "<class 'zip'>\n[('p', 'b'), ('a', 'a'), ('l', 'l'), ('e', 'e')]".to_string(),
        tool_calls: Vec::new(),
        duration_secs: None,
        title: None,
        tool_data: Some(crate::message::ToolCall {
            id: "call_bash_output".to_string(),
            name: "bash".to_string(),
            input: serde_json::json!({
                "command": "python3 -c \"s='pale'; t='bale'; print(type(zip(s,t))); print(list(zip(s,t)))\""
            }),
            intent: None,
            thought_signature: None,
        }),
    };

    let lines = render_tool_message(&msg, 120, crate::config::DiffDisplayMode::Off);
    let rendered = lines.iter().map(extract_line_text).collect::<Vec<_>>();

    assert!(!rendered.iter().any(|line| line.contains("<class 'zip'>")));
    assert!(!rendered.iter().any(|line| line.contains("[('p', 'b')")));
}

#[test]
fn render_tool_message_shows_bash_output_when_enabled() {
    crate::tui::ui::tools_ui::tests_show_bash_output_override::set(true);
    let msg = DisplayMessage {
        role: "tool".to_string(),
        content: "one\ntwo\nthree\nfour".to_string(),
        tool_calls: Vec::new(),
        duration_secs: None,
        title: None,
        tool_data: Some(crate::message::ToolCall {
            id: "call_bash_output_enabled".to_string(),
            name: "bash".to_string(),
            input: serde_json::json!({"command": "printf output"}),
            intent: Some("Print output".to_string()),
            thought_signature: None,
        }),
    };

    let rendered = render_tool_message(&msg, 120, crate::config::DiffDisplayMode::Off)
        .iter()
        .map(extract_line_text)
        .collect::<Vec<_>>();

    assert_eq!(rendered.len(), 4);
    assert!(!rendered.iter().any(|line| line.trim() == "one"));
    assert!(rendered.iter().any(|line| line.trim() == "two"));
    assert!(rendered.iter().any(|line| line.trim() == "four"));
    crate::tui::ui::tools_ui::tests_show_bash_output_override::set(false);
}

#[test]
fn render_batch_tool_message_shows_flat_and_nested_subcall_intents() {
    crate::tui::ui::tools_ui::tests_tool_call_details_override::set(true);
    let msg = DisplayMessage {
        role: "tool".to_string(),
        content: "--- [1] read ---\nflat output\n\n--- [2] read ---\nnested output\n\nCompleted: 2 succeeded, 0 failed".to_string(),
        tool_calls: Vec::new(),
        duration_secs: None,
        title: None,
        tool_data: Some(crate::message::ToolCall {
            id: "call_batch_intents".to_string(),
            name: "batch".to_string(),
            input: serde_json::json!({
                "tool_calls": [
                    {
                        "tool": "read",
                        "intent": "Inspect flat batch input",
                        "file_path": "src/flat.rs"
                    },
                    {
                        "tool": "read",
                        "parameters": {
                            "intent": "Inspect nested batch input",
                            "file_path": "src/nested.rs"
                        }
                    }
                ]
            }),
            intent: None,
            thought_signature: None,
        }),
    };

    let plain = render_tool_message(&msg, 120, crate::config::DiffDisplayMode::Off)
        .iter()
        .map(extract_line_text)
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        plain.contains("read · Inspect flat batch input ·"),
        "{plain}"
    );
    assert!(
        plain.contains("read · Inspect nested batch input ·"),
        "{plain}"
    );
    assert!(plain.contains("flat.rs"), "{plain}");
    assert!(plain.contains("nested.rs"), "{plain}");
    crate::tui::ui::tools_ui::tests_tool_call_details_override::set(false);
}

#[test]
fn render_tool_message_colors_high_token_badge() {
    let msg = DisplayMessage {
        role: "tool".to_string(),
        content: "x".repeat(48_000),
        tool_calls: Vec::new(),
        duration_secs: None,
        title: None,
        tool_data: Some(crate::message::ToolCall {
            id: "call_3".to_string(),
            name: "read".to_string(),
            input: serde_json::json!({"file_path": "src/main.rs"}),
            intent: None,
            thought_signature: None,
        }),
    };

    let lines = render_tool_message(&msg, 120, crate::config::DiffDisplayMode::Off);
    let badge_span = lines[0]
        .spans
        .iter()
        .find(|span| span.content.contains("12k tok"))
        .expect("missing token badge");

    assert_eq!(badge_span.style.fg, Some(rgb(255, 100, 100)));
}

#[test]
fn render_tool_message_shows_inline_diff_for_pascal_case_multiedit() {
    let msg = DisplayMessage {
        role: "tool".to_string(),
        content: "Edited demo.txt\n\nApplied:\n  ✓ Edit 1: replaced 1 occurrence\n\nTotal: 1 applied, 0 failed\n"
            .to_string(),
        tool_calls: Vec::new(),
        duration_secs: None,
        title: Some("demo.txt".to_string()),
        tool_data: Some(crate::message::ToolCall {
            id: "call_multiedit_pascal".to_string(),
            name: "MultiEdit".to_string(),
            input: serde_json::json!({
                "file_path": "demo.txt",
                "edits": [
                    {"old_string": "old line\n", "new_string": "new line\n"}
                ]
            }),
            intent: None, thought_signature: None, }),
    };

    let lines = render_tool_message(&msg, 100, crate::config::DiffDisplayMode::Inline);
    let plain = lines
        .iter()
        .map(extract_line_text)
        .collect::<Vec<_>>()
        .join("\n");

    assert!(plain.contains("┌─ diff"), "plain={plain}");
    assert!(plain.contains("old line"), "plain={plain}");
    assert!(plain.contains("new line"), "plain={plain}");
}

#[test]
fn render_tool_message_labels_single_file_apply_patch_diff() {
    let msg = DisplayMessage {
        role: "tool".to_string(),
        content: "✓ src/example.rs: modified (1 hunks)".to_string(),
        tool_calls: Vec::new(),
        duration_secs: None,
        title: Some("src/example.rs".to_string()),
        tool_data: Some(crate::message::ToolCall {
            id: "call_apply_patch_single".to_string(),
            name: "apply_patch".to_string(),
            input: serde_json::json!({
                "intent": "Update example behavior",
                "patch_text": "*** Begin Patch\n*** Update File: src/example.rs\n@@\n-old_value\n+new_value\n*** End Patch\n"
            }),
            intent: Some("Update example behavior".to_string()),
            thought_signature: None,
        }),
    };

    let lines = render_tool_message(&msg, 100, crate::config::DiffDisplayMode::Inline);
    let plain = lines
        .iter()
        .map(extract_line_text)
        .collect::<Vec<_>>()
        .join("\n");

    assert!(plain.contains("┌─ diff · src/example.rs"), "plain={plain}");
    assert!(plain.contains("old_value"), "plain={plain}");
    assert!(plain.contains("new_value"), "plain={plain}");
}

#[test]
fn render_tool_message_preserves_multi_file_apply_patch_boundaries() {
    let msg = DisplayMessage {
        role: "tool".to_string(),
        content: "✓ a.txt: modified (1 hunks)\n1- old a\n1+ new a\n✓ b.txt: modified (1 hunks)\n1- old b\n1+ new b\n".to_string(),
        tool_calls: Vec::new(),
        duration_secs: None,
        title: Some("2 files".to_string()),
        tool_data: Some(crate::message::ToolCall {
            id: "call_apply_patch_multi".to_string(),
            name: "apply_patch".to_string(),
            input: serde_json::json!({
                "intent": "Update both examples",
                "patch_text": "*** Begin Patch\n*** Update File: a.txt\n@@\n-old a\n+new a\n*** Update File: b.txt\n@@\n-old b\n+new b\n*** End Patch\n"
            }),
            intent: Some("Update both examples".to_string()),
            thought_signature: None,
        }),
    };

    let lines = render_tool_message(&msg, 100, crate::config::DiffDisplayMode::Inline);
    let plain = lines
        .iter()
        .map(extract_line_text)
        .collect::<Vec<_>>()
        .join("\n");

    let a_header = plain.find("┌─ diff · a.txt").expect("missing a.txt header");
    let old_a = plain.find("old a").expect("missing a.txt deletion");
    let new_a = plain.find("new a").expect("missing a.txt addition");
    let b_header = plain
        .find("├─ diff · b.txt")
        .expect("missing b.txt boundary");
    let old_b = plain.find("old b").expect("missing b.txt deletion");
    let new_b = plain.find("new b").expect("missing b.txt addition");
    assert!(
        a_header < old_a && old_a < new_a && new_a < b_header,
        "plain={plain}"
    );
    assert!(b_header < old_b && old_b < new_b, "plain={plain}");
}

#[test]
fn render_tool_message_shows_numbered_write_result_diff_after_input_compaction() {
    let msg = DisplayMessage {
        role: "tool".to_string(),
        content: "Created /tmp/head-to-head.html (2 lines):\n1+ <!doctype html>\n2+ <html lang=\"en\">\n..."
            .to_string(),
        tool_calls: Vec::new(),
        duration_secs: None,
        title: Some("/tmp/head-to-head.html".to_string()),
        tool_data: Some(crate::message::ToolCall {
            id: "call_write_compacted".to_string(),
            name: "write".to_string(),
            input: serde_json::json!({"file_path": "/tmp/head-to-head.html"}),
            intent: Some("Create an honest data-driven benchmark comparison page".to_string()),
            thought_signature: None,
        }),
    };

    let lines = render_tool_message(&msg, 100, crate::config::DiffDisplayMode::Inline);
    let plain = lines
        .iter()
        .map(extract_line_text)
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        lines[0].spans.iter().any(|span| span.content == "+2"),
        "plain={plain}"
    );
    assert!(plain.contains("┌─ diff"), "plain={plain}");
    assert!(plain.contains("<!doctype html>"), "plain={plain}");
    assert!(plain.contains("<html lang=\"en\">"), "plain={plain}");
}

#[test]
fn render_tool_message_never_draws_an_empty_edit_diff_frame() {
    for (name, content) in [
        ("write", "Created empty.txt (0 lines):\n"),
        ("edit", "Edited demo.txt: replaced 1 occurrence(s)"),
        (
            "multiedit",
            "Edited demo.txt\n\nTotal: 1 applied, 0 failed\n",
        ),
        ("patch", "Patch applied successfully"),
        ("apply_patch", "✓ demo.txt: modified (1 hunks)"),
    ] {
        let msg = DisplayMessage {
            role: "tool".to_string(),
            content: content.to_string(),
            tool_calls: Vec::new(),
            duration_secs: None,
            title: Some("demo.txt".to_string()),
            tool_data: Some(crate::message::ToolCall {
                id: format!("call_{name}_compacted"),
                name: name.to_string(),
                input: serde_json::json!({"file_path": "demo.txt"}),
                intent: None,
                thought_signature: None,
            }),
        };

        let lines = render_tool_message(&msg, 100, crate::config::DiffDisplayMode::Inline);
        let plain = lines
            .iter()
            .map(extract_line_text)
            .collect::<Vec<_>>()
            .join("\n");

        assert!(!plain.contains("┌─ diff"), "tool={name}, plain={plain}");
        assert!(!plain.contains("(+0 -0)"), "tool={name}, plain={plain}");
    }
}

#[test]
fn render_tool_message_marks_failed_apply_patch_without_empty_diff() {
    let msg = DisplayMessage {
        role: "tool".to_string(),
        content:
            "[apply_patch] ✗ /tmp/main.rs: Failed to find expected lines in /tmp/main.rs:\nfn missing() {}"
                .to_string(),
        tool_calls: Vec::new(),
        duration_secs: None,
        title: Some("/tmp/main.rs".to_string()),
        tool_data: Some(crate::message::ToolCall {
            id: "call_apply_patch_failed".to_string(),
            name: "apply_patch".to_string(),
            input: serde_json::json!({"file_path": "/tmp/main.rs"}),
            intent: Some("Replace the benchmark placeholder".to_string()),
            thought_signature: None,
        }),
    };

    let lines = render_tool_message(&msg, 100, crate::config::DiffDisplayMode::Inline);
    let plain = lines
        .iter()
        .map(extract_line_text)
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        plain.trim_start().starts_with("✗ apply_patch"),
        "plain={plain}"
    );
    assert!(!plain.contains("┌─ diff"), "plain={plain}");
    assert!(!plain.contains("(+0 -0)"), "plain={plain}");
}

#[test]
fn render_tool_message_inline_mode_truncates_large_diffs() {
    let old = (1..=7)
        .map(|i| format!("old line {i}\n"))
        .collect::<String>();
    let new = (1..=7)
        .map(|i| format!("new line {i} suffix_{i}_abcdefghijklmnopqrstuvwxyz0123456789\n"))
        .collect::<String>();
    let msg = DisplayMessage {
        role: "tool".to_string(),
        content: "Edited demo.txt".to_string(),
        tool_calls: Vec::new(),
        duration_secs: None,
        title: Some("demo.txt".to_string()),
        tool_data: Some(crate::message::ToolCall {
            id: "call_edit_inline_truncated".to_string(),
            name: "edit".to_string(),
            input: serde_json::json!({
                "file_path": "demo.txt",
                "old_string": old,
                "new_string": new,
            }),
            intent: None,
            thought_signature: None,
        }),
    };

    let lines = render_tool_message(&msg, 40, crate::config::DiffDisplayMode::Inline);
    let plain = lines
        .iter()
        .map(extract_line_text)
        .collect::<Vec<_>>()
        .join("\n");

    assert!(plain.contains("... 2 more changes ..."), "plain={plain}");
    assert!(plain.contains("old line 3"), "plain={plain}");
    assert!(!plain.contains("old line 7"), "plain={plain}");
    assert!(
        !plain.contains("new line 1 suffix_1_abcdefghijklmnopqrstuvwxyz0123456789"),
        "plain={plain}"
    );
    assert!(plain.contains("suffix_2_abcdefghijklm…"), "plain={plain}");
}

#[test]
fn render_tool_message_full_inline_mode_shows_full_diff() {
    let old = (1..=7)
        .map(|i| format!("old line {i}\n"))
        .collect::<String>();
    let new = (1..=7)
        .map(|i| format!("new line {i} suffix_{i}_abcdefghijklmnopqrstuvwxyz0123456789\n"))
        .collect::<String>();
    let msg = DisplayMessage {
        role: "tool".to_string(),
        content: "Edited demo.txt".to_string(),
        tool_calls: Vec::new(),
        duration_secs: None,
        title: Some("demo.txt".to_string()),
        tool_data: Some(crate::message::ToolCall {
            id: "call_edit_inline_full".to_string(),
            name: "edit".to_string(),
            input: serde_json::json!({
                "file_path": "demo.txt",
                "old_string": old,
                "new_string": new,
            }),
            intent: None,
            thought_signature: None,
        }),
    };

    let lines = render_tool_message(&msg, 40, crate::config::DiffDisplayMode::FullInline);
    let plain = lines
        .iter()
        .map(extract_line_text)
        .collect::<Vec<_>>()
        .join("\n");

    assert!(!plain.contains("more changes"), "plain={plain}");
    assert!(plain.contains("old line 4"), "plain={plain}");
    assert!(
        plain.contains("new line 4 suffix_4_abcdefghijklmnopqrstuvwxyz0123456789"),
        "plain={plain}"
    );
    assert!(!plain.contains('…'), "plain={plain}");
}

#[test]
fn render_tool_message_memory_store_centered_mode_left_aligns_with_padding() {
    let saved = crate::tui::markdown::center_code_blocks();
    crate::tui::markdown::set_center_code_blocks(true);
    let msg = DisplayMessage {
        role: "tool".to_string(),
        content: "Saved memory".to_string(),
        tool_calls: Vec::new(),
        duration_secs: None,
        title: None,
        tool_data: Some(crate::message::ToolCall {
            id: "call_memory_store_centered".to_string(),
            name: "memory".to_string(),
            input: serde_json::json!({
                "action": "remember",
                "category": "fact",
                "content": "Centered mode should pad saved memory cards too"
            }),
            intent: None,
            thought_signature: None,
        }),
    };

    let lines = render_tool_message(&msg, 120, crate::config::DiffDisplayMode::Off);
    let rendered: Vec<String> = lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect()
        })
        .collect();

    assert!(!rendered.is_empty(), "expected rendered saved-memory card");
    assert!(
        rendered.iter().all(|line| line.starts_with("  ")),
        "centered saved-memory card should include shared left padding: {rendered:?}"
    );
    assert_eq!(
        lines[0].alignment,
        Some(ratatui::layout::Alignment::Left),
        "centered saved-memory card should be left-aligned after padding"
    );

    crate::tui::markdown::set_center_code_blocks(saved);
}

#[test]
fn render_tool_message_shows_swarm_spawn_prompt_summary() {
    let msg = DisplayMessage {
        role: "tool".to_string(),
        content: "spawned".to_string(),
        tool_calls: Vec::new(),
        duration_secs: None,
        title: None,
        tool_data: Some(crate::message::ToolCall {
            id: "call_swarm_spawn".to_string(),
            name: "swarm".to_string(),
            input: serde_json::json!({
                "action": "spawn",
                "prompt": "Extract the restart command cluster from cli commands and validate it"
            }),
            intent: None,
            thought_signature: None,
        }),
    };

    let lines = render_tool_message(&msg, 120, crate::config::DiffDisplayMode::Off);
    let rendered: String = lines[0]
        .spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect();

    assert!(rendered.contains("swarm spawn"), "rendered={rendered}");
    assert!(
        rendered.contains("Extract the restart command cluster"),
        "rendered={rendered}"
    );
}

#[test]
fn render_tool_message_batch_subcall_shows_swarm_dm_details() {
    let msg = DisplayMessage {
        role: "tool".to_string(),
        content: "--- [1] swarm ---\nDone\n\nCompleted: 1 succeeded, 0 failed".to_string(),
        tool_calls: Vec::new(),
        duration_secs: None,
        title: None,
        tool_data: Some(crate::message::ToolCall {
            id: "call_batch_swarm".to_string(),
            name: "batch".to_string(),
            input: serde_json::json!({
                "tool_calls": [
                    {
                        "tool": "swarm",
                        "action": "dm",
                        "to_session": "shark",
                        "message": "Please validate the restart extraction and report back"
                    }
                ]
            }),
            intent: None,
            thought_signature: None,
        }),
    };

    let lines = render_tool_message(&msg, 120, crate::config::DiffDisplayMode::Off);
    let rendered = lines
        .iter()
        .map(extract_line_text)
        .collect::<Vec<_>>()
        .join("\n");

    assert!(rendered.contains("swarm dm → shark"), "rendered={rendered}");
    assert!(
        rendered.contains("Please validate the restart"),
        "rendered={rendered}"
    );
}

#[test]
fn render_kgrep_output_body_borders_each_line() {
    let content = "crates/foo.rs\n  symbols: 1 matched\n    - fn bar @ 1-5";
    let lines = super::render_kgrep_output_body(content, 120);
    let rendered = lines
        .iter()
        .map(extract_line_text)
        .collect::<Vec<_>>()
        .join("\n");

    assert!(rendered.contains("│ crates/foo.rs"), "rendered={rendered}");
    assert!(
        rendered.contains("│   symbols: 1 matched"),
        "rendered={rendered}"
    );
    assert!(
        rendered.contains("│     - fn bar @ 1-5"),
        "rendered={rendered}"
    );
    assert_eq!(lines.len(), 3, "one bordered line per source line");
}

#[test]
fn render_kgrep_output_body_caps_huge_output() {
    let content = (0..1000)
        .map(|i| format!("line {i}"))
        .collect::<Vec<_>>()
        .join("\n");
    let lines = super::render_kgrep_output_body(&content, 120);
    // 400-line cap plus a single truncation summary line.
    assert_eq!(lines.len(), 401, "should cap the body and add a summary");
    let last = extract_line_text(&lines[lines.len() - 1]);
    assert!(last.contains("more lines"), "last={last}");
}

#[test]
fn render_assistant_message_plan_card_wraps_instead_of_truncating() {
    let saved = crate::tui::markdown::center_code_blocks();
    crate::tui::markdown::set_center_code_blocks(false);
    // Long paragraph and long list items must wrap inside the card, not be
    // clipped at the right border by render_rounded_box's truncation.
    let plan_body = "# Long content plan\n\n\
        Goal\n\
        Produce an up-to-date ranked report grounded in current crate paths, then fix the highest-leverage low-risk offenders without destabilizing active work.\n\n\
        Approach\n\
        1. Write an audit document that regenerates metrics with current crate paths, ranks the top issues with evidence, and marks which items from the previous audit are complete versus stale.\n\
        2. Map the provider migration and record whether each module is a thin wrapper, partial duplicate, or full duplicate of the extracted crate.\n";
    let content = format!("Intro text.\n\n```plan\n{plan_body}```\n\nAfter the card.");
    let msg = DisplayMessage::assistant(&content);

    for width in [40u16, 60, 80, 100, 140] {
        let lines = render_assistant_message(&msg, width, crate::config::DiffDisplayMode::Off);
        let squashed = lines
            .iter()
            .map(extract_line_text)
            .collect::<Vec<_>>()
            .join(" ")
            .replace(['│', '╭', '╮', '╰', '╯', '─'], " ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        for phrase in [
            "without destabilizing active work.",
            "complete versus stale.",
            "or full duplicate of the extracted crate.",
        ] {
            assert!(
                squashed.contains(phrase),
                "width {width}: plan card lost trailing content {phrase:?}\n{squashed}"
            );
        }
        // Card borders stay intact.
        for line in lines
            .iter()
            .map(extract_line_text)
            .filter(|l| l.contains('│'))
        {
            assert!(
                line.trim_end().ends_with('│'),
                "width {width}: card row missing right border: {line:?}"
            );
        }
    }
    crate::tui::markdown::set_center_code_blocks(saved);
}

#[test]
fn render_empty_todo_tool_result_collapses_to_compact_line() {
    let msg = DisplayMessage {
        role: "tool".to_string(),
        content: "[todo] []".to_string(),
        tool_calls: Vec::new(),
        duration_secs: None,
        title: Some("0 todos".to_string()),
        tool_data: Some(crate::message::ToolCall {
            id: "call_todo_empty".to_string(),
            name: "todo".to_string(),
            input: serde_json::json!({}),
            intent: Some("Read the todo list".to_string()),
            thought_signature: None,
        }),
    };

    let plain = render_tool_message(&msg, 100, crate::config::DiffDisplayMode::Off)
        .iter()
        .map(extract_line_text)
        .collect::<Vec<_>>()
        .join("\n");

    assert!(!plain.contains("No tasks yet"), "{plain}");
    assert!(plain.contains("no rows"), "{plain}");
}

#[test]
fn batched_retry_renders_the_todo_row_count_from_a_nested_call() {
    // A todo call nested inside a batch result keeps one compact line naming
    // the row count; the pinned list is the surface, not the transcript.
    let todos = vec![crate::todo::TaskItem {
        id: "inspect".to_string(),
        content: "Inspect the starter project".to_string(),
        status: "in_progress".to_string(),
        priority: "high".to_string(),
        group: Some("pelican-bike".to_string()),
        ..Default::default()
    }];
    let todo_output = serde_json::to_string_pretty(&todos).unwrap();
    let content = format!(
        "--- [1] todo ---\n{todo_output}\n\n--- [2] ls ---\n./\n\n0 files, 0 directories\n\nCompleted: 2 succeeded, 0 failed"
    );
    let msg = DisplayMessage {
        role: "tool".to_string(),
        content,
        tool_calls: Vec::new(),
        duration_secs: None,
        title: None,
        tool_data: Some(crate::message::ToolCall {
            id: "call_batch".to_string(),
            name: "batch".to_string(),
            input: serde_json::json!({
                "intent": "Inspect starter files",
                "tool_calls": [
                    { "tool": "todo", "intent": "Plan the work", "todos": todos },
                    { "tool": "ls", "path": "." }
                ]
            }),
            intent: Some("Inspect starter files".to_string()),
            thought_signature: None,
        }),
    };

    let rendered = render_tool_message(&msg, 84, crate::config::DiffDisplayMode::Off)
        .iter()
        .map(extract_line_text)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains("todo 1 rows"), "{rendered}");
    assert!(
        !rendered.contains("Inspect the starter project"),
        "the pinned list is the surface, not the transcript:\n{rendered}"
    );
}
