//! "Manual checks for you" through the window (T-0499, d0125): SHOP-1's agent ends Dev with two `--manual-check`
//! items, so SHOP-1 waits in Review; the Overview lists them above Approve, the Local merge dialog says "2 manual
//! checks not ticked", and a tick from the Overview leaves one. The panel offers Run next to Approve.
//!
//! The app talks to a core `TestService` (temporary `OTAQ_HOME` and repositories); the agent's session is a plain
//! `sh` sending requests through the socket. No agent tool runs.

use std::time::Duration;

use otaq_core::domain::Status;
use otaq_core::git::testing::commit;
use otaq_core::service::testing::{AgentShell, TestService};
use serde_json::{Value, json};

use crate::App;
use crate::api::request;
use crate::service::testing::{app_on, connected};

async fn send(w: &TestService, shell: &AgentShell, method: &str, params: Value) -> Value {
    let request =
        json!({"protocol_version": 1, "id": 1, "request": {"method": method, "params": params}});
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

#[tokio::test(flavor = "multi_thread")]
async fn manual_checks_on_the_overview_and_in_the_merge_dialog() {
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
    send(&w, &dev, "ticket.prime", json!({ "key": "SHOP-1" })).await;
    let ended = send(
        &w,
        &dev,
        "ticket.stage_next",
        json!({"key": "SHOP-1", "summary": "Pages cache for offline use.", "manual_checks": [
            "Production build: stop the server and reload a visited page",
            "Open a never-visited detail page offline",
        ]}),
    )
    .await;
    assert!(ended["reply"]["stage_ended"].is_object(), "{ended}");
    assert_eq!(w.status("SHOP-1").await, Status::WaitingForYou);
    let app = app(&w).await;

    let overview = request(&app, "overview.view", json!({ "key": "SHOP-1" }))
        .await
        .unwrap();
    let list = &overview["manual_checks"];
    assert_eq!(list["heading"], "Manual checks for you");
    assert_eq!(
        list["items"][0]["text"],
        "Production build: stop the server and reload a visited page"
    );
    assert_eq!(list["items"][0]["source"], "Rust developer \u{b7} Dev");
    assert_eq!(list["items"][1]["ticked"], false);
    let dialog = request(&app, "review.approve", json!({ "key": "SHOP-1" }))
        .await
        .unwrap();
    assert_eq!(dialog["confirm"]["manual_checks"], "2 manual checks not ticked");

    request(
        &app,
        "overview.manual_check_tick",
        json!({ "key": "SHOP-1", "n": 2, "ticked": true }),
    )
    .await
    .unwrap();
    let overview = request(&app, "overview.view", json!({ "key": "SHOP-1" }))
        .await
        .unwrap();
    assert_eq!(overview["manual_checks"]["items"][1]["ticked"], true);
    let dialog = request(&app, "review.approve", json!({ "key": "SHOP-1" }))
        .await
        .unwrap();
    assert_eq!(dialog["confirm"]["manual_checks"], "1 manual check not ticked");

    // Run next to Approve, as during Dev.
    let panel = request(&app, "panel.view", json!({ "key": "SHOP-1" }))
        .await
        .unwrap();
    let actions: Vec<&str> = panel["view"]["buttons"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|b| b["action"].as_str())
        .collect();
    assert_eq!(actions, ["approve", "send_back", "run"]);
}
