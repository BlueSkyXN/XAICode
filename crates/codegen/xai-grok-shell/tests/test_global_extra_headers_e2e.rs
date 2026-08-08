//! End-to-end test for the global `[models]` defaults.
//!
//! Runs the built grok binary against the mock inference server with a
//! caller-owned `$GROK_HOME` whose `config.toml` sets every global `[models]`
//! default. Asserts the turn succeeds with all of them set and that the
//! wire-observable one — `extra_headers` — reaches the `/v1/chat/completions`
//! request header, for a model with no per-model `[model.<id>]` override.
//!
//! The scalar defaults (temperature, top_p, max_completion_tokens, max_retries,
//! inference_idle_timeout_secs, stream_tool_calls) are exercised here to prove
//! they parse and the turn still completes; their resolution onto the model is
//! covered directly by `config.rs` unit tests. The headless turn does not
//! surface sampling params in the chat-completions body, so they are not
//! wire-asserted here.
//!
//! `#[ignore]` (needs a built binary). Run locally (auto-builds the pager):
//! ```bash
//! cargo test -p xai-grok-shell --test test_global_extra_headers_e2e -- --ignored
//! ```

use std::sync::atomic::Ordering;
use std::time::Duration;

use xai_grok_test_support::*;

const PROVIDER_MODEL: &str = "clean-provider-model";
const PROVIDER_KEY: &str = "clean-provider-test-key";

fn configure_clean_provider(
    sandbox: &mut TestSandbox,
    provider_url: &str,
    provider_key: &str,
    proxy_url: &str,
) {
    for proxy_var in [
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "ALL_PROXY",
        "http_proxy",
        "https_proxy",
        "all_proxy",
    ] {
        sandbox.set_env(proxy_var, proxy_url);
    }
    sandbox
        .set_env("XAICODE_PROVIDER_KEY", provider_key)
        .set_env("XAICODE_TENANT_HEADER", " tenant-from-env ");

    std::fs::write(
        sandbox.grok_home().join("config.toml"),
        format!(
            r#"[models]
default = "clean-provider"

[model.clean-provider]
model = "{PROVIDER_MODEL}"
base_url = "{provider_url}?keep=base&replace=old"
env_key = ["XAICODE_MISSING_KEY", "XAICODE_PROVIDER_KEY"]
api_backend = "responses"
auth_scheme = "bearer"
context_window = 200000
max_retries = 0
extra_headers = {{ "X-Client-Mode" = "clean-e2e" }}
query_params = {{ "api-version" = "2026-08-08", replace = "configured" }}
env_http_headers = {{ "X-Tenant" = "XAICODE_TENANT_HEADER" }}
"#,
        ),
    )
    .expect("write clean provider config");
}

async fn run_clean_provider(sandbox: &TestSandbox, output_format: &str) -> HeadlessResult {
    let mut cmd = tokio::process::Command::new(grok_binary());
    cmd.args([
        "-p",
        "say hi",
        "--yolo",
        "--model",
        "clean-provider",
        "--max-turns",
        "1",
        "--output-format",
        output_format,
    ])
    .arg("--cwd")
    .arg(sandbox.workspace())
    .current_dir(sandbox.workspace())
    .stdin(std::process::Stdio::null())
    .stdout(std::process::Stdio::piped())
    .stderr(std::process::Stdio::piped())
    .kill_on_drop(true);
    run_headless_in_sandbox_borrowed(cmd, sandbox).await
}

async fn run_clean_provider_resume(sandbox: &TestSandbox, session_id: &str) -> HeadlessResult {
    let mut cmd = tokio::process::Command::new(grok_binary());
    cmd.args([
        "-p",
        "say hi after reopen",
        "--resume",
        session_id,
        "--yolo",
        "--model",
        "clean-provider",
        "--max-turns",
        "1",
        "--output-format",
        "json",
    ])
    .arg("--cwd")
    .arg(sandbox.workspace())
    .current_dir(sandbox.workspace())
    .stdin(std::process::Stdio::null())
    .stdout(std::process::Stdio::piped())
    .stderr(std::process::Stdio::piped())
    .kill_on_drop(true);
    run_headless_in_sandbox_borrowed(cmd, sandbox).await
}

async fn run_local_command(sandbox: &TestSandbox, args: &[&str]) -> std::process::Output {
    let mut cmd = tokio::process::Command::new(grok_binary());
    cmd.args(args)
        .current_dir(sandbox.workspace())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    sandbox.apply_to_tokio_command(&mut cmd);
    cmd.output().await.expect("run local session command")
}

