//! Local merge in a scratch worktree (T-0089, GIT-01, GIT-03, REV-08, EPC-03), against temporary repositories.
//! The repository's own folder stands for the user's checkout.

use std::path::{Path, PathBuf};

use otaq_core::board::projects::AddProject;
use otaq_core::board::tickets::{EventKind, Ticket};
use otaq_core::domain::{Actor, AutoMergeStop, RemoteWarning, Status, WentOn};
use otaq_core::flow::Delivers;
use otaq_core::flow::default::testing::delivering;
use otaq_core::git::GitStep;
use otaq_core::git::testing::{commit, git, snapshot};
use otaq_core::review::LocalMergeOutcome;
use otaq_core::service::testing::TestService;

fn me() -> Actor {
    Actor::User("me".into())
}

/// A ticket in Ready for Review with one commit on `file` in its worktree.
async fn reviewed(w: &TestService, title: &str, file: &str, content: &str) -> (Ticket, PathBuf) {
    let t = w.create("SHOP", title).await;
    let wt = w.start_worktree(&t).await;
    commit(&wt, file, content);
    w.force_status(&t, Status::WaitingForYou).await;
    (w.board.ticket(&t.key.to_string()).await.unwrap(), wt)
}

fn user_files(repo: &Path) -> Vec<(String, String)> {
    snapshot(repo)
        .into_iter()
        .filter(|p| !p.starts_with(".git"))
        .filter(|p| repo.join(p).is_file())
        .map(|p| {
            let content = std::fs::read_to_string(repo.join(&p)).unwrap_or_default();
            (p, content)
        })
        .collect()
}

fn no_stash(repo: &Path) {
    assert_eq!(git(repo, &["stash", "list"]), "");
}

#[tokio::test]
async fn a_clean_merge_squashes_into_main_and_fast_forwards_the_clean_checkout() {
    let w = TestService::start().await;
    let project = w.add_project("shop-web").await;
    let repo = PathBuf::from(&project.repo_path);
    let (t, wt) = reviewed(&w, "Checkout API", "api.ts", "api\n").await;
    commit(&wt, "api.ts", "api v2\n");
    let waiting = w.create("SHOP", "Checkout page").await;
    w.board
        .add_dependency("SHOP-2", "SHOP-1", &me())
        .await
        .unwrap();
    w.force_status(&waiting, Status::Queued).await;
    // main moved on after the ticket left it: merging it into the branch takes care of it.
    commit(&repo, "other.ts", "other\n");
    let old_main = git(&repo, &["rev-parse", "main"]);

    let proposal = w.board.merge_proposal("SHOP-1").await.unwrap();
    assert_eq!(proposal.target, "main");
    assert_eq!(proposal.message, "SHOP-1: Checkout API");

    let outcome = w
        .review
        .local_merge(
            "SHOP-1",
            "SHOP-1: Checkout API\n\nAdds the API.",
            false,
            &me(),
        )
        .await
        .unwrap();
    let LocalMergeOutcome::Merged {
        ticket,
        target,
        commit,
        started,
        hook_changed,
    } = outcome
    else {
        panic!("expected merged, got {outcome:?}");
    };
    // No hook changed anything: nothing extra.
    assert_eq!(hook_changed, None);
    assert_eq!(target, "main");
    assert_eq!(ticket.status, Status::Merged);
    assert_eq!(started, ["SHOP-2".parse().unwrap()]);
    let main = git(&repo, &["rev-parse", "main"]);
    assert_eq!(commit, main);
    assert_eq!(
        git(&repo, &["rev-parse", "main~1"]),
        old_main,
        "one squash commit on top"
    );
    assert_eq!(
        git(&repo, &["log", "-1", "--format=%B", "main"]),
        "SHOP-1: Checkout API\n\nAdds the API."
    );
    // The user's clean checkout was fast-forwarded.
    assert_eq!(git(&repo, &["rev-parse", "HEAD"]), main);
    assert_eq!(
        std::fs::read_to_string(repo.join("api.ts")).unwrap(),
        "api v2\n"
    );
    assert_eq!(git(&repo, &["status", "--porcelain"]), "");
    let row = w.board.store().ticket(&t.id).await.unwrap().unwrap();
    assert_eq!(row.outcome.as_deref(), Some("merged"));
    assert_eq!(row.merge_commit_sha.as_deref(), Some(main.as_str()));
    assert_eq!(
        row.base_sha.as_deref(),
        Some(old_main.as_str()),
        "main merged into the branch"
    );
    // The Done ticket's diff is still its own change.
    let diff = w.board.diff_summary("SHOP-1").await.unwrap();
    let paths: Vec<_> = diff.files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, ["api.ts"]);
    let history = w.board.history("SHOP-1").await.unwrap();
    assert!(history.iter().any(|e| e.kind == EventKind::Merged));
    // The scratch worktree is Otaq's, inside the data folder.
    assert!(w.data.scratch_path("shop-web").unwrap().is_dir());
    no_stash(&repo);
    w.stop().await;
}

