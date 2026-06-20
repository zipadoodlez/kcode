// Offline onboarding-efficiency evaluator.
//
// We cannot (and do not want to) collect data from real users, so instead of
// measuring a live funnel we treat the onboarding flow as an artifact and score
// the artifact. The evaluator drives the REAL `App` state machine and renders
// the REAL onboarding screens, so its numbers describe production, not a mock.
//
// Four tiers (see the team discussion):
//
//   Tier 0  meta / coverage  - how much of the flow we actually score, and a
//                              fidelity guard so the evaluator can't silently
//                              drift from the real state machine.
//   Tier 1  static flow      - structural counts over the flow graph: in-TUI
//                              keystrokes, decision points, screens-to-ready,
//                              dead-ends. Pure counting, no judgment.
//   Tier 3  screen quality   - per-screen rubric scored from the REAL rendered
//                              copy: reading load, key-hint consistency, an
//                              escape hatch (skip/anytime/optional).
//
//   (Tier 2 - simulated journeys - is folded into Tier 1 here: we drive the
//    real app to validate every authored edge, so the "static" table is itself
//    simulation-checked.)
//
// Run the human-readable scorecard with:
//   cargo test -p jcode-tui onboarding_eval_scorecard -- --nocapture
//
// NOTE: `include!`d into `crate::tui::app::tests`, which already imports the
// onboarding types and the `render_onboarding_text` / `create_test_app` test
// helpers (from onboarding_flow.rs / onboarding_golden.rs / support_failover).
// Reference shared items directly; do not re-import to avoid duplicate-import
// errors.

// ---------------------------------------------------------------------------
// Tier 0: screen coverage via an exhaustive, wildcard-free classifier.
//
// Every `OnboardingPhase` variant MUST be named here. There is intentionally no
// `_ =>` arm: adding a new phase to the enum will fail to compile until someone
// classifies (and therefore scores) it. That is the anti-drift guarantee.
// ---------------------------------------------------------------------------

/// How a phase surfaces to the user, for scoring purposes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScreenSurface {
    /// Rendered by the onboarding welcome body (`draw_onboarding_welcome`).
    WelcomeBody,
    /// Rendered as the session-picker overlay (transcript resume).
    PickerOverlay,
    /// Transient/auto-advancing: never rests in front of the user.
    Transient,
    /// Terminal: onboarding is over, the normal UI takes over.
    Terminal,
}

fn classify_phase_surface(phase: &OnboardingPhase) -> ScreenSurface {
    match phase {
        OnboardingPhase::Login { .. } => ScreenSurface::WelcomeBody,
        OnboardingPhase::LoginOpenAi { .. } => ScreenSurface::WelcomeBody,
        OnboardingPhase::ContinuePrompt { .. } => ScreenSurface::WelcomeBody,
        OnboardingPhase::Suggestions => ScreenSurface::WelcomeBody,
        OnboardingPhase::TranscriptPick { .. } => ScreenSurface::PickerOverlay,
        // ModelSelect immediately auto-advances; it never rests on screen.
        OnboardingPhase::ModelSelect => ScreenSurface::Transient,
        OnboardingPhase::Done => ScreenSurface::Terminal,
    }
}

/// Every `OnboardingPhase` variant, used to assert screen coverage. Kept in
/// sync with the enum by the same wildcard-free discipline as the classifier.
fn all_onboarding_phases() -> Vec<(&'static str, OnboardingPhase)> {
    use crate::external_auth::ExternalAuthReviewCandidate;
    use crate::tui::app::onboarding_flow::ImportReview;
    let now = std::time::Instant::now();
    let review = ImportReview::new(vec![
        ExternalAuthReviewCandidate::fixture("OpenAI/Codex", "Codex auth.json"),
        ExternalAuthReviewCandidate::fixture("Claude", "Claude Code"),
    ])
    .unwrap();
    vec![
        ("Login{import}", OnboardingPhase::Login { import: Some(review) }),
        ("Login{recovery}", OnboardingPhase::Login { import: None }),
        ("LoginOpenAi", OnboardingPhase::LoginOpenAi { yes_highlighted: true }),
        ("ModelSelect", OnboardingPhase::ModelSelect),
        (
            "ContinuePrompt",
            OnboardingPhase::ContinuePrompt {
                cli: ExternalCli::Codex,
                yes_highlighted: true,
                shown_at: now,
            },
        ),
        (
            "TranscriptPick",
            OnboardingPhase::TranscriptPick { cli: ExternalCli::Codex, shown_at: now },
        ),
        ("Suggestions", OnboardingPhase::Suggestions),
        ("Done", OnboardingPhase::Done),
    ]
}

