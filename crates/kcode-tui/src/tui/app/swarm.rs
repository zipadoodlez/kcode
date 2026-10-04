//! App-side swarm UI state.

/// The swarm the server reports on: member status snapshots and the rows of the
/// pinned work list. One home, so the list and its `TuiState` accessors read one
/// struct.
#[derive(Default)]
pub(super) struct Swarm {
    /// Member status snapshots (remote mode only).
    pub(super) members: Vec<crate::protocol::SwarmMemberStatus>,
    /// The pinned work list's rows, as the server last pushed them.
    pub(super) plan_items: Vec<crate::plan::TaskItem>,
}
