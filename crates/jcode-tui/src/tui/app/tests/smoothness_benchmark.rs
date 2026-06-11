// Deterministic anchor-stability (smoothness) benchmark over a simulated
// streaming turn.
//
// Drives a scripted reasoning -> answer -> commit turn through full
// `ui::draw` frames on a TestBackend, feeding each rendered messages-area
// frame into a local AnchorStabilityRecorder, then asserts budgets on the
// jarring-event counts. This is the regression gate for transcript
// smoothness: changes that make committed blocks pop, content reposition, or
// the screen reflow will fail here before anyone sees them live.

/// Render one frame and feed the messages area into the recorder.
fn observe_smoothness_frame(
    app: &App,
    terminal: &mut ratatui::Terminal<ratatui::backend::TestBackend>,
    recorder: &mut jcode_tui_core::anchor_stability::AnchorStabilityRecorder,
) {
    terminal
        .draw(|f| crate::tui::ui::draw(f, app))
        .expect("draw");
    let layout = crate::tui::ui::last_layout_snapshot().expect("layout snapshot");
    let frame = crate::tui::ui::smoothness_frame_from_buffer(
        terminal.backend().buffer(),
        layout.messages_area,
        app.scroll_offset,
        !app.auto_scroll_paused,
    )
    .expect("messages area frame");
    recorder.observe(frame);
}

/// Debug variant: also return the rendered text so failures can be diagnosed.
#[allow(dead_code)]
fn observe_smoothness_frame_text(
    app: &App,
    terminal: &mut ratatui::Terminal<ratatui::backend::TestBackend>,
    recorder: &mut jcode_tui_core::anchor_stability::AnchorStabilityRecorder,
) -> String {
    observe_smoothness_frame(app, terminal, recorder);
    let buf = terminal.backend().buffer();
    let mut out = String::new();
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            out.push_str(buf[(x, y)].symbol());
        }
        out.push('\n');
    }
    out
}

#[test]
fn smoothness_benchmark_simulated_streaming_turn_stays_within_budget() {
    let _render_lock = scroll_render_test_lock();
    let mut app = create_test_app();
    app.session.short_name = Some("test".to_string());
    let rt = tokio::runtime::Runtime::new().unwrap();
    let _guard = rt.enter();
    let mut remote = crate::tui::backend::RemoteConnection::dummy();

    let backend = ratatui::backend::TestBackend::new(100, 32);
    let mut terminal = ratatui::Terminal::new(backend).expect("terminal");
    let mut recorder = jcode_tui_core::anchor_stability::AnchorStabilityRecorder::new();

    app.is_processing = true;
    app.status = ProcessingStatus::Streaming;
    observe_smoothness_frame(&app, &mut terminal, &mut recorder);

    // Scripted turn: bursty reasoning, then an answer, then a commit, then a
    // second short reasoning+answer round. Drain the paced buffer between
    // event batches the way the redraw tick does, drawing after every step.
    let reasoning_chunks = [
        "Considering the request and weighing several options carefully.\n",
        "The first option needs less work but covers fewer cases overall.\n",
        "The second option is more robust; checking its constraints now.\n",
    ];
    for chunk in reasoning_chunks {
        app.handle_server_event(
            crate::protocol::ServerEvent::ReasoningDelta {
                text: chunk.to_string(),
            },
            &mut remote,
        );
        // Drain with multiple paced frames per burst, drawing each.
        for _ in 0..4 {
            let ops = app.stream_buffer.flush_smooth_frame();
            app.apply_stream_ops(ops);
            observe_smoothness_frame(&app, &mut terminal, &mut recorder);
        }
    }
    app.handle_server_event(
        crate::protocol::ServerEvent::ReasoningDone {
            duration_secs: None,
        },
        &mut remote,
    );

    let answer_chunks = [
        "Here is the plan: ",
        "first do the setup, ",
        "then run the checks, ",
        "and finally report the results in a table.",
    ];
    for chunk in answer_chunks {
        app.handle_server_event(
            crate::protocol::ServerEvent::TextDelta {
                text: chunk.to_string(),
            },
            &mut remote,
        );
        for _ in 0..4 {
            let ops = app.stream_buffer.flush_smooth_frame();
            app.apply_stream_ops(ops);
            observe_smoothness_frame(&app, &mut terminal, &mut recorder);
        }
    }
    // Force-drain anything left so the commit is deterministic.
    let ops = app.stream_buffer.flush();
    app.apply_stream_ops(ops);
    observe_smoothness_frame(&app, &mut terminal, &mut recorder);

    // Commit (as a tool call boundary would) and keep drawing.
    app.commit_pending_streaming_assistant_message();
    observe_smoothness_frame(&app, &mut terminal, &mut recorder);

    // Second round: more reasoning then a short answer.
    app.handle_server_event(
        crate::protocol::ServerEvent::ReasoningDelta {
            text: "Re-checking the output before finishing.\n".to_string(),
        },
        &mut remote,
    );
    for _ in 0..4 {
        let ops = app.stream_buffer.flush_smooth_frame();
        app.apply_stream_ops(ops);
        observe_smoothness_frame(&app, &mut terminal, &mut recorder);
    }
    app.handle_server_event(
        crate::protocol::ServerEvent::ReasoningDone {
            duration_secs: None,
        },
        &mut remote,
    );
    app.handle_server_event(
        crate::protocol::ServerEvent::TextDelta {
            text: "All done.".to_string(),
        },
        &mut remote,
    );
    let ops = app.stream_buffer.flush();
    app.apply_stream_ops(ops);
    observe_smoothness_frame(&app, &mut terminal, &mut recorder);

    let report = recorder.report();
    assert!(
        report.frames_compared >= 20,
        "benchmark must observe a realistic number of frames, got {}",
        report.frames_compared
    );
    // Budgets: a paced streaming turn must not reposition content or reflow
    // the screen. Commits may pop (bounded), and nothing should blink.
    assert_eq!(
        report.reposition_events, 0,
        "no content may move out of step with its anchor: {report:?}"
    );
    assert_eq!(
        report.mass_reflow_events, 0,
        "no whole-screen reflows during a streaming turn: {report:?}"
    );
    assert_eq!(
        report.blink_events, 0,
        "no rows may blink out and back: {report:?}"
    );
    assert!(
        report.big_pop_events <= 2,
        "at most the commit boundaries may pop, got {}: {report:?}",
        report.big_pop_events
    );
}
