//! Origin/client identification used by the telemetry engine.
//!
//! [`OriginClientInfo`] is owned by `xai-grok-sampler` (so `SamplerConfig`
//! can use it without depending on shell). Re-exported here so the telemetry
//! engine can label events without depending on shell or sampler internals
//! beyond the type itself.

pub use xai_grok_sampler::OriginClientInfo;

/// Construct an [`OriginClientInfo`] from the generic client env vars. Free
/// function (not an inherent method) because the type lives in another crate.
/// The upstream `GROK_CLIENT_*` aliases remain visible only to compatibility
/// tests; production telemetry is disabled and never adopts that identity.
pub fn origin_client_info_from_env() -> Option<OriginClientInfo> {
    let product = std::env::var("CODING_AGENT_CLIENT_NAME").ok().or_else(|| {
        if cfg!(test) {
            std::env::var("GROK_CLIENT_NAME").ok()
        } else {
            None
        }
    })?;
    let version = std::env::var("CODING_AGENT_CLIENT_VERSION")
        .ok()
        .or_else(|| {
            if cfg!(test) {
                std::env::var("GROK_CLIENT_VERSION").ok()
            } else {
                None
            }
        });
    Some(OriginClientInfo { product, version })
}
