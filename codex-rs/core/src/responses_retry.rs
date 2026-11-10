//! Shared retry and transport fallback decisions for Responses requests.
//! Content-filter guidance is recorded for sampling requests before retry decisions.
//! Server advice controls timing without extending configured retry limits.

use std::ops::RangeInclusive;
use std::time::Duration;

use crate::client::ModelClientSession;
use crate::context::ContentFilterGuidance;
use crate::context::ContextualUserFragment;
use crate::session::session::Session;
use crate::session::step_context::StepContext;
use crate::session::turn_context::TurnContext;
use codex_client::RetryOperation;
use codex_features::Feature;
use codex_http_client::RetryAfter;
use codex_protocol::error::CodexErr;
use codex_protocol::error::CodexErrorDetails;
use codex_protocol::protocol::EventMsg;
use codex_protocol::protocol::WarningEvent;
use tokio::time::Instant;
use tracing::warn;

const INITIAL_CONNECTION_RETRY_DELAY: Duration = Duration::from_secs(5);
const MAX_CONNECTION_RETRY_DELAY: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Copy)]
pub(crate) enum ResponsesStreamRequest {
    Sampling,
    RemoteCompactionV2,
}

pub(crate) struct ResponsesStreamRetryState {
    retries: u64,
    connection_retries: u64,
    connection_retry_delay: Duration,
    outage_started_at: Option<Instant>,
    outage_retries: u64,
}

impl Default for ResponsesStreamRetryState {
    fn default() -> Self {
        Self {
            retries: 0,
            connection_retries: 0,
            connection_retry_delay: INITIAL_CONNECTION_RETRY_DELAY,
            outage_started_at: None,
            outage_retries: 0,
        }
    }
}

impl ResponsesStreamRetryState {
    /// Failed sampling attempts that preceded the next successful response.
    pub(crate) fn retry_count(&self) -> u32 {
        let attempts = self
            .retries
            .saturating_add(self.connection_retries)
            .saturating_add(self.outage_retries);
        u32::try_from(attempts).unwrap_or(u32::MAX)
    }
}

/// Server retry advice retained after stream retries are exhausted. The turn ID
/// prevents a reused Guardian session from applying advice from an earlier review.
pub(crate) struct ExhaustedResponseRetry {
    pub(crate) turn_id: String,
    pub(crate) retry_at: Option<tokio::time::Instant>,
}

