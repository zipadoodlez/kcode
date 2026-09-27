//! The pinned background-task band: retained rows for running and recently
//! finished background tasks, rendered beneath the pinned todo band.
//!
//! Invariant: `rows` holds at most the two most recently active tasks, newest
//! last. Every mutation below keeps that trim, and `rows` is not reachable for
//! mutation from outside, so no caller can leave the band with stale rows.

use crate::tui::{BackgroundTaskRow, BackgroundTaskRowStatus};

#[derive(Default)]
pub(super) struct BackgroundTaskBand {
    rows: Vec<BackgroundTaskRow>,
}

impl BackgroundTaskBand {
    pub(super) fn rows(&self) -> &[BackgroundTaskRow] {
        &self.rows
    }

    pub(super) fn upsert_running(&mut self, task_id: String, label: String, percent: Option<f32>) {
        if let Some(index) = self.rows.iter().position(|task| task.task_id == task_id) {
            let mut task = self.rows.remove(index);
            task.label = label;
            // Output parsers can alternate between determinate updates (for
            // example `42%`) and phase-only updates (`Compiling foo`). A
            // phase-only update must not erase the last useful percentage,
            // otherwise the pinned progress bar jumps back to zero until the
            // task completes at 100%.
            if percent.is_some() || task.percent.is_none() {
                task.percent = percent;
            }
            task.status = BackgroundTaskRowStatus::Running;
            task.completed_at = None;
            self.rows.push(task);
            return;
        }
        self.rows.push(BackgroundTaskRow {
            task_id,
            label,
            percent,
            status: BackgroundTaskRowStatus::Running,
            completed_at: None,
        });
        self.retain_latest();
    }

    pub(super) fn upsert_progress(&mut self, content: &str) -> bool {
        let Some(progress) =
            crate::message::parse_background_task_progress_notification_markdown(content)
        else {
            return false;
        };
        let label = crate::message::background_task_display_label(
            &progress.tool_name,
            progress.display_name.as_deref(),
        );
        self.upsert_running(progress.task_id, label, progress.percent);
        true
    }

    pub(super) fn upsert_started(&mut self, content: &str) -> bool {
        let Some(started) =
            crate::message::parse_background_task_started_notification_markdown(content)
        else {
            return false;
        };
        self.upsert_running(started.task_id, started.label, None);
        true
    }

    pub(super) fn finish(
        &mut self,
        task_id: String,
        label: String,
        status: BackgroundTaskRowStatus,
    ) {
        if let Some(index) = self.rows.iter().position(|task| task.task_id == task_id) {
            let mut task = self.rows.remove(index);
            task.label = label;
            task.status = status;
            task.completed_at =
                (status == BackgroundTaskRowStatus::Completed).then(std::time::Instant::now);
            if status == BackgroundTaskRowStatus::Completed {
                task.percent = Some(100.0);
            }
            self.rows.push(task);
            return;
        }
        self.rows.push(BackgroundTaskRow {
            task_id,
            label,
            percent: (status == BackgroundTaskRowStatus::Completed).then_some(100.0),
            status,
            completed_at: (status == BackgroundTaskRowStatus::Completed)
                .then(std::time::Instant::now),
        });
        self.retain_latest();
    }

    /// Successful tasks are useful as short-lived confirmation, but should not
    /// permanently consume the pinned todo band's limited space. Failures stay
    /// until acted on, and running tasks always stay visible.
    pub(super) fn prune_irrelevant(&mut self) -> bool {
        const COMPLETED_TASK_VISIBILITY: std::time::Duration = std::time::Duration::from_secs(12);
        let now = std::time::Instant::now();
        let previous_len = self.rows.len();
        self.rows.retain(|task| {
            task.completed_at.is_none_or(|completed_at| {
                now.saturating_duration_since(completed_at) < COMPLETED_TASK_VISIBILITY
            })
        });
        self.rows.len() != previous_len
    }

    fn retain_latest(&mut self) {
        const MAX_PINNED_BACKGROUND_TASKS: usize = 2;
        if self.rows.len() > MAX_PINNED_BACKGROUND_TASKS {
            let stale = self.rows.len() - MAX_PINNED_BACKGROUND_TASKS;
            self.rows.drain(..stale);
        }
    }

    #[cfg(test)]
    pub(super) fn rows_mut(&mut self) -> &mut [BackgroundTaskRow] {
        &mut self.rows
    }
}
