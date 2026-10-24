//! An open question holds Needs you (T-0489; ATT-01, ATT-02, ATT-05; d0119, d0042, d0027, d0083, d0068): the
//! status line's bookkeeping and the agent's reads are no signal; an open `human_questions` row keeps Asks you with
//! its question whatever hooks say, with a permission prompt on top; Enter on a line in the agent's terminal (or the
//! Ask bar's Continue) answers it, a new question replaces it, closing or parking the ticket closes it; the strip
//! answers only the open question. Requests are sent over the socket from inside the sessions (an ordinary `sh` as
//! each agent, running `nc`), so each is judged as its caller. No test starts `claude`.

use std::time::Duration;

use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::agent_tools::{LaunchContext, Tool};
use otaq_core::board::Refusal;
use otaq_core::board::questions::Choice;
use otaq_core::board::tickets::Ticket;
use otaq_core::caller::Caller;
use otaq_core::client::{Client, ClientConfig};
use otaq_core::domain::{Actor, Status};
use otaq_core::git::testing::TempRepos;
use otaq_core::project_agents::lead::{Lead, LeadStatus};
use otaq_core::protocol::{InputParams, Request, encode_bytes};
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::{AgentShell, TestService, path_without_build_folders};
use otaq_core::session::SessionKind;
use otaq_core::signals::Signal;
use serde_json::{Value, json};

const QUESTION: &str = "Which email provider should I use?";

/// The sample board (SHOP-40 Wishlist with 40.2 Working); `claude` is an interactive `sh` in the service's data
/// folder, so Project agents and leads can open.
async fn world() -> TestService {
    let home = tempfile::TempDir::new().unwrap();
    let data = home.path().join("h");
    let script = format!(
        "export {}='{}'; export PS1=''; exec sh",
        otaq_core::data_dir::DataDir::env_var(),
        data.display()
    );
    let w = TestService::start_in(home, TempRepos::new(), |config| ServiceConfig {
        launch: Some(LaunchContext {
            bin_dir: "/nonexistent".into(),
            plugin_dir: "/nonexistent/plugin".into(),
            path: path_without_build_folders().into(),
            claude: ClaudeLaunch {
                program: vec!["sh".into(), "-c".into(), script, "sh".into()],
                config_dir: None,
            },
        }),
        ..config
    })
    .await;
    w.shop_sample().await;
    w
}

/// A Working ticket of its own with its agent's session.
async fn working(w: &TestService, title: &str) -> (Ticket, AgentShell) {
    let t = w.create("SHOP", title).await;
    w.start_worktree(&t).await;
    w.force_status(&t, Status::Working).await;
    let shell = w.agent_shell(&t).await;
    (w.board.ticket(&t.key.to_string()).await.unwrap(), shell)
}

fn caller(t: &Ticket, shell: &AgentShell) -> Caller {
    Caller::Agent {
        ticket: t.key.clone(),
        ticket_id: t.id.clone(),
        session: shell.session.clone(),
    }
}

/// One request from inside the session.
async fn send(w: &TestService, shell: &AgentShell, method: &str, params: Value) -> Value {
    let request =
        json!({"protocol_version": 1, "id": 1, "request": {"method": method, "params": params}});
    let ran = shell
        .send_raw(&w.data.socket_path(), &request.to_string())
        .await;
    serde_json::from_str(ran.stdout.lines().next().unwrap_or_default())
        .unwrap_or_else(|e| panic!("{e}: {ran:?}"))
}

fn ok(answer: &Value) -> &Value {
    assert!(answer.get("error").is_none(), "{answer}");
    &answer["reply"]
}

async fn asks(w: &TestService, t: &Ticket, shell: &AgentShell, question: &str) {
    ok(&send(
        w,
        shell,
        "ticket.ask_human",
        json!({"key": t.key, "question": question, "options": ["Postmark", "SES"], "recommended": 1}),
    )
    .await);
}

/// The user typing `keys` into `session`, as the app relays them.
async fn type_in(w: &TestService, session: &str, keys: &[u8]) {
    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    client
        .request(Request::SessionInput(InputParams {
            session: session.to_owned(),
            data: encode_bytes(keys),
            reply: false,
        }))
        .await
        .unwrap();
}

async fn hook(w: &TestService, who: &Caller, event: &str, payload: Value) {
    w.signals
        .hook(who, Tool::Claude, event, &payload)
        .await
        .unwrap();
}

