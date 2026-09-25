//! Install the active palette.
//!
//! The theme is configuration, not detection. Resolution order:
//!
//! 1. `JCODE_THEME=dark|light` env override.
//! 2. `display.theme` config: "dark" or "light".
//! 3. Fallback: dark.
//!
//! "light" installs the baked light palette ([`Palette::light`]); "dark" the
//! built-in one. `[display.palette]` (16 base16 slots) then layers on top, and
//! `[display.colors]` (per-role) layers on top of that, so a base16 theme can be
//! pasted in and individual roles still tweaked. There is no terminal
//! background query: a light terminal is a palette, not a per-frame transform.

use jcode_tui_style::Palette;

/// Install the configured palette. Safe to call repeatedly; the TUI calls it
/// again after `/colors` edits so changes apply without a restart.
pub fn init_palette() {
    let base = match configured_theme().as_str() {
        "light" => Palette::light(),
        _ => Palette::default(),
    };
    let slots = &crate::config::config().display.palette;
    let (palette, slot_errors) = Palette::from_slot_pairs_over(
        base,
        slots.iter().map(|(key, value)| (key.as_str(), value.as_str())),
    );
    for error in slot_errors {
        crate::logging::warn(&format!("display.palette: {error}"));
    }
    let configured = &crate::config::config().display.colors;
    let (palette, errors) = Palette::from_pairs_over(
        palette,
        configured
            .iter()
            .map(|(key, value)| (key.as_str(), value.as_str())),
    );
    for error in errors {
        crate::logging::warn(&format!("display.colors: {error}"));
    }
    jcode_tui_style::set_palette(palette);
}

/// The configured theme name, normalized. Unknown values fall back to dark.
pub fn configured_theme() -> String {
    std::env::var("JCODE_THEME")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| crate::config::config().display.theme.clone())
        .trim()
        .to_ascii_lowercase()
}
