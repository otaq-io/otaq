//! Bringing a branch up to date by merging main into it, and the checkpoint commit around it (T-0088, GIT-02),
//! against temporary repositories. Every test checks that no stash was ever made.

use std::path::{Path, PathBuf};

use otaq_core::git::Git;
use otaq_core::git::checkpoint::{UpdateError, UpdateOutcome, checkpoint_message};
use otaq_core::git::testing::{TempRepos, commit, git};

/// `main` checked out in the repository, `feature` (one commit on `feature.txt`) in a worktree beside it.
fn setup(repos: &TempRepos) -> (PathBuf, PathBuf) {
    let repo = repos.repo("shop-web", 1);
    let wt = repos.root().join("wt-feature");
    git(
        &repo,
        &[
            "worktree",
            "add",
            "--quiet",
            "-b",
            "feature",
            wt.to_str().unwrap(),
        ],
    );
    commit(&wt, "feature.txt", "feature\n");
    (repo, wt)
}

fn status(wt: &Path) -> String {
    git(wt, &["status", "--porcelain=v1", "--untracked-files=all"])
}

fn read(wt: &Path, file: &str) -> String {
    std::fs::read_to_string(wt.join(file)).unwrap()
}

fn no_stash(repo: &Path) {
    assert_eq!(git(repo, &["stash", "list"]), "", "no stash entries");
}

fn no_checkpoint_in_history(wt: &Path) {
    let log = git(wt, &["log", "--format=%s"]);
    assert!(!log.contains(&checkpoint_message()), "{log}");
}

#[tokio::test]
async fn a_clean_worktree_merges_main_in_without_a_checkpoint() {
    let repos = TempRepos::new();
    let (repo, wt) = setup(&repos);
    commit(&repo, "main.txt", "moved on\n");
    let main = git(&repo, &["rev-parse", "main"]);

    let outcome = Git::find()
        .unwrap()
        .merge_with_checkpoint(&wt, "main")
        .await
        .unwrap();
    assert_eq!(outcome, UpdateOutcome::Clean);
    assert_eq!(git(&wt, &["rev-parse", "HEAD^2"]), main, "main merged in");
    assert_eq!(
        git(&wt, &["log", "-1", "--format=%s"]),
        "Merge branch 'main' into feature",
        "git's plain merge message"
    );
    assert_eq!(status(&wt), "");
    // No checkpoint was made at all: the reflog has no commit with its message.
    let reflog = git(&wt, &["reflog", "--format=%gs"]);
    assert!(!reflog.contains("wip"), "{reflog}");
    no_stash(&repo);
}

#[tokio::test]
async fn staged_unstaged_and_untracked_files_come_back_as_they_were() {
    let repos = TempRepos::new();
    let (repo, wt) = setup(&repos);
    commit(&wt, "both.txt", "1\n2\n3\n");
    commit(&repo, "main.txt", "moved on\n");
    // Staged: a new file and a change; unstaged: a further change on top of a staged one and a plain change;
    // untracked: a new file in a new folder.
    std::fs::write(wt.join("staged_new.txt"), "new\n").unwrap();
    std::fs::write(wt.join("feature.txt"), "feature staged\n").unwrap();
    git(&wt, &["add", "staged_new.txt", "feature.txt"]);
    std::fs::write(wt.join("feature.txt"), "feature staged\nand unstaged\n").unwrap();
    std::fs::write(wt.join("both.txt"), "1\n2 changed\n3\n").unwrap();
    std::fs::create_dir(wt.join("notes")).unwrap();
    std::fs::write(wt.join("notes/todo.md"), "untracked\n").unwrap();
    let before = status(&wt);
    let staged_before = git(&wt, &["diff", "--cached"]);

    let outcome = Git::find()
        .unwrap()
        .merge_with_checkpoint(&wt, "main")
        .await
        .unwrap();
    assert_eq!(outcome, UpdateOutcome::Clean);
    assert_eq!(status(&wt), before);
    assert_eq!(
        git(&wt, &["diff", "--cached"]),
        staged_before,
        "the same staged change"
    );
    assert_eq!(read(&wt, "feature.txt"), "feature staged\nand unstaged\n");
    assert_eq!(read(&wt, "both.txt"), "1\n2 changed\n3\n");
    assert_eq!(read(&wt, "notes/todo.md"), "untracked\n");
    assert!(wt.join("main.txt").is_file(), "main merged in");
    no_checkpoint_in_history(&wt);
    no_stash(&repo);
}

