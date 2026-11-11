//! "Tell <agent> now" (T-0495; d0123, d0048): the user's comment posted with `tell_agent` on a ticket whose
//! agent (or check) runs queues one line for that session, "The user added comment N. Run `otaq prime` to read it.",
//! typed once it is idle with an empty input line, "1 message waiting" until then. It is not a hand-off. Without the
//! flag, from an agent, or with no running session: nothing. An ordinary `sh` stands in for `claude`.

use otaq_core::agent_tools::Tool;
use otaq_core::board::ticket_comments::tell_agent_line;
use otaq_core::caller::Caller;
use otaq_core::client::{Client, ClientConfig};
use otaq_core::domain::Status;
use otaq_core::protocol::{Reply, Request, TicketCommentPostParams};
use otaq_core::service::testing::TestService;
use otaq_core::session::SessionKind;
use otaq_core::session::deliveries::DeliveryKind;
use serde_json::json;

fn comment(key: &str, body: &str, tell_agent: bool) -> Request {
    Request::TicketCommentPost(TicketCommentPostParams {
        key: key.into(),
        body: body.into(),
        attachments: Default::default(),
        tell_agent,
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn a_ticked_comment_tells_the_working_agent_once_idle() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let shop1 = w.create("SHOP", "Price table").await;
    w.force_status(&shop1, Status::Working).await;
    let agent = w.agent_shell(&shop1).await;
    let app = Client::connect(ClientConfig::new(&w.data)).await.unwrap();

    app.request(comment("SHOP-1", "First, for later stages.", false))
        .await
        .unwrap();
    let reply = app
        .request(comment(
            "SHOP-1",
            "Use useToday() on the detail and Stats pages.",
            true,
        ))
        .await
        .unwrap();
    let Reply::TicketComment(posted) = reply else {
        panic!("{reply:?}");
    };
    assert_eq!(posted.n, 2);
    // The comment says who was told; the plain one before it told nobody.
    assert!(posted.told.is_some(), "{posted:?}");
    let first = w.board.ticket_comment("SHOP-1", 1).await.unwrap();
    assert_eq!(
        first.told, None,
        "a plain comment reaches the running agent only when chosen"
    );

    // Queued, never the comment itself: "1 message waiting" until the agent is idle.
    let pending = w.deliveries.pending(&agent.session).await.unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].kind, "ticket_change");
    assert_eq!(
        pending[0].text,
        "The user added comment 2. Run `otaq prime` to read it."
    );
    assert_eq!(pending[0].text, tell_agent_line(2));
    let flags = w.board.ticket("SHOP-1").await.unwrap().flags;
    assert_eq!(flags.messages_waiting, 1);
    assert_eq!(flags.hand_off, None, "a comment's line isn't a hand-off");

    // Idle with an empty line: typed.
    let me = Caller::Agent {
        ticket: shop1.key.clone(),
        ticket_id: shop1.id.clone(),
        session: agent.session.clone(),
    };
    w.signals
        .hook(
            &me,
            Tool::Claude,
            "Stop",
            &json!({"background_tasks": [], "session_crons": []}),
        )
        .await
        .unwrap();
    w.deliveries.deliver_if_ready(&agent.session).await.unwrap();
    assert!(
        w.deliveries
            .pending(&agent.session)
            .await
            .unwrap()
            .is_empty()
    );
    let out = w
        .wait_for_output(&agent.session, "The user added comment 2.")
        .await;
    assert!(!out.contains("useToday"), "the comment isn't typed in");
    let flags = w.board.ticket("SHOP-1").await.unwrap().flags;
    assert_eq!(flags.messages_waiting, 0);
    assert_eq!(flags.hand_off, None);
    drop(app);
    w.stop().await;
}

/// While the ticket's check runs, the line goes to the checker's session, not the agent's.
#[tokio::test(flavor = "multi_thread")]
async fn while_checking_the_check_is_told() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let shop1 = w.create("SHOP", "Price table").await;
    let agent = w.agent_shell(&shop1).await;
    let checker = w.shell(&shop1, SessionKind::Validator).await;
    w.validating_with(&shop1, &checker.session).await;
    let app = Client::connect(ClientConfig::new(&w.data)).await.unwrap();

    app.request(comment("SHOP-1", "Check the mobile widths too.", true))
        .await
        .unwrap();
    let pending = w.deliveries.pending(&checker.session).await.unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].text, tell_agent_line(1));
    assert!(
        w.deliveries
            .pending(&agent.session)
            .await
            .unwrap()
            .is_empty()
    );
    drop(app);
    w.stop().await;
}

