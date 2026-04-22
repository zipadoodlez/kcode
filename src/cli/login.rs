use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};

use crate::auth;
use crate::provider_catalog::{
    LoginProviderDescriptor, LoginProviderTarget, OPENAI_COMPAT_LOCAL_ENABLED_ENV,
    OpenAiCompatibleProfile, resolve_login_selection, resolve_openai_compatible_profile,
};

use super::provider_init::{ProviderChoice, login_provider_for_choice, save_named_api_key};

#[derive(Debug, Clone, Default)]
pub struct LoginOptions {
    pub no_browser: bool,
    pub print_auth_url: bool,
    pub callback_url: Option<String>,
    pub auth_code: Option<String>,
    pub json: bool,
    pub complete: bool,
    pub google_access_tier: Option<auth::google::GmailAccessTier>,
}

impl LoginOptions {
    fn has_provided_input(&self) -> bool {
        self.callback_url.is_some() || self.auth_code.is_some()
    }

    fn resolve_provided_input(&self) -> Result<Option<ProvidedAuthInput>> {
        match (&self.callback_url, &self.auth_code) {
            (Some(_), Some(_)) => {
                anyhow::bail!("Specify only one of --callback-url or --auth-code.")
            }
            (Some(value), None) => Ok(Some(ProvidedAuthInput::CallbackUrl(resolve_auth_input(
                value,
            )?))),
            (None, Some(value)) => Ok(Some(ProvidedAuthInput::AuthCode(resolve_auth_input(
                value,
            )?))),
            (None, None) => Ok(None),
        }
    }

    fn uses_scriptable_flow(&self) -> Result<bool> {
        Ok(self.print_auth_url || self.complete || self.has_provided_input())
    }
}

