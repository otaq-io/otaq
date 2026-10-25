//! Recovery and who may run what, by stage kind (T-0402, STG-09, STG-10; d0108, d0045, d0063). Every project runs
//! the default flow in M31, so the test flow's stages (`grill`, `security`, `qa` of `tests/flows/every-kind.yaml`)
//! are written onto tickets by hand: a stage the flow doesn't have goes by its status's set, and its runs record the
//! stage. The pure tables run on the test flow itself (`recovery::reconcile`'s and `policy`'s unit tests). `claude`
//! is stood in for by ordinary commands.

use std::path::Path;
use std::time::Duration;

use otaq_core::agent_tools::LaunchContext;
use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::board::moves::DropTarget;
use otaq_core::board::settings::{AgentLimits, GlobalSettings};
use otaq_core::board::tickets::Ticket;
use otaq_core::domain::{Actor, Place, Status};
use otaq_core::flow::{FlowFile, yaml};
use otaq_core::recovery::{ResumeAs, resume_as};
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::{AgentShell, TestService};
use otaq_core::session::{SessionKind, SessionTicket, StoredConversation};
use serde_json::{Value, json};

fn me() -> Actor {
    Actor::User("me".into())
}

/// Keeps running, printing its arguments.
const KEEPS_RUNNING: &str = "printf 'ARGS %s\\n' \"$*\"; exec sleep 30";

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

