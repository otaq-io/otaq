//! The focused-terminal rule (T-0110, ATT-03, d0042): while a ticket's terminal has focus, its agent's waiting is
//! recorded but raises no turquoise, no needs-you item (and so no notification); "your turn" is shown instead. Leaving the
//! terminal brings the turquoise and the item back at once. An ordinary `sh` stands in for `claude`.

use std::time::Duration;

use otaq_core::agent_tools::Tool;
use otaq_core::board::tickets::Ticket;
use otaq_core::caller::Caller;
use otaq_core::client::{Client, ClientConfig};
use otaq_core::domain::{Status, StatusSet};
use otaq_core::protocol::{Event, FocusParams, Request};
use otaq_core::service::testing::{AgentShell, TestService};
use otaq_core::session::SessionKind;
use otaq_core::signals::Signal;
use otaq_core::store::now_ms;
use serde_json::json;

fn agent(ticket: &Ticket, shell: &AgentShell) -> Caller {
    Caller::Agent {
        ticket: ticket.key.clone(),
        ticket_id: ticket.id.clone(),
        session: shell.session.clone(),
    }
}

async fn focus(client: &Client, session: &str, focused: bool) {
    client
        .request(Request::SessionFocus(FocusParams {
            session: session.to_owned(),
            focused,
        }))
        .await
        .unwrap();
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

/// Whether SHOP-1 is a needs-you item, and the project's needs-you count.
async fn in_needs_you(w: &TestService) -> (bool, u32) {
    let items = w.board.needs_you_items(now_ms()).await.unwrap();
    let info = w.board.project_info_of("SHOP").await.unwrap();
    (
        items.iter().any(|i| i.key.to_string() == "SHOP-1"),
        info.needs_you,
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn waiting_while_focused_is_recorded_but_calm_until_the_user_leaves() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let shop1 = w.create("SHOP", "Split user full name").await;
    w.force_status(&shop1, Status::Working).await;
    let shell = w.agent_shell(&shop1).await;
    let app = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    let mut events = app.subscribe().await.unwrap();

    focus(&app, &shell.session, true).await;
    // The turn ends with a question in the text: Idle, recorded on the ticket.
    w.signals
        .hook(
            &agent(&shop1, &shell),
            Tool::Claude,
            "Stop",
            &json!({"hook_event_name": "Stop", "stop_hook_active": false,
                    "last_assistant_message": "I split the name.\n\nShould I also update the tests?\n",
                    "background_tasks": [], "session_crons": []}),
        )
        .await
        .unwrap();
    let t = w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(t.status, Status::Idle, "recorded");
    assert!(t.flags.your_turn, "your turn instead of turquoise");
    assert!(!t.shows_needs_you() && !t.needs_user());
    assert_eq!(
        in_needs_you(&w).await,
        (false, 0),
        "no needs_you item while focused"
    );
    let dev = w.board.board_snapshot("SHOP").await.unwrap();
    let dev = dev
        .columns
        .iter()
        .find(|c| c.column == StatusSet::AgentStage)
        .unwrap();
    assert_eq!(dev.cards[0].key.to_string(), "SHOP-1");

    // A permission prompt while focused: also calm.
    w.signals
        .hook(
            &agent(&shop1, &shell),
            Tool::Claude,
            "Notification",
            &json!({"hook_event_name": "Notification", "notification_type": "permission_prompt",
                    "message": "Claude needs your permission to use Bash"}),
        )
        .await
        .unwrap();
    let t = w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!((t.status, t.flags.your_turn), (Status::NeedsYou, true));
    assert_eq!(in_needs_you(&w).await, (false, 0));

    // Leaving the terminal with the agent still waiting: turquoise and the item appear at that moment.
    while events.try_recv().is_ok() {}
    focus(&app, &shell.session, false).await;
    let t = w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!((t.status, t.flags.your_turn), (Status::NeedsYou, false));
    assert!(t.shows_needs_you());
    assert_eq!(in_needs_you(&w).await, (true, 1));
    let changed = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(Event::TicketChanged { key, .. }) = events.recv().await
                && key.to_string() == "SHOP-1"
            {
                return;
            }
        }
    })
    .await;
    assert!(changed.is_ok(), "clients are told to refetch the card");

    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_ticket_stays_calm_while_any_client_looks_and_a_disconnect_lets_go() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let shop1 = w.create("SHOP", "Order confirmation email").await;
    w.force_status(&shop1, Status::Working).await;
    let shell = w.agent_shell(&shop1).await;
    let app = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    // `otaq attach` reports focus the same way and holds it while attached.
    let attach = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    focus(&app, &shell.session, true).await;
    focus(&attach, &shell.session, true).await;

    w.signals
        .ask_human(
            &agent(&shop1, &shell),
            &shop1.id,
            "Which email provider should I use?",
        )
        .await
        .unwrap();
    assert!(w.board.ticket("SHOP-1").await.unwrap().flags.your_turn);

    // The app's terminal loses focus; attach still looks at it.
    focus(&app, &shell.session, false).await;
    assert!(w.board.ticket("SHOP-1").await.unwrap().flags.your_turn);
    assert!(w.sessions.is_focused(&shell.session).unwrap());
    assert_eq!(in_needs_you(&w).await, (false, 0));

    // attach goes away without a word (Ctrl+C, a closed terminal): its connection closing counts as focus lost.
    drop(attach);
    eventually("the disconnect releasing focus", async || {
        !w.board.ticket("SHOP-1").await.unwrap().flags.your_turn
    })
    .await;
    let t = w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(t.status, Status::AsksYou);
    assert_eq!(in_needs_you(&w).await, (true, 1));

    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_ended_session_holds_no_focus() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let shop1 = w.create("SHOP", "Order confirmation email").await;
    w.force_status(&shop1, Status::Working).await;
    let shell = w.agent_shell(&shop1).await;
    let app = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    focus(&app, &shell.session, true).await;
    w.signals
        .ask_human(&agent(&shop1, &shell), &shop1.id, "Which provider?")
        .await
        .unwrap();
    assert!(w.board.ticket("SHOP-1").await.unwrap().flags.your_turn);

    w.sessions.stop(&shell.session).await.unwrap();
    eventually("the ended session releasing focus", async || {
        !w.board.ticket("SHOP-1").await.unwrap().flags.your_turn
    })
    .await;
    // Reports for an ended session are ignored.
    focus(&app, &shell.session, true).await;
    assert!(!w.board.ticket("SHOP-1").await.unwrap().flags.your_turn);

    w.stop().await;
}