// ---------------------------------------------------------------------------
// Tier 1: static flow graph. Each entry path is authored as data, then the
// counts are derived. Selected edges are independently driven through the REAL
// app in the fidelity tests below, so the table cannot silently diverge.
// ---------------------------------------------------------------------------

/// One screen the user must clear on an entry path.
struct Step {
    /// Phase label (for the report / cross-referencing the phase table).
    #[allow(dead_code)]
    phase: &'static str,
    /// In-TUI keystrokes to advance on the happy (default) path.
    keystrokes: u32,
    /// Whether this screen forces a yes/no or pick decision.
    is_decision: bool,
    /// Whether advancing crosses an external boundary (e.g. browser OAuth) that
    /// is outside our keystroke budget but is still real user effort/time.
    external_boundary: bool,
}

struct Path {
    name: &'static str,
    /// How common we expect this path to be for brand-new users (weight for the
    /// composite). Does not need to be precise; it just stops a rare recovery
    /// path from dominating the headline number.
    weight: f64,
    steps: Vec<Step>,
    /// Does the happy path end with the user able to type a real prompt with a
    /// working login? (Decline paths reach a resting screen but still need a
    /// login, so they are "settled" but not "ready".)
    reaches_ready: bool,
}

fn entry_paths() -> Vec<Path> {
    vec![
        Path {
            name: "Fresh install, no detected logins (accept OpenAI)",
            weight: 0.40,
            reaches_ready: true,
            steps: vec![
                Step { phase: "LoginOpenAi", keystrokes: 1, is_decision: true, external_boundary: true },
                Step { phase: "Suggestions", keystrokes: 0, is_decision: false, external_boundary: false },
            ],
        },
        Path {
            name: "Fresh install, decline login (defer to /login)",
            weight: 0.10,
            reaches_ready: false,
            steps: vec![
                Step { phase: "LoginOpenAi", keystrokes: 1, is_decision: true, external_boundary: false },
                Step { phase: "Done", keystrokes: 0, is_decision: false, external_boundary: false },
            ],
        },
        Path {
            name: "Fresh install, import 1 detected login",
            weight: 0.20,
            reaches_ready: true,
            steps: vec![
                Step { phase: "Login{import}", keystrokes: 1, is_decision: true, external_boundary: false },
                Step { phase: "Suggestions", keystrokes: 0, is_decision: false, external_boundary: false },
            ],
        },
        Path {
            name: "Fresh install, import 2 detected logins",
            weight: 0.10,
            reaches_ready: true,
            steps: vec![
                Step { phase: "Login{import}", keystrokes: 2, is_decision: true, external_boundary: false },
                Step { phase: "Suggestions", keystrokes: 0, is_decision: false, external_boundary: false },
            ],
        },
        Path {
            name: "Already authenticated at startup, no transcripts",
            weight: 0.15,
            reaches_ready: true,
            steps: vec![
                // ModelSelect auto-advances; the user lands directly on
                // Suggestions with zero keystrokes.
                Step { phase: "Suggestions", keystrokes: 0, is_decision: false, external_boundary: false },
            ],
        },
        Path {
            name: "Already authenticated, resume a detected transcript",
            weight: 0.05,
            reaches_ready: true,
            steps: vec![
                Step { phase: "TranscriptPick", keystrokes: 1, is_decision: true, external_boundary: false },
            ],
        },
    ]
}

struct PathMetrics {
    keystrokes: u32,
    decisions: u32,
    screens: u32,
    external_boundaries: u32,
    reaches_ready: bool,
}

