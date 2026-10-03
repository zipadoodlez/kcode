#![cfg_attr(test, allow(clippy::items_after_test_module))]

use crate::protocol::ServerEvent;
use tokio::sync::{broadcast, mpsc};

#[cfg(test)]
#[path = "comm_control_tests.rs"]
mod tests;

pub(super) async fn handle_client_debug_command(
    id: u64,
    client_event_tx: &mpsc::UnboundedSender<ServerEvent>,
) {
    let _ = client_event_tx.send(ServerEvent::Error {
        id,
        message: "ClientDebugCommand is for internal use only".to_string(),
        retry_after_secs: None,
    });
}

pub(super) fn handle_client_debug_response(
    id: u64,
    output: String,
    client_debug_response_tx: &broadcast::Sender<(u64, String)>,
) {
    let _ = client_debug_response_tx.send((id, output));
}
