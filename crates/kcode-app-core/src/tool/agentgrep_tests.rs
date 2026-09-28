use super::*;
use std::fs;

fn test_ctx(root: &Path) -> ToolContext {
    ToolContext {
        session_id: "test".to_string(),
        message_id: "test".to_string(),
        tool_call_id: "test".to_string(),
        working_dir: Some(root.to_path_buf()),
        stdin_request_tx: None,
        graceful_shutdown_signal: None,
        execution_mode: super::super::ToolExecutionMode::Direct,
    }
}

fn grep_input(query: &str, max_regions: Option<usize>) -> AgentGrepInput {
    AgentGrepInput {
        mode: "grep".to_string(),
        query: Some(query.to_string()),
        file: None,
        terms: None,
        regex: Some(false),
        path: None,
        glob: None,
        file_type: None,
        hidden: None,
        no_ignore: None,
        max_files: None,
        max_regions,
        max_tokens: None,
        full_region: None,
        debug_score: None,
        paths_only: None,
    }
}

/// The same input, asked to run in another mode.
fn input_with_mode(mut params: AgentGrepInput, mode: &str) -> AgentGrepInput {
    params.mode = mode.to_string();
    params
}

#[tokio::test]
async fn foreground_budget_returns_fast_search_result_directly() {
    let handle = tokio::spawn(async { Ok(ToolOutput::new("fast result")) });
    let output = await_or_background_search(
        handle,
        std::time::Duration::from_secs(1),
        "fast search".to_string(),
        "agentgrep-fast-test".to_string(),
    )
    .await
    .expect("fast search should complete in foreground");

    assert_eq!(output.output, "fast result");
    assert!(output.metadata.is_none());
}

#[tokio::test]
async fn foreground_budget_promotes_slow_search_without_cancelling_it() {
    let handle = tokio::spawn(async {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        Ok(ToolOutput::new("eventual search result"))
    });
    let output = await_or_background_search(
        handle,
        std::time::Duration::from_millis(1),
        "slow search".to_string(),
        "agentgrep-slow-test".to_string(),
    )
    .await
    .expect("slow search should be promoted");
    let metadata = output.metadata.expect("expected background metadata");

    assert_eq!(metadata["background"], true);
    assert_eq!(metadata["timeout_promoted"], true);
    assert_eq!(metadata["foreground_timeout_ms"], 1);
    assert!(output.output.contains("continuing in background"));

    // The adopted handle must remain alive after the foreground call returns.
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    let output_path = metadata["output_file"].as_str().expect("output path");
    let saved = tokio::fs::read_to_string(output_path)
        .await
        .expect("background manager should persist the eventual result");
    assert!(saved.contains("eventual search result"));
}

#[test]
fn agentgrep_rejects_missing_session_cwd_instead_of_using_process_cwd() {
    let mut ctx = test_ctx(Path::new("/unused"));
    ctx.working_dir = None;

    let error = run_agentgrep_blocking(&grep_input("needle", None), &ctx)
        .expect_err("workspace search without a session cwd must fail");

    assert!(error.to_string().contains("session working directory"));
}

#[test]
fn grep_max_regions_limits_rendered_match_excerpts() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::write(
        temp.path().join("a.rs"),
        "fn one() { status_notice(); }\nfn two() { status_notice(); }\nfn three() { status_notice(); }\n",
    )
    .expect("write file");

    let output = execute_linked_agentgrep(
        &grep_input("status_notice", Some(2)),
        &test_ctx(temp.path()),
    )
    .expect("agentgrep execute")
    .output;

    assert_eq!(output.matches("      - @ ").count(), 2, "{output}");
    assert!(
        output.contains("1 more matches counted but not stored"),
        "{output}"
    );
}

#[test]
fn grep_caps_non_code_file_match_excerpts_by_default() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::write(
        temp.path().join("timeline.json"),
        (0..5)
            .map(|idx| format!("{{\"event\":\"status_notice {idx}\"}}\n"))
            .collect::<String>(),
    )
    .expect("write file");

    let output =
        execute_linked_agentgrep(&grep_input("status_notice", None), &test_ctx(temp.path()))
            .expect("agentgrep execute")
            .output;

    assert_eq!(output.matches("      - @ ").count(), 3, "{output}");
    assert!(
        output.contains("2 more non-code matches omitted"),
        "{output}"
    );
}

