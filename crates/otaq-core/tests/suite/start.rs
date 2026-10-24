//! The start sequence (T-0053) against temporary repositories, with an ordinary command in place of `claude`:
//! success, setup failure → Start interrupted → Retry start, diverged base → Held, no silent re-runs, and over
//! the socket: entering Queued queues, the scheduler starts it (T-0079).

use crate::common;

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::common::{Fixture, me};
use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::agent_tools::{Adapters, LaunchContext};
use otaq_core::board::moves::DropTarget;
use otaq_core::board::projects::AddProject;
use otaq_core::board::settings::ProjectSettings;
use otaq_core::board::tickets::{NewTicketInput, StartProblem, Ticket};
use otaq_core::client::{Client, ClientConfig};
use otaq_core::data_dir::DataDir;
use otaq_core::domain::{StartStep, Status, WentOn};
use otaq_core::flow::Delivers;
use otaq_core::flow::default::testing::delivering;
use otaq_core::git::testing::{TempRepos, commit, git};
use otaq_core::naming;
use otaq_core::ports::{PortPool, is_free_on_machine};
use otaq_core::protocol::{KeyParams, MoveParams, Reply, Request};
use otaq_core::service::testing::TestService;
use otaq_core::service::{EventBus, Service, ServiceConfig};
use otaq_core::session::{SessionKind, SessionState, Sessions};
use otaq_core::start::{StartOutcome, Starter};
use otaq_core::ticket_env::resources;

/// An ordinary command standing in for `claude`: prints a line, then waits like an interactive program.
fn stand_in() -> LaunchContext {
    LaunchContext {
        bin_dir: "/opt/cb/bin".into(),
        plugin_dir: "/opt/cb/plugin".into(),
        path: std::env::var_os("PATH").unwrap(),
        claude: ClaudeLaunch {
            program: vec![
                "sh".into(),
                "-c".into(),
                "echo agent started; exec cat".into(),
                "sh".into(),
            ],
            config_dir: None,
        },
    }
}

fn free_ports(store: otaq_core::store::Store) -> PortPool {
    // Pick a range where the first ports are free right now.
    for _ in 0..50 {
        let start = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        if let Some(end) = start.checked_add(9)
            && (start..=start + 3).all(is_free_on_machine)
        {
            return PortPool::with_range(store, start..=end);
        }
    }
    panic!("no free ports");
}

struct Setup {
    f: Fixture,
    sessions: Sessions,
    starter: Starter,
    data: DataDir,
}

fn starter_for(f: &Fixture, launch: Option<LaunchContext>) -> (Sessions, Starter, DataDir) {
    let data = DataDir::new(f.home.path().join("home"));
    let events = EventBus::new(64);
    let sessions = Sessions::new(f.board.store().clone(), events.clone());
    let starter = Starter::new(
        f.board.clone(),
        data.clone(),
        free_ports(f.board.store().clone()),
        sessions.clone(),
        Adapters::launching_only(launch, data.clone()),
        events,
    );
    (sessions, starter, data)
}

async fn setup() -> Setup {
    let f = Fixture::shop().await;
    let (sessions, starter, data) = starter_for(&f, Some(stand_in()));
    Setup {
        f,
        sessions,
        starter,
        data,
    }
}

async fn queued(f: &Fixture, title: &str) -> Ticket {
    let t = f
        .create_with(NewTicketInput {
            title: title.into(),
            ..common::shop_input()
        })
        .await;
    let t = f
        .board
        .move_ticket(&t.key.to_string(), DropTarget::ReadyForDev, false, &me())
        .await
        .unwrap();
    assert_eq!(t.status, Status::Queued);
    t
}

async fn set_project(f: &Fixture, settings: ProjectSettings) {
    f.board
        .set_project_settings("SHOP", &settings)
        .await
        .unwrap();
}

async fn event_kinds(f: &Fixture, t: &Ticket) -> Vec<String> {
    let rows: Vec<(String,)> =
        sqlx::query_as("SELECT kind FROM events WHERE ticket_id = ? ORDER BY at, id")
            .bind(&t.id)
            .fetch_all(f.board.store().pool())
            .await
            .unwrap();
    rows.into_iter().map(|(k,)| k).collect()
}