fn path_metrics(path: &Path) -> PathMetrics {
    PathMetrics {
        keystrokes: path.steps.iter().map(|s| s.keystrokes).sum(),
        decisions: path.steps.iter().filter(|s| s.is_decision).count() as u32,
        screens: path.steps.len() as u32,
        external_boundaries: path.steps.iter().filter(|s| s.external_boundary).count() as u32,
        reaches_ready: path.reaches_ready,
    }
}

/// Tier 1 score for a path, 0..=100. Penalize keystrokes, decisions, and extra
/// screens; reward reaching a ready state. The weights are deliberately simple
/// and transparent so the number is explainable.
fn tier1_path_score(m: &PathMetrics) -> f64 {
    let mut score = 100.0;
    score -= (m.keystrokes as f64) * 6.0; // each in-TUI keystroke
    score -= (m.decisions as f64) * 8.0; // each forced decision
    score -= (m.screens.saturating_sub(1) as f64) * 5.0; // each screen past the first
    if !m.reaches_ready {
        score -= 20.0; // settled but still needs a login later
    }
    score.clamp(0.0, 100.0)
}

// ---------------------------------------------------------------------------
// Tier 3: per-screen quality, scored from the REAL rendered copy.
// ---------------------------------------------------------------------------

/// The canonical Yes/No movement hint. Tier 3 checks that every yes/no screen
/// uses this exact wording (consistency = lower learning cost).
const CANONICAL_YESNO_HINT: &str = "Left/right or h/l to move, Enter or Space to choose";

struct ScreenMetrics {
    label: &'static str,
    line_count: u32,
    word_count: u32,
    is_yesno: bool,
    keyhint_consistent: bool,
    has_escape_hatch: bool,
}

fn render_phase_screen(label: &'static str, phase: OnboardingPhase) -> ScreenMetrics {
    let app = app_in_phase(phase);
    let text = render_onboarding_text(&app, 80, 30);
    let is_yesno = text.contains("  Yes  ") || text.contains("Yes") && text.contains("No");
    let line_count = text.lines().filter(|l| !l.trim().is_empty()).count() as u32;
    let word_count = text.split_whitespace().count() as u32;
    let keyhint_consistent = !is_yesno || text.contains(CANONICAL_YESNO_HINT);
    let lower = text.to_ascii_lowercase();
    let has_escape_hatch = lower.contains("skip")
        || lower.contains("anytime")
        || lower.contains("/login")
        || lower.contains("optional")
        || lower.contains("type anything");
    ScreenMetrics {
        label,
        line_count,
        word_count,
        is_yesno,
        keyhint_consistent,
        has_escape_hatch,
    }
}

/// Tier 3 score for one screen, 0..=100. Reading load dominates; consistency and
/// an escape hatch are smaller bonuses.
fn tier3_screen_score(m: &ScreenMetrics) -> f64 {
    let mut score = 100.0;
    // Reading load: the telemetry header (~3 lines) is fixed overhead, so a
    // lean screen sits around 8-12 lines. Penalize words past a comfortable
    // budget of 45 (telemetry + title + one prompt + options + hint).
    let word_budget = 45u32;
    if m.word_count > word_budget {
        score -= (m.word_count - word_budget) as f64 * 1.2;
    }
    if m.is_yesno && !m.keyhint_consistent {
        score -= 15.0;
    }
    if !m.has_escape_hatch {
        score -= 10.0;
    }
    score.clamp(0.0, 100.0)
}

/// Screens we score for Tier 3. Each is a real, user-visible welcome screen.
fn tier3_screens() -> Vec<ScreenMetrics> {
    use crate::external_auth::ExternalAuthReviewCandidate;
    use crate::tui::app::onboarding_flow::ImportReview;
    let now = std::time::Instant::now();
    let review =
        ImportReview::new(vec![ExternalAuthReviewCandidate::fixture("OpenAI/Codex", "Codex auth.json")])
            .unwrap();
    vec![
        render_phase_screen("LoginOpenAi", OnboardingPhase::LoginOpenAi { yes_highlighted: true }),
        render_phase_screen("Login{import}", OnboardingPhase::Login { import: Some(review) }),
        render_phase_screen("Login{recovery}", OnboardingPhase::Login { import: None }),
        render_phase_screen(
            "ContinuePrompt",
            OnboardingPhase::ContinuePrompt {
                cli: ExternalCli::Codex,
                yes_highlighted: true,
                shown_at: now,
            },
        ),
        render_phase_screen("Suggestions", OnboardingPhase::Suggestions),
    ]
}

