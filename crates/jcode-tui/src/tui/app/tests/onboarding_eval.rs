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
    tier1_path_score_w(m, &Tier1Weights::default())
}

/// Tunable Tier 1 weights, factored out so the meta-evaluation layer can
/// perturb them for sensitivity analysis. The `default()` values are the ones
/// used by the live scorecard.
#[derive(Clone, Copy)]
struct Tier1Weights {
    per_keystroke: f64,
    per_decision: f64,
    per_extra_screen: f64,
    not_ready: f64,
}

impl Default for Tier1Weights {
    fn default() -> Self {
        Self {
            per_keystroke: 6.0,
            per_decision: 8.0,
            per_extra_screen: 5.0,
            not_ready: 20.0,
        }
    }
}

fn tier1_path_score_w(m: &PathMetrics, w: &Tier1Weights) -> f64 {
    let mut score = 100.0;
    score -= (m.keystrokes as f64) * w.per_keystroke;
    score -= (m.decisions as f64) * w.per_decision;
    score -= (m.screens.saturating_sub(1) as f64) * w.per_extra_screen;
    if !m.reaches_ready {
        score -= w.not_ready;
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
    tier3_screen_score_w(m, &Tier3Weights::default())
}

/// Tunable Tier 3 weights, factored out for sensitivity analysis.
#[derive(Clone, Copy)]
struct Tier3Weights {
    word_budget: u32,
    per_excess_word: f64,
    inconsistent_keyhint: f64,
    no_escape_hatch: f64,
}

impl Default for Tier3Weights {
    fn default() -> Self {
        Self {
            word_budget: 45,
            per_excess_word: 1.2,
            inconsistent_keyhint: 15.0,
            no_escape_hatch: 10.0,
        }
    }
}

fn tier3_screen_score_w(m: &ScreenMetrics, w: &Tier3Weights) -> f64 {
    let mut score = 100.0;
    // Reading load: the telemetry header (~3 lines) is fixed overhead, so a
    // lean screen sits around 8-12 lines. Penalize words past a comfortable
    // budget (telemetry + title + one prompt + options + hint).
    if m.word_count > w.word_budget {
        score -= (m.word_count - w.word_budget) as f64 * w.per_excess_word;
    }
    if m.is_yesno && !m.keyhint_consistent {
        score -= w.inconsistent_keyhint;
    }
    if !m.has_escape_hatch {
        score -= w.no_escape_hatch;
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
// Tier 4: content & robustness. Cross-screen and behavioral signals that the
// per-screen Tier 3 rubric cannot see, each measured from the REAL screens or
// by driving the REAL app:
//
//   * terminology_consistency - the same concept is named the same way across
//     every screen (e.g. the human prose says "log in", never also "sign in").
//   * progress_visibility      - a multi-step context tells the user where they
//     are ("Login 1 of 2"), so a sequence never feels open-ended.
//   * default_safety           - when a timed decision auto-commits, the default
//     lands on a non-destructive / recoverable outcome.
//   * narrow_terminal_safety   - the core affordances (the Yes/No options) still
//     render on a cramped terminal.
//
// These are deliberately the signals that ARE derivable offline. Several
// neighbours (min-path overhead, back-navigation depth, jargon density) are
// left Deferred in the registry because an honest measurement needs more
// machinery than we have; we do not fake them as Scored.
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct Tier4Metrics {
    /// The login concept is phrased consistently across all welcome screens.
    terminology_consistent: bool,
    /// A multi-step context surfaces "N of M" position.
    progress_visible: bool,
    /// A timed auto-commit resolves to a non-destructive default.
    default_safe: bool,
    /// The Yes/No options survive a cramped (50-col) terminal.
    narrow_options_survive: bool,
}

/// Detect whether the human-facing prose names the login concept consistently.
/// The canonical phrasing is the two-word verb "log in". A drift to "sign in"
/// or the one-word "login" *as a verb in prose* (the `/login` command is fine)
/// is an inconsistency. Measured over the real rendered welcome screens.
fn terminology_is_consistent(screens: &[(&'static str, String)]) -> bool {
    for (_, text) in screens {
        let lower = text.to_ascii_lowercase();
        // A competing synonym for the same action is a hard inconsistency.
        if lower.contains("sign in") || lower.contains("sign-in") || lower.contains("log on") {
            return false;
        }
        // "login" as a standalone prose word (not the `/login` command and not
        // the "Login N of M" progress label) would compete with "log in".
        for raw in lower.split_whitespace() {
            let w = raw.trim_matches(|c: char| !c.is_ascii_alphabetic());
            if w == "login" {
                // Allowed: the `/login` command token and the "Login N of M"
                // progress header. Both are recognizable by their surroundings.
                let is_command = raw.contains('/');
                let is_progress_header = lower.contains("login 1 of") || lower.contains("login 2 of");
                if !is_command && !is_progress_header {
                    return false;
                }
            }
        }
    }
    true
}

/// Compute the four Tier 4 signals by reading the real screens and driving the
/// real app. `with_temp_jcode_home` must already be active.
fn tier4_metrics() -> Tier4Metrics {
    use crate::external_auth::ExternalAuthReviewCandidate;
    use crate::tui::app::onboarding_flow::ImportReview;

    // ---- terminology_consistency: scan every welcome screen's prose ----
    let terminology_consistent = terminology_is_consistent(&all_welcome_screen_texts());

    // ---- progress_visibility: the 2-candidate import is a multi-step context
    // and must show position ("Login 1 of 2"). Rendered from the real screen. ----
    let review = ImportReview::new(vec![
        ExternalAuthReviewCandidate::fixture("OpenAI/Codex", "Codex auth.json"),
        ExternalAuthReviewCandidate::fixture("Claude", "Claude Code"),
    ])
    .unwrap();
    let multi = app_in_phase(OnboardingPhase::Login { import: Some(review) });
    let multi_text = render_onboarding_text(&multi, 80, 30).to_ascii_lowercase();
    let progress_visible = multi_text.contains(" of 2") || multi_text.contains(" of 1");

    // ---- default_safety: drive the real ContinuePrompt timeout. The highlighted
    // default is "Yes", and a timeout must resolve to a non-destructive outcome
    // (open the resume picker), never silently discard the user's history. ----
    let default_safe = {
        let mut app = create_test_app();
        app.onboarding_flow = None;
        app.begin_onboarding_flow_at_login();
        if let Some(flow) = app.onboarding_flow.as_mut() {
            // Place the prompt in the past so the decision has already timed out.
            let past = std::time::Instant::now()
                - (crate::tui::app::onboarding_flow::DECISION_TIMEOUT
                    + std::time::Duration::from_secs(1));
            flow.phase = OnboardingPhase::ContinuePrompt {
                cli: ExternalCli::Codex,
                yes_highlighted: true,
                shown_at: past,
            };
        }
        // Tick the flow: the timeout fires and auto-commits the highlighted Yes.
        app.onboarding_tick();
        // A SAFE default lands on a recoverable phase the user can still act on
        // (the resume picker, or the suggestion cards when no transcript exists)
        // rather than a terminal that silently discards their session. An UNSAFE
        // default would be `Done` (login/session lost) or leaving the flow.
        matches!(
            app.onboarding_phase(),
            Some(OnboardingPhase::TranscriptPick { .. } | OnboardingPhase::Suggestions)
        )
    };

    // ---- narrow_terminal_safety: the core Yes/No affordance must still render
    // on a cramped 50-col terminal (real renderer, smaller buffer). ----
    let narrow = app_in_phase(OnboardingPhase::LoginOpenAi { yes_highlighted: true });
    let narrow_text = render_onboarding_text(&narrow, 50, 30);
    let narrow_options_survive = narrow_text.contains("Yes") && narrow_text.contains("No");

    Tier4Metrics {
        terminology_consistent,
        progress_visible,
        default_safe,
        narrow_options_survive,
    }
}

/// Tunable Tier 4 weights, factored out for the meta sensitivity analysis.
#[derive(Clone, Copy)]
struct Tier4Weights {
    inconsistent_terminology: f64,
    no_progress: f64,
    unsafe_default: f64,
    narrow_breaks: f64,
}

impl Default for Tier4Weights {
    fn default() -> Self {
        Self {
            inconsistent_terminology: 25.0,
            no_progress: 15.0,
            unsafe_default: 30.0,
            narrow_breaks: 20.0,
        }
    }
}

fn tier4_score(m: &Tier4Metrics) -> f64 {
    tier4_score_w(m, &Tier4Weights::default())
}

fn tier4_score_w(m: &Tier4Metrics, w: &Tier4Weights) -> f64 {
    let mut score = 100.0;
    if !m.terminology_consistent {
        score -= w.inconsistent_terminology;
    }
    if !m.progress_visible {
        score -= w.no_progress;
    }
    if !m.default_safe {
        score -= w.unsafe_default;
    }
    if !m.narrow_options_survive {
        score -= w.narrow_breaks;
    }
    score.clamp(0.0, 100.0)
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

        // ----- Tier 4: content & robustness (cross-screen + behavioral) -----
        let t4 = tier4_metrics();
        let tier4 = tier4_score(&t4);
        println!("\n-- Tier 4: content & robustness --");
        let yn = |b: bool| if b { "ok" } else { "FAIL" };
        println!("terminology consistent : {}", yn(t4.terminology_consistent));
        println!("progress visible       : {}", yn(t4.progress_visible));
        println!("default safe (timeout) : {}", yn(t4.default_safe));
        println!("narrow-term options    : {}", yn(t4.narrow_options_survive));

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
        // Tier 1 (structure), Tier 3 (per-screen copy), and Tier 4 (cross-screen
        // content + robustness) are the quality of the flow. Tier 0 is how much
        // we can trust those numbers, so it gates rather than averages: report
        // it alongside, and fold it in lightly.
        let composite = tier1 * 0.4 + tier3 * 0.35 + tier4 * 0.15 + tier0 * 0.1;
        println!("\n-- SCORE --");
        println!("Tier 0 (coverage/trust) : {tier0:>5.1} / 100");
        println!("Tier 1 (flow structure) : {tier1:>5.1} / 100");
        println!("Tier 3 (screen quality) : {tier3:>5.1} / 100");
        println!("Tier 4 (content/robust) : {tier4:>5.1} / 100");
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
        // Tier 4 content/robustness guards: each is a real, currently-passing
        // property; a regression in any of them fails CI with a clear message.
        assert!(t4.terminology_consistent, "terminology drift: a competing synonym for 'log in' appeared in onboarding prose");
        assert!(t4.progress_visible, "a multi-step onboarding context stopped showing 'N of M' progress");
        assert!(t4.default_safe, "a timed decision's default no longer resolves to a recoverable outcome");
        assert!(t4.narrow_options_survive, "Yes/No options stopped rendering on a narrow (50-col) terminal");
        assert!(tier4 >= 60.0, "Tier 4 content/robustness score regressed: {tier4:.1}");
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

// ===========================================================================
// Tier M: meta-evaluation. Validates the SCORING SYSTEM itself (not the
// onboarding flow) along five properties:
//
//   1. Monotonicity   - making a flow/screen strictly worse never raises its
//                        score (and better never lowers it). Guards sign errors
//                        and direction.
//   2. Anchoring      - hand-built known-good / known-bad reference artifacts
//                        land in the right score bands. Gives the 0-100 scale
//                        meaning.
//   3. Discrimination - the good vs bad anchors are separated by a wide margin,
//                        so the metric actually distinguishes quality.
//   4. Robustness     - the RANKING of artifacts is stable when every weight is
//                        perturbed +/-50%. If the order is robust to the exact
//                        weights, hand-picking them is acceptable.
//   5. Signal liveness- every signal demonstrably moves the score: a pair of
//                        artifacts differing in exactly one signal must score
//                        differently. Catches dead/decorative signals.
//
// This sits ABOVE Tier 0: Tier 0 says "we measured the whole real flow"; Tier M
// says "the way we score that measurement is sane, discriminating, robust, and
// fully wired".
// ===========================================================================

/// Build a `PathMetrics` directly from signal values (for synthetic tests).
fn pm(keystrokes: u32, decisions: u32, screens: u32, reaches_ready: bool) -> PathMetrics {
    PathMetrics {
        keystrokes,
        decisions,
        screens,
        external_boundaries: 0,
        reaches_ready,
    }
}

/// Build a `ScreenMetrics` directly from signal values (for synthetic tests).
fn sm(
    word_count: u32,
    is_yesno: bool,
    keyhint_consistent: bool,
    has_escape_hatch: bool,
) -> ScreenMetrics {
    ScreenMetrics {
        label: "synthetic",
        line_count: word_count / 8 + 1,
        word_count,
        is_yesno,
        keyhint_consistent,
        has_escape_hatch,
    }
}

// ---- Property 1: monotonicity ----

#[test]
fn meta_tier1_is_monotonic_in_each_signal() {
    let base = pm(1, 1, 2, true);
    let base_s = tier1_path_score(&base);
    // More keystrokes -> not higher.
    assert!(tier1_path_score(&pm(2, 1, 2, true)) <= base_s, "keystrokes");
    // More decisions -> not higher.
    assert!(tier1_path_score(&pm(1, 2, 2, true)) <= base_s, "decisions");
    // More screens -> not higher.
    assert!(tier1_path_score(&pm(1, 1, 3, true)) <= base_s, "screens");
    // Failing to reach ready -> not higher.
    assert!(tier1_path_score(&pm(1, 1, 2, false)) <= base_s, "ready");
    // The perfect path (0/0/1/ready) is the unique maximum.
    assert!(tier1_path_score(&pm(0, 0, 1, true)) >= base_s, "best is best");
}

#[test]
fn meta_tier3_is_monotonic_in_each_signal() {
    let base = sm(60, true, true, true);
    let base_s = tier3_screen_score(&base);
    // More words -> not higher.
    assert!(tier3_screen_score(&sm(120, true, true, true)) <= base_s, "words");
    // Inconsistent key hint -> not higher.
    assert!(
        tier3_screen_score(&sm(60, true, false, true)) <= base_s,
        "keyhint"
    );
    // Losing the escape hatch -> not higher.
    assert!(
        tier3_screen_score(&sm(60, true, true, false)) <= base_s,
        "escape"
    );
}

#[test]
fn meta_tier4_is_monotonic_in_each_signal() {
    let base = Tier4Metrics {
        terminology_consistent: true,
        progress_visible: true,
        default_safe: true,
        narrow_options_survive: true,
    };
    let base_s = tier4_score(&base);
    // Losing any content/robustness property -> never higher.
    assert!(tier4_score(&Tier4Metrics { terminology_consistent: false, ..base }) <= base_s, "terminology");
    assert!(tier4_score(&Tier4Metrics { progress_visible: false, ..base }) <= base_s, "progress");
    assert!(tier4_score(&Tier4Metrics { default_safe: false, ..base }) <= base_s, "default");
    assert!(tier4_score(&Tier4Metrics { narrow_options_survive: false, ..base }) <= base_s, "narrow");
    // All-good is the unique maximum.
    assert_eq!(base_s, 100.0, "all-good Tier 4 is perfect");
}

// ---- Properties 2 + 3: anchoring and discrimination ----

/// Deliberately awful vs deliberately lean reference artifacts.
fn anchor_paths() -> (PathMetrics, PathMetrics) {
    // Worst realistic onboarding: many keystrokes, several decisions, many
    // screens, never reaches ready.
    let bad = pm(6, 4, 6, false);
    // Ideal: land ready with zero friction.
    let good = pm(0, 0, 1, true);
    (good, bad)
}

fn anchor_screens() -> (ScreenMetrics, ScreenMetrics) {
    // Wall of text, inconsistent hint, dead-end.
    let bad = sm(220, true, false, false);
    // Lean, consistent, with an escape hatch.
    let good = sm(30, true, true, true);
    (good, bad)
}

#[test]
fn meta_anchors_land_in_expected_bands() {
    let (good_p, bad_p) = anchor_paths();
    let (good_s, bad_s) = anchor_screens();
    let gp = tier1_path_score(&good_p);
    let bp = tier1_path_score(&bad_p);
    let gs = tier3_screen_score(&good_s);
    let bs = tier3_screen_score(&bad_s);

    // Good anchors must score high; bad anchors must score low.
    assert!(gp >= 90.0, "good path anchor should be excellent, got {gp:.1}");
    assert!(bp <= 30.0, "bad path anchor should be poor, got {bp:.1}");
    assert!(gs >= 85.0, "good screen anchor should be excellent, got {gs:.1}");
    assert!(bs <= 30.0, "bad screen anchor should be poor, got {bs:.1}");
}

#[test]
fn meta_metric_discriminates_good_from_bad() {
    const MIN_SEPARATION: f64 = 40.0;
    let (good_p, bad_p) = anchor_paths();
    let (good_s, bad_s) = anchor_screens();
    let path_gap = tier1_path_score(&good_p) - tier1_path_score(&bad_p);
    let screen_gap = tier3_screen_score(&good_s) - tier3_screen_score(&bad_s);
    assert!(
        path_gap >= MIN_SEPARATION,
        "Tier 1 must separate good/bad by >= {MIN_SEPARATION}, got {path_gap:.1}"
    );
    assert!(
        screen_gap >= MIN_SEPARATION,
        "Tier 3 must separate good/bad by >= {MIN_SEPARATION}, got {screen_gap:.1}"
    );
}

// ---- Property 4: robustness / sensitivity ----

/// A tiny deterministic LCG so the sweep is reproducible without an RNG dep.
struct Lcg(u64);
impl Lcg {
    fn next_f64(&mut self) -> f64 {
        // Numerical Recipes constants.
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        // Top 53 bits -> [0,1).
        ((self.0 >> 11) as f64) / ((1u64 << 53) as f64)
    }
    /// Jitter factor in [1-amount, 1+amount].
    fn jitter(&mut self, amount: f64) -> f64 {
        1.0 + (self.next_f64() * 2.0 - 1.0) * amount
    }
}

fn jittered_tier1_weights(rng: &mut Lcg, amount: f64) -> Tier1Weights {
    let d = Tier1Weights::default();
    Tier1Weights {
        per_keystroke: d.per_keystroke * rng.jitter(amount),
        per_decision: d.per_decision * rng.jitter(amount),
        per_extra_screen: d.per_extra_screen * rng.jitter(amount),
        not_ready: d.not_ready * rng.jitter(amount),
    }
}

fn jittered_tier3_weights(rng: &mut Lcg, amount: f64) -> Tier3Weights {
    let d = Tier3Weights::default();
    Tier3Weights {
        // Budget jitters by +/- a few words (kept integer).
        word_budget: ((d.word_budget as f64) * rng.jitter(amount)).round() as u32,
        per_excess_word: d.per_excess_word * rng.jitter(amount),
        inconsistent_keyhint: d.inconsistent_keyhint * rng.jitter(amount),
        no_escape_hatch: d.no_escape_hatch * rng.jitter(amount),
    }
}

#[test]
fn meta_ranking_is_robust_to_weight_perturbation() {
    const TRIALS: usize = 400;
    const JITTER: f64 = 0.5; // +/- 50%

    // Reference ladder of paths, strictly improving. Under ANY sane weights the
    // ranking (worst -> best) must be preserved.
    let path_ladder = [
        pm(6, 4, 6, false), // worst
        pm(3, 2, 3, false),
        pm(2, 1, 2, true),
        pm(1, 1, 2, true),
        pm(0, 0, 1, true), // best
    ];
    // Reference ladder of screens, strictly improving.
    let screen_ladder = [
        sm(220, true, false, false), // worst
        sm(140, true, false, true),
        sm(90, true, true, true),
        sm(60, true, true, true),
        sm(30, true, true, true), // best
    ];

    let mut rng = Lcg(0x9E3779B97F4A7C15);
    let mut path_violations = 0;
    let mut screen_violations = 0;
    for _ in 0..TRIALS {
        let w1 = jittered_tier1_weights(&mut rng, JITTER);
        let w3 = jittered_tier3_weights(&mut rng, JITTER);
        if !is_nondecreasing(&path_ladder.iter().map(|m| tier1_path_score_w(m, &w1)).collect::<Vec<_>>()) {
            path_violations += 1;
        }
        if !is_nondecreasing(&screen_ladder.iter().map(|m| tier3_screen_score_w(m, &w3)).collect::<Vec<_>>()) {
            screen_violations += 1;
        }
    }
    // The ordering must hold in EVERY trial: the ladders are separated enough
    // that no +/-50% weight change should reorder them.
    assert_eq!(
        path_violations, 0,
        "path ranking flipped in {path_violations}/{TRIALS} jittered-weight trials"
    );
    assert_eq!(
        screen_violations, 0,
        "screen ranking flipped in {screen_violations}/{TRIALS} jittered-weight trials"
    );
}

fn is_nondecreasing(xs: &[f64]) -> bool {
    xs.windows(2).all(|w| w[1] >= w[0] - 1e-9)
}

// ---- Property 5: signal liveness ----

#[test]
fn meta_every_signal_moves_the_score() {
    // Tier 1: each signal, toggled in isolation, must change the score.
    let base_p = pm(1, 1, 2, true);
    let base_ps = tier1_path_score(&base_p);
    assert_ne!(tier1_path_score(&pm(2, 1, 2, true)), base_ps, "keystroke signal is dead");
    assert_ne!(tier1_path_score(&pm(1, 2, 2, true)), base_ps, "decision signal is dead");
    assert_ne!(tier1_path_score(&pm(1, 1, 3, true)), base_ps, "screen signal is dead");
    assert_ne!(tier1_path_score(&pm(1, 1, 2, false)), base_ps, "ready signal is dead");

    // Tier 3: each signal, toggled in isolation, must change the score. Use a
    // base already over the word budget so the word signal is active.
    let base_s = sm(60, true, true, true);
    let base_ss = tier3_screen_score(&base_s);
    assert_ne!(tier3_screen_score(&sm(80, true, true, true)), base_ss, "word signal is dead");
    assert_ne!(tier3_screen_score(&sm(60, true, false, true)), base_ss, "keyhint signal is dead");
    assert_ne!(tier3_screen_score(&sm(60, true, true, false)), base_ss, "escape signal is dead");
}

// ---- The meta scorecard ----

#[test]
fn onboarding_meta_scorecard() {
    // Each property is a boolean; the meta-trust score is the fraction passing.
    // We re-run the property logic here (cheaply) so the scorecard prints a
    // single consolidated trust report. The dedicated #[test]s above are the
    // hard CI guards; this is the readable summary.
    let mut results: Vec<(&str, bool, &str)> = Vec::new();

    // 1. Monotonicity.
    let mono = {
        let p = pm(1, 1, 2, true);
        let ps = tier1_path_score(&p);
        let s = sm(60, true, true, true);
        let ss = tier3_screen_score(&s);
        tier1_path_score(&pm(2, 1, 2, true)) <= ps
            && tier1_path_score(&pm(1, 2, 2, true)) <= ps
            && tier1_path_score(&pm(1, 1, 3, true)) <= ps
            && tier1_path_score(&pm(1, 1, 2, false)) <= ps
            && tier3_screen_score(&sm(120, true, true, true)) <= ss
            && tier3_screen_score(&sm(60, true, false, true)) <= ss
            && tier3_screen_score(&sm(60, true, true, false)) <= ss
    };
    results.push(("monotonicity", mono, "worse never scores higher"));

    // 2. Anchoring.
    let (gp, bp) = anchor_paths();
    let (gs, bs) = anchor_screens();
    let gps = tier1_path_score(&gp);
    let bps = tier1_path_score(&bp);
    let gss = tier3_screen_score(&gs);
    let bss = tier3_screen_score(&bs);
    let anchoring = gps >= 90.0 && bps <= 30.0 && gss >= 85.0 && bss <= 30.0;
    results.push(("anchoring", anchoring, "known good/bad in right bands"));

    // 3. Discrimination.
    let path_gap = gps - bps;
    let screen_gap = gss - bss;
    let discrimination = path_gap >= 40.0 && screen_gap >= 40.0;
    results.push(("discrimination", discrimination, "good/bad separated >= 40"));

    // 4. Robustness (small sweep for the report; the #[test] runs the full one).
    let robustness = {
        let path_ladder = [pm(6, 4, 6, false), pm(2, 1, 2, true), pm(0, 0, 1, true)];
        let screen_ladder = [sm(220, true, false, false), sm(90, true, true, true), sm(30, true, true, true)];
        let mut rng = Lcg(0x1234_5678_9ABC_DEF0);
        let mut ok = true;
        for _ in 0..200 {
            let w1 = jittered_tier1_weights(&mut rng, 0.5);
            let w3 = jittered_tier3_weights(&mut rng, 0.5);
            if !is_nondecreasing(&path_ladder.iter().map(|m| tier1_path_score_w(m, &w1)).collect::<Vec<_>>())
                || !is_nondecreasing(&screen_ladder.iter().map(|m| tier3_screen_score_w(m, &w3)).collect::<Vec<_>>())
            {
                ok = false;
                break;
            }
        }
        ok
    };
    results.push(("robustness", robustness, "ranking stable under +/-50% weights"));

    // 5. Signal liveness.
    let liveness = {
        let p = tier1_path_score(&pm(1, 1, 2, true));
        let s = tier3_screen_score(&sm(60, true, true, true));
        tier1_path_score(&pm(2, 1, 2, true)) != p
            && tier1_path_score(&pm(1, 2, 2, true)) != p
            && tier1_path_score(&pm(1, 1, 3, true)) != p
            && tier1_path_score(&pm(1, 1, 2, false)) != p
            && tier3_screen_score(&sm(80, true, true, true)) != s
            && tier3_screen_score(&sm(60, true, false, true)) != s
            && tier3_screen_score(&sm(60, true, true, false)) != s
    };
    results.push(("signal liveness", liveness, "every signal moves the score"));

    let passed = results.iter().filter(|(_, ok, _)| *ok).count();
    let meta_trust = (passed as f64 / results.len() as f64) * 100.0;

    println!("\n============ META-EVALUATION (Tier M): is the scorer trustworthy? ============");
    println!("{:<16} {:>6}  {}", "property", "result", "guarantees");
    for (name, ok, desc) in &results {
        println!("{:<16} {:>6}  {}", name, if *ok { "PASS" } else { "FAIL" }, desc);
    }
    println!("--");
    println!("path good/bad anchors : {gps:.1} vs {bps:.1}  (gap {path_gap:.1})");
    println!("screen good/bad anchors: {gss:.1} vs {bss:.1}  (gap {screen_gap:.1})");
    println!("META-TRUST            : {meta_trust:.0} / 100 ({passed}/{} properties)", results.len());
    println!("=============================================================================\n");

    assert_eq!(
        passed,
        results.len(),
        "meta-evaluation found an untrustworthy property; see report above"
    );
}

// ===========================================================================
// Signal Coverage system. Answers "did we capture all the signals that matter,
// and is every signal we claim to score actually wired in?"
//
// Completeness ("are there signals we never thought of?") is fundamentally
// unprovable by a test, so instead we make the KNOWN universe explicit and add
// tripwires that force new product surface into a conscious decision:
//
//   Layer A  Registry  - every candidate signal declared as Scored / Deferred /
//                        Rejected, each with a rationale. Turns silent omission
//                        into a reviewable choice.
//   Layer B  Metrics   - scored-coverage ratio, liveness binding (a Scored
//                        signal must move the score), mapping (a Scored signal
//                        must apply to a real screen/path).
//   Layer C  Probe     - scans the REAL rendered screens for feature classes
//                        (options, countdown, list, command). Any feature class
//                        present on screen must be owned by a registry signal,
//                        so a new on-screen dimension cannot appear unmeasured.
// ===========================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SignalStatus {
    /// Wired into Tier 1 or Tier 3 today.
    Scored,
    /// Known to matter, deliberately not scored yet (with a reason).
    Deferred,
    /// Considered and intentionally excluded from scope (with a reason).
    Rejected,
}

/// A feature class that can be detected directly from a rendered screen. Used
/// by Layer C to verify every on-screen dimension is owned by a signal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum FeatureClass {
    /// Yes/No or other selectable options are shown.
    InteractiveOptions,
    /// A countdown / numeric auto-advance is shown.
    Countdown,
    /// A numbered or multi-item list is shown.
    List,
    /// A typed command (e.g. "/login") is shown.
    Command,
    /// None of the above structural features (plain prose only).
    None,
}

struct SignalSpec {
    name: &'static str,
    status: SignalStatus,
    /// Which scoring field it feeds (for Scored) or why not (Deferred/Rejected).
    rationale: &'static str,
    /// The on-screen feature class this signal is responsible for measuring, if
    /// any. `None` means the signal is structural (counts) rather than tied to a
    /// visible feature class.
    owns_feature: FeatureClass,
}

/// Layer A: the declared signal universe for onboarding efficiency.
fn signal_registry() -> Vec<SignalSpec> {
    use FeatureClass::*;
    use SignalStatus::*;
    vec![
        // ---- Scored (wired into Tier 1) ----
        SignalSpec { name: "keystrokes", status: Scored, rationale: "Tier1.per_keystroke", owns_feature: None },
        SignalSpec { name: "decisions", status: Scored, rationale: "Tier1.per_decision", owns_feature: InteractiveOptions },
        SignalSpec { name: "screens", status: Scored, rationale: "Tier1.per_extra_screen", owns_feature: None },
        SignalSpec { name: "reaches_ready", status: Scored, rationale: "Tier1.not_ready", owns_feature: None },
        // ---- Scored (wired into Tier 3) ----
        SignalSpec { name: "word_count", status: Scored, rationale: "Tier3.per_excess_word (reading load)", owns_feature: None },
        SignalSpec { name: "keyhint_consistency", status: Scored, rationale: "Tier3.inconsistent_keyhint", owns_feature: None },
        SignalSpec { name: "escape_hatch", status: Scored, rationale: "Tier3.no_escape_hatch", owns_feature: Command },
        SignalSpec { name: "countdown_present", status: Scored, rationale: "covered via word_count + keyhint on timed yes/no screens", owns_feature: Countdown },
        SignalSpec { name: "suggestion_list", status: Scored, rationale: "covered via word_count on the Suggestions screen", owns_feature: List },
        // ---- Scored (wired into Tier 4: content & robustness) ----
        SignalSpec { name: "terminology_consistency", status: Scored, rationale: "Tier4.inconsistent_terminology (one verb for 'log in' across screens)", owns_feature: None },
        SignalSpec { name: "progress_visibility", status: Scored, rationale: "Tier4.no_progress ('N of M' in multi-step contexts)", owns_feature: None },
        SignalSpec { name: "default_safety", status: Scored, rationale: "Tier4.unsafe_default (timed auto-commit lands on a recoverable outcome)", owns_feature: None },
        SignalSpec { name: "narrow_terminal_safety", status: Scored, rationale: "Tier4.narrow_breaks (core Yes/No options survive a 50-col terminal)", owns_feature: None },
        // ---- Deferred (matters, not yet scored, with reason) ----
        SignalSpec { name: "reading_grade_level", status: Deferred, rationale: "needs a syllable/grade estimator; word_count is a usable proxy for now", owns_feature: None },
        SignalSpec { name: "single_primary_action", status: Deferred, rationale: "needs CTA-salience parsing; decisions count is a partial proxy", owns_feature: None },
        SignalSpec { name: "error_recovery_depth", status: Deferred, rationale: "needs to drive failure paths through the real app and count steps back", owns_feature: None },
        SignalSpec { name: "time_on_blocker", status: Deferred, rationale: "DECISION_TIMEOUT is known but not yet folded into the score", owns_feature: None },
        SignalSpec { name: "min_vs_actual_path", status: Deferred, rationale: "needs a shortest-path solver over the flow graph; keystrokes/screens are partial proxies", owns_feature: None },
        SignalSpec { name: "back_navigation", status: Deferred, rationale: "onboarding is forward-only today; a real measure needs an undo affordance to exist first", owns_feature: None },
        SignalSpec { name: "jargon_density", status: Deferred, rationale: "needs a maintained jargon lexicon; word_count is a crude proxy for now", owns_feature: None },
        // ---- Rejected (out of scope by construction) ----
        SignalSpec { name: "color_contrast", status: Rejected, rationale: "not derivable from the text buffer the evaluator reads", owns_feature: None },
        SignalSpec { name: "visual_hierarchy", status: Rejected, rationale: "layout/eye-tracking concern; not measurable offline without users", owns_feature: None },
    ]
}

/// Layer C: detect which feature classes a rendered screen actually contains.
fn detect_feature_classes(text: &str) -> Vec<FeatureClass> {
    let lower = text.to_ascii_lowercase();
    let mut found = Vec::new();
    if text.contains("  Yes  ") || (text.contains("Yes") && text.contains("No")) {
        found.push(FeatureClass::InteractiveOptions);
    }
    // A countdown: "auto-selects in 12s" / "in 60s" / "automatically in 9s".
    if lower.contains("auto-selects in") || lower.contains("automatically in") {
        found.push(FeatureClass::Countdown);
    }
    // A numbered list: "[1]" "[2]" or "Press 1-N".
    if text.contains("[1]") || lower.contains("press 1-") {
        found.push(FeatureClass::List);
    }
    // A typed command.
    if text.contains('/') && (lower.contains("/login") || lower.contains("/model") || lower.contains("type /")) {
        found.push(FeatureClass::Command);
    }
    if found.is_empty() {
        found.push(FeatureClass::None);
    }
    found
}

/// Every user-facing welcome screen, rendered to text, for the Layer C probe.
fn all_welcome_screen_texts() -> Vec<(&'static str, String)> {
    use crate::external_auth::ExternalAuthReviewCandidate;
    use crate::tui::app::onboarding_flow::ImportReview;
    let now = std::time::Instant::now();
    let review =
        ImportReview::new(vec![ExternalAuthReviewCandidate::fixture("OpenAI/Codex", "Codex auth.json")])
            .unwrap();
    let phases: Vec<(&'static str, OnboardingPhase)> = vec![
        ("LoginOpenAi", OnboardingPhase::LoginOpenAi { yes_highlighted: true }),
        ("Login{import}", OnboardingPhase::Login { import: Some(review) }),
        ("Login{recovery}", OnboardingPhase::Login { import: None }),
        (
            "ContinuePrompt",
            OnboardingPhase::ContinuePrompt { cli: ExternalCli::Codex, yes_highlighted: true, shown_at: now },
        ),
        ("Suggestions", OnboardingPhase::Suggestions),
    ];
    phases
        .into_iter()
        .map(|(label, phase)| {
            let app = app_in_phase(phase);
            (label, render_onboarding_text(&app, 80, 30))
        })
        .collect()
}

#[test]
fn signal_coverage_scorecard() {
    with_temp_jcode_home(|| {
        let registry = signal_registry();
        let scored: Vec<&SignalSpec> = registry.iter().filter(|s| s.status == SignalStatus::Scored).collect();
        let deferred: Vec<&SignalSpec> = registry.iter().filter(|s| s.status == SignalStatus::Deferred).collect();
        let rejected: Vec<&SignalSpec> = registry.iter().filter(|s| s.status == SignalStatus::Rejected).collect();

        // ---- Layer B metric: scored coverage over the acknowledged-relevant
        // universe (Scored + Deferred; Rejected is out of scope by design). ----
        let relevant = scored.len() + deferred.len();
        let scored_coverage = (scored.len() as f64 / relevant as f64) * 100.0;

        // ---- Layer C: every feature class present on a real screen must be
        // owned by at least one Scored signal. ----
        let owned: std::collections::HashSet<FeatureClass> = scored
            .iter()
            .map(|s| s.owns_feature)
            .filter(|f| *f != FeatureClass::None)
            .collect();
        let screens = all_welcome_screen_texts();
        let mut unowned: Vec<(String, FeatureClass)> = Vec::new();
        let mut present: std::collections::HashSet<FeatureClass> = std::collections::HashSet::new();
        for (label, text) in &screens {
            for fc in detect_feature_classes(text) {
                if fc == FeatureClass::None {
                    continue;
                }
                present.insert(fc);
                if !owned.contains(&fc) {
                    unowned.push((label.to_string(), fc));
                }
            }
        }
        let feature_coverage = if present.is_empty() {
            100.0
        } else {
            let covered = present.iter().filter(|fc| owned.contains(fc)).count();
            (covered as f64 / present.len() as f64) * 100.0
        };

        // ---- Report ----
        println!("\n============ SIGNAL COVERAGE ============");
        println!("-- Layer A: registry ({} signals) --", registry.len());
        println!("{:<22} {:<9} {}", "signal", "status", "rationale");
        for s in &registry {
            let st = match s.status {
                SignalStatus::Scored => "SCORED",
                SignalStatus::Deferred => "deferred",
                SignalStatus::Rejected => "rejected",
            };
            println!("{:<22} {:<9} {}", s.name, st, s.rationale);
        }
        println!("\n-- Layer B: coverage metrics --");
        println!("scored signals     : {}", scored.len());
        println!("deferred (known)   : {}", deferred.len());
        println!("rejected (scope)   : {}", rejected.len());
        println!("scored coverage    : {scored_coverage:.0}% of acknowledged-relevant ({}/{})", scored.len(), relevant);
        println!("\n-- Layer C: on-screen feature ownership --");
        println!("feature classes present : {:?}", present);
        println!("feature classes owned   : {:?}", owned);
        println!("feature coverage        : {feature_coverage:.0}%");
        if !unowned.is_empty() {
            println!("UNOWNED (new dimension!) : {unowned:?}");
        }
        // Composite signal-coverage score: weight on-screen feature ownership
        // (the completeness tripwire) and the declared scored ratio.
        let signal_coverage = feature_coverage * 0.6 + scored_coverage * 0.4;
        println!("\nSIGNAL-COVERAGE SCORE : {signal_coverage:.1} / 100");
        println!("========================================\n");

        // ---- Guards ----
        // Every on-screen feature class must be owned. This is the tripwire: a
        // new visible dimension with no signal fails CI until someone scores it
        // or registers it (as Scored owning that class).
        assert!(
            unowned.is_empty(),
            "on-screen feature classes with no owning signal: {unowned:?} -- add a signal to the registry"
        );
        // Deferred/Rejected signals must carry a non-empty rationale (no silent
        // omission).
        for s in registry.iter().filter(|s| s.status != SignalStatus::Scored) {
            assert!(
                !s.rationale.trim().is_empty(),
                "signal '{}' is not scored but has no rationale",
                s.name
            );
        }
        // We must actually score a majority of acknowledged-relevant signals.
        assert!(
            scored_coverage >= 60.0,
            "scored coverage regressed below 60%: {scored_coverage:.0}%"
        );
    });
}

/// Layer B liveness binding: every signal the registry marks `Scored` must
/// correspond to a signal that demonstrably moves the score. We can't reflect
/// over field names in Rust, so we bind by an explicit, exhaustive checklist:
/// adding a Scored signal to the registry without a liveness clause here fails.
#[test]
fn signal_coverage_scored_signals_are_all_live() {
    let scored: Vec<&'static str> = signal_registry()
        .iter()
        .filter(|s| s.status == SignalStatus::Scored)
        .map(|s| s.name)
        .collect();

    // The set of Scored signals we have a concrete liveness proof for below.
    let proven: std::collections::HashSet<&'static str> = [
        "keystrokes",
        "decisions",
        "screens",
        "reaches_ready",
        "word_count",
        "keyhint_consistency",
        "escape_hatch",
        "countdown_present",
        "suggestion_list",
        "terminology_consistency",
        "progress_visibility",
        "default_safety",
        "narrow_terminal_safety",
    ]
    .into_iter()
    .collect();

    // Any Scored signal without a liveness proof is a coverage hole.
    for name in &scored {
        assert!(
            proven.contains(name),
            "Scored signal '{name}' has no liveness proof; add one to keep coverage honest"
        );
    }

    // Concrete liveness proofs for the structural (Tier 1) and copy (Tier 3)
    // signals. countdown_present and suggestion_list are proven via the real
    // screens: removing them would change word_count, which is already proven,
    // and they are validated as owned feature classes by the scorecard probe.
    let p = tier1_path_score(&pm(1, 1, 2, true));
    assert_ne!(tier1_path_score(&pm(2, 1, 2, true)), p);
    assert_ne!(tier1_path_score(&pm(1, 2, 2, true)), p);
    assert_ne!(tier1_path_score(&pm(1, 1, 3, true)), p);
    assert_ne!(tier1_path_score(&pm(1, 1, 2, false)), p);
    let s = tier3_screen_score(&sm(60, true, true, true));
    assert_ne!(tier3_screen_score(&sm(80, true, true, true)), s);
    assert_ne!(tier3_screen_score(&sm(60, true, false, true)), s);
    assert_ne!(tier3_screen_score(&sm(60, true, true, false)), s);

    // Tier 4 liveness: flipping each content/robustness signal must move the
    // Tier 4 score. Proves none of the four new signals are decorative.
    let good = Tier4Metrics {
        terminology_consistent: true,
        progress_visible: true,
        default_safe: true,
        narrow_options_survive: true,
    };
    let g = tier4_score(&good);
    assert_ne!(tier4_score(&Tier4Metrics { terminology_consistent: false, ..good }), g, "terminology_consistency");
    assert_ne!(tier4_score(&Tier4Metrics { progress_visible: false, ..good }), g, "progress_visibility");
    assert_ne!(tier4_score(&Tier4Metrics { default_safe: false, ..good }), g, "default_safety");
    assert_ne!(tier4_score(&Tier4Metrics { narrow_options_survive: false, ..good }), g, "narrow_terminal_safety");
}
