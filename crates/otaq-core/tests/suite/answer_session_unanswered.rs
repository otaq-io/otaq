//! A question whose answer session ended unanswered goes to Needs you (re-test 6, NEW-4): an answer session that is
//! killed, or ends its turn without `otaq answer`, puts its question in Needs you on the asking ticket (answer it
//! there, or Resume the answering ticket); Otaq never starts it again by itself. Its worktree goes when clean and is
//! kept for Cleanup otherwise. The same after a restart for a question whose answer session went with the service. An
//! answer session a pause stopped isn't this case: its question waits for the project's Resume. A worktree a live
//! ticket uses (the ticket was reopened while its answer session ran) is never removed.
//!
//! The answer session is the real resume path of a finished ticket (its worktree re-made, its conversation resumed
//! answer-only); an ordinary command stands in for `claude`, as in `answer_resume.rs`. Killing that command's process
//! is what a crashed or killed agent tool is to Otaq: its session ends by itself. A turn end is Claude Code's turn
//! events as the service applies a hook's (a new turn, then its end), which is what a real tool's turn end gives.

use std::path::{Path, PathBuf};
use std::time::Duration;

use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::agent_tools::claude::transcript::project_folder;
use otaq_core::agent_tools::{LaunchContext, Tool, ToolEvent};
use otaq_core::agents::pause::PauseScope;
use otaq_core::board::tickets::Ticket;
use otaq_core::caller::Caller;
use otaq_core::domain::{Actor, Status};
use otaq_core::git::testing::commit;
use otaq_core::messages::{Message, MessageState, Route};
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::TestService;
use otaq_core::session::SessionKind;
use otaq_core::signals::background::BackgroundWork;
use tempfile::TempDir;

const CONVERSATION: &str = "0199aaaa-bbbb-7ccc-8ddd-eeeeffff0002";

fn agent(key: &str) -> Actor {
    Actor::Agent(key.parse().unwrap())
}

fn me() -> Actor {
    Actor::User("me".into())
}

/// The stand-in for `claude`: prints its arguments, then waits.
fn launching(claude_dir: PathBuf) -> impl FnOnce(ServiceConfig) -> ServiceConfig {
    move |mut c| {
        c.launch = Some(LaunchContext {
            bin_dir: PathBuf::from("/nonexistent"),
            plugin_dir: PathBuf::from("/nonexistent/plugin"),
            path: std::env::var_os("PATH").unwrap_or_default(),
            claude: ClaudeLaunch {
                program: vec![
                    "sh".into(),
                    "-c".into(),
                    "echo \"answer-session: $*\"; exec cat".into(),
                    "sh".into(),
                ],
                config_dir: None,
            },
        });
        c.tools.claude_config_dir = Some(claude_dir);
        c
    }
}

struct World {
    w: TestService,
    claude: TempDir,
    parent: Ticket,
    child: Ticket,
    worktree: PathBuf,
}

/// SHOP-1 finished as `done` and cleaned up (its conversation on disk), SHOP-2 depending on it and working.
async fn world(done: Status) -> World {
    let claude = TempDir::new().unwrap();
    let w = TestService::start_with(launching(claude.path().to_path_buf())).await;
    w.add_project("shop-web").await;
    let parent = w.create("SHOP", "Checkout API").await;
    let child = w.create("SHOP", "Checkout page").await;
    w.board
        .add_dependency("SHOP-2", "SHOP-1", &me())
        .await
        .unwrap();
    let worktree = w.start_worktree(&parent).await;
    commit(&worktree, "api.ts", "checkout\n");
    w.record_ended_agent(&parent, CONVERSATION).await;
    let folder = claude
        .path()
        .join("projects")
        .join(project_folder(&worktree));
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join(format!("{CONVERSATION}.jsonl")), "{}\n").unwrap();
    w.force_status(&parent, done).await;
    let parent = w.board.ticket("SHOP-1").await.unwrap();
    w.review.finish(&parent).await.unwrap();
    assert!(!worktree.exists(), "Done cleanup removed it");
    w.force_status(&child, Status::Working).await;
    World {
        w,
        claude,
        parent,
        child,
        worktree,
    }
}

fn answer_session(w: &TestService, t: &Ticket) -> Option<String> {
    w.sessions
        .running_of(&t.id, SessionKind::Answer)
        .map(|s| s.id)
}

/// SHOP-2 asks SHOP-1, and the answer session starts.
async fn ask(w: &TestService, parent: &Ticket) -> (Message, String) {
    let asked = w
        .messages
        .ask(
            "SHOP-2",
            "SHOP-1",
            "Which status code on a declined card?",
            &agent("SHOP-2"),
        )
        .await
        .unwrap();
    assert_eq!(asked.route, Some(Route::Resumed));
    let session = answer_session(w, parent).expect("an answer session");
    w.wait_for_output(&session, "answer-session:").await;
    (asked, session)
}

