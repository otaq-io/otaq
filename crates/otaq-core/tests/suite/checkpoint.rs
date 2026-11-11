//! Bringing a branch up to date by merging main into it, and the checkpoint commit around it (T-0088, GIT-02),
//! against temporary repositories. Every test checks that no stash was ever made; the checkpoint runs no hook, the
//! merge commit runs them all.

use std::path::{Path, PathBuf};

use otaq_core::git::checkpoint::{UpdateError, UpdateOutcome, checkpoint_message};
use otaq_core::git::testing::{TempRepos, commit, git};
use otaq_core::git::{Git, GitError};

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

/// Every hook a commit can run, each logging its name to `log`; `commit-msg` refuses anything but `feat:` and
/// `pre-commit` refuses staged work with a TODO in it, like a Conventional Commits rule and a lint.
fn strict_hooks(repo: &Path, log: &Path) {
    let log = log.display();
    for name in ["prepare-commit-msg", "post-commit", "pre-merge-commit"] {
        hook(repo, name, &format!("echo {name} >> '{log}'"));
    }
    hook(
        repo,
        "pre-commit",
        &format!(
            "echo pre-commit >> '{log}'\n\
             git diff --cached | grep -q TODO && {{ echo 'lint: no TODOs' >&2; exit 1; }}\nexit 0"
        ),
    );
    hook(
        repo,
        "commit-msg",
        &format!(
            "echo commit-msg >> '{log}'\n\
             grep -qE '^feat:' \"$1\" || {{ echo 'commit-msg: use feat:' >&2; exit 1; }}"
        ),
    );
}

fn hooks_run(log: &Path) -> Vec<String> {
    std::fs::read_to_string(log)
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .collect()
}

/// The temporary save is the one place the hooks don't run: a message rule and a lint that would refuse it never
/// see it, and the files come back exactly as they were.
#[tokio::test]
async fn the_temporary_save_runs_no_hook() {
    let repos = TempRepos::new();
    let (repo, wt) = setup(&repos);
    let log = repos.root().join("hooks.log");
    strict_hooks(&repo, &log);
    // A prepare-commit-msg that refuses too: `commit --no-verify` would still run it.
    hook(
        &repo,
        "prepare-commit-msg",
        &format!("echo prepare-commit-msg >> '{}'\nexit 1", log.display()),
    );
    std::fs::write(wt.join("feature.txt"), "// TODO: finish\n").unwrap();
    git(&wt, &["add", "feature.txt"]);
    std::fs::write(wt.join("loose.txt"), "TODO untracked\n").unwrap();
    let head = git(&wt, &["rev-parse", "HEAD"]);
    let before = status(&wt);
    let staged_before = git(&wt, &["diff", "--cached"]);

    let outcome = Git::find()
        .unwrap()
        .merge_with_checkpoint(&wt, "main")
        .await
        .unwrap();
    assert_eq!(outcome, UpdateOutcome::Clean);
    assert_eq!(hooks_run(&log), Vec::<String>::new(), "no hook ran");
    assert_eq!(git(&wt, &["rev-parse", "HEAD"]), head, "nothing to merge");
    assert_eq!(status(&wt), before);
    assert_eq!(git(&wt, &["diff", "--cached"]), staged_before);
    assert_eq!(read(&wt, "feature.txt"), "// TODO: finish\n");
    assert_eq!(read(&wt, "loose.txt"), "TODO untracked\n");
    no_checkpoint_in_history(&wt);
    no_stash(&repo);
}

/// The merge of main that follows the save is a commit that lands: it runs the hooks, and they can refuse it.
#[tokio::test]
async fn the_merge_after_the_temporary_save_still_runs_the_hooks() {
    let repos = TempRepos::new();
    let (repo, wt) = setup(&repos);
    commit(&repo, "main.txt", "moved on\n");
    let log = repos.root().join("hooks.log");
    strict_hooks(&repo, &log);
    std::fs::write(wt.join("feature.txt"), "// TODO: finish\n").unwrap();
    git(&wt, &["add", "feature.txt"]);
    std::fs::write(wt.join("loose.txt"), "untracked\n").unwrap();
    let head = git(&wt, &["rev-parse", "HEAD"]);
    let before = status(&wt);
    let staged_before = git(&wt, &["diff", "--cached"]);

    // git's plain "Merge branch 'main' into feature" isn't `feat:`: commit-msg refuses the merge commit.
    let err = Git::find()
        .unwrap()
        .merge_with_checkpoint(&wt, "main")
        .await
        .unwrap_err();
    let UpdateError::Git(GitError::HookRefused(refusal)) = err else {
        panic!("expected a hook's refusal, got {err}");
    };
    assert_eq!(refusal.name(), "commit-msg");
    assert!(
        refusal.output().starts_with("commit-msg: use feat:"),
        "{}",
        refusal.output()
    );
    assert_eq!(
        hooks_run(&log),
        ["pre-merge-commit", "prepare-commit-msg", "commit-msg"],
        "the merge's hooks, and none for the save"
    );
    assert_eq!(git(&wt, &["rev-parse", "HEAD"]), head, "not merged");
    assert_eq!(status(&wt), before);
    assert_eq!(git(&wt, &["diff", "--cached"]), staged_before);
    assert_eq!(read(&wt, "feature.txt"), "// TODO: finish\n");
    assert_eq!(read(&wt, "loose.txt"), "untracked\n");
    no_stash(&repo);

    // A rule the merge passes: it is made, with the hooks run once, and the files are back on top of it.
    std::fs::remove_file(&log).unwrap();
    hook(
        &repo,
        "commit-msg",
        &format!(
            "echo commit-msg >> '{}'\ngrep -qE '^(feat:|Merge )' \"$1\" || exit 1",
            log.display()
        ),
    );
    let outcome = Git::find()
        .unwrap()
        .merge_with_checkpoint(&wt, "main")
        .await
        .unwrap();
    assert_eq!(outcome, UpdateOutcome::Clean);
    assert_eq!(
        hooks_run(&log),
        ["pre-merge-commit", "prepare-commit-msg", "commit-msg"],
        "putting the files back runs no hook"
    );
    assert!(wt.join("main.txt").is_file(), "main merged in");
    assert_eq!(status(&wt), before);
    assert_eq!(git(&wt, &["diff", "--cached"]), staged_before);
    no_checkpoint_in_history(&wt);
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
        "grep -qE '^(feat|fix):' \"$1\" || { echo 'not a conventional message' >&2; exit 1; }",
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
