use super::{Tool, ToolContext, ToolOutput};
use crate::{logging, util};
use anyhow::Result;
use async_trait::async_trait;
use kgrep::model::{Budget, FullRegionMode, Packet, Query, RenderOptions, Verb, Where};
use kgrep::packet::{render_find_text, render_grep_text, render_outline_text, render_trace_text};
use kgrep::{find, lexical, outline, trace};
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::time::Duration;

const KGREP_FOREGROUND_BUDGET: Duration = Duration::from_secs(5);

mod args;

#[cfg(test)]
use self::args::trace_or_smart_terms_owned;
use self::args::{query_from_params, summarize_kgrep_request};

#[derive(Debug, Deserialize)]
struct KgrepInput {
    #[serde(default = "default_kgrep_mode")]
    mode: String,
    // `pattern` accepted for legacy grep-tool calls aliased to kgrep.
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
    // `include` accepted for legacy grep-tool calls aliased to kgrep.
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

fn default_kgrep_mode() -> String {
    "grep".to_string()
}

pub struct KgrepTool;

impl KgrepTool {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for KgrepTool {
    fn name(&self) -> &str {
        "kgrep"
    }

    fn description(&self) -> &str {
        "Search code and file names; the only search tool (no grep, glob, or rg). Modes: grep (literal, or regex with regex=true; glob and type filters), find (file names), outline (one file), trace (relationship DSL)."
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
                    "description": "Maximum number of match records to return. Prefer max_tokens for the size of the answer."
                },
                "max_tokens": {
                    "type": "integer",
                    "description": "Maximum estimated tokens of detail to return. Raise it to see more. Outline ignores it."
                },
                "paths_only": {
                    "type": "boolean",
                    "description": "Return only matching paths instead of match excerpts where supported."
                }
            }
        })
    }

    async fn execute(&self, input: Value, ctx: ToolContext) -> Result<ToolOutput> {
        let params: KgrepInput = serde_json::from_value(input)?;
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
        let work_handle = tokio::task::spawn_blocking(move || run_kgrep_blocking(&params, &ctx));
        await_or_background_search(
            work_handle,
            KGREP_FOREGROUND_BUDGET,
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
        Ok(joined) => joined.map_err(|err| anyhow::anyhow!("kgrep task failed to join: {err}"))?,
        Err(_) => {
            let info = crate::background::global()
                .adopt_with_options(
                    "kgrep",
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

fn summarize_background_search(params: &KgrepInput) -> String {
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
    format!("kgrep {}: {}", params.mode, util::truncate_str(subject, 80))
}

fn run_kgrep_blocking(params: &KgrepInput, ctx: &ToolContext) -> Result<ToolOutput> {
    if ctx.working_dir.is_none() {
        let explicit_path = params.path.as_deref().or(params.file.as_deref());
        if explicit_path.is_none_or(|path| !Path::new(path).is_absolute()) {
            anyhow::bail!(
                "kgrep requires a session working directory unless an absolute path is provided"
            );
        }
    }
    let request = summarize_kgrep_request(params, ctx);
    let started_at = std::time::Instant::now();
    let outcome = execute_linked_kgrep(params, ctx);
    let elapsed_ms = started_at.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;

    match outcome {
        Ok(output) => {
            if elapsed_ms >= 2_000 {
                logging::warn(&format!(
                    "kgrep slow mode={} elapsed_ms={} request={}",
                    params.mode, elapsed_ms, request
                ));
            }
            Ok(output)
        }
        Err(err) => {
            let detail = err.to_string();
            let detail = util::truncate_str(detail.trim(), 600);
            logging::warn(&format!(
                "kgrep failure mode={} elapsed_ms={} request={} error={}",
                params.mode, elapsed_ms, request, detail
            ));
            Err(anyhow::anyhow!(
                "kgrep {} failed after {}ms: {}",
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
fn budget_from_params(params: &KgrepInput) -> Budget {
    let default = Budget::default();
    Budget {
        max_total_matches: params.max_regions.unwrap_or(default.max_total_matches),
        max_hits: params.max_files.map_or(default.max_hits, Some),
        max_detail_tokens: params.max_tokens.map_or(default.max_detail_tokens, Some),
    }
}

fn execute_linked_kgrep(params: &KgrepInput, ctx: &ToolContext) -> Result<ToolOutput> {
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
            Ok(ToolOutput::new(render_grep_text(&packet)).with_title("kgrep grep"))
        }
        Verb::Path { .. } => {
            let packet = filter_packet_to_exact_file(
                find::run_find(&query, budget_from_params(params)).map_err(anyhow::Error::msg)?,
                exact_file.as_deref(),
            );
            Ok(
                ToolOutput::new(render_find_text(&packet, &render_options))
                    .with_title("kgrep find"),
            )
        }
        Verb::Outline { .. } => {
            let result = outline::run_outline(&query).map_err(anyhow::Error::msg)?;
            Ok(ToolOutput::new(render_outline_text(&result)).with_title("kgrep outline"))
        }
        Verb::Structural { .. } => {
            let packet = filter_packet_to_exact_file(
                trace::run_trace(&query, budget_from_params(params)).map_err(anyhow::Error::msg)?,
                exact_file.as_deref(),
            );
            Ok(ToolOutput::new(render_trace_text(&packet, &render_options))
                .with_title(format!("kgrep {}", params.mode)))
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

fn normalized_kgrep_glob(glob: Option<&str>) -> Option<&str> {
    let glob = glob?.trim();
    if glob.is_empty() {
        return None;
    }

    if is_match_all_glob(glob) {
        return None;
    }

    Some(glob)
}

fn normalized_kgrep_glob_owned(glob: Option<&str>) -> Option<String> {
    normalized_kgrep_glob(glob).map(ToOwned::to_owned)
}

fn is_match_all_glob(glob: &str) -> bool {
    matches!(glob, "*" | "**" | "**/*" | "./*" | "./**" | "./**/*")
}

#[cfg(test)]
#[path = "kgrep_tests.rs"]
mod tests;
