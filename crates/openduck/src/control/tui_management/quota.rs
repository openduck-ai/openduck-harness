//! Typed quota status for the TUI management domain.
//!
//! Sits beside [`super::models::QuotaSnapshotRecord`] as the read model for a
//! later store and API. It does not query providers, choose agents, or rewrite
//! legacy snapshot rows.
//!
//! Availability, freshness, and refresh outcome answer different questions.
//! A failed or cached read keeps the last observation: it is not a refresh,
//! and it is not evidence that the pool just became exhausted. A relative
//! reset stays anchored to the source observation timestamp stored with it.
//!
//! [`QuotaAgentStatus::admission_at`] is the scheduling decision for one book.
//! It does not refresh a provider or rewrite observations.

use std::collections::BTreeSet;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use openduck_mcp::tui_session::AgentCliKind;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Read-model default for how long an observation stays fresh.
///
/// This is not a scheduler weight or a session-blocking policy.
pub const DEFAULT_QUOTA_STALE_AFTER_SECONDS: u64 = 300;

/// Remaining percent at or above this keeps [`NORMAL_SAMPLING_WEIGHT`].
pub const LOW_REMAINING_PERCENT: f64 = 20.0;

/// Full positive sampling weight. Eligible weights are `1..=NORMAL`.
pub const NORMAL_SAMPLING_WEIGHT: u32 = 1_000;

/// Smallest weight that is still eligible to sample.
pub const MIN_POSITIVE_SAMPLING_WEIGHT: u32 = 50;

const FAILED_REFRESH_ERROR: &str = "quota refresh failed";

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum QuotaModelError {
    #[error("unsupported agent CLI kind '{0}'")]
    UnsupportedAgent(String),
    #[error("pool id is required")]
    EmptyPoolId,
    #[error("quota type is required")]
    EmptyQuotaType,
    #[error("observation source is required")]
    EmptySource,
    #[error("unparsed reset text is required")]
    EmptyUnparsedReset,
    #[error("remaining percent must be finite")]
    RemainingPercentNotFinite,
    #[error("remaining percent {0} is outside 0..=100")]
    RemainingPercentOutOfRange(String),
    #[error("relative reset is anchored to {anchor} but the observation was at {observed_at}")]
    ResetAnchorMismatch {
        anchor: DateTime<Utc>,
        observed_at: DateTime<Utc>,
    },
    #[error("observation agent {actual} does not match status agent {expected}")]
    ScopeAgentMismatch {
        expected: AgentCliKind,
        actual: AgentCliKind,
    },
    #[error("duplicate quota scope in one collection")]
    DuplicateScope,
}

/// Whether a scoped pool can still be used, independent of freshness.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum QuotaAvailability {
    Unknown,
    Available,
    Exhausted,
}

/// Read-time age of an observation. Never stored as the source of truth.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum QuotaFreshness {
    NeverObserved,
    Fresh,
    Stale,
}

/// Outcome of the attempt recorded on a status row or on the agent book.
///
/// `Refreshed` means that attempt wrote a new observation. Cached, empty,
/// unsupported, and failed attempts use another variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum QuotaRefreshOutcome {
    NeverAttempted,
    Refreshed,
    Unavailable,
    Unsupported,
    Failed,
}

/// When the quota resets, without inventing a timestamp the source did not give.
///
/// Serialized as an internally tagged object so every variant has a `kind`.
/// `After` adds `seconds` to its stored `observedAt`. Later refresh attempts
/// must not move that anchor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum QuotaReset {
    Unknown,
    At {
        at: DateTime<Utc>,
    },
    After {
        seconds: u64,
        observed_at: DateTime<Utc>,
    },
    Unparsed {
        text: String,
    },
}

impl QuotaReset {
    pub fn at(at: DateTime<Utc>) -> Self {
        Self::At { at }
    }

    pub fn after(seconds: u64, observed_at: DateTime<Utc>) -> Self {
        Self::After {
            seconds,
            observed_at,
        }
    }

    pub fn unparsed(text: impl Into<String>) -> Result<Self, QuotaModelError> {
        let text = text.into();
        if text.trim().is_empty() {
            return Err(QuotaModelError::EmptyUnparsedReset);
        }
        Ok(Self::Unparsed { text })
    }

    /// Absolute reset instant when the source defined one.
    ///
    /// Relative durations use the timestamp stored on this value. `Unknown`
    /// and `Unparsed` do not become instants.
    pub fn resolves_at(&self) -> Option<DateTime<Utc>> {
        match self {
            Self::Unknown | Self::Unparsed { .. } => None,
            Self::At { at } => Some(*at),
            Self::After {
                seconds,
                observed_at,
            } => {
                let seconds = i64::try_from(*seconds).ok()?;
                let delta = chrono::Duration::try_seconds(seconds)?;
                observed_at.checked_add_signed(delta)
            }
        }
    }
}

/// Canonical quota identity. `None` account or model means shared across that
/// dimension. Exact `*` is an id, not a glob, and ids are not lowercased.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaScope {
    pub agent: AgentCliKind,
    pub account_id: Option<String>,
    pub model_id: Option<String>,
    pub pool_id: String,
    pub quota_type: String,
}

impl QuotaScope {
    pub fn try_from_parts(
        agent: &str,
        account_id: Option<&str>,
        model_id: Option<&str>,
        pool_id: &str,
        quota_type: &str,
    ) -> Result<Self, QuotaModelError> {
        let pool_id = required_token(pool_id, QuotaModelError::EmptyPoolId)?;
        let quota_type = required_token(quota_type, QuotaModelError::EmptyQuotaType)?;
        Ok(Self {
            agent: parse_agent(agent)?,
            account_id: optional_token(account_id),
            model_id: optional_token(model_id),
            pool_id,
            quota_type,
        })
    }

    /// Stable identity: `[agent, account|null, model|null, pool, type]`.
    pub fn identity_key(&self) -> String {
        serde_json::json!([
            self.agent.to_string(),
            self.account_id,
            self.model_id,
            self.pool_id,
            self.quota_type,
        ])
        .to_string()
    }
}

/// One source observation. Percentage `None` means the source did not say.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaObservation {
    pub scope: QuotaScope,
    pub availability: QuotaAvailability,
    pub remaining_percent: Option<f64>,
    pub reset: QuotaReset,
    pub source: String,
    pub observed_at: DateTime<Utc>,
}

impl QuotaObservation {
    pub fn try_new(
        scope: QuotaScope,
        availability: QuotaAvailability,
        remaining_percent: Option<f64>,
        reset: QuotaReset,
        source: impl Into<String>,
        observed_at: DateTime<Utc>,
    ) -> Result<Self, QuotaModelError> {
        let source = source.into();
        if source.trim().is_empty() {
            return Err(QuotaModelError::EmptySource);
        }
        let remaining_percent = normalize_remaining_percent(remaining_percent)?;
        let observation = Self {
            scope,
            availability,
            remaining_percent,
            reset,
            source: source.trim().to_string(),
            observed_at,
        };
        validate_observation(&observation)?;
        Ok(observation)
    }
}

