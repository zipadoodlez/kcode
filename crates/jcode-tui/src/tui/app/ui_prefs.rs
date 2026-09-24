//! Small persisted UI preferences that survive restarts and session resumes.
//!
//! These are deliberately separate from the main config file: they capture
//! in-app toggles that the user flips at runtime and expects to stick, without
//! editing `config.toml`.

#![allow(dead_code)]

use serde::{Deserialize, Serialize};

const UI_PREFS_FILE: &str = "ui_preferences.json";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct UiPreferences {
    #[serde(default)]
    pub version: u8,
}

fn prefs_path() -> Option<std::path::PathBuf> {
    crate::storage::app_config_dir()
        .ok()
        .map(|dir| dir.join(UI_PREFS_FILE))
}

pub(crate) fn load() -> UiPreferences {
    let Some(path) = prefs_path() else {
        return UiPreferences::default();
    };
    crate::storage::read_json::<UiPreferences>(&path).unwrap_or_default()
}
