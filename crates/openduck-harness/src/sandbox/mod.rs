pub mod container;
pub mod local;

use crate::types::{ExecOptions, ExecOutput, SnapshotId};
use anyhow::Result;
use async_trait::async_trait;
use std::path::Path;

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

#[derive(Debug, Clone)]
pub enum SandboxKind {
    Local,
    Container { image: String },
}
