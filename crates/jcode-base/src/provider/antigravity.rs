use super::{EventStream, Provider};
use crate::auth::antigravity as antigravity_auth;
use crate::message::{ConnectionPhase, Message, StreamEvent, ToolDefinition};
use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use jcode_provider_gemini::{
    CodeAssistGenerateRequest, CodeAssistGenerateResponse, GeminiFunctionCallingConfig,
    GeminiToolConfig, VertexGenerateContentRequest,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use uuid::Uuid;

const DEFAULT_MODEL: &str = "default";
/// Known-good model id used when the backend default is unknown. The literal
/// alias `"default"` is rejected by `generateContent` with HTTP 404, so we must
/// always resolve it to a real model id before issuing a request.
const DEFAULT_FALLBACK_MODEL: &str = "gemini-3-flash";
const AVAILABLE_MODELS: &[&str] = &[
    "claude-opus-4-6-thinking",
    "claude-sonnet-4-6",
    "gemini-3.1-pro-high",
    "gemini-3.1-pro-low",
    "gemini-3-flash",
    "gemini-3-flash-agent",
    "gemini-3.5-flash-low",
    "gpt-oss-120b-medium",
];
const FETCH_MODELS_API_URL: &str =
    "https://cloudcode-pa.googleapis.com/v1internal:fetchAvailableModels";
const GENERATE_CONTENT_API_URL: &str =
    "https://cloudcode-pa.googleapis.com/v1internal:generateContent";
const VERSION_ENV: &str = "JCODE_ANTIGRAVITY_VERSION";
const ANTIGRAVITY_VERSION: &str = "1.18.3";
const X_GOOG_API_CLIENT: &str = "google-cloud-sdk vscode_cloudshelleditor/0.1";
const CATALOG_REFRESH_TTL_HOURS: i64 = 6;

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
struct PersistedCatalog {
    models: Vec<CatalogModel>,
    fetched_at_rfc3339: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    default_model_id: Option<String>,
}

/// Result of parsing the backend `fetchAvailableModels` response: the ordered
/// catalog plus the backend-advertised default agent model id. The alias
/// `"default"` is not a real model id, so the resolved backend default is what
/// inference must actually send.
#[derive(Debug, Clone, Default, PartialEq)]
struct CatalogSnapshot {
    models: Vec<CatalogModel>,
    default_model_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
struct CatalogModel {
    id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reset_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tag_title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    model_provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_output_tokens: Option<u64>,
    #[serde(default)]
    recommended: bool,
    #[serde(default)]
    available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    remaining_fraction_milli: Option<u16>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FetchAvailableModelsResponse {
    #[serde(default)]
    models: HashMap<String, FetchAvailableModelEntry>,
    #[serde(default)]
    default_agent_model_id: Option<String>,
    #[serde(default)]
    command_model_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FetchAvailableModelEntry {
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    model_name: Option<String>,
    #[serde(default)]
    quota_info: Option<FetchAvailableQuotaInfo>,
    #[serde(default)]
    recommended: bool,
    #[serde(default)]
    tag_title: Option<String>,
    #[serde(default)]
    model_provider: Option<String>,
    #[serde(default)]
    max_tokens: Option<u64>,
    #[serde(default)]
    max_output_tokens: Option<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FetchAvailableQuotaInfo {
    #[serde(default)]
    remaining_fraction: Option<f64>,
    #[serde(default)]
    reset_time: Option<String>,
}

fn metadata_platform() -> &'static str {
    // The Cloud Code backend currently rejects OS-specific string enum values
    // such as MACOS, WINDOWS, and LINUX for ClientMetadata.Platform. Use the
    // string value that is accepted across platforms instead of varying by OS.
    "PLATFORM_UNSPECIFIED"
}

fn antigravity_version() -> String {
    std::env::var(VERSION_ENV)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| ANTIGRAVITY_VERSION.to_string())
}

fn antigravity_user_agent() -> String {
    if cfg!(target_os = "windows") {
        format!("antigravity/{} windows/amd64", antigravity_version())
    } else if cfg!(target_arch = "aarch64") {
        format!("antigravity/{} darwin/arm64", antigravity_version())
    } else {
        format!("antigravity/{} darwin/amd64", antigravity_version())
    }
}

fn client_metadata_header() -> String {
    format!(
        "{{\"ideType\":\"ANTIGRAVITY\",\"platform\":\"{}\",\"pluginType\":\"GEMINI\"}}",
        metadata_platform()
    )
}

fn remaining_fraction_to_milli(value: Option<f64>) -> Option<u16> {
    let value = value?;
    if !value.is_finite() {
        return None;
    }
    let clamped = value.clamp(0.0, 1.0);
    Some((clamped * 1000.0).round() as u16)
}

fn merge_antigravity_model_ids(models: impl IntoIterator<Item = String>) -> Vec<String> {
    let models: Vec<String> = models
        .into_iter()
        .map(|model| model.trim().to_string())
        .filter(|model| !model.is_empty())
        .collect();

    let mut seen = HashSet::new();
    let mut preferred = Vec::new();

    for known in AVAILABLE_MODELS {
        if models.iter().any(|model| model == known) && seen.insert((*known).to_string()) {
            preferred.push((*known).to_string());
        }
    }

    let mut extras: Vec<String> = models
        .into_iter()
        .filter(|model| seen.insert(model.clone()))
        .collect();
    extras.sort();
    preferred.extend(extras);
    preferred
}

pub(crate) fn is_known_model(model: &str) -> bool {
    let normalized = model.trim();
    !normalized.is_empty() && AVAILABLE_MODELS.contains(&normalized)
}

fn parse_fetch_available_models_response(
    response: &FetchAvailableModelsResponse,
) -> CatalogSnapshot {
    let default_model_id = response
        .default_agent_model_id
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_string);

    let mut preferred_ids = Vec::new();
    if let Some(default_agent_model_id) = response.default_agent_model_id.as_deref() {
        preferred_ids.push(default_agent_model_id.trim().to_string());
    }
    preferred_ids.extend(
        response
            .command_model_ids
            .iter()
            .map(|id| id.trim().to_string())
            .filter(|id| !id.is_empty()),
    );
    preferred_ids.extend(response.models.keys().map(|id| id.trim().to_string()));

    let ordered_ids = merge_antigravity_model_ids(preferred_ids);
    let mut by_id: HashMap<String, CatalogModel> = HashMap::new();

    for (model_id, entry) in &response.models {
        let id = model_id.trim();
        if id.is_empty() {
            continue;
        }
        let available = entry
            .quota_info
            .as_ref()
            .and_then(|quota| quota.remaining_fraction)
            .map(|remaining| remaining > 0.0)
            .unwrap_or(true);
        by_id.insert(
            id.to_string(),
            CatalogModel {
                id: id.to_string(),
                display_name: entry
                    .display_name
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string),
                reset_time: entry
                    .quota_info
                    .as_ref()
                    .and_then(|quota| quota.reset_time.as_deref())
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string),
                tag_title: entry
                    .tag_title
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string),
                model_provider: entry
                    .model_provider
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string),
                max_tokens: entry.max_tokens,
                max_output_tokens: entry.max_output_tokens,
                recommended: entry.recommended,
                available,
                remaining_fraction_milli: remaining_fraction_to_milli(
                    entry
                        .quota_info
                        .as_ref()
                        .and_then(|quota| quota.remaining_fraction),
                ),
            },
        );

        if let Some(alias) = entry.model_name.as_deref().map(str::trim)
            && !alias.is_empty()
            && alias != id
        {
            by_id
                .entry(alias.to_string())
                .or_insert_with(|| CatalogModel {
                    id: alias.to_string(),
                    display_name: entry
                        .display_name
                        .as_deref()
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(str::to_string),
                    reset_time: entry
                        .quota_info
                        .as_ref()
                        .and_then(|quota| quota.reset_time.as_deref())
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(str::to_string),
                    tag_title: entry
                        .tag_title
                        .as_deref()
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(str::to_string),
                    model_provider: entry
                        .model_provider
                        .as_deref()
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(str::to_string),
                    max_tokens: entry.max_tokens,
                    max_output_tokens: entry.max_output_tokens,
                    recommended: entry.recommended,
                    available,
                    remaining_fraction_milli: remaining_fraction_to_milli(
                        entry
                            .quota_info
                            .as_ref()
                            .and_then(|quota| quota.remaining_fraction),
                    ),
                });
        }
    }

    let mut models: Vec<CatalogModel> = ordered_ids
        .into_iter()
        .map(|id| {
            by_id.remove(&id).unwrap_or(CatalogModel {
                id,
                display_name: None,
                reset_time: None,
                tag_title: None,
                model_provider: None,
                max_tokens: None,
                max_output_tokens: None,
                recommended: false,
                available: true,
                remaining_fraction_milli: None,
            })
        })
        .collect();
    models.sort_by_key(|model| !model.available);
    CatalogSnapshot {
        models,
        default_model_id,
    }
}

