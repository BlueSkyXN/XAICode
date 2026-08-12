use thiserror::Error;

#[derive(Debug, Error)]
pub enum VoiceError {
    #[error("configuration: {0}")]
    Config(String),
}
