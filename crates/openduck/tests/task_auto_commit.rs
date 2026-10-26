use openduck::control::git::TaskAutoCommit;
use openduck_harness::project::ProjectHarnessConfig;
use std::fs;
use std::path::Path;
use std::process::Command;

fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

#[test]
fn auto_commit_defaults_on_and_can_be_disabled_in_project_settings() {
    assert!(ProjectHarnessConfig::default().execution.auto_commit);
    for key in ["autoCommit", "auto_commit"] {
        let config: ProjectHarnessConfig =
            serde_yaml::from_str(&format!("execution:\n  {key}: false\n")).unwrap();
        assert!(!config.execution.auto_commit);
        let saved = serde_yaml::to_string(&config).unwrap();
        let reloaded: ProjectHarnessConfig = serde_yaml::from_str(&saved).unwrap();
        assert!(!reloaded.execution.auto_commit);
    }
}

#[tokio::test]
async fn non_git_workspace_is_skipped() {
    let dir = tempfile::tempdir().unwrap();
    assert!(TaskAutoCommit::capture(dir.path(), vec![])
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn commits_task_files_with_attribution_and_preserves_existing_staged_changes() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    git(root, &["init"]);
    git(root, &["config", "user.name", "Test"]);
    git(root, &["config", "user.email", "test@example.com"]);
    fs::write(root.join("existing.txt"), "original").unwrap();
    git(root, &["add", "existing.txt"]);
    git(root, &["commit", "-m", "initial"]);
    fs::write(root.join("existing.txt"), "user changes").unwrap();
    git(root, &["add", "existing.txt"]);

    let snapshot = TaskAutoCommit::capture(root, vec![root.join(".agent")])
        .await
        .unwrap()
        .unwrap();
    fs::create_dir(root.join("src")).unwrap();
    fs::write(root.join("src/new.txt"), "task changes").unwrap();
    fs::create_dir(root.join(".agent")).unwrap();
    fs::write(root.join(".agent/notes.md"), "scratch notes").unwrap();
    let commit = snapshot.finish().await.unwrap().unwrap();
    assert_eq!(git(root, &["rev-parse", "HEAD"]).trim(), commit.sha);
    assert!(git(root, &["log", "-1", "--format=%B"]).contains("Generated-by: OpenDuck Harness"));
    assert_eq!(
        git(
            root,
            &["diff-tree", "--no-commit-id", "--name-only", "-r", "HEAD"]
        )
        .trim(),
        "src/new.txt"
    );
    assert_eq!(
        git(root, &["diff", "--cached", "--name-only"]).trim(),
        "existing.txt"
    );
    let snapshot = TaskAutoCommit::capture(root, vec![])
        .await
        .unwrap()
        .unwrap();
    assert!(snapshot.finish().await.unwrap().is_none());
}