#[test]
fn query_from_params_includes_scope_flags() {
    let ctx = test_ctx(Path::new("/tmp/root"));
    let params = AgentGrepInput {
        mode: "grep".to_string(),
        query: Some("auth_status".to_string()),
        file: None,
        terms: None,
        regex: Some(true),
        path: Some("src".to_string()),
        glob: Some("src/**/*.rs".to_string()),
        file_type: Some("rs".to_string()),
        hidden: Some(true),
        no_ignore: Some(true),
        max_files: None,
        max_regions: None,
        max_tokens: None,
        full_region: None,
        debug_score: None,
        paths_only: Some(true),
    };

    let query = query_from_params(&params, &ctx).unwrap();
    let Verb::Lexical { text, regex } = &query.verb else {
        panic!("expected a lexical verb");
    };
    assert_eq!(text, "auth_status");
    assert!(*regex);
    assert_eq!(query.where_.file_type.as_deref(), Some("rs"));
    assert!(query.paths_only);
    assert!(query.where_.hidden);
    assert!(query.where_.no_ignore);
    assert_eq!(query.where_.root, PathBuf::from("/tmp/root/src"));
    assert_eq!(query.where_.glob.as_deref(), Some("src/**/*.rs"));
}

#[test]
fn query_from_params_drops_match_all_glob() {
    let ctx = test_ctx(Path::new("/tmp/root"));
    let params = AgentGrepInput {
        mode: "grep".to_string(),
        query: Some("agentgrep".to_string()),
        file: None,
        terms: None,
        regex: Some(false),
        path: Some(".".to_string()),
        glob: Some("**/*".to_string()),
        file_type: Some("rs".to_string()),
        hidden: None,
        no_ignore: None,
        max_files: None,
        max_regions: None,
        max_tokens: None,
        full_region: None,
        debug_score: None,
        paths_only: None,
    };

    let query = query_from_params(&params, &ctx).unwrap();
    let Verb::Lexical { text, .. } = &query.verb else {
        panic!("expected a lexical verb");
    };
    assert_eq!(text, "agentgrep");
    assert_eq!(query.where_.file_type.as_deref(), Some("rs"));
    assert_eq!(query.where_.root, PathBuf::from("/tmp/root/."));
    assert_eq!(query.where_.glob, None);
}

#[test]
fn query_from_params_scopes_file_path_to_parent_and_exact_glob() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(temp.path().join("src")).expect("mkdir");
    fs::write(temp.path().join("src/app.rs"), "fn auth_status() {}\n").expect("write file");

    let ctx = test_ctx(temp.path());
    let params = AgentGrepInput {
        mode: "grep".to_string(),
        query: Some("auth_status".to_string()),
        file: None,
        terms: None,
        regex: Some(false),
        path: Some("src/app.rs".to_string()),
        glob: Some("**/*.rs".to_string()),
        file_type: Some("rs".to_string()),
        hidden: None,
        no_ignore: None,
        max_files: None,
        max_regions: None,
        max_tokens: None,
        full_region: None,
        debug_score: None,
        paths_only: None,
    };

    let query = query_from_params(&params, &ctx).unwrap();
    assert_eq!(query.where_.root, temp.path().join("src"));
    assert_eq!(query.where_.glob.as_deref(), Some("app.rs"));
}

#[test]
fn query_from_params_scopes_file_field_to_exact_file() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(temp.path().join("src")).expect("mkdir");
    fs::write(temp.path().join("src/app.rs"), "fn auth_status() {}\n").expect("write file");

    let ctx = test_ctx(temp.path());
    let params = AgentGrepInput {
        mode: "grep".to_string(),
        query: Some("auth_status".to_string()),
        file: Some("src/app.rs".to_string()),
        terms: None,
        regex: Some(false),
        path: None,
        glob: Some("**/*.rs".to_string()),
        file_type: Some("rs".to_string()),
        hidden: None,
        no_ignore: None,
        max_files: None,
        max_regions: None,
        max_tokens: None,
        full_region: None,
        debug_score: None,
        paths_only: None,
    };

    let grep = query_from_params(&params, &ctx).unwrap();
    let find = query_from_params(&input_with_mode(params, "find"), &ctx).unwrap();
    let expected_parent = temp.path().join("src");
    assert_eq!(grep.where_.root, expected_parent);
    assert_eq!(grep.where_.glob.as_deref(), Some("app.rs"));
    assert_eq!(find.where_.root, expected_parent);
    assert_eq!(find.where_.glob.as_deref(), Some("app.rs"));
}