#[derive(Debug, Clone)]
enum ProvidedAuthInput {
    CallbackUrl(String),
    AuthCode(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LoginFlowOutcome {
    Completed,
    Deferred,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "provider", rename_all = "snake_case")]
enum PendingScriptableLogin {
    Claude {
        account_label: String,
        verifier: String,
        redirect_uri: String,
    },
    Openai {
        account_label: String,
        verifier: String,
        state: String,
        redirect_uri: String,
    },
    Gemini {
        verifier: String,
        redirect_uri: String,
    },
    Antigravity {
        verifier: String,
        state: String,
        redirect_uri: String,
    },
    Google {
        verifier: String,
        state: String,
        redirect_uri: String,
        tier: auth::google::GmailAccessTier,
    },
    Copilot {
        device_code: String,
        user_code: String,
        verification_uri: String,
        interval: u64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PendingScriptableLoginRecord {
    expires_at_ms: i64,
    login: PendingScriptableLogin,
}

impl PendingScriptableLogin {
    fn key(&self) -> &'static str {
        match self {
            Self::Claude { .. } => "claude",
            Self::Openai { .. } => "openai",
            Self::Gemini { .. } => "gemini",
            Self::Antigravity { .. } => "antigravity",
            Self::Google { .. } => "google",
            Self::Copilot { .. } => "copilot",
        }
    }

    fn pending_path(&self) -> Result<PathBuf> {
        pending_login_path(self.key())
    }

    fn default_expires_at_ms(&self) -> i64 {
        current_time_ms() + 30 * 60 * 1000
    }
}

#[derive(Debug, Clone, Serialize)]
struct ScriptableAuthPrompt {
    status: &'static str,
    provider: String,
    auth_url: String,
    input_kind: String,
    pending_path: String,
    user_code: Option<String>,
    expires_at_ms: i64,
    resume_command: String,
}

#[derive(Debug, Clone, Serialize)]
struct ScriptableAuthSuccess {
    status: &'static str,
    provider: String,
    account_label: Option<String>,
    credentials_path: Option<String>,
    email: Option<String>,
}

pub async fn run_login(
    choice: &ProviderChoice,
    account_label: Option<&str>,
    options: LoginOptions,
) -> Result<()> {
    if let Some(provider) = login_provider_for_choice(choice) {
        if matches!(choice, ProviderChoice::ClaudeSubprocess) {
            eprintln!(
                "Warning: Claude subprocess transport is deprecated and will be removed. Direct Anthropic API is already the default for `--provider claude`."
            );
        }
        return run_login_provider(provider, account_label, options).await;
    }

    match choice {
        ProviderChoice::Auto => {
            if options.uses_scriptable_flow()? {
                anyhow::bail!(
                    "Scriptable login flags require an explicit provider. Use `jcode login --provider <provider> ...`."
                );
            }
            crate::telemetry::record_setup_step_once("login_picker_opened");
            let providers = crate::provider_catalog::cli_login_providers();
            if !io::stdin().is_terminal() {
                anyhow::bail!(
                    "`jcode login --provider auto` requires an interactive terminal. Use `jcode login --provider <provider>` in non-interactive mode."
                );
            }
            if let Some(imported) =
                super::provider_init::maybe_run_external_auth_auto_import_flow().await?
                && imported > 0
            {
                eprintln!("\nImported {} existing auth source(s).", imported);
                return Ok(());
            }
            eprintln!("Choose a provider to log in:");
            for (index, provider) in providers.iter().enumerate() {
                eprintln!(
                    "  {}. {:<16} - {}",
                    index + 1,
                    provider.display_name,
                    provider.menu_detail
                );
            }
            eprintln!();
            let recommended = providers
                .iter()
                .filter(|provider| provider.recommended)
                .map(|provider| provider.display_name)
                .collect::<Vec<_>>();
            if !recommended.is_empty() {
                eprintln!(
                    "  Recommended if you have a subscription: {}.",
                    recommended.join(", ")
                );
            }
            eprint!("\nEnter 1-{}: ", providers.len());
            io::stdout().flush()?;

            let mut input = String::new();
            io::stdin().read_line(&mut input)?;
            if let Some(provider) = resolve_login_selection(input.trim(), &providers) {
                run_login_provider(provider, account_label, options).await?;
            } else {
                let valid = providers
                    .iter()
                    .map(|provider| provider.id)
                    .collect::<Vec<_>>()
                    .join("|");
                anyhow::bail!("Invalid choice. Use --provider {}", valid);
            }
        }
        _ => unreachable!("handled above"),
    }
    Ok(())
}

pub async fn run_login_provider(
    provider: LoginProviderDescriptor,
    account_label: Option<&str>,
    options: LoginOptions,
) -> Result<()> {
    crate::telemetry::record_provider_selected(provider.id);
    crate::telemetry::record_auth_started(provider.id, provider.auth_kind.label());
    let login_result = if options.uses_scriptable_flow()? {
        run_scriptable_login_provider(provider, account_label, &options).await
    } else {
        match provider.target {
            LoginProviderTarget::AutoImport => {
                let imported = super::provider_init::maybe_run_external_auth_auto_import_flow()
                    .await?
                    .unwrap_or(0);
                if imported == 0 {
                    anyhow::bail!(
                        "No existing logins were imported. Either none were found, nothing was approved, or validation failed."
                    );
                }
                eprintln!("Imported {} existing auth source(s).", imported);
                Ok(LoginFlowOutcome::Completed)
            }
            LoginProviderTarget::Jcode => login_jcode_flow().map(|_| LoginFlowOutcome::Completed),
            LoginProviderTarget::Claude => login_claude_flow(account_label, options.no_browser)
                .await
                .map(|_| LoginFlowOutcome::Completed),
            LoginProviderTarget::OpenAi => login_openai_flow(account_label, options.no_browser)
                .await
                .map(|_| LoginFlowOutcome::Completed),
            LoginProviderTarget::OpenRouter => {
                login_openrouter_flow().map(|_| LoginFlowOutcome::Completed)
            }
            LoginProviderTarget::Azure => login_azure_flow().map(|_| LoginFlowOutcome::Completed),
            LoginProviderTarget::OpenAiCompatible(profile) => {
                login_openai_compatible_flow(&profile).map(|_| LoginFlowOutcome::Completed)
            }
            LoginProviderTarget::Cursor => login_cursor_flow().map(|_| LoginFlowOutcome::Completed),
            LoginProviderTarget::Copilot => {
                login_copilot_flow(options.no_browser).map(|_| LoginFlowOutcome::Completed)
            }
            LoginProviderTarget::Gemini => login_gemini_flow(options.no_browser)
                .await
                .map(|_| LoginFlowOutcome::Completed),
            LoginProviderTarget::Antigravity => login_antigravity_flow(options.no_browser)
                .await
                .map(|_| LoginFlowOutcome::Completed),
            LoginProviderTarget::Google => login_google_flow(options.no_browser)
                .await
                .map(|_| LoginFlowOutcome::Completed),
        }
    };
    let outcome = match login_result {
        Ok(outcome) => outcome,
        Err(err) => {
            let reason =
                crate::auth::login_diagnostics::classify_auth_failure_message(&err.to_string());
            crate::telemetry::record_auth_failed_reason(
                provider.id,
                provider.auth_kind.label(),
                reason.label(),
            );
            return Err(anyhow::anyhow!(
                crate::auth::login_diagnostics::augment_auth_error_message(
                    provider.id,
                    err.to_string(),
                )
            ));
        }
    };
    if matches!(outcome, LoginFlowOutcome::Deferred) {
        return Ok(());
    }
    auth::AuthStatus::invalidate_cache();
    if let Err(err) = super::commands::run_post_login_validation(provider).await {
        let reason =
            crate::auth::login_diagnostics::classify_auth_failure_message(&err.to_string());
        crate::telemetry::record_auth_failed_reason(
            provider.id,
            provider.auth_kind.label(),
            reason.label(),
        );
        return Err(anyhow::anyhow!(
            crate::auth::login_diagnostics::augment_auth_error_message(
                provider.id,
                err.to_string()
            )
        ));
    }
    auth::AuthStatus::invalidate_cache();
    Ok(())
}

async fn run_scriptable_login_provider(
    provider: LoginProviderDescriptor,
    account_label: Option<&str>,
    options: &LoginOptions,
) -> Result<LoginFlowOutcome> {
    if options.print_auth_url {
        return start_scriptable_login(provider, account_label, options).await;
    }

    let input = options.resolve_provided_input()?;
    if options.complete && input.is_some() {
        anyhow::bail!(
            "Use either --complete or an explicit --callback-url / --auth-code input, not both."
        );
    }
    complete_scriptable_login(provider, account_label, options, input).await
}

async fn start_scriptable_login(
    provider: LoginProviderDescriptor,
    account_label: Option<&str>,
    options: &LoginOptions,
) -> Result<LoginFlowOutcome> {
    let (pending, auth_url, input_kind, user_code, expires_at_ms) = match provider.target {
        LoginProviderTarget::Claude => {
            let label = auth::claude::login_target_label(account_label)?;
            let (verifier, challenge) = auth::oauth::generate_pkce_public();
            let redirect_uri = auth::oauth::claude::REDIRECT_URI.to_string();
            let auth_url = auth::oauth::claude_auth_url(&redirect_uri, &challenge, &verifier);
            (
                PendingScriptableLogin::Claude {
                    account_label: label,
                    verifier,
                    redirect_uri,
                },
                auth_url,
                "auth_code_or_callback_url",
                None,
                PendingScriptableLogin::Claude {
                    account_label: String::new(),
                    verifier: String::new(),
                    redirect_uri: String::new(),
                }
                .default_expires_at_ms(),
            )
        }
        LoginProviderTarget::OpenAi => {
            let label = auth::codex::login_target_label(account_label)?;
            let (verifier, challenge) = auth::oauth::generate_pkce_public();
            let state = auth::oauth::generate_state_public();
            let redirect_uri = auth::oauth::openai::default_redirect_uri();
            let auth_url = auth::oauth::openai_auth_url_with_prompt(
                &redirect_uri,
                &challenge,
                &state,
                Some("login"),
            );
            (
                PendingScriptableLogin::Openai {
                    account_label: label,
                    verifier,
                    state,
                    redirect_uri,
                },
                auth_url,
                "callback_url",
                None,
                PendingScriptableLogin::Openai {
                    account_label: String::new(),
                    verifier: String::new(),
                    state: String::new(),
                    redirect_uri: String::new(),
                }
                .default_expires_at_ms(),
            )
        }
        LoginProviderTarget::Gemini => {
            let (verifier, challenge) = auth::oauth::generate_pkce_public();
            let state = auth::oauth::generate_state_public();
            let redirect_uri = auth::gemini::GEMINI_MANUAL_REDIRECT_URI.to_string();
            let auth_url = auth::gemini::build_manual_auth_url(&redirect_uri, &challenge, &state)?;
            (
                PendingScriptableLogin::Gemini {
                    verifier,
                    redirect_uri,
                },
                auth_url,
                "auth_code",
                None,
                PendingScriptableLogin::Gemini {
                    verifier: String::new(),
                    redirect_uri: String::new(),
                }
                .default_expires_at_ms(),
            )
        }
        LoginProviderTarget::Antigravity => {
            let (verifier, challenge) = auth::oauth::generate_pkce_public();
            let state = auth::oauth::generate_state_public();
            let redirect_uri = auth::antigravity::redirect_uri(auth::antigravity::DEFAULT_PORT);
            let auth_url = auth::antigravity::build_auth_url(&redirect_uri, &challenge, &state)?;
            (
                PendingScriptableLogin::Antigravity {
                    verifier,
                    state,
                    redirect_uri,
                },
                auth_url,
                "callback_url",
                None,
                PendingScriptableLogin::Antigravity {
                    verifier: String::new(),
                    state: String::new(),
                    redirect_uri: String::new(),
                }
                .default_expires_at_ms(),
            )
        }
        LoginProviderTarget::Google => {
            let creds = auth::google::load_credentials().context(
                "Google/Gmail scriptable auth requires saved OAuth credentials first. Run `jcode login --provider google` once or save google credentials manually.",
            )?;
            let tier = options
                .google_access_tier
                .unwrap_or(auth::google::GmailAccessTier::Full);
            let (verifier, challenge) = auth::oauth::generate_pkce_public();
            let state = auth::oauth::generate_state_public();
            let redirect_uri = format!("http://127.0.0.1:{}", auth::google::DEFAULT_PORT);
            let auth_url =
                auth::google::build_auth_url(&creds, tier, &redirect_uri, &challenge, &state);
            (
                PendingScriptableLogin::Google {
                    verifier,
                    state,
                    redirect_uri,
                    tier,
                },
                auth_url,
                "callback_url",
                None,
                PendingScriptableLogin::Google {
                    verifier: String::new(),
                    state: String::new(),
                    redirect_uri: String::new(),
                    tier,
                }
                .default_expires_at_ms(),
            )
        }
        LoginProviderTarget::Copilot => {
            let client = reqwest::Client::new();
            let device_resp = auth::copilot::initiate_device_flow(&client).await?;
            (
                PendingScriptableLogin::Copilot {
                    device_code: device_resp.device_code.clone(),
                    user_code: device_resp.user_code.clone(),
                    verification_uri: device_resp.verification_uri.clone(),
                    interval: device_resp.interval,
                },
                device_resp.verification_uri,
                "complete",
                Some(device_resp.user_code),
                current_time_ms() + (device_resp.expires_in as i64 * 1000),
            )
        }
        _ => {
            anyhow::bail!(
                "`--print-auth-url` is currently supported for: claude, openai, gemini, antigravity, google, copilot."
            )
        }
    };

    let pending_path = pending.pending_path()?;
    cleanup_stale_pending_login_files()?;
    let record = PendingScriptableLoginRecord {
        expires_at_ms,
        login: pending,
    };
    crate::storage::write_json_secret(&pending_path, &record)?;
    emit_scriptable_auth_prompt(
        provider.id,
        &auth_url,
        input_kind,
        &pending_path,
        user_code.as_deref(),
        expires_at_ms,
        options.json,
    )?;
    Ok(LoginFlowOutcome::Deferred)
}

async fn complete_scriptable_login(
    provider: LoginProviderDescriptor,
    account_label: Option<&str>,
    options: &LoginOptions,
    input: Option<ProvidedAuthInput>,
) -> Result<LoginFlowOutcome> {
    if account_label.is_some() {
        anyhow::bail!(
            "Do not pass --account when completing a scriptable login. The pending login already stores the target account."
        );
    }

    match provider.target {
        LoginProviderTarget::Claude => {
            complete_scriptable_claude_login(provider.id, options, require_scriptable_input(input)?)
                .await
        }
        LoginProviderTarget::OpenAi => {
            complete_scriptable_openai_login(provider.id, options, require_scriptable_input(input)?)
                .await
        }
        LoginProviderTarget::Gemini => {
            complete_scriptable_gemini_login(provider.id, options, require_scriptable_input(input)?)
                .await
        }
        LoginProviderTarget::Antigravity => {
            complete_scriptable_antigravity_login(
                provider.id,
                options,
                require_scriptable_input(input)?,
            )
            .await
        }
        LoginProviderTarget::Google => {
            complete_scriptable_google_login(provider.id, options, require_scriptable_input(input)?)
                .await
        }
        LoginProviderTarget::Copilot => {
            if input.is_some() {
                anyhow::bail!(
                    "Copilot completion uses `--complete` and does not accept --callback-url or --auth-code."
                )
            }
            if !options.complete {
                anyhow::bail!("Copilot completion requires `--complete`.")
            }
            complete_scriptable_copilot_login(provider.id, options).await
        }
        _ => anyhow::bail!(
            "Scriptable completion is currently supported for: claude, openai, gemini, antigravity, google, copilot."
        ),
    }
}

async fn complete_scriptable_claude_login(
    provider_id: &str,
    options: &LoginOptions,
    input: ProvidedAuthInput,
) -> Result<LoginFlowOutcome> {
    let pending_path = pending_login_path("claude")?;
    let PendingScriptableLogin::Claude {
        account_label,
        verifier,
        redirect_uri,
    } = load_pending_login(&pending_path, "claude")?
    else {
        anyhow::bail!("Pending Claude login state is invalid.");
    };

    let raw_input = match input {
        ProvidedAuthInput::CallbackUrl(value) | ProvidedAuthInput::AuthCode(value) => value,
    };
    let selected_redirect_uri =
        auth::oauth::claude_redirect_uri_for_input(&raw_input, &redirect_uri);
    let tokens =
        auth::oauth::exchange_claude_code(&verifier, &raw_input, &selected_redirect_uri).await?;
    auth::oauth::save_claude_tokens_for_account(&tokens, &account_label)?;
    let profile_email =
        auth::oauth::update_claude_account_profile(&account_label, &tokens.access_token)
            .await
            .unwrap_or(None);
    clear_pending_login(&pending_path);
    crate::telemetry::record_auth_success(provider_id, "oauth");
    emit_scriptable_auth_success(
        options.json,
        ScriptableAuthSuccess {
            status: "authenticated",
            provider: provider_id.to_string(),
            account_label: Some(account_label.clone()),
            credentials_path: Some(auth::claude::jcode_path()?.display().to_string()),
            email: profile_email.clone(),
        },
    )?;
    if !options.json {
        eprintln!("Successfully logged in to Claude!");
        eprintln!(
            "Account '{}' stored at {}",
            account_label,
            auth::claude::jcode_path()?.display()
        );
        if let Some(email) = profile_email {
            eprintln!("Profile email: {}", email);
        }
    }
    Ok(LoginFlowOutcome::Completed)
}

async fn complete_scriptable_openai_login(
    provider_id: &str,
    options: &LoginOptions,
    input: ProvidedAuthInput,
) -> Result<LoginFlowOutcome> {
    let pending_path = pending_login_path("openai")?;
    let PendingScriptableLogin::Openai {
        account_label,
        verifier,
        state,
        redirect_uri,
    } = load_pending_login(&pending_path, "openai")?
    else {
        anyhow::bail!("Pending OpenAI login state is invalid.");
    };

    let callback_input = match input {
        ProvidedAuthInput::CallbackUrl(value) => value,
        ProvidedAuthInput::AuthCode(_) => {
            anyhow::bail!(
                "OpenAI completion requires --callback-url because state validation is required."
            )
        }
    };
    let tokens = auth::oauth::exchange_openai_callback_input(
        &verifier,
        &callback_input,
        &state,
        &redirect_uri,
    )
    .await?;
    auth::oauth::save_openai_tokens_for_account(&tokens, &account_label)?;
    clear_pending_login(&pending_path);
    crate::telemetry::record_auth_success(provider_id, "oauth");
    let credentials_path = crate::storage::jcode_dir()?.join("openai-auth.json");
    emit_scriptable_auth_success(
        options.json,
        ScriptableAuthSuccess {
            status: "authenticated",
            provider: provider_id.to_string(),
            account_label: Some(account_label.clone()),
            credentials_path: Some(credentials_path.display().to_string()),
            email: None,
        },
    )?;
    if !options.json {
        eprintln!(
            "Successfully logged in to OpenAI! Account '{}' saved to {}",
            account_label,
            credentials_path.display()
        );
    }
    Ok(LoginFlowOutcome::Completed)
}

async fn complete_scriptable_gemini_login(
    provider_id: &str,
    options: &LoginOptions,
    input: ProvidedAuthInput,
) -> Result<LoginFlowOutcome> {
    let pending_path = pending_login_path("gemini")?;
    let PendingScriptableLogin::Gemini {
        verifier,
        redirect_uri,
    } = load_pending_login(&pending_path, "gemini")?
    else {
        anyhow::bail!("Pending Gemini login state is invalid.");
    };

    let auth_code = match input {
        ProvidedAuthInput::AuthCode(value) => value,
        ProvidedAuthInput::CallbackUrl(_) => {
            anyhow::bail!("Gemini completion requires --auth-code.")
        }
    };
    let tokens = auth::gemini::exchange_callback_code(&auth_code, &verifier, &redirect_uri).await?;
    clear_pending_login(&pending_path);
    crate::telemetry::record_auth_success(provider_id, "oauth");
    emit_scriptable_auth_success(
        options.json,
        ScriptableAuthSuccess {
            status: "authenticated",
            provider: provider_id.to_string(),
            account_label: None,
            credentials_path: Some(auth::gemini::tokens_path()?.display().to_string()),
            email: tokens.email.clone(),
        },
    )?;
    if !options.json {
        eprintln!("Successfully logged in to Gemini!");
        eprintln!("Tokens saved to {}", auth::gemini::tokens_path()?.display());
        if let Some(email) = tokens.email.as_deref() {
            eprintln!("Google account: {}", email);
        }
    }
    Ok(LoginFlowOutcome::Completed)
}

async fn complete_scriptable_antigravity_login(
    provider_id: &str,
    options: &LoginOptions,
    input: ProvidedAuthInput,
) -> Result<LoginFlowOutcome> {
    let pending_path = pending_login_path("antigravity")?;
    let PendingScriptableLogin::Antigravity {
        verifier,
        state,
        redirect_uri,
    } = load_pending_login(&pending_path, "antigravity")?
    else {
        anyhow::bail!("Pending Antigravity login state is invalid.");
    };

    let callback_input = match input {
        ProvidedAuthInput::CallbackUrl(value) => value,
        ProvidedAuthInput::AuthCode(_) => {
            anyhow::bail!("Antigravity completion requires --callback-url.")
        }
    };
    let tokens = auth::antigravity::exchange_callback_input(
        &verifier,
        &callback_input,
        Some(&state),
        &redirect_uri,
    )
    .await?;
    clear_pending_login(&pending_path);
    crate::telemetry::record_auth_success(provider_id, "oauth");
    emit_scriptable_auth_success(
        options.json,
        ScriptableAuthSuccess {
            status: "authenticated",
            provider: provider_id.to_string(),
            account_label: None,
            credentials_path: Some(auth::antigravity::tokens_path()?.display().to_string()),
            email: tokens.email.clone(),
        },
    )?;
    if !options.json {
        eprintln!("Successfully logged in to Antigravity!");
        eprintln!(
            "Tokens saved to {}",
            auth::antigravity::tokens_path()?.display()
        );
        if let Some(email) = tokens.email.as_deref() {
            eprintln!("Google account: {}", email);
        }
        if let Some(project_id) = tokens.project_id.as_deref() {
            eprintln!("Resolved Antigravity project: {}", project_id);
        }
    }
    Ok(LoginFlowOutcome::Completed)
}

async fn complete_scriptable_google_login(
    provider_id: &str,
    options: &LoginOptions,
    input: ProvidedAuthInput,
) -> Result<LoginFlowOutcome> {
    let pending_path = pending_login_path("google")?;
    let PendingScriptableLogin::Google {
        verifier,
        state,
        redirect_uri,
        tier,
    } = load_pending_login(&pending_path, "google")?
    else {
        anyhow::bail!("Pending Google login state is invalid.");
    };

    let callback_input = match input {
        ProvidedAuthInput::CallbackUrl(value) => value,
        ProvidedAuthInput::AuthCode(_) => {
            anyhow::bail!("Google completion requires --callback-url.")
        }
    };
    let creds = auth::google::load_credentials().context(
        "Google/Gmail completion requires saved OAuth credentials first. Run `jcode login --provider google` once or save google credentials manually.",
    )?;
    let tokens = auth::google::exchange_callback_input(
        &creds,
        &verifier,
        &callback_input,
        &state,
        &redirect_uri,
        tier,
    )
    .await?;
    clear_pending_login(&pending_path);
    crate::telemetry::record_auth_success(provider_id, "oauth");
    emit_scriptable_auth_success(
        options.json,
        ScriptableAuthSuccess {
            status: "authenticated",
            provider: provider_id.to_string(),
            account_label: None,
            credentials_path: Some(auth::google::tokens_path()?.display().to_string()),
            email: tokens.email.clone(),
        },
    )?;
    if !options.json {
        eprintln!("Successfully logged in to Google/Gmail!");
        if let Some(email) = tokens.email.as_deref() {
            eprintln!("Account: {}", email);
        }
        eprintln!("Access tier: {}", tokens.tier.label());
        eprintln!("Tokens saved to {}", auth::google::tokens_path()?.display());
    }
    Ok(LoginFlowOutcome::Completed)
}

async fn complete_scriptable_copilot_login(
    provider_id: &str,
    options: &LoginOptions,
) -> Result<LoginFlowOutcome> {
    let pending_path = pending_login_path("copilot")?;
    let PendingScriptableLogin::Copilot {
        device_code,
        interval,
        ..
    } = load_pending_login(&pending_path, "copilot")?
    else {
        anyhow::bail!("Pending Copilot login state is invalid.");
    };

    let client = reqwest::Client::new();
    let token = auth::copilot::poll_for_access_token(&client, &device_code, interval).await?;
    let username = auth::copilot::fetch_github_username(&client, &token)
        .await
        .unwrap_or_else(|_| "unknown".to_string());
    auth::copilot::save_github_token(&token, &username)?;
    clear_pending_login(&pending_path);
    crate::telemetry::record_auth_success(provider_id, "oauth_device_code");
    emit_scriptable_auth_success(
        options.json,
        ScriptableAuthSuccess {
            status: "authenticated",
            provider: provider_id.to_string(),
            account_label: Some(username.clone()),
            credentials_path: Some(auth::copilot::saved_hosts_path().display().to_string()),
            email: None,
        },
    )?;
    if !options.json {
        eprintln!("✓ Authenticated as {} via GitHub Copilot", username);
        eprintln!("Saved at {}", auth::copilot::saved_hosts_path().display());
    }
    Ok(LoginFlowOutcome::Completed)
}

fn pending_login_path(key: &str) -> Result<PathBuf> {
    Ok(crate::storage::jcode_dir()?
        .join("pending-login")
        .join(format!("{key}.json")))
}

fn pending_login_dir() -> Result<PathBuf> {
    Ok(crate::storage::jcode_dir()?.join("pending-login"))
}

fn require_scriptable_input(input: Option<ProvidedAuthInput>) -> Result<ProvidedAuthInput> {
    input.ok_or_else(|| anyhow::anyhow!("No scriptable auth input was provided."))
}

fn load_pending_login(path: &PathBuf, provider: &str) -> Result<PendingScriptableLogin> {
    if !path.exists() {
        anyhow::bail!(
            "No pending {} login state found. Run `jcode login --provider {} --print-auth-url` first.",
            provider,
            provider
        );
    }
    cleanup_stale_pending_login_files()?;
    crate::storage::harden_secret_file_permissions(path);
    let data = std::fs::read_to_string(path).with_context(|| {
        format!(
            "Failed to read pending {} login state from {}",
            provider,
            path.display()
        )
    })?;
    if let Ok(record) = serde_json::from_str::<PendingScriptableLoginRecord>(&data) {
        if record.expires_at_ms <= current_time_ms() {
            clear_pending_login(path);
            anyhow::bail!(
                "Pending {} login state expired. Run `jcode login --provider {} --print-auth-url` again.",
                provider,
                provider
            );
        }
        return Ok(record.login);
    }
    serde_json::from_str::<PendingScriptableLogin>(&data).with_context(|| {
        format!(
            "Failed to load pending {} login state from {}",
            provider,
            path.display()
        )
    })
}

fn clear_pending_login(path: &PathBuf) {
    let _ = std::fs::remove_file(path);
}

fn cleanup_stale_pending_login_files() -> Result<()> {
    let dir = pending_login_dir()?;
    if !dir.exists() {
        return Ok(());
    }
    for entry in std::fs::read_dir(&dir)? {
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => continue,
        };
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let Ok(data) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(record) = serde_json::from_str::<PendingScriptableLoginRecord>(&data) else {
            continue;
        };
        if record.expires_at_ms <= current_time_ms() {
            let _ = std::fs::remove_file(&path);
        }
    }
    Ok(())
}

fn current_time_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn resolve_auth_input(value: &str) -> Result<String> {
    if value != "-" {
        return Ok(value.to_string());
    }

    let mut input = String::new();
    std::io::stdin()
        .read_line(&mut input)
        .context("Failed to read auth input from stdin")?;
    let trimmed = input.trim();
    if trimmed.is_empty() {
        anyhow::bail!("No auth input was provided on stdin.");
    }
    Ok(trimmed.to_string())
}

fn emit_scriptable_auth_prompt(
    provider: &str,
    auth_url: &str,
    input_kind: &str,
    pending_path: &Path,
    user_code: Option<&str>,
    expires_at_ms: i64,
    json: bool,
) -> Result<()> {
    let resume_command = scriptable_resume_command(provider, input_kind);
    let prompt = ScriptableAuthPrompt {
        status: "pending",
        provider: provider.to_string(),
        auth_url: auth_url.to_string(),
        input_kind: input_kind.to_string(),
        pending_path: pending_path.display().to_string(),
        user_code: user_code.map(str::to_string),
        expires_at_ms,
        resume_command: resume_command.clone(),
    };
    if json {
        println!("{}", serde_json::to_string(&prompt)?);
    } else {
        println!("{}", auth_url);
        if let Some(user_code) = user_code {
            eprintln!("User code: {}", user_code);
        }
        eprintln!("Auth URL printed to stdout.");
        eprintln!("Complete this login later with `{}`.", resume_command);
        eprintln!(
            "This pending login expires at {} ms since epoch.",
            expires_at_ms
        );
        eprintln!("Pending login state saved at {}", pending_path.display());
    }
    Ok(())
}

fn scriptable_resume_command(provider: &str, input_kind: &str) -> String {
    match input_kind {
        "callback_url" => {
            format!(
                "jcode login --provider {} --callback-url '<url-or-query>'",
                provider
            )
        }
        "auth_code" => format!("jcode login --provider {} --auth-code '<code>'", provider),
        "complete" => format!("jcode login --provider {} --complete", provider),
        _ => format!(
            "jcode login --provider {} --callback-url '<url>'  # or --auth-code '<code>'",
            provider
        ),
    }
}

fn emit_scriptable_auth_success(json: bool, success: ScriptableAuthSuccess) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string(&success)?);
    }
    Ok(())
}

