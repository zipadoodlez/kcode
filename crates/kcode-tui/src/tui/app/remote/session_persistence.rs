use super::*;

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
