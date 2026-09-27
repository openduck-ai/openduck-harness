//! Parent directories that new projects must live in.
//!
//! A project folder is a direct child of one of these roots. Roots are chosen
//! explicitly so registration cannot point at an arbitrary path.

use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

use super::projects::validate_working_directory_with_policy;

const MAX_CHILD_DIRECTORIES: usize = 500;
const MAX_DIRECTORY_NAME_BYTES: usize = 255;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectRoot {
    pub path: PathBuf,
    pub name: String,
    pub available: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectDirectory {
    pub name: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectDirectoryList {
    pub root: PathBuf,
    pub directories: Vec<ProjectDirectory>,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatedProjectDirectory {
    pub directory: ProjectDirectory,
    pub created: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct ProjectRootsFile {
    roots: Vec<String>,
}

pub struct ProjectRootsStore {
    path: PathBuf,
}

impl ProjectRootsStore {
    pub fn from_config() -> Self {
        Self {
            path: crate::config::paths::Paths::in_config_dir("project-roots.json"),
        }
    }

    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn list(&self) -> Result<Vec<ProjectRoot>> {
        let _guard = lock_store()?;
        Ok(self.read_unlocked()?.into_iter().map(to_root).collect())
    }

    pub fn add(&self, path: &Path, allow_untrusted_path: bool) -> Result<ProjectRoot> {
        let canonical = validate_working_directory_with_policy(path, allow_untrusted_path)?;
        reject_filesystem_root(&canonical)?;
        let _guard = lock_store()?;
        let mut roots = self.read_unlocked()?;
        if let Some(existing) = roots.iter().find(|root| *root == &canonical) {
            return Ok(to_root(existing.clone()));
        }
        roots.push(canonical.clone());
        roots.sort();
        self.write_unlocked(&roots)?;
        Ok(to_root(canonical))
    }

    pub fn remove(&self, path: &Path) -> Result<()> {
        let _guard = lock_store()?;
        let requested = canonical_or_original(path);
        let roots = self.read_unlocked()?;
        let next: Vec<PathBuf> = roots
            .iter()
            .filter(|root| *root != &requested && *root != path)
            .cloned()
            .collect();
        if next.len() != roots.len() {
            self.write_unlocked(&next)?;
        }
        Ok(())
    }

    pub fn list_directories(&self, root: &Path) -> Result<ProjectDirectoryList> {
        let root = {
            let _guard = lock_store()?;
            self.resolve_root_unlocked(root)?
        };
        read_child_directories(&root)
    }

    pub fn create_directory(&self, root: &Path, name: &str) -> Result<CreatedProjectDirectory> {
        validate_directory_name(name)?;
        let root = {
            let _guard = lock_store()?;
            self.resolve_root_unlocked(root)?
        };
        let path = root.join(name);
        if path.exists() {
            if path.is_dir() {
                return Ok(CreatedProjectDirectory {
                    directory: ProjectDirectory {
                        name: name.to_string(),
                        path,
                    },
                    created: false,
                });
            }
            return Err(anyhow!("a file with that name already exists"));
        }
        match std::fs::create_dir(&path) {
            Ok(()) => Ok(CreatedProjectDirectory {
                directory: ProjectDirectory {
                    name: name.to_string(),
                    path,
                },
                created: true,
            }),
            Err(error) if error.kind() == ErrorKind::AlreadyExists && path.is_dir() => {
                Ok(CreatedProjectDirectory {
                    directory: ProjectDirectory {
                        name: name.to_string(),
                        path,
                    },
                    created: false,
                })
            }
            Err(error) => Err(anyhow!("unable to create project directory: {error}")),
        }
    }

    pub fn require_child_of_root(&self, path: &Path) -> Result<PathBuf> {
        let canonical = super::projects::validate_working_directory(path)?;
        let parent = canonical.parent().map(Path::to_path_buf).ok_or_else(|| {
            anyhow!("project directory must be a folder directly inside a configured project root")
        })?;
        let _guard = lock_store()?;
        let roots = self.read_unlocked()?;
        if roots.is_empty() {
            return Err(anyhow!(
                "configure a project root directory before registering a project"
            ));
        }
        if roots.iter().any(|root| root == &parent) {
            Ok(canonical)
        } else {
            Err(anyhow!(
                "project directory must be a folder directly inside a configured project root"
            ))
        }
    }

    fn resolve_root_unlocked(&self, requested: &Path) -> Result<PathBuf> {
        let requested_canonical = canonical_or_original(requested);
        let roots = self.read_unlocked()?;
        match roots
            .into_iter()
            .find(|root| root == &requested_canonical || root == requested)
        {
            Some(root) if root.is_dir() => Ok(root),
            Some(_) => Err(anyhow!("project root is not accessible")),
            None => Err(anyhow!("directory is not a configured project root")),
        }
    }

    fn read_unlocked(&self) -> Result<Vec<PathBuf>> {
        let bytes = match std::fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error.into()),
        };
        let file: ProjectRootsFile = serde_json::from_slice(&bytes)?;
        let mut roots: Vec<PathBuf> = file
            .roots
            .into_iter()
            .map(|path| canonical_or_original(Path::new(&path)))
            .collect();
        roots.sort();
        roots.dedup();
        Ok(roots)
    }

    fn write_unlocked(&self, roots: &[PathBuf]) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = ProjectRootsFile {
            roots: roots
                .iter()
                .map(|path| path.to_string_lossy().into_owned())
                .collect(),
        };
        let mut bytes = serde_json::to_vec_pretty(&file)?;
        bytes.push(b'\n');
        let file_name = self
            .path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .ok_or_else(|| anyhow!("project roots path has no file name"))?;
        let tmp = self.path.with_file_name(format!("{file_name}.tmp"));
        std::fs::write(&tmp, bytes)?;
        std::fs::rename(&tmp, &self.path)?;
        Ok(())
    }
}

fn lock_store() -> Result<std::sync::MutexGuard<'static, ()>> {
    store_lock()
        .lock()
        .map_err(|_| anyhow!("project roots store is unavailable"))
}

