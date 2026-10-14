use crate::sandbox::SandboxDriver;
use crate::types::{ExecOptions, ExecOutput, SnapshotId};
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tokio::fs;
use tokio::process::Command;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UntrackedFileMode {
    #[default]
    Symlink,
    Copy,
    None,
}

#[derive(Debug, Clone)]
pub struct WorktreeConfig {
    pub repo_root: PathBuf,
    pub worktree_dir: Option<PathBuf>,
    pub branch_name: Option<String>,
    pub base_ref: Option<String>,
    pub untracked_mode: UntrackedFileMode,
    pub sync_patterns: Vec<String>,
    pub shared_cargo_target: bool,
    pub keep_worktree: bool,
    pub keep_branch: bool,
}

impl WorktreeConfig {
    pub fn new(repo_root: impl Into<PathBuf>) -> Self {
        Self {
            repo_root: repo_root.into(),
            worktree_dir: None,
            branch_name: None,
            base_ref: None,
            untracked_mode: UntrackedFileMode::Symlink,
            sync_patterns: vec![
                ".env*".to_string(),
                "*.local.*".to_string(),
                "*.local".to_string(),
            ],
            shared_cargo_target: true,
            keep_worktree: false,
            keep_branch: true,
        }
    }

    pub fn with_branch(mut self, branch: impl Into<String>) -> Self {
        self.branch_name = Some(branch.into());
        self
    }

    pub fn with_worktree_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.worktree_dir = Some(dir.into());
        self
    }

    pub fn with_untracked_mode(mut self, mode: UntrackedFileMode) -> Self {
        self.untracked_mode = mode;
        self
    }
}

pub struct WorktreeSandbox {
    config: WorktreeConfig,
    worktree_path: PathBuf,
    branch_name: String,
    temp_dir: Option<tempfile::TempDir>,
    snapshots: HashMap<SnapshotId, String>,
    snapshot_counter: AtomicU64,
    initialized: bool,
}

impl WorktreeSandbox {
    pub fn new(config: WorktreeConfig) -> Self {
        let branch_name = config
            .branch_name
            .clone()
            .unwrap_or_else(|| format!("agent/task-{}", Uuid::new_v4().simple()));

        let worktree_path = config
            .worktree_dir
            .clone()
            .unwrap_or_else(|| std::env::temp_dir().join(format!("worktree-{}", Uuid::new_v4())));

        Self {
            config,
            worktree_path,
            branch_name,
            temp_dir: None,
            snapshots: HashMap::new(),
            snapshot_counter: AtomicU64::new(0),
            initialized: false,
        }
    }

    pub fn for_repo(repo_root: impl Into<PathBuf>, branch: impl Into<String>) -> Self {
        let config = WorktreeConfig::new(repo_root).with_branch(branch);
        Self::new(config)
    }

    pub fn branch_name(&self) -> &str {
        &self.branch_name
    }

    pub fn worktree_path(&self) -> &Path {
        &self.worktree_path
    }

    fn resolve_path(&self, path: &Path) -> PathBuf {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.worktree_path.join(path)
        }
    }

    async fn run_git_in(dir: &Path, args: &[&str]) -> Result<String> {
        let mut cmd = Command::new("git");
        cmd.args(args).current_dir(dir);
        let output = cmd
            .output()
            .await
            .with_context(|| format!("Failed to run git {:?}", args))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!(
                "git {} failed in {:?}: {}",
                args.join(" "),
                dir,
                stderr.trim()
            ));
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    async fn sync_untracked_files(&self) -> Result<()> {
        if self.config.untracked_mode == UntrackedFileMode::None {
            return Ok(());
        }

        let repo_root = &self.config.repo_root;
        let mut files_to_sync: Vec<PathBuf> = Vec::new();

        // 1. Gather untracked files
        if let Ok(stdout) =
            Self::run_git_in(repo_root, &["ls-files", "--others", "--exclude-standard"]).await
        {
            for line in stdout.lines() {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    files_to_sync.push(PathBuf::from(trimmed));
                }
            }
        }

        // 2. Discover standard ignored configuration files (.env, *.local, etc.)
        for pattern in &self.config.sync_patterns {
            let direct_path = repo_root.join(pattern);
            if direct_path.is_file() {
                files_to_sync.push(PathBuf::from(pattern));
            }
        }

        // Check common config file names in root
        for name in &[
            ".env",
            ".env.local",
            ".env.development",
            ".env.test",
            "openduck.local.json",
        ] {
            let p = repo_root.join(name);
            if p.is_file()
                && !files_to_sync
                    .iter()
                    .any(|existing| existing == Path::new(name))
            {
                files_to_sync.push(PathBuf::from(name));
            }
        }

        for rel_path in files_to_sync {
            let src = repo_root.join(&rel_path);
            let dst = self.worktree_path.join(&rel_path);

            if !src.exists() || dst.exists() {
                continue;
            }

            if let Some(parent) = dst.parent() {
                fs::create_dir_all(parent).await?;
            }

            match self.config.untracked_mode {
                UntrackedFileMode::Symlink => {
                    let symlink_res = Self::create_symlink(&src, &dst).await;
                    if symlink_res.is_err() {
                        // Fall back to copy if symlink is unsupported or fails
                        let _ = fs::copy(&src, &dst).await;
                    }
                }
                UntrackedFileMode::Copy => {
                    let _ = fs::copy(&src, &dst).await;
                }
                UntrackedFileMode::None => {}
            }
        }

        Ok(())
    }

    #[cfg(unix)]
    async fn create_symlink(src: &Path, dst: &Path) -> Result<()> {
        tokio::fs::symlink(src, dst)
            .await
            .with_context(|| format!("Failed to symlink {:?} -> {:?}", src, dst))
    }

    #[cfg(windows)]
    async fn create_symlink(src: &Path, dst: &Path) -> Result<()> {
        if src.is_dir() {
            tokio::fs::symlink_dir(src, dst).await
        } else {
            tokio::fs::symlink_file(src, dst).await
        }
        .with_context(|| format!("Failed to symlink {:?} -> {:?}", src, dst))
    }

    #[cfg(not(any(unix, windows)))]
    async fn create_symlink(src: &Path, dst: &Path) -> Result<()> {
        fs::copy(src, dst).await?;
        Ok(())
    }

    pub async fn export_diff(&self) -> Result<String> {
        let base = self.config.base_ref.as_deref().unwrap_or("HEAD");
        Self::run_git_in(&self.worktree_path, &["diff", base]).await
    }
}