fn catalog_model_detail(model: &CatalogModel) -> String {
    let mut parts = Vec::new();
    if let Some(display_name) = model.display_name.as_deref()
        && display_name != model.id
    {
        parts.push(display_name.to_string());
    }
    if model.recommended {
        parts.push("recommended".to_string());
    }
    if let Some(tag_title) = model.tag_title.as_deref() {
        parts.push(tag_title.to_string());
    }
    if let Some(model_provider) = model.model_provider.as_deref() {
        parts.push(model_provider.to_ascii_lowercase());
    }
    if let Some(remaining) = model.remaining_fraction_milli {
        let percent = remaining as f64 / 10.0;
        parts.push(format!("quota {:.1}%", percent));
    }
    if let Some(reset_time) = model.reset_time.as_deref() {
        parts.push(format!("resets {}", reset_time));
    }
    parts.join(" · ")
}

fn catalog_is_stale(fetched_at_rfc3339: &str) -> bool {
    let Ok(fetched_at) = DateTime::parse_from_rfc3339(fetched_at_rfc3339) else {
        return true;
    };
    Utc::now()
        .signed_duration_since(fetched_at.with_timezone(&Utc))
        .num_hours()
        >= CATALOG_REFRESH_TTL_HOURS
}

pub struct AntigravityProvider {
    client: reqwest::Client,
    model: Arc<RwLock<String>>,
    fetched_catalog: Arc<RwLock<Vec<CatalogModel>>>,
    /// Backend-advertised default agent model id (from `fetchAvailableModels`).
    /// Used to resolve the `"default"` alias to a real model for inference.
    backend_default_model: Arc<RwLock<Option<String>>>,
}

