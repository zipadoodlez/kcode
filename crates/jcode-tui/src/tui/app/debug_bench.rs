use super::*;

impl App {
    pub(in crate::tui::app) fn build_scroll_test_content(
        sections: usize,
        padding: usize,
        _override_text: Option<&str>,
    ) -> String {
        let mut out = String::new();
        let intro_lines = padding.max(4);
        for i in 0..intro_lines {
            out.push_str(&format!(
                "Intro line {:02} - quick brown fox jumps over the lazy dog.\n",
                i + 1
            ));
        }

        for idx in 0..sections {
            for j in 0..padding {
                out.push_str(&format!(
                    "Section {} line {:02} - stretch content for scrolling.\n",
                    idx + 1,
                    j + 1
                ));
            }
        }

        out
    }

    fn build_side_panel_latency_snapshot(
        sections: usize,
        padding: usize,
    ) -> crate::side_panel::SidePanelSnapshot {
        let content = Self::build_scroll_test_content(sections, padding, None);
        crate::side_panel::SidePanelSnapshot {
            focus_revision: 0,
            focused_page_id: Some("latency_bench".to_string()),
            pages: vec![crate::side_panel::SidePanelPage {
                id: "latency_bench".to_string(),
                title: "Latency Bench".to_string(),
                file_path: "latency_bench.md".to_string(),
                format: crate::side_panel::SidePanelPageFormat::Markdown,
                pdf_data: None,
                source: crate::side_panel::SidePanelPageSource::Managed,
                content,
                updated_at_ms: 1,
            }],
        }
    }