fn persisted_session_dirs(grok_home: &std::path::Path) -> Vec<std::path::PathBuf> {
    let sessions = grok_home.join("sessions");
    let mut found = Vec::new();
    let Ok(cwd_dirs) = std::fs::read_dir(sessions) else {
        return found;
    };
    for cwd_dir in cwd_dirs.flatten().filter(|entry| entry.path().is_dir()) {
        let Ok(session_dirs) = std::fs::read_dir(cwd_dir.path()) else {
            continue;
        };
        found.extend(
            session_dirs
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| path.join("summary.json").is_file()),
        );
    }
    found.sort();
    found
}

/// Every global `[models]` default is accepted, and the wire-observable
/// `extra_headers` reaches the inference request with no per-model block in play.
#[tokio::test]
#[ignore] // requires pre-built binary; run with --ignored
async fn global_models_config_reaches_inference_request() {
    let server = MockInferenceServer::start()
        .await
        .expect("start mock server");
    let workdir = git_workdir();
    let sandbox = TestSandbox::builder().mock_url(server.url()).build();

    let grok_home = sandbox.grok_home().to_path_buf();
    std::fs::write(
        grok_home.join("config.toml"),
        r#"[models]
extra_headers = { "X-Request-Tags" = "team=example,env=prod" }
temperature = 0.5
top_p = 0.25
max_completion_tokens = 4096
max_retries = 7
inference_idle_timeout_secs = 600
stream_tool_calls = true
"#,
    )
    .expect("write config.toml");

    let mut cmd = tokio::process::Command::new(grok_binary());
    cmd.args(["-p", "say hi", "--yolo", "--output-format", "json"])
        .arg("--cwd")
        .arg(workdir.workspace())
        .current_dir(workdir.workspace())
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);

    let result = run_headless_in_sandbox(cmd, sandbox).await;
    assert_headless_success(&result, "global models config e2e", Some(&server));

    let requests = server.requests();
    let chat = requests
        .iter()
        .find(|e| e.method == "POST" && e.path.contains("chat/completions"))
        .unwrap_or_else(|| {
            panic!(
                "no POST /v1/chat/completions request logged; requests:\n{}",
                server.request_log_summary()
            )
        });
    assert_eq!(
        chat.header("x-request-tags"),
        Some("team=example,env=prod"),
        "global [models].extra_headers must reach the request header; requests:\n{}",
        server.request_log_summary()
    );
}

