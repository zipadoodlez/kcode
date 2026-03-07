#[cfg(feature = "jemalloc")]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

// Tune jemalloc for a long-running server with bursty allocations (e.g. loading
// and unloading an ~87 MB ONNX embedding model). The defaults (muzzy_decay_ms:0,
// retain:true, narenas:8*ncpu) caused 1.4 GB RSS in previous testing.
//
// dirty_decay_ms:1000  — return dirty pages to OS after 1 s idle
// muzzy_decay_ms:1000  — release muzzy pages after 1 s
// narenas:4            — limit arena count (17 threads don't need 64 arenas)
#[cfg(feature = "jemalloc")]
#[allow(non_upper_case_globals)]
#[no_mangle]
pub static malloc_conf: Option<&'static [u8; 50]> =
    Some(b"dirty_decay_ms:1000,muzzy_decay_ms:1000,narenas:4\0");

mod agent;
mod ambient;
mod ambient_runner;
mod ambient_scheduler;
mod auth;
mod background;
mod browser;
mod build;
mod bus;
mod cache_tracker;
mod channel;
mod compaction;
mod config;
mod copilot_usage;
#[cfg(feature = "embeddings")]
mod embedding;
#[cfg(not(feature = "embeddings"))]
mod embedding_stub;
#[cfg(not(feature = "embeddings"))]
use embedding_stub as embedding;
mod gateway;
mod gmail;
mod id;
mod logging;
mod login_qr;
mod mcp;
mod memory;
mod memory_agent;
mod memory_graph;
mod memory_log;
mod message;
mod notifications;
mod perf;
mod plan;
mod platform;
mod prompt;
mod protocol;
mod provider;
mod provider_catalog;
mod registry;
mod replay;
mod safety;
mod server;
mod session;
mod setup_hints;
mod sidecar;
mod skill;
mod startup_profile;
mod stdin_detect;
mod storage;
mod telegram;
mod todo;
mod tool;
mod transport;
mod tui;
mod update;
mod usage;
mod util;
mod video_export;

mod cli;

fn set_current_session(session_id: &str) {
    cli::terminal::set_current_session(session_id);
}

use anyhow::Result;
#[tokio::main]
async fn main() -> Result<()> {
    cli::startup::run().await
}
