//! Opt-in, content-redacted **external OTEL** telemetry stream.
//!
//! Customers point XAICode at *their own* OpenTelemetry collector (standard
//! `OTEL_*` env vars + the stable `GROK_EXTERNAL_OTEL` master
//! switch) and receive a curated, ZDR-safe schema: ~6 counters and ~17
//! log-record events fanned out from the same typed call sites that emit the
//! product events ([`crate::session_ctx::log_event`]).
//!
//! Structural invariants (enforced by construction and tests):
//! - The providers here are **never** registered with `opentelemetry::global`;
//!   everything is handle-based through the [`EXTERNAL`] registry.
//! - The exporters carry **only** customer headers/metadata from
//!   `OTEL_EXPORTER_OTLP_HEADERS` — this module has no dependency on
//!   `AuthCredentialProvider` and no code path that can attach internal auth
//!   headers.
//! - Default **off**: with `GROK_EXTERNAL_OTEL` unset (or no exporter
//!   selected) nothing is constructed — zero allocation, zero threads, zero
//!   sockets.
//! - Independent of local diagnostics settings: this stream ships only to the
//!   customer's own collector under the customer's own explicit double opt-in.
//!
//! This module is the second authoritative privacy boundary in this crate
//! (alongside the local redaction helpers).

pub mod config;
mod emit;
mod providers;
mod redact;
pub mod schema;
pub mod truncate;

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};

use opentelemetry::logs::LoggerProvider as _;
use opentelemetry::metrics::MeterProvider as _;
use opentelemetry_sdk::logs::{SdkLogger, SdkLoggerProvider};
use opentelemetry_sdk::metrics::SdkMeterProvider;

pub use config::{ContentGates, ExternalOtelConfig, ExternalOtelFileConfig};

static EXTERNAL: OnceLock<Option<Arc<ExternalTelemetry>>> = OnceLock::new();

/// The handle owning both providers. Never global; reached only through the
/// [`EXTERNAL`] registry.
pub struct ExternalTelemetry {
    logger_provider: Option<SdkLoggerProvider>,
    meter_provider: Option<SdkMeterProvider>,
    logger: Option<SdkLogger>,
    instruments: Option<emit::Instruments>,
    /// Emission gate; cleared during local shutdown.
    active: AtomicBool,
    /// Content gates; may only TIGHTEN post-init.
    gates: redact::SharedGates,
    /// `event.sequence` (monotonic, per-process).
    sequence: AtomicU64,
    shutdown_once: std::sync::Once,
    include_session_id_on_metrics: bool,
    include_version_on_metrics: bool,
    app_version: String,
    health: Arc<redact::ExportHealth>,
}

impl ExternalTelemetry {
    pub(crate) fn next_sequence(&self) -> u64 {
        self.sequence.fetch_add(1, Ordering::Relaxed)
    }
}

/// Initialize the customer stream. Called once from binary startup after
/// local config resolution. `None` records the dormant state — the default
/// path allocates nothing.
pub fn init(cfg: Option<ExternalOtelConfig>) {
    // Compatibility and validator tests retain the exporter implementation,
    // but production activation is a separate product decision. This second
    // guard also protects embedders that pass a config directly instead of
    // using `ExternalOtelConfig::resolve`.
    if !cfg!(test) {
        let _ = cfg;
        return;
    }
    let value = cfg.and_then(build_handle);
    if EXTERNAL.set(value).is_err() {
        tracing::debug!("external otel: init called more than once; keeping first registration");
    }
}

fn build_handle(cfg: ExternalOtelConfig) -> Option<Arc<ExternalTelemetry>> {
    let gates: redact::SharedGates = Arc::new(parking_lot::RwLock::new(cfg.gates));
    let health = Arc::new(redact::ExportHealth::default());
    let built = match providers::build(&cfg, gates.clone(), health.clone()) {
        Ok(built) => built,
        Err(e) => {
            tracing::warn!(error = %e, "external otel: exporter construction failed; stream disabled");
            return None;
        }
    };
    if built.logger_provider.is_none() && built.meter_provider.is_none() {
        return None;
    }

    let logger = built
        .logger_provider
        .as_ref()
        .map(|p| p.logger(schema::SCOPE_NAME));
    let instruments = built
        .meter_provider
        .as_ref()
        .map(|p| emit::Instruments::new(&p.meter(schema::SCOPE_NAME)));

    tracing::debug!(
        metrics_exporter = ?cfg.metrics_exporter,
        logs_exporter = ?cfg.logs_exporter,
        "external otel: stream active"
    );

    Some(Arc::new(ExternalTelemetry {
        logger_provider: built.logger_provider,
        meter_provider: built.meter_provider,
        logger,
        instruments,
        active: AtomicBool::new(true),
        gates,
        sequence: AtomicU64::new(0),
        shutdown_once: std::sync::Once::new(),
        include_session_id_on_metrics: cfg.include_session_id_on_metrics,
        include_version_on_metrics: cfg.include_version_on_metrics,
        app_version: cfg.client.client_version.clone(),
        health,
    }))
}

fn handle() -> Option<Arc<ExternalTelemetry>> {
    EXTERNAL.get().and_then(|opt| opt.clone())
}

fn active_handle() -> Option<Arc<ExternalTelemetry>> {
    handle().filter(|ext| ext.active.load(Ordering::Relaxed))
}

