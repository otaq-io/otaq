//! Pause and Resume (T-0455; PAU-01, PAU-02, PAU-03, PAU-06, PAU-07; d0112): Pause sets the project's flag first,
//! then stops every agent session of the project at once (ticket agents, checks, Project agents) and
//! leaves Run, Shell, the Project terminal and the other projects alone; its tickets go the restart's way with the
//! cause "paused"; the flag survives a restart and nothing resumes; Resume brings each agent back on its own
//! conversation. `claude` stands in as an `sh` printing its arguments.

use std::time::Duration;

use otaq_core::agent_tools::LaunchContext;
use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::agents::pause::{PauseCounts, PauseScope};
use otaq_core::board::moves::DropTarget;
use otaq_core::board::tickets::{EventKind, Ticket};
use otaq_core::caller::Caller;
use otaq_core::client::{Client, ClientConfig};
use otaq_core::domain::{Actor, Status, ValidationTrigger};
use otaq_core::project_agents::AgentState;
use otaq_core::protocol::{Event, Reply, Request};
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::{TestService, said};
use otaq_core::session::{END_PAUSED, SessionInfo, SessionKind, SessionTicket};
use serde_json::json;

/// Keeps running, printing its arguments.
const KEEPS_RUNNING: &str = "printf 'ARGS %s\\n' \"$*\"; exec sleep 30";

fn me() -> Actor {
    Actor::User("me".into())
}

fn launch(config: ServiceConfig) -> ServiceConfig {
    ServiceConfig {
        launch: Some(LaunchContext {
            bin_dir: "/nonexistent".into(),
            plugin_dir: "/nonexistent/plugin".into(),
            path: std::env::var_os("PATH").unwrap_or_default(),
            claude: ClaudeLaunch {
                program: vec!["sh".into(), "-c".into(), KEEPS_RUNNING.into(), "sh".into()],
                config_dir: None,
            },
        }),
        ..config
    }
}

