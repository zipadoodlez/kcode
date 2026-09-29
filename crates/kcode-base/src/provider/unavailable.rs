use super::*;

impl MultiProvider {
    pub(super) fn provider_is_configured(&self, provider: ActiveProvider) -> bool {
        self.reconcile_auth_if_provider_missing(provider)
    }

    pub(super) fn summarize_error(err: &anyhow::Error) -> String {
        err.to_string()
            .lines()
            .next()
            .unwrap_or("unknown error")
            .trim()
            .to_string()
    }

    /// Guidance for a provider whose accounts are all exhausted.
    pub(super) fn additional_no_provider_guidance(&self) -> Vec<String> {
        [ActiveProvider::Claude, ActiveProvider::OpenAI]
            .into_iter()
            .filter_map(crate::provider::account_failover::account_switch_guidance)
            .collect()
    }

    /// The error a caller sees when the active provider cannot serve the turn.
    ///
    /// kcode does not switch providers for the user: the message names the
    /// failure and how to switch.
    pub(super) fn no_provider_available_error(&self, notes: &[String]) -> anyhow::Error {
        let mut msg = format!(
            "Provider unavailable: {}. Switch with `/model`, or start with `kcode --provider <id>`.",
            notes.join(" | ")
        );
        let extra_guidance = self.additional_no_provider_guidance();
        if !extra_guidance.is_empty() {
            msg.push(' ');
            msg.push_str(&extra_guidance.join(" "));
        }
        msg.push_str(
            " Run `kcode provider list` for ids, `/usage` for limits, and `/login <provider>` to re-authenticate.",
        );
        anyhow::anyhow!(msg)
    }
}