#[async_trait]
impl SandboxDriver for WorktreeSandbox {
    fn name(&self) -> &str {
        "worktree-sandbox"
    }

    fn workspace_root(&self) -> &Path {
        &self.worktree_path
    }

    async fn initialize(&mut self) -> Result<()> {
        if self.initialized {
            return Ok(());
        }

        let repo_root = &self.config.repo_root;
        if !repo_root.exists() {
            return Err(anyhow!("Repository root does not exist: {:?}", repo_root));
        }

        // Verify it is inside a git repo
        Self::run_git_in(repo_root, &["rev-parse", "--is-inside-work-tree"])
            .await
            .context("Directory is not a valid git repository")?;

        if self.config.worktree_dir.is_none() {
            let temp_dir = tempfile::Builder::new()
                .prefix("openduck-worktree-")
                .tempdir()
                .context("Failed to allocate temporary directory for worktree")?;
            self.worktree_path = temp_dir.path().to_path_buf();
            self.temp_dir = Some(temp_dir);
        } else if let Some(parent) = self.worktree_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        // Add git worktree
        let path_str = self
            .worktree_path
            .to_str()
            .ok_or_else(|| anyhow!("Invalid worktree path"))?;

        let mut args = vec!["worktree", "add", "-b", &self.branch_name, path_str];
        if let Some(ref base) = self.config.base_ref {
            args.push(base);
        }

        let add_res = Self::run_git_in(repo_root, &args).await;
        if let Err(err) = add_res {
            // If branch already exists, try checking it out without -b
            let fallback_args = vec!["worktree", "add", path_str, &self.branch_name];
            Self::run_git_in(repo_root, &fallback_args)
                .await
                .with_context(|| format!("Failed to create worktree: {}", err))?;
        }

        self.sync_untracked_files().await?;
        self.initialized = true;
        Ok(())
    }

