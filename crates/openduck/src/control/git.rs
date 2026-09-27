use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use anyhow::{anyhow, bail, Result};
use serde::{Deserialize, Serialize};
use tokio::process::Command;

use super::files::sanitize_relative_path;
use crate::subprocess::{git_command, SubprocessExt};

const STATUS_TIMEOUT: Duration = Duration::from_secs(5);
const LOG_TIMEOUT: Duration = Duration::from_secs(8);
const DIFF_TIMEOUT: Duration = Duration::from_secs(8);
const MUTATE_TIMEOUT: Duration = Duration::from_secs(30);
const MESSAGE_TIMEOUT: Duration = Duration::from_secs(25);
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

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
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

async fn run_git(path: &Path, args: &[&str], timeout: Duration) -> Result<GitOutput> {
    let mut command = git_tokio();
    command.args(args).current_dir(path);
    let output = tokio::time::timeout(timeout, command.output())
        .await
        .map_err(|_| anyhow!("git timed out"))?
        .map_err(|error| anyhow!("git is not available: {error}"))?;
    Ok(GitOutput {
        stdout: output.stdout,
        stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        success: output.status.success(),
        code: output.status.code(),
    })
}

fn git_error(output: &GitOutput, fallback: &str) -> anyhow::Error {
    if output.stderr.is_empty() {
        anyhow!("{fallback}")
    } else {
        anyhow!("{}", output.stderr)
    }
}

async fn run_git_ok(path: &Path, args: &[&str], timeout: Duration) -> Result<String> {
    let output = run_git(path, args, timeout).await?;
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
        &["rev-parse", "--is-inside-work-tree"],
        STATUS_TIMEOUT,
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
        &["status", "--porcelain=v1", "-z", "-b"],
        STATUS_TIMEOUT,
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

pub async fn log(path: &Path, limit: Option<usize>) -> Result<GitLogResponse> {
    ensure_repo(path).await?;
    let limit = limit.unwrap_or(DEFAULT_LOG).clamp(1, MAX_LOG);
    let head = run_git_ok(path, &["rev-parse", "HEAD"], STATUS_TIMEOUT)
        .await
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty() && value.chars().all(|ch| ch.is_ascii_hexdigit()));
    let branch_name = run_git_ok(path, &["rev-parse", "--abbrev-ref", "HEAD"], STATUS_TIMEOUT)
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
        &[
            "log",
            "--all",
            "--decorate=short",
            "--date-order",
            &max_count,
            &pretty,
        ],
        LOG_TIMEOUT,
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
    let output = run_git(path, &args, DIFF_TIMEOUT).await?;
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
        &[
            "log",
            "-1",
            "--decorate=short",
            &format!("--pretty=format:{format}"),
            sha,
        ],
        LOG_TIMEOUT,
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
        &["show", "--no-color", "--no-ext-diff", "--format=", sha],
        DIFF_TIMEOUT,
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
    ensure_repo(path).await?;
    let paths = validate_paths(paths)?;
    let mut args = vec!["add".to_string(), "--".to_string()];
    args.extend(paths);
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    run_git_ok(path, &arg_refs, MUTATE_TIMEOUT).await?;
    status(path).await
}

pub async fn unstage(path: &Path, paths: &[String]) -> Result<GitStatusResponse> {
    ensure_repo(path).await?;
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
    run_git_ok(path, &arg_refs, MUTATE_TIMEOUT).await?;
    status(path).await
}

pub async fn discard(path: &Path, paths: &[String]) -> Result<GitStatusResponse> {
    ensure_repo(path).await?;
    let paths = validate_paths(paths)?;
    let current = status(path).await?;
    for rel in &paths {
        let change = current.files.iter().find(|file| file.path == *rel);
        discard_one(path, rel, change).await?;
    }
    status(path).await
}

async fn discard_one(root: &Path, rel: &str, change: Option<&GitFileChange>) -> Result<()> {
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
        run_git_ok(root, &arg_refs, MUTATE_TIMEOUT).await?;
        return Ok(());
    }
    if added {
        run_git_ok(root, &["restore", "--staged", "--", rel], MUTATE_TIMEOUT).await?;
        if root.join(rel).exists() {
            run_git_ok(root, &["clean", "-f", "--", rel], MUTATE_TIMEOUT).await?;
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
    run_git_ok(root, &arg_refs, MUTATE_TIMEOUT).await?;
    Ok(())
}

pub async fn commit(path: &Path, message: &str, paths: &[String]) -> Result<GitCommitResponse> {
    ensure_repo(path).await?;
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
        run_git_ok(path, &arg_refs, MUTATE_TIMEOUT).await?;
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
    let output = run_git(path, &arg_refs, MUTATE_TIMEOUT).await?;
    drop(file);
    if !output.success {
        if output.stderr.is_empty() {
            bail!("git commit failed");
        }
        bail!("{}", output.stderr);
    }
    let line = run_git_ok(path, &["log", "-1", "--format=%H%x1f%s"], STATUS_TIMEOUT).await?;
    let mut parts = line.trim().splitn(2, '\u{1f}');
    let sha = parts.next().unwrap_or_default().to_string();
    let subject = parts.next().unwrap_or_default().to_string();
    if sha.is_empty() {
        bail!("Commit succeeded but the new revision could not be read");
    }
    Ok(GitCommitResponse { sha, subject })
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
    let output = run_git(path, args, DIFF_TIMEOUT).await?;
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
    let model_name = config
        .get_goose_model()
        .unwrap_or_else(|_| "default".to_string());
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

#[cfg(test)]
mod tests {
    use super::*;
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
}
