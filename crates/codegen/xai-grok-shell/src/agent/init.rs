//! Agent bootstrap and lifecycle hooks.
//!
//! [`bootstrap`] runs the full init sequence (config resolution, process
//! singletons, model catalog) and returns a resolved config + `ModelsManager`.
//! [`update_telemetry_config`] re-initializes telemetry after auth changes.

use std::sync::Arc;

use indexmap::IndexMap;

use crate::agent::config::{Config as AgentConfig, ModelEntry};
use crate::agent::models::ModelsManager;
use crate::auth::AuthManager;
use crate::config::StorageMode;

/// Resolve config, init process singletons, build the model catalog.
///
/// The `ModelsManager` is `Clone + Send`, so callers that need a handle
/// for the config watcher can clone it before passing it to
/// `MvpAgent::with_models`.
pub fn bootstrap(
    cfg: &AgentConfig,
    auth_manager: &Arc<AuthManager>,
    prefetched: Option<IndexMap<String, ModelEntry>>,
) -> Result<(AgentConfig, ModelsManager), String> {
    xai_grok_telemetry::startup::enter(xai_grok_telemetry::startup::StartupPhase::Bootstrap);
    let mut cfg = cfg.clone();
    // The upstream bootstrap fetched remote settings and enforced managed
    // policy before constructing the agent. Keep configuration strictly local
    // in the clean build, even when stale fields exist on disk.
    cfg.remote_settings = None;
    let cfg = {
        let _timer = crate::instrumentation_timer!("startup.bootstrap.resolve_config");
        let cfg = resolve_config(&cfg, auth_manager);
        cfg.validate_model_filters()?;
        cfg
    };
    {
        let _timer = crate::instrumentation_timer!("startup.bootstrap.init_process");
        init_process(&cfg, auth_manager);
    }
    xai_grok_telemetry::startup::enter(xai_grok_telemetry::startup::StartupPhase::ModelCatalog);
    let models_manager = {
        let _timer = crate::instrumentation_timer!("startup.bootstrap.models_manager");
        ModelsManager::from_config(&cfg, prefetched, auth_manager.clone())?
    };

    Ok((cfg, models_manager))
}

/// Print a `bootstrap`/`MvpAgent::new` config error and exit (process boundary).
///
/// Restores native stderr first: a managed-policy refusal on the ACP/server path reaches here
/// while fd 2 may still point at the `/dev/null` the TUI's `redirect_native_stderr()` set, which
/// would swallow the message. No-op when stderr was never redirected (headless).
pub(crate) fn exit_on_config_error<T>(e: String) -> T {
    xai_tty_utils::restore_native_stderr();
    eprintln!("\nConfiguration error:\n\n    {e}\n");
    std::process::exit(1);
}

/// Fill `remote_settings` if absent and apply process-global remote side effects
/// (signature kill-switch and caches). Safe to call more than once.
///
/// `sync_managed`: when true, missing-settings fallback may also refresh
/// managed-config. Must be false before the managed-policy gate.
fn ensure_remote_settings_side_effects(cfg: &mut AgentConfig, sync_managed: bool) {
    // Kept as a compatibility shim for older embedders.  The clean build has
    // no hosted settings/managed-policy bootstrap, so neither a stale config
    // value nor a caller's `sync_managed` flag may start a network request.
    let _ = (cfg, sync_managed);
}

/// Config transform: apply managed settings, fetch remote settings,
/// resolve storage mode.
fn resolve_config(cfg: &AgentConfig, auth_manager: &AuthManager) -> AgentConfig {
    let mut cfg = cfg.clone();
    let _ = auth_manager;
    cfg.remote_settings = None;
    cfg.storage_mode = StorageMode::Local;
    cfg
}

/// Initialize process-level singletons (deployment sync, built-in metadata,
/// telemetry). `Once`-guarded: only the first call takes effect.
/// Telemetry user ID is updated separately via [`update_telemetry_config`].
fn init_process(cfg: &AgentConfig, auth_manager: &AuthManager) {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        // Every agent mode (stdio/headless/leader and the in-process TUI
        // agent) passes through here, so diagnostic uploads always carry
        // the version stamp and the resource ceilings in effect.
        xai_grok_telemetry::unified_log::set_version(xai_grok_version::VERSION);
        let limits = crate::util::limits::ProcessLimits::read();
        limits.log();

        let grok_home = crate::util::grok_home::grok_home();
        crate::builtin::extract_builtin_files(&grok_home);
        if !cfg!(test) {
            // Deletes dirs; must never touch a unit-test process's real home.
            crate::builtin::purge_stale_extracted_skills(&grok_home);
        }

        // Marketplace/default-skill migrations belong to the hosted product;
        // a local agent must not mutate a user's plugin directory at startup.

        let telemetry_mode = cfg.resolve_telemetry_mode();
        let trace_upload = cfg.resolve_trace_upload();
        let feedback = cfg.resolve_feedback();
        let feedback_url = cfg.endpoints.resolve_feedback_base_url();
        let trace_upload_url = cfg.endpoints.resolve_trace_upload_url();
        tracing::info!(
            telemetry = %telemetry_mode,
            trace_upload = %trace_upload,
            feedback = %feedback,
            feedback_url = %feedback_url,
            feedback_url_custom = cfg.endpoints.feedback_base_url.is_some(),
            trace_upload_url = %trace_upload_url,
            trace_upload_url_custom = cfg.endpoints.trace_upload_url.is_some(),
            trace_upload_bucket = cfg.endpoints.trace_upload_bucket.as_deref().unwrap_or("none"),
            trace_upload_region = cfg.endpoints.trace_upload_region.as_deref().unwrap_or("none"),
            "data capture config resolved",
        );
        if telemetry_mode.value.is_disabled() && trace_upload.value {
            tracing::info!(
                "Telemetry disabled but trace uploads enabled: \
                 session artifacts will be uploaded, analytics events will not"
            );
        }
        let _ = (cfg, auth_manager);
    });
}

/// Compatibility hook for callers that refresh local diagnostics after config
/// or auth changes. Hosted telemetry identity is intentionally not retained.
pub fn update_telemetry_config(config: &AgentConfig, auth_manager: &AuthManager) {
    // Product analytics and vendor OTLP are removed. Retain this compatibility
    // hook for callers that invoke it after auth/model changes.
    let _ = (config, auth_manager);
}
