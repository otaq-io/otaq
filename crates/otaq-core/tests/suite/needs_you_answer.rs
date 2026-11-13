//! Needs-you answers (T-0114, ATT-08; d0045, d0048): typed into the session as the user's click, the unsent-text
//! question first, and resume-and-deliver in one step for an Interrupted ticket that was waiting. `cat` and an
//! `sh` that prints its arguments stand in for `claude`.

use std::path::PathBuf;
use std::time::Duration;

use otaq_core::agent_tools::LaunchContext;
use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::board::Refusal;
use otaq_core::board::moves::DropTarget;
use otaq_core::board::questions::Choice;
use otaq_core::board::tickets::Ticket;
use otaq_core::caller::Caller;
use otaq_core::client::{Client, ClientConfig};
use otaq_core::domain::{Actor, Status};
use otaq_core::protocol::{NeedsYouAnswerParams, Reply, Request};
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::TestService;
use otaq_core::session::deliveries::Delivery;
use otaq_core::session::{SessionKind, SessionTicket, SpawnSpec, Start, TerminalSize};
use otaq_core::store::now_ms;

fn me() -> Actor {
    Actor::User("me".into())
}

/// `cat` as SHOP-1's agent, so whatever is typed comes back in its output.
async fn cat_agent(w: &TestService, ticket: &Ticket) -> String {
    let project = w.board.project("SHOP").await.unwrap();
    w.sessions
        .spawn(SpawnSpec {
            kind: SessionKind::Agent,
            start: Start::User,
            project_id: project.id,
            project_key: project.key,
            ticket: Some(SessionTicket {
                id: ticket.id.clone(),
                key: ticket.key.clone(),
            }),
            program: "cat".into(),
            args: vec![],
            cwd: PathBuf::from(&project.repo_path),
            env: vec![],
            size: TerminalSize::default(),
            tool: None,
            tool_session_id: None,
            model: None,
            agent: None,
            effort: None,
            theme: None,
        })
        .await
        .unwrap()
        .id
}

async fn asks(w: &TestService, ticket: &Ticket, session: &str, options: &[&str]) {
    let options: Vec<String> = options.iter().map(|o| (*o).to_owned()).collect();
    w.board
        .store_question(
            &ticket.id,
            "Which email provider should I use?",
            &options,
            &[],
            if options.is_empty() { &[] } else { &[1] },
            false,
        )
        .await
        .unwrap();
    let agent = Caller::Agent {
        ticket: ticket.key.clone(),
        ticket_id: ticket.id.clone(),
        session: session.to_owned(),
    };
    w.signals
        .ask_human(&agent, &ticket.id, "Which email provider should I use?")
        .await
        .unwrap();
}

async fn answer(client: &Client, key: &str, choice: Choice, clear: bool) -> Delivery {
    let reply = client
        .request(Request::NeedsYouAnswer(NeedsYouAnswerParams {
            key: key.into(),
            choice,
            clear,
        }))
        .await
        .unwrap();
    let Reply::Delivery(d) = reply else {
        panic!("{reply:?}")
    };
    d
}

async fn in_needs_you(w: &TestService, key: &str) -> bool {
    w.board
        .needs_you_items(now_ms())
        .await
        .unwrap()
        .iter()
        .any(|i| i.key.to_string() == key)
}

