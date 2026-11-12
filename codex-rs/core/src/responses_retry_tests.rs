use super::INITIAL_WOL_PACKET_INTERVAL;
use super::MAX_WOL_PACKET_INTERVAL;
use super::ResponsesStreamRequest;
use super::ResponsesStreamRetryState;
use super::handle_response_stream_error;
use super::is_transient_provider_outage;
use super::log_retry;
use crate::realtime_history::RealtimeHistoryState;
use crate::session::step_context::StepContext;
use crate::session::tests::make_session_and_context;
use codex_http_client::RetryAfter;
use codex_model_provider::create_model_provider;
use codex_model_provider_info::ModelProviderInfo;
use codex_protocol::error::CodexErr;
use codex_protocol::error::CodexErrorDetails;
use codex_protocol::error::UnexpectedResponseError;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::Instant;
use tracing_test::internal::MockWriter;

#[tokio::test]
async fn sampling_retry_logs_stream_error_context() {
    let (_session, turn_context) = make_session_and_context().await;
    let buffer: &'static std::sync::Mutex<Vec<u8>> =
        Box::leak(Box::new(std::sync::Mutex::new(Vec::new())));
    let subscriber = tracing_subscriber::fmt()
        .with_ansi(false)
        .with_max_level(tracing::Level::WARN)
        .with_writer(MockWriter::new(buffer))
        .finish();
    let _subscriber_guard = tracing::subscriber::set_default(subscriber);

    log_retry(
        ResponsesStreamRequest::Sampling,
        &turn_context,
        &CodexErr::Stream("websocket closed by server before response.completed".to_string()),
        /*retries*/ 2,
        /*max_retries*/ 5,
        Duration::from_secs(1),
    );

    let logs = String::from_utf8(
        buffer
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone(),
    )
    .expect("retry log should be valid utf-8");
    assert!(logs.contains("stream disconnected - retrying sampling request"));
    assert!(logs.contains(&format!("turn_id={}", turn_context.sub_id)));
    assert!(logs.contains("retries=2"));
    assert!(logs.contains("max_retries=5"));
    assert!(logs.contains(
        "sampling_error=stream disconnected before completion: websocket closed by server before response.completed"
    ));
}

/// Time spent reporting a retry must count toward the server's original deadline.
#[tokio::test]
#[expect(
    clippy::await_holding_invalid_type,
    reason = "test holds the event-delivery lock to delay notification while virtual time advances"
)]
async fn stream_retry_preserves_deadline_across_delayed_notification() {
    let (mut session, turn_context) = make_session_and_context().await;
    let step_context = StepContext::for_test(Arc::new(turn_context));
    session.realtime_history = Some(Mutex::new(RealtimeHistoryState::default()));
    let mut client_session = session.services.model_client.new_session();
    // The second retry emits a notification even when release builds hide the first.
    let mut retry_state = ResponsesStreamRetryState {
        retries: 1,
        ..Default::default()
    };

    tokio::time::pause();
    let advice = RetryAfter::from_delay(Duration::from_secs(10)).expect("retry deadline");
    tokio::time::advance(Duration::from_secs(4)).await;

    // Hold the event-delivery lock so reporting the error consumes part of the deadline.
    let history_guard = session.realtime_history.as_ref().unwrap().lock().await;
    let retry = handle_response_stream_error(
        &mut retry_state,
        /*max_retries*/ 2,
        CodexErr::InternalServerError.with_retry_after(advice),
        &mut client_session,
        &session,
        &step_context,
        ResponsesStreamRequest::Sampling,
    );
    tokio::pin!(retry);
    assert!(futures::poll!(&mut retry).is_pending());
    tokio::time::advance(Duration::from_secs(4)).await;
    drop(history_guard);

    assert!(futures::poll!(&mut retry).is_pending());
    tokio::time::advance(Duration::from_secs(1)).await;
    assert!(futures::poll!(&mut retry).is_pending());
    retry.await.expect("retry should be allowed");
    // Tokio rounds timer deadlines up to the next millisecond.
    let resumed_at = Instant::now();
    assert!(
        (advice.deadline()..=advice.deadline() + Duration::from_millis(1)).contains(&resumed_at),
        "retry resumed at {resumed_at:?}, expected {advice:?}"
    );
}

const NON_FINITE_LOGITS: &str = "FreeToken aborted this request: non-finite logits";

/// Provider config for the deadline tests: a caller-chosen window polled every interval.
fn retry_deadline_provider(
    deadline_ms: Option<u64>,
    interval_ms: Option<u64>,
) -> ModelProviderInfo {
    let mut provider_info = ModelProviderInfo::default();
    provider_info.stream_retry_deadline_ms = deadline_ms;
    provider_info.stream_retry_interval_ms = interval_ms;
    provider_info
}

