//! Throwaway test roots, installed before the test harness starts.
//!
//! A test that builds an agent or saves a session resolves `KCODE_HOME` (and
//! reload state under `KCODE_RUNTIME_DIR`). With the developer's shell running
//! the suite, an unset root means the real `~/.kcode`, so tests wrote real
//! sessions and left `active_pids` markers that the next TUI start swept as
//! crashes. This installs a per-process throwaway root for both, before any
//! test thread runs, so a test cannot forget.
//!
//! Per-test scoping still wins: this only acts when a root is unset, so
//! `storage::lock_test_env` plus `configure_test_env` can override and restore
//! it around one test. Compiled only under `cfg(test)` or the `test-support`
//! feature, which is the dev-dependency feature downstream crates enable, so
//! one install covers every crate's test binary that links `kcode-base`.

use std::path::PathBuf;

/// Point this process at throwaway `KCODE_HOME` and `KCODE_RUNTIME_DIR` when
/// the environment does not already pin them. Idempotent.
pub fn ensure_test_env() {
    if std::env::var_os("KCODE_HOME").is_none() {
        crate::env::set_var("KCODE_HOME", root("home"));
    }
    if std::env::var_os("KCODE_RUNTIME_DIR").is_none() {
        crate::env::set_var("KCODE_RUNTIME_DIR", root("runtime"));
    }
}

/// One directory per process and kind. Created once and left for the OS temp
/// cleaner, so it outlives every test in the binary.
fn root(kind: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("kcode-test-{kind}-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&path);
    path
}

/// Runs before the test harness's `main`, so every test thread starts with the
/// throwaway roots in place.
#[ctor::ctor(unsafe)]
fn install() {
    ensure_test_env();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A test binary that lost this install would write the developer's real
    /// store; pin the process root so it cannot be dropped silently.
    #[test]
    fn the_process_root_is_a_temp_dir() {
        ensure_test_env();
        let dir = crate::storage::kcode_dir().expect("kcode dir");
        assert!(
            dir.starts_with(std::env::temp_dir()),
            "kcode_dir() resolved to {dir:?}, outside the temp dir"
        );
    }
}
