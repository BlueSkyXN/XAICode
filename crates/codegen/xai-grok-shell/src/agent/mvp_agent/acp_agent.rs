#![cfg_attr(rustfmt, rustfmt::skip)]
#![allow(unused_imports)]
//! [`acp::Agent`] trait implementation for [`MvpAgent`].
//! Co-located child of `mvp_agent` (`use super::*`).
use super::*;
use super::agent_ops::apply_yolo_mode_to_matching_sessions;
use crate::auth::PreferredAuthMethod;
use crate::leader::protocol::InternalMethod;
/// Which `x_search` sub-tools enforce the date cutoff, sent in `initialize`. `x_user_search` and
/// `x_thread_fetch` are `false`: they don't honor it yet.
#[derive(serde::Serialize)]
struct ToolOverridesCapability {
    x_keyword_search: bool,
    x_semantic_search: bool,
    x_user_search: bool,
    x_thread_fetch: bool,
}
const TOOL_OVERRIDES_CAPABILITY: ToolOverridesCapability = ToolOverridesCapability {
    x_keyword_search: true,
    x_semantic_search: true,
    x_user_search: false,
    x_thread_fetch: false,
};
fn tool_overrides_capability() -> serde_json::Value {
    serde_json::to_value(TOOL_OVERRIDES_CAPABILITY)
        .expect("ToolOverridesCapability is always serializable")
}
async fn read_applied_tool_overrides(
    cmd_tx: &tokio::sync::mpsc::UnboundedSender<SessionCommand>,
) -> Option<xai_grok_sampling_types::ToolOverrides> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    if cmd_tx
        .send(SessionCommand::GetToolOverrides {
            respond_to: tx,
        })
        .is_err()
    {
        tracing::warn!("tool-overrides echo: session actor command channel closed");
        return None;
    }
    match rx.await {
        Ok(overrides) => overrides,
        Err(_) => {
            tracing::warn!("tool-overrides echo: session actor dropped the response channel");
            None
        }
    }
}
fn insert_applied_tool_overrides(
    meta: &mut serde_json::Map<String, serde_json::Value>,
    echo: Option<&xai_grok_sampling_types::ToolOverrides>,
) {
    if let Some(overrides) = echo {
        meta.insert(
            "toolOverrides".to_string(),
            serde_json::to_value(overrides)
                .expect("ToolOverrides is always serializable"),
        );
    }
}

