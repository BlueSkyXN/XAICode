//! Hosted workspace configuration compatibility carrier.
//!
//! The connection/server implementation was removed from the local workspace
//! build.  `HubConfig` remains parseable so existing callers and serialized
//! configuration can round-trip without changing field names or defaults; no
//! value in this carrier is used to open a socket.

use std::sync::Arc;
use url::Url;
use xai_computer_hub_sdk::AuthProvider;

#[derive(Clone)]
pub struct HubConfig {
    pub url: Url,
    pub auth: Arc<dyn AuthProvider>,
    pub activity_tracker: Option<Arc<crate::activity::ActivityTracker>>,
    pub server_id: Option<String>,
    pub alpha_test_key: Option<String>,
    pub allow_insecure_ws: bool,
    pub diag: Option<crate::diag_server::DiagHandle>,
}

impl std::fmt::Debug for HubConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HubConfig")
            .field("url", &self.url.as_str())
            .field("auth", &"<redacted>")
            .field("server_id", &self.server_id)
            .finish_non_exhaustive()
    }
}
