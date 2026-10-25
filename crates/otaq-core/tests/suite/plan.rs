//! A ticket's Plan section (T-0536, d0133): stored with who wrote it and when, edited like the description with
//! History keeping every version (0109), printed by `prime` with its pieces' order, and a user's edit reaching the
//! working agent as a change note naming the pieces (0048, audit answer 6).

use otaq_core::board::tickets::{EventKind, NewTicketInput, TicketUpdate};
use otaq_core::client::{Client, ClientConfig};
use otaq_core::domain::plan::PlanBy;
use otaq_core::domain::{Actor, Status};
use otaq_core::protocol::{Request, TicketUpdateParams};
use otaq_core::service::testing::TestService;

const PLAN: &str = "Why: the API lands first so the page has something to call.\n\n## Pieces\n\n\
                    1. Wishlist API — waits for nothing\n2. Wishlist button — waits for 1\n\
                    3. Wishlist page — waits for 1\n4. Share link — waits for 2, 3\n";

fn me() -> Actor {
    Actor::User("me".into())
}

fn plan(text: &str) -> TicketUpdate {
    TicketUpdate {
        plan: Some(text.into()),
        ..Default::default()
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn written_at_creation_edited_and_cleared_with_history() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let created = w
        .board
        .create_ticket(
            &NewTicketInput {
                project: "SHOP".into(),
                title: "Wishlist".into(),
                body: "Shoppers keep a list.".into(),
                plan: PLAN.into(),
                ..Default::default()
            },
            &me(),
        )
        .await
        .unwrap();
    assert_eq!(created.plan, PLAN);
    assert_eq!(created.plan_by, Some(PlanBy::User("me".into())));
    assert!(created.plan_at.is_some());

    // A description edit leaves the Plan's writer and time alone.
    let body_only = w
        .board
        .update_ticket(
            "SHOP-1",
            &TicketUpdate {
                body: Some("Shoppers keep a list of products.".into()),
                ..Default::default()
            },
            &me(),
        )
        .await
        .unwrap();
    assert_eq!(
        (body_only.plan_by.clone(), body_only.plan_at),
        (created.plan_by.clone(), created.plan_at)
    );

    // Noyan rewrites it: History has the before and after.
    let edited = PLAN.replace("4. Share link — waits for 2, 3\n", "");
    let noyan = Actor::ProjectAgent("pa1".into());
    let after = w
        .board
        .update_ticket("SHOP-1", &plan(&edited), &noyan)
        .await
        .unwrap();
    assert_eq!(after.plan, edited);
    assert_eq!(after.plan_by.as_ref().map(PlanBy::label), Some("Noyan"));
    let history = w.board.history("SHOP-1").await.unwrap();
    let plan_rows: Vec<_> = history
        .iter()
        .filter(|e| e.kind == EventKind::Updated)
        .filter_map(|e| {
            let changes = e.payload.get("changes")?.as_array()?;
            let c = changes.iter().find(|c| c["field"] == "plan")?;
            Some((e.actor.clone(), c["before"].clone(), c["after"].clone()))
        })
        .collect();
    assert_eq!(
        plan_rows,
        [(noyan.clone(), PLAN.into(), edited.clone().into())]
    );

    // The same text again changes nothing; an empty Plan removes it and its writer.
    let same = w
        .board
        .update_ticket("SHOP-1", &plan(&edited), &me())
        .await
        .unwrap();
    assert_eq!(same.plan_by.as_ref().map(PlanBy::label), Some("Noyan"));
    let cleared = w
        .board
        .update_ticket("SHOP-1", &plan("  \n"), &me())
        .await
        .unwrap();
    assert_eq!(
        (cleared.plan.as_str(), cleared.plan_by, cleared.plan_at),
        ("", None, None)
    );
    let rows = w
        .board
        .history("SHOP-1")
        .await
        .unwrap()
        .iter()
        .filter(|e| {
            e.payload["changes"]
                .as_array()
                .is_some_and(|c| c.iter().any(|c| c["field"] == "plan"))
        })
        .count();
    assert_eq!(rows, 2, "every version is kept");
    w.stop().await;
}

/// A stage's agent is named by its stage ("Written by Dev"), not by its ticket.
#[tokio::test(flavor = "multi_thread")]
async fn a_stage_agent_is_named_by_its_stage() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let t = w.create("SHOP", "Wishlist").await;
    w.force_status(&t, Status::Working).await;
    let agent = Actor::Agent(t.key.clone());
    let after = w
        .board
        .update_ticket("SHOP-1", &plan(PLAN), &agent)
        .await
        .unwrap();
    assert_eq!(after.plan_by, Some(PlanBy::Stage("Dev".into())));
    assert_eq!(after.plan_by.unwrap().to_string(), "stage:Dev");
    w.stop().await;
}

async fn prime(w: &TestService, key: &str) -> String {
    otaq_core::prime::render(
        &w.board
            .prime_context(key, w.adapters.for_new_session())
            .await
            .unwrap(),
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn prime_prints_the_plan_and_its_order() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    w.create("SHOP", "Wishlist").await;
    assert!(
        !prime(&w, "SHOP-1").await.contains("## Plan"),
        "no plan, no section"
    );

    w.board
        .update_ticket("SHOP-1", &plan(PLAN), &me())
        .await
        .unwrap();
    let text = prime(&w, "SHOP-1").await;
    let section = format!(
        "## What and why\n\n(no description)\n\n## Plan\n\n{}\n\nPieces: 4 (1 \u{2192} 2, 3 \u{2192} 4)\n",
        PLAN.trim()
    );
    assert!(text.contains(&section), "{text}");

    // A plan with no Pieces list is printed with no count.
    w.board
        .update_ticket("SHOP-1", &plan("Build it in one go."), &me())
        .await
        .unwrap();
    let text = prime(&w, "SHOP-1").await;
    assert!(text.contains("## Plan\n\nBuild it in one go.\n"), "{text}");
    assert!(!text.contains("Pieces:"), "{text}");
    w.stop().await;
}

/// The user's Plan edit on a working ticket queues the note naming the pieces (audit answer 6); the agent's own edit
/// queues nothing.
#[tokio::test(flavor = "multi_thread")]
async fn a_user_plan_edit_reaches_the_working_agent() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    for _ in 1..40 {
        w.create("SHOP", "filler").await;
    }
    let t = w.create("SHOP", "Wishlist").await;
    w.board
        .update_ticket("SHOP-40", &plan(PLAN), &me())
        .await
        .unwrap();
    w.force_status(&t, Status::Working).await;
    let agent = w.agent_shell(&t).await;
    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    let edited = PLAN.replace("4. Share link — waits for 2, 3\n", "");
    client
        .request(Request::TicketUpdate(TicketUpdateParams {
            key: "SHOP-40".into(),
            update: plan(&edited),
        }))
        .await
        .unwrap();
    let pending = w.deliveries.pending(&agent.session).await.unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(
        pending[0].text,
        "The ticket was updated by the user. The plan changed: 40.4 removed. Run `otaq prime` for the full \
         ticket."
    );
    w.stop().await;
}
