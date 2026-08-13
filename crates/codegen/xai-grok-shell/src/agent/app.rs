use parking_lot::Mutex;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use agent_client_protocol as acp;
use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _, BufReader, simplex};
use tokio::sync::{Mutex as TokioMutex, mpsc};
use tokio::time::Duration;
use tokio_util::compat::{TokioAsyncReadCompatExt as _, TokioAsyncWriteCompatExt as _};
use tracing::{debug, info, warn};

use xai_acp_lib::{
    AcpAgentGatewayReceiver as GatewayReceiver, AcpAgentGatewaySender as GatewaySender,
    LineBufferedRead,
};

use crate::agent::config::{Config as AgentConfig, ModelEntry};
use crate::agent::init::{bootstrap, exit_on_config_error};
use crate::agent::models::{ModelFetchAuth, prefetch_models_blocking};
use crate::agent::mvp_agent::MvpAgent;
use crate::auth::{AuthManager, GrokComConfig};
use crate::leader::protocol::InternalMethod;
use crate::util::grok_home;
use dirs;

const MAX_BUFFER_SIZE: usize = 8 * 1024 * 1024;

use indexmap::IndexMap;

/// Configuration for periodic auto-update checking in leader mode.
///
/// When the leader is running for a long time, it periodically calls `check_fn`
/// to check for updates. The `check_fn` is responsible for both detecting
/// whether a newer version is available **and** downloading/installing it.
/// It returns `true` only when the new binary is on disk and the leader
/// should shut down so the next `connect_or_spawn` picks up the updated binary.
///
/// If the download fails, `check_fn` should return `false` so the leader
/// stays alive and retries on the next interval.
pub struct LeaderAutoUpdateConfig {
    /// Interval between update checks (default: 1 hour).
    pub check_interval: Duration,
    /// Async function that checks for, downloads, and installs an update.
    /// Returns `true` if the update was installed successfully and the leader
    /// should shut down. Returns `false` to stay alive (no update, or download
    /// failed).
    pub check_fn:
        Box<dyn Fn() -> Pin<Box<dyn std::future::Future<Output = bool> + Send>> + Send + Sync>,
}

/// Timeout for a single check_fn call. The check_fn may include both a
/// version check and a binary download, so this must be generous enough to
/// cover large downloads on slow connections. Kept in sync with the artifact
/// download request timeout (20 minutes) so the leader does not abandon a
/// transfer that is still within the HTTP client's budget. If the call takes
/// longer than this, we abandon the attempt and retry on the next interval.
/// The select! with the cancellation token ensures the loop remains
/// responsive to shutdown signals even while waiting.
const AUTO_UPDATE_CHECK_TIMEOUT: Duration = Duration::from_secs(20 * 60);

/// How long the auto-update shutdown waits for session actors to flush
/// before the leader exits. Aliases the shared
/// [`crate::agent::activity::SESSION_FLUSH_GRACE`] so this path and the
/// in-process agent's `/exit` / headless-quit flush cannot drift apart.
const AUTO_UPDATE_FLUSH_GRACE: Duration = crate::agent::activity::SESSION_FLUSH_GRACE;

/// Consecutive busy deferrals after which an installed update proceeds
/// anyway (with the graceful flush). Bounds how long a permanently-"busy"
/// signal — an orphaned parked interaction, a wedged turn — can pin the
/// leader to an old binary: ~24h at the default 1h check interval. Mirrors
/// the bounded-grace semantics of the `RelaunchForUpdate` drain.
const MAX_AUTO_UPDATE_BUSY_DEFERRALS: u32 = 24;

/// Bounded wait for the leader flock when it is held but no socket is bound yet
/// (a spawner mid-handoff, an old-flow client holding the flock across its ~10s
/// spawn window, or a same-version sibling briefly holding it). Exceeds that
/// old-flow window so a legitimately-spawning peer wins the race.
const LEADER_ACQUIRE_TIMEOUT: Duration = Duration::from_secs(15);

