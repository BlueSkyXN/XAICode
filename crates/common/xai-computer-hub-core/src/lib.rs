//! xAI Computer Hub — transport + registry + resolver core.
//!
//! Object-safe abstractions for local tool registration and dispatch.

#![forbid(unsafe_code)]

pub mod inner;
pub mod local;
pub mod registry;
pub mod resolver;
pub mod transport;

pub use inner::InnerDispatchForResolver;
pub use local::{LOCAL_INVOKE_SCOPE, LocalTransport};
pub use registry::{
    ConnectionCleanupReport, SessionCleanupReport, ToolRegistry, ToolSessionBindOutcome,
    ToolSessionUnbindOutcome,
};
pub use resolver::{CompoundResolver, ErasedTool, ResolvedTool, ToolHandle};
pub use transport::{Principal, Transport, TransportKind};
