use super::*;
use crate::tui::backend;

impl App {
    pub(super) fn handle_server_event(
        &mut self,
        event: crate::protocol::ServerEvent,
        remote: &mut impl backend::RemoteEventState,
    ) -> bool {
        remote::handle_server_event(self, event, remote)
    }

    #[cfg(test)]
    pub(super) fn handle_remote_char_input(&mut self, c: char) {
        remote::handle_remote_char_input(self, c);
    }

    /// Handle keyboard input in remote mode
    #[cfg(test)]
    pub(super) async fn handle_remote_key(
        &mut self,
        code: KeyCode,
        modifiers: KeyModifiers,
        remote: &mut backend::RemoteConnection,
    ) -> Result<()> {
        remote::handle_remote_key(self, code, modifiers, remote).await
    }
}
