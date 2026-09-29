use super::super::{App, DisplayMessage};
use super::RemoteConnection;
use crate::bus::{BusEvent, UiActivity, UiActivityKind};
use crate::message::parse_background_task_progress_notification_markdown;

pub(crate) async fn handle_bus_event(
    app: &mut App,
    remote: &mut RemoteConnection,
    bus_event: std::result::Result<BusEvent, tokio::sync::broadcast::error::RecvError>,
) -> bool {
    match bus_event {
        Ok(BusEvent::UsageReport(results)) => {
            app.handle_usage_report(results);
            true
        }
        Ok(BusEvent::ClipboardPasteCompleted(result)) => {
            app.handle_clipboard_paste_completed(result)
        }
        Ok(BusEvent::ModelRefreshCompleted(result)) => {
            app.handle_model_refresh_completed(result);
            true
        }
        Ok(BusEvent::UiActivity(activity)) => handle_ui_activity(app, activity),
        Ok(BusEvent::GitStatusCompleted(result)) => {
            super::super::commands::handle_git_status_completed(app, result);
            true
        }
        Ok(BusEvent::MermaidRenderCompleted) => true,
        Ok(BusEvent::UsageReportProgress(progress)) => {
            app.handle_usage_report_progress(progress);
            true
        }
        Ok(BusEvent::LoginCompleted(login)) => {
            if crate::tui::is_ssh_remote() {
                app.set_status_notice("Local login does not change SSH server credentials");
                return true;
            }
            let success = login.success && login.provider != "copilot_code";
            let provider_hint = auth_provider_hint_for_login_provider(&login.provider);
            let auth = auth_changed_event_for_login_provider(&login.provider);
            let prefer_strongest = success && app.should_prefer_strongest_model();
            app.handle_login_completed(login);
            if success
                && let Err(error) = remote
                    .notify_auth_changed_event(provider_hint, auth, prefer_strongest)
                    .await
            {
                crate::logging::warn(&format!(
                    "Failed to notify server about refreshed auth: {error}"
                ));
                app.finish_auth_catalog_refresh();
                app.set_status_notice("Model setup will retry after reconnect");
            }
            true
        }
        Ok(BusEvent::UpdateStatus(status)) => {
            app.handle_update_status(status);
            true
        }
        Ok(BusEvent::SessionUpdateStatus(status)) => {
            app.handle_session_update_status(status);
            true
        }
        _ => false,
    }
}

/// Apply a bus `UiActivity` to the client's visible state.
///
/// Activities for another session are dropped. Background work contributes to
/// the background-task band, auth/catalog work contributes a transcript line
/// (catalog progress updates an existing row instead), and any `status_notice`
/// is applied. Returns whether the app changed, so the caller can redraw.
pub(crate) fn handle_ui_activity(app: &mut App, activity: UiActivity) -> bool {
    let Some(session_id) = app.active_client_session_id() else {
        return false;
    };
    if !activity.is_visible_to_session(session_id) {
        return false;
    }

    match activity.kind {
        UiActivityKind::Background => {
            if !app.background_tasks.upsert_started(&activity.message) {
                app.push_display_message(DisplayMessage::background_task(activity.message.clone()))
            }
        }
        UiActivityKind::Auth | UiActivityKind::Catalog => {
            if activity.message.trim().is_empty() {
                // Status-only lifecycle updates should not leave blank transcript
                // entries.
            } else if activity.kind == UiActivityKind::Catalog
                && parse_background_task_progress_notification_markdown(&activity.message).is_some()
            {
                app.background_tasks.upsert_progress(&activity.message);
            } else {
                app.push_display_message(DisplayMessage::system(activity.message.clone()))
            }
        }
    }
    if let Some(status_notice) = activity.status_notice {
        app.set_status_notice(status_notice);
    }
    true
}

