use agent_client_protocol as acp;

use crate::agent::config::ModelEntry;
use crate::auth::PreferredAuthMethod;

/// Shared, live handle to the agent's current ACP auth method id.
///
/// `Arc` so a clone can cross the per-session-thread boundary at spawn; the
/// `ArcSwapOption` interior lets the agent's `authenticate` handler publish a
/// new method that every running session's per-turn auth gate observes on its
/// next turn -- no re-spawn. `None` until the first `authenticate`. Auth is
/// process-global (one user, one `AuthManager`), so all sessions sharing one
/// cell is correct.
pub(crate) type SharedAuthMethodId = std::sync::Arc<arc_swap::ArcSwapOption<acp::AuthMethodId>>;

/// Construct a [`SharedAuthMethodId`]. `None` is the pre-`authenticate` state.
pub(crate) fn new_shared_auth_method_id(initial: Option<acp::AuthMethodId>) -> SharedAuthMethodId {
    std::sync::Arc::new(arc_swap::ArcSwapOption::new(
        initial.map(std::sync::Arc::new),
    ))
}

/// Generic provider key used by the clean local agent.
///
/// The constant keeps its original Rust symbol for source compatibility, but
/// the public environment interface no longer names xAI or Grok.
pub const XAI_API_KEY_ENV_VAR: &str = "CODING_AGENT_API_KEY";

/// OpenAI-compatible fallback key used by local provider configurations.
pub const LEGACY_XAI_API_KEY_ENV_VAR: &str = "OPENAI_API_KEY";

/// Read the API key from the environment.
///
/// Checks the local xaicode key first, then the OpenAI-compatible name.
pub fn read_xai_api_key_env() -> Result<String, std::env::VarError> {
    std::env::var(XAI_API_KEY_ENV_VAR).or_else(|_| std::env::var(LEGACY_XAI_API_KEY_ENV_VAR))
}

/// Returns `true` if either generic key is set.
pub fn has_xai_api_key_env() -> bool {
    read_xai_api_key_env().is_ok()
}

/// Whether `xai.api_key` should be advertised (and pushed FIRST) when building
/// the `auth_methods` list at `initialize()` time.
///
/// Regression: `xai.api_key` must stay first when only per-model credentials
/// exist (no global `XAI_API_KEY`). Deferring it made BYOK users hit the login
/// screen because the pager uses `auth_methods.first()` for startup metadata.
///
/// [`build_auth_methods`] consumes this predicate and pins the ordering;
/// its tests catch call-site and predicate regressions.
///
/// Probes `std::env` at call time and consults each `ModelEntry` for a
/// resolvable api_key/env_key -- both inputs can change between calls, so the
/// result is not cached.
///
/// `disable_api_key_auth` (`[grok_com_config] disable_api_key_auth` /
/// `GROK_DISABLE_API_KEY_AUTH`) is the admin kill switch: when true the
/// method is never advertised, regardless of available credentials, so
/// `XAI_API_KEY` can't bypass a deployment's forced IdP login.
///
/// Presence-only for the first-party env key (treats it as usable). Login
/// paths that have run the validity probe should call
/// [`should_advertise_xai_api_key_with_env_ok`] with the probe result instead.
pub(crate) fn should_advertise_xai_api_key<'a, I>(disable_api_key_auth: bool, models: I) -> bool
where
    I: IntoIterator<Item = &'a ModelEntry>,
{
    should_advertise_xai_api_key_with_env_ok(disable_api_key_auth, models, true)
}

/// Single advertise policy for `xai.api_key`: kill switch, BYOK, and first-party
/// env gated by `first_party_env_ok` (probe result, or `true` for presence-only).
/// BYOK still advertises without a probe.
pub(crate) fn should_advertise_xai_api_key_with_env_ok<'a, I>(
    disable_api_key_auth: bool,
    models: I,
    first_party_env_ok: bool,
) -> bool
where
    I: IntoIterator<Item = &'a ModelEntry>,
{
    if disable_api_key_auth {
        return false;
    }
    let has_byok = models.into_iter().any(ModelEntry::has_own_credentials);
    has_byok || (has_xai_api_key_env() && first_party_env_ok)
}

