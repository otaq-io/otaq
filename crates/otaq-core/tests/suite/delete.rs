//! Deleting never-started tickets (TKT-08; d0060): refusals on every state that has been to Dev, links named
//! and removed in both directions, numbers never reused.

use crate::common::{Fixture, me};
use otaq_core::board::Refusal;
use otaq_core::board::moves::DropTarget;
use otaq_core::store::new_id;

async fn count(f: &Fixture, table: &str) -> i64 {
    sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT COUNT(*) FROM {table}")))
        .fetch_one(f.board.store().pool())
        .await
        .unwrap()
}

#[tokio::test]
async fn refused_once_a_ticket_has_been_to_dev() {
    let f = Fixture::shop().await;
    f.skip_to(19).await;
    let t = f.create("Price table").await;
    f.board
        .move_ticket("SHOP-19", DropTarget::ReadyForDev, true, &me())
        .await
        .unwrap();
    let err = f.board.delete_preview("SHOP-19").await.unwrap_err();
    assert_eq!(err.refusal(), Some(Refusal::Started));
    assert_eq!(
        err.to_string(),
        "SHOP-19 has been to Dev. Close it instead."
    );
    assert!(f.board.delete_ticket("SHOP-19").await.is_err());

    // Closed after a start, Parked, or with a branch, worktree or session row: all count.
    for (sql, title) in [
        (
            "UPDATE tickets SET state = 'parked' , started_at = 1 WHERE id = ?",
            "Parked",
        ),
        (
            "UPDATE tickets SET branch_name = 'x' WHERE id = ?",
            "Branch",
        ),
        (
            "UPDATE tickets SET worktree_path = '/tmp/x' WHERE id = ?",
            "Worktree",
        ),
    ] {
        let other = f.create(title).await;
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(&other.id)
            .execute(f.board.store().pool())
            .await
            .unwrap();
        let err = f
            .board
            .delete_preview(&other.key.to_string())
            .await
            .unwrap_err();
        assert_eq!(err.refusal(), Some(Refusal::Started), "{title}");
    }
    let with_session = f.create("Session").await;
    let project_id: String = sqlx::query_scalar("SELECT id FROM projects")
        .fetch_one(f.board.store().pool())
        .await
        .unwrap();
    sqlx::query("INSERT INTO sessions (id, project_id, ticket_id, kind, state, started_at) VALUES (?, ?, ?, 'agent', 'ended', 1)")
        .bind(new_id())
        .bind(project_id)
        .bind(&with_session.id)
        .execute(f.board.store().pool())
        .await
        .unwrap();
    assert!(
        f.board
            .delete_preview(&with_session.key.to_string())
            .await
            .is_err()
    );
    let _ = t;
}

#[tokio::test]
async fn delete_names_and_removes_links_and_never_reuses_the_number() {
    let f = Fixture::shop().await;
    f.skip_to(20).await;
    for title in ["Checkout page", "Order confirmation email", "Fix login"] {
        f.create(title).await;
    }
    f.board
        .add_dependency("SHOP-21", "SHOP-20", &me())
        .await
        .unwrap();
    f.board
        .add_dependency("SHOP-22", "SHOP-21", &me())
        .await
        .unwrap();
    let preview = f.board.delete_preview("SHOP-21").await.unwrap();
    assert_eq!(
        preview.question(),
        "Delete SHOP-21 Order confirmation email?"
    );
    assert_eq!(
        preview.link_lines(),
        [
            "SHOP-20 is a dependency of SHOP-21; that link will be removed",
            "SHOP-21 is a dependency of SHOP-22; that link will be removed",
        ]
    );
    let review_ticket: String = sqlx::query_scalar("SELECT id FROM tickets WHERE number = 21")
        .fetch_one(f.board.store().pool())
        .await
        .unwrap();
    sqlx::query("INSERT INTO review_comments (id, ticket_id, round, file, line, side, body, created_at) VALUES (?, ?, 1, 'a', 1, 'new', 'b', 0)")
        .bind(new_id())
        .bind(&review_ticket)
        .execute(f.board.store().pool())
        .await
        .unwrap();
    f.board.delete_ticket("SHOP-21").await.unwrap();
    assert!(f.board.ticket("SHOP-21").await.is_err(), "unknown ticket");
    assert!(
        f.ticket("SHOP-22").await.depends_on.is_empty(),
        "the link is gone in both directions"
    );
    assert_eq!(count(&f, "ticket_deps").await, 0);
    assert_eq!(count(&f, "review_comments").await, 0);
    assert_eq!(
        f.create("Next").await.key.to_string(),
        "SHOP-23",
        "21 is never handed out again"
    );
}