/// Persisted per-scope status. `freshness` is computed by [`Self::freshness_at`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaStatus {
    pub scope: QuotaScope,
    pub observation: Option<QuotaObservation>,
    pub last_attempt_at: Option<DateTime<Utc>>,
    pub refresh_outcome: QuotaRefreshOutcome,
    pub last_error: Option<String>,
    /// True only when `last_attempt_at` wrote `observation`.
    /// A later attempt that keeps this row clears the flag without labeling the
    /// cached observation as refreshed.
    #[serde(default)]
    pub replaced_by_latest_attempt: bool,
    #[serde(default = "default_stale_after_seconds")]
    pub stale_after_seconds: u64,
}

impl QuotaStatus {
    pub fn freshness_at(&self, now: DateTime<Utc>) -> QuotaFreshness {
        match &self.observation {
            Some(observation) => observation_freshness(observation, now, self.stale_after_seconds),
            None => QuotaFreshness::NeverObserved,
        }
    }
}

/// Agent-level book. No rows means the agent has no scoped observation yet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaAgentStatus {
    pub agent: AgentCliKind,
    pub statuses: Vec<QuotaStatus>,
    pub last_attempt_at: Option<DateTime<Utc>>,
    pub refresh_outcome: QuotaRefreshOutcome,
    pub last_error: Option<String>,
    #[serde(default = "default_stale_after_seconds")]
    pub stale_after_seconds: u64,
}

impl QuotaAgentStatus {
    pub fn never_observed(agent: AgentCliKind) -> Self {
        Self {
            agent,
            statuses: Vec::new(),
            last_attempt_at: None,
            refresh_outcome: QuotaRefreshOutcome::NeverAttempted,
            last_error: None,
            stale_after_seconds: DEFAULT_QUOTA_STALE_AFTER_SECONDS,
        }
    }

    pub fn try_from_agent_name(agent: &str) -> Result<Self, QuotaModelError> {
        Ok(Self::never_observed(parse_agent(agent)?))
    }

    /// Applies one collection at `attempt_at` without reading a provider.
    ///
    /// Invalid batches, empty batches, and failed queries keep every prior
    /// observation. Only a non-empty valid `Observed` collection replaces the
    /// scopes it names. Omitted scopes keep their observation and are not
    /// marked replaced by this attempt.
    #[must_use]
    pub fn apply_refresh(&self, attempt_at: DateTime<Utc>, collection: QuotaCollection) -> Self {
        match collection {
            QuotaCollection::Failed { error } => self.retain(
                attempt_at,
                QuotaRefreshOutcome::Failed,
                Some(normalize_error(error)),
            ),
            QuotaCollection::Unavailable => {
                self.retain(attempt_at, QuotaRefreshOutcome::Unavailable, None)
            }
            QuotaCollection::Unsupported => {
                self.retain(attempt_at, QuotaRefreshOutcome::Unsupported, None)
            }
            QuotaCollection::Observed(observations) if observations.is_empty() => {
                self.retain(attempt_at, QuotaRefreshOutcome::Unavailable, None)
            }
            QuotaCollection::Observed(observations) => {
                match self.validate_collection(&observations) {
                    Ok(()) => self.replace_observed(attempt_at, observations),
                    Err(error) => self.retain(
                        attempt_at,
                        QuotaRefreshOutcome::Failed,
                        Some(error.to_string()),
                    ),
                }
            }
        }
    }

    pub fn replaced_by_latest_attempt(&self, status: &QuotaStatus) -> bool {
        status.replaced_by_latest_attempt
            && status.refresh_outcome == QuotaRefreshOutcome::Refreshed
            && status.observation.is_some()
            && status.last_attempt_at == self.last_attempt_at
    }

    pub fn views_at(&self, now: DateTime<Utc>) -> Vec<QuotaStatusView> {
        self.statuses
            .iter()
            .map(|status| QuotaStatusView::from_status(status, self, now))
            .collect()
    }

    /// Scheduling decision at `now`. Does not modify this book.
    ///
    /// Blocks only when a fresh scope is [`QuotaAvailability::Exhausted`] and
    /// its reset resolves to an instant still in the future. An unknown or
    /// unparsed reset does not block. Any other stale scope asks for a recheck
    /// instead of a block. A scope whose known reset has been reached drops
    /// out of both the block and the weight, and the stored observation is
    /// left as recorded.
    #[must_use]
    pub fn admission_at(&self, now: DateTime<Utc>) -> QuotaAdmission {
        let mut blocked_until: Option<DateTime<Utc>> = None;
        let mut recheck = false;
        let mut weight: Option<u32> = None;

        for status in &self.statuses {
            match scope_scheduling_effect(status, now) {
                ScopeSchedulingEffect::Ignore | ScopeSchedulingEffect::Restored => {}
                ScopeSchedulingEffect::Recheck => recheck = true,
                ScopeSchedulingEffect::Block { until } => {
                    blocked_until = Some(match blocked_until {
                        Some(existing) => existing.max(until),
                        None => until,
                    });
                }
                ScopeSchedulingEffect::Weight(scope_weight) => {
                    weight = Some(match weight {
                        Some(existing) => existing.min(scope_weight),
                        None => scope_weight,
                    });
                }
            }
        }

        if let Some(until) = blocked_until {
            return QuotaAdmission::Blocked { until };
        }
        if recheck {
            return QuotaAdmission::Recheck;
        }
        QuotaAdmission::Eligible {
            weight: weight.unwrap_or(NORMAL_SAMPLING_WEIGHT),
        }
    }

    fn retain(
        &self,
        attempt_at: DateTime<Utc>,
        outcome: QuotaRefreshOutcome,
        error: Option<String>,
    ) -> Self {
        debug_assert_ne!(outcome, QuotaRefreshOutcome::Refreshed);
        let statuses = self
            .statuses
            .iter()
            .map(|status| QuotaStatus {
                scope: status.scope.clone(),
                observation: status.observation.clone(),
                last_attempt_at: Some(attempt_at),
                refresh_outcome: outcome,
                last_error: error.clone(),
                replaced_by_latest_attempt: false,
                stale_after_seconds: status.stale_after_seconds,
            })
            .collect();
        Self {
            agent: self.agent,
            statuses,
            last_attempt_at: Some(attempt_at),
            refresh_outcome: outcome,
            last_error: error,
            stale_after_seconds: self.stale_after_seconds,
        }
    }

    fn replace_observed(
        &self,
        attempt_at: DateTime<Utc>,
        observations: Vec<QuotaObservation>,
    ) -> Self {
        let mut statuses: Vec<QuotaStatus> = self
            .statuses
            .iter()
            .map(|status| {
                let mut kept = status.clone();
                kept.replaced_by_latest_attempt = false;
                kept
            })
            .collect();
        for observation in observations {
            let key = observation.scope.identity_key();
            let stale_after_seconds = statuses
                .iter()
                .find(|status| status.scope.identity_key() == key)
                .map(|status| status.stale_after_seconds)
                .unwrap_or(self.stale_after_seconds);
            let updated = QuotaStatus {
                scope: observation.scope.clone(),
                observation: Some(observation),
                last_attempt_at: Some(attempt_at),
                refresh_outcome: QuotaRefreshOutcome::Refreshed,
                last_error: None,
                replaced_by_latest_attempt: true,
                stale_after_seconds,
            };
            if let Some(existing) = statuses
                .iter_mut()
                .find(|status| status.scope.identity_key() == key)
            {
                *existing = updated;
            } else {
                statuses.push(updated);
            }
        }
        Self {
            agent: self.agent,
            statuses,
            last_attempt_at: Some(attempt_at),
            refresh_outcome: QuotaRefreshOutcome::Refreshed,
            last_error: None,
            stale_after_seconds: self.stale_after_seconds,
        }
    }

