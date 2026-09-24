//! Mapping from parsed CLI arguments to an initial process title.
//!
//! This logic depends on the clap `Args`/`Command` types defined in `cli`, so
//! it lives in the CLI layer. The low-level title-setting primitives it uses
//! (`compact_process_title`, `session_name`, `set_title`) live in the
//! `process_title` core module.

use crate::cli::args::{Args, Command};
use crate::process_title::{compact_process_title, session_name, set_title};

pub(crate) fn initial_title(args: &Args) -> String {
    match &args.command {
        Some(Command::Serve { .. }) => "kcode:server".to_string(),
        Some(Command::Acp) => "kcode acp".to_string(),
        Some(Command::Server { .. }) => "kcode server".to_string(),
        Some(Command::Connect) => "kcode:client".to_string(),
        Some(Command::Run { .. }) => "kcode run".to_string(),
        Some(Command::Login { .. }) => "kcode login".to_string(),
        Some(Command::Account { .. }) => "kcode account".to_string(),
        Some(Command::Repl) => "kcode repl".to_string(),
        Some(Command::Version { .. }) => "kcode version".to_string(),
        Some(Command::Usage { .. }) => "kcode usage".to_string(),
        Some(Command::Debug { .. }) => "kcode debug".to_string(),
        Some(Command::Auth(_)) => "kcode auth".to_string(),
        Some(Command::Provider(_)) => "kcode provider".to_string(),
        Some(Command::Session(_)) => "kcode session".to_string(),
        Some(Command::Permissions) => "kcode permissions".to_string(),
        Some(Command::Transcript { .. }) => "kcode transcript".to_string(),
        Some(Command::Browser { .. }) => "kcode browser".to_string(),
        Some(Command::Model(_)) => "kcode model".to_string(),
        Some(Command::ProviderTestCoverage { .. }) => "kcode provider-test-coverage".to_string(),
        Some(Command::ProviderDoctor { .. }) => "kcode provider-doctor".to_string(),
        Some(Command::AuthTest { .. }) => "kcode auth-test".to_string(),
        Some(Command::Restart { .. }) => "kcode restart".to_string(),
        None => {
            if let Some(resume) = args.resume.as_deref().filter(|resume| !resume.is_empty()) {
                let prefix = if crate::client_mode::client_selfdev_requested() {
                    "kcode:d:"
                } else {
                    "kcode:c:"
                };
                compact_process_title(prefix, Some(&session_name(resume)))
            } else if crate::client_mode::client_selfdev_requested() {
                "kcode:selfdev".to_string()
            } else {
                "kcode:client".to_string()
            }
        }
    }
}

pub(crate) fn set_initial_title(args: &Args) {
    set_title(initial_title(args));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::lock_test_env;
    use clap::Parser;

    const SELFDEV_ENV: &str = crate::client_mode::CLIENT_SELFDEV_ENV;

    fn with_selfdev_env_removed<T>(f: impl FnOnce() -> T) -> T {
        let _guard = lock_test_env();
        let previous = std::env::var_os(SELFDEV_ENV);
        crate::env::remove_var(SELFDEV_ENV);
        let result = f();
        if let Some(value) = previous {
            crate::env::set_var(SELFDEV_ENV, value);
        }
        result
    }

    #[test]
    fn initial_title_labels_server() {
        with_selfdev_env_removed(|| {
            let args = Args::parse_from(["kcode", "serve"]);
            assert_eq!(initial_title(&args), "kcode:server");
        });
    }

    #[test]
    fn initial_title_labels_resume_client_with_short_name() {
        with_selfdev_env_removed(|| {
            let args = Args::parse_from(["kcode", "--resume", "session_fox_123"]);
            assert_eq!(initial_title(&args), "kcode:c:fox");
        });
    }
}
