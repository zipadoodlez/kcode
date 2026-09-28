// Redraw-cadence policy: which app states justify a fast tick, and which must
// not.
//
// This exists because of a real, user-visible bug: a transient status notice
// (e.g. "Swarm plan synced ...") put the whole client on the fast cadence.
// At 60fps that is ~180 full frames per notice, each re-deriving the transcript,
// header, status line, and composer into essentially identical cells. Keystrokes
// landed behind one of those frames, so a freshly spawned session felt laggy,
// and a notice that kept re-arming held the client there indefinitely.
//
// `scripts/repro_input_lag.py --live` measures the same thing end to end against
// a real binary; these tests are the cheap gate that runs in CI.

/// A state whose only "live" element is a piece of static text chrome.
fn static_chrome_state(notice: Option<&str>) -> TestState {
    TestState {
        display_messages: vec![DisplayMessage {
            role: "user".to_string(),
            content: "a real prompt".to_string(),
            tool_calls: vec![],
            duration_secs: None,
            title: None,
            tool_data: None,
        }],
        status_notice: notice.map(str::to_string),
        status: ProcessingStatus::Idle,
        time_since_activity: Some(Duration::from_secs(1)),
        ..Default::default()
    }
}

fn fast_interval() -> Duration {
    let fps = crate::perf::tui_policy().redraw_fps.max(1);
    Duration::from_millis(1000 / u64::from(fps))
}

/// The regression gate: a status notice must not want a fast tick.
#[test]
fn a_status_notice_does_not_force_a_fast_tick() {
    let with_notice = static_chrome_state(Some("Swarm plan synced (v55, 98 items)"));
    assert!(
        !crate::tui::wants_fast_tick(&with_notice),
        "a static notice must not want the fast cadence"
    );
    assert_eq!(
        crate::tui::tick_period(&with_notice),
        crate::tui::REDRAW_IDLE,
        "a notice must tick at the slow cadence"
    );
    assert!(
        crate::tui::REDRAW_IDLE > fast_interval(),
        "the slow cadence must be slower than the fast one"
    );
}

/// The fix must not slow down states that genuinely animate: streaming output
/// still wants the fast tick even though a notice may be on screen too.
#[test]
fn streaming_output_keeps_the_fast_tick_even_with_a_notice() {
    let mut state = static_chrome_state(Some("Swarm plan synced"));
    state.streaming_text = "partial assistant answer".to_string();

    assert!(crate::tui::wants_fast_tick(&state));
    assert_eq!(crate::tui::tick_period(&state), fast_interval());
}

/// `periodic_redraw_required` decides whether a tick draws at all. A notice must
/// still get frames (otherwise it would never appear or retire); the fix is
/// about cadence, not about dropping the notice.
#[test]
fn a_status_notice_still_requires_periodic_frames() {
    assert!(
        crate::tui::periodic_redraw_required(&static_chrome_state(Some("Swarm plan synced"))),
        "a visible notice must still be repainted so it can appear and expire"
    );
}

/// The post-onboarding notice screen: the transcript holds only system
/// notices ("Here are a few things you can try", the login summary), the user
/// pressed a key moments ago, and no stream has ever run in this process.
///
/// `time_since_activity()` reports "past the deep-idle threshold" for any
/// non-empty never-streamed transcript, which is meant for *restored dormant*
/// sessions. A recent keystroke is proof the session is not dormant, so the
/// notice must not be parked at the 5s crawl.
fn just_touched_notice_screen() -> TestState {
    TestState {
        display_messages: vec![DisplayMessage {
            role: "system".to_string(),
            content: "Here are a few things you can try: ...".to_string(),
            tool_calls: vec![],
            duration_secs: None,
            title: None,
            tool_data: None,
        }],
        status: ProcessingStatus::Idle,
        // What `time_since_activity()` actually reports for a non-empty
        // transcript that has never streamed: already past deep idle.
        time_since_activity: Some(crate::tui::REDRAW_DEEP_IDLE_AFTER + Duration::from_secs(1)),
        time_since_user_interaction: Some(Duration::from_secs(2)),
        ..Default::default()
    }
}

#[test]
fn a_recent_keystroke_keeps_the_notice_screen_out_of_deep_idle() {
    let state = just_touched_notice_screen();
    assert_eq!(
        crate::tui::tick_period(&state),
        crate::tui::REDRAW_IDLE,
        "a notice screen the user just touched must not be parked at deep idle"
    );
}

/// The flip side: once the user walks away for the deep-idle window, the same
/// screen must still fall back to the crawl. The fix is about *recent*
/// interaction, not about disabling deep idle for notice screens.
#[test]
fn a_notice_screen_left_alone_still_reaches_deep_idle() {
    let mut state = just_touched_notice_screen();
    state.time_since_user_interaction =
        Some(crate::tui::REDRAW_DEEP_IDLE_AFTER + Duration::from_secs(1));
    assert_eq!(
        crate::tui::tick_period(&state),
        crate::tui::REDRAW_DEEP_IDLE,
        "a dormant notice screen must tick at the deep-idle crawl"
    );
}
