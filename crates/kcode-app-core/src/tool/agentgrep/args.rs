use super::*;

struct ResolvedSearchScope {
    root: Option<String>,
    glob: Option<String>,
}

fn resolved_search_scope(
    ctx: &ToolContext,
    path: Option<&str>,
    file: Option<&str>,
    glob: Option<&str>,
) -> ResolvedSearchScope {
    // `file` scopes grep/find to one exact file when `path` is absent.
    let path = path.or(file);
    let Some(path) = path else {
        return ResolvedSearchScope {
            root: None,
            glob: normalized_agentgrep_glob_owned(glob),
        };
    };

    let resolved = resolve_path_arg(ctx, path);
    if resolved.is_file() {
        let root = resolved
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .display()
            .to_string();
        let glob = resolved
            .file_name()
            .map(|name| name.to_string_lossy().into_owned());
        return ResolvedSearchScope {
            root: Some(root),
            glob,
        };
    }

    ResolvedSearchScope {
        root: Some(resolved.display().to_string()),
        glob: normalized_agentgrep_glob_owned(glob),
    }
}

/// The one mapping from this tool's parameters to kgrep's query type.
///
/// This function is the seam. Everything kcode knows about searching is behind
/// it, and everything above it is the agent envelope: schema, budget, background
/// adoption and context.json. The four modes dispatch here, building a different
/// verb each time, because the verbs are not interchangeable.
pub(super) fn query_from_params(params: &AgentGrepInput, ctx: &ToolContext) -> Result<Query> {
    let paths_only = params.paths_only.unwrap_or(false);
    let (where_, verb) = match params.mode.as_str() {
        "grep" => {
            let text = params
                .query
                .clone()
                .ok_or_else(|| anyhow::anyhow!("agentgrep grep requires 'query'"))?;
            (
                narrowed_where(params, ctx)?,
                Verb::Lexical {
                    text,
                    regex: params.regex.unwrap_or(false),
                },
            )
        }
        "find" => {
            let query = params.query.as_deref().unwrap_or_default();
            if query.trim().is_empty()
                && params.path.as_deref().is_none_or(str::is_empty)
                && params.file.as_deref().is_none_or(str::is_empty)
                && normalized_agentgrep_glob(params.glob.as_deref()).is_none()
                && params.file_type.as_deref().is_none_or(str::is_empty)
            {
                return Err(anyhow::anyhow!(
                    "agentgrep find requires 'query' unless path, glob, or type narrows the search"
                ));
            }
            (
                narrowed_where(params, ctx)?,
                Verb::Path {
                    terms: query.split_whitespace().map(ToOwned::to_owned).collect(),
                    max_files: params.max_files.unwrap_or(10),
                },
            )
        }
        "outline" => {
            let (root, file) = outline_target(params, ctx)?;
            (
                Where::new(root),
                Verb::Outline {
                    file,
                    max_items: None,
                },
            )
        }
        "trace" | "smart" => {
            let terms = trace_or_smart_terms_owned(params)?;
            let query = kgrep::trace::parse_query(&terms).map_err(|err| {
                anyhow::anyhow!(
                    "{}\n\ntrace queries use a small DSL. Example:\n  agentgrep trace subject:auth_status relation:rendered support:ui",
                    err
                )
            })?;
            (
                narrowed_where(params, ctx)?,
                Verb::Structural {
                    query,
                    max_files: params.max_files.unwrap_or(5),
                    max_regions: params.max_regions.unwrap_or(6),
                    full_region: parse_full_region_mode(params.full_region.as_deref())?,
                },
            )
        }
        other => {
            return Err(anyhow::anyhow!(
                "Unsupported agentgrep mode: {other}. Use grep, find, outline, or trace."
            ));
        }
    };
    Ok(Query {
        where_,
        paths_only,
        verb,
    })
}

/// The narrowing every sweeping verb shares, as kgrep's `Where`.
fn narrowed_where(params: &AgentGrepInput, ctx: &ToolContext) -> Result<Where> {
    let scope = resolved_search_scope(
        ctx,
        params.path.as_deref(),
        params.file.as_deref(),
        params.glob.as_deref(),
    );
    let root = match scope.root {
        Some(root) => PathBuf::from(root),
        None => resolve_search_root(ctx, None)?,
    };
    Ok(Where {
        root,
        glob: scope.glob,
        file_type: params.file_type.clone(),
        hidden: params.hidden.unwrap_or(false),
        no_ignore: params.no_ignore.unwrap_or(false),
        follow: false,
    })
}