#[tokio::test]
async fn a_dirty_checkout_of_the_base_is_refused_and_left_alone() {
    let w = TestService::start().await;
    let project = w.add_project("shop-web").await;
    let repo = PathBuf::from(&project.repo_path);
    reviewed(&w, "Checkout API", "api.ts", "api\n").await;
    std::fs::write(repo.join("file1.txt"), "my edit\n").unwrap();
    let before = user_files(&repo);
    let main = git(&repo, &["rev-parse", "main"]);

    let outcome = w
        .review
        .local_merge("SHOP-1", "m", false, &me())
        .await
        .unwrap();
    assert_eq!(
        outcome,
        LocalMergeOutcome::RefusedDirtyCheckout {
            message: "shop-web has uncommitted changes on main. Clean up and retry.".into()
        }
    );
    assert_eq!(git(&repo, &["rev-parse", "main"]), main, "main unchanged");
    assert_eq!(user_files(&repo), before, "the user's edit untouched");
    assert_eq!(w.status("SHOP-1").await, Status::WaitingForYou);
    no_stash(&repo);
    w.stop().await;
}

#[tokio::test]
async fn uncommitted_files_in_the_ticket_are_refused() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let (_, wt) = reviewed(&w, "Price table", "price.ts", "p\n").await;
    std::fs::write(wt.join("price.ts"), "changed\n").unwrap();
    std::fs::write(wt.join("scratch.txt"), "x\n").unwrap();

    let outcome = w
        .review
        .local_merge("SHOP-1", "m", false, &me())
        .await
        .unwrap();
    assert_eq!(
        outcome,
        LocalMergeOutcome::RefusedUncommitted {
            files: vec!["price.ts".into(), "scratch.txt".into()],
            message: "SHOP-1 has 2 uncommitted files".into(),
        }
    );
    assert_eq!(w.status("SHOP-1").await, Status::WaitingForYou);
    w.stop().await;
}

#[tokio::test]
async fn a_conflict_merging_main_in_merges_nothing() {
    let w = TestService::start().await;
    let project = w.add_project("shop-web").await;
    let repo = PathBuf::from(&project.repo_path);
    let (t, wt) = reviewed(&w, "Pricing", "lib.ts", "ticket\n").await;
    commit(&repo, "lib.ts", "main\n");
    let main = git(&repo, &["rev-parse", "main"]);
    let head = git(&wt, &["rev-parse", "HEAD"]);
    w.agent_shell(&t).await;

    let outcome = w
        .review
        .local_merge("SHOP-1", "m", false, &me())
        .await
        .unwrap();
    assert_eq!(
        outcome,
        LocalMergeOutcome::Conflict {
            target: "main".into(),
            files: vec!["lib.ts".into()]
        }
    );
    assert_eq!(git(&repo, &["rev-parse", "main"]), main);
    assert_eq!(
        git(&wt, &["rev-parse", "HEAD"]),
        head,
        "the branch as it was"
    );
    assert_eq!(git(&wt, &["status", "--porcelain"]), "");
    // FLW-17: back to Dev at once, its agent fixes it with restack (no click).
    assert_eq!(w.status("SHOP-1").await, Status::MergeConflictFixing);
    let row = w.board.store().ticket(&t.id).await.unwrap().unwrap();
    assert_eq!(row.merge_commit_sha, None);
    no_stash(&repo);
    w.stop().await;
}