#[tokio::test(flavor = "multi_thread")]
async fn an_answer_is_typed_into_the_session_and_clears_the_item() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let t = w.create("SHOP", "Order confirmation email").await;
    w.force_status(&t, Status::Working).await;
    let session = cat_agent(&w, &t).await;
    asks(&w, &t, &session, &["Postmark", "SES", "Resend"]).await;
    assert!(in_needs_you(&w, "SHOP-1").await);
    let question = w.board.open_question(&t.id).await.unwrap().unwrap();

    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    assert_eq!(
        answer(&client, "SHOP-1", Choice::Option(1), false).await,
        Delivery::Sent { count: 1 }
    );
    w.wait_for_output(&session, "Answer to your question: Postmark (option 1).")
        .await;
    assert_eq!(
        w.status("SHOP-1").await,
        Status::Working,
        "turquoise cleared"
    );
    assert!(!in_needs_you(&w, "SHOP-1").await);
    assert!(w.board.open_question(&t.id).await.unwrap().is_none());
    let (stored, at): (Option<String>, Option<i64>) =
        sqlx::query_as("SELECT answer, answered_at FROM human_questions WHERE id = ?")
            .bind(&question.id)
            .fetch_one(w.board.store().pool())
            .await
            .unwrap();
    assert_eq!(
        stored.as_deref(),
        Some("Answer to your question: Postmark (option 1).")
    );
    assert!(at.is_some());

    // Idle takes a free-text answer as written.
    w.force_status(&t, Status::Idle).await;
    assert_eq!(
        answer(
            &client,
            "SHOP-1",
            Choice::Other("Yes, update the tests too".into()),
            false
        )
        .await,
        Delivery::Sent { count: 1 }
    );
    w.wait_for_output(&session, "Yes, update the tests too")
        .await;
    assert_eq!(w.status("SHOP-1").await, Status::Working);

    // Only waiting items take answers; a wrong option is refused.
    let refused = client
        .request(Request::NeedsYouAnswer(NeedsYouAnswerParams {
            key: "SHOP-1".into(),
            choice: Choice::Other("x".into()),
            clear: false,
        }))
        .await
        .unwrap_err();
    assert!(
        refused.to_string().contains("isn't waiting for an answer"),
        "{refused}"
    );
    w.force_status(&t, Status::NeedsYou).await;
    assert_eq!(
        w.needs_you
            .answer("SHOP-1", &Choice::Other("y".into()), false, &me())
            .await
            .unwrap_err()
            .refusal(),
        Some(Refusal::MoveRefused),
        "a permission prompt is answered in the terminal"
    );
    w.force_status(&t, Status::Working).await;
    asks(&w, &t, &session, &["Postmark", "SES"]).await;
    assert_eq!(
        w.needs_you
            .answer("SHOP-1", &Choice::Option(3), false, &me())
            .await
            .unwrap_err()
            .refusal(),
        Some(Refusal::BadQuestion)
    );
    w.sessions.stop(&session).await.unwrap();
    w.stop().await;
}