    pub(in crate::tui::app) fn run_side_panel_latency_bench(
        &mut self,
        raw: Option<&str>,
    ) -> String {
        let cfg: SidePanelLatencyConfig = if let Some(raw) = raw {
            if raw.trim().is_empty() {
                SidePanelLatencyConfig {
                    width: None,
                    height: None,
                    iterations: None,
                    warmup_iterations: None,
                    padding: None,
                    sections: None,
                    include_samples: None,
                }
            } else {
                match serde_json::from_str(raw) {
                    Ok(cfg) => cfg,
                    Err(e) => return format!("side-panel-latency parse error: {}", e),
                }
            }
        } else {
            SidePanelLatencyConfig {
                width: None,
                height: None,
                iterations: None,
                warmup_iterations: None,
                padding: None,
                sections: None,
                include_samples: None,
            }
        };

        let width = cfg.width.unwrap_or(100).max(40);
        let height = cfg.height.unwrap_or(40).max(20);
        let iterations = cfg.iterations.unwrap_or(40).clamp(4, 400);
        let warmup_iterations = cfg.warmup_iterations.unwrap_or(6).min(50);
        let padding = cfg.padding.unwrap_or(24).max(8);
        let sections = cfg.sections.unwrap_or(2).clamp(1, 3);
        let include_samples = cfg.include_samples.unwrap_or(true);

        let saved_state = ScrollTestState::capture(self);
        let was_visual_debug = crate::tui::visual_debug::is_enabled();
        crate::tui::visual_debug::enable();

        self.display_messages = vec![
            DisplayMessage {
                role: "user".to_string(),
                content: "Headless side-panel latency benchmark".to_string(),
                tool_calls: vec![],
                duration_secs: None,
                title: None,
                tool_data: None,
            },
            DisplayMessage {
                role: "assistant".to_string(),
                content: "Benchmarking side-panel input latency.".to_string(),
                tool_calls: vec![],
                duration_secs: None,
                title: None,
                tool_data: None,
            },
        ];
        self.bump_display_messages_version();
        self.side_panel = Self::build_side_panel_latency_snapshot(sections, padding);
        self.diff_mode = crate::config::DiffDisplayMode::Off;
        self.diff_pane_scroll = 0;
        self.diff_pane_scroll_x = 0;
        self.diff_pane_focus = false;
        self.diff_pane_auto_scroll = false;
        self.follow_chat_bottom();
        self.is_processing = false;
        self.clear_streaming_render_state();
        self.queued_messages.clear();
        self.interleave_message = None;
        self.interleave_images.clear();
        self.pending_soft_interrupts.clear();
        self.status = ProcessingStatus::Idle;
        self.processing_started = None;
        self.status_notice = None;

        use crossterm::event::{KeyModifiers, MouseEvent, MouseEventKind};
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;

        let result = (|| -> Result<serde_json::Value, String> {
            let backend = TestBackend::new(width, height);
            let mut terminal = Terminal::new(backend)
                .map_err(|e| format!("side-panel-latency terminal error: {}", e))?;

            terminal
                .draw(|f| crate::tui::ui::draw(f, self))
                .map_err(|e| format!("side-panel-latency baseline draw error: {}", e))?;

            let diff_area = crate::tui::ui::last_layout_snapshot()
                .and_then(|layout| layout.diff_pane_area)
                .ok_or_else(|| "side-panel-latency: diff pane area missing".to_string())?;
            let total_lines = crate::tui::ui::pinned_pane_total_lines();
            let max_scroll = total_lines.saturating_sub(diff_area.height as usize);
            if max_scroll == 0 {
                return Err("side-panel-latency: side panel did not become scrollable".to_string());
            }

            self.diff_pane_scroll = max_scroll / 2;
            terminal
                .draw(|f| crate::tui::ui::draw(f, self))
                .map_err(|e| format!("side-panel-latency mid draw error: {}", e))?;

            let center_x = diff_area.x + diff_area.width / 2;
            let center_y = diff_area.y + diff_area.height / 2;
            let total_runs = warmup_iterations + iterations;
            let mut samples: Vec<SidePanelLatencySample> = Vec::with_capacity(iterations);
            let mut latency_values: Vec<f64> = Vec::with_capacity(iterations);
            let mut render_values: Vec<f64> = Vec::with_capacity(iterations);
            let mut scroll_only_count = 0usize;
            let mut unchanged_scroll_count = 0usize;

            for idx in 0..total_runs {
                let direction = if idx % 2 == 0 { "down" } else { "up" };
                let kind = if idx % 2 == 0 {
                    MouseEventKind::ScrollDown
                } else {
                    MouseEventKind::ScrollUp
                };
                let before_frame = crate::tui::visual_debug::latest_frame();
                let before_frame_id = before_frame.as_ref().map(|frame| frame.frame_id);
                let scroll_before = if self.diff_pane_scroll == usize::MAX {
                    crate::tui::ui::last_diff_pane_effective_scroll()
                } else {
                    self.diff_pane_scroll
                };
                let started = Instant::now();
                let scroll_only = self.handle_mouse_event(MouseEvent {
                    kind,
                    column: center_x,
                    row: center_y,
                    modifiers: KeyModifiers::empty(),
                });
                if scroll_only {
                    scroll_only_count += 1;
                    std::thread::sleep(crate::tui::tick_period(self));
                }
                terminal
                    .draw(|f| crate::tui::ui::draw(f, self))
                    .map_err(|e| format!("side-panel-latency draw error: {}", e))?;
                let latency_ms = started.elapsed().as_secs_f64() * 1000.0;
                let after_frame = crate::tui::visual_debug::latest_frame();
                let after_frame_id = after_frame.as_ref().map(|frame| frame.frame_id);
                let scroll_after = crate::tui::ui::last_diff_pane_effective_scroll();
                let scroll_changed = scroll_after != scroll_before;
                if !scroll_changed {
                    unchanged_scroll_count += 1;
                }
                let render_ms = after_frame
                    .as_ref()
                    .and_then(|frame| frame.render_timing.as_ref().map(|timing| timing.total_ms));

                if idx >= warmup_iterations {
                    latency_values.push(latency_ms);
                    if let Some(render_ms) = render_ms {
                        render_values.push(render_ms as f64);
                    }
                    samples.push(SidePanelLatencySample {
                        iteration: idx - warmup_iterations,
                        direction,
                        scroll_only,
                        latency_ms,
                        render_ms,
                        scroll_before,
                        scroll_after,
                        frame_id_before: before_frame_id,
                        frame_id_after: after_frame_id,
                        scroll_changed,
                    });
                }
            }

            let mut sorted_latencies = latency_values.clone();
            sorted_latencies.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let mut sorted_render = render_values.clone();
            sorted_render.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

            Ok(serde_json::json!({
                "ok": scroll_only_count == 0 && unchanged_scroll_count == 0,
                "config": {
                    "width": width,
                    "height": height,
                    "iterations": iterations,
                    "warmup_iterations": warmup_iterations,
                    "padding": padding,
                    "sections": sections,
                },
                "summary": {
                    "samples": latency_values.len(),
                    "scroll_only_count": scroll_only_count,
                    "unchanged_scroll_count": unchanged_scroll_count,
                    "max_scroll": max_scroll,
                    "latency_ms": {
                        "p50": percentile_ms(&sorted_latencies, 0.50),
                        "p95": percentile_ms(&sorted_latencies, 0.95),
                        "p99": percentile_ms(&sorted_latencies, 0.99),
                        "max": sorted_latencies.last().copied().unwrap_or(0.0),
                        "avg": if latency_values.is_empty() { 0.0 } else { latency_values.iter().sum::<f64>() / latency_values.len() as f64 },
                    },
                    "render_ms": {
                        "p50": percentile_ms(&sorted_render, 0.50),
                        "p95": percentile_ms(&sorted_render, 0.95),
                        "p99": percentile_ms(&sorted_render, 0.99),
                        "max": sorted_render.last().copied().unwrap_or(0.0),
                        "avg": if render_values.is_empty() { 0.0 } else { render_values.iter().sum::<f64>() / render_values.len() as f64 },
                    }
                },
                "samples": if include_samples { serde_json::to_value(&samples).unwrap_or(serde_json::Value::Null) } else { serde_json::Value::Null },
                "notes": [
                    "This is a headless end-to-end app benchmark: injected side-panel mouse scroll event -> event classification -> redraw scheduling -> offscreen frame update.",
                    "It does not include terminal emulator/compositor/image protocol wall-clock paint latency outside jcode."
                ]
            }))
        })();

        saved_state.restore(self);
        if !was_visual_debug {
            crate::tui::visual_debug::disable();
        }

        match result {
            Ok(value) => serde_json::to_string_pretty(&value).unwrap_or_else(|_| "{}".to_string()),
            Err(e) => e,
        }
    }

