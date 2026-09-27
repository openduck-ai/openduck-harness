use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

use super::insight::{is_forbidden_system_path, operator_home, path_is_inside};

const MAX_TAGS: usize = 16;
const MAX_TAG_LENGTH: usize = 32;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ProjectKind {
    Software,
    Docs,
    Automation,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ProjectStatus {
    Active,
    Paused,
    Archived,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectMetadata {
    pub working_dirs: Vec<String>,
    pub kind: ProjectKind,
    pub status: ProjectStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email_recipients: Option<Vec<String>>,
}

pub fn validate_project_metadata(metadata: &ProjectMetadata) -> Result<()> {
    if metadata.working_dirs.is_empty() {
        return Err(anyhow!("workingDirs must contain at least one directory"));
    }
    for directory in &metadata.working_dirs {
        validate_working_directory(Path::new(directory))?;
    }
    if metadata.tags.len() > MAX_TAGS {
        return Err(anyhow!("tags must contain at most {MAX_TAGS} entries"));
    }
    for tag in &metadata.tags {
        if tag.is_empty()
            || tag.len() > MAX_TAG_LENGTH
            || !tag
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        {
            return Err(anyhow!(
                "tag '{tag}' must match [a-z0-9-]{{1,{MAX_TAG_LENGTH}}}"
            ));
        }
    }
    if let Some(language) = metadata.language.as_deref() {
        if language.trim().is_empty() || language.len() > 64 {
            return Err(anyhow!("language must be between 1 and 64 characters"));
        }
    }
    Ok(())
}

pub fn validate_project_slug(slug: &str) -> Result<()> {
    if slug.is_empty()
        || slug.len() > 80
        || !slug
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        || slug.starts_with('-')
        || slug.ends_with('-')
        || slug.contains("--")
    {
        return Err(anyhow!("invalid project slug '{slug}'"));
    }
    Ok(())
}

pub fn home_jail_enabled() -> bool {
    match std::env::var("GOOSE_CONTROL_HOME_JAIL") {
        Ok(value) => value != "0",
        Err(_) => true,
    }
}

pub fn validate_working_directory(path: &Path) -> Result<PathBuf> {
    if !path.is_absolute() {
        return Err(anyhow!("project working directory must be absolute"));
    }
    if path.as_os_str().to_string_lossy().contains('\0') {
        return Err(anyhow!("project working directory contains a NUL byte"));
    }
    let canonical = path
        .canonicalize()
        .map_err(|error| anyhow!("project working directory is not accessible: {error}"))?;
    if !canonical.is_dir() {
        return Err(anyhow!("project working directory is not a directory"));
    }
    if is_forbidden_system_path(&canonical) {
        return Err(anyhow!(
            "project working directory cannot be a system path such as /proc, /sys, or /dev"
        ));
    }
    Ok(canonical)
}

pub fn validate_working_directory_with_policy(
    path: &Path,
    allow_untrusted_path: bool,
) -> Result<PathBuf> {
    let canonical = validate_working_directory(path)?;
    if home_jail_enabled() && !allow_untrusted_path {
        if let Some(home) = operator_home() {
            if !path_is_inside(&canonical, &home) {
                return Err(anyhow!(
                    "project working directory is outside $HOME; pass allowUntrustedPath to override"
                ));
            }
        }
    }
    Ok(canonical)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn metadata(path: &Path) -> ProjectMetadata {
        ProjectMetadata {
            working_dirs: vec![path.to_string_lossy().into_owned()],
            kind: ProjectKind::Software,
            status: ProjectStatus::Active,
            language: Some("rust".into()),
            tags: vec!["backend".into()],
            email_recipients: None,
        }
    }

    #[test]
    fn validates_project_metadata_and_canonical_paths() {
        let dir = tempdir().unwrap();
        assert!(validate_project_metadata(&metadata(dir.path())).is_ok());
    }

    #[test]
    fn rejects_relative_missing_and_file_paths() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("file");
        std::fs::write(&file, "content").unwrap();
        assert!(validate_working_directory(Path::new("relative")).is_err());
        assert!(validate_working_directory(&dir.path().join("missing")).is_err());
        assert!(validate_working_directory(&file).is_err());
    }

    #[test]
    fn validates_slug_and_tags() {
        assert!(validate_project_slug("acme-api").is_ok());
        assert!(validate_project_slug("Acme").is_err());
        assert!(validate_project_slug("a--b").is_err());
        let dir = tempdir().unwrap();
        let mut value = metadata(dir.path());
        value.tags = vec!["Not-Lowercase".into()];
        assert!(validate_project_metadata(&value).is_err());
    }

    #[test]
    fn home_jail_rejects_paths_outside_home() {
        let dir = tempdir().unwrap();
        let previous_home = std::env::var_os("HOME");
        let previous_jail = std::env::var_os("GOOSE_CONTROL_HOME_JAIL");
        std::env::set_var("HOME", dir.path());
        std::env::set_var("GOOSE_CONTROL_HOME_JAIL", "1");
        let outside = tempdir().unwrap();
        let err = validate_working_directory_with_policy(outside.path(), false).unwrap_err();
        assert!(err.to_string().contains("allowUntrustedPath"));
        assert!(validate_working_directory_with_policy(outside.path(), true).is_ok());
        match previous_home {
            Some(value) => std::env::set_var("HOME", value),
            None => std::env::remove_var("HOME"),
        }
        match previous_jail {
            Some(value) => std::env::set_var("GOOSE_CONTROL_HOME_JAIL", value),
            None => std::env::remove_var("GOOSE_CONTROL_HOME_JAIL"),
        }
    }
}
