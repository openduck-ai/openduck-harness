use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use anyhow::{anyhow, bail, Result};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::sync::Mutex as AsyncMutex;

use super::files::sanitize_relative_path;
use crate::subprocess::{git_command, SubprocessExt};

static REPO_WRITE_LOCKS: Lazy<std::sync::Mutex<HashMap<PathBuf, Arc<AsyncMutex<()>>>>> =
    Lazy::new(std::sync::Mutex::default);

const STALE_LOCK_AGE: Duration = Duration::from_secs(10);
const LOCK_REMOVE_RETRIES: usize = 10;
const LOCK_RETRY_DELAY: Duration = Duration::from_millis(100);

fn repo_write_mutex(root: &Path) -> Arc<AsyncMutex<()>> {
    let key = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let mut registry = REPO_WRITE_LOCKS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    registry
        .entry(key)
        .or_insert_with(|| Arc::new(AsyncMutex::new(())))
        .clone()
}

async fn with_repo_write_lock<F, T>(root: &Path, operation: F) -> Result<T>
where
    F: Future<Output = Result<T>>,
{
    let mutex = repo_write_mutex(root);
    let _guard = mutex.lock().await;
    operation.await
}

#[cfg(target_os = "linux")]
fn has_live_git_process_in(root: &Path) -> bool {
    let entries = match std::fs::read_dir("/proc") {
        Ok(entries) => entries,
        Err(_) => return false,
    };
    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let Some(pid) = file_name.to_str() else {
            continue;
        };
        if !pid.bytes().all(|byte| byte.is_ascii_digit()) {
            continue;
        }
        let Ok(comm) = std::fs::read_to_string(entry.path().join("comm")) else {
            continue;
        };
        if comm.trim() != "git" {
            continue;
        }
        if process_covers_root(&entry.path(), root) {
            return true;
        }
    }
    false
}

#[cfg(target_os = "linux")]
fn process_covers_root(proc_dir: &Path, root: &Path) -> bool {
    let Ok(cwd) = std::fs::read_link(proc_dir.join("cwd")) else {
        return false;
    };
    cwd == root || cwd.starts_with(root)
}

#[cfg(not(target_os = "linux"))]
fn has_live_git_process_in(_root: &Path) -> bool {
    false
}

struct RepoLayout {
    toplevel: PathBuf,
    index_lock: PathBuf,
}

async fn repo_layout(path: &Path) -> Result<RepoLayout> {
    let stdout = run_git_ok(
        path,
        "git rev-parse",
        &[
            "rev-parse",
            "--is-inside-work-tree",
            "--path-format=absolute",
            "--show-toplevel",
            "--git-path",
            "index.lock",
            "--absolute-git-dir",
        ],
        READ_TIMEOUT,
    )
    .await
    .map_err(|_| anyhow!("Not a git repository"))?;
    parse_repo_layout(&stdout)
}

fn parse_repo_layout(stdout: &str) -> Result<RepoLayout> {
    // git 2.43.0 prints four lines: inside, toplevel, index.lock, git dir.
    // `--path-format=absolute` only changes how later paths are printed.
    let lines: Vec<&str> = stdout.lines().map(str::trim).collect();
    if lines.len() != 4 || lines.iter().any(|line| line.is_empty()) || lines[0] != "true" {
        bail!("Not a git repository");
    }
    let toplevel = PathBuf::from(lines[1]);
    let index_lock = PathBuf::from(lines[2]);
    let git_dir = PathBuf::from(lines[3]);
    if !toplevel.is_absolute() || !index_lock.is_absolute() || !git_dir.is_absolute() {
        bail!("Not a git repository");
    }
    let toplevel = toplevel.canonicalize().unwrap_or(toplevel);
    Ok(RepoLayout {
        toplevel,
        index_lock,
    })
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct LockIdentity {
    len: u64,
    modified: Option<SystemTime>,
    dev_ino: Option<(u64, u64)>,
}

fn lock_identity(path: &Path) -> Option<LockIdentity> {
    let metadata = std::fs::symlink_metadata(path).ok()?;
    Some(identity_from_metadata(&metadata))
}

fn identity_from_metadata(metadata: &std::fs::Metadata) -> LockIdentity {
    LockIdentity {
        len: metadata.len(),
        modified: metadata.modified().ok(),
        dev_ino: device_and_inode(metadata),
    }
}

#[cfg(unix)]
fn device_and_inode(metadata: &std::fs::Metadata) -> Option<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    Some((metadata.dev(), metadata.ino()))
}

#[cfg(not(unix))]
fn device_and_inode(_metadata: &std::fs::Metadata) -> Option<(u64, u64)> {
    None
}

async fn reclaim_stale_index_lock(layout: &RepoLayout) -> Result<()> {
    let metadata = match std::fs::symlink_metadata(&layout.index_lock) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    if !metadata.is_file() {
        bail!(
            "git index lock path is not a regular file: {}",
            layout.index_lock.display()
        );
    }
    let stale = metadata
        .modified()
        .ok()
        .and_then(|modified| modified.elapsed().ok())
        .is_some_and(|age| age >= STALE_LOCK_AGE);
    if stale && !has_live_git_process_in(&layout.toplevel) {
        return remove_observed_index_lock(&layout.index_lock, identity_from_metadata(&metadata))
            .await;
    }
    Err(index_lock_in_progress(&layout.index_lock))
}

fn index_lock_in_progress(path: &Path) -> anyhow::Error {
    anyhow!(
        "Another git operation is in progress in this repository (lock: {}). If no git process is running, delete that file and retry.",
        path.display()
    )
}

async fn remove_observed_index_lock(path: &Path, expected: LockIdentity) -> Result<()> {
    for attempt in 0..LOCK_REMOVE_RETRIES {
        match lock_identity(path) {
            None => return Ok(()),
            Some(current) if current != expected => return Err(index_lock_in_progress(path)),
            Some(_) => {
                if unlink_index_lock(path, attempt).await? {
                    return Ok(());
                }
            }
        }
    }
    Ok(())
}

/// Remove a lock our own child created or replaced.
///
/// `expected_changed_from` is the identity from before that child was spawned
/// (`None` when the lock was absent). A later identity change means another
/// writer took the path, so removal stops and leaves the file.
async fn remove_created_index_lock(
    path: &Path,
    expected_changed_from: Option<LockIdentity>,
) -> Result<()> {
    let mut claimed: Option<LockIdentity> = None;
    for attempt in 0..LOCK_REMOVE_RETRIES {
        let current = lock_identity(path);
        if current.is_none() || current == expected_changed_from {
            return Ok(());
        }
        if let Some(claimed_id) = claimed {
            if current != Some(claimed_id) {
                return Ok(());
            }
        } else {
            claimed = current;
        }
        if unlink_index_lock(path, attempt).await? {
            return Ok(());
        }
    }
    Ok(())
}

async fn unlink_index_lock(path: &Path, attempt: usize) -> Result<bool> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(true),
        Err(_) if attempt + 1 == LOCK_REMOVE_RETRIES => Err(anyhow!(
            "failed to remove stale git lock {}",
            path.display()
        )),
        Err(_) => {
            tokio::time::sleep(LOCK_RETRY_DELAY).await;
            Ok(false)
        }
    }
}

/// Best-effort reclaim when the future around a mutating git child is dropped.
///
/// Dropping that future is the only cancellation signal. Async cleanup would be
/// cancelled with it, and a spawned task could re-enter `with_repo_write_lock`.
/// The synchronous unlink below is the only cleanup that still runs.
struct OwnedLockGuard {
    lock_path: PathBuf,
    before: Option<LockIdentity>,
    armed: bool,
}

