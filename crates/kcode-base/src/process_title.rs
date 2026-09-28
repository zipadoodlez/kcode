const LINUX_PROCESS_TITLE_LIMIT: usize = 15;
#[cfg(target_os = "linux")]
const KILLALL_PROCESS_NAME: &str = "kcode";

pub fn compact_process_title(prefix: &str, name: Option<&str>) -> String {
    let mut title = prefix.to_string();
    if let Some(name) = name.filter(|name| !name.is_empty()) {
        let remaining = LINUX_PROCESS_TITLE_LIMIT.saturating_sub(title.len());
        if remaining > 0 {
            title.push_str(&name.chars().take(remaining).collect::<String>());
        }
    }
    title
}

pub fn session_name(session_id: &str) -> String {
    crate::id::extract_session_name(session_id)
        .map(|name| name.to_string())
        .unwrap_or_else(|| session_id.to_string())
}

fn capitalize_ascii_label(label: &str) -> String {
    let mut chars = label.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// The terminal window label for a session: `kcode <Name>`, or
/// `kcode/<server> <Name>` in a remote session. The session's name is its
/// memorable word; there is no title layer.
pub fn session_window_label(session_id: &str, server_name: Option<&str>) -> String {
    let name = capitalize_ascii_label(&session_name(session_id));
    match server_name.filter(|server| !server.is_empty() && !server.eq_ignore_ascii_case("kcode")) {
        Some(server) => format!("kcode/{} {name}", server.to_lowercase()),
        None => format!("kcode {name}"),
    }
}

/// Build the deliberately minimal terminal window title. The emoji already
/// identifies the session/connection, so do not repeat `kcode` or the memorable
/// animal name in window chrome.
pub fn terminal_window_title(icon: &str, label: &str, is_selfdev: bool) -> String {
    let suffix = if is_selfdev { " [self-dev]" } else { "" };
    let title = format!("{icon} {label}{suffix}");
    crate::output_style::terminal_text(&title).into_owned()
}

pub fn set_title(title: impl AsRef<str>) {
    proctitle::set_title(title.as_ref());
    set_killall_process_name();
}

fn set_killall_process_name() {
    #[cfg(target_os = "linux")]
    unsafe {
        let mut name = [0u8; 16];
        let bytes = KILLALL_PROCESS_NAME.as_bytes();
        let len = bytes.len().min(name.len().saturating_sub(1));
        name[..len].copy_from_slice(&bytes[..len]);
        let _ = libc::prctl(libc::PR_SET_NAME, name.as_ptr(), 0, 0, 0);
    }
}

pub fn set_server_title(server_name: &str) {
    set_title(compact_process_title("kcode:s:", Some(server_name)));
}

pub fn set_client_generic_title(is_selfdev: bool) {
    let prefix = if is_selfdev {
        "kcode:selfdev"
    } else {
        "kcode:client"
    };
    set_title(compact_process_title(prefix, None));
}

pub fn set_client_session_title(session_id: &str, is_selfdev: bool) {
    set_client_display_title(&session_name(session_id), is_selfdev);
}

pub fn set_client_display_title(session_name: &str, is_selfdev: bool) {
    let prefix = if is_selfdev { "kcode:d:" } else { "kcode:c:" };
    set_title(compact_process_title(prefix, Some(session_name)));
}

pub fn set_client_remote_display_title(server_name: &str, session_name: &str, is_selfdev: bool) {
    if server_name.is_empty() || server_name.eq_ignore_ascii_case("kcode") {
        set_client_display_title(session_name, is_selfdev);
        return;
    }
    let prefix = if is_selfdev { "kcode:d:" } else { "kcode:c:" };
    set_title(format!("{prefix}{server_name}/{session_name}"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_window_label_uses_the_memorable_name() {
        assert_eq!(session_window_label("session_fox_123", None), "kcode Fox");
        assert_eq!(
            session_window_label("session_fox_123", Some("kcode")),
            "kcode Fox"
        );
        assert_eq!(
            session_window_label("session_fox_123", Some("Harbor")),
            "kcode/harbor Fox"
        );
    }

    #[test]
    fn session_window_label_falls_back_to_the_raw_id() {
        assert_eq!(
            session_window_label("imported_codex_abc", None),
            "kcode Imported_codex_abc"
        );
    }

    #[test]
    fn terminal_window_title_is_icon_plus_label() {
        assert_eq!(
            terminal_window_title("\u{1f419}", "kcode Fox", false),
            "\u{1f419} kcode Fox"
        );
        assert_eq!(
            terminal_window_title("\u{1f419}", "kcode/harbor Fox", true),
            "\u{1f419} kcode/harbor Fox [self-dev]"
        );
    }
}
