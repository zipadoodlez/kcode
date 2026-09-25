pub(super) use jcode_tui_style::theme::{
    accent_color, ai_color, ai_text, asap_color, dim_color, file_link_color, header_icon_color,
    header_name_color, header_session_color, pending_color, queued_color, system_message_color,
    tool_color, user_bg, user_color, user_text,
};

pub(super) fn activity_indicator_frame_index(elapsed: f32, fps: f32) -> usize {
    jcode_tui_style::theme::activity_indicator_frame_index(elapsed, fps)
}

pub(super) fn activity_indicator(elapsed: f32, fps: f32) -> &'static str {
    jcode_tui_style::theme::activity_indicator(elapsed, fps)
}
