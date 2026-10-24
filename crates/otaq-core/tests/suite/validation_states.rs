//! The M12 states and flags (T-0182; SDD §7.2, §12.2, §13; d0063, d0064, d0042, d0016): needs-you ones group like
//! the needs-you family, Validating holds one slot and makes no needs-you item, and the ticket columns become the
//! card's flags.

use std::collections::HashSet;

use crate::common::Fixture;
use otaq_core::board::needs_you::{NeedsYouGroup, group_of};
use otaq_core::board::ticket_comments::{CommentAuthor, post_in};
use otaq_core::domain::{ReviewEntry, Status, StatusSet, ValidationRound};
use otaq_core::scheduler::{HOLDS_A_SLOT, running_count};

const M12_DEV: [Status; 6] = [
    Status::Checking,
    Status::CheckerNeedsYou,
    Status::CheckFailed,
    Status::CheckNoResult,
    Status::MergeConflictFixing,
    Status::DeliveryStopped,
];

#[test]
fn needs_you_states_are_needs_you_group_one_and_validating_makes_no_item() {
    for status in M12_DEV {
        let expected = status
            .is_needs_you_family()
            .then_some(NeedsYouGroup::Family);
        assert_eq!(group_of(status), expected, "{status}");
    }
    assert_eq!(group_of(Status::Checking), None);
    assert_eq!(group_of(Status::MergeConflictFixing), None);
}

#[test]
fn every_m12_dev_state_holds_one_slot_per_ticket() {
    for status in M12_DEV {
        assert!(HOLDS_A_SLOT.contains(&status), "{status}");
    }
    assert_eq!(
        HOLDS_A_SLOT
            .iter()
            .filter(|s| **s == Status::Checking)
            .count(),
        1
    );
}

#[tokio::test]
async fn a_validating_ticket_counts_once_toward_the_limit() {
    let f = Fixture::shop().await;
    let t = f.create("Checkout").await;
    set(
        &f,
        &t.id,
        "UPDATE tickets SET state = 'validating', stage_id = 'validate' WHERE id = ?",
    )
    .await;
    let tickets = f.board.tickets("SHOP").await.unwrap();
    assert_eq!(running_count(&tickets, &HashSet::new()), 1);
}

/// Posts a comment on the ticket `id` the way a hand-over does (`post_in`).
async fn post(f: &Fixture, id: &str, author: &CommentAuthor, body: &str) {
    let mut tx = f.board.store().pool().begin().await.unwrap();
    post_in(&mut tx, id, author, body.trim(), None, &crate::common::me())
        .await
        .unwrap();
    tx.commit().await.unwrap();
}

async fn set(f: &Fixture, id: &str, sql: &str) {
    sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .execute(f.board.store().pool())
        .await
        .unwrap();
}

/// Validate's own counter (`check_counters`, T-0417): its fails in a row, its extra rounds and the round it passed in.
async fn validate_counter(f: &Fixture, id: &str, fails: i64, extra: i64, passed: Option<i64>) {
    sqlx::query(
        "INSERT OR REPLACE INTO check_counters (ticket_id, stage_id, fails, extra, passed_round, updated_at)
         VALUES (?, 'validate', ?, ?, ?, 1)",
    )
    .bind(id)
    .bind(fails)
    .bind(extra)
    .bind(passed)
    .execute(f.board.store().pool())
    .await
    .unwrap();
}

/// The Review entry shows in Review; the round marker on a working ticket after a failed round, with the
/// project's rounds (plus extra ones); Merged automatically on Done.
#[tokio::test]
async fn ticket_columns_become_flags() {
    let f = Fixture::shop().await;
    let t = f.create("Checkout").await;
    let key = t.key.to_string();

    set(
        &f,
        &t.id,
        "UPDATE tickets SET state = 'working', stage_id = 'dev' WHERE id = ?",
    )
    .await;
    validate_counter(&f, &t.id, 1, 0, None).await;
    let ticket = f.ticket(&key).await;
    assert_eq!(
        ticket.flags.validation_round,
        Some(ValidationRound { round: 1, of: 3 })
    );
    assert_eq!(
        ticket.flags.validation_round.unwrap().to_string(),
        "Validation failed · round 1 of 3"
    );
    validate_counter(&f, &t.id, 1, 1, None).await;
    assert_eq!(
        f.ticket(&key).await.flags.validation_round,
        Some(ValidationRound { round: 1, of: 4 }),
        "Give it another round allows one more"
    );
    set(
        &f,
        &t.id,
        "UPDATE tickets SET state = 'validating' WHERE id = ?",
    )
    .await;
    assert_eq!(f.ticket(&key).await.flags.validation_round, None);

    set(
        &f,
        &t.id,
        "UPDATE tickets SET state = 'ready_for_review', stage_id = 'review', \
         review_entry = 'validated' WHERE id = ?",
    )
    .await;
    validate_counter(&f, &t.id, 0, 0, Some(2)).await;
    let ticket = f.ticket(&key).await;
    let entry = ticket.flags.review_entry.unwrap();
    assert_eq!(entry.entry, ReviewEntry::Validated);
    assert_eq!(entry.to_string(), "Validated · round 2");
    assert_eq!(ticket.flags.validation_round, None);

    set(
        &f,
        &t.id,
        "UPDATE tickets SET state = 'merged', stage_id = NULL, merged_automatically = TRUE WHERE id = ?",
    )
    .await;
    let ticket = f.ticket(&key).await;
    assert!(ticket.flags.merged_automatically);
    assert_eq!(ticket.board_group(), StatusSet::Done);
}

