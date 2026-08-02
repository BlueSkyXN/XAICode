//! Plugin marketplace browse and index crate.
//!
//! Provides marketplace source configuration, plugin discovery (indexed +
//! filesystem fallback), and install integration with the existing
//! `InstallRegistry` pipeline.

pub mod catalog;
pub mod config;
pub mod error;
pub mod git;
pub mod index;
pub mod install_resolve;
pub mod installer;
pub mod matcher;
pub mod scanner;
pub mod types;

pub use config::{
    env_require_sha, load_extra_sources_from_settings, load_extra_sources_from_settings_in,
    load_require_sha, load_sources,
};
pub use error::MarketplaceError;
pub use scanner::scan_marketplace;
pub use types::*;

/// Legacy display name retained only so older config files can be parsed.
/// The clean build never auto-registers this source.
pub const OFFICIAL_SOURCE_NAME: &str = "xAI Official";

/// Legacy source URL retained only for migration/test compatibility.
pub const OFFICIAL_SOURCE_GIT_URL: &str = "https://github.com/xai-org/plugin-marketplace.git";

/// Whether `url` points at the removed vendor-owned marketplace source.
///
/// This is intentionally a deny-list predicate. It is used to reject stale
/// vendor sources, never to select or install an “official” source.
pub fn is_official_source_url(url: &str) -> bool {
    canonical_github_owner_repo(url).as_deref() == Some("xai-org/plugin-marketplace")
}

/// Provider-neutral name for the deny-list predicate. New code should use
/// this instead of the legacy `is_official_source_url` spelling.
pub fn is_blocked_vendor_source_url(url: &str) -> bool {
    if is_official_source_url(url) {
        return true;
    }

    // Reject the former vendor's entire GitHub organization, not only the
    // legacy marketplace repository.  A stale catalog can otherwise point at
    // another first-party repository and still reintroduce vendor code.
    if canonical_github_owner_repo(url)
        .as_deref()
        .is_some_and(|repo| repo == "xai-org" || repo.starts_with("xai-org/"))
    {
        return true;
    }

    // Marketplace entries may be arbitrary HTTPS/SSH URLs, so avoid adding a
    // URL dependency just for host parsing.  Strip user-info and ports before
    // applying exact/suffix matches to the vendor-owned domains.
    let lower = url.trim().to_ascii_lowercase();
    let rest = lower
        .strip_prefix("https://")
        .or_else(|| lower.strip_prefix("http://"))
        .or_else(|| lower.strip_prefix("ssh://"))
        .or_else(|| lower.strip_prefix("git://"))
        .unwrap_or(&lower);
    let authority = rest
        .split(|c| matches!(c, '/' | '?' | '#'))
        .next()
        .unwrap_or(rest);
    let host_port = authority.rsplit('@').next().unwrap_or(authority);
    let host = host_port.split(':').next().unwrap_or(host_port);
    matches!(host, "x.ai" | "grok.com" | "grok.build" | "grok.ai")
        || host.ends_with(".x.ai")
        || host.ends_with(".grok.com")
        || host.ends_with(".grok.build")
        || host.ends_with(".grok.ai")
}

/// Normalized lowercase `owner/repo` from a GitHub URL (HTTPS/http/ssh/scp,
/// `www.`, trailing `.git`/`/`), or `None` if not a GitHub URL.
pub(crate) fn canonical_github_owner_repo(url: &str) -> Option<String> {
    let s = url.trim();
    let s = s.strip_suffix('/').unwrap_or(s);
    let s = s.strip_suffix(".git").unwrap_or(s);
    let lower = s.to_ascii_lowercase();
    let rest = lower
        .strip_prefix("https://")
        .or_else(|| lower.strip_prefix("http://"))
        .or_else(|| lower.strip_prefix("ssh://"))
        .unwrap_or(&lower);
    let rest = rest.strip_prefix("git@").unwrap_or(rest);
    let rest = rest.strip_prefix("www.").unwrap_or(rest);
    let owner_repo = rest
        .strip_prefix("github.com/")
        .or_else(|| rest.strip_prefix("github.com:"))?;
    if owner_repo.is_empty() {
        None
    } else {
        Some(owner_repo.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_official_matches_canonical_https() {
        assert!(is_official_source_url(OFFICIAL_SOURCE_GIT_URL));
        assert!(is_official_source_url(
            "https://github.com/xai-org/plugin-marketplace"
        ));
    }

    #[test]
    fn is_official_matches_ssh_form() {
        assert!(is_official_source_url(
            "git@github.com:xai-org/plugin-marketplace.git"
        ));
        assert!(is_official_source_url(
            "git@github.com:xai-org/plugin-marketplace"
        ));
        assert!(is_official_source_url(
            "ssh://git@github.com/xai-org/plugin-marketplace.git"
        ));
        assert!(is_official_source_url(
            "ssh://git@github.com/xai-org/plugin-marketplace"
        ));
    }

    #[test]
    fn is_official_rejects_unrelated_urls() {
        assert!(!is_official_source_url(
            "https://github.com/anthropics/claude-plugins-official.git"
        ));
        assert!(!is_official_source_url(
            "https://github.com/xai-org/some-other-repo.git"
        ));
        assert!(!is_official_source_url(""));
    }

    #[test]
    fn is_official_matches_noncanonical_forms() {
        assert!(is_official_source_url(
            "https://GitHub.com/XAI-org/Plugin-Marketplace"
        ));
        assert!(is_official_source_url(
            "https://github.com/xai-org/plugin-marketplace/"
        ));
        assert!(is_official_source_url(
            "https://github.com/xai-org/plugin-marketplace.git/"
        ));
        assert!(is_official_source_url(
            "http://github.com/xai-org/plugin-marketplace"
        ));
        assert!(is_official_source_url(
            "https://www.github.com/xai-org/plugin-marketplace.git"
        ));
        assert!(is_official_source_url(
            "git@github.com:XAI-org/plugin-marketplace.git"
        ));
    }

    #[test]
    fn blocked_vendor_sources_cover_domains_and_organization() {
        assert!(is_blocked_vendor_source_url("https://api.x.ai/plugins.git"));
        assert!(is_blocked_vendor_source_url(
            "https://plugins.grok.com/catalog.git"
        ));
        assert!(is_blocked_vendor_source_url(
            "https://github.com/xai-org/another-first-party-repo.git"
        ));
        assert!(!is_blocked_vendor_source_url(
            "https://github.com/example/community-plugins.git"
        ));
        assert!(!is_blocked_vendor_source_url(
            "https://plugins.example.test/index.git"
        ));
    }
}
