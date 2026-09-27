use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use serde::Serialize;
use tokio::process::Command;

const MAX_ENTRIES: usize = 200;
const GIT_TIMEOUT: Duration = Duration::from_secs(2);
const SKIP_NAMES: &[&str] = &["node_modules", "target"];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InsightEntry {
    pub name: String,
    pub kind: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitLastCommit {
    pub sha: String,
    pub subject: String,
    pub at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitInsight {
    pub is_repo: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dirty: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ahead: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub behind: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changed: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_commit: Option<GitLastCommit>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectInsight {
    pub exists: bool,
    pub entry_count: usize,
    pub entries: Vec<InsightEntry>,
    pub truncated: bool,
    pub git: GitInsight,
}

impl ProjectInsight {
    pub fn empty() -> Self {
        Self {
            exists: true,
            entry_count: 0,
            entries: Vec::new(),
            truncated: false,
            git: GitInsight::not_a_repo(),
        }
    }

    fn missing() -> Self {
        Self {
            exists: false,
            entry_count: 0,
            entries: Vec::new(),
            truncated: false,
            git: GitInsight::not_a_repo(),
        }
    }
}

impl GitInsight {
    fn not_a_repo() -> Self {
        Self {
            is_repo: false,
            branch: None,
            dirty: None,
            ahead: None,
            behind: None,
            changed: None,
            last_commit: None,
        }
    }
}

pub fn list_directory(path: &Path) -> (Vec<InsightEntry>, usize, bool) {
    let mut entries = Vec::new();
    let Ok(read_dir) = std::fs::read_dir(path) else {
        return (entries, 0, false);
    };
    let mut collected: Vec<InsightEntry> = read_dir
        .flatten()
        .filter_map(|item| {
            let name = item.file_name().to_string_lossy().into_owned();
            if SKIP_NAMES.contains(&name.as_str()) {
                return None;
            }
            let kind = if item.path().is_dir() { "dir" } else { "file" };
            Some(InsightEntry { name, kind })
        })
        .collect();
    collected.sort_by(|a, b| match (a.kind, b.kind) {
        ("dir", "file") => std::cmp::Ordering::Less,
        ("file", "dir") => std::cmp::Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });
    let entry_count = collected.len();
    let truncated = collected.len() > MAX_ENTRIES;
    collected.truncate(MAX_ENTRIES);
    entries = collected;
    (entries, entry_count, truncated)
}

pub async fn inspect_project(path: &Path) -> ProjectInsight {
    if !path.is_dir() {
        return ProjectInsight::missing();
    }
    let (entries, entry_count, truncated) = list_directory(path);
    let git = git_insight(path).await;
    ProjectInsight {
        exists: true,
        entry_count,
        entries,
        truncated,
        git,
    }
}

async fn git_insight(path: &Path) -> GitInsight {
    let inside = match git_output(path, &["rev-parse", "--is-inside-work-tree"]).await {
        Some(stdout) if stdout.trim() == "true" => true,
        _ => return GitInsight::not_a_repo(),
    };
    if !inside {
        return GitInsight::not_a_repo();
    }

    let status_fut = git_output(path, &["status", "--porcelain=v1", "-b"]);
    let log_fut = git_output(path, &["log", "-1", "--format=%h%x09%s%x09%cI"]);
    let (status, last_commit_raw) = tokio::join!(status_fut, log_fut);

    let (branch, ahead, behind, changed) = status
        .as_deref()
        .map(parse_porcelain_status)
        .unwrap_or((None, 0, 0, 0));
    let last_commit = last_commit_raw.as_deref().and_then(parse_last_commit);

    GitInsight {
        is_repo: true,
        branch,
        dirty: Some(changed > 0),
        ahead: Some(ahead),
        behind: Some(behind),
        changed: Some(changed),
        last_commit,
    }
}

async fn git_output(path: &Path, args: &[&str]) -> Option<String> {
    let mut command = Command::new("git");
    command
        .args(args)
        .current_dir(path)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .stdin(Stdio::null())
        .kill_on_drop(true);
    let output = tokio::time::timeout(GIT_TIMEOUT, command.output())
        .await
        .ok()?
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

fn parse_porcelain_status(output: &str) -> (Option<String>, u32, u32, u32) {
    let mut lines = output.lines();
    let header = lines.next().unwrap_or_default();
    let branch = parse_branch_header(header);
    let (ahead, behind) = parse_ahead_behind(header);
    let changed = lines
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .count() as u32;
    (branch, ahead, behind, changed)
}

fn parse_branch_header(header: &str) -> Option<String> {
    let rest = header.strip_prefix("## ")?;
    let name = rest.split(['.', ' ', '\t']).next().unwrap_or(rest).trim();
    if name.is_empty() || name == "HEAD" {
        return None;
    }
    Some(name.to_string())
}

fn parse_ahead_behind(header: &str) -> (u32, u32) {
    let mut ahead = 0;
    let mut behind = 0;
    if let (Some(start), Some(end)) = (header.find('['), header.find(']')) {
        if start < end {
            if let Some(inner) = header.get(start + 1..end) {
                for part in inner.split(',') {
                    let part = part.trim();
                    if let Some(value) = part.strip_prefix("ahead ") {
                        ahead = value.trim().parse().unwrap_or(0);
                    } else if let Some(value) = part.strip_prefix("behind ") {
                        behind = value.trim().parse().unwrap_or(0);
                    }
                }
            }
        }
    }
    (ahead, behind)
}

fn parse_last_commit(line: &str) -> Option<GitLastCommit> {
    let mut parts = line.trim().splitn(3, '\t');
    let sha = parts.next()?.to_string();
    let subject = parts.next()?.to_string();
    let at = parts.next()?.to_string();
    if sha.is_empty() {
        return None;
    }
    Some(GitLastCommit { sha, subject, at })
}

pub fn is_forbidden_system_path(path: &Path) -> bool {
    let forbidden = [Path::new("/proc"), Path::new("/sys"), Path::new("/dev")];
    forbidden
        .iter()
        .any(|root| path == *root || path.starts_with(root))
}

pub fn path_is_inside(path: &Path, root: &Path) -> bool {
    path == root || path.starts_with(root)
}

pub fn operator_home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn lists_root_entries_and_skips_build_dirs() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();
        std::fs::create_dir(dir.path().join("src")).unwrap();
        std::fs::create_dir(dir.path().join("target")).unwrap();
        std::fs::create_dir(dir.path().join("node_modules")).unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();

        let (entries, count, truncated) = list_directory(dir.path());
        let names: Vec<_> = entries.iter().map(|entry| entry.name.as_str()).collect();
        assert!(names.contains(&"Cargo.toml"));
        assert!(names.contains(&"src"));
        assert!(names.contains(&".git"));
        assert!(!names.contains(&"target"));
        assert!(!names.contains(&"node_modules"));
        assert_eq!(count, 3);
        assert!(!truncated);
        assert_eq!(entries[0].kind, "dir");
    }

    #[test]
    fn truncates_directory_listings() {
        let dir = tempdir().unwrap();
        for index in 0..(MAX_ENTRIES + 5) {
            std::fs::write(dir.path().join(format!("f{index}")), "x").unwrap();
        }
        let (entries, count, truncated) = list_directory(dir.path());
        assert_eq!(entries.len(), MAX_ENTRIES);
        assert_eq!(count, MAX_ENTRIES + 5);
        assert!(truncated);
    }

    #[test]
    fn parses_git_status_and_commit_lines() {
        let (branch, ahead, behind, changed) = parse_porcelain_status(
            "## main...origin/main [ahead 2, behind 1]\n M src/lib.rs\n?? new.rs\n",
        );
        assert_eq!(branch.as_deref(), Some("main"));
        assert_eq!(ahead, 2);
        assert_eq!(behind, 1);
        assert_eq!(changed, 2);

        let commit = parse_last_commit("abc1234\tfix parser\t2026-08-12T18:01:00Z").unwrap();
        assert_eq!(commit.sha, "abc1234");
        assert_eq!(commit.subject, "fix parser");
    }

    #[test]
    fn rejects_system_paths() {
        assert!(is_forbidden_system_path(Path::new("/proc")));
        assert!(is_forbidden_system_path(Path::new("/proc/1")));
        assert!(!is_forbidden_system_path(Path::new("/home/you")));
    }

    #[tokio::test]
    async fn inspects_non_repo_directory() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("README"), "hi").unwrap();
        let insight = inspect_project(dir.path()).await;
        assert!(insight.exists);
        assert!(!insight.git.is_repo);
        assert_eq!(insight.entry_count, 1);
    }

    #[test]
    fn empty_insight_has_default_values() {
        let empty = ProjectInsight::empty();
        assert!(empty.exists);
        assert_eq!(empty.entry_count, 0);
        assert!(empty.entries.is_empty());
        assert!(!empty.truncated);
        assert!(!empty.git.is_repo);
    }
}