/// Run the auto-update checker loop.
///
/// Periodically calls `check_fn` to check for, download, and install updates.
/// If `check_fn` returns `true` (update installed) and the agent is idle,
/// flushes every session actor ([`AgentActivity::flush_all_sessions`]) and
/// then cancels the provided token to trigger a graceful leader shutdown.
/// Connected clients will receive a `ShuttingDown` → `Shutdown` sequence and
/// can seamlessly reconnect to a new leader with the updated binary (via
/// `connect_or_spawn` → `resolve_exe_for_spawn`).
///
/// Idle means BOTH `agent_busy` is false (no IPC client request in flight)
/// AND `activity.is_busy()` is false (no running turn, parked interaction,
/// or live subagent). The second signal covers relay-driven (grok.com
/// WebSocket) leaders, whose traffic bypasses the IPC server and never sets
/// `agent_busy`.
///
/// If `check_fn` returns `true` but the agent is busy, the shutdown is
/// deferred until the next interval when the agent may be idle — bounded by
/// [`MAX_AUTO_UPDATE_BUSY_DEFERRALS`], after which the update proceeds
/// anyway (still flushing first) so a permanently-busy signal (orphaned
/// parked interaction, wedged turn) cannot pin the leader to an old binary
/// forever.
///
/// The `check_fn` call is wrapped in a `select!` with the cancellation token
/// and a timeout so that a stalled download cannot block the loop from
/// responding to shutdown signals.
///
/// This is extracted as a standalone function so it can be unit-tested
/// independently from the full leader infrastructure.
pub(crate) async fn run_auto_update_checker(
    config: LeaderAutoUpdateConfig,
    agent_busy: Arc<AtomicBool>,
    activity: crate::agent::activity::AgentActivity,
    cancel: tokio_util::sync::CancellationToken,
    shutdown_tx: tokio::sync::watch::Sender<crate::leader::ShutdownReason>,
) {
    // The updater crate and all production update checks are removed from the
    // clean build. Keep the helper for historical unit fixtures and API
    // compatibility, but never execute it in a shipped binary.
    if !cfg!(test) {
        let _ = (config, agent_busy, activity, cancel, shutdown_tx);
        return;
    }
    let mut interval = tokio::time::interval(config.check_interval);
    // Skip the first tick (fires immediately)
    interval.tick().await;
    let mut busy_deferrals: u32 = 0;

    loop {
        tokio::select! {
            _ = interval.tick() => {}
            _ = cancel.cancelled() => break,
        }

        info!("Leader auto-update: running update check");

        // Run check_fn inside a select! with cancellation and a timeout so a
        // stalled network call cannot block the loop from responding to shutdown.
        // The check_fn may include a binary download, so the timeout is generous.
        let update_installed = tokio::select! {
            biased;
            _ = cancel.cancelled() => break,
            result = tokio::time::timeout(AUTO_UPDATE_CHECK_TIMEOUT, (config.check_fn)()) => {
                match result {
                    Ok(installed) => installed,
                    Err(_elapsed) => {
                        warn!("Leader auto-update: check/download timed out, will retry next interval");
                        continue;
                    }
                }
            }
        };

        if update_installed {
            let busy = agent_busy.load(Ordering::Relaxed) || activity.is_busy();
            if busy && busy_deferrals < MAX_AUTO_UPDATE_BUSY_DEFERRALS {
                busy_deferrals += 1;
                info!(
                    busy_deferrals,
                    "Leader auto-update: update installed but agent is busy, deferring shutdown"
                );
                continue;
            }
            if busy {
                warn!(
                    busy_deferrals,
                    "Leader auto-update: deferral limit reached while busy; shutting down anyway"
                );
            } else {
                info!("Leader auto-update: update installed and agent is idle, shutting down");
            }
            // Flush session actors BEFORE cancelling — cancellation drops
            // the LocalSet, which aborts actors mid-instruction.
            activity.flush_all_sessions(AUTO_UPDATE_FLUSH_GRACE).await;
            // Signal the shutdown reason BEFORE cancelling so the IPC server reads
            // AutoUpdate when it processes the cancellation.
            let _ = shutdown_tx.send(crate::leader::ShutdownReason::AutoUpdate);
            cancel.cancel();
            break;
        } else {
            info!("Leader auto-update: no update installed");
        }
    }
}

