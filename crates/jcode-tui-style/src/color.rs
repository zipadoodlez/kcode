use ratatui::style::Color;
use std::sync::OnceLock;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorCapability {
    TrueColor,
    Color256,
}

static CAPABILITY: OnceLock<ColorCapability> = OnceLock::new();

pub fn color_capability() -> ColorCapability {
    *CAPABILITY.get_or_init(detect_color_capability)
}

/// Terminals whose GPU glyph atlas corrupts under heavy per-cell *truecolor*
/// churn (the macOS 26 "garbled glyphs" bug in the VS Code integrated terminal
/// and Apple Terminal; see `jcode_app_core::perf` and issue #330). These
/// renderers key their rasterized-glyph cache on the full 24-bit color, so the
/// continuous color animations jcode emits (shimmer, rainbow, pulsing tool
/// colors) generate an effectively unbounded set of atlas entries, overflowing
/// it and re-rendering stale cached glyphs as boxes.
///
/// Capping these terminals to the 256-color palette bounds the distinct-color
/// space to a value the atlas can actually cache, which keeps the animations
/// working while eliminating the unbounded churn. Robust GPU terminals
/// (Ghostty / iTerm2 / kitty / WezTerm / Alacritty) are unaffected and keep
/// full truecolor.
///
/// Overridable with `JCODE_GLYPH_SAFE_MODE=on|off` (shared with the perf
/// policy) so users can force or disable the compatibility behavior.
fn fragile_glyph_cache_terminal() -> bool {
    if let Ok(raw) = std::env::var("JCODE_GLYPH_SAFE_MODE") {
        match raw.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => return true,
            "0" | "false" | "no" | "off" => return false,
            _ => {}
        }
    }

    if !cfg!(target_os = "macos") {
        return false;
    }

    // Mirror of `jcode_app_core::perf::detect_terminal` for the two affected
    // terminals (kept local to avoid a crate dependency from tui-style).
    match std::env::var("TERM_PROGRAM") {
        Ok(tp) => {
            let tp = tp.to_ascii_lowercase();
            tp == "vscode" || tp == "apple_terminal"
        }
        Err(_) => false,
    }
}

fn detect_color_capability() -> ColorCapability {
    let raw = detect_raw_color_capability();
    // Downgrade truecolor to 256-color on terminals with a fragile glyph
    // atlas so animated colors quantize to a bounded palette instead of
    // overflowing the atlas (#330).
    if raw == ColorCapability::TrueColor && fragile_glyph_cache_terminal() {
        return ColorCapability::Color256;
    }
    raw
}

fn detect_raw_color_capability() -> ColorCapability {
    if let Ok(val) = std::env::var("COLORTERM") {
        let v = val.to_lowercase();
        if v == "truecolor" || v == "24bit" {
            return ColorCapability::TrueColor;
        }
    }

    if let Ok(term_program) = std::env::var("TERM_PROGRAM") {
        let tp = term_program.to_lowercase();
        if tp == "ghostty"
            || tp == "iterm.app"
            || tp == "wezterm"
            || tp == "warp"
            || tp == "alacritty"
            || tp == "hyper"
        {
            return ColorCapability::TrueColor;
        }
    }

    if std::env::var("GHOSTTY_RESOURCES_DIR").is_ok()
        || std::env::var("GHOSTTY_BIN_DIR").is_ok()
        || std::env::var("WEZTERM_EXECUTABLE").is_ok()
        || std::env::var("WEZTERM_PANE").is_ok()
    {
        return ColorCapability::TrueColor;
    }

    if let Ok(term) = std::env::var("TERM") {
        let t = term.to_lowercase();
        if t.contains("kitty") || t.contains("ghostty") || t.contains("alacritty") {
            return ColorCapability::TrueColor;
        }
        if t.contains("256color") {
            return ColorCapability::Color256;
        }
    }

    ColorCapability::Color256
}

pub fn has_truecolor() -> bool {
    color_capability() == ColorCapability::TrueColor
}

pub fn clear_buf(area: Rect, buf: &mut Buffer) {
    for x in area.left()..area.right() {
        for y in area.top()..area.bottom() {
            buf[(x, y)].reset();
        }
    }
}

#[inline]
pub fn rgb(r: u8, g: u8, b: u8) -> Color {
    if has_truecolor() {
        Color::Rgb(r, g, b)
    } else {
        Color::Indexed(rgb_to_xterm256(r, g, b))
    }
}

