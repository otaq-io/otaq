//! Keeping and changing recorded decisions (T-0277; DEC-03, DEC-05, DEC-06; d0084) against temporary repositories:
//! the user's Local merge keeps every open decision with one History line per ticket, Auto-merge keeps nothing,
//! and Change… on a ticket in review sends it back with a message naming the decision. Sessions are an ordinary `sh`; no test starts claude.

use otaq_core::board::Refusal;
use otaq_core::board::comments::{NewComment, Side};
use otaq_core::board::tickets::{EventKind, Ticket};
use otaq_core::client::{Client, ClientConfig};
use otaq_core::decisions::outcome::{self, ChangeOutcome, change_message};
use otaq_core::decisions::{self, NewDecision, Outcome};
use otaq_core::domain::{Actor, Status};
use otaq_core::git::testing::commit;
use otaq_core::naming;
use otaq_core::protocol::{DecisionChangeParams, LocalMergeParams, Reply, Request};
use otaq_core::review::LocalMergeOutcome;
use otaq_core::service::testing::TestService;
use serde_json::json;

fn me() -> Actor {
    Actor::User("me".into())
}

fn split_name() -> NewDecision {
    NewDecision {
        question: "Split fullName everywhere, or only in the profile form?".into(),
        built: "only in the profile form".into(),
        other_options: vec!["everywhere".into()],
        why: "the description only mentions the profile form".into(),
    }
}

fn saved_order() -> NewDecision {
    NewDecision {
        question: "Order of saved items?".into(),
        built: "newest first".into(),
        other_options: vec!["in the order they were added".into()],
        why: "matches the order history page".into(),
    }
}

async fn record(w: &TestService, t: &Ticket, d: &NewDecision) -> decisions::Decision {
    decisions::record(
        &w.board,
        &t.key.to_string(),
        d,
        &Actor::Agent(t.key.clone()),
    )
    .await
    .unwrap()
}

/// One commit on `file` in the ticket's worktree, then Ready for Review.
async fn in_review(w: &TestService, t: &Ticket, file: &str) {
    let wt = w.start_worktree(t).await;
    commit(&wt, file, &format!("{file}\n"));
    w.force_status(t, Status::WaitingForYou).await;
}

fn history_of(
    events: &[otaq_core::board::tickets::HistoryEvent],
    kind: EventKind,
) -> Vec<serde_json::Value> {
    events
        .iter()
        .filter(|e| e.kind == kind)
        .map(|e| e.payload.clone())
        .collect()
}

/// The user's Local merge (the request the app sends) keeps every open decision with one History line; the
/// count drops to 0.
#[tokio::test(flavor = "multi_thread")]
async fn local_merge_keeps_every_open_decision_with_one_history_line() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let t = w.create("SHOP", "Split user full name").await;
    record(&w, &t, &split_name()).await;
    record(&w, &t, &saved_order()).await;
    let plain = w.create("SHOP", "Checkout API").await;
    in_review(&w, &t, "profile.ts").await;
    assert_eq!(outcome::open_count(&w.board, "SHOP-1").await.unwrap(), 2);
    assert_eq!(outcome::open_count(&w.board, "SHOP-2").await.unwrap(), 0);

    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    let reply = client
        .request(Request::TicketLocalMerge(LocalMergeParams {
            key: "SHOP-1".into(),
            message: "SHOP-1: Split user full name".into(),
            approve_anyway: false,
        }))
        .await
        .unwrap();
    assert!(
        matches!(reply, Reply::LocalMerge(LocalMergeOutcome::Merged { .. })),
        "{reply:?}"
    );

    let after = decisions::list(&w.board, "SHOP-1").await.unwrap();
    assert!(
        after.iter().all(|d| d.outcome == Outcome::Kept),
        "{after:?}"
    );
    assert!(after.iter().all(|d| d.outcome_at.is_some()));
    assert_eq!(outcome::open_count(&w.board, "SHOP-1").await.unwrap(), 0);
    let history = w.board.history("SHOP-1").await.unwrap();
    let kept = history_of(&history, EventKind::DecisionsKept);
    assert_eq!(
        kept,
        [json!({
            "count": 2,
            "decisions": [
                {"ordinal": 1, "question": "Split fullName everywhere, or only in the profile form?",
                 "built": "only in the profile form"},
                {"ordinal": 2, "question": "Order of saved items?", "built": "newest first"},
            ],
        })]
    );
    // A ticket without decisions gets no line.
    assert_eq!(
        outcome::keep_all(&w.board, &plain.key.to_string(), &me())
            .await
            .unwrap(),
        0
    );
    assert!(
        history_of(
            &w.board.history("SHOP-2").await.unwrap(),
            EventKind::DecisionsKept
        )
        .is_empty()
    );
    w.stop().await;
}

