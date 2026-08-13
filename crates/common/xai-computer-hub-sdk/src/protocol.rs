//! Neutral in-process tool protocol types.
//!
//! This module deliberately contains no socket, pool, relay, or hosted-server
//! runtime.  Local MCP and workspace code can still share the handler contract
//! and session-resolution DTOs without acquiring a computer-hub connection.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use xai_tool_protocol::{HookFrame, SessionId, ToolId, ToolServerEvictParams};
use xai_tool_runtime::{ToolCallContext, ToolError, ToolStream, TypedToolOutput};
use xai_tool_types::ToolDescription;

/// A local tool implementation using the shared JSON/progress protocol.
#[async_trait]
pub trait ToolServerHandler: Send + Sync + 'static {
    fn tool_id(&self) -> ToolId;
    fn description(&self) -> ToolDescription;

    fn input_schema(&self) -> Option<Value> {
        None
    }

    async fn handle_call(&self, ctx: ToolCallContext, args: Value) -> ToolStream<TypedToolOutput>;

    #[allow(unused_variables)]
    async fn handle_hook(&self, _session_id: SessionId, _frame: HookFrame) {}

    #[allow(unused_variables)]
    async fn handle_hook_request(
        &self,
        _session_id: SessionId,
        _frame: HookFrame,
    ) -> Option<Value> {
        None
    }

    #[allow(unused_variables)]
    async fn handle_evict(&self, _params: ToolServerEvictParams) {}
}

/// Result of resolving a local session's handler set.
#[derive(Default)]
pub struct ResolvedSessionHandlers {
    pub handlers: Vec<Arc<dyn ToolServerHandler>>,
    pub unserved_tool_ids: Vec<String>,
    pub resolve_error: Option<String>,
}

impl ResolvedSessionHandlers {
    pub fn full(handlers: Vec<Arc<dyn ToolServerHandler>>) -> Self {
        Self {
            handlers,
            unserved_tool_ids: Vec::new(),
            resolve_error: None,
        }
    }
}

/// Optional local resolver for callers that build a per-session tool catalog.
pub type SessionHandlerResolver = Arc<
    dyn Fn(
            SessionId,
            Option<Value>,
        )
            -> futures::future::BoxFuture<'static, Result<ResolvedSessionHandlers, ToolError>>
        + Send
        + Sync,
>;

/// Compatibility carrier for callers that used to inspect a hosted notify ack.
/// No local runtime sends this value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemNotifyAck {
    Accepted,
    ForwardingUnsupported,
}
