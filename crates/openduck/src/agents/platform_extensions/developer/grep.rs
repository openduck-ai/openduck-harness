use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use ignore::overrides::OverrideBuilder;
use ignore::WalkBuilder;
use regex::{Regex, RegexBuilder};
use rmcp::model::{CallToolResult, ContentBlock};
use schemars::JsonSchema;
use serde::Deserialize;

const DEFAULT_HEAD_LIMIT: usize = 50;
const MAX_HEAD_LIMIT: usize = 200;
const MAX_LINE_LENGTH: usize = 500;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GrepSearchParams {
    /// Search term or regex pattern to look for within files.
    pub query: String,
    /// Path to search (file or directory). Defaults to current workspace root.
    #[serde(default)]
    pub path: Option<String>,
    /// Case-sensitive matching. Defaults to false (case-insensitive).
    #[serde(default)]
    pub case_sensitive: Option<bool>,
    /// Treat query as a regular expression. Defaults to false.
    #[serde(default)]
    pub is_regex: Option<bool>,
    /// Glob patterns to filter files (e.g. ["*.rs", "!**/target/*"]).
    #[serde(default)]
    pub includes: Option<Vec<String>>,
    /// Maximum number of matching lines to return. Defaults to 50 (max 200).
    #[serde(default)]
    pub head_limit: Option<usize>,
    /// Number of lines of context before and after each match (0-5). Defaults to 0.
    #[serde(default)]
    pub context_lines: Option<usize>,
}

pub struct GrepTool;

impl GrepTool {
    pub fn new() -> Self {
        Self
    }

    pub fn grep(&self, params: GrepSearchParams) -> CallToolResult {
        self.grep_with_cwd(params, None)
    }

    pub fn grep_with_cwd(
        &self,
        params: GrepSearchParams,
        working_dir: Option<&Path>,
    ) -> CallToolResult {
        if params.query.is_empty() {
            return CallToolResult::error(vec![ContentBlock::text(
                "Search query must not be empty",
            )]);
        }

        let base_dir = working_dir
            .map(Path::to_path_buf)
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("."));

        let target_path = match &params.path {
            Some(p) => {
                let pb = PathBuf::from(p);
                if pb.is_absolute() {
                    pb
                } else {
                    base_dir.join(pb)
                }
            }
            None => base_dir.clone(),
        };

        if !target_path.exists() {
            return CallToolResult::error(vec![ContentBlock::text(format!(
                "Path does not exist: {}",
                target_path.display()
            ))]);
        }

        let case_sensitive = params.case_sensitive.unwrap_or(false);
        let is_regex = params.is_regex.unwrap_or(false);
        let head_limit = params
            .head_limit
            .unwrap_or(DEFAULT_HEAD_LIMIT)
            .clamp(1, MAX_HEAD_LIMIT);
        let context_lines = params.context_lines.unwrap_or(0).min(5);

        let regex = match build_matcher(&params.query, is_regex, case_sensitive) {
            Ok(r) => r,
            Err(e) => {
                return CallToolResult::error(vec![ContentBlock::text(format!(
                    "Invalid regex pattern: {e}"
                ))]);
            }
        };

        if target_path.is_file() {
            return match_single_file(&target_path, &base_dir, &regex, head_limit, context_lines);
        }

        match_directory(
            &target_path,
            &base_dir,
            &regex,
            params.includes.as_deref(),
            head_limit,
            context_lines,
        )
    }
}

impl Default for GrepTool {
    fn default() -> Self {
        Self::new()
    }
}

fn build_matcher(query: &str, is_regex: bool, case_sensitive: bool) -> Result<Regex, regex::Error> {
    let pattern = if is_regex {
        query.to_string()
    } else {
        regex::escape(query)
    };

    RegexBuilder::new(&pattern)
        .case_insensitive(!case_sensitive)
        .build()
}

fn is_binary_file(path: &Path) -> bool {
    let mut file = match File::open(path) {
        Ok(f) => f,
        Err(_) => return true,
    };
    use std::io::Read;
    let mut buf = [0u8; 1024];
    let n = match file.read(&mut buf) {
        Ok(n) => n,
        Err(_) => return true,
    };
    buf[..n].contains(&0)
}