impl Clone for AntigravityProvider {
    fn clone(&self) -> Self {
        Self {
            client: self.client.clone(),
            model: self.model.clone(),
            fetched_catalog: self.fetched_catalog.clone(),
            backend_default_model: self.backend_default_model.clone(),
        }
    }
}

impl AntigravityProvider {
    fn persisted_catalog_path() -> Result<std::path::PathBuf> {
        Ok(crate::storage::app_config_dir()?.join("antigravity_models_cache.json"))
    }

    fn load_persisted_catalog() -> Option<PersistedCatalog> {
        let path = Self::persisted_catalog_path().ok()?;
        crate::storage::read_json(&path)
            .ok()
            .filter(|catalog: &PersistedCatalog| !catalog.models.is_empty())
    }

    fn persist_catalog(snapshot: &CatalogSnapshot) {
        if snapshot.models.is_empty() {
            return;
        }
        let Ok(path) = Self::persisted_catalog_path() else {
            return;
        };
        let payload = PersistedCatalog {
            models: snapshot.models.clone(),
            fetched_at_rfc3339: Utc::now().to_rfc3339(),
            default_model_id: snapshot.default_model_id.clone(),
        };
        if let Err(error) = crate::storage::write_json(&path, &payload) {
            crate::logging::warn(&format!(
                "Failed to persist Antigravity model catalog {}: {}",
                path.display(),
                error
            ));
        }
    }

    fn seed_cached_catalog(&self) {
        if let Some(catalog) = Self::load_persisted_catalog() {
            if catalog_is_stale(&catalog.fetched_at_rfc3339) {
                crate::logging::info(
                    "Loaded stale persisted Antigravity model catalog; a refresh will update it on next prefetch",
                );
            }
            if let Some(default_model_id) = catalog.default_model_id.clone() {
                *self
                    .backend_default_model
                    .write()
                    .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(default_model_id);
            }
            if let Ok(mut models) = self.fetched_catalog.write() {
                *models = catalog.models;
            }
        }
    }

