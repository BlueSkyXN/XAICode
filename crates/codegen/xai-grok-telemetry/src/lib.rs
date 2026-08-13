//! Local diagnostics and explicitly configured customer OTLP for XAICode.
//!
//! Product analytics, error-reporting clients, and authenticated vendor OTLP
//! are not part of this crate. The remaining modules provide local logs,
//! W3C trace context, typed diagnostics, and the opt-in generic OTLP stream.

mod appender;
pub mod config;
pub mod context;
pub mod debug_log;
pub mod enums;
pub mod events;
pub mod external;
pub mod hooks_log;
pub mod id;
pub mod instrumentation;
pub mod memory_log;
pub mod memory_telemetry;
pub(crate) mod otlp_http;
pub mod prompt_timing;
pub(crate) mod redact_common;
pub mod sampling_log;
pub mod session_ctx;
pub mod session_metrics;
pub mod startup;
pub mod unified_log;

pub use events::TelemetryEvent;
pub use session_ctx::{
    EmitterOrigin, TelemetryCtx, emit_event, emit_event_with_origin, log_event, log_session_event,
    log_session_event_with_origin, with_session_ctx,
};
