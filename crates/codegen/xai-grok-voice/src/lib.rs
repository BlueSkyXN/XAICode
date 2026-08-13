//! Compatibility-only voice configuration types.
//!
//! Hosted voice/STT clients and microphone capture are intentionally absent
//! from the local product. The public configuration/catalog types remain so
//! existing `[voice]` tables can still deserialize and round-trip unchanged.

pub mod config;
pub mod error;
pub mod language;

pub use config::VoiceConfig;
pub use error::VoiceError;
pub use language::{
    STT_LANGUAGE_AUTO, STT_LANGUAGE_DEFAULT, STT_LANGUAGES, SttLanguage, canonicalize_stt_language,
    language_for_api, stt_language_by_code,
};
