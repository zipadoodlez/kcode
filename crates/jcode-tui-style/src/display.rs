//! Apply the active palette to a rendered frame.
//!
//! Colors reach the terminal through role accessors (`theme::user_color()`),
//! which return a role's built-in *default*. Once per frame this pass rewrites
//! any cell whose color is exactly a role default onto that role's configured
//! color, so a configured palette repaints the whole UI without per-widget
//! changes. An unconfigured palette is a no-op, and `role_color()` deliberately
//! returns the default so a cell can never be remapped twice.
//!
//! There is no light/dark runtime adaptation here: a light terminal is a
//! palette (see `Palette::light`), selected up front, not a per-frame transform.

use ratatui::buffer::Buffer;
use ratatui::style::Color;
use std::collections::HashMap;

/// Final display pass: attribute configured palette overrides over the frame.
pub fn adapt_buffer_for_display(buf: &mut Buffer) {
    let Some(palette) = crate::palette::configured_palette() else {
        return;
    };
    // Frames contain few distinct colors; memoize the substitution per unique
    // color so the per-cell cost is a hash lookup.
    let mut cache: HashMap<Color, Color> = HashMap::new();
    let mut adapt = |c: Color| -> Color {
        if c == Color::Reset {
            return c;
        }
        *cache
            .entry(c)
            .or_insert_with(|| crate::palette::configured_native_color(&palette, c).unwrap_or(c))
    };
    for cell in buf.content.iter_mut() {
        cell.fg = adapt(cell.fg);
        cell.bg = adapt(cell.bg);
        cell.underline_color = adapt(cell.underline_color);
    }
}

/// The same substitution for a foreground patched outside a full-frame redraw.
pub fn adapt_foreground_for_display(color: Color) -> Color {
    match crate::palette::configured_palette() {
        Some(palette) => crate::palette::configured_native_color(&palette, color).unwrap_or(color),
        None => color,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palette::{Palette, Role, role_color, set_palette};
    use ratatui::layout::Rect;

    // The active palette is process-global; the crate-level lock serializes
    // every test that touches it.
    use crate::STYLE_TEST_LOCK as TEST_LOCK;

    fn with_palette(palette: Palette, body: impl FnOnce()) {
        struct Restore;
        impl Drop for Restore {
            fn drop(&mut self) {
                set_palette(Palette::default());
            }
        }
        let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _restore = Restore;
        set_palette(palette);
        body();
    }

    fn buffer_with(colors: &[Color]) -> Buffer {
        let mut buf = Buffer::empty(Rect::new(0, 0, colors.len() as u16, 1));
        for (cell, color) in buf.content.iter_mut().zip(colors) {
            cell.fg = *color;
        }
        buf
    }

    // The default palette must render byte-identically to the historical
    // hard-coded look. This is the regression that would silently recolor
    // every existing user's terminal.
    #[test]
    fn default_palette_leaves_the_frame_untouched() {
        with_palette(Palette::default(), || {
            let original = buffer_with(&[
                Color::Rgb(255, 200, 100),
                Color::White,
                Color::Indexed(42),
                Color::Reset,
            ]);
            let mut adapted = original.clone();
            adapt_buffer_for_display(&mut adapted);
            assert_eq!(adapted, original);
        });
    }

    #[test]
    fn configured_role_recolors_role_cells_and_named_colors_only() {
        let mut palette = Palette::default();
        palette.set(Role::Error, (10, 80, 240));
        with_palette(palette, || {
            let mut buf = buffer_with(&[
                role_color(Role::Error), // the role's own output
                Color::Red,              // the named stand-in for error
                Color::Rgb(40, 200, 90), // unrelated green, no role
                Color::Reset,
            ]);
            adapt_buffer_for_display(&mut buf);

            assert_eq!(buf.content[0].fg, crate::color::rgb(10, 80, 240));
            assert_eq!(buf.content[1].fg, crate::color::rgb(10, 80, 240));
            assert_eq!(
                buf.content[2].fg,
                crate::color::rgb(40, 200, 90),
                "an untagged literal must not follow any role"
            );
            assert_eq!(buf.content[3].fg, Color::Reset, "Reset must be preserved");
        });
    }

    // Applying the pass twice must be a no-op beyond the first, otherwise a
    // double-render path would compound shifts.
    #[test]
    fn palette_substitution_is_idempotent() {
        let mut palette = Palette::default();
        palette.set(Role::Warning, (90, 220, 130));
        with_palette(palette, || {
            let mut once = buffer_with(&[role_color(Role::Warning)]);
            adapt_buffer_for_display(&mut once);
            let mut twice = once.clone();
            adapt_buffer_for_display(&mut twice);
            assert_eq!(
                once, twice,
                "a second palette pass must not shift colors again"
            );
        });
    }

    /// The baked light palette keeps the terminal readable: every foreground is
    /// distinguishable from the surface it sits on. It is static data, so this
    /// pins the values the removed transform produced.
    #[test]
    fn light_palette_replaces_every_role() {
        let light = Palette::light();
        for role in crate::palette::ALL_ROLES.iter().copied() {
            assert!(light.is_overridden(role));
            assert_ne!(light.rgb(role), role.default_rgb());
        }
    }
}
