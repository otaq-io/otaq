//! `ticket.terminal` (T-0317; SHL-10, CLI-09, CLI-03; d0087, d0031, d0068, d0083, d0045): a ticket's terminal as
//! the user sees it, read-only. The text comes from the session's kept output replayed through a terminal
//! emulator; who may read it is judged from where the request comes from (an ordinary `sh` started as an agent,
//! a validator or a Project agent, running `nc`).

use std::path::PathBuf;

use otaq_core::agent_tools::LaunchContext;
use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::board::tickets::Ticket;
use otaq_core::client::{Client, ClientConfig, ClientError};
use otaq_core::domain::Status;
use otaq_core::git::testing::TempRepos;
use otaq_core::protocol::{ErrorKind, Reply, Request, TerminalParams};
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::{AgentShell, TestService, path_without_build_folders};
use otaq_core::session::{SessionKind, SessionTicket, SpawnSpec, Start, TerminalSize};
use serde_json::{Value, json};

/// The sample board (SHOP), a second project `notes-api`, and
/// `claude` an interactive `sh` in the service's data folder (for Project agents).
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

async fn ticket(w: &TestService, key: &str) -> Ticket {
    w.board.ticket(key).await.unwrap()
}

/// A session of `kind` for `ticket` whose program prints `output` (printf syntax) and then waits, in a terminal of
/// `size`.
async fn printing(
    w: &TestService,
    ticket: &Ticket,
    kind: SessionKind,
    output: &str,
    size: TerminalSize,
) -> String {
    let project = w.board.project(ticket.key.project.as_str()).await.unwrap();
    let info = w
        .sessions
        .spawn(SpawnSpec {
            kind,
            start: Start::User,
            project_id: project.id.clone(),
            project_key: project.key.clone(),
            ticket: Some(SessionTicket {
                id: ticket.id.clone(),
                key: ticket.key.clone(),
            }),
            program: "sh".into(),
            args: vec![
                "-c".into(),
                format!("printf '{output}'; printf '\\033[0m<end>'; exec cat"),
            ],
            cwd: PathBuf::from(&project.repo_path),
            env: vec![],
            size,
            tool: None,
            tool_session_id: None,
            model: None,
            agent: None,
            effort: None,
            fallback: None,
            theme: None,
        })
        .await
        .unwrap();
    w.wait_for_output(&info.id, "<end>").await;
    info.id
}

/// `ticket.terminal` from the user (the app, or their own terminal).
async fn as_user(
    w: &TestService,
    key: &str,
    stage: Option<&str>,
) -> Result<(String, String), String> {
    of_agent(w, key, stage, None).await
}

/// `ticket.terminal` from the user with `--agent` (d0130 row 17); a refusal or a not-found is its message.
async fn of_agent(
    w: &TestService,
    key: &str,
    stage: Option<&str>,
    agent: Option<&str>,
) -> Result<(String, String), String> {
    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    match client
        .request(Request::TicketTerminal(TerminalParams {
            key: key.into(),
            stage: stage.map(str::to_owned),
            agent: agent.map(str::to_owned),
        }))
        .await
    {
        Ok(Reply::TicketTerminal(t)) => {
            assert_eq!(t.key.to_string(), key);
            Ok((t.session, t.text))
        }
        Ok(other) => panic!("unexpected {other:?}"),
        Err(ClientError::Service(e)) => {
            assert!(
                matches!(e.kind, ErrorKind::Refused | ErrorKind::NotFound),
                "{e:?}"
            );
            Err(e.message)
        }
        Err(e) => panic!("{e}"),
    }
}

/// `ticket.terminal` from inside `shell`'s session, in the socket's shape.
async fn from(w: &TestService, shell: &AgentShell, key: &str) -> Value {
    let request = json!({"protocol_version": 1, "id": 1,
        "request": {"method": "ticket.terminal", "params": {"key": key}}});
    let ran = shell
        .send_raw(&w.data.socket_path(), &request.to_string())
        .await;
    serde_json::from_str(ran.stdout.lines().next().unwrap_or_default())
        .unwrap_or_else(|e| panic!("{e}: {ran:?}"))
}

fn allowed(answer: &Value) -> &str {
    assert!(answer.get("error").is_none(), "{answer}");
    answer["reply"]["ticket_terminal"]["text"].as_str().unwrap()
}

fn refused(answer: &Value) -> &str {
    assert_eq!(answer["error"]["kind"], "refused", "{answer}");
    answer["error"]["message"].as_str().unwrap()
}

const PROMPT: &str = "\\033[2J\\033[H\\033[1mBash command\\033[22m\\r\\n\\r\\n  \\033[2mrm -rf .next\\033[22m\
                      \\033[5;1HDo you want to proceed?\\r\\n\\033[36m> 1. Yes\\033[39m   \\r\\n  2. No";

