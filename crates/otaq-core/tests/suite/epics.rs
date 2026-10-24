//! Epics (EPC-01, EPC-06, EPC-07, TKT-07) with the sample SHOP-40 Wishlist: 40.1 Merged, 40.2 working (simulated),
//! 40.3 Backlog; and SHOP-51 moved into the epic as SHOP-40.4.

use crate::common::{Fixture, me};
use otaq_core::board::Refusal;
use otaq_core::board::tickets::{Base, Ticket, TicketUpdate};
use otaq_core::domain::{Status, StatusSet};
use otaq_core::store::new_id;

async fn set_status(f: &Fixture, t: &Ticket, status: Status) {
    sqlx::query("UPDATE tickets SET stage_id = ?, state = ?, started_at = 1 WHERE id = ?")
        .bind(otaq_core::flow::engine::default_stage(status, None))
        .bind(status.as_str())
        .bind(&t.id)
        .execute(f.board.store().pool())
        .await
        .unwrap();
}

async fn wishlist() -> (Fixture, Ticket) {
    let f = Fixture::shop().await;
    f.skip_to(40).await;
    let epic = f.epic("Wishlist").await;
    let c1 = f.child(&epic, "Wishlist API").await;
    let c2 = f.child(&epic, "Wishlist UI").await;
    f.child(&epic, "Share wishlist").await;
    set_status(&f, &c1, Status::Merged).await;
    set_status(&f, &c2, Status::Working).await;
    let project_id: String = sqlx::query_scalar("SELECT id FROM projects WHERE key = 'SHOP'")
        .fetch_one(f.board.store().pool())
        .await
        .unwrap();
    sqlx::query("INSERT INTO sessions (id, project_id, ticket_id, kind, state, started_at) VALUES (?, ?, ?, 'agent', 'running', 1)")
        .bind(new_id())
        .bind(project_id)
        .bind(&c2.id)
        .execute(f.board.store().pool())
        .await
        .unwrap();
    (f, epic)
}

#[tokio::test]
async fn children_and_progress() {
    let (f, epic) = wishlist().await;
    let children = f.board.epic_children("SHOP-40").await.unwrap();
    let keys: Vec<_> = children.iter().map(|c| c.key.to_string()).collect();
    assert_eq!(keys, ["SHOP-40.1", "SHOP-40.2", "SHOP-40.3"]);
    for c in &children {
        assert_eq!(c.base, Base::Epic(epic.key.clone()));
        assert_eq!(c.external_id, None);
    }
    assert_eq!(f.ticket("SHOP-40").await.next_child_number, Some(4));
    assert_eq!(f.ticket("SHOP-40.1").await.next_child_number, None);
    let progress = f.board.epic_progress("SHOP-40").await.unwrap();
    assert_eq!(progress.label(), "1 of 3 merged");
    assert!(f.board.epic_progress("SHOP-40.1").await.is_err());
}

#[tokio::test]
async fn move_to_epic_gives_a_child_key_and_keeps_the_old_one() {
    let (f, _) = wishlist().await;
    f.skip_to(51).await;
    let t = f.create("Wishlist badge").await;
    assert_eq!(t.key.to_string(), "SHOP-51");

    let moved = f
        .board
        .move_to_epic("SHOP-51", "SHOP-40", &me())
        .await
        .unwrap();
    assert_eq!(moved.key.to_string(), "SHOP-40.4");
    assert_eq!(moved.id, t.id);
    assert_eq!(moved.aliases, vec!["SHOP-51".parse().unwrap()]);
    assert_eq!(moved.base, Base::Epic("SHOP-40".parse().unwrap()));
    assert_eq!(
        f.ticket("SHOP-51").await.key.to_string(),
        "SHOP-40.4",
        "the old key still resolves"
    );
    assert_eq!(
        f.board.epic_progress("SHOP-40").await.unwrap().label(),
        "1 of 4 merged"
    );
    assert_eq!(
        f.create("next").await.key.to_string(),
        "SHOP-52",
        "51 isn't handed out again"
    );

    // A child can't move again, and an alias keeps working for updates too.
    let err = f
        .board
        .move_to_epic("SHOP-51", "SHOP-40", &me())
        .await
        .unwrap_err();
    assert_eq!(err.refusal(), Some(Refusal::AlreadyInEpic));
    f.board
        .update_ticket(
            "SHOP-51",
            &TicketUpdate {
                title: Some("Badge".into()),
                ..Default::default()
            },
            &me(),
        )
        .await
        .unwrap();
    assert_eq!(f.ticket("SHOP-40.4").await.title, "Badge");
}

