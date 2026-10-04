//! The glyphs and cadence a live session status draws with.
//!
//! Shared by the pinned work list (`kcode-tui`) and the session picker.

/// Cadence for the active-turn spinner frames.
///
/// Keep this aligned with the TUI redraw interval. 80 ms matches the primary
/// status spinner and avoids the visibly stepped motion of the old 125 ms
/// cadence without redrawing faster than the glyph can change.
pub const SPINNER_FRAME_MS: u64 = 80;

pub const SPINNER_FPS: f32 = 1000.0 / SPINNER_FRAME_MS as f32;

/// Frames for the spinner shown while a session's turn is live.
pub const SPINNER_FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// The spinner cell for `frame`.
pub fn spinner_glyph(frame: usize) -> &'static str {
    SPINNER_FRAMES[frame % SPINNER_FRAMES.len()]
}

/// Whether a member status means the session is mid-turn.
pub fn is_active_status(status: &str) -> bool {
    matches!(status, "running" | "streaming" | "thinking")
}

/// Compact age formatting for a member viewport (now/Ns/Nm/Nh).
pub fn humanize_age(age: u64) -> String {
    if age < 2 {
        "now".to_string()
    } else if age < 60 {
        format!("{age}s")
    } else if age < 3600 {
        format!("{}m", age / 60)
    } else {
        format!("{}h", age / 3600)
    }
}
