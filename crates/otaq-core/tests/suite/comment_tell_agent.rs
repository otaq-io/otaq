//! "Also tell the agent now" (T-0495; d0123, d0048): the user's comment posted with `tell_agent` on a ticket whose
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
use serde_json::json;

fn comment(key: &str, body: &str, tell_agent: bool) -> Request {
    Request::TicketCommentPost(TicketCommentPostParams {
        key: key.into(),
        body: body.into(),
        report: None,
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
    let request = json!({"protocol_version": 1, "id": 1, "request": {"method": "ticket.comment_post",
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
