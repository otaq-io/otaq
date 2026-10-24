//! Startup reconcile and Interrupted (T-0111, REC-01, SVC-03; d0018, d0045, d0022): after a restart or a crash,
//! tickets whose agent was at work are Interrupted (needs-you ones keeping their label and question), Review and
//! Approved tickets only get a History note, nothing resumes; an agent that exits on its own interrupts its ticket
//! too. `claude` is stood in for by ordinary commands; the crash test kills a real service process.

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::time::Duration;

use otaq_core::agent_tools::LaunchContext;
use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::board::moves::DropTarget;
use otaq_core::board::needs_you::NeedsYouGroup;
use otaq_core::board::projects::AddProject;
use otaq_core::board::tickets::EventKind;
use otaq_core::caller::Caller;
use otaq_core::data_dir::DataDir;
use otaq_core::domain::{Actor, Status};
use otaq_core::git::testing::TempRepos;
use otaq_core::protocol::{Reply, Request};
use otaq_core::service::testing::TestService;
use otaq_core::service::{Service, ServiceConfig};
use otaq_core::session::{END_SERVICE_STOPPED, SessionKind};
use otaq_core::store::now_ms;

fn me() -> Actor {
    Actor::User("me".into())
}

/// `claude` as an `sh` running `script`.
fn stand_in(script: &str) -> LaunchContext {
    LaunchContext {
        bin_dir: "/nonexistent".into(),
        plugin_dir: "/nonexistent/plugin".into(),
        path: std::env::var_os("PATH").unwrap_or_default(),
        claude: ClaudeLaunch {
            program: vec!["sh".into(), "-c".into(), script.into(), "sh".into()],
            config_dir: None,
        },
    }
}

