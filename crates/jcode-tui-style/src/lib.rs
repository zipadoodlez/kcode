pub mod color;
pub mod display;
pub mod palette;
pub mod theme;

pub use color::{ColorCapability, clear_buf, color_capability, has_truecolor, indexed_to_rgb, rgb};
pub use display::{adapt_buffer_for_display, adapt_foreground_for_display};
pub use palette::{
    ALL_ROLES, ALL_SLOTS, Palette, Role, Slot, default_slot_for, palette, role_color, set_palette,
};

/// The active palette is process-global. One lock serializes every test that
/// reads or mutates it, wherever the test lives.
#[cfg(test)]
pub(crate) static STYLE_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Restore the terminal, logging any failure instead of printing it.
///
/// `ratatui::restore()` reports failures with `eprintln!`. On a dead terminal
/// (closed window, dropped SSH) the restore fails with EIO and stderr is
/// equally dead, so that `eprintln!` itself panics inside
/// `std::io::stdio::print_to`. The panic hook can then record a live session as
/// crashed and clobber its snapshot.
///
/// Always prefer this over `ratatui::restore()` on cleanup and orphan-exit
/// paths. See issue #599 (and #129, the same class via another path).
pub fn restore_terminal_quietly() {
    if let Err(error) = ratatui::try_restore() {
        jcode_logging::warn(&format!("failed to restore terminal: {error}"));
    }
}
