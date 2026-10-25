//! The card's "N decisions" (DEC-03, d0084): `Flags.open_decisions` counts the open recorded decisions of a ticket
//! in Ready for Review, and is zero in every other column and once approved.

use otaq_core::board::tickets::Ticket;
use otaq_core::decisions::{self, NewDecision, outcome};
use otaq_core::domain::{Actor, Status};
use otaq_core::git::testing::commit;
use otaq_core::service::testing::TestService;

fn me() -> Actor {
    Actor::User("me".into())
}

fn decision(question: &str) -> NewDecision {
    NewDecision {
        question: question.into(),
        built: "only in the profile form".into(),
        other_options: vec!["everywhere".into()],
        why: "the description only mentions the profile form".into(),
    }
}

async fn record(w: &TestService, t: &Ticket, question: &str) {
    decisions::record(
        &w.board,
        &t.key.to_string(),
        &decision(question),
        &Actor::Agent(t.key.clone()),
    )
    .await
    .unwrap();
}

async fn count(w: &TestService, key: &str) -> u32 {
    w.board.ticket(key).await.unwrap().flags.open_decisions
}

#[tokio::test(flavor = "multi_thread")]
async fn the_count_shows_only_in_ready_for_review() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let t = w.create("SHOP", "Split user full name").await;
    record(
        &w,
        &t,
        "Split fullName everywhere, or only in the profile form?",
    )
    .await;
    assert_eq!(count(&w, "SHOP-1").await, 0, "Backlog: nothing on the card");

    let wt = w.start_worktree(&t).await;
    commit(&wt, "profile.ts", "profile\n");
    w.force_status(&t, Status::Working).await;
    record(&w, &t, "Keep the old column?").await;
    assert_eq!(
        count(&w, "SHOP-1").await,
        0,
        "working: the Ticket tab has them"
    );

    w.force_status(&t, Status::WaitingForYou).await;
    assert_eq!(count(&w, "SHOP-1").await, 2);

    // Approve keeps them: nothing after approval.
    outcome::keep_all(&w.board, "SHOP-1", &me()).await.unwrap();
    assert_eq!(count(&w, "SHOP-1").await, 0);
    w.stop().await;
}
