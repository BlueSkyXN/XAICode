//! Compatibility wire types for the retired hosted session registry.
//!
//! Session listing, replication and cloud restore now use local persistence
//! only.  These serde/request shapes remain so old ACP payloads and local
//! merge code can be read without retaining a network-capable client.

use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisterRequest {
    pub session_id: String,
    pub cwd: String,
    pub gcs_trace_prefix: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo_remote_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo_branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo_head_at_start: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hostname: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subagent_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subagent_persona: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subagent_role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fork_context_source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subagent_depth: Option<u32>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_turn_number: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo_head_at_end: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub restorable_turn_number: Option<i32>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionRecord {
    pub session_id: String,
    pub summary: String,
    pub first_prompt: Option<String>,
    pub model_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub last_turn_number: i32,
    #[serde(default)]
    pub restorable_turn_number: Option<i32>,
    pub cwd: String,
    pub repo_remote_url: Option<String>,
    pub hostname: Option<String>,
    pub status: String,
    pub gcs_trace_prefix: String,
    pub gcs_bucket: String,
    #[serde(default)]
    pub last_active_at: Option<String>,
}

impl From<crate::session::persistence::Summary> for SessionRecord {
    fn from(s: crate::session::persistence::Summary) -> Self {
        Self {
            session_id: s.info.id.to_string(),
            summary: s.session_summary,
            first_prompt: None,
            model_id: Some(s.current_model_id.to_string()),
            created_at: s.created_at.to_rfc3339(),
            updated_at: s.updated_at.to_rfc3339(),
            last_turn_number: s.num_messages as i32,
            restorable_turn_number: None,
            cwd: s.info.cwd,
            repo_remote_url: None,
            hostname: None,
            status: "local".into(),
            gcs_trace_prefix: String::new(),
            gcs_bucket: String::new(),
            last_active_at: s.last_active_at.map(|t| t.to_rfc3339()),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResponse {
    pub sessions: Vec<SessionRecord>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadResponse {
    pub download_url: String,
    pub file: String,
    pub turn: i32,
}

#[derive(Clone, Default)]
pub struct SessionRegistryClient;

impl SessionRegistryClient {
    pub fn new(_base_url: impl Into<String>, _user_token: impl Into<String>) -> Self {
        Self
    }
    pub fn with_deployment_key(self, _key: Option<String>) -> Self {
        self
    }
    pub fn with_alpha_test_key(self, _key: Option<String>) -> Self {
        self
    }
    pub fn with_session_id(self, _session_id: impl Into<String>) -> Self {
        self
    }
    pub fn with_auth(self, _auth_manager: std::sync::Arc<crate::auth::AuthManager>) -> Self {
        self
    }
    fn disabled<T>(&self, operation: &'static str) -> Result<T> {
        Err(anyhow!("hosted session registry disabled: {operation}"))
    }
    pub async fn register(&self, _req: &RegisterRequest) -> Result<()> {
        self.disabled("register")
    }
    pub async fn update(&self, _session_id: &str, _req: &UpdateRequest) -> Result<()> {
        self.disabled("update")
    }
    pub async fn finalize(&self, _session_id: &str) -> Result<()> {
        self.disabled("finalize")
    }
    pub async fn search(&self, _query: Option<&str>, _limit: i64) -> Result<Vec<SessionRecord>> {
        self.disabled("search")
    }
    pub async fn get_session(&self, _session_id: &str) -> Result<SessionRecord> {
        self.disabled("get_session")
    }
    pub(crate) async fn get_download_url(
        &self,
        _session_id: &str,
        _file: &str,
        _turn: i32,
    ) -> Result<String> {
        self.disabled("get_download_url")
    }
    pub async fn download_file(
        &self,
        _session_id: &str,
        _file: &str,
        _turn: i32,
        _dest: &std::path::Path,
    ) -> Result<()> {
        self.disabled("download_file")
    }
}
