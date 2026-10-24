//! Agent signals in the running service (T-0067, ATT-01, ATT-02, ATT-05): the stored label and text, clearing by
//! Enter in the ticket's terminal and by any command of the agent, the last signal, and automatic deliveries
//! typed once the agent is idle between turns. An ordinary `sh` stands in for `claude`.

use std::time::Duration;

use otaq_core::agent_tools::Tool;
use otaq_core::caller::Caller;
use otaq_core::client::{Client, ClientConfig};
use otaq_core::domain::Status;
use otaq_core::protocol::{InputParams, Request, encode_bytes};
use otaq_core::service::testing::{AgentShell, TestService};
use otaq_core::session::SessionKind;
use otaq_core::session::deliveries::DeliveryKind;
use otaq_core::signals::Signal;
use serde_json::json;

fn agent_caller(w_ticket: &otaq_core::board::tickets::Ticket, shell: &AgentShell) -> Caller {
    Caller::Agent {
        ticket: w_ticket.key.clone(),
        ticket_id: w_ticket.id.clone(),
        session: shell.session.clone(),
    }
}

async fn eventually(what: &str, mut check: impl AsyncFnMut() -> bool) {
    tokio::time::timeout(Duration::from_secs(10), async {
        while !check().await {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("{what} never happened"));
}

#[tokio::test(flavor = "multi_thread")]
async fn labels_texts_clearing_and_last_signal() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let shop1 = w.create("SHOP", "Order confirmation email").await;
    w.force_status(&shop1, Status::Working).await;
    let shell = w.agent_shell(&shop1).await;
    let me = agent_caller(&shop1, &shell);

    // Asks you keeps the question; Needs you keeps the prompt.
    w.signals
        .ask_human(&me, &shop1.id, "Which email provider should I use?")
        .await
        .unwrap();
    let t = w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(t.status, Status::AsksYou);
    let signal = t.signal.unwrap();
    assert_eq!(
        signal.text.as_deref(),
        Some("Which email provider should I use?")
    );
    let asked_at = t.flags.last_signal_at.unwrap();

    // Enter in SHOP-1's terminal (relayed by the service, as the app does) clears Asks you.
    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    client
        .request(Request::SessionInput(InputParams {
            session: shell.session.clone(),
            data: encode_bytes(b"Postmark\r"),
            reply: false,
        }))
        .await
        .unwrap();
    eventually("Enter clearing Asks you", async || {
        w.status("SHOP-1").await == Status::Working
    })
    .await;
    assert!(w.board.ticket("SHOP-1").await.unwrap().signal.is_none());

    // Needs you, then a command of the agent that acts (here `ticket.notes`, which marks its notes read, sent from
    // inside the session) clears it. A read (`ping`) is no signal at all (d0119).
    w.signals
        .hook(
            &me,
            Tool::Claude,
            "Notification",
            &json!({"notification_type": "permission_prompt", "message": "Allow `rm -rf .next`?"}),
        )
        .await
        .unwrap();
    let t = w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(t.status, Status::NeedsYou);
    assert_eq!(
        t.signal.unwrap().text.as_deref(),
        Some("Allow `rm -rf .next`?")
    );
    let prompted_at = t.flags.last_signal_at.unwrap();
    assert!(prompted_at >= asked_at);
    tokio::time::sleep(Duration::from_millis(5)).await;
    let ran = shell
        .send_raw(
            &w.data.socket_path(),
            r#"{"protocol_version":1,"id":1,"request":{"method":"ping"}}"#,
        )
        .await;
    assert!(ran.stdout.contains("pong"), "{ran:?}");
    let t = w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(t.status, Status::NeedsYou, "a read clears nothing");
    assert_eq!(
        t.flags.last_signal_at,
        Some(prompted_at),
        "a read is no signal"
    );
    let ran = shell
        .send_raw(
            &w.data.socket_path(),
            r#"{"protocol_version":1,"id":1,"request":{"method":"ticket.notes","params":{"key":"SHOP-1"}}}"#,
        )
        .await;
    assert!(!ran.stdout.contains("error"), "{ran:?}");
    let t = w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(t.status, Status::Working);
    assert!(
        t.flags.last_signal_at.unwrap() > prompted_at,
        "the command is a signal"
    );
    drop(client);
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn enter_in_the_shell_tab_keeps_the_question() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let shop1 = w.create("SHOP", "Cart totals").await;
    w.force_status(&shop1, Status::Working).await;
    let agent = w.agent_shell(&shop1).await;
    let shell = w.shell(&shop1, SessionKind::Shell).await;
    w.signals
        .ask_human(
            &agent_caller(&shop1, &agent),
            &shop1.id,
            "Should totals include tax?",
        )
        .await
        .unwrap();

    // The Shell tab is "not the agent": a command run there answers nothing.
    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    client
        .request(Request::SessionInput(InputParams {
            session: shell.session.clone(),
            data: encode_bytes(b"pwd\r"),
            reply: false,
        }))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    let t = w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(t.status, Status::AsksYou);
    assert_eq!(
        t.signal.unwrap().text.as_deref(),
        Some("Should totals include tax?")
    );
    drop(client);
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn waiting_messages_are_typed_once_the_agent_is_idle() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let shop1 = w.create("SHOP", "Split user full name").await;
    w.force_status(&shop1, Status::Working).await;
    let shell = w.agent_shell(&shop1).await;
    let me = agent_caller(&shop1, &shell);
    let marker = w.home.path().join("delivered");

    // Mid-turn: the message waits.
    w.deliveries
        .queue_delivery(
            &shell.session,
            DeliveryKind::TicketChange,
            &format!("touch '{}'", marker.display()),
        )
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(w.deliveries.pending(&shell.session).await.unwrap().len(), 1);
    assert!(!marker.exists());

    // Stop with nothing pending: Idle, and the waiting message is typed (the shell runs it).
    let signal = w
        .signals
        .hook(
            &me,
            Tool::Claude,
            "Stop",
            &json!({"last_assistant_message": "Done for now.", "background_tasks": [], "session_crons": []}),
        )
        .await
        .unwrap();
    assert!(matches!(signal[..], [Signal::Idle { .. }]));
    // The service delivers on the hook request; here the hook was applied directly, so ask for it as it does.
    w.deliveries.deliver_if_ready(&shell.session).await.unwrap();
    eventually("the message typed", async || marker.exists()).await;
    assert!(
        w.deliveries
            .pending(&shell.session)
            .await
            .unwrap()
            .is_empty()
    );
    // Typing it started the next turn: a new message waits for the next Stop.
    assert!(!w.signals.is_idle_between_turns(&shell.session));
    w.stop().await;
}

/// A tool id reported after start (d0088 "Session ids", ADP-02 *tool id known*) is stored only while the session
/// has none. No Claude Code hook reports it: Claude Code's id is chosen up front.
#[tokio::test(flavor = "multi_thread")]
async fn a_reported_tool_id_is_stored_only_when_empty() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let shop1 = w.create("SHOP", "Order confirmation email").await;
    let shell = w.agent_shell(&shop1).await;
    let stored = async || -> Option<String> {
        sqlx::query_scalar("SELECT tool_session_id FROM sessions WHERE id = ?")
            .bind(&shell.session)
            .fetch_one(w.board.store().pool())
            .await
            .unwrap()
    };
    assert_eq!(stored().await, None);
    assert!(
        w.signals
            .tool_id_known(&shell.session, "019a-first")
            .await
            .unwrap()
    );
    assert_eq!(stored().await.as_deref(), Some("019a-first"));
    assert!(
        !w.signals
            .tool_id_known(&shell.session, "019a-second")
            .await
            .unwrap()
    );
    assert_eq!(stored().await.as_deref(), Some("019a-first"));
    w.stop().await;
}

/// A hook the adapter doesn't know changes nothing, not even the last signal (ADP-11); one that gives no event
/// (another notification type) still moves the last signal, as before.
#[tokio::test(flavor = "multi_thread")]
async fn unrecognised_and_eventless_hooks() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let shop1 = w.create("SHOP", "Order confirmation email").await;
    w.force_status(&shop1, Status::Working).await;
    let shell = w.agent_shell(&shop1).await;
    let me = agent_caller(&shop1, &shell);
    let last_signal = async || w.board.ticket("SHOP-1").await.unwrap().flags.last_signal_at;
    let before = last_signal().await;
    let signals = w
        .signals
        .hook(&me, Tool::Claude, "SessionStart", &json!({}))
        .await
        .unwrap();
    assert!(signals.is_empty());
    assert_eq!(last_signal().await, before);
    let signals = w
        .signals
        .hook(
            &me,
            Tool::Claude,
            "Notification",
            &json!({"notification_type": "idle_prompt", "message": "Claude is waiting for your input"}),
        )
        .await
        .unwrap();
    assert_eq!(signals, [Signal::Ignored]);
    assert!(last_signal().await.is_some());
    assert_eq!(w.status("SHOP-1").await, Status::Working);
    w.stop().await;
}
