//! Discovery, parsing, and management of Global and Project-Specific Rules.
//!
//! Rules provide declarative guidelines, coding standards, and safety
//! protocols for AI agent sessions. Rules can be defined globally in user
//! configuration directories or locally within a project/repository.

pub mod matcher;

pub use matcher::RuleMatcher;

use crate::config::paths::Paths;
use crate::source_roots::SourceRoot;
use crate::sources::parse_frontmatter;
use agent_client_protocol::Error;
use anyhow::{Context, Result};
use openduck_sdk_types::custom_requests::{SourceEntry, SourceType};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RuleFrontmatter {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub globs: Vec<String>,
    #[serde(default)]
    pub always_apply: Option<bool>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub order: Option<i32>,
    #[serde(default, flatten)]
    pub properties: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Rule {
    pub name: String,
    pub description: String,
    pub globs: Vec<String>,
    pub always_apply: bool,
    pub tags: Vec<String>,
    pub order: i32,
    pub path: PathBuf,
    pub global: bool,
    pub writable: bool,
    pub content: String,
    pub properties: HashMap<String, serde_json::Value>,
}

impl Rule {
    /// Parse a rule file from its markdown content and path.
    pub fn parse(content: &str, path: &Path, global: bool, writable: bool) -> Result<Self> {
        let trimmed = content.trim_start();
        if trimmed.starts_with("---") {
            let normalized = trimmed.replace("\r\n", "\n");
            if let Ok(Some((frontmatter, body))) = parse_frontmatter::<RuleFrontmatter>(&normalized)
            {
                let file_stem = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("rule")
                    .to_string();

                let name = match frontmatter.name {
                    Some(declared) if !declared.trim().is_empty() => declared,
                    _ => file_stem,
                };

                let always_apply = frontmatter
                    .always_apply
                    .unwrap_or(frontmatter.globs.is_empty());

                return Ok(Rule {
                    name,
                    description: frontmatter.description,
                    globs: frontmatter.globs,
                    always_apply,
                    tags: frontmatter.tags,
                    order: frontmatter.order.unwrap_or(0),
                    path: path.to_path_buf(),
                    global,
                    writable,
                    content: body.trim().to_string(),
                    properties: frontmatter.properties,
                });
            }
        }

        // Fallback for plain markdown files without YAML frontmatter (e.g. AGENTS.md)
        let file_stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("rule")
            .to_string();

        Ok(Rule {
            name: file_stem,
            description: String::new(),
            globs: Vec::new(),
            always_apply: true,
            tags: Vec::new(),
            order: 0,
            path: path.to_path_buf(),
            global,
            writable,
            content: content.trim().to_string(),
            properties: HashMap::new(),
        })
    }

    /// Load a rule directly from a file on disk.
    pub fn from_path(path: &Path, global: bool, writable: bool) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read rule file at {}", path.display()))?;
        Self::parse(&content, path, global, writable)
    }

    /// Convert rule into an ACP-compatible SourceEntry.
    pub fn to_source_entry(&self) -> SourceEntry {
        let mut props = self.properties.clone();
        if !self.globs.is_empty() {
            props.insert("globs".to_string(), serde_json::json!(self.globs));
        }
        if !self.tags.is_empty() {
            props.insert("tags".to_string(), serde_json::json!(self.tags));
        }
        props.insert(
            "alwaysApply".to_string(),
            serde_json::json!(self.always_apply),
        );
        props.insert("order".to_string(), serde_json::json!(self.order));

        SourceEntry {
            source_type: SourceType::Rule,
            name: self.name.clone(),
            description: self.description.clone(),
            content: self.content.clone(),
            path: self.path.to_string_lossy().to_string(),
            global: self.global,
            writable: self.writable,
            supporting_files: Vec::new(),
            properties: props,
        }
    }

    /// Build Markdown string with YAML frontmatter for storing on disk.
    #[allow(clippy::too_many_arguments)]
    pub fn build_markdown(
        name: &str,
        description: &str,
        content: &str,
        globs: &[String],
        always_apply: Option<bool>,
        tags: &[String],
        order: Option<i32>,
        properties: &HashMap<String, serde_json::Value>,
    ) -> Result<String, Error> {
        let mut frontmatter = serde_yaml::Mapping::new();
        frontmatter.insert(
            serde_yaml::Value::String("name".into()),
            serde_yaml::Value::String(name.into()),
        );
        if !description.is_empty() {
            frontmatter.insert(
                serde_yaml::Value::String("description".into()),
                serde_yaml::Value::String(description.into()),
            );
        }
        if !globs.is_empty() {
            let globs_val = serde_yaml::to_value(globs).map_err(|e| {
                Error::internal_error().data(format!("Failed to serialize rule globs: {e}"))
            })?;
            frontmatter.insert(serde_yaml::Value::String("globs".into()), globs_val);
        }
        if let Some(always) = always_apply {
            frontmatter.insert(
                serde_yaml::Value::String("always_apply".into()),
                serde_yaml::Value::Bool(always),
            );
        }
        if !tags.is_empty() {
            let tags_val = serde_yaml::to_value(tags).map_err(|e| {
                Error::internal_error().data(format!("Failed to serialize rule tags: {e}"))
            })?;
            frontmatter.insert(serde_yaml::Value::String("tags".into()), tags_val);
        }
        if let Some(ord) = order {
            frontmatter.insert(
                serde_yaml::Value::String("order".into()),
                serde_yaml::Value::Number(ord.into()),
            );
        }

        for (key, value) in properties {
            if matches!(
                key.as_str(),
                "name"
                    | "description"
                    | "globs"
                    | "always_apply"
                    | "alwaysApply"
                    | "tags"
                    | "order"
            ) {
                continue;
            }
            let val = serde_yaml::to_value(value).map_err(|e| {
                Error::internal_error().data(format!("Failed to serialize property '{key}': {e}"))
            })?;
            frontmatter.insert(serde_yaml::Value::String(key.clone()), val);
        }

        let yaml = serde_yaml::to_string(&frontmatter)
            .map_err(|e| Error::internal_error().data(format!("Failed to serialize YAML: {e}")))?;

        let mut output = format!("---\n{}---\n", yaml);
        if !content.is_empty() {
            output.push('\n');
            output.push_str(content.trim());
            output.push('\n');
        }
        Ok(output)
    }
}

