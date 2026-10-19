//! Bounded JSONL log of harness memory-monitor events.
//! The file is the record another process can read. The in-process latch is
//! reported only when the caller is the serve process that owns it.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::config::paths::Paths;
use crate::memory_gate;

pub const MAX_ENTRIES: usize = 500;
pub const DEFAULT_LIMIT: usize = 100;
const LOG_FILE: &str = "logs/memory_monitor.jsonl";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MemoryLogEntry {
    pub at: String,
    pub level: String,
    pub event: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub available_mib: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit_mib: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_rss_mib: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cgroup_limited: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocked: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paused_tasks: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stopped_schedules: Option<u64>,
}

impl MemoryLogEntry {
    pub fn new(
        level: impl Into<String>,
        event: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            level: level.into(),
            event: event.into(),
            message: message.into(),
            available_mib: None,
            limit_mib: None,
            process_rss_mib: None,
            cgroup_limited: None,
            blocked: None,
            paused_tasks: None,
            stopped_schedules: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MemoryLogView {
    pub blocked: bool,
    pub path: String,
    pub entries: Vec<MemoryLogEntry>,
}

pub fn log_path() -> PathBuf {
    Paths::in_state_dir(LOG_FILE)
}

pub(crate) fn normalize_limit(limit: usize) -> usize {
    match limit {
        0 => DEFAULT_LIMIT,
        value => value.min(MAX_ENTRIES),
    }
}

pub fn record(entry: MemoryLogEntry) {
    let path = log_path();
    if let Err(error) = append(&path, &entry) {
        tracing::warn!(
            %error,
            path = %path.display(),
            "Failed to write harness memory monitor log"
        );
    }
}

/// Recent events for the serve process. `blocked` is the live admission latch.
pub fn recent(limit: usize) -> MemoryLogView {
    load(&log_path(), limit, Some(memory_gate::is_blocked()))
}

/// Recent events for a process that does not own the latch.
/// `blocked` is taken from the newest entry that recorded it.
pub fn recent_recorded(limit: usize) -> MemoryLogView {
    load(&log_path(), limit, None)
}

pub(crate) fn load(path: &Path, limit: usize, live_blocked: Option<bool>) -> MemoryLogView {
    let entries = match read_entries(path, limit) {
        Ok(entries) => entries,
        Err(error) => {
            tracing::warn!(
                %error,
                path = %path.display(),
                "Failed to read harness memory monitor log"
            );
            Vec::new()
        }
    };
    let blocked = live_blocked.unwrap_or_else(|| {
        entries
            .iter()
            .rev()
            .find_map(|entry| entry.blocked)
            .unwrap_or(false)
    });
    MemoryLogView {
        blocked,
        path: path.display().to_string(),
        entries,
    }
}

pub(crate) fn append(path: &Path, entry: &MemoryLogEntry) -> anyhow::Result<()> {
    let _guard = writer_lock();
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let mut lines = valid_lines(path)?;
    lines.push(serde_json::to_string(entry)?);
    if lines.len() > MAX_ENTRIES {
        let drop_count = lines.len() - MAX_ENTRIES;
        lines.drain(0..drop_count);
    }
    let tmp = temp_path(path);
    std::fs::write(&tmp, format!("{}\n", lines.join("\n")))?;
    if let Err(error) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(error.into());
    }
    Ok(())
}

pub(crate) fn read_entries(path: &Path, limit: usize) -> anyhow::Result<Vec<MemoryLogEntry>> {
    let limit = normalize_limit(limit);
    if !path.exists() {
        return Ok(Vec::new());
    }
    let mut entries = Vec::new();
    for line in std::fs::read_to_string(path)?.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Ok(entry) = serde_json::from_str::<MemoryLogEntry>(trimmed) {
            entries.push(entry);
        }
    }
    if entries.len() > limit {
        let drop_count = entries.len() - limit;
        entries.drain(0..drop_count);
    }
    Ok(entries)
}

fn valid_lines(path: &Path) -> anyhow::Result<Vec<String>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let mut lines = Vec::new();
    for line in std::fs::read_to_string(path)?.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if serde_json::from_str::<MemoryLogEntry>(trimmed).is_ok() {
            lines.push(trimmed.to_string());
        }
    }
    Ok(lines)
}

fn temp_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("memory_monitor.jsonl");
    path.with_file_name(format!(".{name}.{}.tmp", std::process::id()))
}

fn writer_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_the_newest_entries_and_returns_the_oldest_of_the_window_first() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("memory_monitor.jsonl");
        for index in 0..=MAX_ENTRIES {
            let mut entry = MemoryLogEntry::new("info", "sample", format!("sample {index}"));
            entry.at = format!("t{index:04}");
            append(&path, &entry).unwrap();
        }
        let entries = read_entries(&path, MAX_ENTRIES).unwrap();
        assert_eq!(entries.len(), MAX_ENTRIES);
        assert_eq!(entries[0].message, "sample 1");
        assert_eq!(
            entries.last().unwrap().message,
            format!("sample {MAX_ENTRIES}")
        );
        let window = read_entries(&path, 2).unwrap();
        assert_eq!(window[0].message, format!("sample {}", MAX_ENTRIES - 1));
        assert_eq!(window[1].message, format!("sample {MAX_ENTRIES}"));
    }

    #[test]
    fn skips_malformed_lines_when_appending_and_reading() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("memory_monitor.jsonl");
        std::fs::write(
            &path,
            "not-json\n{\"at\":\"t0\",\"level\":\"info\",\"event\":\"sample\",\"message\":\"keep\"}\n{broken\n",
        )
        .unwrap();
        let mut entry = MemoryLogEntry::new("warn", "read_failure", "failed");
        entry.at = "t1".into();
        append(&path, &entry).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.contains("not-json"));
        assert!(!text.contains("{broken"));
        let entries = read_entries(&path, 10).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].message, "keep");
        assert_eq!(entries[1].event, "read_failure");
    }

    #[test]
    fn missing_log_is_empty_and_live_blocked_wins_over_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing.jsonl");
        let view = load(&path, 10, None);
        assert!(view.entries.is_empty());
        assert!(!view.blocked);
        assert_eq!(view.path, path.display().to_string());

        let mut paused = MemoryLogEntry::new("error", "pause", "paused");
        paused.blocked = Some(true);
        append(&path, &paused).unwrap();
        append(&path, &MemoryLogEntry::new("info", "alert_sent", "sent")).unwrap();
        assert!(load(&path, 10, None).blocked);
        assert!(!load(&path, 10, Some(false)).blocked);
    }

    #[test]
    fn entry_json_uses_camel_case_and_omits_empty_fields() {
        let mut entry = MemoryLogEntry::new("info", "sample", "ok");
        entry.available_mib = Some(12);
        entry.blocked = Some(false);
        let json = serde_json::to_value(&entry).unwrap();
        assert_eq!(json["availableMib"], 12);
        assert_eq!(json["blocked"], false);
        assert!(json.get("available_mib").is_none());
        assert!(json.get("limitMib").is_none());
    }

    #[test]
    fn limit_is_clamped_and_append_creates_parent_directories() {
        assert_eq!(normalize_limit(0), DEFAULT_LIMIT);
        assert_eq!(normalize_limit(1), 1);
        assert_eq!(normalize_limit(10_000), MAX_ENTRIES);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("logs").join("memory_monitor.jsonl");
        append(&path, &MemoryLogEntry::new("info", "started", "up")).unwrap();
        assert!(path.is_file());
        assert_eq!(read_entries(&path, 0).unwrap().len(), 1);
    }
}
