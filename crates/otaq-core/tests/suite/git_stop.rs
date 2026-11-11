//! The user's Stop on a running git step (Approve & merge, a start): there is no time limit, so a hanging hook runs
//! until Stop, which ends git, the hook and whatever the hook started, and leaves main, the branch and the worktree
//! as a refusal does. Real hooks in temporary repositories.

use std::path::{Path, PathBuf};
use std::time::Duration;

use otaq_core::domain::{Actor, Status};
use otaq_core::git::checkpoint::UpdateError;
use otaq_core::git::testing::{TempRepos, commit, git};
use otaq_core::git::{Git, GitError, GitWatch};
use otaq_core::review::LocalMergeOutcome;
use otaq_core::service::testing::TestService;

fn me() -> Actor {
    Actor::User("me".into())
}

fn write_hook(hooks: &Path, name: &str, body: &str) {
    let path = hooks.join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    // Git for Windows runs hooks with its own sh; only macOS and Linux need the executable bit.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
}

/// A hook that never ends: it writes its own PID and a background child's to `pids`, then waits on a foreground
/// sleep.
fn hanging(pids: &Path) -> String {
    format!(
        "echo $$ >> '{p}'\nsleep 1000 &\necho $! >> '{p}'\nsleep 1000\n",
        p = pids.display()
    )
}

/// The PIDs the hanging hook wrote, once it wrote both.
fn pids_of(pids: &Path) -> Vec<String> {
    std::fs::read_to_string(pids)
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .collect()
}

