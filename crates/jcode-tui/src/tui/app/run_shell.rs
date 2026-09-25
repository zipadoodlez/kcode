use super::*;

fn report_reload_interaction_gap() {
    let Ok(started) = std::env::var("JCODE_RELOAD_GAP_STARTED_MS") else {
        return;
    };
    crate::env::remove_var("JCODE_RELOAD_GAP_STARTED_MS");
    let Some(started_ms) = started.parse::<u128>().ok() else {
        return;
    };
    let Ok(now) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) else {
        return;
    };
    let gap_ms = now.as_millis().saturating_sub(started_ms);
    crate::logging::info(&format!(
        "client_reload_interaction_gap_ms={} milestone=first_frame",
        gap_ms
    ));
}
use crossterm::terminal::{BeginSynchronizedUpdate, EndSynchronizedUpdate};
use ratatui::buffer::Buffer;

pub(super) fn redraw_timer(period: Duration) -> tokio::time::Interval {
    let mut interval = tokio::time::interval_at(tokio::time::Instant::now() + period, period);
    // Redraw ticks represent visual liveness, not elapsed simulation steps. An
    // immediate first tick or Burst catch-up after a slow frame only schedules
    // redundant full renders and can lock the UI into a slow-frame loop.
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    interval
}

/// Statuses whose full status line starts with the primary green circular spinner.
///
/// Keep this in sync with `ui_input::draw_status`: these statuses can be safely
/// refreshed by the one-cell spinner fast path when the status line is left aligned.
/// Network waits use a static amber retry marker, so they do not belong here.
pub(crate) fn status_uses_primary_spinner(status: &ProcessingStatus) -> bool {
    matches!(
        status,
        ProcessingStatus::Sending
            | ProcessingStatus::Connecting(_)
            | ProcessingStatus::Thinking(_)
            | ProcessingStatus::Streaming
            | ProcessingStatus::RunningTool(_)
    )
}

/// How the next full frame should invalidate ratatui's diff state, if at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FullFrameInvalidation {
    /// `Terminal::clear()`: an ED2 Clear-All escape plus a full re-emit.
    /// Needed when the real screen diverged from ratatui's model in cells the
    /// next diff may not repaint (native terminal scroll, external commands).
    HardClear,
    /// Sentinel-invalidate the previous buffer: full re-emit with no
    /// intermediate clear escape, so the repaint stays atomic inside the
    /// synchronized update. Used for scroll-driven repaints (issue #404).
    SoftRepaint,
    /// Normal incremental diff.
    None,
}

/// Pure routing for `draw_full`: a hard clear supersedes a soft repaint.
pub(crate) fn full_frame_invalidation(
    force_full_redraw: bool,
    force_full_repaint: bool,
) -> FullFrameInvalidation {
    if force_full_redraw {
        FullFrameInvalidation::HardClear
    } else if force_full_repaint {
        FullFrameInvalidation::SoftRepaint
    } else {
        FullFrameInvalidation::None
    }
}

/// A cell no real frame produces: a Unicode noncharacter symbol with an
/// improbable style, so a diff against it sees every cell as changed.
fn full_repaint_sentinel_cell() -> ratatui::buffer::Cell {
    let mut cell = ratatui::buffer::Cell::EMPTY;
    cell.set_symbol("\u{FDD0}");
    cell.fg = ratatui::style::Color::Rgb(1, 2, 3);
    cell.bg = ratatui::style::Color::Rgb(3, 2, 1);
    cell
}

/// Fill ratatui's "previous" buffer with sentinel cells so the next
/// `Terminal::draw` diff re-emits every cell.
///
/// This is the flicker-free alternative to `Terminal::clear()` for repaints
/// that need full cell coverage (ratatui #2357 wide-grapheme ghosts on
/// scroll) but not a real screen wipe: `Terminal::clear()` emits an ED2
/// Clear-All escape before the frame is redrawn, and terminals that paint
/// image placeholder cells non-atomically flash blank during the
/// clear-then-repaint on every scroll tick (issue #404). Overwriting every
/// cell in place inside the surrounding synchronized update repaints
/// atomically instead.
pub(crate) fn invalidate_previous_terminal_buffer<B: ratatui::backend::Backend>(
    terminal: &mut ratatui::Terminal<B>,
) {
    // `swap_buffers` resets the inactive buffer and flips. Two swaps with a
    // sentinel fill in between leave: previous = all-sentinel, current = reset
    // and ready for the next `draw`.
    terminal.swap_buffers();
    let sentinel = full_repaint_sentinel_cell();
    for cell in terminal.current_buffer_mut().content.iter_mut() {
        *cell = sentinel.clone();
    }
    terminal.swap_buffers();
}

