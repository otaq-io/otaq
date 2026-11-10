//! An agent asks the user to send its ticket back to an earlier stage (`otaq ask human --send-back <stage>`): on the
//! test flow (`tests/flows/every-kind.yaml`: Product spec, which the user may skip · Dev · Validate · Security · QA),
//! a work agent in Dev or a checker in Validate names an earlier stage the ticket is past, with its reason. The
//! question shows as Needs you; the user sends the ticket back there (a skip on it is cleared, since only the user
//! skips a stage and so only the user undoes a skip) or answers here, and the ticket stays. An agent never moves a
//! ticket back itself, and can't name a later stage, its own, or one the ticket never went through. Requests are sent
//! from inside the sessions, so each is judged as its caller; `claude` is stood in for by ordinary commands.

use std::path::Path;
use std::time::Duration;

use otaq_core::agent_tools::LaunchContext;
use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::board::questions::Choice;
use otaq_core::board::ticket_comments::{CommentAuthor, TicketComment};
use otaq_core::board::tickets::{EventKind, Ticket};
use otaq_core::client::{Client, ClientConfig};
use otaq_core::domain::{Place, Status};
use otaq_core::flow::{FlowFile, yaml};
use otaq_core::git::testing::commit;
use otaq_core::protocol::{NeedsYouAnswerParams, Reply, Request};
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::{AgentShell, TestService, path_without_build_folders};
use otaq_core::session::SessionKind;
use otaq_core::session::deliveries::Delivery;
use otaq_core::store::{new_id, now_ms};
use serde_json::{Value, json};

const REASON: &str = "Product gap: refunds over 30 days are not specified.";

/// Keeps running, printing its arguments.
const KEEPS_RUNNING: &str = "printf 'ARGS %s\\n' \"$*\"; exec sleep 30";

async fn world() -> TestService {
    let w = TestService::start_with(|config| ServiceConfig {
        launch: Some(LaunchContext {
            bin_dir: "/nonexistent".into(),
            plugin_dir: "/nonexistent/plugin".into(),
            path: path_without_build_folders().into(),
            claude: ClaudeLaunch {
                program: vec!["sh".into(), "-c".into(), KEEPS_RUNNING.into(), "sh".into()],
                config_dir: None,
            },
        }),
        ..config
    })
    .await;
    w.add_project("shop-web").await;
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/flows/every-kind.yaml");
    let file: FlowFile = yaml::read(&std::fs::read_to_string(path).unwrap()).unwrap();
    w.board.use_flow_for_tests("SHOP", &file).await;
    w
}

async fn place_in(w: &TestService, t: &Ticket, stage: &str, status: Status) {
    sqlx::query("UPDATE tickets SET stage_id = ?, state = ?, state_since = ? WHERE id = ?")
        .bind(stage)
        .bind(status.as_str())
        .bind(now_ms())
        .bind(&t.id)
        .execute(w.board.store().pool())
        .await
        .unwrap();
}

/// The ticket passed Product spec as skipped: its run of result `skipped`, as the flow engine records it. (Runs of
/// the stages it enters are opened by the store as it moves.)
async fn skipped_run(w: &TestService, t: &Ticket) {
    sqlx::query(
        "INSERT INTO stage_runs (id, ticket_id, stage_id, kind, result, started_at, ended_at)
         VALUES (?, ?, 'product-spec', 'agent', 'skipped', ?, ?)",
    )
    .bind(new_id())
    .bind(&t.id)
    .bind(now_ms())
    .bind(now_ms())
    .execute(w.board.store().pool())
    .await
    .unwrap();
}

async fn skips_product_spec(w: &TestService, t: &Ticket) {
    sqlx::query(
        "INSERT INTO ticket_stage_choices (ticket_id, stage_id, choice, set_by, set_at)
         VALUES (?, 'product-spec', 'skip', 'user', 1)",
    )
    .bind(&t.id)
    .execute(w.board.store().pool())
    .await
    .unwrap();
    skipped_run(w, t).await;
}

/// A ticket that skipped Product spec and works in Dev, with its Dev agent's session.
async fn in_dev(w: &TestService, title: &str) -> (Ticket, AgentShell) {
    let t = w.create("SHOP", title).await;
    let wt = w.start_worktree(&t).await;
    commit(&wt, "notes.txt", &format!("{title}\n"));
    skips_product_spec(w, &t).await;
    place_in(w, &t, "dev", Status::Working).await;
    let shell = w.agent_shell(&t).await;
    (w.board.ticket(&t.key.to_string()).await.unwrap(), shell)
}