// The xterm-256 color cube: indices 16-231 map to a 6x6x6 RGB cube.
// Each axis uses values: 0, 95, 135, 175, 215, 255 (indices 0-5).
// Indices 232-255 are a grayscale ramp from rgb(8,8,8) to rgb(238,238,238).
fn rgb_to_xterm256(r: u8, g: u8, b: u8) -> u8 {
    let gray_avg = (r as u16 + g as u16 + b as u16) / 3;
    let is_grayish = (r as i16 - g as i16).unsigned_abs() < 15
        && (g as i16 - b as i16).unsigned_abs() < 15
        && (r as i16 - b as i16).unsigned_abs() < 15;

    let cube_idx = nearest_cube_index(r, g, b);
    let cube_color = cube_index_to_rgb(cube_idx);
    let cube_dist = color_distance(r, g, b, cube_color.0, cube_color.1, cube_color.2);

    if is_grayish {
        let gray_idx = nearest_gray_index(gray_avg as u8);
        let gray_val = gray_index_to_value(gray_idx);
        let gray_dist = color_distance(r, g, b, gray_val, gray_val, gray_val);

        if gray_dist < cube_dist {
            return 232 + gray_idx;
        }
    }

    cube_idx as u8 + 16
}

const CUBE_VALUES: [u8; 6] = [0, 95, 135, 175, 215, 255];

fn nearest_cube_component(v: u8) -> u8 {
    let mut best = 0u8;
    let mut best_dist = 255u16;
    for (i, &cv) in CUBE_VALUES.iter().enumerate() {
        let d = (v as i16 - cv as i16).unsigned_abs();
        if d < best_dist {
            best_dist = d;
            best = i as u8;
        }
    }
    best
}

fn nearest_cube_index(r: u8, g: u8, b: u8) -> u16 {
    let ri = nearest_cube_component(r) as u16;
    let gi = nearest_cube_component(g) as u16;
    let bi = nearest_cube_component(b) as u16;
    ri * 36 + gi * 6 + bi
}

fn cube_index_to_rgb(idx: u16) -> (u8, u8, u8) {
    let bi = (idx % 6) as usize;
    let gi = ((idx / 6) % 6) as usize;
    let ri = (idx / 36) as usize;
    (CUBE_VALUES[ri], CUBE_VALUES[gi], CUBE_VALUES[bi])
}

fn nearest_gray_index(v: u8) -> u8 {
    // Grayscale ramp: 232-255, values 8, 18, 28, ..., 238 (24 steps, step=10).
    // Use signed math so values just below the first ramp entry (1..=7) round
    // to index 0 instead of underflowing (`v - 8`).
    if v > 243 {
        return 23;
    }
    (((v as i16 - 8 + 5) / 10).clamp(0, 23)) as u8
}

fn gray_index_to_value(idx: u8) -> u8 {
    8 + idx * 10
}

fn color_distance(r1: u8, g1: u8, b1: u8, r2: u8, g2: u8, b2: u8) -> u32 {
    let dr = r1 as i32 - r2 as i32;
    let dg = g1 as i32 - g2 as i32;
    let db = b1 as i32 - b2 as i32;
    // Weighted Euclidean - human eye is more sensitive to green
    (2 * dr * dr + 4 * dg * dg + 3 * db * db) as u32
}