#[derive(Default)]
pub(super) struct FrameRenderer {
    last_frame: Option<Buffer>,
}

impl FrameRenderer {
    pub(super) fn invalidate(&mut self) {
        self.last_frame = None;
    }
    pub(super) fn draw_full(
        &mut self,
        app: &mut App,
        terminal: &mut DefaultTerminal,
    ) -> Result<()> {
        // Painting a frame is progress, including during long streaming turns.
        crate::logging::watchdog::beat("tui.draw");
        let invalidation = full_frame_invalidation(app.force_full_redraw, app.force_full_repaint);
        let force_full_redraw = invalidation != FullFrameInvalidation::None;
        // Wrap the whole frame (optional clear + diff flush) in a synchronized update so the
        // terminal applies every cell change atomically. Without this, ratatui's crossterm
        // backend streams cells one-by-one and eagerly-repainting terminals (and slow/remote or
        // multiplexed sessions) show visible flicker. See issue #282.
        let sync = crossterm::execute!(terminal.backend_mut(), BeginSynchronizedUpdate).is_ok();
        match invalidation {
            FullFrameInvalidation::HardClear => {
                terminal.clear()?;
                self.invalidate();
            }
            FullFrameInvalidation::SoftRepaint => {
                invalidate_previous_terminal_buffer(terminal);
                self.invalidate();
            }
            FullFrameInvalidation::None => {}
        }
        app.force_full_redraw = false;
        app.force_full_repaint = false;

        let previous_frame = self.last_frame.as_ref();
        let draw_start = Instant::now();
        let mut render_elapsed = Duration::ZERO;
        let completed = terminal.draw(|frame| {
            let render_start = Instant::now();
            crate::tui::ui::draw(frame, app);
            render_elapsed = render_start.elapsed();
        })?;
        let total_elapsed = draw_start.elapsed();
        let changed_cells = previous_frame
            .filter(|previous| previous.area == completed.buffer.area)
            .map(|previous| {
                previous
                    .content
                    .iter()
                    .zip(completed.buffer.content.iter())
                    .filter(|(left, right)| left != right)
                    .count()
            });
        let total_cells = Some(completed.buffer.content.len());
        let completed_buffer = completed.buffer.clone();
        // `completed` borrows the terminal; it is unused past this point, so the
        // borrow ends here (NLL) before we touch the backend again below.
        if sync {
            let _ = crossterm::execute!(terminal.backend_mut(), EndSynchronizedUpdate);
        }
        crate::tui::ui::record_draw_call_attribution(crate::tui::ui::DrawCallAttribution {
            timestamp_ms: crate::tui::ui::wall_clock_ms(),
            total_ms: total_elapsed.as_secs_f64() * 1000.0,
            render_ms: render_elapsed.as_secs_f64() * 1000.0,
            backend_flush_ms: total_elapsed.saturating_sub(render_elapsed).as_secs_f64() * 1000.0,
            changed_cells,
            total_cells,
            force_full_redraw,
            input: crate::tui::ui::frame_input_attribution_snapshot(),
        });
        self.last_frame = Some(completed_buffer);
        // Close the key-to-paint clock here rather than at render time: the user
        // sees the keystroke when the frame reaches the terminal, so anything
        // before the flush would understate the latency they feel.
        crate::tui::ui::note_frame_painted();
        Ok(())
    }
}