    async fn exec_command(&self, cmd: &str, opts: &ExecOptions) -> Result<ExecOutput> {
        let cwd = opts
            .working_dir
            .as_ref()
            .map(|p| self.resolve_path(p))
            .unwrap_or_else(|| self.worktree_path.clone());

        let mut command = if cfg!(target_os = "windows") {
            let mut c = Command::new("cmd");
            c.arg("/C").arg(cmd);
            c
        } else {
            let mut c = Command::new("sh");
            c.arg("-c").arg(cmd);
            c
        };

        command.current_dir(cwd);
        for (k, v) in &opts.env {
            command.env(k, v);
        }

        // Inject shared Cargo target directory if enabled to optimize Rust compilations
        if self.config.shared_cargo_target && !opts.env.contains_key("CARGO_TARGET_DIR") {
            let target_dir = self.config.repo_root.join("target");
            command.env("CARGO_TARGET_DIR", target_dir);
        }

        command.stdout(std::process::Stdio::piped());
        command.stderr(std::process::Stdio::piped());

        let mut child = command
            .spawn()
            .with_context(|| format!("Failed to spawn command: {}", cmd))?;

        let mut stdout = child.stdout.take().ok_or_else(|| anyhow!("No stdout"))?;
        let mut stderr = child.stderr.take().ok_or_else(|| anyhow!("No stderr"))?;

        let mut stdout_buf = Vec::new();
        let mut stderr_buf = Vec::new();
        let mut stdout_chunk = [0u8; 4096];
        let mut stderr_chunk = [0u8; 4096];
        let mut stdout_done = false;
        let mut stderr_done = false;

        let hard_limit = opts.timeout_seconds.map(Duration::from_secs);
        let start_instant = Instant::now();

        while !stdout_done || !stderr_done {
            let idle_timeout = Duration::from_secs(opts.timeout_seconds.unwrap_or(120).max(1));
            let idle_sleep = tokio::time::sleep(idle_timeout);
            tokio::pin!(idle_sleep);

            tokio::select! {
                res = tokio::io::AsyncReadExt::read(&mut stdout, &mut stdout_chunk), if !stdout_done => {
                    match res {
                        Ok(0) => { stdout_done = true; }
                        Ok(n) => { stdout_buf.extend_from_slice(&stdout_chunk[..n]); }
                        Err(_) => { stdout_done = true; }
                    }
                }
                res = tokio::io::AsyncReadExt::read(&mut stderr, &mut stderr_chunk), if !stderr_done => {
                    match res {
                        Ok(0) => { stderr_done = true; }
                        Ok(n) => { stderr_buf.extend_from_slice(&stderr_chunk[..n]); }
                        Err(_) => { stderr_done = true; }
                    }
                }
                _ = &mut idle_sleep => {
                    let _ = child.kill().await;
                    let secs = opts.timeout_seconds.unwrap_or(0);
                    return Err(anyhow!("Command timed out after {}s: {}", secs, cmd));
                }
            }

            if let Some(limit) = hard_limit {
                if start_instant.elapsed() > limit {
                    let _ = child.kill().await;
                    return Err(anyhow!("Command exceeded time limit: {}", cmd));
                }
            }
        }

        let status = child
            .wait()
            .await
            .context("Failed to wait on command process")?;

        Ok(ExecOutput {
            exit_code: status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&stdout_buf).to_string(),
            stderr: String::from_utf8_lossy(&stderr_buf).to_string(),
        })
    }

    async fn read_file(&self, path: &Path) -> Result<Vec<u8>> {
        let full_path = self.resolve_path(path);
        fs::read(&full_path)
            .await
            .with_context(|| format!("Failed to read file: {:?}", full_path))
    }

    async fn write_file(&self, path: &Path, content: &[u8]) -> Result<()> {
        let full_path = self.resolve_path(path);
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent).await?;
        }
        fs::write(&full_path, content)
            .await
            .with_context(|| format!("Failed to write file: {:?}", full_path))
    }

    async fn snapshot(&mut self, label: &str) -> Result<SnapshotId> {
        let count = self.snapshot_counter.fetch_add(1, Ordering::SeqCst);
        let id = SnapshotId(format!("worktree-{}-{}-{}", label, count, Uuid::new_v4()));

        // Snapshot via git stash create or commit
        let stash_res = Self::run_git_in(&self.worktree_path, &["stash", "create"]).await;
        let commit_sha = match stash_res {
            Ok(sha) if !sha.is_empty() => sha,
            _ => {
                // If clean, use HEAD
                Self::run_git_in(&self.worktree_path, &["rev-parse", "HEAD"])
                    .await
                    .unwrap_or_else(|_| "HEAD".to_string())
            }
        };

        self.snapshots.insert(id.clone(), commit_sha);
        Ok(id)
    }

    async fn restore_snapshot(&mut self, id: &SnapshotId) -> Result<()> {
        let commit_sha = self
            .snapshots
            .get(id)
            .ok_or_else(|| anyhow!("Snapshot not found: {:?}", id))?;

        // Reset hard to commit sha and clean untracked
        Self::run_git_in(
            &self.worktree_path,
            &["reset", "--hard", commit_sha.as_str()],
        )
        .await?;
        let _ = Self::run_git_in(&self.worktree_path, &["clean", "-fd"]).await;

        Ok(())
    }

    async fn cleanup(&mut self) -> Result<()> {
        if self.initialized && !self.config.keep_worktree {
            let repo_root = &self.config.repo_root;
            let path_str = self
                .worktree_path
                .to_str()
                .ok_or_else(|| anyhow!("Invalid worktree path"))?;

            let _ = Self::run_git_in(repo_root, &["worktree", "remove", "--force", path_str]).await;

            if !self.config.keep_branch {
                let _ = Self::run_git_in(repo_root, &["branch", "-D", &self.branch_name]).await;
            }
        }

        if let Some(td) = self.temp_dir.take() {
            let _ = td.close();
        }

        self.initialized = false;
        Ok(())
    }
}