    fn validate_collection(
        &self,
        observations: &[QuotaObservation],
    ) -> Result<(), QuotaModelError> {
        let mut seen = BTreeSet::new();
        for observation in observations {
            if observation.scope.agent != self.agent {
                return Err(QuotaModelError::ScopeAgentMismatch {
                    expected: self.agent,
                    actual: observation.scope.agent,
                });
            }
            validate_observation(observation)?;
            if !seen.insert(observation.scope.identity_key()) {
                return Err(QuotaModelError::DuplicateScope);
            }
        }
        Ok(())
    }
}

/// Pure collection result. Constructing this does not read a provider.
#[derive(Debug, Clone, PartialEq)]
pub enum QuotaCollection {
    Observed(Vec<QuotaObservation>),
    Unavailable,
    Unsupported,
    Failed { error: String },
}

/// Freshness joined onto a status at `as_of`. Do not persist this view.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaStatusView {
    pub scope: QuotaScope,
    pub observation: Option<QuotaObservation>,
    pub freshness: QuotaFreshness,
    pub last_attempt_at: Option<DateTime<Utc>>,
    pub refresh_outcome: QuotaRefreshOutcome,
    pub last_error: Option<String>,
    pub replaced_by_latest_attempt: bool,
    pub stale_after_seconds: u64,
    pub as_of: DateTime<Utc>,
}

impl QuotaStatusView {
    fn from_status(status: &QuotaStatus, agent: &QuotaAgentStatus, now: DateTime<Utc>) -> Self {
        Self {
            scope: status.scope.clone(),
            observation: status.observation.clone(),
            freshness: status.freshness_at(now),
            last_attempt_at: status.last_attempt_at,
            refresh_outcome: status.refresh_outcome,
            last_error: status.last_error.clone(),
            replaced_by_latest_attempt: agent.replaced_by_latest_attempt(status),
            stale_after_seconds: status.stale_after_seconds,
            as_of: now,
        }
    }
}

pub fn parse_remaining_percent(value: f64) -> Result<f64, QuotaModelError> {
    if !value.is_finite() {
        return Err(QuotaModelError::RemainingPercentNotFinite);
    }
    if !(0.0..=100.0).contains(&value) {
        return Err(QuotaModelError::RemainingPercentOutOfRange(format!(
            "{value}"
        )));
    }
    Ok(value)
}

pub fn normalize_remaining_percent(value: Option<f64>) -> Result<Option<f64>, QuotaModelError> {
    value.map(parse_remaining_percent).transpose()
}

/// Pure scheduling decision for one agent quota book.
///
/// `weight` is an integer share of [`NORMAL_SAMPLING_WEIGHT`]. It stays
/// positive whenever the agent is eligible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuotaAdmission {
    Eligible {
        weight: u32,
    },
    Blocked {
        until: DateTime<Utc>,
    },
    /// A stored scope is stale for a reason other than its reset deadline.
    Recheck,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScopeSchedulingEffect {
    Ignore,
    Restored,
    Recheck,
    Block { until: DateTime<Utc> },
    Weight(u32),
}

/// Positive sampling weight for a remaining percent.
///
/// `None`, a non-finite value, and any percent at or above
/// [`LOW_REMAINING_PERCENT`] use [`NORMAL_SAMPLING_WEIGHT`]. A lower percent
/// scales linearly from [`MIN_POSITIVE_SAMPLING_WEIGHT`] at 0% toward, but
/// not onto, the normal weight. The map truncates toward zero and is
/// deterministic.
#[must_use]
pub fn sampling_weight_for_remaining(remaining_percent: Option<f64>) -> u32 {
    let Some(percent) = remaining_percent else {
        return NORMAL_SAMPLING_WEIGHT;
    };
    if !percent.is_finite() || percent >= LOW_REMAINING_PERCENT {
        return NORMAL_SAMPLING_WEIGHT;
    }
    let low_millis = (LOW_REMAINING_PERCENT * 1_000.0) as u64;
    let mut millis = (percent.max(0.0) * 1_000.0).round() as u64;
    if millis >= low_millis {
        millis = low_millis - 1;
    }
    let span = u64::from(NORMAL_SAMPLING_WEIGHT - MIN_POSITIVE_SAMPLING_WEIGHT);
    let weight = u64::from(MIN_POSITIVE_SAMPLING_WEIGHT) + span * millis / low_millis;
    u32::try_from(weight).unwrap_or(MIN_POSITIVE_SAMPLING_WEIGHT)
}

fn scope_scheduling_effect(status: &QuotaStatus, now: DateTime<Utc>) -> ScopeSchedulingEffect {
    let Some(observation) = status.observation.as_ref() else {
        return ScopeSchedulingEffect::Ignore;
    };
    // A timestamp in the future is not a reading we can schedule on.
    if observation.observed_at > now {
        return ScopeSchedulingEffect::Recheck;
    }
    // Reaching the deadline ends the block and the weight penalty. Callers
    // still see the original observation.
    if let Some(reset_at) = observation.reset.resolves_at() {
        if now >= reset_at {
            return ScopeSchedulingEffect::Restored;
        }
    }
    if status.freshness_at(now) != QuotaFreshness::Fresh {
        return ScopeSchedulingEffect::Recheck;
    }
    if observation.availability == QuotaAvailability::Exhausted {
        if let Some(until) = observation.reset.resolves_at() {
            return ScopeSchedulingEffect::Block { until };
        }
    }
    ScopeSchedulingEffect::Weight(sampling_weight_for_remaining(observation.remaining_percent))
}

/// Freshness at `now`. Expired resets and future observation timestamps are
/// stale. Neither case rewrites availability or the remaining percentage.
pub fn observation_freshness(
    observation: &QuotaObservation,
    now: DateTime<Utc>,
    stale_after_seconds: u64,
) -> QuotaFreshness {
    if observation.observed_at > now {
        return QuotaFreshness::Stale;
    }
    if let Some(ttl) = u64_seconds_to_chrono(stale_after_seconds) {
        if now.signed_duration_since(observation.observed_at) >= ttl {
            return QuotaFreshness::Stale;
        }
    }
    if let Some(reset_at) = observation.reset.resolves_at() {
        if now >= reset_at {
            return QuotaFreshness::Stale;
        }
    }
    QuotaFreshness::Fresh
}

fn default_stale_after_seconds() -> u64 {
    DEFAULT_QUOTA_STALE_AFTER_SECONDS
}

fn parse_agent(agent: &str) -> Result<AgentCliKind, QuotaModelError> {
    AgentCliKind::from_str(agent)
        .map_err(|_| QuotaModelError::UnsupportedAgent(agent.trim().to_string()))
}

fn optional_token(value: Option<&str>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn required_token(value: &str, empty: QuotaModelError) -> Result<String, QuotaModelError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        Err(empty)
    } else {
        Ok(trimmed.to_string())
    }
}