impl App {
    /// Run the TUI application
    /// Returns Some(session_id) if hot-reload was requested
    pub async fn run(mut self, mut terminal: DefaultTerminal) -> Result<RunResult> {
        super::terminal_liveness::capture_initial_tty();
        let mut event_stream = EventStream::new();
        let mut redraw_period = crate::tui::redraw_interval(&self);
        let mut redraw_interval = redraw_timer(redraw_period);
        let mut frame_renderer = FrameRenderer::default();
        let mut needs_redraw = true;
        let mut first_frame_reported = false;
        let mut handterm_native_scroll =
            super::handterm_native_scroll::HandtermNativeScrollClient::connect_from_env();
        // Subscribe to bus for background task completion notifications
        let mut bus_receiver = Bus::global().subscribe();
        if let Some(status) = Bus::global().latest_update_status() {
            self.handle_update_status(status);
        }

        loop {
            self.sync_sleep_guard();
            let desired_redraw = crate::tui::redraw_interval(&self);
            if desired_redraw != redraw_period {
                redraw_period = desired_redraw;
                redraw_interval = redraw_timer(redraw_period);
            }

            if needs_redraw {
                frame_renderer.draw_full(&mut self, &mut terminal)?;
                if !first_frame_reported {
                    first_frame_reported = true;
                    report_reload_interaction_gap();
                }
                if let Some(native) = handterm_native_scroll.as_mut() {
                    native.sync_from_app(&self);
                }
                needs_redraw = false;
            }

            if self.should_quit {
                break;
            }

            // Process pending turn OR wait for input/redraw
            if self.pending_turn {
                self.pending_turn = false;
                // Process turn while still handling input
                self.process_turn_with_input(&mut terminal, &mut event_stream, &mut bus_receiver)
                    .await;
                needs_redraw = true;
            } else if self.pending_queued_dispatch {
                self.pending_queued_dispatch = false;
                self.process_queued_messages(&mut terminal, &mut event_stream)
                    .await;
                local::finish_turn(&mut self);
                needs_redraw = true;
            } else {
                // Wait for input or redraw tick
                tokio::select! {
                    // Declaration-order polling: user input outranks timers and
                    // bus chatter (see the remote loop for the rationale).
                    biased;
                    event = event_stream.next() => {
                        if event.is_some() {
                            needs_redraw |= local::handle_terminal_event(&mut self, &mut terminal, event)?;
                        } else if super::terminal_liveness::terminal_abandoned() {
                            // Input EOF and the controlling terminal is gone:
                            // this client is an orphan (window died without a
                            // deliverable SIGHUP). Exit instead of looping
                            // forever holding ~100 MB. The session persists
                            // and can be resumed.
                            crate::logging::warn(
                                "Terminal input closed and controlling terminal is gone; exiting orphaned client",
                            );
                            self.should_quit = true;
                        } else {
                            tokio::time::sleep(redraw_period).await;
                        }
                    }
                    _ = redraw_interval.tick() => {
                        needs_redraw |= local::handle_tick(&mut self);
                    }
                    command = async {
                        match handterm_native_scroll.as_mut() {
                            Some(native) => native.recv().await,
                            None => futures::future::pending::<Option<super::handterm_native_scroll::HostToApp>>().await,
                        }
                    } => {
                        if let Some(command) = command {
                            self.apply_handterm_native_scroll(command);
                            self.request_full_redraw();
                            needs_redraw = true;
                        } else {
                            handterm_native_scroll = None;
                        }
                    }
                    // Handle background task completion notifications
                    bus_event = bus_receiver.recv() => {
                        needs_redraw |= local::handle_bus_event(&mut self, bus_event);
                    }
                }
            }
        }

        Ok(RunResult {
            reload_session: self.reload_requested.take(),
            rebuild_session: self.rebuild_requested.take(),
            update_session: self.update_requested.take(),
            restart_session: self.restart_requested.take(),
            exit_code: self.requested_exit_code,
            session_id: Some(self.session.id.clone()),
        })
    }