/// A question that takes several answers: two options and Other… ticked are typed as one answer naming each, and the
/// ticket's Messages log it the same. A question stored before several answers existed still takes one pick.
#[tokio::test(flavor = "multi_thread")]
async fn several_answers_are_typed_and_logged_together() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let t = w.create("SHOP", "Order confirmation email").await;
    w.force_status(&t, Status::Working).await;
    let session = cat_agent(&w, &t).await;
    let agent = Caller::Agent {
        ticket: t.key.clone(),
        ticket_id: t.id.clone(),
        session: session.clone(),
    };
    let question = "Which email providers should the fallback chain use?";
    let stored = w
        .board
        .store_question(
            &t.id,
            question,
            &["Postmark".into(), "SES".into(), "Resend".into()],
            &[],
            &[3, 1],
            true,
        )
        .await
        .unwrap();
    assert_eq!(
        (stored.recommended.clone(), stored.multiple),
        (vec![1, 3], true)
    );
    w.signals.ask_human(&agent, &t.id, question).await.unwrap();
    let open = w.board.open_question(&t.id).await.unwrap().unwrap();
    assert_eq!((open.recommended, open.multiple), (vec![1, 3], true));

    // An option that isn't there is refused, and so is ticking nothing.
    for nothing in [
        Choice::Several {
            options: vec![1, 4],
            other: None,
        },
        Choice::Several {
            options: Vec::new(),
            other: None,
        },
    ] {
        assert!(
            w.needs_you
                .answer("SHOP-1", &nothing, false, &me())
                .await
                .is_err()
        );
    }
    let typed =
        "Answer to your question: Postmark (option 1), SES (option 2). Also: Mailgun for receipts";
    assert_eq!(
        w.needs_you
            .answer(
                "SHOP-1",
                &Choice::Several {
                    options: vec![1, 2],
                    other: Some(" Mailgun for receipts ".into()),
                },
                false,
                &me()
            )
            .await
            .unwrap(),
        Delivery::Sent { count: 1 }
    );
    w.wait_for_output(&session, typed).await;
    let logged = w.board.answered_human_questions(&t.id).await.unwrap();
    assert_eq!(
        logged
            .iter()
            .map(|a| (a.question.as_str(), a.answer.as_str()))
            .collect::<Vec<_>>(),
        [(question, typed)]
    );

    // A question stored before: no `multiple`, one recommended option. It reads as one answer and takes one pick.
    sqlx::query(
        "INSERT INTO human_questions (id, ticket_id, question, options_json, recommended, asked_at)
         VALUES ('old', ?, 'Which one?', '[\"Postmark\",\"SES\"]', 2, ?)",
    )
    .bind(&t.id)
    .bind(now_ms())
    .execute(w.board.store().pool())
    .await
    .unwrap();
    w.signals
        .ask_human(&agent, &t.id, "Which one?")
        .await
        .unwrap();
    let old = w.board.open_question(&t.id).await.unwrap().unwrap();
    assert_eq!(
        (old.id.as_str(), old.recommended.clone(), old.multiple),
        ("old", vec![2], false)
    );
    assert_eq!(
        w.needs_you
            .answer(
                "SHOP-1",
                &Choice::Several {
                    options: vec![1, 2],
                    other: None
                },
                false,
                &me()
            )
            .await
            .unwrap_err()
            .refusal(),
        Some(Refusal::BadQuestion),
        "one answer only"
    );
    assert_eq!(
        w.needs_you
            .answer("SHOP-1", &Choice::Option(2), false, &me())
            .await
            .unwrap(),
        Delivery::Sent { count: 1 }
    );
    w.wait_for_output(&session, "Answer to your question: SES (option 2).")
        .await;
    w.sessions.stop(&session).await.unwrap();
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn unsent_text_asks_first_and_clear_sends() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let t = w.create("SHOP", "Order confirmation email").await;
    w.force_status(&t, Status::Working).await;
    let session = cat_agent(&w, &t).await;
    asks(&w, &t, &session, &[]).await;
    // The user has typed something in the terminal without sending it.
    w.sessions.input(&session, b"half a thought").unwrap();

    let d = w
        .needs_you
        .answer("SHOP-1", &Choice::Other("Postmark".into()), false, &me())
        .await
        .unwrap();
    assert!(matches!(d, Delivery::NeedsConfirmation { .. }), "{d:?}");
    tokio::time::sleep(Duration::from_millis(200)).await;
    let out =
        String::from_utf8_lossy(&w.sessions.attach(&session).unwrap().scrollback).into_owned();
    assert!(!out.contains("Postmark"), "nothing typed: {out:?}");
    assert_eq!(w.status("SHOP-1").await, Status::AsksYou, "still waiting");

    // "Clear it and send".
    let d = w
        .needs_you
        .answer("SHOP-1", &Choice::Other("Postmark".into()), true, &me())
        .await
        .unwrap();
    assert_eq!(d, Delivery::Sent { count: 1 });
    w.wait_for_output(&session, "Postmark").await;
    assert_eq!(w.status("SHOP-1").await, Status::Working);
    w.sessions.stop(&session).await.unwrap();
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_interrupted_question_resumes_and_delivers_in_one_click() {
    let launch = |config: ServiceConfig| ServiceConfig {
        launch: Some(LaunchContext {
            bin_dir: "/nonexistent".into(),
            plugin_dir: "/nonexistent/plugin".into(),
            path: std::env::var_os("PATH").unwrap_or_default(),
            claude: ClaudeLaunch {
                program: vec![
                    "sh".into(),
                    "-c".into(),
                    "printf 'ARGS %s\\n' \"$*\"; exec sleep 30".into(),
                    "sh".into(),
                ],
                config_dir: None,
            },
        }),
        ..config
    };
    let w = TestService::start_with(launch).await;
    w.add_project("shop-web").await;
    let t = w.create("SHOP", "Order confirmation email").await;
    w.board
        .move_ticket("SHOP-1", DropTarget::ReadyForDev, true, &me())
        .await
        .unwrap();
    w.starter
        .start("SHOP-1", &me(), otaq_core::session::Start::Auto)
        .await
        .unwrap();
    let first = w.sessions.running_of(&t.id, SessionKind::Agent).unwrap();
    asks(&w, &t, &first.id, &["Postmark", "SES"]).await;
    let w = w.restart_with(launch).await;
    let interrupted = w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(
        (interrupted.status, interrupted.interrupted_from),
        (Status::Interrupted, Some(Status::AsksYou))
    );

    let d = w
        .needs_you
        .answer("SHOP-1", &Choice::Option(2), false, &me())
        .await
        .unwrap();
    assert_eq!(d, Delivery::Sent { count: 1 });
    let resumed = w.sessions.running_of(&t.id, SessionKind::Agent).unwrap();
    let conversation = first.tool_session_id.unwrap();
    let out = w.wait_for_output(&resumed.id, "ARGS").await;
    // Resumed first, then the answer as the conversation's first prompt.
    assert!(
        out.contains(&format!(
            "--resume {conversation} Answer to your question: SES (option 2)."
        )),
        "{out}"
    );
    assert_eq!(w.status("SHOP-1").await, Status::Working);
    assert!(!in_needs_you(&w, "SHOP-1").await);
    assert!(w.board.open_question(&t.id).await.unwrap().is_none());
    let row = w.board.store().ticket(&t.id).await.unwrap().unwrap();
    assert_eq!(row.return_mode, None);
    assert!(
        row.resume_prompt.is_some(),
        "kept until the agent shows it got it"
    );
    w.signals
        .command(&Caller::Agent {
            ticket: t.key.clone(),
            ticket_id: t.id.clone(),
            session: resumed.id.clone(),
        })
        .await
        .unwrap();
    let row = w.board.store().ticket(&t.id).await.unwrap().unwrap();
    assert_eq!(row.resume_prompt, None);
    w.sessions.stop(&resumed.id).await.unwrap();
    w.stop().await;
}
