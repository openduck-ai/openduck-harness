use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

use openduck::control::git::commit;

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

fn init_repo(root: &Path) {
    git(root, &["init"]);
    git(root, &["config", "user.name", "Test"]);
    git(root, &["config", "user.email", "test@example.com"]);
}

#[cfg(unix)]
#[tokio::test]
async fn cancelled_commit_releases_repo_write_lock() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repo(root);
    fs::write(root.join("a.txt"), "one\n").unwrap();
    git(root, &["add", "a.txt"]);
    git(root, &["commit", "-m", "base"]);
    fs::write(root.join("a.txt"), "two\n").unwrap();

    let hook = root.join(".git/hooks/pre-commit");
    fs::write(&hook, "#!/bin/sh\nsleep 5\n").unwrap();
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();

    let cancelled = tokio::time::timeout(
        Duration::from_millis(500),
        commit(root, "hooked", &["a.txt".into()]),
    )
    .await;
    assert!(
        cancelled.is_err(),
        "commit finished before the outer timeout"
    );
    fs::remove_file(&hook).unwrap();

    let recovered = tokio::time::timeout(
        Duration::from_secs(30),
        commit(root, "recovered", &["a.txt".into()]),
    )
    .await;
    let recovered = recovered.expect("second commit blocked on the repo write mutex");
    assert!(
        recovered.is_ok(),
        "second commit failed: {:?}",
        recovered.err()
    );
    assert!(!recovered.unwrap().sha.is_empty());
}
