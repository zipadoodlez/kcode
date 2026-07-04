//! Composition-root registry for externally-implemented provider runtimes.
//!
//! Provider runtime implementations are being moved out of `jcode-base` into
//! downstream crates (e.g. `jcode-provider-gemini-runtime`) so that editing a
//! provider no longer rebuilds the base -> app-core -> tui -> root spine.
//! Because those crates sit *downstream* of base, base cannot name their
//! concrete types. Instead, the binary's composition root (`src/cli/startup.rs`)
//! registers a factory here before any `MultiProvider` is constructed, and
//! base instantiates through the registry.
//!
//! This is deliberately a process-global registry rather than constructor
//! injection: `MultiProvider` is constructed from many call sites (startup,
//! post-auth hot-init, TUI onboarding/overnight flows), and threading factories
//! through each would couple all of them to the full provider set. The
//! registry is written once at startup and read-only afterwards.

use super::Provider;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};

/// Registry key for the Gemini provider runtime.
pub const GEMINI_RUNTIME: &str = "gemini";

/// Registry key for the Cursor provider runtime.
pub const CURSOR_RUNTIME: &str = "cursor";

/// Registry key for the Antigravity provider runtime.
pub const ANTIGRAVITY_RUNTIME: &str = "antigravity";

/// Registry key for the GitHub Copilot provider runtime.
pub const COPILOT_RUNTIME: &str = "copilot";

/// Registry key for the deprecated Claude CLI provider runtime.
pub const CLAUDE_CLI_RUNTIME: &str = "claude-cli";

/// Registry key for the direct Anthropic API provider runtime.
pub const ANTHROPIC_RUNTIME: &str = "anthropic";

/// Registry key for the OpenAI (Codex) provider runtime.
pub const OPENAI_RUNTIME: &str = "openai";

/// Factories are fallible: a runtime whose constructor needs credentials
/// (e.g. Copilot's GitHub token load) returns `None` when they are absent
/// or invalid, and callers treat that like an unavailable provider.
type Factory = Arc<dyn Fn() -> Option<Arc<dyn Provider>> + Send + Sync>;

fn registry() -> &'static RwLock<HashMap<&'static str, Factory>> {
    static REGISTRY: OnceLock<RwLock<HashMap<&'static str, Factory>>> = OnceLock::new();
    REGISTRY.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Register a factory for an externally-implemented provider runtime.
///
/// Call this from the binary's composition root before any provider selection
/// runs. Registering the same key again replaces the previous factory (useful
/// for tests).
pub fn register_external_provider<F>(key: &'static str, factory: F)
where
    F: Fn() -> Arc<dyn Provider> + Send + Sync + 'static,
{
    register_external_provider_fallible(key, move || Some(factory()));
}

/// Register a fallible factory for an externally-implemented provider runtime.
///
/// Use this for runtimes whose construction can fail (e.g. a credential load);
/// returning `None` is treated as "provider unavailable" rather than a wiring
/// bug.
pub fn register_external_provider_fallible<F>(key: &'static str, factory: F)
where
    F: Fn() -> Option<Arc<dyn Provider>> + Send + Sync + 'static,
{
    registry()
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(key, Arc::new(factory));
}

/// Whether a runtime factory has been registered for `key`.
pub fn external_provider_registered(key: &str) -> bool {
    registry()
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .contains_key(key)
}

/// Instantiate a registered external provider runtime.
///
/// Returns `None` when no factory is registered. Callers that already verified
/// credentials should treat `None` as a wiring bug and log it: the binary
/// forgot to register the runtime at startup.
pub fn instantiate_external_provider(key: &str) -> Option<Arc<dyn Provider>> {
    let factory = registry()
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(key)
        .cloned()?;
    factory()
}

/// Instantiate `key`, logging a wiring warning when credentials exist but the
/// runtime was never registered by the composition root.
pub(crate) fn instantiate_expected_external_provider(key: &str) -> Option<Arc<dyn Provider>> {
    let provider = instantiate_external_provider(key);
    if provider.is_none() {
        crate::logging::warn(&format!(
            "{key} credentials are available but no {key} provider runtime is registered; \
             the composition root must call register_external_provider() at startup"
        ));
    }
    provider
}