/// A stream aborted by a self-hosted server that is restarting is retried on a flat poll at the
/// configured interval until the deadline is consumed, not on doubling backoff.
#[tokio::test]
async fn sampling_retry_polls_until_provider_retry_deadline_expires() {
    let (mut session, mut turn_context) = make_session_and_context().await;
    let provider = retry_deadline_provider(Some(60_000), Some(15_000));
    turn_context.provider = create_model_provider(provider, /*auth_manager*/ None);
    let step_context = StepContext::for_test(Arc::new(turn_context));
    session.realtime_history = Some(Mutex::new(RealtimeHistoryState::default()));
    let mut client_session = session.services.model_client.new_session();
    let mut retry_state = ResponsesStreamRetryState::default();

    tokio::time::pause();
    let started_at = Instant::now();

    for attempt in 1..=4 {
        let err = CodexErr::Stream(NON_FINITE_LOGITS.to_string());
        let retry = handle_response_stream_error(
            &mut retry_state,
            /*max_retries*/ 5,
            err,
            &mut client_session,
            &session,
            &step_context,
            ResponsesStreamRequest::Sampling,
        )
        .await;
        retry.unwrap_or_else(|err| panic!("attempt {attempt} should retry: {err}"));
        let expected = Duration::from_secs(15 * attempt);
        let resumed_at = Instant::now();
        let elapsed = resumed_at - started_at;
        assert!(
            (expected..=expected + Duration::from_millis(10)).contains(&elapsed),
            "attempt {attempt} resumed at {elapsed:?}, expected {expected:?}"
        );
    }
    assert_eq!(retry_state.retry_count(), 4);
}

/// A 503 with no `Retry-After` is terminal under the count-based policy, so the deadline window
/// is the only thing that keeps the turn alive while the server comes back.
#[tokio::test]
async fn provider_retry_deadline_covers_unavailable_without_retry_advice() {
    let (mut session, mut turn_context) = make_session_and_context().await;
    let provider = retry_deadline_provider(Some(30_000), Some(10_000));
    turn_context.provider = create_model_provider(provider, /*auth_manager*/ None);
    let step_context = StepContext::for_test(Arc::new(turn_context));
    session.realtime_history = Some(Mutex::new(RealtimeHistoryState::default()));
    let mut client_session = session.services.model_client.new_session();
    let mut retry_state = ResponsesStreamRetryState::default();

    tokio::time::pause();

    for attempt in 1..=3 {
        let retry = handle_response_stream_error(
            &mut retry_state,
            /*max_retries*/ 5,
            CodexErr::ServerOverloaded,
            &mut client_session,
            &session,
            &step_context,
            ResponsesStreamRequest::Sampling,
        )
        .await;
        retry.unwrap_or_else(|err| panic!("attempt {attempt} should retry: {err}"));
    }

    let result = handle_response_stream_error(
        &mut retry_state,
        /*max_retries*/ 5,
        CodexErr::ServerOverloaded,
        &mut client_session,
        &session,
        &step_context,
        ResponsesStreamRequest::Sampling,
    )
    .await;
    let surfaced = result.expect_err("a 503 without advice must surface once the window closes");
    assert!(matches!(
        surfaced.details(),
        CodexErrorDetails::ServerOverloaded
    ));
}

/// Requests rejected for their contents must not consume the retry window.
#[tokio::test]
async fn provider_retry_deadline_ignores_rejected_requests() {
    let (mut session, mut turn_context) = make_session_and_context().await;
    let provider = retry_deadline_provider(Some(60_000), Some(15_000));
    turn_context.provider = create_model_provider(provider, /*auth_manager*/ None);
    let step_context = StepContext::for_test(Arc::new(turn_context));
    session.realtime_history = Some(Mutex::new(RealtimeHistoryState::default()));
    let mut client_session = session.services.model_client.new_session();
    let mut retry_state = ResponsesStreamRetryState::default();

    tokio::time::pause();

    let err = CodexErr::InvalidRequest("unsupported parameter".to_string());
    let result = handle_response_stream_error(
        &mut retry_state,
        /*max_retries*/ 5,
        err,
        &mut client_session,
        &session,
        &step_context,
        ResponsesStreamRequest::Sampling,
    )
    .await;
    let surfaced = result.expect_err("a rejected request must never be retried");
    assert!(matches!(
        surfaced.details(),
        CodexErrorDetails::InvalidRequest(_)
    ));
    assert_eq!(retry_state.retry_count(), 0);
}

