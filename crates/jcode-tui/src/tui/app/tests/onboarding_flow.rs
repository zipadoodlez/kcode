// Integration tests for the first-run onboarding flow control logic.

use super::onboarding_flow::{ExternalCli, OnboardingFlow, OnboardingPhase};

fn onboarding_test_app() -> App {
    let mut app = create_test_app();
    // Force the flow on regardless of the on-disk new-user heuristic.
    app.onboarding_flow = Some(OnboardingFlow::begin());
    app
}

#[test]
fn onboarding_begins_at_model_select() {
    let mut app = create_test_app();
    app.onboarding_flow = None;
    app.begin_onboarding_flow();
    assert!(matches!(
        app.onboarding_phase(),
        Some(OnboardingPhase::ModelSelect)
    ));
    // begin is idempotent: a second call does not reset the phase.
    if let Some(flow) = app.onboarding_flow.as_mut() {
        flow.phase = OnboardingPhase::Suggestions;
    }
    app.begin_onboarding_flow();
    assert!(matches!(
        app.onboarding_phase(),
        Some(OnboardingPhase::Suggestions)
    ));
}

#[test]
fn answering_no_on_continue_prompt_shows_suggestions() {
    with_temp_jcode_home(|| {
        let mut app = onboarding_test_app();
        if let Some(flow) = app.onboarding_flow.as_mut() {
            flow.phase = OnboardingPhase::ContinuePrompt {
                cli: ExternalCli::Codex,
                shown_at: std::time::Instant::now(),
            };
        }
        app.onboarding_answer_continue(false);
        assert!(matches!(
            app.onboarding_phase(),
            Some(OnboardingPhase::Suggestions)
        ));
        // No session picker overlay opened on the "No" path.
        assert!(app.session_picker_overlay.is_none());
    });
}

#[test]
fn continue_prompt_key_y_consumes_and_advances() {
    with_temp_jcode_home(|| {
        let mut app = onboarding_test_app();
        if let Some(flow) = app.onboarding_flow.as_mut() {
            flow.phase = OnboardingPhase::ContinuePrompt {
                cli: ExternalCli::ClaudeCode,
                shown_at: std::time::Instant::now(),
            };
        }
        // 'Y' is consumed by the onboarding handler.
        assert!(app.handle_onboarding_continue_prompt_key(KeyCode::Char('Y')));
        // It either opened the picker (TranscriptPick) or fell back depending on
        // whether transcripts exist in the temp home; either way it leaves
        // ContinuePrompt.
        assert!(!matches!(
            app.onboarding_phase(),
            Some(OnboardingPhase::ContinuePrompt { .. })
        ));
    });
}

#[test]
fn continue_prompt_key_ignored_when_not_in_phase() {
    let mut app = create_test_app();
    app.onboarding_flow = None;
    assert!(!app.handle_onboarding_continue_prompt_key(KeyCode::Char('y')));
}

#[test]
fn no_external_transcripts_falls_back_to_session_search() {
    with_temp_jcode_home(|| {
        let mut app = onboarding_test_app();
        if let Some(flow) = app.onboarding_flow.as_mut() {
            flow.phase = OnboardingPhase::ContinuePrompt {
                cli: ExternalCli::Codex,
                shown_at: std::time::Instant::now(),
            };
        }
        // Temp home has no Codex transcripts, so opening the picker should fall
        // back to the session-search prompt and finish the flow.
        app.onboarding_open_transcript_picker(ExternalCli::Codex);
        assert!(matches!(
            app.onboarding_phase(),
            None | Some(OnboardingPhase::Done)
        ));
        assert!(app.session_picker_overlay.is_none());
        // The fallback announced it's finding and continuing the latest session.
        assert!(
            app.display_messages()
                .iter()
                .any(|m| m.content.contains("find and continue"))
        );
    });
}

#[test]
fn onboarding_picker_mode_carries_cli() {
    let mode = SessionPickerMode::Onboarding {
        cli: ExternalCli::ClaudeCode,
    };
    assert!(matches!(mode, SessionPickerMode::Onboarding { .. }));
    assert_ne!(mode, SessionPickerMode::Resume);
}