/// The card's details (T-0192): the round the validator checks, the last failed round's verdict (its checker's
/// comment, T-0446), and the files a conflict touches.
#[tokio::test]
async fn validating_round_verdict_and_conflict_files() {
    let f = Fixture::shop().await;
    let t = f.create("Checkout").await;
    let key = t.key.to_string();

    set(
        &f,
        &t.id,
        "UPDATE tickets SET state = 'validating', stage_id = 'validate' WHERE id = ?",
    )
    .await;
    validate_counter(&f, &t.id, 1, 0, None).await;
    assert_eq!(
        f.ticket(&key).await.flags.validating_round,
        Some(ValidationRound { round: 2, of: 3 }),
        "the second round is being checked"
    );
    set(
        &f,
        &t.id,
        "UPDATE tickets SET state = 'validator_needs_you' WHERE id = ?",
    )
    .await;
    assert!(f.ticket(&key).await.flags.validating_round.is_some());

    // Round 1 fails; the checker's verdict is its comment from that round (CMT-05). The card reads its first line
    // (CMT-10): not the user's comment, not another stage's, not another round's.
    set(
        &f,
        &t.id,
        "INSERT INTO validations (id, ticket_id, round, trigger, started_at, ended_at, outcome) \
         VALUES ('v1', ?, 1, 'agent', 1, 2, 'fail')",
    )
    .await;
    let validator = |stage: &str, round| CommentAuthor::Agent {
        id: Some("validator".into()),
        name: Some("Validator".into()),
        tool: None,
        stage: Some(stage.into()),
        round,
    };
    post(
        &f,
        &t.id,
        &validator("validate", Some(1)),
        "\n  Fail: the 10,000 row limit isn't enforced; 3 tests fail.\nmore",
    )
    .await;
    post(
        &f,
        &t.id,
        &CommentAuthor::User { name: "me".into() },
        "Also keep the column order.",
    )
    .await;
    post(
        &f,
        &t.id,
        &validator("security", Some(1)),
        "Another check's text",
    )
    .await;
    post(
        &f,
        &t.id,
        &validator("validate", Some(2)),
        "A later round's text",
    )
    .await;
    set(
        &f,
        &t.id,
        "UPDATE tickets SET state = 'working' WHERE id = ?",
    )
    .await;
    let ticket = f.ticket(&key).await;
    assert_eq!(ticket.flags.validating_round, None);
    assert_eq!(
        ticket.flags.verdict.as_deref(),
        Some("Fail: the 10,000 row limit isn't enforced; 3 tests fail.")
    );
    set(
        &f,
        &t.id,
        "UPDATE tickets SET state = 'validation_failed' WHERE id = ?",
    )
    .await;
    assert!(f.ticket(&key).await.flags.verdict.is_some());
    // A newer verdict in the same round wins; a deleted one is gone.
    post(
        &f,
        &t.id,
        &validator("validate", Some(1)),
        "Fail: still failing",
    )
    .await;
    assert_eq!(
        f.ticket(&key).await.flags.verdict.as_deref(),
        Some("Fail: still failing")
    );
    set(
        &f,
        &t.id,
        "UPDATE ticket_comments SET deleted_at = 9 WHERE ticket_id = ? AND n = 5",
    )
    .await;
    assert_eq!(
        f.ticket(&key).await.flags.verdict.as_deref(),
        Some("Fail: the 10,000 row limit isn't enforced; 3 tests fail.")
    );
    set(
        &f,
        &t.id,
        "UPDATE tickets SET state = 'ready_for_review', stage_id = 'review', \
         review_entry = 'failed_moved' WHERE id = ?",
    )
    .await;
    assert!(
        f.ticket(&key).await.flags.verdict.is_none(),
        "only while the verdict matters on the card"
    );
    // A later failed round without a verdict comment shows none, never round 1's.
    set(
        &f,
        &t.id,
        "UPDATE tickets SET state = 'validation_failed', stage_id = 'validate', review_entry = NULL WHERE id = ?",
    )
    .await;
    set(
        &f,
        &t.id,
        "INSERT INTO validations (id, ticket_id, round, trigger, started_at, ended_at, outcome) \
         VALUES ('v3', ?, 3, 'agent', 5, 6, 'fail')",
    )
    .await;
    assert_eq!(f.ticket(&key).await.flags.verdict, None);

    set(
        &f,
        &t.id,
        "UPDATE tickets SET state = 'merge_conflict_fixing', stage_id = 'dev', review_entry = NULL, \
         conflicts_with = 'main', conflict_files = '[\"lib/wishlist.ts\"]' WHERE id = ?",
    )
    .await;
    assert_eq!(
        f.ticket(&key).await.flags.conflict_files,
        ["lib/wishlist.ts"]
    );
}
