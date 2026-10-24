//! A child's question goes to its epic's lead first (T-0284; LEAD-06, LEAD-04; d0083, d0030, d0061, d0011, d0016,
//! d0048). Requests are sent over the socket from inside the sessions (an ordinary `sh` as the child's agent and as
//! the lead, running `nc`), so each is judged as its caller. The lead's turns are its hooks, fed as the service gets
//! them. No test starts `claude`.

use std::time::Duration;

use otaq_core::agent_tools::LaunchContext;
use otaq_core::agent_tools::Tool;
use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::attention::notify::should_notify;
use otaq_core::board::questions::{Choice, ViaLead};
use otaq_core::board::tickets::{EventKind, NewTicketInput, Ticket};
use otaq_core::caller::Caller;
use otaq_core::client::{Client, ClientConfig, ClientError};
use otaq_core::domain::{Actor, AskedLead, Kind, Status, TicketKey};
use otaq_core::epic_lead::questions::{
    self, LeadQuestion, LeadQuestionState, ONLY_A_LEAD_PASSES_ON, ONLY_ITS_LEAD_ANSWERS,
};
use otaq_core::git::testing::TempRepos;
use otaq_core::project_agents::lead::{Lead, LeadStatus};
use otaq_core::protocol::Request;
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::{AgentShell, TestService, path_without_build_folders};
use otaq_core::store::now_ms;
use serde_json::{Value, json};

const HEART: &str = "Should the button show a filled heart once the item is saved?";
const GUESTS: &str = "Can guests save items without logging in?";
const ANSWER: &str = "Yes: filled heart means saved, outline means not saved (plan, step 2).";
const REASON: &str = "PAY-123 only mentions signed-in users.";

/// The sample board (SHOP-40 Wishlist: 40.1 Merged, 40.2 Working, 40.3 Backlog) plus 40.4 Wishlist share link
/// (Working) and the epic SHOP-41 Search with SHOP-41.1; `claude` is an interactive `sh`.
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
    let share = child(&w, "SHOP-40", "Wishlist share link").await;
    w.force_status(&share, Status::Working).await;
    let search = w
        .board
        .create_ticket(
            &NewTicketInput {
                project: "SHOP".into(),
                title: "Search".into(),
                kind: Kind::Epic,
                ..Default::default()
            },
            &me(),
        )
        .await
        .unwrap();
    child(&w, &search.key.to_string(), "Search index API").await;
    w
}

fn me() -> Actor {
    Actor::User("me".into())
}

async fn child(w: &TestService, epic: &str, title: &str) -> Ticket {
    w.board
        .create_ticket(
            &NewTicketInput {
                project: "SHOP".into(),
                title: title.into(),
                epic: Some(epic.into()),
                ..Default::default()
            },
            &me(),
        )
        .await
        .unwrap()
}

async fn ticket(w: &TestService, key: &str) -> Ticket {
    w.board.ticket(key).await.unwrap()
}

/// The child's agent: an `sh` session of its own.
struct Child {
    ticket: Ticket,
    shell: AgentShell,
}

impl Child {
    fn caller(&self) -> Caller {
        Caller::Agent {
            ticket: self.ticket.key.clone(),
            ticket_id: self.ticket.id.clone(),
            session: self.shell.session.clone(),
        }
    }
}

async fn running_child(w: &TestService, key: &str) -> Child {
    let ticket = ticket(w, key).await;
    let shell = w.agent_shell(&ticket).await;
    Child { ticket, shell }
}

/// Its turn ends after `ask human` (Stop), as an agent that waits for the answer does.
async fn child_stops(w: &TestService, child: &Child) {
    let signal = w
        .signals
        .hook(
            &child.caller(),
            Tool::Claude,
            "Stop",
            &json!({ "last_assistant_message": "Waiting for the answer." }),
        )
        .await
        .unwrap();
    assert!(signal.iter().all(otaq_core::signals::Signal::ends_turn));
    w.deliveries
        .deliver_if_ready(&child.shell.session)
        .await
        .unwrap();
}