fn login_jcode_flow() -> Result<()> {
    eprintln!("Setting up Jcode subscription access...");
    eprintln!(
        "Paste the jcode subscription API key from your account portal. This key is used for your curated jcode router access.\n"
    );
    eprint!("Paste your Jcode API key: ");
    io::stdout().flush()?;

    let key = read_secret_line()?;
    if key.is_empty() {
        anyhow::bail!("No API key provided.");
    }

    eprint!("Optional router base URL (press Enter to use the default placeholder): ");
    io::stdout().flush()?;
    let api_base = read_secret_line()?;

    let mut content = format!(
        "{}={}\n",
        crate::subscription_catalog::JCODE_API_KEY_ENV,
        key
    );
    if !api_base.trim().is_empty() {
        content.push_str(&format!(
            "{}={}\n",
            crate::subscription_catalog::JCODE_API_BASE_ENV,
            api_base.trim()
        ));
    }

    let config_dir = crate::storage::app_config_dir()?;
    let file_path = config_dir.join(crate::subscription_catalog::JCODE_ENV_FILE);
    crate::storage::write_text_secret(&file_path, &content)?;

    crate::env::set_var(crate::subscription_catalog::JCODE_API_KEY_ENV, key);
    if !api_base.trim().is_empty() {
        crate::env::set_var(
            crate::subscription_catalog::JCODE_API_BASE_ENV,
            api_base.trim(),
        );
    }

    eprintln!("\nSuccessfully saved Jcode subscription credentials!");
    eprintln!(
        "Stored at ~/.config/jcode/{}",
        crate::subscription_catalog::JCODE_ENV_FILE
    );
    eprintln!(
        "Curated models available now: {}",
        crate::subscription_catalog::curated_models()
            .iter()
            .map(|model| model.display_name)
            .collect::<Vec<_>>()
            .join(", ")
    );
    crate::telemetry::record_auth_success("jcode", "api_key");
    Ok(())
}

