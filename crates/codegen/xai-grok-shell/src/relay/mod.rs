//! Hosted relay/session sharing was removed from the clean local build.
//!
//! Local leader IPC lives under [`crate::leader`] and is intentionally kept
//! separate from this retired hosted namespace.  The empty module remains as
//! a source-compatibility anchor for downstream code that used the old path.

/// Inert compatibility handle for local persistence. Session updates remain
/// durable on disk; hosted relay/share delivery is no longer a consumer.
#[derive(Clone, Default)]
pub struct RelaySync;

impl RelaySync {
    pub fn queue(&self, _notification: agent_client_protocol::SessionNotification) {}
    pub fn flush(&self) {}
}