impl OwnedLockGuard {
    fn arm(lock_path: PathBuf, before: Option<LockIdentity>) -> Self {
        Self {
            lock_path,
            before,
            armed: true,
        }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for OwnedLockGuard {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let Some(current) = lock_identity(&self.lock_path) else {
            return;
        };
        if Some(current) == self.before {
            return;
        }
        let _ = std::fs::remove_file(&self.lock_path);
    }
}

// Core commit subprocess subtotal = repo discovery READ_TIMEOUT(30s) + ADD_TIMEOUT(60s)
// + COMMIT_TIMEOUT(120s) + post-commit log READ_TIMEOUT(30s) = 240s. Discovery
// replaces the ensure-repo rev-parse and keeps that 30s phase. The Hub client
// allows 300s for /git/commit (ui/hub-core/src/rest.ts), 60s headroom. These
// per-subprocess budgets do not bound repo write mutex waiting or git-side lock waiting.
const ADD_TIMEOUT: Duration = Duration::from_secs(60);
const COMMIT_TIMEOUT: Duration = Duration::from_secs(120);
const MUTATE_TIMEOUT: Duration = Duration::from_secs(90);
const READ_TIMEOUT: Duration = Duration::from_secs(30);
const MESSAGE_TIMEOUT: Duration = Duration::from_secs(90);
const PULL_TIMEOUT: Duration = Duration::from_secs(120);
const PUSH_TIMEOUT: Duration = Duration::from_secs(120);
const MAX_LOG: usize = 500;
const DEFAULT_LOG: usize = 150;
const MAX_DIFF_BYTES: usize = 200 * 1024;
const MAX_MESSAGE_BYTES: usize = 64 * 1024;
const MAX_MODEL_DIFF_CHARS: usize = 12_000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GitFileChange {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_path: Option<String>,
    pub index_status: String,
    pub worktree_status: String,
    pub staged: bool,
    pub unstaged: bool,
    pub untracked: bool,
    pub conflict: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GitStatusResponse {
    pub is_repo: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    pub detached: bool,
    pub ahead: u32,
    pub behind: u32,
    pub files: Vec<GitFileChange>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GitRepositoryEntry {
    pub path: String,
    pub status: GitStatusResponse,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GitRepositoriesResponse {
    pub repositories: Vec<GitRepositoryEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GitCommitEntry {
    pub sha: String,
    pub short_sha: String,
    pub parents: Vec<String>,
    pub author_name: String,
    pub author_email: String,
    pub authored_at: String,
    pub subject: String,
    pub refs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GitWorktreeEntry {
    pub path: String,
    pub head: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    pub is_bare: bool,
    pub is_detached: bool,
    pub is_locked: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lock_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GitWorktreeListResponse {
    pub is_repo: bool,
    pub worktrees: Vec<GitWorktreeEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GitLogResponse {
    pub is_repo: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub head: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    pub detached: bool,
    pub commits: Vec<GitCommitEntry>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GitDiffResponse {
    pub path: String,
    pub staged: bool,
    pub diff: String,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GitShowResponse {
    pub commit: GitCommitEntry,
    pub body: String,
    pub diff: String,
    pub truncated: bool,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct GitLogQuery {
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct GitDiffQuery {
    pub path: Option<String>,
    pub staged: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitShowQuery {
    pub sha: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitPathsRequest {
    #[serde(default)]
    pub paths: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitCommitRequest {
    pub message: String,
    #[serde(default)]
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum GitRepositoryActionType {
    Pull,
    Commit,
    Push,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GitRepositoryActionRequest {
    pub path: String,
    pub action: GitRepositoryActionType,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub paths: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GitRepositoryActionResponse {
    pub success: bool,
    pub action: GitRepositoryActionType,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit: Option<GitCommitResponse>,
    pub status: GitStatusResponse,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GitCommitResponse {
    pub sha: String,
    pub subject: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitCommitMessageRequest {
    #[serde(default)]
    pub paths: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GitCommitMessageResponse {
    pub message: String,
    pub source: String,
}

#[derive(Debug)]
struct GitOutput {
    stdout: Vec<u8>,
    stderr: String,
    success: bool,
    code: Option<i32>,
}

fn git_tokio() -> Command {
    let mut command = Command::from(git_command());
    command.set_no_window();
    command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null())
        .kill_on_drop(true)
        .env("GIT_LITERAL_PATHSPECS", "1")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_COMMON_DIR")
        .env_remove("GIT_INDEX_FILE");
    command
}

async fn read_pipe_to_end(
    pipe: Option<impl tokio::io::AsyncRead + Unpin>,
) -> std::io::Result<Vec<u8>> {
    let Some(mut pipe) = pipe else {
        return Ok(Vec::new());
    };
    let mut buffer = Vec::new();
    pipe.read_to_end(&mut buffer).await?;
    Ok(buffer)
}

async fn reap_timed_out_git(mut child: tokio::process::Child, label: &str) -> anyhow::Error {
    let _ = child.start_kill();
    let _ = child.wait().await;
    anyhow!("{label} timed out")
}

enum GitChildOutcome {
    Finished(GitOutput),
    TimedOut(anyhow::Error),
}

fn finish_git_run(outcome: GitChildOutcome) -> Result<GitOutput> {
    match outcome {
        GitChildOutcome::Finished(output) => Ok(output),
        GitChildOutcome::TimedOut(error) => Err(error),
    }
}

async fn run_git_timed(
    path: &Path,
    label: &str,
    args: &[&str],
    timeout: Duration,
) -> Result<GitChildOutcome> {
    let mut command = git_tokio();
    command.args(args).current_dir(path);
    let mut child = command
        .spawn()
        .map_err(|error| anyhow!("git is not available: {error}"))?;
    if timeout.is_zero() {
        return Ok(GitChildOutcome::TimedOut(
            reap_timed_out_git(child, label).await,
        ));
    }
    let stdout_pipe = child.stdout.take();
    let stderr_pipe = child.stderr.take();
    let drain = async {
        let (stdout, stderr, status) = tokio::join!(
            async { read_pipe_to_end(stdout_pipe).await.unwrap_or_default() },
            async { read_pipe_to_end(stderr_pipe).await.unwrap_or_default() },
            child.wait(),
        );
        let status = status?;
        Ok::<GitOutput, std::io::Error>(GitOutput {
            stdout,
            stderr: String::from_utf8_lossy(&stderr).trim().to_string(),
            success: status.success(),
            code: status.code(),
        })
    };
    match tokio::time::timeout(timeout, drain).await {
        Ok(output) => {
            Ok(GitChildOutcome::Finished(output.map_err(|error| {
                anyhow!("git is not available: {error}")
            })?))
        }
        Err(_) => Ok(GitChildOutcome::TimedOut(
            reap_timed_out_git(child, label).await,
        )),
    }
}

async fn run_git(path: &Path, label: &str, args: &[&str], timeout: Duration) -> Result<GitOutput> {
    finish_git_run(run_git_timed(path, label, args, timeout).await?)
}

async fn run_git_mutating(
    path: &Path,
    lock_path: &Path,
    label: &str,
    args: &[&str],
    timeout: Duration,
) -> Result<GitOutput> {
    let before = lock_identity(lock_path);
    let mut guard = OwnedLockGuard::arm(lock_path.to_path_buf(), before);
    let outcome = run_git_timed(path, label, args, timeout).await;
    guard.disarm();
    let _ = remove_created_index_lock(lock_path, before).await;
    finish_git_run(outcome?)
}

async fn run_git_mutating_ok(
    path: &Path,
    lock_path: &Path,
    label: &str,
    args: &[&str],
    timeout: Duration,
) -> Result<String> {
    let output = run_git_mutating(path, lock_path, label, args, timeout).await?;
    if !output.success {
        if output.stderr.is_empty() {
            bail!("git {} failed", args.first().unwrap_or(&"command"));
        }
        bail!("{}", output.stderr);
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn git_error(output: &GitOutput, fallback: &str) -> anyhow::Error {
    if output.stderr.is_empty() {
        anyhow!("{fallback}")
    } else {
        anyhow!("{}", output.stderr)
    }
}

async fn run_git_ok(path: &Path, label: &str, args: &[&str], timeout: Duration) -> Result<String> {
    let output = run_git(path, label, args, timeout).await?;
    if !output.success {
        if output.stderr.is_empty() {
            bail!("git {} failed", args.first().unwrap_or(&"command"));
        }
        bail!("{}", output.stderr);
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

pub fn validate_git_path(path: &str) -> Result<String> {
    if path.is_empty() {
        bail!("Path is required");
    }
    if path.starts_with('-') {
        bail!("Path must not start with '-'");
    }
    if path.starts_with(':') {
        bail!("Git pathspec magic is not permitted");
    }
    if path.contains('\0') || path.contains('\n') || path.contains('\r') {
        bail!("Path contains invalid characters");
    }
    if path.chars().any(|ch| matches!(ch, '*' | '?' | '[' | ']')) {
        bail!("Glob characters are not permitted in git paths");
    }
    let clean = sanitize_relative_path(path)?;
    let normalized = clean.to_string_lossy().replace('\\', "/");
    if normalized.is_empty() {
        bail!("Path must be relative to the project root");
    }
    if normalized.split('/').next() == Some(".git") {
        bail!("Modifying .git is not permitted");
    }
    Ok(normalized)
}

fn validate_paths(paths: &[String]) -> Result<Vec<String>> {
    if paths.is_empty() {
        bail!("At least one path is required");
    }
    paths.iter().map(|path| validate_git_path(path)).collect()
}

fn validate_sha(sha: &str) -> Result<&str> {
    let sha = sha.trim();
    if sha.len() < 7 || sha.len() > 40 || !sha.chars().all(|ch| ch.is_ascii_hexdigit()) {
        bail!("Invalid commit id");
    }
    Ok(sha)
}

fn validate_commit_message(message: &str) -> Result<String> {
    let trimmed = message.trim().to_string();
    if trimmed.is_empty() {
        bail!("Commit message is required");
    }
    if trimmed.len() > MAX_MESSAGE_BYTES {
        bail!("Commit message is too long");
    }
    Ok(trimmed)
}

async fn ensure_repo(path: &Path) -> Result<()> {
    let inside = run_git_ok(
        path,
        "git rev-parse",
        &["rev-parse", "--is-inside-work-tree"],
        READ_TIMEOUT,
    )
    .await;
    match inside {
        Ok(value) if value.trim() == "true" => Ok(()),
        _ => Err(anyhow!("Not a git repository")),
    }
}

pub fn parse_branch_header(header: &str) -> (Option<String>, bool, u32, u32) {
    let rest = header.strip_prefix("## ").unwrap_or(header).trim();
    let (ahead, behind) = parse_ahead_behind(rest);
    if let Some(name) = rest.strip_prefix("No commits yet on ") {
        let branch = name
            .split(['.', ' ', '\t'])
            .next()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string);
        return (branch, false, ahead, behind);
    }
    if rest.starts_with("HEAD (") || rest.starts_with("HEAD...") || rest == "HEAD" {
        return (None, true, ahead, behind);
    }
    let name = rest.split(['.', ' ', '\t']).next().unwrap_or(rest).trim();
    let branch = if name.is_empty() || name == "HEAD" {
        None
    } else {
        Some(name.to_string())
    };
    let is_detached = branch.is_none();
    (branch, is_detached, ahead, behind)
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

pub fn parse_porcelain_z(output: &[u8]) -> (String, Vec<GitFileChange>) {
    let mut parts = output.split(|byte| *byte == 0);
    let header = parts
        .next()
        .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
        .unwrap_or_default();
    let mut files = Vec::new();
    while let Some(entry) = parts.next() {
        if entry.is_empty() {
            continue;
        }
        let text = String::from_utf8_lossy(entry);
        if text.starts_with('#') {
            continue;
        }
        if text.len() < 3 {
            continue;
        }
        let index_status = text.chars().next().unwrap_or(' ').to_string();
        let worktree_status = text.chars().nth(1).unwrap_or(' ').to_string();
        let first_path = text.get(3..).unwrap_or("").replace('\\', "/");
        let is_rename = index_status == "R"
            || index_status == "C"
            || worktree_status == "R"
            || worktree_status == "C";
        let (path, original_path) = if is_rename {
            let dest = parts
                .next()
                .map(|bytes| String::from_utf8_lossy(bytes).replace('\\', "/"))
                .unwrap_or_default();
            if dest.is_empty() {
                (first_path, None)
            } else {
                (dest, Some(first_path))
            }
        } else {
            (first_path, None)
        };
        if path.is_empty() {
            continue;
        }
        let untracked = index_status == "?" && worktree_status == "?";
        let conflict = index_status == "U" || worktree_status == "U";
        let staged = !untracked && index_status != " ";
        let unstaged = untracked || worktree_status != " ";
        files.push(GitFileChange {
            path,
            original_path,
            index_status,
            worktree_status,
            staged,
            unstaged,
            untracked,
            conflict,
        });
    }
    (header, files)
}

pub fn parse_log_output(output: &str) -> Vec<GitCommitEntry> {
    output
        .split('\n')
        .filter(|line| !line.is_empty())
        .filter_map(parse_log_line)
        .collect()
}

fn parse_log_line(line: &str) -> Option<GitCommitEntry> {
    let mut parts = line.splitn(7, '\u{1f}');
    let sha = parts.next()?.trim().to_string();
    if sha.is_empty() {
        return None;
    }
    let parents = parts
        .next()
        .unwrap_or_default()
        .split_whitespace()
        .map(ToString::to_string)
        .collect();
    let author_name = parts.next().unwrap_or_default().to_string();
    let author_email = parts.next().unwrap_or_default().to_string();
    let authored_at = parts.next().unwrap_or_default().to_string();
    let refs = parse_decorate(parts.next().unwrap_or_default());
    let subject = parts.next().unwrap_or_default().to_string();
    let short_sha = sha.chars().take(7).collect();
    Some(GitCommitEntry {
        sha,
        short_sha,
        parents,
        author_name,
        author_email,
        authored_at,
        subject,
        refs,
    })
}

pub fn parse_decorate(raw: &str) -> Vec<String> {
    let inner = raw
        .trim()
        .trim_start_matches('(')
        .trim_end_matches(')')
        .trim();
    if inner.is_empty() {
        return Vec::new();
    }
    let mut refs = Vec::new();
    for part in inner.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Some(rest) = part.strip_prefix("HEAD -> ") {
            refs.push("HEAD".to_string());
            if !rest.is_empty() {
                refs.push(rest.to_string());
            }
        } else if let Some(tag) = part.strip_prefix("tag: ") {
            refs.push(tag.to_string());
        } else {
            refs.push(part.to_string());
        }
    }
    refs
}

fn truncate_diff(raw: String) -> (String, bool) {
    if raw.len() <= MAX_DIFF_BYTES {
        return (raw, false);
    }
    let mut end = MAX_DIFF_BYTES;
    while end > 0 && !raw.is_char_boundary(end) {
        end -= 1;
    }
    match raw.get(..end) {
        Some(slice) => (slice.to_string(), true),
        None => (raw, false),
    }
}

pub async fn status(path: &Path) -> Result<GitStatusResponse> {
    ensure_repo(path).await?;
    let output = run_git(
        path,
        "git status",
        &["status", "--porcelain=v1", "-z", "-b"],
        READ_TIMEOUT,
    )
    .await?;
    if !output.success {
        bail!(
            "{}",
            if output.stderr.is_empty() {
                "git status failed".to_string()
            } else {
                output.stderr
            }
        );
    }
    let (header, files) = parse_porcelain_z(&output.stdout);
    let (branch, detached, ahead, behind) = parse_branch_header(&header);
    Ok(GitStatusResponse {
        is_repo: true,
        branch,
        detached,
        ahead,
        behind,
        files,
    })
}

fn path_has_git_component(path: &Path) -> bool {
    path.components().any(|c| c.as_os_str() == ".git")
}

fn looks_like_git_root(dir: &Path) -> bool {
    if path_has_git_component(dir) {
        return false;
    }
    let git_path = dir.join(".git");
    let Ok(meta) = std::fs::symlink_metadata(&git_path) else {
        return false;
    };
    if meta.file_type().is_symlink() {
        return false;
    }
    if meta.is_dir() {
        dir.join(".git").join("HEAD").exists()
    } else if meta.file_type().is_file() {
        std::fs::read_to_string(&git_path)
            .map(|content| content.trim_start().starts_with("gitdir:"))
            .unwrap_or(false)
    } else {
        false
    }
}

async fn is_actual_repo_root(candidate: &Path) -> bool {
    let Ok(layout) = repo_layout(candidate).await else {
        return false;
    };
    let Ok(candidate_canonical) = candidate.canonicalize() else {
        return false;
    };
    candidate_canonical == layout.toplevel
}

fn collect_candidate_roots(root: &Path) -> Vec<PathBuf> {
    if path_has_git_component(root) {
        return Vec::new();
    }
    let Ok(root_meta) = std::fs::symlink_metadata(root) else {
        return Vec::new();
    };
    if root_meta.file_type().is_symlink() || !root_meta.is_dir() {
        return Vec::new();
    }

    let mut candidates = Vec::new();
    let mut queue = std::collections::VecDeque::new();
    queue.push_back(root.to_path_buf());

    while let Some(dir) = queue.pop_front() {
        if path_has_git_component(&dir) {
            continue;
        }
        let Ok(meta) = std::fs::symlink_metadata(&dir) else {
            continue;
        };
        if meta.file_type().is_symlink() || !meta.is_dir() {
            continue;
        }

        if looks_like_git_root(&dir) {
            candidates.push(dir.clone());
        }

        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };

        for entry in entries.flatten() {
            let file_name = entry.file_name();
            if file_name == ".git" {
                continue;
            }
            let entry_path = entry.path();
            let Ok(entry_meta) = std::fs::symlink_metadata(&entry_path) else {
                continue;
            };
            if entry_meta.file_type().is_symlink() {
                continue;
            }
            if entry_meta.is_dir() {
                queue.push_back(entry_path);
            }
        }
    }

    candidates
}

pub async fn discover_repositories(root: &Path) -> Result<GitRepositoriesResponse> {
    if path_has_git_component(root) {
        return Ok(GitRepositoriesResponse {
            repositories: Vec::new(),
        });
    }

    let candidates = collect_candidate_roots(root);
    let mut repositories = Vec::new();
    let root_canonical = root.canonicalize().ok();

    for candidate in candidates {
        if !is_actual_repo_root(&candidate).await {
            continue;
        }

        match status(&candidate).await {
            Ok(repo_status) => {
                let candidate_canonical = candidate.canonicalize().ok();
                let is_root = candidate == root
                    || (root_canonical.is_some() && root_canonical == candidate_canonical);

                let rel_path = if is_root {
                    ".".to_string()
                } else {
                    let rel = match candidate.strip_prefix(root) {
                        Ok(p) => p.to_path_buf(),
                        Err(_) => {
                            if let (Some(rc), Some(cc)) = (&root_canonical, &candidate_canonical) {
                                cc.strip_prefix(rc)
                                    .map(Path::to_path_buf)
                                    .unwrap_or(candidate.clone())
                            } else {
                                candidate.clone()
                            }
                        }
                    };
                    let s = rel.to_string_lossy().replace('\\', "/");
                    let clean = s.trim_start_matches("./").trim_matches('/');
                    if clean.is_empty() || clean == "." {
                        ".".to_string()
                    } else {
                        clean.to_string()
                    }
                };

                repositories.push(GitRepositoryEntry {
                    path: rel_path,
                    status: repo_status,
                });
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to get git status for repository candidate {}: {e}",
                    candidate.display()
                );
            }
        }
    }

    repositories.sort_by(|a, b| {
        if a.path == "." && b.path != "." {
            std::cmp::Ordering::Less
        } else if a.path != "." && b.path == "." {
            std::cmp::Ordering::Greater
        } else {
            a.path.cmp(&b.path)
        }
    });

    Ok(GitRepositoriesResponse { repositories })
}

fn validate_and_resolve_repo_path(project_root: &Path, rel_path: &str) -> Result<PathBuf> {
    if rel_path.contains('\0') {
        bail!("Invalid path containing null bytes");
    }
    let p = Path::new(rel_path);
    if p.is_absolute() {
        bail!("Absolute paths are not allowed");
    }
    for component in p.components() {
        match component {
            std::path::Component::ParentDir => bail!("Path traversal (..) is not allowed"),
            std::path::Component::RootDir | std::path::Component::Prefix(_) => {
                bail!("Absolute paths are not allowed")
            }
            _ => {}
        }
    }
    if path_has_git_component(p) {
        bail!("Access to git internal directories (.git) is not allowed");
    }

    let clean = rel_path.replace('\\', "/");
    let trimmed = clean.trim_start_matches("./").trim_matches('/');
    let target = if trimmed.is_empty() || trimmed == "." {
        project_root.to_path_buf()
    } else {
        project_root.join(trimmed)
    };

    let target_canonical = target
        .canonicalize()
        .map_err(|_| anyhow!("Repository path does not exist: {rel_path}"))?;

    let root_canonical = project_root
        .canonicalize()
        .map_err(|_| anyhow!("Project root path does not exist"))?;

    if !target_canonical.starts_with(&root_canonical) {
        bail!("Repository path is outside project root");
    }

    Ok(target_canonical)
}

pub async fn resolve_discovered_repo(project_root: &Path, rel_path: &str) -> Result<PathBuf> {
    let target_canonical = validate_and_resolve_repo_path(project_root, rel_path)?;

    if !is_actual_repo_root(&target_canonical).await {
        bail!("Path is not a git repository root: {rel_path}");
    }

    let discovered = discover_repositories(project_root).await?;
    let mut is_discovered = false;
    for entry in &discovered.repositories {
        let entry_dir = if entry.path == "." {
            project_root.to_path_buf()
        } else {
            project_root.join(&entry.path)
        };
        if let Ok(entry_canon) = entry_dir.canonicalize() {
            if entry_canon == target_canonical {
                is_discovered = true;
                break;
            }
        }
    }

    if !is_discovered {
        bail!("Repository at '{rel_path}' is not among discovered repositories in project");
    }

    Ok(target_canonical)
}

async fn resolve_default_remote(
    repo_root: &Path,
    branch: &str,
    remotes: &[&str],
) -> Option<String> {
    let branch_push_remote_key = format!("branch.{}.pushRemote", branch);
    if let Ok(val) = run_git_ok(
        repo_root,
        "git config",
        &["config", "--get", &branch_push_remote_key],
        READ_TIMEOUT,
    )
    .await
    {
        let trimmed = val.trim();
        if !trimmed.is_empty() && remotes.contains(&trimmed) {
            return Some(trimmed.to_string());
        }
    }

    if let Ok(val) = run_git_ok(
        repo_root,
        "git config",
        &["config", "--get", "remote.pushDefault"],
        READ_TIMEOUT,
    )
    .await
    {
        let trimmed = val.trim();
        if !trimmed.is_empty() && remotes.contains(&trimmed) {
            return Some(trimmed.to_string());
        }
    }

    let branch_remote_key = format!("branch.{}.remote", branch);
    if let Ok(val) = run_git_ok(
        repo_root,
        "git config",
        &["config", "--get", &branch_remote_key],
        READ_TIMEOUT,
    )
    .await
    {
        let trimmed = val.trim();
        if !trimmed.is_empty() && remotes.contains(&trimmed) {
            return Some(trimmed.to_string());
        }
    }

    if remotes.contains(&"origin") {
        return Some("origin".to_string());
    }

    if remotes.len() == 1 {
        return Some(remotes[0].to_string());
    }

    None
}

async fn determine_push_args(repo_root: &Path, remotes: &[&str]) -> Vec<String> {
    let has_upstream = run_git_ok(
        repo_root,
        "git rev-parse",
        &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"],
        READ_TIMEOUT,
    )
    .await
    .is_ok();

    if has_upstream {
        return vec!["push".to_string()];
    }

    let branch_out = run_git_ok(
        repo_root,
        "git rev-parse",
        &["rev-parse", "--abbrev-ref", "HEAD"],
        READ_TIMEOUT,
    )
    .await
    .ok();

    let branch = branch_out.as_deref().map(str::trim).unwrap_or("");
    if branch.is_empty() || branch == "HEAD" || branch.starts_with('-') {
        return vec!["push".to_string()];
    }

    if let Some(remote) = resolve_default_remote(repo_root, branch, remotes).await {
        if !remote.is_empty() && !remote.starts_with('-') {
            return vec![
                "push".to_string(),
                "--set-upstream".to_string(),
                remote,
                branch.to_string(),
            ];
        }
    }

    vec!["push".to_string()]
}

pub async fn repository_action(
    project_root: &Path,
    req: GitRepositoryActionRequest,
) -> Result<GitRepositoryActionResponse> {
    let repo_root = resolve_discovered_repo(project_root, &req.path).await?;
    with_repo_write_lock(&repo_root, async {
        repository_action_locked(&repo_root, req).await
    })
    .await
}

async fn repository_action_locked(
    repo_root: &Path,
    req: GitRepositoryActionRequest,
) -> Result<GitRepositoryActionResponse> {
    let layout = repo_layout(repo_root).await?;
    reclaim_stale_index_lock(&layout).await?;

    match req.action {
        GitRepositoryActionType::Commit => {
            let message = match &req.message {
                Some(msg) => validate_commit_message(msg)?,
                None => bail!("Commit message is required"),
            };

            let current_status = status(repo_root).await?;
            let paths = req.paths.as_deref().unwrap_or(&[]);
            if current_status.files.is_empty() {
                bail!("Nothing to commit: working tree is clean");
            }
            if paths.is_empty() && !current_status.files.iter().any(|f| f.staged) {
                bail!(
                    "No changes added to commit (stage changes first or specify files to commit)"
                );
            }

            let commit_resp = commit_locked(repo_root, &message, paths).await?;
            let status_after = status(repo_root).await?;
            Ok(GitRepositoryActionResponse {
                success: true,
                action: req.action,
                path: req.path,
                message: Some(commit_resp.subject.clone()),
                commit: Some(commit_resp),
                status: status_after,
            })
        }
        GitRepositoryActionType::Pull => {
            let remotes_out =
                run_git_ok(repo_root, "git remote", &["remote"], READ_TIMEOUT).await?;
            if remotes_out.trim().is_empty() {
                bail!("No remote repository configured");
            }

            let output = run_git_mutating(
                repo_root,
                &layout.index_lock,
                "git pull",
                &["pull"],
                PULL_TIMEOUT,
            )
            .await?;

            if !output.success {
                let stderr = output.stderr.trim();
                let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                let err_msg = if !stderr.is_empty() {
                    stderr.to_string()
                } else if !stdout.is_empty() {
                    stdout
                } else {
                    "git pull failed".to_string()
                };
                bail!("{err_msg}");
            }

            let stdout_msg = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let msg = if stdout_msg.is_empty() {
                if !output.stderr.trim().is_empty() {
                    output.stderr.trim().to_string()
                } else {
                    "Pull completed successfully".to_string()
                }
            } else {
                stdout_msg
            };

            let status_after = status(repo_root).await?;
            Ok(GitRepositoryActionResponse {
                success: true,
                action: req.action,
                path: req.path,
                message: Some(msg),
                commit: None,
                status: status_after,
            })
        }
        GitRepositoryActionType::Push => {
            let remotes_out =
                run_git_ok(repo_root, "git remote", &["remote"], READ_TIMEOUT).await?;
            let remotes: Vec<&str> = remotes_out
                .lines()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .collect();
            if remotes.is_empty() {
                bail!("No remote repository configured");
            }

            let push_cmd_args = determine_push_args(repo_root, &remotes).await;
            let push_cmd_refs: Vec<&str> = push_cmd_args.iter().map(String::as_str).collect();

            let output = run_git_mutating(
                repo_root,
                &layout.index_lock,
                "git push",
                &push_cmd_refs,
                PUSH_TIMEOUT,
            )
            .await?;

            if !output.success {
                let stderr = output.stderr.trim();
                let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                let err_msg = if !stderr.is_empty() {
                    stderr.to_string()
                } else if !stdout.is_empty() {
                    stdout
                } else {
                    "git push failed".to_string()
                };
                bail!("{err_msg}");
            }

            let stdout_msg = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let msg = if stdout_msg.is_empty() {
                if !output.stderr.trim().is_empty() {
                    output.stderr.trim().to_string()
                } else {
                    "Push completed successfully".to_string()
                }
            } else {
                stdout_msg
            };

            let status_after = status(repo_root).await?;
            Ok(GitRepositoryActionResponse {
                success: true,
                action: req.action,
                path: req.path,
                message: Some(msg),
                commit: None,
                status: status_after,
            })
        }
    }
}

pub async fn log(path: &Path, limit: Option<usize>) -> Result<GitLogResponse> {
    ensure_repo(path).await?;
    let limit = limit.unwrap_or(DEFAULT_LOG).clamp(1, MAX_LOG);
    let head = run_git_ok(path, "git rev-parse", &["rev-parse", "HEAD"], READ_TIMEOUT)
        .await
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty() && value.chars().all(|ch| ch.is_ascii_hexdigit()));
    let branch_name = run_git_ok(
        path,
        "git rev-parse",
        &["rev-parse", "--abbrev-ref", "HEAD"],
        READ_TIMEOUT,
    )
    .await
    .ok()
    .map(|value| value.trim().to_string());
    let detached = branch_name.as_deref() == Some("HEAD");
    let branch = branch_name.filter(|name| name != "HEAD");
    if head.is_none() {
        return Ok(GitLogResponse {
            is_repo: true,
            head: None,
            branch,
            detached,
            commits: Vec::new(),
            truncated: false,
        });
    }
    let format = "%H%x1f%P%x1f%an%x1f%ae%x1f%cI%x1f%d%x1f%s";
    let max_count = format!("--max-count={limit}");
    let pretty = format!("--pretty=format:{format}");
    let stdout = run_git_ok(
        path,
        "git log",
        &[
            "log",
            "--all",
            "--decorate=short",
            "--date-order",
            &max_count,
            &pretty,
        ],
        READ_TIMEOUT,
    )
    .await?;
    let commits = parse_log_output(&stdout);
    let truncated = commits.len() >= limit;
    Ok(GitLogResponse {
        is_repo: true,
        head,
        branch,
        detached,
        commits,
        truncated,
    })
}

fn synthesize_untracked_diff(rel_path: &str, contents: &str) -> String {
    let mut diff = format!(
        "diff --git a/{rel_path} b/{rel_path}\nnew file mode 100644\n--- /dev/null\n+++ b/{rel_path}\n"
    );
    if contents.is_empty() {
        return diff;
    }
    let line_count = contents.lines().count().max(1);
    diff.push_str(&format!("@@ -0,0 +1,{line_count} @@\n"));
    for line in contents.lines() {
        diff.push('+');
        diff.push_str(line);
        diff.push('\n');
    }
    diff
}

pub async fn diff(path: &Path, rel_path: &str, staged: bool) -> Result<GitDiffResponse> {
    ensure_repo(path).await?;
    let rel_path = validate_git_path(rel_path)?;
    let current = status(path).await?;
    let change = current.files.iter().find(|file| file.path == rel_path);
    if !staged && change.is_some_and(|file| file.untracked) {
        let contents = tokio::fs::read_to_string(path.join(&rel_path))
            .await
            .unwrap_or_default();
        let (diff, truncated) = truncate_diff(synthesize_untracked_diff(&rel_path, &contents));
        return Ok(GitDiffResponse {
            path: rel_path,
            staged,
            diff,
            truncated,
        });
    }
    let mut args = vec!["diff", "--no-color", "--no-ext-diff"];
    if staged {
        args.push("--cached");
    }
    args.push("--");
    args.push(&rel_path);
    let output = run_git(path, "git diff", &args, READ_TIMEOUT).await?;
    if !output.success && output.code != Some(1) {
        return Err(git_error(&output, "git diff failed"));
    }
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let (diff, truncated) = truncate_diff(stdout);
    Ok(GitDiffResponse {
        path: rel_path,
        staged,
        diff,
        truncated,
    })
}

pub async fn show(path: &Path, sha: &str) -> Result<GitShowResponse> {
    ensure_repo(path).await?;
    let sha = validate_sha(sha)?;
    let format = "%H%x1f%P%x1f%an%x1f%ae%x1f%cI%x1f%d%x1f%s%x1f%b";
    let stdout = run_git_ok(
        path,
        "git log",
        &[
            "log",
            "-1",
            "--decorate=short",
            &format!("--pretty=format:{format}"),
            sha,
        ],
        READ_TIMEOUT,
    )
    .await?;
    let mut parts = stdout.splitn(8, '\u{1f}');
    let commit_line = format!(
        "{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}",
        parts.next().unwrap_or_default(),
        parts.next().unwrap_or_default(),
        parts.next().unwrap_or_default(),
        parts.next().unwrap_or_default(),
        parts.next().unwrap_or_default(),
        parts.next().unwrap_or_default(),
        parts.next().unwrap_or_default(),
    );
    let commit = parse_log_line(&commit_line).ok_or_else(|| anyhow!("Commit not found"))?;
    let body = parts.next().unwrap_or_default().trim().to_string();
    let diff_output = run_git(
        path,
        "git show",
        &["show", "--no-color", "--no-ext-diff", "--format=", sha],
        READ_TIMEOUT,
    )
    .await?;
    if !diff_output.success && diff_output.code != Some(1) {
        return Err(git_error(&diff_output, "git show failed"));
    }
    let (diff, truncated) =
        truncate_diff(String::from_utf8_lossy(&diff_output.stdout).into_owned());
    Ok(GitShowResponse {
        commit,
        body,
        diff,
        truncated,
    })
}

pub async fn stage(path: &Path, paths: &[String]) -> Result<GitStatusResponse> {
    with_repo_write_lock(path, async { stage_locked(path, paths).await }).await
}

async fn stage_locked(path: &Path, paths: &[String]) -> Result<GitStatusResponse> {
    let layout = repo_layout(path).await?;
    reclaim_stale_index_lock(&layout).await?;
    let paths = validate_paths(paths)?;
    let mut args = vec!["add".to_string(), "--".to_string()];
    args.extend(paths);
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    run_git_mutating_ok(
        path,
        &layout.index_lock,
        "git add",
        &arg_refs,
        MUTATE_TIMEOUT,
    )
    .await?;
    status(path).await
}

pub async fn unstage(path: &Path, paths: &[String]) -> Result<GitStatusResponse> {
    with_repo_write_lock(path, async { unstage_locked(path, paths).await }).await
}

async fn unstage_locked(path: &Path, paths: &[String]) -> Result<GitStatusResponse> {
    let layout = repo_layout(path).await?;
    reclaim_stale_index_lock(&layout).await?;
    let paths = validate_paths(paths)?;
    let current = status(path).await?;
    let staged: Vec<String> = paths
        .into_iter()
        .filter(|rel| {
            current
                .files
                .iter()
                .any(|file| file.path == *rel && file.staged)
        })
        .collect();
    if staged.is_empty() {
        return Ok(current);
    }
    let mut args = vec![
        "restore".to_string(),
        "--staged".to_string(),
        "--".to_string(),
    ];
    args.extend(staged);
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    run_git_mutating_ok(
        path,
        &layout.index_lock,
        "git restore",
        &arg_refs,
        MUTATE_TIMEOUT,
    )
    .await?;
    status(path).await
}

pub async fn discard(path: &Path, paths: &[String]) -> Result<GitStatusResponse> {
    with_repo_write_lock(path, async { discard_locked(path, paths).await }).await
}

async fn discard_locked(path: &Path, paths: &[String]) -> Result<GitStatusResponse> {
    let layout = repo_layout(path).await?;
    reclaim_stale_index_lock(&layout).await?;
    let paths = validate_paths(paths)?;
    let current = status(path).await?;
    for rel in &paths {
        let change = current.files.iter().find(|file| file.path == *rel);
        discard_one(path, &layout, rel, change).await?;
    }
    status(path).await
}

async fn discard_one(
    root: &Path,
    layout: &RepoLayout,
    rel: &str,
    change: Option<&GitFileChange>,
) -> Result<()> {
    let untracked = change.map(|file| file.untracked).unwrap_or(false);
    let added = change.is_some_and(|file| file.index_status == "A");
    if untracked {
        let target = root.join(rel);
        let mut args = vec!["clean".to_string(), "-f".to_string()];
        if target.is_dir() {
            args.push("-d".to_string());
        }
        args.push("--".to_string());
        args.push(rel.to_string());
        let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
        run_git_mutating_ok(
            root,
            &layout.index_lock,
            "git clean",
            &arg_refs,
            MUTATE_TIMEOUT,
        )
        .await?;
        return Ok(());
    }
    if added {
        run_git_mutating_ok(
            root,
            &layout.index_lock,
            "git restore",
            &["restore", "--staged", "--", rel],
            MUTATE_TIMEOUT,
        )
        .await?;
        if root.join(rel).exists() {
            run_git_mutating_ok(
                root,
                &layout.index_lock,
                "git clean",
                &["clean", "-f", "--", rel],
                MUTATE_TIMEOUT,
            )
            .await?;
        }
        return Ok(());
    }
    let mut args = vec![
        "restore".to_string(),
        "--source=HEAD".to_string(),
        "--staged".to_string(),
        "--worktree".to_string(),
        "--".to_string(),
        rel.to_string(),
    ];
    if let Some(original) = change.and_then(|file| file.original_path.as_deref()) {
        args.push(original.to_string());
    }
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    run_git_mutating_ok(
        root,
        &layout.index_lock,
        "git restore",
        &arg_refs,
        MUTATE_TIMEOUT,
    )
    .await?;
    Ok(())
}

pub async fn commit(path: &Path, message: &str, paths: &[String]) -> Result<GitCommitResponse> {
    with_repo_write_lock(path, async { commit_locked(path, message, paths).await }).await
}

async fn commit_locked(path: &Path, message: &str, paths: &[String]) -> Result<GitCommitResponse> {
    let layout = repo_layout(path).await?;
    reclaim_stale_index_lock(&layout).await?;
    let message = validate_commit_message(message)?;
    let commit_paths = if paths.is_empty() {
        Vec::new()
    } else {
        validate_paths(paths)?
    };
    if !commit_paths.is_empty() {
        let mut args = vec!["add".to_string(), "--".to_string()];
        args.extend(commit_paths.iter().cloned());
        let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
        run_git_mutating_ok(path, &layout.index_lock, "git add", &arg_refs, ADD_TIMEOUT).await?;
    }
    let file = tempfile::NamedTempFile::new()?;
    std::fs::write(file.path(), message.as_bytes())?;
    let file_path = file.path().to_string_lossy().into_owned();
    let mut args = vec![
        "commit".to_string(),
        "--no-status".to_string(),
        "-F".to_string(),
        file_path,
    ];
    if !commit_paths.is_empty() {
        args.push("--".to_string());
        args.extend(commit_paths);
    }
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let output = run_git_mutating(
        path,
        &layout.index_lock,
        "git commit",
        &arg_refs,
        COMMIT_TIMEOUT,
    )
    .await?;
    drop(file);
    if !output.success {
        if output.stderr.is_empty() {
            bail!("git commit failed");
        }
        bail!("{}", output.stderr);
    }
    let line = run_git_ok(
        path,
        "git log",
        &["log", "-1", "--format=%H%x1f%s"],
        READ_TIMEOUT,
    )
    .await;
    Ok(commit_response_from_log(line, &message))
}

fn commit_response_from_log(line: Result<String>, message: &str) -> GitCommitResponse {
    if let Ok(line) = line {
        if let Some((sha, subject)) = line.trim().split_once('\u{1f}') {
            if !sha.is_empty() && !subject.is_empty() {
                return GitCommitResponse {
                    sha: sha.to_string(),
                    subject: subject.to_string(),
                };
            }
        }
    }
    GitCommitResponse {
        sha: String::new(),
        subject: message.lines().next().unwrap_or_default().to_string(),
    }
}

pub fn heuristic_commit_message(files: &[GitFileChange]) -> String {
    if files.is_empty() {
        return "Update files".to_string();
    }
    let names: Vec<&str> = files
        .iter()
        .map(|file| {
            Path::new(&file.path)
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or(file.path.as_str())
        })
        .collect();
    let added = files
        .iter()
        .filter(|file| file.untracked || file.index_status == "A")
        .count();
    let deleted = files
        .iter()
        .filter(|file| file.index_status == "D" || file.worktree_status == "D")
        .count();
    let verb = if added == files.len() {
        "Add"
    } else if deleted == files.len() {
        "Delete"
    } else {
        "Update"
    };
    if names.len() == 1 {
        format!("{verb} {}", names[0])
    } else if names.len() == 2 {
        format!("{verb} {} and {}", names[0], names[1])
    } else {
        format!("{verb} {} and {} other files", names[0], names.len() - 1)
    }
}

fn clean_generated_message(text: &str) -> String {
    let mut value = text.trim().to_string();
    if let Some(rest) = value.strip_prefix("```") {
        let rest = rest
            .strip_prefix("text")
            .or_else(|| rest.strip_prefix("markdown"))
            .unwrap_or(rest)
            .trim_start_matches('\n');
        value = rest.to_string();
        if let Some(index) = value.rfind("```") {
            if let Some(stripped) = value.get(..index) {
                value = stripped.to_string();
            }
        }
    }
    value.trim().to_string()
}

async fn git_diff_text(path: &Path, args: &[&str]) -> Result<String> {
    let output = run_git(path, "git diff", args, READ_TIMEOUT).await?;
    if !output.success && output.code != Some(1) {
        return Err(git_error(&output, "git diff failed"));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

async fn collect_diff_for_message(path: &Path, files: &[GitFileChange]) -> Result<String> {
    let selected: Vec<&str> = files.iter().map(|file| file.path.as_str()).collect();
    let staged = files.iter().any(|file| file.staged && !file.untracked);
    let mut args = vec![
        "diff".to_string(),
        "--no-color".to_string(),
        "--no-ext-diff".to_string(),
        "--stat".to_string(),
    ];
    if staged {
        args.push("--cached".to_string());
    }
    args.push("--".to_string());
    args.extend(selected.iter().map(|value| (*value).to_string()));
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let stat = git_diff_text(path, &arg_refs).await.unwrap_or_default();

    let mut patch_args = vec![
        "diff".to_string(),
        "--no-color".to_string(),
        "--no-ext-diff".to_string(),
    ];
    if staged {
        patch_args.push("--cached".to_string());
    }
    patch_args.push("--".to_string());
    patch_args.extend(files.iter().map(|file| file.path.clone()));
    let patch_refs: Vec<&str> = patch_args.iter().map(String::as_str).collect();
    let patch = git_diff_text(path, &patch_refs).await.unwrap_or_default();
    let mut combined = format!("{stat}\n{patch}");
    if combined.chars().count() > MAX_MODEL_DIFF_CHARS {
        combined = combined.chars().take(MAX_MODEL_DIFF_CHARS).collect();
        combined.push_str("\n…");
    }
    Ok(combined)
}

fn files_for_message<'a>(
    status: &'a GitStatusResponse,
    paths: &[String],
) -> Result<Vec<&'a GitFileChange>> {
    if paths.is_empty() {
        let staged: Vec<&GitFileChange> = status
            .files
            .iter()
            .filter(|file| file.staged && !file.untracked)
            .collect();
        if !staged.is_empty() {
            return Ok(staged);
        }
        return Ok(status.files.iter().collect());
    }
    let wanted: Vec<String> = paths
        .iter()
        .map(|path| validate_git_path(path))
        .collect::<Result<Vec<_>>>()?;
    let selected: Vec<&GitFileChange> = status
        .files
        .iter()
        .filter(|file| wanted.iter().any(|path| path == &file.path))
        .collect();
    if selected.is_empty() {
        bail!("None of the selected paths have changes");
    }
    Ok(selected)
}

async fn generate_with_model(diff: &str) -> Result<String> {
    use crate::config::Config;
    use crate::conversation::message::{Message, MessageContent};
    use crate::model_config::model_config_from_user_config;

    let config = Config::global();
    let provider_name = config.get_goose_provider()?;
    let model_name = config.get_goose_model()?;
    let model_config = model_config_from_user_config(&provider_name, &model_name)?;
    let provider = crate::providers::create(&provider_name, vec![]).await?;
    let system = "You write concise git commit messages. Prefer conventional commits (feat, fix, docs, refactor, test, chore) when they fit. First line at most 72 characters. Optional body after a blank line. Output only the commit message.";
    let prompt = format!("Write a commit message for this diff:\n\n{diff}");
    let messages = vec![Message::user().with_text(prompt)];
    let tools: [rmcp::model::Tool; 0] = [];
    let (reply, _) = tokio::time::timeout(
        MESSAGE_TIMEOUT,
        provider.complete(&model_config, system, &messages, &tools),
    )
    .await
    .map_err(|_| anyhow!("commit message generation timed out"))??;
    let text = reply
        .content
        .iter()
        .filter_map(|block| match block {
            MessageContent::Text(raw) if !raw.text.is_empty() => Some(raw.text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    let cleaned = clean_generated_message(&text);
    if cleaned.is_empty() {
        bail!("model returned an empty commit message");
    }
    Ok(cleaned)
}

pub async fn suggest_commit_message(
    path: &Path,
    paths: &[String],
) -> Result<GitCommitMessageResponse> {
    ensure_repo(path).await?;
    let current = status(path).await?;
    let selected = files_for_message(&current, paths)?;
    let owned: Vec<GitFileChange> = selected.into_iter().cloned().collect();
    let heuristic = heuristic_commit_message(&owned);
    let diff = collect_diff_for_message(path, &owned)
        .await
        .unwrap_or_default();
    if diff.trim().is_empty() {
        return Ok(GitCommitMessageResponse {
            message: heuristic,
            source: "heuristic".to_string(),
        });
    }
    match generate_with_model(&diff).await {
        Ok(message) => Ok(GitCommitMessageResponse {
            message,
            source: "model".to_string(),
        }),
        Err(_) => Ok(GitCommitMessageResponse {
            message: heuristic,
            source: "heuristic".to_string(),
        }),
    }
}

pub struct TaskAutoCommit {
    root: std::path::PathBuf,
    initially_dirty: std::collections::HashSet<String>,
    excluded: Vec<std::path::PathBuf>,
}

impl TaskAutoCommit {
    async fn changes(root: &Path) -> Result<Vec<GitFileChange>> {
        let output = run_git_ok(
            root,
            "git status",
            &[
                "status",
                "--porcelain=v1",
                "-z",
                "-b",
                "--untracked-files=all",
            ],
            READ_TIMEOUT,
        )
        .await?;
        Ok(parse_porcelain_z(output.as_bytes()).1)
    }

    pub async fn capture(
        workspace: &Path,
        excluded: Vec<std::path::PathBuf>,
    ) -> Result<Option<Self>> {
        if ensure_repo(workspace).await.is_err() {
            return Ok(None);
        }
        let root = run_git_ok(
            workspace,
            "git rev-parse",
            &["rev-parse", "--show-toplevel"],
            READ_TIMEOUT,
        )
        .await?;
        let root = std::path::PathBuf::from(root.trim());
        let current = Self::changes(&root).await?;
        let initially_dirty = current
            .into_iter()
            .flat_map(|file| std::iter::once(file.path).chain(file.original_path))
            .collect();
        Ok(Some(Self {
            root,
            initially_dirty,
            excluded,
        }))
    }

    pub async fn finish(self) -> Result<Option<GitCommitResponse>> {
        let current = Self::changes(&self.root).await?;
        let owned: Vec<GitFileChange> = current
            .into_iter()
            .filter(|file| {
                !file.conflict
                    && !self.initially_dirty.contains(&file.path)
                    && !file
                        .original_path
                        .as_ref()
                        .is_some_and(|path| self.initially_dirty.contains(path))
                    && !self
                        .excluded
                        .iter()
                        .any(|path| self.root.join(&file.path).starts_with(path))
            })
            .collect();
        let paths: Vec<String> = owned
            .iter()
            .flat_map(|file| std::iter::once(file.path.clone()).chain(file.original_path.clone()))
            .collect();
        if paths.is_empty() {
            return Ok(None);
        }
        let diff = collect_diff_for_message(&self.root, &owned).await?;
        let generated = if diff.trim().is_empty() {
            heuristic_commit_message(&owned)
        } else {
            generate_with_model(&diff)
                .await
                .unwrap_or_else(|_| heuristic_commit_message(&owned))
        };
        let message = format!("{}\n\nGenerated-by: OpenDuck Harness", generated.trim());
        commit(&self.root, &message, &paths).await.map(Some)
    }
}

pub fn parse_worktree_porcelain(raw: &str) -> Vec<GitWorktreeEntry> {
    let mut entries = Vec::new();
    let mut current_path: Option<String> = None;
    let mut current_head = String::new();
    let mut current_branch: Option<String> = None;
    let mut is_bare = false;
    let mut is_detached = false;
    let mut is_locked = false;
    let mut lock_reason: Option<String> = None;

    let flush = |entries: &mut Vec<GitWorktreeEntry>,
                 path: &mut Option<String>,
                 head: &mut String,
                 branch: &mut Option<String>,
                 bare: &mut bool,
                 detached: &mut bool,
                 locked: &mut bool,
                 lock_msg: &mut Option<String>| {
        if let Some(p) = path.take() {
            entries.push(GitWorktreeEntry {
                path: p,
                head: std::mem::take(head),
                branch: branch.take(),
                is_bare: *bare,
                is_detached: *detached,
                is_locked: *locked,
                lock_reason: lock_msg.take(),
            });
            *bare = false;
            *detached = false;
            *locked = false;
        }
    };

    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() {
            flush(
                &mut entries,
                &mut current_path,
                &mut current_head,
                &mut current_branch,
                &mut is_bare,
                &mut is_detached,
                &mut is_locked,
                &mut lock_reason,
            );
            continue;
        }

        if let Some(path) = line.strip_prefix("worktree ") {
            flush(
                &mut entries,
                &mut current_path,
                &mut current_head,
                &mut current_branch,
                &mut is_bare,
                &mut is_detached,
                &mut is_locked,
                &mut lock_reason,
            );
            current_path = Some(path.trim().to_string());
        } else if let Some(h) = line.strip_prefix("HEAD ") {
            current_head = h.trim().to_string();
        } else if let Some(b) = line.strip_prefix("branch ") {
            let ref_name = b.trim();
            let branch = ref_name.strip_prefix("refs/heads/").unwrap_or(ref_name);
            current_branch = Some(branch.to_string());
        } else if line == "bare" {
            is_bare = true;
        } else if line == "detached" {
            is_detached = true;
        } else if let Some(reason) = line.strip_prefix("locked ") {
            is_locked = true;
            lock_reason = Some(reason.trim().to_string());
        } else if line == "locked" {
            is_locked = true;
        }
    }

    flush(
        &mut entries,
        &mut current_path,
        &mut current_head,
        &mut current_branch,
        &mut is_bare,
        &mut is_detached,
        &mut is_locked,
        &mut lock_reason,
    );

    entries
}

pub async fn worktree_list(path: &Path) -> Result<GitWorktreeListResponse> {
    ensure_repo(path).await?;
    let raw = run_git_ok(
        path,
        "git worktree",
        &["worktree", "list", "--porcelain"],
        READ_TIMEOUT,
    )
    .await?;
    let worktrees = parse_worktree_porcelain(&raw);
    Ok(GitWorktreeListResponse {
        is_repo: true,
        worktrees,
    })
}

pub async fn worktree_add(
    path: &Path,
    worktree_dir: &Path,
    branch: &str,
    base_commit: Option<&str>,
) -> Result<()> {
    ensure_repo(path).await?;
    let dir_str = worktree_dir
        .to_str()
        .ok_or_else(|| anyhow!("Invalid worktree directory path"))?;

    let mut args = vec!["worktree", "add", "-b", branch, dir_str];
    if let Some(base) = base_commit {
        args.push(base);
    }

    let output = run_git(path, "git worktree add", &args, MUTATE_TIMEOUT).await?;
    if !output.success {
        // Fallback: branch might already exist
        let fallback_args = vec!["worktree", "add", dir_str, branch];
        let fallback = run_git(path, "git worktree add", &fallback_args, MUTATE_TIMEOUT).await?;
        if !fallback.success {
            bail!("{}", git_error(&output, "Failed to create worktree"));
        }
    }
    Ok(())
}

pub async fn worktree_remove(path: &Path, worktree_dir: &Path, force: bool) -> Result<()> {
    ensure_repo(path).await?;
    let dir_str = worktree_dir
        .to_str()
        .ok_or_else(|| anyhow!("Invalid worktree directory path"))?;

    let mut args = vec!["worktree", "remove"];
    if force {
        args.push("--force");
    }
    args.push(dir_str);

    let output = run_git(path, "git worktree remove", &args, MUTATE_TIMEOUT).await?;
    if !output.success {
        bail!("{}", git_error(&output, "Failed to remove worktree"));
    }
    Ok(())
}

pub async fn list_untracked_files(path: &Path) -> Result<Vec<String>> {
    ensure_repo(path).await?;
    let output = run_git_ok(
        path,
        "git ls-files",
        &["ls-files", "--others", "--exclude-standard"],
        READ_TIMEOUT,
    )
    .await?;
    Ok(output
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(ToString::to_string)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::anyhow;
    use std::fs;
    use std::process::Command;
    use tempfile::tempdir;

    fn run_plain_git(cwd: &Path, args: &[&str]) {
        let output = Command::new("git")
            .args(args)
            .current_dir(cwd)
            .output()
            .expect("git");
        assert!(
            output.status.success(),
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn init_repo(dir: &Path) {
        run_plain_git(dir, &["-c", "init.defaultBranch=main", "init"]);
        run_plain_git(dir, &["config", "user.name", "Test"]);
        run_plain_git(dir, &["config", "user.email", "test@example.com"]);
    }

    #[test]
    fn rejects_unsafe_paths() {
        assert!(validate_git_path("../secret").is_err());
        assert!(validate_git_path(".git/config").is_err());
        assert!(validate_git_path("-n").is_err());
        assert!(validate_git_path(":(glob)*").is_err());
        assert!(validate_git_path("src/*.rs").is_err());
        assert!(validate_git_path("").is_err());
        assert_eq!(validate_git_path("src/lib.rs").unwrap(), "src/lib.rs");
    }

    #[test]
    fn parses_status_and_branch_metadata() {
        let (branch, detached, ahead, behind) =
            parse_branch_header("## main...origin/main [ahead 2, behind 1]");
        assert_eq!(branch.as_deref(), Some("main"));
        assert!(!detached);
        assert_eq!(ahead, 2);
        assert_eq!(behind, 1);

        let mut payload = b"## main\0 M src/lib.rs\0?? new.rs\0R  old.rs\0renamed.rs\0".to_vec();
        payload.push(0);
        let (header, files) = parse_porcelain_z(&payload);
        assert!(header.contains("main"));
        assert_eq!(files.len(), 3);
        assert_eq!(files[0].path, "src/lib.rs");
        assert!(files[0].unstaged);
        assert!(!files[0].staged);
        assert!(files[1].untracked);
        assert_eq!(files[2].path, "renamed.rs");
        assert_eq!(files[2].original_path.as_deref(), Some("old.rs"));
        assert!(files[2].staged);
    }

    #[test]
    fn parses_log_lines_and_decorations() {
        let line = "abc1234def\u{1f}parent1 parent2\u{1f}Ada\u{1f}ada@example.com\u{1f}2026-08-12T18:01:00Z\u{1f} (HEAD -> main, origin/main, tag: v1.0.0)\u{1f}fix parser";
        let commit = parse_log_line(line).unwrap();
        assert_eq!(commit.short_sha, "abc1234");
        assert_eq!(commit.parents.len(), 2);
        assert_eq!(commit.refs, vec!["HEAD", "main", "origin/main", "v1.0.0"]);
        assert_eq!(commit.subject, "fix parser");
        assert_eq!(
            parse_decorate(" (HEAD, tag: nightly)"),
            vec!["HEAD", "nightly"]
        );
    }

    #[test]
    fn heuristic_message_uses_file_verbs() {
        let added = GitFileChange {
            path: "src/new.rs".into(),
            original_path: None,
            index_status: "?".into(),
            worktree_status: "?".into(),
            staged: false,
            unstaged: true,
            untracked: true,
            conflict: false,
        };
        assert_eq!(
            heuristic_commit_message(std::slice::from_ref(&added)),
            "Add new.rs"
        );
        let other = GitFileChange {
            path: "README.md".into(),
            original_path: None,
            index_status: "M".into(),
            worktree_status: " ".into(),
            staged: true,
            unstaged: false,
            untracked: false,
            conflict: false,
        };
        assert_eq!(
            heuristic_commit_message(&[added, other]),
            "Update new.rs and README.md"
        );
    }

    #[test]
    fn strips_fenced_model_output() {
        assert_eq!(
            clean_generated_message("```text\nfeat: add graph\n```\n"),
            "feat: add graph"
        );
    }

    #[tokio::test]
    async fn status_stage_commit_discard_and_log() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        fs::write(root.join("README.md"), "hello\n").unwrap();
        run_plain_git(root, &["add", "README.md"]);
        run_plain_git(root, &["commit", "-m", "initial"]);

        fs::write(root.join("README.md"), "hello world\n").unwrap();
        fs::write(root.join("extra.txt"), "extra\n").unwrap();

        let extra_diff = diff(root, "extra.txt", false).await.unwrap();
        assert!(extra_diff.diff.contains("+extra"));

        let dirty = status(root).await.unwrap();
        assert_eq!(dirty.branch.as_deref(), Some("main"));
        assert_eq!(dirty.files.len(), 2);

        let after_stage = stage(root, &["README.md".into()]).await.unwrap();
        let readme = after_stage
            .files
            .iter()
            .find(|file| file.path == "README.md")
            .unwrap();
        assert!(readme.staged);

        let unstaged = unstage(root, &["README.md".into()]).await.unwrap();
        let readme = unstaged
            .files
            .iter()
            .find(|file| file.path == "README.md")
            .unwrap();
        assert!(!readme.staged);
        assert!(readme.unstaged);

        discard(root, &["README.md".into()]).await.unwrap();
        let after_discard = status(root).await.unwrap();
        assert!(after_discard
            .files
            .iter()
            .all(|file| file.path != "README.md"));

        fs::write(root.join("README.md"), "hello again\n").unwrap();
        let committed = commit(root, "docs: update readme", &["README.md".into()])
            .await
            .unwrap();
        assert!(!committed.sha.is_empty());
        assert_eq!(committed.subject, "docs: update readme");

        discard(root, &["extra.txt".into()]).await.unwrap();
        let clean = status(root).await.unwrap();
        assert!(clean.files.is_empty());

        let history = log(root, Some(10)).await.unwrap();
        assert_eq!(history.commits.len(), 2);
        assert_eq!(history.commits[0].subject, "docs: update readme");
        assert_eq!(history.commits[0].parents.len(), 1);
        assert_eq!(history.commits[1].subject, "initial");

        let shown = show(root, &history.commits[0].sha).await.unwrap();
        assert_eq!(shown.commit.subject, "docs: update readme");
        assert!(shown.diff.contains("hello again") || shown.diff.contains("README.md"));

        let patch = diff(root, "README.md", false).await.unwrap();
        assert!(patch.diff.is_empty());
    }

    #[tokio::test]
    async fn rejects_non_repo_and_empty_commit() {
        let dir = tempdir().unwrap();
        assert!(status(dir.path())
            .await
            .unwrap_err()
            .to_string()
            .contains("Not a git"));
        init_repo(dir.path());
        let empty = log(dir.path(), None).await.unwrap();
        assert!(empty.commits.is_empty());
        fs::write(dir.path().join("a.txt"), "a\n").unwrap();
        run_plain_git(dir.path(), &["add", "a.txt"]);
        run_plain_git(dir.path(), &["commit", "-m", "first"]);
        assert!(commit(dir.path(), "   ", &[]).await.is_err());
    }

    #[test]
    fn parses_worktree_porcelain_records() {
        let sample = "\
worktree /home/user/project
HEAD a1b2c3d4e5f6071829304152637485960718293a
branch refs/heads/main

worktree /home/user/project-worktree
HEAD b2c3d4e5f6071829304152637485960718293a4b
branch refs/heads/feature-x
locked test lock reason

worktree /home/user/project-detached
HEAD c3d4e5f6071829304152637485960718293a4b5c
detached
";
        let parsed = parse_worktree_porcelain(sample);
        assert_eq!(parsed.len(), 3);

        assert_eq!(parsed[0].path, "/home/user/project");
        assert_eq!(parsed[0].branch.as_deref(), Some("main"));
        assert!(!parsed[0].is_detached);
        assert!(!parsed[0].is_locked);

        assert_eq!(parsed[1].path, "/home/user/project-worktree");
        assert_eq!(parsed[1].branch.as_deref(), Some("feature-x"));
        assert!(parsed[1].is_locked);
        assert_eq!(parsed[1].lock_reason.as_deref(), Some("test lock reason"));

        assert_eq!(parsed[2].path, "/home/user/project-detached");
        assert!(parsed[2].is_detached);
        assert!(parsed[2].branch.is_none());
    }

    #[tokio::test]
    async fn worktree_lifecycle_test() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        fs::write(root.join("README.md"), "main\n").unwrap();
        run_plain_git(root, &["add", "README.md"]);
        run_plain_git(root, &["commit", "-m", "init"]);

        let wt_dir = tempdir().unwrap();
        let wt_path = wt_dir.path().join("wt1");

        worktree_add(root, &wt_path, "feature-wt", None)
            .await
            .unwrap();

        let list = worktree_list(root).await.unwrap();
        assert!(list.worktrees.len() >= 2);
        assert!(list
            .worktrees
            .iter()
            .any(|w| w.branch.as_deref() == Some("feature-wt")));

        worktree_remove(root, &wt_path, true).await.unwrap();

        let list_after = worktree_list(root).await.unwrap();
        assert!(!list_after
            .worktrees
            .iter()
            .any(|w| w.branch.as_deref() == Some("feature-wt")));
    }
    #[tokio::test]
    async fn run_git_timeout_error_names_the_phase() {
        let dir = tempdir().unwrap();
        init_repo(dir.path());
        let error = run_git(dir.path(), "git add", &["add", "."], Duration::ZERO)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("git add timed out"));
    }

    #[tokio::test]
    async fn commit_reports_git_rejection_from_stderr() {
        let dir = tempdir().unwrap();
        init_repo(dir.path());
        run_plain_git(dir.path(), &["commit", "--allow-empty", "-m", "base"]);
        let error = commit(dir.path(), "test: empty", &[]).await.unwrap_err();
        assert_eq!(error.to_string(), "git commit failed");
    }

    fn seed_repo_with_change(dir: &Path) -> PathBuf {
        init_repo(dir);
        fs::write(dir.join("tracked.txt"), "initial\n").unwrap();
        run_plain_git(dir, &["add", "tracked.txt"]);
        run_plain_git(dir, &["commit", "-m", "initial"]);
        fs::write(dir.join("tracked.txt"), "changed\n").unwrap();
        run_plain_git(dir, &["add", "tracked.txt"]);
        dir.to_path_buf()
    }

    fn create_index_lock_with_age(repo: &Path, age: Duration) {
        let lock = repo.join(".git").join("index.lock");
        fs::write(&lock, b"").unwrap();
        let file = fs::OpenOptions::new().write(true).open(&lock).unwrap();
        let modified = std::time::SystemTime::now()
            .checked_sub(age)
            .expect("system clock");
        file.set_times(std::fs::FileTimes::new().set_modified(modified))
            .unwrap();
    }

    #[tokio::test]
    async fn stale_index_lock_is_recovered_before_commit() {
        let dir = tempdir().unwrap();
        let repo = seed_repo_with_change(dir.path());
        create_index_lock_with_age(&repo, Duration::from_secs(60));

        let result = commit(&repo, "test: stale lock recovery", &[]).await;

        assert!(result.is_ok(), "commit failed: {:?}", result.err());
        assert!(!repo.join(".git").join("index.lock").exists());
    }

    #[tokio::test]
    async fn fresh_index_lock_blocks_commit_without_removal() {
        let dir = tempdir().unwrap();
        let repo = seed_repo_with_change(dir.path());
        create_index_lock_with_age(&repo, Duration::ZERO);

        let error = commit(&repo, "test: blocked by fresh lock", &[])
            .await
            .unwrap_err();

        assert!(error
            .to_string()
            .contains("Another git operation is in progress"));
        assert!(repo.join(".git").join("index.lock").exists());
        fs::remove_file(repo.join(".git").join("index.lock")).unwrap();
    }

    #[tokio::test]
    async fn commit_stage_and_commit_roundtrip() {
        let dir = tempdir().unwrap();
        init_repo(dir.path());
        run_plain_git(dir.path(), &["commit", "--allow-empty", "-m", "base"]);
        fs::write(dir.path().join("tracked.txt"), "hello\n").unwrap();
        let result = commit(
            dir.path(),
            "test: add tracked file\n\nDetails",
            &["tracked.txt".to_string()],
        )
        .await
        .unwrap();
        assert!(!result.sha.is_empty());
        assert_eq!(result.subject, "test: add tracked file");
        assert_eq!(
            run_git_ok(
                dir.path(),
                "git rev-parse",
                &["rev-parse", "HEAD"],
                READ_TIMEOUT,
            )
            .await
            .unwrap()
            .trim(),
            result.sha,
        );

        fs::write(dir.path().join("tracked.txt"), "changed\n").unwrap();
        stage(dir.path(), &["tracked.txt".to_string()])
            .await
            .unwrap();
        let staged = commit(dir.path(), "test: commit staged file", &[])
            .await
            .unwrap();
        assert!(!staged.sha.is_empty());
        assert_eq!(staged.subject, "test: commit staged file");
        assert_eq!(
            run_git_ok(
                dir.path(),
                "git rev-parse",
                &["rev-parse", "HEAD"],
                READ_TIMEOUT,
            )
            .await
            .unwrap()
            .trim(),
            staged.sha,
        );
    }

    #[test]
    fn commit_metadata_success_preserves_git_values() {
        let response = commit_response_from_log(
            Ok("abc1234def\u{1f}actual git subject\n".to_string()),
            "requested subject",
        );
        assert_eq!(response.sha, "abc1234def");
        assert_eq!(response.subject, "actual git subject");
    }

    #[test]
    fn commit_metadata_errors_preserve_success_response() {
        for error in [
            anyhow!("git log timed out"),
            anyhow!("git is not available: spawn failed"),
            anyhow!("fatal: cannot read revision"),
        ] {
            let response = commit_response_from_log(Err(error), "feat: save work\n\nDetails");
            assert_eq!(response.sha, "");
            assert_eq!(response.subject, "feat: save work");
            assert_eq!(
                serde_json::to_value(&response).unwrap(),
                serde_json::json!({"sha": "", "subject": "feat: save work"}),
            );
        }
    }

    #[test]
    fn commit_metadata_missing_or_malformed_uses_fallback() {
        for line in ["", "\n", "abc1234", "\u{1f}subject", "abc1234\u{1f}"] {
            let response =
                commit_response_from_log(Ok(line.to_string()), "feat: save work\n\nDetails");
            assert_eq!(response.sha, "", "input: {line:?}");
            assert_eq!(response.subject, "feat: save work", "input: {line:?}");
        }
    }

    #[test]
    fn commit_core_phase_budget_is_240_seconds() {
        assert_eq!(
            READ_TIMEOUT + ADD_TIMEOUT + COMMIT_TIMEOUT + READ_TIMEOUT,
            Duration::from_secs(240),
        );
    }

    #[test]
    fn parse_repo_layout_accepts_git_output() {
        let layout = parse_repo_layout(
            "true\n/no/such/openduck/repo\n/no/such/openduck/repo/.git/index.lock\n/no/such/openduck/repo/.git\n",
        )
        .unwrap();
        assert_eq!(layout.toplevel, PathBuf::from("/no/such/openduck/repo"));
        assert_eq!(
            layout.index_lock,
            PathBuf::from("/no/such/openduck/repo/.git/index.lock")
        );
    }

    #[test]
    fn parse_repo_layout_rejects_bad_output() {
        for stdout in [
            "",
            "true\n/no/such/openduck/repo\n/no/such/openduck/repo/.git/index.lock\n",
            "true\n/a\n/b\n/c\n/d\n",
            "false\n/no/such/openduck/repo\n/no/such/openduck/repo/.git/index.lock\n/no/such/openduck/repo/.git\n",
            "true\nrelative\n/no/such/openduck/repo/.git/index.lock\n/no/such/openduck/repo/.git\n",
            "true\n/no/such/openduck/repo\nrelative.lock\n/no/such/openduck/repo/.git\n",
            "true\n/no/such/openduck/repo\n/no/such/openduck/repo/.git/index.lock\nrelative-git\n",
        ] {
            let Err(error) = parse_repo_layout(stdout) else {
                panic!("expected reject for {stdout:?}");
            };
            assert!(
                error.to_string().contains("Not a git repository"),
                "stdout {stdout:?} produced {error}"
            );
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn timed_out_git_child_lock_is_reclaimed() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempdir().unwrap();
        let root = seed_repo_with_change(dir.path());
        let hook = root.join(".git/hooks/pre-commit");
        fs::write(
            &hook,
            "#!/bin/sh\ntouch \"$(git rev-parse --git-path index.lock)\"\nsleep 2\n",
        )
        .unwrap();
        fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();

        let layout = repo_layout(&root).await.unwrap();
        let error = run_git_mutating(
            &root,
            &layout.index_lock,
            "git commit",
            &["commit", "-m", "hooked"],
            Duration::from_millis(300),
        )
        .await
        .unwrap_err();

        assert!(
            error.to_string().contains("timed out"),
            "error string mentions timeout: {error}"
        );
        assert!(!layout.index_lock.exists());
    }

    #[tokio::test]
    async fn discover_repositories_multiple_nested_and_ordinary_dirs() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        run_plain_git(root, &["commit", "--allow-empty", "-m", "root init"]);

        let ordinary_dir = root.join("docs").join("guides");
        fs::create_dir_all(&ordinary_dir).unwrap();
        fs::write(ordinary_dir.join("readme.md"), "# Guide\n").unwrap();

        let nested_a = root.join("packages").join("repo_a");
        fs::create_dir_all(&nested_a).unwrap();
        init_repo(&nested_a);
        run_plain_git(&nested_a, &["commit", "--allow-empty", "-m", "repo a init"]);
        fs::write(nested_a.join("untracked_a.txt"), "hello a\n").unwrap();

        let nested_b = root.join("services").join("repo_b");
        fs::create_dir_all(&nested_b).unwrap();
        init_repo(&nested_b);
        run_plain_git(&nested_b, &["commit", "--allow-empty", "-m", "repo b init"]);
        fs::write(nested_b.join("staged_b.txt"), "hello b\n").unwrap();
        stage(&nested_b, &["staged_b.txt".to_string()])
            .await
            .unwrap();

        let response = discover_repositories(root).await.unwrap();

        assert_eq!(response.repositories.len(), 3);
        assert_eq!(response.repositories[0].path, ".");
        assert_eq!(response.repositories[1].path, "packages/repo_a");
        assert_eq!(response.repositories[2].path, "services/repo_b");

        let status_root = &response.repositories[0].status;
        assert!(status_root.is_repo);
        assert!(status_root.files.iter().any(|f| f.path.contains("docs")));

        let status_a = &response.repositories[1].status;
        assert!(status_a.is_repo);
        assert_eq!(status_a.files.len(), 1);
        assert_eq!(status_a.files[0].path, "untracked_a.txt");
        assert!(status_a.files[0].untracked);

        let status_b = &response.repositories[2].status;
        assert!(status_b.is_repo);
        assert_eq!(status_b.files.len(), 1);
        assert_eq!(status_b.files[0].path, "staged_b.txt");
        assert!(status_b.files[0].staged);
    }

    #[tokio::test]
    async fn discover_repositories_excludes_git_internals_and_fake_object_repos() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        run_plain_git(root, &["commit", "--allow-empty", "-m", "root init"]);

        let fake_repo = root.join(".git").join("objects").join("fake_repo");
        fs::create_dir_all(fake_repo.join(".git")).unwrap();
        fs::write(
            fake_repo.join(".git").join("HEAD"),
            "ref: refs/heads/main\n",
        )
        .unwrap();

        let fake_module = root.join(".git").join("modules").join("sub");
        fs::create_dir_all(fake_module.join(".git")).unwrap();
        fs::write(
            fake_module.join(".git").join("HEAD"),
            "ref: refs/heads/main\n",
        )
        .unwrap();

        let response = discover_repositories(root).await.unwrap();

        assert_eq!(response.repositories.len(), 1);
        assert_eq!(response.repositories[0].path, ".");

        let response_git = discover_repositories(&root.join(".git")).await.unwrap();
        assert!(response_git.repositories.is_empty());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn discover_repositories_avoids_symlink_traversal() {
        use std::os::unix::fs::symlink;

        let dir = tempdir().unwrap();
        let root = dir.path().join("main_proj");
        fs::create_dir_all(&root).unwrap();
        init_repo(&root);
        run_plain_git(&root, &["commit", "--allow-empty", "-m", "root init"]);

        let external_repo = dir.path().join("external_repo");
        fs::create_dir_all(&external_repo).unwrap();
        init_repo(&external_repo);
        run_plain_git(
            &external_repo,
            &["commit", "--allow-empty", "-m", "ext init"],
        );

        symlink(&external_repo, root.join("linked_repo")).unwrap();
        symlink(&root, root.join("loop_link")).unwrap();

        let response = discover_repositories(&root).await.unwrap();

        assert_eq!(response.repositories.len(), 1);
        assert_eq!(response.repositories[0].path, ".");
    }

    #[tokio::test]
    async fn discover_repositories_non_repo_root_with_nested_repos() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        let repo1 = root.join("backend");
        fs::create_dir_all(&repo1).unwrap();
        init_repo(&repo1);
        run_plain_git(&repo1, &["commit", "--allow-empty", "-m", "init backend"]);

        let repo2 = root.join("frontend");
        fs::create_dir_all(&repo2).unwrap();
        init_repo(&repo2);
        run_plain_git(&repo2, &["commit", "--allow-empty", "-m", "init frontend"]);

        let docs = root.join("docs");
        fs::create_dir_all(&docs).unwrap();
        fs::write(docs.join("info.txt"), "hello").unwrap();

        let response = discover_repositories(root).await.unwrap();

        assert_eq!(response.repositories.len(), 2);
        assert_eq!(response.repositories[0].path, "backend");
        assert_eq!(response.repositories[1].path, "frontend");
        assert!(response.repositories[0].status.is_repo);
        assert!(response.repositories[1].status.is_repo);
    }

    #[tokio::test]
    async fn discover_repositories_empty_when_no_git_repositories() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("some").join("nested").join("dir")).unwrap();
        fs::write(root.join("some").join("file.txt"), "plain").unwrap();

        let response = discover_repositories(root).await.unwrap();
        assert!(response.repositories.is_empty());
    }

    #[tokio::test]
    async fn repository_action_path_validation() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        run_plain_git(root, &["commit", "--allow-empty", "-m", "init root"]);

        let nested = root.join("packages").join("nested");
        fs::create_dir_all(&nested).unwrap();
        init_repo(&nested);
        run_plain_git(&nested, &["commit", "--allow-empty", "-m", "init nested"]);

        let non_repo = root.join("docs");
        fs::create_dir_all(&non_repo).unwrap();

        // 1. Absolute path rejected
        let abs_path = nested.to_string_lossy().to_string();
        let err = repository_action(
            root,
            GitRepositoryActionRequest {
                path: abs_path,
                action: GitRepositoryActionType::Pull,
                message: None,
                paths: None,
            },
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("Absolute paths are not allowed"));

        // 2. Traversal path rejected
        let err = repository_action(
            root,
            GitRepositoryActionRequest {
                path: "../outside".to_string(),
                action: GitRepositoryActionType::Pull,
                message: None,
                paths: None,
            },
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("Path traversal"));

        let err = repository_action(
            root,
            GitRepositoryActionRequest {
                path: "packages/../../outside".to_string(),
                action: GitRepositoryActionType::Pull,
                message: None,
                paths: None,
            },
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("Path traversal"));

        // 3. Non-repo directory rejected
        let err = repository_action(
            root,
            GitRepositoryActionRequest {
                path: "docs".to_string(),
                action: GitRepositoryActionType::Pull,
                message: None,
                paths: None,
            },
        )
        .await
        .unwrap_err();
        assert!(
            err.to_string().contains("not a git repository root")
                || err
                    .to_string()
                    .contains("not among discovered repositories")
        );

        // 4. Non-existent path rejected
        let err = repository_action(
            root,
            GitRepositoryActionRequest {
                path: "packages/doesnotexist".to_string(),
                action: GitRepositoryActionType::Pull,
                message: None,
                paths: None,
            },
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("does not exist"));

        // 5. Access inside .git rejected
        let err = repository_action(
            root,
            GitRepositoryActionRequest {
                path: ".git".to_string(),
                action: GitRepositoryActionType::Pull,
                message: None,
                paths: None,
            },
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains(".git"));

        // 6. Valid discovered nested repo is accepted for resolution
        let resolved = resolve_discovered_repo(root, "packages/nested")
            .await
            .unwrap();
        assert_eq!(resolved, nested.canonicalize().unwrap());

        // 7. Symlink pointing outside project root rejected
        #[cfg(unix)]
        {
            let outside_dir = tempdir().unwrap();
            let outside_repo = outside_dir.path().join("outside_repo");
            fs::create_dir_all(&outside_repo).unwrap();
            init_repo(&outside_repo);
            let symlink_path = root.join("symlinked_repo");
            let _ = std::os::unix::fs::symlink(&outside_repo, &symlink_path);
            let err = repository_action(
                root,
                GitRepositoryActionRequest {
                    path: "symlinked_repo".to_string(),
                    action: GitRepositoryActionType::Pull,
                    message: None,
                    paths: None,
                },
            )
            .await
            .unwrap_err();
            assert!(
                err.to_string().contains("outside project root")
                    || err
                        .to_string()
                        .contains("not among discovered repositories")
                    || err.to_string().contains("not a git repository root")
            );
        }
    }

    #[tokio::test]
    async fn repository_action_parent_nested_isolation() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        run_plain_git(root, &["commit", "--allow-empty", "-m", "root base"]);
        let root_head_before =
            run_git_ok(root, "git rev-parse", &["rev-parse", "HEAD"], READ_TIMEOUT)
                .await
                .unwrap();

        let nested = root.join("services").join("subrepo");
        fs::create_dir_all(&nested).unwrap();
        init_repo(&nested);
        run_plain_git(&nested, &["commit", "--allow-empty", "-m", "nested base"]);
        let nested_head_before = run_git_ok(
            &nested,
            "git rev-parse",
            &["rev-parse", "HEAD"],
            READ_TIMEOUT,
        )
        .await
        .unwrap();

        // Create dirty files in both root and nested
        fs::write(root.join("root_dirty.txt"), "parent content\n").unwrap();
        fs::write(nested.join("nested_file.txt"), "nested content\n").unwrap();

        // Stage file ONLY in nested repo
        stage(&nested, &["nested_file.txt".to_string()])
            .await
            .unwrap();

        // Commit action targeting nested repo
        let res = repository_action(
            root,
            GitRepositoryActionRequest {
                path: "services/subrepo".to_string(),
                action: GitRepositoryActionType::Commit,
                message: Some("feat: nested commit".to_string()),
                paths: None,
            },
        )
        .await
        .unwrap();

        assert!(res.success);
        assert_eq!(res.action, GitRepositoryActionType::Commit);
        assert_eq!(res.path, "services/subrepo");
        assert_eq!(res.message.as_deref(), Some("feat: nested commit"));
        assert!(res.commit.is_some());

        // Verify nested HEAD advanced
        let nested_head_after = run_git_ok(
            &nested,
            "git rev-parse",
            &["rev-parse", "HEAD"],
            READ_TIMEOUT,
        )
        .await
        .unwrap();
        assert_ne!(nested_head_before, nested_head_after);

        // Verify root HEAD is completely UNCHANGED
        let root_head_after =
            run_git_ok(root, "git rev-parse", &["rev-parse", "HEAD"], READ_TIMEOUT)
                .await
                .unwrap();
        assert_eq!(root_head_before, root_head_after);

        // Verify root dirty file is STILL dirty and uncommitted
        let root_status = status(root).await.unwrap();
        assert!(root_status.files.iter().any(|f| f.path == "root_dirty.txt"));

        // Now stage and commit in root repo via path "."
        stage(root, &["root_dirty.txt".to_string()]).await.unwrap();
        let res_root = repository_action(
            root,
            GitRepositoryActionRequest {
                path: ".".to_string(),
                action: GitRepositoryActionType::Commit,
                message: Some("feat: root commit".to_string()),
                paths: None,
            },
        )
        .await
        .unwrap();
        assert!(res_root.success);

        // Verify root HEAD advanced, nested HEAD was unaffected
        let root_head_final =
            run_git_ok(root, "git rev-parse", &["rev-parse", "HEAD"], READ_TIMEOUT)
                .await
                .unwrap();
        let nested_head_final = run_git_ok(
            &nested,
            "git rev-parse",
            &["rev-parse", "HEAD"],
            READ_TIMEOUT,
        )
        .await
        .unwrap();
        assert_ne!(root_head_after, root_head_final);
        assert_eq!(nested_head_after, nested_head_final);
    }

    #[tokio::test]
    async fn repository_action_no_remote_failure() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        run_plain_git(root, &["commit", "--allow-empty", "-m", "init"]);

        let pull_err = repository_action(
            root,
            GitRepositoryActionRequest {
                path: ".".to_string(),
                action: GitRepositoryActionType::Pull,
                message: None,
                paths: None,
            },
        )
        .await
        .unwrap_err();
        assert!(pull_err
            .to_string()
            .contains("No remote repository configured"));

        let push_err = repository_action(
            root,
            GitRepositoryActionRequest {
                path: ".".to_string(),
                action: GitRepositoryActionType::Push,
                message: None,
                paths: None,
            },
        )
        .await
        .unwrap_err();
        assert!(push_err
            .to_string()
            .contains("No remote repository configured"));
    }

    #[tokio::test]
    async fn repository_action_empty_commit_and_message_validation() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        init_repo(root);
        run_plain_git(root, &["commit", "--allow-empty", "-m", "init"]);

        // Missing message
        let err = repository_action(
            root,
            GitRepositoryActionRequest {
                path: ".".to_string(),
                action: GitRepositoryActionType::Commit,
                message: None,
                paths: None,
            },
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("Commit message is required"));

        // Whitespace message
        let err = repository_action(
            root,
            GitRepositoryActionRequest {
                path: ".".to_string(),
                action: GitRepositoryActionType::Commit,
                message: Some("   \n  ".to_string()),
                paths: None,
            },
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("Commit message is required"));

        // Empty commit on clean repo
        let err = repository_action(
            root,
            GitRepositoryActionRequest {
                path: ".".to_string(),
                action: GitRepositoryActionType::Commit,
                message: Some("valid msg".to_string()),
                paths: None,
            },
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("Nothing to commit"));

        // Unstaged file, but nothing staged and no paths
        fs::write(root.join("unstaged.txt"), "hello").unwrap();
        let err = repository_action(
            root,
            GitRepositoryActionRequest {
                path: ".".to_string(),
                action: GitRepositoryActionType::Commit,
                message: Some("valid msg".to_string()),
                paths: None,
            },
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("No changes added to commit"));
    }

    #[tokio::test]
    async fn repository_action_unavailable_remote_and_dirty_pull_failure() {
        let dir = tempdir().unwrap();
        let upstream = dir.path().join("upstream");
        fs::create_dir_all(&upstream).unwrap();
        init_repo(&upstream);
        fs::write(upstream.join("shared.txt"), "initial\n").unwrap();
        run_plain_git(&upstream, &["add", "shared.txt"]);
        run_plain_git(&upstream, &["commit", "-m", "init shared"]);

        // Clone/create downstream repo inside project
        let project_root = dir.path().join("project");
        let local_repo = project_root.join("nested").join("repo");
        fs::create_dir_all(&local_repo).unwrap();
        init_repo(&local_repo);
        run_plain_git(
            &local_repo,
            &["remote", "add", "origin", upstream.to_str().unwrap()],
        );
        run_plain_git(&local_repo, &["pull", "origin", "main"]);

        // Commit new change upstream
        fs::write(upstream.join("shared.txt"), "upstream update\n").unwrap();
        run_plain_git(&upstream, &["commit", "-am", "upstream change"]);

        // In local repo, create dirty conflict
        fs::write(
            local_repo.join("shared.txt"),
            "local dirty conflicting change\n",
        )
        .unwrap();

        // 1. Dirty pull should fail with useful error
        let err = repository_action(
            &project_root,
            GitRepositoryActionRequest {
                path: "nested/repo".to_string(),
                action: GitRepositoryActionType::Pull,
                message: None,
                paths: None,
            },
        )
        .await
        .unwrap_err();
        assert!(
            err.to_string().contains("overwritten by merge") || err.to_string().contains("pull")
        );

        // 2. Unavailable remote failure
        let broken_repo = project_root.join("nested").join("broken");
        fs::create_dir_all(&broken_repo).unwrap();
        init_repo(&broken_repo);
        run_plain_git(&broken_repo, &["commit", "--allow-empty", "-m", "init"]);
        run_plain_git(
            &broken_repo,
            &[
                "remote",
                "add",
                "origin",
                "http://127.0.0.1:9/nonexistent.git",
            ],
        );

        let err = repository_action(
            &project_root,
            GitRepositoryActionRequest {
                path: "nested/broken".to_string(),
                action: GitRepositoryActionType::Pull,
                message: None,
                paths: None,
            },
        )
        .await
        .unwrap_err();
        let err_str = err.to_string();
        assert!(
            err_str.contains("fatal:")
                || err_str.contains("Failed to connect")
                || err_str.contains("git pull failed")
        );
    }

    #[tokio::test]
    async fn repository_action_push_and_pull_success() {
        let dir = tempdir().unwrap();
        let upstream = dir.path().join("upstream");
        fs::create_dir_all(&upstream).unwrap();
        init_repo(&upstream);
        run_plain_git(
            &upstream,
            &["config", "receive.denyCurrentBranch", "ignore"],
        );
        fs::write(upstream.join("file.txt"), "v1\n").unwrap();
        run_plain_git(&upstream, &["add", "file.txt"]);
        run_plain_git(&upstream, &["commit", "-m", "init upstream"]);

        let project_root = dir.path().join("project");
        let nested_repo = project_root.join("packages").join("service");
        fs::create_dir_all(&nested_repo).unwrap();
        init_repo(&nested_repo);
        run_plain_git(
            &nested_repo,
            &["remote", "add", "origin", upstream.to_str().unwrap()],
        );
        run_plain_git(&nested_repo, &["pull", "origin", "main"]);

        // 1. Commit and Push from nested repo to upstream
        fs::write(nested_repo.join("file.txt"), "v2 from nested\n").unwrap();
        stage(&nested_repo, &["file.txt".to_string()])
            .await
            .unwrap();

        let commit_res = repository_action(
            &project_root,
            GitRepositoryActionRequest {
                path: "packages/service".to_string(),
                action: GitRepositoryActionType::Commit,
                message: Some("feat: update from nested".to_string()),
                paths: None,
            },
        )
        .await
        .unwrap();
        assert!(commit_res.success);
        let commit_sha = commit_res.commit.unwrap().sha;

        let push_res = repository_action(
            &project_root,
            GitRepositoryActionRequest {
                path: "packages/service".to_string(),
                action: GitRepositoryActionType::Push,
                message: None,
                paths: None,
            },
        )
        .await
        .unwrap();
        assert!(push_res.success);

        // Verify upstream tracking was established by push
        let tracking = run_git_ok(
            &nested_repo,
            "git rev-parse",
            &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"],
            READ_TIMEOUT,
        )
        .await
        .unwrap();
        assert_eq!(tracking.trim(), "origin/main");

        // Verify upstream has the pushed commit
        let upstream_head = run_git_ok(
            &upstream,
            "git rev-parse",
            &["rev-parse", "HEAD"],
            READ_TIMEOUT,
        )
        .await
        .unwrap();
        assert_eq!(upstream_head.trim(), commit_sha.trim());

        // 2. Make change in upstream, then Pull from nested repo
        fs::write(upstream.join("new_file.txt"), "upstream addition\n").unwrap();
        run_plain_git(&upstream, &["add", "new_file.txt"]);
        run_plain_git(&upstream, &["commit", "-m", "upstream new file"]);

        let pull_res = repository_action(
            &project_root,
            GitRepositoryActionRequest {
                path: "packages/service".to_string(),
                action: GitRepositoryActionType::Pull,
                message: None,
                paths: None,
            },
        )
        .await
        .unwrap();
        assert!(pull_res.success);
        assert!(nested_repo.join("new_file.txt").exists());
        assert_eq!(
            fs::read_to_string(nested_repo.join("new_file.txt")).unwrap(),
            "upstream addition\n"
        );
    }

    #[tokio::test]
    async fn repository_action_push_rejected_preserves_error() {
        let dir = tempdir().unwrap();
        let upstream = dir.path().join("upstream");
        fs::create_dir_all(&upstream).unwrap();
        init_repo(&upstream);
        run_plain_git(
            &upstream,
            &["config", "receive.denyCurrentBranch", "ignore"],
        );
        fs::write(upstream.join("file.txt"), "v1\n").unwrap();
        run_plain_git(&upstream, &["add", "file.txt"]);
        run_plain_git(&upstream, &["commit", "-m", "init upstream"]);

        let project_root = dir.path().join("project");
        let nested_repo = project_root.join("packages").join("service");
        fs::create_dir_all(&nested_repo).unwrap();
        init_repo(&nested_repo);
        run_plain_git(
            &nested_repo,
            &["remote", "add", "origin", upstream.to_str().unwrap()],
        );
        run_plain_git(&nested_repo, &["pull", "origin", "main"]);

        // Make an upstream commit
        fs::write(upstream.join("file.txt"), "v2 upstream\n").unwrap();
        run_plain_git(&upstream, &["commit", "-am", "upstream update"]);

        // Make a conflicting local commit without pulling first
        fs::write(nested_repo.join("file.txt"), "v2 local conflict\n").unwrap();
        stage(&nested_repo, &["file.txt".to_string()])
            .await
            .unwrap();
        let _ = commit(
            &nested_repo,
            "feat: local conflict",
            &["file.txt".to_string()],
        )
        .await
        .unwrap();

        // Push should fail because remote has work we do not have
        let push_err = repository_action(
            &project_root,
            GitRepositoryActionRequest {
                path: "packages/service".to_string(),
                action: GitRepositoryActionType::Push,
                message: None,
                paths: None,
            },
        )
        .await
        .unwrap_err();

        let err_str = push_err.to_string();
        assert!(
            err_str.contains("rejected")
                || err_str.contains("fetch first")
                || err_str.contains("failed to push")
        );
    }

    #[tokio::test]
    async fn repository_action_commit_with_explicit_paths() {
        let dir = tempdir().unwrap();
        let project_root = dir.path();
        let nested_repo = project_root.join("packages").join("pkg-a");
        fs::create_dir_all(&nested_repo).unwrap();
        init_repo(&nested_repo);
        run_plain_git(&nested_repo, &["commit", "--allow-empty", "-m", "init"]);

        // Create two dirty files, unstaged
        fs::write(nested_repo.join("file1.txt"), "one\n").unwrap();
        fs::write(nested_repo.join("file2.txt"), "two\n").unwrap();

        // Commit ONLY file1.txt using explicit paths
        let res = repository_action(
            project_root,
            GitRepositoryActionRequest {
                path: "packages/pkg-a".to_string(),
                action: GitRepositoryActionType::Commit,
                message: Some("feat: commit only file1".to_string()),
                paths: Some(vec!["file1.txt".to_string()]),
            },
        )
        .await
        .unwrap();

        assert!(res.success);
        assert_eq!(res.action, GitRepositoryActionType::Commit);

        // Verify file1 is committed (not in status), file2 is still untracked/dirty
        let nested_status = status(&nested_repo).await.unwrap();
        assert!(!nested_status.files.iter().any(|f| f.path == "file1.txt"));
        assert!(nested_status.files.iter().any(|f| f.path == "file2.txt"));
    }
}
