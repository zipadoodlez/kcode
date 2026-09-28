use super::*;
use crate::cli::provider_init::ProviderChoice;

#[test]
fn credential_import_cli_requires_stdin_and_preserves_explicit_provider() {
    for provider in ["openai", "claude"] {
        let args = Args::try_parse_from([
            "kcode",
            "auth",
            "import",
            "--provider",
            provider,
            "--stdin",
            "--json",
        ])
        .unwrap();
        assert_eq!(args.provider.as_arg_value(), provider);
        assert!(matches!(
            args.command,
            Some(Command::Auth(AuthCommand::Import {
                stdin: true,
                json: true
            }))
        ));
    }
    for args in [
        vec!["kcode", "auth", "import", "--provider", "openai"],
        vec!["kcode", "auth", "import", "--stdin", "--token", "secret"],
        vec!["kcode", "auth", "import", "--stdin", "--overwrite"],
    ] {
        assert!(Args::try_parse_from(args).is_err());
    }
}

#[test]
fn native_ssh_attach_arguments_parse_and_do_not_steal_local_socket() {
    let args = Args::try_parse_from([
        "kcode",
        "--ssh",
        "dev",
        "--ssh-binary",
        "/opt/kcode",
        "--ssh-server-socket",
        "/run/native/kcode.sock",
        "--remote-working-dir",
        "/srv/project",
        "self-dev",
    ])
    .unwrap();
    assert_eq!(args.ssh.as_deref(), Some("dev"));
    assert_eq!(args.ssh_binary.as_deref(), Some("/opt/kcode"));
    assert_eq!(
        args.ssh_server_socket.as_deref(),
        Some("/run/native/kcode.sock")
    );
    assert_eq!(args.remote_working_dir.as_deref(), Some("/srv/project"));
    assert!(args.socket.is_none());
    for argv in [
        vec!["kcode", "--ssh", "dev", "--socket", "/local.sock"],
        vec!["kcode", "--ssh-binary", "/opt/kcode"],
        vec!["kcode", "--ssh-server-socket", "/remote.sock"],
    ] {
        assert!(Args::try_parse_from(argv).is_err());
    }
}

#[test]
fn native_server_stdio_preserves_socket_override() {
    let args =
        Args::try_parse_from(["kcode", "--socket", "/run/native.sock", "server", "stdio"]).unwrap();
    assert_eq!(args.socket.as_deref(), Some("/run/native.sock"));
    assert!(matches!(
        args.command,
        Some(Command::Server {
            action: ServerCommand::Stdio
        })
    ));
}

#[test]
fn server_start_and_internal_keepalive_parse() {
    let args = Args::try_parse_from(["kcode", "server", "start", "--json"])
        .expect("server start should parse");
    assert!(matches!(
        args.command,
        Some(Command::Server {
            action: ServerCommand::Start { json: true }
        })
    ));

    let keepalive = Args::try_parse_from(["kcode", "server", "keepalive"])
        .expect("internal server keepalive should parse");
    assert!(matches!(
        keepalive.command,
        Some(Command::Server {
            action: ServerCommand::Keepalive
        })
    ));
}

#[test]
fn test_provider_choice_aliases_parse() {
    let args = Args::try_parse_from(["kcode", "--provider", "z.ai", "run", "smoke"]).unwrap();
    assert_eq!(args.provider, ProviderChoice::Zai);

    let args =
        Args::try_parse_from(["kcode", "--provider", "kimi-for-coding", "run", "smoke"]).unwrap();
    assert_eq!(args.provider, ProviderChoice::Kimi);

    let args =
        Args::try_parse_from(["kcode", "--provider", "cerebrascode", "run", "smoke"]).unwrap();
    assert_eq!(args.provider, ProviderChoice::Cerebras);

    let args = Args::try_parse_from(["kcode", "--provider", "compat", "run", "smoke"]).unwrap();
    assert_eq!(args.provider, ProviderChoice::OpenaiCompatible);

    let args = Args::try_parse_from(["kcode", "--provider", "bailian", "run", "smoke"]).unwrap();
    assert_eq!(args.provider, ProviderChoice::AlibabaCodingPlan);

    let args = Args::try_parse_from(["kcode", "--provider", "together", "run", "smoke"]).unwrap();
    assert_eq!(args.provider, ProviderChoice::TogetherAi);

    let args = Args::try_parse_from(["kcode", "--provider", "grok", "run", "smoke"]).unwrap();
    assert_eq!(args.provider, ProviderChoice::Xai);

    let args = Args::try_parse_from(["kcode", "--provider", "grok-build"]).unwrap();
    assert_eq!(args.provider, ProviderChoice::GrokBuild);

    let args = Args::try_parse_from(["kcode", "--provider", "cgc", "run", "smoke"]).unwrap();
    assert_eq!(args.provider, ProviderChoice::Comtegra);
}