/// One request from inside the session.
async fn send(w: &TestService, shell: &AgentShell, method: &str, params: Value) -> Value {
    let request = json!({"protocol_version": otaq_core::protocol::PROTOCOL_VERSION, "id": 1, "request": {"method": method, "params": params}});
    let ran = shell
        .send_raw(&w.data.socket_path(), &request.to_string())
        .await;
    serde_json::from_str(ran.stdout.lines().next().unwrap_or_default())
        .unwrap_or_else(|e| panic!("{e}: {ran:?}"))
}

/// Names the agent `session` runs, as a real launch records it.
async fn runs_agent(w: &TestService, session: &str, id: &str, name: &str) {
    sqlx::query("UPDATE sessions SET agent_id = ?, agent_name = ? WHERE id = ?")
        .bind(id)
        .bind(name)
        .bind(session)
        .execute(w.board.store().pool())
        .await
        .unwrap();
}

/// The ticket's latest comment, which must be the asking agent's: by `name`, written from `stage`.
async fn agents_reason(w: &TestService, key: &str, name: &str, stage: &str) -> TicketComment {
    let comment = crate::common::last_comment(&w.board, key).await.unwrap();
    match &comment.author {
        CommentAuthor::Agent {
            name: by,
            stage: from,
            ..
        } => {
            assert_eq!(by.as_deref(), Some(name), "{comment:?}");
            assert_eq!(from.as_deref(), Some(stage), "{comment:?}");
        }
        CommentAuthor::User { .. } => panic!("the reason is posted as the user's: {comment:?}"),
    }
    assert_eq!(comment.author_label(), name);
    assert!(comment.heading().starts_with(name), "{}", comment.heading());
    assert!(comment.body.contains(REASON), "{}", comment.body);
    comment
}

/// The agent asks to send `t` back to `stage`; the service's answer.
async fn asks_back(w: &TestService, shell: &AgentShell, t: &Ticket, stage: &str) -> Value {
    send(
        w,
        shell,
        "ticket.ask_human",
        json!({"key": t.key, "question": REASON, "send_back": stage}),
    )
    .await
}

fn refusal(answer: &Value) -> String {
    answer["error"]["message"]
        .as_str()
        .unwrap_or_else(|| panic!("refused: {answer}"))
        .to_owned()
}

