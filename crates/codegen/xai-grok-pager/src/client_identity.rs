pub const PAGER_CLIENT_TYPE: &str = "xaicode";
pub const HEADLESS_CLIENT_TYPE: &str = "xaicode";

pub const PAGER_CLIENT_VERSION: &str = xai_grok_version::VERSION;

/// Local client identifier retained for provider-compatible request headers.
pub fn client_user_agent() -> String {
    format!(
        "{}/{} ({}; {})",
        HEADLESS_CLIENT_TYPE,
        PAGER_CLIENT_VERSION,
        std::env::consts::OS,
        std::env::consts::ARCH,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_user_agent_has_expected_shape() {
        // Keep a stable, provider-neutral shape for compatible endpoints.
        let ua = client_user_agent();
        assert_eq!(
            ua,
            format!(
                "xaicode/{} ({}; {})",
                PAGER_CLIENT_VERSION,
                std::env::consts::OS,
                std::env::consts::ARCH
            )
        );
    }
}
