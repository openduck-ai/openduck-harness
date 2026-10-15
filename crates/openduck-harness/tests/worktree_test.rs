use openduck_harness::sandbox::worktree::{UntrackedFileMode, WorktreeConfig, WorktreeSandbox};
use openduck_harness::sandbox::SandboxDriver;
use openduck_harness::types::ExecOptions;
use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::tempdir;

fn run_git(cwd: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .expect("git");
    assert!(
        output.status.success(),
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn init_git_repo(dir: &Path) {
    run_git(dir, &["-c", "init.defaultBranch=main", "init"]);
    run_git(dir, &["config", "user.name", "Tester"]);
    run_git(dir, &["config", "user.email", "tester@example.com"]);
}

#[tokio::test]
async fn test_worktree_sandbox_lifecycle() {
    let repo_temp = tempdir().unwrap();
    let repo_root = repo_temp.path();
    init_git_repo(repo_root);

    // Create tracked file
    fs::write(repo_root.join("README.md"), "# Original\n").unwrap();
    run_git(repo_root, &["add", "README.md"]);
    run_git(repo_root, &["commit", "-m", "Initial commit"]);

    // Create untracked .env file
    fs::write(repo_root.join(".env"), "SECRET_KEY=12345\n").unwrap();

    // Configure worktree sandbox
    let config = WorktreeConfig::new(repo_root)
        .with_branch("feature/test-worktree")
        .with_untracked_mode(UntrackedFileMode::Symlink);

    let mut sandbox = WorktreeSandbox::new(config);
    sandbox.initialize().await.unwrap();

    let wt_path = sandbox.workspace_root().to_path_buf();
    assert!(wt_path.exists());
    assert_eq!(sandbox.branch_name(), "feature/test-worktree");

    // Tracked file exists in worktree
    let readme = sandbox.read_file(Path::new("README.md")).await.unwrap();
    assert_eq!(String::from_utf8_lossy(&readme), "# Original\n");

    // Untracked .env was propagated
    let env_content = sandbox.read_file(Path::new(".env")).await.unwrap();
    assert!(String::from_utf8_lossy(&env_content).contains("SECRET_KEY=12345"));

    // Write file in worktree
    sandbox
        .write_file(Path::new("new_file.txt"), b"created in worktree")
        .await
        .unwrap();

    let read_back = sandbox.read_file(Path::new("new_file.txt")).await.unwrap();
    assert_eq!(&read_back, b"created in worktree");

    // Execute command in worktree
    let exec_res = sandbox
        .exec_command("git status --porcelain", &ExecOptions::default())
        .await
        .unwrap();
    assert!(exec_res.stdout.contains("new_file.txt"));

    // Snapshot and restore
    let snap_id = sandbox.snapshot("test-snap").await.unwrap();
    sandbox
        .write_file(Path::new("extra_scratch.txt"), b"scratch")
        .await
        .unwrap();
    assert!(sandbox
        .read_file(Path::new("extra_scratch.txt"))
        .await
        .is_ok());

    sandbox.restore_snapshot(&snap_id).await.unwrap();
    assert!(sandbox
        .read_file(Path::new("extra_scratch.txt"))
        .await
        .is_err());

    // Cleanup
    sandbox.cleanup().await.unwrap();
    assert!(!wt_path.exists());

    // Verify the created file was committed to the branch
    let show_output = Command::new("git")
        .args(["show", "feature/test-worktree:new_file.txt"])
        .current_dir(repo_root)
        .output()
        .expect("git show");
    assert!(
        show_output.status.success(),
        "File should be committed to branch feature/test-worktree"
    );
    assert_eq!(
        String::from_utf8_lossy(&show_output.stdout),
        "created in worktree"
    );
}