/// FLW-20 on the test flow: `--stage validate` reads the Validate check's terminal (its one check agent) while the
/// ticket is in it, and is refused in any other stage; the plain form starts with a line naming the stage while the
/// check runs.
#[tokio::test(flavor = "multi_thread")]
async fn the_agent_terminal_as_on_screen_and_a_check_s_with_stage() {
    let w = world().await;
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/flows/every-kind.yaml");
    let file = otaq_core::flow::yaml::read(&std::fs::read_to_string(path).unwrap()).unwrap();
    w.board.use_flow_for_tests("SHOP", &file).await;
    let note = ticket(&w, "SHOP-22").await;
    let size = TerminalSize { cols: 60, rows: 12 };
    let session = printing(&w, &note, SessionKind::Agent, PROMPT, size).await;

    let (from_session, text) = as_user(&w, "SHOP-22", None).await.unwrap();
    assert_eq!(from_session, session);
    assert_eq!(
        text,
        "Bash command\n\n  rm -rf .next\n\nDo you want to proceed?\n> 1. Yes\n  2. No<end>"
    );

    // In Dev: `--stage dev` is the agent's terminal, `--stage security` is refused.
    assert_eq!(
        as_user(&w, "SHOP-22", Some("Dev")).await.unwrap().0,
        session
    );
    assert_eq!(
        as_user(&w, "SHOP-22", Some("security")).await.unwrap_err(),
        "SHOP-22 isn't in Security"
    );

    // In Validate, its check running: `--stage validate` (or its name, any case) reads the check's terminal, the
    // plain form starts with a line saying so, and Dev is refused.
    w.force_stage(&note, "validate", Status::Checking).await;
    let check = printing(
        &w,
        &note,
        SessionKind::Validator,
        "\\033[32mrunning the tests\\033[0m\\r\\n",
        size,
    )
    .await;
    for stage in ["validate", "VALIDATE", " Validate "] {
        let (from_session, text) = as_user(&w, "SHOP-22", Some(stage)).await.unwrap();
        assert_eq!(
            (from_session.as_str(), text.as_str()),
            (check.as_str(), "running the tests\n<end>"),
            "{stage}"
        );
    }
    let (_, text) = as_user(&w, "SHOP-22", None).await.unwrap();
    assert!(
        text.starts_with(
            "SHOP-22 is in Validate \u{b7} its terminal: --stage validate\nBash command\n"
        ),
        "{text}"
    );
    assert_eq!(
        as_user(&w, "SHOP-22", Some("dev")).await.unwrap_err(),
        "SHOP-22 isn't in Dev"
    );

    // The check ended: nothing to read with --stage; the plain form loses its line.
    w.sessions.stop(&check).await.unwrap();
    assert_eq!(
        as_user(&w, "SHOP-22", Some("validate")).await.unwrap_err(),
        "SHOP-22's Validate check isn't running"
    );
    let (_, text) = as_user(&w, "SHOP-22", None).await.unwrap();
    assert!(text.starts_with("Bash command\n"), "{text}");

    // A check by you has no agent.
    w.force_stage(&note, "qa", Status::WaitingForYou).await;
    assert_eq!(
        as_user(&w, "SHOP-22", Some("QA")).await.unwrap_err(),
        "SHOP-22's QA is your check: no agent runs it"
    );

    // The agent ended: its last session is still in memory and still readable.
    w.sessions.stop(&session).await.unwrap();
    let (from_session, text) = as_user(&w, "SHOP-22", None).await.unwrap();
    assert_eq!(from_session, session);
    assert!(text.contains("Do you want to proceed?"), "{text}");
    w.stop().await;
}