/// A project whose repository is a clone of a bare `origin` (base `main`), with `seed`, another clone, to
/// push from.
async fn with_origin(w: &TestService) -> (PathBuf, PathBuf) {
    let clone = w.repos.clone_with_origin("shop-web", "main", 1);
    w.board
        .add_project(&AddProject {
            path: clone.display().to_string(),
            ..Default::default()
        })
        .await
        .unwrap();
    (clone, w.repos.root().join("shop-web-seed"))
}

#[tokio::test]
async fn a_base_behind_its_remote_is_fast_forwarded_first_and_never_pushed() {
    let w = TestService::start().await;
    let (repo, seed) = with_origin(&w).await;
    reviewed(&w, "Checkout API", "api.ts", "api\n").await;
    commit(&seed, "team.ts", "from the team\n");
    git(&seed, &["push", "--quiet", "origin", "main"]);
    let remote = git(&seed, &["rev-parse", "main"]);

    let outcome = w
        .review
        .local_merge("SHOP-1", "SHOP-1: Checkout API", false, &me())
        .await
        .unwrap();
    assert!(
        matches!(outcome, LocalMergeOutcome::Merged { .. }),
        "{outcome:?}"
    );
    assert_eq!(
        git(&repo, &["rev-parse", "main~1"]),
        remote,
        "on top of the fetched base"
    );
    assert!(repo.join("team.ts").is_file() && repo.join("api.ts").is_file());
    // Never pushed: the remote still has only the team's commit.
    git(&seed, &["fetch", "--quiet", "origin"]);
    assert_eq!(git(&seed, &["rev-parse", "origin/main"]), remote);
    w.stop().await;
}

#[tokio::test]
async fn a_diverged_base_is_refused() {
    let w = TestService::start().await;
    let (repo, seed) = with_origin(&w).await;
    reviewed(&w, "Checkout API", "api.ts", "api\n").await;
    commit(&seed, "team.ts", "from the team\n");
    git(&seed, &["push", "--quiet", "origin", "main"]);
    commit(&repo, "mine.ts", "local only\n");
    let main = git(&repo, &["rev-parse", "main"]);

    let outcome = w
        .review
        .local_merge("SHOP-1", "m", false, &me())
        .await
        .unwrap();
    let LocalMergeOutcome::RefusedDiverged { message } = outcome else {
        panic!("expected diverged, got {outcome:?}");
    };
    assert_eq!(
        message,
        "`main` and `origin/main` have diverged. Push or pull, then retry"
    );
    assert_eq!(git(&repo, &["rev-parse", "main"]), main);
    assert_eq!(w.status("SHOP-1").await, Status::WaitingForYou);
    w.stop().await;
}

/// d0121: `origin` can't be reached. A flow that delivers locally (the default lets the user pick) merges into the local
/// `main` as it is, with the warning on the ticket and in History; a PR flow still fails as before.
#[tokio::test]
async fn an_unreachable_remote_merges_into_the_local_base_with_a_warning() {
    let w = TestService::start().await;
    let (repo, _seed) = with_origin(&w).await;
    git(
        &repo,
        &[
            "remote",
            "set-url",
            "origin",
            "/nonexistent/shop-web-origin.git",
        ],
    );
    reviewed(&w, "Checkout API", "api.ts", "api\n").await;
    let waiting = w.create("SHOP", "Checkout page").await;
    w.board
        .add_dependency("SHOP-2", "SHOP-1", &me())
        .await
        .unwrap();
    w.force_status(&waiting, Status::Queued).await;
    let main = git(&repo, &["rev-parse", "main"]);

    let outcome = w
        .review
        .local_merge("SHOP-1", "SHOP-1: Checkout API", false, &me())
        .await
        .unwrap();
    let LocalMergeOutcome::Merged {
        ticket, started, ..
    } = outcome
    else {
        panic!("expected merged, got {outcome:?}");
    };
    assert_eq!(
        git(&repo, &["rev-parse", "main~1"]),
        main,
        "onto the local main"
    );
    assert_eq!(
        started,
        ["SHOP-2".parse().unwrap()],
        "the freed dependent starts"
    );
    let warning = ticket.work.unwrap().remote_warning.unwrap();
    assert_eq!(
        (
            warning.went_on,
            warning.branch.as_str(),
            warning.remote.as_str()
        ),
        (WentOn::Merged, "main", "origin")
    );
    assert!(
        warning
            .reason
            .starts_with("git fetch --quiet origin failed"),
        "{}",
        warning.reason
    );
    let history = w.board.history("SHOP-1").await.unwrap();
    let logged = history
        .iter()
        .find(|e| e.kind == EventKind::RemoteUnreachable)
        .expect("History logs it");
    assert_eq!(logged.payload["went_on"], "merged");

    // A PR flow needs the remote: the merge fails as before, nothing moves.
    w.board
        .edit_flow_for_tests("SHOP", delivering(Delivers::Pr))
        .await;
    reviewed(&w, "Checkout total", "total.ts", "total\n").await;
    let main = git(&repo, &["rev-parse", "main"]);
    let err = w
        .review
        .local_merge("SHOP-3", "SHOP-3: Checkout total", false, &me())
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("git fetch --quiet origin failed"),
        "{err}"
    );
    assert_eq!(git(&repo, &["rev-parse", "main"]), main);
    assert_eq!(w.status("SHOP-3").await, Status::WaitingForYou);
    w.stop().await;
}