fn truncate_line(line: &str) -> String {
    let mut chars = line.chars();
    let truncated: String = chars.by_ref().take(MAX_LINE_LENGTH).collect();
    if chars.next().is_some() {
        format!("{truncated} ... [truncated]")
    } else {
        line.to_string()
    }
}

fn match_single_file(
    path: &Path,
    base_dir: &Path,
    regex: &Regex,
    head_limit: usize,
    context_lines: usize,
) -> CallToolResult {
    if is_binary_file(path) {
        return CallToolResult::success(vec![ContentBlock::text("(binary file ignored)")]);
    }

    let file = match File::open(path) {
        Ok(f) => f,
        Err(e) => {
            return CallToolResult::error(vec![ContentBlock::text(format!(
                "Failed to read file {}: {e}",
                path.display()
            ))]);
        }
    };

    let display_path = path
        .strip_prefix(base_dir)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned();

    let reader = BufReader::new(file);
    let lines: Vec<String> = reader.lines().map_while(Result::ok).collect();

    let mut output = String::new();
    let mut match_count = 0;
    let mut total_matches = 0;

    for (idx, line) in lines.iter().enumerate() {
        if regex.is_match(line) {
            total_matches += 1;
            if match_count < head_limit {
                if context_lines > 0 && match_count > 0 {
                    output.push_str("--\n");
                }

                let start = idx.saturating_sub(context_lines);
                let end = (idx + context_lines + 1).min(lines.len());

                for (ctx_idx, ctx_line) in lines.iter().enumerate().take(end).skip(start) {
                    let line_num = ctx_idx + 1;
                    let sep = if ctx_idx == idx { ":" } else { "-" };
                    output.push_str(&format!(
                        "{}{}{}: {}\n",
                        display_path,
                        sep,
                        line_num,
                        truncate_line(ctx_line)
                    ));
                }
                match_count += 1;
            }
        }
    }

    if match_count == 0 {
        return CallToolResult::success(vec![ContentBlock::text("No matches found.")]);
    }

    if total_matches > head_limit {
        let omitted = total_matches - head_limit;
        output.push_str(&format!(
            "\n[Showing first {head_limit} matches. {omitted} additional matches omitted. Refine query or use more specific path.]\n"
        ));
    }

    CallToolResult::success(vec![ContentBlock::text(output)])
}