/// T-0194 (ATT-03, d0063): the rule applies per session. While the validator's terminal has focus (the app's
/// Validator side, or `otaq attach SHOP-1 --stage validate`), its permission prompt raises no turquoise and no Needs you
/// item; looking at the agent's terminal doesn't calm it.
#[tokio::test(flavor = "multi_thread")]
async fn the_validators_terminal_calms_only_the_validator() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let shop1 = w.create("SHOP", "Shipping estimate").await;
    w.force_status(&shop1, Status::Working).await;
    let agent_shell = w.agent_shell(&shop1).await;
    let validator = w.shell(&shop1, SessionKind::Validator).await;
    w.validating_with(&shop1, &validator.session).await;
    let app = Client::connect(ClientConfig::new(&w.data)).await.unwrap();

    // The user looks at the agent's terminal; the validator asks for a permission.
    focus(&app, &agent_shell.session, true).await;
    w.validation
        .validator_signal(
            &shop1.id,
            &validator.session,
            &Signal::NeedsYou {
                prompt: "Allow Bash(pnpm test)?".into(),
            },
        )
        .await
        .unwrap();
    let t = w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(
        (t.status, t.flags.your_turn),
        (Status::CheckerNeedsYou, false)
    );
    assert!(t.shows_needs_you());
    assert_eq!(
        in_needs_you(&w).await,
        (true, 1),
        "the agent's terminal doesn't calm it"
    );

    // The user switches to the Validator side (or attaches with --stage validate): calm, no item.
    focus(&app, &agent_shell.session, false).await;
    let attach = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    focus(&attach, &validator.session, true).await;
    let t = w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(
        (t.status, t.flags.your_turn),
        (Status::CheckerNeedsYou, true)
    );
    assert_eq!(in_needs_you(&w).await, (false, 0));

    // Detaching brings the turquoise and the item back.
    drop(attach);
    eventually("the detach releasing the validator's focus", async || {
        !w.board.ticket("SHOP-1").await.unwrap().flags.your_turn
    })
    .await;
    assert_eq!(in_needs_you(&w).await, (true, 1));

    w.stop().await;
}
