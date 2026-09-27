use super::{Tool, ToolContext, ToolOutput};
use crate::message::{ContentBlock, ToolCall};
use crate::session::Session;
use crate::storage;
use crate::{logging, util};
use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use kgrep::model::{Budget, FullRegionMode, Packet, Query, RenderOptions, Verb, Where};
use kgrep::packet::{render_find_text, render_grep_text, render_outline_text, render_trace_text};
use kgrep::{find, lexical, outline, trace};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

const AGENTGREP_FOREGROUND_BUDGET: Duration = Duration::from_secs(5);

mod args;
mod context;

#[cfg(test)]
use self::args::trace_or_smart_terms_owned;
use self::args::{query_from_params, summarize_agentgrep_request};
use self::context::maybe_write_context_json;
#[cfg(test)]
use self::context::{
    collect_bash_exposure, collect_trace_exposure, tune_known_file, tune_known_region,
};

#[derive(Debug, Deserialize)]
struct AgentGrepInput {
    #[serde(default = "default_agentgrep_mode")]
    mode: String,
    // `pattern` accepted for legacy grep-tool calls aliased to agentgrep.
    #[serde(default, alias = "pattern")]
    query: Option<String>,
    // `file_path` accepted because agents frequently pass it instead of `file`.
    #[serde(default, alias = "file_path")]
    file: Option<String>,
    #[serde(default)]
    terms: Option<Vec<String>>,
    #[serde(default)]
    regex: Option<bool>,
    #[serde(default)]
    path: Option<String>,
    // `include` accepted for legacy grep-tool calls aliased to agentgrep.
    #[serde(default, alias = "include")]
    glob: Option<String>,
    #[serde(rename = "type", default)]
    file_type: Option<String>,
    #[serde(default)]
    hidden: Option<bool>,
    #[serde(default)]
    no_ignore: Option<bool>,
    #[serde(default)]
    max_files: Option<usize>,
    #[serde(default)]
    max_regions: Option<usize>,
    #[serde(default)]
    max_tokens: Option<usize>,
    #[serde(default)]
    full_region: Option<String>,
    #[serde(default)]
    debug_score: Option<bool>,
    #[serde(default)]
    paths_only: Option<bool>,
}

fn default_agentgrep_mode() -> String {
    "grep".to_string()
}

#[derive(Debug, Serialize, Default)]
struct AgentGrepHarnessContext {
    version: u32,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    known_regions: Vec<AgentGrepKnownRegion>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    known_files: Vec<AgentGrepKnownFile>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    known_symbols: Vec<AgentGrepKnownSymbol>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    focus_files: Vec<String>,
}

#[derive(Debug, Serialize)]
struct AgentGrepKnownRegion {
    path: String,
    start_line: usize,
    end_line: usize,
    body_confidence: f32,
    current_version_confidence: f32,
    prune_confidence: f32,
    source_strength: &'static str,
    reasons: Vec<&'static str>,
}

#[derive(Debug, Serialize)]
struct AgentGrepKnownFile {
    path: String,
    structure_confidence: f32,
    body_confidence: f32,
    current_version_confidence: f32,
    prune_confidence: f32,
    source_strength: &'static str,
    reasons: Vec<&'static str>,
}

#[derive(Debug, Serialize)]
struct AgentGrepKnownSymbol {
    path: String,
    symbol: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    kind: Option<&'static str>,
    structure_confidence: f32,
    body_confidence: f32,
    current_version_confidence: f32,
    prune_confidence: f32,
    source_strength: &'static str,
    reasons: Vec<&'static str>,
}

#[derive(Debug, Clone, Copy)]
struct RegionConfidenceProfile {
    body_confidence: f32,
    current_version_confidence: f32,
    prune_confidence: f32,
    source_strength: &'static str,
}

#[derive(Debug, Clone)]
struct PendingTraceRegion {
    path: String,
    kind: Option<&'static str>,
    start_line: usize,
    end_line: usize,
}

#[derive(Debug, Clone)]
struct ToolExposureObservation {
    tool: ToolCall,
    content: String,
    timestamp: Option<DateTime<Utc>>,
    message_index: usize,
}

#[derive(Debug, Clone, Copy)]
struct ExposureDescriptor {
    timestamp: Option<DateTime<Utc>>,
    message_index: usize,
    total_messages: usize,
    compaction_cutoff: Option<usize>,
}

pub struct AgentGrepTool;

impl AgentGrepTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for AgentGrepTool {
    fn name(&self) -> &str {
        "agentgrep"
    }

