use super::*;

impl App {
    pub(in crate::tui::app) fn debug_picker_state_json(
        &self,
        visible_limit: Option<usize>,
    ) -> String {
        let Some(picker) = self.inline_interactive_state.as_ref() else {
            return serde_json::json!({
                "open": false,
                "input": self.input,
                "cursor_pos": self.cursor_pos,
            })
            .to_string();
        };

        let filtered_count = picker.filtered.len();
        let total_count = picker.entries.len();
        let list_height = visible_limit.unwrap_or_else(|| filtered_count.min(40));
        let selected = picker.selected.min(filtered_count.saturating_sub(1));
        let start = if filtered_count == 0 || list_height == 0 {
            0
        } else {
            let half = list_height / 2;
            if selected <= half {
                0
            } else if selected + list_height - half > filtered_count {
                filtered_count.saturating_sub(list_height)
            } else {
                selected - half
            }
        };
        let end = if list_height == 0 {
            start
        } else {
            (start + list_height).min(filtered_count)
        };

        let rows: Vec<serde_json::Value> = picker
            .filtered
            .iter()
            .enumerate()
            .skip(start)
            .take(end.saturating_sub(start))
            .filter_map(|(visible_index, entry_index)| {
                let entry = picker.entries.get(*entry_index)?;
                let route = entry.active_option();
                let filter_text = picker.filter_text(entry);
                Some(serde_json::json!({
                    "visible_index": visible_index,
                    "entry_index": entry_index,
                    "selected": visible_index == selected,
                    "name": entry.name,
                    "provider": route.map(|r| r.provider.as_str()).unwrap_or(""),
                    "api_method": route.map(|r| r.api_method.as_str()).unwrap_or(""),
                    "available": route.map(|r| r.available).unwrap_or(false),
                    "detail": route.map(|r| r.detail.as_str()).unwrap_or(""),
                    "filter_text": filter_text,
                    "fuzzy_score": if picker.filter.is_empty() {
                        None
                    } else {
                        Self::picker_fuzzy_score(&picker.filter, &filter_text)
                    },
                    "recommended": entry.recommended,
                    "current": entry.is_current,
                    "default": entry.is_default,
                    "old": entry.old,
                    "created_date": entry.created_date,
                }))
            })
            .collect();

        serde_json::to_string_pretty(&serde_json::json!({
            "open": true,
            "kind": format!("{:?}", picker.kind),
            "preview": picker.preview,
            "input": self.input,
            "cursor_pos": self.cursor_pos,
            "filter": picker.filter,
            "selected": picker.selected,
            "column": picker.column,
            "total_count": total_count,
            "filtered_count": filtered_count,
            "visible_start": start,
            "visible_end": end,
            "visible_count": rows.len(),
            "visible_limit": visible_limit,
            "rows": rows,
        }))
        .unwrap_or_else(|_| "{}".to_string())
    }

