//! A ticket whose agent stopped because its tool's login expired offers Sign in… on its sheet at once: SHOP-1's agent
//! (a plain `sh`) sends Claude Code's `StopFailure` hook, "Login expired · Please run /login", and the sheet's
//! red callout has Sign in… without a check of the tools (Settings → Agents never opened). Seen in re-test 5: it read
//! "Sign in to Claude Code, then Resume" with only Resume and Answer in terminal until the tools were checked again.
//!
//! The app talks to a core `TestService` (temporary `OTAQ_HOME` and repositories); Claude Code is a stand-in `sh`
//! script on the service's PATH whose `auth status` says signed in. No agent tool runs.

use std::time::Duration;

use otaq_core::agent_tools::Tool;
use otaq_core::domain::Status;
use otaq_core::service::testing::TestService;
use otaq_view::view_models::tool_problems::Action;
use serde_json::json;

use super::{ViewParams, view};
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
async fn a_login_expired_stop_offers_sign_in_on_the_sheet_at_once() {
    let dir = tempfile::tempdir().unwrap();
    let bin = dir.path().join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    otaq_core::process::testing::stub_command(
        &bin,
        "claude",
        r#"case "$1" in auth) echo '{"loggedIn": true, "authMethod": "claude.ai"}';; *) echo "2.1.4 (Claude Code)";; esac"#,
    );
    let managed = dir.path().join("managed");
    let w = TestService::start_with(move |mut config| {
        config.tools.path = bin.into_os_string();
        config.tools.managed_settings_dir = managed;
        config
    })
    .await;
    w.connected.recheck(None).await.unwrap();
    assert!(w.connected.status(Tool::Claude).state.is_connected());

    w.add_project("shop-web").await;
    let ticket = w.create("SHOP", "Order email").await;
    w.force_status(&ticket, Status::Working).await;
    let agent = w.agent_shell(&ticket).await;
    let app = app(&w).await;

    let payload = json!({"hook_event_name": "StopFailure", "error": "authentication_failed",
        "last_assistant_message": "Login expired \u{b7} Please run /login"});
    let request = json!({"protocol_version": otaq_core::protocol::PROTOCOL_VERSION, "id": 1, "request":
        {"method": "hook", "params": {"tool": "claude", "event": "StopFailure", "payload": payload}}});
    let ran = agent
        .send_raw(&w.data.socket_path(), &request.to_string())
        .await;
    assert!(ran.stdout.contains("\"reply\":\"ok\""), "{ran:?}");

    let sheet = view(
        &app,
        ViewParams {
            key: "SHOP-1".into(),
            opened: false,
        },
    )
    .await
    .unwrap()
    .view
    .unwrap();
    assert_eq!(sheet.pill.label, "Stopped \u{b7} login expired");
    let callout = &sheet.holds[0];
    assert_eq!(callout.heading, "Claude Code stopped \u{b7} login expired");
    let buttons: Vec<(&str, &Action)> = callout
        .buttons
        .iter()
        .map(|b| (b.label.as_str(), &b.action))
        .collect();
    assert_eq!(
        buttons,
        [(
            "Sign in\u{2026}",
            &Action::SignIn {
                tool: "claude".into()
            }
        )]
    );
    w.stop().await;
}