/// `(answer, answered_via)` of every question of the ticket, oldest first.
async fn rows(
    w: &TestService,
    owner: &str,
    id: &str,
) -> Vec<(String, Option<String>, Option<String>)> {
    sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT question, answer, answered_via FROM human_questions WHERE {owner} = ? ORDER BY asked_at, rowid"
    )))
    .bind(id)
    .fetch_all(w.board.store().pool())
    .await
    .unwrap()
}

/// The strip's line for the ticket, if it is listed.
async fn strip_line(w: &TestService, key: &str) -> Option<String> {
    w.inbox
        .list()
        .await
        .unwrap()
        .items
        .into_iter()
        .find(|i| i.key.to_string() == key)
        .map(|i| i.reason)
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

/// The dogfood reproduction (#37): `ask human`, then the status line's `usage.record_limits`, with a `ticket.prime`
/// and a `diff.files` read in between. The ticket stays Asks you with its question in the strip, and none of them
/// moves the last signal; a hook still does (a busy agent's hooks keep it from looking stale).
#[tokio::test(flavor = "multi_thread")]
async fn the_status_line_and_reads_keep_the_question() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let (t, shell) = working(&w, "Order confirmation email").await;
    asks(&w, &t, &shell, QUESTION).await;
    let asked = w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(asked.status, Status::AsksYou);
    let last = asked.flags.last_signal_at.unwrap();
    tokio::time::sleep(Duration::from_millis(5)).await;

    let limits = json!({"tool": "claude", "windows": [
        {"key": "five_hour", "length_minutes": 300, "used_percent": 23.5},
        {"key": "seven_day", "length_minutes": 10080, "used_percent": 41.2},
    ]});
    ok(&send(&w, &shell, "usage.record_limits", limits.clone()).await);
    ok(&send(&w, &shell, "ticket.prime", json!({"key": "SHOP-1"})).await);
    ok(&send(&w, &shell, "diff.files", json!({"key": "SHOP-1"})).await);
    ok(&send(&w, &shell, "usage.record_limits", limits).await);

    let t = w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(t.status, Status::AsksYou);
    assert_eq!(t.question.as_ref().map(|q| q.text.as_str()), Some(QUESTION));
    assert_eq!(
        t.flags.last_signal_at,
        Some(last),
        "reads and bookkeeping are no signal"
    );
    assert_eq!(
        strip_line(&w, "SHOP-1").await.as_deref(),
        Some(&*format!("asks: {QUESTION}"))
    );

    // The turn's Stop keeps it, and a hook is a signal.
    hook(
        &w,
        &caller(&t, &shell),
        "Stop",
        json!({"last_assistant_message": "Waiting."}),
    )
    .await;
    let t = w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(t.status, Status::AsksYou);
    assert!(t.flags.last_signal_at.unwrap() > last, "a hook is a signal");
    w.stop().await;
}

/// The hooks can't take it away: a new turn or Idle keeps Asks you; a permission prompt shows on top and gives way to
/// the question again once answered. Enter on an empty line closes nothing; Enter on a line answers it, stored as
/// answered in the terminal, and the ticket leaves Needs you.
#[tokio::test(flavor = "multi_thread")]
async fn enter_on_a_line_answers_it_and_a_prompt_shows_on_top() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let (t, shell) = working(&w, "Order confirmation email").await;
    let me = caller(&t, &shell);
    asks(&w, &t, &shell, QUESTION).await;

    hook(&w, &me, "UserPromptSubmit", json!({"prompt": "go on"})).await;
    hook(
        &w,
        &me,
        "Stop",
        json!({"last_assistant_message": "Which provider?"}),
    )
    .await;
    assert_eq!(w.status("SHOP-1").await, Status::AsksYou);

    hook(
        &w,
        &me,
        "Notification",
        json!({"notification_type": "permission_prompt", "message": "Allow `npm test`?"}),
    )
    .await;
    assert_eq!(w.status("SHOP-1").await, Status::NeedsYou);
    // The prompt's own Enter (a line in its menu) answers the prompt, not the question.
    type_in(&w, &shell.session, b"1\r").await;
    eventually("the question back on top", async || {
        w.status("SHOP-1").await == Status::AsksYou
    })
    .await;
    let t = w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(t.signal.unwrap().text.as_deref(), Some(QUESTION));

    type_in(&w, &shell.session, b"\r").await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(w.status("SHOP-1").await, Status::AsksYou, "an empty line");
    assert_eq!(rows(&w, "ticket_id", &t.id).await[0].2, None);

    type_in(&w, &shell.session, b"SES, we have a contract\r").await;
    eventually("the answer typed in the terminal", async || {
        w.status("SHOP-1").await == Status::Working
    })
    .await;
    assert_eq!(
        rows(&w, "ticket_id", &t.id).await,
        [(
            QUESTION.to_owned(),
            Some("SES, we have a contract".to_owned()),
            Some("terminal".to_owned())
        )]
    );
    assert_eq!(strip_line(&w, "SHOP-1").await, None);
    // Answered, a later Stop is Idle as usual.
    hook(&w, &me, "UserPromptSubmit", json!({"prompt": "SES"})).await;
    hook(&w, &me, "Stop", json!({"last_assistant_message": "Done."})).await;
    assert_eq!(w.status("SHOP-1").await, Status::Idle);
    w.stop().await;
}

