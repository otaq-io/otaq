//! A ticket's agents run its app (agents run the app): the ticket's own agent and its check start, read and stop its Run with
//! `run.start`, `run.log` and `run.stop` from inside their sessions; another ticket's agent, a helper and Noyan are
//! refused. The Run records who started it ("started by Dev", on the card too). An agent never stops a Run the user
//! started; Otaq stops an agent's Run when that agent's stage ends (`stage next`) or its verdict lands ("stage
//! ended"), never the user's. The processes of an agent's Run act for that agent, never as the user. Sessions are an
//! ordinary `sh` (requests through the socket from inside them); no agent tool runs; ports come from 26_000 up.

use std::net::{Ipv4Addr, SocketAddrV4, TcpListener};
use std::ops::RangeInclusive;
use std::time::Duration;

use otaq_core::board::settings::ProjectSettings;
use otaq_core::board::tickets::Ticket;
use otaq_core::domain::Status;
use otaq_core::ports::PortPool;
use otaq_core::service::testing::{AgentShell, TestService};
use otaq_core::session::SessionKind;
use otaq_core::terminals::run::{RunInfo, RunState, StartedBy, StopCause};
use serde_json::{Value, json};

/// SHOP without Validate, so `stage next` goes to Review.
async fn world() -> TestService {
    let w = with_validate().await;
    w.board
        .edit_flow_for_tests("SHOP", otaq_core::flow::default::testing::without_validate)
        .await;
    w
}

/// SHOP on the default flow, with Validate.
async fn with_validate() -> TestService {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    w
}

async fn set_run_command(w: &TestService, command: &str) {
    let settings = ProjectSettings {
        apps: otaq_core::board::settings::RunApp::only(command, ""),
        ..Default::default()
    };
    w.board
        .set_project_settings("SHOP", &settings)
        .await
        .unwrap();
}

/// A ticket working in Dev with a worktree and its two ports from `range`, and its agent's session (`sh`). Returns
/// the ticket, its Run port and the session.
async fn in_dev(
    w: &TestService,
    title: &str,
    range: RangeInclusive<u16>,
) -> (Ticket, u16, AgentShell) {
    let t = w.create("SHOP", title).await;
    w.give_worktree(&t).await;
    let ports = PortPool::with_range(w.board.store().clone(), range)
        .allocate(&t.id, &[otaq_core::board::settings::DEFAULT_APP.to_owned()])
        .await
        .unwrap();
    w.force_status(&t, Status::Working).await;
    let dev = w.agent_shell(&t).await;
    w.run_as_agent(&dev, "rust-dev", "Rust developer").await;
    let t = w.board.ticket(&t.key.to_string()).await.unwrap();
    (t, ports.apps[0].1, dev)
}

/// One raw request from inside a session (no `'` in it: it goes through `sh`).
async fn send(w: &TestService, shell: &AgentShell, method: &str, params: Value) -> Value {
    let request = json!({"protocol_version": otaq_core::protocol::PROTOCOL_VERSION, "id": 1, "request": {"method": method, "params": params}});
    let ran = shell
        .send_raw(&w.data.socket_path(), &request.to_string())
        .await;
    serde_json::from_str(ran.stdout.lines().next().unwrap_or_default())
        .unwrap_or_else(|e| panic!("{e}: {ran:?}"))
}

fn refused(answer: &Value) -> (&str, &str) {
    let error = &answer["error"];
    assert_eq!(error["kind"], "refused", "{answer}");
    (
        error["data"]["refusal"].as_str().unwrap_or_default(),
        error["message"].as_str().unwrap(),
    )
}

fn run_of(answer: &Value) -> RunInfo {
    serde_json::from_value(answer["reply"]["run"].clone())
        .unwrap_or_else(|e| panic!("{e}: {answer}"))
}

/// Waits until the ticket's Run is in `state` (10 s at most).
async fn wait_state(w: &TestService, key: &str, state: RunState) -> RunInfo {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let info = w.runs.info(key, None).await.unwrap();
            if info.state == state {
                return info;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("{key}'s Run never became {state:?}"))
}

