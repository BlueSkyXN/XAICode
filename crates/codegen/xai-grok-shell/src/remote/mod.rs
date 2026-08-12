//! Compatibility seam for provider-neutral model metadata.
//!
//! Hosted sandbox, conversation, product-skill, workspace, pull/sync and chat
//! mode clients were removed from the clean build.  The remaining client
//! helpers only describe user-configured model endpoints and deliberately
//! fail closed before any hosted credential or request is constructed.

pub mod client;

pub use client::{BackendError, SettingsFetch, fetch_settings_blocking};
pub(crate) use client::{
    DEFAULT_CONTEXT_WINDOW, FetchModelsResult, fetch_models_blocking, models_list_url,
    parse_remote_model_value,
};

/// Compatibility handle retained by local persistence callers. The hosted
/// writeback transport is gone; this handle never creates a request.
#[derive(Clone, Default)]
pub struct RemoteSync {
    observer: std::sync::Arc<
        std::sync::Mutex<
            Option<tokio::sync::mpsc::UnboundedSender<agent_client_protocol::SessionNotification>>,
        >,
    >,
}

impl RemoteSync {
    pub fn new<T, U>(_session_id: String, _metadata: T, _client: U) -> Self {
        Self::default()
    }

    pub fn queue(&self, notification: agent_client_protocol::SessionNotification) {
        if let Ok(observer) = self.observer.lock()
            && let Some(observer) = observer.as_ref()
        {
            let _ = observer.send(notification);
        }
    }

    pub fn flush(&self) {}
    pub fn set_model_id(&self, _model_id: String) {}
    pub fn set_title(&self, _title: String) {}

    #[cfg(test)]
    pub fn test_observer() -> (
        Self,
        tokio::sync::mpsc::UnboundedReceiver<agent_client_protocol::SessionNotification>,
    ) {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let sync = Self {
            observer: std::sync::Arc::new(std::sync::Mutex::new(Some(tx))),
        };
        (sync, rx)
    }
}
