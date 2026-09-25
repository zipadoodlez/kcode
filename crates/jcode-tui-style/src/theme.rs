use ratatui::prelude::*;

pub fn user_color() -> Color {
    crate::palette::role_color(crate::palette::Role::User)
}
pub fn ai_color() -> Color {
    crate::palette::role_color(crate::palette::Role::Ai)
}
pub fn tool_color() -> Color {
    crate::palette::role_color(crate::palette::Role::Tool)
}
pub fn file_link_color() -> Color {
    crate::palette::role_color(crate::palette::Role::FileLink)
}
pub fn dim_color() -> Color {
    crate::palette::role_color(crate::palette::Role::Dim)
}
pub fn accent_color() -> Color {
    crate::palette::role_color(crate::palette::Role::Accent)
}
pub fn system_message_color() -> Color {
    crate::palette::role_color(crate::palette::Role::System)
}
pub fn queued_color() -> Color {
    crate::palette::role_color(crate::palette::Role::Queued)
}
pub fn asap_color() -> Color {
    crate::palette::role_color(crate::palette::Role::Asap)
}
pub fn pending_color() -> Color {
    crate::palette::role_color(crate::palette::Role::Pending)
}
pub fn user_text() -> Color {
    crate::palette::role_color(crate::palette::Role::UserText)
}
pub fn user_bg() -> Color {
    crate::palette::role_color(crate::palette::Role::UserBg)
}
pub fn ai_text() -> Color {
    crate::palette::role_color(crate::palette::Role::AiText)
}
pub fn header_icon_color() -> Color {
    crate::palette::role_color(crate::palette::Role::HeaderIcon)
}
pub fn header_name_color() -> Color {
    crate::palette::role_color(crate::palette::Role::HeaderName)
}
pub fn header_session_color() -> Color {
    crate::palette::role_color(crate::palette::Role::HeaderSession)
}

/// Success / additions accent.
pub fn success_color() -> Color {
    crate::palette::role_color(crate::palette::Role::Success)
}
/// Warning accent.
pub fn warning_color() -> Color {
    crate::palette::role_color(crate::palette::Role::Warning)
}
/// Error / deletions accent.
pub fn error_color() -> Color {
    crate::palette::role_color(crate::palette::Role::Error)
}
/// Informational accent.
pub fn info_color() -> Color {
    crate::palette::role_color(crate::palette::Role::Info)
}
/// Borders and rules.
pub fn border_color() -> Color {
    crate::palette::role_color(crate::palette::Role::Border)
}
/// Selected-row background.
pub fn selection_bg_color() -> Color {
    crate::palette::role_color(crate::palette::Role::SelectionBg)
}

// Spinner frames for animated status. Keep these single-cell because the fast
// spinner-only renderer patches one status cell between full TUI redraws. This
// sequence should read as a circular spin, not a grow/recede pulse.
const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// Frame rate for slow, full-line "liveness" indicators that can only be
/// repainted by a full TUI redraw (e.g. the running-tool progress bar) when
/// decorative animations are disabled (Minimal tier, SSH, WSL, etc.). These
/// ride the ~1 Hz passive-liveness redraw, so advancing them faster would just
/// skip frames. Keep this slow so they read as alive without forcing more
/// expensive full-frame redraws.
pub const LIVENESS_INDICATOR_FPS: f32 = 1.5;

/// Frame rate for the low-cost single-cell circular spinner when decorative
/// animations are disabled. Unlike the full-line indicators above, this spinner
/// is patched by the cheap one-cell fast path between full redraws, so it can
pub fn spinner_frame_index(elapsed: f32, fps: f32) -> usize {
    ((elapsed * fps) as usize) % SPINNER_FRAMES.len()
}

pub fn spinner_frame(elapsed: f32, fps: f32) -> &'static str {
    SPINNER_FRAMES[spinner_frame_index(elapsed, fps)]
}

/// Whether `symbol` is one of the cells owned by the primary activity spinner.
///
/// The TUI's single-cell spinner redraw uses this to avoid patching a status-row
/// cell after a late overlay, such as the slash-command palette, has taken
/// ownership of it.
pub fn is_activity_indicator_frame(symbol: &str) -> bool {
    SPINNER_FRAMES.contains(&symbol)
}

pub fn activity_indicator_frame_index(elapsed: f32, fps: f32) -> usize {
    spinner_frame_index(elapsed, fps)
}

pub fn activity_indicator(elapsed: f32, fps: f32) -> &'static str {
    SPINNER_FRAMES[activity_indicator_frame_index(elapsed, fps)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spinner_frames_are_circular_braille_sequence() {
        assert_eq!(
            SPINNER_FRAMES,
            &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]
        );
        assert!(is_activity_indicator_frame("⠋"));
        assert!(is_activity_indicator_frame("⠏"));
        assert!(!is_activity_indicator_frame("/"));
    }

    #[test]
    fn spinner_frame_wraps_at_sequence_length() {
        let fps = 10.0;
        assert_eq!(spinner_frame(0.0, fps), "⠋");
        assert_eq!(spinner_frame(0.9, fps), "⠏");
        assert_eq!(spinner_frame(1.0, fps), "⠋");
    }

}