#[tokio::test]
async fn a_merge_conflict_puts_everything_back() {
    let repos = TempRepos::new();
    let (repo, wt) = setup(&repos);
    commit(&wt, "lib.txt", "feature version\n");
    commit(&repo, "lib.txt", "main version\n");
    std::fs::write(wt.join("scratch.txt"), "untracked\n").unwrap();
    std::fs::write(wt.join("feature.txt"), "edited\n").unwrap();
    let head = git(&wt, &["rev-parse", "HEAD"]);
    let before = status(&wt);

    let outcome = Git::find()
        .unwrap()
        .merge_with_checkpoint(&wt, "main")
        .await
        .unwrap();
    assert_eq!(
        outcome,
        UpdateOutcome::Conflict {
            files: vec!["lib.txt".into()]
        }
    );
    assert_eq!(
        git(&wt, &["rev-parse", "HEAD"]),
        head,
        "the branch as it was"
    );
    assert_eq!(status(&wt), before);
    assert_eq!(read(&wt, "feature.txt"), "edited\n");
    assert_eq!(read(&wt, "lib.txt"), "feature version\n");
    assert_eq!(read(&wt, "scratch.txt"), "untracked\n");
    no_checkpoint_in_history(&wt);
    no_stash(&repo);
}

#[tokio::test]
async fn staged_changes_that_dont_fit_the_merged_branch_put_everything_back() {
    let repos = TempRepos::new();
    let (repo, wt) = setup(&repos);
    commit(&wt, "y.txt", "a\nb\nc\n");
    // main changes the line the staged change touches; the working file is back to the committed text, so the
    // checkpoint itself has nothing for y.txt and the merge is clean.
    git(&repo, &["merge", "--quiet", "--no-edit", "feature"]);
    commit(&repo, "y.txt", "a\nX\nc\n");
    std::fs::write(wt.join("y.txt"), "a\nB\nc\n").unwrap();
    git(&wt, &["add", "y.txt"]);
    std::fs::write(wt.join("y.txt"), "a\nb\nc\n").unwrap();
    std::fs::write(wt.join("other.txt"), "untracked\n").unwrap();
    let head = git(&wt, &["rev-parse", "HEAD"]);
    let before = status(&wt);
    let staged_before = git(&wt, &["diff", "--cached"]);

    let outcome = Git::find()
        .unwrap()
        .merge_with_checkpoint(&wt, "main")
        .await
        .unwrap();
    assert_eq!(
        outcome,
        UpdateOutcome::UndoConflict {
            files: vec!["y.txt".into()]
        }
    );
    assert_eq!(git(&wt, &["rev-parse", "HEAD"]), head);
    assert_eq!(status(&wt), before);
    assert_eq!(git(&wt, &["diff", "--cached"]), staged_before);
    assert_eq!(read(&wt, "y.txt"), "a\nb\nc\n");
    assert_eq!(read(&wt, "other.txt"), "untracked\n");
    no_stash(&repo);
}

#[tokio::test]
async fn a_rejecting_pre_commit_hook_stops_with_an_error_and_changes_nothing() {
    let repos = TempRepos::new();
    let (repo, wt) = setup(&repos);
    commit(&repo, "main.txt", "moved on\n");
    let hook = repo.join(".git/hooks/pre-commit");
    std::fs::write(&hook, "#!/bin/sh\necho 'lint failed' >&2\nexit 1\n").unwrap();
    // Git for Windows runs hooks with its own sh; only macOS and Linux need the executable bit.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    std::fs::write(wt.join("feature.txt"), "dirty\n").unwrap();
    git(&wt, &["add", "feature.txt"]);
    std::fs::write(wt.join("loose.txt"), "untracked\n").unwrap();
    let head = git(&wt, &["rev-parse", "HEAD"]);
    let before = status(&wt);

    let err = Git::find()
        .unwrap()
        .merge_with_checkpoint(&wt, "main")
        .await
        .unwrap_err();
    match err {
        UpdateError::CheckpointRejected { message } => assert!(message.contains("lint failed")),
        other => panic!("expected a rejected checkpoint, got {other}"),
    }
    assert_eq!(git(&wt, &["rev-parse", "HEAD"]), head, "not merged");
    assert_eq!(status(&wt), before);
    no_stash(&repo);
}