    /// Measure how much the info widgets move/flicker while scrolling the *current*
    /// transcript. Renders the live app over an offscreen backend, advances the
    /// scroll position one content line at a time, captures the resulting widget
    /// placements, and runs the shared stability analyzer.
    pub(in crate::tui::app) fn run_widget_stability(&mut self, raw: Option<&str>) -> String {
        use crate::tui::info_widget_stability::{PlacedRect, intern_kind};
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;

        let cfg: WidgetStabilityConfig = match raw {
            Some(raw) if !raw.trim().is_empty() => match serde_json::from_str(raw) {
                Ok(cfg) => cfg,
                Err(e) => return format!("widget-stability parse error: {}", e),
            },
            _ => WidgetStabilityConfig::default(),
        };

        let width = cfg.width.unwrap_or(120).max(40);
        let height = cfg.height.unwrap_or(40).max(20);
        let step = cfg.step.unwrap_or(1).max(1);
        let max_frames = cfg.max_frames.unwrap_or(160).clamp(4, 2000);
        let include_frames = cfg.include_frames.unwrap_or(false);

        let saved_state = ScrollTestState::capture(self);
        let was_visual_debug = crate::tui::visual_debug::is_enabled();
        crate::tui::visual_debug::enable();

        let backend = TestBackend::new(width, height);
        let mut terminal = match Terminal::new(backend) {
            Ok(t) => t,
            Err(e) => {
                saved_state.restore(self);
                if !was_visual_debug {
                    crate::tui::visual_debug::disable();
                }
                return format!("widget-stability terminal error: {}", e);
            }
        };

        // Establish total content height from a baseline (bottom) render.
        self.follow_chat_bottom();
        let mut errors: Vec<String> = Vec::new();
        if let Err(e) = terminal.draw(|f| crate::tui::ui::draw(f, self)) {
            errors.push(format!("baseline draw error: {}", e));
        }
        let baseline = crate::tui::visual_debug::latest_frame();
        let (visible_height, total_lines) = if let Some(frame) = baseline.as_ref() {
            let vh = frame
                .layout
                .messages_area
                .map(|r| r.height as usize)
                .unwrap_or(height as usize);
            (vh, frame.layout.estimated_content_height.max(1))
        } else {
            (height as usize, 1usize)
        };
        let max_scroll = total_lines.saturating_sub(visible_height);

        // Walk from top to bottom one (or `step`) content lines at a time, recording
        // the widget placements at each scroll position.
        let mut frames: Vec<Vec<PlacedRect>> = Vec::new();
        // Absolute transcript line shown on the first visible row of each frame, so
        // the analyzer can subtract the scroll-ride and report content-relative
        // travel (how much widgets move *relative to the text* they sit beside).
        let mut scroll_tops_abs: Vec<i64> = Vec::new();
        let mut frame_payloads: Vec<serde_json::Value> = Vec::new();
        self.auto_scroll_paused = true;

        let mut scroll_top = 0usize;
        while scroll_top <= max_scroll && frames.len() < max_frames {
            let offset = max_scroll.saturating_sub(scroll_top);
            self.scroll_offset = offset;
            if let Err(e) = terminal.draw(|f| crate::tui::ui::draw(f, self)) {
                errors.push(format!("draw error at scroll_top {}: {}", scroll_top, e));
                break;
            }
            scroll_tops_abs.push(crate::tui::ui::last_resolved_chat_scroll() as i64);
            let placed: Vec<PlacedRect> = match crate::tui::visual_debug::latest_frame() {
                Some(frame) => frame
                    .info_widgets
                    .as_ref()
                    .map(|info| {
                        info.placements
                            .iter()
                            .map(|p| PlacedRect {
                                kind: intern_kind(&p.kind),
                                x: p.rect.x,
                                y: p.rect.y,
                                width: p.rect.width,
                                height: p.rect.height,
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
                None => Vec::new(),
            };
            if include_frames {
                frame_payloads.push(serde_json::json!({
                    "scroll_top": scroll_top,
                    "widgets": placed.iter().map(|p| serde_json::json!({
                        "kind": p.kind,
                        "x": p.x,
                        "y": p.y,
                        "width": p.width,
                        "height": p.height,
                    })).collect::<Vec<_>>(),
                }));
            }
            frames.push(placed);

            if scroll_top == max_scroll {
                break;
            }
            scroll_top = (scroll_top + step).min(max_scroll);
        }

        let report = crate::tui::info_widget_stability::analyze_frames_with_scroll(
            &frames,
            &scroll_tops_abs,
        );

        saved_state.restore(self);
        if !was_visual_debug {
            crate::tui::visual_debug::disable();
        }

        let out = serde_json::json!({
            "ok": errors.is_empty(),
            "config": {
                "width": width,
                "height": height,
                "step": step,
                "max_frames": max_frames,
            },
            "layout": {
                "total_lines": total_lines,
                "visible_height": visible_height,
                "max_scroll": max_scroll,
            },
            "report": report,
            "frames": if include_frames { serde_json::Value::Array(frame_payloads) } else { serde_json::Value::Null },
            "errors": errors,
            "notes": [
                "Scrolls the current transcript one content line at a time over an offscreen backend.",
                "travel_per_100_lines = total widget x+y movement per 100 scroll lines (lower is calmer).",
                "content_travel_per_100_lines = movement RELATIVE TO THE TRANSCRIPT (scroll-ride subtracted); ~0 means widgets stick to one negative-space spot and just scroll along.",
                "flicker_per_100_lines = widget appear/disappear transitions per 100 scroll lines.",
                "distraction_per_100_lines = travel + weighted flicker; the single headline number."
            ],
        });

        serde_json::to_string_pretty(&out).unwrap_or_else(|_| "{}".to_string())
    }
}
