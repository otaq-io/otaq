//! Nothing starts on its own while paused (T-0456; PAU-04, PAU-05; d0112, d0007, d0030, d0063, d0064, d0083, d0086,
//! d0108): every session says who asked, and the backstop refuses an automatic one in a paused project; each automatic
//! caller keeps its start, which runs once on Resume; the user's click on one agent starts it and the pause stays.
//! `claude` stands in as an `sh` printing its arguments.

use std::path::Path;
use std::time::Duration;

use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::agent_tools::{LaunchContext, Tool};
use otaq_core::agents::kept::kept_tickets;
use otaq_core::agents::pause::PauseScope;
use otaq_core::agents::{AgentHold, HoldReason, SessionRole};
use otaq_core::board::moves::{DropOn, DropTarget};
use otaq_core::board::tickets::Ticket;
use otaq_core::caller::Caller;
use otaq_core::domain::{Actor, Status, ValidationTrigger};
use otaq_core::flow::apply_now::ApplyNowParams;
use otaq_core::flow::yaml;
use otaq_core::project_agents::AgentState;
use otaq_core::protocol::Event;
use otaq_core::review::drag::DragAnswers;
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::{TestService, said};
use otaq_core::session::{
    SessionError, SessionInfo, SessionKind, SessionTicket, SpawnSpec, Start, TerminalSize,
};
use otaq_core::start::StartOutcome;

/// Keeps running, printing its arguments.
const KEEPS_RUNNING: &str = "printf 'ARGS %s\\n' \"$*\"; exec sleep 30";

const PAUSED: &str = "shop-web is paused. Resume to continue.";

fn me() -> Actor {
    Actor::User("me".into())
}

fn shop() -> PauseScope {
    PauseScope::Project {
        project: "SHOP".into(),
    }
}

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
    w.add_project("shop-web").await;
    w.add_project("billing-api").await;
    w
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

/// Nothing happens for a while: long enough for the scheduler's pass the move triggered.
async fn settle() {
    tokio::time::sleep(Duration::from_millis(400)).await;
}

async fn ticket(w: &TestService, key: &str) -> Ticket {
    w.board.ticket(key).await.unwrap()
}

/// Every session of `kind` the ticket ever had, newest first.
async fn sessions_of(w: &TestService, key: &str, kind: SessionKind) -> Vec<SessionInfo> {
    let t = ticket(w, key).await;
    let mut all = w
        .sessions
        .history(&SessionTicket {
            id: t.id,
            key: t.key,
        })
        .await
        .unwrap();
    all.retain(|s| s.kind == kind);
    all
}

async fn running(w: &TestService, key: &str, kind: SessionKind) -> usize {
    sessions_of(w, key, kind)
        .await
        .iter()
        .filter(|s| s.is_running())
        .count()
}

/// Queued in Ready for Dev, and a scheduler pass asked for (the board's own move publishes nothing; the request's
/// handler does).
async fn queue(w: &TestService, key: &str) {
    w.board
        .move_ticket(key, DropTarget::ReadyForDev, true, &me())
        .await
        .unwrap();
    w.events.publish(Event::ProjectsChanged);
}

/// Queued and started at once, as the scheduler would.
async fn start_agent(w: &TestService, key: &str) {
    queue(w, key).await;
    let _ = w.starter.start(key, &me(), Start::Auto).await.unwrap();
    until_status(w, key, Status::Working).await;
}

fn spec_first() -> otaq_core::flow::FlowFile {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("flows/spec-first.yaml");
    yaml::read(&std::fs::read_to_string(path).unwrap()).unwrap()
}