/// A hook script in the repository's `.git/hooks` (shared by its worktrees).
fn hook(repo: &Path, name: &str, body: &str) {
    let path = repo.join(".git/hooks").join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    // Git for Windows runs hooks with its own sh; only macOS and Linux need the executable bit.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
}

#[tokio::test]
async fn a_pre_rebase_hook_that_refuses_never_runs() {
    let repos = TempRepos::new();
    let (repo, wt) = setup(&repos);
    commit(&repo, "main.txt", "moved on\n");
    let log = repos.root().join("hooks.log");
    hook(
        &repo,
        "pre-rebase",
        &format!(
            "echo pre-rebase >> '{}'\necho 'merge instead' >&2\nexit 1",
            log.display()
        ),
    );
    std::fs::write(wt.join("feature.txt"), "dirty\n").unwrap();

    let outcome = Git::find()
        .unwrap()
        .merge_with_checkpoint(&wt, "main")
        .await
        .unwrap();
    assert_eq!(outcome, UpdateOutcome::Clean);
    assert!(wt.join("main.txt").is_file(), "main merged in");
    assert_eq!(read(&wt, "feature.txt"), "dirty\n");
    assert!(!log.exists(), "pre-rebase never ran");
    no_checkpoint_in_history(&wt);
    no_stash(&repo);
}

#[tokio::test]
async fn the_merge_commit_runs_the_hooks_and_a_refusal_changes_nothing() {
    let repos = TempRepos::new();
    let (repo, wt) = setup(&repos);
    commit(&repo, "main.txt", "moved on\n");
    // A Conventional Commits rule: git's plain "Merge branch …" message doesn't pass.
    hook(
        &repo,
        "commit-msg",
        "grep -qE '^(feat|fix|wip):' \"$1\" || { echo 'not a conventional message' >&2; exit 1; }",
    );
    std::fs::write(wt.join("feature.txt"), "staged\n").unwrap();
    git(&wt, &["add", "feature.txt"]);
    std::fs::write(wt.join("loose.txt"), "untracked\n").unwrap();
    let head = git(&wt, &["rev-parse", "HEAD"]);
    let before = status(&wt);
    let staged_before = git(&wt, &["diff", "--cached"]);

    let err = Git::find()
        .unwrap()
        .merge_with_checkpoint(&wt, "main")
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("not a conventional message"),
        "{err}"
    );
    assert_eq!(git(&wt, &["rev-parse", "HEAD"]), head, "not merged");
    assert_eq!(status(&wt), before);
    assert_eq!(git(&wt, &["diff", "--cached"]), staged_before);
    assert_eq!(read(&wt, "loose.txt"), "untracked\n");
    assert!(!wt.join("main.txt").exists());
    no_checkpoint_in_history(&wt);
    no_stash(&repo);

    // A clean worktree: the same refusal, and no merge left in progress.
    git(&wt, &["reset", "--quiet", "--hard"]);
    std::fs::remove_file(wt.join("loose.txt")).unwrap();
    let err = Git::find()
        .unwrap()
        .merge_in(&wt, "main")
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("not a conventional message"),
        "{err}"
    );
    assert_eq!(git(&wt, &["rev-parse", "HEAD"]), head);
    assert_eq!(status(&wt), "");
    let merge_head = git(&wt, &["rev-parse", "--git-path", "MERGE_HEAD"]);
    assert!(!wt.join(merge_head).exists(), "no merge in progress");
}

#[tokio::test]
async fn uncommitted_changes_that_conflict_with_main_put_everything_back() {
    let repos = TempRepos::new();
    let (repo, wt) = setup(&repos);
    commit(&wt, "lib.txt", "base\n");
    git(&repo, &["merge", "--quiet", "--no-edit", "feature"]);
    commit(&repo, "lib.txt", "main version\n");
    std::fs::write(wt.join("lib.txt"), "my uncommitted version\n").unwrap();
    let head = git(&wt, &["rev-parse", "HEAD"]);
    let before = status(&wt);

    let outcome = Git::find()
        .unwrap()
        .merge_with_checkpoint(&wt, "main")
        .await
        .unwrap();
    assert_eq!(
        outcome,
        UpdateOutcome::UndoConflict {
            files: vec!["lib.txt".into()]
        }
    );
    assert_eq!(git(&wt, &["rev-parse", "HEAD"]), head);
    assert_eq!(status(&wt), before);
    assert_eq!(read(&wt, "lib.txt"), "my uncommitted version\n");
    no_checkpoint_in_history(&wt);
    no_stash(&repo);
}
