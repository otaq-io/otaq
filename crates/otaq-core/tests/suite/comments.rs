//! Review comments and rounds (T-0085).

use crate::common::Fixture;
use otaq_core::board::Refusal;
use otaq_core::board::comments::{NewComment, Side};
use otaq_core::domain::Status;

fn comment(line: u32, side: Side, body: &str) -> NewComment {
    NewComment {
        key: "SHOP-1".into(),
        file: "lib/pricing.ts".into(),
        line,
        side,
        body: body.into(),
    }
}

/// What Send back does to the comments (T-0086 does it for real): marks the pending ones sent and starts the
/// next round.
async fn send_round(f: &Fixture) {
    sqlx::query(
        "UPDATE review_comments SET sent_at = 1 WHERE sent_at IS NULL;
         UPDATE tickets SET sent_back_round = sent_back_round + 1;",
    )
    .execute(f.board.store().pool())
    .await
    .unwrap();
}

async fn in_review(f: &Fixture) {
    let t = f.create("Checkout API").await;
    f.force_status(&t, Status::WaitingForYou).await;
}

#[tokio::test]
async fn add_edit_delete_and_threads() {
    let f = Fixture::shop().await;
    in_review(&f).await;
    let first = f
        .board
        .add_comment(&comment(12, Side::New, "Round half-up here"))
        .await
        .unwrap();
    assert_eq!(first.round, 1);
    assert_eq!(
        (first.file.as_str(), first.line, first.side),
        ("lib/pricing.ts", 12, Side::New)
    );
    assert!(first.is_pending());
    let reply = f
        .board
        .reply_to_comment(&first.id, "and add a test")
        .await
        .unwrap();
    assert!(reply.same_anchor(&first));
    let other = f
        .board
        .add_comment(&comment(4, Side::Old, "Why remove this?"))
        .await
        .unwrap();
    assert!(!other.same_anchor(&first));

    let edited = f
        .board
        .edit_comment(&first.id, "Round half-even")
        .await
        .unwrap();
    assert_eq!(edited.body, "Round half-even");
    let listed = f.board.comments("SHOP-1", None).await.unwrap();
    assert_eq!(listed.pending, 3);
    assert_eq!(listed.round, 1);
    let thread: Vec<_> = listed
        .comments
        .iter()
        .filter(|c| c.same_anchor(&first))
        .map(|c| c.body.as_str())
        .collect();
    assert_eq!(thread, ["Round half-even", "and add a test"]);

    f.board.delete_comment(&other.id).await.unwrap();
    assert_eq!(f.board.comments("SHOP-1", None).await.unwrap().pending, 2);
}

#[tokio::test]
async fn rounds_follow_send_backs_and_sent_comments_are_read_only() {
    let f = Fixture::shop().await;
    in_review(&f).await;
    let first = f
        .board
        .add_comment(&comment(1, Side::New, "one"))
        .await
        .unwrap();
    send_round(&f).await;

    let listed = f.board.comments("SHOP-1", None).await.unwrap();
    assert_eq!(listed.round, 2);
    assert_eq!(listed.pending, 0);
    let row = f
        .board
        .store()
        .ticket(&f.ticket("SHOP-1").await.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.sent_back_round, 1, "the round sent matches the ticket");

    let edit = f
        .board
        .edit_comment(&first.id, "changed")
        .await
        .unwrap_err();
    assert_eq!(edit.refusal(), Some(Refusal::CommentSent));
    let delete = f.board.delete_comment(&first.id).await.unwrap_err();
    assert_eq!(delete.refusal(), Some(Refusal::CommentSent));

    let second = f
        .board
        .add_comment(&comment(2, Side::New, "two"))
        .await
        .unwrap();
    assert_eq!(second.round, 2);
    let round_one = f.board.comments("SHOP-1", Some(1)).await.unwrap();
    assert_eq!(round_one.comments.len(), 1);
    assert_eq!(round_one.comments[0].body, "one");
    assert_eq!(f.board.comments("SHOP-1", None).await.unwrap().pending, 1);
}

#[tokio::test]
async fn comments_need_a_ticket_in_review_and_some_text() {
    let f = Fixture::shop().await;
    let t = f.create("Checkout API").await;
    f.force_status(&t, Status::Working).await;
    let err = f
        .board
        .add_comment(&comment(1, Side::New, "x"))
        .await
        .unwrap_err();
    assert_eq!(err.refusal(), Some(Refusal::MoveRefused));
    f.force_status(&t, Status::WaitingForYou).await;
    let err = f
        .board
        .add_comment(&comment(1, Side::New, "  "))
        .await
        .unwrap_err();
    assert_eq!(err.refusal(), Some(Refusal::EmptyField));
}

/// Any human check of a custom flow takes comments, not only the default flow's Review (FTB-05): the test flow's QA.
#[tokio::test]
async fn a_custom_flows_human_check_takes_comments() {
    let f = Fixture::shop().await;
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/flows/every-kind.yaml");
    let flow = otaq_core::flow::yaml::read(&std::fs::read_to_string(path).unwrap()).unwrap();
    f.board.use_flow_for_tests("SHOP", &flow).await;
    let t = f.create("Checkout API").await;
    sqlx::query("UPDATE tickets SET stage_id = 'qa', state = ? WHERE id = ?")
        .bind(Status::WaitingForYou.as_str())
        .bind(&t.id)
        .execute(f.board.store().pool())
        .await
        .unwrap();
    let c = f
        .board
        .add_comment(&comment(7, Side::New, "Say why it's rejected"))
        .await
        .unwrap();
    assert!(c.is_pending());
    assert_eq!(f.board.comments("SHOP-1", None).await.unwrap().pending, 1);
}