/// Spawn the agent inside a LocalSet and return a handle to the I/O future.
fn spawn_agent_local(
    agent_config: AgentConfig,
    auth_manager: Arc<AuthManager>,
    prefetched_models: Option<IndexMap<String, ModelEntry>>,
    memory_config: Option<crate::config::MemoryConfig>,
    outgoing: impl futures::AsyncWrite + Unpin + 'static,
    incoming: impl futures::AsyncRead + Unpin + 'static,
) -> impl std::future::Future<Output = Result<(), acp::Error>> {
    let (gw_tx, gw_rx) = tokio::sync::mpsc::unbounded_channel();
    let gateway = GatewaySender::new(gw_tx);
    let mut agent = MvpAgent::new(gateway, &agent_config, auth_manager, prefetched_models)
        .unwrap_or_else(exit_on_config_error);
    // The clean build uses only models declared in local config. Do not start
    // the original hosted catalog refresh task.
    if let Some(mc) = memory_config {
        agent.set_memory_config(mc);
    }
    let incoming = LineBufferedRead::spawn_local(incoming);
    let (conn, handle_io) = acp::AgentSideConnection::new(agent, outgoing, incoming, |fut| {
        tokio::task::spawn_local(fut);
    });
    tokio::task::spawn_local(
        GatewayReceiver::new(gw_rx, conn)
            .with_on_meta(xai_file_utils::trace_context::span_from_meta_traceparent)
            .run(),
    );
    handle_io
}

fn internal_reload_request_line(
    id: &str,
    method: InternalMethod,
    params: serde_json::Value,
) -> String {
    crate::leader::protocol::internal_request_line(id, method, params)
}

/// Start a skills file watcher and wire it to inject `x.ai/internal/reload_skills`
/// messages into the shared ACP incoming stream when SKILL.md files change on disk.
///
/// or `None` if no directories could be watched.
fn spawn_skills_file_watcher<W>(
    acp_incoming_tx: &Arc<TokioMutex<W>>,
    skills_paths: &[String],
) -> Option<tokio::task::JoinHandle<()>>
where
    W: tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let cwd = std::env::current_dir().unwrap_or_default();
    let workspace_user_dir = xai_grok_agent::prompt::workspace_user::optional_workspace_user_dir();
    let (mut watcher, mut skills_rx) = crate::config::watcher::SkillsFileWatcher::start(
        Some(cwd.as_path()),
        workspace_user_dir.as_deref(),
        skills_paths,
    )?;
    let skills_tx = acp_incoming_tx.clone();
    let task = tokio::spawn(async move {
        while let Some(change) = skills_rx.recv().await {
            let created_discovery_dir = watcher.refresh_new_discovery_dirs();
            let (id, method) = match change {
                crate::config::watcher::DiscoveryChange::Skills if !created_discovery_dir => {
                    info!("Skill directory changed on disk, reloading skills for all sessions");
                    ("skills-reload", InternalMethod::ReloadSkills)
                }
                crate::config::watcher::DiscoveryChange::Skills => {
                    info!("Discovery directory created on disk, reloading skills and workflows");
                    ("skills-reload", InternalMethod::ReloadSkills)
                }
                crate::config::watcher::DiscoveryChange::Workflows => {
                    info!(
                        "Workflow directory changed on disk, re-advertising commands for all sessions"
                    );
                    ("workflows-reload", InternalMethod::ReloadWorkflows)
                }
            };
            let line = internal_reload_request_line(id, method, serde_json::json!({}));
            let mut tx = skills_tx.lock().await;
            if let Err(e) = tx.write_all(line.as_bytes()).await {
                warn!(
                    error = %e,
                    "failed to inject skills reload into ACP stream"
                );
            }
        }
    });
    Some(task)
}

/// Register the process-lifetime runtime so shared filesystem watchers
/// ([`xai_fsnotify::shared`]) run their event loops on a runtime that outlives
/// individual sessions (each session builds its own short-lived runtime).
/// Idempotent — safe to call from every agent entrypoint.
fn register_fs_watch_runtime() {
    xai_fsnotify::set_runtime_handle(tokio::runtime::Handle::current());
}

