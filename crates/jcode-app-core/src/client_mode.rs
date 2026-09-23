//! Client-side self-development mode flag.
//!
//! `JCODE_CLIENT_SELFDEV_MODE` marks a client launched from a jcode source
//! checkout. It selects the canary/debug session surface. The build/install and
//! update machinery that used to consume it is gone: the package manager owns
//! installed binaries, so this flag now only distinguishes the debug session
//! surface.

/// Environment variable that marks a client running from a jcode checkout.
pub const CLIENT_SELFDEV_ENV: &str = "JCODE_CLIENT_SELFDEV_MODE";

pub fn client_selfdev_requested() -> bool {
    std::env::var(CLIENT_SELFDEV_ENV).is_ok()
}