/// The e2e finding: right after a check passes the ticket is in Review while its Validator session is still closing;
/// a ticked comment then goes to the agent's session, never into the check that has just finished (d0123: "while …
/// its check is running").
#[tokio::test(flavor = "multi_thread")]
async fn after_the_check_the_agent_is_told_not_the_closing_check() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let shop1 = w.create("SHOP", "Price table").await;
    let agent = w.agent_shell(&shop1).await;
    let checker = w.shell(&shop1, SessionKind::Validator).await;
    w.force_status(&shop1, Status::WaitingForYou).await;
    assert!(
        w.sessions
            .running_of(&shop1.id, SessionKind::Validator)
            .is_some(),
        "the check's session is still registered as running"
    );
    let app = Client::connect(ClientConfig::new(&w.data)).await.unwrap();

    app.request(comment("SHOP-1", "Keep the column order.", true))
        .await
        .unwrap();
    let pending = w.deliveries.pending(&agent.session).await.unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].text, tell_agent_line(1));
    assert!(
        w.deliveries
            .pending(&checker.session)
            .await
            .unwrap()
            .is_empty()
    );
    drop(app);
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn nothing_without_the_flag_from_an_agent_or_with_no_session() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let shop1 = w.create("SHOP", "Price table").await;
    let shop2 = w.create("SHOP", "Checkout page").await;
    w.force_status(&shop1, Status::Working).await;
    w.force_status(&shop2, Status::Queued).await;
    let agent = w.agent_shell(&shop1).await;
    w.run_as_agent(&agent, "rust-dev", "Rust developer").await;
    let app = Client::connect(ClientConfig::new(&w.data)).await.unwrap();

    // Unticked: for later stages only (d0109).
    app.request(comment("SHOP-1", "For the next stage.", false))
        .await
        .unwrap();
    assert!(
        w.deliveries
            .pending(&agent.session)
            .await
            .unwrap()
            .is_empty()
    );

    // The agent's own comment never tells it, even with the flag.
    let request = json!({"protocol_version": otaq_core::protocol::PROTOCOL_VERSION, "id": 1, "request": {"method": "ticket.comment_post",
                   "params": {"key": "SHOP-1", "body": "Done with the table.", "tell_agent": true}}});
    let ran = agent
        .send_raw(&w.data.socket_path(), &request.to_string())
        .await;
    assert!(ran.stdout.contains("\"reply\""), "{ran:?}");
    assert!(
        w.deliveries
            .pending(&agent.session)
            .await
            .unwrap()
            .is_empty()
    );

    // No running session: posted, nothing queued anywhere.
    let reply = app
        .request(comment("SHOP-2", "Pay button first.", true))
        .await
        .unwrap();
    assert!(matches!(reply, Reply::TicketComment(_)), "{reply:?}");
    assert_eq!(
        w.board
            .ticket("SHOP-2")
            .await
            .unwrap()
            .flags
            .messages_waiting,
        0
    );
    assert_eq!(
        w.board
            .ticket("SHOP-1")
            .await
            .unwrap()
            .flags
            .messages_waiting,
        0
    );
    drop(app);
    w.stop().await;
}

async fn eventually(what: &str, mut check: impl AsyncFnMut() -> bool) {
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        while !check().await {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("{what} never happened"));
}

/// The re-test finding: a ticked comment's line waited for the round's check, the check ended and the ticket went
/// back to Dev, and its agent session's header read "1 message waiting · Deliver now" until the merge, for a line
/// that could never go. The check's line goes with the check, and the agent's header never counts it.
#[tokio::test(flavor = "multi_thread")]
async fn a_checks_line_ends_with_the_check_and_never_waits_in_the_agents_header() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let shop1 = w.create("SHOP", "Price table").await;
    let agent = w.agent_shell(&shop1).await;
    let checker = w.shell(&shop1, SessionKind::Validator).await;
    w.validating_with(&shop1, &checker.session).await;
    let app = Client::connect(ClientConfig::new(&w.data)).await.unwrap();

    app.request(comment("SHOP-1", "Check the mobile widths too.", true))
        .await
        .unwrap();
    assert_eq!(
        w.deliveries.pending(&checker.session).await.unwrap().len(),
        1
    );
    assert_eq!(
        w.board
            .ticket("SHOP-1")
            .await
            .unwrap()
            .flags
            .messages_waiting,
        0,
        "the agent's header doesn't count the check's line"
    );

    // The check ends and the ticket is sent back to Dev.
    w.sessions.stop(&checker.session).await.unwrap();
    w.force_status(&shop1, Status::Working).await;
    eventually("the check's line dropped", async || {
        w.deliveries
            .pending(&checker.session)
            .await
            .unwrap()
            .is_empty()
    })
    .await;
    assert!(
        w.deliveries
            .pending(&agent.session)
            .await
            .unwrap()
            .is_empty(),
        "a check's line isn't the agent's"
    );
    assert_eq!(
        w.board
            .ticket("SHOP-1")
            .await
            .unwrap()
            .flags
            .messages_waiting,
        0
    );

    // Queued for a check that has already ended: it doesn't wait either.
    w.deliveries
        .queue_delivery(
            &checker.session,
            DeliveryKind::TicketChange,
            &tell_agent_line(2),
        )
        .await
        .unwrap();
    assert!(
        w.deliveries
            .pending(&checker.session)
            .await
            .unwrap()
            .is_empty()
    );
    drop(app);
    w.stop().await;
}

/// A line for the ticket's agent that comes for its agent session after that session ended (its end and a restart
/// crossed it) goes to the ticket's live agent session.
#[tokio::test(flavor = "multi_thread")]
async fn a_line_for_an_ended_agent_session_goes_to_the_live_one() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let shop1 = w.create("SHOP", "Price table").await;
    w.force_status(&shop1, Status::Working).await;
    let old = w.agent_shell(&shop1).await;
    w.sessions.stop(&old.session).await.unwrap();
    let live = w.agent_shell(&shop1).await;

    w.deliveries
        .queue_delivery(
            &old.session,
            DeliveryKind::TicketChange,
            &tell_agent_line(1),
        )
        .await
        .unwrap();
    assert!(w.deliveries.pending(&old.session).await.unwrap().is_empty());
    let moved = w.deliveries.pending(&live.session).await.unwrap();
    assert_eq!(moved.len(), 1, "{moved:?}");
    assert_eq!(moved[0].text, tell_agent_line(1));
    assert_eq!(
        w.board
            .ticket("SHOP-1")
            .await
            .unwrap()
            .flags
            .messages_waiting,
        1
    );
    w.stop().await;
}
