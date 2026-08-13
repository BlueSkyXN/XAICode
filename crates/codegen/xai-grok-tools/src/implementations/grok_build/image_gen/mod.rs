//! Compatibility carrier for the removed hosted image-generation tools.
//!
//! The client, HTTP request types, tool implementation, and slash-command
//! wiring were removed. The small config enum remains so older embedding code
//! can deserialize/merge feature values without turning them into a runtime
//! capability.

#[derive(Debug, Clone, Default)]
pub enum ImageGenConfig {
    #[default]
    Disabled,
    /// Legacy shape retained only for config compatibility. No production
    /// code constructs a client or registers a tool from this value.
    Enabled {
        api_key: String,
        base_url: String,
        extra_headers: indexmap::IndexMap<String, String>,
        image_gen_enabled: bool,
        image_edit_enabled: bool,
        model_override: Option<String>,
        edit_model_override: Option<String>,
        tier_restricted: bool,
    },
}

pub const SESSION_ID_HEADER: &str = "x-grok-session-id";

impl ImageGenConfig {
    /// Hosted image generation is no longer a local capability.
    pub fn has_credentials(&self) -> bool {
        false
    }

    /// Kept as a no-op for callers that still carry this config value.
    pub fn stamp_session_id_header(&mut self, _session_id: &str) {}

    pub fn image_gen_enabled(&self) -> bool {
        false
    }

    pub fn image_edit_enabled(&self) -> bool {
        false
    }

    pub fn model_override(&self) -> Option<&str> {
        None
    }
}
