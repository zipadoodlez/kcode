use super::{App, PendingCatchupResume};
use crate::side_panel::SidePanelPage;

pub(super) const CATCHUP_PAGE_ID: &str = "catchup";
const CATCHUP_PAGE_TITLE: &str = "Catch Up";

/// Catch-up navigation bookkeeping: the resume waiting to fire, the one in
/// flight, and the stack of sessions to return to. One home, so the resume
/// handshake reads one struct.
#[derive(Default)]
pub(super) struct Catchup {
    pub(super) return_stack: Vec<String>,
    pub(super) pending_resume: Option<PendingCatchupResume>,
    pub(super) in_flight_resume: Option<PendingCatchupResume>,
}

impl Catchup {
    pub(super) fn queue(
        &mut self,
        target_session_id: String,
        source_session_id: Option<String>,
        queue_position: Option<(usize, usize)>,
        show_brief: bool,
    ) {
        self.pending_resume = Some(PendingCatchupResume {
            target_session_id,
            source_session_id,
            queue_position,
            show_brief,
        });
    }

    pub(super) fn take_pending(&mut self) -> Option<PendingCatchupResume> {
        self.pending_resume.take()
    }

    pub(super) fn begin_in_flight(&mut self, request: PendingCatchupResume) {
        if request.show_brief
            && let Some(source) = request.source_session_id.as_ref()
            && self.return_stack.last() != Some(source)
        {
            self.return_stack.push(source.clone());
        }
        self.in_flight_resume = Some(request);
    }

    pub(super) fn clear_in_flight(&mut self) {
        self.in_flight_resume = None;
    }

    pub(super) fn pop_return_target(&mut self) -> Option<String> {
        self.return_stack.pop()
    }
}

impl App {
    pub(super) fn maybe_show_catchup_after_history(&mut self, session_id: &str) {
        let Some(request) = self.catchup.in_flight_resume.clone() else {
            return;
        };
        if request.target_session_id != session_id {
            return;
        }
        self.catchup.in_flight_resume = None;
        if !request.show_brief {
            return;
        }

        let Ok(session) = crate::session::Session::load(session_id) else {
            self.push_display_message(crate::tui::DisplayMessage::error(format!(
                "Catch Up loaded session `{}` but could not read its persisted state.",
                session_id
            )));
            return;
        };

        let brief = crate::catchup::build_brief(&session);
        let markdown = crate::catchup::render_markdown(
            &session,
            request.source_session_id.as_deref(),
            request.queue_position,
            &brief,
        );
        let snapshot = self.decorate_side_panel_with_page(
            self.snapshot_without_page(CATCHUP_PAGE_ID),
            self.catchup_page(session_id, markdown),
            true,
        );
        self.apply_side_panel_snapshot(snapshot);
        let _ = crate::catchup::mark_seen(&session.id, session.updated_at);
    }

    fn catchup_page(&self, session_id: &str, markdown: String) -> SidePanelPage {
        SidePanelPage::ephemeral_markdown(
            CATCHUP_PAGE_ID,
            CATCHUP_PAGE_TITLE,
            format!("catchup://{}", session_id),
            markdown,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_millis() as u64)
                .unwrap_or(1)
                .max(1),
        )
    }
}
