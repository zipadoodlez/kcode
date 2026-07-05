//! Buffer-level (terminal cell) verification for the inline swarm strip and
//! the notification line.
//!
//! Prior swarm-strip/notification tests only checked `Line` construction
//! (span widths). These tests close that gap: they render through ratatui's
//! `TestBackend` so actual cell writes are exercised, including the full
//! `ui::draw` layout path (ui.rs strip Paragraph at chunk 2, notification at
//! chunk 4) and direct widget draws into sub-areas, asserting no panics and
//! that nothing is written outside the target area even with wide glyphs.

use super::*;
use crate::protocol::SwarmMemberStatus;
use crate::tui::ui::clear_flicker_frame_history_for_tests;
use ratatui::Terminal;
use ratatui::backend::TestBackend;

fn strip_member(id: &str, name: &str, status: &str) -> SwarmMemberStatus {
    SwarmMemberStatus {
        session_id: id.to_string(),
        friendly_name: Some(name.to_string()),
        status: status.to_string(),
        detail: Some("working on task".to_string()),
        task_label: None,
        role: None,
        is_headless: Some(true),
        live_attachments: None,
        status_age_secs: Some(5),
        output_tail: None,
        report_back_to_session_id: None,
        todo_progress: Some((2, 5)),
        todo_items: Vec::new(),
    }
}

/// Buffer contents as one string per row (not trimmed, full width).
fn buffer_rows(terminal: &Terminal<TestBackend>) -> Vec<String> {
    let buf = terminal.backend().buffer();
    let width = buf.area.width;
    let height = buf.area.height;
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buf[(x, y)].symbol().to_string())
                .collect::<String>()
        })
        .collect()
}

#[test]
fn swarm_strip_full_draw_writes_chips_row_above_status_line() {
    let _lock = viewport_snapshot_test_lock();
    clear_flicker_frame_history_for_tests();
    // Placement state is process-global; a dock placed by another test would
    // make the strip stand down. Clear it so this frame is self-contained.
    crate::tui::info_widget::clear_widget_placements_for_tests();
    let state = TestState {
        display_messages: vec![DisplayMessage::assistant("hello from the coordinator")],
        messages_version: 1,
        swarm_members: vec![
            strip_member("s1", "researcher", "running"),
            strip_member("s2", "reviewer", "completed"),
        ],
        ..Default::default()
    };

    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| crate::tui::ui::draw(frame, &state))
        .expect("full draw with inline swarm strip should not panic");

    let status_area = crate::tui::ui::last_status_area().expect("status area recorded");
    assert!(status_area.y > 0, "status line should not be the top row");
    let rows = buffer_rows(&terminal);
    // Vertical strip (default layout): one agent per row directly above the
    // status line, first row carrying the 🐝 marker.
    let strip_rows = rows[..status_area.y as usize]
        .iter()
        .rev()
        .take(2)
        .cloned()
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        strip_rows.contains("🐝"),
        "expected the swarm marker above the status line, got: {strip_rows:?}"
    );
    assert!(
        strip_rows.contains("researcher"),
        "expected member row in strip cells, got: {strip_rows:?}"
    );
    assert!(
        strip_rows.contains("2/5"),
        "expected todo progress counter in strip cells, got: {strip_rows:?}"
    );
}

#[test]
fn swarm_strip_full_draw_survives_narrow_width_sweep() {
    let _lock = viewport_snapshot_test_lock();
    crate::tui::info_widget::clear_widget_placements_for_tests();
    let state = TestState {
        display_messages: vec![DisplayMessage::assistant("narrow sweep")],
        messages_version: 1,
        swarm_members: vec![
            strip_member("s1", "alpha", "running"),
            strip_member("s2", "beta", "ready"),
            strip_member("s3", "gamma", "failed"),
        ],
        ..Default::default()
    };

    for width in 12_u16..=44 {
        for height in [8_u16, 12, 20] {
            clear_flicker_frame_history_for_tests();
            let backend = TestBackend::new(width, height);
            let mut terminal = Terminal::new(backend).expect("test terminal");
            terminal
                .draw(|frame| crate::tui::ui::draw(frame, &state))
                .unwrap_or_else(|e| {
                    panic!("swarm strip draw failed at {width}x{height}: {e}");
                });
        }
    }
}

