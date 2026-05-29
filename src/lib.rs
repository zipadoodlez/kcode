#![allow(
    unknown_lints,
    clippy::collapsible_match,
    clippy::manual_checked_ops,
    clippy::unnecessary_sort_by,
    clippy::useless_conversion
)]

//! Root `jcode` crate: the presentation + entrypoint layer (cli, tui,
//! video_export) on top of the `jcode-app-core` application core.
//!
//! All non-presentation modules live in `jcode-app-core` and are re-exported
//! here via `pub use jcode_app_core::*`, so existing `crate::<module>` paths
//! (e.g. `crate::config`, `crate::server`) keep resolving unchanged across the
//! cli/tui code that was not moved.

// Re-export the entire application core so `crate::<module>` paths resolve.
pub use jcode_app_core::*;

// Presentation + entrypoint layer (kept in the root crate).
pub mod cli;
pub mod tui;
pub mod video_export;

use anyhow::Result;

pub async fn run() -> Result<()> {
    cli::startup::run().await
}