async fn login_claude_flow(requested_label: Option<&str>, no_browser: bool) -> Result<()> {
    let label = auth::claude::login_target_label(requested_label)?;
    eprintln!("Logging in to Claude (account: {})...", label);
    let tokens = auth::oauth::login_claude(no_browser).await?;
    auth::oauth::save_claude_tokens_for_account(&tokens, &label)?;
    let profile_email =
        match auth::oauth::update_claude_account_profile(&label, &tokens.access_token).await {
            Ok(email) => email,
            Err(e) => {
                eprintln!(
                    "Warning: logged in but failed to fetch profile metadata: {}",
                    e
                );
                None
            }
        };
    eprintln!("Successfully logged in to Claude!");
    eprintln!(
        "Account '{}' stored at {}",
        label,
        auth::claude::jcode_path()?.display()
    );
    if let Some(email) = profile_email {
        eprintln!("Profile email: {}", email);
    }
    crate::telemetry::record_auth_success("claude", "oauth");
    Ok(())
}

async fn login_openai_flow(requested_label: Option<&str>, no_browser: bool) -> Result<()> {
    let label = auth::codex::login_target_label(requested_label)?;
    eprintln!("Logging in to OpenAI/Codex (account: {})...", label);
    let tokens = auth::oauth::login_openai(no_browser).await?;
    auth::oauth::save_openai_tokens_for_account(&tokens, &label)?;
    eprintln!(
        "Successfully logged in to OpenAI! Account '{}' saved to {}",
        label,
        crate::storage::jcode_dir()?
            .join("openai-auth.json")
            .display()
    );
    crate::telemetry::record_auth_success("openai", "oauth");
    Ok(())
}

