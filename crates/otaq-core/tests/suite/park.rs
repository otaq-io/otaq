//! Park (T-0104, CLS-06): the agent stops; branch, worktree (with uncommitted files) and the Claude session ID
//! stay; ports return; back in Dev the same conversation resumes with new ports. `claude` stands in as an `sh`
//! printing its arguments.

use std::time::Duration;

use otaq_core::agent_tools::LaunchContext;
use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::board::Refusal;
use otaq_core::board::moves::DropTarget;
use otaq_core::domain::{Actor, Status};
use otaq_core::scheduler::HOLDS_A_SLOT;
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::TestService;
use otaq_core::session::SessionKind;

fn me() -> Actor {
    Actor::User("me".into())
}

async fn until_status(w: &TestService, key: &str, want: Status) {
    let reached = tokio::time::timeout(Duration::from_secs(20), async {
        while w.status(key).await != want {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await;
    if reached.is_err() {
        let t = w.board.ticket(key).await.unwrap();
        panic!(
            "{key} never became {want}: {} {:?}",
            t.status, t.start_problem
        );
    }
}

#[tokio::test]
async fn park_keeps_the_work_and_back_in_dev_resumes_the_conversation() {
    let w = TestService::start_with(|config| ServiceConfig {
        launch: Some(LaunchContext {
            bin_dir: "/nonexistent".into(),
            plugin_dir: "/nonexistent/plugin".into(),
            path: std::env::var_os("PATH").unwrap_or_default(),
            claude: ClaudeLaunch {
                program: vec![
                    "sh".into(),
                    "-c".into(),
                    "printf 'ARGS %s\\n' \"$*\"; exec sleep 30".into(),
                    "sh".into(),
                ],
                config_dir: None,
            },
        }),
        ..config
    })
    .await;
    w.add_project("shop-web").await;
    let t = w.create("SHOP", "Coupon codes").await;
    w.board
        .move_ticket("SHOP-1", DropTarget::ReadyForDev, true, &me())
        .await
        .unwrap();
    w.starter
        .start("SHOP-1", &me(), otaq_core::session::Start::Auto)
        .await
        .unwrap();
    until_status(&w, "SHOP-1", Status::Working).await;
    let first = w.sessions.running_of(&t.id, SessionKind::Agent).unwrap();
    let conversation = first.tool_session_id.clone().unwrap();
    let worktree = w
        .board
        .ticket("SHOP-1")
        .await
        .unwrap()
        .work
        .unwrap()
        .worktree
        .unwrap();
    std::fs::write(std::path::Path::new(&worktree).join("draft.ts"), "wip\n").unwrap();

    let err = w.review.park("SHOP-1", false, &me()).await.unwrap_err();
    assert_eq!(err.refusal(), Some(Refusal::NeedsConfirmation));
    assert_eq!(
        err.to_string(),
        "SHOP-1's agent will be stopped. Its branch, files and conversation are kept."
    );
    let parked = w.review.park("SHOP-1", true, &me()).await.unwrap();
    assert_eq!(parked.status, Status::Parked);
    assert_eq!(parked.status.label(), "Parked · has work");
    assert!(!HOLDS_A_SLOT.contains(&Status::Parked), "no agent slot");
    assert!(
        !w.sessions.info(&first.id).unwrap().is_running(),
        "agent stopped"
    );
    assert!(
        std::path::Path::new(&worktree).join("draft.ts").is_file(),
        "files kept"
    );
    let row = w.board.store().ticket(&t.id).await.unwrap().unwrap();
    assert!(row.parked);
    assert_eq!((row.agent_port, row.run_port), (None, None), "ports freed");
    assert!(row.branch_name.is_some());

    // Back to Dev: the same conversation, new ports.
    w.board
        .move_ticket("SHOP-1", DropTarget::ReadyForDev, true, &me())
        .await
        .unwrap();
    w.starter
        .start("SHOP-1", &me(), otaq_core::session::Start::Auto)
        .await
        .unwrap();
    until_status(&w, "SHOP-1", Status::Working).await;
    let resumed = w.sessions.running_of(&t.id, SessionKind::Agent).unwrap();
    assert_ne!(resumed.id, first.id);
    let out = w.wait_for_output(&resumed.id, "ARGS").await;
    assert!(out.contains(&format!("--resume {conversation}")), "{out}");
    assert_eq!(
        resumed.tool_session_id.as_deref(),
        Some(conversation.as_str())
    );
    assert_eq!(
        (resumed.tool, resumed.model.as_deref()),
        (Some(otaq_core::agent_tools::Tool::Claude), None)
    );
    let row = w.board.store().ticket(&t.id).await.unwrap().unwrap();
    assert!(!row.parked);
    let back = w.board.ticket(&t.key.to_string()).await.unwrap();
    assert!(
        row.agent_port.is_some() && back.work.is_some_and(|w| w.app_port("app").is_some()),
        "new ports"
    );
    assert!(std::path::Path::new(&worktree).join("draft.ts").is_file());
    w.sessions.stop(&resumed.id).await.unwrap();
    w.stop().await;
}