/// d0121: the next fetch for the ticket that works takes its warning away.
#[tokio::test]
async fn a_good_fetch_clears_the_warning() {
    let w = TestService::start().await;
    with_origin(&w).await;
    let (t, _) = reviewed(&w, "Checkout API", "api.ts", "api\n").await;
    let warning = RemoteWarning {
        went_on: WentOn::Started,
        branch: "main".into(),
        remote: "origin".into(),
        reason: "git fetch --quiet origin failed (exit 128): Host key verification failed.".into(),
    };
    w.board
        .record_remote_warning(&t.id, &warning, &me())
        .await
        .unwrap();
    let before = w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(before.work.unwrap().remote_warning, Some(warning));

    let outcome = w
        .review
        .local_merge("SHOP-1", "SHOP-1: Checkout API", false, &me())
        .await
        .unwrap();
    let LocalMergeOutcome::Merged { ticket, .. } = outcome else {
        panic!("expected merged, got {outcome:?}");
    };
    assert_eq!(ticket.work.unwrap().remote_warning, None);
    w.stop().await;
}

/// The project's variables that point git at a throwaway global settings file, and the user's own `git` the same
/// way: the machine's own settings are never read or written.
fn throwaway_global(global: &Path) -> Vec<(String, String)> {
    vec![
        (
            "GIT_CONFIG_GLOBAL".into(),
            global.to_str().unwrap().to_owned(),
        ),
        ("GIT_CONFIG_NOSYSTEM".into(), "1".into()),
    ]
}

/// A fresh machine often has no name and email for git: the merge says so in plain words before git fails half way,
/// changes nothing, and goes through once the user has saved them (`tools.git_identity`).
#[tokio::test]
async fn a_merge_without_a_git_identity_asks_for_it_then_merges_once_it_is_saved() {
    let w = TestService::start().await;
    let project = w.add_project("shop-web").await;
    let repo = PathBuf::from(&project.repo_path);
    let home = tempfile::tempdir().unwrap();
    let global = home.path().join("gitconfig");
    std::fs::write(&global, "").unwrap();
    // No name or email anywhere git looks, and no guessing them from the machine's name.
    git(&repo, &["config", "--unset", "user.name"]);
    git(&repo, &["config", "--unset", "user.email"]);
    git(&repo, &["config", "user.useConfigOnly", "true"]);
    let mut settings = w.board.project_settings("SHOP").await.unwrap();
    settings.env_vars = throwaway_global(&global)
        .iter()
        .map(|(k, v)| otaq_core::board::settings::EnvVar::new(k, v))
        .collect();
    w.board
        .save_project_settings("SHOP", &settings)
        .await
        .unwrap();
    reviewed(&w, "Checkout API", "api.ts", "api\n").await;
    let main = git(&repo, &["rev-parse", "main"]);

    let outcome = w
        .review
        .local_merge("SHOP-1", "SHOP-1: Checkout API", false, &me())
        .await
        .unwrap();
    assert_eq!(
        outcome,
        LocalMergeOutcome::NeedsIdentity {
            has: otaq_core::git::Identity::default()
        }
    );
    assert_eq!(git(&repo, &["rev-parse", "main"]), main, "nothing merged");
    assert_eq!(w.status("SHOP-1").await, Status::WaitingForYou);

    // The user's one step: their name and email, saved where every repository reads them.
    let theirs = otaq_core::git::Git::find()
        .unwrap()
        .with_env(throwaway_global(&global));
    theirs
        .set_global_identity(" Ada Lovelace ", "ada@example.com")
        .await
        .unwrap();
    let saved = std::fs::read_to_string(&global).unwrap();
    assert!(
        saved.contains("Ada Lovelace") && saved.contains("ada@example.com"),
        "{saved}"
    );

    let outcome = w
        .review
        .local_merge("SHOP-1", "SHOP-1: Checkout API", false, &me())
        .await
        .unwrap();
    assert!(
        matches!(outcome, LocalMergeOutcome::Merged { .. }),
        "{outcome:?}"
    );
    assert_eq!(
        git(&repo, &["log", "-1", "--format=%an <%ae>", "main"]),
        "Ada Lovelace <ada@example.com>"
    );
    w.stop().await;
}

