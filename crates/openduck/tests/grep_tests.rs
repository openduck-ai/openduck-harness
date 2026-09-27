use openduck::agents::platform_extensions::developer::grep::{GrepSearchParams, GrepTool};
use tempfile::tempdir;

#[test]
fn test_grep_tool_full_lifecycle() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    // Create folder structure
    let src_dir = root.join("src");
    let target_dir = root.join("target");
    let custom_dir = root.join("vendor");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::create_dir_all(&target_dir).unwrap();
    std::fs::create_dir_all(&custom_dir).unwrap();

    // Create files
    std::fs::write(
        src_dir.join("main.rs"),
        "fn main() {\n    println!(\"Hello OpenDuck\");\n}\n",
    )
    .unwrap();
    std::fs::write(
        src_dir.join("lib.rs"),
        "pub fn search_target() -> &'static str {\n    \"Hello OpenDuck lib\"\n}\n",
    )
    .unwrap();
    std::fs::write(
        target_dir.join("build.rs"),
        "// should be ignored: Hello OpenDuck\n",
    )
    .unwrap();
    std::fs::write(
        custom_dir.join("dep.rs"),
        "// custom vendor: Hello OpenDuck\n",
    )
    .unwrap();

    // Add .gitignore
    std::fs::write(root.join(".gitignore"), "target/\nvendor/\n").unwrap();

    let tool = GrepTool::new();

    // 1. Search without .ignore -> target/ and vendor/ are skipped
    let res = tool.grep_with_cwd(
        GrepSearchParams {
            query: "Hello OpenDuck".to_string(),
            path: None,
            case_sensitive: Some(true),
            is_regex: Some(false),
            includes: None,
            head_limit: Some(50),
            context_lines: Some(0),
        },
        Some(root),
    );

    assert!(!res.is_error.unwrap_or(false));
    let text = res.content[0].as_text().unwrap().text.as_str();
    assert!(text.contains("src/main.rs:2:"));
    assert!(text.contains("src/lib.rs:2:"));
    assert!(!text.contains("target/build.rs"));
    assert!(!text.contains("vendor/dep.rs"));

    // 2. Un-ignore vendor using .ignore
    std::fs::write(root.join(".ignore"), "!vendor/\n").unwrap();
    let res_unignore = tool.grep_with_cwd(
        GrepSearchParams {
            query: "Hello OpenDuck".to_string(),
            path: None,
            case_sensitive: None,
            is_regex: None,
            includes: None,
            head_limit: None,
            context_lines: None,
        },
        Some(root),
    );
    let text_unignore = res_unignore.content[0].as_text().unwrap().text.as_str();
    assert!(text_unignore.contains("vendor/dep.rs:1:"));
    assert!(!text_unignore.contains("target/build.rs"));

    // 3. Glob includes filter
    let res_glob = tool.grep_with_cwd(
        GrepSearchParams {
            query: "Hello OpenDuck".to_string(),
            path: None,
            case_sensitive: None,
            is_regex: None,
            includes: Some(vec!["lib.rs".to_string()]),
            head_limit: None,
            context_lines: None,
        },
        Some(root),
    );
    let text_glob = res_glob.content[0].as_text().unwrap().text.as_str();
    assert!(text_glob.contains("src/lib.rs"));
    assert!(!text_glob.contains("src/main.rs"));

    // 4. Regex search
    let res_regex = tool.grep_with_cwd(
        GrepSearchParams {
            query: r"pub fn \w+\(\)".to_string(),
            path: None,
            case_sensitive: None,
            is_regex: Some(true),
            includes: None,
            head_limit: None,
            context_lines: None,
        },
        Some(root),
    );
    let text_regex = res_regex.content[0].as_text().unwrap().text.as_str();
    assert!(text_regex.contains("src/lib.rs:1: pub fn search_target()"));
}
