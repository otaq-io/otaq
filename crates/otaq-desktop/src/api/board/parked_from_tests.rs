//! The stage a parked ticket left, after more than one park: SHOP-1 on a flow of Plan → Dev is parked from Plan,
//! dragged on into Dev and parked again. Its drag names Dev (the latest park), not Plan (History is newest first).
//!
//! The app talks to a core `TestService` (temporary `OTAQ_HOME` and repositories). No agent tool runs.

use std::time::Duration;

use otaq_core::domain::{Actor, Status};
use otaq_core::flow::default::DEV;
use otaq_core::service::testing::TestService;

use super::{flow, parked_from};
use crate::App;
use crate::service::testing::{app_on, connected};

async fn app(w: &TestService) -> App {
    let app = app_on(w.data.clone());
    tokio::time::timeout(Duration::from_secs(10), async {
        while !connected(&app.service.connection()) {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the app connects");
    app
}

#[tokio::test(flavor = "multi_thread")]
async fn a_ticket_parked_twice_names_the_stage_of_its_latest_park() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    // A Plan stage before Dev: two Agent stages a ticket can be parked from.
    w.board
        .edit_flow_for_tests("SHOP", |file| {
            let dev = file.flow.stages.iter().find(|s| s.id == DEV).unwrap();
            let mut plan = dev.clone();
            plan.id = "plan".into();
            plan.name = "Plan".into();
            file.flow.stages.insert(0, plan);
        })
        .await;
    let t = w.create("SHOP", "Offline pages").await;
    w.start_worktree(&t).await;
    let me = Actor::User("me".into());
    let app = app(&w).await;
    let flow = flow(&app, "SHOP").await;

    w.force_stage(&t, "plan", Status::Working).await;
    let parked = w.review.park("SHOP-1", true, &me).await.unwrap();
    assert_eq!(parked.status, Status::Parked);
    assert_eq!(parked_from(&app, &parked, &flow).await.as_deref(), Some("Plan"));

    // Later, on into Dev and parked again.
    tokio::time::sleep(Duration::from_millis(5)).await;
    w.force_stage(&t, DEV, Status::Working).await;
    let parked = w.review.park("SHOP-1", true, &me).await.unwrap();
    assert_eq!(parked.status, Status::Parked);
    assert_eq!(parked_from(&app, &parked, &flow).await.as_deref(), Some("Dev"));
}
