use anyhow::{anyhow, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};

pub const MAX_FILE_SIZE_BYTES: usize = 5 * 1024 * 1024; // 5 MB

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectFileEntry {
    pub name: String,
    pub path: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extension: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileListResponse {
    pub current_path: String,
    pub parent_path: Option<String>,
    pub entries: Vec<ProjectFileEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileSearchQuery {
    pub q: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileSearchResponse {
    pub query: String,
    pub entries: Vec<ProjectFileEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileContentResponse {
    pub path: String,
    pub content: String,
    pub size: u64,
    pub is_binary: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateFileRequest {
    pub path: String,
    pub kind: String,
    pub content: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteFileRequest {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameFileRequest {
    pub old_path: String,
    pub new_path: String,
}

pub fn sanitize_relative_path(rel_path: &str) -> Result<PathBuf> {
    if rel_path.contains('\0') {
        return Err(anyhow!("Path contains a NUL byte"));
    }
    let mut normalized = PathBuf::new();
    for component in Path::new(rel_path).components() {
        match component {
            Component::Normal(part) => normalized.push(part),
            Component::CurDir => {}
            Component::ParentDir => {
                return Err(anyhow!("Path traversal (..) is not permitted"));
            }
            Component::RootDir | Component::Prefix(_) => {}
        }
    }
    Ok(normalized)
}

pub fn resolve_project_path(project_root: &Path, rel_path: &str) -> Result<PathBuf> {
    let canonical_root = project_root
        .canonicalize()
        .map_err(|e| anyhow!("Invalid project root: {e}"))?;

    let raw_rel = rel_path
        .trim()
        .strip_prefix("file://")
        .unwrap_or(rel_path.trim());
    let raw_path = Path::new(raw_rel);
    if raw_path.is_absolute() {
        if let Ok(canon) = raw_path.canonicalize() {
            if canon.starts_with(&canonical_root) {
                return Ok(canon);
            }
        }
    }

    let clean_rel = sanitize_relative_path(raw_rel)?;
    let target = canonical_root.join(&clean_rel);

    if target.exists() {
        let canonical_target = target
            .canonicalize()
            .map_err(|e| anyhow!("Cannot resolve path: {e}"))?;
        if !canonical_target.starts_with(&canonical_root) {
            return Err(anyhow!("Resolved path escapes project root"));
        }
        Ok(canonical_target)
    } else {
        if let Some(parent) = target.parent() {
            if parent.exists() {
                let canonical_parent = parent
                    .canonicalize()
                    .map_err(|e| anyhow!("Cannot resolve parent directory: {e}"))?;
                if !canonical_parent.starts_with(&canonical_root) {
                    return Err(anyhow!("Target directory escapes project root"));
                }
            }
        }
        Ok(target)
    }
}

fn path_to_clean_string(project_root: &Path, absolute: &Path) -> String {
    if let Ok(rel) = absolute.strip_prefix(project_root) {
        rel.to_string_lossy().replace('\\', "/")
    } else {
        String::new()
    }
}

pub async fn list_files(project_root: &Path, rel_path: &str) -> Result<FileListResponse> {
    let canonical_root = project_root.canonicalize()?;
    let target_dir = resolve_project_path(project_root, rel_path)?;

    if !target_dir.is_dir() {
        return Err(anyhow!("Target path is not a directory"));
    }

    let clean_rel = path_to_clean_string(&canonical_root, &target_dir);
    let parent_path = if clean_rel.is_empty() {
        None
    } else {
        let parent = Path::new(&clean_rel).parent().unwrap_or(Path::new(""));
        let p = parent.to_string_lossy().replace('\\', "/");
        Some(p)
    };

    let mut read_dir = tokio::fs::read_dir(&target_dir).await?;
    let mut entries = Vec::new();

    while let Some(entry) = read_dir.next_entry().await? {
        let file_name = entry.file_name().to_string_lossy().into_owned();
        let path = entry.path();
        let metadata = entry.metadata().await.ok();
        let is_dir = metadata.as_ref().map(|m| m.is_dir()).unwrap_or(false);

        let size = if is_dir {
            None
        } else {
            metadata.as_ref().map(|m| m.len())
        };

        let modified_at = metadata
            .and_then(|m| m.modified().ok())
            .map(|t| DateTime::<Utc>::from(t).to_rfc3339());

        let extension = if is_dir {
            None
        } else {
            path.extension()
                .map(|ext| ext.to_string_lossy().into_owned())
        };

        let entry_rel = path_to_clean_string(&canonical_root, &path);

        entries.push(ProjectFileEntry {
            name: file_name,
            path: entry_rel,
            kind: if is_dir {
                "dir".to_string()
            } else {
                "file".to_string()
            },
            size,
            modified_at,
            extension,
        });
    }

    entries.sort_by(|a, b| match (a.kind.as_str(), b.kind.as_str()) {
        ("dir", "file") => std::cmp::Ordering::Less,
        ("file", "dir") => std::cmp::Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });

    Ok(FileListResponse {
        current_path: clean_rel,
        parent_path,
        entries,
    })
}

const SKIP_DIRECTORIES: &[&str] = &[
    ".git",
    ".svn",
    ".hg",
    "node_modules",
    "target",
    "dist",
    "build",
    ".cache",
    ".npm",
    ".yarn",
    "__pycache__",
    ".venv",
    "venv",
    "env",
    ".next",
    ".turbo",
    ".nuxt",
    ".svelte-kit",
    ".idea",
    "coverage",
    "out",
    ".output",
    ".parcel-cache",
    ".gradle",
];

pub async fn search_files(
    project_root: &Path,
    query: &str,
    limit: usize,
) -> Result<FileSearchResponse> {
    let canonical_root = project_root.canonicalize()?;
    let query_str = query.trim().to_string();
    let effective_limit = limit.clamp(1, 200);

    tokio::task::spawn_blocking(move || {
        let mut builder = ignore::WalkBuilder::new(&canonical_root);
        builder.hidden(false);
        builder.git_ignore(true);
        builder.git_exclude(true);
        builder.require_git(false);
        builder.ignore(true);

        builder.filter_entry(|entry| {
            if let Some(name) = entry.file_name().to_str() {
                if entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false)
                    && SKIP_DIRECTORIES.contains(&name)
                {
                    return false;
                }
                if name.starts_with('.') && name != ".github" && name != ".vscode" && name != "." {
                    return false;
                }
            }
            true
        });

        let query_lower = query_str.to_lowercase();
        let tokens: Vec<&str> = query_lower.split_whitespace().collect();

        struct ScoredEntry {
            entry: ProjectFileEntry,
            score: i32,
        }

        let mut scored_entries = Vec::new();

        for result in builder.build() {
            let walk_entry = match result {
                Ok(e) => e,
                Err(_) => continue,
            };

            let path = walk_entry.path();
            if path == canonical_root {
                continue;
            }

            let clean_rel = path_to_clean_string(&canonical_root, path);
            if clean_rel.is_empty() {
                continue;
            }

            let file_name = walk_entry.file_name().to_string_lossy().to_string();
            let is_dir = walk_entry
                .file_type()
                .map(|ft| ft.is_dir())
                .unwrap_or(false);

            let clean_rel_lower = clean_rel.to_lowercase();
            let file_name_lower = file_name.to_lowercase();

            let score = if tokens.is_empty() {
                let depth = clean_rel.matches('/').count() as i32;
                100 - depth * 10
            } else {
                let all_match = tokens.iter().all(|token| {
                    clean_rel_lower.contains(token) || file_name_lower.contains(token)
                });

                if !all_match {
                    continue;
                }

                let mut s = 50;

                if file_name_lower == query_lower {
                    s += 1000;
                } else if file_name_lower.starts_with(&query_lower) {
                    s += 500;
                } else if file_name_lower.contains(&query_lower) {
                    s += 300;
                } else if clean_rel_lower.contains(&query_lower) {
                    s += 100;
                }

                if let Some(pos) = file_name_lower.find(&query_lower) {
                    if pos == 0
                        || matches!(
                            file_name_lower.as_bytes().get(pos - 1),
                            Some(b'-' | b'_' | b'.' | b' ' | b'/')
                        )
                    {
                        s += 50;
                    }
                }

                if !is_dir {
                    s += 20;
                }

                let depth = clean_rel.matches('/').count() as i32;
                s -= depth * 5;

                s
            };

            let metadata = walk_entry.metadata().ok();
            let size = if is_dir {
                None
            } else {
                metadata.as_ref().map(|m| m.len())
            };
            let extension = if is_dir {
                None
            } else {
                path.extension().map(|e| e.to_string_lossy().to_string())
            };

            scored_entries.push(ScoredEntry {
                entry: ProjectFileEntry {
                    name: file_name,
                    path: clean_rel,
                    kind: if is_dir {
                        "dir".to_string()
                    } else {
                        "file".to_string()
                    },
                    size,
                    modified_at: None,
                    extension,
                },
                score,
            });
        }

        scored_entries.sort_by(|a, b| {
            b.score
                .cmp(&a.score)
                .then_with(|| a.entry.path.cmp(&b.entry.path))
        });
        let entries: Vec<ProjectFileEntry> = scored_entries
            .into_iter()
            .take(effective_limit)
            .map(|s| s.entry)
            .collect();

        Ok(FileSearchResponse {
            query: query_str,
            entries,
        })
    })
    .await?
}

pub async fn read_file_content(project_root: &Path, rel_path: &str) -> Result<FileContentResponse> {
    let canonical_root = project_root.canonicalize()?;
    let target = resolve_project_path(project_root, rel_path)?;

    if !target.is_file() {
        return Err(anyhow!("Target path is not a file"));
    }

    let metadata = tokio::fs::metadata(&target).await?;
    let size = metadata.len();

    if size > MAX_FILE_SIZE_BYTES as u64 {
        return Err(anyhow!(
            "File size exceeds maximum supported limit of {} MB",
            MAX_FILE_SIZE_BYTES / (1024 * 1024)
        ));
    }

    let raw_bytes = tokio::fs::read(&target).await?;
    let is_binary = raw_bytes.iter().take(1024).any(|&b| b == 0);

    let content = if is_binary {
        String::new()
    } else {
        String::from_utf8(raw_bytes).map_err(|_| anyhow!("File is not valid UTF-8 text"))?
    };

    let clean_rel = path_to_clean_string(&canonical_root, &target);

    Ok(FileContentResponse {
        path: clean_rel,
        content,
        size,
        is_binary,
    })
}

pub async fn write_file_content(project_root: &Path, rel_path: &str, content: &str) -> Result<()> {
    write_file_bytes(project_root, rel_path, content.as_bytes()).await
}

pub async fn write_file_bytes(project_root: &Path, rel_path: &str, content: &[u8]) -> Result<()> {
    if content.len() > MAX_FILE_SIZE_BYTES {
        return Err(anyhow!(
            "File size exceeds maximum supported limit of {} MB",
            MAX_FILE_SIZE_BYTES / (1024 * 1024)
        ));
    }

    let target = resolve_project_path(project_root, rel_path)?;

    if let Some(parent) = target.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    tokio::fs::write(&target, content).await?;
    Ok(())
}

pub fn mime_type_from_path(path: &Path) -> String {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "bmp" => "image/bmp",
        "avif" => "image/avif",
        "pdf" => "application/pdf",
        "json" => "application/json",
        "txt" => "text/plain; charset=utf-8",
        "md" | "markdown" => "text/markdown; charset=utf-8",
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "ts" | "tsx" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
    .to_string()
}

pub async fn read_file_bytes(project_root: &Path, rel_path: &str) -> Result<(Vec<u8>, String)> {
    let target = resolve_project_path(project_root, rel_path)?;

    if !target.is_file() {
        return Err(anyhow!("Target path is not a file"));
    }

    let metadata = tokio::fs::metadata(&target).await?;
    let size = metadata.len();

    if size > MAX_FILE_SIZE_BYTES as u64 {
        return Err(anyhow!(
            "File size exceeds maximum supported limit of {} MB",
            MAX_FILE_SIZE_BYTES / (1024 * 1024)
        ));
    }

    let mime_type = mime_type_from_path(&target);
    let bytes = tokio::fs::read(&target).await?;
    Ok((bytes, mime_type))
}

pub async fn create_entry(
    project_root: &Path,
    rel_path: &str,
    kind: &str,
    content: Option<&str>,
) -> Result<()> {
    let target = resolve_project_path(project_root, rel_path)?;

    if target.exists() {
        return Err(anyhow!("Path already exists"));
    }

    if kind == "dir" {
        tokio::fs::create_dir_all(&target).await?;
    } else {
        if let Some(parent) = target.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let initial = content.unwrap_or("");
        tokio::fs::write(&target, initial.as_bytes()).await?;
    }

    Ok(())
}

pub async fn delete_entry(project_root: &Path, rel_path: &str) -> Result<()> {
    let canonical_root = project_root.canonicalize()?;
    let target = resolve_project_path(project_root, rel_path)?;

    if target == canonical_root {
        return Err(anyhow!("Cannot delete project root directory"));
    }

    if !target.exists() {
        return Err(anyhow!("Target path does not exist"));
    }

    if target.is_dir() {
        tokio::fs::remove_dir_all(&target).await?;
    } else {
        tokio::fs::remove_file(&target).await?;
    }

    Ok(())
}

pub async fn rename_entry(
    project_root: &Path,
    old_rel_path: &str,
    new_rel_path: &str,
) -> Result<()> {
    let canonical_root = project_root.canonicalize()?;
    let src = resolve_project_path(project_root, old_rel_path)?;
    let dst = resolve_project_path(project_root, new_rel_path)?;

    if src == canonical_root {
        return Err(anyhow!("Cannot rename project root directory"));
    }
    if !src.exists() {
        return Err(anyhow!("Source path does not exist"));
    }
    if dst.exists() {
        return Err(anyhow!("Target path already exists"));
    }

    if let Some(parent) = dst.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    tokio::fs::rename(&src, &dst).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_file_operations_flow() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        // 1. Create directory and files
        create_entry(root, "src", "dir", None).await.unwrap();
        create_entry(root, "src/main.rs", "file", Some("fn main() {}\n"))
            .await
            .unwrap();
        create_entry(root, "README.md", "file", Some("# Hello World"))
            .await
            .unwrap();

        // 2. List root files
        let listing = list_files(root, "").await.unwrap();
        assert_eq!(listing.current_path, "");
        assert_eq!(listing.parent_path, None);
        assert_eq!(listing.entries.len(), 2);
        assert_eq!(listing.entries[0].name, "src");
        assert_eq!(listing.entries[0].kind, "dir");
        assert_eq!(listing.entries[1].name, "README.md");
        assert_eq!(listing.entries[1].kind, "file");

        // 3. List src subdirectory
        let src_listing = list_files(root, "src").await.unwrap();
        assert_eq!(src_listing.current_path, "src");
        assert_eq!(src_listing.parent_path, Some("".to_string()));
        assert_eq!(src_listing.entries.len(), 1);
        assert_eq!(src_listing.entries[0].name, "main.rs");

        // 4. Read file content
        let content = read_file_content(root, "src/main.rs").await.unwrap();
        assert_eq!(content.content, "fn main() {}\n");
        assert!(!content.is_binary);

        // 5. Write/Update file content
        write_file_content(root, "src/main.rs", "fn main() { println!(\"Hi\"); }")
            .await
            .unwrap();
        let updated = read_file_content(root, "src/main.rs").await.unwrap();
        assert_eq!(updated.content, "fn main() { println!(\"Hi\"); }");

        // 6. Rename file
        rename_entry(root, "src/main.rs", "src/app.rs")
            .await
            .unwrap();
        assert!(read_file_content(root, "src/app.rs").await.is_ok());
        assert!(read_file_content(root, "src/main.rs").await.is_err());

        // 7. Delete file
        delete_entry(root, "src/app.rs").await.unwrap();
        assert!(read_file_content(root, "src/app.rs").await.is_err());
    }

    #[tokio::test]
    async fn test_path_traversal_prevention() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        assert!(resolve_project_path(root, "../outside").is_err());
        assert!(resolve_project_path(root, "src/../../outside").is_err());
        assert!(resolve_project_path(root, "foo\0bar").is_err());

        let subfile = root.join("test.txt");
        tokio::fs::write(&subfile, "content").await.unwrap();
        assert_eq!(
            resolve_project_path(root, subfile.to_str().unwrap()).unwrap(),
            subfile.canonicalize().unwrap()
        );
        let file_url = format!("file://{}", subfile.to_str().unwrap());
        assert_eq!(
            resolve_project_path(root, &file_url).unwrap(),
            subfile.canonicalize().unwrap()
        );
    }

    #[tokio::test]
    async fn write_file_bytes_round_trips_pdf_and_png_magic() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        let pdf: &[u8] = b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\nbinary-payload";
        write_file_bytes(root, "docs/spec.pdf", pdf).await.unwrap();
        let pdf_back = tokio::fs::read(root.join("docs/spec.pdf")).await.unwrap();
        assert_eq!(pdf_back.as_slice(), pdf);

        let png: &[u8] = &[
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x01, 0x02, 0xFF,
        ];
        write_file_bytes(root, "assets/icon.png", png)
            .await
            .unwrap();
        let png_back = tokio::fs::read(root.join("assets/icon.png")).await.unwrap();
        assert_eq!(png_back.as_slice(), png);
        assert_eq!(png_back[0], 0x89);

        let (bytes, mime) = read_file_bytes(root, "assets/icon.png").await.unwrap();
        assert_eq!(bytes.as_slice(), png);
        assert_eq!(mime, "image/png");

        let (pdf_bytes, pdf_mime) = read_file_bytes(root, "docs/spec.pdf").await.unwrap();
        assert_eq!(pdf_bytes.as_slice(), pdf);
        assert_eq!(pdf_mime, "application/pdf");
    }

    #[test]
    fn test_mime_type_detection() {
        assert_eq!(mime_type_from_path(Path::new("pic.PNG")), "image/png");
        assert_eq!(mime_type_from_path(Path::new("pic.jpg")), "image/jpeg");
        assert_eq!(mime_type_from_path(Path::new("pic.jpeg")), "image/jpeg");
        assert_eq!(mime_type_from_path(Path::new("pic.svg")), "image/svg+xml");
        assert_eq!(mime_type_from_path(Path::new("pic.webp")), "image/webp");
        assert_eq!(mime_type_from_path(Path::new("pic.gif")), "image/gif");
        assert_eq!(
            mime_type_from_path(Path::new("file.unknown")),
            "application/octet-stream"
        );
    }

    #[tokio::test]
    async fn test_search_files_finds_deep_and_root_files() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        tokio::fs::create_dir_all(root.join("nested/deeply/dir"))
            .await
            .unwrap();
        tokio::fs::write(root.join("2026-04-16-claude-kyc-script.md"), "hello")
            .await
            .unwrap();
        tokio::fs::write(root.join("2026-04-17-claude-kyc-v2-script.md"), "v2")
            .await
            .unwrap();
        tokio::fs::write(root.join("nested/deeply/dir/other.md"), "other")
            .await
            .unwrap();
        tokio::fs::write(root.join("nested/deeply/dir/claude-spec.txt"), "spec")
            .await
            .unwrap();

        let res = search_files(root, "claude-kyc", 50).await.unwrap();
        assert_eq!(res.entries.len(), 2);
        let names: Vec<_> = res.entries.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"2026-04-16-claude-kyc-script.md"));
        assert!(names.contains(&"2026-04-17-claude-kyc-v2-script.md"));

        let res_deep = search_files(root, "spec", 50).await.unwrap();
        assert_eq!(res_deep.entries.len(), 1);
        assert_eq!(res_deep.entries[0].name, "claude-spec.txt");
    }
}