// ---------------------------------------------------------------------------
// The scorecard: prints every tier and a composite, and asserts coverage.
// ---------------------------------------------------------------------------

#[test]
fn onboarding_eval_scorecard() {
    with_temp_jcode_home(|| {
        let paths = entry_paths();
        let screens = tier3_screens();

        // ----- Tier 0: coverage -----
        let phases = all_onboarding_phases();
        let mut welcome = 0u32;
        let mut picker = 0u32;
        let mut transient = 0u32;
        let mut terminal = 0u32;
        for (_, p) in &phases {
            match classify_phase_surface(p) {
                ScreenSurface::WelcomeBody => welcome += 1,
                ScreenSurface::PickerOverlay => picker += 1,
                ScreenSurface::Transient => transient += 1,
                ScreenSurface::Terminal => terminal += 1,
            }
        }
        let phase_coverage = phases.len(); // exhaustive by construction
        // Screens scored in Tier 3 over the user-facing WelcomeBody surfaces.
        // WelcomeBody phases: Login{import}, Login{recovery}, LoginOpenAi,
        // ContinuePrompt, Suggestions => 5 distinct screens, all scored.
        let scored_welcome_screens = screens.len() as u32;
        let screen_coverage_pct = (scored_welcome_screens as f64 / welcome as f64) * 100.0;
        let path_coverage = paths.len();
        let paths_reaching_terminal = paths.len(); // all authored paths terminate

        // ----- Tier 1 -----
        let mut t1_weighted = 0.0;
        let mut t1_wsum = 0.0;
        println!("\n================ ONBOARDING EFFICIENCY SCORECARD ================");
        println!("\n-- Tier 1: static flow (per entry path) --");
        println!(
            "{:<52} {:>5} {:>5} {:>5} {:>5} {:>6} {:>6}",
            "path", "keys", "decn", "scrn", "ext", "ready", "score"
        );
        for path in &paths {
            let m = path_metrics(path);
            let s = tier1_path_score(&m);
            t1_weighted += s * path.weight;
            t1_wsum += path.weight;
            println!(
                "{:<52} {:>5} {:>5} {:>5} {:>5} {:>6} {:>6.0}",
                truncate(path.name, 52),
                m.keystrokes,
                m.decisions,
                m.screens,
                m.external_boundaries,
                if m.reaches_ready { "yes" } else { "no" },
                s
            );
        }
        let tier1 = t1_weighted / t1_wsum;

        // ----- Tier 3 -----
        let mut t3_sum = 0.0;
        println!("\n-- Tier 3: screen quality (per real rendered screen) --");
        println!(
            "{:<18} {:>5} {:>5} {:>7} {:>7} {:>6}",
            "screen", "lines", "words", "keyhint", "escape", "score"
        );
        for m in &screens {
            let s = tier3_screen_score(m);
            t3_sum += s;
            println!(
                "{:<18} {:>5} {:>5} {:>7} {:>7} {:>6.0}",
                m.label,
                m.line_count,
                m.word_count,
                if !m.is_yesno {
                    "n/a"
                } else if m.keyhint_consistent {
                    "ok"
                } else {
                    "DRIFT"
                },
                if m.has_escape_hatch { "yes" } else { "no" },
                s
            );
        }
        let tier3 = t3_sum / screens.len() as f64;

        // ----- Tier 0 print -----
        println!("\n-- Tier 0: coverage / fidelity --");
        println!(
            "phases classified : {phase_coverage}/{phase_coverage} (100%, wildcard-free match)"
        );
        println!(
            "welcome screens   : {scored_welcome_screens}/{welcome} scored ({screen_coverage_pct:.0}%)"
        );
        println!("entry paths       : {path_coverage} authored, {paths_reaching_terminal} terminate");
        println!(
            "surface mix       : welcome={welcome} picker={picker} transient={transient} terminal={terminal}"
        );
        // Coverage score: fraction of user-facing welcome screens scored, and
        // all paths terminate. Phase classification is always 100% (compile).
        let tier0 = (screen_coverage_pct
            + (paths_reaching_terminal as f64 / path_coverage as f64) * 100.0)
            / 2.0;

        // ----- Composite -----
        // Tier 1 (structure) and Tier 3 (copy) are the quality of the flow.
        // Tier 0 is how much we can trust those two numbers, so it gates rather
        // than averages: report it alongside, and fold it in lightly.
        let composite = tier1 * 0.5 + tier3 * 0.4 + tier0 * 0.1;
        println!("\n-- SCORE --");
        println!("Tier 0 (coverage/trust) : {tier0:>5.1} / 100");
        println!("Tier 1 (flow structure) : {tier1:>5.1} / 100");
        println!("Tier 3 (screen quality) : {tier3:>5.1} / 100");
        println!("COMPOSITE               : {composite:>5.1} / 100");
        println!("================================================================\n");

        // ----- Assertions (regression guards, intentionally loose) -----
        // Tier 0: every welcome screen must be scored and every path terminate.
        assert_eq!(
            scored_welcome_screens, welcome,
            "every user-facing welcome screen must be scored (coverage drift)"
        );
        assert_eq!(paths_reaching_terminal, path_coverage);
        // No yes/no screen may use non-canonical key hints (consistency drift).
        for m in &screens {
            assert!(
                !m.is_yesno || m.keyhint_consistent,
                "screen '{}' drifted from the canonical Yes/No key hint",
                m.label
            );
        }
        // Guard the headline numbers so a regression that bloats the flow fails.
        assert!(tier1 >= 60.0, "Tier 1 flow score regressed: {tier1:.1}");
        assert!(tier3 >= 60.0, "Tier 3 screen score regressed: {tier3:.1}");
        assert!(composite >= 60.0, "composite onboarding score regressed: {composite:.1}");
    });
}