/// An outline's root and target file.
///
/// Outline does not use `narrowed_where`: its `file` is the thing being
/// described, not a filter, so scoping it the way a sweep is scoped would turn
/// the target into a glob and lose it. A file-valued `path` is treated as the
/// target itself, so the file argument is not joined onto it.
fn outline_target(params: &AgentGrepInput, ctx: &ToolContext) -> Result<(PathBuf, String)> {
    if let Some(path) = params.path.as_deref() {
        let resolved = resolve_path_arg(ctx, path);
        if resolved.is_file() {
            let root = ctx
                .working_dir
                .clone()
                .unwrap_or_else(|| PathBuf::from("."));
            return Ok((root, resolved.display().to_string()));
        }
        return Ok((resolved, outline_file_arg(params)?));
    }

    let root = ctx
        .working_dir
        .clone()
        .ok_or_else(|| anyhow::anyhow!("agentgrep requires a session working directory"))?;
    Ok((root, outline_file_arg(params)?))
}

pub(super) fn trace_or_smart_terms_owned(params: &AgentGrepInput) -> Result<Vec<String>> {
    if let Some(terms) = params.terms.as_ref().filter(|terms| !terms.is_empty()) {
        return Ok(terms.clone());
    }

    if params.mode == "smart"
        && let Some(query) = params.query.as_deref()
    {
        let split_terms: Vec<String> = query
            .split_whitespace()
            .filter(|term| !term.is_empty())
            .map(ToOwned::to_owned)
            .collect();
        if !split_terms.is_empty() {
            return Ok(split_terms);
        }
    }

    let field_hint = if params.mode == "smart" {
        "non-empty 'terms' or 'query'"
    } else {
        "non-empty 'terms'"
    };

    Err(anyhow::anyhow!(
        "agentgrep {} requires {}",
        params.mode,
        field_hint
    ))
}

fn outline_file_arg(params: &AgentGrepInput) -> Result<String> {
    params
        .file
        .clone()
        .or_else(|| params.query.clone())
        .or_else(|| {
            params
                .terms
                .as_ref()
                .and_then(|terms| terms.first().cloned())
        })
        .ok_or_else(|| {
            anyhow::anyhow!("agentgrep outline requires 'file' (or legacy 'query' / first term)")
        })
}

fn parse_full_region_mode(value: Option<&str>) -> Result<FullRegionMode> {
    match value.unwrap_or("auto").trim().to_ascii_lowercase().as_str() {
        "auto" => Ok(FullRegionMode::Auto),
        "always" => Ok(FullRegionMode::Always),
        "never" => Ok(FullRegionMode::Never),
        other => Err(anyhow::anyhow!(
            "agentgrep trace full_region must be one of: auto, always, never; got {other}"
        )),
    }
}

fn resolved_root_string(ctx: &ToolContext, path: Option<&str>) -> Option<String> {
    path.map(|path| resolve_path_arg(ctx, path).display().to_string())
}

pub(super) fn resolve_search_root(ctx: &ToolContext, path: Option<&str>) -> Result<PathBuf> {
    path.map(PathBuf::from)
        .or_else(|| ctx.working_dir.clone())
        .ok_or_else(|| anyhow::anyhow!("agentgrep requires a session working directory"))
}

pub(super) fn summarize_agentgrep_request(params: &AgentGrepInput, ctx: &ToolContext) -> String {
    let mut parts = vec![format!("mode={}", params.mode)];
    if let Some(query) = params.query.as_deref() {
        parts.push(format!("query={}", util::truncate_str(query, 80)));
    }
    if let Some(file) = params.file.as_deref() {
        parts.push(format!("file={file}"));
    }
    if let Some(terms) = params.terms.as_ref() {
        parts.push(format!(
            "terms={}",
            util::truncate_str(&terms.join(" "), 80)
        ));
    }
    if let Some(path) = resolved_root_string(ctx, params.path.as_deref()) {
        parts.push(format!("root={path}"));
    }
    if let Some(glob) = normalized_agentgrep_glob(params.glob.as_deref()) {
        parts.push(format!("glob={glob}"));
    }
    if let Some(file_type) = params.file_type.as_deref() {
        parts.push(format!("type={file_type}"));
    }
    if params.paths_only.unwrap_or(false) {
        parts.push("paths_only=true".to_string());
    }
    parts.join(" ")
}
