//! Local-only tool harness.
//!
//! `ToolHarness` is intentionally limited to an in-process
//! [`LocalRegistry`].  It retains the shared tool/progress surface used by
//! workspace and MCP code, while hosted dispatch, connection binding, and
//! relay discovery are physically absent.

use std::sync::Arc;

use async_trait::async_trait;
use dashmap::DashMap;
use indexmap::IndexMap;
use parking_lot::RwLock;
use serde_json::Value;
use xai_computer_hub_core::{ErasedTool, ToolHandle};
use xai_tool_protocol::{SessionId, ToolId};
use xai_tool_runtime::{
    ListToolsContext, Tool, ToolCallContext, ToolError, ToolStream, TypedToolOutput, terminal_only,
};
use xai_tool_types::ToolDescription;

use crate::error::ClientError;

/// Well-known local hook kind retained for protocol consumers.
pub const PERMISSION_REQUEST_KIND: &str = "permission_request";

/// Compatibility marker. Local dispatch never emits a remote cancel hook.
#[derive(Clone, Copy, Debug)]
pub struct CancelOnDrop(pub bool);

pub type ModelOutputExtractor =
    Arc<dyn Fn(&Value) -> Option<Vec<xai_tool_runtime::ContentBlock>> + Send + Sync>;

pub fn extractor_for<T>() -> ModelOutputExtractor
where
    T: xai_tool_runtime::ToolOutput + serde::de::DeserializeOwned + 'static,
{
    Arc::new(|value: &Value| {
        serde_json::from_value::<T>(value.clone())
            .ok()
            .map(|output| output.model_output().to_vec())
    })
}

#[derive(Default)]
struct LocalRegistryInner {
    entries: RwLock<IndexMap<ToolId, Arc<dyn ToolHandle>>>,
    extractors: DashMap<ToolId, ModelOutputExtractor>,
}

/// In-process registry shared by local workspaces and MCP tool setup.
#[derive(Clone, Default)]
pub struct LocalRegistry {
    inner: Arc<LocalRegistryInner>,
}

impl std::fmt::Debug for LocalRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LocalRegistry")
            .field("entries", &self.inner.entries.read().len())
            .field("extractors", &self.inner.extractors.len())
            .finish()
    }
}

impl LocalRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register<T>(&self, tool: T) -> Option<Arc<dyn ToolHandle>>
    where
        T: Tool + std::fmt::Debug + 'static,
    {
        self.register_arc(Arc::new(tool))
    }

    pub fn register_arc<T>(&self, tool: Arc<T>) -> Option<Arc<dyn ToolHandle>>
    where
        T: Tool + std::fmt::Debug + 'static,
    {
        let id = tool.id();
        let handle: Arc<dyn ToolHandle> = Arc::new(ErasedTool::from_arc(tool));
        self.inner.entries.write().insert(id, handle)
    }

    pub fn register_dyn(
        &self,
        tool: Arc<dyn xai_tool_runtime::ToolDyn>,
    ) -> Option<Arc<dyn ToolHandle>> {
        let id = tool.id();
        let handle: Arc<dyn ToolHandle> = Arc::new(DynToolAdapter(tool));
        self.inner.entries.write().insert(id, handle)
    }

    pub fn find(&self, tool_id: &ToolId) -> Option<Arc<dyn ToolHandle>> {
        self.inner.entries.read().get(tool_id).cloned()
    }

    pub fn unregister(&self, tool_id: &ToolId) -> bool {
        self.inner.entries.write().shift_remove(tool_id).is_some()
    }

    pub fn len(&self) -> usize {
        self.inner.entries.read().len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.entries.read().is_empty()
    }

    pub fn contains(&self, tool_id: &ToolId) -> bool {
        self.inner.entries.read().contains_key(tool_id)
    }

    pub fn register_alias(&self, alias_id: ToolId, target_id: &ToolId) -> bool {
        if let Some(handle) = self.find(target_id) {
            if let Some(extractor) = self.inner.extractors.get(target_id) {
                self.inner
                    .extractors
                    .insert(alias_id.clone(), extractor.clone());
            }
            self.inner.entries.write().insert(alias_id, handle);
            true
        } else {
            false
        }
    }

    pub fn register_with_model_output<T>(&self, tool: T) -> Option<Arc<dyn ToolHandle>>
    where
        T: Tool + std::fmt::Debug + 'static,
        T::Output: xai_tool_runtime::ToolOutput + serde::de::DeserializeOwned + 'static,
    {
        let id = tool.id();
        self.inner
            .extractors
            .insert(id, extractor_for::<T::Output>());
        self.register(tool)
    }

    pub fn register_extractor(&self, tool_id: ToolId, extractor: ModelOutputExtractor) {
        self.inner.extractors.insert(tool_id, extractor);
    }

    pub fn model_output(
        &self,
        tool_id: &ToolId,
        output: &Value,
    ) -> Option<Vec<xai_tool_runtime::ContentBlock>> {
        self.inner
            .extractors
            .get(tool_id)
            .and_then(|extractor| extractor.value()(output))
    }

    pub fn list_tools(&self, ctx: &ListToolsContext) -> Vec<ToolDescription> {
        self.inner
            .entries
            .read()
            .values()
            .filter(|handle| handle.should_list(ctx))
            .map(|handle| handle.description(ctx))
            .collect()
    }
}

