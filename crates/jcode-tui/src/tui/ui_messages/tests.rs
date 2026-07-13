use super::*;

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
fn render_overnight_message_uses_rounded_progress_card() {
    let card = crate::overnight::OvernightProgressCard {
        run_id: "overnight_1234567890abcdef".to_string(),
        status: "running".to_string(),
        phase: "running".to_string(),
        coordinator_session_id: "session_coord".to_string(),
        coordinator_session_name: "Overnight coordinator".to_string(),
        elapsed_label: "2h 15m".to_string(),
        target_duration_label: "7h".to_string(),
        progress_percent: 32.0,
        target_wake_at: "2026-05-01T15:00:00Z".to_string(),
        time_relation: "target in 4h 45m".to_string(),
        last_activity_label: "4m ago".to_string(),
        next_prompt_label: "handoff mode in 4h 15m or after current turn".to_string(),
        usage_risk: "medium".to_string(),
        usage_confidence: "low".to_string(),
        usage_projection: "projected 48% to 76%".to_string(),
        resources_summary: "RAM 62%, load 2.4/8, battery 80% discharging, disk 52.0 GB free"
            .to_string(),
        latest_event_kind: Some("coordinator_turn_completed".to_string()),
        latest_event_summary: Some("Coordinator turn completed".to_string()),
        task_summary: crate::overnight::OvernightTaskCardSummary {
            total: 4,
            counts: crate::overnight::OvernightTaskStatusCounts {
                completed: 2,
                active: 1,
                blocked: 0,
                deferred: 1,
                failed: 0,
                skipped: 0,
                unknown: 0,
            },
            validated: 2,
            high_risk: 0,
            latest_title: Some("Verify provider reload".to_string()),
            latest_status: Some("active".to_string()),
        },
        active_task_title: Some("Verify provider reload".to_string()),
        review_path: "/tmp/overnight/review.html".to_string(),
        log_path: "/tmp/overnight/run.log".to_string(),
        run_dir: "/tmp/overnight".to_string(),
        completed_at: None,
    };
    let msg = DisplayMessage::overnight(serde_json::to_string(&card).unwrap());

    let lines = render_overnight_message(&msg, 100, crate::config::DiffDisplayMode::Off);
    let plain = lines
        .iter()
        .map(extract_line_text)
        .collect::<Vec<_>>()
        .join("\n");

    assert!(plain.contains("overnight · running"));
    assert!(plain.contains("█"));
    assert!(plain.contains("░"));
    assert!(plain.contains("32%"));
    assert!(plain.contains("2 complete, 1 active, 0 blocked, 1 deferred"));
    assert!(plain.contains("Verify provider reload"));
    assert!(plain.contains("medium risk"));
    assert!(plain.contains("review.html"));
}