#[test]
fn query_from_params_find_allows_glob_only_search() {
    let ctx = test_ctx(Path::new("/tmp/root"));
    let params = AgentGrepInput {
        mode: "find".to_string(),
        query: None,
        file: None,
        terms: None,
        regex: None,
        path: Some(".".to_string()),
        glob: Some("**/*release*".to_string()),
        file_type: None,
        hidden: None,
        no_ignore: None,
        max_files: Some(25),
        max_regions: None,
        max_tokens: None,
        full_region: None,
        debug_score: None,
        paths_only: Some(true),
    };

    let query = query_from_params(&params, &ctx).expect("glob-only find should be valid");
    let Verb::Path { terms, max_files } = &query.verb else {
        panic!("expected a path verb");
    };
    assert!(terms.is_empty());
    assert_eq!(query.where_.root, PathBuf::from("/tmp/root/."));
    assert_eq!(query.where_.glob.as_deref(), Some("**/*release*"));
    assert_eq!(*max_files, 25);
    assert!(query.paths_only);
}

#[test]
fn query_from_params_find_still_rejects_unscoped_empty_query() {
    let ctx = test_ctx(Path::new("/tmp/root"));
    let params = AgentGrepInput {
        mode: "find".to_string(),
        query: None,
        file: None,
        terms: None,
        regex: None,
        path: None,
        glob: None,
        file_type: None,
        hidden: None,
        no_ignore: None,
        max_files: None,
        max_regions: None,
        max_tokens: None,
        full_region: None,
        debug_score: None,
        paths_only: None,
    };

    let error = query_from_params(&params, &ctx).unwrap_err();
    assert_eq!(
        error.to_string(),
        "agentgrep find requires 'query' unless path, glob, or type narrows the search"
    );
}

#[test]
fn query_from_params_smart_uses_terms() {
    let ctx = test_ctx(Path::new("/workspace"));
    let params = AgentGrepInput {
        mode: "smart".to_string(),
        query: None,
        file: None,
        terms: Some(vec![
            "subject:auth_status".to_string(),
            "relation:rendered".to_string(),
            "path:src/tui".to_string(),
        ]),
        regex: None,
        path: Some("repo".to_string()),
        glob: None,
        file_type: Some("rs".to_string()),
        hidden: None,
        no_ignore: None,
        max_files: Some(3),
        max_regions: Some(4),
        max_tokens: None,
        full_region: Some("auto".to_string()),
        debug_score: Some(true),
        paths_only: None,
    };

    let query = query_from_params(&params, &ctx).unwrap();
    let Verb::Structural {
        query: structural,
        max_files,
        max_regions,
        full_region,
    } = &query.verb
    else {
        panic!("expected a structural verb");
    };
    assert_eq!(*max_files, 3);
    assert_eq!(*max_regions, 4);
    assert!(matches!(*full_region, FullRegionMode::Auto));
    assert_eq!(query.where_.file_type.as_deref(), Some("rs"));
    assert_eq!(query.where_.root, PathBuf::from("/workspace/repo"));
    assert_eq!(structural.subject, "auth_status");
    assert_eq!(structural.relation.as_str(), "rendered");
    assert_eq!(structural.path_hint.as_deref(), Some("src/tui"));
}

#[test]
fn query_from_params_smart_falls_back_to_query() {
    let ctx = test_ctx(Path::new("/workspace"));
    let params = AgentGrepInput {
        mode: "smart".to_string(),
        query: Some(
            "subject:auth_status relation:rendered path:src/tui support:current".to_string(),
        ),
        file: None,
        terms: None,
        regex: None,
        path: Some("repo".to_string()),
        glob: None,
        file_type: Some("rs".to_string()),
        hidden: None,
        no_ignore: None,
        max_files: Some(3),
        max_regions: Some(4),
        max_tokens: None,
        full_region: Some("auto".to_string()),
        debug_score: Some(true),
        paths_only: None,
    };

    let query = query_from_params(&params, &ctx).unwrap();
    let Verb::Structural {
        query: structural, ..
    } = &query.verb
    else {
        panic!("expected a structural verb");
    };
    assert_eq!(structural.subject, "auth_status");
    assert_eq!(structural.relation.as_str(), "rendered");
    assert_eq!(structural.path_hint.as_deref(), Some("src/tui"));
    assert_eq!(structural.support, vec!["current".to_string()]);
}

