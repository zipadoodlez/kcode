// Golden state-space walker for the first-run onboarding welcome screen.
//
// This renders every onboarding phase to an offscreen TestBackend and captures
// the exact text the user sees. It serves two purposes:
//
//   1. A reviewable, deterministic dump of every onboarding screen (run with
//      `--nocapture` to read them), so we can verify every word of copy without
//      manually walking the live flow.
//   2. Regression guards on the exact wording / option layout of each phase.
//
// To see all rendered screens:
//   cargo test -p jcode-tui onboarding_golden -- --nocapture

// NOTE: This file is `include!`d into `crate::tui::app::tests`, which already
// imports `ExternalCli`, `OnboardingFlow`, and `OnboardingPhase` via the
// sibling `onboarding_flow.rs` include. To avoid duplicate-import errors we
// reference types through fully-qualified paths / local aliases below instead
// of adding module-level `use` statements.

/// Render the onboarding welcome screen for `app` into a fixed-size buffer and
/// return the visible text, one line per row, trailing blank rows trimmed.
fn render_onboarding_text(app: &App, width: u16, height: u16) -> String {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let area = frame.area();
            crate::tui::ui::draw_onboarding_welcome_for_tests(frame, app, area);
        })
        .unwrap();

    let buffer = terminal.backend().buffer().clone();
    let mut rows: Vec<String> = Vec::with_capacity(height as usize);
    for y in 0..height {
        let mut row = String::new();
        for x in 0..width {
            row.push_str(buffer[(x, y)].symbol());
        }
        rows.push(row.trim_end().to_string());
    }
    while rows.last().map(|r| r.is_empty()).unwrap_or(false) {
        rows.pop();
    }
    rows.join("\n")
}

/// Force the app into a specific onboarding phase, bypassing the on-disk
/// new-user heuristic.
fn app_in_phase(phase: OnboardingPhase) -> App {
    let mut app = create_test_app();
    let mut flow = OnboardingFlow::begin();
    flow.phase = phase;
    app.onboarding_flow = Some(flow);
    app
}

fn dump(title: &str, text: &str) {
    println!("\n========== {title} ==========");
    println!("{text}");
    println!("==========================================");
}

#[test]
fn onboarding_golden_walks_every_phase() {
    use crate::external_auth::ExternalAuthReviewCandidate;
    use crate::tui::app::onboarding_flow::ImportReview;

    let width = 80u16;
    let height = 30u16;

    // 1. No detected imports: "Log in to OpenAI?" Yes/No prompt.
    {
        let app = app_in_phase(OnboardingPhase::LoginOpenAi {
            yes_highlighted: true,
        });
        let text = render_onboarding_text(&app, width, height);
        dump("LoginOpenAi (no imports)", &text);
        assert!(text.contains("First, log in to get started."), "{text}");
        assert!(text.contains("Log in to OpenAI?"), "{text}");
        assert!(
            text.contains("Choose \"No\" to skip for now (run /login anytime)."),
            "{text}"
        );
        assert!(text.contains("Yes") && text.contains("No"), "{text}");
    }

    // 1b. Recovery fallback: bare Login phase with no import (import declined or
    // failed) points the user at the provider picker.
    {
        let app = app_in_phase(OnboardingPhase::Login { import: None });
        let text = render_onboarding_text(&app, width, height);
        dump("Login (no imports, recovery)", &text);
        assert!(text.contains("First, log in to get started."), "{text}");
        assert!(
            text.contains("Press Enter to pick who to log in with"),
            "{text}"
        );
    }

    // 2. Login with detected imports (two-column list + Next button).
    {
        let review = ImportReview::new(vec![
            ExternalAuthReviewCandidate::fixture("OpenAI/Codex", "Codex auth.json"),
            ExternalAuthReviewCandidate::fixture("Claude", "Claude Code"),
        ])
        .unwrap();
        let app = app_in_phase(OnboardingPhase::Login {
            import: Some(review),
        });
        let text = render_onboarding_text(&app, width, height);
        dump("Login (import two-column list, 2 logins)", &text);
        assert!(text.contains("We found 2 existing logins."), "count: {text}");
        // Both logins are listed at once, each with a Yes/No choice.
        assert!(text.contains("OpenAI/Codex"), "provider 1: {text}");
        assert!(text.contains("Codex auth.json"), "source 1: {text}");
        assert!(text.contains("Claude"), "provider 2: {text}");
        // Each login carries a Yes/No pair, pre-set to Yes (import).
        assert!(text.contains("( Yes )"), "yes pill: {text}");
        assert!(text.contains("( No )"), "no pill: {text}");
        assert!(
            text.contains("Press Enter to continue."),
            "continue action: {text}"
        );
        assert!(
            text.contains("Left/Right for Yes/No"),
            "toggle hint: {text}"
        );
        assert!(text.contains("Imports all checked in"), "countdown: {text}");
    }

    // 2b. Singular phrasing for a single detected login.
    {
        let review =
            ImportReview::new(vec![ExternalAuthReviewCandidate::fixture("Cursor", "Cursor")])
                .unwrap();
        let app = app_in_phase(OnboardingPhase::Login {
            import: Some(review),
        });
        let text = render_onboarding_text(&app, width, height);
        dump("Login (import two-column list, single login)", &text);
        assert!(
            text.contains("We found 1 existing login."),
            "singular count: {text}"
        );
        assert!(text.contains("( Yes )"), "yes pill: {text}");
    }

    // 4. Continue prompt (resume an external session).
    {
        let app = app_in_phase(OnboardingPhase::ContinuePrompt {
            cli: ExternalCli::Codex,
            yes_highlighted: true,
            shown_at: std::time::Instant::now(),
        });
        let text = render_onboarding_text(&app, width, height);
        dump("ContinuePrompt (Codex)", &text);
        assert!(
            text.contains("Continue where you left off in Codex?"),
            "continue prompt: {text}"
        );
        assert!(
            text.contains("( Yes )") && text.contains("( No )"),
            "continue prompt Yes/No pills: {text}"
        );
        assert!(
            text.contains('<') && text.contains('>'),
            "continue prompt movability chevrons: {text}"
        );
        assert!(
            text.contains("Opens the resume menu automatically in"),
            "resume-menu hint: {text}"
        );
    }

    // 5. Suggestions (resting state).
    {
        let app = app_in_phase(OnboardingPhase::Suggestions);
        let text = render_onboarding_text(&app, width, height);
        dump("Suggestions", &text);
        assert!(text.contains("Welcome to jcode onboarding"), "{text}");
    }
}
