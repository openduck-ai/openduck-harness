use crate::eval::task::TaskSpec;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::Path;
use tokio::fs;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecordMode {
    Record,
    Replay,
    Passthrough,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CassetteFrame {
    pub key: String,
    pub query: String,
    pub response: String,
    pub is_tool: bool,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cassette {
    pub name: String,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub task_spec: Option<TaskSpec>,
    pub frames: HashMap<String, CassetteFrame>,
}

impl Cassette {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            created_at: Utc::now(),
            task_spec: None,
            frames: HashMap::new(),
        }
    }

    pub fn hash_key(query: &str, is_tool: bool) -> String {
        let mut hasher = Sha256::new();
        if is_tool {
            hasher.update(b"tool:");
        } else {
            hasher.update(b"llm:");
        }
        hasher.update(query.as_bytes());
        let hash = hasher.finalize();
        hash.iter().map(|b| format!("{:02x}", b)).collect()
    }

    pub fn record(&mut self, query: &str, response: &str, is_tool: bool) {
        let key = Self::hash_key(query, is_tool);
        let frame = CassetteFrame {
            key: key.clone(),
            query: query.to_string(),
            response: response.to_string(),
            is_tool,
            timestamp: Utc::now(),
        };
        self.frames.insert(key, frame);
    }

    pub fn replay(&self, query: &str, is_tool: bool) -> Option<&CassetteFrame> {
        let key = Self::hash_key(query, is_tool);
        self.frames.get(&key)
    }

    pub async fn save_to_file(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }
        let json =
            serde_json::to_string_pretty(self).context("Failed to serialize cassette to JSON")?;
        fs::write(path, json)
            .await
            .with_context(|| format!("Failed to save cassette to {:?}", path))?;
        Ok(())
    }

    pub async fn load_from_file(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path)
            .await
            .with_context(|| format!("Failed to read cassette file {:?}", path))?;
        let cassette: Self = serde_json::from_str(&content)
            .with_context(|| format!("Failed to deserialize cassette from {:?}", path))?;
        Ok(cassette)
    }
}
