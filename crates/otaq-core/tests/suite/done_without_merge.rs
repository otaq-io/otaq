//! "Done, nothing to merge": a ticket whose branch has no commits beyond its base finishes from the Approve menu. It
//! ends Merged with the outcome that counts as merged but isn't a merge (no merge commit), its worktree goes and its
//! branch stays, and a ticket waiting on it starts.

use std::path::Path;

use otaq_core::board::tickets::EventKind;
use otaq_core::domain::{Actor, Outcome, Status};
use otaq_core::git::testing::{commit, git};
use otaq_core::service::testing::TestService;

fn me() -> Actor {
    Actor::User("me".into())
}

#[tokio::test]
async fn an_empty_branch_ends_done_counting_as_merged_and_its_dependent_starts() {
    let w = TestService::start().await;
    let project = w.add_project("shop-web").await;
    let repo = Path::new(&project.repo_path);
    let t = w.create("SHOP", "Configure the CI secret").await;
    let wt = w.start_worktree(&t).await;
    w.force_status(&t, Status::WaitingForYou).await;
    let waiting = w.create("SHOP", "Use the CI secret").await;
    w.board
        .add_dependency("SHOP-2", "SHOP-1", &me())
        .await
        .unwrap();
    w.force_status(&waiting, Status::Queued).await;
    let main_before = git(repo, &["rev-parse", "main"]);

    let proposal = w.board.merge_proposal("SHOP-1").await.unwrap();
    assert_eq!(proposal.commits, 0, "the diff decides: nothing to merge");

    let done = w.review.done("SHOP-1", &me()).await.unwrap();
    assert_eq!(done.ticket.status, Status::Merged);
    assert!(done.ticket.flags.done_without_merge);
    assert_eq!(done.ticket.flags.nothing_to_merge.as_deref(), Some(""));
    assert_eq!(
        done.started,
        ["SHOP-2".parse().unwrap()],
        "its dependent started"
    );

    let row = w.board.store().ticket(&t.id).await.unwrap().unwrap();
    assert_eq!(
        row.outcome.as_deref(),
        Some(Outcome::DoneWithoutMerge.as_str())
    );
    assert_ne!(row.outcome.as_deref(), Some(Outcome::Merged.as_str()));
    assert_eq!(row.merge_commit_sha, None, "no merge commit");
    assert_eq!(
        git(repo, &["rev-parse", "main"]),
        main_before,
        "nothing landed"
    );
    assert!(!wt.exists(), "worktree removed");
    assert_eq!(row.worktree_path, None);
    let branch = otaq_core::naming::branch("SHOP-1");
    assert_eq!(
        git(repo, &["branch", "--list", &branch]).trim_start_matches(['*', '+', ' ']),
        branch,
        "branch kept"
    );
    let action: String = sqlx::query_scalar("SELECT action FROM reviews WHERE ticket_id = ?")
        .bind(&t.id)
        .fetch_one(w.board.store().pool())
        .await
        .unwrap();
    assert_eq!(action, "done_without_merge");
    let history = w.board.history("SHOP-1").await.unwrap();
    assert!(
        history
            .iter()
            .any(|e| e.kind == EventKind::DoneWithoutMerge)
    );
    // Readiness needs nothing new: a Merged parent is a merged parent.
    let tickets = w.board.tickets("SHOP").await.unwrap();
    let shop2 = tickets
        .iter()
        .find(|t| t.key.to_string() == "SHOP-2")
        .unwrap();
    assert!(otaq_core::board::readiness::readiness(shop2, &tickets).is_ready());
    w.stop().await;
}

#[tokio::test]
async fn a_branch_with_commits_is_refused_and_stays() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let t = w.create("SHOP", "Checkout API").await;
    let wt = w.start_worktree(&t).await;
    commit(&wt, "api.ts", "api\n");
    commit(&wt, "api.ts", "api v2\n");
    w.force_status(&t, Status::WaitingForYou).await;

    assert_eq!(w.board.merge_proposal("SHOP-1").await.unwrap().commits, 2);
    let refused = w.review.done("SHOP-1", &me()).await.unwrap_err();
    assert!(
        refused.to_string().contains("SHOP-1 has 2 commits"),
        "{refused}"
    );
    assert_eq!(
        w.board.ticket("SHOP-1").await.unwrap().status,
        Status::WaitingForYou
    );
    assert!(wt.exists());
    w.stop().await;
}