fn repo(f: &Fixture) -> PathBuf {
    f.repos.root().join("shop-web")
}

#[tokio::test]
async fn a_successful_start_leaves_the_ticket_working() {
    let s = setup().await;
    std::fs::write(repo(&s.f).join(".env.local"), "SECRET=1\n").unwrap();
    set_project(
        &s.f,
        ProjectSettings {
            env_files: vec![".env.local".into()],
            setup_command: "echo installed > setup-ran.txt".into(),
            ..Default::default()
        },
    )
    .await;
    queued(&s.f, "Checkout API").await;

    let outcome = s
        .starter
        .start("SHOP-1", &me(), otaq_core::session::Start::Auto)
        .await
        .unwrap();
    let StartOutcome::Started(session) = outcome else {
        panic!("{outcome:?}")
    };
    let t = s.f.ticket("SHOP-1").await;
    assert_eq!(t.status, Status::Working);
    assert_eq!(t.start_problem, None);
    let work = t.work.clone().unwrap();
    let worktree = PathBuf::from(work.worktree.unwrap());
    assert_eq!(
        worktree,
        s.data.worktree_path("shop-web", "SHOP-1").unwrap()
    );
    assert!(worktree.join(".env.local").is_file());
    assert!(worktree.join("setup-ran.txt").is_file());
    assert_eq!(
        work.branch.as_deref(),
        Some(otaq_core::naming::branch("SHOP-1").as_str())
    );
    assert_eq!(
        work.base_sha,
        Some(git(&repo(&s.f), &["rev-parse", "main"]))
    );
    let (agent, run) = (work.agent_port.unwrap(), work.run_port.unwrap());
    assert_ne!(agent, run);

    // Otaq's status line (USG-04) and the theme's kind (d0117 point 4): a settings file in the data folder, never
    // in the worktree.
    let settings = otaq_core::agent_tools::claude::statusline::settings_path(
        &s.data,
        &t.key,
        otaq_core::agent_tools::ThemeKind::Dark,
    );
    assert!(settings.starts_with(s.data.root()));
    let written: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&settings).unwrap()).unwrap();
    let keys: Vec<&String> = written.as_object().unwrap().keys().collect();
    assert_eq!(keys, ["statusLine", "theme"]);
    // Nothing picked yet: the service's theme is the dark one (d0117 point 4).
    assert_eq!(written["theme"], "dark");
    assert_eq!(
        written["statusLine"]["command"],
        otaq_core::agent_tools::claude::statusline::command_line(std::path::Path::new(
            "/opt/cb/bin"
        ))
    );
    assert!(!worktree.join(".claude").exists());

    // A live agent session with its tool, tool session id and no model, running in the worktree with $PORT.
    assert_eq!(session.kind, SessionKind::Agent);
    assert!(session.tool_session_id.is_some());
    assert_eq!(
        (session.tool, session.model.as_deref()),
        (Some(otaq_core::agent_tools::Tool::Claude), None)
    );
    let running = s
        .sessions
        .running_of(&t.id, SessionKind::Agent)
        .expect("live session");
    assert_eq!(running.id, session.id);
    s.sessions.write(&session.id, b"echo-check\n").unwrap();
    let mut attached = s.sessions.attach(&session.id).unwrap();
    let mut seen = attached.scrollback.clone();
    tokio::time::timeout(Duration::from_secs(10), async {
        while !String::from_utf8_lossy(&seen).contains("agent started") {
            seen.extend_from_slice(&attached.output.recv().await.unwrap().data);
        }
    })
    .await
    .unwrap();

    assert_eq!(event_kinds(&s.f, &t).await, ["created", "moved", "started"]);
    s.sessions.shutdown().await;
}