fn login_openrouter_flow() -> Result<()> {
    eprintln!("Setting up OpenRouter...");
    eprintln!("Get your API key from: https://openrouter.ai/keys\n");
    eprint!("Paste your OpenRouter API key: ");
    io::stdout().flush()?;

    let key = read_secret_line()?;

    if key.is_empty() {
        anyhow::bail!("No API key provided.");
    }

    if !key.starts_with("sk-or-") {
        eprintln!("Warning: OpenRouter API keys typically start with 'sk-or-'. Saving anyway.");
    }

    save_named_api_key("openrouter.env", "OPENROUTER_API_KEY", &key)?;
    eprintln!("\nSuccessfully saved OpenRouter API key!");
    eprintln!("Stored at ~/.config/jcode/openrouter.env");
    crate::telemetry::record_auth_success("openrouter", "api_key");
    Ok(())
}

fn login_azure_flow() -> Result<()> {
    use crate::auth::azure;

    eprintln!("Setting up Azure OpenAI...");
    eprintln!(
        "Reference: OpenCode supports Azure OpenAI with Entra credentials. jcode uses Azure OpenAI's newer `/openai/v1` API with either Microsoft Entra ID or an API key.\n"
    );

    let endpoint_raw = read_line_trimmed(
        "Azure OpenAI endpoint (for example `https://your-resource.openai.azure.com`): ",
    )?;
    let endpoint = azure::normalize_endpoint(&endpoint_raw).ok_or_else(|| {
        anyhow::anyhow!(
            "Invalid Azure OpenAI endpoint. Use https://<resource>.openai.azure.com (or the full /openai/v1 URL)."
        )
    })?;

    let model =
        read_line_trimmed("Azure deployment/model name (required, for example `gpt-4.1-nano`): ")?;
    if model.is_empty() {
        anyhow::bail!("No deployment/model name provided.");
    }

    eprintln!("\nAuthentication method:");
    eprintln!("  1. Microsoft Entra ID (recommended)");
    eprintln!("  2. API key");
    let auth_choice = read_line_trimmed("Enter 1-2 [1]: ")?;
    let use_entra = match auth_choice.trim() {
        "" | "1" => true,
        "2" => false,
        other if other.eq_ignore_ascii_case("entra") || other.eq_ignore_ascii_case("oauth") => true,
        other if other.eq_ignore_ascii_case("key") || other.eq_ignore_ascii_case("api-key") => {
            false
        }
        other => anyhow::bail!("Invalid auth choice '{}'. Use 1 or 2.", other),
    };

    let mut assignments = vec![
        (azure::ENDPOINT_ENV, endpoint),
        (azure::MODEL_ENV, model),
        (
            azure::USE_ENTRA_ENV,
            if use_entra { "1" } else { "0" }.to_string(),
        ),
    ];

    if use_entra {
        eprintln!();
        eprintln!("Using Microsoft Entra ID via Azure's DefaultAzureCredential chain.");
        eprintln!(
            "That means jcode can authenticate via `az login`, managed identity, or Azure environment credentials."
        );
    } else {
        eprint!("Paste your Azure OpenAI API key: ");
        io::stdout().flush()?;
        let key = read_secret_line()?;
        if key.is_empty() {
            anyhow::bail!("No API key provided.");
        }
        assignments.push((azure::API_KEY_ENV, key));
    }

    save_named_env_vars(azure::ENV_FILE, &assignments)?;
    azure::apply_runtime_env()?;

    eprintln!("\nSuccessfully saved Azure OpenAI configuration!");
    eprintln!("Stored at ~/.config/jcode/{}", azure::ENV_FILE);
    eprintln!("Base URL: {}", azure::load_endpoint().unwrap_or_default());
    if let Some(model) = azure::load_model() {
        eprintln!("Default deployment/model: {}", model);
    }
    if use_entra {
        eprintln!(
            "Next step: if you're using Azure CLI auth, run `az login` (and ensure your identity has the Cognitive Services OpenAI User role)."
        );
    }
    crate::telemetry::record_auth_success("azure", if use_entra { "entra_id" } else { "api_key" });
    Ok(())
}

