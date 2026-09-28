use std::ffi::{OsStr, OsString};

/// The public spelling of kcode's environment surface.
const PUBLIC_ENV_PREFIX: &str = "KCODE_";

/// The upstream spelling, still what the code reads.
const LEGACY_ENV_PREFIX: &str = "JCODE_";

/// Accept `KCODE_*` as the public spelling of every `JCODE_*` variable.
///
/// Configuration is read through `JCODE_*` names throughout the tree, while the
/// fork's public name is `kcode`. Rather than rewrite several thousand literals
/// to change which spelling users type, the process copies each `KCODE_*`
/// variable onto its `JCODE_*` twin at startup. `KCODE_*` wins when both are
/// set, so the public spelling always overrides the legacy one.
///
/// Call this once, as early as possible in the process, before anything reads
/// configuration: a variable read before this runs still needs the legacy name.
/// When the literals are eventually renamed, flip the direction of the copy and
/// keep it for one release as the legacy fallback.
pub fn adopt_kcode_env_prefix() {
    // Collect before mutating, because `vars_os` borrows the live environment.
    let public: Vec<(OsString, OsString)> = std::env::vars_os()
        .filter_map(|(key, value)| {
            let suffix = key.to_str()?.strip_prefix(PUBLIC_ENV_PREFIX)?;
            Some((
                OsString::from(format!("{LEGACY_ENV_PREFIX}{suffix}")),
                value,
            ))
        })
        .collect();

    for (legacy_key, value) in public {
        set_var(legacy_key, value);
    }
}

/// Mutate the process environment for kcode runtime configuration.
///
/// Rust 2024 makes environment mutation unsafe because it can race with
/// concurrent environment access in foreign code. jcode intentionally mutates
/// process-local env vars to coordinate provider/runtime bootstrap before or
/// during task execution. We centralize that unsafety here so call sites remain
/// auditable.
pub fn set_var<K, V>(key: K, value: V)
where
    K: AsRef<OsStr>,
    V: AsRef<OsStr>,
{
    // SAFETY: jcode treats these mutations as process-global configuration.
    // They are a pre-existing design choice used throughout startup, auth,
    // provider bootstrap, tests, and self-dev flows. Centralizing the unsafe
    // operation here makes the Rust 2024 requirement explicit without
    // scattering unsafe blocks across hundreds of call sites.
    unsafe {
        std::env::set_var(key, value);
    }
}

/// Remove a process environment variable used by kcode runtime configuration.
pub fn remove_var<K>(key: K)
where
    K: AsRef<OsStr>,
{
    // SAFETY: see `set_var` above; this is the corresponding centralized
    // removal operation for the same process-global configuration surface.
    unsafe {
        std::env::remove_var(key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Unique per test so parallel tests cannot see each other's variables.
    const SUFFIX: &str = "KCODE_ENV_PREFIX_TEST_7A31";

    fn public() -> String {
        format!("{PUBLIC_ENV_PREFIX}{SUFFIX}")
    }

    fn legacy() -> String {
        format!("{LEGACY_ENV_PREFIX}{SUFFIX}")
    }

    #[test]
    fn kcode_prefix_is_mirrored_onto_the_legacy_name() {
        remove_var(legacy());
        set_var(public(), "from-public");

        adopt_kcode_env_prefix();

        assert_eq!(std::env::var(legacy()).unwrap(), "from-public");

        remove_var(public());
        remove_var(legacy());
    }

    #[test]
    fn kcode_prefix_wins_over_the_legacy_name() {
        set_var(legacy(), "from-legacy");
        set_var(public(), "from-public");

        adopt_kcode_env_prefix();

        assert_eq!(std::env::var(legacy()).unwrap(), "from-public");

        remove_var(public());
        remove_var(legacy());
    }

    #[test]
    fn adoption_leaves_an_unpaired_legacy_variable_alone() {
        remove_var(public());
        set_var(legacy(), "from-legacy");

        adopt_kcode_env_prefix();

        assert_eq!(std::env::var(legacy()).unwrap(), "from-legacy");

        remove_var(legacy());
    }
}
