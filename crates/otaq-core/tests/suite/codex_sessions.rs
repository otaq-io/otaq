//! A Codex agent's id, captured after start (T-0349; ADP-07, ADP-04, SES-03; d0088 "Session ids"): the session
//! stores none until the tool reports it; Park waits briefly for it (with it, the same conversation resumes; without
//! it, the ticket is parked anyway and its next start is fresh); an agent that dies before its id is a failed start
//! with Retry start. A stand-in `codex` (an `sh` script in Otaq's bin folder, first on the agent's PATH) prints its
//! arguments: no test starts codex.
//!
//! Unix only: the stand-in agent tools are `sh` scripts started by name, which Windows can't run (Q598).
#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::agent_tools::{LaunchContext, Tool};
use otaq_core::agents::AgentModel;
use otaq_core::board::moves::DropTarget;
use otaq_core::domain::{Actor, StartStep, Status};
use otaq_core::review::park::TOOL_ID_WAIT;
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::TestService;
use otaq_core::session::{SessionKind, StoredConversation};
use tempfile::TempDir;

/// Prints its arguments, then waits.
const WAITS: &str = "printf 'ARGS %s\\n' \"$*\"; exec cat";

/// Prints its arguments, then exits a moment later (before it says its id).
const DIES: &str = "printf 'ARGS %s\\n' \"$*\"; sleep 1; exit 1";

const ID: &str = "01a14bae-dd63-7c03-b96e-8f7557586707";

fn me() -> Actor {
    Actor::User("me".into())
}

struct World {
    w: TestService,
    _bin: TempDir,
}

/// A service whose `codex` is `script`, with project SHOP and ticket SHOP-1, whose Developer agent runs on Codex
/// (the tool default), in Ready for Dev.
async fn world(script: &str) -> World {
    let bin = TempDir::new().unwrap();
    let codex = bin.path().join(Tool::Codex.command_name());
    std::fs::write(&codex, format!("#!/bin/sh\n{script}\n")).unwrap();
    std::fs::set_permissions(&codex, std::fs::Permissions::from_mode(0o755)).unwrap();
    let bin_dir: PathBuf = bin.path().to_owned();
    let w = TestService::start_with(move |config| ServiceConfig {
        launch: Some(LaunchContext {
            bin_dir,
            plugin_dir: "/nonexistent/plugin".into(),
            path: std::env::var_os("PATH").unwrap_or_default(),
            claude: ClaudeLaunch {
                program: vec!["false".into()],
                config_dir: None,
            },
        }),
        ..config
    })
    .await;
    w.add_project("shop-web").await;
    w.create("SHOP", "Coupon codes").await;
    // Developer (the Dev stage's agent) runs on Codex.
    w.set_agent_model(
        "Developer",
        AgentModel::Tool {
            tool: Tool::Codex,
            model: None,
        },
    )
    .await;
    w.board
        .move_ticket("SHOP-1", DropTarget::ReadyForDev, true, &me())
        .await
        .unwrap();
    World { w, _bin: bin }
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

/// Starts SHOP-1 and returns its running agent session's id, once Codex printed its arguments.
async fn started(w: &TestService) -> (String, String) {
    w.starter
        .start("SHOP-1", &me(), otaq_core::session::Start::Auto)
        .await
        .unwrap();
    until_status(w, "SHOP-1", Status::Working).await;
    let ticket = w.board.ticket("SHOP-1").await.unwrap();
    let session = w
        .sessions
        .running_of(&ticket.id, SessionKind::Agent)
        .unwrap();
    assert_eq!(session.tool, Some(Tool::Codex));
    assert_eq!(session.tool_session_id, None, "captured after start");
    let out = w.wait_for_output(&session.id, "ARGS").await;
    assert!(out.contains("--no-daemon"), "{out}");
    (ticket.id, session.id)
}

/// The id arrives while Park waits: it is stored before the agent stops, and back in Dev the same conversation
/// resumes (`codex … resume <id>`).
#[tokio::test(flavor = "multi_thread")]
async fn park_waits_for_the_id_and_the_conversation_resumes() {
    let World { w, _bin } = world(WAITS).await;
    let (ticket_id, session) = started(&w).await;

    let signals = w.signals.clone();
    let reporting = session.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(400)).await;
        assert!(signals.tool_id_known(&reporting, ID).await.unwrap());
    });
    let began = Instant::now();
    let parked = w.review.park("SHOP-1", true, &me()).await.unwrap();
    assert!(
        began.elapsed() < TOOL_ID_WAIT,
        "it didn't wait out the limit"
    );
    assert_eq!(parked.status, Status::Parked);
    let ended = w.sessions.info(&session).unwrap();
    assert!(!ended.is_running());
    assert_eq!(ended.tool_session_id.as_deref(), Some(ID));
    let stored = StoredConversation::latest_agent(w.board.store().pool(), &ticket_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!((stored.tool, stored.id.as_str()), (Tool::Codex, ID));

    w.board
        .move_ticket("SHOP-1", DropTarget::ReadyForDev, true, &me())
        .await
        .unwrap();
    w.starter
        .start("SHOP-1", &me(), otaq_core::session::Start::Auto)
        .await
        .unwrap();
    until_status(&w, "SHOP-1", Status::Working).await;
    let resumed = w
        .sessions
        .running_of(&ticket_id, SessionKind::Agent)
        .unwrap();
    let out = w.wait_for_output(&resumed.id, "ARGS").await;
    assert!(out.contains(&format!("resume {ID}")), "{out}");
    assert_eq!(resumed.tool_session_id.as_deref(), Some(ID));
    w.sessions.stop(&resumed.id).await.unwrap();
    w.stop().await;
}