    pub(in crate::tui::app) fn handle_debug_command(&mut self, cmd: &str) -> String {
        let cmd = cmd.trim();
        if cmd == "frame" {
            return self.handle_debug_command("screen-json");
        }
        if cmd == "frame-normalized" {
            return self.handle_debug_command("screen-json-normalized");
        }
        if cmd == "enable" || cmd == "debug-enable" {
            crate::tui::visual_debug::enable();
            return "Visual debugging enabled.".to_string();
        }
        if cmd == "disable" || cmd == "debug-disable" {
            crate::tui::visual_debug::disable();
            return "Visual debugging disabled.".to_string();
        }
        if cmd == "status" {
            let enabled = crate::tui::visual_debug::is_enabled();
            let overlay = crate::tui::visual_debug::overlay_enabled();
            return serde_json::json!({
                "visual_debug_enabled": enabled,
                "visual_debug_overlay": overlay
            })
            .to_string();
        }
        if cmd == "stream-jitter" {
            // Arrival-vs-reveal smoothness report for the paced stream buffer.
            // `reveals.bucket_100ms_cv` well below `arrivals.bucket_100ms_cv`
            // means pacing is smoothing provider bursts (text and reasoning).
            return serde_json::to_string_pretty(&self.stream_buffer.jitter_profile())
                .unwrap_or_else(|_| "{}".to_string());
        }
        if cmd == "stream-jitter:reset" {
            self.stream_buffer.reset_jitter();
            return "OK: stream jitter stats reset".to_string();
        }
        if cmd == "smoothness" {
            // Anchor-stability report: jarring transcript motion (repositions,
            // insertions above, big pops, blinks, mass reflows) per rendered
            // frame, with expected motion (scroll/resize/tail-follow) excluded.
            return crate::tui::ui::smoothness_report_json();
        }
        if cmd == "smoothness:reset" {
            crate::tui::ui::smoothness_reset();
            return "OK: smoothness stats reset".to_string();
        }
        if cmd == "overlay" || cmd == "overlay:status" {
            let overlay = crate::tui::visual_debug::overlay_enabled();
            return serde_json::json!({
                "visual_debug_overlay": overlay
            })
            .to_string();
        }
        if cmd == "overlay:on" || cmd == "overlay:enable" {
            crate::tui::visual_debug::set_overlay(true);
            return "Visual debug overlay enabled.".to_string();
        }
        if cmd == "overlay:off" || cmd == "overlay:disable" {
            crate::tui::visual_debug::set_overlay(false);
            return "Visual debug overlay disabled.".to_string();
        }
        if cmd.starts_with("message:") {
            let msg = cmd.strip_prefix("message:").unwrap_or("");
            // Inject the message respecting queue mode (like keyboard Enter)
            self.input = msg.to_string();
            match self.send_action(false) {
                SendAction::Submit => {
                    self.submit_input();
                    self.debug_trace
                        .record("message", format!("submitted:{}", msg));
                    format!("OK: submitted message '{}'", msg)
                }
                SendAction::Queue => {
                    self.queue_message();
                    self.debug_trace
                        .record("message", format!("queued:{}", msg));
                    format!(
                        "OK: queued message '{}' (will send after current turn)",
                        msg
                    )
                }
                SendAction::Interleave => {
                    let prepared = input::take_prepared_input(self);
                    input::stage_local_interleave(self, prepared.expanded, prepared.images);
                    self.debug_trace
                        .record("message", format!("interleave:{}", msg));
                    format!("OK: interleave message '{}' (injecting now)", msg)
                }
            }
        } else if cmd == "reload" {
            // Trigger reload
            self.input = "/reload".to_string();
            self.submit_input();
            self.debug_trace.record("reload", "triggered".to_string());
            "OK: reload triggered".to_string()
        } else if cmd == "state" {
            // Return current state as JSON for easier parsing
            serde_json::json!({
                "processing": self.is_processing,
                "messages": self.messages.len(),
                "display_messages": self.display_messages.len(),
                "input": self.input,
                "cursor_pos": self.cursor_pos,
                "scroll_offset": self.scroll_offset,
                "queued_messages": self.queued_messages.len(),
                "provider_session_id": self.provider_session_id,
                "model": self.provider.name(),
                "version": jcode_build_meta::version(),
            })
            .to_string()
        } else if cmd.starts_with("mouse:") {
            // Inject a raw mouse event: mouse:<kind>:<col>,<row>
            // kind: down|up|drag|click (click = down then up at same cell)
            let raw = cmd.strip_prefix("mouse:").unwrap_or("");
            let (kind, coords) = match raw.split_once(':') {
                Some(pair) => pair,
                None => return "mouse error: expected mouse:<kind>:<col>,<row>".to_string(),
            };
            let (col, row) = match coords.split_once(',').and_then(|(c, r)| {
                Some((c.trim().parse::<u16>().ok()?, r.trim().parse::<u16>().ok()?))
            }) {
                Some(pair) => pair,
                None => return "mouse error: bad coords (expected <col>,<row>)".to_string(),
            };
            use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
            let mut inject = |kind: MouseEventKind| {
                self.handle_mouse_event(MouseEvent {
                    kind,
                    column: col,
                    row,
                    modifiers: crossterm::event::KeyModifiers::empty(),
                })
            };
            match kind {
                "down" => {
                    inject(MouseEventKind::Down(MouseButton::Left));
                }
                "up" => {
                    inject(MouseEventKind::Up(MouseButton::Left));
                }
                "drag" => {
                    inject(MouseEventKind::Drag(MouseButton::Left));
                }
                "click" => {
                    inject(MouseEventKind::Down(MouseButton::Left));
                    inject(MouseEventKind::Up(MouseButton::Left));
                }
                // A real kitty click with sub-cell hand jitter: kitty reports
                // motion at pixel granularity, so Down, Drag (same cell), Up.
                "jitter-click" => {
                    inject(MouseEventKind::Down(MouseButton::Left));
                    inject(MouseEventKind::Drag(MouseButton::Left));
                    inject(MouseEventKind::Up(MouseButton::Left));
                }
                other => return format!("mouse error: unknown kind '{other}'"),
            }
            self.debug_trace
                .record("mouse", format!("{kind} at {col},{row}"));
            format!(
                "OK: mouse {kind} at {col},{row} (status: {:?})",
                self.status_notice.as_ref().map(|(text, _)| text.as_str())
            )
        } else if cmd == "expand-badge-fixture" {
            let old_string = (0..24)
                .map(|idx| format!("old fixture line {idx}\n"))
                .collect::<String>();
            let new_string = (0..24)
                .map(|idx| format!("new fixture line {idx}\n"))
                .collect::<String>();
            self.display_messages = vec![
                DisplayMessage::user("please edit demo.txt"),
                DisplayMessage::tool(
                    "Edited demo.txt".to_string(),
                    crate::message::ToolCall {
                        id: "debug_expand_edit_1".to_string(),
                        name: "edit".to_string(),
                        input: serde_json::json!({
                            "file_path": "demo.txt",
                            "old_string": old_string,
                            "new_string": new_string,
                        }),
                        intent: None,
                        thought_signature: None,
                    },
                ),
            ];
            self.bump_display_messages_version();
            self.diff_mode = crate::config::DiffDisplayMode::Inline;
            self.scroll_offset = 0;
            self.auto_scroll_paused = false;
            self.input.clear();
            self.cursor_pos = 0;
            self.set_status_notice("Debug expand badge fixture ready");
            serde_json::json!({
                "ok": true,
                "diff_mode": format!("{:?}", self.diff_mode),
                "display_edit_tool_message_count": self.display_edit_tool_message_count,
                "input": self.input,
            })
            .to_string()
        } else if cmd == "expand-badge-state" {
            serde_json::json!({
                "diff_mode": format!("{:?}", self.diff_mode),
                "display_edit_tool_message_count": self.display_edit_tool_message_count,
                "input": self.input,
                "cursor_pos": self.cursor_pos,
                "status_notice": self.status_notice.as_ref().map(|(text, _)| text),
                "display_messages": self.display_messages.len(),
            })
            .to_string()
        } else if cmd == "picker" || cmd == "picker:state" {
            self.debug_picker_state_json(None)
        } else if cmd == "model-picker" || cmd == "model-picker:live" {
            self.debug_model_picker_live_json(None)
        } else if let Some(raw) = cmd.strip_prefix("model-picker:") {
            let raw = raw.trim();
            if raw == "live" || raw == "state" {
                self.debug_model_picker_live_json(None)
            } else if let Some(limit_raw) = raw.strip_prefix("live:") {
                self.debug_model_picker_live_json(limit_raw.trim().parse::<usize>().ok())
            } else {
                self.debug_model_picker_live_json(raw.parse::<usize>().ok())
            }
        } else if let Some(raw) = cmd.strip_prefix("picker:") {
            let raw = raw.trim();
            let limit = raw.parse::<usize>().ok();
            self.debug_picker_state_json(limit)
        } else if let Some(raw) = cmd.strip_prefix("swarm-gallery:") {
            // Debug-only: inject synthetic inline swarm members and force the
            // inline gallery active so the band can be captured in a frame.
            // Format: swarm-gallery:<N>  (N synthetic agents), or
            //         swarm-gallery:off  (clear injected members + force flag).
            let raw = raw.trim();
            if raw == "off" {
                self.debug_force_inline_gallery = false;
                self.remote_swarm_members.clear();
                "OK: inline swarm gallery cleared".to_string()
            } else {
                let n: usize = raw.parse().unwrap_or(3);
                let statuses = ["running", "thinking", "ready", "completed", "blocked"];
                let names = [
                    "fox", "owl", "bee", "elk", "ant", "cat", "dog", "jay", "ram", "yak", "ox",
                    "emu",
                ];
                let samples = [
                    "Editing crates/jcode-tui/src/tui/ui.rs\n  carving the gallery band off chat_area",
                    "Thinking about how to wire the bus tap\n  into the streaming loop without",
                    "Running cargo build --profile selfdev\n  Compiling jcode-app-core",
                    "Done: 4 tests passed, committed.",
                    "Waiting on coordinator approval for plan",
                ];
                self.remote_swarm_members = (0..n)
                    .map(|i| crate::protocol::SwarmMemberStatus {
                        session_id: format!("session_{:02}", i),
                        friendly_name: Some(names[i % names.len()].to_string()),
                        status: statuses[i % statuses.len()].to_string(),
                        detail: Some(format!("task {}", i + 1)),
                        task_label: None,
                        role: if i == 0 {
                            Some("coordinator".to_string())
                        } else {
                            Some("agent".to_string())
                        },
                        is_headless: Some(i != 0),
                        live_attachments: Some(1),
                        status_age_secs: Some((i as u64) * 7),
                        output_tail: Some(samples[i % samples.len()].to_string()),
                        report_back_to_session_id: None,
                        todo_progress: Some(((i as u32 * 3) % 9, 9)),
                        todo_items: (0..5)
                            .map(|t| {
                                let status = if (t as u32) < (i as u32 * 3) % 9 {
                                    "completed".to_string()
                                } else if t as u32 == (i as u32 * 3) % 9 {
                                    "in_progress".to_string()
                                } else {
                                    "pending".to_string()
                                };
                                let tool_intents = if status == "in_progress" {
                                    vec![
                                        crate::protocol::SwarmToolIntent {
                                            tool_call_id: String::new(),
                                            tool_name: "agentgrep".into(),
                                            intent: "Locate the affected rendering path".into(),
                                            status: "completed".into(),
                                            progress: None,
                                        },
                                        crate::protocol::SwarmToolIntent {
                                            tool_call_id: String::new(),
                                            tool_name: "read".into(),
                                            intent: "Inspect the active todo state".into(),
                                            status: "completed".into(),
                                            progress: None,
                                        },
                                        crate::protocol::SwarmToolIntent {
                                            tool_call_id: String::new(),
                                            tool_name: "bash".into(),
                                            intent: "Run targeted swarm card tests".into(),
                                            status: "running".into(),
                                            progress: Some(crate::protocol::SwarmToolProgress {
                                                current: 27,
                                                total: 43,
                                                unit: Some("tests".into()),
                                            }),
                                        },
                                    ]
                                } else {
                                    Vec::new()
                                };
                                crate::protocol::SwarmTodoItem {
                                    content: format!("step {} of synthetic plan", t + 1),
                                    status,
                                    tool_intents,
                                }
                            })
                            .collect(),
                        runtime: crate::protocol::SwarmMemberRuntime {
                            model: Some("gpt-5.6".into()),
                            provider: Some("OpenAI".into()),
                            auth_method: Some("OAuth".into()),
                            effort: Some("high".into()),
                            elapsed_secs: Some(18),
                        },
                    })
                    .collect();
                self.debug_force_inline_gallery = true;
                format!("OK: injected {n} inline swarm members; gallery forced active")
            }
        } else if cmd == "swarm" || cmd == "swarm-status" {
            if self.is_remote {
                serde_json::json!({
                    "session_count": self.remote_sessions.len(),
                    "client_count": self.remote_client_count,
                    "members": self.remote_swarm_members,
                })
                .to_string()
            } else {
                serde_json::json!({
                    "session_count": 1,
                    "client_count": null,
                    "members": vec![crate::protocol::SwarmMemberStatus {
                        session_id: self.session.id.clone(),
                        friendly_name: Some(self.session.display_name().to_string()),
                        status: match &self.status {
                            ProcessingStatus::Idle => "ready".to_string(),
                            ProcessingStatus::Sending | ProcessingStatus::Connecting(_) => "running".to_string(),
                            ProcessingStatus::Thinking(_) => "thinking".to_string(),
                            ProcessingStatus::Streaming => "running".to_string(),
                            ProcessingStatus::WaitingForNetwork { .. } => "waiting_network".to_string(),
                            ProcessingStatus::RunningTool(_) => "running".to_string(),
                        },
                        detail: self.subagent_status.clone(),
                        task_label: None,
                        role: None,
                        is_headless: Some(false),
                        live_attachments: Some(1),
                        status_age_secs: Some(0),
                        output_tail: None,
                        report_back_to_session_id: None,
                        todo_progress: None,
                        todo_items: Vec::new(),
                        runtime: crate::protocol::SwarmMemberRuntime::default(),
                    }],
                })
                .to_string()
            }
        } else if cmd == "snapshot" {
            let snapshot = self.build_debug_snapshot();
            serde_json::to_string_pretty(&snapshot).unwrap_or_else(|_| "{}".to_string())
        } else if cmd.starts_with("wait:") {
            let raw = cmd.strip_prefix("wait:").unwrap_or("0");
            if let Ok(ms) = raw.parse::<u64>() {
                return self.apply_wait_ms(ms);
            }
            format!("ERR: invalid wait '{}'", raw)
        } else if cmd == "wait" {
            if self.is_processing {
                "wait: processing".to_string()
            } else {
                "wait: idle".to_string()
            }
        } else if cmd == "last_response" {
            // Get last assistant message
            self.display_messages
                .iter()
                .rev()
                .find(|m| m.role == "assistant" || m.role == "error")
                .map(|m| format!("last_response: [{}] {}", m.role, m.content))
                .unwrap_or_else(|| "last_response: none".to_string())
        } else if cmd == "history" {
            // Return all messages as JSON
            let msgs: Vec<serde_json::Value> = self
                .display_messages
                .iter()
                .map(|m| {
                    serde_json::json!({
                        "role": m.role,
                        "content": m.content,
                        "tool_calls": m.tool_calls,
                    })
                })
                .collect();
            serde_json::to_string_pretty(&msgs).unwrap_or_else(|_| "[]".to_string())
        } else if cmd == "screen" {
            // Capture current visual state
            use crate::tui::visual_debug;
            visual_debug::enable(); // Ensure enabled
            // Force a frame dump to file and return path
            match visual_debug::dump_to_file() {
                Ok(path) => format!("screen: {}", path.display()),
                Err(e) => format!("screen error: {}", e),
            }
        } else if cmd == "screen-json" {
            use crate::tui::visual_debug;
            visual_debug::enable();
            visual_debug::latest_frame_json()
                .unwrap_or_else(|| "screen-json: no frames captured".to_string())
        } else if cmd == "screen-json-normalized" {
            use crate::tui::visual_debug;
            visual_debug::enable();
            visual_debug::latest_frame_json_normalized()
                .unwrap_or_else(|| "screen-json-normalized: no frames captured".to_string())
        } else if cmd == "layout" {
            use crate::tui::visual_debug;
            visual_debug::enable();
            match visual_debug::latest_frame() {
                Some(frame) => serde_json::to_string_pretty(&serde_json::json!({
                    "frame_id": frame.frame_id,
                    "terminal_size": frame.terminal_size,
                    "layout": frame.layout,
                }))
                .unwrap_or_else(|_| "{}".to_string()),
                None => "layout: no frames captured".to_string(),
            }
        } else if cmd == "margins" {
            use crate::tui::visual_debug;
            visual_debug::enable();
            match visual_debug::latest_frame() {
                Some(frame) => serde_json::to_string_pretty(&serde_json::json!({
                    "frame_id": frame.frame_id,
                    "margins": frame.layout.margins,
                }))
                .unwrap_or_else(|_| "{}".to_string()),
                None => "margins: no frames captured".to_string(),
            }
        } else if cmd == "widgets" || cmd == "info-widgets" {
            use crate::tui::visual_debug;
            visual_debug::enable();
            match visual_debug::latest_frame() {
                Some(frame) => serde_json::to_string_pretty(&serde_json::json!({
                    "frame_id": frame.frame_id,
                    "info_widgets": frame.info_widgets,
                }))
                .unwrap_or_else(|_| "{}".to_string()),
                None => "widgets: no frames captured".to_string(),
            }
        } else if cmd == "render-stats" {
            use crate::tui::visual_debug;
            visual_debug::enable();
            let draw_calls = crate::tui::ui::debug_draw_call_history(16);
            match visual_debug::latest_frame() {
                Some(frame) => serde_json::to_string_pretty(&serde_json::json!({
                    "frame_id": frame.frame_id,
                    "render_timing": frame.render_timing,
                    "render_order": frame.render_order,
                    "draw_calls": draw_calls,
                }))
                .unwrap_or_else(|_| "{}".to_string()),
                None => serde_json::to_string_pretty(&serde_json::json!({
                    "frame_id": serde_json::Value::Null,
                    "render_timing": serde_json::Value::Null,
                    "render_order": serde_json::Value::Null,
                    "draw_calls": draw_calls,
                }))
                .unwrap_or_else(|_| "render-stats: no frames captured".to_string()),
            }
        } else if cmd == "draw-stats" {
            let mut payload = crate::tui::ui::debug_draw_call_history(32);
            attach_redraw_schedule_debug(&mut payload, self);
            serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string())
        } else if cmd.starts_with("draw-stats ") {
            let raw_limit = cmd.strip_prefix("draw-stats ").unwrap_or("32").trim();
            let limit = raw_limit.parse::<usize>().unwrap_or(32);
            let mut payload = crate::tui::ui::debug_draw_call_history(limit);
            attach_redraw_schedule_debug(&mut payload, self);
            serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string())
        } else if cmd == "render-order" {
            use crate::tui::visual_debug;
            visual_debug::enable();
            match visual_debug::latest_frame() {
                Some(frame) => serde_json::to_string_pretty(&frame.render_order)
                    .unwrap_or_else(|_| "[]".to_string()),
                None => "render-order: no frames captured".to_string(),
            }
        } else if cmd == "anomalies" {
            use crate::tui::visual_debug;
            visual_debug::enable();
            match visual_debug::latest_frame() {
                Some(frame) => serde_json::to_string_pretty(&frame.anomalies)
                    .unwrap_or_else(|_| "[]".to_string()),
                None => "anomalies: no frames captured".to_string(),
            }
        } else if cmd == "theme" {
            use crate::tui::visual_debug;
            visual_debug::enable();
            match visual_debug::latest_frame() {
                Some(frame) => serde_json::to_string_pretty(&frame.theme)
                    .unwrap_or_else(|_| "null".to_string()),
                None => "theme: no frames captured".to_string(),
            }
        } else if cmd == "memory" {
            serde_json::to_string_pretty(&self.debug_memory_profile())
                .unwrap_or_else(|_| "{}".to_string())
        } else if cmd == "memory-history" {
            let payload = crate::process_memory::history(128);
            serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "[]".to_string())
        } else if cmd == "allocator" {
            serde_json::to_string_pretty(&crate::process_memory::allocator_info())
                .unwrap_or_else(|_| "{}".to_string())
        } else if cmd == "allocator:purge" {
            let before = crate::process_memory::snapshot();
            crate::process_memory::release_retained_heap("client_debug_allocator_purge");
            let after = crate::process_memory::snapshot();
            let rss_recovered_bytes = before
                .rss_bytes
                .unwrap_or(0)
                .saturating_sub(after.rss_bytes.unwrap_or(0));
            serde_json::to_string_pretty(&serde_json::json!({
                "before": before,
                "after": after,
                "rss_recovered_bytes": rss_recovered_bytes,
            }))
            .unwrap_or_else(|_| "{}".to_string())
        } else if cmd == "slow-frames" {
            let payload = crate::tui::ui::debug_slow_frame_history(32);
            serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string())
        } else if cmd.starts_with("slow-frames ") {
            let raw_limit = cmd.strip_prefix("slow-frames ").unwrap_or("32").trim();
            let limit = raw_limit.parse::<usize>().unwrap_or(32);
            let payload = crate::tui::ui::debug_slow_frame_history(limit);
            serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string())
        } else if cmd == "flicker-frames" {
            let payload = crate::tui::ui::debug_flicker_frame_history(32);
            serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string())
        } else if cmd.starts_with("flicker-frames ") {
            let raw_limit = cmd.strip_prefix("flicker-frames ").unwrap_or("32").trim();
            let limit = raw_limit.parse::<usize>().unwrap_or(32);
            let payload = crate::tui::ui::debug_flicker_frame_history(limit);
            serde_json::to_string_pretty(&payload).unwrap_or_else(|_| "{}".to_string())
        } else if cmd == "markdown:stats" {
            let stats = crate::tui::markdown::debug_stats();
            serde_json::to_string_pretty(&stats).unwrap_or_else(|_| "{}".to_string())
        } else if cmd == "side-panel:stats" || cmd == "side-panel:debug" {
            crate::tui::side_panel_debug_json()
                .and_then(|value| serde_json::to_string_pretty(&value).ok())
                .unwrap_or_else(|| "null".to_string())
        } else if cmd == "markdown:memory" {
            let profile = crate::tui::markdown::debug_memory_profile();
            serde_json::to_string_pretty(&profile).unwrap_or_else(|_| "{}".to_string())
        } else if cmd.starts_with("assert:") {
            let raw = cmd.strip_prefix("assert:").unwrap_or("");
            self.handle_assertions(raw)
        } else if cmd.starts_with("run:") {
            let raw = cmd.strip_prefix("run:").unwrap_or("");
            self.handle_script_run(raw)
        } else if cmd.starts_with("inject:") {
            let raw = cmd.strip_prefix("inject:").unwrap_or("");
            let (role, content) = if let Some((r, c)) = raw.split_once(':') {
                let role = match r {
                    "user" | "assistant" | "system" | "background_task" | "tool" | "error"
                    | "meta" => r,
                    _ => "assistant",
                };
                if role == "assistant" && r != "assistant" {
                    ("assistant", raw)
                } else {
                    (role, c)
                }
            } else {
                ("assistant", raw)
            };

            self.push_display_message(DisplayMessage {
                role: role.to_string(),
                content: content.to_string(),
                tool_calls: vec![],
                duration_secs: None,
                title: None,
                tool_data: None,
            });
            format!("OK: injected {} message ({} chars)", role, content.len())
        } else if cmd == "widget-stability" || cmd.starts_with("widget-stability:") {
            let raw = cmd.strip_prefix("widget-stability:");
            self.run_widget_stability(raw)
        } else if cmd == "side-panel-latency" || cmd.starts_with("side-panel-latency:") {
            let raw = cmd.strip_prefix("side-panel-latency:");
            self.run_side_panel_latency_bench(raw)
        } else if cmd == "quit" {
            self.should_quit = true;
            "OK: quitting".to_string()
        } else if cmd == "trace-start" {
            self.debug_trace.enabled = true;
            self.debug_trace.started_at = Instant::now();
            self.debug_trace.events.clear();
            "OK: trace started".to_string()
        } else if cmd == "trace-stop" {
            self.debug_trace.enabled = false;
            "OK: trace stopped".to_string()
        } else if cmd == "trace" {
            serde_json::to_string_pretty(&self.debug_trace.events)
                .unwrap_or_else(|_| "[]".to_string())
        } else if cmd.starts_with("scroll:") {
            let dir = cmd.strip_prefix("scroll:").unwrap_or("");
            match dir {
                "up" => {
                    self.debug_scroll_up(5);
                    format!("scroll: up to {}", self.scroll_offset)
                }
                "down" => {
                    self.debug_scroll_down(5);
                    format!("scroll: down to {}", self.scroll_offset)
                }
                "top" => {
                    self.debug_scroll_top();
                    "scroll: top".to_string()
                }
                "bottom" => {
                    self.debug_scroll_bottom();
                    "scroll: bottom".to_string()
                }
                _ => format!("scroll error: unknown direction '{}'", dir),
            }
        } else if cmd.starts_with("keys:") {
            let keys_str = cmd.strip_prefix("keys:").unwrap_or("");
            let mut results = Vec::new();
            for key_spec in keys_str.split(',') {
                match self.parse_and_inject_key(key_spec.trim()) {
                    Ok(desc) => {
                        self.debug_trace.record("key", desc.to_string());
                        results.push(format!("OK: {}", desc));
                    }
                    Err(e) => results.push(format!("ERR: {}", e)),
                }
            }
            results.join("\n")
        } else if cmd == "input" {
            format!("input: {:?}", self.input)
        } else if cmd.starts_with("set_input:") {
            let new_input = cmd.strip_prefix("set_input:").unwrap_or("");
            self.input = new_input.to_string();
            self.cursor_pos = self.input.len();
            self.debug_trace
                .record("input", format!("set:{}", self.input));
            format!("OK: input set to {:?}", self.input)
        } else if cmd == "submit" {
            if self.input.is_empty() {
                "submit error: input is empty".to_string()
            } else {
                self.submit_input();
                self.debug_trace.record("input", "submitted".to_string());
                "OK: submitted".to_string()
            }
        } else if cmd == "record-start" {
            use crate::tui::test_harness;
            test_harness::start_recording();
            "OK: event recording started".to_string()
        } else if cmd == "record-stop" {
            use crate::tui::test_harness;
            test_harness::stop_recording();
            "OK: event recording stopped".to_string()
        } else if cmd == "record-events" {
            use crate::tui::test_harness;
            test_harness::get_recorded_events_json()
        } else if cmd == "clock-enable" {
            use crate::tui::test_harness;
            test_harness::enable_test_clock();
            "OK: test clock enabled".to_string()
        } else if cmd == "clock-disable" {
            use crate::tui::test_harness;
            test_harness::disable_test_clock();
            "OK: test clock disabled".to_string()
        } else if cmd.starts_with("clock-advance:") {
            use crate::tui::test_harness;
            let ms_str = cmd.strip_prefix("clock-advance:").unwrap_or("0");
            match ms_str.parse::<u64>() {
                Ok(ms) => {
                    test_harness::advance_clock(std::time::Duration::from_millis(ms));
                    format!("OK: clock advanced {}ms", ms)
                }
                Err(_) => "clock-advance error: invalid ms value".to_string(),
            }
        } else if cmd == "clock-now" {
            use crate::tui::test_harness;
            format!("clock: {}ms", test_harness::now_ms())
        } else if cmd.starts_with("replay:") {
            use crate::tui::test_harness;
            let json = cmd.strip_prefix("replay:").unwrap_or("[]");
            match test_harness::EventPlayer::from_json(json) {
                Ok(mut player) => {
                    player.start();
                    let mut results = Vec::new();
                    while let Some(event) = player.next_event() {
                        results.push(format!("{:?}", event));
                    }
                    format!(
                        "replay: {} events processed, {} remaining",
                        results.len(),
                        player.remaining()
                    )
                }
                Err(e) => format!("replay error: {}", e),
            }
        } else if cmd.starts_with("bundle-start:") {
            let name = cmd.strip_prefix("bundle-start:").unwrap_or("test");
            crate::env::set_var("JCODE_TEST_BUNDLE", name);
            format!("OK: test bundle '{}' started", name)
        } else if cmd == "bundle-save" {
            use crate::tui::test_harness::TestBundle;
            let name = std::env::var("JCODE_TEST_BUNDLE").unwrap_or_else(|_| "unnamed".to_string());
            let bundle = TestBundle::new(&name);
            let path = TestBundle::default_path(&name);
            match bundle.save(&path) {
                Ok(_) => format!("OK: bundle saved to {}", path.display()),
                Err(e) => format!("bundle-save error: {}", e),
            }
        } else if cmd.starts_with("script:") {
            let raw = cmd.strip_prefix("script:").unwrap_or("{}");
            match serde_json::from_str::<crate::tui::test_harness::TestScript>(raw) {
                Ok(script) => self.handle_test_script(script),
                Err(e) => format!("script error: {}", e),
            }
        } else if cmd == "version" {
            format!("version: {}", jcode_build_meta::version())
        } else if cmd == "help" {
            "Debug commands:\n\
                 - message:<text> - inject and submit a message\n\
                 - inject:<role>:<text> - inject display message without sending\n\
                 - reload - trigger /reload\n\
                 - state - get basic state info\n\
                 - snapshot - get combined state + frame snapshot JSON\n\
                 - assert:<json> - run assertions (see docs)\n\
                 - run:<json> - run scripted steps + assertions\n\
                 - trace-start - start recording trace events\n\
                 - trace-stop - stop recording trace events\n\
                 - trace - dump trace events JSON\n\
                 - quit - exit the TUI\n\
                 - last_response - get last assistant message\n\
                 - history - get all messages as JSON\n\
                 - screen - dump visual debug frames\n\
                 - screen-json - dump latest visual frame JSON\n\
                 - screen-json-normalized - dump normalized frame (for diffs)\n\
                 - frame - alias for screen-json\n\
                 - frame-normalized - alias for screen-json-normalized\n\
                 - layout - dump latest layout JSON\n\
                 - margins - dump layout margins JSON\n\
                 - widgets - dump info widget summary/placements\n\
                 - render-stats - dump render timing + order + draw-call attribution JSON\n\
                 - draw-stats [n] - dump per-draw attribution history (render_ms, changed cells)\n\
                 - render-order - dump render order list\n\
                 - anomalies - dump visual debug anomalies\n\
                 - theme - dump current palette snapshot\n\
                 - markdown:stats - dump markdown debug stats\n\
                 - markdown:memory - dump markdown cache memory estimate\n\
                 - memory - dump aggregate client memory profile\n\
                 - memory-history - dump recent process memory samples\n\
                 - slow-frames [n] - dump recent slow-frame records\n\
                 - flicker-frames [n] - dump recent frame-stability and flicker records\n\
                 - overlay:on/off/status - toggle overlay boxes\n\
                 - enable/disable/status - control visual debug capture\n\
                 - wait - check if processing\n\
                 - wait:<ms> - block until idle or timeout\n\
                 - scroll:<up|down|top|bottom> - control scroll\n\
                 - widget-stability[:<json>] - quantify info-widget movement while scrolling current transcript\n\
                 - side-panel-latency[:<json>] - benchmark headless side-panel input->frame latency\n\
                 - keys:<keyspec> - inject key events (e.g. keys:ctrl+r)\n\
                 - mouse:<kind>:<col>,<row> - inject mouse events (down|up|drag|click|jitter-click)\n\
                 - input - get current input buffer\n\
                 - set_input:<text> - set input buffer\n\
                 - submit - submit current input\n\
                 - record-start - start event recording\n\
                 - record-stop - stop event recording\n\
                 - record-events - get recorded events JSON\n\
                 - clock-enable - enable deterministic test clock\n\
                 - clock-disable - disable test clock\n\
                 - clock-advance:<ms> - advance test clock\n\
                 - clock-now - get current clock time\n\
                 - replay:<json> - replay recorded events\n\
                 - bundle-start:<name> - start test bundle\n\
                 - bundle-save - save test bundle\n\
                 - script:<json> - run test script\n\
                 - version - get version\n\
                 - help - show this help"
                .to_string()
        } else {
            format!("ERROR: unknown command '{}'. Use 'help' for list.", cmd)
        }
    }
}

