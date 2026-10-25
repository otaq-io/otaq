//! Validation reports in History, and Create ticket from a note (T-0195 core; VAL-12, d0063): every round with its
//! outcome, duration, report, notes and tokens over the socket, newest first; a "Not covered by any ticket" note
//! becomes a prefilled Backlog ticket on the user's click. `claude` stands in as `sh`.

use otaq_core::agent_tools::LaunchContext;
use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::board::tickets::{NewTicketInput, Ticket};
use otaq_core::client::{Client, ClientConfig};
use otaq_core::domain::{Actor, Status, StatusSet, ValidationOutcome, ValidationTrigger};
use otaq_core::protocol::{KeyParams, NoteParams, Reply, Request};
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::TestService;
use otaq_core::service::testing::said;
use otaq_core::session::SessionKind;

async fn world() -> TestService {
    TestService::start_with(|config| ServiceConfig {
        launch: Some(LaunchContext {
            bin_dir: "/nonexistent".into(),
            plugin_dir: "/nonexistent/plugin".into(),
            path: std::env::var_os("PATH").unwrap_or_default(),
            claude: ClaudeLaunch {
                program: vec![
                    "sh".into(),
                    "-c".into(),
                    "exec sleep 30".into(),
                    "sh".into(),
                ],
                config_dir: None,
            },
        }),
        ..config
    })
    .await
}

/// SHOP-1 Save items, working with a committed worktree.
async fn working_ticket(w: &TestService) -> Ticket {
    w.add_project("shop-web").await;
    let me = Actor::current_user();
    let child = w
        .board
        .create_ticket(
            &NewTicketInput {
                project: "SHOP".into(),
                title: "Save items".into(),
                ..NewTicketInput::default()
            },
            &me,
        )
        .await
        .unwrap();
    w.give_worktree(&child).await;
    w.force_status(&child, Status::Working).await;
    w.board.ticket(&child.key.to_string()).await.unwrap()
}

async fn round(w: &TestService, t: &Ticket, pass: bool, report: &str) {
    let key = t.key.to_string();
    w.validation
        .submit(
            &key,
            said(&w.board, &key, "Done.").await.as_ref(),
            &Actor::Agent(t.key.clone()),
            ValidationTrigger::Agent,
        )
        .await
        .unwrap();
    let session = w
        .sessions
        .running_of(&t.id, SessionKind::Validator)
        .expect("a validator runs")
        .id;
    w.validation
        .verdict(
            &key,
            pass,
            said(&w.board, &key, report).await.as_ref(),
            &session,
        )
        .await
        .unwrap();
}

const NOTE: &str = "Not covered by any ticket: refunds endpoint";

#[tokio::test(flavor = "multi_thread")]
async fn history_lists_every_round_with_its_report() {
    let w = world().await;
    let t = working_ticket(&w).await;
    let _agent = w.agent_shell(&t).await;
    round(&w, &t, false, "src/wishlist.ts:4 items aren't saved").await;
    round(&w, &t, true, &format!("All checks pass.\n- Note: {NOTE}")).await;

    let app = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    let Reply::Validations(rounds) = app
        .request(Request::TicketValidations(KeyParams {
            key: t.key.to_string(),
        }))
        .await
        .unwrap()
    else {
        panic!("validations reply");
    };
    let shown: Vec<_> = rounds
        .iter()
        .map(|r| (r.round, r.outcome, r.notes.clone()))
        .collect();
    assert_eq!(
        shown,
        [
            (2, Some(ValidationOutcome::Pass), vec![NOTE.to_owned()]),
            (1, Some(ValidationOutcome::Fail), vec![]),
        ],
        "newest first"
    );
    assert_eq!(
        rounds[1].report.as_deref(),
        Some("src/wishlist.ts:4 items aren't saved")
    );
    for r in &rounds {
        assert_eq!(r.trigger, ValidationTrigger::Agent);
        assert!(r.duration_ms.is_some_and(|d| d >= 0), "{r:?}");
        assert!(r.session.is_some());
    }
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn create_ticket_from_a_note() {
    let w = world().await;
    let t = working_ticket(&w).await;
    round(
        &w,
        &t,
        true,
        &format!("Works.\n- Note: {NOTE}\n- Note: covered by SHOP-2"),
    )
    .await;

    let app = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    let create = |note: &str| {
        Request::TicketCreateFromNote(NoteParams {
            key: t.key.to_string(),
            note: note.into(),
        })
    };
    let Reply::Ticket(created) = app.request(create(NOTE)).await.unwrap() else {
        panic!("ticket reply");
    };
    assert_eq!(created.title, "Refunds endpoint");
    assert_eq!(
        (created.board_group(), created.status),
        (StatusSet::Backlog, Status::Backlog)
    );
    assert!(
        created.body.contains(&t.key.to_string()),
        "{}",
        created.body
    );
    assert!(created.body.contains(NOTE), "{}", created.body);
    assert!(!created.created_by.is_agent(), "the user's click");

    // A note about something a ticket covers, or one not in the reports, makes nothing.
    for note in [
        "covered by SHOP-2",
        "Not covered by any ticket: something else",
    ] {
        let refused = app.request(create(note)).await;
        assert!(refused.is_err(), "{note}: {refused:?}");
    }
    assert_eq!(w.board.tickets("SHOP").await.unwrap().len(), 2);
    w.stop().await;
}