#[test]
fn build_args_for_trace_still_requires_terms() {
    let params = AgentGrepInput {
        mode: "trace".to_string(),
        query: Some("subject:auth_status relation:rendered".to_string()),
        file: None,
        terms: None,
        regex: None,
        path: None,
        glob: None,
        file_type: None,
        hidden: None,
        no_ignore: None,
        max_files: None,
        max_regions: None,
        max_tokens: None,
        full_region: None,
        debug_score: None,
        paths_only: None,
    };

    let error = trace_or_smart_terms_owned(&params).unwrap_err();
    assert_eq!(
        error.to_string(),
        "agentgrep trace requires non-empty 'terms'"
    );
}

#[test]
fn schema_only_advertises_common_public_fields() {
    let schema = AgentGrepTool::new().parameters_schema();
    let props = schema["properties"]
        .as_object()
        .expect("agentgrep schema should have properties");
    let required = schema["required"].as_array().cloned().unwrap_or_default();
    let mode_enum = props["mode"]["enum"]
        .as_array()
        .expect("agentgrep mode should expose enum values");

    assert!(
        !required.contains(&json!("mode")),
        "agentgrep mode should be optional because omitted mode defaults to grep"
    );
    assert!(props.contains_key("mode"));
    assert!(props.contains_key("query"));
    assert!(props.contains_key("file"));
    assert!(props.contains_key("terms"));
    assert!(props.contains_key("regex"));
    assert!(props.contains_key("path"));
    assert!(props.contains_key("glob"));
    assert!(props.contains_key("type"));
    assert!(props.contains_key("max_files"));
    assert!(props.contains_key("max_regions"));
    assert!(props.contains_key("max_tokens"));
    assert!(props.contains_key("paths_only"));
    assert_eq!(
        mode_enum,
        &vec![
            json!("grep"),
            json!("find"),
            json!("outline"),
            json!("trace")
        ]
    );
    assert!(!props.contains_key("hidden"));
    assert!(!props.contains_key("no_ignore"));
    assert!(!props.contains_key("full_region"));
    assert!(!props.contains_key("debug_plan"));
    assert!(!props.contains_key("debug_score"));
}

#[test]
fn input_defaults_missing_mode_to_grep() {
    let params: AgentGrepInput = serde_json::from_value(json!({
        "query": "auth_status",
        "path": "src"
    }))
    .expect("agentgrep input without mode should deserialize");

    assert_eq!(params.mode, "grep");
    assert_eq!(params.query.as_deref(), Some("auth_status"));
}

#[test]
fn query_from_params_outline_accepts_file_field() {
    let ctx = test_ctx(Path::new("/workspace"));
    let params = AgentGrepInput {
        mode: "outline".to_string(),
        query: None,
        file: Some("src/tool/agentgrep.rs".to_string()),
        terms: None,
        regex: None,
        path: Some("repo".to_string()),
        glob: None,
        file_type: None,
        hidden: None,
        no_ignore: None,
        max_files: None,
        max_regions: None,
        max_tokens: None,
        full_region: None,
        debug_score: None,
        paths_only: None,
    };

    let query = query_from_params(&params, &ctx).unwrap();
    let Verb::Outline { file, .. } = &query.verb else {
        panic!("expected an outline verb");
    };
    assert_eq!(file, "src/tool/agentgrep.rs");
    assert_eq!(query.where_.root, PathBuf::from("/workspace/repo"));
}

#[test]
fn input_accepts_file_path_alias_for_file() {
    let params: AgentGrepInput = serde_json::from_value(json!({
        "mode": "outline",
        "file_path": "src/app.rs"
    }))
    .expect("agentgrep input with file_path should deserialize");

    assert_eq!(params.file.as_deref(), Some("src/app.rs"));
}

