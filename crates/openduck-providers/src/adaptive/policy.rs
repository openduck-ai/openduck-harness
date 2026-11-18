use chrono::{DateTime, Duration as ChronoDuration, FixedOffset, Timelike, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RoutingMode {
    FixedRegular,
    FixedEconomy,
    #[default]
    Auto,
}

impl RoutingMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            RoutingMode::FixedRegular => "fixed_regular",
            RoutingMode::FixedEconomy => "fixed_economy",
            RoutingMode::Auto => "auto",
        }
    }
}

impl FromStr for RoutingMode {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "auto" | "adaptive" => Ok(RoutingMode::Auto),
            "regular" | "fixed_regular" | "fixed-regular" => Ok(RoutingMode::FixedRegular),
            "economy" | "fixed_economy" | "fixed-economy" => Ok(RoutingMode::FixedEconomy),
            other => Err(anyhow::anyhow!(
                "Invalid routing mode '{other}': expected auto, regular, or economy"
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CandidateKind {
    Regular,
    Economy,
}

impl CandidateKind {
    pub fn other(self) -> Self {
        match self {
            CandidateKind::Regular => CandidateKind::Economy,
            CandidateKind::Economy => CandidateKind::Regular,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            CandidateKind::Regular => "regular",
            CandidateKind::Economy => "economy",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestPurpose {
    MainLoop,
    Auxiliary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeWindow {
    pub start_minute: u16,
    pub end_minute: u16,
}

impl TimeWindow {
    pub fn new(start_minute: u16, end_minute: u16) -> anyhow::Result<Self> {
        if start_minute >= 24 * 60 || end_minute >= 24 * 60 {
            anyhow::bail!("TimeWindow minutes must be < 1440");
        }
        Ok(Self {
            start_minute,
            end_minute,
        })
    }

    pub fn contains_minute(&self, minute_of_day: u16) -> bool {
        if self.start_minute == self.end_minute {
            return true;
        }
        if self.start_minute < self.end_minute {
            minute_of_day >= self.start_minute && minute_of_day < self.end_minute
        } else {
            minute_of_day >= self.start_minute || minute_of_day < self.end_minute
        }
    }

    pub fn parse(spec: &str) -> anyhow::Result<Self> {
        let (start_str, end_str) = spec
            .trim()
            .split_once('-')
            .ok_or_else(|| anyhow::anyhow!("Invalid time window '{spec}', expected HH:MM-HH:MM"))?;
        let start_minute = parse_hhmm(start_str)?;
        let end_minute = parse_hhmm(end_str)?;
        Self::new(start_minute, end_minute)
    }
}

fn parse_hhmm(s: &str) -> anyhow::Result<u16> {
    let (hh, mm) = s
        .trim()
        .split_once(':')
        .ok_or_else(|| anyhow::anyhow!("Invalid time '{s}', expected HH:MM"))?;
    let h: u16 = hh.parse()?;
    let m: u16 = mm.parse()?;
    if h >= 24 || m >= 60 {
        anyhow::bail!("Time out of range: '{s}'");
    }
    Ok(h * 60 + m)
}

pub fn parse_timezone_offset(tz: &str) -> anyhow::Result<FixedOffset> {
    let trimmed = tz.trim();
    match trimmed {
        "Asia/Shanghai" | "Asia/Chongqing" | "Asia/Hong_Kong" | "Asia/Taipei" | "CST" => {
            FixedOffset::east_opt(8 * 3600).ok_or_else(|| anyhow::anyhow!("Invalid offset"))
        }
        "UTC" | "Z" | "Etc/UTC" | "GMT" => {
            FixedOffset::east_opt(0).ok_or_else(|| anyhow::anyhow!("Invalid offset"))
        }
        "Asia/Tokyo" | "JST" => {
            FixedOffset::east_opt(9 * 3600).ok_or_else(|| anyhow::anyhow!("Invalid offset"))
        }
        "America/New_York" | "EST" => {
            FixedOffset::west_opt(5 * 3600).ok_or_else(|| anyhow::anyhow!("Invalid offset"))
        }
        "America/Los_Angeles" | "PST" => {
            FixedOffset::west_opt(8 * 3600).ok_or_else(|| anyhow::anyhow!("Invalid offset"))
        }
        _ => {
            let (sign, rest) = if let Some(r) = trimmed.strip_prefix('+') {
                (1i32, r)
            } else if let Some(r) = trimmed.strip_prefix('-') {
                (-1i32, r)
            } else {
                anyhow::bail!(
                    "Unsupported timezone '{trimmed}'. Use Asia/Shanghai, UTC, or +HH:MM/-HH:MM"
                );
            };
            let (hh, mm) = rest
                .split_once(':')
                .ok_or_else(|| anyhow::anyhow!("Invalid offset '{trimmed}'"))?;
            let h: i32 = hh.parse()?;
            let m: i32 = mm.parse()?;
            if h > 23 || m > 59 {
                anyhow::bail!("Timezone offset out of range: '{trimmed}'");
            }
            let secs = sign * (h * 3600 + m * 60);
            FixedOffset::east_opt(secs)
                .ok_or_else(|| anyhow::anyhow!("Invalid timezone offset '{trimmed}'"))
        }
    }
}

pub trait Clock: Send + Sync + std::fmt::Debug {
    fn now(&self) -> DateTime<Utc>;
}

#[derive(Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

#[derive(Debug, Clone)]
pub struct AdaptivePolicyConfig {
    pub timezone: String,
    pub off_peak: Vec<TimeWindow>,
    pub probe_ratio: f64,
    pub probe_jitter: f64,
    pub stats_window: Duration,
    pub max_samples_per_model: usize,
    pub min_samples_per_side: usize,
    pub confirm_count: usize,
    pub cooldown: Duration,
    pub economy_severe_timeout_pause: Duration,
    pub severe_timeout_threshold: Duration,
    pub switch_to_economy_ratio: f64,
    pub switch_to_economy_abs: Duration,
    pub switch_to_regular_ratio: f64,
    pub switch_to_regular_abs: Duration,
    pub input_token_buckets: Vec<usize>,
    pub ema_alpha: f64,
    pub min_output_tps_ratio: f64,
    pub max_total_duration_ratio: f64,
    pub max_in_flight_probes: usize,
}

impl Default for AdaptivePolicyConfig {
    fn default() -> Self {
        Self {
            timezone: "Asia/Shanghai".to_string(),
            off_peak: vec![TimeWindow::new(60, 7 * 60).expect("valid default off-peak window")],
            probe_ratio: 0.10,
            probe_jitter: 0.03,
            stats_window: Duration::from_secs(15 * 60),
            max_samples_per_model: 30,
            min_samples_per_side: 3,
            confirm_count: 2,
            cooldown: Duration::from_secs(300),
            economy_severe_timeout_pause: Duration::from_secs(300),
            severe_timeout_threshold: Duration::from_secs(30),
            switch_to_economy_ratio: 1.25,
            switch_to_economy_abs: Duration::from_secs(2),
            switch_to_regular_ratio: 1.60,
            switch_to_regular_abs: Duration::from_secs(3),
            input_token_buckets: vec![2_000, 8_000, 32_000],
            ema_alpha: 0.3,
            min_output_tps_ratio: 0.4,
            max_total_duration_ratio: 2.5,
            max_in_flight_probes: 1,
        }
    }
}

impl AdaptivePolicyConfig {
    pub fn validate(&self) -> anyhow::Result<()> {
        parse_timezone_offset(&self.timezone)?;
        if !(0.0..=1.0).contains(&self.probe_ratio) {
            anyhow::bail!("probe_ratio must be in [0.0, 1.0]");
        }
        if !(0.0..=1.0).contains(&self.ema_alpha) || self.ema_alpha == 0.0 {
            anyhow::bail!("ema_alpha must be in (0.0, 1.0]");
        }
        if self.min_samples_per_side == 0 {
            anyhow::bail!("min_samples_per_side must be >= 1");
        }
        if self.confirm_count == 0 {
            anyhow::bail!("confirm_count must be >= 1");
        }
        if self.max_samples_per_model < self.min_samples_per_side {
            anyhow::bail!("max_samples_per_model must be >= min_samples_per_side");
        }
        Ok(())
    }

    pub fn timezone_offset(&self) -> FixedOffset {
        parse_timezone_offset(&self.timezone)
            .unwrap_or_else(|_| FixedOffset::east_opt(8 * 3600).unwrap())
    }

    pub fn is_off_peak(&self, now: DateTime<Utc>) -> bool {
        let local = now.with_timezone(&self.timezone_offset());
        let minute = (local.hour() * 60 + local.minute()) as u16;
        self.off_peak.iter().any(|w| w.contains_minute(minute))
    }

    pub fn bucket_for(&self, input_tokens: usize) -> usize {
        self.input_token_buckets
            .iter()
            .position(|&bound| input_tokens < bound)
            .unwrap_or(self.input_token_buckets.len())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestOutcome {
    Success,
    Timeout,
    AvailabilityError,
    NonSwitchableError,
    Cancelled,
}

#[derive(Debug, Clone)]
pub struct RequestObservation {
    pub candidate: CandidateKind,
    pub actual_model: String,
    pub is_probe: bool,
    pub purpose: RequestPurpose,
    pub config_version: u64,
    pub input_tokens: usize,
    pub output_tokens: usize,
    pub cache_read_tokens: usize,
    pub cache_creation_tokens: usize,
    pub ttft: Option<Duration>,
    pub total_duration: Duration,
    pub attempts: u32,
    pub outcome: RequestOutcome,
}

#[derive(Debug, Clone)]
pub struct LatencySample {
    pub timestamp: DateTime<Utc>,
    pub bucket: usize,
    pub ttft_ms: f64,
    pub total_ms: f64,
    pub output_tps: Option<f64>,
}

#[derive(Debug, Clone, Default)]
pub struct BucketEma {
    pub count: usize,
    pub ttft_ms: f64,
    pub total_ms: f64,
    pub output_tps: Option<f64>,
}

#[derive(Debug, Clone, Default)]
pub struct CandidateStats {
    samples: VecDeque<LatencySample>,
    failures: VecDeque<DateTime<Utc>>,
    consecutive_failures: usize,
}

impl CandidateStats {
    fn prune(&mut self, now: DateTime<Utc>, window: Duration) {
        let cutoff =
            now - ChronoDuration::from_std(window).unwrap_or_else(|_| ChronoDuration::zero());
        while self.samples.front().is_some_and(|s| s.timestamp < cutoff) {
            self.samples.pop_front();
        }
        while self.failures.front().is_some_and(|&t| t < cutoff) {
            self.failures.pop_front();
        }
    }

    fn record_success(
        &mut self,
        now: DateTime<Utc>,
        bucket: usize,
        ttft: Duration,
        total: Duration,
        output_tokens: usize,
        max_samples: usize,
    ) {
        self.consecutive_failures = 0;
        let output_tps = if output_tokens >= 8 && total.as_secs_f64() > 0.0 {
            Some(output_tokens as f64 / total.as_secs_f64())
        } else {
            None
        };
        if self.samples.len() >= max_samples {
            self.samples.pop_front();
        }
        self.samples.push_back(LatencySample {
            timestamp: now,
            bucket,
            ttft_ms: ttft.as_secs_f64() * 1000.0,
            total_ms: total.as_secs_f64() * 1000.0,
            output_tps,
        });
    }

    fn record_failure(&mut self, now: DateTime<Utc>) {
        self.consecutive_failures += 1;
        self.failures.push_back(now);
    }

    fn bucket_emas(&self, alpha: f64) -> HashMap<usize, BucketEma> {
        let mut map: HashMap<usize, BucketEma> = HashMap::new();
        for s in &self.samples {
            let entry = map.entry(s.bucket).or_default();
            if entry.count == 0 {
                entry.ttft_ms = s.ttft_ms;
                entry.total_ms = s.total_ms;
                entry.output_tps = s.output_tps;
            } else {
                entry.ttft_ms = alpha * s.ttft_ms + (1.0 - alpha) * entry.ttft_ms;
                entry.total_ms = alpha * s.total_ms + (1.0 - alpha) * entry.total_ms;
                if let Some(tps) = s.output_tps {
                    entry.output_tps = Some(match entry.output_tps {
                        Some(prev) => alpha * tps + (1.0 - alpha) * prev,
                        None => tps,
                    });
                }
            }
            entry.count += 1;
        }
        map
    }
}

#[derive(Debug, Clone)]
pub struct RoutingDecision {
    pub selected: CandidateKind,
    pub is_probe: bool,
    pub primary: CandidateKind,
    pub mode: RoutingMode,
    pub reason: String,
    pub config_version: u64,
}

#[derive(Debug, Clone)]
pub struct AdaptivePolicyState {
    config: AdaptivePolicyConfig,
    primary: CandidateKind,
    last_off_peak: Option<bool>,
    last_switch_reason: String,
    cooldown_until: Option<DateTime<Utc>>,
    economy_paused_until: Option<DateTime<Utc>>,
    recovery_probe_sent: bool,
    in_flight_probes: usize,
    to_economy_confirmations: usize,
    to_regular_confirmations: usize,
    regular_stats: CandidateStats,
    economy_stats: CandidateStats,
    last_actual: Option<CandidateKind>,
    last_actual_model: Option<String>,
    last_is_probe: bool,
    clock: Arc<dyn Clock>,
}

impl AdaptivePolicyState {
    pub fn new(config: AdaptivePolicyConfig, clock: Arc<dyn Clock>) -> Self {
        let now = clock.now();
        let off_peak = config.is_off_peak(now);
        let initial_primary = if off_peak {
            CandidateKind::Economy
        } else {
            CandidateKind::Regular
        };
        let reason = if off_peak {
            "initial off-peak default".to_string()
        } else {
            "initial peak default".to_string()
        };
        Self {
            config,
            primary: initial_primary,
            last_off_peak: Some(off_peak),
            last_switch_reason: reason,
            cooldown_until: None,
            economy_paused_until: None,
            recovery_probe_sent: false,
            in_flight_probes: 0,
            to_economy_confirmations: 0,
            to_regular_confirmations: 0,
            regular_stats: CandidateStats::default(),
            economy_stats: CandidateStats::default(),
            last_actual: None,
            last_actual_model: None,
            last_is_probe: false,
            clock,
        }
    }

    pub fn config(&self) -> &AdaptivePolicyConfig {
        &self.config
    }

    pub fn primary(&self) -> CandidateKind {
        self.primary
    }

    pub fn in_flight_probes(&self) -> usize {
        self.in_flight_probes
    }

    pub fn last_switch_reason(&self) -> &str {
        &self.last_switch_reason
    }

    pub fn last_actual(&self) -> Option<CandidateKind> {
        self.last_actual
    }

    pub fn last_actual_model(&self) -> Option<&str> {
        self.last_actual_model.as_deref()
    }

    pub fn last_is_probe(&self) -> bool {
        self.last_is_probe
    }

    pub fn economy_paused_until(&self) -> Option<DateTime<Utc>> {
        self.economy_paused_until
    }

    pub fn cooldown_until(&self) -> Option<DateTime<Utc>> {
        self.cooldown_until
    }

    pub fn reset_for_mode_change(&mut self, mode: RoutingMode) {
        self.to_economy_confirmations = 0;
        self.to_regular_confirmations = 0;
        match mode {
            RoutingMode::FixedRegular => {
                self.primary = CandidateKind::Regular;
                self.last_switch_reason = "user pinned regular".to_string();
            }
            RoutingMode::FixedEconomy => {
                self.primary = CandidateKind::Economy;
                self.last_switch_reason = "user pinned economy".to_string();
            }
            RoutingMode::Auto => {
                self.last_switch_reason = "user enabled auto".to_string();
            }
        }
    }

    fn in_cooldown(&self, now: DateTime<Utc>) -> bool {
        self.cooldown_until.is_some_and(|until| now < until)
    }

    fn is_economy_paused(&self, now: DateTime<Utc>) -> bool {
        self.economy_paused_until.is_some_and(|until| now < until)
    }

    fn is_economy_recovery_probe_due(&self, now: DateTime<Utc>) -> bool {
        matches!(self.economy_paused_until, Some(until) if now >= until)
            && !self.recovery_probe_sent
    }

    fn prune_all(&mut self, now: DateTime<Utc>) {
        self.regular_stats.prune(now, self.config.stats_window);
        self.economy_stats.prune(now, self.config.stats_window);
    }

    /// Evaluates whether comparable fresh samples favor switching to Economy or Regular.
    /// Returns `Some(true)` if Economy meets budget, `Some(false)` if Economy exceeds budget,
    /// or `None` if there are insufficient comparable samples.
    fn evaluate_economy_vs_regular(&self, now: DateTime<Utc>) -> Option<bool> {
        let mut regular = self.regular_stats.clone();
        let mut economy = self.economy_stats.clone();
        regular.prune(now, self.config.stats_window);
        economy.prune(now, self.config.stats_window);

        let reg_buckets = regular.bucket_emas(self.config.ema_alpha);
        let eco_buckets = economy.bucket_emas(self.config.ema_alpha);

        let mut comparable_count = 0usize;
        let mut all_within_economy_budget = true;
        let mut any_exceeds_regular_threshold = false;

        for (bucket, reg_ema) in &reg_buckets {
            let Some(eco_ema) = eco_buckets.get(bucket) else {
                continue;
            };
            if reg_ema.count < self.config.min_samples_per_side
                || eco_ema.count < self.config.min_samples_per_side
            {
                continue;
            }
            comparable_count += 1;

            let diff_ms = eco_ema.ttft_ms - reg_ema.ttft_ms;
            let ratio = if reg_ema.ttft_ms > 0.0 {
                eco_ema.ttft_ms / reg_ema.ttft_ms
            } else {
                1.0
            };

            let within_ttft_budget = ratio <= self.config.switch_to_economy_ratio
                && diff_ms <= self.config.switch_to_economy_abs.as_secs_f64() * 1000.0;

            let exceeds_ttft_budget = ratio > self.config.switch_to_regular_ratio
                && diff_ms > self.config.switch_to_regular_abs.as_secs_f64() * 1000.0;

            let output_tps_ok = match (reg_ema.output_tps, eco_ema.output_tps) {
                (Some(r_tps), Some(e_tps)) if r_tps > 0.0 => {
                    e_tps >= r_tps * self.config.min_output_tps_ratio
                }
                _ => true,
            };

            let total_ok = if reg_ema.total_ms > 0.0 {
                eco_ema.total_ms <= reg_ema.total_ms * self.config.max_total_duration_ratio
            } else {
                true
            };

            if !within_ttft_budget || !output_tps_ok || !total_ok {
                all_within_economy_budget = false;
            }
            if exceeds_ttft_budget || !output_tps_ok || !total_ok {
                any_exceeds_regular_threshold = true;
            }
        }

        if comparable_count == 0 {
            return None;
        }
        if any_exceeds_regular_threshold {
            Some(false)
        } else if all_within_economy_budget {
            Some(true)
        } else {
            // Hysteresis band between switch_to_economy and switch_to_regular:
            // keep current primary.
            Some(self.primary == CandidateKind::Economy)
        }
    }

    pub fn decide(
        &mut self,
        mode: RoutingMode,
        purpose: RequestPurpose,
        config_version: u64,
        rng_unit: f64,
    ) -> RoutingDecision {
        let now = self.clock.now();
        self.prune_all(now);

        match mode {
            RoutingMode::FixedRegular => {
                self.last_actual = Some(CandidateKind::Regular);
                self.last_is_probe = false;
                return RoutingDecision {
                    selected: CandidateKind::Regular,
                    is_probe: false,
                    primary: CandidateKind::Regular,
                    mode,
                    reason: "fixed regular".to_string(),
                    config_version,
                };
            }
            RoutingMode::FixedEconomy => {
                self.last_actual = Some(CandidateKind::Economy);
                self.last_is_probe = false;
                return RoutingDecision {
                    selected: CandidateKind::Economy,
                    is_probe: false,
                    primary: CandidateKind::Economy,
                    mode,
                    reason: "fixed economy".to_string(),
                    config_version,
                };
            }
            RoutingMode::Auto => {}
        }

        // Time-window boundary evaluation (fires once per boundary transition).
        let current_off_peak = self.config.is_off_peak(now);
        if self.last_off_peak != Some(current_off_peak) {
            self.last_off_peak = Some(current_off_peak);
            if current_off_peak {
                if self.primary == CandidateKind::Regular
                    && !self.is_economy_paused(now)
                    && self.economy_stats.consecutive_failures == 0
                    && !self.in_cooldown(now)
                {
                    let healthy = self.evaluate_economy_vs_regular(now) != Some(false);
                    if healthy {
                        self.primary = CandidateKind::Economy;
                        self.last_switch_reason = "entered off-peak window".to_string();
                    }
                }
            } else if self.primary == CandidateKind::Economy {
                match self.evaluate_economy_vs_regular(now) {
                    Some(true) => {
                        self.last_switch_reason =
                            "left off-peak; economy within latency budget".to_string();
                    }
                    _ => {
                        self.primary = CandidateKind::Regular;
                        self.last_switch_reason = "left off-peak; returning to regular".to_string();
                    }
                }
            }
        }

        if purpose == RequestPurpose::Auxiliary {
            self.last_actual = Some(self.primary);
            self.last_is_probe = false;
            return RoutingDecision {
                selected: self.primary,
                is_probe: false,
                primary: self.primary,
                mode,
                reason: "auxiliary request uses healthy primary".to_string(),
                config_version,
            };
        }

        // Recovery probe after economy pause expires.
        if self.primary == CandidateKind::Regular
            && self.is_economy_recovery_probe_due(now)
            && self.in_flight_probes < self.config.max_in_flight_probes
        {
            self.in_flight_probes += 1;
            self.recovery_probe_sent = true;
            self.last_actual = Some(CandidateKind::Economy);
            self.last_is_probe = true;
            return RoutingDecision {
                selected: CandidateKind::Economy,
                is_probe: true,
                primary: self.primary,
                mode,
                reason: "economy recovery probe".to_string(),
                config_version,
            };
        }

        let probe_candidate = self.primary.other();
        let probe_paused = probe_candidate == CandidateKind::Economy && self.is_economy_paused(now);
        let recovery_pending =
            probe_candidate == CandidateKind::Economy && self.economy_paused_until.is_some();

        if !probe_paused
            && !recovery_pending
            && self.in_flight_probes < self.config.max_in_flight_probes
            && self.config.probe_ratio > 0.0
        {
            let jitter = (rng_unit - 0.5) * 2.0 * self.config.probe_jitter;
            let threshold = (self.config.probe_ratio + jitter).clamp(0.0, 1.0);
            if rng_unit < threshold {
                self.in_flight_probes += 1;
                self.last_actual = Some(probe_candidate);
                self.last_is_probe = true;
                return RoutingDecision {
                    selected: probe_candidate,
                    is_probe: true,
                    primary: self.primary,
                    mode,
                    reason: format!("probing {}", probe_candidate.as_str()),
                    config_version,
                };
            }
        }

        self.last_actual = Some(self.primary);
        self.last_is_probe = false;
        RoutingDecision {
            selected: self.primary,
            is_probe: false,
            primary: self.primary,
            mode,
            reason: self.last_switch_reason.clone(),
            config_version,
        }
    }

    pub fn record_observation(
        &mut self,
        mode: RoutingMode,
        current_config_version: u64,
        obs: RequestObservation,
    ) {
        if obs.is_probe && self.in_flight_probes > 0 {
            self.in_flight_probes -= 1;
        }

        if obs.config_version != current_config_version {
            return;
        }

        self.last_actual = Some(obs.candidate);
        self.last_actual_model = Some(obs.actual_model.clone());

        if obs.purpose == RequestPurpose::Auxiliary || mode != RoutingMode::Auto {
            return;
        }

        if obs.outcome == RequestOutcome::Cancelled {
            return;
        }

        let now = self.clock.now();
        self.prune_all(now);

        match obs.outcome {
            RequestOutcome::Timeout | RequestOutcome::AvailabilityError => match obs.candidate {
                CandidateKind::Economy => {
                    self.economy_stats.record_failure(now);
                    let is_severe = obs.outcome == RequestOutcome::AvailabilityError
                        || obs.total_duration >= self.config.severe_timeout_threshold;
                    if is_severe || self.economy_stats.consecutive_failures >= 2 {
                        self.economy_paused_until = Some(
                            now + ChronoDuration::from_std(
                                self.config.economy_severe_timeout_pause,
                            )
                            .unwrap_or_else(|_| ChronoDuration::seconds(300)),
                        );
                        self.recovery_probe_sent = false;
                        if self.primary == CandidateKind::Economy {
                            self.primary = CandidateKind::Regular;
                            self.last_switch_reason =
                                "economy severe timeout or unavailable".to_string();
                            self.to_economy_confirmations = 0;
                            self.to_regular_confirmations = 0;
                        }
                    }
                }
                CandidateKind::Regular => {
                    self.regular_stats.record_failure(now);
                    if self.primary == CandidateKind::Regular
                        && self.regular_stats.consecutive_failures >= 2
                        && !self.is_economy_paused(now)
                    {
                        self.primary = CandidateKind::Economy;
                        self.last_switch_reason =
                            "regular unavailable; failing over to economy".to_string();
                        self.to_economy_confirmations = 0;
                        self.to_regular_confirmations = 0;
                    }
                }
            },
            RequestOutcome::NonSwitchableError => {
                // Auth/config/parameter errors: do not trigger switching.
            }
            RequestOutcome::Cancelled => {}
            RequestOutcome::Success => {
                let Some(ttft) = obs.ttft else {
                    return;
                };
                if obs.candidate == CandidateKind::Economy && self.recovery_probe_sent {
                    self.economy_paused_until = None;
                    self.recovery_probe_sent = false;
                }
                let bucket = self.config.bucket_for(obs.input_tokens);
                match obs.candidate {
                    CandidateKind::Regular => self.regular_stats.record_success(
                        now,
                        bucket,
                        ttft,
                        obs.total_duration,
                        obs.output_tokens,
                        self.config.max_samples_per_model,
                    ),
                    CandidateKind::Economy => self.economy_stats.record_success(
                        now,
                        bucket,
                        ttft,
                        obs.total_duration,
                        obs.output_tokens,
                        self.config.max_samples_per_model,
                    ),
                }

                if self.in_cooldown(now) {
                    return;
                }

                match (self.primary, self.evaluate_economy_vs_regular(now)) {
                    (CandidateKind::Regular, Some(true)) if !self.is_economy_paused(now) => {
                        self.to_economy_confirmations += 1;
                        self.to_regular_confirmations = 0;
                        if self.to_economy_confirmations >= self.config.confirm_count {
                            self.primary = CandidateKind::Economy;
                            self.cooldown_until = Some(
                                now + ChronoDuration::from_std(self.config.cooldown)
                                    .unwrap_or_else(|_| ChronoDuration::seconds(300)),
                            );
                            self.to_economy_confirmations = 0;
                            self.last_switch_reason = "economy latency within budget".to_string();
                        }
                    }
                    (CandidateKind::Economy, Some(false)) => {
                        self.to_regular_confirmations += 1;
                        self.to_economy_confirmations = 0;
                        if self.to_regular_confirmations >= self.config.confirm_count {
                            self.primary = CandidateKind::Regular;
                            self.cooldown_until = Some(
                                now + ChronoDuration::from_std(self.config.cooldown)
                                    .unwrap_or_else(|_| ChronoDuration::seconds(300)),
                            );
                            self.to_regular_confirmations = 0;
                            self.last_switch_reason = "economy latency exceeded budget".to_string();
                        }
                    }
                    _ => {
                        self.to_economy_confirmations = 0;
                        self.to_regular_confirmations = 0;
                    }
                }
            }
        }
    }
}