    pub fn new() -> Self {
        let model =
            std::env::var("JCODE_ANTIGRAVITY_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.into());

        let provider = Self {
            client: crate::provider::shared_http_client(),
            model: Arc::new(RwLock::new(model)),
            fetched_catalog: Arc::new(RwLock::new(Vec::new())),
            backend_default_model: Arc::new(RwLock::new(None)),
        };
        provider.seed_cached_catalog();
        provider
    }

    fn fetched_catalog(&self) -> Vec<CatalogModel> {
        self.fetched_catalog
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    fn backend_default_model(&self) -> Option<String> {
        self.backend_default_model
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    /// Resolve the live OAuth credential the runtime would use, for the
    /// provider-doctor's native Antigravity driver.
    ///
    /// Antigravity authenticates exclusively via the Google OAuth tokens minted
    /// by `jcode login --provider antigravity`; there is no API-key path. This
    /// loads (and refreshes if needed) those tokens through the exact same code
    /// path inference uses, returning only the resolved Google account email so
    /// the doctor can confirm the credential without ever surfacing the token
    /// itself.
    pub async fn resolve_account_for_doctor(&self) -> Result<String> {
        let tokens = antigravity_auth::load_or_refresh_tokens().await?;
        if tokens.access_token.trim().is_empty() {
            anyhow::bail!("resolved an empty Antigravity access token");
        }
        match antigravity_auth::fetch_email(&tokens.access_token).await {
            Ok(email) if !email.trim().is_empty() => Ok(email),
            _ => Ok(String::new()),
        }
    }

    /// Fetch the live Antigravity model catalog using the resolved credential.
    ///
    /// Mirrors [`Provider::prefetch_models`] but returns the available model ids
    /// to the caller (rather than only persisting them) so the doctor can assert
    /// the live `fetchAvailableModels` endpoint works and that the model under
    /// test is in the live catalog. The warm catalog is persisted exactly like
    /// the runtime's own prefetch so the rest of the process benefits.
    pub async fn fetch_live_model_ids_for_doctor(&self) -> Result<Vec<String>> {
        let snapshot = self.fetch_available_models().await?;
        if snapshot.models.is_empty() {
            anyhow::bail!("Antigravity model catalog returned no models");
        }
        Self::persist_catalog(&snapshot);
        if let Some(default_model_id) = snapshot.default_model_id.clone() {
            *self
                .backend_default_model
                .write()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(default_model_id);
        }
        let model_ids: Vec<String> = snapshot
            .models
            .iter()
            .filter(|model| model.available)
            .map(|model| model.id.clone())
            .collect();
        *self
            .fetched_catalog
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = snapshot.models;
        if model_ids.is_empty() {
            anyhow::bail!("Antigravity model catalog returned no available models");
        }
        Ok(model_ids)
    }

    /// Resolve a requested model id into a real backend model id. The literal
    /// alias `"default"` (and the empty string) is rejected by the
    /// `generateContent` endpoint with HTTP 404, so it is mapped to the
    /// backend-advertised default, then to a known-good catalog model, and
    /// finally to a hardcoded fallback.
    ///
    /// Note: this only resolves the `"default"` alias / empty input. An
    /// explicit model id from the user is always honoured verbatim.
    fn resolve_model_for_request(&self, model: &str) -> String {
        let trimmed = model.trim();
        if !trimmed.is_empty() && trimmed != DEFAULT_MODEL {
            return trimmed.to_string();
        }

        if let Some(backend_default) = self
            .backend_default_model()
            .map(|id| id.trim().to_string())
            .filter(|id| !id.is_empty() && id != DEFAULT_MODEL)
        {
            return backend_default;
        }

        // No backend-advertised default: pick a usable catalog model. Prefer a
        // Gemini model, which works reliably with tool use on this backend.
        // Claude models on the Cloud Code backend currently reject jcode's tool
        // schemas (they require JSON Schema draft 2020-12), so they are a poor
        // automatic default even when listed first in the catalog.
        let catalog = self.fetched_catalog();
        if let Some(gemini_model) = catalog
            .iter()
            .find(|model| {
                model.available
                    && model.id.trim() != DEFAULT_MODEL
                    && model.id.starts_with("gemini-")
            })
            .map(|model| model.id.clone())
        {
            return gemini_model;
        }
        if let Some(catalog_model) = catalog
            .iter()
            .find(|model| model.available && model.id.trim() != DEFAULT_MODEL)
            .map(|model| model.id.clone())
        {
            return catalog_model;
        }

        DEFAULT_FALLBACK_MODEL.to_string()
    }

    async fn fetch_available_models_with_project(
        &self,
        access_token: &str,
        project_id: Option<&str>,
    ) -> Result<CatalogSnapshot> {
        let request = if let Some(project_id) = project_id.filter(|value| !value.trim().is_empty())
        {
            serde_json::json!({ "project": project_id })
        } else {
            serde_json::json!({})
        };

        let response = self
            .client
            .post(FETCH_MODELS_API_URL)
            .header(
                reqwest::header::AUTHORIZATION,
                format!("Bearer {}", access_token),
            )
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .header(reqwest::header::USER_AGENT, antigravity_user_agent())
            .header(
                reqwest::header::HeaderName::from_static("x-goog-api-client"),
                X_GOOG_API_CLIENT,
            )
            .header(
                reqwest::header::HeaderName::from_static("client-metadata"),
                client_metadata_header(),
            )
            .json(&request)
            .send()
            .await
            .context("Failed to fetch Antigravity model catalog")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = crate::util::http_error_body(response, "HTTP error").await;
            anyhow::bail!(
                "Antigravity model catalog request failed ({}): {}",
                status,
                body.trim()
            );
        }

        let parsed: FetchAvailableModelsResponse = response
            .json()
            .await
            .context("Failed to decode Antigravity model catalog response")?;
        Ok(parse_fetch_available_models_response(&parsed))
    }

    async fn fetch_available_models(&self) -> Result<CatalogSnapshot> {
        let mut tokens = antigravity_auth::load_or_refresh_tokens().await?;

        if let Some(project_id) = tokens
            .project_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            && let Ok(snapshot) = self
                .fetch_available_models_with_project(&tokens.access_token, Some(project_id))
                .await
            && !snapshot.models.is_empty()
        {
            return Ok(snapshot);
        }

        if let Ok(project_id) = antigravity_auth::fetch_project_id(&tokens.access_token).await {
            tokens.project_id = Some(project_id.clone());
            let _ = antigravity_auth::save_tokens(&tokens);
            if let Ok(snapshot) = self
                .fetch_available_models_with_project(&tokens.access_token, Some(&project_id))
                .await
                && !snapshot.models.is_empty()
            {
                return Ok(snapshot);
            }
        }

        self.fetch_available_models_with_project(&tokens.access_token, None)
            .await
    }

    async fn generate_content(
        &self,
        model: &str,
        messages: &[Message],
        tools: &[ToolDefinition],
        system: &str,
        resume_session_id: Option<&str>,
    ) -> Result<CodeAssistGenerateResponse> {
        let mut tokens = antigravity_auth::load_or_refresh_tokens().await?;
        let project = match tokens
            .project_id
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            Some(project_id) => project_id.to_string(),
            None => {
                let project_id = antigravity_auth::fetch_project_id(&tokens.access_token).await?;
                tokens.project_id = Some(project_id.clone());
                let _ = antigravity_auth::save_tokens(&tokens);
                project_id
            }
        };
        let resolved_model = self.resolve_model_for_request(model);
        let tools_is_empty = tools.is_empty();
        let mut tools = super::gemini::build_tools(tools);
        // Normalize each tool's JSON schema for the specific Antigravity backend
        // path the resolved model uses. The Cloud Code backend forwards each
        // model family to a different upstream (Gemini-native, Gemini->Anthropic,
        // or an OpenAI-compatible bridge), and each upstream rejects a different
        // construct. Gemini-native accepts everything jcode emits, so Gemini
        // models pass through unchanged. See `antigravity_compatible_schema`.
        if let Some(tools) = tools.as_mut() {
            for tool in tools.iter_mut() {
                for decl in tool.function_declarations.iter_mut() {
                    decl.parameters =
                        antigravity_compatible_schema(&decl.parameters, &resolved_model);
                }
            }
        }
        let request = CodeAssistGenerateRequest {
            model: resolved_model,
            project,
            user_prompt_id: Uuid::new_v4().to_string(),
            request: VertexGenerateContentRequest {
                contents: super::gemini::build_contents(messages),
                system_instruction: super::gemini::build_system_instruction(system),
                tools,
                tool_config: if tools_is_empty {
                    None
                } else {
                    Some(GeminiToolConfig {
                        function_calling_config: GeminiFunctionCallingConfig { mode: "AUTO" },
                    })
                },
                session_id: resume_session_id
                    .filter(|value| !value.trim().is_empty())
                    .map(str::to_string),
            },
        };

        let contents_value = serde_json::to_value(&request.request.contents).unwrap_or(Value::Null);
        let content_items = contents_value.as_array().cloned().unwrap_or_default();
        let system_value = request
            .request
            .system_instruction
            .as_ref()
            .and_then(|system| serde_json::to_value(system).ok());
        let tools_value = request
            .request
            .tools
            .as_ref()
            .and_then(|tools| serde_json::to_value(tools).ok());
        let payload = json!({
            "model": &request.model,
            "contents": contents_value,
            "system_instruction": system_value.as_ref(),
            "tools": tools_value.as_ref(),
            "tool_config": &request.request.tool_config,
        });
        super::fingerprint::log_provider_canonical_input(
            "antigravity",
            model,
            "gemini_generate_content",
            &payload,
            &content_items,
            system_value.as_ref(),
            tools_value.as_ref(),
            request.request.tools.as_ref().map(|tools| tools.len()),
            &[
                (
                    "session_id_present",
                    request.request.session_id.is_some().to_string(),
                ),
                ("project_present", (!request.project.is_empty()).to_string()),
            ],
        );

        let response = self
            .client
            .post(GENERATE_CONTENT_API_URL)
            .bearer_auth(&tokens.access_token)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .header(reqwest::header::USER_AGENT, antigravity_user_agent())
            .header("x-goog-api-client", X_GOOG_API_CLIENT)
            .header(
                "x-goog-request-params",
                format!("project={}", request.project),
            )
            .header("x-goog-client-metadata", client_metadata_header())
            .json(&request)
            .send()
            .await
            .context("Failed to send Antigravity generateContent request")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = crate::util::http_error_body(response, "HTTP error").await;
            anyhow::bail!(
                "Antigravity generateContent failed (HTTP {}): {}",
                status,
                body.trim()
            );
        }

        response
            .json()
            .await
            .context("Failed to decode Antigravity generateContent response")
    }
}