/// Inputs to [`build_auth_methods`].
///
/// Booleans are computed by the caller (`MvpAgent::initialize()`) because they
/// depend on async side effects (token refresh) and shared mutable state
/// (`AuthManager`). The list-construction logic itself is pure so it can be
/// unit-tested without any of that machinery.
pub struct AuthMethodsBuildInputs<'a> {
    /// True if `xai.api_key` should be advertised AT ALL. Login/initialize
    /// callers compute via [`should_advertise_xai_api_key_with_env_ok`] after
    /// the validity probe; presence-only paths may use
    /// [`should_advertise_xai_api_key`]. When `preferred_method` is `Oidc`,
    /// this is ignored (API key is never advertised under that pin).
    pub has_external_api_key: bool,
    /// True if a cached session token is available (either present at startup
    /// or recovered via silent refresh).
    pub has_cached_token: bool,
    /// True if enterprise OIDC is configured. Mutually exclusive with the
    /// default `grok.com` method.
    pub has_enterprise_oidc: bool,
    /// Required when `has_enterprise_oidc` is true; ignored otherwise.
    pub enterprise_oidc_issuer: Option<&'a str>,
    /// Optional display label for the login method (`grok.com` or `oidc`).
    pub login_label: Option<&'a str>,
    /// True if `grok_com_config.auth_provider_command` is configured (sets
    /// `meta.external_provider = true` on the `grok.com` method).
    pub has_auth_provider_command: bool,
    /// Config pin (`[auth] preferred_method`). `None` keeps multi-method
    /// fallthrough; `Some` is fail-closed (only that method family).
    pub preferred_method: Option<PreferredAuthMethod>,
}

/// Output of [`build_auth_methods`].
pub struct BuiltAuthMethods {
    /// Auth methods in advertised order. ORDER IS THE CONTRACT: the pager's
    /// `startup_auth_metadata()` reads `methods.first()` to decide whether
    /// interactive login is needed.
    pub methods: Vec<acp::AuthMethod>,
    /// The default `auth_method_id` to install on the agent. When unpinned,
    /// `cached_token` wins over `xai.api_key` when both are present. When
    /// pinned, only the preferred method may appear; `None` means unavailable
    /// (fail auth — no cross-method fallthrough).
    pub default_auth_method_id: Option<acp::AuthMethodId>,
}

/// Build the `auth_methods` list and default `auth_method_id` from
/// pre-computed inputs.
///
/// REGRESSION GUARD: when unpinned and
/// `has_external_api_key` is true, the **first** entry MUST be `xai.api_key`.
/// A prior change deferred it to the END for per-model credentials, which made
/// the pager send per-model-key users to the login screen. Unit tests lock this.
///
/// Unpinned ordering (when each method is enabled):
/// 1. `xai.api_key`     (if `has_external_api_key`)
/// 2. `cached_token`    (if `has_cached_token`)
/// 3. exactly one of:
///    - `oidc`          (if `has_enterprise_oidc`)
///    - `grok.com`      (otherwise)
///
/// Unpinned `default_auth_method_id`:
/// - `cached_token` if `has_cached_token`
/// - `xai.api_key`  else if `has_external_api_key`
/// - `None`         otherwise
///
/// Pinned (`preferred_method`):
/// - `ApiKey`: only `xai.api_key` if available; else empty list + `None` (fail).
/// - `Oidc`: `cached_token` (if any) + interactive login; never `xai.api_key`.
///   Default is `cached_token` when present, else `None` (interactive).
pub fn build_auth_methods(inputs: AuthMethodsBuildInputs<'_>) -> BuiltAuthMethods {
    // Interactive account login, cached sessions, enterprise OIDC and
    // provider-command auth are deliberately not part of the clean build.
    // Keep the input shape so the original agent initialization code remains
    // source-compatible, but advertise only a generic API-key method.
    let has_external_api_key = inputs.has_external_api_key;
    if has_external_api_key {
        BuiltAuthMethods {
            methods: vec![xai_api_key_auth_method()],
            default_auth_method_id: Some(acp::AuthMethodId::new(XAI_API_KEY_METHOD_ID)),
        }
    } else {
        BuiltAuthMethods {
            methods: Vec::new(),
            default_auth_method_id: None,
        }
    }
}

