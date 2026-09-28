//! On-disk model-catalog cache: one mechanism, one home.
//!
//! Providers that fetch a live model list persist it so the next process starts
//! warm instead of paying for a cold fetch. The *payload* differs per provider (a
//! bare model-id list for Gemini/Copilot/Cursor, parsed catalog entries plus a
//! backend default for Antigravity), so the payload stays with the provider and
//! only the path/read/write/warn mechanism is shared here.
//!
//! Every provider surfaces the same bug when this is copied: the copies drift.
//! Copilot's seed used `try_write` while Gemini's used `write`, and Bedrock read
//! through a non-recovering reader, so a corrupt cache file behaved differently
//! per provider. One home is what stops that.

use serde::Serialize;
use serde::de::DeserializeOwned;
use std::path::PathBuf;

/// Path of `file_name` inside the app config directory, or `None` when the
/// directory cannot be resolved.
pub fn catalog_cache_path(file_name: &str) -> Option<PathBuf> {
    crate::storage::app_config_dir()
        .ok()
        .map(|dir| dir.join(file_name))
}

/// Read a cache payload. `None` covers absent, unreadable, and corrupt files:
/// a bad cache must never be a startup failure.
pub fn load_catalog_cache<T: DeserializeOwned>(file_name: &str) -> Option<T> {
    let path = catalog_cache_path(file_name)?;
    crate::storage::read_json(&path).ok()
}

/// Write a cache payload, warning rather than failing.
pub fn store_catalog_cache<T: Serialize>(file_name: &str, provider_label: &str, value: &T) {
    let Some(path) = catalog_cache_path(file_name) else {
        return;
    };
    if let Err(error) = crate::storage::write_json(&path, value) {
        crate::logging::warn(&format!(
            "Failed to persist {provider_label} model catalog {}: {}",
            path.display(),
            error
        ));
    }
}

/// The cache payload shared by the providers that persist a bare model-id list.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct CachedModelList {
    pub models: Vec<String>,
    pub fetched_at_rfc3339: String,
}

/// Load the shared list payload, treating an empty catalog as absent.
pub fn load_model_list(file_name: &str) -> Option<CachedModelList> {
    load_catalog_cache::<CachedModelList>(file_name).filter(|catalog| !catalog.models.is_empty())
}

/// Persist the shared list payload, skipping an empty list.
pub fn store_model_list(file_name: &str, provider_label: &str, models: &[String]) {
    if models.is_empty() {
        return;
    }
    store_catalog_cache(
        file_name,
        provider_label,
        &CachedModelList {
            models: models.to_vec(),
            fetched_at_rfc3339: chrono::Utc::now().to_rfc3339(),
        },
    );
}
