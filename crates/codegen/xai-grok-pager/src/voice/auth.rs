//! Bridge the shell's `AuthManager` onto the voice crate's bearer provider.
//!
//! Compatibility adapter for the upstream voice crate. Voice is disabled by
//! the clean composition root, so this module is not instantiated at runtime.
//!
//! If a downstream build re-enables the feature, credentials are still resolved
//! through the generic provider-key path rather than a hosted account.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use xai_grok_tools::types::SharedApiKeyProvider;
use xai_grok_voice::{SharedVoiceAuth, VoiceAuthProvider};

/// Adapts the shell's `ApiKeyProvider` onto [`VoiceAuthProvider`].
///
/// Resolves a token per request (never a static snapshot) so a long session
/// follows the underlying `AuthManager` instead of pinning a token that 401s.
struct AuthManagerVoiceAuth(SharedApiKeyProvider);

impl std::fmt::Debug for AuthManagerVoiceAuth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AuthManagerVoiceAuth")
    }
}

impl VoiceAuthProvider for AuthManagerVoiceAuth {
    fn bearer(&self) -> Pin<Box<dyn Future<Output = Option<String>> + Send + '_>> {
        let provider = self.0.clone();
        Box::pin(async move { provider.current_api_key_async().await })
    }
}

/// Build the voice bearer provider from the connection's `AuthManager`.
///
/// Works with the generic provider-key adapter used by the clean build.
pub fn build_voice_auth(auth_manager: Arc<xai_grok_shell::auth::AuthManager>) -> SharedVoiceAuth {
    Arc::new(AuthManagerVoiceAuth(
        xai_grok_shell::auth::shared_api_key_provider(auth_manager),
    ))
}