/// Whether a resolved Antigravity model id targets an Anthropic Claude model.
fn model_is_claude(model: &str) -> bool {
    model.trim().to_ascii_lowercase().contains("claude")
}

/// Whether a resolved Antigravity model id targets a Gemini model.
///
/// Gemini is the backend's native path and accepts every JSON Schema construct
/// jcode emits, so no schema rewriting is needed for these models.
fn model_is_gemini(model: &str) -> bool {
    model.trim().to_ascii_lowercase().starts_with("gemini")
}

/// Normalize a tool-parameter JSON schema for the Antigravity backend path that
/// the resolved model uses.
///
/// The Antigravity Cloud Code backend multiplexes several upstreams behind one
/// `generateContent` endpoint, and each upstream validates tool schemas
/// differently. jcode's emitted schemas are valid JSON Schema draft 2020-12
/// (verified against the metaschema), but two upstreams reject specific
/// constructs after their own re-translation:
///
/// - **Claude** (Gemini->Anthropic translation): rejects combiners
///   (`anyOf`/`oneOf`/`allOf`) with HTTP 400 "must match JSON Schema draft
///   2020-12". We collapse each combiner to its first branch.
/// - **gpt-oss / other OpenAI-compatible bridges**: round-trip numeric schema
///   bounds through a protobuf `int64`, which proto3 JSON re-encodes as a
///   string, then reject it ("'10' is not of type 'integer'"). We drop
///   `minItems`/`maxItems`/`minLength`/`maxLength`/`minProperties`/
///   `maxProperties` for these models. These are advisory bounds the model does
///   not need to satisfy a call, so dropping them is safe.
///
/// Gemini (the native path) is returned unchanged.
fn antigravity_compatible_schema(schema: &Value, model: &str) -> Value {
    if model_is_gemini(model) {
        return schema.clone();
    }
    if model_is_claude(model) {
        return flatten_schema_combiners(schema);
    }
    // Non-Gemini, non-Claude models (e.g. gpt-oss) reach an OpenAI-compatible
    // bridge that mangles numeric bounds; also flatten combiners defensively
    // since those bridges share Anthropic's strictness about them.
    strip_numeric_schema_bounds(&flatten_schema_combiners(schema))
}