#[test]
fn swarm_strip_full_draw_handles_wide_glyph_member_names() {
    let _lock = viewport_snapshot_test_lock();
    crate::tui::info_widget::clear_widget_placements_for_tests();
    let mut coordinator = strip_member("s0", "調整役エージェント", "running");
    coordinator.role = Some("coordinator".to_string());
    let mut streaming = strip_member("s1", "深度搜索智能体", "running");
    streaming.output_tail = Some("正在分析：渲染管線的寬字元邊界 🐝🎨".to_string());
    let members = vec![
        coordinator,
        streaming,
        strip_member("s2", "🦊🦊🦊 fox-agent 🦊🦊🦊", "completed"),
    ];

    // Unfocused (1 line) and focused (chips + preview + hints) variants both
    // must survive cell-level rendering with wide glyphs at every width.
    for focused in [false, true] {
        let state = TestState {
            display_messages: vec![DisplayMessage::assistant("wide glyph check")],
            messages_version: 1,
            swarm_members: members.clone(),
            swarm_panel_focused: focused,
            swarm_panel_selected: 1,
            ..Default::default()
        };
        for width in [24_u16, 25, 30, 31, 44, 80] {
            clear_flicker_frame_history_for_tests();
            let backend = TestBackend::new(width, 16);
            let mut terminal = Terminal::new(backend).expect("test terminal");
            terminal
                .draw(|frame| crate::tui::ui::draw(frame, &state))
                .unwrap_or_else(|e| {
                    panic!("wide-glyph strip draw failed at width {width} focused={focused}: {e}");
                });
        }
    }
}

#[test]
fn swarm_strip_paragraph_never_writes_outside_target_area() {
    // Mirror the exact ui.rs render path (Paragraph::new(lines) into a chunk),
    // but deliberately render lines built for a wider area into a narrow Rect
    // to prove clipping happens at the cell level, including wide glyphs.
    let members = vec![
        strip_member("s1", "深度搜索エージェント", "running"),
        strip_member("s2", "reviewer-with-a-long-name", "completed"),
    ];
    let gallery_lines = crate::tui::info_widget::swarm_gallery::render_swarm_strip_lines(
        &members, 0, true, "ctrl+t", 3, 80, 16,
    );
    assert!(!gallery_lines.is_empty(), "expected focused strip lines");

    let backend = TestBackend::new(40, 6);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    let area = Rect::new(2, 1, 20, gallery_lines.len() as u16);
    terminal
        .draw(|frame| {
            frame.render_widget(Paragraph::new(gallery_lines.clone()), area);
        })
        .expect("over-wide strip paragraph should clip, not panic");

    let rows = buffer_rows(&terminal);
    for (y, row) in rows.iter().enumerate() {
        let cells: Vec<char> = row.chars().collect();
        let inside_rows = (area.y as usize)..(area.y + area.height) as usize;
        if !inside_rows.contains(&y) {
            assert!(
                row.trim().is_empty(),
                "row {y} outside strip area should be untouched, got: {row:?}"
            );
            continue;
        }
        for (x, ch) in cells.iter().enumerate() {
            if x < area.x as usize || x >= (area.x + area.width) as usize {
                assert_eq!(
                    *ch, ' ',
                    "cell ({x},{y}) outside strip area must stay blank, got {ch:?} in row {row:?}"
                );
            }
        }
    }
    let first_row = &rows[area.y as usize];
    assert!(
        !first_row.trim().is_empty(),
        "strip content should be written inside the area"
    );
}

#[test]
fn notification_full_draw_survives_overwide_swarm_plan_notice() {
    let _lock = viewport_snapshot_test_lock();
    let notice = "Swarm plan v3 · 12/24 tasks · gate 'critique-swarm-ui' blocked · \
                  reassigning 深度搜索エージェント → sheep-1 · awaiting verify-buffer-draw \
                  · retry budget 2/5 · ⚠ worker fox timed out"
        .to_string();

    for (width, height) in [(30_u16, 12_u16), (44, 16), (80, 24)] {
        clear_flicker_frame_history_for_tests();
        let state = TestState {
            display_messages: vec![DisplayMessage::assistant("plan running")],
            messages_version: 1,
            status_notice: Some(notice.clone()),
            ..Default::default()
        };
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        terminal
            .draw(|frame| crate::tui::ui::draw(frame, &state))
            .unwrap_or_else(|e| {
                panic!("over-wide notification draw failed at {width}x{height}: {e}");
            });

        let status_area = crate::tui::ui::last_status_area().expect("status area recorded");
        let rows = buffer_rows(&terminal);
        let notification_row = &rows[(status_area.y + 1) as usize];
        assert!(
            notification_row.contains("Swarm plan v3"),
            "expected notification cells below status line at {width}x{height}, got: {notification_row:?}"
        );
    }
}

