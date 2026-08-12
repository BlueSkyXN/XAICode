//! Local authentication compatibility façade.
//!
//! The upstream account login funnel (WebLogin, OIDC, device code, cached
//! session adoption, switching and logout) is intentionally absent from the
//! local runtime.  This module keeps the small source-compatible surface used
//! by older composition callers while routing only to the generic/API-key
//! manager.  No function in this file opens a browser, reads `auth.json`,
//! invokes an account login helper, or clears an account scope.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use tokio::sync::{mpsc, oneshot};

use crate::auth::{AuthManager, GrokAuth, GrokComConfig};

pub(crate) type StderrCallback = Box<dyn Fn(&str)>;

/// Legacy transport selector retained for wire/config compatibility.  The
/// local build never resolves it into a browser or device-code flow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LoginTransportOverride {
    #[default]
    None,
    ForceLoopback,
    ForceDevice,
    Preresolved(bool),
}

impl LoginTransportOverride {
    pub fn from_flags(force_loopback: bool, force_device: bool) -> Self {
        if force_loopback {
            Self::ForceLoopback
        } else if force_device {
            Self::ForceDevice
        } else {
            Self::None
        }
    }
}

/// Legacy UI mode values. No local runtime produces an auth URL.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthUrlMode {
    Loopback,
    Command,
    Device,
}

impl AuthUrlMode {
    pub fn as_wire_str(self) -> &'static str {
        match self {
            Self::Loopback => "loopback",
            Self::Command => "command",
            Self::Device => "device",
        }
    }

    pub(crate) fn is_external_provider(self) -> bool {
        matches!(self, Self::Command)
    }
}

pub struct AuthUrlInfo {
    pub url: String,
    pub mode: AuthUrlMode,
}

/// Compatibility channel type. It is never consumed by the local façade.
pub struct AuthChannels {
    pub url_tx: Option<oneshot::Sender<AuthUrlInfo>>,
    pub code_rx: mpsc::Receiver<String>,
}

fn disabled() -> anyhow::Error {
    anyhow::anyhow!(
        "Interactive xAI account authentication is unavailable in the local build; set a provider API key."
    )
}

pub(crate) async fn run_auth_flow_with_stderr_bridge(
    auth_manager: &Arc<AuthManager>,
    grok_com_config: &GrokComConfig,
    channels: AuthChannels,
    reauth: bool,
    force_interactive: bool,
    login_override: LoginTransportOverride,
) -> anyhow::Result<(GrokAuth, bool)> {
    let _ = (
        auth_manager,
        grok_com_config,
        channels,
        reauth,
        force_interactive,
        login_override,
    );
    Err(disabled())
}

pub(crate) async fn run_auth_flow(
    auth_manager: &Arc<AuthManager>,
    grok_com_config: &GrokComConfig,
    reauth: bool,
    on_stderr: Option<StderrCallback>,
    url_tx: Option<Rc<RefCell<Option<oneshot::Sender<AuthUrlInfo>>>>>,
    code_rx: Option<mpsc::Receiver<String>>,
    login_override: LoginTransportOverride,
) -> anyhow::Result<(GrokAuth, bool)> {
    let _ = (
        auth_manager,
        grok_com_config,
        reauth,
        on_stderr,
        url_tx,
        code_rx,
        login_override,
    );
    Err(disabled())
}

pub(crate) async fn run_auth_flow_interactive(
    auth_manager: &Arc<AuthManager>,
    grok_com_config: &GrokComConfig,
    on_stderr: Option<StderrCallback>,
    url_tx: Option<Rc<RefCell<Option<oneshot::Sender<AuthUrlInfo>>>>>,
    code_rx: Option<mpsc::Receiver<String>>,
    login_override: LoginTransportOverride,
) -> anyhow::Result<(GrokAuth, bool)> {
    let _ = (
        auth_manager,
        grok_com_config,
        on_stderr,
        url_tx,
        code_rx,
        login_override,
    );
    Err(disabled())
}

/// Return an already available generic/API-key credential. Construction uses
/// `new_local`, so this path never adopts xAI account state from disk.
pub async fn try_ensure_fresh_auth(grok_com_config: &GrokComConfig) -> Option<GrokAuth> {
    let manager = Arc::new(AuthManager::new_local(
        &crate::util::grok_home::grok_home(),
        grok_com_config.clone(),
    ));
    manager.auth().await.ok()
}

pub(crate) async fn try_noninteractive_auth_no_mint(
    grok_com_config: &GrokComConfig,
) -> Option<GrokAuth> {
    try_ensure_fresh_auth(grok_com_config).await
}

/// Account/session minting is deliberately removed. Explicit provider API
/// keys are resolved by the provider credential path instead.
pub(crate) async fn mint_session_noninteractive(
    _auth_manager: &Arc<AuthManager>,
) -> Option<GrokAuth> {
    None
}

pub async fn ensure_authenticated(
    grok_com_config: &GrokComConfig,
    _reauth: bool,
    _message_prefix: Option<&str>,
) -> anyhow::Result<GrokAuth> {
    try_ensure_fresh_auth(grok_com_config)
        .await
        .ok_or_else(disabled)
}

pub async fn ensure_authenticated_with_override(
    grok_com_config: &GrokComConfig,
    reauth: bool,
    message_prefix: Option<&str>,
    _login_override: LoginTransportOverride,
) -> anyhow::Result<GrokAuth> {
    ensure_authenticated(grok_com_config, reauth, message_prefix).await
}

pub async fn ensure_authenticated_or_noninteractive(
    grok_com_config: &GrokComConfig,
    has_noninteractive_auth: bool,
    _message_prefix: Option<&str>,
) -> anyhow::Result<Option<GrokAuth>> {
    if has_noninteractive_auth {
        Ok(try_ensure_fresh_auth(grok_com_config).await)
    } else {
        Ok(None)
    }
}

pub async fn run_cli_login(
    _config: &crate::agent::config::Config,
    _oauth: bool,
    _device_auth: bool,
    _devbox: bool,
) -> anyhow::Result<()> {
    Err(disabled())
}

#[derive(Debug, Default)]
pub struct LogoutResult {
    pub was_logged_in: bool,
    pub email: Option<String>,
    pub api_key_still_set: bool,
}

/// Logout is an account operation and therefore has no local implementation.
/// In particular, this function never removes a file or scope from disk.
pub fn perform_logout(
    _auth_manager: &AuthManager,
    _scope: Option<&str>,
) -> std::io::Result<LogoutResult> {
    Ok(LogoutResult {
        was_logged_in: false,
        email: None,
        api_key_still_set: crate::agent::auth_method::has_xai_api_key_env(),
    })
}

pub fn run_cli_logout(_config: &crate::agent::config::Config) -> anyhow::Result<()> {
    Err(disabled())
}