pub fn canonicalize_or_original(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

pub fn global_rules_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    dirs.push(Paths::in_agents_home_dir("rules"));
    dirs.push(Paths::in_config_dir("rules"));

    if let Some(home) = dirs::home_dir() {
        dirs.push(home.join(".agents").join("rules"));
        dirs.push(home.join(".openduck").join("rules"));
        dirs.push(home.join(".goose").join("rules"));
        dirs.push(home.join(".gemini").join("config").join("rules"));
        dirs.push(home.join(".claude").join("rules"));
    }

    let mut unique = Vec::new();
    let mut seen = HashSet::new();
    for dir in dirs {
        let canonical = canonicalize_or_original(&dir);
        if seen.insert(canonical) {
            unique.push(dir);
        }
    }
    unique
}

pub fn project_rules_dirs(project_dir: &Path) -> Vec<PathBuf> {
    vec![
        project_dir.join(".agents").join("rules"),
        project_dir.join(".openduck").join("rules"),
        project_dir.join(".goose").join("rules"),
        project_dir.join(".cursor").join("rules"),
        project_dir.join(".claude").join("rules"),
    ]
}

pub fn is_global_rule_file(path: &Path) -> bool {
    let canonical = canonicalize_or_original(path);
    for dir in global_rules_dirs() {
        if canonical.starts_with(canonicalize_or_original(&dir)) {
            return true;
        }
    }
    false
}

