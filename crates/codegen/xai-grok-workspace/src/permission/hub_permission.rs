//! Compatibility boundary for the removed hosted permission bridge.
//!
//! Local ACP permission prompting remains in `prompter`.  The old network
//! request path is retained only as a fail-closed trait/function carrier so
//! older embedding code can compile without providing a transport.

use crate::permission::prompter::PromptOutcome;
use crate::permission::types::AccessKind;
use async_trait::async_trait;
use serde_json::Value;

pub const HITL_PERMISSION_LIVE_ENV: &str = "GROK_HITL_PERMISSION_LIVE";

pub(crate) fn init_metrics() {}

pub fn hitl_permission_live_enabled() -> bool {
    false
}

#[async_trait]
pub trait PermissionHookTransport: Send + Sync {
    async fn request_permission(&self, payload: Value) -> Result<Value, String>;
}

pub fn access_kind_for_hub_tool(_tool_name: &str, _args: &Value) -> Option<AccessKind> {
    None
}

pub fn prompt_outcome_allows(outcome: &PromptOutcome) -> bool {
    matches!(
        outcome,
        PromptOutcome::AllowOnce
            | PromptOutcome::AllowAlways
            | PromptOutcome::AllowEditsForSession
            | PromptOutcome::AllowAlwaysBashCommand(_)
            | PromptOutcome::AllowAlwaysBashGlob(_)
            | PromptOutcome::AllowAlwaysDomain(_)
            | PromptOutcome::AllowAlwaysMcpTool(_)
            | PromptOutcome::AllowAlwaysMcpServer(_)
    )
}

/// The hosted permission bridge is unavailable in a local-only workspace.
/// Failing closed preserves the local permission manager's safety invariant.
pub async fn request_permission_via_hub(
    _transport: &dyn PermissionHookTransport,
    _access: &AccessKind,
    _tool_call_id: &str,
) -> PromptOutcome {
    PromptOutcome::Error("hosted permission transport is disabled".to_owned())
}