fn store_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn canonical_or_original(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

fn reject_filesystem_root(path: &Path) -> Result<()> {
    if path.parent().is_none() {
        return Err(anyhow!("project root cannot be the filesystem root"));
    }
    Ok(())
}

fn validate_directory_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name.contains('/')
        || name.contains('\\')
        || name == "."
        || name == ".."
        || name.chars().any(|character| character.is_control())
    {
        return Err(anyhow!("enter a single folder name without slashes"));
    }
    if name.len() > MAX_DIRECTORY_NAME_BYTES {
        return Err(anyhow!("folder name is too long"));
    }
    Ok(())
}

fn to_root(path: PathBuf) -> ProjectRoot {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| path.to_string_lossy().into_owned());
    let available = path.is_dir();
    ProjectRoot {
        path,
        name,
        available,
    }
}

fn read_child_directories(root: &Path) -> Result<ProjectDirectoryList> {
    let mut directories = Vec::new();
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') || name.is_empty() {
            continue;
        }
        let path = root.join(&name);
        if !path.is_dir() {
            continue;
        }
        let Ok(canonical) = path.canonicalize() else {
            continue;
        };
        if canonical.parent() != Some(root) {
            continue;
        }
        directories.push(ProjectDirectory {
            path: canonical,
            name,
        });
    }
    directories.sort_by(|left, right| {
        left.name
            .to_lowercase()
            .cmp(&right.name.to_lowercase())
            .then_with(|| left.name.cmp(&right.name))
    });
    let truncated = directories.len() > MAX_CHILD_DIRECTORIES;
    directories.truncate(MAX_CHILD_DIRECTORIES);
    Ok(ProjectDirectoryList {
        root: root.to_path_buf(),
        directories,
        truncated,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::{OsStr, OsString};
    use tempfile::tempdir;

    struct EnvGuard {
        key: &'static str,
        previous: Option<OsString>,
    }

    impl EnvGuard {
        fn set(key: &'static str, value: impl AsRef<OsStr>) -> Self {
            let previous = std::env::var_os(key);
            std::env::set_var(key, value);
            Self { key, previous }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match self.previous.take() {
                Some(value) => std::env::set_var(self.key, value),
                None => std::env::remove_var(self.key),
            }
        }
    }

    fn store_at(dir: &Path) -> ProjectRootsStore {
        ProjectRootsStore::new(dir.join("project-roots.json"))
    }

    #[test]
    fn adds_lists_and_removes_canonical_roots() {
        let root = tempdir().unwrap();
        let store_dir = tempdir().unwrap();
        let store = store_at(store_dir.path());
        let added = store.add(root.path(), true).unwrap();
        assert_eq!(added.path, root.path().canonicalize().unwrap());
        assert!(added.available);
        assert_eq!(store.add(root.path(), true).unwrap().path, added.path);
        assert_eq!(store.list().unwrap().len(), 1);
        store.remove(root.path()).unwrap();
        assert!(store.list().unwrap().is_empty());
        store.remove(root.path()).unwrap();
    }

    #[test]
    fn rejects_relative_files_and_filesystem_root() {
        let store_dir = tempdir().unwrap();
        let store = store_at(store_dir.path());
        let dir = tempdir().unwrap();
        let file = dir.path().join("notes.txt");
        std::fs::write(&file, "x").unwrap();
        assert!(store.add(Path::new("relative"), true).is_err());
        assert!(store.add(&file, true).is_err());
        let err = store.add(Path::new("/"), true).unwrap_err();
        assert!(err.to_string().contains("filesystem root"));
    }

    #[test]
    fn home_jail_gates_new_roots() {
        let home = tempdir().unwrap();
        let home_path = home.path().canonicalize().unwrap();
        let inside = home_path.join("projects");
        std::fs::create_dir(&inside).unwrap();
        let outside = tempdir().unwrap();
        let store_dir = tempdir().unwrap();
        let store = store_at(store_dir.path());
        let _home = EnvGuard::set("HOME", &home_path);
        let _jail = EnvGuard::set("GOOSE_CONTROL_HOME_JAIL", "1");
        assert!(store
            .add(outside.path(), false)
            .unwrap_err()
            .to_string()
            .contains("allowUntrustedPath"));
        assert!(store.add(outside.path(), true).is_ok());
        assert!(store.add(&inside, false).is_ok());
    }

    #[test]
    fn lists_and_creates_only_direct_child_directories() {
        let root_dir = tempdir().unwrap();
        let store_dir = tempdir().unwrap();
        let store = store_at(store_dir.path());
        assert!(store.create_directory(root_dir.path(), "demo").is_err());
        assert!(!root_dir.path().join("demo").exists());

        store.add(root_dir.path(), true).unwrap();
        let created = store.create_directory(root_dir.path(), "demo").unwrap();
        assert!(created.created);
        let again = store.create_directory(root_dir.path(), "demo").unwrap();
        assert!(!again.created);

        std::fs::create_dir(root_dir.path().join("Visible")).unwrap();
        std::fs::create_dir(root_dir.path().join(".secret")).unwrap();
        std::fs::write(root_dir.path().join("file.txt"), "x").unwrap();
        let names: Vec<_> = store
            .list_directories(root_dir.path())
            .unwrap()
            .directories
            .into_iter()
            .map(|directory| directory.name)
            .collect();
        assert_eq!(names, vec!["demo".to_string(), "Visible".to_string()]);

        assert!(store
            .create_directory(root_dir.path(), "../escape")
            .is_err());
        assert!(store.create_directory(root_dir.path(), "a/b").is_err());
        assert!(store.create_directory(root_dir.path(), "..").is_err());
        std::fs::write(root_dir.path().join("notes"), "x").unwrap();
        assert!(store.create_directory(root_dir.path(), "notes").is_err());

        let canonical = store
            .require_child_of_root(&created.directory.path)
            .unwrap();
        assert_eq!(canonical, created.directory.path.canonicalize().unwrap());
        let nested = created.directory.path.join("nested");
        std::fs::create_dir(&nested).unwrap();
        assert!(store.require_child_of_root(&nested).is_err());
        let elsewhere = tempdir().unwrap();
        assert!(store
            .require_child_of_root(elsewhere.path())
            .unwrap_err()
            .to_string()
            .contains("directly inside"));
    }

    #[test]
    fn require_child_asks_for_a_root_when_none_are_configured() {
        let root = tempdir().unwrap();
        let store_dir = tempdir().unwrap();
        let store = store_at(store_dir.path());
        let err = store.require_child_of_root(root.path()).unwrap_err();
        assert!(err.to_string().contains("configure a project root"));
    }

    #[test]
    fn missing_root_stays_listed_but_cannot_be_browsed() {
        let root = tempdir().unwrap();
        let store_dir = tempdir().unwrap();
        let store = store_at(store_dir.path());
        let path = root.path().to_path_buf();
        store.add(&path, true).unwrap();
        drop(root);
        let listed = store.list().unwrap();
        assert_eq!(listed.len(), 1);
        assert!(!listed[0].available);
        assert!(store.list_directories(&path).is_err());
    }

    #[test]
    fn truncates_large_root_listings() {
        let root = tempdir().unwrap();
        let store_dir = tempdir().unwrap();
        let store = store_at(store_dir.path());
        store.add(root.path(), true).unwrap();
        for index in 0..=MAX_CHILD_DIRECTORIES {
            std::fs::create_dir(root.path().join(format!("dir-{index:04}"))).unwrap();
        }
        let listing = store.list_directories(root.path()).unwrap();
        assert!(listing.truncated);
        assert_eq!(listing.directories.len(), MAX_CHILD_DIRECTORIES);
    }
}