fn build_pinned_api_key(has_external_api_key: bool) -> BuiltAuthMethods {
    if !has_external_api_key {
        xai_grok_telemetry::unified_log::warn(
            "auth: preferred_method=api_key but no API key credentials available",
            None,
            None,
        );
        return BuiltAuthMethods {
            methods: Vec::new(),
            default_auth_method_id: None,
        };
    }
    BuiltAuthMethods {
        methods: vec![xai_api_key_auth_method()],
        default_auth_method_id: Some(acp::AuthMethodId::new(XAI_API_KEY_METHOD_ID)),
    }
}

fn build_pinned_oidc(
    has_cached_token: bool,
    has_enterprise_oidc: bool,
    enterprise_oidc_issuer: Option<&str>,
    login_label: Option<&str>,
    has_auth_provider_command: bool,
) -> BuiltAuthMethods {
    let mut methods: Vec<acp::AuthMethod> = Vec::new();
    let mut default_auth_method_id: Option<acp::AuthMethodId> = None;

    if has_cached_token {
        methods.push(cached_token_auth_method());
        default_auth_method_id = Some(acp::AuthMethodId::new(CACHED_TOKEN_AUTH_METHOD_ID));
    }

    push_interactive_login(
        &mut methods,
        has_enterprise_oidc,
        enterprise_oidc_issuer,
        login_label,
        has_auth_provider_command,
    );

    BuiltAuthMethods {
        methods,
        default_auth_method_id,
    }
}

fn build_unpinned(
    has_external_api_key: bool,
    has_cached_token: bool,
    has_enterprise_oidc: bool,
    enterprise_oidc_issuer: Option<&str>,
    login_label: Option<&str>,
    has_auth_provider_command: bool,
) -> BuiltAuthMethods {
    let mut methods: Vec<acp::AuthMethod> = Vec::new();
    let mut default_auth_method_id: Option<acp::AuthMethodId> = None;

    if has_external_api_key {
        methods.push(xai_api_key_auth_method());
        default_auth_method_id = Some(acp::AuthMethodId::new(XAI_API_KEY_METHOD_ID));
    }

    if has_cached_token {
        methods.push(cached_token_auth_method());
        // cached_token wins over xai.api_key for default_auth_method_id so
        // is_session_based_auth() returns true and OIDC refresh stays alive.
        let overrode_api_key = default_auth_method_id.is_some();
        default_auth_method_id = Some(acp::AuthMethodId::new(CACHED_TOKEN_AUTH_METHOD_ID));
        if overrode_api_key {
            xai_grok_telemetry::unified_log::info(
                "auth method priority: cached_token overrides xai.api_key for default_auth_method_id",
                None,
                Some(serde_json::json!({
                    "has_external_api_key": has_external_api_key,
                    "has_cached_token": has_cached_token,
                })),
            );
        }
    }

    push_interactive_login(
        &mut methods,
        has_enterprise_oidc,
        enterprise_oidc_issuer,
        login_label,
        has_auth_provider_command,
    );

    BuiltAuthMethods {
        methods,
        default_auth_method_id,
    }
}

fn push_interactive_login(
    methods: &mut Vec<acp::AuthMethod>,
    has_enterprise_oidc: bool,
    enterprise_oidc_issuer: Option<&str>,
    login_label: Option<&str>,
    has_auth_provider_command: bool,
) {
    if has_enterprise_oidc {
        // Caller invariant: `enterprise_oidc_issuer` MUST be `Some(...)` when
        // `has_enterprise_oidc` is true. Production callers derive both from
        // the same `cfg.grok_com_config.oidc` Option, so the inconsistent
        // `(true, None)` combination is a programmer error -- panic loudly
        // (matches the original `cfg.grok_com_config.oidc.as_ref().unwrap()`
        // call in `MvpAgent::initialize()` before this refactor).
        let issuer = enterprise_oidc_issuer
            .expect("enterprise_oidc_issuer is required when has_enterprise_oidc is true");
        methods.push(oidc_auth_method(issuer, login_label));
    } else {
        methods.push(grok_com_auth_method(login_label, has_auth_provider_command));
    }
}

