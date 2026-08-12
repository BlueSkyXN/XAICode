#![cfg_attr(rustfmt, rustfmt::skip)]
#![allow(unused_imports)]
use std::path::PathBuf;
use std::sync::OnceLock;
use std::{cell::RefCell, collections::HashMap, rc::Rc, sync::Arc};
use tokio::sync::mpsc;
/// A `'static` reference to a value on a single-threaded `LocalSet`.
///
/// Encapsulates the raw-pointer pattern used when `spawn_local` tasks need
/// `&T` but the borrow checker requires `'static`. The pointer is valid as
/// long as:
///
/// 1. `T` is heap-allocated and never moved (e.g., behind `Rc` or owned by
///    the ACP connection for the process lifetime).
/// 2. All access happens on the **same** `LocalSet` thread (no `Send`).
/// 3. The `LocalRef` does not outlive the `LocalSet`.
///
/// These invariants are upheld by construction: `LocalRef` is `!Send`
/// (via `*const T`) and only used inside `spawn_local` closures on the
/// agent's `LocalSet`.
pub(crate) struct LocalRef<T> {
    ptr: *const T,
}
impl<T> LocalRef<T> {
    /// Create a `LocalRef` from a shared reference.
    ///
    /// # Safety contract (enforced by the caller, not by the type system)
    ///
    /// The referenced `T` must live for the entire duration of the `LocalSet`
    /// and must not be moved or deallocated while any `LocalRef` clone exists.
    pub(crate) fn new(val: &T) -> Self {
        Self { ptr: val as *const T }
    }
    /// Dereference back to `&T`.
    ///
    /// # Safety
    ///
    /// Safe because the caller of `new()` guarantees the pointee is alive
    /// and pinned, and `LocalRef` is `!Send` (only used on the same thread).
    pub(crate) fn get(&self) -> &T {
        unsafe { &*self.ptr }
    }
}
impl<T> Clone for LocalRef<T> {
    fn clone(&self) -> Self {
        Self { ptr: self.ptr }
    }
}
use agent_client_protocol::Client as _;
use agent_client_protocol::{self as acp, AuthenticateResponse};
use indexmap::IndexMap;
use tokio::sync::oneshot;
use xai_acp_lib::AcpAgentGatewaySender as GatewaySender;
use crate::agent::auth_method;
use crate::agent::config::{self, Config as AgentConfig, ModelEntry, resolve_credentials};
use crate::agent::folder_trust;
use crate::agent::models::{resolve_catalog_key, selectable_catalog_key_for_persisted};
use crate::agent::session_config;
use xai_grok_sampling_types::{
    REASONING_EFFORT_META_KEY, ReasoningEffortOption, reasoning_effort_meta_value,
    supports_reasoning_effort_meta,
};
use crate::agent::update_chunk_merge;
use crate::auth::AuthManager;
use crate::config::StorageMode;
use crate::extensions::notification::{SessionNotification, SessionUpdate};
use xai_grok_telemetry::session_ctx::log_event;
use xai_grok_workspace::file_system::{AcpSessionFs, CodebaseIndexManager, LocalFs};
use xai_grok_workspace::permission::{ClientType, PermissionEvent};
use crate::sampling::Client as OaiCompatClient;
use crate::sampling::error::map_sampling_err_to_acp;
use crate::session::mcp_servers::{McpMetaConfigMap, parse_mcp_meta_config};
use xai_grok_sampler::SamplerConfig as SamplingConfig;
use crate::session::persistence::PersistenceHandle;
use crate::session::worktree::BackgroundCopyContext;
use crate::session::{
    ParsedPromptInfo, SCHEDULER_BACKGROUND_LOOPS_META_KEY, SessionCommand, SessionHandle,
    CancelOptions, CancelTrigger, SessionLiveState, SessionThread, ShutdownKind,
    info::Info as SessionInfo, spawn_session_on_thread,
};
use crate::terminal::{AcpTerminalRunner, TerminalRunner};
use crate::tools::ToolContext;
use tokio_util::sync::CancellationToken;
use xai_grok_paths::AbsPathBuf;
use xai_grok_workspace::session::git::GitDiscoveryResult;
use xai_hunk_tracker::HunkTrackerActor;
/// Hard-error message for legacy Direct hub-bind sessions (`x.ai/cloud_server_id`).
pub(crate) const DIRECT_HUB_CLOUD_REMOVED_MSG: &str = "Direct hub cloud removed; use Gateway (envId or existing-workspace attach)";
/// Reject session `_meta` that still requests Direct hub bind (D8).
///
/// Shared by `new_session` / `load_session` via [`MvpAgent::spawn_and_register_session`].
pub(crate) fn reject_direct_hub_cloud_meta(
    session_meta: Option<&acp::Meta>,
) -> Result<(), acp::Error> {
    if session_meta.and_then(|m| m.get("x.ai/cloud_server_id")).is_some() {
        return Err(acp::Error::invalid_params().data(DIRECT_HUB_CLOUD_REMOVED_MSG));
    }
    Ok(())
}
/// ACP `_meta` key for chat+local workspace intent (pager stamps on chat create).
#[cfg(feature = "local-workspace")]
const LOCAL_WORKSPACE_META_KEY: &str = "x.ai/local_workspace";
/// True when `_meta` carries a valid chat+local intent object
/// (`mode` is `"own"` or `"attach"`).
#[cfg(feature = "local-workspace")]
fn local_workspace_intent_present(meta: Option<&acp::Meta>) -> bool {
    meta.and_then(|m| m.get(LOCAL_WORKSPACE_META_KEY))
        .and_then(|v| v.as_object())
        .and_then(|o| o.get("mode"))
        .and_then(|m| m.as_str())
        .is_some_and(|mode| mode == "own" || mode == "attach")
}
/// valid local-workspace intent → ExistingWorkspace only.
///
/// `server_id` comes from the intent object, else `cloud_existing_workspace`.
/// Never reads `envId` / never emits `SandboxEnvironment`.
#[cfg(feature = "local-workspace")]
fn parse_local_workspace_existing(
    meta: Option<&acp::Meta>,
) -> Option<crate::gateway_bridge::ComputerSession> {
    use crate::gateway_bridge::ComputerSession;
    let local = meta.and_then(|m| m.get(LOCAL_WORKSPACE_META_KEY))?;
    let mode = local.get("mode").and_then(|v| v.as_str())?;
    if mode != "own" && mode != "attach" {
        return None;
    }
    let server_id = meta_non_empty_str(local, "server_id")
        .or_else(|| {
            meta
                .and_then(|m| m.get(CLOUD_EXISTING_WORKSPACE_META_KEY))
                .and_then(|w| meta_non_empty_str(w, "server_id"))
        })?;
    let cwd = meta_non_empty_str(local, "cwd")
        .or_else(|| {
            meta
                .and_then(|m| m.get(CLOUD_EXISTING_WORKSPACE_META_KEY))
                .and_then(|w| meta_non_empty_str(w, "cwd"))
        });
    Some(ComputerSession::ExistingWorkspace {
        server_id,
        cwd,
    })
}
#[allow(dead_code)]
fn parse_session_computer_sessions(_meta: Option<&acp::Meta>) -> Option<Vec<()>> {
    None
}
fn resolve_session_computer_sessions(
    _meta: Option<&acp::Meta>,
) -> Result<Option<Vec<()>>, acp::Error> {
    Ok(None)
}
pub(crate) struct SessionSpawnOptions<'a> {
    pub session_info: SessionInfo,
    pub cwd: AbsPathBuf,
    pub mcp_servers: Vec<acp::McpServer>,
    pub initial_client_mcp_servers: Vec<acp::McpServer>,
    pub mcp_meta_config_map: McpMetaConfigMap,
    pub persistence: PersistenceHandle,
    pub chat_history: Vec<crate::sampling::ConversationItem>,
    pub rewind_points_file_path: Option<std::path::PathBuf>,
    pub initial_total_tokens: u64,
    pub origin_client: Option<crate::http::OriginClientInfo>,
    pub client_code_nav_enabled: bool,
    pub client_terminal: bool,
    pub client_fs_read: bool,
    pub client_fs_write: bool,
    /// In-flight `.envrc` load; `None` → `spawn_and_register_session` spawns its own.
    pub envrc: Option<xai_grok_workspace::envrc::EnvrcLoad>,
    pub persisted_signals: Option<crate::session::signals::SessionSignals>,
    pub persisted_plan_mode: Option<crate::session::plan_mode::PlanModeSnapshot>,
    pub persisted_goal_mode: Option<crate::session::goal_tracker::GoalOrchestration>,
    pub persisted_workflow_runs: Vec<
        crate::session::workflow::store::RestoredWorkflowRun,
    >,
    pub persisted_announcement_state: Option<
        crate::session::announcement_state::AnnouncementState,
    >,
    pub session_meta: Option<&'a acp::Meta>,
    pub model_agent_type: Option<&'a str>,
    pub session_model_id: acp::ModelId,
    pub session_yolo_mode: bool,
    pub session_auto_mode: bool,
    pub prompt_display_cwd: Option<String>,
    /// Sticky chat kind from session metadata; local skills remain the source
    /// for slash-command discovery.
    pub is_chat_kind: bool,
}
#[derive(Clone, Copy)]
#[allow(dead_code)]
pub(crate) enum BridgeAttach {
    /// No session handle, or no gateway URL and no pre-existing bridge.
    NotAttached,
    /// A bridge already existed — the caller's options (incl. any
    /// `initial_model` seed) were dropped.
    AlreadyAttached,
    /// This call spawned the bridge; its options took effect.
    Spawned,
}
impl BridgeAttach {
    #[allow(dead_code)]
    pub(crate) fn attached(self) -> bool {
        !matches!(self, Self::NotAttached)
    }
}
/// `_meta["x.ai/session"].kind` → [`SessionKind`]; absent/unknown/malformed → `Build`.
fn parse_session_kind(
    meta: Option<&acp::Meta>,
) -> crate::session::unified_list::SessionKind {
    use crate::session::unified_list::SessionKind;
    use serde::Deserialize;
    meta.and_then(|m| m.get("x.ai/session"))
        .and_then(|s| s.get("kind"))
        .and_then(|k| SessionKind::deserialize(k).ok())
        .unwrap_or(SessionKind::Build)
}
/// Whether meta requests chat kind (ignores whether the binary supports it).
fn wants_chat_session_kind(meta: Option<&acp::Meta>) -> bool {
    matches!(
        parse_session_kind(meta),
        crate::session::unified_list::SessionKind::Chat
    )
}
use crate::session::unified_list::SessionKind;
/// A chat-kind assertion from a request; only a claim until checked. Always
/// false without the `chat` feature.
#[derive(Clone, Copy)]
pub(crate) struct ChatKindClaim(bool);
impl ChatKindClaim {
    pub(crate) fn from_meta(meta: Option<&acp::Meta>) -> Self {
        {
            let _ = meta;
            Self(false)
        }
    }
    /// Which pipeline owns this id. The session decides; the claim answers
    /// only for an id the registry has never seen.
    pub(crate) fn resolve(self, agent: &MvpAgent, id: &acp::SessionId) -> SessionKind {
        let _ = (agent, id);
        self.declared()
    }
    /// What the request asked to create; authoritative only at `session/new`.
    pub(crate) fn declared(self) -> SessionKind {
        if self.0 { SessionKind::Chat } else { SessionKind::Build }
    }
}
/// Fail closed when a client requests `kind: "chat"` on a build-only binary.
fn reject_chat_kind_without_feature(meta: Option<&acp::Meta>) -> Result<(), acp::Error> {
    {
        if wants_chat_session_kind(meta) {
            return Err(
                acp::Error::invalid_params()
                    .data("session kind \"chat\" requires a chat-enabled binary"),
            );
        }
        Ok(())
    }
}
fn chat_initial_model(
    is_chat_kind: bool,
    custom_model_id: Option<&str>,
) -> Option<String> {
    if is_chat_kind { custom_model_id.map(str::to_owned) } else { None }
}
fn chat_new_session_model_state(
    mut state: acp::SessionModelState,
    requested: Option<String>,
) -> acp::SessionModelState {
    let Some(requested) = requested else {
        return state;
    };
    if !state.available_models.is_empty()
        && !state.available_models.iter().any(|m| m.model_id.0.as_ref() == requested)
    {
        tracing::warn!(
            requested_model = %requested,
            "chat session/new _meta.modelId not in the /rest/modes catalog; \
             reporting it as current anyway (picker may diverge from catalog)"
        );
    }
    state.current_model_id = acp::ModelId::new(requested);
    state
}
/// `session/new` / `session/load` `_meta` key carrying per-session plugin roots.
pub(crate) const SESSION_PLUGIN_DIRS_META_KEY: &str = "pluginDirs";
/// `initialize` response `_meta` key advertising [`SESSION_PLUGIN_DIRS_META_KEY`] support.
pub(crate) const SESSION_PLUGIN_DIRS_CAPABILITY_KEY: &str = "x.ai/pluginDirs";
/// Per-session plugin roots from `session/new` / `session/load` `_meta.pluginDirs`,
/// loaded at CliOverride scope (always trusted) into this session's registry only.
/// Paths must be absolute (the SDKs resolve before sending); anything else is
/// warned and skipped.
pub(crate) fn parse_session_plugin_dirs(
    meta: Option<&acp::Meta>,
) -> Vec<std::path::PathBuf> {
    let Some(entries) = meta
        .and_then(|m| m.get(SESSION_PLUGIN_DIRS_META_KEY))
        .and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    let mut dirs = Vec::new();
    for entry in entries {
        let Some(raw) = entry.as_str() else {
            tracing::warn!(?entry, "pluginDirs entry is not a string; skipping");
            continue;
        };
        let path = std::path::PathBuf::from(raw);
        if !path.is_absolute() {
            tracing::warn!("pluginDirs entry is not absolute; skipping");
            continue;
        }
        let canonical = dunce::canonicalize(&path).unwrap_or(path);
        if !canonical.is_dir() {
            tracing::warn!("pluginDirs entry is not a directory; skipping");
            continue;
        }
        if !dirs.contains(&canonical) {
            dirs.push(canonical);
        }
    }
    dirs
}
/// Thin chat-kind profile shared by [`MvpAgent::load_chat_session`] and
/// chat-kind `session/new` (K10): noop persistence, no MCP, no client
/// FS / terminal / code-nav. Keeps spawn options from drifting between
/// new and load.
pub(crate) fn chat_session_spawn_options<'a>(
    session_info: SessionInfo,
    cwd: AbsPathBuf,
    session_meta: Option<&'a acp::Meta>,
    model_agent_type: Option<&'a str>,
    session_model_id: acp::ModelId,
    session_yolo_mode: bool,
) -> SessionSpawnOptions<'a> {
    SessionSpawnOptions {
        session_info,
        cwd,
        mcp_servers: Vec::new(),
        initial_client_mcp_servers: Vec::new(),
        mcp_meta_config_map: Default::default(),
        persistence: crate::session::persistence::PersistenceHandle::noop(),
        chat_history: Vec::new(),
        rewind_points_file_path: None,
        initial_total_tokens: 0,
        origin_client: None,
        client_code_nav_enabled: false,
        client_terminal: false,
        client_fs_read: false,
        client_fs_write: false,
        envrc: None,
        persisted_signals: None,
        persisted_plan_mode: None,
        persisted_goal_mode: None,
        persisted_workflow_runs: Vec::new(),
        persisted_announcement_state: None,
        session_meta,
        model_agent_type,
        session_model_id,
        session_yolo_mode,
        session_auto_mode: false,
        prompt_display_cwd: None,
        is_chat_kind: true,
    }
}
/// `_meta.noReplay` → skip gateway replay (client already has the transcript).
fn parse_no_replay(meta: Option<&acp::Meta>) -> bool {
    meta.and_then(|m| m.get("noReplay")).and_then(|v| v.as_bool()).unwrap_or(false)
}
/// Insert `key`/`value` into a notification's `_meta`, creating the map if absent.
/// Used to stamp `x.ai/leaderClientId` onto replay notifications so the leader can
/// unicast them to the loading client only (see `forward_raw_replay_line`).
fn stamp_meta_value(meta: &mut Option<acp::Meta>, key: &str, value: &serde_json::Value) {
    meta.get_or_insert_with(acp::Meta::new).insert(key.to_string(), value.clone());
}
fn mark_as_replay(
    meta: &mut Option<acp::Meta>,
    persist_data: Option<&serde_json::Value>,
) {
    let is_replay = serde_json::json!(true);
    let obj = meta.get_or_insert_with(acp::Meta::new);
    obj.insert("isReplay".to_string(), is_replay);
    if let Some(persist) = persist_data {
        obj.insert("x.ai/persist".to_string(), persist.clone());
    }
}
/// Resolve a session's REQUESTED auto flag from `_meta`: an explicit `autoMode`
/// (or snake_case `auto_mode`) wins; when absent, fall back to the config default
/// with yolo taking precedence (yolo suppresses the default auto seed). Shared by
/// the new_session / load_session parse paths (the feature gate is enforced later
/// at the `set_auto_mode` seam) and unit-tested directly.
pub(crate) fn resolve_session_auto_mode(
    meta: Option<&acp::Meta>,
    default_auto_mode: bool,
    session_yolo_mode: bool,
) -> bool {
    meta.and_then(|m| m.get("autoMode").or_else(|| m.get("auto_mode")))
        .and_then(|v| v.as_bool())
        .unwrap_or(default_auto_mode && !session_yolo_mode)
}
/// Typed `_meta` payload for `PromptResponse`.
/// camelCase keys match the bot's `_META_TOKEN_KEY_MAP`.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PromptResponseMeta {
    pub session_id: String,
    pub request_id: String,
    pub prompt_id: String,
    pub total_tokens: u64,
    pub model_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached_read_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_tokens: Option<u32>,
    /// Whole-prompt billing (sibling token fields are last call only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<crate::extensions::notification::PromptUsage>,
    /// Cancellation category when the turn was terminated by the system
    /// (e.g. doom loop). `None` for normal completions and user cancels.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cancellation_category: Option<String>,
    /// What triggered a cancelled turn's cancel (`"send_now"`, `"ctrl_c"`,
    /// `"esc"`); surfaced as `cancelTrigger`. `None` for non-cancel completions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cancel_trigger: Option<String>,
    /// Schema-validated `--json-schema` output. Delivered in `_meta` (not a
    /// side-channel notification) so the client reads it deterministically when
    /// the prompt RPC resolves. Absent unless requested and produced; on
    /// failure `structured_output_error` carries the message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub structured_output: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub structured_output_error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_overrides: Option<xai_grok_sampling_types::ToolOverrides>,
}
/// Inputs for [`build_prompt_response_meta`]. A struct (not positional args)
/// so call sites are self-documenting and adding a field can't silently
/// reorder an existing one.
pub(crate) struct PromptResponseMetaArgs<'a> {
    pub session_id: &'a str,
    pub prompt_id: &'a str,
    pub total_tokens: u64,
    pub model_id: &'a str,
    pub last_turn_usage: Option<&'a xai_grok_sampling_types::TokenUsage>,
    pub prompt_usage: Option<crate::extensions::notification::PromptUsage>,
    pub cancellation_category: Option<String>,
    pub cancel_trigger: Option<String>,
    pub structured_output: Option<Result<serde_json::Value, String>>,
    pub tool_overrides: Option<xai_grok_sampling_types::ToolOverrides>,
}
/// Build the `_meta` JSON for `PromptResponse`. Includes baseline
/// session/prompt/model identifiers plus optional per-turn token counts
/// from the most recent `TokenUsage`.
pub(crate) fn build_prompt_response_meta(
    args: PromptResponseMetaArgs<'_>,
) -> serde_json::Value {
    let PromptResponseMetaArgs {
        session_id,
        prompt_id,
        total_tokens,
        model_id,
        last_turn_usage,
        prompt_usage,
        cancellation_category,
        cancel_trigger,
        structured_output,
        tool_overrides,
    } = args;
    let (structured_output, structured_output_error) = match structured_output {
        Some(Ok(value)) => (Some(value), None),
        Some(Err(error)) => (None, Some(error)),
        None => (None, None),
    };
    let meta = PromptResponseMeta {
        session_id: session_id.to_string(),
        request_id: prompt_id.to_string(),
        prompt_id: prompt_id.to_string(),
        total_tokens,
        model_id: model_id.to_string(),
        input_tokens: last_turn_usage.map(|u| u.prompt_tokens),
        output_tokens: last_turn_usage.map(|u| u.completion_tokens),
        cached_read_tokens: last_turn_usage.map(|u| u.cached_prompt_tokens),
        reasoning_tokens: last_turn_usage.map(|u| u.reasoning_tokens),
        usage: prompt_usage,
        cancellation_category,
        cancel_trigger,
        structured_output,
        structured_output_error,
        tool_overrides,
    };
    serde_json::to_value(meta).expect("PromptResponseMeta is always serializable")
}
/// Typed payload for the `x.ai/settings/update` notification sent to pager
/// clients after remote settings settings are refreshed on `/new`.
///
/// Keeping this as a `#[derive(Serialize)]` struct gives compile-time
/// contract safety between the shell and the pager deserializer.
#[derive(serde::Serialize)]
struct SettingsUpdateNotification {
    show_resolved_model: Option<bool>,
    sharing_enabled: Option<bool>,
    privacy_notice_rollout: Option<bool>,
    privacy_banner_reshow_days: Option<u64>,
    session_picker_grouped: Option<bool>,
    tips: Option<Vec<String>>,
    slash_command_tags: Option<std::collections::BTreeMap<String, String>>,
    auto_permission_mode_enabled: Option<bool>,
    /// Soft-default permission mode for the pager (post-auth / `/new` refresh).
    permission_mode: Option<String>,
    group_tool_verbs: Option<bool>,
    collapsed_edit_blocks: Option<bool>,
}
/// Reason why a client is not eligible to use codebase indexing.
///
/// Returned by [`MvpAgent::code_nav_eligibility`] when one of the policy
/// gates fails.  Used in `x.ai/code/status` responses and to generate
/// clear error messages on code-nav requests from ineligible clients.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CodeNavEligibility {
    /// Client type is not web (web-only for initial rollout).
    ClientNotWeb,
    /// Client did not advertise `x.ai/codeNavigation.enabled`.
    CapabilityNotAdvertised,
    /// `codebase_indexing` feature is disabled in config (or excluded by glob).
    DisabledByConfig,
    /// The cwd is not inside a git repository.
    NotGitRepo,
    /// `sessionId` is required for code navigation but was absent or refers to
    /// an unknown / evicted session.  Per-client capability cannot be determined
    /// without a valid session context.
    SessionRequired,
}
/// Interval between join-handle supervisor sweeps. A panicked/exited actor is
/// reaped within one tick. Kept small so reaping is prompt
/// without busy-spinning the single `LocalSet` thread.
const SESSION_SUPERVISOR_TICK: std::time::Duration = std::time::Duration::from_millis(
    200,
);
/// Upper bound on the `SessionHandle::is_busy` round-trip used by the
/// idle-unload decision (PR-2). Only consulted when no turn is running (so the
/// actor is between turns and responsive); on timeout we conservatively treat
/// the session as busy and keep it resident.
const IDLE_QUERY_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(500);
/// Per-session state freed on removal or idle-unload (but kept across a reload
/// rebuild); retained state instead survives an unload and is freed only at
/// removal.
#[derive(Default)]
struct ResidentResources {
    /// Strong ref pinning the code-nav index; the manager holds only a `Weak`.
    codebase_index: Option<std::sync::Arc<xai_codebase_graph::IndexManagerHandle>>,
    require_gateway: bool,
}
/// Per-session state that survives an idle-unload (so the session stays
/// resumable); freed only at `remove_session`. See [`ResidentResources`].
#[derive(Default)]
struct RetainedResources {
    turn_number: Option<u64>,
    dispatch_lock: Option<std::rc::Rc<tokio::sync::Mutex<()>>>,
    permission_event_receiver: Option<
        tokio::sync::mpsc::UnboundedReceiver<PermissionEvent>,
    >,
}
/// Per-resident-session `(title, last_turn_summary)` display cache; see
/// `resident_roster_titles`.
type RosterDisplayCache = HashMap<String, (Option<String>, Option<String>)>;
pub struct MvpAgent {
    /// LEADER-SAFE(shared): `Send + Sync` mirror of per-session activity for the
    /// leader's auto-update checker, which cannot read the `!Send` maps. Expires
    /// when the actor exits. See [`crate::agent::activity::AgentActivity`].
    pub(crate) activity: crate::agent::activity::AgentActivity,
    /// LEADER-SAFE(per-session).
    session_registry: SessionRegistry,
    /// `(title, last_turn_summary)` per resident session id, refreshed each
    /// `build_roster`. Lets the synchronous roster deltas reuse both instead
    /// of emitting empty ones — `resident_roster_entry` can't read disk.
    resident_roster_titles: RefCell<RosterDisplayCache>,
    pub(crate) initialize_request: OnceLock<acp::InitializeRequest>,
    pub(crate) gateway: GatewaySender,
    /// Agent configuration. LEADER-SAFE(init-once): never mutated after construction.
    pub(crate) cfg: RefCell<AgentConfig>,
    /// Current auth method. LEADER-SAFE(shared): all clients share the same auth;
    /// last authenticate() call wins, which is correct (same user, same creds).
    /// Held as a shared live handle cloned into every running session so a
    /// mid-session `authenticate` (`/login`) is observed by each session's
    /// per-turn auth gate without re-spawning.
    pub(crate) auth_method_id: crate::agent::auth_method::SharedAuthMethodId,
    /// Global sampling config (API key + default base_url). LEADER-SAFE(shared):
    /// only api_key is written here (same for all clients). Per-session base_url
    /// is resolved at session creation time in `new_session` / `load_session`.
    pub(crate) sampling_config: RefCell<SamplingConfig>,
    pub(crate) auth_manager: Arc<AuthManager>,
    pub(crate) models_manager: crate::agent::models::ModelsManager,
    /// Single-flight guard for interactive login (device poll / loopback
    /// wait). Owns the active attempt's cancel token and its code/url
    /// channels; a new `authenticate` or `x.ai/auth/cancel` cancels the
    /// prior attempt.
    /// Client type. LEADER-SAFE(init-once): set once during `initialize` from
    /// `_meta.clientIdentifier` (injected by the IPC server in leader mode).
    ///
    /// **Known limitation (leader mode)**: in a session with multiple concurrent
    /// clients, the last `initialize` call wins and overwrites the global value.
    /// This means per-client telemetry attribution (AB experiments, analytics,
    /// worktree-pool eligibility) uses the identity of whichever client most
    /// recently initialized — not the client that owns the current session.
    ///
    /// This is considered acceptable because `client_type` is used only for
    /// non-safety-critical telemetry and experiment filtering.  Fully per-session
    /// attribution would require threading `clientIdentifier` from `_meta` through
    /// every session handler, which is deferred to future work.
    client_type: RefCell<ClientType>,
    /// Whether the current client advertised `x.ai/codeNavigation.enabled`.
    /// Updated on every `initialize()` call — same last-client-wins semantics
    /// as `client_type`.  Using `Cell<bool>` (not `RefCell`) so `.get()` is a
    /// plain copy with no borrow that could be held across an await point.
    code_nav_enabled: std::cell::Cell<bool>,
    /// Whether the current client advertised `x.ai/folderTrust.interactive` (it
    /// can render the interactive folder-trust prompt). Set on every
    /// `initialize()` (last-client-wins, like `code_nav_enabled`); gates the
    /// DORMANT agent→client trust round-trip in `new_session`/`load_session`.
    /// `Cell<bool>` so `.get()` is a borrow-free copy across await points.
    interactive_trust_client: std::cell::Cell<bool>,
    /// Workspaces (canonical `workspace_key`) already prompted/decided for the
    /// interactive folder-trust round-trip this process — dedups re-prompts on
    /// `load_session` reconnect and concurrent same-workspace sessions. Agent-
    /// owned (mirrors the `DECISIONS` cache, but not a process global), captured
    /// into the detached prompt task; cleared for a workspace on GUI untrust
    /// (`execute_hooks_action`) so a later re-open can re-prompt.
    interactive_trust_prompted: Rc<RefCell<std::collections::HashSet<PathBuf>>>,
    /// Writeback vs local. `Cell` so [`Self::reapply_storage_mode`] can
    /// upgrade it when remote settings land; persistence reads the live value.
    /// Authoritative post-construction — `Config.storage_mode` is only the
    /// boot seed.
    storage_mode: std::cell::Cell<StorageMode>,
    /// Default YOLO mode - when true, sessions start with auto-approve enabled.
    /// Per-session YOLO tracking lives in SessionHandle.yolo_mode.
    default_yolo_mode: bool,
    default_auto_mode: bool,
    /// Memory system configuration (None when --experimental-memory not set).
    memory_config: Option<crate::config::MemoryConfig>,
    /// Optional channel to the leader's `ConfigFileWatcher` for dynamic
    /// per-cwd registration as new sessions open. Each
    /// successful session insert in `spawn_and_register_session` sends
    /// the session's cwd to the watcher task spawned in
    /// `agent/app.rs`, which calls
    /// [`crate::config::watcher::ConfigFileWatcher::watch_path`] (a
    /// **non-recursive** watch on `<cwd>/` and `<cwd>/.grok/`).
    ///
    /// `None` outside leader mode and in tests — the registration is a
    /// no-op in that case, which is fine: the existing per-extra-path
    /// loop already covers the leader's startup cwd.
    /// Plain `Option` (not `RefCell`) — this is written
    /// exactly once, by `set_config_watcher_path_tx(&mut self)` during
    /// leader construction while the agent is still uniquely owned, and
    /// only read thereafter. No interior mutability is required.
    pub(crate) config_watcher_path_tx: Option<
        tokio::sync::mpsc::UnboundedSender<std::path::PathBuf>,
    >,
    relay_sync_enabled: bool,
    /// Buffering configuration. LEADER-SAFE(init-once): set once per connection
    /// during initialize from client capabilities, read when spawning sessions.
    /// In leader mode, the last client to initialize overwrites previous settings
    /// (same caveat as client_type — acceptable for non-safety-critical config).
    buffering_settings: RefCell<Option<update_chunk_merge::BufferingSettings>>,
    /// Context for managing background copy operations (e.g., copying ignored files)
    pub(crate) background_copy_context: BackgroundCopyContext,
    /// LEADER-SAFE(shared): agent-level code-nav index manager, keyed by cwd,
    /// no per-client state.
    codebase_indexes: Arc<parking_lot::Mutex<CodebaseIndexManager>>,
    /// Worktree creation type (resolved: local config > remote > default Linked).
    pub(crate) worktree_type: crate::util::config::WorktreeType,
    /// Restore codebase state on worktree resume (resolved: local config > remote > default false).
    pub(crate) restore_code: bool,
    /// Local session-registry override: `GROK_SESSION_REGISTRY` env, else
    /// `[cli] session_registry`.
    /// `Some(true)` enables, `Some(false)` disables, `None` defers to remote settings.
    session_registry_local: Option<bool>,
    /// Agent-level MCP server state. LEADER-SAFE(shared): MCP servers are
    /// agent-scoped, not per-client.
    agent_mcp_state: std::sync::Arc<
        tokio::sync::Mutex<crate::session::mcp_servers::McpState>,
    >,
    /// Unified sender for all subagent coordinator events.
    /// LEADER-SAFE(shared): channel is multi-producer, coordinator drains.
    subagent_event_tx: tokio::sync::mpsc::UnboundedSender<
        xai_grok_tools::implementations::grok_build::task::types::SubagentEvent,
    >,
    /// Receiver for subagent events. Taken once by `start_subagent_coordinator()`.
    /// `None` after the coordinator drain task has been spawned.
    subagent_event_rx: RefCell<
        Option<
            tokio::sync::mpsc::UnboundedReceiver<
                xai_grok_tools::implementations::grok_build::task::types::SubagentEvent,
            >,
        >,
    >,
    /// Shell-only presentation state; lifecycle lives in the channel actor.
    subagent_presentation: RefCell<crate::agent::subagent::SubagentPresentation>,
    /// Shared buffer for mid-turn monitor event notifications.
    /// Pushed by the `InjectNotification` handler when a turn is active and the
    /// notification has `Next` priority. Drained by the session turn loop
    /// (`inject_pending_monitor_events`) into a hidden synthetic user message.
    monitor_event_buffer: xai_grok_tools::implementations::grok_build::monitor::types::MonitorEventBuffer,
    /// The process launch directory, captured once at construction so the
    /// deferred launch-dir init paths share one source of truth instead of each
    /// re-calling `std::env::current_dir()` (which could drift if the process
    /// cwd ever changes after startup).
    launch_cwd: PathBuf,
    /// Memoizes the single [`folder_trust::resolve_launch_dir_trust`] gather for
    /// the launch dir; see it for the dedup + TOCTOU contract.
    launch_dir_trust: std::cell::OnceCell<bool>,
    /// Shared plugin registry handle.
    pub(crate) plugin_registry_handle: xai_grok_agent::plugins::SharedPluginRegistryHandle,
    /// One-shot guard for the lazy launch-dir population of
    /// `plugin_registry_handle`.
    ///
    /// Boot-time plugin discovery is deferred past ACP `initialize` (it walks
    /// cwd→git root plus user/marketplace dirs and stalled grok-desktop's first
    /// `initialize`), so the shared snapshot starts empty. It is built once on
    /// the first session-creating call via [`Self::ensure_plugin_registry`];
    /// this flag keeps that to a single discovery walk.
    plugin_registry_initialized: std::cell::Cell<bool>,
    /// Single-flight guard for the proactive bundle sync background task.
    ///
    /// `maybe_sync_bundle_in_background` is invoked from each post-auth path
    /// (initialize, cached-token reauth, oidc) and a rapid reconnect can fire
    /// all three within the TTL window, giving us multiple concurrent
    /// `tokio::task::spawn_local` tasks racing to extract the tar archive,
    /// rewrite `manifest.json`, and prune stale files. The non-atomic
    /// per-file write/prune semantics in `bundle::extract_bundle_archive`
    /// make that race observable as a partially-written cache.
    ///
    /// We use an `Arc<AtomicBool>` so the spawned task can clear the flag
    /// on completion without re-borrowing `&self`. `Send` is required
    /// because the inner `sync_bundle_to_root` now uses `spawn_blocking`.
    bundle_sync_in_flight: Arc<std::sync::atomic::AtomicBool>,
    /// Local workspace ops, built lazily via [`Self::ensure_local_workspace_ops`].
    /// The agent never opens Computer Hub as a harness/client; remote cloud
    /// sandboxes are gateway-owned (`gateway_bridge` / `computer_sessions`).
    workspace_ops: RefCell<Option<xai_grok_workspace::WorkspaceOps>>,
    /// Per-session owned local `workspace_server` handles (chat+local `own`).
    #[cfg(all(feature = "local-workspace", unix))]
    local_workspace_supervisors: Rc<
        RefCell<
            HashMap<
                acp::SessionId,
                crate::gateway_bridge::local_workspace_supervisor::LocalWorkspaceHandle,
            >,
        >,
    >,
    /// Invalidates in-flight crash restarts when the session supervisor is reaped.
    #[cfg(all(feature = "local-workspace", unix))]
    local_workspace_generations: Rc<RefCell<HashMap<acp::SessionId, u64>>>,
    /// Sessions whose own supervisor is mid crash-restart (map entry temporarily empty).
    #[cfg(all(feature = "local-workspace", unix))]
    local_workspace_restart_pending: Rc<
        RefCell<std::collections::HashSet<acp::SessionId>>,
    >,
    /// Sessions that already have a local existing workspace (own or attach).
    /// mid-session add refuses while this is set; cleared on session end.
    #[cfg(feature = "local-workspace")]
    local_workspace_bound: Rc<RefCell<std::collections::HashSet<acp::SessionId>>>,
    /// Idempotency guard: the join-handle supervisor task is spawned at most
    /// once (on the first `spawn_and_register_session`). See
    /// `ensure_session_supervisor`.
    supervisor_started: std::cell::Cell<bool>,
    /// Dedup guard for `spawn_settings_reapply`; at most one task in flight.
    /// `Rc` so the drop-guard owns a clone without dereferencing the agent.
    settings_reapply_in_flight: std::rc::Rc<std::cell::Cell<bool>>,
    /// Separate dedup guard for `spawn_post_auth_settings`, so an in-flight
    /// reapply can't coalesce away a freshly authenticated identity's gate and
    /// settings resolution.
    post_auth_settings_in_flight: std::rc::Rc<std::cell::Cell<bool>>,
    /// Test-only spy recording every session id whose cloud replica was
    /// finalized via `finalize_session_replica`. Lets the no-evict tests assert
    /// that `finalize()` does NOT fire on a mere client disconnect (only on a
    /// terminal/explicit close).
    #[cfg(test)]
    finalize_spy: RefCell<Vec<String>>,
    /// Test-only spy recording every terminal roster delta `(session_id,
    /// final_state)` emitted by `record_roster_delta` (reap → `DeadFailed`,
    /// explicit close → `Completed`). Lets tests observe a terminal demotion
    /// even though the `session_live_state` entry is dropped on removal
    /// (the map is kept bounded).
    #[cfg(test)]
    roster_delta_spy: RefCell<Vec<(String, SessionLiveState)>>,
    /// Test-only counter of how many times the join-handle supervisor task was
    /// actually spawned. Asserts `ensure_session_supervisor` is idempotent.
    #[cfg(test)]
    supervisor_spawn_count: std::cell::Cell<usize>,
    /// Test-only: counts `spawn_settings_reapply` tasks spawned past the
    /// in-flight guard.
    #[cfg(test)]
    settings_reapply_spawn_count: std::cell::Cell<usize>,
    /// Test-only: counts `spawn_post_auth_settings` tasks spawned past its
    /// own guard.
    #[cfg(test)]
    post_auth_settings_spawn_count: std::cell::Cell<usize>,
}
/// Spawn a thread to warm the shared async HTTP client (`OnceLock`-cached).
/// Loading TLS root certs is ~95ms; doing it here avoids a cold-start hit
/// on the first request. Idempotent.
pub fn warm_async_http_client() {
    std::thread::spawn(|| {
        let _timer = crate::instrumentation_timer!("startup.async_http_warmup");
        let _ = crate::http::shared_client();
    });
}
pub(crate) fn resolve_required_agent_type(
    model_agent_type: Option<&str>,
    session_default: &str,
) -> String {
    model_agent_type.unwrap_or(session_default).to_owned()
}
/// The harness template a profile should adopt from the model it pins, or
/// `None` to leave it unchanged.
///
/// Lets a profile keep its own identity/prompt/toolset while adopting the
/// template its pinned model needs. Returns `Some` only when the template is
/// still the default (an explicit `userMessageTemplate` wins), the model needs
/// a strict harness, and that harness is non-default. Pure, so the decision is
/// unit-testable without a live catalog.
pub(crate) fn inherited_harness_template(
    current: &xai_grok_agent::prompt::user_message::UserMessageTemplate,
    pinned_model_agent_type: Option<&str>,
    cwd: &std::path::Path,
) -> Option<xai_grok_agent::prompt::user_message::UserMessageTemplate> {
    use xai_grok_agent::prompt::user_message::UserMessageTemplate;
    if !matches!(current, UserMessageTemplate::Default) {
        return None;
    }
    let agent_type = pinned_model_agent_type?;
    if !xai_grok_agent::config::is_strict_harness_agent_type(agent_type) {
        return None;
    }
    let harness = xai_grok_agent::discovery::by_name_in_cwd(agent_type, cwd)?;
    (!matches!(harness.user_message_template, UserMessageTemplate::Default))
        .then_some(harness.user_message_template)
}
/// The `agent_name` a [`crate::session::SessionHandle`] should hold after a
/// model switch.
///
/// `SessionHandle.agent_name` is the harness identity that subagent spawning
/// reads as `parent_agent_name` to decide the child's harness (alternate vs
/// stock), while the child's *model* is read from the parent's live sampling
/// config. The two must stay consistent: a strict-harness model implies the
/// alternate harness.
///
/// When a zero-turn switch rebuilds the harness (`did_rebuild`), the handle
/// must adopt the rebuilt harness's agent type. Otherwise the name is left
/// unchanged — compatible stock switches (e.g. `grok-build` →
/// `grok-build-plan`) intentionally preserve the session's original ACP
/// `agentProfile`.
pub(crate) fn agent_name_after_model_switch(
    did_rebuild: bool,
    rebuilt_agent_type: &str,
    current_agent_name: &str,
) -> String {
    if did_rebuild {
        rebuilt_agent_type.to_owned()
    } else {
        current_agent_name.to_owned()
    }
}
/// Harness compatibility for zero-turn / mid-turn model switching.
///
/// Two stock (non-strict) agents are interchangeable — they share the
/// default wire format and toolset, so switching e.g. `grok-build` →
/// `grok-build-plan` doesn't require rebuilding the harness and would
/// destroy a client-supplied `_meta.agentProfile` if it did.
///
/// Strict harnesses (`codex`, …) are only compatible with
/// themselves. Strict↔stock transitions are never compatible.
pub(crate) fn harnesses_are_compatible(active: &str, required: &str) -> bool {
    use xai_grok_agent::config::is_strict_harness_agent_type;
    match (
        is_strict_harness_agent_type(active),
        is_strict_harness_agent_type(required),
    ) {
        (false, false) => true,
        (true, true) => active == required,
        _ => false,
    }
}
/// Read a string field from `session_meta` first, falling back to
/// `init_meta`. The session path bypasses the `initialize_request`
/// `OnceLock`, so a fresh client can supply `rules` / `systemPromptOverride`
/// even when the leader has been warmed by an earlier client.
fn read_session_or_init_meta_str<'a>(
    session_meta: Option<&'a acp::Meta>,
    init_meta: Option<&'a acp::Meta>,
    key: &str,
) -> Option<&'a str> {
    let read = |m: Option<&'a acp::Meta>| -> Option<&'a str> {
        m.and_then(|m| m.get(key)).and_then(|v| v.as_str())
    };
    read(session_meta).or_else(|| read(init_meta))
}
/// Resolve `startupHints` for a session spawn: the session request `_meta`
/// wins over the connection-level `initialize` `_meta`.
///
/// Same OnceLock-bypass rationale as [`read_session_or_init_meta_str`], and
/// it matters most for headless clients: the shared `initialize_request`
/// holds whichever client initialized this process first, and a leader can
/// multiplex many logical clients — so on a leader-routed `session/load`
/// the init-level hints can belong to a *different* client than the one
/// loading the session. Losing `nonInteractive` silently downgrades
/// `McpInitStrategy::Blocking` to `Progressive`, letting the first prompt
/// of a loaded headless session run while the MCP server carrying its only
/// user-visible output channel is still handshaking.
///
/// The first parseable `startupHints` object wins whole (no per-field
/// merge), mirroring how a client would send it on `initialize`; an
/// unparseable value falls through, matching the sibling helper's
/// treatment of wrong-typed values.
fn startup_hints_from_meta(
    session_meta: Option<&acp::Meta>,
    init_meta: Option<&acp::Meta>,
) -> crate::session::StartupHints {
    explicit_startup_hints(session_meta)
        .or_else(|| explicit_startup_hints(init_meta))
        .unwrap_or_default()
}
/// Parse `startupHints` carried explicitly on one `_meta` object. `None`
/// when absent or unparseable — callers that must distinguish "client made
/// no claim" from "client sent defaults" (the resident re-attach rail) key
/// on this, so an attach without hints never resets a session's policy.
fn explicit_startup_hints(
    meta: Option<&acp::Meta>,
) -> Option<crate::session::StartupHints> {
    meta.and_then(|m| m.get("startupHints"))
        .and_then(|v| serde_json::from_value(v.clone()).ok())
}
use xai_chat_state::conversation_util::replace_or_insert_system_head;
/// Non-empty `systemPromptOverride` from session meta (preferred) or init meta.
/// A blank string (empty or whitespace-only) is treated as "no override" so a
/// client cannot accidentally blank the system prompt.
fn system_prompt_override_from_meta<'a>(
    session_meta: Option<&'a acp::Meta>,
    init_meta: Option<&'a acp::Meta>,
) -> Option<&'a str> {
    read_session_or_init_meta_str(session_meta, init_meta, "systemPromptOverride")
        .filter(|s| !s.trim().is_empty())
}
/// Compose the system prompt for a *fresh* session: a full `systemPromptOverride`
/// verbatim, else the agent template with `_meta.rules` folded into
/// `<human_rules>`. Note: `rules` is applied at creation only — resumed sessions
/// sync `systemPromptOverride` (see `enqueue_replace_system_prompt_override`) but
/// not `rules`, by design.
fn build_spawn_system_prompt(
    session_meta: Option<&acp::Meta>,
    init_meta: Option<&acp::Meta>,
    agent_system_prompt: &str,
) -> String {
    if let Some(override_prompt) = system_prompt_override_from_meta(
        session_meta,
        init_meta,
    ) {
        override_prompt.to_owned()
    } else {
        let mut prompt = agent_system_prompt.to_owned();
        if let Some(rules) = read_session_or_init_meta_str(
            session_meta,
            init_meta,
            "rules",
        ) {
            prompt.push_str("\n\n<human_rules>\n");
            prompt.push_str(rules);
            prompt.push_str("\n</human_rules>");
        }
        prompt
    }
}
/// Enqueue a `ReplaceSystemPrompt` for a resident session actor. No-op when
/// the client sent no (non-empty) `systemPromptOverride`, or when the head
/// already matches (e.g. a cold load that pre-applied the override).
///
/// Note: only `systemPromptOverride` is synced on attach. `_meta.rules` is
/// folded into the prompt at session creation only (see
/// `build_spawn_system_prompt`); resumed sessions keep their original prompt
/// unless a full override is supplied. Updating `rules` mid-session is out of
/// scope by design.
fn enqueue_replace_system_prompt_override(
    cmd_tx: &tokio::sync::mpsc::UnboundedSender<crate::session::SessionCommand>,
    session_meta: Option<&acp::Meta>,
    init_meta: Option<&acp::Meta>,
) {
    let Some(override_prompt) = system_prompt_override_from_meta(session_meta, init_meta)
    else {
        return;
    };
    let _ = cmd_tx
        .send(crate::session::SessionCommand::ReplaceSystemPrompt {
            system_prompt: override_prompt.to_owned(),
        });
}
/// Warn that a `ValidateType` arrived for an evicted/unknown parent session,
/// so ops can diagnose "Unknown subagent type" errors for project agents.
pub(crate) fn warn_on_missing_parent_session_for_validate_type(
    parent_session_id: &str,
    parent_session_present: bool,
) {
    if !parent_session_present {
        tracing::warn!(
            parent_session_id,
            "ValidateType received for unknown parent session — \
             validating against built-ins only",
        );
    }
}
/// Parse an env var as a JSON object. Returns `None` if unset or not a valid JSON object.
pub(crate) fn parse_json_object_env(var: &str) -> Option<serde_json::Value> {
    let val = std::env::var(var).ok()?;
    match serde_json::from_str::<serde_json::Value>(&val) {
        Ok(v) if v.is_object() => Some(v),
        Ok(_) => {
            tracing::warn!("{var} is not a JSON object, ignoring");
            None
        }
        Err(e) => {
            tracing::warn!("{var} is invalid JSON: {e}");
            None
        }
    }
}
#[derive(Debug, Default, serde::Deserialize)]
struct AuthRequestMeta {
    #[serde(default)]
    headless: bool,
    #[serde(default)]
    reauth: bool,
    /// `--oauth`: force loopback. The only transport override sent over ACP
    /// (loopback is the default; device is opt-in via env/config).
    #[serde(default)]
    use_oauth: bool,
    /// When true, skip cached tokens and force the interactive browser login
    /// flow. Used by the `/login` slash command for mid-session re-auth.
    /// Unlike `reauth`, this does NOT clear existing credentials — if the
    /// user abandons the browser flow, the current session continues.
    #[serde(default)]
    force_interactive: bool,
    /// Pager auth `request_seq` for this attempt. Scopes `x.ai/auth/cancel`
    /// so a delayed cancel cannot tear down a successor login.
    #[serde(default)]
    request_seq: Option<u64>,
}
impl AuthRequestMeta {
    /// `--oauth` → force loopback; otherwise default (loopback).
    fn login_override(&self) -> crate::auth::LoginTransportOverride {
        if self.use_oauth {
            crate::auth::LoginTransportOverride::ForceLoopback
        } else {
            crate::auth::LoginTransportOverride::None
        }
    }
    fn from_json(meta: Option<&acp::Meta>) -> Self {
        meta.cloned()
            .and_then(|value| {
                serde_json::from_value(serde_json::Value::Object(value)).ok()
            })
            .unwrap_or_default()
    }
}
/// Inject standard proxy headers into an `extra_headers` map.
///
/// Every authenticated request to cli-chat-proxy (web search, image gen, and
/// any future tools that go through the proxy) must carry these headers.
/// Centralising them here means new tool code paths only need one call instead
/// of remembering which headers the proxy expects.
///
/// Headers injected:
///  - `x-grok-client-version` -- required by the proxy's version-gate check.
///    Uses `client_version` when provided, otherwise falls back to cli-chat-proxy
///    compile-time `CARGO_PKG_VERSION`.
///  - `X-XAI-Token-Auth` / `x-authenticateresponse` -- required by the
///    cli-chat-proxy auth middleware when the `base_url` is a known proxy URL.
///  - optional extra access header -- only set when the corresponding key is
///    `Some` *and* the `base_url` points at a matching non-production host
///    (requires the optional non-production feature).
///
/// Existing entries are never overwritten so callers can pre-set a value.
fn inject_proxy_headers(
    headers: &mut indexmap::IndexMap<String, String>,
    client_version: Option<&str>,
    alpha_test_key: Option<&str>,
    base_url: &str,
) {
    if cfg!(test) {
        headers
            .entry("x-grok-client-version".to_string())
            .or_insert_with(|| {
                client_version
                    .map(String::from)
                    .unwrap_or_else(|| xai_grok_version::VERSION.to_string())
            });
        headers
            .entry("x-grok-client-identifier".to_string())
            .or_insert_with(crate::http::process_client_identifier);
    }
    if cfg!(test) && crate::util::is_cli_chat_proxy_url(base_url) {
        // The proxy-specific markers remain only for compatibility fixtures.
        // A clean production build sends provider-neutral headers.
        headers
            .entry("X-XAI-Token-Auth".to_string())
            .or_insert_with(|| "xai-grok-cli".to_string());
        headers
            .entry("x-authenticateresponse".to_string())
            .or_insert_with(|| "authenticate-response".to_string());
        headers
            .entry(crate::http::CLIENT_MODE_HEADER.to_string())
            .or_insert_with(|| crate::http::process_client_mode().to_string());
    }
    let _ = (alpha_test_key, base_url);
}
fn resolve_inference_idle_timeout_secs(
    models: &indexmap::IndexMap<String, crate::agent::config::ModelEntry>,
    model: &str,
    remote_settings: Option<&crate::util::config::RemoteSettings>,
) -> u64 {
    let per_model = models
        .get(model)
        .or_else(|| models.values().find(|entry| entry.info.model == model))
        .and_then(|entry| entry.info.inference_idle_timeout_secs);
    let remote = remote_settings.and_then(|s| s.inference_idle_timeout_secs);
    per_model.or(remote).unwrap_or(600).max(10)
}
/// Parse the client-advertised `x.ai/hunkTracker.mode` string. Case-insensitive
/// and trimmed. Absent/blank/`off`/`disabled` => `None`; unknown => `AllDirty`.
fn resolve_hunk_tracking_mode(
    mode_str: Option<&str>,
) -> Option<xai_hunk_tracker::TrackingMode> {
    let mode = mode_str.map(str::trim)?;
    if mode.is_empty() || mode.eq_ignore_ascii_case("off")
        || mode.eq_ignore_ascii_case("disabled")
    {
        return None;
    }
    Some(
        serde_json::from_value(serde_json::Value::String(mode.to_ascii_lowercase()))
            .unwrap_or(xai_hunk_tracker::TrackingMode::AllDirty),
    )
}
/// Session wiring derived from the resolved tracking mode. Disabling the tracker
/// (`actor_mode == None`) turns off the actor, the per-event forward, and the
/// LOC sink together, so the disable path can't be left half-wired.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct HunkTrackingPlan {
    /// `Some` → spawn the actor in this mode; `None` → use `noop()`, no actor.
    actor_mode: Option<xai_hunk_tracker::TrackingMode>,
}
impl HunkTrackingPlan {
    /// Gate for the fs-notify forward sites (via `ToolContext.hunk_tracking_enabled`)
    /// and LOC-sink eligibility.
    fn enabled(&self) -> bool {
        self.actor_mode.is_some()
    }
}
fn plan_hunk_tracking(mode_str: Option<&str>) -> HunkTrackingPlan {
    HunkTrackingPlan {
        actor_mode: resolve_hunk_tracking_mode(mode_str),
    }
}
/// Bound on waiting out an evicted session's flushing actor thread.
pub(super) const DRAIN_OLD_THREAD_WAIT: std::time::Duration = std::time::Duration::from_secs(
    5,
);
/// RAII marker for an in-flight attach; dropping it wakes every waiter.
pub(crate) struct SessionLoadGuard<'a> {
    agent: &'a MvpAgent,
    session_id: acp::SessionId,
    rx: tokio::sync::watch::Receiver<bool>,
    /// Dropped with the guard — closes the watch channel, waking waiters.
    _tx: tokio::sync::watch::Sender<bool>,
}
impl Drop for SessionLoadGuard<'_> {
    fn drop(&mut self) {
        self.agent.session_registry.settle_attach(&self.session_id, &self.rx);
    }
}
mod code_nav;
mod folder_trust_prompt;
mod resource_telemetry;
mod session_registry;
mod session_lifecycle;
mod subagent_coordinator;
mod agent_ops;
mod acp_agent;
mod session_setup;
use session_registry::SessionRegistry;
pub(crate) use session_lifecycle::RegistrySnapshot;
pub(super) use super::ext_parsers;
/// Emit the `auth.lifecycle` login span with an optional error category.
/// Named `auth.lifecycle` (not `auth`) to avoid colliding with the
/// pre-existing per-request `AuthManager::auth()` `#[instrument]` span.
fn emit_login_span(
    success: bool,
    auth_method: &str,
    error_category: Option<&str>,
) {
    let span = tracing::info_span!(
        "auth.lifecycle",
        action = "login",
        success,
        auth_method,
        error_category = tracing::field::Empty,
    );
    if let Some(ec) = error_category {
        span.record("error_category", ec);
    }
    span.in_scope(|| {});
}
/// Metadata captured from a replayed `task_backgrounded` entry.
pub(crate) struct OrphanedTask {
    task_id: String,
    command: String,
    cwd: String,
}
impl MvpAgent {
    /// Scan persisted updates for `task_backgrounded` entries that have no
    /// matching `task_completed`. Applies rewind dead-branch filtering so
    /// tasks from rewound branches are not included.
    pub(super) fn find_orphaned_background_tasks(
        updates_file_path: &Option<PathBuf>,
    ) -> Vec<OrphanedTask> {
        use crate::session::wire_tags::{TASK_BACKGROUNDED, TASK_COMPLETED};
        let Some(updates_path) = updates_file_path else {
            return Vec::new();
        };
        let contents = match std::fs::read_to_string(updates_path) {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        let all_lines: Vec<&str> = contents
            .lines()
            .filter(|l| !l.trim().is_empty())
            .collect();
        let live_lines = crate::session::storage::filter_rewind_lines(all_lines);
        let mut pending = std::collections::HashMap::<String, OrphanedTask>::new();
        for line in live_lines {
            if !line.contains(&*TASK_BACKGROUNDED) && !line.contains(&*TASK_COMPLETED) {
                continue;
            }
            let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };
            let update = &v["params"]["update"];
            match update["sessionUpdate"].as_str() {
                Some(tag) if tag == *TASK_BACKGROUNDED => {
                    if let Some(id) = update["task_id"].as_str() {
                        pending
                            .insert(
                                id.to_string(),
                                OrphanedTask {
                                    task_id: id.to_string(),
                                    command: update["command"]
                                        .as_str()
                                        .unwrap_or_default()
                                        .to_string(),
                                    cwd: update["cwd"].as_str().unwrap_or_default().to_string(),
                                },
                            );
                    }
                }
                Some(tag) if tag == *TASK_COMPLETED => {
                    if let Some(id) = update["task_snapshot"]["task_id"].as_str() {
                        pending.remove(id);
                    }
                }
                _ => {}
            }
        }
        pending.into_values().collect()
    }
    /// Emit `task_completed` for background tasks that were replayed as
    /// "Running" but whose processes no longer exist (cold session load).
    /// Returns completion receivers so the caller can drain them before
    /// returning LoadSessionResponse.
    pub(super) fn reconcile_stale_background_tasks(
        &self,
        session_id: &acp::SessionId,
        updates_file_path: &Option<PathBuf>,
    ) -> Vec<tokio::sync::oneshot::Receiver<xai_acp_lib::AcpResult<()>>> {
        let orphaned = Self::find_orphaned_background_tasks(updates_file_path);
        if orphaned.is_empty() {
            return Vec::new();
        }
        if self.is_resident(session_id) {
            return Vec::new();
        }
        let mut completions = Vec::with_capacity(orphaned.len());
        for task in &orphaned {
            let snapshot = xai_grok_tools::types::TaskSnapshot {
                task_id: task.task_id.clone(),
                command: task.command.clone(),
                display_command: None,
                cwd: task.cwd.clone(),
                start_time: std::time::SystemTime::now(),
                end_time: Some(std::time::SystemTime::now()),
                output: String::new(),
                output_file: std::path::PathBuf::new(),
                truncated: false,
                exit_code: None,
                signal: Some("session_restart".to_string()),
                completed: true,
                kind: xai_grok_tools::computer::types::TaskKind::Bash,
                block_waited: false,
                explicitly_killed: false,
                owner_session_id: None,
                description: None,
                is_backgrounded: true,
                output_total_bytes: 0,
            };
            let mut notification = crate::extensions::notification::SessionNotification {
                session_id: session_id.clone(),
                update: crate::extensions::notification::SessionUpdate::TaskCompleted {
                    task_snapshot: snapshot,
                    will_wake: false,
                },
                meta: None,
            };
            if let Some(params) = crate::tools::task_completed_frame::encode(
                &mut notification,
            ) {
                completions
                    .push(
                        self
                            .gateway
                            .forward_with_completion(
                                acp::ExtNotification::new(
                                    "x.ai/task_completed",
                                    params.into_inner().into(),
                                ),
                            ),
                    );
            }
        }
        if !completions.is_empty() {
            tracing::info!(
                session_id = %session_id.0,
                stale_count = completions.len(),
                "Emitted task_completed for stale background tasks"
            );
        }
        completions
    }
    /// Extracts initial_total_tokens by scanning only the tail of the updates file.
    /// Avoids loading and deserializing all updates when replay is skipped (noReplay).
    pub(super) fn extract_initial_tokens_from_updates(
        updates_file_path: &Option<PathBuf>,
    ) -> u64 {
        use std::io::{Read, Seek, SeekFrom};
        let Some(updates_path) = updates_file_path else {
            return 0;
        };
        let mut file = match std::fs::File::open(updates_path) {
            Ok(f) => f,
            Err(_) => return 0u64,
        };
        let file_len = match file.metadata() {
            Ok(m) => m.len(),
            Err(_) => return 0,
        };
        const TAIL_SIZE: u64 = 64 * 1024;
        let start_pos = file_len.saturating_sub(TAIL_SIZE);
        if file.seek(SeekFrom::Start(start_pos)).is_err() {
            return 0;
        }
        let mut buf = String::new();
        if file.read_to_string(&mut buf).is_err() {
            return 0;
        }
        let result = buf
            .lines()
            .rev()
            .filter(|line| !line.trim().is_empty())
            .find_map(|line| {
                let value: serde_json::Value = serde_json::from_str(line).ok()?;
                value.get("params")?.get("meta")?.get("totalTokens")?.as_u64()
            })
            .unwrap_or(0);
        if result == 0 {
            tracing::warn!(
                path = %updates_path.display(),
                "extract_initial_tokens: no totalTokens found in updates tail, \
                 token tracking will rely on conversation estimate until first model response"
            );
        }
        result
    }
    pub(crate) fn auth_response_with_meta(&self) -> AuthenticateResponse {
        let show_resolved_model = {
            let cfg = self.cfg.borrow();
            let rs = cfg.remote_settings.as_ref();
            rs.and_then(|s| s.show_resolved_model)
        };
        let meta = self
            .auth_manager
            .current()
            .map(|auth| {
                let auth_meta = crate::auth::AuthMeta {
                    email: auth.email.clone(),
                    auth_mode: Some(format!("{:?}", auth.auth_mode)),
                    team_id: auth.team_id.clone(),
                    team_name: auth.team_name.clone(),
                    is_zdr: auth.is_zdr_team(),
                    team_role: auth.team_role.clone(),
                    coding_data_retention_opt_out: auth.coding_data_retention_opt_out,
                    show_resolved_model,
                    gate: None,
                    subscription_tier: None,
                };
                serde_json::to_value(auth_meta)
                    .ok()
                    .and_then(|v| v.as_object().cloned())
                    .unwrap_or_default()
            });
        AuthenticateResponse::new().meta(meta)
    }
    /// Fetch remote settings after authentication when early prefetch had none.
    /// Notifies the pager so soft-default permission_mode applies post-login.
    pub(super) async fn maybe_fetch_post_auth_settings(&self) {
        if self.cfg.borrow().remote_settings.is_some() {
            return;
        }
        if !crate::util::config::resolve_remote_fetch_enabled() {
            return;
        }
        let Some(auth) = self.auth_manager.current() else {
            return;
        };
        let Some(settings) = self.fetch_post_auth_settings_disabled(&auth).await else {
            return;
        };
        tracing::info!("post-auth remote_settings fetch succeeded");
        self.install_remote_settings(settings);
        self.spawn_auto_worktree_gc();
    }
    /// Resolve current auto-GC policy and run it on the blocking pool.
    pub(super) fn spawn_auto_worktree_gc(&self) {
        let auto_gc_policy = self.cfg.borrow().resolve_worktree_auto_gc();
        tokio::task::spawn_blocking(move || {
            let opts = xai_fast_worktree::AutoGcOptions::from_resolved(auto_gc_policy);
            if let Err(e) = xai_fast_worktree::WorktreeDb::open_default()
                .and_then(|db| xai_fast_worktree::maybe_auto_gc(&db, &opts))
            {
                tracing::warn!(error = %e, "auto worktree gc failed");
            }
        });
    }
    /// Fire-and-forget `x.ai/settings/update` from the current remote snapshot.
    pub(super) fn emit_settings_update_notification(&self) {
        let payload = {
            let cfg = self.cfg.borrow();
            let rs = cfg.remote_settings.as_ref();
            SettingsUpdateNotification {
                show_resolved_model: rs.and_then(|s| s.show_resolved_model),
                sharing_enabled: rs.and_then(|s| s.sharing_enabled),
                privacy_notice_rollout: rs.and_then(|s| s.privacy_notice_rollout),
                privacy_banner_reshow_days: rs
                    .and_then(|s| s.privacy_banner_reshow_days),
                session_picker_grouped: rs.and_then(|s| s.session_picker_grouped),
                tips: rs.and_then(|s| s.tips.clone()),
                slash_command_tags: rs.and_then(|s| s.slash_command_tags.clone()),
                auto_permission_mode_enabled: crate::util::config::remote_auto_mode_enabled(
                    rs,
                ),
                permission_mode: rs.and_then(|s| s.permission_mode.clone()),
                group_tool_verbs: rs.and_then(|s| s.group_tool_verbs),
                collapsed_edit_blocks: rs.and_then(|s| s.collapsed_edit_blocks),
            }
        };
        if let Ok(params) = serde_json::value::to_raw_value(&payload) {
            self.gateway
                .forward_fire_and_forget(
                    acp::ExtNotification::new("x.ai/settings/update", params.into()),
                );
        }
    }
    /// Fan out `RefreshSkillBaseline` to each provided sender.
    pub(super) fn broadcast_refresh_skill_baseline(
        senders: Vec<tokio::sync::mpsc::UnboundedSender<crate::session::SessionCommand>>,
    ) {
        for tx in senders {
            let _ = tx.send(crate::session::SessionCommand::RefreshSkillBaseline);
        }
    }
    /// Snapshot live session senders and broadcast `RefreshSkillBaseline`.
    pub(super) fn refresh_skill_baseline_for_all_sessions(&self) {
        let senders = self.resident_cmd_txs();
        Self::broadcast_refresh_skill_baseline(senders);
    }
    /// Eagerly fan out the current on-disk plugin registry to every live
    /// session so each adopts a cwd-correct snapshot (hooks + MCP + skills +
    /// client slash-command catalog) — the same refresh the session where the
    /// plugin changed already gets. Mirrors the MCP fan-out in
    /// `handle_plugins_reload`, extended to the whole registry. Each session
    /// gets its own `build_for_cwd` result because project-scoped plugins
    /// differ by working directory. `skip` avoids redundant work on a session
    /// that just self-updated (the originating session of a per-session
    /// reload). Subagents are skipped by the receiving actor.
    pub(crate) fn broadcast_plugin_registry_to_sessions(
        &self,
        skip: Option<&acp::SessionId>,
    ) {
        let targets: Vec<
            (
                std::path::PathBuf,
                tokio::sync::mpsc::UnboundedSender<crate::session::SessionCommand>,
            ),
        > = {
            let mut targets = Vec::new();
            self.session_registry
                .for_each_resident(|sid, h| {
                    if skip == Some(sid) {
                        return;
                    }
                    targets
                        .push((std::path::PathBuf::from(&h.info.cwd), h.cmd_tx.clone()));
                });
            targets
        };
        let remote_settings = self.cfg.borrow().remote_settings.clone();
        for (cwd, cmd_tx) in targets {
            let project_trusted = folder_trust::resolve_and_record(
                cwd.as_path(),
                remote_settings.as_ref(),
                false,
            );
            let disk_cfg = crate::config::resolve_effective_plugins_config(cwd.as_path())
                .to_discovery_config();
            let registry = self
                .plugin_registry_handle
                .build_for_cwd(cwd.as_path(), &disk_cfg, &[], project_trusted);
            let _ = cmd_tx
                .send(crate::session::SessionCommand::ReloadPlugins {
                    registry,
                });
        }
    }
    /// Hosted bundled-agent catalogs are not part of the local runtime.
    /// Keep this compatibility hook inert so initialization cannot resolve
    /// credentials or start a hosted fetch task.
    pub(crate) fn maybe_sync_bundle_in_background(&self, _force: bool) {
    }
}
mod replay;
#[cfg(test)]
mod replay_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod prompt_response_meta_tests;