fn match_directory(
    root: &Path,
    base_dir: &Path,
    regex: &Regex,
    includes: Option<&[String]>,
    head_limit: usize,
    context_lines: usize,
) -> CallToolResult {
    let mut builder = WalkBuilder::new(root);
    builder.git_ignore(true);
    builder.git_exclude(true);
    builder.git_global(true);
    builder.require_git(false);
    builder.ignore(true); // Supports .ignore file (e.g. !node_modules/)
    builder.hidden(true); // Ignores hidden files like .git

    if let Some(inc_list) = includes {
        if !inc_list.is_empty() {
            let mut ov_builder = OverrideBuilder::new(root);
            for pattern in inc_list {
                if let Err(e) = ov_builder.add(pattern) {
                    return CallToolResult::error(vec![ContentBlock::text(format!(
                        "Invalid glob pattern '{pattern}': {e}"
                    ))]);
                }
            }
            if let Ok(overrides) = ov_builder.build() {
                builder.overrides(overrides);
            }
        }
    }

    let mut output = String::new();
    let mut match_count = 0;
    let mut total_matches = 0;
    let mut matched_files = 0;
    let mut reached_limit = false;

    for entry in builder.build().flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        if is_binary_file(path) {
            continue;
        }

        let file = match File::open(path) {
            Ok(f) => f,
            Err(_) => continue,
        };

        let display_path = path
            .strip_prefix(base_dir)
            .unwrap_or(path)
            .to_string_lossy()
            .into_owned();

        let reader = BufReader::new(file);
        let lines: Vec<String> = reader.lines().map_while(Result::ok).collect();

        let mut file_had_match = false;

        for (idx, line) in lines.iter().enumerate() {
            if regex.is_match(line) {
                total_matches += 1;
                file_had_match = true;

                if match_count < head_limit {
                    let start = idx.saturating_sub(context_lines);
                    let end = (idx + context_lines + 1).min(lines.len());

                    for (ctx_idx, ctx_line) in lines.iter().enumerate().take(end).skip(start) {
                        let line_num = ctx_idx + 1;
                        let sep = if ctx_idx == idx { ":" } else { "-" };
                        output.push_str(&format!(
                            "{}{}{}: {}\n",
                            display_path,
                            sep,
                            line_num,
                            truncate_line(ctx_line)
                        ));
                    }
                    match_count += 1;
                } else {
                    reached_limit = true;
                }
            }
        }

        if file_had_match {
            matched_files += 1;
        }

        // Short-circuit scanning once we exceed limit by a reasonable margin to avoid long loops in huge repos
        if reached_limit && total_matches >= head_limit + 500 {
            break;
        }
    }

    if match_count == 0 {
        return CallToolResult::success(vec![ContentBlock::text("No matches found.")]);
    }

    if total_matches > head_limit {
        let omitted = total_matches - head_limit;
        output.push_str(&format!(
            "\n[Found {total_matches}+ matches in {matched_files} file(s). Showing first {head_limit} matches ({omitted}+ omitted). Refine query or narrow search path.]\n"
        ));
    }

    CallToolResult::success(vec![ContentBlock::text(output)])
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_grep_basic() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");
        std::fs::write(&file_path, "hello world\nfoo bar\nhello rust").unwrap();

        let tool = GrepTool::new();
        let result = tool.grep_with_cwd(
            GrepSearchParams {
                query: "hello".to_string(),
                path: None,
                case_sensitive: Some(false),
                is_regex: Some(false),
                includes: None,
                head_limit: Some(10),
                context_lines: Some(0),
            },
            Some(dir.path()),
        );

        assert!(!result.is_error.unwrap_or(false));
        let text = result.content[0].as_text().unwrap().text.as_str();
        assert!(text.contains("test.txt:1: hello world"));
        assert!(text.contains("test.txt:3: hello rust"));
    }

    #[test]
    fn test_grep_head_limit_truncation() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("numbers.txt");
        let content = (1..=100)
            .map(|i| format!("item {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(&file_path, content).unwrap();

        let tool = GrepTool::new();
        let result = tool.grep_with_cwd(
            GrepSearchParams {
                query: "item".to_string(),
                path: None,
                case_sensitive: None,
                is_regex: None,
                includes: None,
                head_limit: Some(10),
                context_lines: None,
            },
            Some(dir.path()),
        );

        let text = result.content[0].as_text().unwrap().text.as_str();
        assert!(text.contains("item 1"));
        assert!(text.contains("item 10"));
        assert!(!text.contains("item 11:"));
        assert!(text.contains("Showing first 10 matches (90+ omitted)"));
    }

    #[test]
    fn test_gitignore_and_ignore_file() {
        let dir = tempdir().unwrap();
        let ignored_dir = dir.path().join("ignored");
        std::fs::create_dir(&ignored_dir).unwrap();
        std::fs::write(ignored_dir.join("secret.txt"), "target_word").unwrap();
        std::fs::write(dir.path().join(".gitignore"), "ignored/\n").unwrap();
        std::fs::write(dir.path().join("normal.txt"), "target_word").unwrap();

        let tool = GrepTool::new();
        let result = tool.grep_with_cwd(
            GrepSearchParams {
                query: "target_word".to_string(),
                path: None,
                case_sensitive: None,
                is_regex: None,
                includes: None,
                head_limit: None,
                context_lines: None,
            },
            Some(dir.path()),
        );

        let text = result.content[0].as_text().unwrap().text.as_str();
        assert!(text.contains("normal.txt:1: target_word"));
        assert!(!text.contains("secret.txt"));

        // Now test .ignore un-ignoring
        std::fs::write(dir.path().join(".ignore"), "!ignored/\n").unwrap();
        let result2 = tool.grep_with_cwd(
            GrepSearchParams {
                query: "target_word".to_string(),
                path: None,
                case_sensitive: None,
                is_regex: None,
                includes: None,
                head_limit: None,
                context_lines: None,
            },
            Some(dir.path()),
        );
        let text2 = result2.content[0].as_text().unwrap().text.as_str();
        assert!(text2.contains("normal.txt"));
        assert!(text2.contains("ignored/secret.txt") || text2.contains("ignored\\secret.txt"));
    }
}
