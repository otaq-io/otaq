//! The checkpoint commit around Otaq's rebases (T-0088, GIT-02), against temporary repositories. Every test
//! checks that no stash was ever made.

use std::path::{Path, PathBuf};

use otaq_core::git::Git;
use otaq_core::git::checkpoint::{RebaseError, RebaseOutcome, checkpoint_message};
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
async fn a_clean_worktree_rebases_without_a_checkpoint() {
    let repos = TempRepos::new();
    let (repo, wt) = setup(&repos);
    commit(&repo, "main.txt", "moved on\n");
    let main = git(&repo, &["rev-parse", "main"]);

    let outcome = Git::find()
        .unwrap()
        .rebase_with_checkpoint(&wt, "main")
        .await
        .unwrap();
    assert_eq!(outcome, RebaseOutcome::Clean);
    assert_eq!(git(&wt, &["rev-parse", "HEAD~1"]), main, "on top of main");
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
        .rebase_with_checkpoint(&wt, "main")
        .await
        .unwrap();
    assert_eq!(outcome, RebaseOutcome::Clean);
    assert_eq!(status(&wt), before);
    assert_eq!(
        git(&wt, &["diff", "--cached"]),
        staged_before,
        "the same staged change"
    );
    assert_eq!(read(&wt, "feature.txt"), "feature staged\nand unstaged\n");
    assert_eq!(read(&wt, "both.txt"), "1\n2 changed\n3\n");
    assert_eq!(read(&wt, "notes/todo.md"), "untracked\n");
    assert!(wt.join("main.txt").is_file(), "rebased onto main");
    no_checkpoint_in_history(&wt);
    no_stash(&repo);
}

#[tokio::test]
async fn a_rebase_conflict_puts_everything_back() {
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
        .rebase_with_checkpoint(&wt, "main")
        .await
        .unwrap();
    assert_eq!(
        outcome,
        RebaseOutcome::RebaseConflict {
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
async fn staged_changes_that_dont_fit_the_new_base_put_everything_back() {
    let repos = TempRepos::new();
    let (repo, wt) = setup(&repos);
    commit(&wt, "y.txt", "a\nb\nc\n");
    // main changes the line the staged change touches; the working file is back to the committed text, so the
    // checkpoint itself has nothing for y.txt and the rebase is clean.
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
        .rebase_with_checkpoint(&wt, "main")
        .await
        .unwrap();
    assert_eq!(
        outcome,
        RebaseOutcome::UndoConflict {
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
        .rebase_with_checkpoint(&wt, "main")
        .await
        .unwrap_err();
    match err {
        RebaseError::CheckpointRejected { message, refusal } => {
            assert!(message.contains("lint failed"));
            let refusal = refusal.expect("the hook is named");
            assert_eq!(refusal.name(), "pre-commit");
            assert_eq!(refusal.output(), "lint failed");
        }
        other => panic!("expected a rejected checkpoint, got {other}"),
    }
    assert_eq!(git(&wt, &["rev-parse", "HEAD"]), head, "not rebased");
    assert_eq!(status(&wt), before);
    no_stash(&repo);
}