/// Numeric JSON Schema bounds an OpenAI-compatible Antigravity bridge corrupts
/// when round-tripping through a protobuf `int64` field.
const NUMERIC_SCHEMA_BOUND_KEYS: &[&str] = &[
    "minItems",
    "maxItems",
    "minLength",
    "maxLength",
    "minProperties",
    "maxProperties",
];

/// Recursively drop [`NUMERIC_SCHEMA_BOUND_KEYS`] from a schema. See
/// `antigravity_compatible_schema` for why this is needed.
fn strip_numeric_schema_bounds(schema: &Value) -> Value {
    match schema {
        Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (key, value) in map {
                if NUMERIC_SCHEMA_BOUND_KEYS.contains(&key.as_str()) {
                    continue;
                }
                out.insert(key.clone(), strip_numeric_schema_bounds(value));
            }
            Value::Object(out)
        }
        Value::Array(items) => {
            Value::Array(items.iter().map(strip_numeric_schema_bounds).collect())
        }
        _ => schema.clone(),
    }
}

/// Collapse JSON Schema combiners (`anyOf`/`oneOf`/`allOf`) to their first
/// branch throughout a tool-parameter schema.
///
/// The Antigravity Cloud Code backend forwards Claude tool calls through a
/// Gemini->Anthropic schema translation that rejects these combiners with
/// HTTP 400 ("input_schema: JSON schema is invalid. It must match JSON Schema
/// draft 2020-12"). Collapsing to the first branch preserves a usable, valid
/// schema (e.g. `anyOf: [string, array<string>]` becomes `string`) so the tool
/// call is accepted; the agent simply uses the primary branch's shape.
fn flatten_schema_combiners(schema: &Value) -> Value {
    match schema {
        Value::Object(map) => {
            for combiner in ["anyOf", "oneOf", "allOf"] {
                if let Some(Value::Array(branches)) = map.get(combiner)
                    && let Some(first) = branches.first()
                {
                    // Merge sibling keys (e.g. `description`) onto the chosen
                    // branch so we don't lose prompt-visible metadata.
                    let mut flattened = match flatten_schema_combiners(first) {
                        Value::Object(branch_map) => branch_map,
                        other => return other,
                    };
                    for (key, value) in map {
                        if key == combiner {
                            continue;
                        }
                        flattened
                            .entry(key.clone())
                            .or_insert_with(|| flatten_schema_combiners(value));
                    }
                    return Value::Object(flattened);
                }
            }
            let mut out = serde_json::Map::new();
            for (key, value) in map {
                out.insert(key.clone(), flatten_schema_combiners(value));
            }
            Value::Object(out)
        }
        Value::Array(items) => {
            Value::Array(items.iter().map(flatten_schema_combiners).collect())
        }
        _ => schema.clone(),
    }
}