/// Saving the name and email refuses, in plain words, what git couldn't use, and writes nothing then.
#[tokio::test]
async fn saving_a_git_identity_refuses_what_git_cant_use() {
    let home = tempfile::tempdir().unwrap();
    let global = home.path().join("gitconfig");
    std::fs::write(&global, "").unwrap();
    let theirs = otaq_core::git::Git::find()
        .unwrap()
        .with_env(throwaway_global(&global));
    for (name, email, why) in [
        ("", "ada@example.com", "Type your name first."),
        ("Ada", " ", "Type your email first."),
        (
            "Ada",
            "ada",
            "\u{201c}ada\u{201d} doesn\u{2019}t look like an email address.",
        ),
        (
            "Ada\nB",
            "ada@example.com",
            "A name can\u{2019}t contain line breaks or < >.",
        ),
    ] {
        let err = theirs.set_global_identity(name, email).await.unwrap_err();
        assert_eq!(err.to_string(), why);
    }
    assert_eq!(std::fs::read_to_string(&global).unwrap(), "");
}

/// A hook script in the repository's `.git/hooks` (shared by its worktrees and Otaq's scratch worktree).
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

/// A team that forbids rebasing: with main moved on, Approve & merge merges main into the branch (git's plain
/// message, the hooks running on it) and lands one squash commit; the pre-rebase hook never runs.
#[tokio::test]
async fn a_refusing_pre_rebase_hook_never_stops_approve_and_merge() {
    let w = TestService::start().await;
    let project = w.add_project("shop-web").await;
    let repo = PathBuf::from(&project.repo_path);
    let (_, wt) = reviewed(&w, "Checkout API", "api.ts", "api\n").await;
    commit(&repo, "other.ts", "other\n");
    let old_main = git(&repo, &["rev-parse", "main"]);
    let head = git(&wt, &["rev-parse", "HEAD"]);
    let log = w.repos.root().join("hooks.log");
    hook(
        &repo,
        "pre-rebase",
        &format!(
            "echo pre-rebase >> '{}'\necho 'merge instead' >&2\nexit 1",
            log.display()
        ),
    );
    for name in ["pre-merge-commit", "commit-msg"] {
        hook(&repo, name, &format!("echo {name} >> '{}'", log.display()));
    }

    let outcome = w
        .review
        .local_merge("SHOP-1", "SHOP-1: Checkout API", false, &me())
        .await
        .unwrap();
    assert!(
        matches!(outcome, LocalMergeOutcome::Merged { .. }),
        "{outcome:?}"
    );
    assert_eq!(
        git(&repo, &["rev-parse", "main~1"]),
        old_main,
        "one squash commit"
    );
    assert_eq!(
        git(&repo, &["log", "-1", "--format=%B", "main"]),
        "SHOP-1: Checkout API"
    );
    let branch = otaq_core::naming::branch("SHOP-1");
    assert_eq!(git(&repo, &["rev-parse", &format!("{branch}^1")]), head);
    assert_eq!(git(&repo, &["rev-parse", &format!("{branch}^2")]), old_main);
    assert_eq!(
        git(&repo, &["log", "-1", "--format=%s", &branch]),
        format!("Merge branch 'main' into {branch}")
    );
    let ran = std::fs::read_to_string(&log).unwrap();
    assert!(!ran.contains("pre-rebase"), "pre-rebase never ran: {ran}");
    assert!(
        ran.contains("pre-merge-commit"),
        "the merge commit runs the hooks: {ran}"
    );
    no_stash(&repo);
    w.stop().await;
}