/// A clean production-style process uses only the selected custom provider.
/// It preserves ordered env-key auth, headers and streaming, while a proxy
/// trap observes zero auxiliary/vendor connections. Query folding is asserted
/// by the sampler's request_query_and_headers integration test.
#[tokio::test]
#[ignore] // requires pre-built binary; run with --ignored
async fn clean_custom_provider_is_the_only_egress_and_preserves_request_options() {
    let provider = MockInferenceServer::start_with_required_auth(
        vec![MockModelEntry::new(PROVIDER_MODEL).with_api_backend("responses")],
        PROVIDER_KEY,
    )
    .await
    .expect("start provider");
    let (trap_url, trap_accepts, _trap_heads) = spawn_counting_server().await;
    let proxy_url = trap_url.trim_end_matches("/v1");
    let mut sandbox = TestSandbox::builder().git().build();
    configure_clean_provider(&mut sandbox, &provider.url(), PROVIDER_KEY, proxy_url);

    let inspected = run_local_command(&sandbox, &["inspect", "--json"]).await;
    assert!(
        inspected.status.success(),
        "inspect failed:\n{}",
        String::from_utf8_lossy(&inspected.stderr)
    );
    let inspection: serde_json::Value =
        serde_json::from_slice(&inspected.stdout).expect("inspect JSON");
    assert!(
        inspection
            .get("configWarnings")
            .and_then(serde_json::Value::as_array)
            .is_none_or(Vec::is_empty),
        "generic custom-provider fields must all be recognized: {inspection}"
    );

    let result = run_clean_provider(&sandbox, "streaming-json").await;
    assert_headless_success(&result, "clean custom provider", Some(&provider));

    let events: Vec<serde_json::Value> = result
        .stdout
        .lines()
        .map(|line| serde_json::from_str(line).expect("valid streaming-json line"))
        .collect();
    assert_eq!(
        events
            .last()
            .and_then(|event| event.get("type"))
            .and_then(serde_json::Value::as_str),
        Some("end"),
        "custom-provider streaming output must terminate cleanly: {events:?}"
    );

    let requests = provider.requests();
    assert!(!requests.is_empty(), "provider received no requests");
    assert!(
        requests
            .iter()
            .all(|request| request.method == "POST" && request.path.starts_with("/v1/responses")),
        "only inference requests may reach the selected provider:\n{}",
        provider.request_log_summary()
    );
    let inference = requests
        .iter()
        .find(|request| {
            request
                .body
                .as_ref()
                .and_then(|body| body.get("model"))
                .and_then(serde_json::Value::as_str)
                == Some(PROVIDER_MODEL)
        })
        .expect("selected-model responses request");
    assert_eq!(
        inference.authorization.as_deref(),
        Some("Bearer clean-provider-test-key")
    );
    assert_eq!(inference.header("x-client-mode"), Some("clean-e2e"));
    assert_eq!(inference.header("x-tenant"), Some("tenant-from-env"));
    assert_eq!(
        inference
            .body
            .as_ref()
            .and_then(|body| body.get("model"))
            .and_then(serde_json::Value::as_str),
        Some(PROVIDER_MODEL)
    );
    assert!(
        !inference.headers.iter().any(|(name, _)| {
            name.starts_with("x-grok-") || name.starts_with("x-xai-") || name == "x-xai-token-auth"
        }),
        "vendor marker header reached custom provider: {:?}",
        inference.headers
    );
    drop(requests);

    let session_dirs = persisted_session_dirs(sandbox.grok_home());
    assert_eq!(
        session_dirs.len(),
        1,
        "one headless turn must create one local session: {session_dirs:?}"
    );
    let session_id = session_dirs[0]
        .file_name()
        .and_then(|name| name.to_str())
        .expect("UTF-8 session id")
        .to_owned();
    assert!(session_dirs[0].join("updates.jsonl").is_file());

    let listed = run_local_command(&sandbox, &["sessions", "list"]).await;
    assert!(
        listed.status.success(),
        "sessions list failed:\n{}",
        String::from_utf8_lossy(&listed.stderr)
    );
    assert!(
        String::from_utf8_lossy(&listed.stdout).contains(&session_id),
        "sessions list did not reopen the temporary home:\n{}",
        String::from_utf8_lossy(&listed.stdout)
    );

    let searched = run_local_command(&sandbox, &["sessions", "search", "say hi"]).await;
    assert!(
        searched.status.success(),
        "sessions search failed:\n{}",
        String::from_utf8_lossy(&searched.stderr)
    );
    assert!(
        String::from_utf8_lossy(&searched.stdout).contains(&session_id),
        "sessions search did not find the persisted prompt:\n{}",
        String::from_utf8_lossy(&searched.stdout)
    );

    let resumed = run_clean_provider_resume(&sandbox, &session_id).await;
    assert_headless_success(&resumed, "clean custom provider resume", Some(&provider));
    assert_eq!(
        persisted_session_dirs(sandbox.grok_home()),
        session_dirs,
        "resume must reopen the existing session instead of creating a replacement"
    );

    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(
        trap_accepts.load(Ordering::SeqCst),
        0,
        "startup, turn and shutdown must make no auxiliary/vendor connection"
    );
}

/// A provider 401 is terminal for this run: XAICode reports failure and does
/// not fall back to an account login, first-party endpoint or auxiliary sink.
#[tokio::test]
#[ignore] // requires pre-built binary; run with --ignored
async fn clean_custom_provider_error_does_not_fall_back_or_egress() {
    let provider = MockInferenceServer::start_with_required_auth(
        vec![MockModelEntry::new(PROVIDER_MODEL).with_api_backend("responses")],
        "different-required-key",
    )
    .await
    .expect("start provider");
    let (trap_url, trap_accepts, _trap_heads) = spawn_counting_server().await;
    let proxy_url = trap_url.trim_end_matches("/v1");
    let mut sandbox = TestSandbox::builder().git().build();
    configure_clean_provider(&mut sandbox, &provider.url(), PROVIDER_KEY, proxy_url);

    let result = run_clean_provider(&sandbox, "json").await;
    assert!(
        !result.timed_out && !result.status.success(),
        "provider auth failure must exit non-zero\nstdout:\n{}\nstderr:\n{}",
        result.stdout,
        result.stderr
    );
    let requests = provider.requests();
    assert!(
        requests
            .iter()
            .any(|request| request.path.starts_with("/v1/responses")),
        "provider error path was not exercised:\n{}",
        provider.request_log_summary()
    );
    assert!(
        requests
            .iter()
            .all(|request| request.path.starts_with("/v1/responses")),
        "provider error must not trigger hosted side requests:\n{}",
        provider.request_log_summary()
    );
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(
        trap_accepts.load(Ordering::SeqCst),
        0,
        "provider error must not fall back to any auxiliary/vendor endpoint"
    );
}
