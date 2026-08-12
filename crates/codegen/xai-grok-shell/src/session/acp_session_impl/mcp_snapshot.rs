//! MCP snapshot refresh and templated-prefix readiness for local MCP servers.
//!
//! The snapshot contains only tools registered by explicitly configured or
//! client-provided MCP transports. Hosted gateway catalogs are not a source of
//! tools in the local composition.

use super::*;

pub(super) const MCP_INIT_CANCELLED_CONFIG_CHANGED: &str = "config_changed";

impl McpReminderMode {
    pub(super) fn from_env() -> Self {
        match std::env::var("MCP_REMINDER_MODE")
            .unwrap_or_default()
            .to_lowercase()
            .as_str()
        {
            "full" => Self::Full,
            _ => Self::Delta,
        }
    }
}

pub(super) async fn refresh_mcp_snapshot_and_schedule_reminder_with(
    tool_bridge: Arc<crate::tools::bridge::ToolBridge>,
    mcp_state: Arc<TokioMutex<McpState>>,
    tool_metadata_snapshot: Arc<std::sync::Mutex<crate::session::tool_index::ToolMetadataSnapshot>>,
    mcp_reminder_dirty: Arc<std::sync::atomic::AtomicBool>,
    mcp_initialized: bool,
    mcps_root: Option<std::path::PathBuf>,
) {
    use crate::session::tool_index::{
        ServerMetadata, ToolMetadata, extract_parameter_names, split_qualified_name,
    };

    let all_defs = tool_bridge.tool_definitions().await;
    let mut seen_tools = std::collections::HashSet::new();
    let mcp_tools: Vec<ToolMetadata> = all_defs
        .iter()
        .filter(|d| d.function.name.contains("__"))
        .filter(|d| seen_tools.insert(d.function.name.clone()))
        .map(|d| {
            let (server, tool) = split_qualified_name(&d.function.name);
            ToolMetadata {
                qualified_name: d.function.name.clone(),
                server_name: server.to_string(),
                tool_name: tool.to_string(),
                description: d.function.description.clone().unwrap_or_default(),
                parameters: extract_parameter_names(&d.function.parameters),
                input_schema: d.function.parameters.clone(),
            }
        })
        .collect();

    let servers_with_tools: std::collections::HashSet<&str> =
        mcp_tools.iter().map(|t| t.server_name.as_str()).collect();
    let server_metadata: Vec<ServerMetadata> = {
        let mcp_state = mcp_state.lock().await;
        let mut metadata = Vec::new();
        for (name, client) in mcp_state.all_clients() {
            if servers_with_tools.contains(name.as_str()) {
                metadata.push(ServerMetadata {
                    name: name.clone(),
                    description: client.server_instructions().await,
                });
            }
        }
        metadata
    };

    {
        let mut snapshot = tool_metadata_snapshot.lock().unwrap();
        snapshot.tools = mcp_tools;
        snapshot.servers = server_metadata;
        snapshot.mcp_initialized = mcp_initialized;
    }

    mcp_reminder_dirty.store(true, std::sync::atomic::Ordering::Relaxed);
    tracing::debug!("MCP snapshot updated, reminder marked dirty");

    if let Some(mcps_root) = mcps_root {
        let clients: Vec<(String, Arc<crate::session::mcp_servers::McpClient>)> = {
            let state = mcp_state.lock().await;
            state
                .all_clients()
                .map(|(name, client)| (name.clone(), Arc::clone(client)))
                .collect()
        };
        crate::session::mcp_descriptors::materialize_descriptors_for_clients(&mcps_root, clients)
            .await;
    }
}

#[cfg(test)]
pub(crate) async fn refresh_mcp_snapshot_for_test(
    tool_bridge: Arc<crate::tools::bridge::ToolBridge>,
    mcp_state: Arc<TokioMutex<McpState>>,
    tool_metadata_snapshot: Arc<std::sync::Mutex<crate::session::tool_index::ToolMetadataSnapshot>>,
) {
    refresh_mcp_snapshot_and_schedule_reminder_with(
        tool_bridge,
        mcp_state,
        tool_metadata_snapshot,
        Arc::new(std::sync::atomic::AtomicBool::new(false)),
        false,
        None,
    )
    .await;
}

impl SessionActor {
    /// Block the templated user prefix until local MCP handshakes finish.
    pub(super) async fn wait_for_mcp_templated_prefix_ready(
        &self,
        template: &xai_grok_agent::prompt::user_message::UserMessageTemplate,
    ) {
        use xai_grok_agent::prompt::user_message::UserMessageTemplate;
        if matches!(template, UserMessageTemplate::Default) {
            return;
        }

        let notified = self.mcp_handshakes_done.notified();
        tokio::pin!(notified);
        let state = self.mcp_state.lock().await;
        if state.configs.is_empty() || state.is_initialized() {
            return;
        }
        drop(state);
        let _ = tokio::time::timeout(std::time::Duration::from_secs(3), notified).await;
    }

    pub(super) async fn wait_for_mcp_handshakes_bounded(&self, timeout: std::time::Duration) {
        let notified = self.mcp_handshakes_done.notified();
        tokio::pin!(notified);
        let state = self.mcp_state.lock().await;
        if state.configs.is_empty() || state.is_initialized() {
            return;
        }
        drop(state);
        let _ = tokio::time::timeout(timeout, notified).await;
    }

    /// Re-register tools from connected local MCP clients after a bridge rebuild.
    pub(super) async fn re_register_mcp_tools_on_rebuilt_bridge(&self) {
        let clients: Vec<(
            String,
            std::sync::Arc<crate::session::mcp_servers::McpClient>,
        )> = {
            let state = self.mcp_state.lock().await;
            state
                .all_clients()
                .map(|(name, client)| (name.clone(), std::sync::Arc::clone(client)))
                .collect()
        };
        if clients.is_empty() {
            self.refresh_mcp_snapshot_and_schedule_reminder().await;
            return;
        }

        let mcp_state = std::sync::Arc::clone(&self.mcp_state);
        let mut ui_tools = std::collections::HashMap::new();
        for (server_name, client) in clients {
            let Ok(registrations) = client.get_tool_registrations(mcp_state.clone()).await else {
                continue;
            };
            let mut state = self.mcp_state.lock().await;
            for registration in registrations {
                self.register_mcp_tool(&server_name, registration, &mut state, &mut ui_tools)
                    .await;
            }
        }
        self.refresh_mcp_snapshot_and_schedule_reminder().await;
        self.emit_mcp_tools_changed_notifications(ui_tools);
    }
}