async fn answer(w: &TestService, key: &str, choice: Choice) -> Delivery {
    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    let reply = client
        .request(Request::NeedsYouAnswer(NeedsYouAnswerParams {
            key: key.into(),
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

async fn in_needs_you(w: &TestService, key: &str) -> bool {
    w.board
        .needs_you_items(now_ms())
        .await
        .unwrap()
        .iter()
        .any(|i| i.key.to_string() == key)
}

async fn until_status(w: &TestService, key: &str, want: Status) {
    let reached = tokio::time::timeout(Duration::from_secs(20), async {
        while w.status(key).await != want {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await;
    assert!(reached.is_ok(), "{key} never became {want}");
}

async fn skips(w: &TestService, key: &str) -> Vec<String> {
    let t = w.board.ticket(key).await.unwrap();
    let flow = w.board.effective_flow("SHOP").await.unwrap();
    w.board.ticket_skips(&t, &flow).await.unwrap()
}

/// `(answer, answered_via)` of the ticket's latest question.
async fn last_answer(w: &TestService, t: &Ticket) -> (Option<String>, Option<String>) {
    sqlx::query_as(
        "SELECT answer, answered_via FROM human_questions WHERE ticket_id = ? ORDER BY asked_at DESC LIMIT 1",
    )
    .bind(&t.id)
    .fetch_one(w.board.store().pool())
    .await
    .unwrap()
}

/// Dev's agent finds a product gap: its question shows as Needs you, naming Product spec; Send back moves the ticket
/// there, clears the skip, stops the asking session and starts Product spec fresh with the reason as the asking
/// agent's comment, by its name; History records the reason and the answer.
#[tokio::test(flavor = "multi_thread")]
async fn send_back_moves_the_ticket_to_the_named_stage_and_clears_its_skip() {
    let w = world().await;
    let (t, shell) = in_dev(&w, "Refunds").await;
    runs_agent(&w, &shell.session, "rust-dev", "Rust developer").await;
    assert_eq!(skips(&w, "SHOP-1").await, ["product-spec"]);

    let asked = asks_back(&w, &shell, &t, "Product spec").await;
    assert!(asked.get("error").is_none(), "{asked}");
    assert_eq!(w.status("SHOP-1").await, Status::AsksYou);
    assert!(
        in_needs_you(&w, "SHOP-1").await,
        "the question shows as Needs you"
    );
    let question = w.board.open_question(&t.id).await.unwrap().unwrap();
    let to = question.send_back.expect("it names the stage");
    assert_eq!(
        (to.stage_id.as_str(), to.stage.as_str()),
        ("product-spec", "Product spec")
    );
    assert!(question.options.is_empty());
    assert_eq!(
        w.board.ticket("SHOP-1").await.unwrap().place,
        Place::stage_of("dev"),
        "asking moves nothing"
    );

    assert_eq!(
        answer(&w, "SHOP-1", Choice::SendBack).await,
        Delivery::Sent { count: 1 }
    );
    until_status(&w, "SHOP-1", Status::Working).await;
    let after = w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(after.place, Place::stage_of("product-spec"));
    assert!(skips(&w, "SHOP-1").await.is_empty(), "the skip is cleared");
    assert!(!in_needs_you(&w, "SHOP-1").await);
    assert_eq!(
        last_answer(&w, &t).await,
        (
            Some("Sent back to Product spec.".into()),
            Some("box".into())
        )
    );
    // The asking agent's session stops, as on any drag back; Product spec starts fresh with its skill.
    let running = w.sessions.running_of(&t.id, SessionKind::Agent).unwrap();
    assert_ne!(running.id, shell.session, "the asking session stopped");
    let out = w.wait_for_output(&running.id, "ARGS").await;
    assert!(out.contains("/otaq:product-spec"), "{out}");
    let comment = agents_reason(&w, "SHOP-1", "Rust developer", "dev")
        .await
        .body;
    assert_eq!(
        comment.trim(),
        format!("Dev sent it back to Product spec: {REASON}"),
        "each stage is named once"
    );
    let event = w
        .board
        .history("SHOP-1")
        .await
        .unwrap()
        .into_iter()
        .rfind(|e| e.kind == EventKind::Dragged)
        .expect("a Dragged event");
    assert_eq!(event.payload["stage"], "Product spec");
    assert_eq!(event.payload["asked_by"], "Dev");
    assert_eq!(event.payload["reason"], REASON);
    assert_eq!(event.payload["skip_cleared"], true);
    w.stop().await;
}

/// Answer here: the answer goes into the asking agent's session, the ticket stays in Dev and the skip stays.
#[tokio::test(flavor = "multi_thread")]
async fn answer_here_keeps_the_ticket_where_it_is() {
    let w = world().await;
    let (t, shell) = in_dev(&w, "Refunds").await;
    let asked = asks_back(&w, &shell, &t, "product-spec").await;
    assert!(asked.get("error").is_none(), "{asked}");

    assert_eq!(
        answer(
            &w,
            "SHOP-1",
            Choice::Other("Store credit only after 30 days".into())
        )
        .await,
        Delivery::Sent { count: 1 }
    );
    w.wait_for_output(&shell.session, "Store credit only after 30 days")
        .await;
    let after = w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(after.place, Place::stage_of("dev"));
    assert_eq!(after.status, Status::Working, "Needs you cleared");
    assert_eq!(
        skips(&w, "SHOP-1").await,
        ["product-spec"],
        "the skip stays"
    );
    assert_eq!(
        last_answer(&w, &t).await,
        (
            Some("Store credit only after 30 days".into()),
            Some("box".into())
        )
    );
    assert!(
        w.sessions
            .running_of(&t.id, SessionKind::Agent)
            .is_some_and(|s| s.id == shell.session),
        "the asking session goes on"
    );
    w.stop().await;
}

/// The agent names only an earlier stage the ticket is past: a later stage, its own stage, a stage the flow hasn't,
/// or one the ticket never went through is refused, and nothing waits for the user. A send-back question takes no
/// options. Only the user's answer sends a ticket back: Send back on a question that names no stage is refused.
#[tokio::test(flavor = "multi_thread")]
async fn an_agent_names_only_an_earlier_stage_the_ticket_is_past() {
    let w = world().await;
    let (t, shell) = in_dev(&w, "Refunds").await;
    assert_eq!(
        refusal(&asks_back(&w, &shell, &t, "Validate").await),
        "Validate isn't before Dev: a ticket is sent back only to an earlier stage"
    );
    assert_eq!(
        refusal(&asks_back(&w, &shell, &t, "Dev").await),
        "Dev isn't before Dev: a ticket is sent back only to an earlier stage"
    );
    assert!(
        refusal(&asks_back(&w, &shell, &t, "Design").await)
            .starts_with("SHOP-1's flow has no stage Design"),
    );
    let with_options = send(
        &w,
        &shell,
        "ticket.ask_human",
        json!({"key": t.key, "question": REASON, "send_back": "Product spec", "options": ["Yes", "No"]}),
    )
    .await;
    assert_eq!(
        refusal(&with_options),
        "A send-back question takes no --option: its two ways out are Send back and Answer here"
    );
    assert_eq!(w.status("SHOP-1").await, Status::Working, "nothing asked");
    assert!(w.board.open_question(&t.id).await.unwrap().is_none());

    // A ticket that never went through Product spec (it started in Dev under a flow without it).
    let other = w.create("SHOP", "Never specced").await;
    place_in(&w, &other, "dev", Status::Working).await;
    let other_shell = w.agent_shell(&other).await;
    assert_eq!(
        refusal(&asks_back(&w, &other_shell, &other, "Product spec").await),
        "SHOP-2 hasn't been through Product spec: a ticket is sent back only to a stage it is past"
    );

    // An ordinary question can't be answered with Send back.
    let plain = send(
        &w,
        &shell,
        "ticket.ask_human",
        json!({"key": t.key, "question": "Which provider?"}),
    )
    .await;
    assert!(plain.get("error").is_none(), "{plain}");
    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    let refused = client
        .request(Request::NeedsYouAnswer(NeedsYouAnswerParams {
            key: "SHOP-1".into(),
            choice: Choice::SendBack,
            clear: false,
        }))
        .await
        .unwrap_err();
    assert!(
        refused
            .to_string()
            .contains("SHOP-1's agent didn't ask to send it back"),
        "{refused}"
    );
    assert_eq!(
        w.board.ticket("SHOP-1").await.unwrap().place,
        Place::stage_of("dev")
    );
    w.stop().await;
}

/// A checker asks too: from Validate it names Product spec; Send back stops its round and the ticket goes there.
#[tokio::test(flavor = "multi_thread")]
async fn a_checker_asks_and_the_ticket_goes_back_from_its_check() {
    let w = world().await;
    let t = w.create("SHOP", "Refunds").await;
    let wt = w.start_worktree(&t).await;
    commit(&wt, "notes.txt", "Refunds\n");
    skips_product_spec(&w, &t).await;
    place_in(&w, &t, "dev", Status::Working).await;
    w.record_ended_agent(&t, "refunds-dev").await;
    let checker = w.shell(&t, SessionKind::Validator).await;
    runs_agent(&w, &checker.session, "reviewer", "Reviewer").await;
    w.validating_with(&t, &checker.session).await;
    place_in(&w, &t, "validate", Status::Checking).await;

    let asked = asks_back(&w, &checker, &t, "Product spec").await;
    assert!(asked.get("error").is_none(), "{asked}");
    assert_eq!(w.status("SHOP-1").await, Status::AsksYou);
    assert!(in_needs_you(&w, "SHOP-1").await);

    assert_eq!(
        answer(&w, "SHOP-1", Choice::SendBack).await,
        Delivery::Sent { count: 1 }
    );
    until_status(&w, "SHOP-1", Status::Working).await;
    let after = w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(after.place, Place::stage_of("product-spec"));
    assert!(skips(&w, "SHOP-1").await.is_empty());
    assert!(
        w.sessions
            .running_of(&t.id, SessionKind::Validator)
            .is_none(),
        "the checker stopped"
    );
    agents_reason(&w, "SHOP-1", "Reviewer", "validate").await;
    w.stop().await;
}
