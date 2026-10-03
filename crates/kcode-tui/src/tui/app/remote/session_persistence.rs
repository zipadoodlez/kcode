use super::*;

pub(super) fn persist_replay_display_message(
    app: &mut App,
    role: &str,
    title: Option<String>,
    content: &str,
) {
    if app.is_remote_client() {
        // In remote mode, the server owns authoritative session history. Persisting the
        // client's stale shadow copy can roll back newer turns after reconnect/reload.
        return;
    }
    app.session
        .record_replay_display_message(role.to_string(), title, content.to_string());
    let _ = app.session.save();
}

pub(super) fn persist_remote_session_metadata<F>(app: &mut App, update: F) -> Result<()>
where
    F: FnOnce(&mut crate::session::Session),
{
    if crate::tui::is_ssh_remote() {
        anyhow::bail!("Session metadata belongs to the SSH server; local persistence is disabled");
    }
    let session_id = app
        .resume_target_session_id()
        .unwrap_or(app.session.id.as_str());
    let mut session = crate::session::Session::load(session_id)?;
    update(&mut session);
    session.save()?;
    app.session = session;
    Ok(())
}

pub(super) fn reload_marker_active() -> bool {
    crate::server::reload_marker_active(RELOAD_MARKER_MAX_AGE)
}