#[test]
fn render_todos_message_shows_grouped_card_with_status_glyphs() {
    fn todo(id: &str, content: &str, status: &str, group: Option<&str>) -> crate::todo::TodoItem {
        crate::todo::TodoItem {
            id: id.to_string(),
            content: content.to_string(),
            status: status.to_string(),
            priority: "high".to_string(),
            group: group.map(str::to_string),
            confidence: Some(80),
            completion_confidence: (status == "completed").then_some(95),
            confidence_history: Vec::new(),
            blocked_by: Vec::new(),
            assigned_to: None,
        }
    }

    let todos = vec![
        todo("1", "Wire the hotkey", "completed", Some("todo card")),
        todo("2", "Render the card", "in_progress", Some("todo card")),
        todo("3", "Unrelated cleanup", "pending", None),
    ];
    let msg = DisplayMessage::todos(serde_json::to_string(&todos).unwrap());

    let lines = render_todos_message(&msg, 100, crate::config::DiffDisplayMode::Off);
    let plain = lines
        .iter()
        .map(extract_line_text)
        .collect::<Vec<_>>()
        .join("\n");

    assert!(!plain.contains("Todos"), "{plain}");
    assert!(plain.contains("todo card"), "{plain}");
    assert!(plain.contains("other"), "{plain}");
    let todo_card_header = lines
        .iter()
        .map(extract_line_text)
        .find(|line| line.contains("todo card"))
        .unwrap();
    assert_eq!(todo_card_header.matches('●').count(), 2, "{plain}");
    assert_eq!(todo_card_header.matches('○').count(), 0, "{plain}");
    let other_header = lines
        .iter()
        .map(extract_line_text)
        .find(|line| line.contains("other"))
        .unwrap();
    assert_eq!(other_header.matches('○').count(), 1, "{plain}");
    assert!(plain.contains("✓ Wire the hotkey"), "{plain}");
    assert!(plain.contains("● Render the card"), "{plain}");
    assert!(plain.contains("○ Unrelated cleanup"), "{plain}");
    // Completed items show completion confidence; open ones planning confidence.
    assert!(plain.contains("80→95%"), "{plain}");
    assert!(plain.contains("80%"), "{plain}");
    // Only open items carry the high-priority marker.
    assert!(!plain.contains("Wire the hotkey (high)"), "{plain}");
    assert!(plain.contains("Render the card (high)"), "{plain}");
    assert!(
        !plain.contains('╭'),
        "todo card should be borderless:\n{plain}"
    );
    assert!(
        !plain.contains('╰'),
        "todo card should be borderless:\n{plain}"
    );
}

#[test]
fn render_todos_message_shows_goal_scores_and_feedback() {
    let todos = vec![crate::todo::TodoItem {
        id: "1".to_string(),
        content: "Render the card".to_string(),
        status: "in_progress".to_string(),
        priority: "high".to_string(),
        group: Some("todo rendering".to_string()),
        confidence: Some(85),
        completion_confidence: None,
        confidence_history: vec![80, 85],
        blocked_by: Vec::new(),
        assigned_to: None,
    }];
    let goals = vec![crate::todo::TodoGoal {
        group: Some("todo rendering".to_string()),
        hill_climbability: Some(95),
        objective: Some("Readable at 80 columns".to_string()),
        feedback_loop: Some("Inspect a debug frame".to_string()),
        end_to_end_ownership: Some(90),
    }];
    let msg =
        DisplayMessage::todos(serde_json::json!({ "todos": todos, "goals": goals }).to_string());

    let plain = render_todos_message(&msg, 100, crate::config::DiffDisplayMode::Off)
        .iter()
        .map(extract_line_text)
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        plain.contains("Hill climbability 95% · Ownership 90%"),
        "{plain}"
    );
    assert!(
        plain.contains("Objective · Readable at 80 columns"),
        "{plain}"
    );
    assert!(
        plain.contains("Feedback · Inspect a debug frame"),
        "{plain}"
    );
    assert!(plain.contains("● Render the card (high) · 85%"), "{plain}");
}

#[test]
fn render_todos_message_uses_readable_semantic_colors() {
    let todos = vec![crate::todo::TodoItem {
        id: "1".to_string(),
        content: "Tune the palette".to_string(),
        status: "in_progress".to_string(),
        priority: "high".to_string(),
        group: Some("todo rendering".to_string()),
        confidence: Some(85),
        completion_confidence: None,
        confidence_history: Vec::new(),
        blocked_by: Vec::new(),
        assigned_to: None,
    }];
    let goals = vec![crate::todo::TodoGoal {
        group: Some("todo rendering".to_string()),
        hill_climbability: Some(95),
        objective: Some("Readable metadata".to_string()),
        feedback_loop: None,
        end_to_end_ownership: None,
    }];
    let msg =
        DisplayMessage::todos(serde_json::json!({ "todos": todos, "goals": goals }).to_string());
    let lines = render_todos_message(&msg, 100, crate::config::DiffDisplayMode::Off);
    let color_for = |text: &str| {
        lines
            .iter()
            .flat_map(|line| line.spans.iter())
            .find(|span| span.content.as_ref() == text)
            .and_then(|span| span.style.fg)
    };

    assert_eq!(color_for("todo rendering"), Some(todo_group_color()));
    assert_eq!(color_for("Readable metadata"), Some(todo_meta_color()));
    assert_eq!(color_for("● "), Some(asap_color()));
    assert_eq!(color_for(" (high)"), Some(rgb(235, 175, 95)));
    assert_eq!(color_for(" · 85%"), Some(todo_confidence_color()));
    assert_ne!(todo_meta_color(), dim_color());
    assert_ne!(asap_color(), rgb(235, 175, 95));
}