    /// Run the TUI in remote mode, connecting to a server
    pub async fn run_remote(
        mut self,
        mut terminal: DefaultTerminal,
        remote_working_dir: Option<String>,
    ) -> Result<RunResult> {
        if crate::tui::is_ssh_remote() {
            self.session.working_dir = remote_working_dir.clone();
        }
        super::terminal_liveness::capture_initial_tty();
        let mut event_stream = EventStream::new();
        let mut redraw_period = crate::tui::redraw_interval(&self);
        let mut redraw_interval = redraw_timer(redraw_period);
        let mut frame_renderer = FrameRenderer::default();
        let mut needs_redraw = true;
        let mut first_frame_reported = false;
        // While unfocused and idle, redraws are throttled to this interval so a
        // backgrounded session does not repaint at full rate on shared-server bus
        // chatter. `None` means "no throttled frame drawn yet since losing focus".
        const UNFOCUSED_IDLE_REDRAW_MIN_INTERVAL: std::time::Duration =
            std::time::Duration::from_millis(1000);
        let mut last_unfocused_draw: Option<std::time::Instant> = None;
        let mut handterm_native_scroll =
            super::handterm_native_scroll::HandtermNativeScrollClient::connect_from_env();
        let mut remote_state = remote::RemoteRunState::default();

        'outer: loop {
            if self.display_messages.is_empty() {
                if self.server_spawning {
                    self.set_remote_startup_phase(super::RemoteStartupPhase::StartingServer);
                } else {
                    self.set_remote_startup_phase(super::RemoteStartupPhase::Connecting);
                }
            }
            if needs_redraw {
                frame_renderer.draw_full(&mut self, &mut terminal)?;
                if !first_frame_reported {
                    first_frame_reported = true;
                    report_reload_interaction_gap();
                }
                // Close the startup-profile gap: `pre_run_remote` is the last
                // pre-loop mark, so the first completed paint here is the real
                // process-to-first-frame point. Logged once via a static guard so
                // the end-to-end launch cost (including the ~5ms first draw) is
                // visible in the startup profile without re-marking every frame.
                {
                    use std::sync::atomic::{AtomicBool, Ordering};
                    static FIRST_FRAME_MARKED: AtomicBool = AtomicBool::new(false);
                    if !FIRST_FRAME_MARKED.swap(true, Ordering::Relaxed) {
                        crate::startup_profile::mark("first_frame");
                        crate::startup_profile::report_to_log();
                    }
                }
                needs_redraw = false;
            }

            let session_to_resume = self.reconnect_target_session_id();

            let mut remote_conn = match remote::connect_with_retry(
                &mut self,
                &mut terminal,
                &mut event_stream,
                &mut remote_state,
                session_to_resume.as_deref(),
                remote_working_dir.as_deref(),
            )
            .await?
            {
                remote::ConnectOutcome::Connected(remote) => remote,
                remote::ConnectOutcome::Retry => continue,
                remote::ConnectOutcome::Quit => break 'outer,
            };
            frame_renderer.invalidate();

            match remote::handle_post_connect(
                &mut self,
                &mut terminal,
                &mut remote_conn,
                &mut remote_state,
                session_to_resume.as_deref(),
            )
            .await?
            {
                remote::PostConnectOutcome::Ready => {}
                remote::PostConnectOutcome::Quit => break 'outer,
            }
            frame_renderer.invalidate();
            needs_redraw = true;

            let mut bus_receiver_remote = Bus::global().subscribe();
            if let Some(status) = Bus::global().latest_update_status() {
                self.handle_update_status(status);
                needs_redraw = true;
            }

            // Main event loop
            loop {
                self.sync_sleep_guard();
                let desired_redraw = crate::tui::redraw_interval(&self);
                if desired_redraw != redraw_period {
                    redraw_period = desired_redraw;
                    redraw_interval = redraw_timer(redraw_period);
                }

                if needs_redraw {
                    // Throttle idle full-frame renders while the terminal is
                    // backgrounded (FocusLost). An unfocused, idle session has
                    // nothing changing worth a 60fps repaint, so it should not
                    // repaint at full rate just because other sessions on the
                    // shared server broadcast bus updates -- that is what made a
                    // swarm of background windows saturate the CPU. We keep full-
                    // rate redraws while streaming/processing so visible-but-
                    // unfocused windows in a tiling WM still show live progress,
                    // and set_client_focused(true) forces a full repaint on refocus.
                    let allow_redraw = self.client_focused()
                        || self.unfocused_redraw_warranted()
                        || last_unfocused_draw
                            .map(|t| t.elapsed() >= UNFOCUSED_IDLE_REDRAW_MIN_INTERVAL)
                            .unwrap_or(true);
                    if allow_redraw {
                        frame_renderer.draw_full(&mut self, &mut terminal)?;
                        if let Some(native) = handterm_native_scroll.as_mut() {
                            native.sync_from_app(&self);
                        }
                        last_unfocused_draw =
                            (!self.client_focused()).then(std::time::Instant::now);
                        needs_redraw = false;
                    }
                    // When unfocused and throttled, leave needs_redraw set so the
                    // pending update is coalesced into the next allowed frame.
                }

                if self.should_quit {
                    break 'outer;
                }

                if self.pending_queued_dispatch {
                    self.pending_queued_dispatch = false;
                    remote::process_remote_followups(&mut self, &mut remote_conn).await;
                    needs_redraw = true;
                    continue;
                }

                tokio::select! {
                    // Poll in declaration order so user input always wins the
                    // race against server/bus chatter. During heavy streaming
                    // the remote event branch is almost always ready; with the
                    // default random polling it repeatedly outcompetes buffered
                    // keystrokes, which shows up as a laggy, stuttering input
                    // line while a turn is running.
                    biased;
                    event = event_stream.next() => {
                        if event.is_some() {
                            needs_redraw |= remote::handle_terminal_event(&mut self, &mut terminal, &mut remote_conn, event).await?;
                        } else if super::terminal_liveness::terminal_abandoned() {
                            // Input EOF with the controlling terminal gone:
                            // orphaned client (see local loop). Exit; the
                            // server-side session keeps running and can be
                            // reattached with --resume.
                            crate::logging::warn(
                                "Terminal input closed and controlling terminal is gone; exiting orphaned client",
                            );
                            self.should_quit = true;
                        } else {
                            tokio::time::sleep(redraw_period).await;
                        }
                    }
                    _ = redraw_interval.tick() => {
                        needs_redraw |= remote::handle_tick(&mut self, &mut remote_conn).await;
                    }
                    event = remote_conn.next_event() => {
                        let (outcome, event_redraw) = remote::handle_remote_event(
                            &mut self,
                            &mut terminal,
                            &mut remote_conn,
                            &mut remote_state,
                            event,
                        )
                        .await?;
                        needs_redraw |= event_redraw;
                        match outcome {
                            remote::RemoteEventOutcome::Continue => {}
                            remote::RemoteEventOutcome::Reconnect => continue 'outer,
                            remote::RemoteEventOutcome::Quit => break 'outer,
                        }
                    }
                    command = async {
                        match handterm_native_scroll.as_mut() {
                            Some(native) => native.recv().await,
                            None => futures::future::pending::<Option<super::handterm_native_scroll::HostToApp>>().await,
                        }
                    } => {
                        if let Some(command) = command {
                            self.apply_handterm_native_scroll(command);
                            self.request_full_redraw();
                            needs_redraw = true;
                        } else {
                            handterm_native_scroll = None;
                        }
                    }
                    bus_event = bus_receiver_remote.recv() => {
                        needs_redraw |= remote::handle_bus_event(&mut self, &mut remote_conn, bus_event).await;
                    }
                }
            }
        }