/// Returns `Ok(())` when the caller should retry the request loop, or the original error when
/// it is terminal or the retry budget is exhausted.
#[tracing::instrument(level = "trace", skip_all, fields(request = ?request))]
pub(crate) async fn handle_response_stream_error(
    retry_state: &mut ResponsesStreamRetryState,
    max_retries: u64,
    err: CodexErr,
    client_session: &mut ModelClientSession,
    sess: &Session,
    step_context: &StepContext,
    request: ResponsesStreamRequest,
) -> Result<(), CodexErr> {
    let turn_context = &step_context.turn;
    if matches!(request, ResponsesStreamRequest::Sampling)
        && matches!(err.details(), CodexErrorDetails::ContentFilter)
    {
        let model_info = &step_context.settings.model_info;
        let guidance = ContentFilterGuidance {
            text: codex_prompts::ResolvedModelMessages::from_model(model_info)
                .content_filter_guidance()
                .to_string(),
        };
        sess.record_conversation_items(
            turn_context,
            model_info,
            &[ContextualUserFragment::into(guidance)],
        )
        .await;
    }
    let operation = match request {
        ResponsesStreamRequest::Sampling => RetryOperation::Sampling,
        ResponsesStreamRequest::RemoteCompactionV2 => RetryOperation::RemoteCompactionV2,
    };

    // A self-hosted server that is restarting answers with 503s or aborts the stream outright.
    // Keep the turn alive on a flat poll until the configured wall-clock deadline expires, then
    // fall through to the count-based policy so the error is still surfaced.
    if let Some(deadline) = turn_context.provider.info().stream_retry_deadline()
        && matches!(request, ResponsesStreamRequest::Sampling)
        && !turn_context.session_source.is_internal()
        && is_transient_provider_outage(&err)
    {
        let now = Instant::now();
        let elapsed = retry_state
            .outage_started_at
            .map(|started_at| now.saturating_duration_since(started_at))
            .unwrap_or_default();
        if elapsed < deadline {
            if retry_state.outage_started_at.is_none() {
                retry_state.outage_started_at = Some(now);
            }
            retry_state.outage_retries = retry_state.outage_retries.saturating_add(1);
            let delay = turn_context
                .provider
                .info()
                .stream_retry_interval()
                .min(deadline.saturating_sub(elapsed));
            warn!(
                turn_id = %turn_context.sub_id,
                outage_retries = retry_state.outage_retries,
                ?elapsed,
                ?deadline,
                ?delay,
                outage_error = %err,
                "provider could not serve the request; retrying before deadline"
            );
            sess.notify_stream_error(
                turn_context,
                format!("Reconnecting... provider unavailable ({elapsed:?}/{deadline:?})"),
                err,
            )
            .await;
            codex_client::record_retry!(retry_state.outage_retries, delay, operation);
            tokio::time::sleep(delay).await;
            return Ok(());
        }
        warn!(
            turn_id = %turn_context.sub_id,
            ?deadline,
            "provider still unavailable after retry deadline"
        );
    }

    let retry_count = retry_state.retries.saturating_add(1);
    let Some(delay) = err.retry_delay(retry_count) else {
        return Err(err);
    };
    let retry_after = err.retry_after();

    if turn_context
        .config
        .features
        .enabled(Feature::UnboundedConnectionRetries)
        && matches!(request, ResponsesStreamRequest::Sampling)
        && matches!(err.details(), CodexErrorDetails::ConnectionFailed(_))
        && !turn_context.session_source.is_internal()
        && !turn_context.provider.info().is_amazon_bedrock()
    {
        let retry_delay = retry_state.connection_retry_delay;
        warn!(
            turn_id = %turn_context.sub_id,
            error = %err,
            ?retry_delay,
            "stream connection failed; waiting to retry"
        );
        sess.notify_stream_error(turn_context, "Reconnecting... waiting for network", err)
            .await;
        retry_state.connection_retries = retry_state.connection_retries.saturating_add(1);
        codex_client::record_retry!(retry_state.connection_retries, retry_delay, operation);
        tokio::time::sleep(retry_delay).await;
        retry_state.connection_retry_delay = retry_delay
            .saturating_mul(2)
            .min(MAX_CONNECTION_RETRY_DELAY);
        return Ok(());
    }

    if retry_state.retries >= max_retries
        && client_session.try_switch_fallback_transport(
            &turn_context.session_telemetry,
            turn_context.model_info(),
        )
    {
        // Changing transport must not bypass the server's retry deadline.
        if let Some(retry_after) = retry_after {
            tokio::time::sleep_until(retry_after.deadline()).await;
        }
        sess.send_event(
            turn_context,
            EventMsg::Warning(WarningEvent {
                message: format!("Falling back from WebSockets to HTTPS transport. {err:#}"),
            }),
        )
        .await;
        retry_state.retries = 0;
        return Ok(());
    }

    if retry_state.retries < max_retries {
        retry_state.retries = retry_count;
        log_retry(request, turn_context, &err, retry_count, max_retries, delay);

        // In release builds, hide the first websocket retry notification to reduce noisy
        // transient reconnect messages. In debug builds, keep full visibility for diagnosis.
        let report_error = retry_count > 1
            || cfg!(debug_assertions)
            || !sess.services.model_client.responses_websocket_enabled();
        if report_error {
            // Surface retry information to any UI/front-end so the user understands what is
            // happening instead of staring at a seemingly frozen screen.
            sess.notify_stream_error(
                turn_context,
                format!("Reconnecting... {retry_count}/{max_retries}"),
                err,
            )
            .await;
        }
        // Use one clock sample so local backoff telemetry retains the selected delay.
        let now = Instant::now();
        let retry_at = retry_after.map(RetryAfter::deadline).unwrap_or(now + delay);
        let delay = retry_at.saturating_duration_since(now);
        codex_client::record_retry!(retry_count, delay, operation);
        tokio::time::sleep_until(retry_at).await;
        return Ok(());
    }

    sess.services
        .thread_extension_data
        .insert(ExhaustedResponseRetry {
            turn_id: turn_context.sub_id.clone(),
            retry_at: retry_after.map(RetryAfter::deadline),
        });
    Err(err)
}

/// Statuses that mean the provider could not serve the request right now rather than rejecting
/// its contents, so spending the retry deadline on them is worthwhile.
const TRANSIENT_SERVER_STATUS_RANGE: RangeInclusive<u16> = 500..=599;

/// Returns whether `err` indicates transient provider unavailability, as opposed to a request
/// that was rejected for its contents. Only transient failures consume the retry deadline.
fn is_transient_provider_outage(err: &CodexErr) -> bool {
    if let Some(status) = err.http_status_code_value() {
        return TRANSIENT_SERVER_STATUS_RANGE.contains(&status);
    }
    matches!(
        err.details(),
        CodexErrorDetails::Stream(..)
            | CodexErrorDetails::ResponseStreamFailed(_)
            | CodexErrorDetails::ConnectionFailed(_)
            | CodexErrorDetails::InternalServerError
            | CodexErrorDetails::ServerOverloaded
            | CodexErrorDetails::PolicyCapacityUnavailable
            | CodexErrorDetails::Timeout
            | CodexErrorDetails::RequestTimeout
    )
}

fn log_retry(
    request: ResponsesStreamRequest,
    turn_context: &TurnContext,
    err: &CodexErr,
    retries: u64,
    max_retries: u64,
    delay: Duration,
) {
    match request {
        ResponsesStreamRequest::Sampling => {
            warn!(
                turn_id = %turn_context.sub_id,
                retries,
                max_retries,
                sampling_error = %err,
                "stream disconnected - retrying sampling request ({retries}/{max_retries} in {delay:?})...",
            );
        }
        ResponsesStreamRequest::RemoteCompactionV2 => {
            warn!(
                turn_id = %turn_context.sub_id,
                retries,
                max_retries,
                compact_error = %err,
                "remote compaction v2 stream failed; retrying request after delay"
            );
        }
    }
}

#[cfg(test)]
#[path = "responses_retry_tests.rs"]
mod tests;
