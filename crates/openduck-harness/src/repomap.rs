use std::fs;
use std::path::Path;

const IGNORED_DIRS: &[&str] = &[
    ".git",
    ".github",
    ".goose",
    ".openduck",
    ".agents",
    "target",
    "node_modules",
    "dist",
    "build",
    "venv",
    ".venv",
    "__pycache__",
    ".idea",
    ".vscode",
];

/// Generates a compact, high-level architecture overview of the workspace
/// to provide agents with initial context without requiring blind directory scans.
pub fn generate_workspace_repo_map(root: &Path) -> Option<String> {
    if !root.exists() || !root.is_dir() {
        return None;
    }

    let mut entries = Vec::new();

    // Check root Cargo.toml
    let root_cargo = root.join("Cargo.toml");
    if root_cargo.is_file() {
        if let Ok(content) = fs::read_to_string(&root_cargo) {
            if content.contains("[workspace]") {
                entries.push(
                    "• [Rust Workspace Root] `Cargo.toml` defines workspace crates.".to_string(),
                );
            } else if let Some(pkg_name) = extract_toml_value(&content, "name") {
                entries.push(format!("• [Rust Crate] `{pkg_name}` (Root package)"));
            }
        }
    }

    // Check root package.json
    let root_pkg = root.join("package.json");
    if root_pkg.is_file() {
        if let Ok(content) = fs::read_to_string(&root_pkg) {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                let name = val.get("name").and_then(|v| v.as_str()).unwrap_or("app");
                let desc = val
                    .get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Node/JS Project");
                entries.push(format!("• [Node/JS Root] `{name}`: {desc}"));
            }
        }
    }

    // Scan subdirectories
    let read_dir = fs::read_dir(root).ok()?;
    let mut subdirs: Vec<_> = read_dir
        .filter_map(Result::ok)
        .filter(|e| {
            e.file_type().map(|ft| ft.is_dir()).unwrap_or(false)
                && !IGNORED_DIRS.contains(&e.file_name().to_string_lossy().as_ref())
        })
        .collect();

    subdirs.sort_by_key(|e| e.file_name());

    for dir in subdirs {
        let dir_name = dir.file_name().to_string_lossy().to_string();
        let dir_path = dir.path();

        let cargo_path = dir_path.join("Cargo.toml");
        let pkg_path = dir_path.join("package.json");
        let py_path = dir_path.join("pyproject.toml");
        let go_path = dir_path.join("go.mod");

        if cargo_path.is_file() {
            if let Ok(content) = fs::read_to_string(&cargo_path) {
                let pkg_name =
                    extract_toml_value(&content, "name").unwrap_or_else(|| dir_name.clone());
                let desc = extract_toml_value(&content, "description").unwrap_or_default();
                let desc_suffix = if desc.is_empty() {
                    String::new()
                } else {
                    format!(" - {desc}")
                };
                entries.push(format!(
                    "• `{dir_name}/` [Rust Crate: `{pkg_name}`]{desc_suffix}"
                ));
            }
        } else if pkg_path.is_file() {
            if let Ok(content) = fs::read_to_string(&pkg_path) {
                let (name, desc) =
                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                        let n = val
                            .get("name")
                            .and_then(|v| v.as_str())
                            .map(ToOwned::to_owned)
                            .unwrap_or_else(|| dir_name.clone());
                        let d = val
                            .get("description")
                            .and_then(|v| v.as_str())
                            .map(ToOwned::to_owned)
                            .unwrap_or_default();
                        (n, d)
                    } else {
                        (dir_name.clone(), String::new())
                    };
                let desc_suffix = if desc.is_empty() {
                    String::new()
                } else {
                    format!(" - {desc}")
                };
                entries.push(format!(
                    "• `{dir_name}/` [Frontend/Node: `{name}`]{desc_suffix}"
                ));
            }
        } else if py_path.is_file() {
            entries.push(format!("• `{dir_name}/` [Python Project]"));
        } else if go_path.is_file() {
            entries.push(format!("• `{dir_name}/` [Go Module]"));
        } else {
            // General sub-directory overview
            let key_children = summarize_directory_children(&dir_path);
            if !key_children.is_empty() {
                entries.push(format!("• `{dir_name}/` ({key_children})"));
            } else {
                entries.push(format!("• `{dir_name}/`"));
            }
        }
    }

    if entries.is_empty() {
        return None;
    }

    let mut output = String::from("## Workspace Architecture Overview:\n");
    for entry in entries.into_iter().take(20) {
        output.push_str(&entry);
        output.push('\n');
    }
    Some(output)
}

fn extract_toml_value(content: &str, key: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with(key) {
            let parts: Vec<&str> = trimmed.splitn(2, '=').collect();
            if parts.len() == 2 && parts[0].trim() == key {
                let val = parts[1].trim().trim_matches('"').trim_matches('\'');
                return Some(val.to_string());
            }
        }
    }
    None
}

fn summarize_directory_children(dir: &Path) -> String {
    let Ok(read_dir) = fs::read_dir(dir) else {
        return String::new();
    };

    let mut sub_names = Vec::new();
    for entry in read_dir.filter_map(Result::ok) {
        let name = entry.file_name().to_string_lossy().to_string();
        if !IGNORED_DIRS.contains(&name.as_str()) && !name.starts_with('.') {
            sub_names.push(name);
        }
    }
    sub_names.sort();
    sub_names.into_iter().take(4).collect::<Vec<_>>().join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_generate_repo_map_with_crates_and_packages() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        // Create a root Cargo.toml
        fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"core\", \"api\"]\n",
        )
        .unwrap();

        // Create a rust crate subdir
        let core_dir = root.join("core");
        fs::create_dir(&core_dir).unwrap();
        fs::write(
            core_dir.join("Cargo.toml"),
            "[package]\nname = \"my-core\"\ndescription = \"Core business logic\"\n",
        )
        .unwrap();

        // Create a frontend package subdir
        let ui_dir = root.join("ui");
        fs::create_dir(&ui_dir).unwrap();
        fs::write(
            ui_dir.join("package.json"),
            r#"{"name": "my-ui", "description": "Vue Dashboard"}"#,
        )
        .unwrap();

        let repo_map = generate_workspace_repo_map(root).expect("Should generate repo map");
        assert!(repo_map.contains("Workspace Architecture Overview"));
        assert!(repo_map.contains("my-core"));
        assert!(repo_map.contains("Core business logic"));
        assert!(repo_map.contains("my-ui"));
        assert!(repo_map.contains("Vue Dashboard"));
    }
}