        Ok(RunResult {
            reload_session: self.reload_requested.take(),
            rebuild_session: self.rebuild_requested.take(),
            update_session: self.update_requested.take(),
            restart_session: self.restart_requested.take(),
            exit_code: self.requested_exit_code,
            session_id: if self.is_remote {
                self.remote_session_id.clone()
            } else {
                Some(self.session.id.clone())
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn redraw_timer_waits_one_period_and_skips_missed_ticks() {
        let mut timer = redraw_timer(Duration::from_millis(250));
        assert!(
            tokio::time::timeout(Duration::from_millis(20), timer.tick())
                .await
                .is_err(),
            "the first redraw tick must not fire immediately"
        );
        assert_eq!(
            timer.missed_tick_behavior(),
            tokio::time::MissedTickBehavior::Skip
        );
    }

    #[test]
    fn primary_spinner_statuses_are_explicit() {
        assert!(status_uses_primary_spinner(&ProcessingStatus::Sending));
        assert!(status_uses_primary_spinner(&ProcessingStatus::Streaming));
        assert!(status_uses_primary_spinner(&ProcessingStatus::RunningTool(
            "bash".to_string()
        )));
        assert!(!status_uses_primary_spinner(&ProcessingStatus::Idle));
        assert!(!status_uses_primary_spinner(
            &ProcessingStatus::WaitingForNetwork {
                listener: "network".to_string(),
            }
        ));
    }
}