/// ACP session auth method. Use `is_session_based_method` for classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthMethodKind {
    XaiApiKey,
    CachedToken,
    GrokCom,
    Oidc,
    Unknown,
}

impl AuthMethodKind {
    pub fn from_id(id: &acp::AuthMethodId) -> Self {
        match id.0.as_ref() {
            XAI_API_KEY_METHOD_ID => Self::XaiApiKey,
            CACHED_TOKEN_AUTH_METHOD_ID => Self::CachedToken,
            GROK_COM_METHOD_ID => Self::GrokCom,
            OIDC_METHOD_ID => Self::Oidc,
            _ => Self::Unknown,
        }
    }

    /// API key auth: no auth.json, no refresh, no user interaction.
    pub fn is_api_key(self) -> bool {
        matches!(self, Self::XaiApiKey)
    }

    /// `true` for session-based methods (cached_token, grok.com, oidc).
    pub(crate) fn is_session_based(self) -> bool {
        matches!(self, Self::CachedToken | Self::GrokCom | Self::Oidc)
    }

    /// Requires user interaction (browser, OIDC redirect, or external auth command).
    pub fn needs_interactive_login(self) -> bool {
        matches!(self, Self::GrokCom | Self::Oidc)
    }
}

/// `true` for session-based ACP methods (cached_token, grok.com, oidc).
pub(crate) fn is_session_based_method(method_id: &acp::AuthMethodId) -> bool {
    AuthMethodKind::from_id(method_id).is_session_based()
}

/// Per-model BYOK status: whether the selected model carries its own
/// `[model.*]` `api_key`/`env_key`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ModelByok {
    /// Model has its own per-model key (not refreshable).
    Byok,
    /// Model has no per-model key (session auth governs).
    NotByok,
    /// Config couldn't be loaded/parsed — BYOK status indeterminate.
    Unknown,
}

impl ModelByok {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Byok => "byok",
            Self::NotByok => "not_byok",
            Self::Unknown => "unknown",
        }
    }
}

/// Whether this session+model uses a refreshable session token.
///
/// Gates on stable inputs, not `Credentials.auth_type`: that field collapses
/// to `ApiKey` when the session-token cache is momentarily empty and
/// `XAI_API_KEY` is set, which demoted live OIDC sessions to non-refreshable
/// api-key mode and 401'd every prompt until restart. `model_byok` still
/// excludes genuine per-model BYOK, whose keys are not refreshable.
///
/// `Unknown` (BYOK status indeterminate — config currently unparseable, no
/// sampling config yet, or the per-model memo was cleared) must **not** demote
/// a live session to non-refreshable api-key mode: that re-sends the stale
/// buffered token on every turn and 401s with `bad-credentials` until restart
/// (the stale-token regression this gate addresses; fall back rather than
/// demote on `Unknown`). It refreshes when `endpoint_is_first_party` — the
/// request targets a first-party host (cli-chat-proxy / first-party API),
/// where sending the session token cannot leak to a third-party BYOK
/// endpoint. A definite `NotByok` always refreshes (it only ever routes to
/// the session endpoint); a definite `Byok` never does.
pub(crate) fn session_token_auth_gate(
    is_session_based_method: bool,
    model_byok: ModelByok,
    endpoint_is_first_party: bool,
) -> bool {
    is_session_based_method
        && match model_byok {
            ModelByok::NotByok => true,
            ModelByok::Byok => false,
            ModelByok::Unknown => endpoint_is_first_party,
        }
}

pub const AUTH_ERROR_SESSION_EXPIRED: &str =
    "Session authentication is disabled. Configure a local provider API key.";