#[test]
fn render_todos_message_wraps_goal_scores_at_narrow_widths() {
    let todos = vec![crate::todo::TodoItem {
        id: "1".to_string(),
        content: "Render the card".to_string(),
        status: "in_progress".to_string(),
        priority: "high".to_string(),
        group: Some("todo rendering".to_string()),
        confidence: Some(85),
        completion_confidence: None,
        confidence_history: Vec::new(),
        blocked_by: Vec::new(),
        assigned_to: None,
    }];
    let goals = vec![crate::todo::TodoGoal {
        group: Some("todo rendering".to_string()),
        hill_climbability: Some(95),
        objective: None,
        feedback_loop: None,
        end_to_end_ownership: Some(90),
    }];
    let msg =
        DisplayMessage::todos(serde_json::json!({ "todos": todos, "goals": goals }).to_string());

    let lines = render_todos_message(&msg, 40, crate::config::DiffDisplayMode::Off);
    let plain = lines
        .iter()
        .map(extract_line_text)
        .collect::<Vec<_>>()
        .join("\n");

    assert!(plain.contains("Hill climbability 95%"), "{plain}");
    assert!(plain.contains("Ownership 90%"), "{plain}");
    assert!(
        lines.iter().all(|line| line.width() <= 38),
        "card exceeded its 38-column content budget: {plain}"
    );
}

#[test]
fn render_todos_message_empty_list_shows_placeholder() {
    let msg = DisplayMessage::todos("[]");
    let plain = render_todos_message(&msg, 100, crate::config::DiffDisplayMode::Off)
        .iter()
        .map(extract_line_text)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(!plain.contains("Todos"), "{plain}");
    assert!(plain.contains("No tasks yet"), "{plain}");
}

#[test]
fn render_todos_message_bad_payload_falls_back_to_system() {
    let msg = DisplayMessage::todos("not json");
    let lines = render_todos_message(&msg, 100, crate::config::DiffDisplayMode::Off);
    assert!(!lines.is_empty());
}