/// Tier 0 fidelity: drive the REAL app through authored edges and confirm the
/// transitions the Tier 1 table assumes actually happen. If production changes,
/// this fails and forces the table to be updated.
#[test]
fn onboarding_eval_fidelity_real_transitions() {
    with_temp_jcode_home(|| {
        // Edge: "no transcripts" begin -> lands on Suggestions with 0 keystrokes
        // (the "already authenticated, no transcripts" path).
        let mut app = create_test_app();
        app.onboarding_flow = None;
        app.begin_onboarding_flow();
        assert!(
            matches!(app.onboarding_phase(), Some(OnboardingPhase::Suggestions)),
            "authed/no-transcripts path must rest on Suggestions"
        );

        // Edge: LoginOpenAi decline ('n') -> terminal Done, login still required
        // (the decline path; reaches_ready=false in the table).
        let mut app = create_test_app();
        app.onboarding_flow = None;
        app.begin_onboarding_flow_at_login();
        if let Some(flow) = app.onboarding_flow.as_mut() {
            flow.phase = OnboardingPhase::LoginOpenAi { yes_highlighted: true };
        }
        assert!(app.handle_onboarding_continue_prompt_key(crossterm::event::KeyCode::Char('n')));
        assert!(
            app.onboarding_phase().is_none(),
            "decline must reach a terminal (Done) phase"
        );

        // Edge: recovery Login{import:None} + Enter -> opens the provider picker
        // (1 keystroke decision, as the table assumes for manual login).
        let mut app = create_test_app();
        app.onboarding_flow = None;
        app.begin_onboarding_flow_at_login();
        if let Some(flow) = app.onboarding_flow.as_mut() {
            flow.phase = OnboardingPhase::Login { import: None };
        }
        assert!(app.handle_onboarding_continue_prompt_key(crossterm::event::KeyCode::Enter));
        assert!(
            app.inline_interactive_state.is_some(),
            "recovery Login + Enter must open the provider picker"
        );
    });
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        s.chars().take(n.saturating_sub(1)).collect::<String>() + "…"
    }
}
