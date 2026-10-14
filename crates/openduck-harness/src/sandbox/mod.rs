pub mod container;
pub mod local;
pub mod worktree;

pub use worktree::{UntrackedFileMode, WorktreeConfig, WorktreeSandbox};

use crate::types::{ExecOptions, ExecOutput, SnapshotId};
use anyhow::Result;
use async_trait::async_trait;
use std::path::{Path, PathBuf};

#[async_trait]
pub trait SandboxDriver: Send + Sync {
    fn name(&self) -> &str;
    fn workspace_root(&self) -> &Path;

    async fn initialize(&mut self) -> Result<()>;
    async fn exec_command(&self, cmd: &str, opts: &ExecOptions) -> Result<ExecOutput>;
    async fn read_file(&self, path: &Path) -> Result<Vec<u8>>;
    async fn write_file(&self, path: &Path, content: &[u8]) -> Result<()>;
    async fn snapshot(&mut self, label: &str) -> Result<SnapshotId>;
    async fn restore_snapshot(&mut self, id: &SnapshotId) -> Result<()>;
    async fn cleanup(&mut self) -> Result<()>;
}

#[async_trait]
impl SandboxDriver for Box<dyn SandboxDriver> {
    fn name(&self) -> &str {
        (**self).name()
    }

    fn workspace_root(&self) -> &Path {
        (**self).workspace_root()
    }

    async fn initialize(&mut self) -> Result<()> {
        (**self).initialize().await
    }

    async fn exec_command(&self, cmd: &str, opts: &ExecOptions) -> Result<ExecOutput> {
        (**self).exec_command(cmd, opts).await
    }

    async fn read_file(&self, path: &Path) -> Result<Vec<u8>> {
        (**self).read_file(path).await
    }

    async fn write_file(&self, path: &Path, content: &[u8]) -> Result<()> {
        (**self).write_file(path, content).await
    }

    async fn snapshot(&mut self, label: &str) -> Result<SnapshotId> {
        (**self).snapshot(label).await
    }

    async fn restore_snapshot(&mut self, id: &SnapshotId) -> Result<()> {
        (**self).restore_snapshot(id).await
    }

    async fn cleanup(&mut self) -> Result<()> {
        (**self).cleanup().await
    }
}

#[derive(Debug, Clone)]
pub enum SandboxKind {
    Local,
    Worktree {
        repo_root: PathBuf,
        branch: Option<String>,
    },
    Container {
        image: String,
    },
}
