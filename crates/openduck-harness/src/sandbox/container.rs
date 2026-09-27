use crate::sandbox::SandboxDriver;
use crate::types::{ExecOptions, ExecOutput, SnapshotId};
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use tokio::process::Command;
use uuid::Uuid;

pub struct ContainerSandbox {
    image: String,
    container_name: String,
    workdir: PathBuf,
    is_running: bool,
}

impl ContainerSandbox {
    pub fn new(image: impl Into<String>, workdir: PathBuf) -> Self {
        let container_name = format!("openduck-harness-{}", Uuid::new_v4());
        Self {
            image: image.into(),
            container_name,
            workdir,
            is_running: false,
        }
    }
}

#[async_trait]
impl SandboxDriver for ContainerSandbox {
    fn name(&self) -> &str {
        "container-sandbox"
    }

    fn workspace_root(&self) -> &Path {
        &self.workdir
    }

    async fn initialize(&mut self) -> Result<()> {
        let status = Command::new("docker")
            .args([
                "run",
                "-d",
                "--name",
                &self.container_name,
                "-w",
                self.workdir.to_string_lossy().as_ref(),
                &self.image,
                "tail",
                "-f",
                "/dev/null",
            ])
            .status()
            .await
            .context("Failed to start docker container")?;

        if !status.success() {
            return Err(anyhow!(
                "Docker container startup failed with status: {:?}",
                status
            ));
        }

        self.is_running = true;
        Ok(())
    }

    async fn exec_command(&self, cmd: &str, opts: &ExecOptions) -> Result<ExecOutput> {
        if !self.is_running {
            return Err(anyhow!("Container is not initialized or running"));
        }

        let mut command = Command::new("docker");
        command.arg("exec");

        if let Some(ref cwd) = opts.working_dir {
            command.arg("-w").arg(cwd.to_string_lossy().as_ref());
        }

        for (k, v) in &opts.env {
            command.arg("-e").arg(format!("{}={}", k, v));
        }

        command.arg(&self.container_name).args(["sh", "-c", cmd]);

        let output = if let Some(secs) = opts.timeout_seconds {
            tokio::time::timeout(std::time::Duration::from_secs(secs), command.output())
                .await
                .map_err(|_| anyhow!("Command in container timed out after {}s", secs))?
                .context("Failed to execute command in container")?
        } else {
            command
                .output()
                .await
                .context("Failed to execute command in container")?
        };

        Ok(ExecOutput {
            exit_code: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        })
    }

    async fn read_file(&self, path: &Path) -> Result<Vec<u8>> {
        let path_str = path.to_string_lossy();
        let target = format!("{}:{}", self.container_name, path_str);
        let temp_file = tempfile::NamedTempFile::new()?;
        let temp_path = temp_file.path().to_string_lossy().to_string();

        let status = Command::new("docker")
            .args(["cp", &target, &temp_path])
            .status()
            .await
            .context("Failed to copy file from container")?;

        if !status.success() {
            return Err(anyhow!("Failed to read file {} from container", path_str));
        }

        tokio::fs::read(temp_file.path())
            .await
            .context("Failed to read copied file")
    }

    async fn write_file(&self, path: &Path, content: &[u8]) -> Result<()> {
        let path_str = path.to_string_lossy();
        let target = format!("{}:{}", self.container_name, path_str);
        let temp_file = tempfile::NamedTempFile::new()?;
        tokio::fs::write(temp_file.path(), content).await?;
        let temp_path = temp_file.path().to_string_lossy().to_string();

        let status = Command::new("docker")
            .args(["cp", &temp_path, &target])
            .status()
            .await
            .context("Failed to copy file to container")?;

        if !status.success() {
            return Err(anyhow!("Failed to write file {} into container", path_str));
        }

        Ok(())
    }

    async fn snapshot(&mut self, label: &str) -> Result<SnapshotId> {
        let commit_tag = format!("snap-{}-{}", label, Uuid::new_v4());
        let status = Command::new("docker")
            .args(["commit", &self.container_name, &commit_tag])
            .status()
            .await
            .context("Failed to snapshot container")?;

        if !status.success() {
            return Err(anyhow!("Failed to commit container snapshot"));
        }

        Ok(SnapshotId(commit_tag))
    }

    async fn restore_snapshot(&mut self, id: &SnapshotId) -> Result<()> {
        self.cleanup().await?;
        self.image = id.0.clone();
        self.initialize().await
    }

    async fn cleanup(&mut self) -> Result<()> {
        if self.is_running {
            let _ = Command::new("docker")
                .args(["rm", "-f", &self.container_name])
                .status()
                .await;
            self.is_running = false;
        }
        Ok(())
    }
}