/// Attach the live redraw-schedule decision to a `draw-stats` payload.
///
/// Frame cost alone cannot explain a choppy animation: a screen can render each
/// frame in 4ms and still look laggy because the scheduler only asks for a few
/// frames per second. Reporting the resolved interval and the policy FPS next
/// to the measured draw rate makes that distinction obvious.
fn attach_redraw_schedule_debug(payload: &mut serde_json::Value, app: &App) {
    let Some(map) = payload.as_object_mut() else {
        return;
    };
    let policy = crate::perf::tui_policy();
    map.insert(
        "redraw_schedule".to_string(),
        serde_json::json!({
            "interval_ms": crate::tui::tick_period(app).as_millis() as u64,
            "redraw_fps": policy.redraw_fps,
            "wants_fast_tick": crate::tui::wants_fast_tick(app),
            "client_focused": crate::tui::TuiState::client_focused(app),
            "periodic_redraw_required": crate::tui::periodic_redraw_required(app),
            // Latency the user actually feels: key read -> frame flushed. Reported
            // here because render/flush timings only cover work inside a draw and
            // say nothing about a keystroke waiting for one to start.
            "key_to_paint": crate::tui::ui::key_to_paint_debug_json(),
            // The notice text/age, so a notice that keeps re-arming (and so keeps
            // the loop at animation cadence) is identifiable rather than anonymous.
            "status_notice": crate::tui::TuiState::status_notice(app),
            "learn_hint": crate::tui::TuiState::learn_hint(app),
            "has_notification": crate::tui::TuiState::has_notification(app),
            "is_processing": crate::tui::TuiState::is_processing(app),
        }),
    );
}