fn normalize_error(error: String) -> String {
    let trimmed = error.trim();
    if trimmed.is_empty() {
        FAILED_REFRESH_ERROR.to_string()
    } else {
        trimmed.to_string()
    }
}

fn u64_seconds_to_chrono(seconds: u64) -> Option<chrono::Duration> {
    let seconds = i64::try_from(seconds).ok()?;
    chrono::Duration::try_seconds(seconds)
}

fn validate_observation(observation: &QuotaObservation) -> Result<(), QuotaModelError> {
    if observation.source.trim().is_empty() {
        return Err(QuotaModelError::EmptySource);
    }
    normalize_remaining_percent(observation.remaining_percent)?;
    match &observation.reset {
        QuotaReset::Unparsed { text } if text.trim().is_empty() => {
            Err(QuotaModelError::EmptyUnparsedReset)
        }
        QuotaReset::After { observed_at, .. } if *observed_at != observation.observed_at => {
            Err(QuotaModelError::ResetAnchorMismatch {
                anchor: *observed_at,
                observed_at: observation.observed_at,
            })
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ts(epoch: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(epoch, 0).expect("timestamp")
    }

    fn scope(
        agent: &str,
        account: Option<&str>,
        model: Option<&str>,
        pool: &str,
        quota_type: &str,
    ) -> QuotaScope {
        QuotaScope::try_from_parts(agent, account, model, pool, quota_type).expect("scope")
    }

    fn observe(
        scope: QuotaScope,
        availability: QuotaAvailability,
        percent: Option<f64>,
        reset: QuotaReset,
        observed_at: DateTime<Utc>,
    ) -> QuotaObservation {
        QuotaObservation::try_new(
            scope,
            availability,
            percent,
            reset,
            "cli_billing_log",
            observed_at,
        )
        .expect("observation")
    }

    fn book_with(observation: QuotaObservation) -> QuotaAgentStatus {
        let observed_at = observation.observed_at;
        QuotaAgentStatus::never_observed(observation.scope.agent)
            .apply_refresh(observed_at, QuotaCollection::Observed(vec![observation]))
    }

    #[test]
    fn relative_reset_stays_anchored_to_source_observation() {
        let observed_at = ts(1_700_000_000);
        let reset = QuotaReset::after(90, observed_at);
        let resolved = ts(1_700_000_090);
        assert_eq!(reset.resolves_at(), Some(resolved));
        assert_eq!(reset.resolves_at(), Some(resolved));

        let observation = observe(
            scope("grok", None, None, "credits", "weekly"),
            QuotaAvailability::Available,
            Some(40.0),
            reset,
            observed_at,
        );
        let attempt_at = ts(1_700_000_500);
        let failed = book_with(observation.clone()).apply_refresh(
            attempt_at,
            QuotaCollection::Failed {
                error: "timeout".to_string(),
            },
        );
        let kept = failed.statuses[0].observation.clone().expect("kept");
        assert_eq!(kept.observed_at, observed_at);
        assert_eq!(kept.reset.resolves_at(), Some(resolved));
        assert_ne!(failed.last_attempt_at, Some(observed_at));

        let mismatched = QuotaObservation {
            reset: QuotaReset::after(90, attempt_at),
            ..observation.clone()
        };
        let rejected = book_with(observation)
            .apply_refresh(attempt_at, QuotaCollection::Observed(vec![mismatched]));
        assert_eq!(rejected.refresh_outcome, QuotaRefreshOutcome::Failed);
        assert_eq!(
            rejected.statuses[0]
                .observation
                .as_ref()
                .expect("history")
                .observed_at,
            observed_at
        );
        assert!(rejected.last_error.unwrap().contains("anchored"));
    }

    #[test]
    fn absolute_unknown_and_unparsed_resets_round_trip() {
        let at = ts(1_700_000_100);
        let samples = [
            QuotaReset::at(at),
            QuotaReset::Unknown,
            QuotaReset::unparsed("next Tuesday").expect("text"),
            QuotaReset::after(90, ts(1_700_000_000)),
        ];
        assert_eq!(samples[0].resolves_at(), Some(at));
        assert_eq!(samples[1].resolves_at(), None);
        assert_eq!(samples[2].resolves_at(), None);
        assert!(QuotaReset::unparsed("  ").is_err());
        assert!(QuotaReset::after(u64::MAX, ts(1_700_000_000))
            .resolves_at()
            .is_none());

        let json = serde_json::to_value(&samples[3]).expect("json");
        assert_eq!(json["kind"], "after");
        assert_eq!(json["seconds"], 90);
        let anchored: DateTime<Utc> =
            serde_json::from_value(json["observedAt"].clone()).expect("time");
        assert_eq!(anchored, ts(1_700_000_000));
        let restored: QuotaReset = serde_json::from_value(json).expect("reset");
        assert_eq!(restored, samples[3]);

        let unknown = serde_json::to_value(QuotaReset::Unknown).expect("json");
        assert_eq!(unknown["kind"], "unknown");
        let unparsed = serde_json::to_value(&samples[2]).expect("json");
        assert_eq!(unparsed["kind"], "unparsed");
        assert_eq!(unparsed["text"], "next Tuesday");
        let absolute = serde_json::to_value(&samples[0]).expect("json");
        assert_eq!(absolute["kind"], "at");
    }

    #[test]
    fn freshness_expires_at_ttl_and_reset_boundary() {
        let observed_at = ts(1_000);
        let reset_at = ts(1_500);
        let observation = observe(
            scope("grok", None, None, "credits", "weekly"),
            QuotaAvailability::Exhausted,
            Some(0.0),
            QuotaReset::at(reset_at),
            observed_at,
        );
        assert_eq!(
            observation_freshness(&observation, ts(1_299), 300),
            QuotaFreshness::Fresh
        );
        assert_eq!(
            observation_freshness(&observation, ts(1_300), 300),
            QuotaFreshness::Stale
        );

        assert_eq!(
            observation_freshness(&observation, ts(1_499), 1_000),
            QuotaFreshness::Fresh
        );
        assert_eq!(
            observation_freshness(&observation, reset_at, 1_000),
            QuotaFreshness::Stale
        );

        let relative = observe(
            scope("grok", None, None, "credits", "weekly"),
            QuotaAvailability::Available,
            Some(80.0),
            QuotaReset::after(600, observed_at),
            observed_at,
        );
        assert_eq!(
            observation_freshness(&relative, ts(1_599), 1_000),
            QuotaFreshness::Fresh
        );
        assert_eq!(
            observation_freshness(&relative, ts(1_600), 1_000),
            QuotaFreshness::Stale
        );
        assert_eq!(relative.reset.resolves_at(), Some(ts(1_600)));

        let unknown_reset = observe(
            scope("grok", None, None, "credits", "weekly"),
            QuotaAvailability::Unknown,
            None,
            QuotaReset::Unknown,
            observed_at,
        );
        assert_eq!(
            observation_freshness(&unknown_reset, ts(1_299), 300),
            QuotaFreshness::Fresh
        );
        assert_eq!(
            observation_freshness(&unknown_reset, ts(1_300), 300),
            QuotaFreshness::Stale
        );
        assert_eq!(
            observation_freshness(&observation, observed_at, 0),
            QuotaFreshness::Stale
        );
        assert_eq!(
            observation_freshness(&unknown_reset, ts(2_000), u64::MAX),
            QuotaFreshness::Fresh
        );
    }

    #[test]
    fn future_observation_is_stale_without_rewriting_availability() {
        let observed_at = ts(2_000);
        let observation = observe(
            scope("grok", Some("Acc"), None, "credits", "weekly"),
            QuotaAvailability::Exhausted,
            Some(0.0),
            QuotaReset::at(ts(9_000)),
            observed_at,
        );
        let now = ts(1_500);
        assert_eq!(
            observation_freshness(&observation, now, 300),
            QuotaFreshness::Stale
        );
        let book = book_with(observation.clone());
        let view = &book.views_at(now)[0];
        assert_eq!(view.freshness, QuotaFreshness::Stale);
        let current = view.observation.clone().expect("observation");
        assert_eq!(current.availability, QuotaAvailability::Exhausted);
        assert_eq!(current.remaining_percent, Some(0.0));
        assert_eq!(current.observed_at, observed_at);
        assert_ne!(current.availability, QuotaAvailability::Available);
    }

    #[test]
    fn failed_refresh_retains_history_and_is_not_exhausted_or_refreshed() {
        let observed_at = ts(1_000);
        let observation = observe(
            scope(
                "grok-build",
                Some("Acc"),
                Some("grok-4.7"),
                "credits",
                "weekly",
            ),
            QuotaAvailability::Available,
            Some(40.0),
            QuotaReset::unparsed("next Tuesday").expect("text"),
            observed_at,
        );
        assert_eq!(observation.scope.agent, AgentCliKind::Grok);
        let previous = book_with(observation.clone());
        assert_eq!(previous.refresh_outcome, QuotaRefreshOutcome::Refreshed);

        let attempt_at = ts(1_100);
        let failed = previous.apply_refresh(
            attempt_at,
            QuotaCollection::Failed {
                error: "  provider timeout  ".to_string(),
            },
        );
        assert_eq!(failed.refresh_outcome, QuotaRefreshOutcome::Failed);
        assert_eq!(failed.last_error.as_deref(), Some("provider timeout"));
        assert_eq!(failed.last_attempt_at, Some(attempt_at));
        assert_eq!(failed.statuses.len(), 1);
        let status = &failed.statuses[0];
        assert_eq!(status.observation.as_ref(), Some(&observation));
        assert_eq!(
            status.observation.as_ref().unwrap().availability,
            QuotaAvailability::Available
        );
        assert_eq!(status.refresh_outcome, QuotaRefreshOutcome::Failed);
        assert!(!status.replaced_by_latest_attempt);
        assert!(!failed.replaced_by_latest_attempt(status));
        let view = &failed.views_at(ts(1_150))[0];
        assert_eq!(view.freshness, QuotaFreshness::Fresh);
        assert_eq!(
            view.observation.as_ref().unwrap().availability,
            QuotaAvailability::Available
        );

        let unsupported = previous.apply_refresh(attempt_at, QuotaCollection::Unsupported);
        assert_eq!(
            unsupported.refresh_outcome,
            QuotaRefreshOutcome::Unsupported
        );
        assert_eq!(
            unsupported.statuses[0].observation.as_ref(),
            Some(&observation)
        );
        assert!(!unsupported.statuses[0].replaced_by_latest_attempt);

        let unavailable = previous.apply_refresh(attempt_at, QuotaCollection::Unavailable);
        assert_eq!(
            unavailable.refresh_outcome,
            QuotaRefreshOutcome::Unavailable
        );
        assert_ne!(unavailable.refresh_outcome, QuotaRefreshOutcome::Refreshed);

        let blank = previous.apply_refresh(
            attempt_at,
            QuotaCollection::Failed {
                error: " ".to_string(),
            },
        );
        assert_eq!(blank.last_error.as_deref(), Some(FAILED_REFRESH_ERROR));

        let empty_agent = QuotaAgentStatus::try_from_agent_name("antigravity").expect("agy");
        assert_eq!(empty_agent.agent, AgentCliKind::Agy);
        assert_eq!(
            empty_agent.refresh_outcome,
            QuotaRefreshOutcome::NeverAttempted
        );
        assert!(empty_agent.views_at(attempt_at).is_empty());
        let still_empty = empty_agent.apply_refresh(
            attempt_at,
            QuotaCollection::Failed {
                error: "down".to_string(),
            },
        );
        assert!(still_empty.statuses.is_empty());
        assert_eq!(still_empty.refresh_outcome, QuotaRefreshOutcome::Failed);
    }

    #[test]
    fn unknown_and_empty_collection_do_not_fabricate_recovery() {
        let observed_at = ts(1_000);
        let exhausted = observe(
            scope("codex", None, None, "pool-a", "session"),
            QuotaAvailability::Exhausted,
            Some(0.0),
            QuotaReset::Unknown,
            observed_at,
        );
        let previous = book_with(exhausted.clone());
        let attempt_at = ts(1_050);
        let empty = previous.apply_refresh(attempt_at, QuotaCollection::Observed(Vec::new()));
        assert_eq!(empty.refresh_outcome, QuotaRefreshOutcome::Unavailable);
        assert_eq!(empty.statuses[0].observation.as_ref(), Some(&exhausted));
        assert!(!empty.replaced_by_latest_attempt(&empty.statuses[0]));
        assert_eq!(
            empty.statuses[0]
                .observation
                .as_ref()
                .unwrap()
                .remaining_percent,
            Some(0.0)
        );
        assert_eq!(
            empty.statuses[0].observation.as_ref().unwrap().availability,
            QuotaAvailability::Exhausted
        );

        let unknown = observe(
            scope("claude", Some("A"), None, "pool-a", "session"),
            QuotaAvailability::Unknown,
            None,
            QuotaReset::Unknown,
            observed_at,
        );
        let failed = book_with(unknown.clone()).apply_refresh(
            attempt_at,
            QuotaCollection::Failed {
                error: "no schema".to_string(),
            },
        );
        let kept = failed.statuses[0].observation.clone().expect("history");
        assert_eq!(kept.availability, QuotaAvailability::Unknown);
        assert_eq!(kept.remaining_percent, None);
        assert_eq!(kept.reset, QuotaReset::Unknown);
        let fresh = &failed.views_at(ts(1_100))[0];
        assert_eq!(fresh.freshness, QuotaFreshness::Fresh);
        assert_eq!(
            fresh.observation.as_ref().unwrap().availability,
            QuotaAvailability::Unknown
        );
    }

    #[test]
    fn cached_scope_omitted_from_collection_is_not_marked_refreshed() {
        let observed_at = ts(1_000);
        let credits = observe(
            scope("grok", Some("Acc"), Some("grok-4.7"), "credits", "weekly"),
            QuotaAvailability::Exhausted,
            Some(0.0),
            QuotaReset::at(ts(5_000)),
            observed_at,
        );
        let session = observe(
            scope("grok", Some("Acc"), Some("grok-4.7"), "session", "burst"),
            QuotaAvailability::Available,
            Some(70.0),
            QuotaReset::Unknown,
            observed_at,
        );
        let previous = QuotaAgentStatus::never_observed(AgentCliKind::Grok).apply_refresh(
            observed_at,
            QuotaCollection::Observed(vec![credits.clone(), session.clone()]),
        );
        let attempt_at = ts(1_200);
        let updated_session = observe(
            scope("grok", Some("Acc"), Some("grok-4.7"), "session", "burst"),
            QuotaAvailability::Available,
            Some(65.0),
            QuotaReset::Unknown,
            ts(1_180),
        );
        let next = previous.apply_refresh(
            attempt_at,
            QuotaCollection::Observed(vec![updated_session.clone()]),
        );
        assert_eq!(next.refresh_outcome, QuotaRefreshOutcome::Refreshed);
        let cached = next
            .statuses
            .iter()
            .find(|status| status.scope.pool_id == "credits")
            .expect("cached scope");
        assert_eq!(cached.observation.as_ref(), Some(&credits));
        assert!(!cached.replaced_by_latest_attempt);
        assert!(!next.replaced_by_latest_attempt(cached));
        assert_eq!(cached.last_attempt_at, Some(observed_at));
        assert_eq!(cached.refresh_outcome, QuotaRefreshOutcome::Refreshed);

        let refreshed = next
            .statuses
            .iter()
            .find(|status| status.scope.pool_id == "session")
            .expect("refreshed scope");
        assert_eq!(refreshed.observation.as_ref(), Some(&updated_session));
        assert!(next.replaced_by_latest_attempt(refreshed));
        assert_eq!(refreshed.last_attempt_at, Some(attempt_at));
        assert_ne!(
            refreshed.observation.as_ref().unwrap().observed_at,
            attempt_at
        );
    }

    #[test]
    fn remaining_percent_must_be_finite_and_within_0_to_100() {
        assert_eq!(parse_remaining_percent(0.0).unwrap(), 0.0);
        assert_eq!(parse_remaining_percent(100.0).unwrap(), 100.0);
        assert_eq!(parse_remaining_percent(12.5).unwrap(), 12.5);
        assert!(normalize_remaining_percent(None).unwrap().is_none());
        assert!(matches!(
            parse_remaining_percent(100.1),
            Err(QuotaModelError::RemainingPercentOutOfRange(_))
        ));
        assert!(matches!(
            parse_remaining_percent(-0.1),
            Err(QuotaModelError::RemainingPercentOutOfRange(_))
        ));
        assert_eq!(
            parse_remaining_percent(f64::NAN),
            Err(QuotaModelError::RemainingPercentNotFinite)
        );
        assert_eq!(
            parse_remaining_percent(f64::INFINITY),
            Err(QuotaModelError::RemainingPercentNotFinite)
        );
        assert_eq!(
            parse_remaining_percent(f64::NEG_INFINITY),
            Err(QuotaModelError::RemainingPercentNotFinite)
        );

        let observed_at = ts(1_000);
        let original = observe(
            scope("grok", None, None, "credits", "weekly"),
            QuotaAvailability::Available,
            Some(25.0),
            QuotaReset::Unknown,
            observed_at,
        );
        assert_eq!(original.availability, QuotaAvailability::Available);
        let zero = observe(
            original.scope.clone(),
            QuotaAvailability::Available,
            Some(0.0),
            QuotaReset::Unknown,
            observed_at,
        );
        assert_eq!(zero.availability, QuotaAvailability::Available);
        assert!(QuotaObservation::try_new(
            original.scope.clone(),
            QuotaAvailability::Exhausted,
            Some(101.0),
            QuotaReset::Unknown,
            "cli_billing_log",
            observed_at,
        )
        .is_err());
        assert!(QuotaObservation::try_new(
            original.scope.clone(),
            QuotaAvailability::Unknown,
            None,
            QuotaReset::Unknown,
            " ",
            observed_at,
        )
        .is_err());

        let mut invalid = original.clone();
        invalid.remaining_percent = Some(f64::NAN);
        let retained = book_with(original.clone())
            .apply_refresh(ts(1_100), QuotaCollection::Observed(vec![invalid]));
        assert_eq!(retained.refresh_outcome, QuotaRefreshOutcome::Failed);
        assert_eq!(retained.statuses[0].observation.as_ref(), Some(&original));
        assert!(!retained.statuses[0].replaced_by_latest_attempt);
        assert!(retained.last_error.unwrap().contains("finite"));

        let duplicate = book_with(original.clone()).apply_refresh(
            ts(1_100),
            QuotaCollection::Observed(vec![original.clone(), original.clone()]),
        );
        assert_eq!(duplicate.refresh_outcome, QuotaRefreshOutcome::Failed);
        assert_eq!(duplicate.statuses[0].observation.as_ref(), Some(&original));
    }

    #[test]
    fn scope_normalizes_agent_aliases_and_preserves_exact_ids() {
        let aliased = scope(
            " antigravity ",
            Some("  Acc  "),
            Some("ModelX"),
            " Pool ",
            "Credits",
        );
        let canonical = scope("agy", Some("Acc"), Some("ModelX"), "Pool", "Credits");
        assert_eq!(aliased, canonical);
        assert_eq!(aliased.identity_key(), canonical.identity_key());
        assert_eq!(
            aliased.identity_key(),
            r#"["agy","Acc","ModelX","Pool","Credits"]"#
        );
        assert_eq!(aliased.account_id.as_deref(), Some("Acc"));
        assert_eq!(aliased.model_id.as_deref(), Some("ModelX"));

        let shared = scope("agy", Some(" "), Some(""), "Pool", "Credits");
        assert_eq!(shared.account_id, None);
        assert_eq!(shared.model_id, None);
        assert_eq!(
            shared.identity_key(),
            r#"["agy",null,null,"Pool","Credits"]"#
        );
        let exact_star = scope("agy", None, Some("*"), "Pool", "Credits");
        assert_eq!(exact_star.model_id.as_deref(), Some("*"));
        assert_ne!(exact_star.identity_key(), shared.identity_key());

        let other_case = scope("agy", Some("acc"), Some("modelx"), "pool", "credits");
        assert_ne!(other_case.identity_key(), canonical.identity_key());
        let other_pool = scope("agy", Some("Acc"), Some("ModelX"), "Pool-2", "Credits");
        assert_ne!(other_pool.identity_key(), canonical.identity_key());

        assert!(matches!(
            QuotaScope::try_from_parts("gpt", None, None, "Pool", "Credits"),
            Err(QuotaModelError::UnsupportedAgent(_))
        ));
        assert!(matches!(
            QuotaScope::try_from_parts("grok", None, None, " ", "Credits"),
            Err(QuotaModelError::EmptyPoolId)
        ));
        assert!(matches!(
            QuotaAgentStatus::try_from_agent_name("generic"),
            Ok(status) if status.agent == AgentCliKind::Custom
        ));

        let mismatch = observe(
            scope("claude", None, None, "Pool", "Credits"),
            QuotaAvailability::Available,
            Some(10.0),
            QuotaReset::Unknown,
            ts(1_000),
        );
        let rejected = QuotaAgentStatus::never_observed(AgentCliKind::Grok)
            .apply_refresh(ts(1_100), QuotaCollection::Observed(vec![mismatch]));
        assert!(rejected.statuses.is_empty());
        assert_eq!(rejected.refresh_outcome, QuotaRefreshOutcome::Failed);
        assert!(rejected.last_error.unwrap().contains("claude"));
    }

    fn with_ttl(mut book: QuotaAgentStatus, seconds: u64) -> QuotaAgentStatus {
        book.stale_after_seconds = seconds;
        for status in &mut book.statuses {
            status.stale_after_seconds = seconds;
        }
        book
    }

    #[test]
    fn blocks_only_current_exhausted_scope_with_future_known_reset() {
        let observed_at = ts(1_000);
        let reset_at = ts(5_000);
        let now = ts(1_200);
        let exhausted = observe(
            scope("grok", None, None, "credits", "weekly"),
            QuotaAvailability::Exhausted,
            Some(0.0),
            QuotaReset::at(reset_at),
            observed_at,
        );
        let book = with_ttl(book_with(exhausted), 10_000);
        assert_eq!(
            book.admission_at(now),
            QuotaAdmission::Blocked { until: reset_at }
        );

        let relative = observe(
            scope("codex", None, None, "credits", "weekly"),
            QuotaAvailability::Exhausted,
            Some(0.0),
            QuotaReset::after(600, observed_at),
            observed_at,
        );
        assert_eq!(
            with_ttl(book_with(relative), 10_000).admission_at(ts(1_599)),
            QuotaAdmission::Blocked { until: ts(1_600) }
        );

        let available_zero = observe(
            scope("agy", None, None, "credits", "weekly"),
            QuotaAvailability::Available,
            Some(0.0),
            QuotaReset::at(reset_at),
            observed_at,
        );
        assert_eq!(
            book_with(available_zero).admission_at(now),
            QuotaAdmission::Eligible {
                weight: MIN_POSITIVE_SAMPLING_WEIGHT,
            }
        );

        let unknown = observe(
            scope("claude", None, None, "credits", "weekly"),
            QuotaAvailability::Unknown,
            None,
            QuotaReset::at(reset_at),
            observed_at,
        );
        assert_eq!(
            book_with(unknown).admission_at(now),
            QuotaAdmission::Eligible {
                weight: NORMAL_SAMPLING_WEIGHT,
            }
        );

        let session = observe(
            scope("grok", None, None, "session", "burst"),
            QuotaAvailability::Exhausted,
            Some(0.0),
            QuotaReset::at(ts(9_000)),
            observed_at,
        );
        let healthy = observe(
            scope("grok", None, None, "credits", "weekly"),
            QuotaAvailability::Available,
            Some(90.0),
            QuotaReset::Unknown,
            observed_at,
        );
        let mixed = with_ttl(
            QuotaAgentStatus::never_observed(AgentCliKind::Grok).apply_refresh(
                observed_at,
                QuotaCollection::Observed(vec![session, healthy]),
            ),
            10_000,
        );
        assert_eq!(
            mixed.admission_at(now),
            QuotaAdmission::Blocked { until: ts(9_000) }
        );

        let early = observe(
            scope("grok", None, None, "credits", "weekly"),
            QuotaAvailability::Exhausted,
            Some(0.0),
            QuotaReset::at(ts(4_000)),
            observed_at,
        );
        let later = observe(
            scope("grok", None, None, "session", "burst"),
            QuotaAvailability::Exhausted,
            Some(0.0),
            QuotaReset::at(ts(9_000)),
            observed_at,
        );
        let both = with_ttl(
            QuotaAgentStatus::never_observed(AgentCliKind::Grok)
                .apply_refresh(observed_at, QuotaCollection::Observed(vec![early, later])),
            10_000,
        );
        assert_eq!(
            both.admission_at(now),
            QuotaAdmission::Blocked { until: ts(9_000) }
        );

        let empty = QuotaAgentStatus::never_observed(AgentCliKind::Grok).apply_refresh(
            now,
            QuotaCollection::Failed {
                error: "timeout".to_string(),
            },
        );
        assert!(empty.statuses.is_empty());
        assert_eq!(
            empty.admission_at(now),
            QuotaAdmission::Eligible {
                weight: NORMAL_SAMPLING_WEIGHT,
            }
        );
        assert_eq!(
            QuotaAgentStatus::never_observed(AgentCliKind::Agy).admission_at(now),
            QuotaAdmission::Eligible {
                weight: NORMAL_SAMPLING_WEIGHT,
            }
        );

        let retained = book.apply_refresh(
            ts(1_100),
            QuotaCollection::Failed {
                error: "timeout".to_string(),
            },
        );
        assert_eq!(retained.refresh_outcome, QuotaRefreshOutcome::Failed);
        assert_eq!(
            retained.admission_at(now),
            QuotaAdmission::Blocked { until: reset_at }
        );
        assert_eq!(
            retained.statuses[0]
                .observation
                .as_ref()
                .expect("retained")
                .availability,
            QuotaAvailability::Exhausted
        );
    }

    #[test]
    fn unknown_reset_does_not_block_indefinitely() {
        let observed_at = ts(1_000);
        let now = ts(1_050);
        let far_future = ts(1_000_000_000);
        let unknown_reset = observe(
            scope("grok", None, None, "credits", "weekly"),
            QuotaAvailability::Exhausted,
            Some(0.0),
            QuotaReset::Unknown,
            observed_at,
        );
        let fresh = with_ttl(book_with(unknown_reset), u64::MAX);
        assert_eq!(
            fresh.admission_at(now),
            QuotaAdmission::Eligible {
                weight: MIN_POSITIVE_SAMPLING_WEIGHT,
            }
        );
        assert_eq!(
            fresh.admission_at(far_future),
            QuotaAdmission::Eligible {
                weight: MIN_POSITIVE_SAMPLING_WEIGHT,
            }
        );

        let unparsed = observe(
            scope("grok", None, None, "credits", "weekly"),
            QuotaAvailability::Exhausted,
            Some(0.0),
            QuotaReset::unparsed("next Tuesday").expect("text"),
            observed_at,
        );
        let unparsed_book = with_ttl(book_with(unparsed), u64::MAX);
        assert_eq!(
            unparsed_book.admission_at(far_future),
            QuotaAdmission::Eligible {
                weight: MIN_POSITIVE_SAMPLING_WEIGHT,
            }
        );

        let exhausted_without_percent = observe(
            scope("claude", None, None, "credits", "weekly"),
            QuotaAvailability::Exhausted,
            None,
            QuotaReset::Unknown,
            observed_at,
        );
        assert_eq!(
            with_ttl(book_with(exhausted_without_percent), u64::MAX).admission_at(far_future),
            QuotaAdmission::Eligible {
                weight: NORMAL_SAMPLING_WEIGHT,
            }
        );
    }

    #[test]
    fn stale_quota_is_rechecked_instead_of_blocking() {
        let observed_at = ts(1_000);
        let reset_at = ts(10_000);
        let exhausted = observe(
            scope("grok", None, None, "credits", "weekly"),
            QuotaAvailability::Exhausted,
            Some(0.0),
            QuotaReset::at(reset_at),
            observed_at,
        );
        let book = book_with(exhausted.clone());
        assert_eq!(
            book.admission_at(ts(1_299)),
            QuotaAdmission::Blocked { until: reset_at }
        );
        let before = book.clone();
        assert_eq!(book.admission_at(ts(1_300)), QuotaAdmission::Recheck);
        assert_eq!(book, before);
        assert_eq!(book.statuses[0].observation.as_ref(), Some(&exhausted));

        let future_observation = observe(
            scope("grok", None, None, "credits", "weekly"),
            QuotaAvailability::Exhausted,
            Some(0.0),
            QuotaReset::at(ts(9_000)),
            ts(2_000),
        );
        assert_eq!(
            book_with(future_observation).admission_at(ts(1_500)),
            QuotaAdmission::Recheck
        );

        let healthy = observe(
            scope("grok", None, None, "session", "burst"),
            QuotaAvailability::Available,
            Some(80.0),
            QuotaReset::Unknown,
            ts(1_250),
        );
        let mixed = QuotaAgentStatus::never_observed(AgentCliKind::Grok).apply_refresh(
            ts(1_250),
            QuotaCollection::Observed(vec![exhausted, healthy]),
        );
        assert_eq!(mixed.admission_at(ts(1_300)), QuotaAdmission::Recheck);
    }

    #[test]
    fn low_remaining_percent_reduces_weight_deterministically() {
        assert_eq!(sampling_weight_for_remaining(None), NORMAL_SAMPLING_WEIGHT);
        assert_eq!(
            sampling_weight_for_remaining(Some(100.0)),
            NORMAL_SAMPLING_WEIGHT
        );
        assert_eq!(
            sampling_weight_for_remaining(Some(LOW_REMAINING_PERCENT)),
            NORMAL_SAMPLING_WEIGHT
        );
        assert_eq!(sampling_weight_for_remaining(Some(10.0)), 525);
        assert_eq!(
            sampling_weight_for_remaining(Some(0.0)),
            MIN_POSITIVE_SAMPLING_WEIGHT
        );
        assert!(MIN_POSITIVE_SAMPLING_WEIGHT > 0);
        assert!(sampling_weight_for_remaining(Some(19.999)) < NORMAL_SAMPLING_WEIGHT);
        assert!(
            sampling_weight_for_remaining(Some(0.0)) < sampling_weight_for_remaining(Some(10.0))
        );
        assert!(sampling_weight_for_remaining(Some(10.0)) < NORMAL_SAMPLING_WEIGHT);
        assert_eq!(
            sampling_weight_for_remaining(Some(10.0)),
            sampling_weight_for_remaining(Some(10.0))
        );
        assert_eq!(
            sampling_weight_for_remaining(Some(f64::NAN)),
            NORMAL_SAMPLING_WEIGHT
        );

        let observed_at = ts(1_000);
        let now = ts(1_100);
        let low = observe(
            scope("grok", None, None, "credits", "weekly"),
            QuotaAvailability::Available,
            Some(10.0),
            QuotaReset::Unknown,
            observed_at,
        );
        assert_eq!(
            book_with(low.clone()).admission_at(now),
            QuotaAdmission::Eligible { weight: 525 }
        );

        let lower = observe(
            scope("grok", None, None, "session", "burst"),
            QuotaAvailability::Available,
            Some(0.0),
            QuotaReset::Unknown,
            observed_at,
        );
        let book = QuotaAgentStatus::never_observed(AgentCliKind::Grok)
            .apply_refresh(observed_at, QuotaCollection::Observed(vec![low, lower]));
        assert_eq!(
            book.admission_at(now),
            QuotaAdmission::Eligible {
                weight: MIN_POSITIVE_SAMPLING_WEIGHT,
            }
        );

        let edge = observe(
            scope("agy", None, None, "credits", "weekly"),
            QuotaAvailability::Available,
            Some(20.0),
            QuotaReset::Unknown,
            observed_at,
        );
        assert_eq!(
            book_with(edge).admission_at(now),
            QuotaAdmission::Eligible {
                weight: NORMAL_SAMPLING_WEIGHT,
            }
        );
    }

    #[test]
    fn reset_deadline_restores_eligibility_without_mutating_observation() {
        let observed_at = ts(1_000);
        let reset_at = ts(1_500);
        let exhausted = observe(
            scope("grok", Some("Acc"), Some("grok-4.7"), "credits", "weekly"),
            QuotaAvailability::Exhausted,
            Some(0.0),
            QuotaReset::at(reset_at),
            observed_at,
        );
        let book = with_ttl(book_with(exhausted.clone()), 10_000);
        let before = book.clone();
        assert_eq!(
            book.admission_at(ts(1_499)),
            QuotaAdmission::Blocked { until: reset_at }
        );
        assert_eq!(
            book.admission_at(reset_at),
            QuotaAdmission::Eligible {
                weight: NORMAL_SAMPLING_WEIGHT,
            }
        );
        assert_eq!(book, before);
        let current = book.statuses[0].observation.clone().expect("observation");
        assert_eq!(current, exhausted);
        assert_eq!(current.availability, QuotaAvailability::Exhausted);
        assert_eq!(current.remaining_percent, Some(0.0));
        assert_eq!(current.reset, QuotaReset::at(reset_at));
        assert_eq!(current.observed_at, observed_at);

        let also_stale = book_with(exhausted.clone());
        assert_eq!(
            also_stale.admission_at(reset_at),
            QuotaAdmission::Eligible {
                weight: NORMAL_SAMPLING_WEIGHT,
            }
        );
        assert_eq!(
            also_stale.statuses[0].observation.as_ref(),
            Some(&exhausted)
        );

        let relative = observe(
            scope("grok", None, None, "credits", "weekly"),
            QuotaAvailability::Exhausted,
            Some(0.0),
            QuotaReset::after(600, observed_at),
            observed_at,
        );
        let relative_book = with_ttl(book_with(relative.clone()), 10_000);
        let relative_before = relative_book.clone();
        assert_eq!(
            relative_book.admission_at(ts(1_599)),
            QuotaAdmission::Blocked { until: ts(1_600) }
        );
        assert_eq!(
            relative_book.admission_at(ts(1_600)),
            QuotaAdmission::Eligible {
                weight: NORMAL_SAMPLING_WEIGHT,
            }
        );
        assert_eq!(relative_book, relative_before);
        assert_eq!(
            relative_book.statuses[0].observation.as_ref(),
            Some(&relative)
        );

        let low = observe(
            scope("grok", None, None, "session", "burst"),
            QuotaAvailability::Available,
            Some(10.0),
            QuotaReset::Unknown,
            ts(1_450),
        );
        let mixed = with_ttl(
            QuotaAgentStatus::never_observed(AgentCliKind::Grok)
                .apply_refresh(ts(1_450), QuotaCollection::Observed(vec![exhausted, low])),
            10_000,
        );
        let mixed_before = mixed.clone();
        assert_eq!(
            mixed.admission_at(reset_at),
            QuotaAdmission::Eligible { weight: 525 }
        );
        assert_eq!(mixed, mixed_before);
        let credits = mixed
            .statuses
            .iter()
            .find(|status| status.scope.pool_id == "credits")
            .and_then(|status| status.observation.as_ref())
            .expect("credits observation");
        assert_eq!(credits.availability, QuotaAvailability::Exhausted);
        assert_eq!(credits.remaining_percent, Some(0.0));
        assert_eq!(credits.reset, QuotaReset::at(reset_at));
    }
}