#[tokio::test]
async fn move_to_epic_refusals() {
    let (f, _) = wishlist().await;
    let started = f.create("Started").await;
    set_status(&f, &started, Status::Queued).await;
    let err = f
        .board
        .move_to_epic(&started.key.to_string(), "SHOP-40", &me())
        .await
        .unwrap_err();
    assert_eq!(err.refusal(), Some(Refusal::Started), "{err}");

    let other_epic = f.epic("Other").await;
    let err = f
        .board
        .move_to_epic(&other_epic.key.to_string(), "SHOP-40", &me())
        .await
        .unwrap_err();
    assert_eq!(err.refusal(), Some(Refusal::EpicInEpic));
    let err = f
        .board
        .move_to_epic("SHOP-40.3", &other_epic.key.to_string(), &me())
        .await
        .unwrap_err();
    assert_eq!(err.refusal(), Some(Refusal::AlreadyInEpic));

    // Links would cross bases after adopting the epic's branch.
    let a = f.create("Parent on main").await;
    let b = f.create("Child").await;
    f.board
        .add_dependency(&b.key.to_string(), &a.key.to_string(), &me())
        .await
        .unwrap();
    let err = f
        .board
        .move_to_epic(&b.key.to_string(), "SHOP-40", &me())
        .await
        .unwrap_err();
    assert_eq!(err.refusal(), Some(Refusal::CrossBase));
    assert!(
        err.to_string()
            .contains("merges into the epic's branch (SHOP-40)")
            && err
                .to_string()
                .ends_with("Linked tickets must use the same base branch."),
        "{err}"
    );
    let plain = f.create("Plain").await;
    let err = f
        .board
        .move_to_epic(&plain.key.to_string(), &a.key.to_string(), &me())
        .await
        .unwrap_err();
    assert_eq!(err.refusal(), Some(Refusal::NotAnEpic));
}

#[tokio::test]
async fn close_plan_and_close() {
    let (f, _) = wishlist().await;
    let plan = f.board.epic_close_plan("SHOP-40").await.unwrap();
    let close: Vec<_> = plan
        .close
        .iter()
        .map(|c| (c.key.to_string(), c.agent_running))
        .collect();
    assert_eq!(
        close,
        [
            ("SHOP-40.2".to_owned(), true),
            ("SHOP-40.3".to_owned(), false)
        ]
    );
    assert_eq!(
        plan.stay_merged
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        ["SHOP-40.1"]
    );

    f.board.close_epic("SHOP-40", &me()).await.unwrap();
    assert_eq!(
        f.ticket("SHOP-40.1").await.status,
        Status::Merged,
        "merged children stay merged"
    );
    for key in ["SHOP-40.2", "SHOP-40.3", "SHOP-40"] {
        let t = f.ticket(key).await;
        assert_eq!(
            (t.board_group(), t.status),
            (StatusSet::Done, Status::ClosedNotMerged),
            "{key}"
        );
    }
    assert!(
        f.board
            .epic_close_plan("SHOP-40")
            .await
            .unwrap()
            .close
            .is_empty()
    );
}

#[tokio::test]
async fn an_epic_with_nothing_merged_ends_closed_not_merged() {
    let f = Fixture::shop().await;
    let epic = f.epic("Search").await;
    f.child(&epic, "Index").await;
    f.board
        .close_epic(&epic.key.to_string(), &me())
        .await
        .unwrap();
    assert_eq!(
        f.ticket(&epic.key.to_string()).await.status,
        Status::ClosedNotMerged
    );
    assert_eq!(
        f.board
            .epic_progress(&epic.key.to_string())
            .await
            .unwrap()
            .label(),
        "0 merged · 1 closed"
    );
}
