//! The delivery's message, written while the ticket waits in Review: when a ticket reaches Review on a click-mode flow
//! whose `delivery:` names an agent, that agent is asked for the message (typed into its live session once idle),
//! sends it with `otaq pr message --stdin`, and Approve's merge confirmation opens on it. Until it arrives the
//! confirmation holds the plain message and names the agent still writing. The same head isn't asked for twice; new
//! commits are.

use std::path::PathBuf;

use otaq_core::board::projects::AddProject;
use otaq_core::board::tickets::Ticket;
use otaq_core::domain::{Actor, Status};
use otaq_core::git::testing::commit;
use otaq_core::pr::message::MessageState;
use otaq_core::service::testing::{AgentShell, TestService, said};
use serde_json::json;

const CB: &str = env!("CARGO_BIN_EXE_otaq");

struct Setup {
    w: TestService,
    ticket: Ticket,
    agent: AgentShell,
    worktree: PathBuf,
}

fn me() -> Actor {
    Actor::User("me".into())
}

/// SHOP-1 in Review with one commit, its agent's session live (the default flow: Developer builds it and is the
/// delivery's agent).
async fn setup() -> Setup {
    let w = TestService::start().await;
    let repo = w.repos.clone_with_origin("shop-web", "main", 1);
    w.board
        .add_project(&AddProject {
            path: repo.display().to_string(),
            ..Default::default()
        })
        .await
        .unwrap();
    let ticket = w.create("SHOP", "Checkout timeout").await;
    let worktree = w.start_worktree(&ticket).await;
    commit(&worktree, "api.ts", "retry\n");
    w.force_status(&ticket, Status::Working).await;
    w.board
        .submit_for_review(
            "SHOP-1",
            said(&w.board, "SHOP-1", "Retries the order request once.")
                .await
                .as_ref(),
            &me(),
        )
        .await
        .unwrap();
    let ticket = w.board.ticket("SHOP-1").await.unwrap();
    let agent = w.agent_shell(&ticket).await;
    Setup {
        w,
        ticket,
        agent,
        worktree,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_delivery_agent_writes_the_message_while_the_ticket_waits_in_review() {
    let s = setup().await;
    let w = &s.w;
    assert_eq!(s.ticket.status, Status::WaitingForYou);

    // Arriving in Review asks the delivery's agent, in its live session: waiting there until it is idle.
    w.board.delivery().arrived(&s.ticket.id).await;
    assert_eq!(
        w.board
            .ticket("SHOP-1")
            .await
            .unwrap()
            .flags
            .messages_waiting,
        1
    );
    let writing = w
        .board
        .delivery_message(&s.ticket.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(writing.state, MessageState::Writing);
    assert_eq!(writing.agent, "Developer");

    // Approve's confirmation opens at once on the plain message, naming who still writes.
    let plain = w.board.merge_proposal("SHOP-1").await.unwrap();
    assert_eq!(plain.writing.as_deref(), Some("Developer"));
    assert_eq!(plain.prepared_by, None);
    assert!(
        plain.message.starts_with("SHOP-1: Checkout timeout"),
        "{}",
        plain.message
    );

    // The agent sends it; a message without a title is refused.
    let ran = s
        .agent
        .run_with_stdin(&format!("'{CB}' pr message --stdin"), r#"{"title": ""}"#)
        .await;
    assert_ne!(ran.status, 0, "{ran:?}");
    let message = json!({
        "title": "SHOP-1 Retry the order request once on timeout",
        "description": "POST /orders is retried once after a 10 s timeout."
    });
    let ran = s
        .agent
        .run_with_stdin(&format!("'{CB}' pr message --stdin"), &message.to_string())
        .await;
    assert_eq!(ran.status, 0, "{ran:?}");
    let ready = w.board.merge_proposal("SHOP-1").await.unwrap();
    assert_eq!(ready.prepared_by.as_deref(), Some("Developer"));
    assert_eq!(ready.writing, None);
    assert_eq!(
        ready.message,
        "SHOP-1 Retry the order request once on timeout\n\nPOST /orders is retried once after a 10 s timeout."
    );

    // The same head isn't asked for again.
    assert!(!w.prs.prepare_message(&s.ticket.id).await.unwrap());
    // New commits are (a send back that changed the work, here by hand).
    commit(&s.worktree, "api.test.ts", "test\n");
    assert!(w.prs.prepare_message(&s.ticket.id).await.unwrap());
    let again = w
        .board
        .delivery_message(&s.ticket.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(again.state, MessageState::Writing);

    // The merge uses what the user confirms; once merged, a late message is refused.
    let merged = w
        .review
        .local_merge("SHOP-1", &ready.message, true, &me())
        .await
        .unwrap();
    assert!(
        matches!(merged, otaq_core::review::LocalMergeOutcome::Merged { .. }),
        "{merged:?}"
    );
    // (The merge ended the agent's session.)
    assert!(
        w.board
            .store_delivery_message("SHOP-1", &message.to_string())
            .await
            .is_err()
    );
    s.w.stop().await;
}

/// A flow whose delivery names no agent asks nobody: the confirmation starts from the plain message.
#[tokio::test(flavor = "multi_thread")]
async fn a_delivery_without_an_agent_asks_nobody() {
    let s = setup().await;
    let w = &s.w;
    w.board
        .edit_flow_for_tests(
            "SHOP",
            otaq_core::flow::default::testing::delivering(otaq_core::flow::Delivers::Local),
        )
        .await;
    assert!(!w.prs.prepare_message(&s.ticket.id).await.unwrap());
    let plain = w.board.merge_proposal("SHOP-1").await.unwrap();
    assert_eq!((plain.writing, plain.prepared_by), (None, None));
    s.w.stop().await;
}

/// The session asked for the message ends before its agent sends it: the message never comes, so the confirmation
/// stops saying the agent still writes and starts from the plain message.
#[tokio::test(flavor = "multi_thread")]
async fn a_message_whose_session_ends_unwritten_falls_back_to_the_plain_one() {
    let s = setup().await;
    let w = &s.w;
    w.board.delivery().arrived(&s.ticket.id).await;
    let writing = w.board.merge_proposal("SHOP-1").await.unwrap();
    assert_eq!(writing.writing.as_deref(), Some("Developer"));

    w.sessions.stop(&s.agent.session).await.unwrap();
    let gone = tokio::time::timeout(std::time::Duration::from_secs(20), async {
        loop {
            if w.board
                .delivery_message(&s.ticket.id)
                .await
                .unwrap()
                .is_none()
            {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await;
    assert!(
        gone.is_ok(),
        "the unwritten message is dropped when its session ends"
    );
    let plain = w.board.merge_proposal("SHOP-1").await.unwrap();
    assert_eq!((plain.writing, plain.prepared_by), (None, None));
    assert!(
        plain.message.starts_with("SHOP-1: Checkout timeout"),
        "{}",
        plain.message
    );
    s.w.stop().await;
}

/// The request for the message waits behind text left in the agent's input line; meanwhile the ticket is sent back to
/// Dev. Once the line is free nothing is typed in: the request was only for a ticket waiting to be delivered.
#[tokio::test(flavor = "multi_thread")]
async fn a_request_waiting_behind_unsent_text_is_dropped_when_the_ticket_leaves_review() {
    let s = setup().await;
    let w = &s.w;
    let session = &s.agent.session;
    w.sessions.input(session, b"half-typed").unwrap();
    w.board.delivery().arrived(&s.ticket.id).await;
    assert_eq!(w.deliveries.pending(session).await.unwrap().len(), 1);
    assert_eq!(
        w.deliveries.deliver_now(session).await.unwrap(),
        otaq_core::session::deliveries::Delivery::NeedsConfirmation {
            ticket: Some(s.ticket.key.clone()),
            unclearable_by: None,
        },
        "held behind the unsent text"
    );

    // The merge check sends it back to Dev.
    w.force_status(&s.ticket, Status::Working).await;

    // The user sends their line (the shell says there is no such command): every waiting message may go now.
    w.sessions.input(session, b"\r").unwrap();
    let free = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        while w.sessions.has_unsent_input(session).unwrap() {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await;
    assert!(free.is_ok(), "the input line is free");
    assert_eq!(
        w.deliveries.deliver_now(session).await.unwrap(),
        otaq_core::session::deliveries::Delivery::Sent { count: 0 },
        "nothing is typed in"
    );
    assert!(w.deliveries.pending(session).await.unwrap().is_empty());
    assert_eq!(
        w.board
            .ticket("SHOP-1")
            .await
            .unwrap()
            .flags
            .messages_waiting,
        0
    );
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    let shown =
        String::from_utf8_lossy(&w.sessions.attach(session).unwrap().scrollback).into_owned();
    assert!(!shown.contains("is waiting in Review"), "{shown}");
    // The dropped request counts as never made: the ticket isn't recorded as being written, and back in Review on
    // the same branch head the writer is asked again.
    assert_eq!(w.board.delivery_message(&s.ticket.id).await.unwrap(), None);
    assert_eq!(
        w.board.merge_proposal("SHOP-1").await.unwrap().writing,
        None
    );
    w.force_status(&s.ticket, Status::WaitingForYou).await;
    assert!(w.prs.prepare_message(&s.ticket.id).await.unwrap());
    assert_eq!(
        w.board
            .delivery_message(&s.ticket.id)
            .await
            .unwrap()
            .unwrap()
            .state,
        MessageState::Writing
    );
    assert_eq!(w.deliveries.pending(session).await.unwrap().len(), 1);
    s.w.stop().await;
}