/// A hook that refuses the merge commit on the branch is a refusal like any other: nothing merged, main, the branch
/// and the worktree as they were, the refusal on the ticket.
#[tokio::test]
async fn a_hook_refusing_the_merge_of_main_into_the_branch_changes_nothing() {
    let w = TestService::start().await;
    let project = w.add_project("shop-web").await;
    let repo = PathBuf::from(&project.repo_path);
    let (_, wt) = reviewed(&w, "Checkout API", "api.ts", "api\n").await;
    commit(&repo, "other.ts", "other\n");
    let main = git(&repo, &["rev-parse", "main"]);
    let head = git(&wt, &["rev-parse", "HEAD"]);
    hook(
        &repo,
        "commit-msg",
        "grep -q '^SHOP-' \"$1\" || { echo 'messages start with the ticket key' >&2; exit 1; }",
    );

    let outcome = w
        .review
        .local_merge("SHOP-1", "SHOP-1: Checkout API", false, &me())
        .await
        .unwrap();
    let LocalMergeOutcome::HookRefused { hook, step, .. } = outcome else {
        panic!("{outcome:?}");
    };
    assert_eq!(hook, "commit-msg");
    assert_eq!(step, Some(GitStep::UpdateBranch));
    let ticket = w.board.ticket("SHOP-1").await.unwrap();
    let refused = ticket.flags.hook_refusal.unwrap();
    assert!(
        refused
            .output
            .contains("messages start with the ticket key"),
        "{refused:?}"
    );
    assert_eq!(git(&repo, &["rev-parse", "main"]), main, "main unchanged");
    assert_eq!(
        git(&wt, &["rev-parse", "HEAD"]),
        head,
        "the branch unchanged"
    );
    assert_eq!(git(&wt, &["status", "--porcelain"]), "");
    let merge_head = git(&wt, &["rev-parse", "--git-path", "MERGE_HEAD"]);
    assert!(!wt.join(merge_head).exists(), "no merge left in progress");
    assert_eq!(ticket.status, Status::DeliveryStopped);
    no_stash(&repo);
    w.stop().await;
}

/// A hook that refuses Approve & merge's squash commit (the user's click, the dialog since closed): the refusal is on
/// the ticket, not only in a toast. Its Delivery stops with "commit-msg hook refused the merge" (the card's Needs
/// you), the hook's whole output is kept for the sheet, History lists it with that output, and main, the branch and
/// the worktree are as they were. Edited to pass, the merge goes through from there and the refusal goes.
#[tokio::test]
async fn a_hook_refusing_approve_and_merge_shows_on_the_ticket() {
    let w = TestService::start().await;
    let project = w.add_project("shop-web").await;
    let repo = PathBuf::from(&project.repo_path);
    let (_, wt) = reviewed(&w, "Checkout API", "api.ts", "api\n").await;
    let main = git(&repo, &["rev-parse", "main"]);
    let head = git(&wt, &["rev-parse", "HEAD"]);
    let user = user_files(&repo);
    hook(
        &repo,
        "commit-msg",
        "grep -q '^feat: ' \"$1\" && exit 0\n\
         echo 'commitlint: subject may not be empty' >&2\n\
         echo 'commitlint: type must be one of [feat, fix]' >&2\n\
         exit 1",
    );

    let outcome = w
        .review
        .local_merge("SHOP-1", "SHOP-1: Checkout API", false, &me())
        .await
        .unwrap();
    let LocalMergeOutcome::HookRefused {
        hook: name,
        step,
        message,
    } = outcome
    else {
        panic!("{outcome:?}");
    };
    assert_eq!(name, "commit-msg");
    assert_eq!(step, Some(GitStep::Commit));
    assert_eq!(message, "commit-msg hook refused the merge");

    let ticket = w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(ticket.status, Status::DeliveryStopped);
    assert_eq!(ticket.auto_merge_stop, Some(AutoMergeStop::HookRefused));
    assert!(ticket.shows_needs_you(), "Needs you");
    assert_eq!(
        ticket.signal.and_then(|s| s.text).as_deref(),
        Some("commit-msg hook refused the merge")
    );
    let refused = ticket.flags.hook_refusal.expect("kept on the ticket");
    assert_eq!(refused.name, "commit-msg");
    assert_eq!(refused.what, "the merge");
    assert!(
        refused.output.contains(
            "commitlint: subject may not be empty\ncommitlint: type must be one of [feat, fix]"
        ),
        "the hook's whole output: {:?}",
        refused.output
    );
    let events = w.board.history("SHOP-1").await.unwrap();
    let logged = events
        .iter()
        .find(|e| e.kind == EventKind::HookRefused)
        .expect("History lists it");
    assert!(
        logged.payload["hook"]["output"]
            .as_str()
            .is_some_and(|o| o.contains("type must be one of")),
        "{:?}",
        logged.payload
    );
    assert_eq!(git(&repo, &["rev-parse", "main"]), main, "main unchanged");
    assert_eq!(
        git(&wt, &["rev-parse", "HEAD"]),
        head,
        "the branch unchanged"
    );
    assert_eq!(
        git(&wt, &["status", "--porcelain"]),
        "",
        "the worktree clean"
    );
    assert_eq!(user_files(&repo), user, "the user's checkout unchanged");
    no_stash(&repo);

    // Edit the message and retry, from where it stopped.
    let merged = w
        .review
        .local_merge("SHOP-1", "feat: checkout API (SHOP-1)", false, &me())
        .await
        .unwrap();
    assert!(
        matches!(merged, LocalMergeOutcome::Merged { .. }),
        "{merged:?}"
    );
    let ticket = w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(ticket.status, Status::Merged);
    assert_eq!(ticket.flags.hook_refusal, None, "the refusal went");
    w.stop().await;
}