/// Regression: the inline swarm strip must not oscillate against the dock.
///
/// The strip row grows the bottom chrome, so every appearance shoves the
/// transcript up one row. When the strip keyed off raw last-frame dock
/// visibility, the dock's natural placement churn (hidden-in-place blinks
/// while content scrolls under it) made the strip pop in and out every few
/// frames: visible up/down flicker. Now the stand-down is sticky: an anchored
/// (hidden-in-place) dock still counts as engaged, and disengagement is
/// debounced by a linger.
#[test]
fn swarm_strip_stands_down_through_dock_blinks() {
    let _lock = viewport_snapshot_test_lock();
    use crate::tui::info_widget::{
        WidgetKind, calculate_placements, swarm_strip_stands_down_for_dock,
    };
    crate::tui::info_widget::clear_widget_placements_for_tests();

    let mut coordinator = strip_member("s0", "researcher", "running");
    coordinator.role = Some("coordinator".to_string());
    let data = crate::tui::info_widget::InfoWidgetData {
        swarm_info: Some(crate::tui::info_widget::SwarmInfo {
            managed_members: vec![coordinator, strip_member("s1", "reviewer", "completed")],
            ..Default::default()
        }),
        ..Default::default()
    };
    let messages_area = Rect::new(0, 0, 120, 26);
    let wide_margins = crate::tui::info_widget::Margins {
        right_widths: vec![44; 26],
        ..Default::default()
    };
    // Zero free margin: the dock cannot render this frame (a wide line is
    // covering its slot), so it hides in place behind its anchor.
    let covered_margins = crate::tui::info_widget::Margins {
        right_widths: vec![0; 26],
        ..Default::default()
    };

    assert!(
        !swarm_strip_stands_down_for_dock(),
        "no dock engagement yet: strip should be free to show"
    );

    // Dock places: strip stands down.
    let placed = calculate_placements(messages_area, &wide_margins, &data);
    assert!(
        placed.iter().any(|p| p.kind == WidgetKind::SwarmStatus),
        "dock should place with a wide free margin"
    );
    assert!(
        swarm_strip_stands_down_for_dock(),
        "strip must stand down while the dock shows"
    );

    // Full-draw integration: with the dock engaged, ui::draw omits the strip
    // row above the status line (TestState's empty widget data means this
    // draw's own widget pass will clear the engagement afterwards, so this
    // must be checked before continuing the state-machine sequence).
    {
        let state = TestState {
            display_messages: vec![DisplayMessage::assistant("coordinating agents")],
            messages_version: 1,
            swarm_members: vec![
                strip_member("s0", "researcher", "running"),
                strip_member("s1", "reviewer", "completed"),
            ],
            ..Default::default()
        };
        clear_flicker_frame_history_for_tests();
        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        terminal
            .draw(|frame| crate::tui::ui::draw(frame, &state))
            .expect("draw with engaged dock should not panic");
        let status_area = crate::tui::ui::last_status_area().expect("status area recorded");
        let rows = buffer_rows(&terminal);
        let above_status = rows[..status_area.y as usize]
            .iter()
            .rev()
            .take(2)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            !above_status.contains("🐝") && !above_status.contains("researcher"),
            "strip must not render while the dock stands it down, got: {above_status:?}"
        );
    }

    // Re-engage (the integration draw above cleared state via its own empty
    // widget pass), then blink the dock hidden-in-place: anchor retained,
    // nothing placed. The strip must NOT pop back for the blink.
    crate::tui::info_widget::clear_widget_placements_for_tests();
    calculate_placements(messages_area, &wide_margins, &data);
    let blink = calculate_placements(messages_area, &covered_margins, &data);
    assert!(
        blink.iter().all(|p| p.kind != WidgetKind::SwarmStatus),
        "covered margin must hide the dock this frame"
    );
    assert!(
        swarm_strip_stands_down_for_dock(),
        "strip must keep standing down through a hidden-in-place dock blink"
    );

    // Even after the anchor is abandoned (hidden too long), the linger keeps
    // the strip down so a re-homing dock does not race a strip pop-in.
    for _ in 0..32 {
        calculate_placements(messages_area, &covered_margins, &data);
    }
    assert!(
        swarm_strip_stands_down_for_dock(),
        "strip must keep standing down through the post-disengage linger"
    );

    // A real teardown (widget pass skipped entirely) releases the stand-down.
    crate::tui::info_widget::note_widget_pass_skipped();
    assert!(
        !swarm_strip_stands_down_for_dock(),
        "strip should be free to return once the dock is genuinely gone"
    );
}