impl Default for AntigravityProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Provider for AntigravityProvider {
    async fn complete(
        &self,
        messages: &[Message],
        _tools: &[ToolDefinition],
        system: &str,
        _resume_session_id: Option<&str>,
    ) -> Result<EventStream> {
        let model = self
            .model
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        let messages = messages.to_vec();
        let tools = _tools.to_vec();
        let system = system.to_string();
        let resume_session_id = _resume_session_id.map(str::to_string);
        let provider = self.clone();
        let (tx, rx) = mpsc::channel::<Result<crate::message::StreamEvent>>(100);

        tokio::spawn(async move {
            let _ = tx
                .send(Ok(StreamEvent::ConnectionType {
                    connection: "https".to_string(),
                }))
                .await;
            let _ = tx
                .send(Ok(StreamEvent::ConnectionPhase {
                    phase: ConnectionPhase::Authenticating,
                }))
                .await;
            let _ = tx
                .send(Ok(StreamEvent::ConnectionPhase {
                    phase: ConnectionPhase::WaitingForResponse,
                }))
                .await;
            let response = match provider
                .generate_content(
                    &model,
                    &messages,
                    &tools,
                    &system,
                    resume_session_id.as_deref(),
                )
                .await
            {
                Ok(response) => response,
                Err(err) => {
                    let _ = tx.send(Err(err)).await;
                    return;
                }
            };
            let _ = tx
                .send(Ok(StreamEvent::ConnectionPhase {
                    phase: ConnectionPhase::Streaming,
                }))
                .await;
            if let Some(usage) = response
                .response
                .as_ref()
                .and_then(|r| r.usage_metadata.as_ref())
            {
                let _ = tx
                    .send(Ok(StreamEvent::TokenUsage {
                        input_tokens: usage.prompt_token_count,
                        output_tokens: usage.candidates_token_count,
                        cache_read_input_tokens: usage.cached_content_token_count,
                        cache_creation_input_tokens: None,
                    }))
                    .await;
            }
            let Some(candidate) = response
                .response
                .and_then(|r| r.candidates)
                .and_then(|mut c| c.drain(..).next())
            else {
                let _ = tx
                    .send(Err(anyhow::anyhow!(
                        "Antigravity returned no candidates for generateContent"
                    )))
                    .await;
                return;
            };
            if let Some(content) = candidate.content {
                // Gemini 3 attaches a `thoughtSignature` to function-call parts
                // (and occasionally to a standalone preceding part). Emit tool
                // calls through the standard ToolUseStart/End path so jcode
                // drives the multi-turn loop, and replay the signature via a
                // dedicated ToolUseSignature event so it can be persisted on the
                // ToolUse block and resent on later turns (required by the
                // Cloud Code backend, which rejects function calls missing it).
                let mut pending_signature: Option<String> = None;
                for part in content.parts {
                    let part_signature = part
                        .thought_signature
                        .as_ref()
                        .filter(|sig| !sig.is_empty())
                        .cloned();
                    if let Some(text) = part.text.filter(|text| !text.is_empty()) {
                        let _ = tx.send(Ok(StreamEvent::TextDelta(text))).await;
                    }
                    if let Some(function_call) = part.function_call {
                        let signature = part_signature.clone().or_else(|| pending_signature.take());
                        let raw_call_id = function_call
                            .id
                            .clone()
                            .unwrap_or_else(|| Uuid::new_v4().to_string());
                        let call_id = crate::message::sanitize_tool_id(&raw_call_id);
                        let _ = tx
                            .send(Ok(StreamEvent::ToolUseStart {
                                id: call_id,
                                name: function_call.name,
                            }))
                            .await;
                        let _ = tx
                            .send(Ok(StreamEvent::ToolInputDelta(
                                function_call.args.to_string(),
                            )))
                            .await;
                        let _ = tx.send(Ok(StreamEvent::ToolUseEnd)).await;
                        if let Some(signature) = signature {
                            let _ = tx
                                .send(Ok(StreamEvent::ToolUseSignature(signature)))
                                .await;
                        }
                    } else if let Some(signature) = part_signature {
                        // Standalone signature part; remember it for the next
                        // function call in this candidate.
                        pending_signature = Some(signature);
                    }
                }
            }

            let _ = tx
                .send(Ok(StreamEvent::MessageEnd {
                    stop_reason: candidate.finish_reason.clone(),
                }))
                .await;
        });

        Ok(Box::pin(ReceiverStream::new(rx)))
    }