#[tokio::test]
async fn a_failed_setup_is_start_interrupted_and_retry_runs_it_all_again() {
    let s = setup().await;
    set_project(
        &s.f,
        ProjectSettings {
            // `cmd` syntax on Windows, where setup commands run with `cmd` (Q33).
            setup_command: if cfg!(windows) {
                "echo resolving packages& 1>&2 echo ERR_PNPM_NO_LOCKFILE& exit 1"
            } else {
                "echo resolving packages; echo 'ERR_PNPM_NO_LOCKFILE' >&2; exit 1"
            }
            .into(),
            ..Default::default()
        },
    )
    .await;
    let t = queued(&s.f, "Fix flaky login redirect").await;

    let outcome = s
        .starter
        .start("SHOP-1", &me(), otaq_core::session::Start::Auto)
        .await
        .unwrap();
    let StartOutcome::Interrupted { step, message } = &outcome else {
        panic!("{outcome:?}")
    };
    assert_eq!(*step, StartStep::Setup);
    assert!(
        message.starts_with("setup command failed (exit 1):"),
        "{message}"
    );
    assert!(message.contains("ERR_PNPM_NO_LOCKFILE"), "{message}");
    let after = s.f.ticket("SHOP-1").await;
    assert_eq!(after.status, Status::StartInterrupted);
    assert_eq!(
        after.start_problem,
        Some(StartProblem {
            step: StartStep::Setup,
            message: message.clone()
        })
    );
    // Nothing rolled back: the worktree and branch stay; no session, no ports yet.
    let worktree = s.data.worktree_path("shop-web", "SHOP-1").unwrap();
    assert!(worktree.is_dir());
    assert!(s.sessions.running_of(&t.id, SessionKind::Agent).is_none());
    assert_eq!(after.work.as_ref().unwrap().agent_port, None);
    let log = std::fs::read_to_string(s.data.setup_log_path("SHOP-1").unwrap()).unwrap();
    assert!(log.contains("resolving packages"), "{log}");

    // Never re-run by itself: time passes, a start call finds it not Queued.
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(
        s.starter
            .start("SHOP-1", &me(), otaq_core::session::Start::Auto)
            .await
            .unwrap(),
        StartOutcome::Skipped
    );
    assert_eq!(s.f.ticket("SHOP-1").await.status, Status::StartInterrupted);

    // The user fixes the command and clicks Retry start: the whole sequence runs and reuses the worktree.
    set_project(
        &s.f,
        ProjectSettings {
            setup_command: "true".into(),
            ..Default::default()
        },
    )
    .await;
    let retried = s.starter.retry("SHOP-1", &me()).await.unwrap();
    assert_eq!(retried.status, Status::Queued);
    assert_eq!(retried.start_problem, None);
    let outcome = s
        .starter
        .start("SHOP-1", &me(), otaq_core::session::Start::Auto)
        .await
        .unwrap();
    assert!(matches!(outcome, StartOutcome::Started(_)), "{outcome:?}");
    assert_eq!(s.f.ticket("SHOP-1").await.status, Status::Working);
    assert_eq!(
        event_kinds(&s.f, &t).await,
        [
            "created",
            "moved",
            "start_interrupted",
            "start_retried",
            "started"
        ]
    );

    // Retrying a ticket whose start didn't stop is refused.
    let err = s.starter.retry("SHOP-1", &me()).await.unwrap_err();
    assert!(err.to_string().contains("no stopped start"), "{err}");
    s.sessions.shutdown().await;
}

#[tokio::test]
async fn without_a_launch_context_the_launch_step_stops() {
    let f = Fixture::shop().await;
    let (sessions, starter, _) = starter_for(&f, None);
    queued(&f, "t").await;
    let outcome = starter
        .start("SHOP-1", &me(), otaq_core::session::Start::Auto)
        .await
        .unwrap();
    let StartOutcome::Interrupted { step, message } = outcome else {
        panic!()
    };
    assert_eq!(step, StartStep::Launch);
    assert!(message.contains("plugin wasn't found"), "{message}");
    assert!(sessions.in_memory().is_empty());
}