/// Four providers used to expose clap's implicit kebab case as the `-p` value
/// while `as_arg_value()` (and the registry id) said otherwise, so `provider
/// list` printed an id that did not parse. The canonical spelling must parse,
/// and the spelling that used to work must keep working as an alias.
#[test]
fn provider_choice_value_names_match_as_arg_value_and_keep_old_spellings() {
    for (canonical, previous, expected) in [
        ("302ai", "ai302", ProviderChoice::Ai302),
        ("huggingface", "hugging-face", ProviderChoice::HuggingFace),
        ("moonshotai", "moonshot-ai", ProviderChoice::MoonshotAi),
        ("togetherai", "together-ai", ProviderChoice::TogetherAi),
    ] {
        assert_eq!(expected.as_arg_value(), canonical);
        for spelling in [canonical, previous] {
            let args = Args::try_parse_from(["kcode", "--provider", spelling]).unwrap();
            assert_eq!(
                args.provider, expected,
                "`-p {spelling}` should select {expected:?}"
            );
        }
    }
}

#[test]
fn serve_server_name_option_parses() {
    let args =
        Args::try_parse_from(["kcode", "serve", "--server-name", "mount-cloud/fabian"]).unwrap();
    match args.command {
        Some(Command::Serve { server_name, .. }) => {
            assert_eq!(server_name.as_deref(), Some("mount-cloud/fabian"));
        }
        other => panic!("unexpected command: {:?}", other),
    }
}

#[test]
fn remote_working_dir_option_parses() {
    let args = Args::try_parse_from([
        "kcode",
        "--socket",
        "/tmp/kcode.sock",
        "--remote-working-dir",
        "/home/agent/project",
    ])
    .unwrap();

    assert_eq!(
        args.remote_working_dir.as_deref(),
        Some("/home/agent/project")
    );
}

#[test]
fn model_list_subcommand_parses() {
    let args = Args::try_parse_from(["kcode", "model", "list", "--json", "--verbose"]).unwrap();
    match args.command {
        Some(Command::Model(ModelCommand::List { json, verbose })) => {
            assert!(json);
            assert!(verbose);
        }
        other => panic!("unexpected command: {:?}", other),
    }
}

#[test]
fn session_rename_subcommand_parses() {
    let args = Args::try_parse_from([
        "kcode",
        "session",
        "rename",
        "fox",
        "release planning",
        "--json",
    ])
    .unwrap();
    match args.command {
        Some(Command::Session(SessionCommand::Rename {
            session,
            name,
            clear,
            json,
        })) => {
            assert_eq!(session, "fox");
            assert_eq!(name.as_deref(), Some("release planning"));
            assert!(!clear);
            assert!(json);
        }
        other => panic!("unexpected command: {:?}", other),
    }

    let args = Args::try_parse_from(["kcode", "session", "rename", "fox", "--clear"]).unwrap();
    match args.command {
        Some(Command::Session(SessionCommand::Rename {
            session,
            name,
            clear,
            json,
        })) => {
            assert_eq!(session, "fox");
            assert!(name.is_none());
            assert!(clear);
            assert!(!json);
        }
        other => panic!("unexpected command: {:?}", other),
    }
}

