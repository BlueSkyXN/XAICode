//! Local process identifiers retained for upstream type compatibility.
//!
//! The hosted build used a persistent machine identifier for telemetry and
//! remote bucketing. The clean build deliberately does not read or write an
//! identity cache; callers receive a non-identifying local marker instead.

/// Returns true when workspace marker env vars (`XAI_ROOT` and `XAI_USER`) are set.
///
/// Used as a coarse local gate for features that require a full workspace
/// checkout. External installs typically leave both unset.
pub fn has_workspace_env_markers() -> bool {
    std::env::var("XAI_ROOT").is_ok() && std::env::var("XAI_USER").is_ok()
}

/// Opt-in special-user gate for telemetry.
///
/// Enabled only when `GROK_TELEMETRY_SPECIAL_USER=1` (or `true`). There is no
/// hardcoded username allowlist.
pub fn is_special_user() -> bool {
    matches!(
        std::env::var("GROK_TELEMETRY_SPECIAL_USER").as_deref(),
        Ok("1") | Ok("true") | Ok("TRUE")
    )
}
