//! Tickets on a temporary database (TKT-01, TKT-03): numbering, field rules and the event log.

use crate::common;

use crate::common::{Fixture, agent, me, shop_input};
use otaq_core::board::Refusal;
use otaq_core::board::tickets::{Base, NewTicketInput, TicketUpdate};
use otaq_core::domain::Priority;
use otaq_core::domain::{Actor, Status, StatusSet};
use serde_json::Value;

async fn events(f: &Fixture, ticket_id: &str) -> Vec<(String, String, Value)> {
    let rows: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT kind, actor, payload_json FROM events WHERE ticket_id = ? ORDER BY at, id",
    )
    .bind(ticket_id)
    .fetch_all(f.board.store().pool())
    .await
    .unwrap();
    rows.into_iter()
        .map(|(k, a, p)| (k, a, serde_json::from_str(&p).unwrap()))
        .collect()
}

#[tokio::test]
async fn create_allocates_numbers_and_lands_in_backlog() {
    let f = Fixture::shop().await;
    let first = f.create("Checkout page").await;
    assert_eq!(first.key.to_string(), "SHOP-1");
    assert_eq!(
        (first.board_group(), first.status),
        (StatusSet::Backlog, Status::Backlog)
    );
    assert_eq!(first.base, Base::Branch("main".into()));
    assert_eq!(first.created_by, me());

    assert_eq!(first.priority, Priority::Normal);
    assert_eq!(f.create("next").await.key.to_string(), "SHOP-2");
    // There are no child keys (d0133).
    assert!(f.board.ticket("SHOP-2.1").await.is_err());

    // Another project numbers on its own.
    f.add_project("billing-api").await;
    let bill = f
        .create_with(NewTicketInput {
            project: "BILL".into(),
            title: "Invoices".into(),
            ..Default::default()
        })
        .await;
    assert_eq!(bill.key.to_string(), "BILL-1");
}

#[tokio::test]
async fn deleted_numbers_are_never_handed_out_again() {
    let f = Fixture::shop().await;
    f.skip_to(24).await;
    let shop24 = f.create("Order history").await;
    assert_eq!(shop24.key.to_string(), "SHOP-24");
    sqlx::query("DELETE FROM tickets WHERE id = ?")
        .bind(&shop24.id)
        .execute(f.board.store().pool())
        .await
        .unwrap();
    assert!(f.board.ticket("SHOP-24").await.is_err());
    assert_eq!(f.create("Next").await.key.to_string(), "SHOP-25");
}

#[tokio::test]
async fn update_fields() {
    let f = Fixture::shop().await;
    let t = f
        .create_with(NewTicketInput {
            title: "Checkout".into(),
            ..shop_input()
        })
        .await;
    let updated = f
        .board
        .update_ticket(
            &t.key.to_string(),
            &TicketUpdate {
                title: Some("  Checkout page ".into()),
                body: Some("Use the **new** API\n\n```ts\npay()\n```".into()),
                priority: Some(Priority::High),
                base_branch: Some("release/2.3".into()),
                external_id: Some("PAY-123".into()),
                ..Default::default()
            },
            &agent("SHOP-9"),
        )
        .await
        .unwrap();
    assert_eq!(updated.title, "Checkout page");
    assert!(updated.body.contains("```ts\npay()\n```"));
    assert_eq!(updated.priority, Priority::High);
    assert_eq!(updated.base, Base::Branch("release/2.3".into()));
    assert_eq!(updated.external_id.as_deref(), Some("PAY-123"));
    assert_eq!(updated.updated_by, Some(agent("SHOP-9")));

    let cleared = f
        .board
        .update_ticket(
            &t.key.to_string(),
            &TicketUpdate {
                external_id: Some(String::new()),
                ..Default::default()
            },
            &me(),
        )
        .await
        .unwrap();
    assert_eq!(cleared.external_id, None);

    for bad in [TicketUpdate {
        title: Some("  ".into()),
        ..Default::default()
    }] {
        let err = f
            .board
            .update_ticket(&t.key.to_string(), &bad, &me())
            .await
            .unwrap_err();
        assert!(err.refusal().is_some(), "{bad:?}: {err}");
    }
    let err = f
        .board
        .create_ticket(
            &NewTicketInput {
                title: "".into(),
                ..shop_input()
            },
            &me(),
        )
        .await
        .unwrap_err();
    assert_eq!(err.refusal(), Some(Refusal::EmptyField));
    assert_eq!(err.to_string(), "A ticket needs a title");
}

