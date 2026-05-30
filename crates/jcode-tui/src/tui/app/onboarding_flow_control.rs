//! Control logic / phase transitions for the first-run onboarding flow.
//!
//! See [`super::onboarding_flow`] for the phase definitions. This module hangs
//! the driving methods off `App` so the rest of the TUI can advance the flow in
//! response to login, model selection, key presses, and the auto-advance timer.

use super::onboarding_flow::{
    ExternalCli, OnboardingFlow, OnboardingPhase, detect_external_cli_oauth,
};
use super::{App, DisplayMessage, SessionPickerMode};
use crate::tui::session_picker::{self, SessionFilterMode, SessionPicker};
use crossterm::event::KeyCode;
use std::cell::RefCell;
use std::time::Instant;

impl App {
    /// Whether the guided onboarding flow is currently driving the UI.
    pub(super) fn onboarding_flow_active(&self) -> bool {
        self.onboarding_flow
            .as_ref()
            .map(OnboardingFlow::is_active)
            .unwrap_or(false)
    }

    /// The current onboarding phase, if the flow is active.
    pub(super) fn onboarding_phase(&self) -> Option<&OnboardingPhase> {
        self.onboarding_flow
            .as_ref()
            .filter(|flow| flow.is_active())
            .map(|flow| &flow.phase)
    }

    /// Gate + start the flow after a successful login. Only fires for brand-new
    /// users (no prior onboarding flow this session) so returning users who
    /// re-auth aren't dragged through onboarding.
    pub(super) fn maybe_begin_onboarding_flow_after_login(&mut self) {
        if self.onboarding_flow.is_some() {
            return;
        }
        if self.is_remote {
            return;
        }
        if !self.onboarding_preview_mode && !self.is_new_user_for_onboarding() {
            return;
        }
        self.begin_onboarding_flow();
    }

    /// One-shot startup check: the fresh-install path logs the user in at the CLI
    /// *before* the TUI launches, so no in-TUI login event ever fires. If we boot
    /// already authenticated as a brand-new user, kick the guided flow here.
    ///
    /// Returns without committing the one-shot guard until auth is actually
    /// resolved (the server may still be bootstrapping on the first ticks), so a
    /// momentary "not yet authenticated" reading doesn't permanently skip the
    /// flow. Once we either start the flow or conclude it shouldn't run, the
    /// guard is set and this becomes a no-op for the rest of the session.
    pub(super) fn maybe_begin_onboarding_flow_on_startup(&mut self) {
        if self.onboarding_startup_checked {
            return;
        }
        if self.onboarding_flow.is_some() || self.is_remote {
            self.onboarding_startup_checked = true;
            return;
        }
        // Don't hijack a session that already has activity (resume, restored
        // input, or anything already on screen). These are settled states, so we
        // can commit the guard.
        if !self.display_messages.is_empty() || self.is_processing || !self.input.is_empty() {
            self.onboarding_startup_checked = true;
            return;
        }
        if !self.is_new_user_for_onboarding() {
            self.onboarding_startup_checked = true;
            return;
        }
        // Only start once the user actually has working credentials. If auth
        // isn't resolved yet (server still bootstrapping), leave the guard unset
        // and retry on a later tick rather than permanently skipping the flow.
        if !crate::auth::AuthStatus::check_fast().has_any_available() {
            return;
        }
        self.onboarding_startup_checked = true;
        self.begin_onboarding_flow();
    }

    /// Whether this install looks like a brand-new user (few launches).
    fn is_new_user_for_onboarding(&self) -> bool {
        crate::storage::jcode_dir()
            .ok()
            .and_then(|dir| std::fs::read_to_string(dir.join("setup_hints.json")).ok())
            .and_then(|content| serde_json::from_str::<serde_json::Value>(&content).ok())
            .and_then(|v| v.get("launch_count")?.as_u64())
            .map(|count| count <= 5)
            .unwrap_or(true)
    }

    /// Begin the guided flow at the model-selection phase. Called once auth
    /// becomes available on a fresh install (login/import completes).
    ///
    /// No-op if a flow is already running or the user is experienced.
    pub(super) fn begin_onboarding_flow(&mut self) {
        if self.onboarding_flow.is_some() {
            return;
        }
        self.onboarding_flow = Some(OnboardingFlow::begin());
        self.push_display_message(DisplayMessage::system(
            "You're set up. Pick a model to get started (Enter to open the model picker)."
                .to_string(),
        ));
        self.set_status_notice("Onboarding: choose a model");
    }

