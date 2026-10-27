//! Watches host and cgroup memory while `openduck serve` is running.
//! When free memory is already at the pause floor, or a sustained decline will
//! cross that floor inside the horizon, running harness tasks are stopped and
//! new ones are refused until memory recovers.

use std::collections::HashSet;
#[cfg(target_os = "linux")]
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use crate::memory_gate;
use crate::notification::NotificationService;
use crate::scheduler::ScheduledJob;
use crate::scheduler_trait::SchedulerTrait;

const MIB: u64 = 1024 * 1024;

#[derive(Debug, Clone)]
pub struct MemoryMonitorConfig {
    pub enabled: bool,
    pub poll_interval: Duration,
    pub pause_available_ratio: f64,
    pub pause_available_bytes: u64,
    pub recover_available_ratio: f64,
    pub recover_available_bytes: u64,
    pub horizon: Duration,
    pub min_samples: usize,
    pub min_decline_bytes_per_sec: f64,
    pub admin_emails: Vec<String>,
}

impl Default for MemoryMonitorConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            poll_interval: Duration::from_secs(15),
            pause_available_ratio: 0.10,
            pause_available_bytes: 512 * MIB,
            recover_available_ratio: 0.20,
            recover_available_bytes: 1024 * MIB,
            horizon: Duration::from_secs(180),
            min_samples: 3,
            min_decline_bytes_per_sec: 0.25 * MIB as f64,
            admin_emails: Vec::new(),
        }
    }
}

impl MemoryMonitorConfig {
    pub fn load() -> Self {
        let mut config = Self::default();
        config.enabled = load_enabled();
        config.poll_interval = Duration::from_secs(load_u64(
            &["OPENDUCK_MEMORY_POLL_SECS", "GOOSE_MEMORY_POLL_SECS"],
            "memory_monitor_poll_secs",
            config.poll_interval.as_secs(),
        ));
        config.pause_available_ratio = load_f64(
            &["OPENDUCK_MEMORY_PAUSE_RATIO", "GOOSE_MEMORY_PAUSE_RATIO"],
            "memory_monitor_pause_ratio",
            config.pause_available_ratio,
        );
        config.pause_available_bytes = load_u64(
            &["OPENDUCK_MEMORY_PAUSE_MIB", "GOOSE_MEMORY_PAUSE_MIB"],
            "memory_monitor_pause_mib",
            config.pause_available_bytes / MIB,
        ) * MIB;
        config.recover_available_ratio = load_f64(
            &[
                "OPENDUCK_MEMORY_RECOVER_RATIO",
                "GOOSE_MEMORY_RECOVER_RATIO",
            ],
            "memory_monitor_recover_ratio",
            config.recover_available_ratio,
        );
        config.recover_available_bytes = load_u64(
            &["OPENDUCK_MEMORY_RECOVER_MIB", "GOOSE_MEMORY_RECOVER_MIB"],
            "memory_monitor_recover_mib",
            config.recover_available_bytes / MIB,
        ) * MIB;
        config.horizon = Duration::from_secs(load_u64(
            &["OPENDUCK_MEMORY_HORIZON_SECS", "GOOSE_MEMORY_HORIZON_SECS"],
            "memory_monitor_horizon_secs",
            config.horizon.as_secs(),
        ));
        if let Some(decline_mib) = load_optional_f64(
            &[
                "OPENDUCK_MEMORY_MIN_DECLINE_MIB_PER_SEC",
                "GOOSE_MEMORY_MIN_DECLINE_MIB_PER_SEC",
            ],
            "memory_monitor_min_decline_mib_per_sec",
        ) {
            config.min_decline_bytes_per_sec = decline_mib * MIB as f64;
        }
        config.admin_emails = load_admin_emails();
        config.normalize();
        config
    }

    fn normalize(&mut self) {
        if self.poll_interval < Duration::from_secs(5) {
            self.poll_interval = Duration::from_secs(5);
        }
        if self.horizon < Duration::from_secs(30) {
            self.horizon = Duration::from_secs(30);
        }
        if self.min_samples < 3 {
            self.min_samples = 3;
        }
        self.pause_available_ratio = self.pause_available_ratio.clamp(0.01, 0.5);
        let min_recover_ratio = (self.pause_available_ratio + 0.01).min(0.9);
        self.recover_available_ratio = self.recover_available_ratio.clamp(min_recover_ratio, 0.9);
        if self.pause_available_bytes < 64 * MIB {
            self.pause_available_bytes = 64 * MIB;
        }
        if self.recover_available_bytes <= self.pause_available_bytes {
            self.recover_available_bytes = self.pause_available_bytes.saturating_add(256 * MIB);
        }
        if self.min_decline_bytes_per_sec < 1.0 {
            self.min_decline_bytes_per_sec = 1.0;
        }
    }