/// Open PR approves too: once the skill reports the PR (Approved · PR open), the open decisions are kept by the
/// user, not by the agent that reported it.
#[tokio::test(flavor = "multi_thread")]
async fn an_opened_pr_keeps_the_decisions() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let t = w.create("SHOP", "Split user full name").await;
    record(&w, &t, &split_name()).await;
    in_review(&w, &t, "profile.ts").await;

    let after = w
        .prs
        .opened(
            "SHOP-1",
            "https://github.com/acme/shop-web/pull/481",
            &Actor::Agent(t.key.clone()),
        )
        .await
        .unwrap();
    assert_eq!(after.status, Status::PrOpen);
    assert_eq!(
        decisions::get(&w.board, "SHOP-1#1").await.unwrap().outcome,
        Outcome::Kept
    );
    let history = w.board.history("SHOP-1").await.unwrap();
    let kept: Vec<_> = history
        .iter()
        .filter(|e| e.kind == EventKind::DecisionsKept)
        .collect();
    assert_eq!(kept.len(), 1);
    assert!(
        matches!(kept[0].actor, Actor::User(_)),
        "{:?}",
        kept[0].actor
    );
    w.stop().await;
}

/// An automatic Delivery isn't an approval: the merged ticket's decisions stay open, with no History line.
#[tokio::test(flavor = "multi_thread")]
async fn an_automatic_delivery_keeps_nothing() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let api = w.create("SHOP", "Wishlist API").await;
    record(&w, &api, &saved_order()).await;
    in_review(&w, &api, "api.ts").await;

    w.review.deliver_locally("SHOP-1", false).await.unwrap();
    assert_eq!(w.status("SHOP-1").await, Status::Merged);
    let open = decisions::list(&w.board, "SHOP-1").await.unwrap();
    assert_eq!(open[0].outcome, Outcome::Open);
    assert!(
        history_of(
            &w.board.history("SHOP-1").await.unwrap(),
            EventKind::DecisionsKept
        )
        .is_empty()
    );
    assert_eq!(outcome::open_count(&w.board, "SHOP-1").await.unwrap(), 1);
    w.stop().await;
}