/// A second `ask human` replaces the first (no answer); parking, closing or delivering the ticket closes its open
/// question.
#[tokio::test(flavor = "multi_thread")]
async fn a_new_question_replaces_the_old_and_park_or_close_closes_it() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let (t, shell) = working(&w, "Order confirmation email").await;
    asks(&w, &t, &shell, "Question 1 of about 4: which provider?").await;
    asks(
        &w,
        &t,
        &shell,
        "Question 2 of about 4: which sender address?",
    )
    .await;
    assert_eq!(
        rows(&w, "ticket_id", &t.id).await,
        [
            (
                "Question 1 of about 4: which provider?".to_owned(),
                None,
                Some("replaced".to_owned())
            ),
            (
                "Question 2 of about 4: which sender address?".to_owned(),
                None,
                None
            ),
        ]
    );
    let card = w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(
        card.question.map(|q| q.text).as_deref(),
        Some("Question 2 of about 4: which sender address?")
    );

    let me = Actor::User("me".into());
    w.review.park("SHOP-1", true, &me).await.unwrap();
    assert_eq!(
        rows(&w, "ticket_id", &t.id).await[1].2.as_deref(),
        Some("closed")
    );

    let (t2, shell2) = working(&w, "Invoice footer").await;
    asks(&w, &t2, &shell2, "Which footer text?").await;
    w.review.close("SHOP-2", true, &me).await.unwrap();
    assert_eq!(
        rows(&w, "ticket_id", &t2.id).await,
        [(
            "Which footer text?".to_owned(),
            None,
            Some("closed".to_owned())
        )]
    );

    // Delivered (merged, or a PR opened) closes it too, whichever path moved the ticket.
    for (title, delivered) in [
        ("Receipt PDF", Status::Merged),
        ("Refund mail", Status::PrOpen),
    ] {
        let (t, shell) = working(&w, title).await;
        asks(&w, &t, &shell, "Which template?").await;
        w.force_status(&t, delivered).await;
        assert_eq!(
            rows(&w, "ticket_id", &t.id).await[0].2.as_deref(),
            Some("closed"),
            "{delivered}"
        );
    }
    w.stop().await;
}

/// The strip answers only the open question: once it was answered in the terminal, an answer for it is refused and
/// nothing is typed; an Idle agent's answer is free text, never shaped by an older question.
#[tokio::test(flavor = "multi_thread")]
async fn an_answer_to_a_closed_question_is_refused() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let (t, shell) = working(&w, "Order confirmation email").await;
    asks(&w, &t, &shell, QUESTION).await;
    type_in(&w, &shell.session, b"Postmark\r").await;
    eventually("the answer typed in the terminal", async || {
        w.status("SHOP-1").await == Status::Working
    })
    .await;

    // The strip still showed the chip (Asks you) when the user picked an option.
    w.force_status(&t, Status::AsksYou).await;
    let me = Actor::User("me".into());
    let refused = w
        .inbox
        .answer("SHOP-1", &Choice::Option(1), false, &me)
        .await
        .unwrap_err();
    assert_eq!(refused.refusal(), Some(Refusal::MoveRefused));
    assert_eq!(refused.to_string(), "SHOP-1 isn't waiting for an answer");

    // Idle: the old question's options don't apply.
    w.force_status(&t, Status::Idle).await;
    let refused = w
        .inbox
        .answer("SHOP-1", &Choice::Option(1), false, &me)
        .await
        .unwrap_err();
    assert_eq!(refused.to_string(), "There is no option 1 to choose");
    let out = String::from_utf8_lossy(&w.sessions.attach(&shell.session).unwrap().scrollback)
        .into_owned();
    assert!(!out.contains("Answer to your question"), "{out}");
    w.stop().await;
}

