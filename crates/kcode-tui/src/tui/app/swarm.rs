//! App-side swarm UI state.

/// The swarm the server reports on, mirrored for the inline panel: member
/// snapshots, the latest plan, and the panel's selection. One home, so the
/// panel and its `TuiState` accessors read one struct.
///
/// The panel navigation methods and the subtree filtering stay on `App`: they
/// read config, session identity, and the transcript, so they are not this
/// struct's to own.
#[derive(Default)]
pub(super) struct Swarm {
    /// Member status snapshots (remote mode only).
    pub(super) members: Vec<crate::protocol::SwarmMemberStatus>,
    /// Latest swarm plan snapshot (local or remote server event stream).
    pub(super) plan_items: Vec<crate::plan::TaskItem>,
    pub(super) plan_swarm_id: Option<String>,
    /// Currently selected agent index in the inline swarm panel (display order).
    pub(super) panel_selected: usize,
    /// Whether the inline swarm panel has keyboard focus (navigable list + detail).
    pub(super) panel_focused: bool,
    /// Whether the focused swarm panel owns the main transcript viewport.
    pub(super) panel_full_page: bool,
}