#[async_trait::async_trait(?Send)]
impl acp::Agent for MvpAgent {
    /// In the meta, we provide
    ///   - model_state: the model state, useful for the client to display available models and the default model.
    ///
    /// SINGLE-CALL INVARIANT: this method is the sole writer of
    /// `self.auth_method_id` during initialization. It is called exactly once
    /// per agent process by the ACP server before any session-creating
    /// requests, while `auth_method_id` is still `None` (initialized at
    /// `MvpAgent::new`). The auth-method block below relies on that
    /// invariant when it unconditionally writes the default id returned by
    /// `auth_method::build_auth_methods`. If you ever need to call
    /// `initialize()` more than once, restore an `is_none()` guard around
    /// the `auth_method_id` write at the call site so a re-init doesn't
    /// silently downgrade an api-key user to a session-token user.
    async fn initialize(
        &self,
        arguments: acp::InitializeRequest,
    ) -> Result<acp::InitializeResponse, acp::Error> {
        tracing::debug!(target: "sampling_log", "Received initialize request");
        xai_grok_telemetry::unified_log::info("agent initialized", None, None);
        xai_grok_telemetry::startup::mark_agent_serving();
        self.start_subagent_coordinator();
        let (auto_gc_policy, run_auto_gc) = {
            let cfg = self.cfg.borrow();
            let has_remote = cfg.remote_settings.is_some();
            let run = has_remote || !crate::util::config::resolve_remote_fetch_enabled();
            (cfg.resolve_worktree_auto_gc(), run)
        };
        if !run_auto_gc {
            tracing::debug!(
                "auto worktree gc deferred until remote_settings are available"
            );
        }
        tokio::task::spawn_blocking(move || {
            crate::session::worktree_pool::cleanup_stale_pool_worktrees(None);
            if !run_auto_gc {
                return;
            }
            let opts = xai_fast_worktree::AutoGcOptions::from_resolved(auto_gc_policy);
            if let Err(e) = xai_fast_worktree::WorktreeDb::open_default()
                .and_then(|db| xai_fast_worktree::maybe_auto_gc(&db, &opts))
            {
                tracing::warn!(error = %e, "auto worktree gc failed");
            }
        });
        tokio::task::spawn_blocking(|| {
            crate::session::persistence::cleanup_stale_sessions(None);
        });
        {
            let root = crate::util::grok_home::grok_home();
            crate::session::storage::search::SEARCH_INDEX_MANAGER.bootstrap_once(root);
        }
        const PERMISSION_CLEANUP_TTL_DAYS: u64 = 30;
        static CLEANUP_PERMISSIONS_ONCE: std::sync::Once = std::sync::Once::new();
        CLEANUP_PERMISSIONS_ONCE
            .call_once(|| {
                tokio::task::spawn(
                    xai_grok_workspace::permission::cleanup_stale_permission_state(
                        std::time::Duration::from_secs(
                            PERMISSION_CLEANUP_TTL_DAYS * 24 * 60 * 60,
                        ),
                    ),
                );
            });
        xai_grok_workspace::trust::migrate_legacy_hook_trust();
        let mut client_type = arguments
            .meta
            .as_ref()
            .and_then(|m| m.get("clientType"))
            .and_then(|v| serde_json::from_value::<ClientType>(v.clone()).ok())
            .unwrap_or_default();
        let client_identifier = arguments
            .meta
            .as_ref()
            .and_then(|m| m.get("clientIdentifier"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        if let Some(ref id) = client_identifier {
            tracing::info!("Client identifier set to: {}", id);
        }
        if client_type == ClientType::Generic {
            match client_identifier.as_deref() {
                Some("grok-web") => client_type = ClientType::GrokWeb,
                Some("nebula") => client_type = ClientType::Nebula,
                Some("grok-code-extension") => client_type = ClientType::Extension,
                Some("grok-desktop") => client_type = ClientType::Desktop,
                _ => {}
            }
        }
        *self.client_type.borrow_mut() = client_type;
        tracing::info!("Client type set to: {:?}", client_type);
        let code_nav_enabled = Self::parse_code_nav_capability(&arguments);
        self.code_nav_enabled.set(code_nav_enabled);
        tracing::info!(
            code_nav_enabled,
            client_type = ?client_type,
            event = "code_nav_capability_parsed",
            "code-nav capability initialized from initialize request; \
             index will start lazily on first x.ai/code/* request if eligible"
        );
        let interactive_trust_client = Self::parse_interactive_trust_capability(
            &arguments,
        );
        self.interactive_trust_client.set(interactive_trust_client);
        let client_supports_mcp_apps = arguments
            .meta
            .as_ref()
            .and_then(|m| m.get("mcpApps"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if client_supports_mcp_apps {
            tracing::info!("Client supports MCP Apps");
        }
        let buffering_settings = arguments
            .meta
            .as_ref()
            .and_then(|m| m.get("bufferingSettings"))
            .map(|value| serde_json::from_value::<
                update_chunk_merge::BufferingSettings,
            >(value.clone()))
            .transpose()
            .map_err(|err| {
                tracing::warn!(
                    error = ?err,
                    "Failed to parse buffering settings from init meta"
                );
                err
            })
            .unwrap_or(None);
        tracing::info!(?buffering_settings, "Buffering settings from init");
        *self.buffering_settings.borrow_mut() = buffering_settings;
        if self.initialize_request.set(arguments).is_err() {
            tracing::info!("Initialize called on reconnect (already initialized)");
        }
        // Account/OIDC and cached-session initialization is disabled. Generic
        // provider keys are resolved by the local sampling configuration.
        let disable_api_key_auth = false;
        let first_party_env_ok = true;
        let has_external_api_key = auth_method::should_advertise_xai_api_key(
            disable_api_key_auth,
            self.models_manager.models().values(),
        );
        let init_has_current = false;
        let init_is_expired = false;
        xai_grok_telemetry::unified_log::info(
            "auth init token state",
            None,
            Some(
                serde_json::json!({
                "has_current": init_has_current,
                "is_expired": init_is_expired,
            }),
            ),
        );
        // The clean build has one credential family only: a provider API key.
        // Ignore stale account/OIDC/provider-command preferences from an old
        // config file instead of allowing them to hide the generic method or
        // reintroduce an interactive login choice.
        let has_cached_token = false;
        let login_label: Option<String> = None;
        let has_auth_provider = false;
        let has_enterprise_oidc = false;
        let enterprise_oidc_issuer: Option<String> = None;
        let preferred_method: Option<PreferredAuthMethod> = None;
        let built = auth_method::build_auth_methods(auth_method::AuthMethodsBuildInputs {
            has_external_api_key,
            has_cached_token,
            has_enterprise_oidc,
            enterprise_oidc_issuer: enterprise_oidc_issuer.as_deref(),
            login_label: login_label.as_deref(),
            has_auth_provider_command: has_auth_provider,
            preferred_method,
        });
        let auth_methods = built.methods;
        xai_grok_telemetry::unified_log::info(
            "auth: initialize() built auth_methods for ACP response",
            None,
            Some(
                serde_json::json!({
                "has_external_api_key": has_external_api_key,
                "first_party_env_api_key_ok": first_party_env_ok,
                "disable_api_key_auth": disable_api_key_auth,
                "has_cached_token": has_cached_token,
                "has_enterprise_oidc": has_enterprise_oidc,
                "init_has_current": init_has_current,
                "init_is_expired": init_is_expired,
                "methods": auth_methods.iter().map(|m| m.id().0.as_ref()).collect::<Vec<_>>(),
                "default_auth_method_id": built.default_auth_method_id.as_ref().map(|id| id.0.as_ref()),
            }),
            ),
        );
        debug_assert!(
            !has_external_api_key
                || matches!(
                    auth_methods
                        .first()
                        .map(|m| auth_method::AuthMethodKind::from_id(m.id())),
                    Some(auth_method::AuthMethodKind::XaiApiKey)
                ),
            "BYOK invariant violated: xai.api_key MUST be auth_methods.first() \
             when has_external_api_key is true; got {:?}",
            auth_methods.first().map(|m| m.id()),
        );
        let default_auth_method_id_wire: Option<String> = built
            .default_auth_method_id
            .as_ref()
            .map(|id| id.0.to_string());
        if let Some(default_id) = built.default_auth_method_id {
            xai_grok_telemetry::unified_log::info(
                "auth method selection",
                None,
                Some(
                    serde_json::json!({
                    "default_auth_method_id": default_id.0.as_ref(),
                    "has_external_api_key": has_external_api_key,
                    "has_cached_token": has_cached_token,
                    "methods_first": auth_methods.first().map(|m| m.id().0.as_ref()),
                    "methods_count": auth_methods.len(),
                }),
                ),
            );
            self.set_auth_method(default_id);
        }
        self.sync_process_static_api_key(None);
        let current_working_directory = self.launch_cwd.clone();
        let hostname = gethostname::gethostname();
        let mcp_servers: Vec<crate::extensions::mcp::McpServerEntry> = Vec::new();
        // Local MCP configuration remains available. Hosted catalogs and
        // announcements have no production consumer in this composition.
        self.spawn_initialize_launch_mcp_setup();
        let init_model_state = self.model_state(None);
        let session_capabilities = acp::SessionCapabilities::new()
            .close(acp::SessionCloseCapabilities::new());
        let session_capabilities = session_capabilities
            .list(acp::SessionListCapabilities::new())
            .resume(acp::SessionResumeCapabilities::new());
        Ok(
            acp::InitializeResponse::new(acp::ProtocolVersion::V1)
                .agent_capabilities(
                    acp::AgentCapabilities::new()
                        .load_session(true)
                        // These are local ACP compatibility capabilities. The
                        // wire names are retained because existing MCP/plugin
                        // clients use them; they do not identify a hosted
                        // account or open a remote connection by themselves.
                        .meta(
                            serde_json::json!({
                                "x.ai/fs_notify": true,
                                "x.ai/hooks": {
                                    "blockingEvents": crate::extensions::hooks::ADVERTISED_BLOCKING_EVENTS,
                                    "decisions": crate::extensions::hooks::ADVERTISED_DECISIONS,
                                    "stopSignals": crate::extensions::hooks::ADVERTISED_STOP_SIGNALS,
                                },
                                "x.ai/capabilities": {
                                    "toolOverrides": tool_overrides_capability(),
                                },
                            })
                            .as_object()
                            .cloned(),
                        )
                        .prompt_capabilities(
                            acp::PromptCapabilities::new().embedded_context(true),
                        )
                        .mcp_capabilities(
                            acp::McpCapabilities::new().http(true).sse(true),
                        )
                        .session_capabilities(session_capabilities),
                )
                .auth_methods(auth_methods)
                .meta({
                    let metadata = parse_json_object_env("GROK_AGENT_METADATA");
                    serde_json::json!({
                    "codingAgent": true,
                    // Re-deriving this precedence client-side has regressed OIDC
                    // refresh, so clients consume the agent's choice from here.
                    "defaultAuthMethodId": default_auth_method_id_wire,
                    // The local agent still supports in-process SDK MCP servers
                    // over the ACP reverse channel. This is a transport
                    // capability, not a hosted vendor integration.
                    (xai_grok_mcp::wire::MCP_SDK): true,
                    // `session/new` / `session/load` accept per-session plugin roots in
                    // `_meta.pluginDirs`; local clients gate plugin roots on this.
                    (SESSION_PLUGIN_DIRS_CAPABILITY_KEY): true,
                    "currentWorkingDirectory": current_working_directory.to_string_lossy().to_string(),
                    "agentVersion": xai_grok_version::VERSION,
                    "agentId": serde_json::Value::Null,
                    "agentInstanceId": "xaicode-local",
                    "hostname": hostname.to_string_lossy().to_string(),
                    "modelState": init_model_state,
                    "mcpServers": mcp_servers,
                    "mcpApps": client_supports_mcp_apps,
                    "metadata": metadata,
                    "availableCommands": crate::session::slash_commands::builtin_commands(self.command_availability()),
                    "cancelRewind": self.cfg.borrow().resolve_cancel_rewind().value,
                    "sessionRecap": false,
                    "voiceMode": false,
                })
                        .as_object()
                        .cloned()
                }),
        )
    }
    async fn authenticate(
        &self,
        arguments: acp::AuthenticateRequest,
    ) -> Result<AuthenticateResponse, acp::Error> {
        tracing::info!(method = %arguments.method_id.0, "auth: authenticate request");
        if arguments.method_id.0.as_ref() != auth_method::XAI_API_KEY_METHOD_ID {
            return Err(acp::Error::auth_required()
                .data("Interactive account login and cached session authentication are disabled in the clean local build."));
        }
        if self.cfg.borrow().grok_com_config.api_key_auth_disabled() {
            emit_login_span(false, "api_key", Some("disabled_by_admin"));
            return Err(
                acp::Error::auth_required()
                    .data("API-key auth is disabled by your administrator."),
            );
        }
        let mut sampling_config = self.sampling_config.borrow_mut();
        if sampling_config.api_key.is_none() {
            if let Ok(api_key) = auth_method::read_xai_api_key_env() {
                // Provider credentials are process-local and never persisted to
                // the removed xAI account/auth.json format.
                sampling_config.api_key = Some(api_key);
            } else if !self
                .models_manager
                .models()
                .values()
                .any(|m| m.has_own_credentials())
            {
                emit_login_span(false, "api_key", Some("no_credentials"));
                return Err(
                    acp::Error::auth_required()
                        .data("Set CODING_AGENT_API_KEY, OPENAI_API_KEY, or add api_key/env_key to config.toml."),
                );
            }
        }
        self.set_auth_method(arguments.method_id);
        self.sync_process_static_api_key(None);
        emit_login_span(true, "api_key", None);
        Ok(Default::default())
    }
    async fn new_session(
        &self,
        arguments: acp::NewSessionRequest,
    ) -> Result<acp::NewSessionResponse, acp::Error> {
        self.new_session_inner(arguments).await
    }
    async fn load_session(
        &self,
        arguments: acp::LoadSessionRequest,
    ) -> Result<acp::LoadSessionResponse, acp::Error> {
        self.load_session_inner(arguments).await
    }
    async fn list_sessions(
        &self,
        args: acp::ListSessionsRequest,
    ) -> Result<acp::ListSessionsResponse, acp::Error> {
        crate::agent::handlers::session::handle_list_sessions(self, args).await
    }
    async fn resume_session(
        &self,
        args: acp::ResumeSessionRequest,
    ) -> Result<acp::ResumeSessionResponse, acp::Error> {
        self.resume_session_inner(args).await
    }
    async fn close_session(
        &self,
        args: acp::CloseSessionRequest,
    ) -> Result<acp::CloseSessionResponse, acp::Error> {
        self.close_session_inner(args).await
    }
    #[tracing::instrument(
        name = "agent.prompt",
        skip_all,
        fields(session_id = %arguments.session_id.0, turn_number = tracing::field::Empty)
    )]
    #[allow(unused_mut)]
    async fn prompt(
        &self,
        mut arguments: acp::PromptRequest,
    ) -> Result<acp::PromptResponse, acp::Error> {
        use crate::session::plan_mode::PromptMode;
        if let Some(meta) = arguments.meta.as_ref() {
            xai_file_utils::trace_context::link_current_span_to_meta(
                &serde_json::Value::Object(meta.clone()),
            );
        }
        tracing::debug!(
            target: "sampling_log",
            session_id = %arguments.session_id.0,
            "Received prompt request"
        );
        xai_grok_telemetry::unified_log::info(
            "prompt received",
            Some(arguments.session_id.0.as_ref()),
            None,
        );
        let handle = self
            .session_handle_waiting_for_load(&arguments.session_id)
            .await
            .ok_or_else(|| acp::Error::invalid_params().data("unknown session id"))?;
        if self.models_manager.allowlist_excludes_all() {
            self.send_model_auto_switched(
                    &arguments.session_id,
                    &acp::ModelId::new(String::new()),
                    &acp::ModelId::new(String::new()),
                    "None of your models are allowed by allowed_models. \
                 Broaden it or remove it from your config, then restart.",
                )
                .await;
            return Ok(acp::PromptResponse::new(acp::StopReason::EndTurn));
        }
        let latched_model = self
            .session_registry
            .unavailable_model(&arguments.session_id);
        if let Some(unavailable_model) = latched_model {
            let models = self.models_manager.models();
            let available = self.models_manager.available();
            let restore_model_id = selectable_catalog_key_for_persisted(
                    &models,
                    &available,
                    &unavailable_model,
                )
                .unwrap_or(unavailable_model.clone());
            if available.contains_key(&restore_model_id) {
                tracing::info!(
                    session_id = %arguments.session_id.0,
                    model_id = %restore_model_id.0,
                    "prompt: previously-unavailable model is back in the catalog; restoring it and unblocking the session"
                );
                xai_grok_telemetry::unified_log::info(
                    "prompt: previously-unavailable model recovered, unblocking session",
                    Some(arguments.session_id.0.as_ref()),
                    Some(
                        serde_json::json!({
                        "model_id": restore_model_id.0.as_ref(),
                    }),
                    ),
                );
                self.session_registry.take_unavailable_model(&arguments.session_id);
                if let Err(e) = crate::agent::handlers::model_switch::apply(
                        self,
                        acp::SetSessionModelRequest::new(
                            arguments.session_id.clone(),
                            restore_model_id.clone(),
                        ),
                    )
                    .await
                {
                    tracing::warn!(
                        session_id = %arguments.session_id.0,
                        model_id = %restore_model_id.0,
                        error = ?e,
                        "prompt: failed to restore previously-unavailable model; continuing with the session's current model"
                    );
                }
            } else {
                tracing::warn!(
                    session_id = %arguments.session_id.0,
                    unavailable_model = %unavailable_model.0,
                    available_count = available.len(),
                    available_keys = ?available.keys().take(10).collect::<Vec<_>>(),
                    "prompt blocked: session model unavailable since load and still missing from the catalog"
                );
                xai_grok_telemetry::unified_log::warn(
                    "prompt blocked: model unavailable",
                    Some(arguments.session_id.0.as_ref()),
                    Some(
                        serde_json::json!({
                        "unavailable_model": unavailable_model.0.as_ref(),
                        "available_count": available.len(),
                    }),
                    ),
                );
                self.send_model_auto_switched(
                        &arguments.session_id,
                        &acp::ModelId::new(String::new()),
                        &acp::ModelId::new(String::new()),
                        "Your previous model is no longer available and could not \
                     be switched to a compatible model. Please start a new session.",
                    )
                    .await;
                return Ok(acp::PromptResponse::new(acp::StopReason::EndTurn));
            }
        }
        let dispatch_lock = self.dispatch_lock(&arguments.session_id);
        let dispatch_guard = dispatch_lock.lock().await;
        let meta_prompt_mode = arguments
            .meta
            .as_ref()
            .and_then(|m| m.get("mode"))
            .and_then(|v| v.as_str())
            .map(PromptMode::from_meta_str);
        let prompt_mode = if let Some(mode) = meta_prompt_mode {
            mode
        } else {
            let (mode_tx, mode_rx) = oneshot::channel();
            let _ = handle
                .cmd_tx
                .send(crate::session::SessionCommand::GetCurrentPromptMode {
                    responds_to: mode_tx,
                });
            mode_rx.await.unwrap_or_default()
        };
        let prompt_id = arguments
            .meta
            .as_ref()
            .and_then(|m| m.get("promptId"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let (model_tx, model_rx) = oneshot::channel();
        let _ = handle
            .cmd_tx
            .send(crate::session::SessionCommand::GetCurrentModel {
                responds_to: model_tx,
            });
        let model = model_rx
            .await
            .unwrap_or_else(|_| self.sampling_config.borrow().model.clone());
        let verbatim = arguments
            .meta
            .as_ref()
            .and_then(|m| m.get("verbatim"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let send_now = arguments
            .meta
            .as_ref()
            .and_then(|m| m.get("sendNow"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let (tx, rx) = oneshot::channel();
        let prompt_client_identifier = arguments
            .meta
            .as_ref()
            .and_then(|m| m.get("clientIdentifier"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let prompt_screen_mode = arguments
            .meta
            .as_ref()
            .and_then(|m| m.get("screenMode"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let json_schema = arguments
            .meta
            .as_ref()
            .and_then(|m| m.get("outputSchema"))
            .cloned();
        if json_schema.as_ref().is_some_and(|schema| !schema.is_object()) {
            return Err(
                acp::Error::invalid_params()
                    .data("outputSchema must be a JSON object describing a JSON Schema"),
            );
        }
        let tool_overrides_update = match arguments
            .meta
            .as_ref()
            .and_then(|m| m.get("toolOverrides"))
        {
            None => None,
            Some(value) => {
                match xai_grok_sampling_types::ToolOverridesUpdate::parse(value) {
                    Ok(update) => Some(update),
                    Err(reason) => {
                        return Err(
                            acp::Error::invalid_params()
                                .data(format!("toolOverrides: {reason}")),
                        );
                    }
                }
            }
        };
        handle
            .cmd_tx
            .send(SessionCommand::Prompt {
                prompt_id: prompt_id.clone(),
                prompt_blocks: arguments.prompt.clone(),
                prompt_mode,
                client_identifier: prompt_client_identifier,
                screen_mode: prompt_screen_mode,
                verbatim,
                traceparent: xai_file_utils::trace_context::current_traceparent(),
                json_schema,
                send_now,
                admission: None,
                tool_overrides_update,
                respond_to: tx,
                persist_ack: None,
                parsed_prompt_tx: None,
            })
            .map_err(|e| {
                acp::Error::internal_error()
                    .data(format!("failed to dispatch prompt to session: {e}"))
            })?;
        drop(dispatch_guard);
        self.push_roster_activity_delta(
            &arguments.session_id,
            crate::agent::roster::RosterActivity::Working,
        );
        let stop_result = rx
            .await
            .map_err(|_| {
                acp::Error::internal_error().data("session failed to respond")
            })?;
        let last_turn_usage_for_meta = handle
            .chat_state_handle
            .get_last_turn_usage()
            .await;
        let applied_tool_overrides = stop_result
            .as_ref()
            .ok()
            .and_then(|ok| ok.tool_overrides.clone());
        if matches!(
            stop_result,
            Ok(crate::session::commands::PromptTurnOk {
                completion_kind: crate::session::commands::PromptCompletionKind::RemovedFromQueue,
                ..
            })
        ) {
            return Ok(
                acp::PromptResponse::new(acp::StopReason::Cancelled)
                    .meta(
                        build_prompt_response_meta(PromptResponseMetaArgs {
                                session_id: &arguments.session_id.to_string(),
                                prompt_id: &prompt_id,
                                total_tokens: 0,
                                model_id: &model,
                                last_turn_usage: None,
                                prompt_usage: None,
                                cancellation_category: None,
                                cancel_trigger: None,
                                structured_output: None,
                                tool_overrides: applied_tool_overrides.clone(),
                            })
                            .as_object()
                            .cloned(),
                    ),
            );
        }
        let cancel_trigger: Option<String> = stop_result
            .as_ref()
            .ok()
            .and_then(|ok| match &ok.completion_kind {
                crate::session::commands::PromptCompletionKind::Cancelled {
                    context: Some(ctx),
                    ..
                } => ctx.trigger.clone(),
                _ => None,
            });
        {
            let mapped = stop_result
                .as_ref()
                .map(|ok| ok.stop_reason)
                .map_err(Clone::clone);
            let (stop_reason_value, agent_result_value) = crate::sampling::error::prompt_complete_fields(
                &mapped,
            );
            let turn_id = arguments
                .meta
                .as_ref()
                .and_then(|m| m.get("turnId"))
                .and_then(|v| v.as_u64());
            let mut payload = serde_json::json!({
                "sessionId": arguments.session_id.to_string(),
                "promptId": prompt_id.as_str(),
                "stopReason": stop_reason_value,
                "agentResult": agent_result_value,
            });
            if let Some(tid) = turn_id {
                payload["turnId"] = serde_json::json!(tid);
            }
            if let Some(ref t) = cancel_trigger {
                payload["cancelTrigger"] = serde_json::json!(t);
            }
            let params = serde_json::value::to_raw_value(&payload)
                .expect("prompt_complete params serialization");
            self.gateway
                .forward_fire_and_forget(
                    acp::ExtNotification::new(
                        "x.ai/session/prompt_complete",
                        params.into(),
                    ),
                );
        }
        {
            let end_activity = if handle
                .pending_interactions
                .lock()
                .map(|g| !g.is_empty())
                .unwrap_or(false)
            {
                crate::agent::roster::RosterActivity::NeedsInput
            } else {
                crate::agent::roster::RosterActivity::Idle
            };
            self.push_roster_activity_delta(&arguments.session_id, end_activity);
        }
        let resolved_model = handle.get_model_metadata().await.resolved_model_id;
        match stop_result {
            Ok(turn_ok) => {
                let crate::session::commands::PromptTurnOk {
                    stop_reason,
                    total_tokens,
                    turn_snapshot,
                    completion_kind,
                    structured_output,
                    usage: prompt_usage,
                    tool_overrides: _,
                } = turn_ok;
                let last_turn_usage = last_turn_usage_for_meta;
                let cancellation_category = match &completion_kind {
                    crate::session::commands::PromptCompletionKind::Cancelled {
                        category: Some(cat),
                        ..
                    } => Some(format!("{cat:?}")),
                    crate::session::commands::PromptCompletionKind::MaxTurnsReached {
                        ..
                    } => Some("max_turns_reached".to_string()),
                    crate::session::commands::PromptCompletionKind::StationarityEnded => {
                        Some("action_stationarity".to_string())
                    }
                    _ => None,
                };
                Ok(
                    acp::PromptResponse::new(stop_reason)
                        .meta(
                            build_prompt_response_meta(PromptResponseMetaArgs {
                                    session_id: &arguments.session_id.to_string(),
                                    prompt_id: &prompt_id,
                                    total_tokens,
                                    model_id: &model,
                                    last_turn_usage: last_turn_usage.as_ref(),
                                    prompt_usage,
                                    cancellation_category,
                                    cancel_trigger,
                                    structured_output,
                                    tool_overrides: applied_tool_overrides,
                                })
                                .as_object()
                                .cloned(),
                        ),
                )
            }
            Err(err) => {
                let err = if crate::sampling::error::prompt_usage_from_error(&err)
                    .is_some()
                {
                    err
                } else {
                    let prompt_id = handle
                        .current_prompt_id
                        .lock()
                        .ok()
                        .and_then(|g| g.clone());
                    let (tx, rx) = tokio::sync::oneshot::channel();
                    let usage = if handle
                        .cmd_tx
                        .send(crate::session::commands::SessionCommand::ErrorPathUsageFallback {
                            prompt_id,
                            respond_to: tx,
                        })
                        .is_ok()
                    {
                        rx.await.ok().flatten()
                    } else {
                        None
                    };
                    crate::sampling::error::attach_prompt_usage(err, usage)
                };
                Err(err)
            }
        }
    }
    async fn cancel(&self, args: acp::CancelNotification) -> Result<(), acp::Error> {
        tracing::info!("Received cancel request {args:?}");
        let handle = self.session_handle_waiting_for_load(&args.session_id).await;
        let cancel_trigger = args
            .meta
            .as_ref()
            .and_then(|m| m.get("cancelTrigger"))
            .and_then(|v| v.as_str())
            .map(crate::session::CancelTrigger::from_client);
        xai_grok_telemetry::unified_log::info(
            "shell.cancel.received",
            Some(args.session_id.0.as_ref()),
            Some(
                serde_json::json!({
                "session_found": handle.is_some(),
                "trigger": cancel_trigger.as_ref().map(crate::session::CancelTrigger::as_str),
            }),
            ),
        );
        if let Some(handle) = handle {
            let cancel_subagents = args
                .meta
                .as_ref()
                .and_then(|m| m.get("cancelSubagents"))
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            let rewind_if_no_output = args
                .meta
                .as_ref()
                .and_then(|m| {
                    m.get("rewindIfNoOutput").or_else(|| m.get("rewindIfPristine"))
                })
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let dispatch_lock = self.dispatch_lock(&args.session_id);
            let _dispatch_guard = dispatch_lock.lock().await;
            let _ = handle
                .cmd_tx
                .send(
                    SessionCommand::Cancel(crate::session::CancelOptions {
                        cancel_subagents,
                        rewind_if_no_output,
                        trigger: cancel_trigger,
                        user_initiated: true,
                        ..Default::default()
                    }),
                );
        }
        Ok(())
    }
    async fn set_session_mode(
        &self,
        args: acp::SetSessionModeRequest,
    ) -> Result<acp::SetSessionModeResponse, acp::Error> {
        tracing::info!("Received set session mode request {args:?}");
        let handle = self.session_handle_waiting_for_load(&args.session_id).await;
        let (tx, rx) = oneshot::channel();
        if let Some(handle) = handle {
            let _ = handle
                .cmd_tx
                .send(SessionCommand::SessionMode {
                    session_mode: args.mode_id,
                    responds_to: tx,
                });
        }
        let _ = rx
            .await
            .map_err(|_| {
                acp::Error::internal_error().data("response to set session failed")
            })?;
        Ok(acp::SetSessionModeResponse::new())
    }
    async fn set_session_model(
        &self,
        args: acp::SetSessionModelRequest,
    ) -> Result<acp::SetSessionModelResponse, acp::Error> {
        let model = match self.resolve_model_id(&args.model_id) {
            Ok(model) => model,
            Err(_) => {
                self.models_manager.wait_for_first_catalog().await;
                self.resolve_model_id(&args.model_id)?
            }
        };
        if !model.info.user_selectable {
            return Err(
                acp::Error::invalid_params()
                    .data("This model isn't allowed by your allowed_models setting."),
            );
        }
        let session_id = args.session_id.clone();
        let res = crate::agent::handlers::model_switch::apply(self, args).await;
        if res.is_ok()
            && let Some(unavailable) = self
                .session_registry
                .take_unavailable_model(&session_id)
        {
            tracing::info!(
                session_id = %session_id.0,
                previously_unavailable_model = %unavailable.0,
                "set_session_model: user model switch cleared the model-unavailable block"
            );
        }
        res
    }
    #[tracing::instrument(
        name = "agent.ext_method",
        skip_all,
        fields(method = %args.method)
    )]
    async fn ext_method(
        &self,
        args: acp::ExtRequest,
    ) -> Result<acp::ExtResponse, acp::Error> {
        let request_meta = serde_json::from_str::<serde_json::Value>(args.params.get())
            .ok()
            .and_then(|v| v.get("_meta").cloned());
        if let Some(meta) = &request_meta {
            xai_file_utils::trace_context::link_current_span_to_meta(meta);
        }
        tracing::info!("Received extension method call: method={}", args.method);
        #[allow(unused_mut)]
        let mut backend_no_bridge_err: Option<acp::Error> = None;
        let method = args.method.clone();
        let result = match method.as_ref() {
            "x.ai/session/info" | "x.ai/session/close" | "x.ai/session/list"
            | "x.ai/sessions/list" => {
                crate::agent::handlers::session::handle(self, &args).await
            }
            "x.ai/models/list" => {
                crate::agent::handlers::models::handle(self, &args).await
            }
            "x.ai/session/updates" => {
                crate::extensions::session_updates::handle(&args, &self.gateway).await
            }
            "x.ai/session/state" => {
                crate::extensions::session_state::handle_state(&args).await
            }
            "x.ai/session/import" => {
                crate::extensions::session_state::handle_import(&args).await
            }
            "x.ai/session/load_history" => {
                crate::extensions::chat_conversation_history::handle(self, &args).await
            }
            "x.ai/session/search" => {
                crate::extensions::session_search::handle(&args).await
            }
            "x.ai/session/resolve_local_for_worktree_resume"
            | "x.ai/session/rehydrate" => {
                let ops = self.resolve_workspace_ops()?;
                crate::extensions::worktree::handle(self, &ops, &args).await
            }
            #[cfg(feature = "local-workspace")]
            "x.ai/session/add_local_workspace" => {
                crate::extensions::session_admin::handle(self, &args).await
            }
            "x.ai/session/rename" | "x.ai/session/delete"
            | "x.ai/session/update_mcp_servers" | "x.ai/session/fork"
            | "x.ai/plugins/reload" | "x.ai/commands/list" => {
                crate::extensions::session_admin::handle(self, &args).await
            }
            m if InternalMethod::from_name(m).is_some() => {
                crate::extensions::session_admin::handle(self, &args).await
            }
            "x.ai/session/repair" => crate::extensions::repair::handle(self, &args).await,
            "x.ai/session/usage" => crate::extensions::usage::handle(self, &args).await,
            "x.ai/memory/flush" | "x.ai/memory/rewrite" => {
                crate::extensions::memory::handle(self, &args).await
            }
            "x.ai/skills/refresh-baseline" => {
                self.refresh_skill_baseline_for_all_sessions();
                crate::extensions::to_ext_response(
                    Ok(serde_json::json!({"ok": true})),
                )
            }
            "x.ai/interject" => crate::extensions::interject::handle(self, &args).await,
            "x.ai/recap" => crate::extensions::recap::handle(self, &args).await,
            "x.ai/prompt_history" => {
                crate::extensions::prompt_history::handle(self, &args).await
            }
            "x.ai/suggest" => crate::extensions::suggest::handle(self, &args).await,
            "x.ai/suggestPrompt" => crate::extensions::suggest::handle(self, &args).await,
            s if s.starts_with("x.ai/session_summaries/") => {
                crate::agent::handlers::session::handle(self, &args).await
            }
            s if s.starts_with("x.ai/git/worktree/") => {
                let ops = self.resolve_workspace_ops()?;
                crate::extensions::worktree::handle(self, &ops, &args).await
            }
            s if s.starts_with("x.ai/git/") => {
                let ops = self.resolve_workspace_ops()?;
                crate::extensions::git::handle(self, &ops, &args).await
            }
            s if s.starts_with("x.ai/compact_conversation") => {
                crate::extensions::memory::handle(self, &args).await
            }
            s if s.starts_with("x.ai/plugins/") => {
                crate::extensions::plugins::handle(self, &args).await
            }
            s if s.starts_with("x.ai/marketplace/") => {
                crate::extensions::marketplace::handle(self, &args).await
            }
            s if s.starts_with("x.ai/hooks/") => {
                crate::extensions::hooks::handle(self, &args).await
            }
            s if s.starts_with("x.ai/hunk-tracker/") => {
                let ops = self.resolve_workspace_ops()?;
                crate::extensions::hunk_tracker::handle(self, &ops, &args).await
            }
            s if s.starts_with("x.ai/pr/") => {
                crate::extensions::pr::handle(self, &args).await
            }
            s if s.starts_with(crate::extensions::mcp::mcp_methods::PREFIX) => {
                crate::extensions::mcp::handle(self, &args).await
            }
            s if s.starts_with("x.ai/task/") => {
                crate::extensions::task::handle(self, &args).await
            }
            s if s.starts_with("x.ai/scheduler/") => {
                crate::extensions::task::handle_scheduler(self, &args).await
            }
            s if s.starts_with("x.ai/subagent/") => {
                crate::extensions::task::handle_subagent(self, &args).await
            }
            s if s.starts_with("x.ai/terminal/") => {
                crate::extensions::terminal::handle(self, &args).await
            }
            s if crate::extensions::fs::is_fs_method(s) => {
                crate::extensions::fs::handle(self, &args).await
            }
            s if s.starts_with("x.ai/search/") => {
                crate::extensions::search::handle(self, &args).await
            }
            s if s.starts_with("x.ai/code/") => {
                let ops = self.resolve_workspace_ops()?;
                crate::extensions::code_nav::handle(self, &ops, &args).await
            }
            s if s.starts_with("x.ai/skills/") || s == "x.ai/workflows/list" => {
                let compat = self.cfg.borrow().compat_resolved;
                crate::extensions::skills::handle(
                        self,
                        &args,
                        self.plugin_registry_handle.snapshot().as_deref(),
                        compat,
                    )
                    .await
            }
            s if s.starts_with("x.ai/debug/") => {
                crate::extensions::debug::handle(self, &args).await
            }
            s if s.starts_with("x.ai/rewind") => {
                crate::extensions::rewind::handle(self, &args).await
            }
            other => {
                Err(
                    acp::Error::method_not_found()
                        .data(format!("unknown ACP extension method: {other}")),
                )
            }
        };
        if let Some(err) = backend_no_bridge_err
            && matches!(&result, Err(e) if e.code == acp::Error::method_not_found().code)
        {
            return Err(err);
        }
        result
    }
    async fn ext_notification(
        &self,
        args: acp::ExtNotification,
    ) -> Result<(), acp::Error> {
        tracing::info!("Received extension notification: method={}", args.method);
        if args.method.as_ref() == "x.ai/yolo_mode_changed"
            && let Ok(params) = serde_json::from_str::<
                serde_json::Value,
            >(args.params.get())
        {
            let sender_id = params.get("clientIdentifier").and_then(|v| v.as_str());
            let permission_mode = params
                .get("permission_mode")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let yolo_signal = params.get("yolo_mode").and_then(|v| v.as_bool());
            if let Some(yolo_mode) = yolo_signal {
                let mut updated_sessions = 0;
                self.session_registry
                    .for_each_resident_mut(|_, handle| {
                        updated_sessions
                            += apply_yolo_mode_to_matching_sessions(
                                std::iter::once(handle),
                                sender_id,
                                yolo_mode,
                            );
                    });
                tracing::info!(
                    yolo_mode,
                    sender = ?sender_id,
                    target_sessions = updated_sessions,
                    total_sessions = self.resident_count(),
                    "Setting YOLO mode for matching sessions"
                );
            }
            let auto_mode_explicit = params.get("auto_mode").and_then(|v| v.as_bool());
            let want_auto = auto_mode_explicit == Some(true)
                || permission_mode == "auto";
            let clear_auto = auto_mode_explicit == Some(false)
                || (matches!(permission_mode, "always-approve" | "ask" | "default")
                    && !want_auto);
            let enable_auto = want_auto && yolo_signal != Some(true);
            if enable_auto || clear_auto {
                let enabled = enable_auto;
                let matches_sender = |h: &crate::session::SessionHandle| -> bool {
                    sender_id.is_none()
                        || h.origin_client.as_ref().map(|c| c.product.as_str())
                            == sender_id
                };
                let total_sessions = self.resident_count();
                let mut updated = 0;
                self.session_registry
                    .for_each_resident_mut(|_, h| {
                        if !matches_sender(h) {
                            return;
                        }
                        if h
                            .cmd_tx
                            .send(crate::session::SessionCommand::SetAutoMode {
                                enabled,
                            })
                            .is_ok()
                        {
                            if enabled {
                                h.yolo_mode = false;
                            }
                            updated += 1;
                        }
                    });
                tracing::info!(
                    auto_mode = enabled,
                    sender = ?sender_id,
                    target_sessions = updated,
                    total_sessions,
                    "Setting auto permission mode for matching sessions"
                );
            }
        }
        if args.method.as_ref() == "x.ai/permissions/reset" {
            let mut updated = 0;
            self.session_registry
                .for_each_resident(|_, h| {
                    if h
                        .cmd_tx
                        .send(crate::session::SessionCommand::ResetPermissionState)
                        .is_ok()
                    {
                        updated += 1;
                    }
                });
            tracing::info!(
                target_sessions = updated,
                total_sessions = self.resident_count(),
                "Permission state reset for matching sessions"
            );
        }
        if args.method.as_ref() == InternalMethod::EvictSessions.name() {
            self.handle_evict_sessions(&args.params).await;
        }
        if args.method.as_ref() == "x.ai/toggle_plan_mode"
            && let Ok(params) = serde_json::from_str::<
                serde_json::Value,
            >(args.params.get())
        {
            let session_id_str = params
                .get("sessionId")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let handle = self.resident_handle(&acp::SessionId::new(session_id_str));
            if let Some(handle) = handle {
                let is_engaged = handle.plan_mode.lock().state()
                    != crate::session::plan_mode::PlanModeState::Inactive;
                let next_mode_id = acp::SessionModeId::new(
                    if is_engaged { "default" } else { "plan" },
                );
                let (tx, rx) = oneshot::channel();
                let _ = handle
                    .cmd_tx
                    .send(SessionCommand::SessionMode {
                        session_mode: next_mode_id.clone(),
                        responds_to: tx,
                    });
                if rx.await.is_err() {
                    tracing::warn!(
                        session_id = %session_id_str,
                        mode_id = %next_mode_id.0,
                        "toggle_plan_mode: session mode update failed"
                    );
                }
            } else {
                tracing::warn!(
                    session_id = %session_id_str,
                    "toggle_plan_mode: session not found"
                );
            }
        }
        if args.method.as_ref().starts_with("x.ai/queue/")
            && let Ok(params) = serde_json::from_str::<
                serde_json::Value,
            >(args.params.get())
        {
            let owner = params
                .get("owner")
                .or_else(|| params.get("clientIdentifier"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            if let Some(cmd) = crate::agent::ext_parsers::parse_queue_edit_command(
                args.method.as_ref(),
                &params,
                owner,
            ) {
                let session_id_str = params
                    .get("sessionId")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if let Some(handle) = self
                    .resident_handle(&acp::SessionId::new(session_id_str))
                {
                    if handle.cmd_tx.send(cmd).is_err() {
                        tracing::warn!(
                            session_id = %session_id_str,
                            method = %args.method,
                            "queue edit: failed to forward SessionCommand (session actor gone)"
                        );
                    }
                } else {
                    tracing::warn!(
                        session_id = %session_id_str,
                        method = %args.method,
                        "queue edit: session not found"
                    );
                }
            }
        }
        if args.method.as_ref() == "x.ai/terminal/pty/input"
            && let Ok(params) = serde_json::from_str::<
                serde_json::Value,
            >(args.params.get())
        {
            crate::extensions::terminal::handle_pty_input(&params).await;
        }
        if args.method.as_ref() == "_x.ai/session/update" {
            if let Ok(notification) = serde_json::from_str::<
                SessionNotification,
            >(args.params.get()) {
                tracing::info!(
                    "Storing xAI session notification: session_id={}",
                    notification.session_id.0
                );
                if let Some(handle) = self.resident_handle(&notification.session_id) {
                    let _ = handle
                        .cmd_tx
                        .send(crate::session::SessionCommand::XaiSessionNotification {
                            notification,
                        });
                } else {
                    tracing::warn!(
                        "Received xAI session notification for unknown session: {}",
                        notification.session_id.0
                    );
                }
            } else {
                tracing::warn!("Failed to parse xAI session notification params");
            }
        }
        if args.method.as_ref() == "x.ai/telemetry/non_git_decision" {
            #[derive(serde::Deserialize)]
            struct NonGitDecisionParams {
                decision: String,
                session_id: String,
                #[serde(default)]
                client_version: Option<String>,
            }
            if let Ok(params) = serde_json::from_str::<
                NonGitDecisionParams,
            >(args.params.get()) {
                tracing::info!(
                    decision = %params.decision,
                    session_id = %params.session_id,
                    client_version = ?params.client_version,
                    "non_git_decision",
                );
                xai_grok_telemetry::session_ctx::log_event(xai_grok_telemetry::events::NonGitDecisionEvent {
                    decision: params.decision,
                    session_id: params.session_id,
                    client_version: params.client_version,
                });
            } else {
                tracing::warn!("Failed to parse non_git_decision telemetry params");
            }
        }
        if args.method.as_ref() == "x.ai/telemetry/multi_agent_followup" {
            #[derive(serde::Deserialize)]
            struct MultiAgentFollowupParams {
                preferred_agent_label: char,
                preferred_agent_session_id: Option<String>,
                preferred_agent_model_id: Option<String>,
                /// (label, session_id, model_id)
                other_agents: Vec<(char, Option<String>, Option<String>)>,
            }
            if let Ok(params) = serde_json::from_str::<
                MultiAgentFollowupParams,
            >(args.params.get()) {
                tracing::info!(
                    "Logging multi-agent followup telemetry: preferred_agent={}",
                    params.preferred_agent_label
                );
                let total_agents = 1 + params.other_agents.len();
                xai_grok_telemetry::session_ctx::log_event(xai_grok_telemetry::events::MultiAgentFollowup {
                    preferred_agent_label: params.preferred_agent_label.to_string(),
                    preferred_agent_session_id: params.preferred_agent_session_id,
                    preferred_agent_model_id: params.preferred_agent_model_id,
                    other_agents: params
                        .other_agents
                        .into_iter()
                        .map(|(l, s, m)| xai_grok_telemetry::events::AgentInfo {
                            label: l.to_string(),
                            session_id: s,
                            model_id: m,
                        })
                        .collect(),
                    total_agents,
                });
            } else {
                tracing::warn!("Failed to parse multi-agent followup telemetry params");
            }
        }
        if args.method.as_ref() == "x.ai/telemetry/multi_agent_apply" {
            #[derive(serde::Deserialize)]
            struct MultiAgentApplyParams {
                applied_agent_label: char,
                applied_agent_session_id: Option<String>,
                applied_agent_model_id: Option<String>,
                /// (label, session_id, model_id)
                discarded_agents: Vec<(char, Option<String>, Option<String>)>,
            }
            if let Ok(params) = serde_json::from_str::<
                MultiAgentApplyParams,
            >(args.params.get()) {
                tracing::info!(
                    "Logging multi-agent apply telemetry: applied_agent={}",
                    params.applied_agent_label
                );
                let total_agents = 1 + params.discarded_agents.len();
                xai_grok_telemetry::session_ctx::log_event(xai_grok_telemetry::events::MultiAgentApply {
                    applied_agent_label: params.applied_agent_label.to_string(),
                    applied_agent_session_id: params.applied_agent_session_id,
                    applied_agent_model_id: params.applied_agent_model_id,
                    discarded_agents: params
                        .discarded_agents
                        .into_iter()
                        .map(|(l, s, m)| xai_grok_telemetry::events::AgentInfo {
                            label: l.to_string(),
                            session_id: s,
                            model_id: m,
                        })
                        .collect(),
                    total_agents,
                });
            } else {
                tracing::warn!("Failed to parse multi-agent apply telemetry params");
            }
        }
        if args.method.as_ref() == "x.ai/telemetry/multi_agent_discard" {
            #[derive(serde::Deserialize)]
            struct MultiAgentDiscardParams {
                /// (label, session_id, model_id)
                discarded_agents: Vec<(char, Option<String>, Option<String>)>,
            }
            if let Ok(params) = serde_json::from_str::<
                MultiAgentDiscardParams,
            >(args.params.get()) {
                tracing::info!(
                    "Logging multi-agent discard telemetry: {} agents discarded",
                    params.discarded_agents.len()
                );
                let total = params.discarded_agents.len();
                xai_grok_telemetry::session_ctx::log_event(xai_grok_telemetry::events::MultiAgentDiscard {
                    discarded_agents: params
                        .discarded_agents
                        .into_iter()
                        .map(|(l, s, m)| xai_grok_telemetry::events::AgentInfo {
                            label: l.to_string(),
                            session_id: s,
                            model_id: m,
                        })
                        .collect(),
                    total_agents_discarded: total,
                });
            } else {
                tracing::warn!("Failed to parse multi-agent discard telemetry params");
            }
        }
        if args.method.as_ref() == xai_grok_telemetry::unified_log::LOG_METHOD
            && let Ok(params) = serde_json::from_str::<
                xai_grok_telemetry::unified_log::LogNotificationParams,
            >(args.params.get())
        {
            xai_grok_telemetry::unified_log::ingest_client_entries(
                params.src,
                &params.entries,
            );
        }
        Ok(())
    }
}
#[cfg(test)]
mod tool_overrides_capability_tests {
    use super::tool_overrides_capability;
    #[test]
    fn capability_wire_shape_is_pinned() {
        assert_eq!(
            tool_overrides_capability(),
            serde_json::json!({
                "x_keyword_search": true,
                "x_semantic_search": true,
                "x_user_search": false,
                "x_thread_fetch": false,
            }),
        );
    }
}