/// An agent that asks during its check keeps Asks you; the agent's command doesn't turn it back into Validating….
#[tokio::test(flavor = "multi_thread")]
async fn a_question_during_a_check_holds() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let (t, shell) = working(&w, "Order confirmation email").await;
    let validator = w.shell(&t, SessionKind::Validator).await;
    w.validating_with(&t, &validator.session).await;
    asks(&w, &t, &shell, QUESTION).await;
    assert_eq!(w.status("SHOP-1").await, Status::AsksYou);
    ok(&send(&w, &shell, "ticket.notes", json!({"key": "SHOP-1"})).await);
    hook(
        &w,
        &caller(&t, &shell),
        "UserPromptSubmit",
        json!({"prompt": "x"}),
    )
    .await;
    assert_eq!(w.status("SHOP-1").await, Status::AsksYou);
    w.stop().await;
}

fn session(lead: &Lead) -> String {
    lead.agent.session.as_ref().expect("a session").id.clone()
}

/// A Project agent's question (#25's shape): Enter on a line in its terminal answers it, and so does the Ask bar's
/// Continue; a permission prompt shows on top, and Enter then answers the prompt only.
#[tokio::test(flavor = "multi_thread")]
async fn a_project_agents_question_closes_from_its_terminal_or_continue() {
    let w = world().await;
    let agent = w
        .project_agents
        .open("SHOP", "Plan the export")
        .await
        .unwrap();
    let id = agent.id.clone();
    let session = agent.session.as_ref().unwrap().id.clone();
    let shell = w.shell_in(&session);
    let ask = async |question: &str| {
        ok(&send(&w, &shell, "agent.ask_human", json!({"question": question})).await);
    };
    let waiting = async || w.inbox.list().await.unwrap().project_agents;

    ask("Import closed issues too?").await;
    assert_eq!(waiting().await[0].status, Status::AsksYou);
    w.project_agents
        .hook(
            &id,
            &Signal::NeedsYou {
                prompt: "Allow gh issue list?".into(),
            },
        )
        .await
        .unwrap();
    assert_eq!(
        waiting().await[0].status,
        Status::NeedsYou,
        "the prompt on top"
    );
    type_in(&w, &session, b"1\r").await;
    eventually("the question back", async || {
        waiting().await.first().map(|a| a.status) == Some(Status::AsksYou)
    })
    .await;
    type_in(&w, &session, b"Only open ones\r").await;
    eventually("the question answered", async || waiting().await.is_empty()).await;

    ask("Split by label?").await;
    w.project_agents.send(&id, "Yes, by label").await.unwrap();
    assert!(waiting().await.is_empty());
    assert_eq!(
        rows(&w, "project_agent_id", &id).await,
        [
            (
                "Import closed issues too?".to_owned(),
                Some("Only open ones".to_owned()),
                Some("terminal".to_owned())
            ),
            (
                "Split by label?".to_owned(),
                Some("Yes, by label".to_owned()),
                Some("terminal".to_owned())
            ),
        ]
    );
    w.stop().await;
}

/// The epic lead (#25): its question shows on the lane ("asks you") and in the strip until the user replies in its
/// terminal or with Continue; then both clear.
#[tokio::test(flavor = "multi_thread")]
async fn the_leads_question_clears_once_answered() {
    let w = world().await;
    let lead = w.project_agents.leads().open("SHOP-40").await.unwrap();
    let id = lead.agent.id.clone();
    let shell = w.shell_in(&session(&lead));
    let status = async || w.project_agents.leads().status("SHOP-40").await.unwrap();
    let ask = async |question: &str| {
        ok(&send(&w, &shell, "agent.ask_human", json!({"question": question})).await);
    };

    ask("Should 40.4 wait for 40.2?").await;
    assert_eq!(status().await, LeadStatus::AsksYou);
    assert_eq!(w.inbox.list().await.unwrap().project_agents.len(), 1);
    type_in(&w, &session(&lead), b"Yes, both touch routes.ts\r").await;
    eventually("the lane clearing", async || {
        status().await != LeadStatus::AsksYou
    })
    .await;
    assert!(w.inbox.list().await.unwrap().project_agents.is_empty());

    ask("Add a child for the export?").await;
    assert_eq!(status().await, LeadStatus::AsksYou);
    w.project_agents.send(&id, "Not yet").await.unwrap();
    assert_ne!(status().await, LeadStatus::AsksYou);
    assert!(w.inbox.list().await.unwrap().project_agents.is_empty());
    w.stop().await;
}