pub async fn run_stdio_agent(
    agent_config: &AgentConfig,
    prefetched_models: Option<IndexMap<String, ModelEntry>>,
    memory_config: Option<crate::config::MemoryConfig>,
) -> anyhow::Result<()> {
    register_fs_watch_runtime();
    // A stdio agent is a protocol child speaking over pipes inherited from
    // whoever spawned it (grok-desktop, IDE clients, the agent SDKs, a parent
    // agent's subagent harness) — it is useless without that parent. stdin
    // EOF already triggers shutdown below, but an agent wedged mid-turn (or
    // under thread exhaustion) may never read stdin again; bind to parent
    // death (Linux `PR_SET_PDEATHSIG(SIGTERM)`, no-op elsewhere) so the
    // kernel reaps it instead of leaving an orphan accumulating pid slots on
    // shared hosts. The leader entrypoint intentionally does NOT do this —
    // it is designed to outlive its clients.
    if let Err(error) = xai_tty_utils::kill_current_process_on_parent_death() {
        tracing::warn!(
            %error,
            "failed to bind to parent death; agent will not die with its \
             parent — stdin EOF remains the only cleanup"
        );
    }
    let outgoing = tokio::io::stdout().compat_write();
    // Non-blocking boot: catalog refreshes in the background, not before readiness.
    let agent_config = agent_config.clone();

    // Use a simplex intermediary between stdin and the agent so we can
    // inject internal messages (e.g. skill-reload) alongside real client
    // input. This mirrors the pattern used by `run_leader`.
    let (acp_incoming_rx, acp_incoming_tx) = simplex(MAX_BUFFER_SIZE);
    let incoming = acp_incoming_rx.compat();
    let acp_incoming_tx = Arc::new(TokioMutex::new(acp_incoming_tx));

    // Bridge stdin to the simplex writer. A dedicated OS thread does the
    // blocking stdin reads (see `xai_acp_lib::spawn_stdin_line_reader`): on
    // Windows `tokio::io::stdin()` only delivers buffered lines from a
    // redirected pipe at EOF, so a persistent ACP client (which keeps stdin
    // open) would hang the `initialize` handshake. The forwarder writes each
    // complete line to the simplex so injected internal messages (from the
    // skills watcher) never interleave mid-line with client data.
    let stdin_tx = acp_incoming_tx.clone();
    let (stdin_closed_tx, stdin_closed_rx) = tokio::sync::oneshot::channel();
    let mut stdin_lines = xai_acp_lib::spawn_stdin_line_reader();
    tokio::spawn(async move {
        while let Some(line) = stdin_lines.recv().await {
            let mut tx = stdin_tx.lock().await;
            if tx.write_all(&line).await.is_err() {
                break;
            }
        }
        // Signal that stdin closed. The actual simplex shutdown is performed
        // on the LocalSet so pending ACP request handlers can flush their
        // responses first (they run on the same LocalSet and would be
        // starved by an immediate cross-thread shutdown).
        let _ = stdin_closed_tx.send(());
    });

    let _skills_watcher = spawn_skills_file_watcher(&acp_incoming_tx, &agent_config.skills.paths);

    let local_set = tokio::task::LocalSet::new();
    let result = local_set
        .run_until(async move {
            // Shut down the simplex writer on the LocalSet so it's cooperative with ACP handlers.
            let simplex_tx = acp_incoming_tx;
            tokio::task::spawn_local(async move {
                let _ = stdin_closed_rx.await;
                tokio::time::sleep(Duration::from_millis(100)).await;
                let mut tx = simplex_tx.lock().await;
                let _ = tx.shutdown().await;
            });

            // The manager is retained as the ACP agent's local credential
            // state, but never refreshes or logs into a hosted account.
            let auth_manager = Arc::new(agent_config.create_auth_manager());
            let handle_io = spawn_agent_local(
                agent_config,
                auth_manager,
                prefetched_models,
                memory_config,
                outgoing,
                incoming,
            );
            handle_io.await?;
            Ok::<(), anyhow::Error>(())
        })
        .await;
    // Kill PTY child processes so they don't outlive the agent.
    crate::terminal::pty_session::close_all().await;

    result
}

pub async fn run_headless(
    agent_config: &AgentConfig,
    _reauthenticate: bool,
    memory_config: Option<crate::config::MemoryConfig>,
) -> anyhow::Result<()> {
    // Headless mode is the same local ACP/stdin agent in the clean build;
    // there is no hosted relay or browser login fallback.
    run_stdio_agent(agent_config, None, memory_config).await
}