/// No id within the wait: Park stops the agent anyway, and the ticket's next start is a new conversation.
#[tokio::test(flavor = "multi_thread")]
async fn park_without_an_id_waits_then_stops_and_the_next_start_is_fresh() {
    let World { w, _bin } = world(WAITS).await;
    let (ticket_id, session) = started(&w).await;

    let began = Instant::now();
    let parked = w.review.park("SHOP-1", true, &me()).await.unwrap();
    assert!(began.elapsed() >= TOOL_ID_WAIT, "it waited for the id");
    assert_eq!(parked.status, Status::Parked);
    assert!(!w.sessions.info(&session).unwrap().is_running());
    assert_eq!(
        StoredConversation::latest_agent(w.board.store().pool(), &ticket_id)
            .await
            .unwrap(),
        None
    );

    w.board
        .move_ticket("SHOP-1", DropTarget::ReadyForDev, true, &me())
        .await
        .unwrap();
    w.starter
        .start("SHOP-1", &me(), otaq_core::session::Start::Auto)
        .await
        .unwrap();
    until_status(&w, "SHOP-1", Status::Working).await;
    let fresh = w
        .sessions
        .running_of(&ticket_id, SessionKind::Agent)
        .unwrap();
    let out = w.wait_for_output(&fresh.id, "ARGS").await;
    assert!(!out.contains("resume"), "{out}");
    w.sessions.stop(&fresh.id).await.unwrap();
    w.stop().await;
}

/// The agent exits before Codex said its id: nothing to resume, so the start failed, with Retry start; it didn't
/// start, so the Developer row stays editable (Q130).
#[tokio::test(flavor = "multi_thread")]
async fn an_agent_dying_before_its_id_is_a_failed_start() {
    let World { w, _bin } = world(DIES).await;
    let (ticket_id, _) = started(&w).await;
    until_status(&w, "SHOP-1", Status::StartInterrupted).await;
    assert!(
        !w.board.store().developer_started(&ticket_id).await.unwrap(),
        "not started: the Developer row unlocks"
    );
    let ticket = w.board.ticket("SHOP-1").await.unwrap();
    let problem = ticket.start_problem.unwrap();
    assert_eq!(problem.step, StartStep::Launch);
    assert_eq!(
        problem.message,
        "Codex stopped before its conversation started, so there is nothing to resume."
    );
    w.starter.retry("SHOP-1", &me()).await.unwrap();
    assert_eq!(w.status("SHOP-1").await, Status::Queued);
    w.stop().await;
}
