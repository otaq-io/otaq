//! An agent's question while its ticket waits in a check of the user's (re-test 8 NEW-b2): `otaq ask human` on a
//! ticket "Ready for you" in Review reads Asks you on the card, the pill, the strip and in notifications, with its box
//! on the Overview, while the ticket keeps its column and status (Approve, Send back, Move and dragging stay). The
//! answer reaches the agent that asked and the ticket reads "Ready for you" again; Send back closes the question, and
//! an open question doesn't hold Asks you once the ticket is back in Dev. The message writer's permission prompt keeps
//! its calm in Review: a question open beside it reads Asks you, never Needs you.
//!
//! The question is asked as the real `otaq ask human` asks it: the `ticket.ask_human` request sent from inside the
//! agent's session (an `sh` with the agent's environment, so the service judges it as the ticket's agent). The answer
//! is typed into that session as the user's click, as into any tool's terminal; what the session prints back is the
//! terminal's echo of it. The writer's prompt is Codex's recorded `PermissionRequest` hook
//! (`tests/fixtures/codex/hooks/PermissionRequest.json`), as in the writer's own tests.

use std::path::Path;

use otaq_core::agent_tools::Tool;
use otaq_core::attention::notify;
use otaq_core::board::Refusal;
use otaq_core::board::needs_you::NeedsYouGroup;
use otaq_core::board::questions::Choice;
use otaq_core::board::tickets::Ticket;
use otaq_core::caller::Caller;
use otaq_core::client::{Client, ClientConfig};
use otaq_core::domain::{Actor, Place, Status};
use otaq_core::git::testing::commit;
use otaq_core::protocol::{NeedsYouAnswerParams, Reply, Request};
use otaq_core::service::testing::{AgentShell, TestService};
use otaq_core::session::deliveries::Delivery;
use serde_json::{Value, json};

const QUESTION: &str = "R9 b2: asked from Review, should this show?";

fn me() -> Actor {
    Actor::User("me".into())
}

/// SHOP-1 waiting in Review ("Ready for you") with a commit of its own, its agent's session still running.
async fn in_review(w: &TestService) -> (Ticket, AgentShell) {
    w.signals.set_stage_reminders(false);
    w.add_project("shop-web").await;
    let t = w.create("SHOP", "Pricing").await;
    let worktree = w.start_worktree(&t).await;
    commit(&worktree, "login.ts", "redirect\n");
    w.force_status(&t, Status::WaitingForYou).await;
    let shell = w.agent_shell(&t).await;
    let t = w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(t.status, Status::WaitingForYou);
    assert_eq!(t.place, Place::stage_of("review"));
    (t, shell)
}

/// One request from inside the session, as `otaq` sends it there.
async fn send(w: &TestService, shell: &AgentShell, method: &str, params: Value) -> Value {
    let request = json!({"protocol_version": otaq_core::protocol::PROTOCOL_VERSION, "id": 1, "request": {"method": method, "params": params}});
    let ran = shell
        .send_raw(&w.data.socket_path(), &request.to_string())
        .await;
    serde_json::from_str(ran.stdout.lines().next().unwrap_or_default())
        .unwrap_or_else(|e| panic!("{e}: {ran:?}"))
}

/// `otaq ask human --option Yes --option No` from the ticket's agent.
async fn asks(w: &TestService, shell: &AgentShell, t: &Ticket) {
    let asked = send(
        w,
        shell,
        "ticket.ask_human",
        json!({"key": t.key, "question": QUESTION, "options": ["Yes", "No"]}),
    )
    .await;
    assert!(asked.get("error").is_none(), "{asked}");
}

/// The ticket's Needs you items: (group, status, reason).
async fn items(w: &TestService, t: &Ticket) -> Vec<(NeedsYouGroup, Status, String)> {
    w.board
        .needs_you_items(0)
        .await
        .unwrap()
        .into_iter()
        .filter(|i| i.key == t.key)
        .map(|i| (i.group, i.status, i.reason))
        .collect()
}