    /// Advance out of the model-selection phase once a model has been chosen.
    /// Decides whether to offer "continue where you left off" based on detected
    /// external Codex / Claude Code OAuth logins.
    pub(super) fn onboarding_after_model_select(&mut self) {
        if !matches!(self.onboarding_phase(), Some(OnboardingPhase::ModelSelect)) {
            return;
        }
        match detect_external_cli_oauth() {
            Some(cli) => self.onboarding_enter_continue_prompt(cli),
            None => self.onboarding_show_suggestions(),
        }
    }

    /// Enter the "Continue where you left off?" phase with a 10s auto-Yes.
    fn onboarding_enter_continue_prompt(&mut self, cli: ExternalCli) {
        if let Some(flow) = self.onboarding_flow.as_mut() {
            flow.phase = OnboardingPhase::ContinuePrompt {
                cli,
                shown_at: Instant::now(),
            };
        }
        self.push_display_message(DisplayMessage::system(format!(
            "Continue where you left off in {}? [Y] yes  [N] no  (auto-continues in 10s)",
            cli.label()
        )));
        self.set_status_notice(format!("Continue in {}?", cli.label()));
    }

    /// Answer the continue prompt. `true` -> open the transcript picker;
    /// `false` -> fall through to the suggestion cards.
    pub(super) fn onboarding_answer_continue(&mut self, wants_continue: bool) {
        let cli = match self.onboarding_phase() {
            Some(OnboardingPhase::ContinuePrompt { cli, .. }) => *cli,
            _ => return,
        };
        if wants_continue {
            self.onboarding_open_transcript_picker(cli);
        } else {
            self.onboarding_show_suggestions();
        }
    }

