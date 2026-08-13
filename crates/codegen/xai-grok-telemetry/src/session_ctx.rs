//! Ambient session context for local diagnostics and customer OTLP via
//! [`log_event`]. `session_id` and `turn_number` are injected from the
//! task-local [`TelemetryCtx`] active for the duration of a session.
//!
//! Shared by the shell and pager's local diagnostics.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::events::TelemetryEvent;
use serde::Serialize;
#[cfg(test)]
use serde_json::json;

/// Ambient session context for telemetry. Snapshotted synchronously by
/// `log_event` at call time to avoid racing with turn increments.
#[derive(Clone)]
pub struct TelemetryCtx {
    pub session_id: String,
    pub prompt_index: Arc<tokio::sync::Mutex<usize>>,
    /// Per-prompt correlation UUID for the external OTEL stream (`prompt.id`,
    /// events only — never metrics). Set at turn start where `prompt_index`
    /// increments; `None` outside a prompt.
    pub prompt_id: Arc<parking_lot::Mutex<Option<String>>>,
}

impl TelemetryCtx {
    pub fn new(session_id: String, prompt_index: Arc<tokio::sync::Mutex<usize>>) -> Self {
        Self {
            session_id,
            prompt_index,
            prompt_id: Arc::new(parking_lot::Mutex::new(None)),
        }
    }
}

/// Snapshot of the ambient ctx for the external OTEL stream.
pub(crate) struct ExternalCtxSnapshot {
    pub session_id: String,
    pub turn_number: Option<u32>,
    pub prompt_id: Option<String>,
}

/// Rotate the per-prompt correlation UUID at turn start (where
/// `prompt_index` increments). No-op outside a session ctx scope. The id is
/// attached as `prompt.id` to external OTEL events only.
pub fn begin_prompt_id() {
    let _ = TELEMETRY_CTX.try_with(|c| {
        *c.prompt_id.lock() = Some(uuid::Uuid::new_v4().to_string());
    });
}

/// Snapshot the task-local ctx (if any) for external emission. Non-blocking:
/// a contended `prompt_index` lock yields `turn_number = None` rather than
/// stalling the emitting task.
pub(crate) fn external_ctx_snapshot() -> Option<ExternalCtxSnapshot> {
    TELEMETRY_CTX
        .try_with(|c| ExternalCtxSnapshot {
            session_id: c.session_id.clone(),
            turn_number: c.prompt_index.try_lock().map(|g| *g as u32).ok(),
            prompt_id: c.prompt_id.lock().clone(),
        })
        .ok()
}

tokio::task_local! {
    static TELEMETRY_CTX: Arc<TelemetryCtx>;
}

/// The `session_id` field name the debug-log firehose router keys on:
/// `debug_log::SessionIdVisitor` stashes a `SessionId` extension on any span
/// carrying this field — the span *name* is not load-bearing for routing. Shared
/// so the `info_span!` here and the router in `debug_log` can't silently drift; a
/// rename trips `session_span_exposes_router_field` below.
pub(crate) const SESSION_ID_FIELD: &str = "session_id";

/// Build the per-session tracing span the firehose router routes by. The field
/// name MUST be the literal `session_id` (tracing field names can't come from a
/// const); the test below pins it against [`SESSION_ID_FIELD`].
fn session_span(session_id: &str) -> tracing::Span {
    tracing::info_span!("session", session_id = %session_id)
}

/// Run `fut` with telemetry context active. Also sets a `tracing` span.
pub async fn with_session_ctx<F: std::future::Future>(ctx: TelemetryCtx, fut: F) -> F::Output {
    use tracing::Instrument;
    let span = session_span(&ctx.session_id);
    TELEMETRY_CTX
        .scope(Arc::new(ctx), fut.instrument(span))
        .await
}

/// Product surface that emitted a telemetry event. Selects a local diagnostic
/// prefix so shell and workspace events remain distinguishable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::EnumCount)]
pub enum EmitterOrigin {
    /// XAICode shell (and the pager/TUI that emit through it).
    Shell,
    /// Workspace-side sampler/server.
    Workspace,
}

impl EmitterOrigin {
    /// Every emitter origin. Completeness is compiler-enforced by the
    /// `EmitterOrigin::ALL` length assertion below.
    pub const ALL: [EmitterOrigin; 2] = [EmitterOrigin::Shell, EmitterOrigin::Workspace];

    /// Local diagnostic event-name prefix for this origin.
    pub fn event_prefix(self) -> &'static str {
        match self {
            EmitterOrigin::Shell => "xaicode-shell-",
            EmitterOrigin::Workspace => "xaicode-workspace-",
        }
    }
}

/// Compile-time completeness guard for [`EmitterOrigin::ALL`]: adding a variant
/// without listing it in `ALL` makes `ALL.len()` diverge from the
/// `strum::EnumCount`-derived variant count and fails this assertion, so
/// a diagnostic consumer can never silently stop recognizing an origin prefix.
const _: () = assert!(EmitterOrigin::ALL.len() == <EmitterOrigin as strum::EnumCount>::COUNT);

/// Type-safe event fanout to the explicitly configured customer OTLP stream.
pub fn log_event<T: TelemetryEvent>(data: T) {
    crate::external::emit(&data);
}

/// Emit one event to the explicitly configured customer stream. The legacy
/// internal-sink flag is accepted for source compatibility but has no effect.
pub fn log_event_dual<T: TelemetryEvent>(internal_enabled: bool, data: T) {
    let _ = internal_enabled;
    crate::external::emit(&data);
}

