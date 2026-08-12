//! Local feedback and signal bookkeeping.
//!
//! Feedback submissions are persisted with the session and are never sent to
//! a hosted service.  The signal actor remains useful for local usage,
//! diagnostics, and the existing session wire format.

use std::sync::Arc;
use std::time::Duration;

use prod_mc_cli_chat_proxy_types::feedback_types::{
    ClientType, FeedbackContent, FeedbackMode, FeedbackSubmission, FeedbackToolOutcome,
};

use crate::session::feedback::{
    FeedbackEvaluation, FeedbackHeuristics, FeedbackRequest, FeedbackTier, TriggerCondition,
};
use crate::session::persistence::{LocalFeedbackEntry, PersistenceMsg, UserFeedbackEntry};
use crate::session::signals::{SessionSignalsActor, SessionSignalsHandle, TurnDeltaSnapshot};

pub(crate) enum SubmitOutcome {
    /// The feedback was written to local session persistence.
    LocalOnly,
    /// Kept for callers that distinguish a successful remote submission.
    Submitted,
    /// Retained for wire/API compatibility; local-only paths do not produce it.
    Failed(anyhow::Error),
}

pub(crate) fn new_submission(
    session_id: String,
    client_type: ClientType,
    content: FeedbackContent,
) -> FeedbackSubmission {
    let mut submission = FeedbackSubmission::with_content(session_id, client_type, content);
    submission.shell_version = Some(xai_grok_version::VERSION.to_string());
    submission
}

#[derive(Debug)]
pub(crate) struct SubmitFeedbackOptions {
    pub solicited: bool,
    pub telemetry_enabled: bool,
    pub author_identity: Option<crate::util::user_identity::ResolvedUserIdentity>,
}

/// Persist feedback locally.  The old hosted client argument is intentionally
/// absent so a caller cannot accidentally re-enable network submission.
pub(crate) async fn submit_feedback_workflow(
    submission: &mut FeedbackSubmission,
    persistence_tx: Option<&tokio::sync::mpsc::UnboundedSender<PersistenceMsg>>,
    opts: SubmitFeedbackOptions,
) -> SubmitOutcome {
    let SubmitFeedbackOptions {
        solicited,
        telemetry_enabled: _,
        author_identity: _,
    } = opts;
    if let Some(tx) = persistence_tx {
        let entry = LocalFeedbackEntry::UserFeedback(UserFeedbackEntry {
            submitted_at: chrono::Utc::now(),
            session_id: submission.session_id.clone(),
            turn_number: submission.turn_number,
            solicited,
            request_id: submission.request_id.clone(),
            dismissed: false,
            submission: Some(submission.clone()),
        });
        let _ = tx.send(PersistenceMsg::Feedback(entry));
    }
    SubmitOutcome::LocalOnly
}

/// Chat-state fields passed by the session actor for local feedback records.
pub(crate) struct SessionFeedbackData {
    pub model_id: Option<String>,
    pub resolved_model_id: Option<String>,
    pub client_version: Option<String>,
    pub session_cwd: String,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct FeedbackFlags {
    pub enabled: bool,
    pub user: Option<crate::agent::config::FeedbackUserConfig>,
}

#[derive(Debug, Clone)]
pub struct FeedbackManagerConfig {
    pub sync_interval: Duration,
    pub feedback_enabled: bool,
    pub telemetry_enabled: bool,
    pub client_type: ClientType,
    pub loc_tracking_enabled: bool,
    pub drain_timeout: Duration,
    pub user: Option<crate::agent::config::FeedbackUserConfig>,
}

impl Default for FeedbackManagerConfig {
    fn default() -> Self {
        Self {
            sync_interval: Duration::from_secs(60),
            feedback_enabled: false,
            telemetry_enabled: false,
            client_type: ClientType::Agent,
            loc_tracking_enabled: false,
            drain_timeout: Duration::from_secs(30),
            user: None,
        }
    }
}

pub struct FeedbackManager {
    session_id: String,
    signals_handle: SessionSignalsHandle,
    heuristics: Arc<tokio::sync::RwLock<FeedbackHeuristics>>,
    config: FeedbackManagerConfig,
}

impl FeedbackManager {
    pub fn new(session_id: impl Into<String>, config: FeedbackManagerConfig) -> Self {
        let (signals_handle, actor) = SessionSignalsActor::with_sync_interval(config.sync_interval);
        tokio::spawn(actor.run());
        Self {
            session_id: session_id.into(),
            signals_handle,
            heuristics: Arc::new(tokio::sync::RwLock::new(FeedbackHeuristics::new())),
            config,
        }
    }

