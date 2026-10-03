//! `kcode-base`: foundational layer of the kcode application core.
//!
//! This crate holds the downward-closed set of modules that the upper
//! server/tool/agent layer (`kcode-app-core`) depends on: provider, auth,
//! config, session, message, memory, and their supporting leaves.
//! Splitting it out lets the two halves compile as separate rustc units so the
//! largest compilation unit (and its peak memory) is roughly halved.
//!
//! `kcode-app-core` re-exports this crate via `pub use kcode_base::*`, so every
//! existing `crate::<module>` path in the upper layers keeps resolving.

#![allow(
    unknown_lints,
    clippy::collapsible_match,
    clippy::manual_checked_ops,
    clippy::unnecessary_sort_by,
    clippy::useless_conversion
)]

pub mod auth;
pub mod background;
pub mod browser;
pub mod bus;
pub mod cache_invalidation;
pub mod cache_tracker;
pub mod client_input;
pub mod compaction;
pub mod config;
pub mod console;
pub mod copilot_usage;
pub mod env;
pub mod generated_image;
pub mod github;
pub mod hooks;
pub mod id;
pub mod live_tests;
pub mod logging;
pub mod login_qr;
pub mod mcp;
pub mod message;
pub mod model_pricing;
pub mod model_usage;
pub mod output_style;
pub mod plan;
pub mod platform;
pub mod power_inhibit;
pub mod process_memory;
pub mod process_title;
pub mod prompt;
pub mod protocol;
pub mod provider;
pub mod provider_activity;
pub mod provider_catalog;
pub mod recent_session_index;
pub mod registry;
pub mod runtime_memory_log;
pub mod safety;
pub mod secret_input;
pub mod session;
pub mod session_list_cache;
pub mod session_metrics;
pub mod side_panel;
pub mod skill;
pub mod soft_interrupt_store;
pub mod stdin_detect;
pub mod storage;
pub mod terminal_launch;
#[cfg(any(test, feature = "test-support"))]
pub mod test_env;
pub mod todo;
pub mod transport;
pub mod usage;
pub mod util;
pub use kcode_core::{terminal_eprint, terminal_eprintln, terminal_print, terminal_println};