#[test]
fn query_from_params_outline_treats_file_valued_path_as_target() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::write(temp.path().join("app.rs"), "fn main() {}\n").expect("write file");
    let ctx = test_ctx(temp.path());

    let params = AgentGrepInput {
        mode: "outline".to_string(),
        query: Some("fn".to_string()),
        file: None,
        terms: None,
        regex: None,
        path: Some("app.rs".to_string()),
        glob: None,
        file_type: None,
        hidden: None,
        no_ignore: None,
        max_files: None,
        max_regions: None,
        max_tokens: None,
        full_region: None,
        debug_score: None,
        paths_only: None,
    };

    let query = query_from_params(&params, &ctx).unwrap();
    let Verb::Outline { file, .. } = &query.verb else {
        panic!("expected an outline verb");
    };
    assert_eq!(
        file,
        &temp.path().join("app.rs").display().to_string(),
        "file-valued path should become the outline target instead of joining query onto it"
    );
    assert_eq!(query.where_.root, temp.path());
}

#[test]
fn query_from_params_outline_does_not_duplicate_file_valued_path() {
    let temp = tempfile::tempdir().expect("tempdir");
    let relative_file = "src/tool/todo.rs";
    let absolute_file = temp.path().join(relative_file);
    fs::create_dir_all(absolute_file.parent().expect("file parent")).expect("mkdir");
    fs::write(&absolute_file, "pub fn save_todos() {}\n").expect("write file");
    let ctx = test_ctx(temp.path());

    let params = AgentGrepInput {
        mode: "outline".to_string(),
        query: None,
        file: Some(relative_file.to_string()),
        terms: None,
        regex: None,
        path: Some(relative_file.to_string()),
        glob: None,
        file_type: None,
        hidden: None,
        no_ignore: None,
        max_files: None,
        max_regions: None,
        max_tokens: None,
        full_region: None,
        debug_score: None,
        paths_only: None,
    };

    let query = query_from_params(&params, &ctx).unwrap();
    let Verb::Outline { file, .. } = &query.verb else {
        panic!("expected an outline verb");
    };
    assert_eq!(file, &absolute_file.display().to_string());
    assert_eq!(query.where_.root, temp.path());
}

#[tokio::test]
async fn execute_runs_linked_grep() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(temp.path().join("src")).expect("mkdir");
    fs::write(
        temp.path().join("src/app.rs"),
        "pub fn auth_status() {}\nfn render_status_bar() {}\n",
    )
    .expect("write file");

    let tool = AgentGrepTool::new();
    let ctx = test_ctx(temp.path());
    let output = tool
        .execute(
            json!({"mode": "grep", "query": "auth_status", "path": ".", "type": "rs"}),
            ctx,
        )
        .await
        .expect("tool output");
    assert!(output.output.contains("query: auth_status"));
    assert!(output.output.contains("src/app.rs"));
    assert!(output.output.contains("@ 1 pub fn auth_status() {}"));
}

#[tokio::test]
async fn execute_runs_linked_grep_when_mode_is_omitted() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(temp.path().join("src")).expect("mkdir");
    fs::write(temp.path().join("src/app.rs"), "pub fn auth_status() {}\n").expect("write file");

    let tool = AgentGrepTool::new();
    let ctx = test_ctx(temp.path());
    let output = tool
        .execute(json!({"query": "auth_status", "path": "src"}), ctx)
        .await
        .expect("tool output");

    assert!(output.output.contains("query: auth_status"));
    assert!(output.output.contains("app.rs"));
}

#[tokio::test]
async fn execute_grep_file_field_does_not_scan_sibling_files() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(temp.path().join("src")).expect("mkdir");
    fs::write(temp.path().join("src/app.rs"), "fn target() {}\n").expect("write target");
    fs::write(
        temp.path().join("src/sibling.rs"),
        "fn target() { panic!(\"sibling marker\") }\n",
    )
    .expect("write sibling");

    let output = AgentGrepTool::new()
        .execute(
            json!({"mode": "grep", "query": "target", "file": "src/app.rs"}),
            test_ctx(temp.path()),
        )
        .await
        .expect("file-scoped grep");

    assert!(output.output.contains("app.rs"));
    assert!(!output.output.contains("sibling.rs"));
    assert!(!output.output.contains("sibling marker"));
}

