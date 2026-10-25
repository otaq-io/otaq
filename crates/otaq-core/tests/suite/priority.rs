//! Priorities (T-0177, TKT-09; d0065): the default, never empty in the store, and pinning leaving the field alone.

use crate::common::{Fixture, agent, me, shop_input};
use otaq_core::board::tickets::{NewTicketInput, TicketUpdate};
use otaq_core::domain::Priority;

fn set(priority: Priority) -> TicketUpdate {
    TicketUpdate {
        priority: Some(priority),
        ..Default::default()
    }
}

#[tokio::test]
async fn new_tickets_are_normal() {
    let f = Fixture::shop().await;
    let ticket = f.create("Checkout").await;
    assert_eq!(ticket.priority, Priority::Normal);
    // Never NULL in the store (migration 0066).
    let stored: i64 = sqlx::query_scalar("SELECT priority FROM tickets WHERE id = ?")
        .bind(&ticket.id)
        .fetch_one(f.board.store().pool())
        .await
        .unwrap();
    assert_eq!(stored, 3);
    let null = sqlx::query("UPDATE tickets SET priority = NULL WHERE id = ?")
        .bind(&ticket.id)
        .execute(f.board.store().pool())
        .await;
    assert!(null.is_err(), "priority is NOT NULL");
    let urgent = f
        .create_with(NewTicketInput {
            title: "Payments down".into(),
            priority: Some(Priority::Urgent),
            ..shop_input()
        })
        .await;
    assert_eq!(urgent.priority, Priority::Urgent);
}

#[tokio::test]
async fn a_change_is_recorded_and_pinning_never_changes_it() {
    let f = Fixture::shop().await;
    let t = f.create("Checkout").await;
    let key = t.key.to_string();
    f.board
        .update_ticket(&key, &set(Priority::High), &agent("SHOP-9"))
        .await
        .unwrap();
    let edits = f.board.agent_edits(&key).await.unwrap();
    let change = &edits.changes[0].change;
    assert_eq!(change.before.as_deref(), Some("P3 · Normal"));
    assert_eq!(change.after.as_deref(), Some("P2 · High"));

    f.board.pin(&key, &me()).await.unwrap();
    let pinned = f.ticket(&key).await;
    assert!(pinned.flags.pinned_at.is_some());
    assert_eq!(pinned.priority, Priority::High);
    f.board.unpin(&key, &me()).await.unwrap();
    assert_eq!(f.ticket(&key).await.priority, Priority::High);

    // The same value again is no change at all.
    let before = f.board.history(&key).await.unwrap().len();
    f.board
        .update_ticket(&key, &set(Priority::High), &me())
        .await
        .unwrap();
    assert_eq!(f.board.history(&key).await.unwrap().len(), before);
}
