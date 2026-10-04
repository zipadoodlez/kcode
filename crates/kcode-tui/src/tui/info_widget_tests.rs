use super::{
    BackgroundInfo, CacheHitInfo, CacheMissAttribution, InfoWidgetData, Margins, UsageInfo,
    UsageProvider, WidgetKind, calculate_placements, effective_prompt_tokens,
    occasional_status_tip, render_kv_cache_widget, render_model_widget, render_usage_compact,
    render_usage_widget, truncate_smart,
};
use ratatui::layout::Rect;

#[test]
fn effective_prompt_tokens_handles_split_and_subset_accounting() {
    // Anthropic-style split accounting: `input` is only the uncached remainder,
    // so cache_read pushed beyond input means the true prompt is the sum.
    assert_eq!(effective_prompt_tokens(2449, 19499, 684), 22632);
    // OpenAI-style subset accounting: cached tokens are inside `input`.
    assert_eq!(effective_prompt_tokens(10000, 6000, 0), 10000);
    // No cache telemetry at all behaves like a plain input count.
    assert_eq!(effective_prompt_tokens(5000, 0, 0), 5000);
}

#[test]
fn cache_hit_ratio_uses_effective_prompt_for_split_providers() {
    // Mirrors a real Anthropic log line where read >> input and the old code
    // clamped the ratio to 100%.
    let cache = CacheHitInfo {
        reported_input_tokens: 2449,
        read_tokens: 19499,
        creation_tokens: 684,
        ..Default::default()
    };
    // 19499 / (2449 + 19499 + 684) = 0.8616...
    let ratio = cache.hit_ratio().expect("ratio");
    assert!((ratio - 0.8616).abs() < 0.01, "ratio was {ratio}");
}

#[test]
fn truncate_smart_handles_unicode() {
    let s = "eagle running - keep going";
    let out = truncate_smart(s, 15);
    assert_eq!(out, "eagle runnin...");
}

#[test]
fn occasional_status_tip_only_shows_during_part_of_cycle() {
    assert!(occasional_status_tip(60, 5).is_none());
    assert!(occasional_status_tip(60, 27).is_none());
    assert!(occasional_status_tip(60, 28).is_some());
    assert!(occasional_status_tip(60, 39).is_some());
    assert!(occasional_status_tip(60, 40).is_none());
    assert!(occasional_status_tip(60, 89).is_none());
}

#[test]
fn kv_cache_widget_shows_session_hit_ratio() {
    let data = InfoWidgetData {
        cache_hit_info: Some(CacheHitInfo {
            reported_input_tokens: 20_000,
            read_tokens: 15_000,
            creation_tokens: 3_000,
            optimal_input_tokens: 16_667,
            last_reported_input_tokens: Some(10_000),
            last_read_tokens: Some(9_400),
            last_creation_tokens: Some(0),
            last_optimal_input_tokens: Some(9_895),
            miss_attributions: vec![CacheMissAttribution {
                turn_number: 20,
                call_index: 1,
                missed_tokens: 69_000,
                reason: "provider switch".to_string(),
            }],
        }),
        ..Default::default()
    };

    assert!(data.has_data_for(WidgetKind::KvCache));
    let lines = render_kv_cache_widget(&data, Rect::new(0, 0, 40, 5));
    let text = lines_text(&lines);

    assert_eq!(lines.len(), 4);
    assert!(text.contains("KV cache:"));
    assert!(text.contains("yield "));
    assert!(text.contains("90%"));
    assert!(text.contains("last "));
    assert!(text.contains("94%"));
    assert!(text.contains("session "));
    assert!(text.contains("39%"));
    assert!(text.contains("miss attribution"));
    assert!(text.contains("69k missed total"));
    assert!(text.contains("20>"));
    assert!(text.contains("69k miss"));
    assert!(text.contains("provider switch"));
}

#[test]
fn cost_based_usage_widgets_show_price_and_tokens() {
    let usage = UsageInfo {
        provider: UsageProvider::CostBased,
        total_cost: 0.01234,
        input_tokens: 12_345,
        output_tokens: 678,
        available: true,
        ..Default::default()
    };
    let data = InfoWidgetData {
        usage_info: Some(usage.clone()),
        ..Default::default()
    };

    assert!(data.has_data_for(WidgetKind::UsageLimits));

    let expanded_text = lines_text(&render_usage_widget(&data, Rect::new(0, 0, 40, 4)));
    assert!(expanded_text.contains("$0.0123"));
    assert!(expanded_text.contains("12.3K in + 678 out"));

    let compact_text = lines_text(&render_usage_compact(&usage, 40, false));
    assert!(compact_text.contains("$0.0123"));
    assert!(compact_text.contains("12.3K in + 678 out"));
}

