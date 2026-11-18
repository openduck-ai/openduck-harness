use super::policy::{CandidateKind, RequestObservation, RequestOutcome, RequestPurpose};
use crate::base::MessageStream;
use crate::conversation::message::{Message, MessageContentBlock};
use crate::errors::ProviderError;
use futures::future::BoxFuture;
use futures::{Stream, StreamExt};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

pub type FallbackStreamFn =
    Box<dyn FnOnce() -> BoxFuture<'static, Result<(MessageStream, String), ProviderError>> + Send>;

pub fn has_valid_output(message: &Message) -> bool {
    message.content.iter().any(|block| match block {
        MessageContentBlock::Text(t) => !t.text.is_empty(),
        MessageContentBlock::Thinking(t) => !t.thinking.is_empty(),
        MessageContentBlock::RedactedThinking(t) => !t.data.is_empty(),
        MessageContentBlock::ToolRequest(_)
        | MessageContentBlock::ToolResponse(_)
        | MessageContentBlock::FrontendToolRequest(_)
        | MessageContentBlock::Image(_) => true,
        MessageContentBlock::ToolConfirmationRequest(_)
        | MessageContentBlock::ActionRequired(_)
        | MessageContentBlock::SystemNotification(_)
        | MessageContentBlock::Error(_) => false,
    })
}

pub fn is_fallback_eligible(err: &ProviderError) -> bool {
    matches!(
        err,
        ProviderError::RateLimitExceeded { .. }
            | ProviderError::ServerError(_)
            | ProviderError::NetworkError(_)
            | ProviderError::EndpointNotFound(_)
    ) || matches!(err, ProviderError::ExecutionError(msg) if is_timeout_message(msg))
}

pub fn classify_error_outcome(err: &ProviderError) -> RequestOutcome {
    match err {
        ProviderError::NetworkError(msg)
        | ProviderError::ServerError(msg)
        | ProviderError::ExecutionError(msg)
            if is_timeout_message(msg) =>
        {
            RequestOutcome::Timeout
        }
        ProviderError::RateLimitExceeded { .. }
        | ProviderError::ServerError(_)
        | ProviderError::NetworkError(_)
        | ProviderError::EndpointNotFound(_) => RequestOutcome::AvailabilityError,
        ProviderError::Authentication(_)
        | ProviderError::NotConfigured
        | ProviderError::InvalidValue(_)
        | ProviderError::NotImplemented(_)
        | ProviderError::ContextLengthExceeded(_)
        | ProviderError::CreditsExhausted { .. }
        | ProviderError::Refusal { .. }
        | ProviderError::RequestFailed(_)
        | ProviderError::ExecutionError(_)
        | ProviderError::UsageError(_) => RequestOutcome::NonSwitchableError,
    }
}

fn is_timeout_message(msg: &str) -> bool {
    let lower = msg.to_ascii_lowercase();
    lower.contains("timeout") || lower.contains("timed out")
}

pub struct AttemptState {
    pub candidate: CandidateKind,
    pub actual_model: String,
    pub is_probe: bool,
    pub purpose: RequestPurpose,
    pub config_version: u64,
    pub started_at: Instant,
    pub first_valid_at: Option<Instant>,
    pub input_tokens: usize,
    pub output_tokens: usize,
    pub cache_read_tokens: usize,
    pub cache_creation_tokens: usize,
    pub attempts: u32,
    pub finished: bool,
}

impl AttemptState {
    pub fn new(
        candidate: CandidateKind,
        actual_model: String,
        is_probe: bool,
        purpose: RequestPurpose,
        config_version: u64,
        attempts: u32,
    ) -> Self {
        Self {
            candidate,
            actual_model,
            is_probe,
            purpose,
            config_version,
            started_at: Instant::now(),
            first_valid_at: None,
            input_tokens: 0,
            output_tokens: 0,
            cache_read_tokens: 0,
            cache_creation_tokens: 0,
            attempts,
            finished: false,
        }
    }

    pub fn into_observation(&mut self, outcome: RequestOutcome) -> Option<RequestObservation> {
        if self.finished {
            return None;
        }
        self.finished = true;
        let now = Instant::now();
        Some(RequestObservation {
            candidate: self.candidate,
            actual_model: self.actual_model.clone(),
            is_probe: self.is_probe,
            purpose: self.purpose,
            config_version: self.config_version,
            input_tokens: self.input_tokens,
            output_tokens: self.output_tokens,
            cache_read_tokens: self.cache_read_tokens,
            cache_creation_tokens: self.cache_creation_tokens,
            ttft: self
                .first_valid_at
                .map(|t| t.saturating_duration_since(self.started_at)),
            total_duration: now.saturating_duration_since(self.started_at),
            attempts: self.attempts,
            outcome,
        })
    }
}

pub type ObservationRecorder = Arc<dyn Fn(RequestObservation) + Send + Sync>;