fn login_openai_compatible_flow(profile: &OpenAiCompatibleProfile) -> Result<()> {
    let is_custom_profile = profile.id == crate::provider_catalog::OPENAI_COMPAT_PROFILE.id;
    let mut resolved = resolve_openai_compatible_profile(*profile);

    eprintln!("Setting up {}...", resolved.display_name);
    eprintln!("See setup details: {}\n", resolved.setup_url);

    if is_custom_profile {
        eprintln!(
            "You can point this at a hosted OpenAI-compatible API or a local server such as LM Studio or Ollama."
        );
        let api_base_input = read_line_trimmed(&format!("API base URL [{}]: ", resolved.api_base))?;
        if !api_base_input.is_empty() {
            let normalized = crate::provider_catalog::normalize_api_base(&api_base_input)
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "Invalid OpenAI-compatible API base. Use https://... or http://localhost..."
                    )
                })?;
            crate::provider_catalog::save_env_value_to_env_file(
                "JCODE_OPENAI_COMPAT_API_BASE",
                crate::provider_catalog::OPENAI_COMPAT_PROFILE.env_file,
                Some(&normalized),
            )?;
            resolved = resolve_openai_compatible_profile(*profile);
        }
        eprintln!();
    }

    eprintln!("Endpoint: {}", resolved.api_base);
    let auth_method = if resolved.requires_api_key {
        eprintln!("API key env variable: {}\n", resolved.api_key_env);
        eprint!("Paste your {} API key: ", resolved.display_name);
        io::stdout().flush()?;

        let key = read_secret_line()?;
        if key.is_empty() {
            anyhow::bail!("No API key provided.");
        }

        crate::provider_catalog::save_env_value_to_env_file(
            OPENAI_COMPAT_LOCAL_ENABLED_ENV,
            &resolved.env_file,
            None,
        )?;
        save_named_api_key(&resolved.env_file, &resolved.api_key_env, &key)?;
        eprintln!("\nSuccessfully saved {} API key!", resolved.display_name);
        "api_key"
    } else {
        eprintln!("This provider uses a local OpenAI-compatible endpoint.");
        eprintln!(
            "An API key is optional here. Press Enter to skip if your local server does not require one.\n"
        );
        eprint!("Optional {} API key: ", resolved.display_name);
        io::stdout().flush()?;

        let key = read_secret_line()?;
        crate::provider_catalog::save_env_value_to_env_file(
            OPENAI_COMPAT_LOCAL_ENABLED_ENV,
            &resolved.env_file,
            Some("1"),
        )?;
        if key.trim().is_empty() {
            crate::provider_catalog::save_env_value_to_env_file(
                &resolved.api_key_env,
                &resolved.env_file,
                None,
            )?;
            eprintln!("\nSaved {} local endpoint setup.", resolved.display_name);
            "local_endpoint"
        } else {
            crate::provider_catalog::save_env_value_to_env_file(
                &resolved.api_key_env,
                &resolved.env_file,
                Some(key.trim()),
            )?;
            eprintln!(
                "\nSaved {} local endpoint setup and optional API key.",
                resolved.display_name
            );
            "local_endpoint_with_optional_api_key"
        }
    };

    eprintln!("Stored at ~/.config/jcode/{}", resolved.env_file);
    if let Some(default_model) = resolved.default_model {
        eprintln!("Default model hint: {}", default_model);
    }
    crate::telemetry::record_auth_success(&resolved.id, auth_method);
    Ok(())
}

pub fn read_secret_line() -> Result<String> {
    use crossterm::terminal;

    let was_raw = crossterm::terminal::is_raw_mode_enabled().unwrap_or(false);
    if !was_raw {
        terminal::enable_raw_mode()?;
    }

    let mut input = String::new();
    loop {
        if let crossterm::event::Event::Key(key_event) =
            crossterm::event::read().context("Failed to read key input")?
        {
            use crossterm::event::{KeyCode, KeyModifiers};
            match key_event.code {
                KeyCode::Enter => {
                    eprintln!();
                    break;
                }
                KeyCode::Char('c') if key_event.modifiers.contains(KeyModifiers::CONTROL) => {
                    if !was_raw {
                        terminal::disable_raw_mode()?;
                    }
                    anyhow::bail!("Cancelled.");
                }
                KeyCode::Backspace => {
                    input.pop();
                }
                KeyCode::Char(c) => {
                    input.push(c);
                }
                _ => {}
            }
        }
    }

    if !was_raw {
        terminal::disable_raw_mode()?;
    }

    Ok(input.trim().to_string())
}

fn read_line_trimmed(prompt: &str) -> Result<String> {
    print!("{}", prompt);
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input.trim().to_string())
}

fn save_named_env_vars(env_file: &str, vars: &[(&str, String)]) -> Result<()> {
    if !crate::provider_catalog::is_safe_env_file_name(env_file) {
        anyhow::bail!("Invalid env file name: {}", env_file);
    }

    for (key, _) in vars {
        if !crate::provider_catalog::is_safe_env_key_name(key) {
            anyhow::bail!("Invalid API key variable name: {}", key);
        }
    }

    let config_dir = crate::storage::app_config_dir()?;
    std::fs::create_dir_all(&config_dir)?;
    crate::platform::set_directory_permissions_owner_only(&config_dir)?;

    let file_path = config_dir.join(env_file);
    let mut content = String::new();
    for (key, value) in vars {
        content.push_str(&format!("{}={}\n", key, value));
    }
    std::fs::write(&file_path, &content)?;
    crate::platform::set_permissions_owner_only(&file_path)?;

    for (key, value) in vars {
        crate::env::set_var(key, value);
    }

    Ok(())
}

fn login_cursor_flow() -> Result<()> {
    eprintln!("Starting Cursor login...");
    let binary = crate::auth::cursor::cursor_agent_cli_path();

    if crate::auth::cursor::has_cursor_agent_cli() {
        match crate::auth::login_flows::run_external_login_command(&binary, &["login"]) {
            Ok(()) => {
                eprintln!("Cursor login command completed.");
                crate::telemetry::record_auth_success("cursor", "oauth");
                crate::auth::AuthStatus::invalidate_cache();
                return Ok(());
            }
            Err(err) => {
                eprintln!("Cursor browser login failed: {}", err);
                eprintln!();
                eprintln!("Falling back to Cursor API key setup.");
            }
        }
    } else {
        eprintln!("Cursor Agent CLI was not found on PATH.");
        eprintln!("You can still save a Cursor API key now and install Cursor Agent later.");
    }

    eprintln!("Get your API key from: https://cursor.com/settings");
    eprintln!("(Dashboard > Integrations > User API Keys)\n");
    eprint!("Paste your Cursor API key: ");
    io::stdout().flush()?;

    let key = read_secret_line()?;
    if key.is_empty() {
        anyhow::bail!("No API key provided.");
    }

    save_named_api_key("cursor.env", "CURSOR_API_KEY", &key)?;
    crate::auth::AuthStatus::invalidate_cache();
    eprintln!("\nSuccessfully saved Cursor API key!");
    eprintln!("Stored at ~/.config/jcode/cursor.env");
    eprintln!("jcode will pass it to `cursor-agent` automatically.");
    if !crate::auth::cursor::has_cursor_agent_cli() {
        eprintln!("Install Cursor Agent to use the Cursor provider:");
        eprintln!("  - macOS/Linux/WSL: curl https://cursor.com/install -fsS | bash");
        eprintln!("  - Windows (PowerShell): irm 'https://cursor.com/install?win32=true' | iex");
    }
    crate::telemetry::record_auth_success("cursor", "api_key");
    Ok(())
}

