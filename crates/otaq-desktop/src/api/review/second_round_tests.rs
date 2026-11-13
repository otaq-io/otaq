//! "What the agent did" after a second round: SHOP-1's agent ends Dev ("Round one"), so SHOP-1 waits in Review; it
//! goes back to Dev, its agent commits again and ends Dev with another summary ("Round two"). Review's Overview (and
//! Changes, from the same `files.review`) shows the second handoff, not the first.
//!
//! The app talks to a core `TestService` (temporary `OTAQ_HOME` and repositories); the agent's session is a plain
//! `sh` sending requests through the socket. No agent tool runs.

use std::time::Duration;

use otaq_core::board::tickets::Ticket;
use otaq_core::domain::Status;
use otaq_core::git::testing::commit;
use otaq_core::service::testing::{AgentShell, TestService};
use serde_json::{Value, json};

use crate::App;
use crate::api::request;
use crate::service::testing::{app_on, connected};

async fn send(w: &TestService, shell: &AgentShell, method: &str, params: Value) -> Value {
    let request =
        json!({"protocol_version": otaq_core::protocol::PROTOCOL_VERSION, "id": 1, "request": {"method": method, "params": params}});
    let ran = shell
        .send_raw(&w.data.socket_path(), &request.to_string())
        .await;
    serde_json::from_str(ran.stdout.lines().next().unwrap_or_default())
        .unwrap_or_else(|e| panic!("{e}: {ran:?}"))
}

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

/// SHOP-1, working in Dev again, has its agent end the stage with `summary`, so it waits in Review.
async fn end_dev(w: &TestService, t: &Ticket, dev: &AgentShell, summary: &str) {
    w.force_status(t, Status::Working).await;
    send(w, dev, "ticket.prime", json!({ "key": "SHOP-1" })).await;
    let ended = send(
        w,
        dev,
        "ticket.stage_next",
        json!({ "key": "SHOP-1", "summary": summary }),
    )
    .await;
    assert!(ended["reply"]["stage_ended"].is_object(), "{ended}");
    assert_eq!(w.status("SHOP-1").await, Status::WaitingForYou);
}

/// WHAT THE AGENT DID on Review's Overview.
async fn what_the_agent_did(app: &App) -> Value {
    let notes = request(app, "files.review", json!({ "key": "SHOP-1" }))
        .await
        .unwrap();
    notes["summary"]["text"].clone()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_ticket_reaching_review_a_second_time_shows_its_second_handoff() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    w.board
        .edit_flow_for_tests("SHOP", otaq_core::flow::default::testing::without_validate)
        .await;
    let t = w.create("SHOP", "Offline pages").await;
    let wt = w.start_worktree(&t).await;
    commit(&wt, "offline.ts", "export const offline = true;\n");
    w.force_status(&t, Status::Working).await;
    let dev = w.agent_shell(&t).await;
    w.run_as_agent(&dev, "rust-dev", "Rust developer").await;
    end_dev(&w, &t, &dev, "Round one: pages cache for offline use.").await;
    let app = app(&w).await;
    assert_eq!(
        what_the_agent_did(&app).await,
        "Round one: pages cache for offline use."
    );

    // Back to Dev (as a send back or a conflict fix sends it), a new commit, and Dev ends again.
    commit(&wt, "offline.ts", "export const offline = false;\n");
    end_dev(&w, &t, &dev, "Round two: offline waits for the service worker.").await;
    assert_eq!(
        what_the_agent_did(&app).await,
        "Round two: offline waits for the service worker."
    );
}