/// Run the headless agent without opening any browser windows.
/// If no cached credentials exist, returns an error instead of starting OAuth flow.
pub async fn run_headless_no_browser(
    agent_config: &AgentConfig,
    memory_config: Option<crate::config::MemoryConfig>,
) -> anyhow::Result<()> {
    run_stdio_agent(agent_config, None, memory_config).await
}
/// Leader/relay mode is retained as a compatibility entry point, but the local
/// composition has no hosted account or relay runtime.
///
/// # Arguments
///
/// * `agent_config` - The agent configuration
/// * `no_exit_on_disconnect` - If true, the leader will not exit when all clients disconnect
/// * The remaining arguments are accepted for API compatibility and ignored.
pub async fn run_leader(
    agent_config: &AgentConfig,
    no_exit_on_disconnect: bool,
    relay_on_demand: bool,
    auto_update_check: Option<LeaderAutoUpdateConfig>,
    memory_config: Option<crate::config::MemoryConfig>,
) -> anyhow::Result<()> {
    // The upstream leader owns the hosted relay, managed policy refresh,
    // account session refresh and optional updater. The clean composition root
    // never calls it, and this guard also protects downstream embedders that
    // still link the original public function.
    let _ = (
        agent_config,
        no_exit_on_disconnect,
        relay_on_demand,
        &auto_update_check,
        &memory_config,
    );
    return Err(anyhow::anyhow!(
        "leader/relay mode is disabled in the clean local build"
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicU32;
    use tokio::sync::watch;
    use tokio_util::sync::CancellationToken;

    /// Create a throwaway shutdown_tx for tests that don't care about the reason.
    fn dummy_shutdown_tx() -> watch::Sender<crate::leader::ShutdownReason> {
        watch::channel(crate::leader::ShutdownReason::Manual).0
    }

    /// Helper: build a LeaderAutoUpdateConfig whose check_fn always returns the given value.
    fn always_config(update_available: bool) -> LeaderAutoUpdateConfig {
        LeaderAutoUpdateConfig {
            check_interval: Duration::from_millis(10),
            check_fn: Box::new(move || Box::pin(async move { update_available })),
        }
    }

    /// Helper: build a LeaderAutoUpdateConfig that returns `false` for the first
    /// `skip` calls, then `true` for all subsequent calls.
    fn delayed_update_config(skip: u32) -> LeaderAutoUpdateConfig {
        let counter = Arc::new(AtomicU32::new(0));
        LeaderAutoUpdateConfig {
            check_interval: Duration::from_millis(10),
            check_fn: Box::new(move || {
                let counter = counter.clone();
                Box::pin(async move {
                    let n = counter.fetch_add(1, Ordering::Relaxed);
                    n >= skip
                })
            }),
        }
    }

    #[test]
    fn internal_reload_request_line_carries_id_params_and_newline() {
        let line = internal_reload_request_line(
            "config-reload-models",
            InternalMethod::ReloadModels,
            serde_json::json!({}),
        );
        assert!(line.ends_with('\n'), "must be a newline-terminated line");
        let msg: serde_json::Value = serde_json::from_str(line.trim_end()).unwrap();
        assert_eq!(
            msg["method"], "_x.ai/internal/reload_models",
            "wire method must carry the `_` ext prefix or the ACP decoder \
             rejects it with method_not_found"
        );
        assert_eq!(msg["id"], "config-reload-models");
        assert_eq!(msg["jsonrpc"], "2.0");

        // Params must pass through verbatim (project-MCP reload carries cwd).
        let line = internal_reload_request_line(
            "config-reload-project-mcp",
            InternalMethod::ReloadProjectMcpServers,
            serde_json::json!({ "cwd": "/repo/x" }),
        );
        let msg: serde_json::Value = serde_json::from_str(line.trim_end()).unwrap();
        assert_eq!(msg["params"]["cwd"], "/repo/x");

        let line = internal_reload_request_line(
            "config-auth-cleared",
            InternalMethod::AuthCleared,
            serde_json::json!({}),
        );
        let msg: serde_json::Value = serde_json::from_str(line.trim_end()).unwrap();
        assert_eq!(msg["method"], "_x.ai/internal/auth_cleared");
    }

    #[tokio::test]
    async fn auto_update_cancels_when_update_available_and_agent_idle() {
        let agent_busy = Arc::new(AtomicBool::new(false));
        let cancel = CancellationToken::new();

        let config = always_config(true);

        // The checker should cancel the token on its first check (agent idle)
        tokio::time::timeout(
            Duration::from_secs(2),
            run_auto_update_checker(
                config,
                agent_busy,
                crate::agent::activity::AgentActivity::default(),
                cancel.clone(),
                dummy_shutdown_tx(),
            ),
        )
        .await
        .expect("checker should complete within timeout");

        assert!(cancel.is_cancelled(), "cancel token should be triggered");
    }

    #[tokio::test]
    async fn auto_update_defers_when_agent_busy() {
        let agent_busy = Arc::new(AtomicBool::new(true)); // agent is processing a prompt
        let cancel = CancellationToken::new();

        let config = delayed_update_config(0); // always returns true

        let cancel_clone = cancel.clone();
        let checker = tokio::spawn(run_auto_update_checker(
            config,
            agent_busy,
            crate::agent::activity::AgentActivity::default(),
            cancel.clone(),
            dummy_shutdown_tx(),
        ));

        // Wait enough for multiple checks to fire
        tokio::time::sleep(Duration::from_millis(80)).await;

        // Token should NOT be cancelled (agent is busy)
        assert!(
            !cancel_clone.is_cancelled(),
            "cancel token should NOT be triggered when agent is busy"
        );

        // Clean up
        cancel_clone.cancel();
        let _ = checker.await;
    }

    #[tokio::test]
    async fn auto_update_no_cancel_when_no_update_available() {
        let agent_busy = Arc::new(AtomicBool::new(false));
        let cancel = CancellationToken::new();

        let config = always_config(false);

        let cancel_clone = cancel.clone();
        let checker = tokio::spawn(run_auto_update_checker(
            config,
            agent_busy,
            crate::agent::activity::AgentActivity::default(),
            cancel.clone(),
            dummy_shutdown_tx(),
        ));

        // Let several checks fire
        tokio::time::sleep(Duration::from_millis(80)).await;

        assert!(
            !cancel_clone.is_cancelled(),
            "cancel token should NOT be triggered when no update is available"
        );

        // Clean up
        cancel_clone.cancel();
        let _ = checker.await;
    }

    #[tokio::test]
    async fn auto_update_cancels_after_agent_becomes_idle() {
        let agent_busy = Arc::new(AtomicBool::new(true)); // agent processing initially
        let cancel = CancellationToken::new();

        // Update is always available, but agent is busy initially
        let config = always_config(true);

        let agent_busy_clone = agent_busy.clone();
        let cancel_clone = cancel.clone();
        let checker = tokio::spawn(run_auto_update_checker(
            config,
            agent_busy,
            crate::agent::activity::AgentActivity::default(),
            cancel.clone(),
            dummy_shutdown_tx(),
        ));

        // Let a few checks fire while agent is busy
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(
            !cancel_clone.is_cancelled(),
            "should not cancel while agent is busy"
        );

        // Simulate agent finishing its work (prompt completes)
        agent_busy_clone.store(false, Ordering::Relaxed);

        // Wait for the next check to fire and trigger cancellation
        tokio::time::timeout(Duration::from_secs(2), checker)
            .await
            .expect("checker should complete within timeout")
            .expect("checker task should not panic");

        assert!(
            cancel_clone.is_cancelled(),
            "cancel token should be triggered after agent becomes idle"
        );
    }

    #[tokio::test]
    async fn auto_update_stops_when_externally_cancelled() {
        let agent_busy = Arc::new(AtomicBool::new(false));
        let cancel = CancellationToken::new();

        // No update available, so the checker runs indefinitely
        let config = always_config(false);

        let cancel_clone = cancel.clone();
        let checker = tokio::spawn(run_auto_update_checker(
            config,
            agent_busy,
            crate::agent::activity::AgentActivity::default(),
            cancel.clone(),
            dummy_shutdown_tx(),
        ));

        // Cancel externally
        cancel_clone.cancel();

        // Checker should exit promptly
        tokio::time::timeout(Duration::from_secs(2), checker)
            .await
            .expect("checker should exit within timeout after external cancel")
            .expect("checker task should not panic");
    }

    #[tokio::test]
    async fn auto_update_calls_check_fn_multiple_times() {
        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_clone = call_count.clone();

        let agent_busy = Arc::new(AtomicBool::new(true)); // agent busy, so it defers
        let cancel = CancellationToken::new();

        let config = LeaderAutoUpdateConfig {
            check_interval: Duration::from_millis(10),
            check_fn: Box::new(move || {
                let cc = call_count_clone.clone();
                Box::pin(async move {
                    cc.fetch_add(1, Ordering::Relaxed);
                    true // update always available, but won't cancel because agent is busy
                })
            }),
        };

        let cancel_clone = cancel.clone();
        let checker = tokio::spawn(run_auto_update_checker(
            config,
            agent_busy,
            crate::agent::activity::AgentActivity::default(),
            cancel.clone(),
            dummy_shutdown_tx(),
        ));

        // Let several checks fire. Use a generous timeout to avoid flakiness
        // in CI where the first check may take longer due to task scheduling.
        tokio::time::sleep(Duration::from_millis(200)).await;

        let calls = call_count.load(Ordering::Relaxed);
        assert!(
            calls >= 2,
            "check_fn should have been called multiple times, got {}",
            calls
        );

        cancel_clone.cancel();
        let _ = checker.await;
    }

    #[tokio::test]
    async fn auto_update_cancels_during_hanging_check_fn() {
        // Simulates a stalled-HTTP scenario: check_fn hangs (stalled HTTP).
        // The checker should still respond to cancellation thanks to the select!.
        let agent_busy = Arc::new(AtomicBool::new(false));
        let cancel = CancellationToken::new();

        let config = LeaderAutoUpdateConfig {
            check_interval: Duration::from_millis(10),
            check_fn: Box::new(|| {
                Box::pin(async {
                    // Simulate a hanging HTTP call that never completes
                    futures::future::pending::<bool>().await
                })
            }),
        };

        let cancel_clone = cancel.clone();
        let checker = tokio::spawn(run_auto_update_checker(
            config,
            agent_busy,
            crate::agent::activity::AgentActivity::default(),
            cancel.clone(),
            dummy_shutdown_tx(),
        ));

        // Let the checker enter the hanging check_fn
        tokio::time::sleep(Duration::from_millis(30)).await;

        // Cancel externally — should NOT hang
        cancel_clone.cancel();

        // Checker must exit promptly despite the hanging check_fn
        tokio::time::timeout(Duration::from_secs(2), checker)
            .await
            .expect("checker should exit within timeout even with hanging check_fn")
            .expect("checker task should not panic");
    }

    /// The IPC `agent_busy` flag never sees relay-driven traffic — the checker
    /// must also defer on the agent-derived activity signal (running turn,
    /// pending interaction, or live subagent).
    #[tokio::test]
    async fn auto_update_defers_when_agent_activity_busy() {
        let agent_busy = Arc::new(AtomicBool::new(false)); // IPC view: idle
        let activity = crate::agent::activity::AgentActivity::default();
        // Agent view: a subagent is running (e.g. spawned by a relay prompt).
        activity.subagent_gauge().store(1, Ordering::Relaxed);
        let cancel = CancellationToken::new();

        let config = always_config(true); // update always "installed"

        let cancel_clone = cancel.clone();
        let checker = tokio::spawn(run_auto_update_checker(
            config,
            agent_busy,
            activity.clone(),
            cancel.clone(),
            dummy_shutdown_tx(),
        ));

        tokio::time::sleep(Duration::from_millis(80)).await;
        assert!(
            !cancel_clone.is_cancelled(),
            "must not shut down while the agent (not IPC) is busy"
        );

        // Subagent finishes → next tick shuts down.
        activity.subagent_gauge().store(0, Ordering::Relaxed);
        tokio::time::timeout(Duration::from_secs(2), checker)
            .await
            .expect("checker should complete within timeout")
            .expect("checker task should not panic");
        assert!(cancel_clone.is_cancelled());
    }

    /// A permanently-busy signal must not pin the leader to an old binary
    /// forever: after MAX_AUTO_UPDATE_BUSY_DEFERRALS the update proceeds.
    #[tokio::test]
    async fn auto_update_forces_shutdown_after_deferral_limit() {
        let agent_busy = Arc::new(AtomicBool::new(false));
        let activity = crate::agent::activity::AgentActivity::default();
        // Permanently busy (e.g. an orphaned parked interaction).
        activity.subagent_gauge().store(1, Ordering::Relaxed);
        let cancel = CancellationToken::new();

        let config = always_config(true); // update always "installed"

        // 10ms interval × (24 deferrals + 1) ≈ 250ms — well within timeout.
        tokio::time::timeout(
            Duration::from_secs(10),
            run_auto_update_checker(
                config,
                agent_busy,
                activity,
                cancel.clone(),
                dummy_shutdown_tx(),
            ),
        )
        .await
        .expect("checker should force shutdown after the deferral limit");
        assert!(cancel.is_cancelled());
    }

    /// Before cancelling (which drops the LocalSet and aborts session actors),
    /// the checker must ask every registered session actor to shut down and
    /// wait for it to exit, so buffered state is flushed to disk.
    #[tokio::test]
    async fn auto_update_flushes_sessions_before_cancel() {
        let agent_busy = Arc::new(AtomicBool::new(false));
        let activity = crate::agent::activity::AgentActivity::default();
        let (mut cmd_rx, _prompt_id, _pending) = activity.register_for_test("s1");
        let cancel = CancellationToken::new();

        // Simulated session actor: records the Shutdown command, then exits
        // (dropping cmd_rx, which is how the flush observes completion).
        let got_shutdown = Arc::new(AtomicBool::new(false));
        let got_shutdown_clone = got_shutdown.clone();
        let cancel_for_actor = cancel.clone();
        let actor = tokio::spawn(async move {
            while let Some(cmd) = cmd_rx.recv().await {
                if matches!(cmd, crate::session::SessionCommand::Shutdown(_)) {
                    assert!(
                        !cancel_for_actor.is_cancelled(),
                        "session flush must happen BEFORE the leader is cancelled"
                    );
                    got_shutdown_clone.store(true, Ordering::Relaxed);
                    return;
                }
            }
        });

        let config = always_config(true);
        tokio::time::timeout(
            Duration::from_secs(2),
            run_auto_update_checker(
                config,
                agent_busy,
                activity,
                cancel.clone(),
                dummy_shutdown_tx(),
            ),
        )
        .await
        .expect("checker should complete within timeout");

        assert!(cancel.is_cancelled());
        actor.await.expect("actor should exit cleanly");
        assert!(
            got_shutdown.load(Ordering::Relaxed),
            "session actor must receive SessionCommand::Shutdown before leader cancel"
        );
    }

    /// Verify that when an update is installed and the agent is idle, the checker
    /// sends `ShutdownReason::AutoUpdate` via the `shutdown_tx` channel BEFORE
    /// cancelling the token, so the IPC server broadcasts the correct reason.
    #[tokio::test]
    async fn auto_update_sets_shutdown_reason_auto_update() {
        let agent_busy = Arc::new(AtomicBool::new(false));
        let cancel = CancellationToken::new();
        let (shutdown_tx, mut shutdown_rx) = watch::channel(crate::leader::ShutdownReason::Manual);

        let config = always_config(true); // update always available

        tokio::time::timeout(
            Duration::from_secs(2),
            run_auto_update_checker(
                config,
                agent_busy,
                crate::agent::activity::AgentActivity::default(),
                cancel.clone(),
                shutdown_tx,
            ),
        )
        .await
        .expect("checker should complete within timeout");

        assert!(cancel.is_cancelled(), "cancel token should be triggered");

        // The shutdown_tx must have been updated to AutoUpdate before cancel fired.
        shutdown_rx.mark_changed(); // ensure borrow sees latest value
        assert_eq!(
            *shutdown_rx.borrow(),
            crate::leader::ShutdownReason::AutoUpdate,
            "shutdown reason must be AutoUpdate for an auto-update-triggered shutdown"
        );
    }
}
