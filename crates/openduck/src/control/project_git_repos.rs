//! Per-project custom git repository registrations.
//!
//! Stores additional git repository roots explicitly attached to a project from
//! the UI hub, complementing automatic repository discovery.

use std::collections::BTreeMap;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use anyhow::{anyhow, bail, Result};
use serde::{Deserialize, Serialize};

use super::insight::path_is_inside;
use super::projects::{validate_working_directory, validate_working_directory_with_policy};

#[derive(Debug, Default, Serialize, Deserialize)]
struct ProjectGitReposFile {
    #[serde(default)]
    projects: BTreeMap<String, Vec<String>>,
}

pub struct ProjectGitReposStore {
    path: PathBuf,
}

impl ProjectGitReposStore {
    pub fn from_config() -> Self {
        Self {
            path: crate::config::paths::Paths::in_config_dir("project-git-repos.json"),
        }
    }

    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn list(&self, slug: &str) -> Result<Vec<PathBuf>> {
        let _guard = lock_store()?;
        let data = self.read_unlocked()?;
        Ok(data
            .projects
            .get(slug)
            .map(|entries| entries.iter().map(PathBuf::from).collect())
            .unwrap_or_default())
    }

    pub fn validate_candidate_path(
        project_root: &Path,
        input_path: &str,
        allow_untrusted_path: bool,
    ) -> Result<PathBuf> {
        let trimmed = input_path.trim();
        if trimmed.is_empty() {
            bail!("Enter a git repository path");
        }
        if trimmed.contains('\0') {
            bail!("Invalid path containing null bytes");
        }
        let raw = Path::new(trimmed);
        if raw.components().any(|c| c.as_os_str() == ".git") {
            bail!("Access to git internal directories (.git) is not allowed");
        }

        let candidate = if raw.is_absolute() {
            raw.to_path_buf()
        } else {
            project_root.join(trimmed)
        };

        let inside_project = match (project_root.canonicalize(), candidate.canonicalize()) {
            (Ok(proj_canon), Ok(cand_canon)) => path_is_inside(&cand_canon, &proj_canon),
            _ => false,
        };

        if inside_project {
            validate_working_directory(&candidate)
        } else {
            validate_working_directory_with_policy(&candidate, allow_untrusted_path)
        }
    }

    pub fn add_validated(&self, slug: &str, canonical_repo: &Path) -> Result<()> {
        let _guard = lock_store()?;
        let mut data = self.read_unlocked()?;
        let entries = data.projects.entry(slug.to_string()).or_default();
        let canonical_str = canonical_repo.to_string_lossy().to_string();
        let already_present = entries.iter().any(|existing| {
            Path::new(existing)
                .canonicalize()
                .map(|c| c == canonical_repo)
                .unwrap_or(existing == &canonical_str)
        });
        if !already_present {
            entries.push(canonical_str);
            entries.sort();
            self.write_unlocked(&data)?;
        }
        Ok(())
    }

    pub fn remove(&self, slug: &str, project_root: &Path, target: &str) -> Result<bool> {
        let trimmed = target.trim();
        if trimmed.is_empty() {
            return Ok(false);
        }
        let target_path = if Path::new(trimmed).is_absolute() {
            PathBuf::from(trimmed)
        } else {
            project_root.join(trimmed)
        };
        let target_canon = target_path.canonicalize().ok();

        let _guard = lock_store()?;
        let mut data = self.read_unlocked()?;
        let Some(entries) = data.projects.get_mut(slug) else {
            return Ok(false);
        };
        let before = entries.len();
        entries.retain(|existing| {
            if existing == trimmed {
                return false;
            }
            if let Some(ref tc) = target_canon {
                if let Ok(ec) = Path::new(existing).canonicalize() {
                    if &ec == tc {
                        return false;
                    }
                }
            }
            true
        });
        let removed = entries.len() != before;
        if entries.is_empty() {
            data.projects.remove(slug);
        }
        if removed {
            self.write_unlocked(&data)?;
        }
        Ok(removed)
    }

    pub fn resolve(&self, slug: &str, project_root: &Path, rel_or_path: &str) -> Option<PathBuf> {
        let trimmed = rel_or_path.trim();
        if trimmed.is_empty() {
            return None;
        }
        let entries = self.list(slug).ok()?;
        if entries.is_empty() {
            return None;
        }

        let candidate = if Path::new(trimmed).is_absolute() {
            PathBuf::from(trimmed)
        } else {
            project_root.join(trimmed)
        };
        let candidate_canon = candidate.canonicalize().ok();
        let root_canon = project_root.canonicalize().ok();

        for entry in entries {
            if entry.to_string_lossy() == trimmed {
                return Some(entry);
            }
            if let Ok(entry_canon) = entry.canonicalize() {
                if candidate_canon.as_ref() == Some(&entry_canon) {
                    return Some(entry_canon);
                }
                if let Some(ref rc) = root_canon {
                    if let Ok(rel) = entry_canon.strip_prefix(rc) {
                        let rel_norm = rel
                            .to_string_lossy()
                            .replace('\\', "/")
                            .trim_matches('/')
                            .to_string();
                        if rel_norm == trimmed.replace('\\', "/").trim_matches('/') {
                            return Some(entry_canon);
                        }
                    }
                }
            }
        }
        None
    }

    fn read_unlocked(&self) -> Result<ProjectGitReposFile> {
        match std::fs::read_to_string(&self.path) {
            Ok(content) => {
                if content.trim().is_empty() {
                    return Ok(ProjectGitReposFile::default());
                }
                serde_json::from_str(&content)
                    .map_err(|e| anyhow!("invalid project git repos config: {e}"))
            }
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(ProjectGitReposFile::default()),
            Err(e) => Err(anyhow!("failed to read project git repos config: {e}")),
        }
    }

    fn write_unlocked(&self, data: &ProjectGitReposFile) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let serialized = serde_json::to_string_pretty(data)?;
        std::fs::write(&self.path, format!("{serialized}\n"))?;
        Ok(())
    }
}

fn lock_store() -> Result<std::sync::MutexGuard<'static, ()>> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| anyhow!("project git repos lock poisoned"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn add_list_resolve_and_remove_custom_repo() {
        let dir = tempdir().unwrap();
        let project_root = dir.path().join("project");
        let repo = dir.path().join("external_repo");
        std::fs::create_dir_all(&project_root).unwrap();
        std::fs::create_dir_all(&repo).unwrap();

        let store = ProjectGitReposStore::new(dir.path().join("project-git-repos.json"));
        let validated = ProjectGitReposStore::validate_candidate_path(
            &project_root,
            repo.to_str().unwrap(),
            true,
        )
        .unwrap();
        store.add_validated("my-proj", &validated).unwrap();

        let listed = store.list("my-proj").unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0], validated);

        let resolved = store
            .resolve("my-proj", &project_root, &validated.to_string_lossy())
            .unwrap();
        assert_eq!(resolved, validated);

        assert!(store
            .remove("my-proj", &project_root, &validated.to_string_lossy())
            .unwrap());
        assert!(store.list("my-proj").unwrap().is_empty());
    }

    #[test]
    fn inside_project_path_does_not_require_allow_untrusted() {
        let dir = tempdir().unwrap();
        let project_root = dir.path().join("project");
        let nested = project_root.join("services/web");
        std::fs::create_dir_all(&nested).unwrap();

        let validated =
            ProjectGitReposStore::validate_candidate_path(&project_root, "services/web", false)
                .unwrap();
        assert_eq!(validated, nested.canonicalize().unwrap());
    }
}