/// Session lifecycle event (type-safe) for the explicitly configured customer
/// stream. Workspace-side callers use [`log_session_event_with_origin`].
pub fn log_session_event<T: TelemetryEvent>(data: T) {
    crate::external::emit(&data);
}

/// Session lifecycle event tagged with the emitting [`EmitterOrigin`]. Fires in
/// both local diagnostic modes; the origin selects the event-name prefix
/// (`xaicode-shell-*` vs `xaicode-workspace-*`).
///
/// Deliberately **no external fan-out** here: workspace-side callers
/// (`EmitterOrigin::Workspace` — workspace-side sampler/server) invoke this
/// directly; customer export remains restricted to shell-origin events.
pub fn log_session_event_with_origin<T: TelemetryEvent>(origin: EmitterOrigin, data: T) {
    if matches!(origin, EmitterOrigin::Shell) {
        crate::external::emit(&data);
    }
}

/// Emit an event with the default [`EmitterOrigin::Shell`] prefix.
pub fn emit_event<T: Serialize + Send + 'static>(event_suffix: impl Into<String>, data: T) {
    let _ = (event_suffix, data);
}

/// Posts spawned by [`emit_event_with_origin`] that haven't finished. Emission
/// is fire-and-forget so it never blocks a turn, which also means a process
/// exiting right after emitting drops the event — see [`drain_pending`].
static PENDING_EVENTS: AtomicUsize = AtomicUsize::new(0);

/// Clean-build compatibility hook. Legacy event emission is disabled, so
/// there is no remote work to drain.
pub async fn drain_pending(timeout: std::time::Duration) {
    let _ = timeout;
}

/// Emit an event whose analytics name is `{origin prefix}{event_suffix}`.
pub fn emit_event_with_origin<T: Serialize + Send + 'static>(
    origin: EmitterOrigin,
    event_suffix: impl Into<String>,
    data: T,
) {
    let _ = (origin, event_suffix, data);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The debug-log firehose router (`debug_log`) finds the session span by its
    /// `session_id` field (not by name). That field name is a literal in
    /// `session_span` (tracing field names can't be a const), so pin it against the
    /// shared const here — a rename of either breaks this test instead of silently
    /// degrading routing to the per-pid fallback.
    #[test]
    fn session_span_exposes_router_field() {
        // A bare registry enables every callsite, so the span has live metadata.
        let subscriber = tracing_subscriber::registry();
        tracing::subscriber::with_default(subscriber, || {
            let span = session_span("test-id");
            let meta = span
                .metadata()
                .expect("session span must have metadata under an enabling subscriber");
            assert!(
                meta.fields().field(SESSION_ID_FIELD).is_some(),
                "session span must expose `{SESSION_ID_FIELD}` for debug-log routing",
            );
        });
    }

    /// Legacy event emission is a no-op in the clean build, including the
    /// compatibility drain hook retained for upstream callers.
    #[tokio::test]
    async fn drain_pending_has_no_remote_work() {
        emit_event_with_origin(
            EmitterOrigin::Shell,
            "drain_probe",
            json!({ "probe": true }),
        );
        assert_eq!(PENDING_EVENTS.load(Ordering::Acquire), 0);

        let started = std::time::Instant::now();
        let budget = std::time::Duration::from_secs(5);
        drain_pending(budget).await;
        assert!(
            started.elapsed() < std::time::Duration::from_millis(100),
            "disabled telemetry drain must return immediately"
        );
    }

    /// Event-name prefixes are local diagnostic contract — they must not drift.
    #[test]
    fn event_prefix_is_stable_per_origin() {
        assert_eq!(EmitterOrigin::Shell.event_prefix(), "xaicode-shell-");
        assert_eq!(
            EmitterOrigin::Workspace.event_prefix(),
            "xaicode-workspace-"
        );
    }

    #[test]
    fn workspace_origin_event_name_uses_workspace_prefix() {
        let name = format!("{}turn", EmitterOrigin::Workspace.event_prefix());
        assert_eq!(name, "xaicode-workspace-turn");
    }

    /// `ALL` must enumerate every variant. Length completeness is also
    /// compiler-enforced by the const assertion in this module; this test pins that
    /// the known variants are present and that every origin yields a distinct,
    /// non-empty prefix (which `EnumCount` alone does not guarantee).
    #[test]
    fn all_covers_every_origin_with_distinct_nonempty_prefixes() {
        assert!(EmitterOrigin::ALL.contains(&EmitterOrigin::Shell));
        assert!(EmitterOrigin::ALL.contains(&EmitterOrigin::Workspace));
        assert_eq!(
            EmitterOrigin::ALL.len(),
            <EmitterOrigin as strum::EnumCount>::COUNT,
            "ALL must list every EmitterOrigin variant",
        );

        let mut prefixes: Vec<&str> = EmitterOrigin::ALL
            .iter()
            .map(|o| o.event_prefix())
            .collect();
        assert!(
            prefixes.iter().all(|p| !p.is_empty()),
            "every origin must have a non-empty prefix",
        );
        let total = prefixes.len();
        prefixes.sort_unstable();
        prefixes.dedup();
        assert_eq!(
            prefixes.len(),
            total,
            "every origin must yield a distinct prefix",
        );
    }
}
