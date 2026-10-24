//! Resume tells the agent to carry on (T-0494; d0122, d0018, d0045): Resume, Resume all and Retry reopen the
//! conversation with "You were interrupted by a restart. Carry on where you left off; run `otaq prime` if you need
//! the ticket again." as its first prompt, passed when the tool starts (its resume), never typed. A hand-off that
//! never arrived (a Send back's skill) goes instead.
//!
//! Every tool is the stand-in of `tests/fixtures/stand-in` (`crate::common::stand_in`), which prints its arguments
//! at start. Repos, `OTAQ_HOME` and the tools' home are temporary: no test starts a real tool.

use std::path::PathBuf;
use std::time::Duration;

use crate::common::stand_in;
use otaq_core::agent_tools::{LaunchContext, Tool};
use otaq_core::agents::AgentModel;
use otaq_core::board::moves::DropTarget;
use otaq_core::board::tickets::Ticket;
use otaq_core::domain::{Actor, Status};
use otaq_core::git::testing::commit;
use otaq_core::recovery::carry_on_text;
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::{TestService, said};
use otaq_core::session::{SessionInfo, SessionKind, Start};
use tempfile::TempDir;

const TOOLS: [Tool; 4] = [Tool::Claude, Tool::Codex, Tool::Copilot, Tool::OpenCode];

fn me() -> Actor {
    Actor::User("me".into())
}

fn with(launch: &LaunchContext) -> impl FnOnce(ServiceConfig) -> ServiceConfig {
    let launch = launch.clone();
    move |config| ServiceConfig {
        launch: Some(launch),
        ..config
    }
}

/// A service whose four tools are the stand-in, with shop-web (a local origin).
async fn world(dir: &TempDir) -> (TestService, LaunchContext) {
    let launch = stand_in::launch(&dir.path().join("bin"), dir.path());
    let w = TestService::start_with(with(&launch)).await;
    let repo = w.repos.clone_with_origin("shop-web", "main", 1);
    w.board
        .add_project(&otaq_core::board::projects::AddProject {
            path: repo.display().to_string(),
            ..Default::default()
        })
        .await
        .unwrap();
    (w, launch)
}

async fn running(w: &TestService, t: &Ticket) -> SessionInfo {
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            if let Some(s) = w.sessions.running_of(&t.id, SessionKind::Agent) {
                return s;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("{}'s agent never ran", t.key))
}

/// Starts a new ticket on `tool` from Ready for Dev; returns it once the stand-in printed its arguments.
async fn started(w: &TestService, tool: Tool, title: &str) -> (Ticket, SessionInfo) {
    w.set_agent_model("Developer", AgentModel::Tool { tool, model: None })
        .await;
    let t = w.create("SHOP", title).await;
    let key = t.key.to_string();
    w.board
        .move_ticket(&key, DropTarget::ReadyForDev, true, &me())
        .await
        .unwrap();
    w.starter.start(&key, &me(), Start::Auto).await.unwrap();
    let mut session = running(w, &t).await;
    w.wait_for_output(&session.id, "ARGS").await;
    if session.tool_session_id.is_none() {
        // Codex's id arrives after the start (its `SessionStart` hook), which the stand-in doesn't send.
        let id = format!("0199c0de-0000-7000-8000-{:012}", t.key.number);
        assert!(w.signals.tool_id_known(&session.id, &id).await.unwrap());
        session = w.sessions.info(&session.id).unwrap();
    }
    (w.board.ticket(&key).await.unwrap(), session)
}

/// The `ARGS …` line the stand-in printed at start.
async fn args(w: &TestService, session: &SessionInfo) -> String {
    let out = w.wait_for_output(&session.id, "ARGS").await;
    out.lines()
        .find(|l| l.starts_with("ARGS"))
        .unwrap()
        .to_owned()
}

/// Sent back while its session was gone: the resumed conversation got `address-review` as its first prompt, and
/// the restart came before the agent showed it got it, so the hand-off is still waiting.
async fn sent_back_then_gone(w: &TestService, t: &Ticket, session: &SessionInfo) {
    let worktree = PathBuf::from(t.work.as_ref().unwrap().worktree.clone().unwrap());
    commit(&worktree, "login.ts", "redirect\n");
    w.force_status(t, Status::Working).await;
    let key = t.key.to_string();
    w.board
        .submit_for_review(
            &key,
            said(&w.board, &key, "Redirects after login.")
                .await
                .as_ref(),
            &me(),
        )
        .await
        .unwrap();
    w.sessions.stop(&session.id).await.unwrap();
    w.review
        .fail(&key, "Keep the old URL working.", None, false, &me())
        .await
        .unwrap();
    let resumed = running(w, t).await;
    assert!(args(w, &resumed).await.contains("address-review"));
}