async fn until_status(w: &TestService, key: &str, want: Status) {
    let reached = tokio::time::timeout(Duration::from_secs(20), async {
        while w.status(key).await != want {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await;
    assert!(reached.is_ok(), "{key} never became {want}");
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

/// Moves the ticket into `stage` as it stands, as a flow with that stage would have.
async fn into_stage(w: &TestService, ticket: &Ticket, stage: &str) {
    sqlx::query("UPDATE tickets SET stage_id = ? WHERE id = ?")
        .bind(stage)
        .bind(&ticket.id)
        .execute(w.board.store().pool())
        .await
        .unwrap();
}

async fn place(w: &TestService, key: &str) -> Place {
    w.board.ticket(key).await.unwrap().place
}

fn every_kind_file() -> FlowFile {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/flows/every-kind.yaml");
    yaml::read(&std::fs::read_to_string(path).unwrap()).unwrap()
}

async fn conversation_of(w: &TestService, ticket: &Ticket) -> String {
    w.sessions
        .running_of(&ticket.id, SessionKind::Agent)
        .unwrap()
        .tool_session_id
        .unwrap()
}

/// Resumes `ticket` and checks its new session continues `conversation`.
async fn resumes(w: &TestService, ticket: &Ticket, conversation: &str) {
    let key = ticket.key.to_string();
    w.review.resume(&key, &me()).await.unwrap();
    until_status(w, &key, Status::Working).await;
    let resumed = w
        .sessions
        .running_of(&ticket.id, SessionKind::Agent)
        .unwrap();
    let out = w.wait_for_output(&resumed.id, "ARGS").await;
    assert!(
        out.contains(&format!("--resume {conversation}")),
        "{key}: {out}"
    );
    w.sessions.stop(&resumed.id).await.unwrap();
}

/// STG-09 on the test flow: after a restart, Grill working → Interrupted in Grill and Resume reopens Grill's
/// session (Dev's likewise); Security checking → Interrupted, checked afresh on Resume; QA (a check by you) → a
/// History note only. The startup reconcile runs before a test can put the restarted board on the test flow, so it
/// meets these stages as ones its flow doesn't have; Resume runs on the test flow.
#[tokio::test(flavor = "multi_thread")]
async fn a_restart_by_stage_kind() {
    let w = TestService::start_with(launch).await;
    w.add_project("shop-web").await;
    w.board.use_flow_for_tests("SHOP", &every_kind_file()).await;
    let grill = w.create("SHOP", "Sort notes by date").await; // SHOP-1
    let qa = w.create("SHOP", "Export as CSV").await; // SHOP-2
    let security = w.create("SHOP", "Search notes").await; // SHOP-3
    let dev = w.create("SHOP", "Rate limit the export").await; // SHOP-4
    // SHOP-4 skips Grill: its session starts in Dev, the stage it is resumed in (a session belongs to the stage it
    // started in).
    sqlx::query(
        "INSERT INTO ticket_stage_choices (ticket_id, stage_id, choice, set_by, set_at)
         VALUES (?, 'grill', 'skip', 'user', 1)",
    )
    .bind(&dev.id)
    .execute(w.board.store().pool())
    .await
    .unwrap();
    for key in ["SHOP-1", "SHOP-2", "SHOP-3", "SHOP-4"] {
        start_agent(&w, key).await;
    }
    assert_eq!(place(&w, "SHOP-1").await, Place::stage_of("grill"));
    w.force_status(&qa, Status::WaitingForYou).await;
    into_stage(&w, &qa, "qa").await;
    w.validating_with(&security, "checker-gone").await;
    into_stage(&w, &security, "security").await;
    into_stage(&w, &dev, "dev").await;
    let grill_conversation = conversation_of(&w, &grill).await;
    let dev_conversation = conversation_of(&w, &dev).await;

    let w = w.restart_with(launch).await;
    w.board.use_flow_for_tests("SHOP", &every_kind_file()).await;
    let report = w.recovery.startup_report();
    let keys = |list: &[otaq_core::domain::TicketKey]| {
        list.iter().map(ToString::to_string).collect::<Vec<_>>()
    };
    assert_eq!(keys(&report.interrupted), ["SHOP-1", "SHOP-3", "SHOP-4"]);
    assert_eq!(keys(&report.was_validating), ["SHOP-3"]);
    assert_eq!(keys(&report.session_ended), ["SHOP-2"]);

    let t = w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!((t.status, t.interrupted_from), (Status::Interrupted, None));
    assert_eq!(t.place, Place::stage_of("grill"), "Interrupted in Grill");
    assert_eq!(place(&w, "SHOP-4").await, Place::stage_of("dev"));
    let t = w.board.ticket("SHOP-3").await.unwrap();
    assert_eq!(
        (t.status, t.interrupted_from),
        (Status::Interrupted, Some(Status::Checking))
    );
    assert_eq!(t.place, Place::stage_of("security"));
    assert_eq!(
        resume_as(
            &w.board.effective_flow("SHOP").await.unwrap(),
            &t.place,
            t.interrupted_from
        ),
        ResumeAs::FreshCheck,
        "Resume starts a fresh Security"
    );
    assert_eq!(
        w.status("SHOP-2").await,
        Status::WaitingForYou,
        "a note only"
    );
    assert_eq!(place(&w, "SHOP-2").await, Place::stage_of("qa"));

    // Resume reopens the session of the stage the card is in, and the card stays there.
    resumes(&w, &grill, &grill_conversation).await;
    assert_eq!(place(&w, "SHOP-1").await, Place::stage_of("grill"));
    resumes(&w, &dev, &dev_conversation).await;
    assert_eq!(place(&w, "SHOP-4").await, Place::stage_of("dev"));
    w.stop().await;
}

/// The other side of Resume: a plain forward wait before Dev (Grill done, nothing to come back to) starts Dev in a
/// new conversation (STG-07), not Grill's.
#[tokio::test(flavor = "multi_thread")]
async fn a_forward_wait_before_dev_starts_a_new_conversation() {
    let w = TestService::start_with(launch).await;
    w.add_project("shop-web").await;
    w.board.use_flow_for_tests("SHOP", &every_kind_file()).await;
    let t = w.create("SHOP", "Sort notes by date").await;
    start_agent(&w, "SHOP-1").await;
    assert_eq!(place(&w, "SHOP-1").await, Place::stage_of("grill"));
    let grill_conversation = conversation_of(&w, &t).await;
    // Grill finished: waiting before Dev, as the engine's next stage leaves it.
    sqlx::query("UPDATE tickets SET stage_id = 'dev', state = ? WHERE id = ?")
        .bind(Status::Queued.as_str())
        .bind(&t.id)
        .execute(w.board.store().pool())
        .await
        .unwrap();
    assert_eq!(
        place(&w, "SHOP-1").await,
        Place::Waiting(Some("dev".into()))
    );
    w.starter
        .start("SHOP-1", &me(), otaq_core::session::Start::Auto)
        .await
        .unwrap();
    until_status(&w, "SHOP-1", Status::Working).await;
    assert_eq!(place(&w, "SHOP-1").await, Place::stage_of("dev"));
    let dev = w.sessions.running_of(&t.id, SessionKind::Agent).unwrap();
    let out = w.wait_for_output(&dev.id, "ARGS").await;
    assert!(!out.contains("--resume"), "{out}");
    assert!(out.contains("/otaq-work-ticket"), "{out}");
    assert_ne!(
        dev.tool_session_id.as_deref(),
        Some(grill_conversation.as_str())
    );
    w.sessions.stop(&dev.id).await.unwrap();
    w.stop().await;
}

/// An agent session that ended at `at`, with its tool's conversation id.
async fn ended_session(w: &TestService, ticket: &Ticket, conversation: &str, at: i64) {
    let project = w.board.project("SHOP").await.unwrap();
    sqlx::query(
        "INSERT INTO sessions (id, project_id, ticket_id, kind, tool, tool_session_id, state, started_at, ended_at)
         VALUES (?, ?, ?, 'agent', 'claude', ?, 'ended', ?, ?)",
    )
    .bind(otaq_core::store::new_id())
    .bind(&project.id)
    .bind(&ticket.id)
    .bind(conversation)
    .bind(at)
    .bind(at + 10)
    .execute(w.board.store().pool())
    .await
    .unwrap();
}

/// d0108: "Resume after a crash reopens the session of the stage the card is in", by its stage runs: a ticket back
/// in Dev continues Dev's conversation, not Grill's newer one. Waiting before the first stage, the newest.
#[tokio::test(flavor = "multi_thread")]
async fn resume_takes_the_session_of_the_stage_the_card_is_in() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let t = w.create("SHOP", "Sort notes by date").await;
    let now = otaq_core::store::now_ms();
    w.force_status(&t, Status::Working).await; // in Dev
    ended_session(&w, &t, "dev-conversation", now - 5000).await;
    into_stage(&w, &t, "grill").await;
    ended_session(&w, &t, "grill-conversation", now - 1000).await;
    into_stage(&w, &t, "dev").await;
    let latest = || async {
        StoredConversation::latest_agent(w.board.store().pool(), &t.id)
            .await
            .unwrap()
            .map(|c| c.id)
    };
    assert_eq!(latest().await.as_deref(), Some("dev-conversation"));
    w.force_status(&t, Status::Queued).await; // Ready for Dev
    assert_eq!(latest().await.as_deref(), Some("grill-conversation"));
    let runs = w.board.stage_runs("SHOP-1").await.unwrap();
    assert_eq!(
        runs.iter().map(|r| r.stage.as_str()).collect::<Vec<_>>(),
        ["dev", "grill", "dev"]
    );
    w.stop().await;
}

async fn send(w: &TestService, shell: &AgentShell, method: &str, params: Value) -> Value {
    let request =
        json!({"protocol_version": 1, "id": 1, "request": {"method": method, "params": params}});
    let ran = shell
        .send_raw(&w.data.socket_path(), &request.to_string())
        .await;
    serde_json::from_str(ran.stdout.lines().next().unwrap_or_default())
        .unwrap_or_else(|e| panic!("{e}: {ran:?}"))
}

fn refused(answer: &Value) -> &str {
    assert_eq!(answer["error"]["kind"], "refused", "{answer}");
    answer["error"]["message"].as_str().unwrap()
}

/// STG-10: a session's role is the kind of its stage run's stage, whatever kind of session runs it. A session
/// checking Security may not `stage next`; a Grill session may not `check pass`.
#[tokio::test(flavor = "multi_thread")]
async fn rights_follow_the_stage_run() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let grill = w.create("SHOP", "Sort notes by date").await;
    let security = w.create("SHOP", "Search notes").await;

    let grill_session = w.agent_shell(&grill).await;
    w.force_status(&grill, Status::Working).await;
    into_stage(&w, &grill, "grill").await;
    let answer = send(
        &w,
        &grill_session,
        "validation.verdict",
        json!({"key": "SHOP-1", "pass": true}),
    )
    .await;
    assert_eq!(
        refused(&answer),
        "SHOP-1's grill session can't give a check verdict"
    );

    // An agent-kind session whose run is Security's check: a checker, the verdict only.
    let checker = w.agent_shell(&security).await;
    w.force_status(&security, Status::Checking).await;
    into_stage(&w, &security, "security").await;
    sqlx::query("UPDATE stage_runs SET session_id = ? WHERE ticket_id = ? AND ended_at IS NULL")
        .bind(&checker.session)
        .bind(&security.id)
        .execute(w.board.store().pool())
        .await
        .unwrap();
    let answer = send(
        &w,
        &checker,
        "ticket.stage_next",
        json!({"key": "SHOP-2", "summary": "Done"}),
    )
    .await;
    assert_eq!(
        refused(&answer),
        "SHOP-2's security check can only give its verdict"
    );
    let answer = send(
        &w,
        &checker,
        "ticket.update",
        json!({"key": "SHOP-2", "title": "Search all notes"}),
    )
    .await;
    assert_eq!(
        refused(&answer),
        "SHOP-2's security check can only give its verdict"
    );
    let sessions = w
        .sessions
        .history(&SessionTicket {
            id: security.id.clone(),
            key: security.key.clone(),
        })
        .await
        .unwrap();
    assert_eq!(
        sessions[0].kind,
        SessionKind::Agent,
        "by stage run, not by kind"
    );
    w.stop().await;
}

/// A QA Send back with the only slot full waits before Dev (QA's fails set to the nearest earlier Agent stage, not
/// Grill, the flow's first); when the slot frees it Begins in Dev and resumes Dev's conversation (not a new one, not
/// Grill's newer).
#[tokio::test(flavor = "multi_thread")]
async fn a_send_back_with_the_limit_full_waits_before_its_agent_stage() {
    let w = TestService::start_with(launch).await;
    w.add_project("shop-web").await;
    let mut file = every_kind_file();
    let qa = file.flow.position("qa").unwrap();
    file.flow.stages[qa].fail_to = None;
    w.board.use_flow_for_tests("SHOP", &file).await;
    w.board
        .set_global_settings(&GlobalSettings {
            agent_limits: AgentLimits {
                enabled: true,
                max_concurrency: 1,
                back_pressure: 3,
            },
            ..Default::default()
        })
        .await
        .unwrap();
    let busy = w.create("SHOP", "Search notes").await; // SHOP-1
    let t = w.create("SHOP", "Export as CSV").await; // SHOP-2
    w.start_worktree(&t).await;
    let now = otaq_core::store::now_ms();
    w.force_status(&t, Status::Working).await;
    into_stage(&w, &t, "dev").await;
    ended_session(&w, &t, "dev-conversation", now - 5000).await;
    into_stage(&w, &t, "grill").await;
    ended_session(&w, &t, "grill-conversation", now - 1000).await;
    w.force_status(&t, Status::WaitingForYou).await;
    into_stage(&w, &t, "qa").await;
    w.force_status(&busy, Status::Working).await;

    w.review
        .fail("SHOP-2", "The CSV needs a header row.", None, false, &me())
        .await
        .unwrap();
    let queued = w.board.ticket("SHOP-2").await.unwrap();
    assert!(queued.status.is_queued(), "{}", queued.status);
    assert_eq!(
        queued.place,
        Place::Waiting(Some("dev".into())),
        "before Dev, not Grill"
    );

    // The slot frees: it Begins in Dev and continues Dev's conversation with the skill.
    w.force_status(&busy, Status::WaitingForYou).await;
    w.starter
        .start("SHOP-2", &me(), otaq_core::session::Start::Auto)
        .await
        .unwrap();
    until_status(&w, "SHOP-2", Status::Working).await;
    assert_eq!(place(&w, "SHOP-2").await, Place::stage_of("dev"));
    let session = w.sessions.running_of(&t.id, SessionKind::Agent).unwrap();
    let out = w.wait_for_output(&session.id, "ARGS").await;
    assert!(out.contains("--resume dev-conversation"), "{out}");
    w.sessions.stop(&session.id).await.unwrap();
    w.stop().await;
}
