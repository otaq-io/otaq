//! Merge into another ticket (the Approve menu's "Merge into another ticket…"), against temporary repositories: the
//! ticket's branch is squashed into its host's branch as one commit, the ticket waits as Merged into the host with its
//! branch kept, and the host goes back to the human check before its Delivery.

use std::path::{Path, PathBuf};

use otaq_core::board::tickets::{EventKind, Ticket};
use otaq_core::domain::{Actor, Place, Status};
use otaq_core::flow::default::REVIEW;
use otaq_core::git::testing::{commit, git};
use otaq_core::review::merge_into::MergeIntoOutcome;
use otaq_core::service::testing::TestService;

fn me() -> Actor {
    Actor::User("me".into())
}

/// Creates tickets until the next one is SHOP-`number`.
async fn fillers_until(w: &TestService, number: u32) {
    let next = w.board.tickets("SHOP").await.unwrap().len() as u32 + 1;
    for _ in next..number {
        w.create("SHOP", "filler").await;
    }
}

/// A ticket in Ready for Review with a commit on each of `files` in its worktree.
async fn reviewed(w: &TestService, title: &str, files: &[(&str, &str)]) -> (Ticket, PathBuf) {
    let t = w.create("SHOP", title).await;
    let wt = w.start_worktree(&t).await;
    for (file, content) in files {
        commit(&wt, file, content);
    }
    w.force_status(&t, Status::WaitingForYou).await;
    (w.board.ticket(&t.key.to_string()).await.unwrap(), wt)
}

fn branch(t: &Ticket) -> String {
    t.work.as_ref().and_then(|w| w.branch.clone()).unwrap()
}

fn head(repo: &Path, branch: &str) -> String {
    git(repo, &["rev-parse", &format!("refs/heads/{branch}")])
}

#[tokio::test]
async fn merging_shop20_into_shop18_squashes_it_onto_shop18s_branch() {
    let w = TestService::start().await;
    let project = w.add_project("shop-web").await;
    let repo = PathBuf::from(&project.repo_path);
    fillers_until(&w, 18).await;
    let (shop18, wt18) = reviewed(&w, "Checkout API", &[("api.ts", "api\n")]).await;
    w.create("SHOP", "filler").await;
    let (shop20, wt20) = reviewed(
        &w,
        "Coupon codes",
        &[("coupon.ts", "coupon\n"), ("coupon.ts", "coupon v2\n")],
    )
    .await;
    assert_eq!(shop20.key.to_string(), "SHOP-20");
    // SHOP-21 waits on SHOP-20.
    let shop21 = w.create("SHOP", "Coupon page").await;
    w.board
        .add_dependency("SHOP-21", "SHOP-20", &me())
        .await
        .unwrap();
    w.force_status(&shop21, Status::Queued).await;

    let hosts = w.board.merge_into_hosts("SHOP-20").await.unwrap();
    let keys: Vec<String> = hosts.iter().map(|t| t.key.to_string()).collect();
    assert_eq!(keys, ["SHOP-18"], "only the other ticket in review");

    let before = head(&repo, &branch(&shop18));
    let outcome = w
        .review
        .merge_into("SHOP-20", "SHOP-18", &me())
        .await
        .unwrap();
    let MergeIntoOutcome::Merged {
        ticket,
        into,
        commit,
    } = outcome
    else {
        panic!("expected merged, got {outcome:?}");
    };

    // SHOP-18's branch has one new squash commit with SHOP-20's changes, named after SHOP-20.
    let after = head(&repo, &branch(&shop18));
    assert_eq!(after, commit);
    assert_eq!(
        git(
            &repo,
            &["rev-list", "--count", &format!("{before}..{after}")]
        ),
        "1"
    );
    assert_eq!(
        git(&repo, &["log", "-1", "--format=%B", &after]),
        "SHOP-20: Coupon codes\n\n- edit coupon.ts\n- edit coupon.ts"
    );
    assert_eq!(
        git(&repo, &["diff", "--name-only", &before, &after]),
        "coupon.ts"
    );
    assert_eq!(
        std::fs::read_to_string(wt18.join("coupon.ts")).unwrap(),
        "coupon v2\n"
    );
    assert_eq!(git(&wt18, &["status", "--porcelain"]), "");

    // SHOP-20 waits as Merged into SHOP-18: its worktree is gone, its branch kept.
    assert_eq!(ticket.status, Status::MergedInto);
    assert_eq!(ticket.flags.merged_into, Some(shop18.key.clone()));
    assert!(!ticket.place.is_done(), "not Done");
    assert!(!wt20.exists(), "worktree removed");
    assert!(
        ticket
            .work
            .as_ref()
            .and_then(|w| w.worktree.as_ref())
            .is_none(),
        "no worktree recorded"
    );
    git(
        &repo,
        &[
            "rev-parse",
            "--verify",
            &format!("refs/heads/{}", branch(&shop20)),
        ],
    );
    let link = w.board.merged_into("SHOP-20").await.unwrap().unwrap();
    assert_eq!(link.into, shop18.key);
    assert_eq!(link.commit, commit);
    let merged_in: Vec<String> = w
        .board
        .merged_in("SHOP-18")
        .await
        .unwrap()
        .into_iter()
        .map(|m| m.key.to_string())
        .collect();
    assert_eq!(merged_in, ["SHOP-20"]);
    let history = w.board.history("SHOP-20").await.unwrap();
    assert!(history.iter().any(|e| e.kind == EventKind::MergedInto));

    // SHOP-18 is back in review with the combined diff and a History line naming SHOP-20.
    assert_eq!(into.status, Status::WaitingForYou);
    assert_eq!(into.place, Place::stage_of(REVIEW));
    let diff = w.board.diff_summary("SHOP-18").await.unwrap();
    let mut paths: Vec<_> = diff.files.iter().map(|f| f.path.as_str()).collect();
    paths.sort_unstable();
    assert_eq!(paths, ["api.ts", "coupon.ts"]);
    let history = w.board.history("SHOP-18").await.unwrap();
    let line = history
        .iter()
        .find(|e| e.kind == EventKind::MergedIn)
        .expect("merged in");
    assert_eq!(line.payload["ticket"], "SHOP-20");

    // It isn't Done: SHOP-21 keeps waiting for it.
    assert_eq!(w.status("SHOP-21").await, Status::Queued);
    let shop21 = w.board.ticket("SHOP-21").await.unwrap();
    assert!(!otaq_core::board::readiness::ready_to_start(
        &shop21,
        &w.board.tickets("SHOP").await.unwrap()
    ));
    w.stop().await;
}

