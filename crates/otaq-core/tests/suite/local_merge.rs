//! Local merge in a scratch worktree (T-0089, GIT-01, GIT-03, REV-08, EPC-03), against temporary repositories.
//! The repository's own folder stands for the user's checkout.

use std::path::{Path, PathBuf};

use otaq_core::board::projects::AddProject;
use otaq_core::board::tickets::{EventKind, NewTicketInput, Ticket};
use otaq_core::domain::{Actor, Kind, RemoteWarning, Status, WentOn};
use otaq_core::flow::Delivers;
use otaq_core::flow::default::testing::delivering;
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
    // main moved on after the ticket left it: the free rebase takes care of it.
    commit(&repo, "other.ts", "other\n");
    let old_main = git(&repo, &["rev-parse", "main"]);

    let proposal = w.board.merge_proposal("SHOP-1").await.unwrap();
    assert_eq!(proposal.target, "main");
    assert!(!proposal.into_epic);
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
    } = outcome
    else {
        panic!("expected merged, got {outcome:?}");
    };
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
        "rebased onto it"
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
async fn a_rebase_conflict_merges_nothing() {
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

#[tokio::test]
async fn an_epic_child_merges_into_the_epic_branch() {
    let w = TestService::start().await;
    let project = w.add_project("shop-web").await;
    let repo = PathBuf::from(&project.repo_path);
    let epic = w
        .board
        .create_ticket(
            &NewTicketInput {
                project: "SHOP".into(),
                title: "Wishlist".into(),
                kind: Kind::Epic,
                ..Default::default()
            },
            &me(),
        )
        .await
        .unwrap();
    let child = w
        .board
        .create_ticket(
            &NewTicketInput {
                project: "SHOP".into(),
                title: "Wishlist API".into(),
                epic: Some(epic.key.to_string()),
                ..Default::default()
            },
            &me(),
        )
        .await
        .unwrap();
    let wt = w.start_worktree(&child).await;
    commit(&wt, "wishlist.ts", "w\n");
    w.force_status(&child, Status::WaitingForYou).await;
    let main = git(&repo, &["rev-parse", "main"]);

    let proposal = w.board.merge_proposal("SHOP-1.1").await.unwrap();
    assert_eq!(proposal.target, otaq_core::naming::branch("SHOP-1"));
    assert!(proposal.into_epic);
    let outcome = w
        .review
        .local_merge("SHOP-1.1", "SHOP-1.1: Wishlist API", false, &me())
        .await
        .unwrap();
    let LocalMergeOutcome::Merged { target, .. } = outcome else {
        panic!("expected merged, got {outcome:?}");
    };
    assert_eq!(target, proposal.target);
    assert_eq!(git(&repo, &["rev-parse", "main"]), main, "main untouched");
    assert_eq!(
        git(&repo, &["log", "-1", "--format=%s", &target]),
        "SHOP-1.1: Wishlist API"
    );
    assert_eq!(w.status("SHOP-1.1").await, Status::Merged);
    w.stop().await;
}