/// Discover all rules (global and project-specific).
pub fn discover_rules(working_dir: Option<&Path>, additional_roots: &[SourceRoot]) -> Vec<Rule> {
    let mut rules_map = HashMap::<String, Rule>::new();
    let mut seen_paths = HashSet::<PathBuf>::new();

    // 1. Global rules
    for dir in global_rules_dirs() {
        if !dir.is_dir() {
            continue;
        }
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("md") {
                    let canonical = canonicalize_or_original(&path);
                    if seen_paths.insert(canonical) {
                        if let Ok(rule) = Rule::from_path(&path, true, true) {
                            rules_map.insert(rule.name.clone(), rule);
                        }
                    }
                }
            }
        }
    }

    // 2. Additional global/read-only source roots
    for root in additional_roots {
        let dir = &root.path;
        if !dir.is_dir() {
            continue;
        }
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("md") {
                    let canonical = canonicalize_or_original(&path);
                    if seen_paths.insert(canonical) {
                        if let Ok(rule) = Rule::from_path(&path, true, root.writable) {
                            rules_map.insert(rule.name.clone(), rule);
                        }
                    }
                }
            }
        }
    }

    // 3. Project rules (if working_dir provided)
    if let Some(working_dir) = working_dir {
        // Collect ancestor directories up to git root (or root)
        let mut ancestors = Vec::new();
        let mut curr = working_dir;
        loop {
            ancestors.push(curr.to_path_buf());
            if curr.join(".git").exists() {
                break;
            }
            if let Some(parent) = curr.parent() {
                curr = parent;
            } else {
                break;
            }
        }
        // Iterate ancestors in top-down order so closer directories override parent ones
        ancestors.reverse();

        for dir in ancestors {
            for rules_dir in project_rules_dirs(&dir) {
                if !rules_dir.is_dir() {
                    continue;
                }
                if let Ok(entries) = std::fs::read_dir(&rules_dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("md")
                        {
                            let canonical = canonicalize_or_original(&path);
                            if seen_paths.insert(canonical) {
                                if let Ok(rule) = Rule::from_path(&path, false, true) {
                                    // Project rule overrides global rule of same name
                                    rules_map.insert(rule.name.clone(), rule);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    let mut result: Vec<Rule> = rules_map.into_values().collect();
    result.sort_by(|a, b| {
        a.order
            .cmp(&b.order)
            .then_with(|| a.global.cmp(&b.global))
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.path.cmp(&b.path))
    });
    result
}

/// Format active rules into markdown for system prompt injection.
pub fn format_rules_for_prompt(rules: &[Rule]) -> String {
    if rules.is_empty() {
        return String::new();
    }

    let mut global_rules = Vec::new();
    let mut project_rules = Vec::new();

    for rule in rules {
        if rule.global {
            global_rules.push(rule);
        } else {
            project_rules.push(rule);
        }
    }

    let mut out = String::new();
    out.push_str("# Rules\n\n");
    out.push_str("Follow these user and project rules at all times.\n");

    if !global_rules.is_empty() {
        out.push_str("\n## Global Rules\n\n");
        for rule in global_rules {
            out.push_str(&format!("### {}\n", rule.name));
            if !rule.description.is_empty() {
                out.push_str(&format!("*{ }*\n\n", rule.description));
            }
            out.push_str(&rule.content);
            out.push_str("\n\n");
        }
    }

    if !project_rules.is_empty() {
        out.push_str("\n## Project Rules\n\n");
        for rule in project_rules {
            out.push_str(&format!("### {}\n", rule.name));
            if !rule.description.is_empty() {
                out.push_str(&format!("*{ }*\n\n", rule.description));
            }
            out.push_str(&rule.content);
            out.push_str("\n\n");
        }
    }

    out.trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_parse_rule_with_frontmatter() {
        let content = r#"---
name: rust-rules
description: Standards for Rust development
globs: ["**/*.rs"]
always_apply: true
tags: ["rust", "style"]
order: 5
---
- Use anyhow::Result
- Never add unnecessary comments
"#;
        let path = Path::new("/workspace/.agents/rules/rust-rules.md");
        let rule = Rule::parse(content, path, false, true).unwrap();

        assert_eq!(rule.name, "rust-rules");
        assert_eq!(rule.description, "Standards for Rust development");
        assert_eq!(rule.globs, vec!["**/*.rs"]);
        assert!(rule.always_apply);
        assert_eq!(rule.tags, vec!["rust", "style"]);
        assert_eq!(rule.order, 5);
        assert_eq!(
            rule.content,
            "- Use anyhow::Result\n- Never add unnecessary comments"
        );
        assert!(!rule.global);
        assert!(rule.writable);
    }

    #[test]
    fn test_parse_plain_markdown_rule() {
        let content = "Guidelines without frontmatter";
        let path = Path::new("/workspace/AGENTS.md");
        let rule = Rule::parse(content, path, false, true).unwrap();

        assert_eq!(rule.name, "AGENTS");
        assert!(rule.always_apply);
        assert_eq!(rule.content, "Guidelines without frontmatter");
    }

    #[test]
    fn test_project_rule_overrides_global_rule_by_name() {
        let temp_dir = TempDir::new().unwrap();
        let global_dir = temp_dir.path().join("global_rules");
        let project_dir = temp_dir.path().join("project");
        let project_rules_dir = project_dir.join(".agents").join("rules");

        std::fs::create_dir_all(&global_dir).unwrap();
        std::fs::create_dir_all(&project_rules_dir).unwrap();

        let global_rule_path = global_dir.join("style.md");
        std::fs::write(
            &global_rule_path,
            "---\nname: style\n---\nGlobal style rule",
        )
        .unwrap();

        let project_rule_path = project_rules_dir.join("style.md");
        std::fs::write(
            &project_rule_path,
            "---\nname: style\n---\nProject style rule",
        )
        .unwrap();

        let global_root = SourceRoot {
            path: global_dir,
            writable: true,
        };

        let rules = discover_rules(Some(&project_dir), &[global_root]);
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].name, "style");
        assert_eq!(rules[0].content, "Project style rule");
        assert!(!rules[0].global);
    }
}
