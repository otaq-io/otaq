//! Agents add a missing run command (agents run the app, RUN-02's exception): `run.set` from a ticket's agent runs the command
//! as the ticket's Run and saves it as the project's only once that Run is running ("Set by Dev on SHOP-1"); a failed
//! start saves nothing; a project that has a run command refuses it; Undo (the user's) empties it; the user's hand
//! edit of the command drops the line. Sessions are an ordinary `sh`; ports come from 27_000 up.

use std::net::{Ipv4Addr, SocketAddrV4, TcpListener};
use std::ops::RangeInclusive;
use std::time::Duration;

use otaq_core::board::settings::ProjectSettings;
use otaq_core::board::tickets::Ticket;
use otaq_core::domain::Status;
use otaq_core::ports::PortPool;
use otaq_core::service::testing::{AgentShell, TestService};
use otaq_core::terminals::run::{RunInfo, RunState};
use serde_json::{Value, json};

fn on_platform<'a>(unix: &'a str, windows: &'a str) -> &'a str {
    if cfg!(windows) { windows } else { unix }
}

async fn world() -> TestService {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    w.board
        .edit_flow_for_tests("SHOP", otaq_core::flow::default::testing::without_validate)
        .await;
    w
}

/// SHOP-1 working in Dev with a worktree and its ports from `range`, and its agent's shell; returns the Run port.
async fn in_dev(w: &TestService, range: RangeInclusive<u16>) -> (Ticket, u16, AgentShell) {
    let t = w.create("SHOP", "Price table").await;
    w.give_worktree(&t).await;
    let ports = PortPool::with_range(w.board.store().clone(), range)
        .allocate(&t.id)
        .await
        .unwrap();
    w.force_status(&t, Status::Working).await;
    let agent = w.agent_shell(&t).await;
    (t, ports.run, agent)
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

async fn wait_state(w: &TestService, key: &str, state: RunState) -> RunInfo {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let info = w.runs.info(key).await.unwrap();
            if info.state == state {
                return info;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("{key}'s Run never became {state:?}"))
}

async fn settings(w: &TestService) -> ProjectSettings {
    w.board.project_settings("SHOP").await.unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn the_command_is_saved_only_once_its_run_is_running() {
    let w = world().await;
    let (_, port, agent) = in_dev(&w, 27_000..=27_019).await;
    let set = send(
        &w,
        &agent,
        "run.set",
        json!({"key": "SHOP-1", "command": "sleep 30", "open_url": "http://localhost:$PORT/"}),
    )
    .await;
    let info: RunInfo = serde_json::from_value(set["reply"]["run"].clone()).unwrap();
    assert_eq!(info.state, RunState::Starting);
    assert_eq!(info.command, "sleep 30");
    // Starting: nothing saved yet.
    assert_eq!(settings(&w).await.run_command, "");

    let listener = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port)).unwrap();
    let running = wait_state(&w, "SHOP-1", RunState::Running).await;
    let s = settings(&w).await;
    assert_eq!(s.run_command, "sleep 30");
    assert_eq!(s.open_url, "http://localhost:$PORT/");
    let by = s.run_command_set_by.expect("set by the agent");
    assert_eq!(by.line(), "Set by Dev on SHOP-1");
    assert!(by.open_url);
    assert_eq!(
        running.command_set_by.map(|b| b.line()).as_deref(),
        Some("Set by Dev on SHOP-1")
    );
    w.runs.stop("SHOP-1").await.unwrap();
    drop(listener);

    // Now the project has one: an agent never changes it.
    let again = send(
        &w,
        &agent,
        "run.set",
        json!({"key": "SHOP-1", "command": "pnpm dev"}),
    )
    .await;
    assert_eq!(
        refused(&again),
        "shop-web already has a run command (`sleep 30`): an agent never changes one. Start it with `otaq run \
         start --wait`"
    );
    assert_eq!(settings(&w).await.run_command, "sleep 30");
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failed_start_saves_nothing_and_a_users_command_is_never_changed() {
    let w = world().await;
    let (_, _, agent) = in_dev(&w, 27_020..=27_039).await;
    send(
        &w,
        &agent,
        "run.set",
        json!({"key": "SHOP-1", "command": on_platform("echo no such script >&2; exit 2", "echo no such script 1>&2& exit 2")}),
    )
    .await;
    wait_state(&w, "SHOP-1", RunState::FailedToStart).await;
    let s = settings(&w).await;
    assert_eq!((s.run_command.as_str(), s.run_command_set_by), ("", None));

    // The user's own command: refused, unchanged.
    let mine = ProjectSettings {
        run_command: "pnpm dev".into(),
        ..settings(&w).await
    };
    w.board.save_project_settings("SHOP", &mine).await.unwrap();
    let answer = send(
        &w,
        &agent,
        "run.set",
        json!({"key": "SHOP-1", "command": "npm start"}),
    )
    .await;
    assert!(
        refused(&answer).starts_with("shop-web already has a run command (`pnpm dev`)"),
        "{answer}"
    );
    assert_eq!(settings(&w).await.run_command, "pnpm dev");
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn undo_empties_it_and_a_hand_edit_drops_the_line() {
    let w = world().await;
    let (_, port, agent) = in_dev(&w, 27_040..=27_059).await;
    let set_and_run = || async {
        send(
            &w,
            &agent,
            "run.set",
            json!({"key": "SHOP-1", "command": "sleep 30", "open_url": "http://localhost:$PORT/"}),
        )
        .await;
        let listener = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port)).unwrap();
        wait_state(&w, "SHOP-1", RunState::Running).await;
        w.runs.stop("SHOP-1").await.unwrap();
        drop(listener);
    };
    set_and_run().await;

    // Undo is the user's.
    let answer = send(&w, &agent, "run.command_undo", json!({"project": "SHOP"})).await;
    assert_eq!(
        refused(&answer),
        "SHOP-1's agent can't undo a run command; ask the user"
    );
    w.board.undo_agent_run_command("SHOP").await.unwrap();
    let s = settings(&w).await;
    assert_eq!(
        (
            s.run_command.as_str(),
            s.open_url.as_str(),
            s.run_command_set_by
        ),
        ("", "", None)
    );
    assert!(w.board.undo_agent_run_command("SHOP").await.is_err());

    // Saving the page unchanged keeps the line; editing the command by hand drops it.
    set_and_run().await;
    let same = settings(&w).await;
    w.board.save_project_settings("SHOP", &same).await.unwrap();
    assert!(settings(&w).await.run_command_set_by.is_some());
    let edited = ProjectSettings {
        run_command: "sleep 60".into(),
        ..settings(&w).await
    };
    w.board
        .save_project_settings("SHOP", &edited)
        .await
        .unwrap();
    let s = settings(&w).await;
    assert_eq!(
        (s.run_command.as_str(), s.run_command_set_by),
        ("sleep 60", None)
    );
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_agent_is_told_how_to_add_a_missing_command() {
    let w = world().await;
    let (_, _, agent) = in_dev(&w, 27_060..=27_079).await;
    let answer = send(&w, &agent, "run.start", json!({"key": "SHOP-1"})).await;
    assert_eq!(
        refused(&answer),
        "shop-web has no run command yet: if you know how this project starts, run `otaq run set --command …`"
    );
    // The user's Run still asks in the app (RUN-02, unchanged).
    let err = w.runs.start("SHOP-1").await.unwrap_err();
    assert_eq!(err.to_string(), "shop-web has no run command yet");
    w.stop().await;
}