/// Kills the answer session's processes, as a crash or a kill from outside would end the agent tool.
fn kill(w: &TestService, session: &str) {
    let pids: Vec<String> = w
        .sessions
        .agent_roots()
        .into_iter()
        .filter(|r| r.session == session)
        .map(|r| r.pid)
        .chain(w.sessions.process_tree(session).iter().map(|p| p.pid))
        .map(|pid| pid.to_string())
        .collect();
    assert!(!pids.is_empty(), "the answer session's processes");
    for pid in pids {
        #[cfg(unix)]
        let _ = std::process::Command::new("kill")
            .args(["-9", &pid])
            .status();
        #[cfg(windows)]
        let _ = std::process::Command::new("taskkill")
            .args(["/F", "/PID", &pid])
            .status();
    }
}

async fn until(what: &str, mut done: impl AsyncFnMut() -> bool) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    while !done().await {
        assert!(
            tokio::time::Instant::now() < deadline,
            "timed out waiting for {what}"
        );
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

async fn route(w: &TestService, id: &str) -> Option<Route> {
    w.board.message(id).await.unwrap().route
}

async fn until_needs_you(w: &TestService, id: &str) {
    until("the question in Needs you", async || {
        route(w, id).await == Some(Route::NeedsYou)
    })
    .await;
}

/// The asking ticket's Needs you items about questions: their reasons.
async fn asker_items(w: &TestService, child: &Ticket) -> Vec<String> {
    w.board
        .needs_you_items(i64::MAX / 2)
        .await
        .unwrap()
        .into_iter()
        .filter(|i| i.key == child.key && i.message.is_some())
        .map(|i| i.reason)
        .collect()
}

async fn listed(w: &TestService, t: &Ticket) -> bool {
    let scan = w.cleanup.scan().await.unwrap();
    scan.iter().any(|i| i.ticket.as_ref() == Some(&t.key))
}

/// Nothing starts the answer session again by itself: neither a routing pass nor the tick's.
async fn not_restarted(w: &TestService, parent: &Ticket) {
    w.messages.route_waiting().await.unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(answer_session(w, parent).is_none(), "never started again");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_killed_answer_session_puts_the_question_in_needs_you_and_removes_its_clean_worktree() {
    let World {
        w,
        claude: _claude,
        parent,
        child,
        worktree,
    } = world(Status::Merged).await;
    let (asked, session) = ask(&w, &parent).await;
    assert!(worktree.is_dir(), "re-made");

    kill(&w, &session);
    until_needs_you(&w, &asked.id).await;
    let m = w.board.message(&asked.id).await.unwrap();
    assert_eq!(m.state(), MessageState::NeedsYou);
    assert_eq!(m.answer, None);
    assert!(answer_session(&w, &parent).is_none());
    until("the clean worktree removed", async || !worktree.exists()).await;
    not_restarted(&w, &parent).await;
    assert_eq!(
        asker_items(&w, &child).await,
        ["asks SHOP-1 (Merged): Which status code on a declined card?"]
    );
    let asker = w.board.ticket("SHOP-2").await.unwrap();
    assert_eq!(asker.flags.asked, Some(parent.key.clone()), "still asked");
    let answering = w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(answering.flags.answering, None);
    assert!(!listed(&w, &parent).await, "nothing left for Cleanup");

    // Resume SHOP-1 from Needs you starts it again (the user's click), and its answer reaches the asker.
    let resumed = w.messages.resume_target(&asked.id).await.unwrap();
    assert_eq!(resumed.route, Some(Route::Resumed));
    let again = answer_session(&w, &parent).expect("resumed by the user");
    assert_ne!(again, session);
    w.messages
        .answer(&asked.id, "402.", &agent("SHOP-1"))
        .await
        .unwrap();
    assert!(answer_session(&w, &parent).is_none());
    assert!(asker_items(&w, &child).await.is_empty());
    assert!(!worktree.exists());
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_killed_answer_session_keeps_a_changed_worktree_for_cleanup() {
    let World {
        w,
        claude: _claude,
        parent,
        child,
        worktree,
    } = world(Status::Merged).await;
    let (asked, session) = ask(&w, &parent).await;
    std::fs::write(worktree.join("notes.md"), "scratch").unwrap();

    kill(&w, &session);
    until_needs_you(&w, &asked.id).await;
    until("the worktree listed in Cleanup", async || {
        listed(&w, &parent).await
    })
    .await;
    assert!(worktree.join("notes.md").exists(), "kept");
    not_restarted(&w, &parent).await;
    assert_eq!(asker_items(&w, &child).await.len(), 1);

    // Answered from Needs you by the user: the kept worktree stays for Cleanup.
    w.messages.answer(&asked.id, "402.", &me()).await.unwrap();
    assert!(asker_items(&w, &child).await.is_empty());
    assert!(worktree.join("notes.md").exists());
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_answer_session_that_ends_its_turn_without_answering_is_ended_and_goes_to_needs_you() {
    let World {
        w,
        claude: _claude,
        parent,
        child,
        worktree,
    } = world(Status::Merged).await;
    let (asked, session) = ask(&w, &parent).await;
    let caller = Caller::Agent {
        ticket: parent.key.clone(),
        ticket_id: parent.id.clone(),
        session: session.clone(),
    };
    let end = || ToolEvent::TurnEnded {
        background: BackgroundWork::default(),
        last_message: Some("I looked at the API.".into()),
    };

    // A turn end before the session's own turn started isn't one.
    w.tool_events(&caller, Tool::Claude, vec![end()]).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(
        answer_session(&w, &parent).as_deref(),
        Some(session.as_str())
    );
    assert_eq!(route(&w, &asked.id).await, Some(Route::Resumed));

    // Its turn: started, then ended without `otaq answer`.
    w.tool_events(
        &caller,
        Tool::Claude,
        vec![ToolEvent::NewTurn { prompt: None }],
    )
    .await;
    w.tool_events(&caller, Tool::Claude, vec![end()]).await;
    until_needs_you(&w, &asked.id).await;
    until("the answer session ended", async || {
        answer_session(&w, &parent).is_none()
    })
    .await;
    until("the clean worktree removed", async || !worktree.exists()).await;
    not_restarted(&w, &parent).await;
    assert_eq!(asker_items(&w, &child).await.len(), 1);
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn after_a_restart_a_question_whose_answer_session_is_gone_goes_to_needs_you() {
    let World {
        w,
        claude,
        parent,
        child,
        worktree,
    } = world(Status::Merged).await;
    let (asked, _session) = ask(&w, &parent).await;

    let w = w.restart_with(launching(claude.path().to_path_buf())).await;
    let m = w.board.message(&asked.id).await.unwrap();
    assert_eq!(m.route, Some(Route::NeedsYou), "at once, at startup");
    assert!(!worktree.exists(), "clean: removed");
    not_restarted(&w, &parent).await;
    assert_eq!(asker_items(&w, &child).await.len(), 1);
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_answer_session_a_pause_stopped_waits_for_the_projects_resume() {
    let World {
        w,
        claude,
        parent,
        child,
        worktree,
    } = world(Status::Merged).await;
    let (asked, session) = ask(&w, &parent).await;
    let shop = PauseScope::Project {
        project: "SHOP".into(),
    };
    w.pause.pause(&shop).await.unwrap();
    assert!(
        answer_session(&w, &parent).is_none(),
        "stopped by the pause"
    );
    until("the question waiting again", async || {
        route(&w, &asked.id).await.is_none()
    })
    .await;
    assert_eq!(
        w.board.message(&asked.id).await.unwrap().state(),
        MessageState::Asked
    );
    assert!(worktree.is_dir(), "kept for the Resume");
    assert!(asker_items(&w, &child).await.is_empty(), "not Needs you");

    // A restart while paused: it still waits.
    let w = w.restart_with(launching(claude.path().to_path_buf())).await;
    assert_eq!(route(&w, &asked.id).await, None);
    assert!(asker_items(&w, &child).await.is_empty());

    // The project's Resume carries on.
    w.pause.resume(&shop, &me()).await.unwrap();
    let again = answer_session(&w, &parent).expect("Resume routed it");
    assert_ne!(again, session);
    assert_eq!(route(&w, &asked.id).await, Some(Route::Resumed));
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_ticket_reopened_while_its_answer_session_runs_keeps_its_worktree() {
    let World {
        w,
        claude: _claude,
        parent,
        child,
        worktree,
    } = world(Status::ClosedNotMerged).await;
    let (asked, session) = ask(&w, &parent).await;
    w.board.reopen("SHOP-1", &me()).await.unwrap();
    assert_eq!(w.status("SHOP-1").await, Status::Backlog);

    // Killed: the question goes to Needs you, the worktree stays for the reopened ticket.
    kill(&w, &session);
    until_needs_you(&w, &asked.id).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(worktree.is_dir(), "the live ticket's worktree is kept");
    assert_eq!(asker_items(&w, &child).await.len(), 1);

    // Resumed by the user and answered: the worktree still stays.
    w.messages.resume_target(&asked.id).await.unwrap();
    assert!(answer_session(&w, &parent).is_some());
    w.messages
        .answer(&asked.id, "402.", &agent("SHOP-1"))
        .await
        .unwrap();
    assert!(answer_session(&w, &parent).is_none());
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(worktree.is_dir(), "still kept after the answer");
    assert!(Path::new(&worktree).join("api.ts").exists());
    w.stop().await;
}
