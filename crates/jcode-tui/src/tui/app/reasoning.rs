//! App-side live reasoning/thinking stream state.

use std::time::Instant;

use super::TurnReasoningTrace;

/// The model's live thinking output for the current turn: the accumulated
/// thinking buffer, the renderer's reasoning-tail bookkeeping, and the per-turn
/// reasoning traces. One home, so the streaming and commit paths read one struct
/// instead of nine loose fields.
#[derive(Default)]
pub(super) struct ReasoningState {
    /// When the current thinking burst started (for the status line).
    pub(super) thinking_start: Option<Instant>,
    /// Whether the current turn's thought line has been inserted.
    pub(super) thought_line_inserted: bool,
    /// Accumulator for thinking content during a thinking session.
    pub(super) thinking_buffer: String,
    /// Whether the legacy single-line thought prefix was emitted this session.
    pub(super) thinking_prefix_emitted: bool,
    /// Whether we are currently streaming reasoning (dim+italic) text.
    pub(super) streaming: bool,
    /// Incomplete trailing reasoning line awaiting a newline, rendered live as
    /// the streaming buffer's tail so reasoning trickles in token-by-token.
    pub(super) pending_line: String,
    /// Byte length of the live partial-reasoning markup appended to the
    /// streaming text, truncated and re-appended on each delta.
    pub(super) partial_len: usize,
    /// Byte offset in the streaming text where the current reasoning block
    /// began, used to slice a closed block back out in `current` mode.
    pub(super) block_start: Option<usize>,
    /// Reasoning traces anchored during the current turn (`current` mode).
    /// Each tracks the display index plus the transcript's wrapped-line total,
    /// so stale traces can be garbage-collected once scrolled off-screen.
    pub(super) turn_traces: Vec<TurnReasoningTrace>,
}