pub const AUTH_ERROR_API_KEY: &str = "Authentication failed. Set CODING_AGENT_API_KEY, OPENAI_API_KEY, or add api_key/env_key to config.toml.";

/// Next ACP method id when `cached_token` cannot proceed (missing / expired /
/// legacy WebLogin), or `None` when fallthrough is forbidden.
///
/// The clean build has no interactive account fallback. An external key is
/// the only supported credential, and a missing key fails closed.
///
/// Pinned `oidc`: **no** fallthrough to api_key — return `None` so the caller
/// fails auth. Pinned `api_key` should not reach this path (cached_token is
/// not advertised).
pub(crate) fn method_id_after_cached_token_unavailable(
    has_external_api_key: bool,
    preferred_method: Option<PreferredAuthMethod>,
) -> Option<&'static str> {
    match preferred_method {
        Some(PreferredAuthMethod::Oidc) | Some(PreferredAuthMethod::ApiKey) => None,
        None => has_external_api_key.then_some(XAI_API_KEY_METHOD_ID),
    }
}

/// Error when `preferred_method=api_key` but no key/BYOK credentials exist.
pub const PREFERRED_API_KEY_UNAVAILABLE: &str = "No API key is configured (set CODING_AGENT_API_KEY, OPENAI_API_KEY, or model api_key/env_key in config.toml).";

/// Error when `preferred_method=oidc` but the session path cannot proceed.
pub const PREFERRED_OIDC_UNAVAILABLE: &str =
    "Interactive account authentication is disabled in the clean build.";

pub const XAI_API_KEY_METHOD_ID: &str = "api_key";
pub fn xai_api_key_auth_method() -> acp::AuthMethod {
    acp::AuthMethod::Agent(
        acp::AuthMethodAgent::new(
            acp::AuthMethodId::new(XAI_API_KEY_METHOD_ID),
            "api_key".to_string(),
        )
        .description(Some(format!(
            "{XAI_API_KEY_ENV_VAR} or api_key/env_key in config.toml"
        ))),
    )
}

pub const CACHED_TOKEN_AUTH_METHOD_ID: &str = "cached_token";
pub(crate) fn cached_token_auth_method() -> acp::AuthMethod {
    acp::AuthMethod::Agent(
        acp::AuthMethodAgent::new(
            acp::AuthMethodId::new(CACHED_TOKEN_AUTH_METHOD_ID),
            "cached_token".to_string(),
        )
        .description(Some("Cached token from ~/.grok/auth.json".to_string())),
    )
}

pub const GROK_COM_METHOD_ID: &str = "grok.com";

/// xAI OAuth2/OIDC auth. Method id `"grok.com"` kept for ACP wire-compat.
pub(crate) fn grok_com_auth_method(
    label: Option<&str>,
    has_auth_provider_command: bool,
) -> acp::AuthMethod {
    let name = label.unwrap_or("Grok");
    let meta = if has_auth_provider_command {
        let mut m = acp::Meta::new();
        m.insert("external_provider".to_owned(), serde_json::json!(true));
        Some(m)
    } else {
        None
    };
    acp::AuthMethod::Agent(
        acp::AuthMethodAgent::new(acp::AuthMethodId::new(GROK_COM_METHOD_ID), name.to_string())
            .description(Some(format!("Sign in with {name}")))
            .meta(meta),
    )
}

pub const OIDC_METHOD_ID: &str = "oidc";
pub(crate) fn oidc_auth_method(issuer: &str, label: Option<&str>) -> acp::AuthMethod {
    let name = label
        .map(|l| l.to_string())
        .unwrap_or_else(|| format!("Single sign-on ({})", issuer));
    acp::AuthMethod::Agent(
        acp::AuthMethodAgent::new(acp::AuthMethodId::new(OIDC_METHOD_ID), name.clone())
            .description(Some(format!("Sign in with {name}"))),
    )
}

// The upstream test matrix exercises browser login, cached auth.json sessions,
// OIDC and Grok-specific fallback ordering. Those paths are intentionally not
// compiled in the clean build; keep the historical tests in the source tree
// as reference, but compile only the provider-neutral checks below.
