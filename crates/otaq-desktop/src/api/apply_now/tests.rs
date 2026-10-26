//! The example's Apply now end to end through the window's requests (T-0441; APN-05; d0108, d0056, d0079): pocket-notes
//! (NOTE) on Product spec · Dev · Security · Review · Done · PR, with NOTE-1 running in Product spec, NOTE-2's checker
//! running in Security, NOTE-3 running in Dev and NOTE-4 waiting in Review. A proposal removing Product spec and
//! Security waits; `apply_now.view` shows the plan, NOTE-2 is picked back to Dev, `apply_now.apply` applies it: Product
//! spec's agent and Security's checker stop, NOTE-1 starts Dev, NOTE-2's Dev conversation resumes, NOTE-3 and NOTE-4
//! are untouched.
//!
//! The app talks to a core `TestService` (temporary `OTAQ_HOME` and repositories); `claude` is stood in for by `sh`
//! printing its arguments and sleeping. No agent tool runs.

use std::time::Duration;

use otaq_core::agent_tools::LaunchContext;
use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::agents::FixedModels;
use otaq_core::board::moves::DropTarget;
use otaq_core::board::projects::AddProject;
use otaq_core::board::tickets::Ticket;
use otaq_core::domain::{Actor, Place, Status};
use otaq_core::flow::apply::ApplyParams;
use otaq_core::flow::proposal::{ApplyChoices, NewProposal, ProposedBy, Source};
use otaq_core::flow::{FlowFile, yaml};
use otaq_core::git::testing::commit;
use otaq_core::protocol::{Reply, Request};
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::TestService;
use otaq_core::session::SessionKind;
use otaq_core::start::StartOutcome;
use serde_json::{Value, json};

use crate::App;
use crate::api::request;
use crate::service::testing::{app_on, connected};

/// Keeps running, printing its arguments.
const KEEPS_RUNNING: &str = "printf 'ARGS %s\\n' \"$*\"; exec sleep 30";

/// pocket-notes' flow: Backlog → Ready for Dev → Product spec → Dev → Security (agent check) → Review → Done · PR.
const POCKET_NOTES: &str = "\
agents:
  - name: Spec writer
    skills: [product-spec]
    model: default
  - name: Rust developer
    skills: [work-ticket, commit-compact]
    model: default
  - name: Security reviewer
    skills: [security-review]
    model: default
flow:
  - id: product-spec
    name: Product spec
    kind: agent
    column: own
    agent: Spec writer
    skill: product-spec
  - id: dev
    name: Dev
    kind: agent
    column: own
    agent: Rust developer
    skill: work-ticket
  - id: security
    name: Security
    kind: check
    by: agent
    column: own
    agents: [Security reviewer]
    skill: security-review
    rounds: 2
  - id: review
    name: Review
    kind: check
    by: you
    column: own
  - id: deliver
    name: Done
    kind: delivery
    column: own
    agent: Rust developer
    skill: open-pr
    mode: click
    delivers: pr
";

fn me() -> Actor {
    Actor::User("me".into())
}