async fn until(what: &str, mut done: impl AsyncFnMut() -> bool) {
    let reached = tokio::time::timeout(Duration::from_secs(20), async {
        while !done().await {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await;
    assert!(reached.is_ok(), "never: {what}");
}

async fn until_status(w: &TestService, key: &str, want: Status) {
    until(&format!("{key} is {want}"), async || {
        w.status(key).await == want
    })
    .await;
}

async fn start_agent(w: &TestService, key: &str) {
    w.board
        .move_ticket(key, DropTarget::ReadyForDev, true, &me())
        .await
        .unwrap();
    w.starter
        .start(key, &me(), otaq_core::session::Start::Auto)
        .await
        .unwrap();
    until_status(w, key, Status::Working).await;
}

async fn ticket(w: &TestService, key: &str) -> Ticket {
    w.board.ticket(key).await.unwrap()
}

/// The ticket's agent sessions, newest first.
async fn sessions(w: &TestService, key: &str) -> Vec<SessionInfo> {
    let t = ticket(w, key).await;
    let mut all = w
        .sessions
        .history(&SessionTicket {
            id: t.id,
            key: t.key,
        })
        .await
        .unwrap();
    all.retain(|s| s.kind == SessionKind::Agent);
    all
}

async fn history(w: &TestService, key: &str, kind: EventKind) -> Vec<serde_json::Value> {
    w.board
        .history(key)
        .await
        .unwrap()
        .into_iter()
        .filter(|e| e.kind == kind)
        .map(|e| e.payload)
        .collect()
}

fn shop() -> PauseScope {
    PauseScope::Project {
        project: "SHOP".into(),
    }
}

fn counts(c: &PauseCounts) -> Vec<(&str, bool, u32, u32)> {
    c.projects
        .iter()
        .map(|p| {
            (
                p.name.as_str(),
                p.paused_since.is_some(),
                p.working,
                p.paused,
            )
        })
        .collect()
}

/// The validations' outcomes of a ticket, oldest first.
async fn rounds(w: &TestService, t: &Ticket) -> Vec<Option<String>> {
    sqlx::query_scalar(
        "SELECT outcome FROM validations WHERE ticket_id = ? ORDER BY started_at, id",
    )
    .bind(&t.id)
    .fetch_all(w.board.store().pool())
    .await
    .unwrap()
}

/// shop-web: SHOP-1 working, SHOP-2 asks a question, SHOP-3 checking (its agent and its checker), SHOP-4 waiting for
/// review and a Project agent: 6 agent sessions, 5 agents. billing-api: BILL-1
/// working. SHOP-1 has a Run, a Shell and the Project terminal.
struct World {
    w: TestService,
    others: Vec<SessionInfo>,
    agent: String,
}

async fn world() -> World {
    let w = TestService::start_with(launch).await;
    w.add_project("shop-web").await;
    w.add_project("billing-api").await;
    for title in [
        "Search results paging",
        "Order confirmation email",
        "Checkout API",
        "Footer links",
    ] {
        w.create("SHOP", title).await;
    }
    w.create("BILL", "Invoice template").await;
    for key in ["SHOP-1", "SHOP-2", "SHOP-3", "SHOP-4", "BILL-1"] {
        start_agent(&w, key).await;
    }
    // SHOP-2 asks.
    let asks = ticket(&w, "SHOP-2").await;
    let session = w.sessions.running_of(&asks.id, SessionKind::Agent).unwrap();
    w.board
        .store_question(
            &asks.id,
            "Should dark mode follow the system setting?",
            &["Yes".into(), "No".into()],
            &[],
            Some(1),
        )
        .await
        .unwrap();
    w.signals
        .ask_human(
            &Caller::Agent {
                ticket: asks.key.clone(),
                ticket_id: asks.id.clone(),
                session: session.id,
            },
            &asks.id,
            "Should dark mode follow the system setting?",
        )
        .await
        .unwrap();
    // SHOP-3 submits: its checker starts.
    w.validation
        .submit(
            "SHOP-3",
            said(&w.board, "SHOP-3", "Checkout API done.")
                .await
                .as_ref(),
            &Actor::Agent("SHOP-3".parse().unwrap()),
            ValidationTrigger::Agent,
        )
        .await
        .unwrap();
    let checking = ticket(&w, "SHOP-3").await;
    until("SHOP-3's checker runs", async || {
        w.sessions
            .running_of(&checking.id, SessionKind::Validator)
            .is_some()
    })
    .await;
    // SHOP-4 waits for review; its session still runs.
    let review = ticket(&w, "SHOP-4").await;
    w.force_status(&review, Status::WaitingForYou).await;
    // A Project agent.
    let agent = w
        .project_agents
        .open("SHOP", "Import GitHub issues")
        .await
        .unwrap();
    // The user's own terminals.
    let shop1 = ticket(&w, "SHOP-1").await;
    let mut others = Vec::new();
    for kind in [SessionKind::Run, SessionKind::Shell, SessionKind::Project] {
        let shell = w.shell(&shop1, kind).await;
        others.push(w.sessions.stored(&shell.session).await.unwrap());
    }
    World {
        w,
        others,
        agent: agent.id,
    }
}

fn running_of(w: &TestService, project: &str) -> Vec<SessionInfo> {
    w.sessions
        .in_memory()
        .into_iter()
        .filter(|s| s.is_running() && s.project == project && s.kind.runs_agent_tool())
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn pause_stops_the_projects_agents_survives_a_restart_and_resume_brings_each_back() {
    let World { w, others, agent } = world().await;
    let shop_sessions = running_of(&w, "SHOP");
    assert_eq!(shop_sessions.len(), 6, "{shop_sessions:#?}");
    let mut conversations = Vec::new();
    for key in ["SHOP-1", "SHOP-2"] {
        conversations.push(sessions(&w, key).await[0].tool_session_id.clone().unwrap());
    }

    // The counts of the menus: "shop-web · 5 working", "6 working in 2 projects".
    let before = w.pause.counts(&PauseScope::All).await.unwrap();
    assert_eq!(
        counts(&before),
        [("billing-api", false, 1, 0), ("shop-web", false, 5, 0)]
    );
    assert_eq!((before.working, before.paused), (6, 0));

    // Every end the pause causes finds the project paused already.
    let mut listener = w.events.listen();
    let store = w.board.store().clone();
    let watcher = tokio::spawn(async move {
        let mut seen = Vec::new();
        while let Ok(event) = listener.receiver.recv().await {
            match event {
                Event::SessionState { session }
                    if session.end_reason.as_deref() == Some(END_PAUSED) =>
                {
                    let paused = otaq_core::agents::pause::is_paused(&store, "SHOP")
                        .await
                        .unwrap();
                    seen.push(paused);
                    if seen.len() == 6 {
                        break;
                    }
                }
                _ => {}
            }
        }
        seen
    });

    let after = w.pause.pause(&shop()).await.unwrap();
    assert_eq!(
        counts(&after),
        [("shop-web", true, 0, 4)],
        "SHOP-1, SHOP-2, SHOP-3 and the Project agent wait"
    );
    let seen = tokio::time::timeout(Duration::from_secs(20), watcher)
        .await
        .expect("six ends")
        .unwrap();
    assert_eq!(seen, [true; 6]);

    // Every agent session of shop-web ended as paused; billing-api and the user's terminals keep going.
    for s in &shop_sessions {
        let s = w.sessions.stored(&s.id).await.unwrap();
        assert_eq!(s.end_reason.as_deref(), Some(END_PAUSED), "{s:?}");
    }
    assert_eq!(running_of(&w, "BILL").len(), 1);
    for s in &others {
        assert!(
            w.sessions.stored(&s.id).await.unwrap().is_running(),
            "{:?} keeps going",
            s.kind
        );
    }

    // What it leaves (PAU-03), the restart's way with the cause "paused".
    let t = ticket(&w, "SHOP-1").await;
    assert_eq!((t.status, t.interrupted_from), (Status::Interrupted, None));
    let paused = history(&w, "SHOP-1", EventKind::Interrupted).await;
    assert_eq!(
        (&paused[0]["reason"], &paused[0]["from"]),
        (&json!("paused"), &json!("working"))
    );
    let t = ticket(&w, "SHOP-2").await;
    assert_eq!(
        (t.status, t.interrupted_from),
        (Status::Interrupted, Some(Status::AsksYou))
    );
    assert_eq!(
        t.question.unwrap().text,
        "Should dark mode follow the system setting?",
        "the question stays"
    );
    let shop3 = ticket(&w, "SHOP-3").await;
    assert_eq!(
        (shop3.status, shop3.interrupted_from),
        (Status::Interrupted, Some(Status::Checking))
    );
    assert_eq!(
        rounds(&w, &shop3).await,
        [Some("interrupted".to_owned())],
        "the check ended interrupted, not a round"
    );
    assert_eq!(w.status("SHOP-4").await, Status::WaitingForYou);
    let noted = history(&w, "SHOP-4", EventKind::SessionEnded).await;
    assert_eq!(noted.len(), 1);
    assert_eq!(noted[0]["reason"], "paused");
    assert_eq!(
        w.project_agents.get(&agent).await.unwrap().state,
        AgentState::Paused
    );

    // The pause survives a restart; nothing resumes; the startup report and the status name shop-web.
    let w = w.restart_with(launch).await;
    assert_eq!(w.recovery.startup_report().paused, ["SHOP"]);
    assert!(w.pause.is_paused("SHOP").await.unwrap());
    assert!(!w.pause.is_paused("BILL").await.unwrap());
    assert!(running_of(&w, "SHOP").is_empty(), "nothing resumes");
    assert_eq!(
        w.project_agents.get(&agent).await.unwrap().state,
        AgentState::Paused,
        "a restart leaves a paused agent as it is"
    );
    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    let Reply::ServiceStatus(status) = client.request(Request::ServiceStatus).await.unwrap() else {
        panic!("status")
    };
    let paused: Vec<(&str, u32)> = status
        .paused
        .iter()
        .map(|p| (p.project.as_str(), p.paused))
        .collect();
    assert_eq!(paused, [("SHOP", 4)]);
    // The reboot summary counts outside the paused projects (PAU-07, T-0457).
    let outside = w
        .recovery
        .startup_report()
        .interrupted
        .iter()
        .filter(|k| k.project.to_string() != "SHOP")
        .count();
    assert_eq!(status.interrupted_at_start as usize, outside);
    assert!(outside > 0, "billing-api's ticket");
    // billing-api's agent was ended by the restart, not the pause.
    assert_eq!(w.status("BILL-1").await, Status::Interrupted);

    // Resume: the flag goes first, then each agent comes back on its own conversation.
    let Reply::PauseCounts(after) = client.request(Request::AgentsResume(shop())).await.unwrap()
    else {
        panic!("counts")
    };
    assert!(!w.pause.is_paused("SHOP").await.unwrap());
    assert_eq!(after.projects[0].paused_since, None);
    for (key, conversation) in ["SHOP-1", "SHOP-2"].iter().zip(&conversations) {
        until_status(&w, key, Status::Working).await;
        let resumed = &sessions(&w, key).await[0];
        assert!(resumed.is_running(), "{key}");
        let out = w.wait_for_output(&resumed.id, "ARGS").await;
        assert!(
            out.contains(&format!("--resume {conversation}")),
            "{key}: {out}"
        );
    }
    // The check starts afresh.
    until("SHOP-3's fresh checker runs", async || {
        w.sessions
            .running_of(&shop3.id, SessionKind::Validator)
            .is_some()
    })
    .await;
    assert_eq!(w.status("SHOP-3").await, Status::Checking);
    let agent = w.project_agents.get(&agent).await.unwrap();
    assert_eq!(agent.state, AgentState::Running);
    assert!(agent.session.is_some_and(|s| s.is_running()));
    // Review waits for its click; billing-api's Interrupted ticket isn't shop-web's to resume.
    assert_eq!(w.status("SHOP-4").await, Status::WaitingForYou);
    assert_eq!(w.status("BILL-1").await, Status::Interrupted);

    for s in w
        .sessions
        .in_memory()
        .into_iter()
        .filter(|s| s.is_running())
    {
        let _ = w.sessions.stop(&s.id).await;
    }
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn all_projects_pauses_and_resumes_each_and_a_project_added_later_runs() {
    let w = TestService::start_with(launch).await;
    w.add_project("shop-web").await;
    w.add_project("billing-api").await;
    let mut listener = w.events.listen();

    let paused = w.pause.pause(&PauseScope::All).await.unwrap();
    assert_eq!(
        counts(&paused),
        [("billing-api", true, 0, 0), ("shop-web", true, 0, 0)]
    );
    let mut changed = Vec::new();
    while changed.len() < 2 {
        if let Event::PauseChanged {
            project,
            paused_since,
        } = listener.receiver.recv().await.unwrap()
        {
            changed.push((project, paused_since.is_some()));
        }
    }
    changed.sort();
    assert_eq!(
        changed,
        [("BILL".to_owned(), true), ("SHOP".to_owned(), true)]
    );
    // Pausing again keeps the time it was paused.
    let since = w.board.project("SHOP").await.unwrap().paused_since;
    w.pause.pause(&shop()).await.unwrap();
    assert_eq!(w.board.project("SHOP").await.unwrap().paused_since, since);
    let infos = w.board.project_infos().await.unwrap();
    assert!(infos.iter().all(|p| p.paused_since.is_some()));

    w.add_project("pocket-api").await;
    assert!(!w.pause.is_paused("POCK").await.unwrap());

    let resumed = w.pause.resume(&PauseScope::All, &me()).await.unwrap();
    assert!(resumed.projects.iter().all(|p| p.paused_since.is_none()));
    for key in ["SHOP", "BILL", "POCK"] {
        assert!(!w.pause.is_paused(key).await.unwrap(), "{key}");
    }
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_agent_can_neither_pause_nor_resume() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let shop1 = w.create("SHOP", "Split user full name").await;
    let agent = w.agent_shell(&shop1).await;
    for method in ["agents.pause", "agents.resume"] {
        let request = json!({"protocol_version": 1, "id": 1,
            "request": {"method": method, "params": {"scope": "project", "project": "SHOP"}}});
        let ran = agent
            .send_raw(&w.data.socket_path(), &request.to_string())
            .await;
        let answer: serde_json::Value =
            serde_json::from_str(ran.stdout.lines().next().unwrap_or_default()).unwrap();
        assert_eq!(answer["error"]["kind"], "refused", "{method}: {answer}");
    }
    assert!(!w.pause.is_paused("SHOP").await.unwrap());
    // The counts are a read.
    let request = json!({"protocol_version": 1, "id": 1,
        "request": {"method": "agents.pause_counts", "params": {"scope": "all"}}});
    let ran = agent
        .send_raw(&w.data.socket_path(), &request.to_string())
        .await;
    let answer: serde_json::Value =
        serde_json::from_str(ran.stdout.lines().next().unwrap_or_default()).unwrap();
    assert_eq!(
        answer["reply"]["pause_counts"]["projects"][0]["name"], "shop-web",
        "{answer}"
    );
    w.stop().await;
}