async fn open_lead(w: &TestService, epic: &str) -> (Lead, AgentShell) {
    let lead = w.project_agents.leads().open(epic).await.unwrap();
    let shell = w.shell_in(&session(&lead));
    (lead, shell)
}

fn session(lead: &Lead) -> String {
    lead.agent.session.as_ref().expect("a session").id.clone()
}

fn lead_caller(lead: &Lead) -> Caller {
    Caller::EpicLead {
        project: lead.agent.project.clone(),
        agent: lead.agent.id.clone(),
        epic: lead.epic.clone(),
        epic_id: lead.epic_id.clone(),
        session: session(lead),
    }
}

/// A hook of the lead as the service handles it; a turn that ended lets waiting deliveries go.
async fn lead_hook(w: &TestService, lead: &Lead, event: &str, payload: Value) {
    let signals = w
        .signals
        .hook(&lead_caller(lead), Tool::Claude, event, &payload)
        .await
        .unwrap();
    for signal in &signals {
        w.project_agents.hook(&lead.agent.id, signal).await.unwrap();
        if signal.ends_turn() {
            w.deliveries.deliver_if_ready(&session(lead)).await.unwrap();
        }
    }
}

async fn lead_prompt(w: &TestService, lead: &Lead, text: &str) {
    lead_hook(w, lead, "UserPromptSubmit", json!({ "prompt": text })).await;
}

async fn lead_stop(w: &TestService, lead: &Lead) {
    lead_hook(
        w,
        lead,
        "Stop",
        json!({ "last_assistant_message": "Done." }),
    )
    .await;
}

/// The lead's waiting wake-ups.
async fn pending(w: &TestService, lead: &Lead) -> Vec<String> {
    w.deliveries
        .pending(&session(lead))
        .await
        .unwrap()
        .into_iter()
        .map(|d| d.text)
        .collect()
}

/// One request from inside the session; `null` params send a request without any (`lead.questions`).
async fn send(w: &TestService, shell: &AgentShell, method: &str, params: Value) -> Value {
    let request = if params.is_null() {
        json!({"protocol_version": 1, "id": 1, "request": {"method": method}})
    } else {
        json!({"protocol_version": 1, "id": 1, "request": {"method": method, "params": params}})
    };
    let ran = shell
        .send_raw(&w.data.socket_path(), &request.to_string())
        .await;
    serde_json::from_str(ran.stdout.lines().next().unwrap_or_default())
        .unwrap_or_else(|e| panic!("{e}: {ran:?}"))
}

async fn send_as_user(w: &TestService, request: Request) -> Value {
    let method = request.method();
    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    match client.request(request).await {
        Ok(reply) => json!({"reply": serde_json::to_value(reply).unwrap()}),
        Err(ClientError::Service(e)) => json!({"error": serde_json::to_value(e).unwrap()}),
        Err(e) => panic!("{method}: {e}"),
    }
}

fn ok(answer: &Value) -> &Value {
    assert!(answer.get("error").is_none(), "{answer}");
    &answer["reply"]
}

fn refused(answer: &Value) -> String {
    assert_eq!(answer["error"]["kind"], "refused", "{answer}");
    answer["error"]["message"].as_str().unwrap().to_owned()
}

/// `otaq ask human --stdin` from the child's session, with options.
async fn ask(w: &TestService, child: &Child, question: &str, options: &[&str]) {
    let asked = send(
        w,
        &child.shell,
        "ticket.ask_human",
        json!({"key": child.ticket.key, "question": question, "options": options}),
    )
    .await;
    ok(&asked);
}

/// The question waiting for the epic's lead.
async fn waiting_one(w: &TestService, epic: &str) -> LeadQuestion {
    let epic = ticket(w, epic).await;
    let mut waiting = questions::waiting(&w.board, &epic.id).await.unwrap();
    assert_eq!(waiting.len(), 1, "{waiting:?}");
    waiting.remove(0)
}