    fn history_limit(&self) -> usize {
        let ticks = (self.horizon.as_secs_f64() / self.poll_interval.as_secs_f64()).ceil() as usize;
        ticks
            .saturating_add(self.min_samples)
            .clamp(self.min_samples, 24)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct MemoryReading {
    pub total_bytes: u64,
    pub available_bytes: u64,
    pub process_rss_bytes: u64,
    pub cgroup_limit_bytes: Option<u64>,
    pub cgroup_usage_bytes: Option<u64>,
}

#[derive(Debug, Clone, Copy)]
pub struct Headroom {
    pub available_bytes: u64,
    pub limit_bytes: u64,
    pub cgroup_limited: bool,
    pub process_rss_bytes: u64,
}

#[derive(Debug, Clone, Copy)]
pub struct SamplePoint {
    pub available_bytes: u64,
    pub limit_bytes: u64,
    pub cgroup_limited: bool,
    pub elapsed_secs: f64,
    pub process_rss_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PressureAction {
    Hold,
    Pause { reason: String },
    Recover,
}

#[derive(Debug, Clone)]
pub struct PausedTask {
    pub job_id: String,
    pub task_id: String,
    pub description: String,
    pub project_slug: Option<String>,
}

#[derive(Debug, Clone)]
pub struct MemoryAlert {
    pub reason: String,
    pub available_bytes: u64,
    pub limit_bytes: u64,
    pub process_rss_bytes: u64,
    pub paused_tasks: Vec<PausedTask>,
    pub stopped_schedules: Vec<String>,
}

pub fn spawn(scheduler: Option<Arc<dyn SchedulerTrait>>) {
    let config = MemoryMonitorConfig::load();
    if !config.enabled {
        tracing::info!("Harness memory monitor is disabled");
        let mut entry = crate::memory_log::MemoryLogEntry::new(
            "info",
            "disabled",
            "Harness memory monitor is disabled",
        );
        entry.blocked = Some(false);
        crate::memory_log::record(entry);
        return;
    }
    tracing::info!(
        poll_secs = config.poll_interval.as_secs(),
        pause_mib = config.pause_available_bytes / MIB,
        pause_ratio = config.pause_available_ratio,
        recover_mib = config.recover_available_bytes / MIB,
        horizon_secs = config.horizon.as_secs(),
        administrators = config.admin_emails.len(),
        "Harness memory monitor started"
    );
    let mut started = crate::memory_log::MemoryLogEntry::new(
        "info",
        "started",
        format!(
            "Harness memory monitor started. Poll {}s, pause floor {} MiB ({:.0}%), recover {} MiB, horizon {}s, {} administrator email(s).",
            config.poll_interval.as_secs(),
            config.pause_available_bytes / MIB,
            config.pause_available_ratio * 100.0,
            config.recover_available_bytes / MIB,
            config.horizon.as_secs(),
            config.admin_emails.len()
        ),
    );
    started.blocked = Some(memory_gate::is_blocked());
    crate::memory_log::record(started);
    tokio::spawn(async move {
        run_monitor(config, scheduler).await;
    });
}

async fn run_monitor(config: MemoryMonitorConfig, scheduler: Option<Arc<dyn SchedulerTrait>>) {
    let mut samples: Vec<SamplePoint> = Vec::new();
    let started = std::time::Instant::now();
    let mut read_failures = 0u32;
    loop {
        match read_memory_reading() {
            Some(reading) => {
                read_failures = 0;
                let headroom = headroom(&reading);
                samples.push(SamplePoint {
                    available_bytes: headroom.available_bytes,
                    limit_bytes: headroom.limit_bytes,
                    cgroup_limited: headroom.cgroup_limited,
                    elapsed_secs: started.elapsed().as_secs_f64(),
                    process_rss_bytes: headroom.process_rss_bytes,
                });
                let limit = config.history_limit();
                if samples.len() > limit {
                    let drop_count = samples.len() - limit;
                    samples.drain(0..drop_count);
                }
                let latched = memory_gate::is_blocked();
                match decide(&samples, latched, &config) {
                    PressureAction::Hold => {
                        record_reading(
                            "info",
                            "sample",
                            sample_message(&headroom, latched),
                            &headroom,
                            latched,
                        );
                    }
                    PressureAction::Pause { reason } => {
                        if !latched {
                            apply_pause(&config, &reason, &headroom, scheduler.as_ref()).await;
                        } else {
                            record_reading(
                                "info",
                                "sample",
                                sample_message(&headroom, true),
                                &headroom,
                                true,
                            );
                        }
                    }
                    PressureAction::Recover => {
                        if latched {
                            memory_gate::set_blocked(false);
                            tracing::info!(
                                available_mib = headroom.available_bytes / MIB,
                                "Harness memory recovered; new tasks are accepted again"
                            );
                            record_reading(
                                "info",
                                "recover",
                                "Harness memory recovered; new tasks are accepted again",
                                &headroom,
                                false,
                            );
                        }
                    }
                }
            }
            None => {
                read_failures = read_failures.saturating_add(1);
                if read_failures == 1 || read_failures.is_multiple_of(20) {
                    tracing::warn!("Harness memory monitor could not read memory statistics");
                }
                crate::memory_log::record(crate::memory_log::MemoryLogEntry::new(
                    "warn",
                    "read_failure",
                    format!("Could not read memory statistics (attempt {read_failures})"),
                ));
            }
        }
        tokio::time::sleep(config.poll_interval).await;
    }
}

async fn apply_pause(
    config: &MemoryMonitorConfig,
    reason: &str,
    headroom: &Headroom,
    scheduler: Option<&Arc<dyn SchedulerTrait>>,
) {
    memory_gate::set_blocked(true);
    let paused_tasks = pause_harness_jobs(reason);
    let stopped_schedules = match scheduler {
        Some(scheduler) => stop_scheduled_jobs(scheduler.as_ref()).await,
        None => Vec::new(),
    };
    tracing::error!(
        reason,
        paused_tasks = paused_tasks.len(),
        stopped_schedules = stopped_schedules.len(),
        available_mib = headroom.available_bytes / MIB,
        "Paused harness tasks because memory is about to run out"
    );
    let mut pause = reading_entry(
        "error",
        "pause",
        format!(
            "{reason} Paused {} running task(s) and stopped {} scheduled job(s).",
            paused_tasks.len(),
            stopped_schedules.len()
        ),
        headroom,
        true,
    );
    pause.paused_tasks = Some(paused_tasks.len() as u64);
    pause.stopped_schedules = Some(stopped_schedules.len() as u64);
    crate::memory_log::record(pause);
    let alert = MemoryAlert {
        reason: reason.to_string(),
        available_bytes: headroom.available_bytes,
        limit_bytes: headroom.limit_bytes,
        process_rss_bytes: headroom.process_rss_bytes,
        paused_tasks,
        stopped_schedules,
    };
    alert_administrator(config, &alert).await;
}

pub fn headroom(reading: &MemoryReading) -> Headroom {
    let mut available = reading.available_bytes;
    let mut limit = reading.total_bytes.max(1);
    let mut cgroup_limited = false;
    if let (Some(cgroup_limit), Some(cgroup_usage)) = (
        reading.cgroup_limit_bytes.filter(|limit| *limit > 0),
        reading.cgroup_usage_bytes,
    ) {
        let cgroup_available = cgroup_limit.saturating_sub(cgroup_usage);
        if cgroup_available < available {
            available = cgroup_available;
            limit = cgroup_limit;
            cgroup_limited = true;
        }
    }
    Headroom {
        available_bytes: available,
        limit_bytes: limit.max(1),
        cgroup_limited,
        process_rss_bytes: reading.process_rss_bytes,
    }
}

pub fn decide(
    samples: &[SamplePoint],
    latched: bool,
    config: &MemoryMonitorConfig,
) -> PressureAction {
    let Some(latest) = samples.last().copied() else {
        return PressureAction::Hold;
    };
    let pause_floor = pause_floor(latest.limit_bytes, latest.cgroup_limited, config);
    let recover_floor = recover_floor(
        latest.limit_bytes,
        latest.cgroup_limited,
        pause_floor,
        config,
    );
    let projected = project_available(samples, config);
    let predicted_breach = projected
        .map(|(slope, future)| {
            slope < -config.min_decline_bytes_per_sec && future <= pause_floor as f64
        })
        .unwrap_or(false);
    let below_floor = latest.available_bytes <= pause_floor;

    if latched {
        if latest.available_bytes >= recover_floor && !predicted_breach {
            PressureAction::Recover
        } else {
            PressureAction::Hold
        }
    } else if below_floor || predicted_breach {
        PressureAction::Pause {
            reason: pause_reason(latest, pause_floor, projected, config.horizon),
        }
    } else {
        PressureAction::Hold
    }
}

fn pause_floor(limit: u64, cgroup_limited: bool, config: &MemoryMonitorConfig) -> u64 {
    if !cgroup_limited {
        return config.pause_available_bytes.min(limit);
    }
    let ratio_floor = (limit as f64 * config.pause_available_ratio) as u64;
    let absolute = config.pause_available_bytes.min(limit / 2);
    ratio_floor
        .max(absolute)
        .min(limit.saturating_sub(1).max(1))
}

fn recover_floor(
    limit: u64,
    cgroup_limited: bool,
    pause_floor: u64,
    config: &MemoryMonitorConfig,
) -> u64 {
    let raw = if cgroup_limited {
        let ratio_floor = (limit as f64 * config.recover_available_ratio) as u64;
        let absolute = config
            .recover_available_bytes
            .min(limit.saturating_mul(3) / 4);
        ratio_floor.max(absolute)
    } else {
        config.recover_available_bytes
    };
    let raw = raw.min(limit);
    if raw > pause_floor {
        raw
    } else {
        pause_floor
            .saturating_add((limit / 10).max(1))
            .min(limit)
            .max(pause_floor.saturating_add(1))
    }
}

fn project_available(samples: &[SamplePoint], config: &MemoryMonitorConfig) -> Option<(f64, f64)> {
    let window = trend_window(samples, config.horizon.as_secs_f64(), config.min_samples);
    if window.len() < config.min_samples {
        return None;
    }
    let n = window.len() as f64;
    let mut sum_t = 0.0;
    let mut sum_y = 0.0;
    let mut sum_tt = 0.0;
    let mut sum_ty = 0.0;
    for sample in window {
        let t = sample.elapsed_secs;
        let y = sample.available_bytes as f64;
        sum_t += t;
        sum_y += y;
        sum_tt += t * t;
        sum_ty += t * y;
    }
    let denom = n * sum_tt - sum_t * sum_t;
    if denom.abs() < f64::EPSILON {
        return None;
    }
    let slope = (n * sum_ty - sum_t * sum_y) / denom;
    let intercept = (sum_y - slope * sum_t) / n;
    let future_t = window.last()?.elapsed_secs + config.horizon.as_secs_f64();
    Some((slope, intercept + slope * future_t))
}

fn trend_window(samples: &[SamplePoint], horizon_secs: f64, min_samples: usize) -> &[SamplePoint] {
    let Some(latest) = samples.last() else {
        return samples;
    };
    let start = latest.elapsed_secs - horizon_secs;
    let index = samples
        .iter()
        .position(|sample| sample.elapsed_secs >= start)
        .unwrap_or(samples.len().saturating_sub(1));
    let window = &samples[index..];
    if window.len() >= min_samples || samples.len() < min_samples {
        window
    } else {
        &samples[samples.len() - min_samples..]
    }
}

fn pause_reason(
    latest: SamplePoint,
    pause_floor: u64,
    projected: Option<(f64, f64)>,
    horizon: Duration,
) -> String {
    let available = mib(latest.available_bytes);
    let floor = mib(pause_floor);
    let limit = mib(latest.limit_bytes);
    if latest.available_bytes <= pause_floor {
        format!(
            "Paused because memory is about to run out. Available memory is {available} MiB, at or below the {floor} MiB pause floor (limit {limit} MiB)."
        )
    } else {
        let projected_mib = projected.map(|(_, future)| mib_f64(future)).unwrap_or(0);
        let horizon_secs = horizon.as_secs();
        format!(
            "Paused because memory is about to run out. Available memory is {available} MiB and falling; it is projected to reach {projected_mib} MiB within {horizon_secs}s, below the {floor} MiB pause floor (limit {limit} MiB)."
        )
    }
}

fn pause_harness_jobs(reason: &str) -> Vec<PausedTask> {
    let jobs = crate::control::harness::get_active_jobs_list();
    let mut paused = Vec::with_capacity(jobs.len());
    for job in jobs {
        match crate::control::harness::stop_active_job(&job.job_id, Some(reason)) {
            Ok(_) => paused.push(PausedTask {
                job_id: job.job_id,
                task_id: job.task_id,
                description: job.description,
                project_slug: job.project_slug,
            }),
            Err(error) => {
                tracing::warn!(
                    %error,
                    job_id = %job.job_id,
                    "Failed to pause harness job under memory pressure"
                );
            }
        }
    }
    paused
}

fn schedules_to_stop(jobs: &[ScheduledJob]) -> Vec<String> {
    jobs.iter()
        .filter(|job| job.currently_running)
        .map(|job| job.id.clone())
        .collect()
}

async fn stop_scheduled_jobs(scheduler: &dyn SchedulerTrait) -> Vec<String> {
    let jobs = scheduler.list_scheduled_jobs().await;
    let ids = schedules_to_stop(&jobs);
    let mut stopped = Vec::new();
    for id in ids {
        match scheduler.kill_running_job(&id).await {
            Ok(()) => stopped.push(id),
            Err(error) => {
                tracing::warn!(
                    %error,
                    job_id = %id,
                    "Failed to stop scheduled job under memory pressure"
                );
            }
        }
    }
    stopped
}

pub fn render_memory_alert(alert: &MemoryAlert) -> (String, String) {
    let subject = "[ALERT] OpenDuck harness paused: memory is about to run out".to_string();
    let mut body = String::new();
    body.push_str("OpenDuck paused running harness tasks because memory is about to run out.\n\n");
    body.push_str(&alert.reason);
    body.push_str("\n\n");
    body.push_str(&format!(
        "Available: {} MiB\nLimit: {} MiB\nProcess RSS: {} MiB\n",
        mib(alert.available_bytes),
        mib(alert.limit_bytes),
        mib(alert.process_rss_bytes)
    ));
    body.push_str("\nPaused tasks:\n");
    if alert.paused_tasks.is_empty() {
        body.push_str("(none running)\n");
    } else {
        for task in &alert.paused_tasks {
            let project = task.project_slug.as_deref().unwrap_or("-");
            body.push_str(&format!(
                "- {} ({}) project={} job={}\n",
                task.description, task.task_id, project, task.job_id
            ));
        }
    }
    body.push_str("\nStopped scheduled jobs:\n");
    if alert.stopped_schedules.is_empty() {
        body.push_str("(none running)\n");
    } else {
        for id in &alert.stopped_schedules {
            body.push_str(&format!("- {id}\n"));
        }
    }
    body.push_str(
        "\nNew harness tasks are refused until available memory recovers. Continue a paused task from its history after the host has free memory. Tasks are not resumed automatically.\n",
    );
    (subject, body)
}

async fn alert_administrator(config: &MemoryMonitorConfig, alert: &MemoryAlert) {
    let (subject, body) = render_memory_alert(alert);
    if config.admin_emails.is_empty() {
        tracing::error!(
            subject = %subject,
            "Harness memory alert has no administrator email. Set OPENDUCK_ADMIN_EMAIL or email.default_recipients."
        );
        record_alert(
            "error",
            "alert_skipped",
            "Alert email skipped: no administrator email",
        );
        return;
    }
    let notifier = NotificationService::load();
    if let Some(reason) = notifier.alert_skip_reason(&config.admin_emails) {
        let _ = notifier
            .send_alert(&config.admin_emails, &subject, &body)
            .await;
        record_alert(
            "warn",
            "alert_skipped",
            format!("Alert email skipped: {reason}"),
        );
        return;
    }
    match notifier
        .send_alert(&config.admin_emails, &subject, &body)
        .await
    {
        Ok(()) => record_alert(
            "info",
            "alert_sent",
            format!(
                "Alert email sent to {} administrator(s)",
                config.admin_emails.len()
            ),
        ),
        Err(error) => {
            tracing::error!(%error, "Failed to send harness memory alert email");
            record_alert(
                "error",
                "alert_failed",
                format!("Failed to send harness memory alert email: {error}"),
            );
        }
    }
}

fn sample_message(headroom: &Headroom, blocked: bool) -> String {
    let scope = if headroom.cgroup_limited {
        "cgroup"
    } else {
        "host"
    };
    let state = if blocked { ", tasks paused" } else { "" };
    format!(
        "Available {} MiB of {} MiB ({scope}{state})",
        mib(headroom.available_bytes),
        mib(headroom.limit_bytes)
    )
}

fn reading_entry(
    level: &str,
    event: &str,
    message: impl Into<String>,
    headroom: &Headroom,
    blocked: bool,
) -> crate::memory_log::MemoryLogEntry {
    let mut entry = crate::memory_log::MemoryLogEntry::new(level, event, message);
    entry.available_mib = Some(mib(headroom.available_bytes));
    entry.limit_mib = Some(mib(headroom.limit_bytes));
    entry.process_rss_mib = Some(mib(headroom.process_rss_bytes));
    entry.cgroup_limited = Some(headroom.cgroup_limited);
    entry.blocked = Some(blocked);
    entry
}

fn record_reading(
    level: &str,
    event: &str,
    message: impl Into<String>,
    headroom: &Headroom,
    blocked: bool,
) {
    crate::memory_log::record(reading_entry(level, event, message, headroom, blocked));
}

fn record_alert(level: &str, event: &str, message: impl Into<String>) {
    let mut entry = crate::memory_log::MemoryLogEntry::new(level, event, message);
    entry.blocked = Some(memory_gate::is_blocked());
    crate::memory_log::record(entry);
}

fn mib(bytes: u64) -> u64 {
    bytes / MIB
}

fn mib_f64(bytes: f64) -> u64 {
    if bytes <= 0.0 {
        0
    } else {
        (bytes / MIB as f64) as u64
    }
}

pub fn read_memory_reading() -> Option<MemoryReading> {
    #[cfg(target_os = "linux")]
    {
        read_linux_memory()
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

#[cfg(target_os = "linux")]
fn read_linux_memory() -> Option<MemoryReading> {
    let meminfo = std::fs::read_to_string("/proc/meminfo").ok()?;
    let (total_bytes, available_bytes) = parse_meminfo(&meminfo)?;
    let process_rss_bytes = std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| parse_vm_rss(&status))
        .unwrap_or(0);
    let (cgroup_limit_bytes, cgroup_usage_bytes) = read_cgroup_memory();
    Some(MemoryReading {
        total_bytes,
        available_bytes,
        process_rss_bytes,
        cgroup_limit_bytes,
        cgroup_usage_bytes,
    })
}

#[cfg(target_os = "linux")]
fn read_cgroup_memory() -> (Option<u64>, Option<u64>) {
    let Ok(text) = std::fs::read_to_string("/proc/self/cgroup") else {
        return (None, None);
    };
    for line in text.lines() {
        let mut cols = line.splitn(3, ':');
        let Some(_id) = cols.next() else { continue };
        let Some(controllers) = cols.next() else {
            continue;
        };
        let Some(cgroup_path) = cols.next() else {
            continue;
        };
        let relative = cgroup_path.trim_start_matches('/');
        if controllers.is_empty() {
            let base = Path::new("/sys/fs/cgroup").join(relative);
            if let Some(limit) = read_cgroup_file(&base.join("memory.max")) {
                let usage = read_cgroup_counter(&base.join("memory.current"));
                return (Some(limit), usage);
            }
        } else if controllers
            .split(',')
            .any(|controller| controller == "memory")
        {
            let base = Path::new("/sys/fs/cgroup/memory").join(relative);
            if let Some(limit) = read_cgroup_file(&base.join("memory.limit_in_bytes")) {
                let usage = read_cgroup_counter(&base.join("memory.usage_in_bytes"));
                return (Some(limit), usage);
            }
        }
    }
    (None, None)
}

#[cfg(target_os = "linux")]
fn read_cgroup_file(path: &Path) -> Option<u64> {
    let text = std::fs::read_to_string(path).ok()?;
    parse_cgroup_limit(text.trim())
}

#[cfg(target_os = "linux")]
fn read_cgroup_counter(path: &Path) -> Option<u64> {
    let text = std::fs::read_to_string(path).ok()?;
    text.trim().parse().ok()
}

pub fn parse_meminfo(text: &str) -> Option<(u64, u64)> {
    let total = kib_field(text, "MemTotal")?;
    let available = kib_field(text, "MemAvailable")?;
    Some((total, available))
}

pub fn parse_vm_rss(text: &str) -> Option<u64> {
    kib_field(text, "VmRSS")
}

pub fn parse_cgroup_limit(text: &str) -> Option<u64> {
    let text = text.trim();
    if text.is_empty() || text.eq_ignore_ascii_case("max") {
        return None;
    }
    let value: u64 = text.parse().ok()?;
    if value >= 1 << 60 {
        None
    } else {
        Some(value)
    }
}

fn kib_field(text: &str, key: &str) -> Option<u64> {
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        let Some(name) = parts.next() else {
            continue;
        };
        if name.trim_end_matches(':') != key {
            continue;
        }
        let Ok(value) = parts.next().unwrap_or("").parse::<u64>() else {
            continue;
        };
        return Some(value.saturating_mul(1024));
    }
    None
}

fn load_enabled() -> bool {
    for key in ["OPENDUCK_MEMORY_MONITOR", "GOOSE_MEMORY_MONITOR"] {
        if let Ok(raw) = std::env::var(key) {
            return parse_enabled_flag(&raw, true);
        }
    }
    crate::config::Config::global()
        .get_param::<bool>("memory_monitor_enabled")
        .unwrap_or(true)
}

pub fn parse_enabled_flag(raw: &str, default: bool) -> bool {
    match raw.trim().to_ascii_lowercase().as_str() {
        "0" | "false" | "off" | "no" | "disabled" => false,
        "1" | "true" | "on" | "yes" | "enabled" => true,
        _ => default,
    }
}

fn load_u64(env_keys: &[&str], config_key: &str, default: u64) -> u64 {
    if let Some(value) = env_u64(env_keys) {
        return value;
    }
    crate::config::Config::global()
        .get_param::<u64>(config_key)
        .unwrap_or(default)
}

fn load_f64(env_keys: &[&str], config_key: &str, default: f64) -> f64 {
    if let Some(value) = env_f64(env_keys) {
        return value;
    }
    crate::config::Config::global()
        .get_param::<f64>(config_key)
        .unwrap_or(default)
}

fn load_optional_f64(env_keys: &[&str], config_key: &str) -> Option<f64> {
    env_f64(env_keys).or_else(|| {
        crate::config::Config::global()
            .get_param::<f64>(config_key)
            .ok()
    })
}

fn env_u64(keys: &[&str]) -> Option<u64> {
    for key in keys {
        if let Ok(raw) = std::env::var(key) {
            if let Ok(value) = raw.trim().parse::<u64>() {
                return Some(value);
            }
        }
    }
    None
}

fn env_f64(keys: &[&str]) -> Option<f64> {
    for key in keys {
        if let Ok(raw) = std::env::var(key) {
            if let Ok(value) = raw.trim().parse::<f64>() {
                return Some(value);
            }
        }
    }
    None
}

fn load_admin_emails() -> Vec<String> {
    for key in ["OPENDUCK_ADMIN_EMAIL", "GOOSE_ADMIN_EMAIL"] {
        if let Ok(raw) = std::env::var(key) {
            let emails = split_emails(&raw);
            if !emails.is_empty() {
                return emails;
            }
        }
    }
    if let Ok(raw) = crate::config::Config::global().get_param::<String>("admin_email") {
        let emails = split_emails(&raw);
        if !emails.is_empty() {
            return emails;
        }
    }
    if let Ok(list) = crate::config::Config::global().get_param::<Vec<String>>("admin_emails") {
        let emails = dedupe_emails(
            list.into_iter()
                .flat_map(|item| split_emails(&item))
                .collect(),
        );
        if !emails.is_empty() {
            return emails;
        }
    }
    let notification = crate::notification::NotificationConfig::load();
    if !notification.email.default_recipients.is_empty() {
        return dedupe_emails(notification.email.default_recipients);
    }
    for key in ["OPENDUCK_NOTIFY_EMAIL", "GOOSE_NOTIFY_EMAIL"] {
        if let Ok(raw) = std::env::var(key) {
            let emails = split_emails(&raw);
            if !emails.is_empty() {
                return emails;
            }
        }
    }
    Vec::new()
}

pub fn split_emails(raw: &str) -> Vec<String> {
    dedupe_emails(
        raw.split([',', ';', ' ', '\n', '\t'])
            .map(str::trim)
            .filter(|email| email.contains('@'))
            .map(str::to_string)
            .collect(),
    )
}

fn dedupe_emails(emails: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut unique = Vec::new();
    for email in emails {
        let key = email.to_ascii_lowercase();
        if seen.insert(key) {
            unique.push(email);
        }
    }
    unique
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> MemoryMonitorConfig {
        MemoryMonitorConfig::default()
    }

    fn point(available_mib: u64, limit_mib: u64, cgroup: bool, elapsed: f64) -> SamplePoint {
        SamplePoint {
            available_bytes: available_mib * MIB,
            limit_bytes: limit_mib * MIB,
            cgroup_limited: cgroup,
            elapsed_secs: elapsed,
            process_rss_bytes: 128 * MIB,
        }
    }

    fn scheduled(id: &str, running: bool) -> ScheduledJob {
        ScheduledJob {
            id: id.to_string(),
            source: format!("{id}.yaml"),
            cron: "0 * * * *".to_string(),
            last_run: None,
            currently_running: running,
            paused: false,
            current_session_id: None,
            process_start_time: None,
            parameters: Vec::new(),
            recipe_base_dir: None,
            project_id: None,
            working_dir: None,
        }
    }

    #[test]
    fn parse_meminfo_and_rss_use_kib() {
        let meminfo = "MemTotal:       16384000 kB\nMemFree:         1000000 kB\nMemAvailable:    8000000 kB\n";
        let (total, available) = parse_meminfo(meminfo).unwrap();
        assert_eq!(total, 16_384_000 * 1024);
        assert_eq!(available, 8_000_000 * 1024);
        assert_eq!(
            parse_vm_rss("Name:\topenduck\nVmRSS:\t  2048 kB\n").unwrap(),
            2048 * 1024
        );
    }

    #[test]
    fn parse_cgroup_limit_treats_max_and_huge_values_as_unlimited() {
        assert_eq!(parse_cgroup_limit("max"), None);
        assert_eq!(parse_cgroup_limit("9223372036854771712"), None);
        assert_eq!(parse_cgroup_limit("1073741824"), Some(1024 * MIB));
    }

    #[test]
    fn headroom_uses_the_tighter_cgroup_budget() {
        let reading = MemoryReading {
            total_bytes: 16 * 1024 * MIB,
            available_bytes: 8 * 1024 * MIB,
            process_rss_bytes: 256 * MIB,
            cgroup_limit_bytes: Some(1024 * MIB),
            cgroup_usage_bytes: Some(900 * MIB),
        };
        let headroom = headroom(&reading);
        assert!(headroom.cgroup_limited);
        assert_eq!(headroom.available_bytes, 124 * MIB);
        assert_eq!(headroom.limit_bytes, 1024 * MIB);
    }

    #[test]
    fn headroom_keeps_host_memory_when_it_is_tighter_than_the_cgroup() {
        let reading = MemoryReading {
            total_bytes: 16 * 1024 * MIB,
            available_bytes: 100 * MIB,
            process_rss_bytes: 64 * MIB,
            cgroup_limit_bytes: Some(2 * 1024 * MIB),
            cgroup_usage_bytes: Some(200 * MIB),
        };
        let headroom = headroom(&reading);
        assert!(!headroom.cgroup_limited);
        assert_eq!(headroom.available_bytes, 100 * MIB);
        assert_eq!(headroom.limit_bytes, 16 * 1024 * MIB);
    }

    #[test]
    fn immediate_floor_pauses_a_single_low_sample() {
        let action = decide(&[point(100, 16384, false, 0.0)], false, &cfg());
        match action {
            PressureAction::Pause { reason } => {
                assert!(reason.contains("100 MiB"));
                assert!(reason.contains("512 MiB pause floor"));
            }
            other => panic!("expected pause, got {other:?}"),
        }
    }

    #[test]
    fn steady_memory_above_the_floor_holds() {
        let samples = vec![
            point(4096, 16384, false, 0.0),
            point(4080, 16384, false, 15.0),
            point(4064, 16384, false, 30.0),
        ];
        assert_eq!(decide(&samples, false, &cfg()), PressureAction::Hold);
    }

    #[test]
    fn projected_decline_pauses_before_the_floor() {
        let samples = vec![
            point(800, 16384, false, 0.0),
            point(700, 16384, false, 15.0),
            point(600, 16384, false, 30.0),
        ];
        match decide(&samples, false, &cfg()) {
            PressureAction::Pause { reason } => {
                assert!(reason.contains("projected"));
                assert!(reason.contains("600 MiB"));
            }
            other => panic!("expected projected pause, got {other:?}"),
        }
    }

    #[test]
    fn two_samples_do_not_project_a_breach() {
        let samples = vec![
            point(800, 16384, false, 0.0),
            point(600, 16384, false, 15.0),
        ];
        assert_eq!(decide(&samples, false, &cfg()), PressureAction::Hold);
    }

    #[test]
    fn latched_monitor_recovers_only_after_memory_clears_the_upper_floor() {
        let low = vec![point(700, 16384, false, 0.0)];
        assert_eq!(decide(&low, true, &cfg()), PressureAction::Hold);
        let cleared = vec![
            point(2048, 16384, false, 0.0),
            point(2048, 16384, false, 15.0),
            point(2100, 16384, false, 30.0),
        ];
        assert_eq!(decide(&cleared, true, &cfg()), PressureAction::Recover);
    }

    #[test]
    fn latched_monitor_stays_paused_while_the_decline_still_hits_the_floor() {
        let samples = vec![
            point(4000, 16384, false, 0.0),
            point(2500, 16384, false, 15.0),
            point(1500, 16384, false, 30.0),
        ];
        assert_eq!(decide(&samples, true, &cfg()), PressureAction::Hold);
    }

    #[test]
    fn small_cgroup_uses_a_ratio_floor_inside_its_limit() {
        let action = decide(&[point(40, 200, true, 0.0)], false, &cfg());
        assert!(matches!(action, PressureAction::Pause { .. }));
        assert_eq!(
            decide(&[point(150, 200, true, 0.0)], false, &cfg()),
            PressureAction::Hold
        );
    }

    #[test]
    fn alert_names_the_paused_task_and_says_resume_is_manual() {
        let (subject, body) = render_memory_alert(&MemoryAlert {
            reason: "Paused because memory is about to run out.".into(),
            available_bytes: 100 * MIB,
            limit_bytes: 1024 * MIB,
            process_rss_bytes: 800 * MIB,
            paused_tasks: vec![PausedTask {
                job_id: "job-1".into(),
                task_id: "review".into(),
                description: "Daily review".into(),
                project_slug: Some("openduck-harness".into()),
            }],
            stopped_schedules: vec!["nightly".into()],
        });
        assert_eq!(
            subject,
            "[ALERT] OpenDuck harness paused: memory is about to run out"
        );
        assert!(body.contains("Daily review"));
        assert!(body.contains("openduck-harness"));
        assert!(body.contains("nightly"));
        assert!(body.contains("not resumed automatically"));
    }

    #[test]
    fn schedules_to_stop_keeps_only_running_jobs() {
        let jobs = vec![
            scheduled("running", true),
            scheduled("idle", false),
            scheduled("also-running", true),
        ];
        assert_eq!(
            schedules_to_stop(&jobs),
            vec!["running".to_string(), "also-running".to_string()]
        );
    }

    #[test]
    fn split_emails_trims_and_dedupes() {
        assert_eq!(
            split_emails(" Admin@Example.com, admin@example.com; ops@example.com "),
            vec![
                "Admin@Example.com".to_string(),
                "ops@example.com".to_string()
            ]
        );
    }

    #[test]
    fn enabled_flag_accepts_off_switches() {
        assert!(!parse_enabled_flag("off", true));
        assert!(!parse_enabled_flag("0", true));
        assert!(parse_enabled_flag("on", false));
        assert!(parse_enabled_flag("maybe", true));
    }
}