#[tokio::test]
async fn a_diverged_base_holds_the_ticket_without_a_worktree() {
    let f = Fixture::new().await;
    let clone = f.repos.clone_with_origin("billing-api", "develop", 1);
    f.board
        .add_project(&AddProject {
            path: clone.display().to_string(),
            key: Some("BILL".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    let other = f.repos.root().join("billing-api-seed");
    commit(&clone, "mine.txt", "mine\n");
    commit(&other, "theirs.txt", "theirs\n");
    git(&other, &["push", "--quiet", "origin", "develop"]);
    let t = f
        .create_with(NewTicketInput {
            project: "BILL".into(),
            title: "Invoices".into(),
            ..Default::default()
        })
        .await;
    f.board
        .move_ticket("BILL-1", DropTarget::ReadyForDev, false, &me())
        .await
        .unwrap();
    let (_sessions, starter, data) = starter_for(&f, Some(stand_in()));

    let outcome = starter
        .start("BILL-1", &me(), otaq_core::session::Start::Auto)
        .await
        .unwrap();
    let message = "`develop` and `origin/develop` have diverged. Push or pull, then retry";
    assert_eq!(
        outcome,
        StartOutcome::Held {
            message: message.into()
        }
    );
    let held = f.ticket("BILL-1").await;
    assert_eq!(held.status, Status::Held);
    assert_eq!(
        held.start_problem.as_ref().unwrap().step,
        StartStep::SyncBase
    );
    assert_eq!(held.start_problem.as_ref().unwrap().message, message);
    assert!(
        !data
            .worktree_path("billing-api", "BILL-1")
            .unwrap()
            .exists()
    );
    assert_eq!(held.work, None);

    // After the user pulls (here: the local commit goes away), Retry start works.
    git(&clone, &["reset", "--quiet", "--hard", "origin/develop"]);
    starter.retry("BILL-1", &me()).await.unwrap();
    let outcome = starter
        .start("BILL-1", &me(), otaq_core::session::Start::Auto)
        .await
        .unwrap();
    assert!(matches!(outcome, StartOutcome::Started(_)), "{outcome:?}");
    drop(t);
}

/// billing-api as a clone whose `origin` can't be reached (d0121), with BILL-1 and BILL-2 in Ready for Dev.
async fn unreachable_origin() -> (Fixture, PathBuf) {
    let f = Fixture::new().await;
    let clone = f.repos.clone_with_origin("billing-api", "develop", 1);
    f.board
        .add_project(&AddProject {
            path: clone.display().to_string(),
            key: Some("BILL".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    git(
        &clone,
        &[
            "remote",
            "set-url",
            "origin",
            "/nonexistent/billing-api-origin.git",
        ],
    );
    for title in ["Invoices", "Refunds"] {
        let t = f
            .create_with(NewTicketInput {
                project: "BILL".into(),
                title: title.into(),
                ..Default::default()
            })
            .await;
        f.board
            .move_ticket(&t.key.to_string(), DropTarget::ReadyForDev, false, &me())
            .await
            .unwrap();
    }
    (f, clone)
}

/// d0121: the fetch fails, and the flow lets the user pick at delivery (the default): the ticket starts from the
/// local base as it is, with the warning on the ticket and in History.
#[tokio::test]
async fn an_unreachable_remote_starts_from_the_local_base_with_a_warning() {
    let (f, clone) = unreachable_origin().await;
    let (_sessions, starter, _data) = starter_for(&f, Some(stand_in()));
    let local = git(&clone, &["rev-parse", "develop"]);

    let outcome = starter
        .start("BILL-1", &me(), otaq_core::session::Start::Auto)
        .await
        .unwrap();
    assert!(matches!(outcome, StartOutcome::Started(_)), "{outcome:?}");
    let t = f.ticket("BILL-1").await;
    assert_eq!(t.status, Status::Working);
    let work = t.work.clone().unwrap();
    assert_eq!(
        work.base_sha.as_deref(),
        Some(local.as_str()),
        "from the local develop"
    );
    let warning = work.remote_warning.unwrap();
    assert_eq!(
        (
            warning.went_on,
            warning.branch.as_str(),
            warning.remote.as_str()
        ),
        (WentOn::Started, "develop", "origin")
    );
    assert!(
        warning
            .reason
            .starts_with("git fetch --quiet origin failed"),
        "{}",
        warning.reason
    );
    assert!(
        event_kinds(&f, &t)
            .await
            .contains(&"remote_unreachable".to_owned())
    );
}

/// d0121: a flow that delivers by PR needs the remote: the start stops as before.
#[tokio::test]
async fn an_unreachable_remote_stops_a_pr_flow_start() {
    let (f, _clone) = unreachable_origin().await;
    f.board
        .edit_flow_for_tests("BILL", delivering(Delivers::Pr))
        .await;
    let (_sessions, starter, _data) = starter_for(&f, Some(stand_in()));

    let outcome = starter
        .start("BILL-1", &me(), otaq_core::session::Start::Auto)
        .await
        .unwrap();
    let StartOutcome::Interrupted { step, message } = outcome else {
        panic!("expected a stopped start, got {outcome:?}");
    };
    assert_eq!(step, StartStep::SyncBase);
    assert!(
        message.starts_with("git fetch --quiet origin failed (exit "),
        "{message}"
    );
    let t = f.ticket("BILL-1").await;
    assert_eq!(t.status, Status::StartInterrupted);
    assert_eq!(t.work.and_then(|w| w.remote_warning), None);
}

#[tokio::test]
async fn the_first_epic_child_syncs_the_epic_base() {
    let s = setup().await;
    let epic = s.f.epic("Wishlist").await;
    let child =
        s.f.create_with(NewTicketInput {
            title: "Wishlist API".into(),
            epic: Some(epic.key.to_string()),
            ..common::shop_input()
        })
        .await;
    s.f.board
        .move_ticket(
            &child.key.to_string(),
            DropTarget::ReadyForDev,
            false,
            &me(),
        )
        .await
        .unwrap();
    let outcome = s
        .starter
        .start("SHOP-1.1", &me(), otaq_core::session::Start::Auto)
        .await
        .unwrap();
    assert!(matches!(outcome, StartOutcome::Started(_)), "{outcome:?}");
    let epic_branch = otaq_core::naming::branch("SHOP-1");
    assert_eq!(
        git(&repo(&s.f), &["rev-parse", &epic_branch]),
        git(&repo(&s.f), &["rev-parse", "main"])
    );
    s.sessions.shutdown().await;
}

#[tokio::test]
async fn entering_queued_over_the_socket_and_the_scheduler_starts_it() {
    let repos = TempRepos::new();
    let repo = repos.repo("shop-web", 1);
    let tmp = tempfile::tempdir().unwrap();
    let data = DataDir::new(tmp.path().join("home"));
    let mut config = ServiceConfig::new(data.clone());
    config.launch = Some(stand_in());
    let service = Service::bind(config).await.unwrap();
    let stopper = service.stopper();
    let board = service.board();
    let sessions = service.sessions();
    let task = tokio::spawn(service.serve());
    board
        .add_project(&AddProject {
            path: repo.display().to_string(),
            ..Default::default()
        })
        .await
        .unwrap();
    board
        .set_project_settings(
            "SHOP",
            &ProjectSettings {
                setup_command: "exit 7".into(),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    board
        .create_ticket(
            &NewTicketInput {
                project: "SHOP".into(),
                title: "t".into(),
                ..Default::default()
            },
            &me(),
        )
        .await
        .unwrap();
    let client = Client::connect(ClientConfig::new(&data)).await.unwrap();

    let Reply::Ticket(moved) = client
        .request(Request::TicketMove(MoveParams {
            key: "SHOP-1".into(),
            target: DropTarget::ReadyForDev,
            confirmed: false,
        }))
        .await
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(moved.status, Status::Queued);
    let status = wait_for_status(&board, Status::StartInterrupted).await;
    assert_eq!(status.start_problem.unwrap().step, StartStep::Setup);

    board
        .set_project_settings("SHOP", &ProjectSettings::default())
        .await
        .unwrap();
    let Reply::Ticket(retried) = client
        .request(Request::TicketRetryStart(KeyParams {
            key: "SHOP-1".into(),
        }))
        .await
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(retried.status, Status::Queued);
    let working = wait_for_status(&board, Status::Working).await;
    let session = sessions
        .running_of(&working.id, SessionKind::Agent)
        .unwrap();
    assert_eq!(session.state, SessionState::Running);

    let Reply::ServiceStatus(status) = client.request(Request::ServiceStatus).await.unwrap() else {
        panic!()
    };
    assert_eq!(status.agents_working, 1);

    stopper.stop();
    task.await.unwrap();
}

async fn wait_for_status(board: &otaq_core::board::Board, status: Status) -> Ticket {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let t = board.ticket("SHOP-1").await.unwrap();
            if t.status == status {
                return t;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("never reached {status}"))
}

/// `unix` on macOS and Linux, `windows` on Windows, where project commands run with `cmd` (Q33).
fn by_os(unix: &str, windows: &str) -> String {
    if cfg!(windows) { windows } else { unix }.to_owned()
}

/// `$NAME` in `sh`, `%NAME%` in `cmd`.
fn var(name: &str) -> String {
    by_os(&format!("${name}"), &format!("%{name}%"))
}

async fn resource_row(f: &Fixture, t: &Ticket) -> (Option<String>, String, Option<String>) {
    sqlx::query_as(
        "SELECT resource_name, resource_state, resource_problem FROM tickets WHERE id = ?",
    )
    .bind(&t.id)
    .fetch_one(f.board.store().pool())
    .await
    .unwrap()
}

/// A shell line writing the environment to `file` (`sh` for the agent; `cmd` for setup on Windows).
fn writing_env(file: &Path, setup: bool) -> String {
    if setup && cfg!(windows) {
        format!(
            "set > \"{0}.tmp\"& echo END_OF_ENV>> \"{0}.tmp\"& move /y \"{0}.tmp\" \"{0}\" >nul",
            file.display()
        )
    } else {
        format!(
            "env > '{0}.tmp'; echo END_OF_ENV >> '{0}.tmp'; mv '{0}.tmp' '{0}'",
            file.display()
        )
    }
}

/// A file written by [`writing_env`], once it is complete (30 s at most).
async fn written_env(file: &Path) -> String {
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            if let Ok(text) = std::fs::read_to_string(file)
                && text.contains("END_OF_ENV")
            {
                return text;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("{} was never written", file.display()))
}

/// **The M19-d acceptance**: with empty resource settings nothing changes. The setup command and the started
/// agent get no `OTAQ_DB_*` and nothing is recorded. T-0445 (CMT-09, d0109): they still get `OTAQ_RESOURCE_DIR`,
/// the ticket's own folder, made at the start, for long output.
#[tokio::test]
async fn with_empty_resource_settings_nothing_changes() {
    let dir = tempfile::tempdir().unwrap();
    let agent_file = dir.path().join("agent.env");
    let setup_file = dir.path().join("setup.env");
    let launch = LaunchContext {
        claude: ClaudeLaunch {
            program: vec![
                "sh".into(),
                "-c".into(),
                format!("{}; exec cat", writing_env(&agent_file, false)),
                "sh".into(),
            ],
            config_dir: None,
        },
        ..stand_in()
    };
    let w = TestService::start_with(|config| ServiceConfig {
        launch: Some(launch),
        ..config
    })
    .await;
    w.add_project("shop-web").await;
    let ticket = w.create("SHOP", "Env").await;
    let settings = ProjectSettings {
        setup_command: writing_env(&setup_file, true),
        ..Default::default()
    };
    assert_eq!(settings.resource_create, "");
    w.board
        .set_project_settings("SHOP", &settings)
        .await
        .unwrap();
    w.force_status(&ticket, Status::Queued).await;
    let outcome = w
        .starter
        .start("SHOP-1", &me(), otaq_core::session::Start::Auto)
        .await
        .unwrap();
    assert!(matches!(outcome, StartOutcome::Started(_)), "{outcome:?}");

    let resource_vars = [naming::db_name_env_var(), naming::db_template_env_var()];
    let resource_dir = w.data.resource_path("shop-web", &ticket.key).unwrap();
    for (what, text) in [
        ("setup", written_env(&setup_file).await),
        ("agent", written_env(&agent_file).await),
    ] {
        for line in text.lines() {
            let name = line.split('=').next().unwrap_or_default();
            assert!(
                !resource_vars.iter().any(|v| v.eq_ignore_ascii_case(name)),
                "{what}: {line}"
            );
        }
        let ticket_line = format!("{}=SHOP-1", naming::ticket_env_var());
        assert!(
            text.lines().any(|l| l.to_ascii_uppercase() == ticket_line),
            "{what} knows its ticket"
        );
        let dir_line = format!(
            "{}={}",
            naming::resource_dir_env_var(),
            resource_dir.display()
        );
        assert!(
            cfg!(windows) || text.lines().any(|l| l == dir_line),
            "{what} has its resource folder: {text}"
        );
    }
    assert!(resource_dir.is_dir(), "the ticket's folder is made");
    let row = w.board.store().ticket(&ticket.id).await.unwrap().unwrap();
    assert_eq!(
        (row.resource_name, row.resource_state.as_str()),
        (None, "none")
    );
    let work = w.board.ticket("SHOP-1").await.unwrap().work.unwrap();
    assert_eq!(work.resource_name, None);
    w.stop().await;
}

/// Create runs after the env files and before setup, in the worktree, with the resource variables; what it makes in
/// `OTAQ_RESOURCE_DIR` is there for setup. Its output goes to the setup log, before setup's.
#[tokio::test]
async fn create_runs_after_env_files_and_before_setup() {
    let s = setup().await;
    std::fs::write(repo(&s.f).join(".env.local"), "SECRET=1\n").unwrap();
    let dir = var(&naming::resource_dir_env_var());
    let db = var(&naming::db_name_env_var());
    set_project(
        &s.f,
        ProjectSettings {
            env_files: vec![".env.local".into()],
            // The env file is already in the worktree when Create runs.
            resource_create: by_os(
                &format!("cat .env.local && echo {db} > \"{dir}/db.sqlite\" && echo create-ran"),
                &format!("type .env.local && echo {db}> \"{dir}\\db.sqlite\" && echo create-ran"),
            ),
            setup_command: by_os(
                &format!("cat \"{dir}/db.sqlite\" && echo setup-ran"),
                &format!("type \"{dir}\\db.sqlite\" && echo setup-ran"),
            ),
            ..Default::default()
        },
    )
    .await;
    let t = queued(&s.f, "Checkout API").await;
    let outcome = s
        .starter
        .start("SHOP-1", &me(), otaq_core::session::Start::Auto)
        .await
        .unwrap();
    assert!(matches!(outcome, StartOutcome::Started(_)), "{outcome:?}");

    let name = resources::db_name("SHOP", &t.key);
    let resource_dir = s.data.resource_path("shop-web", &t.key).unwrap();
    assert_eq!(
        std::fs::read_to_string(resource_dir.join("db.sqlite"))
            .unwrap()
            .trim(),
        name
    );
    assert_eq!(
        resource_row(&s.f, &t).await,
        (Some(name.clone()), "created".to_owned(), None)
    );
    let work = s.f.ticket("SHOP-1").await.work.unwrap();
    assert_eq!(work.resource_name.as_deref(), Some(name.as_str()));
    let log = std::fs::read_to_string(s.data.setup_log_path("SHOP-1").unwrap()).unwrap();
    // The last match of each is the output (each also appears in its `$ command` line).
    let at = |needle: &str| {
        log.rfind(needle)
            .unwrap_or_else(|| panic!("no {needle} in {log}"))
    };
    let setup_header = format!("$ {}", by_os("cat", "type"));
    assert!(at("SECRET=1") < at("create-ran"), "{log}");
    assert!(at("create-ran") < at(&setup_header), "{log}");
    assert!(at(&setup_header) < at(&name), "{log}");
    assert!(
        at(&name) < at("setup-ran"),
        "setup read what Create made: {log}"
    );
    s.sessions.shutdown().await;
}

/// A failed Create is Start interrupted at the resources step, setup doesn't run, and the name stays recorded;
/// Retry start runs Create again.
#[tokio::test]
async fn a_failed_create_is_start_interrupted_at_resources_and_retry_runs_it_again() {
    let s = setup().await;
    let failing = ProjectSettings {
        resource_create: by_os("echo no-server >&2; exit 3", "1>&2 echo no-server& exit 3"),
        setup_command: "echo setup > setup-ran.txt".into(),
        ..Default::default()
    };
    set_project(&s.f, failing.clone()).await;
    let t = queued(&s.f, "Orders").await;
    let outcome = s
        .starter
        .start("SHOP-1", &me(), otaq_core::session::Start::Auto)
        .await
        .unwrap();
    let StartOutcome::Interrupted { step, message } = &outcome else {
        panic!("{outcome:?}")
    };
    assert_eq!(*step, StartStep::Resources);
    assert_eq!(step.failure_label(), "creating ticket resources failed");
    assert!(
        message.starts_with("create command failed (exit 3):"),
        "{message}"
    );
    assert!(message.contains("no-server"), "{message}");
    let after = s.f.ticket("SHOP-1").await;
    assert_eq!(after.status, Status::StartInterrupted);
    assert_eq!(after.start_problem.unwrap().step, StartStep::Resources);
    let worktree = s.data.worktree_path("shop-web", "SHOP-1").unwrap();
    assert!(!worktree.join("setup-ran.txt").exists(), "setup never ran");
    let name = resources::db_name("SHOP", &t.key);
    assert_eq!(
        resource_row(&s.f, &t).await,
        (
            Some(name.clone()),
            "failed".to_owned(),
            Some(message.clone())
        )
    );

    set_project(
        &s.f,
        ProjectSettings {
            resource_create: "echo created".into(),
            ..failing
        },
    )
    .await;
    s.starter.retry("SHOP-1", &me()).await.unwrap();
    let outcome = s
        .starter
        .start("SHOP-1", &me(), otaq_core::session::Start::Auto)
        .await
        .unwrap();
    assert!(matches!(outcome, StartOutcome::Started(_)), "{outcome:?}");
    assert!(worktree.join("setup-ran.txt").is_file());
    assert_eq!(
        resource_row(&s.f, &t).await,
        (Some(name), "created".to_owned(), None),
        "the same name, now created"
    );
    s.sessions.shutdown().await;
}

/// A start that stops after Create (here at setup) doesn't create again on Retry start.
#[tokio::test]
async fn retry_start_does_not_create_twice() {
    let s = setup().await;
    let counter = s.f.home.path().join("creates.txt");
    let create = format!("echo created>> \"{}\"", counter.display());
    set_project(
        &s.f,
        ProjectSettings {
            resource_create: create.clone(),
            setup_command: "exit 1".into(),
            ..Default::default()
        },
    )
    .await;
    let t = queued(&s.f, "Orders").await;
    let outcome = s
        .starter
        .start("SHOP-1", &me(), otaq_core::session::Start::Auto)
        .await
        .unwrap();
    assert!(
        matches!(
            outcome,
            StartOutcome::Interrupted {
                step: StartStep::Setup,
                ..
            }
        ),
        "{outcome:?}"
    );
    assert_eq!(resource_row(&s.f, &t).await.1, "created");

    set_project(
        &s.f,
        ProjectSettings {
            resource_create: create,
            setup_command: "echo ok".into(),
            ..Default::default()
        },
    )
    .await;
    s.starter.retry("SHOP-1", &me()).await.unwrap();
    let outcome = s
        .starter
        .start("SHOP-1", &me(), otaq_core::session::Start::Auto)
        .await
        .unwrap();
    assert!(matches!(outcome, StartOutcome::Started(_)), "{outcome:?}");
    let creates = std::fs::read_to_string(&counter).unwrap();
    assert_eq!(creates.lines().count(), 1, "{creates}");
    s.sessions.shutdown().await;
}