fn dev() -> StartedBy {
    StartedBy::Agent {
        stage: "Dev".into(),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn an_agent_starts_reads_and_stops_its_tickets_run() {
    let w = world().await;
    let (t, port, agent) = in_dev(&w, "Price table", 26_000..=26_099).await;
    set_run_command(&w, "echo serving on $PORT; sleep 30").await;
    let key = t.key.to_string();

    let started = run_of(&send(&w, &agent, "run.start", json!({"key": key})).await);
    assert_eq!(started.state, RunState::Starting);
    assert_eq!(started.started_by, Some(dev()), "started-by is recorded");
    let listener = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port)).unwrap();
    let running = wait_state(&w, &key, RunState::Running).await;
    assert_eq!(
        running.status_text(),
        format!("running :{port} (started by Dev)")
    );

    // The card says who started it.
    let flags = w.board.ticket(&key).await.unwrap().flags;
    assert_eq!(flags.run_apps.first().map(|a| a.port), Some(port));
    assert_eq!(
        flags.run_apps.first().and_then(|a| a.started_by.as_deref()),
        Some("Dev")
    );

    // Its output, from memory.
    let log = send(&w, &agent, "run.log", json!({"key": key, "tail": 5})).await;
    let text = log["reply"]["run_log"]["text"].as_str().unwrap_or_default();
    assert!(text.contains(&format!("serving on {port}")), "{log}");

    let stopped = run_of(&send(&w, &agent, "run.stop", json!({"key": key})).await);
    assert_eq!(stopped.state, RunState::Stopped);
    assert_eq!(
        stopped.stopped_by,
        Some(StopCause::Agent {
            stage: "Dev".into()
        })
    );
    assert_eq!(stopped.status_text(), "stopped (by Dev)");
    assert_eq!(
        w.board
            .ticket(&key)
            .await
            .unwrap()
            .flags
            .run_apps
            .first()
            .map(|a| a.port),
        None
    );
    drop(listener);
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn only_the_tickets_own_agent_and_check_run_it() {
    let w = world().await;
    let (mine, _, _) = in_dev(&w, "Price table", 26_100..=26_199).await;
    let (_, _, other) = in_dev(&w, "Search", 26_200..=26_299).await;
    set_run_command(&w, "sleep 30").await;
    let key = mine.key.to_string();

    for method in ["run.start", "run.stop", "run.log"] {
        let answer = send(&w, &other, method, json!({"key": key})).await;
        assert_eq!(
            refused(&answer),
            ("not_allowed", "SHOP-2's agent can't run SHOP-1's app"),
            "{method}"
        );
    }
    assert_eq!(
        w.runs.info(&key, None).await.unwrap().state,
        RunState::NotStarted
    );

    // A helper of the ticket gets no Run: the agent in charge runs the app after merging.
    let helper = w.shell(&mine, SessionKind::Helper).await;
    let answer = send(&w, &helper, "run.start", json!({"key": key})).await;
    assert_eq!(
        refused(&answer),
        ("not_allowed", otaq_core::policy::HELPER_NO_RUN)
    );

    // Its check starts one too.
    let check = w.shell(&mine, SessionKind::Validator).await;
    let started = run_of(&send(&w, &check, "run.start", json!({"key": key})).await);
    assert_eq!(started.state, RunState::Starting);
    assert!(
        matches!(started.started_by, Some(StartedBy::Agent { .. })),
        "{:?}",
        started.started_by
    );
    w.runs.stop(&key, None).await.unwrap();
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_agent_uses_the_users_run_and_never_stops_it() {
    let w = world().await;
    let (t, _, agent) = in_dev(&w, "Price table", 26_300..=26_399).await;
    set_run_command(&w, "sleep 30").await;
    let key = t.key.to_string();
    let mine = w.runs.start(&key, None).await.unwrap();
    assert_eq!(mine.started_by, Some(StartedBy::You));

    // Starting it again uses the user's Run.
    let used = run_of(&send(&w, &agent, "run.start", json!({"key": key})).await);
    assert_eq!(used.session, mine.session);
    assert_eq!(used.started_by, Some(StartedBy::You));

    let answer = send(&w, &agent, "run.stop", json!({"key": key})).await;
    assert_eq!(
        refused(&answer),
        (
            "not_allowed",
            "SHOP-1's Run was started by the user, so it stays running: an agent stops only a Run an agent \
             started. Use it as it is."
        )
    );
    assert!(w.runs.info(&key, None).await.unwrap().state.is_live());
    w.runs.stop(&key, None).await.unwrap();
    w.stop().await;
}

/// The backstop: `stage next` stops the Run the agent started, never one the user started.
#[tokio::test(flavor = "multi_thread")]
async fn stage_next_stops_the_agents_run_and_never_the_users() {
    let w = world().await;
    let (t, _, agent) = in_dev(&w, "Price table", 26_400..=26_499).await;
    let (users, _, other) = in_dev(&w, "Search", 26_500..=26_599).await;
    set_run_command(&w, "sleep 30").await;
    let key = t.key.to_string();
    run_of(&send(&w, &agent, "run.start", json!({"key": key})).await);
    let user_key = users.key.to_string();
    w.runs.start(&user_key, None).await.unwrap();

    for (shell, key) in [(&agent, &key), (&other, &user_key)] {
        send(&w, shell, "ticket.prime", json!({"key": key})).await;
        let ended = send(
            &w,
            shell,
            "ticket.stage_next",
            json!({"key": key, "summary": "Done."}),
        )
        .await;
        assert!(ended["reply"].is_object(), "{ended}");
    }

    let info = wait_state(&w, &key, RunState::Stopped).await;
    assert_eq!(
        info.stopped_by,
        Some(StopCause::StageEnded {
            stage: "Dev".into()
        })
    );
    assert_eq!(info.status_text(), "stopped (Dev's stage ended)");
    let user = w.runs.info(&user_key, None).await.unwrap();
    assert!(user.state.is_live(), "{user:?}");
    w.runs.stop(&user_key, None).await.unwrap();
    w.stop().await;
}

/// A verdict stops the Run its check started.
#[tokio::test(flavor = "multi_thread")]
async fn a_verdict_stops_the_run_its_check_started() {
    let w = with_validate().await;
    let (t, _, _) = in_dev(&w, "Price table", 26_600..=26_699).await;
    set_run_command(&w, "sleep 30").await;
    let key = t.key.to_string();
    let check = w.shell(&t, SessionKind::Validator).await;
    w.run_as_agent(&check, "validator", "Validator").await;
    w.validating_with(&t, &check.session).await;
    run_of(&send(&w, &check, "run.start", json!({"key": key})).await);
    assert!(w.runs.info(&key, None).await.unwrap().state.is_live());

    send(&w, &check, "ticket.prime_validate", json!({"key": key})).await;
    let verdict = send(
        &w,
        &check,
        "validation.verdict",
        json!({"key": key, "pass": false, "report": "Fail: the price table is empty."}),
    )
    .await;
    assert!(verdict["reply"]["ticket"].is_object(), "{verdict}");
    let info = wait_state(&w, &key, RunState::Stopped).await;
    assert!(
        matches!(info.stopped_by, Some(StopCause::StageEnded { .. })),
        "{:?}",
        info.stopped_by
    );
    w.stop().await;
}

/// The processes of an agent's Run are that agent's callers: a user-only request from inside it is refused.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn an_agents_run_acts_for_the_agent_never_the_user() {
    let w = world().await;
    let (t, _, agent) = in_dev(&w, "Price table", 26_700..=26_799).await;
    let key = t.key.to_string();
    let request = json!({"protocol_version": otaq_core::protocol::PROTOCOL_VERSION, "id": 1, "request": {"method": "ticket.pin", "params": {"key": key}}});
    set_run_command(
        &w,
        &format!(
            "printf '%s\\n' '{request}' | nc -U -w 5 '{}'; sleep 30",
            w.data.socket_path().display()
        ),
    )
    .await;
    run_of(&send(&w, &agent, "run.start", json!({"key": key})).await);
    let info = w.runs.info(&key, None).await.unwrap();
    let session = info.session.unwrap();
    let out = w.wait_for_output(&session, "\"error\"").await;
    assert!(
        out.contains("SHOP-1's agent can't pin tickets; ask the user"),
        "{out}"
    );
    assert!(!w.board.ticket(&key).await.unwrap().flags.is_pinned());

    // The user's own Run stays the user's.
    w.runs.stop(&key, None).await.unwrap();
    w.runs.start(&key, None).await.unwrap();
    let session = w.runs.info(&key, None).await.unwrap().session.unwrap();
    let out = w.wait_for_output(&session, "\"reply\"").await;
    assert!(!out.contains("\"error\""), "{out}");

    // The agent restarts the user's Run to run its new code (`run restart`, agents run the app, T4): a new session, still the
    // user's (the agent can't stop it, its stage end leaves it up), but its processes act for the agent.
    let restarted = run_of(&send(&w, &agent, "run.restart", json!({"key": key})).await);
    let again = restarted.session.clone().unwrap();
    assert_ne!(again, session);
    assert_eq!(restarted.started_by, Some(StartedBy::You));
    let out = w.wait_for_output(&again, "\"error\"").await;
    assert!(
        out.contains("SHOP-1's agent can't pin tickets; ask the user"),
        "{out}"
    );
    let refused_stop = send(&w, &agent, "run.stop", json!({"key": key})).await;
    assert_eq!(refused_stop["error"]["kind"], "refused", "{refused_stop}");
    assert!(w.runs.info(&key, None).await.unwrap().state.is_live());
    w.runs.stop(&key, None).await.unwrap();
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn log_before_any_run_and_no_command_for_an_agent() {
    let w = world().await;
    let (t, _, agent) = in_dev(&w, "Price table", 26_800..=26_899).await;
    let key = t.key.to_string();
    let answer = send(&w, &agent, "run.log", json!({"key": key})).await;
    assert_eq!(
        refused(&answer),
        (
            "session_ended",
            "SHOP-1's Run hasn't run yet, so there is no output."
        )
    );
    let answer = send(&w, &agent, "run.start", json!({"key": key})).await;
    assert_eq!(
        refused(&answer),
        (
            "no_run_command",
            "shop-web has no run command yet: if you know how this project starts, run `otaq run set --command …`"
        )
    );
    w.stop().await;
}
