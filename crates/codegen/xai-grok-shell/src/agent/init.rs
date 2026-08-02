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
    let mut cfg = cfg.clone();
    // The upstream bootstrap fetched remote settings and enforced managed
    // policy before constructing the agent. Keep configuration strictly local
    // in the clean build, even when stale fields exist on disk.
    cfg.remote_settings = None;
    let cfg = resolve_config(&cfg, auth_manager);
    cfg.validate_model_filters()?;
    init_process(&cfg, auth_manager);
    let models_manager = ModelsManager::from_config(&cfg, prefetched, auth_manager.clone())?;

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
    return;

    #[allow(unreachable_code)]
    // Fallback: if the client didn't pre-supply remote settings, fetch them
    // now so remote-settings-gated features work regardless of which client
    // spawned us. Clients that already call `start_early_prefetch()` and
    // thread the result into `cfg.remote_settings` skip this entirely.
    if cfg.remote_settings.is_none() {
        let handle = if sync_managed {
            crate::agent::models::start_early_prefetch(Some(cfg.grok_com_config.clone()))
        } else {
            crate::agent::models::start_early_prefetch_settings_only(Some(
                cfg.grok_com_config.clone(),
            ))
        };
        if let Some(handle) = handle {
            match handle.join() {
                Ok(result) => {
                    cfg.remote_settings = result.settings;
                    crate::util::config::set_remote_campaigns_from_settings(
                        cfg.remote_settings.as_ref(),
                    );
                    tracing::info!("remote_settings fetched as shell-level fallback");
                }
                Err(_) => {
                    tracing::warn!("remote_settings fallback prefetch thread panicked");
                }
            }
        }
    }
    crate::agent::config::apply_remote_settings_side_effects(cfg.remote_settings.as_ref());
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
        crate::util::limits::log_effective_limits();

        let grok_home = crate::util::grok_home::grok_home();
        crate::builtin::extract_builtin_files(&grok_home);

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

/// Apply current telemetry config + auth identity. Tears down the client
/// when telemetry is disabled, so it's safe to call repeatedly.
pub fn update_telemetry_config(config: &AgentConfig, auth_manager: &AuthManager) {
    // Product analytics, Mixpanel and OTLP identity are removed from the clean
    // build. Retain the hook for callers that still invoke it after auth/model
    // changes, but do not inspect cached account state or initialize a client.
    let _ = (config, auth_manager);
    return;

    #[allow(unreachable_code)]
    let grok_auth = auth_manager.current().filter(|a| a.is_xai_auth());
    let user_id = grok_auth.as_ref().map(|a| a.user_id.clone());
    let team_id = grok_auth.as_ref().and_then(|a| a.team_id.clone());
    let subscription_tier = super::mvp_agent::resolve_subscription_tier_for_telemetry(
        config
            .remote_settings
            .as_ref()
            .and_then(|rs| rs.subscription_tier_display.clone()),
        auth_manager.current_or_expired().as_ref(),
    );
    xai_grok_telemetry::client::init(
        config.telemetry.clone(),
        config.resolve_telemetry_mode().value,
        user_id,
        team_id,
        config.endpoints.deployment_key.clone(),
        crate::http::origin_client_info_from_env(),
        xai_grok_version::VERSION.to_owned(),
        subscription_tier,
        crate::http::shared_client(),
    );
}