/// Change… on SHOP-23 (here SHOP-1): Send back with the message naming the decision and the pending line
/// comments; the decision reads "Changed by you → sent back" and the Sent back marker counts it.
#[tokio::test(flavor = "multi_thread")]
async fn change_on_a_ticket_sends_it_back_with_the_message_and_pending_comments() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let t = w.create("SHOP", "Split user full name").await;
    let decision = record(&w, &t, &split_name()).await;
    in_review(&w, &t, "profile.ts").await;
    w.board
        .add_comment(&NewComment {
            key: "SHOP-1".into(),
            file: "profile.ts".into(),
            line: 1,
            side: Side::New,
            body: "keep the label".into(),
        })
        .await
        .unwrap();
    let agent = w.agent_shell(&t).await;

    // The request the app sends (UserOnly).
    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    let reply = client
        .request(Request::DecisionChange(DecisionChangeParams {
            id: "SHOP-1#1".into(),
            text: Some(" Split fullName everywhere ".into()),
            clear_first: false,
        }))
        .await
        .unwrap();
    let Reply::DecisionChanged(ChangeOutcome::SentBack {
        decision: changed,
        ticket,
    }) = reply
    else {
        panic!("expected sent back, got {reply:?}");
    };
    assert_eq!(ticket.status, Status::Working);
    let sent = ticket.flags.sent_back.unwrap();
    assert_eq!((sent.comments, sent.round, sent.decisions), (1, 1, 1));
    w.wait_for_output(
        &agent.session,
        &format!("/{}", naming::copy_name("address-review")),
    )
    .await;

    let message = "Review feedback on a recorded decision: \u{201c}Split fullName everywhere, or only in the \
                   profile form?\u{201d} You built: only in the profile form. Change it to: Split fullName \
                   everywhere.";
    assert_eq!(
        change_message(&decision, "Split fullName everywhere"),
        message
    );
    // The change is the user's comment (CMT-05).
    assert_eq!(
        w.board.latest_send_back_round(&t.id).await.unwrap(),
        Some(1)
    );
    assert_eq!(
        crate::common::last_comment(&w.board, "SHOP-1")
            .await
            .map(|c| c.body),
        Some(message.to_owned())
    );
    assert_eq!(w.board.comments("SHOP-1", None).await.unwrap().pending, 0);

    assert_eq!(changed.outcome, Outcome::ChangedSentBack);
    assert_eq!(
        changed.outcome_text.as_deref(),
        Some("Split fullName everywhere")
    );
    let history = w.board.history("SHOP-1").await.unwrap();
    assert_eq!(
        history_of(&history, EventKind::DecisionChanged),
        [json!({
            "ordinal": 1,
            "question": "Split fullName everywhere, or only in the profile form?",
            "to": "sent_back",
            "text": "Split fullName everywhere",
            "round": 1,
        })]
    );

    // Already changed: refused; and the count is 0.
    let err = outcome::change_and_send_back(&w.review, "SHOP-1#1", "x", false, &me())
        .await
        .unwrap_err();
    assert_eq!(err.refusal(), Some(Refusal::MoveRefused));
    assert_eq!(outcome::open_count(&w.board, "SHOP-1").await.unwrap(), 0);
    w.stop().await;
}

/// Unsent text in the agent's input line asks first: nothing changes until "Clear it and send".
#[tokio::test(flavor = "multi_thread")]
async fn change_asks_first_over_unsent_text() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let t = w.create("SHOP", "Split user full name").await;
    record(&w, &t, &split_name()).await;
    in_review(&w, &t, "profile.ts").await;
    let agent = w.agent_shell(&t).await;
    w.sessions.input(&agent.session, b"my draft").unwrap();

    let first = outcome::change_and_send_back(&w.review, "SHOP-1#1", "everywhere", false, &me())
        .await
        .unwrap();
    assert_eq!(
        first,
        ChangeOutcome::NeedsConfirmation {
            ticket: t.key.clone(),
            unclearable_by: None,
        }
    );
    assert_eq!(w.status("SHOP-1").await, Status::WaitingForYou);
    assert_eq!(
        decisions::get(&w.board, "SHOP-1#1").await.unwrap().outcome,
        Outcome::Open
    );

    let second = outcome::change_and_send_back(&w.review, "SHOP-1#1", "everywhere", true, &me())
        .await
        .unwrap();
    assert!(
        matches!(second, ChangeOutcome::SentBack { .. }),
        "{second:?}"
    );
    w.stop().await;
}

/// The refusals: a ticket not in Ready for Review, an empty text, and a Change… with no text.
#[tokio::test(flavor = "multi_thread")]
async fn the_refusals() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let t = w.create("SHOP", "Split user full name").await;
    record(&w, &t, &split_name()).await;

    let err = outcome::change_and_send_back(&w.review, "SHOP-1#1", "everywhere", false, &me())
        .await
        .unwrap_err();
    assert_eq!(err.refusal(), Some(Refusal::MoveRefused));
    assert!(err.to_string().contains("Ready for Review"), "{err}");

    in_review(&w, &t, "profile.ts").await;
    let err = outcome::change_and_send_back(&w.review, "SHOP-1#1", "  ", false, &me())
        .await
        .unwrap_err();
    assert_eq!(err.refusal(), Some(Refusal::EmptyField));

    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    let err = client
        .request(Request::DecisionChange(DecisionChangeParams {
            id: "SHOP-1#1".into(),
            text: None,
            clear_first: false,
        }))
        .await
        .unwrap_err();
    assert!(err.to_string().contains("needs the text"), "{err}");
    assert_eq!(
        decisions::get(&w.board, "SHOP-1#1").await.unwrap().outcome,
        Outcome::Open
    );
    assert_eq!(w.status("SHOP-1").await, Status::WaitingForYou);
    w.stop().await;
}