/// The swarm dock widget renders the compact summary at the cell level:
/// place it through the real `calculate_placements` + `render_all` path into
/// a TestBackend and assert the summary + progress bar landed inside the
/// placement rect.
#[test]
fn swarm_dock_widget_full_render_writes_agent_rows_in_margin() {
    let _lock = viewport_snapshot_test_lock();
    let mut coordinator = strip_member("s0", "researcher", "running");
    coordinator.role = Some("coordinator".to_string());
    coordinator.output_tail = Some("tracing the refresh path".to_string());
    let data = crate::tui::info_widget::InfoWidgetData {
        swarm_info: Some(crate::tui::info_widget::SwarmInfo {
            managed_members: vec![coordinator, strip_member("s1", "reviewer", "completed")],
            plan_progress: Some((3, 2, 7)),
            ..Default::default()
        }),
        ..Default::default()
    };

    let backend = TestBackend::new(120, 30);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    let messages_area = Rect::new(0, 0, 120, 26);
    let margins = crate::tui::info_widget::Margins {
        right_widths: vec![44; 26],
        left_widths: Vec::new(),
        centered: false,
        ..Default::default()
    };
    let mut dock_rect: Option<Rect> = None;
    terminal
        .draw(|frame| {
            let placements =
                crate::tui::info_widget::calculate_placements(messages_area, &margins, &data);
            dock_rect = placements
                .iter()
                .find(|p| p.kind == crate::tui::info_widget::WidgetKind::SwarmStatus)
                .map(|p| p.rect);
            crate::tui::info_widget::render_all(frame, &placements, &data);
        })
        .expect("dock widget render should not panic");

    let rect = dock_rect.expect("SwarmStatus dock should be placed with a wide free margin");
    let rows = buffer_rows(&terminal);
    let dock_text: String = rows[rect.y as usize..(rect.y + rect.height) as usize]
        .iter()
        .map(|row| {
            row.chars()
                .skip(rect.x as usize)
                .take(rect.width as usize)
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        dock_text.contains("1/2 agents"),
        "expected agents tally inside dock rect, got:\n{dock_text}"
    );
    assert!(
        dock_text.contains("nodes 3/7"),
        "expected node progress in dock header, got:\n{dock_text}"
    );
    assert!(
        dock_text.contains('▁'),
        "expected plan progress bar inside dock rect, got:\n{dock_text}"
    );
    // Nothing from the dock leaked left of its rect.
    for row in &rows[rect.y as usize..(rect.y + rect.height) as usize] {
        let left: String = row.chars().take(rect.x as usize).collect();
        assert!(
            left.trim().is_empty(),
            "dock must not write left of its rect, got: {left:?}"
        );
    }
}

#[test]
fn draw_notification_clips_overwide_notice_at_area_width() {
    let notice: String = "Swarm plan v3 · 12/24 tasks · gate blocked · ".repeat(8);
    let state = TestState {
        status_notice: Some(notice),
        ..Default::default()
    };

    let backend = TestBackend::new(60, 3);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    let area = Rect::new(2, 1, 20, 1);
    terminal
        .draw(|frame| input_ui::draw_notification(frame, &state, area))
        .expect("over-wide notification should clip, not panic");

    let rows = buffer_rows(&terminal);
    assert!(rows[0].trim().is_empty(), "row above area must be blank");
    assert!(rows[2].trim().is_empty(), "row below area must be blank");
    let cells: Vec<char> = rows[1].chars().collect();
    for (x, ch) in cells.iter().enumerate() {
        if x < area.x as usize || x >= (area.x + area.width) as usize {
            assert_eq!(
                *ch, ' ',
                "cell ({x},1) outside notification area must stay blank, got {ch:?}"
            );
        }
    }
    let inside: String = cells[area.x as usize..(area.x + area.width) as usize]
        .iter()
        .collect();
    assert!(
        inside.starts_with("Swarm plan v3"),
        "expected clipped notice text inside area, got: {inside:?}"
    );
}