    /// Intercept Y/N/Enter/Esc while the "continue where you left off?" prompt
    /// is showing. Returns true if the key was consumed.
    pub(super) fn handle_onboarding_continue_prompt_key(&mut self, code: KeyCode) -> bool {
        if !matches!(
            self.onboarding_phase(),
            Some(OnboardingPhase::ContinuePrompt { .. })
        ) {
            return false;
        }
        match code {
            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                self.onboarding_answer_continue(true);
                true
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                self.onboarding_answer_continue(false);
                true
            }
            _ => false,
        }
    }

    /// Open a single-select resume-style picker filtered to the external CLI's
    /// transcripts. Falls back to the session-search prompt if none load.
    pub(super) fn onboarding_open_transcript_picker(&mut self, cli: ExternalCli) {
        let filter = match cli {
            ExternalCli::Codex => SessionFilterMode::Codex,
            ExternalCli::ClaudeCode => SessionFilterMode::ClaudeCode,
        };

        let (server_groups, orphan_sessions) = match session_picker::load_sessions_grouped() {
            Ok(loaded) => loaded,
            Err(err) => {
                crate::logging::error(&format!(
                    "onboarding: failed to load {} sessions: {err}",
                    cli.label()
                ));
                self.onboarding_fallback_to_session_search(cli);
                return;
            }
        };

        let mut picker = SessionPicker::new_grouped(server_groups, orphan_sessions);
        picker.activate_external_cli_filter(filter);

        if picker.visible_session_count() == 0 {
            self.onboarding_fallback_to_session_search(cli);
            return;
        }

        self.session_picker_overlay = Some(RefCell::new(picker));
        self.session_picker_mode = SessionPickerMode::Onboarding { cli };
        if let Some(flow) = self.onboarding_flow.as_mut() {
            flow.phase = OnboardingPhase::TranscriptPick {
                cli,
                shown_at: Instant::now(),
            };
        }
        self.set_status_notice(format!(
            "Pick a {} session to continue (auto-selects latest in 10s)",
            cli.label()
        ));
    }

    /// Auto-select the most recent transcript in the onboarding picker (called
    /// on the 10s timeout). Falls back to session-search if nothing resolves.
    pub(super) fn onboarding_auto_select_latest_transcript(&mut self, cli: ExternalCli) {
        let target = self
            .session_picker_overlay
            .as_ref()
            .and_then(|cell| cell.borrow().latest_visible_resume_target());

        match target {
            Some(target) => {
                self.session_picker_overlay = None;
                self.handle_session_picker_current_terminal_selection(&[target]);
                self.onboarding_finish();
            }
            None => {
                self.session_picker_overlay = None;
                self.onboarding_fallback_to_session_search(cli);
            }
        }
    }

    /// Fallback: seed the input with a prompt asking the agent to session-search
    /// the latest external session and continue, then submit it.
    pub(super) fn onboarding_fallback_to_session_search(&mut self, cli: ExternalCli) {
        let prompt = format!(
            "Use session search to find my most recent {} session, summarize what we were \
             working on, then continue from exactly where we left off.",
            cli.label()
        );
        self.push_display_message(DisplayMessage::system(format!(
            "Couldn't open your {} transcripts directly. Asking the agent to find and continue \
             your latest session instead.",
            cli.label()
        )));
        self.onboarding_finish();
        self.input = prompt;
        self.cursor_pos = self.input.len();
        self.submit_input();
    }

    /// Drop into the suggestion-card state (the "No" / no-OAuth path). Prints
    /// the same starter prompts the empty-screen welcome offers, as an inline
    /// numbered list the user can pick by typing the number or anything else.
    pub(super) fn onboarding_show_suggestions(&mut self) {
        if let Some(flow) = self.onboarding_flow.as_mut() {
            flow.phase = OnboardingPhase::Suggestions;
        }
        let suggestions = self.suggestion_prompts();
        if suggestions.is_empty() {
            self.onboarding_finish();
            self.set_status_notice("You're all set, type anything to start");
            return;
        }
        let mut body = String::from("Here are a few things you can try:\n");
        for (i, (label, _prompt)) in suggestions.iter().enumerate() {
            body.push_str(&format!("  [{}] {}\n", i + 1, label));
        }
        body.push_str(&format!(
            "Press 1-{} to use one, or just type anything to start.",
            suggestions.len()
        ));
        self.push_display_message(DisplayMessage::system(body));
        self.set_status_notice("Try a suggestion, or type anything to start");
    }

    /// Mark the flow complete; the normal UI takes over.
    pub(super) fn onboarding_finish(&mut self) {
        if let Some(flow) = self.onboarding_flow.as_mut() {
            flow.phase = OnboardingPhase::Done;
        }
    }

    /// Drive auto-advancing phases. Call once per tick/redraw. Returns true if
    /// the flow state changed (so the caller can request a redraw).
    pub(super) fn onboarding_tick(&mut self) -> bool {
        // Fresh-install bootstrap: if we were already logged in at the CLI before
        // the TUI launched, no in-TUI login event fired, so evaluate (once)
        // whether to begin the guided flow now that the TUI is up.
        let mut changed = false;
        if !self.onboarding_startup_checked {
            self.maybe_begin_onboarding_flow_on_startup();
            // If startup just kicked the flow on, request a redraw.
            changed = self.onboarding_flow_active();
        }
        if !self.onboarding_flow_active() {
            return changed;
        }
        let due = self
            .onboarding_flow
            .as_ref()
            .map(OnboardingFlow::auto_advance_due)
            .unwrap_or(false);
        if !due {
            // Keep the countdown visible on the timed phases.
            if let Some(remaining) = self
                .onboarding_flow
                .as_ref()
                .and_then(OnboardingFlow::auto_advance_remaining)
            {
                match self.onboarding_phase() {
                    Some(OnboardingPhase::ContinuePrompt { cli, .. }) => {
                        let label = cli.label();
                        self.set_status_notice(format!(
                            "Continue in {label}? auto-continues in {remaining}s ([Y]/[N])"
                        ));
                        return true;
                    }
                    Some(OnboardingPhase::TranscriptPick { .. }) => {
                        self.set_status_notice(format!(
                            "Pick a session to continue (auto-selects latest in {remaining}s)"
                        ));
                        return true;
                    }
                    _ => {}
                }
            }
            return false;
        }
        match self.onboarding_phase().cloned() {
            Some(OnboardingPhase::ContinuePrompt { cli, .. }) => {
                // Default action on timeout is "yes, continue".
                self.onboarding_open_transcript_picker(cli);
                true
            }
            Some(OnboardingPhase::TranscriptPick { cli, .. }) => {
                self.onboarding_auto_select_latest_transcript(cli);
                true
            }
            _ => false,
        }
    }
}
