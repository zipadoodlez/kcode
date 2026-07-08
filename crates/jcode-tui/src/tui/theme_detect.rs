//! Terminal light/dark theme detection.
//!
//! Resolves the theme mode once per process, before the TUI enters raw mode:
//!
//! 1. `JCODE_THEME=dark|light` env override (also accepts `auto`).
//! 2. `display.theme` config: "dark", "light", or "auto"/empty.
//! 3. Auto: query the terminal's background color (OSC 11 via
//!    `terminal-colorsaurus`) and classify by perceived lightness. The crate
//!    uses a fast feature-detection heuristic, so unsupported terminals fail
//!    quickly instead of hanging on the timeout.
//! 4. Fallback: dark (jcode's native palette).
//!
//! The result is stored in `jcode_tui_style::theme_mode` where the renderer
//! adapts colors for light backgrounds at frame time.

use jcode_tui_style::ThemeMode;
use std::sync::OnceLock;

static DETECTED: OnceLock<ThemeMode> = OnceLock::new();

/// Resolve and install the global theme mode. Idempotent; the first call does
/// the (potentially blocking, sub-second) terminal query and later calls are
/// free. Must be called before entering raw mode / the alternate screen.
pub fn init_theme_mode() -> ThemeMode {
    let mode = *DETECTED.get_or_init(resolve_theme_mode);
    jcode_tui_style::set_theme_mode(mode);
    mode
}

fn resolve_theme_mode() -> ThemeMode {
    let configured = std::env::var("JCODE_THEME")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| crate::config::config().display.theme.clone());

    match configured.trim().to_ascii_lowercase().as_str() {
        "dark" => return ThemeMode::Dark,
        "light" => return ThemeMode::Light,
        "" | "auto" => {}
        other => {
            crate::logging::info(&format!(
                "Unknown theme '{other}' (expected auto/dark/light); using auto detection"
            ));
        }
    }

    detect_terminal_theme().unwrap_or(ThemeMode::Dark)
}

/// Query the terminal background color and classify it as dark or light.
/// Returns None when the terminal does not support querying or the query
/// fails, in which case the caller falls back to dark.
fn detect_terminal_theme() -> Option<ThemeMode> {
    use std::io::IsTerminal;
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        return None;
    }
    let mut options = terminal_colorsaurus::QueryOptions::default();
    // Keep startup snappy; supporting terminals answer in a few ms, and
    // colorsaurus detects non-supporting terminals before the timeout anyway.
    options.timeout = std::time::Duration::from_millis(400);
    match terminal_colorsaurus::theme_mode(options) {
        Ok(terminal_colorsaurus::ThemeMode::Light) => {
            crate::logging::info("Detected light terminal background; adapting theme");
            Some(ThemeMode::Light)
        }
        Ok(terminal_colorsaurus::ThemeMode::Dark) => Some(ThemeMode::Dark),
        Err(e) => {
            crate::logging::info(&format!(
                "Terminal background detection unavailable ({e}); defaulting to dark theme"
            ));
            None
        }
    }
}
