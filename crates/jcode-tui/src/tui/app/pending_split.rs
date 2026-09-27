//! A pending launch of the current work into a new sibling session.
//!
//! The server models every new-session launch as a split: a fresh sibling, a
//! transfer, a review/autoreview spawn, and a workspace add all send `split()`
//! and receive `SplitResponse`; they differ only in `label` and in what payload
//! is attached. The fields below are set by those paths, consumed by the
//! `SplitResponse` handler, and executed by the run loop when `request` or
//! `transfer_request` is set.
//!
//! There is no cross-field invariant, so the fields are plain `pub(super)` reads
//! and writes. The one thing that must stay in step is the payload: it is armed
//! together at every site and dropped together on success or failure, so
//! `clear_payload`/`take_payload` are the only multi-field operations.

use std::time::Instant;

/// Startup user prompt to auto-submit in the new session.
#[derive(Debug, Clone)]
pub(super) struct PendingSplitPrompt {
    pub(super) content: String,
    pub(super) images: Vec<(String, String)>,
}

/// The launch payload, taken as a unit by the `SplitResponse` handler.
pub(super) struct SplitPayload {
    pub(super) startup_message: Option<String>,
    pub(super) parent_session_id: Option<String>,
    pub(super) prompt: Option<PendingSplitPrompt>,
    pub(super) model_override: Option<String>,
    pub(super) provider_key_override: Option<String>,
    pub(super) label: Option<String>,
}

#[derive(Default)]
pub(super) struct PendingSplit {
    /// Run loop: issue a `split()` once idle.
    pub(super) request: bool,
    /// Run loop: issue a `transfer()` once idle.
    pub(super) transfer_request: bool,
    /// Human-friendly flow label, e.g. "Prompt", "Review", "Transfer".
    pub(super) label: Option<String>,
    /// When the launch started, for the brief client-side running state.
    pub(super) started_at: Option<Instant>,
    pub(super) prompt: Option<PendingSplitPrompt>,
    pub(super) startup_message: Option<String>,
    pub(super) parent_session_id: Option<String>,
    pub(super) model_override: Option<String>,
    pub(super) provider_key_override: Option<String>,
}

impl PendingSplit {
    /// Drop the launch payload. Keeps `started_at` (the spinner reads it) and
    /// the request flags, which callers disarm explicitly.
    pub(super) fn clear_payload(&mut self) {
        self.prompt = None;
        self.startup_message = None;
        self.parent_session_id = None;
        self.model_override = None;
        self.provider_key_override = None;
        self.label = None;
    }

    /// Take the launch payload for the `SplitResponse` handler.
    pub(super) fn take_payload(&mut self) -> SplitPayload {
        SplitPayload {
            startup_message: self.startup_message.take(),
            parent_session_id: self.parent_session_id.take(),
            prompt: self.prompt.take(),
            model_override: self.model_override.take(),
            provider_key_override: self.provider_key_override.take(),
            label: self.label.take(),
        }
    }
}