/// After a restart, Resume all on each tool: a ticket with nothing waiting reopens its conversation with "Carry
/// on…", one whose Send back never arrived with the hand-off instead (and no "Carry on").
#[tokio::test(flavor = "multi_thread")]
async fn resume_all_tells_each_tool_to_carry_on_unless_a_handoff_waits() {
    let dir = TempDir::new().unwrap();
    let (w, launch) = world(&dir).await;
    let mut plain = Vec::new();
    let mut waiting = Vec::new();
    for (n, tool) in TOOLS.into_iter().enumerate() {
        let (t, s) = started(&w, tool, &format!("Login redirect {n}")).await;
        plain.push((tool, t, s.tool_session_id.clone().unwrap()));
        let (t, s) = started(&w, tool, &format!("Logout redirect {n}")).await;
        sent_back_then_gone(&w, &t, &s).await;
        waiting.push((tool, t, s.tool_session_id.clone().unwrap()));
    }
    let w = w.restart_with(with(&launch)).await;
    for (tool, t, _) in plain.iter().chain(&waiting) {
        assert_eq!(
            w.status(&t.key.to_string()).await,
            Status::Interrupted,
            "{tool:?}"
        );
    }

    let resumed = w.review.resume_all(Some("SHOP"), &me()).await.unwrap();
    assert_eq!(resumed.len(), 8, "{resumed:?}");
    let carry_on = carry_on_text();
    for (tool, t, conversation) in &plain {
        let args = args(&w, &running(&w, t).await).await;
        assert!(args.contains(conversation.as_str()), "{tool:?}: {args}");
        assert!(args.contains(&carry_on), "{tool:?}: {args}");
    }
    for (tool, t, conversation) in &waiting {
        let args = args(&w, &running(&w, t).await).await;
        assert!(args.contains(conversation.as_str()), "{tool:?}: {args}");
        assert!(args.contains("address-review"), "{tool:?}: {args}");
        assert!(!args.contains("Carry on"), "{tool:?}: {args}");
    }
    w.stop().await;
}

/// Resume on one ticket and Retry after a failed resume pass the same first prompt; Start from scratch begins with
/// the ticket's own skill, without it.
#[tokio::test(flavor = "multi_thread")]
async fn resume_and_retry_carry_on_and_start_from_scratch_does_not() {
    let dir = TempDir::new().unwrap();
    let (w, launch) = world(&dir).await;
    let (t, first) = started(&w, Tool::Claude, "Login redirect").await;
    let conversation = first.tool_session_id.clone().unwrap();
    let key = t.key.to_string();
    let w = w.restart_with(with(&launch)).await;
    assert_eq!(w.status(&key).await, Status::Interrupted);
    let carry_on = carry_on_text();
    assert_eq!(
        carry_on,
        "You were interrupted by a restart. Carry on where you left off; run `otaq prime` if you need the \
         ticket again."
    );

    w.review.resume(&key, &me()).await.unwrap();
    let resumed = running(&w, &t).await;
    let a = args(&w, &resumed).await;
    assert!(
        a.contains(&format!("--resume {conversation} {carry_on}")),
        "{a}"
    );

    // The tool said it couldn't find the conversation: Resume failed, and Retry passes it again.
    w.sessions.stop(&resumed.id).await.unwrap();
    w.force_status(&t, Status::ResumeFailed).await;
    w.review.retry_resume(&key, &me()).await.unwrap();
    let retried = running(&w, &t).await;
    assert_ne!(retried.id, resumed.id);
    let a = args(&w, &retried).await;
    assert!(
        a.contains(&format!("--resume {conversation} {carry_on}")),
        "{a}"
    );

    w.sessions.stop(&retried.id).await.unwrap();
    w.force_status(&t, Status::ResumeFailed).await;
    w.review
        .start_from_scratch(&key, true, &me())
        .await
        .unwrap();
    let fresh = running(&w, &t).await;
    let a = args(&w, &fresh).await;
    assert!(!a.contains("--resume"), "{a}");
    assert!(a.contains("/otaq-work-ticket"), "{a}");
    assert!(!a.contains("Carry on"), "{a}");
    w.stop().await;
}
