//! Local tool protocol and registry helpers.
//!
//! The SDK intentionally has no hosted connection, WebSocket, relay, pool, or
//! donation runtime.  Local workspace/MCP consumers still share generic auth,
//! observability, tool registration, and the JSON tool protocol here.

#![forbid(unsafe_code)]

pub mod auth;
pub mod error;
pub mod harness;
pub mod observability;
pub mod protocol;

pub use auth::{AuthCredential, AuthIdentity, AuthProvider, PrincipalKey, SharedAuthProvider};
pub use harness::{
    CancelOnDrop, LocalRegistry, ModelOutputExtractor, ToolHarness, ToolHarnessBuilder,
    extractor_for,
};
pub use observability::ObservabilityBridge;
pub use protocol::{
    ResolvedSessionHandlers, SessionHandlerResolver, SystemNotifyAck, ToolServerHandler,
};
