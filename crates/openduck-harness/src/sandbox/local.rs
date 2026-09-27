use crate::sandbox::SandboxDriver;
use crate::types::{ExecOptions, ExecOutput, SnapshotId};
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::fs;
use tokio::process::Command;
use uuid::Uuid;

pub struct LocalSandbox {
    root: PathBuf,
    snapshots: HashMap<SnapshotId, PathBuf>,
    temp_dir: Option<tempfile::TempDir>,
    snapshot_counter: AtomicU64,
}

impl LocalSandbox {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            snapshots: HashMap::new(),
            temp_dir: None,
            snapshot_counter: AtomicU64::new(0),
        }
    }

    pub fn ephemeral() -> Result<Self> {
        let temp_dir = tempfile::Builder::new()
            .prefix("openduck-harness-sandbox-")
            .tempdir()
            .context("Failed to create temporary directory for sandbox")?;
        let root = temp_dir.path().to_path_buf();
        Ok(Self {
            root,
            snapshots: HashMap::new(),
            temp_dir: Some(temp_dir),
            snapshot_counter: AtomicU64::new(0),
        })
    }

    fn resolve_path(&self, path: &Path) -> PathBuf {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.root.join(path)
        }
    }

    async fn copy_dir_all(src: &Path, dst: &Path) -> Result<()> {
        fs::create_dir_all(dst).await?;
        let mut entries = fs::read_dir(src).await?;
        while let Some(entry) = entries.next_entry().await? {
            let entry_path = entry.path();
            let dest_path = dst.join(entry.file_name());
            let file_type = entry.file_type().await?;
            if file_type.is_dir() {
                Box::pin(Self::copy_dir_all(&entry_path, &dest_path)).await?;
            } else {
                fs::copy(&entry_path, &dest_path).await?;
            }
        }
        Ok(())
    }
}

#[async_trait]
impl SandboxDriver for LocalSandbox {
    fn name(&self) -> &str {
        "local-sandbox"
    }

    fn workspace_root(&self) -> &Path {
        &self.root
    }

    async fn initialize(&mut self) -> Result<()> {
        if !self.root.exists() {
            fs::create_dir_all(&self.root).await?;
        }
        Ok(())
    }

    async fn exec_command(&self, cmd: &str, opts: &ExecOptions) -> Result<ExecOutput> {
        let cwd = opts
            .working_dir
            .as_ref()
            .map(|p| self.resolve_path(p))
            .unwrap_or_else(|| self.root.clone());

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

        command
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .stdin(std::process::Stdio::null())
            .kill_on_drop(true);

        let mut child = command.spawn().context("Failed to execute command")?;

        let mut stdout = child.stdout.take().context("Failed to capture stdout")?;
        let mut stderr = child.stderr.take().context("Failed to capture stderr")?;

        let mut stdout_buf = Vec::new();
        let mut stderr_buf = Vec::new();
        let mut stdout_chunk = [0u8; 4096];
        let mut stderr_chunk = [0u8; 4096];

        let idle_duration = opts.timeout_seconds.map(std::time::Duration::from_secs);
        let hard_limit = opts
            .timeout_seconds
            .map(|s| std::time::Duration::from_secs((s * 5).max(600)));
        let start_instant = std::time::Instant::now();

        let mut stdout_done = false;
        let mut stderr_done = false;

        while !stdout_done || !stderr_done {
            let idle_sleep = match idle_duration {
                Some(d) => tokio::time::sleep(d),
                None => tokio::time::sleep(std::time::Duration::from_secs(86400 * 365)),
            };
            tokio::pin!(idle_sleep);

            tokio::select! {
                res = tokio::io::AsyncReadExt::read(&mut stdout, &mut stdout_chunk), if !stdout_done => {
                    match res {
                        Ok(0) => {
                            stdout_done = true;
                        }
                        Ok(n) => {
                            stdout_buf.extend_from_slice(&stdout_chunk[..n]);
                        }
                        Err(_) => {
                            stdout_done = true;
                        }
                    }
                }
                res = tokio::io::AsyncReadExt::read(&mut stderr, &mut stderr_chunk), if !stderr_done => {
                    match res {
                        Ok(0) => {
                            stderr_done = true;
                        }
                        Ok(n) => {
                            stderr_buf.extend_from_slice(&stderr_chunk[..n]);
                        }
                        Err(_) => {
                            stderr_done = true;
                        }
                    }
                }
                _ = &mut idle_sleep => {
                    let _ = child.kill().await;
                    let secs = opts.timeout_seconds.unwrap_or(0);
                    return Err(anyhow!("Command timed out after {}s of inactivity: {}", secs, cmd));
                }
            }

            if let Some(limit) = hard_limit {
                if start_instant.elapsed() > limit {
                    let _ = child.kill().await;
                    return Err(anyhow!(
                        "Command exceeded maximum total duration limit of {}s: {}",
                        limit.as_secs(),
                        cmd
                    ));
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
        let id = SnapshotId(format!("snap-{}-{}-{}", label, count, Uuid::new_v4()));
        let snapshot_dir = std::env::temp_dir().join(format!("goose_snap_{}", id.0));

        if self.root.exists() {
            Self::copy_dir_all(&self.root, &snapshot_dir).await?;
        } else {
            fs::create_dir_all(&snapshot_dir).await?;
        }

        self.snapshots.insert(id.clone(), snapshot_dir);
        Ok(id)
    }

    async fn restore_snapshot(&mut self, id: &SnapshotId) -> Result<()> {
        let snap_dir = self
            .snapshots
            .get(id)
            .ok_or_else(|| anyhow!("Snapshot not found: {:?}", id))?;

        if self.root.exists() {
            fs::remove_dir_all(&self.root).await?;
        }
        Self::copy_dir_all(snap_dir, &self.root).await?;
        Ok(())
    }

    async fn cleanup(&mut self) -> Result<()> {
        for (_, path) in self.snapshots.drain() {
            if path.exists() {
                let _ = fs::remove_dir_all(path).await;
            }
        }
        if let Some(td) = self.temp_dir.take() {
            let _ = td.close();
        }
        Ok(())
    }
}