#[tokio::test]
async fn execute_runs_linked_grep_when_path_points_to_file() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(temp.path().join("src")).expect("mkdir");
    fs::write(
        temp.path().join("src/app.rs"),
        "pub fn auth_status() {}\nfn render_status_bar() {}\n",
    )
    .expect("write target file");
    fs::write(
        temp.path().join("src/other.rs"),
        "pub fn auth_status() {}\nfn render_other() {}\n",
    )
    .expect("write sibling file");

    let tool = AgentGrepTool::new();
    let ctx = test_ctx(temp.path());
    let output = tool
        .execute(
            json!({
                "mode": "grep",
                "query": "auth_status",
                "path": "src/app.rs",
                "glob": "**/*.rs",
                "type": "rs"
            }),
            ctx,
        )
        .await
        .expect("tool output for exact-file path");
    assert!(output.output.contains("app.rs"));
    assert!(!output.output.contains("src/other.rs"));
    assert!(!output.output.contains("other.rs"));
}

#[tokio::test]
async fn execute_smart_accepts_query_fallback() {
    let temp = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(temp.path().join("src/tool")).expect("mkdir");
    fs::write(
        temp.path().join("src/tool/lsp.rs"),
        r#"pub struct LspTool;
impl LspTool {}
fn execute() { println!("implementation"); }
"#,
    )
    .expect("write file");

    let tool = AgentGrepTool::new();
    let ctx = test_ctx(temp.path());
    let output = tool
        .execute(
            json!({
                "mode": "smart",
                "query": "subject:lsp relation:implementation path:src/tool",
                "path": ".",
                "max_files": 2,
                "max_regions": 3
            }),
            ctx,
        )
        .await
        .expect("agentgrep execution");
    assert!(output.output.contains("subject:lsp"));
    assert!(output.output.contains("relation:implementation"));
}

#[test]
fn input_accepts_legacy_grep_param_aliases() {
    // Models sometimes call the removed native `grep` tool, which is now
    // aliased to agentgrep. Its `pattern`/`include` params must map to
    // agentgrep's `query`/`glob`.
    let input: AgentGrepInput = serde_json::from_value(serde_json::json!({
        "pattern": "fn main",
        "include": "*.rs",
        "path": "src"
    }))
    .expect("legacy grep params should deserialize");
    assert_eq!(input.query.as_deref(), Some("fn main"));
    assert_eq!(input.glob.as_deref(), Some("*.rs"));
    assert_eq!(input.path.as_deref(), Some("src"));
    assert_eq!(input.mode, "grep");
}

#[test]
fn budget_maps_the_three_model_knobs() {
    // The default is a floor on safety: with no knobs set, every field is
    // kgrep's default.
    let default = budget_from_params(&grep_input("x", None));
    assert_eq!(
        default.max_total_matches,
        Budget::default().max_total_matches
    );
    assert_eq!(default.max_hits, Budget::default().max_hits);
    assert_eq!(
        default.max_detail_tokens,
        Budget::default().max_detail_tokens
    );

    // An explicit value must win in either direction, including a larger one,
    // so the default is a floor on safety and not a ceiling on capability.
    // `max_files` is coverage, `max_regions` records, `max_tokens` detail.
    for explicit in [5usize, 5_000] {
        let params = AgentGrepInput {
            max_files: Some(explicit),
            max_regions: Some(explicit),
            max_tokens: Some(explicit),
            ..grep_input("x", None)
        };
        let budget = budget_from_params(&params);
        assert_eq!(budget.max_hits, Some(explicit), "max_files is coverage");
        assert_eq!(budget.max_total_matches, explicit, "max_regions is records");
        assert_eq!(
            budget.max_detail_tokens,
            Some(explicit),
            "max_tokens is detail"
        );
    }

    // The default has to be generous enough that ordinary code searches are
    // untouched; a cap that clips normal work trades one problem for another.
    // Checked against a real search rather than as a constant comparison, which
    // the compiler would fold away: this repo's own uses of a common internal
    // symbol must fit under the cap.
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let params = grep_input("guard_context_overflow", None);
    let query = query_from_params(&params, &test_ctx(root)).expect("grep query");
    let packet = lexical::run_grep(&query, budget_from_params(&params)).expect("grep should run");
    assert!(
        packet.total_matches > 0,
        "sanity: the probe symbol should exist in this crate"
    );
    assert!(
        !packet.truncated,
        "an ordinary in-repo search was truncated by the default budget"
    );
}
