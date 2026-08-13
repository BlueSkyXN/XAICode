//! Compatibility carrier for the removed hosted video-generation tools.
//!
//! No API client, presigned storage path, tool registration, or slash command
//! remains in the local product. The enum only preserves old construction
//! sites while configuration migrations are allowed to converge.

use serde::Deserialize;

const DEFAULT_ZDR_VIDEO_PRESIGN_EXPIRES_SECS: u64 = 900;
const DEFAULT_ZDR_VIDEO_KEY_PREFIX: &str = "grok-videos/";

#[derive(Clone, Deserialize, PartialEq, Eq)]
pub struct S3AccessCredentials {
    pub access_key_id: String,
    pub secret_access_key: String,
}

impl S3AccessCredentials {
    fn is_valid(&self) -> bool {
        !self.access_key_id.trim().is_empty() && !self.secret_access_key.trim().is_empty()
    }
}

impl std::fmt::Debug for S3AccessCredentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("S3AccessCredentials")
            .field("access_key_id", &"[redacted]")
            .field("secret_access_key", &"[redacted]")
            .finish()
    }
}

#[derive(Clone, Deserialize, PartialEq, Eq)]
pub struct ZdrVideoOutputS3Config {
    pub bucket: String,
    pub endpoint: String,
    pub region: String,
    #[serde(default = "default_zdr_video_key_prefix")]
    pub key_prefix: String,
    #[serde(default = "default_zdr_video_presign_expires_secs")]
    pub expires_secs: u64,
    pub read_write: S3AccessCredentials,
    #[serde(default)]
    pub read_only: Option<S3AccessCredentials>,
}

fn default_zdr_video_key_prefix() -> String {
    DEFAULT_ZDR_VIDEO_KEY_PREFIX.to_owned()
}

fn default_zdr_video_presign_expires_secs() -> u64 {
    DEFAULT_ZDR_VIDEO_PRESIGN_EXPIRES_SECS
}

impl ZdrVideoOutputS3Config {
    pub fn is_valid(&self) -> bool {
        !self.bucket.trim().is_empty()
            && !self.endpoint.trim().is_empty()
            && !self.region.trim().is_empty()
            && self.read_write.is_valid()
    }
}

impl std::fmt::Debug for ZdrVideoOutputS3Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ZdrVideoOutputS3Config")
            .field("bucket", &self.bucket)
            .field("endpoint", &self.endpoint)
            .field("region", &self.region)
            .field("key_prefix", &self.key_prefix)
            .field("expires_secs", &self.expires_secs)
            .field("read_write", &self.read_write)
            .field("read_only", &self.read_only.as_ref().map(|_| "[redacted]"))
            .finish()
    }
}

#[derive(Debug, Clone, Default)]
pub enum VideoGenConfig {
    #[default]
    Disabled,
    Enabled {
        api_key: String,
        base_url: String,
        extra_headers: indexmap::IndexMap<String, String>,
        zdr_video_output_s3: Option<Box<ZdrVideoOutputS3Config>>,
        tier_restricted: bool,
    },
}

impl VideoGenConfig {
    /// Hosted video generation is no longer a local capability.
    pub fn is_enabled(&self) -> bool {
        false
    }

    /// Kept as a no-op for callers that still carry this config value.
    pub fn stamp_session_id_header(&mut self, _session_id: &str) {}
}