async fn answer(w: &TestService, choice: Choice) -> Delivery {
    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    let reply = client
        .request(Request::NeedsYouAnswer(NeedsYouAnswerParams {
            key: "SHOP-1".into(),
            choice,
            clear: false,
        }))
        .await
        .unwrap();
    let Reply::Delivery(d) = reply else {
        panic!("{reply:?}")
    };
    d
}

/// `answered_via` of the ticket's latest question.
async fn answered_via(w: &TestService, t: &Ticket) -> Option<String> {
    sqlx::query_scalar(
        "SELECT answered_via FROM human_questions WHERE ticket_id = ? ORDER BY asked_at DESC LIMIT 1",
    )
    .bind(&t.id)
    .fetch_one(w.board.store().pool())
    .await
    .unwrap()
}

/// Asked in Review: Asks you on the card, the pill, the strip and in notifications, the ticket keeping its column and
/// status; the answer reaches the agent and the ticket reads "Ready for you" again.
#[tokio::test(flavor = "multi_thread")]
async fn a_question_in_review_reads_asks_you_and_its_answer_reaches_the_agent() {
    let w = TestService::start().await;
    let (t, shell) = in_review(&w).await;
    let calm = items(&w, &t).await;
    assert!(
        calm.iter()
            .all(|(group, ..)| *group == NeedsYouGroup::Review),
        "{calm:?}"
    );

    asks(&w, &shell, &t).await;
    let ticket = w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(ticket.status, Status::WaitingForYou, "it keeps its status");
    assert_eq!(ticket.place, Place::stage_of("review"), "and its column");
    let question = ticket.question_in_place().expect("the question shows");
    assert_eq!(question.text, QUESTION);
    assert_eq!(question.options, ["Yes", "No"]);
    assert!(ticket.shows_needs_you() && ticket.needs_user(), "turquoise");
    let shown = ticket.as_asks_you().expect("read as Asks you");
    assert_eq!(shown.status, Status::AsksYou);
    assert_eq!(
        shown.signal.as_ref().and_then(|s| s.text.as_deref()),
        Some(QUESTION)
    );
    assert_eq!(
        items(&w, &t).await,
        [(
            NeedsYouGroup::Family,
            Status::AsksYou,
            format!("asks: {QUESTION}")
        )],
        "the strip"
    );
    let item = w
        .board
        .needs_you_items(0)
        .await
        .unwrap()
        .into_iter()
        .find(|i| i.key == t.key)
        .unwrap();
    assert!(notify::should_notify(&item), "a notification");

    assert_eq!(
        answer(&w, Choice::Option(1)).await,
        Delivery::Sent { count: 1 }
    );
    w.wait_for_output(&shell.session, "Answer to your question: Yes (option 1).")
        .await;
    let ticket = w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(ticket.status, Status::WaitingForYou, "Ready for you again");
    assert_eq!(ticket.question_in_place(), None);
    assert!(!ticket.shows_needs_you());
    assert_eq!(answered_via(&w, &t).await.as_deref(), Some("box"));
    assert_eq!(items(&w, &t).await, calm, "back to its review item");
    w.stop().await;
}

/// Send back closes the question asked in Review (sending a ticket back closes its open question): it leaves Needs
/// you, and back in Dev it doesn't hold Asks you when the agent's turn ends there.
#[tokio::test(flavor = "multi_thread")]
async fn send_back_closes_a_question_asked_in_review() {
    let w = TestService::start().await;
    let (t, shell) = in_review(&w).await;
    asks(&w, &shell, &t).await;
    assert!(
        w.board
            .ticket("SHOP-1")
            .await
            .unwrap()
            .question_in_place()
            .is_some()
    );

    w.review
        .fail("SHOP-1", "Refunds need a window", Vec::new(), false, &me())
        .await
        .unwrap();
    let ticket = w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(ticket.place, Place::stage_of("dev"), "sent back");
    assert_eq!(w.board.open_question(&t.id).await.unwrap(), None);
    assert_eq!(answered_via(&w, &t).await.as_deref(), Some("closed"));
    assert!(items(&w, &t).await.is_empty(), "{:?}", items(&w, &t).await);

    // The agent ends its turn in Dev: no open question brings Asks you back.
    let me = Caller::Agent {
        ticket: t.key.clone(),
        ticket_id: t.id.clone(),
        session: shell.session.clone(),
    };
    w.signals
        .hook(
            &me,
            Tool::Claude,
            "Stop",
            &json!({"last_assistant_message": "Waiting for your answer."}),
        )
        .await
        .unwrap();
    assert_ne!(w.status("SHOP-1").await, Status::AsksYou);
    assert!(
        items(&w, &t)
            .await
            .iter()
            .all(|(_, status, _)| *status != Status::AsksYou)
    );
    w.stop().await;
}