async fn wait_until(what: &str, mut check: impl FnMut() -> bool) {
    tokio::time::timeout(Duration::from_secs(30), async {
        while !check() {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("timed out waiting for {what}"));
}

/// The hook and its child are gone: no git process, hook or anything it started is left.
async fn all_gone(pids: &[String]) {
    #[cfg(unix)]
    for pid in pids {
        let pid = pid.trim().to_owned();
        wait_until(&format!("process {pid} to end"), || {
            !std::process::Command::new("kill")
                .args(["-0", &pid])
                .stderr(std::process::Stdio::null())
                .status()
                .unwrap()
                .success()
        })
        .await;
    }
    #[cfg(windows)]
    let _ = pids;
}

fn main_of(repo: &Path) -> String {
    git(repo, &["rev-parse", "main"])
}

/// The Check: a pre-commit that sleeps forever, Approve & merge, Stop: nothing merged, the ticket back in Review with
/// nothing to answer, main and the branch unchanged, no hook process left; with the hook gone the next merge lands.
#[tokio::test(flavor = "multi_thread")]
async fn a_hanging_pre_commit_is_stopped_and_nothing_is_merged() {
    let w = TestService::start().await;
    let project = w.add_project("shop-web").await;
    let repo = PathBuf::from(&project.repo_path);
    let t = w.create("SHOP", "Calc").await;
    let wt = w.start_worktree(&t).await;
    commit(&wt, "calc.js", "add\n");
    w.force_status(&t, Status::WaitingForYou).await;
    let marks = tempfile::tempdir().unwrap();
    let pids = marks.path().join("pids");
    let hooks = repo.join(".git/hooks");
    write_hook(&hooks, "pre-commit", &hanging(&pids));
    let (main, branch) = (main_of(&repo), git(&wt, &["rev-parse", "HEAD"]));
    assert!(
        w.board.stop_git_step("SHOP-1").await.is_err(),
        "nothing runs yet"
    );

    let review = w.review.clone();
    let merging = tokio::spawn(async move {
        review
            .local_merge("SHOP-1", "SHOP-1: Calc", true, &me())
            .await
            .unwrap()
    });
    wait_until("the hook to run", || pids_of(&pids).len() == 2).await;
    let board = w.board.clone();
    let shown = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let t = board.ticket("SHOP-1").await.unwrap();
            if let Some(hook) = t.flags.git.and_then(|g| g.hook) {
                break hook;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("the running hook shows on the ticket");
    assert_eq!(shown.name, "pre-commit");
    // No time limit: it still runs a moment later.
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(!merging.is_finished());

    w.board.stop_git_step("SHOP-1").await.unwrap();
    let outcome = tokio::time::timeout(Duration::from_secs(20), merging)
        .await
        .expect("the merge ends at Stop")
        .unwrap();
    assert_eq!(
        outcome,
        LocalMergeOutcome::Stopped {
            target: "main".into()
        }
    );
    all_gone(&pids_of(&pids)).await;
    assert_eq!(main_of(&repo), main, "nothing merged");
    assert_eq!(
        git(&wt, &["rev-parse", "HEAD"]),
        branch,
        "the branch as it was"
    );
    assert_eq!(git(&wt, &["status", "--porcelain"]), "");
    let after = w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(after.status, Status::WaitingForYou, "back in Review");
    assert_eq!(
        after.auto_merge_stop, None,
        "not a refusal: nothing to answer"
    );
    assert_eq!(after.flags.git, None);

    std::fs::remove_file(hooks.join("pre-commit")).unwrap();
    let outcome = w
        .review
        .local_merge("SHOP-1", "SHOP-1: Calc", true, &me())
        .await
        .unwrap();
    assert!(
        matches!(outcome, LocalMergeOutcome::Merged { .. }),
        "the scratch worktree and the locks are left usable: {outcome:?}"
    );
    w.stop().await;
}

/// A repository with `main` moved on since `otaq/STOP-1` left it, the branch in its own worktree, and its hooks
/// folder.
fn behind(repos: &TempRepos) -> (PathBuf, PathBuf, PathBuf) {
    let repo = repos.repo("shop", 1);
    git(&repo, &["branch", "otaq/STOP-1"]);
    commit(&repo, "main.txt", "moved on\n");
    let hooks = repos.root().join("hooks");
    std::fs::create_dir_all(&hooks).unwrap();
    git(
        &repo,
        &["config", "core.hooksPath", hooks.to_str().unwrap()],
    );
    let wt = repos.root().join("wt");
    git(
        &repo,
        &[
            "worktree",
            "add",
            "--quiet",
            wt.to_str().unwrap(),
            "otaq/STOP-1",
        ],
    );
    commit(&wt, "calc.js", "add\n");
    (repo, wt, hooks)
}

/// Stops the merge of main into the branch while `hook` hangs, and checks that the branch is as it was.
async fn stop_the_merge_of_main_in(hook: &str) {
    let repos = TempRepos::new();
    let (_repo, wt, hooks) = behind(&repos);
    let pids = repos.root().join("pids");
    write_hook(&hooks, hook, &hanging(&pids));
    let head = git(&wt, &["rev-parse", "HEAD"]);
    let watch = GitWatch::new();
    let git_ = Git::find().unwrap().watched(&watch);
    let path = wt.clone();
    let merging = tokio::spawn(async move { git_.merge_with_checkpoint(&path, "main").await });
    wait_until("the hook to run", || pids_of(&pids).len() == 2).await;
    assert!(watch.stop());

    let err = tokio::time::timeout(Duration::from_secs(20), merging)
        .await
        .expect("the merge ends at Stop")
        .unwrap()
        .unwrap_err();
    let UpdateError::Git(GitError::Stopped { hook: stopped, .. }) = &err else {
        panic!("expected a stop, got {err}");
    };
    assert_eq!(stopped.as_deref(), Some(hook));
    all_gone(&pids_of(&pids)).await;
    assert_eq!(
        git(&wt, &["rev-parse", "HEAD"]),
        head,
        "{hook}: the branch as it was"
    );
    assert_eq!(git(&wt, &["status", "--porcelain"]), "", "{hook}");
    let merge_head = git(&wt, &["rev-parse", "--git-path", "MERGE_HEAD"]);
    assert!(
        !wt.join(merge_head).exists(),
        "{hook}: no merge in progress"
    );
    let lock = git(&wt, &["rev-parse", "--git-path", "index.lock"]);
    assert!(!wt.join(lock).exists(), "{hook}: no lock left");
}

/// Stopped before the merge commit (pre-merge-commit) or after it (post-merge): no merge left in progress, and the
/// branch where it was either way.
#[tokio::test]
async fn a_stop_during_the_merge_of_main_leaves_the_branch_as_it_was() {
    stop_the_merge_of_main_in("pre-merge-commit").await;
    stop_the_merge_of_main_in("post-merge").await;
}

/// A Stop between two commands ends the next one before it starts; the commands that put things back after it run.
#[tokio::test]
async fn a_stop_between_commands_ends_the_next_one_only() {
    let repos = TempRepos::new();
    let repo = repos.repo("shop", 1);
    let watch = GitWatch::new();
    let git_ = Git::find().unwrap().watched(&watch);
    assert!(watch.stop());
    let err = git_.run(&repo, &["status"]).await.unwrap_err();
    assert!(err.is_stopped(), "{err}");
    git_.run(&repo, &["status"]).await.unwrap();
}
