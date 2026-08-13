//! Errors for the local computer-hub protocol helpers.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ClientError {
    #[error("invalid configuration: {0}")]
    InvalidConfig(String),
    #[error("serde error: {0}")]
    Serde(String),
    #[error("protocol error: {0}")]
    ProtocolError(String),
    #[error("auth error: {0}")]
    AuthError(String),
    #[error("network error: {0}")]
    NetworkError(String),
    #[error("closed: {0}")]
    Closed(String),
}

impl From<serde_json::Error> for ClientError {
    fn from(error: serde_json::Error) -> Self {
        Self::Serde(error.to_string())
    }
}