fn login_copilot_flow(no_browser: bool) -> Result<()> {
    eprintln!("Starting GitHub Copilot login...");

    tokio::task::block_in_place(|| {
        tokio::runtime::Handle::current().block_on(login_copilot_device_flow(no_browser))
    })
}

async fn login_copilot_device_flow(no_browser: bool) -> Result<()> {
    let client = reqwest::Client::new();

    let device_resp = crate::auth::copilot::initiate_device_flow(&client).await?;

    eprintln!();
    eprintln!("  Open this URL in your browser:");
    eprintln!("    {}", device_resp.verification_uri);
    eprintln!();
    if let Some(qr) = crate::login_qr::indented_section(
        &device_resp.verification_uri,
        "  Or scan this QR on another device to open the verification page:",
        "    ",
    ) {
        eprintln!("{qr}");
        eprintln!();
    }
    eprintln!("  Enter code: {}", device_resp.user_code);
    eprintln!();
    eprintln!("  Waiting for authorization...");

    maybe_open_browser(&device_resp.verification_uri, no_browser);

    let token = crate::auth::copilot::poll_for_access_token(
        &client,
        &device_resp.device_code,
        device_resp.interval,
    )
    .await?;

    let username = crate::auth::copilot::fetch_github_username(&client, &token)
        .await
        .unwrap_or_else(|_| "unknown".to_string());

    crate::auth::copilot::save_github_token(&token, &username)?;

    eprintln!("  ✓ Authenticated as {} via GitHub Copilot", username);
    crate::telemetry::record_auth_success("copilot", "oauth_device_code");
    Ok(())
}

async fn login_antigravity_flow(no_browser: bool) -> Result<()> {
    eprintln!("Starting native Antigravity login...");
    eprintln!(
        "jcode will authenticate directly with Google Antigravity; the Antigravity desktop app is not required."
    );
    eprintln!(
        "If browser launch fails, or you pass `--no-browser`, jcode will prompt for the callback URL instead."
    );
    eprintln!(
        "If the browser later shows a loopback/callback error page, copy the full URL from the address bar and re-run with `--no-browser`."
    );
    eprintln!();

    let tokens = crate::auth::antigravity::login(no_browser).await?;

    eprintln!("Successfully logged in to Antigravity!");
    eprintln!(
        "Tokens saved to {}",
        crate::auth::antigravity::tokens_path()?.display()
    );
    if let Some(email) = tokens.email.as_deref() {
        eprintln!("Google account: {}", email);
    }
    if let Some(project_id) = tokens.project_id.as_deref() {
        eprintln!("Resolved Antigravity project: {}", project_id);
    }
    crate::telemetry::record_auth_success("antigravity", "oauth");
    Ok(())
}

async fn login_gemini_flow(no_browser: bool) -> Result<()> {
    eprintln!("Starting native Gemini login...");
    eprintln!(
        "If your student/education plan is attached to your Google account, use that account in the browser flow."
    );
    eprintln!(
        "If browser launch fails, or you pass `--no-browser`, jcode will prompt for the manual authorization code."
    );
    eprintln!(
        "Note: school / Workspace Google accounts may also require GOOGLE_CLOUD_PROJECT and GOOGLE_CLOUD_LOCATION for Code Assist entitlement checks."
    );
    eprintln!();

    let tokens = crate::auth::gemini::login(no_browser).await?;

    eprintln!("Successfully logged in to Gemini!");
    eprintln!(
        "Tokens saved to {}",
        crate::auth::gemini::tokens_path()?.display()
    );
    if let Some(email) = tokens.email.as_deref() {
        eprintln!("Google account: {}", email);
    }
    crate::telemetry::record_auth_success("gemini", "oauth");
    Ok(())
}