/// Resolve the canonical auth provider id the server uses to attribute an
/// auth-change refresh for a completed login.
///
/// `LoginCompleted.provider` is the login descriptor's display label (e.g.
/// "Anthropic API"), id, or alias - not the canonical server provider id. This
/// used to only map Azure and OpenAI-compatible logins, so direct logins
/// (Claude OAuth/API key, OpenAI, OpenRouter, Bedrock, ...) sent no hint. With
/// no hint the server fell back to the session's currently active provider,
/// mislabeling the catalog-refresh message ("OpenAI credentials are active"
/// after an Anthropic API-key login) and skipping the post-login model switch.
/// The catalog namespace to attribute an auth change to.
///
/// This is the third provider spelling: not the runtime key
/// (`LoginProviderTarget::key()`) and not `descriptor.id` in the compatible
/// case, where the namespace is the profile id. One question, one function.
pub(crate) fn auth_provider_hint_for_login_provider(provider: &str) -> Option<&'static str> {
    let provider = provider.trim();
    // Azure's runtime id ("azure-openai") differs from its login descriptor id
    // ("azure"); keep the dedicated mapping used across the auth lifecycle.
    if provider.eq_ignore_ascii_case("azure")
        || provider.eq_ignore_ascii_case("azure-openai")
        || provider.eq_ignore_ascii_case("azure openai")
    {
        return Some("azure-openai");
    }

    use crate::provider_catalog::LoginProviderTarget;
    let descriptor = crate::provider_catalog::resolve_login_provider_loose(provider)?;
    match descriptor.target {
        LoginProviderTarget::Azure => Some("azure-openai"),
        // OpenAI-compatible profiles carry their own catalog namespace id.
        LoginProviderTarget::OpenAiCompatible(profile) => Some(profile.id),
        // Auto-import has no single runtime to attribute the refresh to.
        LoginProviderTarget::AutoImport => None,
        _ => Some(descriptor.id),
    }
}

pub(crate) fn auth_changed_event_for_login_provider(
    provider: &str,
) -> Option<crate::protocol::AuthChanged> {
    use crate::provider_catalog::LoginProviderTarget;
    let provider_id = auth_provider_hint_for_login_provider(provider)?;
    let mut auth = crate::protocol::AuthChanged::new(provider_id);
    // These fields are informational; the server routes off `provider` and the
    // `expected_*` hints. Reflect the descriptor's auth kind so OAuth logins are
    // not recorded as API-key pastes.
    let descriptor = crate::provider_catalog::resolve_login_provider_loose(provider);
    let api_key_login = descriptor
        .map(|descriptor| {
            use crate::provider_catalog::LoginProviderAuthKind;
            matches!(
                descriptor.auth_kind,
                LoginProviderAuthKind::ApiKey | LoginProviderAuthKind::Hybrid
            )
        })
        .unwrap_or(true);
    if api_key_login {
        auth.auth_method = Some(crate::protocol::AuthMethod::RemoteTuiPasteApiKey);
        auth.credential_source = Some(crate::protocol::AuthCredentialSource::ApiKeyFile);
    }
    // Only logins whose descriptor actually targets the OpenAI-compatible
    // runtime claim its namespace. Do not key this off
    // `openai_compatible_profile_by_id`: native providers (`anthropic-api`,
    // `openai-api`) alias doctor-probe compat profiles with the same id, but
    // their auth activation deliberately routes through the native runtime.
    if provider_id == "azure-openai" {
        auth.expected_runtime = Some(crate::protocol::RuntimeProviderKey::new("azure-openai"));
        auth.expected_catalog_namespace =
            Some(crate::protocol::CatalogNamespace::new("azure-openai"));
    } else if descriptor
        .is_some_and(|d| matches!(d.target, LoginProviderTarget::OpenAiCompatible(_)))
    {
        auth.expected_runtime = Some(crate::protocol::RuntimeProviderKey::new(
            "openai-compatible",
        ));
        auth.expected_catalog_namespace = Some(crate::protocol::CatalogNamespace::new(provider_id));
    }
    Some(auth)
}
