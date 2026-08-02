//! Local diagnostic hooks for the pager.
//!
//! The upstream product forwarded these entries over the `x.ai/log` ACP
//! extension. The clean build writes them directly to the local file-backed
//! diagnostic log; no ACP notification or hosted telemetry request is made.

use xai_acp_lib::AcpAgentTx;
use xai_grok_telemetry::unified_log::{ClientLogEntry, LogLevel, LogSource};

/// Initialize the unified log forwarder with the ACP sender.
///
/// Must be called once after the ACP connection is established.
/// Spawns a background task that flushes buffered entries every few
/// seconds so events are delivered promptly without manual flush calls.
/// Entries buffered before this call will be picked up on the first tick.
pub fn init(_tx: AcpAgentTx) {}

fn now_ts() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn push_entry(lvl: LogLevel, msg: &str, sid: Option<&str>, ctx: Option<serde_json::Value>) {
    let entry = ClientLogEntry {
        ts: now_ts(),
        pid: Some(std::process::id()),
        ver: Some(xai_grok_version::VERSION.to_owned()),
        lvl,
        sid: sid.map(Into::into),
        msg: msg.into(),
        ctx,
    };
    xai_grok_telemetry::unified_log::ingest_client_entries(LogSource::GrokPager, &[entry]);
}

/// Flush any buffered entries to the shell (fire-and-forget).
pub fn flush() {
    // No remote or ACP log sink in the clean build.
}

/// Flush buffered entries and await delivery.
///
/// Use this before process exit to ensure entries are delivered
/// before the agent shuts down.
pub async fn flush_blocking() {
    // No remote or ACP log sink in the clean build.
}

/// Log an info-level entry.
pub fn info(msg: &str, sid: Option<&str>, ctx: Option<serde_json::Value>) {
    push_entry(LogLevel::Info, msg, sid, ctx);
}

/// Log a warn-level entry.
pub fn warn(msg: &str, sid: Option<&str>, ctx: Option<serde_json::Value>) {
    push_entry(LogLevel::Warn, msg, sid, ctx);
}

/// Log an error-level entry.
pub fn error(msg: &str, sid: Option<&str>, ctx: Option<serde_json::Value>) {
    push_entry(LogLevel::Error, msg, sid, ctx);
}

/// Log a debug-level entry.
pub fn debug(msg: &str, sid: Option<&str>, ctx: Option<serde_json::Value>) {
    push_entry(LogLevel::Debug, msg, sid, ctx);
}