enum StreamPhase {
    Active(MessageStream),
    Switching(BoxFuture<'static, Result<(MessageStream, String), ProviderError>>),
    Done,
}

pub struct ObservedStream {
    phase: StreamPhase,
    attempt: AttemptState,
    fallback: Option<FallbackStreamFn>,
    recorder: ObservationRecorder,
}

impl ObservedStream {
    pub fn new(
        stream: MessageStream,
        attempt: AttemptState,
        fallback: Option<FallbackStreamFn>,
        recorder: ObservationRecorder,
    ) -> Self {
        Self {
            phase: StreamPhase::Active(stream),
            attempt,
            fallback,
            recorder,
        }
    }

    fn record(&mut self, outcome: RequestOutcome) {
        if let Some(obs) = self.attempt.into_observation(outcome) {
            (self.recorder)(obs);
        }
    }
}

impl Drop for ObservedStream {
    fn drop(&mut self) {
        if !self.attempt.finished {
            self.record(RequestOutcome::Cancelled);
        }
    }
}

impl Stream for ObservedStream {
    type Item = Result<
        (
            Option<Message>,
            Option<crate::conversation::token_usage::ProviderUsage>,
        ),
        ProviderError,
    >;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = &mut *self;
        loop {
            match &mut this.phase {
                StreamPhase::Done => return Poll::Ready(None),
                StreamPhase::Switching(fut) => match fut.as_mut().poll(cx) {
                    Poll::Pending => return Poll::Pending,
                    Poll::Ready(Ok((new_stream, new_model))) => {
                        let next_candidate = this.attempt.candidate.other();
                        let next_attempts = this.attempt.attempts + 1;
                        this.attempt = AttemptState::new(
                            next_candidate,
                            new_model,
                            false,
                            this.attempt.purpose,
                            this.attempt.config_version,
                            next_attempts,
                        );
                        this.phase = StreamPhase::Active(new_stream);
                    }
                    Poll::Ready(Err(err)) => {
                        let outcome = classify_error_outcome(&err);
                        let next_candidate = this.attempt.candidate.other();
                        let mut failed_fallback = AttemptState::new(
                            next_candidate,
                            this.attempt.actual_model.clone(),
                            false,
                            this.attempt.purpose,
                            this.attempt.config_version,
                            this.attempt.attempts + 1,
                        );
                        if let Some(obs) = failed_fallback.into_observation(outcome) {
                            (this.recorder)(obs);
                        }
                        this.attempt.finished = true;
                        this.phase = StreamPhase::Done;
                        return Poll::Ready(Some(Err(err)));
                    }
                },
                StreamPhase::Active(stream) => match stream.poll_next_unpin(cx) {
                    Poll::Pending => return Poll::Pending,
                    Poll::Ready(None) => {
                        let outcome = if this.attempt.first_valid_at.is_some() {
                            RequestOutcome::Success
                        } else {
                            // Stream ended with no valid output; treat as success with 0 TTFT
                            // only if no error occurred, or record total duration.
                            this.attempt.first_valid_at =
                                Some(this.attempt.started_at + Duration::from_millis(0));
                            RequestOutcome::Success
                        };
                        this.record(outcome);
                        this.phase = StreamPhase::Done;
                        return Poll::Ready(None);
                    }
                    Poll::Ready(Some(Ok((msg_opt, mut usage_opt)))) => {
                        if this.attempt.first_valid_at.is_none() {
                            if let Some(msg) = &msg_opt {
                                if has_valid_output(msg) {
                                    this.attempt.first_valid_at = Some(Instant::now());
                                }
                            }
                        }
                        if let Some(usage) = &mut usage_opt {
                            if !usage.model.is_empty() {
                                this.attempt.actual_model = usage.model.clone();
                            } else {
                                usage.model = this.attempt.actual_model.clone();
                            }
                            usage
                                .stats
                                .get_or_insert_with(Default::default)
                                .routing_tier = Some(this.attempt.candidate.as_str().to_string());
                            if let Some(i) = usage.usage.input_tokens {
                                this.attempt.input_tokens = i.max(0) as usize;
                            }
                            if let Some(o) = usage.usage.output_tokens {
                                this.attempt.output_tokens = o.max(0) as usize;
                            }
                            if let Some(cr) = usage.usage.cache_read_input_tokens {
                                this.attempt.cache_read_tokens = cr.max(0) as usize;
                            }
                            if let Some(cc) = usage.usage.cache_write_input_tokens {
                                this.attempt.cache_creation_tokens = cc.max(0) as usize;
                            }
                        }
                        return Poll::Ready(Some(Ok((msg_opt, usage_opt))));
                    }
                    Poll::Ready(Some(Err(err))) => {
                        let outcome = classify_error_outcome(&err);
                        this.record(outcome);

                        if this.attempt.first_valid_at.is_none() && is_fallback_eligible(&err) {
                            if let Some(fallback_fn) = this.fallback.take() {
                                this.phase = StreamPhase::Switching(fallback_fn());
                                continue;
                            }
                        }
                        this.phase = StreamPhase::Done;
                        return Poll::Ready(Some(Err(err)));
                    }
                },
            }
        }
    }
}