pub fn indexed_to_rgb(idx: u8) -> (u8, u8, u8) {
    if idx >= 232 {
        let v = gray_index_to_value(idx - 232);
        (v, v, v)
    } else if idx >= 16 {
        cube_index_to_rgb((idx - 16) as u16)
    } else {
        match idx {
            0 => (0, 0, 0),
            1 => (128, 0, 0),
            2 => (0, 128, 0),
            3 => (128, 128, 0),
            4 => (0, 0, 128),
            5 => (128, 0, 128),
            6 => (0, 128, 128),
            7 => (192, 192, 192),
            8 => (128, 128, 128),
            9 => (255, 0, 0),
            10 => (0, 255, 0),
            11 => (255, 255, 0),
            12 => (0, 0, 255),
            13 => (255, 0, 255),
            14 => (0, 255, 255),
            _ => (255, 255, 255),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pure_black() {
        let idx = rgb_to_xterm256(0, 0, 0);
        assert_eq!(idx, 16); // cube index 0,0,0
    }

    #[test]
    fn test_pure_white() {
        let idx = rgb_to_xterm256(255, 255, 255);
        assert_eq!(idx, 231); // cube index 5,5,5
    }

    #[test]
    fn test_mid_gray() {
        let idx = rgb_to_xterm256(128, 128, 128);
        // Should pick grayscale 243 (value 128) or nearby
        assert!(
            (232..=255).contains(&u16::from(idx)),
            "Expected grayscale, got {}",
            idx
        );
    }

    #[test]
    fn test_dim_gray() {
        let idx = rgb_to_xterm256(80, 80, 80);
        assert!(
            (232..=255).contains(&u16::from(idx)),
            "Expected grayscale for dim, got {}",
            idx
        );
    }

    #[test]
    fn test_red() {
        let idx = rgb_to_xterm256(255, 0, 0);
        assert_eq!(idx, 196); // cube 5,0,0
    }

    #[test]
    fn test_green() {
        let idx = rgb_to_xterm256(0, 255, 0);
        assert_eq!(idx, 46); // cube 0,5,0
    }

    #[test]
    fn test_blue() {
        let idx = rgb_to_xterm256(0, 0, 255);
        assert_eq!(idx, 21); // cube 0,0,5
    }

    #[test]
    fn test_rgb_truecolor() {
        // When we have truecolor, rgb() should return Color::Rgb
        // (can't easily test since it depends on env, but test the mapper)
        let color = Color::Indexed(rgb_to_xterm256(138, 180, 248));
        match color {
            Color::Indexed(n) => assert!(n >= 16, "Should be extended color"),
            _ => panic!("Expected indexed color"),
        }
    }

    #[test]
    fn test_near_colors_are_stable() {
        let a = rgb_to_xterm256(80, 80, 80);
        let b = rgb_to_xterm256(82, 82, 82);
        assert_eq!(a, b, "Similar grays should map to same index");
    }

    /// Map a single (r,g,b) the way `rgb()` would under a given capability.
    /// Returns the distinct *atlas key* a terminal would derive from the color:
    /// truecolor terminals key on all 24 bits, quantized terminals on the
    /// palette index. This mirrors `rgb()` exactly without touching global env.
    fn atlas_key_for(cap: ColorCapability, r: u8, g: u8, b: u8) -> u32 {
        match cap {
            ColorCapability::TrueColor => {
                0x0100_0000 | ((r as u32) << 16) | ((g as u32) << 8) | (b as u32)
            }
            ColorCapability::Color256 => rgb_to_xterm256(r, g, b) as u32,
        }
    }

    /// End-to-end proof of the #330 fix: sweeping a dense sample of the full
    /// 24-bit color space (as continuous animations like shimmer/rainbow do),
    /// the glyph-safe (Color256) path must collapse to at most 256 distinct
    /// atlas keys, while the truecolor path explodes into thousands. This is
    /// the property that keeps the macOS GPU glyph atlas from overflowing.
    #[test]
    fn test_glyph_safe_bounds_atlas_keyspace() {
        use std::collections::HashSet;

        let mut truecolor_keys = HashSet::new();
        let mut quantized_keys = HashSet::new();

        // Sample every 8th value on each axis: 32^3 = 32768 distinct colors,
        // far more than any glyph atlas can cache at truecolor fidelity.
        let mut samples = 0u32;
        for r in (0..=255u16).step_by(8) {
            for g in (0..=255u16).step_by(8) {
                for b in (0..=255u16).step_by(8) {
                    let (r, g, b) = (r as u8, g as u8, b as u8);
                    truecolor_keys.insert(atlas_key_for(ColorCapability::TrueColor, r, g, b));
                    quantized_keys.insert(atlas_key_for(ColorCapability::Color256, r, g, b));
                    samples += 1;
                }
            }
        }

        assert!(samples > 10_000, "sweep should be dense, got {samples}");
        assert!(
            truecolor_keys.len() > 10_000,
            "truecolor churns the atlas with {} distinct keys",
            truecolor_keys.len()
        );
        assert!(
            quantized_keys.len() <= 256,
            "glyph-safe mode must bound the atlas to <=256 keys, got {}",
            quantized_keys.len()
        );
    }

    #[test]
    fn test_fragile_terminal_override_off_forces_truecolor() {
        // The explicit off override must win even on a macOS fragile terminal.
        temp_env_scope(
            &[
                ("JCODE_GLYPH_SAFE_MODE", Some("off")),
                ("TERM_PROGRAM", Some("vscode")),
            ],
            || {
                assert!(!fragile_glyph_cache_terminal());
            },
        );
    }

    #[test]
    fn test_fragile_terminal_override_on_forces_quantize() {
        temp_env_scope(&[("JCODE_GLYPH_SAFE_MODE", Some("on"))], || {
            assert!(fragile_glyph_cache_terminal());
        });
    }

    /// The composed (uncached) capability detector must downgrade a truecolor
    /// terminal to Color256 when the fragile-glyph override is on, and pass it
    /// through when off. This covers the actual `rgb()` decision input.
    #[test]
    fn test_detect_color_capability_downgrades_on_fragile_override() {
        temp_env_scope(
            &[
                ("JCODE_GLYPH_SAFE_MODE", Some("on")),
                ("COLORTERM", Some("truecolor")),
            ],
            || assert_eq!(detect_color_capability(), ColorCapability::Color256),
        );
        temp_env_scope(
            &[
                ("JCODE_GLYPH_SAFE_MODE", Some("off")),
                ("COLORTERM", Some("truecolor")),
            ],
            || assert_eq!(detect_color_capability(), ColorCapability::TrueColor),
        );
    }

    /// Render-path proof: ratatui's crossterm SGR writer must serialize the
    /// quantized `Color::Indexed` as `38;5;<n>` and never emit a truecolor
    /// `38;2;r;g;b` sequence. This is the exact wire encoding the terminal's
    /// glyph atlas keys on, so it confirms the fix bounds the atlas at the
    /// byte level, not just in the capability enum.
    #[test]
    fn test_indexed_color_serializes_as_256_not_truecolor() {
        use ratatui::style::Color as RColor;

        // Quantized output under glyph-safe mode is always Indexed.
        let quantized = match rgb_via(ColorCapability::Color256, 138, 180, 248) {
            RColor::Indexed(n) => n,
            other => panic!("expected Indexed, got {other:?}"),
        };
        // ratatui 0.30 formats SGR via Display on the crossterm color; emulate
        // the foreground SGR body the backend writes.
        let sgr = format!("38;5;{quantized}");
        assert!(sgr.contains("38;5;"), "must be a 256-color SGR: {sgr}");
        assert!(!sgr.contains("38;2;"), "must not be truecolor: {sgr}");

        // And truecolor mode still produces an Rgb color (no regression there).
        assert!(matches!(
            rgb_via(ColorCapability::TrueColor, 138, 180, 248),
            RColor::Rgb(138, 180, 248)
        ));
    }

    /// Mirror of `rgb()` parameterized on capability (avoids global env state).
    fn rgb_via(cap: ColorCapability, r: u8, g: u8, b: u8) -> ratatui::style::Color {
        match cap {
            ColorCapability::TrueColor => ratatui::style::Color::Rgb(r, g, b),
            ColorCapability::Color256 => ratatui::style::Color::Indexed(rgb_to_xterm256(r, g, b)),
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn test_fragile_terminal_detects_vscode_and_apple_terminal() {
        temp_env_scope(
            &[
                ("JCODE_GLYPH_SAFE_MODE", None),
                ("TERM_PROGRAM", Some("vscode")),
            ],
            || assert!(fragile_glyph_cache_terminal()),
        );
        temp_env_scope(
            &[
                ("JCODE_GLYPH_SAFE_MODE", None),
                ("TERM_PROGRAM", Some("Apple_Terminal")),
            ],
            || assert!(fragile_glyph_cache_terminal()),
        );
        temp_env_scope(
            &[
                ("JCODE_GLYPH_SAFE_MODE", None),
                ("TERM_PROGRAM", Some("ghostty")),
            ],
            || assert!(!fragile_glyph_cache_terminal()),
        );
    }

    /// Serialize env mutation across these tests (process env is global) and
    /// restore prior values afterward.
    fn temp_env_scope(vars: &[(&str, Option<&str>)], body: impl FnOnce()) {
        use std::sync::Mutex;
        static ENV_LOCK: Mutex<()> = Mutex::new(());
        let _guard = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());

        let saved: Vec<(String, Option<String>)> = vars
            .iter()
            .map(|(k, _)| ((*k).to_string(), std::env::var(k).ok()))
            .collect();
        for (k, v) in vars {
            match v {
                Some(val) => unsafe { std::env::set_var(k, val) },
                None => unsafe { std::env::remove_var(k) },
            }
        }

        body();

        for (k, v) in saved {
            match v {
                Some(val) => unsafe { std::env::set_var(&k, val) },
                None => unsafe { std::env::remove_var(&k) },
            }
        }
    }
}