struct DynToolAdapter(Arc<dyn xai_tool_runtime::ToolDyn>);

impl std::fmt::Debug for DynToolAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DynToolAdapter")
            .field("id", &self.0.id())
            .finish()
    }
}

#[async_trait]
impl ToolHandle for DynToolAdapter {
    fn id(&self) -> ToolId {
        self.0.id()
    }

    fn description(&self, ctx: &ListToolsContext) -> ToolDescription {
        self.0.description(ctx)
    }

    fn capabilities(&self) -> xai_tool_protocol::ToolCapabilities {
        self.0.capabilities()
    }

    fn should_list(&self, ctx: &ListToolsContext) -> bool {
        self.0.should_list(ctx)
    }

    async fn execute(&self, ctx: ToolCallContext, args: Value) -> ToolStream<TypedToolOutput> {
        self.0.execute(ctx, args).await
    }
}

/// Local-only builder retained as a convenient neutral construction API.
#[derive(Default)]
pub struct ToolHarnessBuilder {
    local_registry: LocalRegistry,
    session: Option<SessionId>,
    default_extensions: xai_tool_runtime::TypedExtensions,
}

impl ToolHarnessBuilder {
    pub fn local_registry(mut self, registry: LocalRegistry) -> Self {
        self.local_registry = registry;
        self
    }

    pub fn session(mut self, session: SessionId) -> Self {
        self.session = Some(session);
        self
    }

    pub fn default_extensions(mut self, extensions: xai_tool_runtime::TypedExtensions) -> Self {
        self.default_extensions = extensions;
        self
    }

    pub fn tool<T>(self, tool: T) -> Self
    where
        T: Tool + std::fmt::Debug + 'static,
    {
        self.local_registry.register(tool);
        self
    }

    pub fn build(self) -> ToolHarness {
        ToolHarness::local_only_with(
            self.local_registry,
            self.session
                .unwrap_or_else(|| SessionId::new("local").expect("valid local session id")),
            self.default_extensions,
        )
    }
}

#[derive(Clone)]
pub struct ToolHarness {
    local_registry: LocalRegistry,
    session: SessionId,
    default_extensions: xai_tool_runtime::TypedExtensions,
}

impl std::fmt::Debug for ToolHarness {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolHarness")
            .field("session", &self.session)
            .field("local_tool_count", &self.local_registry.len())
            .finish()
    }
}

impl ToolHarness {
    pub fn local_only_with(
        local_registry: LocalRegistry,
        session: SessionId,
        default_extensions: xai_tool_runtime::TypedExtensions,
    ) -> Self {
        Self {
            local_registry,
            session,
            default_extensions,
        }
    }

    pub fn session(&self) -> &SessionId {
        &self.session
    }

    pub fn local_registry(&self) -> LocalRegistry {
        self.local_registry.clone()
    }

    pub fn list_tools(&self, ctx: &ListToolsContext) -> Vec<ToolDescription> {
        self.local_registry.list_tools(ctx)
    }

    pub fn list_local_tools(&self, ctx: &ListToolsContext) -> Vec<ToolDescription> {
        self.local_registry.list_tools(ctx)
    }

    pub fn model_output(
        &self,
        tool_id: &ToolId,
        output: &Value,
    ) -> Option<Vec<xai_tool_runtime::ContentBlock>> {
        self.local_registry.model_output(tool_id, output)
    }

    pub async fn call(
        &self,
        tool_id: ToolId,
        args: Value,
        mut ctx: ToolCallContext,
    ) -> ToolStream<TypedToolOutput> {
        ctx.extensions.merge_defaults(&self.default_extensions);
        match self.local_registry.find(&tool_id) {
            Some(handle) => handle.execute(ctx, args).await,
            None => terminal_only(Err(ToolError::not_found(
                tool_id.clone(),
                format!("tool not found in local registry: {tool_id}"),
            ))),
        }
    }

    /// Local event emission is intentionally a no-op; callers continue to
    /// publish events to their process-local event sink.
    pub async fn emit_session_event(&self, _event: xai_tool_protocol::session_event::SessionEvent) {
    }

    /// Hosted hooks have no local transport. Keep this explicit fail-closed
    /// result for compatibility callers so they cannot accidentally re-enable
    /// network dispatch through this API.
    pub async fn send_hook(&self, _hook: xai_tool_protocol::HookFrame) -> Result<(), ClientError> {
        Err(ClientError::InvalidConfig(
            "hosted tool hooks are disabled in local-only harness".to_owned(),
        ))
    }

    pub async fn shutdown(&self) -> Result<(), ClientError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_registry_starts_empty() {
        let registry = LocalRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
    }

    #[tokio::test]
    async fn local_only_harness_missing_tool_is_terminal_error() {
        let harness = ToolHarness::local_only_with(
            LocalRegistry::new(),
            SessionId::new("local-test").unwrap(),
            Default::default(),
        );
        let mut stream = harness
            .call(
                ToolId::new("missing").unwrap(),
                Value::Null,
                ToolCallContext::default(),
            )
            .await;
        use futures::StreamExt;
        let item = stream.next().await.expect("terminal item");
        assert!(matches!(
            item,
            xai_tool_runtime::ToolStreamItem::Terminal(_)
        ));
    }

    #[test]
    fn cancel_marker_does_not_enable_transport() {
        assert!(!CancelOnDrop(false).0);
    }
}