#[tokio::test]
async fn a_host_with_an_open_pr_goes_back_to_review_and_keeps_its_pr() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let (host, _) = reviewed(&w, "Checkout API", &[("api.ts", "api\n")]).await;
    w.force_status(&host, Status::PrOpen).await;
    sqlx::query("UPDATE tickets SET pr_number = 481 WHERE id = ?")
        .bind(&host.id)
        .execute(w.board.store().pool())
        .await
        .unwrap();
    reviewed(&w, "Coupon codes", &[("coupon.ts", "coupon\n")]).await;

    let outcome = w
        .review
        .merge_into("SHOP-2", "SHOP-1", &me())
        .await
        .unwrap();
    let MergeIntoOutcome::Merged { into, .. } = outcome else {
        panic!("expected merged, got {outcome:?}");
    };
    assert_eq!(into.status, Status::WaitingForYou);
    assert_eq!(into.place, Place::stage_of(REVIEW));
    assert_eq!(into.flags.pr_number, Some(481), "Push to PR carries it");
    w.stop().await;
}

#[tokio::test]
async fn a_conflicting_pair_merges_nothing() {
    let w = TestService::start().await;
    let project = w.add_project("shop-web").await;
    let repo = PathBuf::from(&project.repo_path);
    let (host, host_wt) = reviewed(&w, "Pricing", &[("lib.ts", "host\n")]).await;
    let (ticket, wt) = reviewed(&w, "Discounts", &[("lib.ts", "ticket\n")]).await;
    let host_head = head(&repo, &branch(&host));
    let ticket_head = head(&repo, &branch(&ticket));

    let outcome = w
        .review
        .merge_into("SHOP-2", "SHOP-1", &me())
        .await
        .unwrap();
    assert_eq!(
        outcome,
        MergeIntoOutcome::Conflict {
            into: host.key.clone(),
            files: vec!["lib.ts".into()],
        }
    );
    assert_eq!(head(&repo, &branch(&host)), host_head, "nothing merged");
    assert_eq!(head(&repo, &branch(&ticket)), ticket_head, "put back");
    assert_eq!(git(&wt, &["status", "--porcelain"]), "");
    assert_eq!(git(&host_wt, &["status", "--porcelain"]), "");
    assert_eq!(w.status("SHOP-1").await, Status::WaitingForYou);
    assert_eq!(w.status("SHOP-2").await, Status::WaitingForYou);
    assert!(w.board.merged_into("SHOP-2").await.unwrap().is_none());
    assert!(w.board.merged_in("SHOP-1").await.unwrap().is_empty());
    w.stop().await;
}

