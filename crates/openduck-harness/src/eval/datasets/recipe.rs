use crate::eval::datasets::jsonl::BenchmarkItem;
use crate::eval::task::{CommandVerifier, TaskSpec, Verifier};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use tokio::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecipeSpec {
    pub title: Option<String>,
    pub description: Option<String>,
    pub instructions: Option<String>,
    pub prompt: Option<String>,
    pub verify_command: Option<String>,
}

pub async fn load_recipe_task(path: &Path) -> Result<BenchmarkItem> {
    let content = fs::read_to_string(path)
        .await
        .with_context(|| format!("Failed to read recipe file: {:?}", path))?;

    let recipe: RecipeSpec = serde_yaml::from_str(&content)
        .with_context(|| format!("Failed to parse YAML recipe: {:?}", path))?;

    let file_stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "recipe-task".to_string());

    let prompt = recipe
        .prompt
        .or(recipe.instructions)
        .or(recipe.description)
        .unwrap_or_else(|| "Execute task".to_string());

    let task = TaskSpec::new(file_stem, "goose-recipe", prompt);

    let verifier: Arc<dyn Verifier> = if let Some(cmd) = recipe.verify_command {
        Arc::new(CommandVerifier::new(cmd))
    } else {
        Arc::new(CommandVerifier::new("true"))
    };

    Ok(BenchmarkItem { task, verifier })
}