#[test]
fn login_no_browser_flag_parses() {
    let args = Args::try_parse_from(["kcode", "login", "--no-browser"]).unwrap();
    match args.command {
        Some(Command::Login {
            provider,
            account,
            no_browser,
            print_auth_url,
            callback_url,
            auth_code,
            json,
            complete,
            api_base,
            api_key,
            api_key_env,
            no_validate,
            flow_id,
            cancel,
        }) => {
            assert!(provider.is_none());
            assert!(account.is_none());
            assert!(no_browser);
            assert!(!print_auth_url);
            assert!(callback_url.is_none());
            assert!(auth_code.is_none());
            assert!(!json);
            assert!(!complete);
            assert!(api_base.is_none());
            assert!(api_key.is_none());
            assert!(api_key_env.is_none());
            assert!(!no_validate);
            assert!(flow_id.is_none());
            assert!(!cancel);
        }
        other => panic!("unexpected command: {:?}", other),
    }

    let args = Args::try_parse_from(["kcode", "login", "--headless"]).unwrap();
    match args.command {
        Some(Command::Login { no_browser, .. }) => assert!(no_browser),
        other => panic!("unexpected command: {:?}", other),
    }
}

#[test]
fn login_accepts_provider_positional() {
    let args = Args::try_parse_from(["kcode", "login", "gemini"]).unwrap();
    match args.command {
        Some(Command::Login { provider, .. }) => {
            assert_eq!(provider, Some(ProviderChoice::Gemini));
        }
        other => panic!("unexpected command: {:?}", other),
    }
}

#[test]
fn login_scoped_flow_flags_parse_and_reject_unsafe_ids() {
    for action in ["--print-auth-url", "--complete", "--cancel"] {
        let args = Args::try_parse_from([
            "kcode",
            "login",
            "--provider",
            "openai",
            "--flow-id",
            "aB_09-safe",
            action,
            "--json",
        ])
        .unwrap();
        assert!(
            matches!(args.command, Some(Command::Login { flow_id: Some(id), .. }) if id == "aB_09-safe")
        );
    }
    for id in [
        "", ".", "..", "../other", "a/b", "a\\b", "a b", "a\n", "é", "a.json", "%2f", "x;touch",
    ] {
        assert!(
            Args::try_parse_from(["kcode", "login", "--flow-id", id, "--print-auth-url"]).is_err(),
            "accepted {id:?}"
        );
    }
    assert!(Args::try_parse_from(["kcode", "login", "--flow-id", &"a".repeat(65)]).is_err());
    assert!(Args::try_parse_from(["kcode", "login", "--flow-id", &"a".repeat(64)]).is_ok());
    assert!(Args::try_parse_from(["kcode", "login", "openai", "--cancel"]).is_err());
    for action in ["--print-auth-url", "--complete"] {
        assert!(
            Args::try_parse_from([
                "kcode",
                "login",
                "openai",
                "--flow-id",
                "safe",
                "--cancel",
                action
            ])
            .is_err()
        );
    }
    for input in ["--callback-url", "--auth-code"] {
        assert!(
            Args::try_parse_from([
                "kcode",
                "login",
                "openai",
                "--flow-id",
                "safe",
                "--cancel",
                input,
                "-"
            ])
            .is_err()
        );
        assert!(
            Args::try_parse_from(["kcode", "login", "openai", "--flow-id", "safe", input, "-"])
                .is_ok()
        );
    }
}

#[test]
fn login_openai_compatible_scriptable_flags_parse() {
    let args = Args::try_parse_from([
        "kcode",
        "--provider",
        "openai-compatible",
        "--model",
        "deepseek-v4-flash",
        "login",
        "--api-base",
        "https://api.deepseek.com",
        "--api-key-env",
        "DEEPSEEK_API_KEY",
    ])
    .unwrap();
    assert_eq!(args.provider, ProviderChoice::OpenaiCompatible);
    assert_eq!(args.model.as_deref(), Some("deepseek-v4-flash"));
    match args.command {
        Some(Command::Login {
            api_base,
            api_key_env,
            ..
        }) => {
            assert_eq!(api_base.as_deref(), Some("https://api.deepseek.com"));
            assert_eq!(api_key_env.as_deref(), Some("DEEPSEEK_API_KEY"));
        }
        other => panic!("unexpected command: {:?}", other),
    }
}

#[test]
fn login_openai_compatible_accepts_global_provider_and_model_after_subcommand() {
    let args = Args::try_parse_from([
        "kcode",
        "login",
        "--provider",
        "openai-compatible",
        "--api-base",
        "https://api.deepseek.com",
        "--model",
        "deepseek-v4-flash",
    ])
    .unwrap();

    assert_eq!(args.provider, ProviderChoice::OpenaiCompatible);
    assert_eq!(args.model.as_deref(), Some("deepseek-v4-flash"));
    match args.command {
        Some(Command::Login { api_base, .. }) => {
            assert_eq!(api_base.as_deref(), Some("https://api.deepseek.com"));
        }
        other => panic!("unexpected command: {:?}", other),
    }
}