async fn needs_you_has(w: &TestService, key: &str) -> bool {
    w.board
        .needs_you_items(now_ms())
        .await
        .unwrap()
        .iter()
        .any(|i| i.key.to_string() == key)
}

/// The steps of the `lead_question` History lines on `key`, oldest first.
async fn steps(w: &TestService, key: &str) -> Vec<String> {
    let mut steps: Vec<String> = w
        .board
        .history(key)
        .await
        .unwrap()
        .into_iter()
        .filter(|e| e.kind == EventKind::LeadQuestion)
        .map(|e| e.payload["step"].as_str().unwrap_or_default().to_owned())
        .collect();
    steps.reverse();
    steps
}

async fn wait_until<F, Fut>(what: &str, mut check: F)
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    tokio::time::timeout(Duration::from_secs(10), async {
        while !check().await {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("timed out waiting: {what}"));
}

/// The lead's turn that its wake-up starts: typed at the end of the turn it waited behind, then submitted.
async fn lead_takes_the_wake_up(w: &TestService, lead: &Lead, wake: &str) {
    assert_eq!(pending(w, lead).await, [wake.to_owned()]);
    lead_stop(w, lead).await;
    assert!(pending(w, lead).await.is_empty(), "typed");
    lead_prompt(w, lead, wake).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_routed_question_makes_no_needs_you_item_and_the_leads_answer_reaches_the_child() {
    let w = world().await;
    let (lead, lead_shell) = open_lead(&w, "SHOP-40").await;
    lead_prompt(&w, &lead, "Plan it").await;
    let button = running_child(&w, "SHOP-40.2").await;

    ask(&w, &button, HEART, &[]).await;
    // Not turquoise, no needs-you item: "asked lead · waiting".
    let asked = ticket(&w, "SHOP-40.2").await;
    assert_eq!(asked.status, Status::Working);
    assert_eq!(
        asked.flags.asked_lead,
        Some(AskedLead {
            lead: "Wishlist lead".into(),
            question: HEART.into()
        })
    );
    assert_eq!(asked.flags.via_lead, None);
    assert!(!needs_you_has(&w, "SHOP-40.2").await);
    // Its turn ends: waiting, not Idle.
    child_stops(&w, &button).await;
    assert_eq!(w.status("SHOP-40.2").await, Status::Working);

    // The lead is woken with the question; its turn answers SHOP-40.2.
    let q = waiting_one(&w, "SHOP-40").await;
    assert_eq!(
        (q.child.to_string(), q.question.as_str(), q.state),
        ("SHOP-40.2".into(), HEART, LeadQuestionState::WithLead)
    );
    let wake = questions::wake_text(
        &"SHOP-40.2".parse().unwrap(),
        &w.board.open_question(&asked.id).await.unwrap().unwrap(),
    );
    assert!(wake.starts_with(&format!("SHOP-40.2 asks (question {}): {HEART}", q.id)));
    lead_takes_the_wake_up(&w, &lead, &wake).await;
    let leads = w.project_agents.leads();
    assert_eq!(
        leads.status("SHOP-40").await.unwrap(),
        LeadStatus::Answering {
            ticket: "SHOP-40.2".parse().unwrap()
        }
    );
    assert_eq!(leads.handovers_of("SHOP-40").await.unwrap().len(), 1);
    let listed = send(&w, &lead_shell, "lead.questions", json!(null)).await;
    assert_eq!(ok(&listed)["lead_questions"][0]["id"], json!(q.id));

    let answered = send(
        &w,
        &lead_shell,
        "message.answer",
        json!({"id": q.id, "answer": ANSWER}),
    )
    .await;
    assert_eq!(ok(&answered)["lead_question"]["state"], "answered");
    // Typed into the child; the card is back to working; still no needs-you item.
    w.wait_for_output(
        &button.shell.session,
        &format!("Answer from Wishlist lead: {ANSWER}"),
    )
    .await;
    let after = ticket(&w, "SHOP-40.2").await;
    assert_eq!(after.status, Status::Working);
    assert_eq!(after.flags.asked_lead, None);
    assert!(!needs_you_has(&w, "SHOP-40.2").await);
    assert!(leads.handovers_of("SHOP-40").await.unwrap().is_empty());
    // A second answer is refused.
    let again = send(
        &w,
        &lead_shell,
        "message.answer",
        json!({"id": q.id, "answer": "Again"}),
    )
    .await;
    assert!(refused(&again).contains("was answered already"));
    // The turn ends: nothing is handed back.
    lead_stop(&w, &lead).await;
    assert_eq!(w.status("SHOP-40.2").await, Status::Working);

    // Messages on the child and the epic; not an answer of the user's.
    for key in ["SHOP-40.2", "SHOP-40"] {
        let log = questions::log_of(&w.board, &ticket(&w, key).await.id)
            .await
            .unwrap();
        assert_eq!(log.len(), 1, "{key}");
        assert_eq!(log[0].state, LeadQuestionState::Answered);
        assert_eq!(log[0].answer.as_deref(), Some(ANSWER));
        assert_eq!(log[0].lead, "Wishlist lead");
        assert_eq!(steps(&w, key).await, ["asked", "answered"], "{key}");
    }
    // The Messages log (T-0290) reads the same question on both.
    for key in ["SHOP-40.2", "SHOP-40"] {
        let log = otaq_core::epic_lead::log::of(&w.board, key).await.unwrap();
        assert_eq!(log.questions.len(), 1, "{key}");
        assert!(log.notes.is_empty(), "{key}");
    }
    assert!(
        w.board
            .answered_human_questions(&asked.id)
            .await
            .unwrap()
            .is_empty()
    );
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_passed_on_question_asks_you_with_the_leads_option_and_reason() {
    let w = world().await;
    let (lead, lead_shell) = open_lead(&w, "SHOP-40").await;
    lead_prompt(&w, &lead, "Plan it").await;
    let button = running_child(&w, "SHOP-40.2").await;
    ask(
        &w,
        &button,
        GUESTS,
        &["Guests too, kept in the browser", "Logged-in users only"],
    )
    .await;
    child_stops(&w, &button).await;
    let q = waiting_one(&w, "SHOP-40").await;
    assert!(pending(&w, &lead).await[0].contains(
        "Options: 1. Guests too, kept in the browser 2. Logged-in users only. Answer with"
    ));
    let wake = pending(&w, &lead).await.remove(0);
    lead_takes_the_wake_up(&w, &lead, &wake).await;

    // A wrong recommendation or no reason is refused and changes nothing.
    let wrong = send(
        &w,
        &lead_shell,
        "lead.pass_on",
        json!({"id": q.id, "recommended": 3, "reason": REASON}),
    )
    .await;
    assert_eq!(refused(&wrong), "--recommended must be between 1 and 2");
    let no_reason = send(
        &w,
        &lead_shell,
        "lead.pass_on",
        json!({"id": q.id, "recommended": 1, "reason": " "}),
    )
    .await;
    assert_eq!(refused(&no_reason), "Write your reason on stdin");
    assert_eq!(
        waiting_one(&w, "SHOP-40").await.state,
        LeadQuestionState::WithLead
    );

    // Passed on with its own options, the first recommended.
    let passed = send(
        &w,
        &lead_shell,
        "lead.pass_on",
        json!({"id": q.id, "options": ["Logged-in users only", "Guests too, kept in the browser"],
               "recommended": 1, "reason": REASON}),
    )
    .await;
    assert_eq!(ok(&passed)["lead_question"]["state"], "passed_on");
    let asks = ticket(&w, "SHOP-40.2").await;
    assert_eq!(asks.status, Status::AsksYou);
    assert_eq!(asks.flags.via_lead.as_deref(), Some("Wishlist lead"));
    assert_eq!(asks.flags.asked_lead, None);
    let question = asks.question.clone().expect("the open question");
    assert_eq!(
        question.options,
        ["Logged-in users only", "Guests too, kept in the browser"]
    );
    assert_eq!(question.recommended, Some(1));
    assert_eq!(
        question.via_lead,
        Some(ViaLead {
            lead: "Wishlist lead".into(),
            reason: REASON.into()
        })
    );
    let items = w.board.needs_you_items(now_ms()).await.unwrap();
    let item = items
        .iter()
        .find(|i| i.key.to_string() == "SHOP-40.2")
        .expect("an Asks you item");
    assert_eq!(
        item.reason,
        format!("asks \u{b7} passed on by Wishlist lead: {GUESTS}")
    );
    assert!(
        should_notify(item),
        "an OS notification as for any Asks you"
    );
    assert!(
        w.project_agents
            .leads()
            .handovers_of("SHOP-40")
            .await
            .unwrap()
            .is_empty()
    );
    lead_stop(&w, &lead).await;
    assert_eq!(w.status("SHOP-40.2").await, Status::AsksYou);

    // The user's answer reaches the child as today, naming the choice; logged on both.
    w.needs_you
        .answer("SHOP-40.2", &Choice::Option(1), false, &me())
        .await
        .unwrap();
    w.wait_for_output(
        &button.shell.session,
        "Answer to your question: Logged-in users only (option 1).",
    )
    .await;
    assert_eq!(w.status("SHOP-40.2").await, Status::Working);
    for key in ["SHOP-40.2", "SHOP-40"] {
        let log = questions::log_of(&w.board, &ticket(&w, key).await.id)
            .await
            .unwrap();
        assert_eq!(log[0].state, LeadQuestionState::PassedOn, "{key}");
        assert_eq!(log[0].reason.as_deref(), Some(REASON));
        assert_eq!(log[0].recommended_label(), Some("Logged-in users only"));
        assert_eq!(
            log[0].answer.as_deref(),
            Some("Answer to your question: Logged-in users only (option 1).")
        );
        assert_eq!(steps(&w, key).await, ["asked", "passed_on"], "{key}");
    }
    let history = w.board.history("SHOP-40").await.unwrap();
    let passed_on = history
        .iter()
        .find(|e| e.kind == EventKind::LeadQuestion && e.payload["step"] == "passed_on")
        .unwrap();
    assert_eq!(passed_on.payload["recommended"], "Logged-in users only");
    assert_eq!(passed_on.payload["reason"], REASON);
    assert_eq!(passed_on.actor, Actor::ProjectAgent(lead.agent.id.clone()));
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_question_the_lead_leaves_is_handed_back_at_turn_end_and_when_the_lead_ends() {
    let w = world().await;
    let (lead, _) = open_lead(&w, "SHOP-40").await;
    lead_prompt(&w, &lead, "Plan it").await;
    let button = running_child(&w, "SHOP-40.2").await;

    // The turn its wake-up started ends with neither an answer nor a pass-on: a plain Asks you.
    ask(&w, &button, HEART, &[]).await;
    child_stops(&w, &button).await;
    let wake = pending(&w, &lead).await.remove(0);
    lead_takes_the_wake_up(&w, &lead, &wake).await;
    assert_eq!(w.status("SHOP-40.2").await, Status::Working);
    lead_stop(&w, &lead).await;
    let asks = ticket(&w, "SHOP-40.2").await;
    assert_eq!(asks.status, Status::AsksYou);
    assert_eq!(asks.flags.via_lead, None);
    assert_eq!(asks.flags.asked_lead, None);
    let items = w.board.needs_you_items(now_ms()).await.unwrap();
    let item = items
        .iter()
        .find(|i| i.key.to_string() == "SHOP-40.2")
        .expect("an Asks you item");
    assert_eq!(item.reason, format!("asks: {HEART}"));
    assert_eq!(steps(&w, "SHOP-40").await, ["asked", "handed_back"]);
    let epic = ticket(&w, "SHOP-40").await;
    let log = questions::log_of(&w.board, &epic.id).await.unwrap();
    assert_eq!(log[0].state, LeadQuestionState::HandedBack);
    // The lead can't answer it any more.
    let late = w
        .project_agents
        .leads()
        .get("SHOP-40")
        .await
        .unwrap()
        .unwrap();
    let refused_late = questions::LeadQuestions::new(
        w.board.clone(),
        w.signals.clone(),
        w.project_agents.leads(),
        w.deliveries.clone(),
        w.sessions.clone(),
        w.events.clone(),
    )
    .answer(&lead_caller(&late), &log[0].id, "Too late", &me())
    .await
    .unwrap_err();
    assert!(
        refused_late
            .to_string()
            .contains("already went to the user")
    );
    w.needs_you
        .answer("SHOP-40.2", &Choice::Other("Filled".into()), false, &me())
        .await
        .unwrap();
    assert_eq!(w.status("SHOP-40.2").await, Status::Working);

    // The lead ends while a question waits for it: handed back too.
    ask(&w, &button, GUESTS, &[]).await;
    assert_eq!(w.status("SHOP-40.2").await, Status::Working);
    w.sessions.stop(&session(&lead)).await.unwrap();
    wait_until("the hand-back", || async {
        w.status("SHOP-40.2").await == Status::AsksYou
    })
    .await;
    assert!(needs_you_has(&w, "SHOP-40.2").await);
    assert_eq!(
        steps(&w, "SHOP-40.2").await,
        ["asked", "handed_back", "asked", "handed_back"]
    );
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn with_no_live_lead_the_question_asks_you_and_a_permission_prompt_never_reaches_the_lead() {
    let w = world().await;
    let button = running_child(&w, "SHOP-40.2").await;
    ask(&w, &button, HEART, &[]).await;
    let asks = ticket(&w, "SHOP-40.2").await;
    assert_eq!(asks.status, Status::AsksYou);
    assert!(needs_you_has(&w, "SHOP-40.2").await);
    let id = asks.question.unwrap().id;
    assert!(questions::find(&w.board, &id).await.unwrap().is_none());
    assert!(steps(&w, "SHOP-40").await.is_empty());

    // A live lead: a child's permission prompt is the user's (d0011), and the lead hears nothing.
    let (lead, _) = open_lead(&w, "SHOP-40").await;
    let share = running_child(&w, "SHOP-40.4").await;
    w.signals
        .hook(
            &share.caller(),
            Tool::Claude,
            "Notification",
            &json!({ "notification_type": "permission_prompt", "message": "Allow npm test?" }),
        )
        .await
        .unwrap();
    assert_eq!(w.status("SHOP-40.4").await, Status::NeedsYou);
    assert!(needs_you_has(&w, "SHOP-40.4").await);
    assert!(pending(&w, &lead).await.is_empty());
    assert!(
        w.project_agents
            .leads()
            .handovers_of("SHOP-40")
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        w.project_agents.leads().status("SHOP-40").await.unwrap(),
        LeadStatus::Idle
    );
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn only_the_lead_of_that_epic_answers_or_passes_on() {
    let w = world().await;
    let (lead, lead_shell) = open_lead(&w, "SHOP-40").await;
    let button = running_child(&w, "SHOP-40.2").await;
    ask(&w, &button, HEART, &["Filled heart", "Outline only"]).await;
    let q = waiting_one(&w, "SHOP-40").await;

    // The asking child's own agent, and another child's.
    let share = running_child(&w, "SHOP-40.4").await;
    for shell in [&button.shell, &share.shell] {
        let answer = send(
            &w,
            shell,
            "message.answer",
            json!({"id": q.id, "answer": "Filled"}),
        )
        .await;
        assert_eq!(refused(&answer), ONLY_ITS_LEAD_ANSWERS);
        let pass = send(
            &w,
            shell,
            "lead.pass_on",
            json!({"id": q.id, "recommended": 1, "reason": "r"}),
        )
        .await;
        assert_eq!(refused(&pass), ONLY_A_LEAD_PASSES_ON);
        let listed = send(&w, shell, "lead.questions", json!(null)).await;
        assert_eq!(answer_kind(&listed), "refused");
    }
    // Another epic's lead.
    let (_, search_shell) = open_lead(&w, "SHOP-41").await;
    let answer = send(
        &w,
        &search_shell,
        "message.answer",
        json!({"id": q.id, "answer": "Filled"}),
    )
    .await;
    assert_eq!(
        refused(&answer),
        "Search lead can only change SHOP-41's children that haven't started"
    );
    let listed = send(&w, &search_shell, "lead.questions", json!(null)).await;
    assert_eq!(ok(&listed)["lead_questions"], json!([]));
    // The user answers from Needs you once it is passed on, not for the lead.
    let user = send_as_user(
        &w,
        Request::MessageAnswer(otaq_core::protocol::AnswerParams {
            id: q.id.clone(),
            answer: "Filled".into(),
        }),
    )
    .await;
    assert_eq!(refused(&user), ONLY_ITS_LEAD_ANSWERS);
    let user_pass = send_as_user(
        &w,
        Request::LeadPassOn(otaq_core::protocol::PassOnParams {
            id: q.id.clone(),
            options: Vec::new(),
            recommended: None,
            notes: Vec::new(),
            reason: "r".into(),
        }),
    )
    .await;
    assert_eq!(refused(&user_pass), ONLY_A_LEAD_PASSES_ON);

    // Still waiting for its lead, which may answer.
    assert_eq!(
        waiting_one(&w, "SHOP-40").await.state,
        LeadQuestionState::WithLead
    );
    let answered = send(
        &w,
        &lead_shell,
        "message.answer",
        json!({"id": q.id, "answer": "Filled heart (plan, step 2)."}),
    )
    .await;
    assert_eq!(ok(&answered)["lead_question"]["state"], "answered");
    let _ = lead;
    w.stop().await;
}

fn answer_kind(answer: &Value) -> &str {
    answer["error"]["kind"].as_str().unwrap_or("reply")
}

#[tokio::test(flavor = "multi_thread")]
async fn after_a_restart_a_question_that_waited_for_the_lead_asks_you() {
    let w = world().await;
    let (_lead, _) = open_lead(&w, "SHOP-40").await;
    let button = running_child(&w, "SHOP-40.2").await;
    ask(&w, &button, HEART, &[]).await;
    assert_eq!(w.status("SHOP-40.2").await, Status::Working);
    let id = waiting_one(&w, "SHOP-40").await.id;

    let w = w.restart_with(|c| c).await;
    let after = ticket(&w, "SHOP-40.2").await;
    assert_eq!(after.status, Status::Interrupted);
    assert_eq!(after.interrupted_from, Some(Status::AsksYou));
    assert_eq!(
        after.question.as_ref().map(|q| q.text.as_str()),
        Some(HEART)
    );
    assert_eq!(after.flags.asked_lead, None);
    assert!(needs_you_has(&w, "SHOP-40.2").await);
    let q = questions::find(&w.board, &id).await.unwrap().unwrap();
    assert_eq!(q.state, LeadQuestionState::HandedBack);
    let epic: TicketKey = "SHOP-40".parse().unwrap();
    assert!(
        questions::waiting(&w.board, &ticket(&w, &epic.to_string()).await.id)
            .await
            .unwrap()
            .is_empty()
    );
    w.stop().await;
}