/// pocket-notes (NOTE) on its flow, with the stand-in `claude`.
async fn world() -> TestService {
    let w = TestService::start_with(|config| ServiceConfig {
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
    })
    .await;
    let repo = w.repos.repo("pocket-notes", 1);
    w.board
        .add_project(&AddProject {
            path: repo.display().to_string(),
            key: Some("NOTE".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    w.board
        .use_flow_for_tests("NOTE", &yaml::read(POCKET_NOTES).unwrap())
        .await;
    w
}

/// A ticket with a worktree holding one commit, in `stage` with `status`.
async fn at(w: &TestService, title: &str, stage: &str, status: Status) -> Ticket {
    let t = w.create("NOTE", title).await;
    let wt = w.start_worktree(&t).await;
    commit(&wt, "notes.txt", &format!("{title}\n"));
    w.force_stage(&t, stage, status).await;
    w.board.ticket(&t.key.to_string()).await.unwrap()
}

/// The ticket ran Dev in the conversation `<key>-dev`, then moved on to `stage`.
async fn ran_dev_then(w: &TestService, title: &str, stage: &str, status: Status) -> Ticket {
    let t = at(w, title, "dev", Status::Working).await;
    w.record_ended_agent(&t, &format!("{}-dev", t.key)).await;
    w.force_stage(&t, stage, status).await;
    w.board.ticket(&t.key.to_string()).await.unwrap()
}

/// The user's proposal of pocket-notes' flow without Product spec and Security, applied: it waits for the busy board.
async fn remove_product_spec_and_security(w: &TestService) {
    let mut file: FlowFile = w.board.current_flow_file("NOTE").await.unwrap();
    file.flow
        .stages
        .retain(|s| !["product-spec", "security"].contains(&s.id.as_str()));
    let proposal = w
        .board
        .open_proposal(
            "NOTE",
            &NewProposal {
                yaml: yaml::write(&file),
                source: Source::Document { file: None },
                proposed_by: ProposedBy::User,
            },
            &FixedModels::sample(),
        )
        .await
        .unwrap();
    let reply = otaq_core::client::request_once(
        &w.data.socket_path(),
        Request::FlowApply(ApplyParams {
            project: "NOTE".into(),
            proposal: proposal.id,
            choices: ApplyChoices::default(),
        }),
    )
    .await
    .unwrap();
    let Reply::FlowApplied(applied) = reply else {
        panic!("{reply:?}")
    };
    assert!(applied.waiting.is_some(), "the change waits");
}

async fn app(w: &TestService) -> App {
    let app = app_on(w.data.clone());
    tokio::time::timeout(Duration::from_secs(10), async {
        while !connected(&app.service.connection()) {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the app connects");
    app
}

async fn view(app: &App, picks: Value) -> Value {
    let load = request(app, "apply_now.view", json!({ "project": "NOTE", "picks": picks }))
        .await
        .unwrap();
    load["view"].clone()
}

fn row<'v>(view: &'v Value, key: &str) -> &'v Value {
    view["moves"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["ticket"] == key)
        .unwrap_or_else(|| panic!("no move for {key}: {view}"))
}

async fn place_and_status(w: &TestService, key: &str) -> (Place, Status) {
    let t = w.board.ticket(key).await.unwrap();
    (t.place, t.status)
}

async fn until(w: &TestService, key: &str, want: impl Fn(&Ticket) -> bool, what: &str) {
    let reached = tokio::time::timeout(Duration::from_secs(20), async {
        while !want(&w.board.ticket(key).await.unwrap()) {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await;
    assert!(reached.is_ok(), "{key} never {what}");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_example_applies_now_through_the_window() {
    let w = world().await;
    // NOTE-1 starts in Product spec, the flow's first stage, with the stand-in `claude`.
    let planning = w.create("NOTE", "Offline sync").await;
    w.board
        .move_ticket("NOTE-1", DropTarget::ReadyForDev, true, &me())
        .await
        .unwrap();
    let started = w.starter.start("NOTE-1", &me(), otaq_core::session::Start::Auto).await.unwrap();
    assert!(matches!(started, StartOutcome::Started(_)), "{started:?}");
    assert_eq!(
        place_and_status(&w, "NOTE-1").await.0,
        Place::stage_of("product-spec")
    );
    let product_spec_agent = w
        .sessions
        .running_of(&planning.id, SessionKind::Agent)
        .unwrap()
        .id;
    let checking = ran_dev_then(&w, "Share a note", "security", Status::Checking).await;
    let checker = w.shell(&checking, SessionKind::Validator).await;
    let working = at(&w, "Tags", "dev", Status::Working).await;
    let dev_agent = w.agent_shell(&working).await;
    let waiting = ran_dev_then(&w, "Search", "review", Status::WaitingForYou).await;
    assert_eq!(waiting.key.to_string(), "NOTE-4");
    remove_product_spec_and_security(&w).await;
    let before = w.board.current_flow("NOTE").await.unwrap().version;
    let app = app(&w).await;

    // The window: two moves, NOTE-1 on to Dev by default; NOTE-2 picked back to Dev resumes its conversation.
    let shown = view(&app, json!({})).await;
    assert_eq!(
        shown["summary"],
        "Removes Product spec and Security from pocket-notes\u{2019}s flow."
    );
    let keys: Vec<&str> = shown["moves"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["ticket"].as_str().unwrap())
        .collect();
    assert_eq!(keys, ["NOTE-1", "NOTE-2"]);
    assert_eq!(row(&shown, "NOTE-1")["picked"], "stage:dev");
    assert_eq!(row(&shown, "NOTE-2")["picked"], "stage:review");
    assert_eq!(
        row(&shown, "NOTE-2")["line"],
        "in Security (removed) \u{b7} agent will stop"
    );
    assert_eq!(
        shown["keep"],
        json!([{ "ticket": "NOTE-3", "title": "Tags", "line": "in Dev \u{b7} keeps running" }])
    );
    let picks = json!({ "NOTE-2": "stage:dev" });
    let shown = view(&app, picks.clone()).await;
    assert_eq!(row(&shown, "NOTE-2")["picked"], "stage:dev");
    assert_eq!(
        shown["hint"],
        "Back to Dev: Dev resumes NOTE-2\u{2019}s earlier session.",
        "{shown}"
    );

    // Apply now: the window closes.
    let answer = request(
        &app,
        "apply_now.apply",
        json!({ "project": "NOTE", "stamp": shown["stamp"], "picks": picks, "basis": shown["basis"] }),
    )
    .await
    .unwrap();
    assert_eq!(answer, json!({ "outcome": "closed" }));
    assert_eq!(view(&app, json!({})).await, Value::Null, "nothing waits");
    assert_eq!(
        w.board.current_flow("NOTE").await.unwrap().version,
        before + 1
    );
    let flow = w.board.effective_flow("NOTE").await.unwrap();
    let names: Vec<&str> = flow.stages.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["Dev", "Review", "Done"]);

    // Product spec's agent and Security's checker stopped; NOTE-3's agent didn't.
    let ended = |id: &str| w.sessions.info(id).map_or(true, |s| !s.is_running());
    for id in [product_spec_agent.as_str(), checker.session.as_str()] {
        let gone = tokio::time::timeout(Duration::from_secs(10), async {
            while !ended(id) {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await;
        assert!(gone.is_ok(), "{id} still runs");
    }
    assert_eq!(
        w.sessions
            .running_of(&working.id, SessionKind::Agent)
            .map(|s| s.id),
        Some(dev_agent.session.clone()),
        "NOTE-3's agent keeps running"
    );
    assert_eq!(
        place_and_status(&w, "NOTE-3").await,
        (Place::stage_of("dev"), Status::Working)
    );
    assert_eq!(
        place_and_status(&w, "NOTE-4").await,
        (Place::stage_of("review"), Status::WaitingForYou),
        "NOTE-4 untouched"
    );

    // NOTE-1 starts Dev in a new session; NOTE-2's Dev conversation resumes.
    until(&w, "NOTE-1", |t| t.status == Status::Working, "started Dev").await;
    assert_eq!(place_and_status(&w, "NOTE-1").await.0, Place::stage_of("dev"));
    let fresh = w
        .sessions
        .running_of(&planning.id, SessionKind::Agent)
        .unwrap();
    assert_ne!(fresh.id, product_spec_agent);
    let out = w.wait_for_output(&fresh.id, "ARGS").await;
    assert!(!out.contains("--resume"), "{out}");
    until(&w, "NOTE-2", |t| t.status == Status::Working, "resumed Dev").await;
    assert_eq!(place_and_status(&w, "NOTE-2").await.0, Place::stage_of("dev"));
    let resumed = w
        .sessions
        .running_of(&checking.id, SessionKind::Agent)
        .unwrap();
    assert_eq!(resumed.tool_session_id.as_deref(), Some("NOTE-2-dev"));
    let out = w.wait_for_output(&resumed.id, "ARGS").await;
    assert!(out.contains("--resume NOTE-2-dev"), "{out}");
    w.stop().await;
}