#[tokio::test]
async fn uncommitted_files_in_either_worktree_are_refused() {
    let w = TestService::start().await;
    let project = w.add_project("shop-web").await;
    let repo = PathBuf::from(&project.repo_path);
    let (host, host_wt) = reviewed(&w, "Pricing", &[("price.ts", "p\n")]).await;
    reviewed(&w, "Discounts", &[("discount.ts", "d\n")]).await;
    std::fs::write(host_wt.join("scratch.txt"), "x\n").unwrap();
    let host_head = head(&repo, &branch(&host));

    let outcome = w
        .review
        .merge_into("SHOP-2", "SHOP-1", &me())
        .await
        .unwrap();
    assert_eq!(
        outcome,
        MergeIntoOutcome::RefusedUncommitted {
            files: vec!["scratch.txt".into()],
            message: "SHOP-1 has 1 uncommitted file".into(),
        }
    );
    assert_eq!(head(&repo, &branch(&host)), host_head);
    assert_eq!(w.status("SHOP-2").await, Status::WaitingForYou);
    w.stop().await;
}

#[tokio::test]
async fn the_picker_lists_only_tickets_waiting_with_a_branch_and_no_agent_at_work() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    reviewed(&w, "Discounts", &[("discount.ts", "d\n")]).await; // SHOP-1: the one merged in
    let (working, _) = reviewed(&w, "Working", &[("a.ts", "a\n")]).await;
    w.force_status(&working, Status::Working).await;
    let (merged, _) = reviewed(&w, "Merged", &[("b.ts", "b\n")]).await;
    w.force_status(&merged, Status::Merged).await;
    w.create("SHOP", "No branch").await; // SHOP-4, Backlog
    let (delivering, _) = reviewed(&w, "To deliver", &[("c.ts", "c\n")]).await; // SHOP-5
    w.force_status(&delivering, Status::ToDeliver).await;
    reviewed(&w, "In review", &[("d.ts", "d\n")]).await; // SHOP-6
    let (pr, _) = reviewed(&w, "PR open", &[("e.ts", "e\n")]).await; // SHOP-7
    w.force_status(&pr, Status::PrOpen).await;

    let keys: Vec<String> = w
        .board
        .merge_into_hosts("SHOP-1")
        .await
        .unwrap()
        .into_iter()
        .map(|t| t.key.to_string())
        .collect();
    assert_eq!(keys, ["SHOP-5", "SHOP-6", "SHOP-7"]);
    // A host the picker doesn't list is refused.
    let refused = w.review.merge_into("SHOP-1", "SHOP-2", &me()).await;
    assert!(refused.is_err(), "a working ticket can't take it");
    w.stop().await;
}

#[tokio::test]
async fn a_hosts_pull_request_context_lists_the_tickets_merged_into_it() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    fillers_until(&w, 18).await;
    let (host, _) = reviewed(&w, "Checkout API", &[("api.ts", "api\n")]).await;
    w.force_status(&host, Status::PrOpen).await;
    sqlx::query("UPDATE tickets SET pr_number = 481 WHERE id = ?")
        .bind(&host.id)
        .execute(w.board.store().pool())
        .await
        .unwrap();
    w.create("SHOP", "filler").await;
    let (shop20, _) = reviewed(&w, "Coupon codes", &[("coupon.ts", "coupon\n")]).await;
    w.board
        .update_ticket(
            "SHOP-20",
            &otaq_core::board::tickets::TicketUpdate {
                body: Some("Customers enter a coupon code at checkout.".into()),
                ..Default::default()
            },
            &me(),
        )
        .await
        .unwrap();

    // Before anything is merged in, the Pull request section says nothing extra.
    let prime = w
        .board
        .prime_context("SHOP-18", w.adapters.for_new_session())
        .await
        .unwrap();
    let pr = prime.pull_request.as_ref().expect("a PR section");
    assert!(pr.merged_in.is_empty());
    assert!(!otaq_core::prime::render(&prime).contains("merged into this one"));

    w.review
        .merge_into(&shop20.key.to_string(), "SHOP-18", &me())
        .await
        .unwrap();
    let prime = w
        .board
        .prime_context("SHOP-18", w.adapters.for_new_session())
        .await
        .unwrap();
    let pr = prime.pull_request.as_ref().expect("a PR section");
    assert_eq!(pr.merged_in.len(), 1);
    assert_eq!(pr.merged_in[0].key.to_string(), "SHOP-20");
    assert_eq!(pr.merged_in[0].title, "Coupon codes");
    assert_eq!(
        pr.merged_in[0].body,
        "Customers enter a coupon code at checkout."
    );
    let text = otaq_core::prime::render(&prime);
    let section = &text[text.find("## Pull request").unwrap()..];
    assert!(
        section.contains(
            "merged into this one (the PR's description lists each of them, with its key and title"
        ),
        "{section}"
    );
    assert!(
        section.contains(
            "- SHOP-20 \u{b7} Coupon codes\n  > Customers enter a coupon code at checkout.\n"
        ),
        "{section}"
    );
    w.stop().await;
}