fn lines_text(lines: &[ratatui::text::Line<'_>]) -> String {
    lines
        .iter()
        .flat_map(|line| line.spans.iter())
        .map(|span| span.content.as_ref())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn overview_requires_multiple_sections() {
    let one_section = InfoWidgetData {
        model: Some("gpt-test".to_string()),
        ..Default::default()
    };
    assert!(!one_section.has_data_for(WidgetKind::Overview));

    let two_sections = InfoWidgetData {
        model: Some("gpt-test".to_string()),
        queue_mode: Some(true),
        ..Default::default()
    };
    assert!(two_sections.has_data_for(WidgetKind::Overview));
}

#[test]
fn overview_widget_is_placed_when_space_allows() {
    {
        let mut guard = super::get_or_init_state();
        if let Some(state) = guard.as_mut() {
            state.enabled = true;
            state.placements.clear();
            state.anchors.clear();
        }
    }

    let data = InfoWidgetData {
        model: Some("gpt-test".to_string()),
        queue_mode: Some(true),
        ..Default::default()
    };
    let margins = Margins {
        right_widths: vec![40; 20],
        left_widths: Vec::new(),
        centered: false,
        ..Default::default()
    };
    let placements = calculate_placements(Rect::new(0, 0, 80, 20), &margins, &data);
    assert!(
        placements.iter().any(|p| p.kind == WidgetKind::Overview),
        "expected overview widget placement"
    );
}

#[test]
fn workspace_widget_has_high_priority_when_enabled() {
    {
        let mut guard = super::get_or_init_state();
        if let Some(state) = guard.as_mut() {
            state.enabled = true;
            state.placements.clear();
            state.anchors.clear();
        }
    }

    let data = InfoWidgetData {
        workspace_rows: vec![crate::tui::workspace_map::VisibleWorkspaceRow {
            workspace: 0,
            is_current: true,
            focused_index: Some(0),
            sessions: vec![crate::tui::workspace_map::WorkspaceSessionTile::new("fox")],
        }],
        model: Some("gpt-test".to_string()),
        queue_mode: Some(true),
        ..Default::default()
    };

    let available = data.available_widgets();
    assert_eq!(available.first(), Some(&WidgetKind::WorkspaceMap));

    let margins = Margins {
        right_widths: vec![40; 20],
        left_widths: Vec::new(),
        centered: false,
        ..Default::default()
    };
    let placements = calculate_placements(Rect::new(0, 0, 80, 20), &margins, &data);
    assert_eq!(
        placements.first().map(|p| p.kind),
        Some(WidgetKind::WorkspaceMap)
    );
}

#[test]
fn model_widget_renders_connection_type() {
    let data = InfoWidgetData {
        model: Some("gpt-5.3-codex".to_string()),
        provider_name: Some("openai".to_string()),
        connection_type: Some("websocket".to_string()),
        ..Default::default()
    };
    let lines = render_model_widget(&data, Rect::new(0, 0, 40, 10));
    let text = lines
        .iter()
        .flat_map(|line| line.spans.iter())
        .map(|span| span.content.as_ref())
        .collect::<Vec<_>>()
        .join("\n")
        .to_lowercase();
    assert!(text.contains("websocket"));
}

#[test]
fn usage_pill_renders_filled_and_empty_segments() {
    let line = super::render_usage_pill(200_000, 1_000_000, 26);
    let text: String = line
        .spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect();

    assert!(text.contains('▰'), "expected filled pill segments: {text}");
    assert!(text.contains('▱'), "expected empty pill segments: {text}");
}

#[test]
fn usage_pill_renders_when_narrow() {
    let line = super::render_usage_pill(200_000, 1_000_000, 10);
    let text: String = line
        .spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect();

    assert!(
        text.contains('▰') || text.contains('▱'),
        "narrow bar should still render pill segments: {text}"
    );
}

#[test]
fn context_usage_line_shows_numeric_label_inside_bar() {
    let line = super::render_context_usage_line("Context", 50_000, 200_000, 40);
    let text: String = line
        .spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect();

    assert!(text.contains("Context"), "expected context label: {text}");
    assert!(
        text.contains("50k/200k"),
        "expected inline token label: {text}"
    );
}

#[test]
fn render_context_compact_prefers_observed_token_usage_for_label() {
    let data = InfoWidgetData {
        context_info: Some(crate::prompt::ContextInfo {
            total_chars: 400_000,
            ..Default::default()
        }),
        context_limit: Some(200_000),
        observed_context_tokens: Some(50_000),
        ..Default::default()
    };

    let lines = super::render_context_compact(&data, Rect::new(0, 0, 40, 1));
    let text: String = lines[0]
        .spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect();

    assert!(
        text.contains("50k/200k"),
        "expected observed token count: {text}"
    );
    assert!(
        !text.contains("100k/200k"),
        "should not fall back to char estimate when observed tokens exist: {text}"
    );
}

#[test]
fn render_context_compact_reports_updating_when_snapshot_is_stale() {
    let data = InfoWidgetData {
        context_info_stale: true,
        context_info: Some(crate::prompt::ContextInfo {
            total_chars: 400_000,
            ..Default::default()
        }),
        context_limit: Some(200_000),
        ..Default::default()
    };

    let lines = super::render_context_compact(&data, Rect::new(0, 0, 40, 1));
    let text: String = lines[0]
        .spans
        .iter()
        .map(|span| span.content.as_ref())
        .collect();

    assert!(
        text.contains("updating"),
        "expected updating marker: {text}"
    );
    assert!(
        !text.contains("100k/200k"),
        "stale snapshots must not render old usage as current: {text}"
    );
}

#[test]
fn background_widget_handles_empty_and_large_task_lists() {
    // No background info: renders nothing.
    let data = InfoWidgetData::default();
    assert!(super::render_background_widget(&data, Rect::new(0, 0, 40, 4)).is_empty());

    // running_count == 0: summary is suppressed even if stale task names linger.
    let info = BackgroundInfo {
        running_count: 0,
        running_tasks: vec!["stale".to_string()],
        ..Default::default()
    };
    assert!(super::render_background_compact(&info).is_empty());

    // Large task list: summary + 3 rows + overflow line, no panic at tiny width.
    let info = BackgroundInfo {
        running_count: 200,
        running_tasks: (0..200).map(|i| format!("task-{i}")).collect(),
        progress_detail: Some("42% · working".to_string()),
        ..Default::default()
    };
    let data = InfoWidgetData {
        background_info: Some(info.clone()),
        ..Default::default()
    };
    let lines = super::render_background_widget(&data, Rect::new(0, 0, 40, 8));
    assert_eq!(lines.len(), 5, "summary + 3 tasks + overflow");
    let text = lines_text(&lines);
    assert!(text.contains("200 running"), "got: {text}");
    assert!(text.contains("+197 more"), "got: {text}");
    // Zero-size rect must not panic (row width clamps to a minimum).
    let _ = super::render_background_widget(&data, Rect::new(0, 0, 0, 0));
}

#[test]
fn background_widget_and_compact_share_summary_format() {
    let info = BackgroundInfo {
        running_count: 4,
        running_tasks: vec![
            "selfdev build".to_string(),
            "train.py".to_string(),
            "cargo test".to_string(),
            "download".to_string(),
        ],
        progress_summary: Some("selfdev build".to_string()),
        progress_detail: Some("[#####-------] 42% · Building (parsed)".to_string()),
    };
    let data = InfoWidgetData {
        background_info: Some(info.clone()),
        ..Default::default()
    };

    let widget_text = lines_text(&super::render_background_widget(
        &data,
        Rect::new(0, 0, 40, 1),
    ));
    let compact_text = lines_text(&super::render_background_compact(&info));

    assert_eq!(widget_text, compact_text);
    assert!(widget_text.contains("Background"), "got: {widget_text}");
    assert!(widget_text.contains("4"), "got: {widget_text}");
    assert!(!widget_text.contains("mem:"), "got: {widget_text}");
    assert!(widget_text.contains("selfdev build"), "got: {widget_text}");
    assert!(widget_text.contains("train.py"), "got: {widget_text}");
    assert!(widget_text.contains("cargo test"), "got: {widget_text}");
    assert!(widget_text.contains("+1 more"), "got: {widget_text}");
    assert!(widget_text.contains("[#####-------]"), "got: {widget_text}");
}

#[test]
fn sticky_placement_clamps_width_to_current_margin() {
    {
        let mut guard = super::get_or_init_state();
        if let Some(state) = guard.as_mut() {
            state.enabled = true;
            state.placements.clear();
            state.anchors.clear();
        }
    }

    let data = InfoWidgetData {
        model: Some("gpt-test".to_string()),
        queue_mode: Some(true),
        ..Default::default()
    };
    let area = Rect::new(0, 0, 100, 10);

    // First frame places a wide widget.
    let first = calculate_placements(
        area,
        &Margins {
            right_widths: vec![30; 10],
            left_widths: Vec::new(),
            centered: false,
            ..Default::default()
        },
        &data,
    );
    assert!(!first.is_empty(), "expected initial placement");
    assert_eq!(first[0].rect.width, 30);

    // Second frame shrinks margin by 4 columns (within sticky tolerance).
    let second_margins = vec![26; 10];
    let second = calculate_placements(
        area,
        &Margins {
            right_widths: second_margins.clone(),
            left_widths: Vec::new(),
            centered: false,
            ..Default::default()
        },
        &data,
    );
    assert!(!second.is_empty(), "expected sticky placement");

    let p = &second[0];
    let row_start = p.rect.y.saturating_sub(area.y) as usize;
    let row_end = row_start + p.rect.height as usize;
    let min_margin = second_margins[row_start..row_end]
        .iter()
        .copied()
        .min()
        .unwrap_or(0);
    assert!(
        p.rect.width <= min_margin,
        "sticky width {} exceeded current margin {}",
        p.rect.width,
        min_margin
    );
}

#[test]
fn placements_never_include_border_only_widgets() {
    {
        let mut guard = super::get_or_init_state();
        if let Some(state) = guard.as_mut() {
            state.enabled = true;
            state.placements.clear();
            state.anchors.clear();
        }
    }

    let data = InfoWidgetData {
        model: Some("gpt-test".to_string()),
        session_count: Some(2),
        context_info: Some(crate::prompt::ContextInfo {
            system_prompt_chars: 24_000,
            total_chars: 40_000,
            ..Default::default()
        }),
        queue_mode: Some(true),
        background_info: Some(BackgroundInfo {
            running_count: 1,
            running_tasks: vec!["bash".to_string()],
            ..Default::default()
        }),
        usage_info: Some(UsageInfo {
            provider: UsageProvider::Anthropic,
            primary_limit_label: Some("5-hour".to_string()),
            five_hour: 0.35,
            secondary_limit_label: Some("Weekly".to_string()),
            seven_day: 0.62,
            available: true,
            ..Default::default()
        }),
        ..Default::default()
    };

    let placements = calculate_placements(
        Rect::new(0, 0, 100, 10),
        &Margins {
            right_widths: vec![40; 10],
            left_widths: Vec::new(),
            centered: false,
            ..Default::default()
        },
        &data,
    );

    assert!(
        placements.iter().all(|p| p.rect.height > 2),
        "found border-only widget placement: {:?}",
        placements
    );
}

/// The compact overview height must match its rendered line count. A mismatch
/// either clips the last section (background tasks render last) or reserves
/// blank rows.
#[test]
fn compact_page_height_estimate_matches_rendered_lines() {
    let data = InfoWidgetData {
        model: Some("claude-test-1".to_string()),
        provider_name: Some("anthropic".to_string()),
        session_count: Some(2),
        context_info: Some(crate::prompt::ContextInfo {
            system_prompt_chars: 10_000,
            total_chars: 30_000,
            ..Default::default()
        }),
        background_info: Some(BackgroundInfo {
            running_count: 2,
            running_tasks: vec!["bash".to_string(), "task".to_string()],
            ..Default::default()
        }),
        usage_info: Some(UsageInfo {
            provider: UsageProvider::Anthropic,
            primary_limit_label: Some("5-hour".to_string()),
            five_hour: 0.3,
            secondary_limit_label: Some("Weekly".to_string()),
            seven_day: 0.5,
            available: true,
            ..Default::default()
        }),
        cache_hit_info: Some(CacheHitInfo {
            reported_input_tokens: 1_000,
            read_tokens: 800,
            ..Default::default()
        }),
        ..Default::default()
    };

    let inner = Rect::new(0, 0, 38, 30);
    let height = super::overview_height(&data, inner.height);
    let lines = super::render_sections(&data, inner);
    assert_eq!(
        lines.len() as u16,
        height,
        "overview height must match its rendered line count"
    );
}

/// The same check for a cost-based (API key) provider, whose usage section
/// renders a single line.
#[test]
fn compact_page_height_matches_for_cost_based_usage() {
    let data = InfoWidgetData {
        model: Some("gpt-test".to_string()),
        background_info: Some(BackgroundInfo {
            running_count: 1,
            running_tasks: vec!["bash".to_string()],
            ..Default::default()
        }),
        usage_info: Some(UsageInfo {
            provider: UsageProvider::CostBased,
            total_cost: 0.42,
            input_tokens: 10_000,
            output_tokens: 2_000,
            available: true,
            ..Default::default()
        }),
        ..Default::default()
    };

    let inner = Rect::new(0, 0, 38, 30);
    let height = super::overview_height(&data, inner.height);
    let lines = super::render_sections(&data, inner);
    assert_eq!(lines.len() as u16, height);
}