async fn stop_all(w: TestService) {
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

/// The backstop (PAU-04): an automatic start in a paused project is refused with the line; the user's isn't, and
/// another project's automatic start runs.
#[tokio::test(flavor = "multi_thread")]
async fn the_backstop_refuses_only_an_automatic_start_in_a_paused_project() {
    let w = world().await;
    let shop1 = w.create("SHOP", "Search results paging").await;
    let bill1 = w.create("BILL", "Invoice template").await;
    w.pause.pause(&shop()).await.unwrap();
    let spec = |t: &Ticket, project_id: &str, start: Start| SpawnSpec {
        kind: SessionKind::Agent,
        start,
        project_id: project_id.to_owned(),
        project_key: t.key.project.to_string(),
        ticket: Some(SessionTicket {
            id: t.id.clone(),
            key: t.key.clone(),
        }),
        program: "sh".into(),
        args: vec!["-c".into(), "exec sleep 30".into()],
        cwd: std::env::temp_dir(),
        env: vec![],
        size: TerminalSize { cols: 80, rows: 24 },
        tool: None,
        tool_session_id: None,
        model: None,
        agent: None,
        effort: None,
        fallback: None,
        theme: None,
    };
    let shop_id = w.board.project("SHOP").await.unwrap().id;
    let bill_id = w.board.project("BILL").await.unwrap().id;

    let refused = w
        .sessions
        .spawn(spec(&shop1, &shop_id, Start::Auto))
        .await
        .unwrap_err();
    assert!(
        matches!(refused, SessionError::Paused { .. }),
        "{refused:?}"
    );
    assert_eq!(refused.to_string(), PAUSED);
    assert!(
        sessions_of(&w, "SHOP-1", SessionKind::Agent)
            .await
            .is_empty()
    );

    let users = w
        .sessions
        .spawn(spec(&shop1, &shop_id, Start::User))
        .await
        .unwrap();
    assert!(users.is_running(), "the user's click starts it");
    let other = w
        .sessions
        .spawn(spec(&bill1, &bill_id, Start::Auto))
        .await
        .unwrap();
    assert!(other.is_running(), "billing-api isn't paused");
    assert!(w.pause.is_paused("SHOP").await.unwrap(), "the pause stays");
    stop_all(w).await;
}

/// The scheduler and the start sequence (PAU-04, PAU-06; d0007, d0108): SHOP-35 dragged into Product spec while paused
/// waits there ("Paused · starts when you resume") and starts on Resume, once; an automatic start in the meantime is
/// kept; billing-api keeps starting. Pause and Resume again bring it back on its conversation, still once.
#[tokio::test(flavor = "multi_thread")]
async fn a_ticket_dragged_into_product_spec_while_paused_waits_and_starts_once_on_resume() {
    let w = world().await;
    w.board.use_flow_for_tests("SHOP", &spec_first()).await;
    w.create("SHOP", "Saved carts").await;
    w.create("BILL", "Invoice template").await;
    w.pause.pause(&shop()).await.unwrap();

    w.review
        .drag(
            "SHOP-1",
            &DropOn::stage("product-spec"),
            DragAnswers {
                confirmed: true,
                ..DragAnswers::default()
            },
            &w.validation,
            &me(),
        )
        .await
        .unwrap();
    queue(&w, "BILL-1").await;
    until_status(&w, "BILL-1", Status::Working).await;
    settle().await;
    assert_eq!(w.status("SHOP-1").await, Status::Queued, "it waits");
    assert!(
        sessions_of(&w, "SHOP-1", SessionKind::Agent)
            .await
            .is_empty()
    );
    assert_eq!(
        kept_tickets(&w.board, "SHOP").await.unwrap(),
        ["SHOP-1".parse().unwrap()],
        "Paused · starts when you resume"
    );
    assert_eq!(
        w.starter.start("SHOP-1", &me(), Start::Auto).await.unwrap(),
        StartOutcome::Kept
    );
    assert_eq!(w.status("SHOP-1").await, Status::Queued);
    assert_eq!(
        ticket(&w, "SHOP-1").await.start_problem,
        None,
        "no start step left to look cut short"
    );

    w.pause.resume(&shop(), &me()).await.unwrap();
    until_status(&w, "SHOP-1", Status::Working).await;
    assert!(ticket(&w, "SHOP-1").await.place.is_in("product-spec"));
    assert!(kept_tickets(&w.board, "SHOP").await.unwrap().is_empty());
    w.pause.resume(&shop(), &me()).await.unwrap();
    settle().await;
    assert_eq!(sessions_of(&w, "SHOP-1", SessionKind::Agent).await.len(), 1);

    // Pause, Resume: back on its conversation, one session running.
    w.pause.pause(&shop()).await.unwrap();
    assert_eq!(w.status("SHOP-1").await, Status::Interrupted);
    w.pause.resume(&shop(), &me()).await.unwrap();
    until_status(&w, "SHOP-1", Status::Working).await;
    settle().await;
    assert_eq!(sessions_of(&w, "SHOP-1", SessionKind::Agent).await.len(), 2);
    assert_eq!(running(&w, "SHOP-1", SessionKind::Agent).await, 1);
    stop_all(w).await;
}

/// The user's clicks (PAU-05; d0112 open point 2): answering SHOP-12's question while paused starts only its agent
/// with the answer, and SHOP-33's Resume starts only SHOP-33; the pause stays and nothing else starts. Resume all
/// leaves the paused project alone.
#[tokio::test(flavor = "multi_thread")]
async fn answering_or_resuming_one_ticket_starts_only_it_and_the_pause_stays() {
    let w = world().await;
    w.board.use_flow_for_tests("SHOP", &spec_first()).await;
    for title in ["Dark mode", "Saved carts", "Gift cards"] {
        w.create("SHOP", title).await;
    }
    for key in ["SHOP-1", "SHOP-2", "SHOP-3"] {
        start_agent(&w, key).await;
    }
    // SHOP-1 (Product spec) asks.
    let asks = ticket(&w, "SHOP-1").await;
    let session = w.sessions.running_of(&asks.id, SessionKind::Agent).unwrap();
    let question = "Should dark mode follow the system setting?";
    w.board
        .store_question(
            &asks.id,
            question,
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
            question,
        )
        .await
        .unwrap();
    w.pause.pause(&shop()).await.unwrap();
    for key in ["SHOP-1", "SHOP-2", "SHOP-3"] {
        assert_eq!(w.status(key).await, Status::Interrupted, "{key}");
    }

    // Resume all leaves a paused project alone.
    assert!(w.review.resume_all(None, &me()).await.unwrap().is_empty());

    w.review
        .resume_with_answer("SHOP-1", "Yes", &me())
        .await
        .unwrap();
    until_status(&w, "SHOP-1", Status::Working).await;
    let spec_writer = &sessions_of(&w, "SHOP-1", SessionKind::Agent).await[0];
    assert!(spec_writer.is_running());
    assert_eq!(
        spec_writer.agent.as_ref().map(|a| a.name.as_str()),
        Some("Spec writer")
    );
    w.review.resume("SHOP-3", &me()).await.unwrap();
    until_status(&w, "SHOP-3", Status::Working).await;
    settle().await;
    assert_eq!(
        w.status("SHOP-2").await,
        Status::Interrupted,
        "only the one"
    );
    assert_eq!(running(&w, "SHOP-2", SessionKind::Agent).await, 0);
    assert!(w.pause.is_paused("SHOP").await.unwrap(), "the pause stays");

    // The project's Resume brings back SHOP-2, and only it.
    w.pause.resume(&shop(), &me()).await.unwrap();
    until_status(&w, "SHOP-2", Status::Working).await;
    settle().await;
    for key in ["SHOP-1", "SHOP-2", "SHOP-3"] {
        assert_eq!(running(&w, key, SessionKind::Agent).await, 1, "{key}");
    }
    stop_all(w).await;
}

/// A check after a stage (PAU-04; d0063): the submit of a ticket the user resumed alone doesn't start its checker; the
/// ticket is taken the pause's way (interrupted from its check, the round not counted) and the project's Resume starts
/// a fresh check, once. The user's Retry validation starts a checker while paused.
#[tokio::test(flavor = "multi_thread")]
async fn a_check_after_a_submit_waits_for_resume() {
    let w = world().await;
    w.create("SHOP", "Checkout API").await;
    start_agent(&w, "SHOP-1").await;
    w.pause.pause(&shop()).await.unwrap();
    w.review.resume("SHOP-1", &me()).await.unwrap();
    until_status(&w, "SHOP-1", Status::Working).await;

    w.validation
        .submit(
            "SHOP-1",
            said(&w.board, "SHOP-1", "Checkout API done.")
                .await
                .as_ref(),
            &Actor::Agent("SHOP-1".parse().unwrap()),
            ValidationTrigger::Agent,
        )
        .await
        .unwrap();
    let t = ticket(&w, "SHOP-1").await;
    assert_eq!(
        (t.status, t.interrupted_from),
        (Status::Interrupted, Some(Status::Checking))
    );
    assert!(
        sessions_of(&w, "SHOP-1", SessionKind::Validator)
            .await
            .is_empty()
    );

    w.pause.resume(&shop(), &me()).await.unwrap();
    until_status(&w, "SHOP-1", Status::Checking).await;
    until("a checker runs", async || {
        running(&w, "SHOP-1", SessionKind::Validator).await == 1
    })
    .await;
    w.pause.resume(&shop(), &me()).await.unwrap();
    settle().await;
    assert_eq!(
        sessions_of(&w, "SHOP-1", SessionKind::Validator)
            .await
            .len(),
        1
    );

    // Retry validation is the user's click: it starts while paused.
    w.pause.pause(&shop()).await.unwrap();
    assert!(
        w.validation
            .start_validator(&t.id, Start::User)
            .await
            .unwrap()
            .session()
            .is_some()
    );
    assert!(w.pause.is_paused("SHOP").await.unwrap());
    stop_all(w).await;
}

/// A Project agent's first message is the user's click (PAU-05): it starts while the project is paused, and the
/// pause stays.
#[tokio::test(flavor = "multi_thread")]
async fn the_users_agents_start_while_paused() {
    let w = world().await;
    w.pause.pause(&shop()).await.unwrap();
    let agent = w
        .project_agents
        .open("SHOP", "Import GitHub issues")
        .await
        .unwrap();
    assert_eq!(agent.state, AgentState::Running);
    assert!(w.pause.is_paused("SHOP").await.unwrap());
    stop_all(w).await;
}

/// The launch gate's lift (PAU-04; d0086): a paused project's held start stays held when the holds lift; Resume
/// lifts it once more and it starts.
#[tokio::test(flavor = "multi_thread")]
async fn the_gates_lift_keeps_a_paused_projects_hold() {
    let w = world().await;
    let t = w.create("SHOP", "Search results paging").await;
    queue(&w, "SHOP-1").await;
    w.pause.pause(&shop()).await.unwrap();
    w.force_status(&t, Status::Held).await;
    sqlx::query("UPDATE tickets SET start_problem_step = 'tool_limit', start_problem_message = 'Claude Code limit reached' WHERE id = ?")
        .bind(&t.id)
        .execute(w.board.store().pool())
        .await
        .unwrap();
    let developer = w
        .board
        .store()
        .agent_named("Developer")
        .await
        .unwrap()
        .unwrap();
    w.board
        .store()
        .set_agent_hold(&AgentHold {
            id: "hold-1".into(),
            ticket_id: Some(t.id.clone()),
            project_id: None,
            check_agent: None,
            role: SessionRole::Developer,
            reason: HoldReason::ToolLimit,
            tool: Tool::Claude,
            model: None,
            agent_id: Some(developer.id),
            fallback_tool: None,
            fallback_held: false,
            fallback_asked: false,
            since: 1,
        })
        .await
        .unwrap();

    assert_eq!(w.gate.lift().await.unwrap(), 0, "kept");
    assert_eq!(w.status("SHOP-1").await, Status::Held);

    w.pause.resume(&shop(), &me()).await.unwrap();
    until_status(&w, "SHOP-1", Status::Working).await;
    settle().await;
    assert_eq!(sessions_of(&w, "SHOP-1", SessionKind::Agent).await.len(), 1);
    stop_all(w).await;
}

/// Apply now and an automatic PR Delivery start agents on their own: refused while paused with the line (PAU-04).
#[tokio::test(flavor = "multi_thread")]
async fn apply_now_and_an_automatic_pr_are_refused_while_paused() {
    let w = world().await;
    w.create("SHOP", "Footer links").await;
    w.pause.pause(&shop()).await.unwrap();
    let refused = w
        .review
        .apply_now(
            &ApplyNowParams {
                project: "SHOP".into(),
                stamp: "any".into(),
                picks: Vec::new(),
            },
            &w.validation,
            &me(),
        )
        .await
        .unwrap_err();
    assert_eq!(refused.to_string(), PAUSED);
    let refused = w.prs.open_automatically("SHOP-1", false).await.unwrap_err();
    assert_eq!(refused.to_string(), PAUSED);
    w.stop().await;
}

/// Send back and a Delivery's conflict (PAU-04; d0064): while paused the ticket goes back to Dev and waits there
/// with its skill to run; the project's Resume resumes its conversation with it, once.
#[tokio::test(flavor = "multi_thread")]
async fn send_back_and_a_conflicts_restack_wait_for_resume() {
    use otaq_core::git::testing::commit;
    use otaq_core::review::LocalMergeOutcome;

    let w = world().await;
    let repo = std::path::PathBuf::from(&w.board.project("SHOP").await.unwrap().repo_path);
    let sent = w.create("SHOP", "Search results paging").await;
    w.start_worktree(&sent).await;
    w.record_ended_agent(&sent, "0199-sent-back").await;
    w.force_status(&sent, Status::WaitingForYou).await;
    let conflicting = w.create("SHOP", "Pricing").await;
    let wt = w.start_worktree(&conflicting).await;
    commit(&wt, "lib.ts", "ticket\n");
    commit(&repo, "lib.ts", "main\n");
    w.record_ended_agent(&conflicting, "0199-conflicting").await;
    w.force_status(&conflicting, Status::WaitingForYou).await;
    w.pause.pause(&shop()).await.unwrap();

    w.review
        .fail("SHOP-1", "Page size 20, not 50.", None, false, &me())
        .await
        .unwrap();
    let merged = w
        .review
        .local_merge("SHOP-2", "m", false, &me())
        .await
        .unwrap();
    assert!(
        matches!(merged, LocalMergeOutcome::Conflict { .. }),
        "{merged:?}"
    );
    settle().await;
    for key in ["SHOP-1", "SHOP-2"] {
        assert_eq!(w.status(key).await, Status::Queued, "{key} waits");
        assert!(
            sessions_of(&w, key, SessionKind::Agent).await.len() == 1,
            "{key}"
        );
    }

    w.pause.resume(&shop(), &me()).await.unwrap();
    for (key, conversation) in [("SHOP-1", "0199-sent-back"), ("SHOP-2", "0199-conflicting")] {
        until(&format!("{key} resumes"), async || {
            running(&w, key, SessionKind::Agent).await == 1
        })
        .await;
        let session = &sessions_of(&w, key, SessionKind::Agent).await[0];
        let out = w.wait_for_output(&session.id, "ARGS").await;
        assert!(
            out.contains(&format!("--resume {conversation}")),
            "{key}: {out}"
        );
    }
    w.pause.resume(&shop(), &me()).await.unwrap();
    settle().await;
    for key in ["SHOP-1", "SHOP-2"] {
        assert_eq!(
            sessions_of(&w, key, SessionKind::Agent).await.len(),
            2,
            "{key}: once"
        );
    }
    stop_all(w).await;
}