#[test]
fn login_scriptable_flags_parse() {
    let args = Args::try_parse_from(["kcode", "login", "--print-auth-url", "--json"]).unwrap();
    match args.command {
        Some(Command::Login {
            print_auth_url,
            json,
            callback_url,
            auth_code,
            complete,
            ..
        }) => {
            assert!(print_auth_url);
            assert!(json);
            assert!(callback_url.is_none());
            assert!(auth_code.is_none());
            assert!(!complete);
        }
        other => panic!("unexpected command: {:?}", other),
    }

    let args = Args::try_parse_from([
        "kcode",
        "login",
        "--callback-url",
        "http://localhost:1455/auth/callback?code=x&state=y",
    ])
    .unwrap();
    match args.command {
        Some(Command::Login { callback_url, .. }) => {
            assert_eq!(
                callback_url.as_deref(),
                Some("http://localhost:1455/auth/callback?code=x&state=y")
            );
        }
        other => panic!("unexpected command: {:?}", other),
    }

    let args = Args::try_parse_from(["kcode", "login", "--auth-code", "abc123"]).unwrap();
    match args.command {
        Some(Command::Login { auth_code, .. }) => {
            assert_eq!(auth_code.as_deref(), Some("abc123"));
        }
        other => panic!("unexpected command: {:?}", other),
    }

    let args = Args::try_parse_from([
        "kcode",
        "login",
        "--complete",
        "--google-access-tier",
        "readonly",
    ])
    .unwrap();
    match args.command {
        Some(Command::Login { complete, .. }) => {
            assert!(complete);
        }
        other => panic!("unexpected command: {:?}", other),
    }
}

#[test]
fn quiet_global_flag_parses() {
    let args = Args::try_parse_from(["kcode", "--quiet", "model", "list"]).unwrap();
    assert!(args.quiet);
}

#[test]
fn acp_subcommand_parses() {
    let args = Args::try_parse_from(["kcode", "acp"]).unwrap();
    match args.command {
        Some(Command::Acp) => {}
        other => panic!("unexpected command: {:?}", other),
    }
}

#[test]
fn run_json_subcommand_parses() {
    let args = Args::try_parse_from(["kcode", "run", "--json", "hello"]).unwrap();
    match args.command {
        Some(Command::Run {
            json,
            ndjson,
            message,
        }) => {
            assert!(json);
            assert!(!ndjson);
            assert_eq!(message, "hello");
        }
        other => panic!("unexpected command: {:?}", other),
    }
}

#[test]
fn run_ndjson_subcommand_parses() {
    let args = Args::try_parse_from(["kcode", "run", "--ndjson", "hello"]).unwrap();
    match args.command {
        Some(Command::Run {
            json,
            ndjson,
            message,
        }) => {
            assert!(!json);
            assert!(ndjson);
            assert_eq!(message, "hello");
        }
        other => panic!("unexpected command: {:?}", other),
    }
}

#[test]
fn version_subcommand_parses() {
    let args = Args::try_parse_from(["kcode", "version", "--json"]).unwrap();
    match args.command {
        Some(Command::Version { json }) => assert!(json),
        other => panic!("unexpected command: {:?}", other),
    }
}

#[test]
fn usage_subcommand_parses() {
    let args = Args::try_parse_from(["kcode", "usage", "--json"]).unwrap();
    match args.command {
        Some(Command::Usage { json }) => assert!(json),
        other => panic!("unexpected command: {:?}", other),
    }
}

#[test]
fn auth_status_subcommand_parses() {
    let args = Args::try_parse_from(["kcode", "auth", "status", "--json"]).unwrap();
    match args.command {
        Some(Command::Auth(AuthCommand::Status { json })) => assert!(json),
        other => panic!("unexpected command: {:?}", other),
    }
}