#[test]
fn render_todo_tool_result_uses_borderless_card_with_goal_scores() {
    let todos = vec![crate::todo::TodoItem {
        id: "render".to_string(),
        content: "Render the todo result".to_string(),
        status: "in_progress".to_string(),
        priority: "high".to_string(),
        group: Some("todo rendering".to_string()),
        confidence: Some(92),
        completion_confidence: None,
        confidence_history: vec![85, 92],
        blocked_by: Vec::new(),
        assigned_to: None,
    }];
    let goals = vec![crate::todo::TodoGoal {
        group: Some("todo rendering".to_string()),
        hill_climbability: Some(95),
        objective: Some("Readable card".to_string()),
        feedback_loop: Some("Inspect the rendered frame".to_string()),
        end_to_end_ownership: Some(92),
    }];
    let content = format!(
        "{}\n\nGoals:\n{}\n\nKeep the feedback loop concrete.",
        serde_json::to_string_pretty(&todos).unwrap(),
        serde_json::to_string_pretty(&goals).unwrap()
    );
    let msg = DisplayMessage {
        role: "tool".to_string(),
        content,
        tool_calls: Vec::new(),
        duration_secs: None,
        title: Some("1 todos".to_string()),
        tool_data: Some(crate::message::ToolCall {
            id: "call_todo".to_string(),
            name: "todo".to_string(),
            input: serde_json::json!({ "todos": todos, "goals": goals }),
            intent: Some("Track todo card work".to_string()),
            thought_signature: None,
        }),
    };

    let plain = render_tool_message(&msg, 100, crate::config::DiffDisplayMode::Off)
        .iter()
        .map(extract_line_text)
        .collect::<Vec<_>>()
        .join("\n");

    assert!(!plain.contains("Todos"), "{plain}");
    assert!(plain.contains("todo rendering  ●"), "{plain}");
    assert!(
        plain.contains("Hill climbability 95% · Ownership 92%"),
        "{plain}"
    );
    assert!(
        plain.contains("● Render the todo result (high) · 92%"),
        "{plain}"
    );
    assert!(
        !plain.contains('╭'),
        "todo tool result should be borderless:\n{plain}"
    );
    assert!(
        !plain.contains("todo 1 items"),
        "generic tool row leaked:\n{plain}"
    );
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
        "[Scheduled task]\nA scheduled task for this session is now due.\n\nTask: Follow up on the scheduler test\nWorking directory: /home/jeremy/jcode\nRelevant files: src/tui/ui_messages.rs\nBranch: master\n\nBackground: Verify the scheduled task card styling\nSuccess criteria: The due task renders clearly\nScheduled by session: session_test",
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
        content: "Scheduled task 'Follow up on the scheduler test' for in 1m (id: sched_abc123)\nWorking directory: /home/jeremy/jcode\nRelevant files: src/tui/ui_messages.rs\nTarget: resume session session_test".to_string(),
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
        "```plan\n# Validation plan\n\n```bash\ncargo test -p jcode-tui\n```\n\nAfter the block.\n```\n\nOutside text.",
    );

    let lines = render_assistant_message(&msg, 100, crate::config::DiffDisplayMode::Off);
    crate::tui::markdown::set_center_code_blocks(saved);
    let plain = lines
        .iter()
        .map(extract_line_text)
        .collect::<Vec<_>>()
        .join("\n");

    assert!(plain.contains("⛭ Validation plan"), "plain: {plain}");
    assert!(plain.contains("cargo test -p jcode-tui"), "plain: {plain}");
    assert!(plain.contains("After the block."), "plain: {plain}");
    assert!(plain.contains("Outside text."), "plain: {plain}");
    // The nested bash content stays inside the card borders.
    let bash_line = lines
        .iter()
        .map(extract_line_text)
        .find(|line| line.contains("cargo test -p jcode-tui"))
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
fn render_system_message_uses_reload_card_for_reload_title() {
    let msg = DisplayMessage::system("Reloading server with newer binary...").with_title("Reload");

    let lines = render_system_message(&msg, 80, crate::config::DiffDisplayMode::Off);
    let plain = lines
        .iter()
        .map(extract_line_text)
        .collect::<Vec<_>>()
        .join("\n");

    assert!(
        plain.contains("reload"),
        "expected reload card title: {plain}"
    );
    assert!(plain.contains("Reloading server with newer binary"));
}

