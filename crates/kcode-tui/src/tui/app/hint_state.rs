//! Persisted one-shot hint state.
//!
//! Three hint subsystems — hotkey familiarity (`hotkey_feedback`), keybinding
//! proficiency (`shortcut_hints`), and the swarm-config nudge (`swarm_hint`) —
//! each persist a small JSON file under the app config dir. Each carried its own
//! copy of the same load/save skeleton. The mechanism lives here once; a caller
//! keeps its own state type and file name.

use serde::{Serialize, de::DeserializeOwned};

/// Read `file` from the app config dir, falling back to `T::default()` when it
/// is absent, unreadable, or does not parse.
pub(super) fn load<T: Default + DeserializeOwned>(file: &str) -> T {
    let Some(path) = state_path(file) else {
        return T::default();
    };
    crate::storage::read_json::<T>(&path).unwrap_or_default()
}

/// Write `state` to `file` in the app config dir. Hint state is best-effort, so
/// a write failure is logged and swallowed.
pub(super) fn save<T: Serialize>(file: &str, state: &T) {
    let Some(path) = state_path(file) else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Err(error) = crate::storage::write_json(&path, state) {
        crate::logging::info(&format!(
            "Failed to persist hint state {}: {}",
            path.display(),
            error
        ));
    }
}

/// Wall-clock time in whole seconds since the Unix epoch.
pub(super) fn now_unix_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn state_path(file: &str) -> Option<std::path::PathBuf> {
    crate::storage::app_config_dir()
        .ok()
        .map(|dir| dir.join(file))
}