/// Cheap check used by the fan-out hook and the split-sink call sites:
/// registry present AND the local runtime emission gate set. A stale `true`
/// read only costs a wasted mapping, never an export ([`emit`] re-checks).
pub fn is_active() -> bool {
    matches!(EXTERNAL.get(), Some(Some(ext)) if ext.active.load(Ordering::Relaxed))
}

/// Map and emit one typed telemetry event. No-op unless the stream is active
/// and the event has an `external = …` mapping. Synchronous and cheap (the
/// batch processor queues; nothing blocks on I/O).
pub fn emit<T: crate::events::TelemetryEvent>(data: &T) {
    let Some(ext) = active_handle() else {
        return;
    };
    let Some(record) = data.external_record() else {
        return;
    };
    emit::emit_record(&ext, record);
}

/// Tighten the local content gates for a running stream.
pub fn restrict_content_gates() {
    let Some(ext) = handle() else {
        return;
    };
    let mut gates = ext.gates.write();
    if gates.log_user_prompts || gates.log_tool_details {
        *gates = ContentGates::default();
    }
}

/// Flush both providers on an explicit local request.
pub fn flush() {
    let Some(ext) = handle() else {
        return;
    };
    flush_on(&ext);
}

pub(crate) fn flush_on(ext: &ExternalTelemetry) {
    if let Some(p) = ext.logger_provider.as_ref()
        && let Err(e) = p.force_flush()
    {
        tracing::debug!(error = %e, "external otel: logger flush failed");
    }
    if let Some(p) = ext.meter_provider.as_ref()
        && let Err(e) = p.force_flush()
    {
        tracing::debug!(error = %e, "external otel: meter flush failed");
    }
}

/// Flush + shutdown both providers with a 2-second watchdog. Idempotent —
/// reachable from every local shutdown path; subsequent calls are no-ops.
pub fn shutdown() {
    let Some(ext) = handle() else {
        return;
    };
    ext.shutdown_once.call_once(|| {
        ext.active.store(false, Ordering::Relaxed);
        let logger_provider = ext.logger_provider.clone();
        let meter_provider = ext.meter_provider.clone();
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        // Detached thread + timed wait: a hung provider must not hang exit
        // (`std::thread::scope` is unusable here — it joins unconditionally).
        std::thread::spawn(move || {
            if let Some(p) = logger_provider
                && let Err(e) = p.shutdown()
            {
                tracing::debug!(error = %e, "external otel: logger shutdown failed");
            }
            if let Some(p) = meter_provider
                && let Err(e) = p.shutdown()
            {
                tracing::debug!(error = %e, "external otel: meter shutdown failed");
            }
            let _ = tx.send(());
        });
        if rx.recv_timeout(std::time::Duration::from_secs(2)).is_err() {
            tracing::debug!("external otel: shutdown watchdog expired; abandoning flush thread");
        }
    });
}

#[cfg(test)]
pub(crate) mod test_support {
    //! Build an [`ExternalTelemetry`] over in-memory exporters so unit tests
    //! can assert exactly what would reach the wire (post-validator).

    use super::*;
    use opentelemetry_sdk::logs::InMemoryLogExporter;
    use opentelemetry_sdk::metrics::{InMemoryMetricExporter, PeriodicReader};

    pub(crate) struct TestStream {
        pub ext: ExternalTelemetry,
        pub logs: InMemoryLogExporter,
        pub metrics: InMemoryMetricExporter,
    }

    pub(crate) fn build(gates: ContentGates) -> TestStream {
        let shared_gates: redact::SharedGates = Arc::new(parking_lot::RwLock::new(gates));
        let health = Arc::new(redact::ExportHealth::default());
        let logs = InMemoryLogExporter::default();
        let metrics = InMemoryMetricExporter::default();

        let logger_provider = SdkLoggerProvider::builder()
            .with_simple_exporter(redact::RedactingLogExporter::new(
                logs.clone(),
                shared_gates.clone(),
                health.clone(),
            ))
            .build();
        let meter_provider = SdkMeterProvider::builder()
            .with_reader(
                PeriodicReader::builder(redact::ValidatingMetricExporter::new(
                    metrics.clone(),
                    health.clone(),
                ))
                .build(),
            )
            .build();

        let logger = logger_provider.logger(schema::SCOPE_NAME);
        let instruments = emit::Instruments::new(&meter_provider.meter(schema::SCOPE_NAME));

        let ext = ExternalTelemetry {
            logger_provider: Some(logger_provider),
            meter_provider: Some(meter_provider),
            logger: Some(logger),
            instruments: Some(instruments),
            active: AtomicBool::new(true),
            gates: shared_gates,
            sequence: AtomicU64::new(0),
            shutdown_once: std::sync::Once::new(),
            include_session_id_on_metrics: true,
            include_version_on_metrics: false,
            app_version: String::new(),
            health,
        };
        TestStream { ext, logs, metrics }
    }

    pub(crate) fn emit_into(stream: &TestStream, record: schema::ExternalRecord) {
        emit::emit_record(&stream.ext, record);
        stream
            .ext
            .logger_provider
            .as_ref()
            .expect("test logger provider")
            .force_flush()
            .expect("flush logs");
        stream
            .ext
            .meter_provider
            .as_ref()
            .expect("test meter provider")
            .force_flush()
            .expect("flush metrics");
    }

    pub(crate) fn emit_event_into<T: crate::events::TelemetryEvent>(
        stream: &TestStream,
        event: &T,
    ) {
        if let Some(record) = event.external_record() {
            emit_into(stream, record);
        }
    }
}
