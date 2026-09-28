//! Graph fidelity: does `onboarding_graph` still describe the running flow?
//!
//! The graph's own invariants prove it is internally consistent. These prove it
//! still matches the state machine: for each transition the live code takes, the
//! graph must declare the same edge. `set_onboarding_phase` logs a mismatch in
//! production, which nobody reads until a user reports being stuck; this fails
//! instead.
//!
//! The behavior of each path is covered in `onboarding_flow.rs`. What is checked
//! here is only the graph's contract, so this file stays small on purpose.

use super::onboarding_flow::OnboardingPhase;
use crate::tui::app::onboarding_graph::{NodeId, node_for_phase, transition_is_declared};

#[test]
fn authenticated_start_lands_on_a_declared_edge() {
    with_temp_kcode_home(|| {
        let mut app = create_test_app();
        app.onboarding_flow = None;
        app.begin_onboarding_flow();

        let phase = app.onboarding_phase().expect("flow must be running");
        assert_eq!(
            node_for_phase(phase),
            NodeId::StartChoice,
            "authenticated startup must rest on the action choice"
        );
        assert!(
            transition_is_declared(NodeId::Start, NodeId::StartChoice),
            "an authenticated start lands on the action choice; the graph must declare that edge"
        );
    });
}

#[test]
fn declining_the_openai_prompt_lands_on_a_declared_edge() {
    with_temp_kcode_home(|| {
        let mut app = create_test_app();
        app.onboarding_flow = None;
        app.begin_onboarding_flow_at_login();
        if let Some(flow) = app.onboarding_flow.as_mut() {
            flow.phase = OnboardingPhase::LoginOpenAi {
                yes_highlighted: true,
            };
        }

        assert!(app.handle_onboarding_continue_prompt_key(crossterm::event::KeyCode::Char('n')));
        assert!(
            app.onboarding_phase().is_none(),
            "declining must end the flow"
        );
        assert!(
            transition_is_declared(NodeId::LoginOpenAi, NodeId::Done),
            "declining the OpenAI prompt ends the flow; the graph must declare that edge"
        );
    });
}
