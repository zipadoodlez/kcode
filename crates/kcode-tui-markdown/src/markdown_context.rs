use crate::MarkdownSpacingMode;
use std::cell::Cell;

thread_local! {
    /// Whether markdown rendering is running in streaming mode.
    static STREAMING_RENDER_CONTEXT: Cell<bool> = const { Cell::new(false) };
    /// Whether code blocks should be horizontally centered within available width.
    /// Set to true in centered mode, false in left-aligned mode.
    static CENTER_CODE_BLOCKS: Cell<bool> = const { Cell::new(true) };
    /// Optional test/debug override for markdown spacing mode.
    static MARKDOWN_SPACING_MODE_OVERRIDE: Cell<Option<MarkdownSpacingMode>> = const { Cell::new(None) };
}

struct ScopedReset<'a, T: Copy> {
    cell: &'a Cell<T>,
    prev: T,
}

impl<T: Copy> Drop for ScopedReset<'_, T> {
    fn drop(&mut self) {
        self.cell.set(self.prev);
    }
}

fn with_scoped_cell_value<T: Copy, R>(cell: &Cell<T>, value: T, f: impl FnOnce() -> R) -> R {
    let prev = cell.replace(value);
    let _guard = ScopedReset { cell, prev };
    f()
}

pub(super) fn effective_markdown_spacing_mode() -> MarkdownSpacingMode {
    MARKDOWN_SPACING_MODE_OVERRIDE.with(|mode| {
        mode.get()
            .unwrap_or(crate::config_snapshot().markdown_spacing)
    })
}

#[cfg(test)]
pub(crate) fn with_markdown_spacing_mode_override<T>(
    mode: Option<MarkdownSpacingMode>,
    f: impl FnOnce() -> T,
) -> T {
    MARKDOWN_SPACING_MODE_OVERRIDE.with(|ctx| with_scoped_cell_value(ctx, mode, f))
}

pub(super) fn with_streaming_render_context<T>(f: impl FnOnce() -> T) -> T {
    STREAMING_RENDER_CONTEXT.with(|ctx| with_scoped_cell_value(ctx, true, f))
}

pub fn set_center_code_blocks(centered: bool) {
    CENTER_CODE_BLOCKS.with(|ctx| ctx.set(centered));
}

pub fn center_code_blocks() -> bool {
    CENTER_CODE_BLOCKS.with(|ctx| ctx.get())
}

/// Override markdown block alignment for one render, restoring the caller's
/// setting afterwards (including during unwinding).
pub fn with_center_code_blocks<T>(centered: bool, f: impl FnOnce() -> T) -> T {
    CENTER_CODE_BLOCKS.with(|ctx| with_scoped_cell_value(ctx, centered, f))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn centered_scope_restores_nested_and_panicking_overrides() {
        let original = center_code_blocks();
        with_center_code_blocks(true, || {
            with_center_code_blocks(false, || assert!(!center_code_blocks()));
            assert!(center_code_blocks());
            let result = std::panic::catch_unwind(|| {
                with_center_code_blocks(false, || panic!("test unwind"));
            });
            assert!(result.is_err());
            assert!(center_code_blocks());
        });
        assert_eq!(center_code_blocks(), original);
    }
}