async fn until_status(w: &TestService, key: &str, want: Status) {
    let reached = tokio::time::timeout(Duration::from_secs(20), async {
        while w.status(key).await != want {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await;
    if reached.is_err() {
        let t = w.board.ticket(key).await.unwrap();
        panic!(
            "{key} never became {want}: {} {:?}",
            t.status, t.start_problem
        );
    }
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

async fn events_of(w: &TestService, key: &str, kind: EventKind) -> Vec<serde_json::Value> {
    w.board
        .history(key)
        .await
        .unwrap()
        .into_iter()
        .filter(|e| e.kind == kind)
        .map(|e| e.payload)
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_restart_interrupts_tickets_at_work_and_notes_review_tickets() {
    let w = TestService::start_with(|config| ServiceConfig {
        launch: Some(stand_in("exec sleep 30")),
        ..config
    })
    .await;
    w.add_project("shop-web").await;
    let working = w.create("SHOP", "Search results paging").await; // SHOP-1
    let asks = w.create("SHOP", "Order confirmation email").await; // SHOP-2
    let review = w.create("SHOP", "Checkout API").await; // SHOP-3
    let backlog = w.create("SHOP", "Footer links").await; // SHOP-4
    let background = w.create("SHOP", "Upgrade Next").await; // SHOP-5
    for key in ["SHOP-1", "SHOP-2", "SHOP-3", "SHOP-5"] {
        start_agent(&w, key).await;
    }
    // SHOP-2 asks a question with options; SHOP-3 submitted for review (its session still runs).
    let session2 = w.sessions.running_of(&asks.id, SessionKind::Agent).unwrap();
    let agent2 = Caller::Agent {
        ticket: asks.key.clone(),
        ticket_id: asks.id.clone(),
        session: session2.id.clone(),
    };
    w.board
        .store_question(
            &asks.id,
            "Which email provider should I use?",
            &["Postmark".into(), "SES".into()],
            &[],
            Some(1),
        )
        .await
        .unwrap();
    w.signals
        .ask_human(&agent2, &asks.id, "Which email provider should I use?")
        .await
        .unwrap();
    w.force_status(&review, Status::WaitingForYou).await;
    w.force_status(&background, Status::WorkingInBackground)
        .await;

    // The user-clicked restart (T-0012) stops the service cleanly; the next start reconciles.
    let w = w.restart().await;
    let report = w.recovery.startup_report();
    assert!(!report.unclean, "a clean stop");
    let keys = |list: &[otaq_core::domain::TicketKey]| {
        list.iter().map(ToString::to_string).collect::<Vec<_>>()
    };
    assert_eq!(keys(&report.interrupted), ["SHOP-1", "SHOP-2", "SHOP-5"]);
    assert_eq!(keys(&report.session_ended), ["SHOP-3"]);

    let t = w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!((t.status, t.interrupted_from), (Status::Interrupted, None));
    assert_eq!(w.status("SHOP-5").await, Status::Interrupted);
    // The needs-you ticket keeps its label, its text and its question.
    let t = w.board.ticket("SHOP-2").await.unwrap();
    assert_eq!(t.status, Status::Interrupted);
    assert_eq!(t.interrupted_from, Some(Status::AsksYou));
    assert_eq!(
        t.signal.unwrap().text.as_deref(),
        Some("Which email provider should I use?")
    );
    let question = t.question.unwrap();
    assert_eq!((question.options.len(), question.recommended), (2, Some(1)));
    // Review: not Interrupted, no needs-you item of its own beyond Ready for Review, a History note.
    assert_eq!(w.status("SHOP-3").await, Status::WaitingForYou);
    assert_eq!(
        events_of(&w, "SHOP-3", EventKind::SessionEnded).await.len(),
        1
    );
    assert_eq!(w.status("SHOP-4").await, Status::Backlog);
    let interrupted = events_of(&w, "SHOP-1", EventKind::Interrupted).await;
    assert_eq!(interrupted[0]["reason"], "restart");
    assert_eq!(interrupted[0]["from"], "working");

    // Interrupted tickets are needs-you items in the second group; nothing was started again.
    let items = w.board.needs_you_items(now_ms()).await.unwrap();
    for key in ["SHOP-1", "SHOP-5"] {
        let item = items.iter().find(|i| i.key.to_string() == key).unwrap();
        assert_eq!(item.group, NeedsYouGroup::Stopped, "{key}");
    }
    // The needs-you one keeps its label: it stays with the needs-you items, with its question (d0045).
    let item = items
        .iter()
        .find(|i| i.key.to_string() == "SHOP-2")
        .unwrap();
    assert_eq!(
        (item.group, item.status, item.interrupted_from),
        (
            NeedsYouGroup::Family,
            Status::Interrupted,
            Some(Status::AsksYou)
        )
    );
    assert_eq!(item.reason, "asks: Which email provider should I use?");
    for ticket in [&working, &asks, &review, &background] {
        assert!(
            w.sessions
                .running_of(&ticket.id, SessionKind::Agent)
                .is_none(),
            "nothing resumes on its own"
        );
    }
    let _ = backlog;

    // The status reports the reconcile for the app's summary (REC-06).
    let client = otaq_core::client::Client::connect(otaq_core::client::ClientConfig::new(&w.data))
        .await
        .unwrap();
    let Reply::ServiceStatus(status) = client.request(Request::ServiceStatus).await.unwrap() else {
        panic!("status")
    };
    assert_eq!(
        (status.unclean_start, status.interrupted_at_start),
        (false, 3)
    );

    // Another restart doesn't note SHOP-3 again or touch Interrupted tickets.
    let w = w.restart().await;
    let report = w.recovery.startup_report();
    assert!(report.interrupted.is_empty() && report.session_ended.is_empty());
    assert_eq!(
        events_of(&w, "SHOP-3", EventKind::SessionEnded).await.len(),
        1
    );
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn shell_sessions_are_stopped_by_restart_and_never_started_again() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let t = w.create("SHOP", "Coupon codes").await;
    w.force_status(&t, Status::WaitingForYou).await;
    let shell = w.shell(&t, SessionKind::Shell).await;
    let w = w.restart().await;
    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let sink = seen.clone();
    // The hook point M9's Run and Shell use.
    w.recovery
        .on_stopped_by_restart(std::sync::Arc::new(move |info| {
            sink.lock()
                .unwrap()
                .push((info.id.clone(), info.end_reason.clone()));
        }));
    assert_eq!(
        *seen.lock().unwrap(),
        [(shell.session.clone(), Some("stopped by restart".to_owned()))]
    );
    assert_eq!(
        w.recovery.startup_report().stopped_by_restart,
        std::slice::from_ref(&shell.session)
    );
    assert!(w.sessions.running_of(&t.id, SessionKind::Shell).is_none());
    // A Review ticket without an agent session gets no note.
    assert!(w.recovery.startup_report().session_ended.is_empty());
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_agent_that_exits_on_its_own_interrupts_its_ticket() {
    let w = TestService::start_with(|config| ServiceConfig {
        launch: Some(stand_in("echo agent started; exec cat")),
        ..config
    })
    .await;
    w.add_project("shop-web").await;
    let t = w.create("SHOP", "Coupon codes").await;
    start_agent(&w, "SHOP-1").await;
    let session = w.sessions.running_of(&t.id, SessionKind::Agent).unwrap();
    // `cat` ends at end of input, as `claude` ends on /exit.
    w.sessions.write(&session.id, b"\x04").unwrap();
    until_status(&w, "SHOP-1", Status::Interrupted).await;
    let payload = &events_of(&w, "SHOP-1", EventKind::Interrupted).await[0];
    assert_eq!(payload["reason"], "exit 0");
    // The History row names the session's tool by its command (Q77).
    assert_eq!(payload["tool"], "claude");

    // A session Otaq stops itself (Park, Close, Done) doesn't interrupt.
    let t2 = w.create("SHOP", "Product images").await;
    start_agent(&w, "SHOP-2").await;
    let session = w.sessions.running_of(&t2.id, SessionKind::Agent).unwrap();
    w.sessions.stop(&session.id).await.unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(w.status("SHOP-2").await, Status::Working);
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_agent_that_exits_before_its_start_is_recorded_still_interrupts() {
    let w = TestService::start_with(|config| ServiceConfig {
        launch: Some(stand_in("exit 3")),
        ..config
    })
    .await;
    w.add_project("shop-web").await;
    w.create("SHOP", "Coupon codes").await;
    w.board
        .move_ticket("SHOP-1", DropTarget::ReadyForDev, true, &me())
        .await
        .unwrap();
    w.starter
        .start("SHOP-1", &me(), otaq_core::session::Start::Auto)
        .await
        .unwrap();
    until_status(&w, "SHOP-1", Status::Interrupted).await;
    w.stop().await;
}

/// The data folder and repository of the crash test's service process.
const CHILD_HOME: &str = "OTAQ_TEST_RECONCILE_HOME";
const CHILD_REPO: &str = "OTAQ_TEST_RECONCILE_REPO";
const READY: &str = "RECONCILE-CHILD-READY";

/// Not a test of its own: the service process the crash test starts and kills. Without the variables it does
/// nothing.
#[tokio::test(flavor = "multi_thread")]
async fn crash_child_service() {
    let (Ok(home), Ok(repo)) = (std::env::var(CHILD_HOME), std::env::var(CHILD_REPO)) else {
        return;
    };
    let data = DataDir::new(std::path::Path::new(&home).join("h"));
    let service = Service::bind(ServiceConfig {
        launch: Some(stand_in("echo agent started; exec sleep 60")),
        ..ServiceConfig::new(data)
    })
    .await
    .unwrap();
    let (board, starter) = (service.board(), service.starter());
    let task = tokio::spawn(service.serve());
    board
        .add_project(&AddProject {
            path: repo,
            ..Default::default()
        })
        .await
        .unwrap();
    for title in ["Split user full name", "Checkout API"] {
        board
            .create_ticket(
                &otaq_core::board::tickets::NewTicketInput {
                    project: "SHOP".into(),
                    title: title.into(),
                    ..Default::default()
                },
                &me(),
            )
            .await
            .unwrap();
        let key = if title.starts_with("Split") {
            "SHOP-1"
        } else {
            "SHOP-2"
        };
        board
            .move_ticket(key, DropTarget::ReadyForDev, true, &me())
            .await
            .unwrap();
        starter
            .start(key, &me(), otaq_core::session::Start::Auto)
            .await
            .unwrap();
    }
    println!("{READY}");
    let _ = task.await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_killed_service_leaves_interrupted_tickets_and_an_unclean_start() {
    let home = tempfile::TempDir::new().unwrap();
    let repos = TempRepos::new();
    let repo = repos.repo("shop-web", 1);
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "reconcile::crash_child_service", "--nocapture"])
        .env(CHILD_HOME, home.path())
        .env(CHILD_REPO, &repo)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let stdout = child.stdout.take().unwrap();
    let ready = tokio::task::spawn_blocking(move || {
        BufReader::new(stdout)
            .lines()
            .map_while(Result::ok)
            .any(|line| line.contains(READY))
    });
    let ready = tokio::time::timeout(Duration::from_secs(60), ready).await;
    if !matches!(ready, Ok(Ok(true))) {
        let _ = child.kill();
        panic!("the child service never got ready");
    }
    // The crash: no clean stop, its live sessions end with it.
    child.kill().unwrap();
    child.wait().unwrap();

    let w = TestService::start_in(home, repos, |config| config).await;
    let report = w.recovery.startup_report();
    assert!(report.unclean, "the previous run didn't stop cleanly");
    let interrupted: Vec<String> = report.interrupted.iter().map(ToString::to_string).collect();
    assert_eq!(interrupted, ["SHOP-1", "SHOP-2"]);
    for key in ["SHOP-1", "SHOP-2"] {
        assert_eq!(w.status(key).await, Status::Interrupted);
        let ticket = w.board.ticket(key).await.unwrap();
        let sessions = w
            .sessions
            .history(&otaq_core::session::SessionTicket {
                id: ticket.id.clone(),
                key: ticket.key.clone(),
            })
            .await
            .unwrap();
        assert_eq!(
            sessions[0].end_reason.as_deref(),
            Some(END_SERVICE_STOPPED),
            "the leftover session is recorded as ended"
        );
        assert!(sessions[0].tool_session_id.is_some(), "kept for Resume");
    }
    w.stop().await;
}