/// The message writer's permission prompt keeps its calm in Review even with a question open: the ticket reads the
/// question's Asks you, never the prompt's Needs you, and the prompt stays on its session's screen for the Approve
/// dialog. Answering the question leaves Review calm.
#[tokio::test(flavor = "multi_thread")]
async fn the_writers_prompt_stays_calm_beside_a_question_in_review() {
    let w = TestService::start().await;
    let (t, shell) = in_review(&w).await;
    let caller = Caller::Agent {
        ticket: t.key.clone(),
        ticket_id: t.id.clone(),
        session: shell.session.clone(),
    };
    let mut payload: Value = serde_json::from_str(
        &std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/codex/hooks/PermissionRequest.json"),
        )
        .unwrap(),
    )
    .unwrap();
    payload["tool_input"]["command"] = "git log --oneline main..HEAD".into();
    w.signals
        .hook(&caller, Tool::Codex, "PermissionRequest", &payload)
        .await
        .unwrap();
    let ticket = w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(ticket.session_prompt(), None, "the writer's prompt is calm");
    assert!(!ticket.shows_needs_you());
    assert!(
        w.signals.open_prompt(&shell.session).is_some(),
        "on its screen, for the Approve dialog"
    );

    w.board
        .store_question(&t.id, QUESTION, &[], &[], &[], false)
        .await
        .unwrap();
    w.signals.ask_human(&caller, &t.id, QUESTION).await.unwrap();
    let ticket = w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(ticket.status, Status::WaitingForYou);
    assert_eq!(ticket.session_prompt(), None, "still calm");
    assert_eq!(
        otaq_core::board::tickets::Ticket::waiting_in_place(&ticket).map(|t| t.status),
        Some(Status::AsksYou),
        "the question, not the prompt"
    );
    assert_eq!(
        items(&w, &t).await,
        [(
            NeedsYouGroup::Family,
            Status::AsksYou,
            format!("asks: {QUESTION}")
        )]
    );
    w.stop().await;
}

/// The agent's session ended by a restart before the user answered its question asked in Review (re-test 10
/// NEW-r10-2): the answer isn't sent, and the refusal says the session ended and points to Send back with the answer
/// as the note. The question stays open and the ticket keeps its status and column.
#[tokio::test(flavor = "multi_thread")]
async fn an_answer_in_review_after_the_session_ended_points_to_send_back() {
    let w = TestService::start().await;
    let (t, shell) = in_review(&w).await;
    asks(&w, &shell, &t).await;
    let w = w.restart().await;

    let refused = w
        .needs_you
        .answer("SHOP-1", &Choice::Option(1), false, &me())
        .await
        .expect_err("refused");
    assert_eq!(refused.refusal(), Some(Refusal::SessionEnded));
    assert_eq!(
        refused.to_string(),
        "SHOP-1's agent session has ended. Send the ticket back with your answer as the note."
    );
    let ticket = w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(ticket.status, Status::WaitingForYou, "it keeps its status");
    assert_eq!(ticket.place, Place::stage_of("review"), "and its column");
    assert_eq!(
        ticket.question_in_place().map(|q| q.text.as_str()),
        Some(QUESTION),
        "the question stays open"
    );
    assert_eq!(answered_via(&w, &t).await, None);
    w.stop().await;
}