#[tokio::test]
async fn base_branch_and_external_id_rules() {
    let f = Fixture::shop().await;
    // Every ticket has its own base and External ID (d0059, d0133).
    let plain = f.create("plain").await;
    let ticket = f
        .board
        .update_ticket(
            &plain.key.to_string(),
            &TicketUpdate {
                base_branch: Some("develop".into()),
                external_id: Some("PAY-123".into()),
                ..Default::default()
            },
            &me(),
        )
        .await
        .unwrap();
    assert_eq!(
        (ticket.base.clone(), ticket.external_id.as_deref()),
        (Base::Branch("develop".into()), Some("PAY-123"))
    );

    // Started tickets lock their base.
    sqlx::query("UPDATE tickets SET branch_locked = TRUE, started_at = 1 WHERE id = ?")
        .bind(&plain.id)
        .execute(f.board.store().pool())
        .await
        .unwrap();
    let err = f
        .board
        .update_ticket(
            &plain.key.to_string(),
            &TicketUpdate {
                base_branch: Some("release/2.3".into()),
                ..Default::default()
            },
            &me(),
        )
        .await
        .unwrap_err();
    assert_eq!(err.refusal(), Some(Refusal::BaseLocked));
    // Setting the same base again is not a change.
    f.board
        .update_ticket(
            &plain.key.to_string(),
            &TicketUpdate {
                base_branch: Some("develop".into()),
                ..Default::default()
            },
            &me(),
        )
        .await
        .unwrap();
}

#[tokio::test]
async fn every_change_writes_the_actor_and_an_event() {
    let f = Fixture::shop().await;
    let bot = agent("SHOP-18");
    let t = f
        .board
        .create_ticket(
            &NewTicketInput {
                title: "Fix cart".into(),
                ..shop_input()
            },
            &bot,
        )
        .await
        .unwrap();
    assert_eq!(t.created_by, bot);
    f.board
        .update_ticket(
            &t.key.to_string(),
            &TicketUpdate {
                title: Some("Fix the cart".into()),
                body: Some("why".into()),
                ..Default::default()
            },
            &me(),
        )
        .await
        .unwrap();
    // A no-op update writes nothing.
    f.board
        .update_ticket(
            &t.key.to_string(),
            &TicketUpdate {
                title: Some("Fix the cart".into()),
                ..Default::default()
            },
            &me(),
        )
        .await
        .unwrap();

    let log = events(&f, &t.id).await;
    assert_eq!(log.len(), 2);
    assert_eq!(
        (log[0].0.as_str(), log[0].1.as_str()),
        ("created", "agent:SHOP-18")
    );
    assert_eq!(log[0].2["title"], "Fix cart");
    assert_eq!((log[1].0.as_str(), log[1].1.as_str()), ("updated", "me"));
    let changes = log[1].2["changes"].as_array().unwrap();
    assert_eq!(changes.len(), 2);
    assert_eq!(changes[0]["field"], "title");
    assert_eq!(changes[0]["after"], "Fix the cart");
    assert_eq!(changes[1]["field"], "body");
    assert_eq!(changes[1]["before"], "");
    assert_eq!(changes[1]["after"], "why");
    let t = f.ticket(&t.key.to_string()).await;
    assert_eq!(t.updated_by, Some(Actor::User("me".into())));
}

#[tokio::test]
async fn history_is_the_event_log_newest_first() {
    use otaq_core::board::moves::DropTarget;
    use otaq_core::board::tickets::EventKind;
    let f = common::Fixture::shop().await;
    let t = f
        .create_with(otaq_core::board::tickets::NewTicketInput {
            title: "t".into(),
            ..common::shop_input()
        })
        .await;
    f.board
        .move_ticket(
            &t.key.to_string(),
            DropTarget::ReadyForDev,
            false,
            &common::me(),
        )
        .await
        .unwrap();
    let history = f.board.history("SHOP-1").await.unwrap();
    let kinds: Vec<_> = history.iter().map(|e| e.kind).collect();
    assert_eq!(kinds, [EventKind::Moved, EventKind::Created]);
    assert_eq!(history[0].payload["to"], "queued");
    assert_eq!(history[0].actor, common::me());
    assert!(f.board.history("SHOP-99").await.is_err());
}

