pub use jcode_tui_usage_overlay::{
    OverlayAction, UsageOverlay, UsageOverlayItem, UsageOverlayStatus, UsageOverlaySummary,
};

/// App-side usage-overlay state: the open overlay and whether a usage refresh is
/// in flight. One home, so the overlay's handling reads one struct.
#[derive(Default)]
pub(super) struct UsageOverlayState {
    pub(super) overlay: Option<std::cell::RefCell<UsageOverlay>>,
    pub(super) report_refreshing: bool,
}
