//! Local IPC transport: a Unix socket exposed as `Listener`/`Stream`.
//!
//! The API matches `tokio::net::Unix*` so door adapters can take the transport
//! as a dependency without pulling in `jcode-base`.

mod unix;
pub use unix::*;