async fn login_google_flow(no_browser: bool) -> Result<()> {
    use auth::google::{GmailAccessTier, GoogleCredentials};

    eprintln!("╔══════════════════════════════════════════╗");
    eprintln!("║       Gmail Integration Setup            ║");
    eprintln!("╚══════════════════════════════════════════╝\n");

    let _creds = match auth::google::load_credentials() {
        Ok(creds) => {
            eprintln!(
                "✓ Google credentials found (client_id: {}...)\n",
                &creds.client_id[..20.min(creds.client_id.len())]
            );
            creds
        }
        Err(_) => {
            eprintln!("No Google credentials found. Let's set them up.\n");
            eprintln!("You need OAuth credentials from Google Cloud Console.");
            eprintln!("How would you like to provide them?\n");
            eprintln!("  [1] Paste client ID and secret directly (easiest)");
            eprintln!("  [2] Provide path to downloaded JSON credentials file");
            eprintln!("  [3] I need help creating credentials (opens setup guide)\n");
            eprint!("Choose [1/2/3]: ");
            io::stdout().flush()?;

            let mut input = String::new();
            io::stdin().read_line(&mut input)?;

            match input.trim() {
                "1" => {
                    eprintln!("\nPaste your Google OAuth Client ID:");
                    eprintln!("  (looks like: 123456789-abc.apps.googleusercontent.com)\n");
                    eprint!("> ");
                    io::stdout().flush()?;
                    let mut client_id = String::new();
                    io::stdin().read_line(&mut client_id)?;
                    let client_id = client_id.trim().to_string();

                    if client_id.is_empty() {
                        anyhow::bail!("No client ID provided.");
                    }

                    eprintln!("\nPaste your Google OAuth Client Secret:");
                    eprintln!("  (looks like: GOCSPX-...)\n");
                    eprint!("> ");
                    io::stdout().flush()?;
                    let mut client_secret = String::new();
                    io::stdin().read_line(&mut client_secret)?;
                    let client_secret = client_secret.trim().to_string();

                    if client_secret.is_empty() {
                        anyhow::bail!("No client secret provided.");
                    }

                    let creds = GoogleCredentials {
                        client_id,
                        client_secret,
                    };
                    auth::google::save_credentials(&creds)?;
                    eprintln!(
                        "\n✓ Credentials saved to {}\n",
                        auth::google::credentials_path()?.display()
                    );
                    creds
                }
                "2" => {
                    eprintln!("\nPaste the path to your downloaded JSON file:\n");
                    eprint!("> ");
                    io::stdout().flush()?;
                    let mut path_input = String::new();
                    io::stdin().read_line(&mut path_input)?;
                    let path_str = path_input.trim();

                    let path_str = if let Some(stripped) = path_str.strip_prefix("~/") {
                        if let Some(home) = dirs::home_dir() {
                            home.join(stripped).to_string_lossy().to_string()
                        } else {
                            path_str.to_string()
                        }
                    } else {
                        path_str.to_string()
                    };

                    let data = std::fs::read_to_string(&path_str)
                        .with_context(|| format!("Could not read file: {}", path_str))?;

                    let dest = auth::google::credentials_path()?;
                    if let Some(parent) = dest.parent() {
                        std::fs::create_dir_all(parent)?;
                        crate::platform::set_directory_permissions_owner_only(parent)?;
                    }
                    std::fs::write(&dest, &data)?;
                    crate::platform::set_permissions_owner_only(&dest)?;

                    let creds = auth::google::load_credentials()
                        .context("Could not parse the credentials file. Make sure it's the OAuth client JSON from Google Cloud Console.")?;

                    eprintln!("\n✓ Credentials imported to {}\n", dest.display());
                    creds
                }
                "3" => {
                    eprintln!("\n── Step-by-step Google Cloud setup ──\n");

                    eprintln!("1. Open Google Cloud Console and create a project:");
                    eprintln!("   Opening: https://console.cloud.google.com/projectcreate\n");
                    maybe_open_browser(
                        "https://console.cloud.google.com/projectcreate",
                        no_browser,
                    );
                    eprint!("   Press Enter when your project is created...");
                    io::stdout().flush()?;
                    let mut wait = String::new();
                    io::stdin().read_line(&mut wait)?;

                    eprintln!("\n2. Enable the Gmail API:");
                    eprintln!("   Opening: Gmail API library page\n");
                    maybe_open_browser(
                        "https://console.cloud.google.com/apis/library/gmail.googleapis.com",
                        no_browser,
                    );
                    eprintln!("   Click the blue 'Enable' button.");
                    eprint!("   Press Enter when done...");
                    io::stdout().flush()?;
                    io::stdin().read_line(&mut wait)?;

                    eprintln!("\n3. Configure OAuth consent screen:");
                    eprintln!("   Opening: OAuth consent screen\n");
                    maybe_open_browser(
                        "https://console.cloud.google.com/apis/credentials/consent",
                        no_browser,
                    );
                    eprintln!("   - Choose 'External' user type");
                    eprintln!("   - Fill in app name (e.g. 'jcode') and your email");
                    eprintln!("   - Skip scopes (we'll request them during login)");
                    eprintln!("   - Add your email as a test user");
                    eprintln!("   - Save and continue through all steps");
                    eprint!("   Press Enter when done...");
                    io::stdout().flush()?;
                    io::stdin().read_line(&mut wait)?;

                    eprintln!("\n4. Create OAuth credentials:");
                    eprintln!("   Opening: Credentials page\n");
                    maybe_open_browser(
                        "https://console.cloud.google.com/apis/credentials",
                        no_browser,
                    );
                    eprintln!("   - Click '+ Create Credentials' > 'OAuth client ID'");
                    eprintln!("   - Application type: 'Desktop app'");
                    eprintln!("   - Name: 'jcode'");
                    eprintln!("   - Click 'Create'\n");
                    eprintln!("   A dialog will show your Client ID and Client Secret.\n");

                    eprintln!("Paste your Client ID:");
                    eprint!("> ");
                    io::stdout().flush()?;
                    let mut client_id = String::new();
                    io::stdin().read_line(&mut client_id)?;
                    let client_id = client_id.trim().to_string();

                    if client_id.is_empty() {
                        anyhow::bail!("No client ID provided.");
                    }

                    eprintln!("\nPaste your Client Secret:");
                    eprint!("> ");
                    io::stdout().flush()?;
                    let mut client_secret = String::new();
                    io::stdin().read_line(&mut client_secret)?;
                    let client_secret = client_secret.trim().to_string();

                    if client_secret.is_empty() {
                        anyhow::bail!("No client secret provided.");
                    }

                    let creds = GoogleCredentials {
                        client_id,
                        client_secret,
                    };
                    auth::google::save_credentials(&creds)?;
                    eprintln!("\n✓ Credentials saved!\n");
                    creds
                }
                _ => {
                    eprintln!("\nInvalid choice. Please enter 1, 2, or 3.\n");
                    std::process::exit(1);
                }
            }
        }
    };

    eprintln!("── Gmail Access Level ──\n");
    eprintln!("  [1] Full Access (recommended)");
    eprintln!("      Search, read, draft, send, and manage emails.");
    eprintln!("      Send and delete always require your confirmation.\n");
    eprintln!("  [2] Read & Draft Only");
    eprintln!("      Search, read emails, create drafts. Cannot send or delete.");
    eprintln!("      API-level restriction - impossible even if the AI tries.\n");
    eprint!("Choose [1/2] (default: 1): ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let tier = match input.trim() {
        "" | "1" => GmailAccessTier::Full,
        "2" => GmailAccessTier::ReadOnly,
        _ => {
            eprintln!("Invalid choice, defaulting to Full Access.");
            GmailAccessTier::Full
        }
    };

    eprintln!("\nAccess level: {}", tier.label());

    eprintln!("\n── Logging in ──\n");

    let tokens = auth::google::login(tier, no_browser).await?;

    eprintln!("\n╔══════════════════════════════════════════╗");
    eprintln!("║  ✓ Gmail setup complete!                 ║");
    eprintln!("╚══════════════════════════════════════════╝\n");
    if let Some(email) = &tokens.email {
        eprintln!("  Account:      {}", email);
    }
    eprintln!("  Access tier:  {}", tokens.tier.label());
    eprintln!(
        "  Credentials:  {}",
        auth::google::credentials_path()?.display()
    );
    eprintln!(
        "  Tokens:       {}\n",
        auth::google::tokens_path()?.display()
    );
    eprintln!("The 'gmail' tool is now available to the AI agent.");
    eprintln!("Try asking: \"check my recent emails\" or \"search emails from ...\"");

    crate::telemetry::record_auth_success("google", "oauth");
    Ok(())
}

fn maybe_open_browser(target: &str, no_browser: bool) -> bool {
    if crate::auth::browser_suppressed(no_browser) {
        false
    } else {
        open::that(target).is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set_or_clear_env(key: &str, value: Option<std::ffi::OsString>) {
        if let Some(value) = value {
            crate::env::set_var(key, value);
        } else {
            crate::env::remove_var(key);
        }
    }

    #[test]
    fn scriptable_resume_command_matches_input_kind() {
        assert_eq!(
            scriptable_resume_command("openai", "callback_url"),
            "jcode login --provider openai --callback-url '<url-or-query>'"
        );
        assert_eq!(
            scriptable_resume_command("gemini", "auth_code"),
            "jcode login --provider gemini --auth-code '<code>'"
        );
        assert_eq!(
            scriptable_resume_command("copilot", "complete"),
            "jcode login --provider copilot --complete"
        );
    }

    #[test]
    fn load_pending_login_removes_expired_record() {
        let _guard = crate::storage::lock_test_env();
        let temp = tempfile::TempDir::new().expect("temp dir");
        let prev_home = std::env::var_os("JCODE_HOME");
        crate::env::set_var("JCODE_HOME", temp.path());

        let path = pending_login_path("openai").expect("pending path");
        let record = PendingScriptableLoginRecord {
            expires_at_ms: current_time_ms() - 1,
            login: PendingScriptableLogin::Openai {
                account_label: "default".to_string(),
                verifier: "verifier".to_string(),
                state: "state".to_string(),
                redirect_uri: "http://localhost:1455/auth/callback".to_string(),
            },
        };
        crate::storage::write_json_secret(&path, &record).expect("write pending login");

        let err = load_pending_login(&path, "openai").expect_err("expected expired state");
        assert!(err.to_string().contains("expired"));
        assert!(!path.exists(), "expired pending login should be removed");

        set_or_clear_env("JCODE_HOME", prev_home);
    }

    #[test]
    fn load_pending_login_accepts_legacy_format() {
        let _guard = crate::storage::lock_test_env();
        let temp = tempfile::TempDir::new().expect("temp dir");
        let prev_home = std::env::var_os("JCODE_HOME");
        crate::env::set_var("JCODE_HOME", temp.path());

        let path = pending_login_path("gemini").expect("pending path");
        let legacy = PendingScriptableLogin::Gemini {
            verifier: "verifier".to_string(),
            redirect_uri: auth::gemini::GEMINI_MANUAL_REDIRECT_URI.to_string(),
        };
        crate::storage::write_json_secret(&path, &legacy).expect("write legacy pending login");

        let loaded = load_pending_login(&path, "gemini").expect("load legacy pending login");
        match loaded {
            PendingScriptableLogin::Gemini {
                verifier,
                redirect_uri,
            } => {
                assert_eq!(verifier, "verifier");
                assert_eq!(redirect_uri, auth::gemini::GEMINI_MANUAL_REDIRECT_URI);
            }
            other => panic!("unexpected login variant: {:?}", other),
        }

        set_or_clear_env("JCODE_HOME", prev_home);
    }

    #[test]
    fn uses_scriptable_flow_detects_dash_input_without_consuming_stdin() {
        let options = LoginOptions {
            callback_url: Some("-".to_string()),
            ..LoginOptions::default()
        };
        assert!(
            options
                .uses_scriptable_flow()
                .expect("uses scriptable flow")
        );
        assert!(options.has_provided_input());
    }
}