    pub fn local_only(session_id: impl Into<String>) -> Self {
        Self::new(session_id, FeedbackManagerConfig::default())
    }

    pub fn signals_handle(&self) -> SessionSignalsHandle {
        self.signals_handle.clone()
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn is_enabled(&self) -> bool {
        self.config.feedback_enabled
    }

    pub fn client_type(&self) -> ClientType {
        self.config.client_type
    }

    pub(crate) async fn submit_text_feedback(
        &self,
        text: String,
        session_data: SessionFeedbackData,
        persistence_tx: Option<&tokio::sync::mpsc::UnboundedSender<PersistenceMsg>>,
        telemetry_enabled: bool,
    ) -> SubmitOutcome {
        let sh = self.signals_handle();
        let (signals, tool_outcomes) = tokio::join!(sh.snapshot(), sh.last_turn_tool_outcomes());
        let signals = signals.unwrap_or_default();
        let tool_outcomes: Vec<FeedbackToolOutcome> = tool_outcomes
            .into_iter()
            .map(|o| FeedbackToolOutcome {
                tool_name: o.tool_name,
                calls: o.successes + o.failures,
                failures: o.failures,
            })
            .collect();
        let mut submission = new_submission(
            self.session_id.clone(),
            self.config.client_type,
            FeedbackContent::Text(text),
        );
        submission.turn_number = Some(signals.turn_count.saturating_sub(1) as i64);
        submission.model_id = session_data.model_id;
        submission.resolved_model_id = session_data.resolved_model_id;
        submission.tool_outcomes = tool_outcomes;
        submission.session_cwd = Some(session_data.session_cwd);
        submission.compaction_count = Some(signals.compaction_count as i64);
        submission.context_window_usage = Some(signals.context_window_usage);
        submission.context_tokens_used = Some(signals.context_tokens_used);
        submission.context_window_tokens = Some(signals.context_window_tokens);
        submission.client_version = session_data.client_version;
        submit_feedback_workflow(
            &mut submission,
            persistence_tx,
            SubmitFeedbackOptions {
                solicited: false,
                telemetry_enabled,
                author_identity: None,
            },
        )
        .await
    }

    pub async fn load_config(&self) {}

    pub async fn maybe_request_feedback(
        &self,
        _prompt_id: Option<String>,
    ) -> Option<FeedbackRequest> {
        None
    }

    pub async fn evaluate_heuristics(&self) -> Option<FeedbackEvaluation> {
        let signals = self.signals_handle.snapshot().await?;
        Some(self.heuristics.write().await.evaluate(&signals))
    }

    pub(crate) async fn force_feedback_request(
        &self,
        tier: FeedbackTier,
        mode: FeedbackMode,
    ) -> FeedbackRequest {
        let condition = TriggerCondition {
            tier,
            condition: "debug/trigger_feedback (local-only)".to_string(),
            signal_snapshot: crate::session::feedback::TriggerSignalSnapshot {
                turn_count: 0,
                tool_calls_count: 0,
                compactions_count: 0,
                errors_count: 0,
                cancellations_count: 0,
                has_reverted: false,
            },
        };
        FeedbackRequest::with_mode(self.session_id.clone(), condition, mode, true, None)
    }

    pub(crate) async fn send_turn_delta_with_snapshot(
        &self,
        _snapshot: Option<TurnDeltaSnapshot>,
        _request_id: Option<String>,
        _turn_duration_ms: Option<i64>,
        _turn_outcome: Option<String>,
        _model_fingerprint: Option<String>,
    ) {
        // Turn deltas stay in local session persistence; no analytics POST.
    }

    pub async fn sync_signals(&self) -> anyhow::Result<()> {
        Ok(())
    }

    pub(crate) async fn force_sync_signals(&self) -> anyhow::Result<()> {
        Ok(())
    }

    pub async fn run_sync_loop(self: Arc<Self>, cancel: tokio_util::sync::CancellationToken) {
        cancel.cancelled().await;
    }

    pub async fn shutdown(&self) {
        self.signals_handle.shutdown();
    }
}