/// T-0304 (M19; d0085, d0086, d0087): the ticket environment columns reach `Work`, and at their defaults they leave
/// an unstarted ticket without work and add nothing to its JSON.
#[tokio::test]
async fn ticket_env_columns_reach_work_only_when_set() {
    use otaq_core::domain::{BuildState, ResourceState};

    let f = Fixture::shop().await;
    let t = f.create("Checkout page").await;
    let id = t.id.clone();
    assert_eq!(t.work, None, "defaults are no work");
    let json = serde_json::to_value(&t).unwrap();
    assert!(json.get("work").is_none(), "{json}");

    let store = f.board.store();
    store
        .set_build_state(&id, BuildState::Freed, Some("a file is open"))
        .await
        .unwrap();
    store
        .set_resource(&id, Some("shop_1"), ResourceState::Created, None)
        .await
        .unwrap();
    store
        .set_schema_overlap(&id, Some(r#"[{"other":"SHOP-19","path":"db/**"}]"#))
        .await
        .unwrap();
    let t = f.board.ticket("SHOP-1").await.unwrap();
    let work = t.work.clone().expect("the environment is work");
    assert_eq!(work.build_state, BuildState::Freed);
    assert_eq!(work.build_problem.as_deref(), Some("a file is open"));
    assert_eq!(work.resource_name.as_deref(), Some("shop_1"));
    assert_eq!(work.resource_state, ResourceState::Created);
    assert_eq!(work.schema_overlap.len(), 1);
    assert_eq!(work.schema_overlap[0].other.to_string(), "SHOP-19");
    assert_eq!(work.schema_overlap[0].path, "db/**");
    let json = serde_json::to_value(&t).unwrap();
    assert_eq!(json["work"]["build_state"], "freed", "{json}");
    assert_eq!(json["work"]["resource_state"], "created", "{json}");

    store
        .set_build_state(&id, BuildState::Present, None)
        .await
        .unwrap();
    store
        .set_resource(&id, None, ResourceState::None, None)
        .await
        .unwrap();
    store.set_schema_overlap(&id, None).await.unwrap();
    let t = f.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(t.work, None, "back at the defaults");
}

/// CMT-14: an agent's description edit records who it was, so History reads "Griller · Grill edited the
/// description"; the user's records no author.
#[tokio::test]
async fn an_agents_edit_names_its_author() {
    use otaq_core::board::ticket_comments::CommentAuthor;
    let f = Fixture::shop().await;
    let t = f.create("Export orders as CSV").await;
    let griller = CommentAuthor::Agent {
        id: Some("griller".into()),
        name: Some("Griller".into()),
        tool: None,
        stage: Some("grill".into()),
        round: None,
    };
    let body = |text: &str| TicketUpdate {
        body: Some(text.into()),
        ..Default::default()
    };
    f.board
        .update_ticket_by(
            &t.key.to_string(),
            &body("Plan\n## Agreed"),
            &agent("SHOP-1"),
            Some(&griller),
        )
        .await
        .unwrap();
    f.board
        .update_ticket(&t.key.to_string(), &body("Plan\n## Agreed\n- CSV"), &me())
        .await
        .unwrap();
    let updated: Vec<Value> = events(&f, &t.id)
        .await
        .into_iter()
        .filter(|(kind, _, _)| kind == "updated")
        .map(|(_, _, payload)| payload)
        .collect();
    assert_eq!(updated.len(), 2);
    assert_eq!(updated[0]["author"]["name"], "Griller");
    assert_eq!(updated[0]["author"]["stage"], "grill");
    assert_eq!(updated[0]["changes"][0]["after"], "Plan\n## Agreed");
    assert!(updated[1].get("author").is_none());
}