    fn name(&self) -> &'static str {
        "antigravity"
    }

    fn model(&self) -> String {
        self.model
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    fn set_model(&self, model: &str) -> Result<()> {
        let trimmed = model.trim();
        if trimmed.is_empty() {
            anyhow::bail!("Antigravity model cannot be empty");
        }
        *self
            .model
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = trimmed.to_string();
        Ok(())
    }

    fn available_models(&self) -> Vec<&'static str> {
        AVAILABLE_MODELS.to_vec()
    }

    fn available_models_display(&self) -> Vec<String> {
        let catalog = self.fetched_catalog();
        merge_antigravity_model_ids(
            catalog
                .into_iter()
                .map(|model| model.id)
                .chain(std::iter::once(self.model())),
        )
    }

    fn available_models_for_switching(&self) -> Vec<String> {
        self.available_models_display()
    }

    fn model_routes(&self) -> Vec<super::ModelRoute> {
        let catalog = self.fetched_catalog();
        if !catalog.is_empty() {
            return catalog
                .into_iter()
                .map(|model| super::ModelRoute {
                    model: model.id.clone(),
                    provider: "Antigravity".to_string(),
                    api_method: "https".to_string(),
                    available: model.available,
                    detail: catalog_model_detail(&model),
                    cheapness: None,
                })
                .collect();
        }

        self.available_models_display()
            .into_iter()
            .map(|model| super::ModelRoute {
                model,
                provider: "Antigravity".to_string(),
                api_method: "https".to_string(),
                available: true,
                detail: "fallback catalog".to_string(),
                cheapness: None,
            })
            .collect()
    }

    fn on_auth_changed(&self) {
        let provider = self.clone();
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                if provider.prefetch_models().await.is_ok() {
                    crate::bus::Bus::global().publish_models_updated();
                }
            });
        }
    }

    async fn prefetch_models(&self) -> Result<()> {
        match self.fetch_available_models().await {
            Ok(snapshot) => {
                if !snapshot.models.is_empty() {
                    crate::logging::info(&format!(
                        "Discovered Antigravity models: {}{}",
                        snapshot
                            .models
                            .iter()
                            .map(|model| model.id.as_str())
                            .collect::<Vec<_>>()
                            .join(", "),
                        snapshot
                            .default_model_id
                            .as_deref()
                            .map(|id| format!(" (default: {id})"))
                            .unwrap_or_default()
                    ));
                    Self::persist_catalog(&snapshot);
                    if let Some(default_model_id) = snapshot.default_model_id.clone() {
                        *self
                            .backend_default_model
                            .write()
                            .unwrap_or_else(|poisoned| poisoned.into_inner()) =
                            Some(default_model_id);
                    }
                    *self
                        .fetched_catalog
                        .write()
                        .unwrap_or_else(|poisoned| poisoned.into_inner()) = snapshot.models;
                }
            }
            Err(err) => {
                crate::logging::warn(&format!(
                    "Antigravity model catalog refresh failed; keeping fallback list: {}",
                    err
                ));
            }
        }

        Ok(())
    }

    fn supports_compaction(&self) -> bool {
        false
    }

    fn fork(&self) -> Arc<dyn Provider> {
        Arc::new(Self {
            client: self.client.clone(),
            model: Arc::new(RwLock::new(self.model())),
            fetched_catalog: self.fetched_catalog.clone(),
            backend_default_model: self.backend_default_model.clone(),
        })
    }
}

#[cfg(test)]
#[path = "antigravity_tests.rs"]
mod antigravity_tests;
