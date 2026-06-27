// Tests for the onboarding simulator (Cmd+5 / `/onboarding-sim`).
//
// `include!`d into `crate::tui::app::tests`, so it shares the `create_test_app`
// harness and the onboarding type imports from the sibling includes.

use crossterm::event::{KeyCode, KeyModifiers};

#[test]
fn onboarding_sim_starts_steps_and_exits() {
    let mut app = create_test_app();
    assert!(!app.onboarding_sim_active());

    // Cmd+5 starts the simulator on the first screen.
    app.handle_key(KeyCode::Char('5'), KeyModifiers::SUPER)
        .unwrap();
    assert!(app.onboarding_sim_active(), "Cmd+5 should start the sim");
    assert!(
        app.onboarding_welcome_active(),
        "sim should render the onboarding welcome screen"
    );
    // First screen is the LoginOpenAi prompt.
    assert!(matches!(
        app.onboarding_phase(),
        Some(OnboardingPhase::LoginOpenAi { .. })
    ));

    // Tab advances to the import-review screen.
    app.handle_key(KeyCode::Tab, KeyModifiers::NONE).unwrap();
    assert!(matches!(
        app.onboarding_phase(),
        Some(OnboardingPhase::Login { import: Some(_) })
    ));

    // Esc exits and clears all onboarding state.
    app.handle_key(KeyCode::Esc, KeyModifiers::NONE).unwrap();
    assert!(!app.onboarding_sim_active(), "Esc should stop the sim");
    assert!(app.onboarding_phase().is_none());
}

#[test]
fn onboarding_sim_highlight_toggles_without_real_action() {
    let mut app = create_test_app();
    app.start_onboarding_simulator();

    // On the LoginOpenAi screen, 'l' previews the "No" highlight and 'h' the
    // "Yes" highlight. Neither triggers a real login (no overlay opens).
    app.handle_key(KeyCode::Char('l'), KeyModifiers::NONE)
        .unwrap();
    assert!(matches!(
        app.onboarding_phase(),
        Some(OnboardingPhase::LoginOpenAi {
            yes_highlighted: false
        })
    ));
    app.handle_key(KeyCode::Char('h'), KeyModifiers::NONE)
        .unwrap();
    assert!(matches!(
        app.onboarding_phase(),
        Some(OnboardingPhase::LoginOpenAi {
            yes_highlighted: true
        })
    ));
    assert!(
        app.onboarding_sim_active(),
        "previewing highlight should not exit the sim"
    );

    app.stop_onboarding_simulator();
    assert!(!app.onboarding_sim_active());
}

#[test]
fn onboarding_sim_advancing_past_last_screen_exits() {
    let mut app = create_test_app();
    app.start_onboarding_simulator();
    // Step forward many times; once we run off the end the sim exits cleanly.
    for _ in 0..20 {
        if !app.onboarding_sim_active() {
            break;
        }
        app.handle_key(KeyCode::Tab, KeyModifiers::NONE).unwrap();
    }
    assert!(
        !app.onboarding_sim_active(),
        "stepping past the last screen should exit the sim"
    );
}