/// The retry window is enabled by default, so a self-hosted server that answers 503 while it
/// reloads a model is polled without the operator configuring `stream_retry_deadline_ms`.
#[tokio::test]
async fn unconfigured_provider_retries_service_unavailable_by_default() {
    let (mut session, mut turn_context) = make_session_and_context().await;
    let provider = ModelProviderInfo::default();
    assert_eq!(
        provider.stream_retry_deadline(),
        Some(Duration::from_secs(600)),
        "an unconfigured provider must still retry a provider outage"
    );
    turn_context.provider = create_model_provider(provider, /*auth_manager*/ None);
    let step_context = StepContext::for_test(Arc::new(turn_context));
    session.realtime_history = Some(Mutex::new(RealtimeHistoryState::default()));
    let mut client_session = session.services.model_client.new_session();
    let mut retry_state = ResponsesStreamRetryState::default();

    tokio::time::pause();

    let err = CodexErr::UnexpectedStatus(UnexpectedResponseError {
        status: http::StatusCode::SERVICE_UNAVAILABLE,
        body: "model is still loading".to_string(),
        user_message: None,
        url: None,
        cf_ray: None,
        request_id: None,
        identity_authorization_error: None,
        identity_error_code: None,
    });
    let result = handle_response_stream_error(
        &mut retry_state,
        /*max_retries*/ 5,
        err,
        &mut client_session,
        &session,
        &step_context,
        ResponsesStreamRequest::Sampling,
    )
    .await;
    result.expect("a 503 from an unconfigured provider must be retried");
    assert_eq!(retry_state.retry_count(), 1);
}

#[test]
fn transient_outage_detection_spans_provider_availability() {
    let aborted = CodexErr::Stream(NON_FINITE_LOGITS.to_string());
    let capacity = CodexErr::new(CodexErrorDetails::PolicyCapacityUnavailable);
    let unavailable = CodexErr::UnexpectedStatus(UnexpectedResponseError {
        status: http::StatusCode::SERVICE_UNAVAILABLE,
        body: "FreeToken is restarting".to_string(),
        user_message: None,
        url: None,
        cf_ray: None,
        request_id: None,
        identity_authorization_error: None,
        identity_error_code: None,
    });
    assert!(is_transient_provider_outage(&aborted));
    assert!(is_transient_provider_outage(&CodexErr::InternalServerError));
    assert!(is_transient_provider_outage(&CodexErr::ServerOverloaded));
    assert!(is_transient_provider_outage(&capacity));
    assert!(is_transient_provider_outage(&unavailable));

    let rejected = CodexErr::InvalidRequest("unsupported parameter".to_string());
    let filtered = CodexErr::new(CodexErrorDetails::ContentFilter);
    let bad_request = CodexErr::UnexpectedStatus(UnexpectedResponseError {
        status: http::StatusCode::BAD_REQUEST,
        body: "unsupported parameter".to_string(),
        user_message: None,
        url: None,
        cf_ray: None,
        request_id: None,
        identity_authorization_error: None,
        identity_error_code: None,
    });
    assert!(!is_transient_provider_outage(&rejected));
    assert!(!is_transient_provider_outage(
        &CodexErr::ContextWindowExceeded
    ));
    assert!(!is_transient_provider_outage(&CodexErr::QuotaExceeded));
    assert!(!is_transient_provider_outage(&filtered));
    assert!(!is_transient_provider_outage(&bad_request));
}

#[test]
fn first_wol_packet_permit_is_immediate_and_the_next_is_throttled() {
    let failed_at = Instant::now();
    let mut retry_state = ResponsesStreamRetryState::default();

    assert!(
        retry_state.take_wol_packet_permit(failed_at),
        "the first connection failure should wake the host without waiting"
    );
    assert!(
        !retry_state.take_wol_packet_permit(failed_at),
        "back-to-back failures must not resend the magic packet"
    );
    assert!(
        !retry_state.take_wol_packet_permit(failed_at + INITIAL_WOL_PACKET_INTERVAL / 2),
        "a retry inside the throttling window must not resend the magic packet"
    );
    assert!(
        retry_state.take_wol_packet_permit(failed_at + INITIAL_WOL_PACKET_INTERVAL),
        "the magic packet should be permitted again once the window expires"
    );
}

#[test]
fn wol_packet_backoff_doubles_until_the_cap() {
    let started_at = Instant::now();
    let mut retry_state = ResponsesStreamRetryState::default();
    let mut sent_at = Vec::new();
    for second in 0..1_200_u64 {
        if retry_state.take_wol_packet_permit(started_at + Duration::from_secs(second)) {
            sent_at.push(Duration::from_secs(second));
        }
    }

    assert_eq!(
        sent_at,
        vec![
            Duration::from_secs(0),
            Duration::from_secs(30),
            Duration::from_secs(90),
            Duration::from_secs(210),
            Duration::from_secs(450),
            Duration::from_secs(750),
            Duration::from_secs(1_050),
        ],
        "magic packets should back off geometrically, then settle at {MAX_WOL_PACKET_INTERVAL:?}"
    );
}