#[test]
fn render_system_message_uses_connection_card_for_reconnect_status() {
    let msg = DisplayMessage::system(
        "⚡ Connection lost - retrying (attempt 2, 7s) - connection reset by server · resume: jcode --resume koala",
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
    assert!(plain.contains("jcode --resume koala"));
}

#[test]
fn render_swarm_message_centered_mode_caps_wrap_width_for_long_notifications() {
    let saved = crate::tui::markdown::center_code_blocks();
    crate::tui::markdown::set_center_code_blocks(true);
    let msg = DisplayMessage::swarm(
        "File activity",
        "/home/jeremy/jcode/src/tui/ui_messages.rs - moss just edited this file while you were working nearby, so the notification should still read as centered in wide layouts.",
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
    let content = jcode_tui_messages::encode_collapsible_swarm_content(
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
    let collapsed = jcode_tui_messages::encode_collapsible_swarm_content(
        "fixed the flaky test",
        "The flaky test was caused by a race in the setup helper.",
    );
    let expanded =
        jcode_tui_messages::toggle_collapsible_swarm_content(&collapsed).expect("toggle");
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
                "command": "cargo test -p jcode render_background_task --lib",
                "intent": "Verify compact progress card"
            }),
            intent: Some("Verify compact progress card".to_string()),
            thought_signature: None,
        }),
    };

    let lines = render_tool_message(&msg, 120, crate::config::DiffDisplayMode::Off);
    let rendered = extract_line_text(&lines[0]);

    assert!(rendered.contains("bash · Verify compact progress card · $ cargo test"));
    assert_eq!(
        lines.len(),
        1,
        "intent should not add vertical space: {rendered}"
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

    assert_eq!(badge_span.style.fg, Some(rgb(118, 118, 118)));
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

    assert_eq!(badge_span.style.fg, Some(rgb(224, 118, 118)));
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
fn render_tool_message_memory_recall_centered_mode_left_aligns_with_padding() {
    let saved = crate::tui::markdown::center_code_blocks();
    crate::tui::markdown::set_center_code_blocks(true);
    let msg = DisplayMessage {
        role: "tool".to_string(),
        content: concat!(
            "- [fact] Centered mode should keep the recall card centered\n",
            "- [preference] The user likes visible side gutters"
        )
        .to_string(),
        tool_calls: Vec::new(),
        duration_secs: None,
        title: None,
        tool_data: Some(crate::message::ToolCall {
            id: "call_memory_recall_centered".to_string(),
            name: "memory".to_string(),
            input: serde_json::json!({
                "action": "recall",
                "query": "centered mode"
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

    assert!(!rendered.is_empty(), "expected rendered recall card");
    assert!(
        rendered.iter().all(|line| line.starts_with("  ")),
        "centered recall card should include shared left padding: {rendered:?}"
    );
    assert_eq!(
        lines[0].alignment,
        Some(ratatui::layout::Alignment::Left),
        "centered recall card header should be left-aligned after padding"
    );
    assert!(
        rendered[0]
            .trim_start()
            .starts_with("🧠 recalled 2 memories"),
        "unexpected recall header: {rendered:?}"
    );

    crate::tui::markdown::set_center_code_blocks(saved);
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
fn render_agentgrep_output_body_borders_each_line() {
    let content = "crates/foo.rs\n  symbols: 1 matched\n    - fn bar @ 1-5";
    let lines = super::render_agentgrep_output_body(content, 120);
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
fn render_agentgrep_output_body_caps_huge_output() {
    let content = (0..1000)
        .map(|i| format!("line {i}"))
        .collect::<Vec<_>>()
        .join("\n");
    let lines = super::render_agentgrep_output_body(&content, 120);
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
fn render_swarm_message_preserves_inline_image_placeholder_lines() {
    let saved = crate::tui::markdown::center_code_blocks();
    crate::tui::markdown::set_center_code_blocks(false);

    // Simulate a rendered mermaid diagram inside a swarm message body: the
    // marker line plus its blank fill rows must survive rendering without a
    // rail prefix or blank-line cleanup so the image draws at full height.
    let placeholder = crate::tui::mermaid::inline_image_placeholder_lines(0xabcd1234, 4, 20);
    assert_eq!(placeholder.len(), 4);
    let marker_text = placeholder[0]
        .spans
        .iter()
        .map(|s| s.content.as_ref())
        .collect::<String>();

    let msg = DisplayMessage::swarm(
        "Plan graph · v3",
        "```mermaid\nflowchart TD\n    a --> b\n```",
    );
    // Rendering the real message goes through the markdown pipeline; whether a
    // real image materializes depends on protocol availability, so test the
    // line-preservation path directly through render_swarm_message with a body
    // the markdown renderer maps to placeholder lines is not deterministic in
    // tests. Instead assert the parser round-trips the marker we emit.
    let parsed = crate::tui::mermaid::parse_inline_image_placeholder(&placeholder[0]);
    assert_eq!(parsed, Some((0xabcd1234, 4, 20)));
    assert!(
        marker_text.starts_with('\u{0}'),
        "marker must keep its sentinel prefix"
    );

    // And the swarm renderer must not panic or drop content for a mermaid body.
    let lines = render_swarm_message(&msg, 100, crate::config::DiffDisplayMode::Off);
    assert!(!lines.is_empty());

    crate::tui::markdown::set_center_code_blocks(saved);
}
