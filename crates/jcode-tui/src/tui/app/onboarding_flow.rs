//! First-run onboarding flow state machine.
//!
//! After the user logs in / imports credentials on a fresh install, we walk
//! them through a short guided flow:
//!
//!   1. `ModelSelect`     - let them pick a model.
//!   2. `ContinuePrompt`  - if we detect an external Codex or Claude Code
//!                          OAuth login, ask whether they want to continue
//!                          where they last left off. Auto-selects "Yes" after
//!                          [`AUTO_ADVANCE`] if they don't choose.
//!   3. `TranscriptPick`  - render their recent external transcripts in a
//!                          resume-style picker (single select). Auto-selects
//!                          the most recent after [`AUTO_ADVANCE`].
//!   4. `Suggestions`     - the existing prompt-suggestion cards. Reached when
//!                          they answer "No", when there is no external OAuth,
//!                          or as the terminal resting state.
//!
//! If anything fails along the continue path (no transcripts, load error,
//! resume failure) we fall back to seeding the input with a prompt that asks
//! the agent to session-search the latest Codex/Claude Code session and
//! continue from there.

use std::path::PathBuf;
use std::time::{Duration, Instant};

/// How long we wait on an auto-advancing phase before choosing the default.
pub(crate) const AUTO_ADVANCE: Duration = Duration::from_secs(10);

/// Which external CLI an OAuth login was detected for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExternalCli {
    Codex,
    ClaudeCode,
}

impl ExternalCli {
    pub(crate) fn label(self) -> &'static str {
        match self {
            ExternalCli::Codex => "Codex",
            ExternalCli::ClaudeCode => "Claude Code",
        }
    }
}

/// The current phase of the onboarding flow.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum OnboardingPhase {
    /// Pick a model. Entered right after login/import.
    ModelSelect,
    /// "Continue where you left off in <cli>?" Yes/No with a 10s auto-Yes.
    ContinuePrompt {
        cli: ExternalCli,
        /// When the prompt was shown (for the auto-advance countdown).
        shown_at: Instant,
    },
    /// Single-select transcript picker with a 10s auto-select of the latest.
    TranscriptPick {
        cli: ExternalCli,
        shown_at: Instant,
    },
    /// Existing prompt-suggestion cards (resting / "No" state).
    Suggestions,
    /// Flow finished; nothing onboarding-specific to render.
    Done,
}

/// Runtime state for the onboarding flow. `None`/`Done` means inactive.
#[derive(Clone, Debug)]
pub(crate) struct OnboardingFlow {
    pub(crate) phase: OnboardingPhase,
}

impl OnboardingFlow {
    /// Start the flow at the model-selection phase.
    pub(crate) fn begin() -> Self {
        Self {
            phase: OnboardingPhase::ModelSelect,
        }
    }

    /// Whether the flow is actively driving the UI.
    pub(crate) fn is_active(&self) -> bool {
        !matches!(self.phase, OnboardingPhase::Done)
    }

    /// Seconds remaining before the current auto-advancing phase fires, if any.
    pub(crate) fn auto_advance_remaining(&self) -> Option<u64> {
        let shown_at = match &self.phase {
            OnboardingPhase::ContinuePrompt { shown_at, .. } => *shown_at,
            OnboardingPhase::TranscriptPick { shown_at, .. } => *shown_at,
            _ => return None,
        };
        let elapsed = shown_at.elapsed();
        Some(AUTO_ADVANCE.saturating_sub(elapsed).as_secs())
    }

    /// Whether the current auto-advancing phase has timed out.
    pub(crate) fn auto_advance_due(&self) -> bool {
        let shown_at = match &self.phase {
            OnboardingPhase::ContinuePrompt { shown_at, .. } => *shown_at,
            OnboardingPhase::TranscriptPick { shown_at, .. } => *shown_at,
            _ => return false,
        };
        shown_at.elapsed() >= AUTO_ADVANCE
    }
}

/// Detect whether an external Codex or Claude Code OAuth login is present.
///
/// Prefers Codex when both exist (it's first in the prompt), but either being
/// present is enough to offer the "continue where you left off" phase.
pub(crate) fn detect_external_cli_oauth() -> Option<ExternalCli> {
    let home = home_dir()?;
    if external_oauth_present(&home.join(".codex/auth.json")) {
        return Some(ExternalCli::Codex);
    }
    if external_oauth_present(&home.join(".claude/.credentials.json")) {
        return Some(ExternalCli::ClaudeCode);
    }
    None
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// A credentials file counts as an OAuth login when it exists and is non-empty.
fn external_oauth_present(path: &PathBuf) -> bool {
    std::fs::metadata(path)
        .map(|meta| meta.is_file() && meta.len() > 0)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flow_starts_at_model_select_and_is_active() {
        let flow = OnboardingFlow::begin();
        assert_eq!(flow.phase, OnboardingPhase::ModelSelect);
        assert!(flow.is_active());
    }

    #[test]
    fn done_phase_is_inactive() {
        let flow = OnboardingFlow {
            phase: OnboardingPhase::Done,
        };
        assert!(!flow.is_active());
    }

    #[test]
    fn auto_advance_none_outside_timed_phases() {
        let flow = OnboardingFlow::begin();
        assert_eq!(flow.auto_advance_remaining(), None);
        assert!(!flow.auto_advance_due());
    }

    #[test]
    fn continue_prompt_counts_down_and_times_out() {
        let past = Instant::now() - (AUTO_ADVANCE + Duration::from_secs(1));
        let flow = OnboardingFlow {
            phase: OnboardingPhase::ContinuePrompt {
                cli: ExternalCli::Codex,
                shown_at: past,
            },
        };
        assert_eq!(flow.auto_advance_remaining(), Some(0));
        assert!(flow.auto_advance_due());
    }

    #[test]
    fn fresh_continue_prompt_has_remaining_time() {
        let flow = OnboardingFlow {
            phase: OnboardingPhase::ContinuePrompt {
                cli: ExternalCli::ClaudeCode,
                shown_at: Instant::now(),
            },
        };
        let remaining = flow.auto_advance_remaining().unwrap();
        assert!(remaining >= 8 && remaining <= 10);
        assert!(!flow.auto_advance_due());
    }

    #[test]
    fn external_oauth_present_requires_nonempty_file() {
        let dir = std::env::temp_dir().join(format!("jcode-onb-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let empty = dir.join("empty.json");
        let full = dir.join("full.json");
        std::fs::write(&empty, b"").unwrap();
        std::fs::write(&full, b"{\"token\":\"x\"}").unwrap();
        assert!(!external_oauth_present(&empty));
        assert!(external_oauth_present(&full));
        assert!(!external_oauth_present(&dir.join("missing.json")));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
