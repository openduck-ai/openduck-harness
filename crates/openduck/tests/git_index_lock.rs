use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime};

use openduck::control::git::{commit, stage};

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

fn index_lock_path(root: &Path) -> PathBuf {
    PathBuf::from(
        git(
            root,
            &[
                "rev-parse",
                "--path-format=absolute",
                "--git-path",
                "index.lock",
            ],
        )
        .trim(),
    )
}

fn backdate_lock(lock: &Path, age: Duration) {
    fs::write(lock, b"lock").unwrap();
    let file = fs::OpenOptions::new().write(true).open(lock).unwrap();
    let modified = SystemTime::now().checked_sub(age).expect("system clock");
    file.set_times(std::fs::FileTimes::new().set_modified(modified))
        .unwrap();
}

#[cfg(unix)]
fn install_pre_commit_hook(root: &Path, script: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;

    let hook = root.join(".git/hooks/pre-commit");
    fs::write(&hook, script).unwrap();
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    hook
}

#[tokio::test]
async fn commits_leave_no_lock_behind() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repo(root);
    let lock = index_lock_path(root);

    fs::write(root.join("a.txt"), "one\n").unwrap();
    let first = commit(root, "first", &["a.txt".into()]).await.unwrap();
    assert!(!first.sha.is_empty());
    assert!(!lock.exists());

    fs::write(root.join("b.txt"), "two\n").unwrap();
    stage(root, &["b.txt".into()]).await.unwrap();
    assert!(!lock.exists());
    let second = commit(root, "second", &[]).await.unwrap();
    assert!(!second.sha.is_empty());
    assert_ne!(first.sha, second.sha);
    assert!(!lock.exists());
}

#[tokio::test]
async fn aged_lock_is_reclaimed_before_commit() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repo(root);
    fs::write(root.join("a.txt"), "one\n").unwrap();
    git(root, &["add", "a.txt"]);
    git(root, &["commit", "-m", "base"]);
    fs::write(root.join("a.txt"), "two\n").unwrap();

    let lock = index_lock_path(root);
    backdate_lock(&lock, Duration::from_secs(60));

    let committed = commit(root, "after aged lock", &["a.txt".into()])
        .await
        .unwrap();
    assert!(!committed.sha.is_empty());
    assert!(!lock.exists());
}

#[tokio::test]
async fn fresh_lock_blocks_commit_and_names_lock_path() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repo(root);
    fs::write(root.join("a.txt"), "one\n").unwrap();
    git(root, &["add", "a.txt"]);
    git(root, &["commit", "-m", "base"]);

    let lock = index_lock_path(root);
    fs::write(&lock, b"fresh").unwrap();

    let error = commit(root, "blocked", &["a.txt".into()])
        .await
        .unwrap_err()
        .to_string();
    assert!(
        error.contains(&lock.display().to_string()),
        "error missing lock path: {error}"
    );
    assert!(
        error.contains("delete that file"),
        "error missing reclaim hint: {error}"
    );
    assert!(lock.exists());
}

#[tokio::test]
async fn non_repo_reports_not_a_git_repository() {
    let dir = tempfile::tempdir().unwrap();
    let error = commit(dir.path(), "nope", &[])
        .await
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("Not a git repository"),
        "unexpected error: {error}"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn cancelled_commit_removes_lock_it_created() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repo(root);
    fs::write(root.join("a.txt"), "one\n").unwrap();
    git(root, &["add", "a.txt"]);
    git(root, &["commit", "-m", "base"]);
    fs::write(root.join("a.txt"), "two\n").unwrap();
    install_pre_commit_hook(
        root,
        "#!/bin/sh\ntouch \"$(git rev-parse --git-path index.lock)\"\nsleep 5\n",
    );

    let result = tokio::time::timeout(
        Duration::from_millis(500),
        commit(root, "hooked", &["a.txt".into()]),
    )
    .await;
    assert!(result.is_err(), "commit finished before the outer timeout");

    let lock = index_lock_path(root);
    // The pre-commit hook may still be running and can recreate index.lock
    // after the cancelled future unlinks it.
    let started = tokio::time::Instant::now();
    while lock.exists() && started.elapsed() < Duration::from_secs(2) {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(
        !lock.exists(),
        "cancelled commit left index.lock at {}",
        lock.display()
    );
}

#[cfg(unix)]
#[tokio::test]
async fn failed_commit_leaves_no_lock_and_repo_stays_usable() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    init_repo(root);
    fs::write(root.join("a.txt"), "one\n").unwrap();
    git(root, &["add", "a.txt"]);
    git(root, &["commit", "-m", "base"]);
    fs::write(root.join("a.txt"), "two\n").unwrap();
    let hook = install_pre_commit_hook(
        root,
        "#!/bin/sh\ntouch \"$(git rev-parse --git-path index.lock)\"\nexit 1\n",
    );

    let error = commit(root, "rejected", &["a.txt".into()])
        .await
        .unwrap_err()
        .to_string();
    assert!(!error.is_empty(), "failed commit returned an empty error");

    let lock = index_lock_path(root);
    assert!(
        !lock.exists(),
        "failed commit left index.lock at {}",
        lock.display()
    );

    fs::remove_file(&hook).unwrap();
    let recovered = commit(root, "recovered", &["a.txt".into()]).await.unwrap();
    assert!(!recovered.sha.is_empty());
    assert!(
        !lock.exists(),
        "recovered commit left index.lock at {}",
        lock.display()
    );
}