    fn description(&self) -> &str {
        "Search code and file names. Defaults to grep mode when mode is omitted."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "intent": super::intent_schema_property(),
                "mode": {
                    "type": "string",
                    "enum": ["grep", "find", "outline", "trace"],
                    "description": "Mode: grep (default), find (file names), outline (one file), trace (relationship DSL)."
                },
                "query": {
                    "type": "string",
                    "description": "Search query. Required for grep (literal unless regex=true); optional ranking terms for find."
                },
                "file": {
                    "type": "string",
                    "description": "Single file to inspect. Required for outline."
                },
                "terms": {
                    "type": "array",
                    "items": {"type": "string"},
                    "description": "Trace DSL terms, e.g. [\"subject:auth_status\", \"relation:rendered\"]. Not for grep/find; use query."
                },
                "regex": {
                    "type": "boolean",
                    "description": "In grep mode, treat query as a regex. Defaults to false (literal)."
                },
                "path": {
                    "type": "string",
                    "description": "Directory or file to search, relative to the workspace. Omit to search the whole workspace."
                },
                "glob": {
                    "type": "string",
                    "description": "Optional file glob filter such as **/*.rs. Omit to search everything."
                },
                "type": {
                    "type": "string",
                    "description": "Optional ripgrep file type filter, such as rs, py, js, ts, or md."
                },
                "max_files": {
                    "type": "integer",
                    "description": "Maximum number of files to return. Bounds coverage in every sweeping mode, grep included."
                },
                "max_regions": {
                    "type": "integer",
                    "description": "Maximum number of match records to return. A coarse cap; prefer max_tokens for the size of the answer."
                },
                "max_tokens": {
                    "type": "integer",
                    "description": "Maximum estimated tokens of match detail to return, across all files. The knob for the size of the answer: raise it to see more, or omit it for the default. Unused by outline."
                },
                "paths_only": {
                    "type": "boolean",
                    "description": "Return only matching paths instead of match excerpts where supported."
                }
            }
        })
    }

    async fn execute(&self, input: Value, ctx: ToolContext) -> Result<ToolOutput> {
        let params: AgentGrepInput = serde_json::from_value(input)?;
        let display_name = summarize_background_search(&params);
        let session_id = ctx.session_id.clone();
        // The search shells out to ripgrep and walks/reads files (and for
        // trace/outline modes also loads the session and reads more files),
        // all of which is blocking work with no async yield points. Offload it
        // to the blocking pool so we never stall a tokio worker thread. When it
        // ran inline, a single poll of this future executed the whole search to
        // completion, freezing the TUI's select! render/input loop, which made
        // the first cold-cache search feel like it "takes forever" with no
        // spinner and an unresponsive interrupt. This mirrors how the sibling
        // grep/glob/ls tools offload their work.
        let work_handle =
            tokio::task::spawn_blocking(move || run_agentgrep_blocking(&params, &ctx));
        await_or_background_search(
            work_handle,
            AGENTGREP_FOREGROUND_BUDGET,
            display_name,
            session_id,
        )
        .await
    }
}

async fn await_or_background_search(
    mut work_handle: tokio::task::JoinHandle<Result<ToolOutput>>,
    foreground_budget: Duration,
    display_name: String,
    session_id: String,
) -> Result<ToolOutput> {
    match tokio::time::timeout(foreground_budget, &mut work_handle).await {
        Ok(joined) => {
            joined.map_err(|err| anyhow::anyhow!("agentgrep task failed to join: {err}"))?
        }
        Err(_) => {
            let info = crate::background::global()
                .adopt_with_options(
                    "agentgrep",
                    Some(display_name.clone()),
                    &session_id,
                    true,
                    false,
                    work_handle,
                )
                .await;
            Ok(ToolOutput::new(format!(
                    "Search is still running after 5s and is continuing in background.\n\n\
                     Task ID: {}\n\
                     Name: {}\n\n\
                     Use `bg` with action=\"wait\" and task_id=\"{}\" to wait for completion, or action=\"output\" to inspect its output.",
                    info.task_id, display_name, info.task_id,
                ))
                .with_title(display_name.clone())
                .with_metadata(json!({
                    "background": true,
                    "task_id": info.task_id,
                    "display_name": display_name,
                    "output_file": info.output_file.to_string_lossy(),
                    "status_file": info.status_file.to_string_lossy(),
                    "timeout_promoted": true,
                    "foreground_timeout_ms": foreground_budget.as_millis(),
                })))
        }
    }
}

fn summarize_background_search(params: &AgentGrepInput) -> String {
    let subject = params
        .query
        .as_deref()
        .or(params.file.as_deref())
        .or_else(|| {
            params
                .terms
                .as_ref()
                .and_then(|terms| terms.first().map(String::as_str))
        })
        .unwrap_or("workspace");
    format!(
        "agentgrep {}: {}",
        params.mode,
        util::truncate_str(subject, 80)
    )
}

