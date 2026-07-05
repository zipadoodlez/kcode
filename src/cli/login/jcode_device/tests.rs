use super::*;
use std::io::{Read, Write};

/// Spawn a local HTTP server that answers successive requests with the given
/// scripted `(status, body)` responses, then exits. Returns the base URL
/// (with a `/v1` suffix, mirroring how the model API base is configured).
fn spawn_scripted_http_server(responses: Vec<(u16, String)>) -> String {
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("bind test server");
    let addr = listener.local_addr().expect("local addr");
    std::thread::spawn(move || {
        for (status, body) in responses {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let mut buf = [0u8; 4096];
            let _ = stream.read(&mut buf);
            let status_text = match status {
                200 => "OK",
                202 => "Accepted",
                400 => "Bad Request",
                404 => "Not Found",
                410 => "Gone",
                429 => "Too Many Requests",
                _ => "OK",
            };
            let response = format!(
                "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                status,
                status_text,
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes());
        }
    });
    format!("http://127.0.0.1:{}/v1", addr.port())
}

fn test_client() -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .build()
        .expect("build test client")
}

#[test]
fn strip_v1_suffix_derives_auth_base() {
    assert_eq!(
        strip_v1_suffix("https://api.solosystems.dev/v1"),
        "https://api.solosystems.dev"
    );
    assert_eq!(
        strip_v1_suffix("https://api.solosystems.dev/v1/"),
        "https://api.solosystems.dev"
    );
    assert_eq!(
        strip_v1_suffix("https://api.solosystems.dev"),
        "https://api.solosystems.dev"
    );
    assert_eq!(
        strip_v1_suffix("https://example.com/router/v1"),
        "https://example.com/router"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn device_code_request_parses_response() {
    let base = spawn_scripted_http_server(vec![(
        200,
        r#"{"device_code":"dc-123","verify_url":"https://verify.example/dc-123","expires_in":600,"interval":2}"#
            .to_string(),
    )]);
    let auth_base = strip_v1_suffix(&base);

    let device = request_device_code(&test_client(), &auth_base, "user@example.com")
        .await
        .expect("device code response");
    assert_eq!(device.device_code, "dc-123");
    assert_eq!(device.verify_url, "https://verify.example/dc-123");
    assert_eq!(device.expires_in, 600);
    assert_eq!(device.interval, 2);
}

#[tokio::test(flavor = "multi_thread")]
// The env lock is a std Mutex shared with sync tests; holding it across the
// scripted-server awaits is intentional (same pattern as provider_init_tests).
#[allow(clippy::await_holding_lock)]
async fn poll_state_machine_pending_then_approved_persists_key() {
    let _guard = crate::storage::lock_test_env();
    let temp = tempfile::TempDir::new().expect("temp dir");
    let prev_home = std::env::var_os("JCODE_HOME");
    let prev_key = std::env::var_os(crate::subscription_catalog::JCODE_API_KEY_ENV);
    let prev_account = std::env::var_os(crate::subscription_catalog::JCODE_ACCOUNT_ID_ENV);
    let prev_email = std::env::var_os(crate::subscription_catalog::JCODE_ACCOUNT_EMAIL_ENV);
    crate::env::set_var("JCODE_HOME", temp.path());
    crate::env::remove_var(crate::subscription_catalog::JCODE_API_KEY_ENV);
    crate::env::remove_var(crate::subscription_catalog::JCODE_ACCOUNT_ID_ENV);
    crate::env::remove_var(crate::subscription_catalog::JCODE_ACCOUNT_EMAIL_ENV);

    let base = spawn_scripted_http_server(vec![
        (202, String::new()),
        (200, r#"{"status":"pending"}"#.to_string()),
        (
            200,
            r#"{"api_key":"jk-live-abc","account_id":"acct_42","email":"user@example.com","tier":"plus"}"#
                .to_string(),
        ),
    ]);
    let auth_base = strip_v1_suffix(&base);
    let client = test_client();

    // Walk the state machine explicitly: pending -> pending -> approved.
    assert_eq!(
        poll_token_once(&client, &auth_base, "dc-1")
            .await
            .expect("poll 1"),
        PollOutcome::Pending
    );
    assert_eq!(
        poll_token_once(&client, &auth_base, "dc-1")
            .await
            .expect("poll 2"),
        PollOutcome::Pending
    );
    let outcome = poll_token_once(&client, &auth_base, "dc-1")
        .await
        .expect("poll 3");
    let PollOutcome::Approved(state) = outcome else {
        panic!("expected approval, got {:?}", outcome);
    };
    assert_eq!(state.api_key, "jk-live-abc");
    assert_eq!(state.account_id.as_deref(), Some("acct_42"));
    assert_eq!(state.email.as_deref(), Some("user@example.com"));
    assert_eq!(state.tier.as_deref(), Some("plus"));

    persist_subscription_credentials(&state).expect("persist credentials");

    let env_path = crate::storage::app_config_dir()
        .expect("config dir")
        .join(crate::subscription_catalog::JCODE_ENV_FILE);
    let content = std::fs::read_to_string(&env_path).expect("env file written");
    assert!(content.contains("JCODE_API_KEY=jk-live-abc"), "{content}");
    assert!(content.contains("JCODE_ACCOUNT_ID=acct_42"), "{content}");
    assert!(
        content.contains("JCODE_ACCOUNT_EMAIL=user@example.com"),
        "{content}"
    );
    assert_eq!(
        crate::subscription_catalog::configured_api_key().as_deref(),
        Some("jk-live-abc")
    );

    for (key, value) in [
        ("JCODE_HOME", prev_home),
        (crate::subscription_catalog::JCODE_API_KEY_ENV, prev_key),
        (
            crate::subscription_catalog::JCODE_ACCOUNT_ID_ENV,
            prev_account,
        ),
        (
            crate::subscription_catalog::JCODE_ACCOUNT_EMAIL_ENV,
            prev_email,
        ),
    ] {
        match value {
            Some(value) => crate::env::set_var(key, value),
            None => crate::env::remove_var(key),
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn poll_for_api_key_resolves_after_pending() {
    let base = spawn_scripted_http_server(vec![
        (202, String::new()),
        (
            200,
            r#"{"api_key":"jk-live-xyz","account_id":"acct_7","email":"a@b.c","tier":"flagship"}"#
                .to_string(),
        ),
    ]);
    let auth_base = strip_v1_suffix(&base);

    let state = poll_for_api_key(&test_client(), &auth_base, "dc-2", 1, 30)
        .await
        .expect("approved");
    assert_eq!(state.api_key, "jk-live-xyz");
    assert_eq!(state.account_id.as_deref(), Some("acct_7"));
}

#[tokio::test(flavor = "multi_thread")]
async fn poll_state_machine_expired_token_yields_clear_error() {
    let base = spawn_scripted_http_server(vec![(
        400,
        r#"{"error":"expired_token","error_description":"device code expired"}"#.to_string(),
    )]);
    let auth_base = strip_v1_suffix(&base);

    let outcome = poll_token_once(&test_client(), &auth_base, "dc-3")
        .await
        .expect("classified outcome");
    assert_eq!(outcome, PollOutcome::Expired);
}

#[tokio::test(flavor = "multi_thread")]
async fn poll_for_api_key_expiry_produces_clear_error() {
    let base = spawn_scripted_http_server(vec![(400, r#"{"error":"expired_token"}"#.to_string())]);
    let auth_base = strip_v1_suffix(&base);

    let err = poll_for_api_key(&test_client(), &auth_base, "dc-4", 1, 30)
        .await
        .expect_err("expected expiry error");
    assert!(
        err.to_string().contains("expired"),
        "unexpected error: {err}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn poll_state_machine_denied_yields_denied_outcome() {
    let base = spawn_scripted_http_server(vec![(
        400,
        r#"{"error":"access_denied","error_description":"user rejected the sign-in"}"#.to_string(),
    )]);
    let auth_base = strip_v1_suffix(&base);

    let outcome = poll_token_once(&test_client(), &auth_base, "dc-5")
        .await
        .expect("classified outcome");
    assert_eq!(
        outcome,
        PollOutcome::Denied("user rejected the sign-in".to_string())
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn poll_state_machine_treats_gone_as_expired() {
    let base = spawn_scripted_http_server(vec![(410, String::new())]);
    let auth_base = strip_v1_suffix(&base);

    let outcome = poll_token_once(&test_client(), &auth_base, "dc-6")
        .await
        .expect("classified outcome");
    assert_eq!(outcome, PollOutcome::Expired);
}

#[tokio::test(flavor = "multi_thread")]
async fn poll_state_machine_handles_live_worker_shapes() {
    // The live backend (subscription worker in the private solosystems-backend
    // repo) replies 428 + nested error while pending,
    // and 400 + nested {"error":{"code":"expired_token",...}} on expiry.
    let base = spawn_scripted_http_server(vec![
        (
            428,
            r#"{"error":{"code":"authorization_pending","message":"user has not approved yet"}}"#
                .to_string(),
        ),
        (
            400,
            r#"{"error":{"code":"expired_token","message":"device code is invalid or expired"}}"#
                .to_string(),
        ),
    ]);
    let auth_base = strip_v1_suffix(&base);
    let client = test_client();

    assert_eq!(
        poll_token_once(&client, &auth_base, "dc-7")
            .await
            .expect("pending outcome"),
        PollOutcome::Pending
    );
    assert_eq!(
        poll_token_once(&client, &auth_base, "dc-7")
            .await
            .expect("expired outcome"),
        PollOutcome::Expired
    );
}
