use super::*;

pub(super) use kcode_tui_messages::{centered_wrap_width, left_pad_lines_for_centered_mode};

pub(crate) fn get_cached_message_lines<F>(
    msg: &DisplayMessage,
    width: u16,
    diff_mode: crate::config::DiffDisplayMode,
    render: F,
) -> Vec<Line<'static>>
where
    F: FnOnce(&DisplayMessage, u16, crate::config::DiffDisplayMode) -> Vec<Line<'static>>,
{
    kcode_tui_messages::get_cached_message_lines(
        msg,
        width,
        diff_mode,
        kcode_tui_messages::MessageCacheContext {
            centered: markdown::center_code_blocks(),
            show_kgrep_output: crate::config::config().display.show_kgrep_output,
            show_bash_output: crate::config::config().display.show_bash_output,
            tool_call_details: crate::config::config().display.tool_call_details,
        },
        render,
    )
}

#[cfg(test)]
mod tests {}