fn run_agentgrep_blocking(params: &AgentGrepInput, ctx: &ToolContext) -> Result<ToolOutput> {
    if ctx.working_dir.is_none() {
        let explicit_path = params.path.as_deref().or(params.file.as_deref());
        if explicit_path.is_none_or(|path| !Path::new(path).is_absolute()) {
            anyhow::bail!(
                "agentgrep requires a session working directory unless an absolute path is provided"
            );
        }
    }
    let context_path = maybe_write_context_json(params, ctx)?;
    let request = summarize_agentgrep_request(params, ctx, context_path.as_deref());
    let started_at = std::time::Instant::now();
    let outcome = execute_linked_agentgrep(params, ctx);
    let elapsed_ms = started_at.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;

    if let Some(path) = context_path {
        let _ = std::fs::remove_file(path);
    }

    match outcome {
        Ok(output) => {
            if elapsed_ms >= 2_000 {
                logging::warn(&format!(
                    "agentgrep slow mode={} elapsed_ms={} request={}",
                    params.mode, elapsed_ms, request
                ));
            }
            Ok(output)
        }
        Err(err) => {
            let detail = err.to_string();
            let detail = util::truncate_str(detail.trim(), 600);
            logging::warn(&format!(
                "agentgrep failure mode={} elapsed_ms={} request={} error={}",
                params.mode, elapsed_ms, request, detail
            ));
            Err(anyhow::anyhow!(
                "agentgrep {} failed after {}ms: {}",
                params.mode,
                elapsed_ms,
                err
            ))
        }
    }
}

/// The bound on a result, shared by every sweeping verb.
///
/// One home for the three knobs. `max_files` reaches grep here as well, because
/// `Verb::Lexical` has no coverage cap of its own; find and trace carry the same
/// number on the verb, and the two compose by whichever is smaller.
fn budget_from_params(params: &AgentGrepInput) -> Budget {
    let default = Budget::default();
    Budget {
        max_total_matches: params.max_regions.unwrap_or(default.max_total_matches),
        max_hits: params.max_files.map_or(default.max_hits, Some),
        max_detail_tokens: params.max_tokens.map_or(default.max_detail_tokens, Some),
    }
}

fn execute_linked_agentgrep(params: &AgentGrepInput, ctx: &ToolContext) -> Result<ToolOutput> {
    let query = query_from_params(params, ctx)?;
    let exact_file = exact_search_file_path(ctx, params.path.as_deref());
    let render_options = RenderOptions {
        debug_score: params.debug_score.unwrap_or(false),
    };

    match &query.verb {
        Verb::Lexical { .. } => {
            let packet = filter_packet_to_exact_file(
                lexical::run_grep(&query, budget_from_params(params))
                    .map_err(anyhow::Error::msg)?,
                exact_file.as_deref(),
            );
            Ok(ToolOutput::new(render_grep_text(&packet)).with_title("agentgrep grep"))
        }
        Verb::Path { .. } => {
            let packet = filter_packet_to_exact_file(
                find::run_find(&query, budget_from_params(params)).map_err(anyhow::Error::msg)?,
                exact_file.as_deref(),
            );
            Ok(ToolOutput::new(render_find_text(&packet, &render_options))
                .with_title("agentgrep find"))
        }
        Verb::Outline { .. } => {
            let result = outline::run_outline(&query).map_err(anyhow::Error::msg)?;
            Ok(ToolOutput::new(render_outline_text(&result)).with_title("agentgrep outline"))
        }
        Verb::Structural { .. } => {
            let packet = filter_packet_to_exact_file(
                trace::run_trace(&query, budget_from_params(params)).map_err(anyhow::Error::msg)?,
                exact_file.as_deref(),
            );
            Ok(ToolOutput::new(render_trace_text(&packet, &render_options))
                .with_title(format!("agentgrep {}", params.mode)))
        }
    }
}

fn resolve_path_arg(ctx: &ToolContext, path: &str) -> PathBuf {
    ctx.resolve_path(Path::new(path))
}

fn exact_search_file_path(ctx: &ToolContext, path: Option<&str>) -> Option<String> {
    let path = path?;
    let resolved = resolve_path_arg(ctx, path);
    if !resolved.is_file() {
        return None;
    }
    resolved
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
}

/// Narrow a packet to the single file the caller named, when it named one.
///
/// A sweep searches the named file's parent and filters the result, because a
/// walker root is a directory. All three sweeping verbs now return the same
/// packet, so this is one function where there were three.
fn filter_packet_to_exact_file(mut packet: Packet, exact_file: Option<&str>) -> Packet {
    let Some(exact_file) = exact_file else {
        return packet;
    };

    packet.hits.retain(|hit| hit.path == exact_file);
    packet.total_files = packet.hits.len();
    packet.total_matches = packet.hits.iter().map(|hit| hit.matches.len()).sum();
    packet
}

fn normalized_agentgrep_glob(glob: Option<&str>) -> Option<&str> {
    let glob = glob?.trim();
    if glob.is_empty() {
        return None;
    }

    if is_match_all_glob(glob) {
        return None;
    }

    Some(glob)
}

fn normalized_agentgrep_glob_owned(glob: Option<&str>) -> Option<String> {
    normalized_agentgrep_glob(glob).map(ToOwned::to_owned)
}

fn is_match_all_glob(glob: &str) -> bool {
    matches!(glob, "*" | "**" | "**/*" | "./*" | "./**" | "./**/*")
}

#[cfg(test)]
#[path = "agentgrep_tests.rs"]
mod tests;
