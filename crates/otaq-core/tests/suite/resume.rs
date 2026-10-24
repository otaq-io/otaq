//! Resume, Resume all, Resume failed, Start from scratch (T-0112, REC-01, REC-03; d0018, d0045). `claude` is an
//! `sh` that prints its arguments; for the failing case it prints the CLI's "No conversation found" message and
//! exits when asked to `--resume`.

use std::time::Duration;

use otaq_core::agent_tools::LaunchContext;
use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::board::Refusal;
use otaq_core::board::moves::DropTarget;
use otaq_core::board::tickets::EventKind;
use otaq_core::domain::{Actor, Status};
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::TestService;
use otaq_core::session::{SessionKind, SessionTicket};

fn me() -> Actor {
    Actor::User("me".into())
}

/// Keeps running, printing its arguments.
const KEEPS_RUNNING: &str = "printf 'ARGS %s\\n' \"$*\"; exec sleep 30";
/// Like `claude --resume <id>` for a conversation that is gone: the message, then exit 1. New sessions run.
const CONVERSATION_GONE: &str = "printf 'ARGS %s\\n' \"$*\"; case \"$*\" in *--resume*) \
     echo 'No conversation found with session ID: 0199-gone'; exit 1;; esac; exec sleep 30";

fn launch(script: &str) -> impl FnOnce(ServiceConfig) -> ServiceConfig {
    let ctx = LaunchContext {
        bin_dir: "/nonexistent".into(),
        plugin_dir: "/nonexistent/plugin".into(),
        path: std::env::var_os("PATH").unwrap_or_default(),
        claude: ClaudeLaunch {
            program: vec!["sh".into(), "-c".into(), script.into(), "sh".into()],
            config_dir: None,
        },
    };
    move |config| ServiceConfig {
        launch: Some(ctx),
        ..config
    }
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

/// SHOP-1 working with a session, then interrupted by a restart.
async fn interrupted(script: &'static str) -> TestService {
    let w = TestService::start_with(launch(script)).await;
    w.add_project("shop-web").await;
    w.create("SHOP", "Upgrade to Next.js 15").await;
    w.board
        .move_ticket("SHOP-1", DropTarget::ReadyForDev, true, &me())
        .await
        .unwrap();
    w.starter
        .start("SHOP-1", &me(), otaq_core::session::Start::Auto)
        .await
        .unwrap();
    until_status(&w, "SHOP-1", Status::Working).await;
    let w = w.restart_with(launch(script)).await;
    assert_eq!(w.status("SHOP-1").await, Status::Interrupted);
    w
}

async fn sessions(w: &TestService, key: &str) -> Vec<otaq_core::session::SessionInfo> {
    let t = w.board.ticket(key).await.unwrap();
    w.sessions
        .history(&SessionTicket {
            id: t.id,
            key: t.key,
        })
        .await
        .unwrap()
}

async fn events(w: &TestService, key: &str, kind: EventKind) -> Vec<serde_json::Value> {
    w.board
        .history(key)
        .await
        .unwrap()
        .into_iter()
        .filter(|e| e.kind == kind)
        .map(|e| e.payload)
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn resume_continues_the_stored_conversation_in_a_new_session() {
    let w = interrupted(KEEPS_RUNNING).await;
    let before = sessions(&w, "SHOP-1").await;
    let conversation = before[0].tool_session_id.clone().unwrap();
    let worktree = w
        .board
        .ticket("SHOP-1")
        .await
        .unwrap()
        .work
        .unwrap()
        .worktree
        .unwrap();

    let t = w.review.resume("SHOP-1", &me()).await.unwrap();
    assert_eq!(t.status, Status::Working);
    let after = sessions(&w, "SHOP-1").await;
    assert_eq!(after.len(), 2, "every session stays in History");
    let resumed = &after[0];
    assert!(resumed.is_running());
    assert_eq!(
        resumed.tool_session_id.as_deref(),
        Some(conversation.as_str())
    );
    assert_eq!(
        (resumed.tool, resumed.model.as_deref()),
        (Some(otaq_core::agent_tools::Tool::Claude), None)
    );
    let out = w.wait_for_output(&resumed.id, "ARGS").await;
    assert!(out.contains(&format!("--resume {conversation}")), "{out}");
    let t = w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(t.work.unwrap().worktree.unwrap(), worktree, "same worktree");
    assert!(!t.flags.resuming_conversation, "the start ran");
    assert_eq!(events(&w, "SHOP-1", EventKind::Resumed).await.len(), 1);

    // Only Interrupted tickets resume.
    let err = w.review.resume("SHOP-1", &me()).await.unwrap_err();
    assert_eq!(err.refusal(), Some(Refusal::MoveRefused));
    w.sessions.stop(&resumed.id).await.unwrap();
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_missing_conversation_is_resume_failed_then_retry_or_start_from_scratch() {
    let w = interrupted(CONVERSATION_GONE).await;
    let old = sessions(&w, "SHOP-1").await[0]
        .tool_session_id
        .clone()
        .unwrap();

    w.review.resume("SHOP-1", &me()).await.unwrap();
    until_status(&w, "SHOP-1", Status::ResumeFailed).await;
    let failed = events(&w, "SHOP-1", EventKind::ResumeFailed).await;
    assert_eq!(failed[0]["tool_session_id"], old.as_str());
    assert_eq!(failed[0]["tool"], "claude");
    assert!(
        events(&w, "SHOP-1", EventKind::Interrupted)
            .await
            .iter()
            .all(|p| p["reason"] == "restart"),
        "never treated as an ordinary exit"
    );
    // Nothing falls back on its own.
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(w.status("SHOP-1").await, Status::ResumeFailed);
    assert_eq!(sessions(&w, "SHOP-1").await.len(), 2);

    // Retry runs the same resume again.
    w.review.retry_resume("SHOP-1", &me()).await.unwrap();
    until_status(&w, "SHOP-1", Status::ResumeFailed).await;
    let all = sessions(&w, "SHOP-1").await;
    assert_eq!(all.len(), 3);
    assert_eq!(all[0].tool_session_id.as_deref(), Some(old.as_str()));

    // Start from scratch asks first, then starts a new conversation in the same worktree.
    let err = w
        .review
        .start_from_scratch("SHOP-1", false, &me())
        .await
        .unwrap_err();
    assert_eq!(err.refusal(), Some(Refusal::NeedsConfirmation));
    assert!(
        err.to_string()
            .contains("branch, commits and files are kept")
    );
    let t = w
        .review
        .start_from_scratch("SHOP-1", true, &me())
        .await
        .unwrap();
    assert_eq!(t.status, Status::Working);
    let all = sessions(&w, "SHOP-1").await;
    assert_eq!(all.len(), 4, "every session id stays");
    let fresh = &all[0];
    let new_id = fresh.tool_session_id.clone().unwrap();
    assert_ne!(new_id, old);
    let out = w.wait_for_output(&fresh.id, "ARGS").await;
    assert!(out.contains(&format!("--session-id {new_id}")), "{out}");
    let scratch = events(&w, "SHOP-1", EventKind::StartedFromScratch).await;
    assert_eq!(scratch[0]["old_tool_session_id"], old.as_str());
    assert_eq!(scratch[0]["tool"], "claude");
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(w.status("SHOP-1").await, Status::Working);

    // Retry follows a failed resume; Start from scratch an interrupted session; neither a working agent.
    let err = w.review.retry_resume("SHOP-1", &me()).await.unwrap_err();
    assert_eq!(err.refusal(), Some(Refusal::MoveRefused));
    let err = w
        .review
        .start_from_scratch("SHOP-1", true, &me())
        .await
        .unwrap_err();
    assert_eq!(err.refusal(), Some(Refusal::MoveRefused));
    w.sessions.stop(&fresh.id).await.unwrap();
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_interrupted_ticket_whose_conversation_is_gone_can_start_from_scratch() {
    let w = interrupted(KEEPS_RUNNING).await;
    let t = w
        .review
        .start_from_scratch("SHOP-1", true, &me())
        .await
        .unwrap();
    assert_eq!(t.status, Status::Working);
    assert_eq!(
        events(&w, "SHOP-1", EventKind::StartedFromScratch)
            .await
            .len(),
        1
    );
    let fresh = &sessions(&w, "SHOP-1").await[0];
    let out = w.wait_for_output(&fresh.id, "ARGS").await;
    assert!(out.contains("--session-id"), "{out}");
    w.sessions.stop(&fresh.id).await.unwrap();
    w.stop().await;
}

/// The first prompt `claude` got after `--resume <id>` (empty for none), from the stand-in's "ARGS" line.
fn first_prompt(out: &str, conversation: &str) -> String {
    // A Windows console starts its output with control sequences: the line is read from "ARGS" on.
    let line = out
        .lines()
        .find_map(|l| l.find("ARGS").map(|at| &l[at..]))
        .unwrap_or_default();
    line.split(&format!("--resume {conversation}"))
        .nth(1)
        .unwrap_or_default()
        .trim()
        .to_owned()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_send_back_whose_resume_failed_gets_its_skill_on_retry() {
    let w = TestService::start_with(launch(CONVERSATION_GONE)).await;
    w.add_project("shop-web").await;
    let t = w.create("SHOP", "Search results paging").await;
    w.start_worktree(&t).await;
    w.force_status(&t, Status::WaitingForYou).await;
    w.record_ended_agent(&t, "0199a1b2-c3d4-7e5f-8a9b-0c1d2e3f4a5b")
        .await;
    let conversation = "0199a1b2-c3d4-7e5f-8a9b-0c1d2e3f4a5b";
    let skill = format!("/{}", otaq_core::naming::copy_name("address-review"));

    // Send back resumes the ended conversation with the skill; the conversation is gone.
    w.review
        .fail("SHOP-1", "", None, false, &me())
        .await
        .unwrap();
    until_status(&w, "SHOP-1", Status::ResumeFailed).await;
    let row = w.board.store().ticket(&t.id).await.unwrap().unwrap();
    assert_eq!(
        row.resume_skill.as_deref(),
        Some("address-review"),
        "never ran"
    );

    // Retry resumes again with the skill and what came back (here it fails again: the stand-in always does).
    w.review.retry_resume("SHOP-1", &me()).await.unwrap();
    until_status(&w, "SHOP-1", Status::ResumeFailed).await;
    let latest = &sessions(&w, "SHOP-1").await[0];
    let out =
        String::from_utf8_lossy(&w.sessions.attach(&latest.id).unwrap().scrollback).into_owned();
    assert_eq!(
        first_prompt(&out, conversation),
        format!("{skill} SHOP-1 was sent back to Dev. Run `otaq prime`."),
        "{out}"
    );

    // Start from scratch begins with the ticket prompt (d0018): the skill is dropped (Q25).
    w.review
        .start_from_scratch("SHOP-1", true, &me())
        .await
        .unwrap();
    let row = w.board.store().ticket(&t.id).await.unwrap().unwrap();
    assert_eq!((row.resume_skill, row.resume_prompt), (None, None));
    let fresh = &sessions(&w, "SHOP-1").await[0];
    let out = w.wait_for_output(&fresh.id, "ARGS").await;
    assert!(!out.contains("address-review"), "{out}");
    w.sessions.stop(&fresh.id).await.unwrap();
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_inbox_answer_whose_resume_failed_is_delivered_on_retry() {
    let w = interrupted(CONVERSATION_GONE).await;
    let t = w.board.ticket("SHOP-1").await.unwrap();
    let conversation = sessions(&w, "SHOP-1").await[0]
        .tool_session_id
        .clone()
        .unwrap();
    // The ticket was waiting for an answer to its open question when the restart interrupted it.
    w.board
        .store_question(&t.id, "Which forms get the new field?", &[], &[], None)
        .await
        .unwrap();
    sqlx::query("UPDATE tickets SET interrupted_from = 'asks_you' WHERE id = ?")
        .bind(&t.id)
        .execute(w.board.store().pool())
        .await
        .unwrap();
    w.inbox
        .answer(
            "SHOP-1",
            &otaq_core::board::questions::Choice::Other("Only the profile form".into()),
            false,
            &me(),
        )
        .await
        .unwrap();
    until_status(&w, "SHOP-1", Status::ResumeFailed).await;
    let row = w.board.store().ticket(&t.id).await.unwrap().unwrap();
    assert_eq!(row.resume_prompt.as_deref(), Some("Only the profile form"));

    w.review.retry_resume("SHOP-1", &me()).await.unwrap();
    until_status(&w, "SHOP-1", Status::ResumeFailed).await;
    let latest = &sessions(&w, "SHOP-1").await[0];
    let out =
        String::from_utf8_lossy(&w.sessions.attach(&latest.id).unwrap().scrollback).into_owned();
    assert_eq!(
        first_prompt(&out, &conversation),
        "Only the profile form",
        "{out}"
    );
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_ordinary_exit_after_a_resume_is_interrupted_not_resume_failed() {
    let w = interrupted(
        "printf 'ARGS %s\\n' \"$*\"; case \"$*\" in *--resume*) exit 2;; esac; exec sleep 30",
    )
    .await;
    w.review.resume("SHOP-1", &me()).await.unwrap();
    until_status(&w, "SHOP-1", Status::Interrupted).await;
    let interrupted = events(&w, "SHOP-1", EventKind::Interrupted).await;
    assert_eq!(interrupted[0]["reason"], "exit 2");
    assert!(
        events(&w, "SHOP-1", EventKind::ResumeFailed)
            .await
            .is_empty()
    );
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn resume_all_takes_exactly_the_interrupted_tickets() {
    let w = TestService::start_with(launch(KEEPS_RUNNING)).await;
    w.add_project("shop-web").await;
    w.add_project("billing-api").await;
    let rows = [
        ("SHOP", "a", Status::Interrupted),
        ("SHOP", "b", Status::StartInterrupted),
        ("SHOP", "c", Status::ResumeFailed),
        ("SHOP", "d", Status::WaitingForYou),
        ("SHOP", "e", Status::PrOpen),
        ("SHOP", "f", Status::Working),
        ("BILL", "g", Status::Interrupted),
    ];
    for (project, title, status) in rows {
        let t = w.create(project, title).await;
        w.start_worktree(&t).await;
        w.force_status(&t, status).await;
    }
    let shop = w.board.tickets("SHOP").await.unwrap();
    let eligible: Vec<String> = otaq_core::recovery::resume_all_eligible(&shop)
        .iter()
        .map(ToString::to_string)
        .collect();
    assert_eq!(eligible, ["SHOP-1"]);

    let resumed = w.review.resume_all(None, &me()).await.unwrap();
    let resumed: Vec<String> = resumed.iter().map(ToString::to_string).collect();
    assert_eq!(resumed, ["BILL-1", "SHOP-1"]);
    for (key, want) in [
        ("SHOP-1", Status::Working),
        ("SHOP-2", Status::StartInterrupted),
        ("SHOP-3", Status::ResumeFailed),
        ("SHOP-4", Status::WaitingForYou),
        ("SHOP-5", Status::PrOpen),
        ("BILL-1", Status::Working),
    ] {
        assert_eq!(w.status(key).await, want, "{key}");
    }
    // One project only.
    assert!(
        w.review
            .resume_all(Some("SHOP"), &me())
            .await
            .unwrap()
            .is_empty()
    );
    for key in ["SHOP-1", "BILL-1"] {
        let t = w.board.ticket(key).await.unwrap();
        if let Some(s) = w.sessions.running_of(&t.id, SessionKind::Agent) {
            w.sessions.stop(&s.id).await.unwrap();
        }
    }
    w.stop().await;
}