/// d0130 row 17: Security has two check agents, one terminal each. `--stage security` alone is refused with the count;
/// `--agent` (any case) picks one's running checker, else its last one; the plain form's line names both.
#[tokio::test(flavor = "multi_thread")]
async fn several_check_agents_each_have_their_terminal() {
    let w = world().await;
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/flows/every-kind.yaml");
    let file = otaq_core::flow::yaml::read(&std::fs::read_to_string(path).unwrap()).unwrap();
    w.board.use_flow_for_tests("SHOP", &file).await;
    let note = ticket(&w, "SHOP-22").await;
    let size = TerminalSize { cols: 60, rows: 12 };
    printing(&w, &note, SessionKind::Agent, "the agent\\r\\n", size).await;
    w.force_stage(&note, "security", Status::Checking).await;
    let security = printing(&w, &note, SessionKind::Validator, "no SQL\\r\\n", size).await;
    let validator = printing(&w, &note, SessionKind::Validator, "tests pass\\r\\n", size).await;
    w.validating_with_agents(
        &note,
        &[("Security reviewer", &security), ("Validator", &validator)],
    )
    .await;
    w.force_stage(&note, "security", Status::Checking).await;

    assert_eq!(
        as_user(&w, "SHOP-22", Some("security")).await.unwrap_err(),
        "Security has 2 check agents; add --agent"
    );
    let (from_session, text) = of_agent(&w, "SHOP-22", Some("security"), Some("security REVIEWER"))
        .await
        .unwrap();
    assert_eq!(
        (from_session.as_str(), text.as_str()),
        (security.as_str(), "no SQL\n<end>")
    );
    let (from_session, _) = of_agent(&w, "SHOP-22", Some("Security"), Some("Validator"))
        .await
        .unwrap();
    assert_eq!(from_session, validator);
    assert_eq!(
        of_agent(&w, "SHOP-22", Some("security"), Some("Performance"))
            .await
            .unwrap_err(),
        "Security has no check agent Performance (its check agents: Security reviewer, Validator)"
    );
    let (_, text) = as_user(&w, "SHOP-22", None).await.unwrap();
    assert!(
        text.starts_with(
            "SHOP-22 is in Security \u{b7} its terminals: --stage security --agent \"Security reviewer\", \
             --stage security --agent \"Validator\"\nthe agent"
        ),
        "{text}"
    );
    // One ended (its verdict is in): its last checker is still readable beside the other's.
    w.sessions.stop(&security).await.unwrap();
    let (from_session, text) = of_agent(&w, "SHOP-22", Some("security"), Some("Security reviewer"))
        .await
        .unwrap();
    assert_eq!(
        (from_session.as_str(), text.as_str()),
        (security.as_str(), "no SQL\n<end>")
    );
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn no_session_in_memory_is_a_plain_refusal() {
    let w = world().await;
    assert_eq!(
        as_user(&w, "SHOP-13", None).await.unwrap_err(),
        "SHOP-13 has no terminal output: its agent hasn't started."
    );
    // A session from before a restart: stored, but its output was never written to disk (d0045).
    let interrupted = ticket(&w, "SHOP-25").await;
    w.record_ended_agent(&interrupted, "c-25").await;
    assert_eq!(
        as_user(&w, "SHOP-25", None).await.unwrap_err(),
        "SHOP-25's terminal output isn't kept after a restart."
    );
    w.stop().await;
}

/// CLI-09: the user always; a Project agent on its project's tickets; ticket
/// agents and validators never.
#[tokio::test(flavor = "multi_thread")]
async fn who_may_read_a_ticket_s_terminal() {
    let w = world().await;
    let size = TerminalSize { cols: 80, rows: 24 };
    let other_project = w.add_project("notes-api").await;
    let other = w.create(&other_project.key, "Rate limits").await;
    let other_key = other.key.to_string();
    for key in ["SHOP-19", "SHOP-42"] {
        printing(
            &w,
            &ticket(&w, key).await,
            SessionKind::Agent,
            "Allow rm -rf .next?\\r\\n",
            size,
        )
        .await;
    }
    printing(&w, &other, SessionKind::Agent, "hello\\r\\n", size).await;

    // The user: any ticket of any project.
    for key in ["SHOP-19", "SHOP-42", other_key.as_str()] {
        assert!(as_user(&w, key, None).await.is_ok(), "{key}");
    }

    // A Project agent: any ticket of its project, not another project's.
    let agent = w
        .project_agents
        .open("SHOP", "Look around the checkout code")
        .await
        .unwrap();
    let shell = w.shell_in(&agent.session.as_ref().unwrap().id);
    w.project_agents
        .rename(&agent.id, "Checkout bug")
        .await
        .unwrap();
    assert!(allowed(&from(&w, &shell, "SHOP-19").await).contains("Allow rm -rf .next?"));
    assert!(allowed(&from(&w, &shell, "SHOP-42").await).contains("Allow rm -rf .next?"));
    assert_eq!(
        refused(&from(&w, &shell, &other_key).await),
        "Checkout bug can only read shop-web's tickets"
    );

    // A ticket's agent and a validator: refused, also for their own ticket.
    let shell = w.agent_shell(&ticket(&w, "SHOP-18").await).await;
    for (key, text) in [
        ("SHOP-19", "SHOP-18's agent can't read SHOP-19's terminal"),
        ("SHOP-18", "SHOP-18's agent can't read SHOP-18's terminal"),
    ] {
        assert_eq!(refused(&from(&w, &shell, key).await), text);
    }
    let shell = w
        .shell(&ticket(&w, "SHOP-19").await, SessionKind::Validator)
        .await;
    assert_eq!(
        refused(&from(&w, &shell, "SHOP-19").await),
        "SHOP-19's check can't read SHOP-19's terminal"
    );
    w.stop().await;
}