#[test]
fn auth_doctor_subcommand_parses() {
    let args = Args::try_parse_from(["kcode", "auth", "doctor", "openai", "--validate", "--json"])
        .unwrap();
    match args.command {
        Some(Command::Auth(AuthCommand::Doctor {
            provider,
            validate,
            json,
        })) => {
            assert_eq!(provider.as_deref(), Some("openai"));
            assert!(validate);
            assert!(json);
        }
        other => panic!("unexpected command: {:?}", other),
    }
}

#[test]
fn provider_list_subcommand_parses() {
    let args = Args::try_parse_from(["kcode", "provider", "list", "--json"]).unwrap();
    match args.command {
        Some(Command::Provider(ProviderCommand::List { json })) => assert!(json),
        other => panic!("unexpected command: {:?}", other),
    }
}

#[test]
fn provider_current_subcommand_parses() {
    let args = Args::try_parse_from(["kcode", "provider", "current", "--json"]).unwrap();
    match args.command {
        Some(Command::Provider(ProviderCommand::Current { json })) => assert!(json),
        other => panic!("unexpected command: {:?}", other),
    }
}

#[test]
fn provider_add_subcommand_parses_agent_friendly_flags() {
    let args = Args::try_parse_from([
        "kcode",
        "provider",
        "add",
        "my-api",
        "--base-url",
        "https://llm.example.com/v1",
        "--model",
        "model-a",
        "--context-window",
        "128000",
        "--api-key-stdin",
        "--auth",
        "bearer",
        "--set-default",
        "--json",
    ])
    .unwrap();

    match args.command {
        Some(Command::Provider(ProviderCommand::Add {
            name,
            base_url,
            model,
            context_window,
            api_key_stdin,
            auth,
            set_default,
            json,
            ..
        })) => {
            assert_eq!(name, "my-api");
            assert_eq!(base_url, "https://llm.example.com/v1");
            assert_eq!(model, "model-a");
            assert_eq!(context_window, Some(128000));
            assert!(api_key_stdin);
            assert_eq!(auth, Some(ProviderAuthArg::Bearer));
            assert!(set_default);
            assert!(json);
        }
        other => panic!("unexpected command: {:?}", other),
    }
}

#[test]
fn restart_save_subcommand_parses() {
    let args = Args::try_parse_from(["kcode", "restart", "save"]).unwrap();
    match args.command {
        Some(Command::Restart {
            action: RestartCommand::Save {
                auto_restore: false,
            },
        }) => {}
        other => panic!("unexpected command: {:?}", other),
    }
}

#[test]
fn restart_save_auto_restore_flag_parses() {
    let args = Args::try_parse_from(["kcode", "restart", "save", "--auto-restore"]).unwrap();
    match args.command {
        Some(Command::Restart {
            action: RestartCommand::Save { auto_restore: true },
        }) => {}
        other => panic!("unexpected command: {:?}", other),
    }
}

/// Contract test for the onboarding agent-repair brief (see
/// `kcode-tui::tui::app::onboarding_repair::build_repair_brief`). The brief
/// tells a coding agent to run these exact commands to diagnose and fix a
/// failed login. If any flag here stops parsing, the brief would hand the agent
/// a broken command, so this guards the agent-facing CLI contract.
#[test]
fn onboarding_repair_brief_commands_are_valid_cli() {
    // Diagnose.
    Args::try_parse_from(["kcode", "auth-test", "--provider", "openai", "--json"])
        .expect("auth-test --provider --json must parse");
    Args::try_parse_from(["kcode", "auth-test", "--all-configured", "--json"])
        .expect("auth-test --all-configured --json must parse");
    Args::try_parse_from(["kcode", "auth", "doctor"]).expect("auth doctor must parse");

    // Fix: OAuth and API-key logins.
    Args::try_parse_from(["kcode", "login", "--provider", "openai"])
        .expect("login --provider must parse");
    Args::try_parse_from(["kcode", "login", "--provider", "openai", "--api-key", "k"])
        .expect("login --provider --api-key must parse");

    // Fix: custom OpenAI-compatible endpoint via provider add + key on stdin.
    Args::try_parse_from([
        "kcode",
        "provider",
        "add",
        "my-endpoint",
        "--base-url",
        "https://api.example.com/v1",
        "--model",
        "some-model",
        "--api-key-stdin",
    ])
    .expect("provider add --base-url --model --api-key-stdin must parse");
}