/// A formatter's pre-commit adds a header to each staged `.js` of the squash commit: it lands as the hook left it,
/// the outcome and History say "Your pre-commit hook changed 2 files", and Show changes is the hook's two lines.
#[tokio::test]
async fn a_hooks_changes_to_the_landing_commit_land_and_are_said() {
    let w = TestService::start().await;
    let project = w.add_project("shop-web").await;
    let repo = PathBuf::from(&project.repo_path);
    let (_, wt) = reviewed(&w, "Calc", "calc.js", "add\n").await;
    commit(&wt, "util.js", "util\n");
    hook(
        &repo,
        "pre-commit",
        r#"for f in $(git diff --cached --name-only --diff-filter=ACM -- '*.js'); do
  { echo '// formatted'; cat "$f"; } > "$f.tmp" && mv "$f.tmp" "$f"
  git add "$f"
done"#,
    );

    let outcome = w
        .review
        .local_merge("SHOP-1", "SHOP-1: Calc", false, &me())
        .await
        .unwrap();
    let LocalMergeOutcome::Merged {
        commit: landed,
        hook_changed: Some(changes),
        ..
    } = outcome
    else {
        panic!("expected merged with a hook's changes, got {outcome:?}");
    };
    assert_eq!(changes.sentence(), "Your pre-commit hook changed 2 files");
    assert_eq!(git(&repo, &["show", "main:util.js"]), "// formatted\nutil");
    let history = w.board.history("SHOP-1").await.unwrap();
    let merged = history
        .iter()
        .find(|e| e.kind == EventKind::Merged)
        .unwrap();
    assert_eq!(
        merged.payload["hook_changed"],
        serde_json::to_value(&changes).unwrap()
    );
    // Show changes: each file's difference between what was reviewed and what landed.
    for file in &changes.files {
        let diff = w
            .board
            .diff_file_between("SHOP-1", file, &changes.reviewed, &landed)
            .await
            .unwrap();
        let lines: Vec<_> = diff.hunks.iter().flat_map(|h| &h.lines).collect();
        let added: Vec<&str> = lines
            .iter()
            .filter(|l| l.kind == otaq_core::git::diff::LineKind::Added)
            .map(|l| l.text.as_str())
            .collect();
        assert_eq!(added, ["// formatted"], "{file}");
        assert!(
            !lines
                .iter()
                .any(|l| l.kind == otaq_core::git::diff::LineKind::Removed)
        );
    }
    // Only commit ids are taken.
    assert!(
        w.board
            .diff_file_between("SHOP-1", "calc.js", "--output=x", &landed)
            .await
            .is_err()
    );
    w.stop().await;
}
