//! `otaq run start|status|log|stop` (agents run the app) from a ticket's agent session (`$OTAQ_TICKET` names the ticket):
//! `start` returns at once, `start --wait` once the Run is running (its URL or port) or with a non-zero exit and the
//! last lines when it failed to start or is still starting after `--timeout`; a held port and a missing run command
//! are errors; `status` says who started it; `log --tail N` prints the kept output; `stop` refuses the user's Run.
//! Ordinary shell commands stand in for a dev server; a listener in the test stands in for its port answering. Ports
//! come from 26_900 up; the data folder is temporary.

use std::net::{Ipv4Addr, SocketAddrV4, TcpListener};
use std::ops::RangeInclusive;
use std::time::Duration;

use otaq_core::board::settings::ProjectSettings;
use otaq_core::board::tickets::Ticket;
use otaq_core::domain::Status;
use otaq_core::ports::PortPool;
use otaq_core::service::testing::{AgentShell, TestService};
use otaq_core::terminals::run::RunState;

const CB: &str = env!("CARGO_BIN_EXE_otaq");

fn on_platform<'a>(unix: &'a str, windows: &'a str) -> &'a str {
    if cfg!(windows) { windows } else { unix }
}

async fn set_run_command(w: &TestService, command: &str, open_url: &str) {
    let settings = ProjectSettings {
        run_command: command.into(),
        open_url: open_url.into(),
        ..Default::default()
    };
    w.board
        .set_project_settings("SHOP", &settings)
        .await
        .unwrap();
}

/// SHOP-1 at work in Dev with a worktree and its ports from `range`, and its agent's shell. Returns the Run port.
async fn in_dev(w: &TestService, range: RangeInclusive<u16>) -> (Ticket, u16, AgentShell) {
    w.add_project("shop-web").await;
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

/// Binds the Run port once the Run is starting, as a dev server coming up would.
fn answer_once_starting(
    w: &TestService,
    key: &str,
    port: u16,
) -> tokio::task::JoinHandle<TcpListener> {
    let runs = w.runs.clone();
    let key = key.to_owned();
    tokio::spawn(async move {
        loop {
            if runs.info(&key).await.unwrap().state == RunState::Starting {
                return TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port)).unwrap();
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn start_wait_returns_once_running_with_the_url() {
    let w = TestService::start().await;
    let (_, port, agent) = in_dev(&w, 26_900..=26_919).await;
    set_run_command(&w, "echo up on $PORT; sleep 30", "http://localhost:$PORT/").await;
    let listener = answer_once_starting(&w, "SHOP-1", port);
    let ran = agent.run(&format!("'{CB}' run start --wait")).await;
    assert_eq!(ran.status, 0, "{ran:?}");
    assert_eq!(
        ran.stdout.trim(),
        format!("SHOP-1's Run: running :{port} (started by Dev)\nhttp://localhost:{port}/")
    );

    let ran = agent.run(&format!("'{CB}' run status")).await;
    assert_eq!(
        ran.stdout.trim(),
        format!("SHOP-1's Run: running :{port} (started by Dev)")
    );
    let ran = agent.run(&format!("'{CB}' run log --tail 3")).await;
    assert_eq!(ran.status, 0, "{ran:?}");
    assert!(ran.stdout.contains(&format!("up on {port}")), "{ran:?}");
    let ran = agent.run(&format!("'{CB}' run status --json")).await;
    let json: serde_json::Value = serde_json::from_str(&ran.stdout).unwrap();
    assert_eq!(
        json["started_by"],
        serde_json::json!({"by": "agent", "stage": "Dev"})
    );

    let ran = agent.run(&format!("'{CB}' run stop")).await;
    assert_eq!(ran.status, 0, "{ran:?}");
    assert_eq!(ran.stdout.trim(), "SHOP-1's Run: stopped (by Dev)");
    drop(listener.await.unwrap());
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn start_without_wait_returns_at_once_and_wait_fails_on_a_failed_start() {
    let w = TestService::start().await;
    let (_, port, agent) = in_dev(&w, 26_920..=26_939).await;
    set_run_command(&w, "sleep 30", "").await;
    let ran = agent.run(&format!("'{CB}' run start")).await;
    assert_eq!(ran.status, 0, "{ran:?}");
    assert_eq!(
        ran.stdout.trim(),
        format!("SHOP-1's Run: starting :{port} (started by Dev)")
    );
    // Still starting after the timeout: an error with what it printed; it keeps running.
    let ran = agent
        .run(&format!("'{CB}' run start --wait --timeout 1"))
        .await;
    assert_ne!(ran.status, 0, "{ran:?}");
    assert!(
        ran.stderr
            .contains("SHOP-1's Run is still starting after 1 s"),
        "{ran:?}"
    );
    agent.run(&format!("'{CB}' run stop")).await;

    set_run_command(
        &w,
        on_platform(
            "echo 'Error: Cannot find module vite' >&2; exit 3",
            "echo Error: Cannot find module vite 1>&2& exit 3",
        ),
        "",
    )
    .await;
    let ran = agent.run(&format!("'{CB}' run start --wait")).await;
    assert_ne!(ran.status, 0, "{ran:?}");
    assert!(
        ran.stderr.contains("SHOP-1's Run failed to start (exit 3)"),
        "{ran:?}"
    );
    assert!(ran.stderr.contains("Its last lines:"), "{ran:?}");
    assert!(ran.stderr.contains("Cannot find module vite"), "{ran:?}");
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_held_port_and_no_run_command_are_errors() {
    let w = TestService::start().await;
    let (_, port, agent) = in_dev(&w, 26_940..=26_959).await;
    let ran = agent.run(&format!("'{CB}' run start --wait")).await;
    assert_ne!(ran.status, 0);
    assert!(
        ran.stderr
            .contains("shop-web has no run command yet: if you know how this project starts, run `otaq run set"),
        "{ran:?}"
    );

    set_run_command(&w, "sleep 30", "").await;
    let held = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port)).unwrap();
    let ran = agent.run(&format!("'{CB}' run start")).await;
    assert_ne!(ran.status, 0, "{ran:?}");
    assert!(
        ran.stderr
            .contains(&format!("SHOP-1's Run port {port} is in use")),
        "{ran:?}"
    );
    drop(held);
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn stop_refuses_the_users_run_and_log_before_a_run_is_an_error() {
    let w = TestService::start().await;
    let (_, _, agent) = in_dev(&w, 26_960..=26_979).await;
    set_run_command(&w, "sleep 30", "").await;
    let ran = agent.run(&format!("'{CB}' run log")).await;
    assert_ne!(ran.status, 0);
    assert!(
        ran.stderr.contains("SHOP-1's Run hasn't run yet"),
        "{ran:?}"
    );
    let ran = agent.run(&format!("'{CB}' run status")).await;
    assert_eq!(ran.stdout.trim(), "SHOP-1's Run: not started");

    w.runs.start("SHOP-1").await.unwrap();
    let ran = agent.run(&format!("'{CB}' run stop")).await;
    assert_ne!(ran.status, 0, "{ran:?}");
    assert!(
        ran.stderr.contains("SHOP-1's Run was started by the user"),
        "{ran:?}"
    );
    assert!(w.runs.info("SHOP-1").await.unwrap().state.is_live());
    w.runs.stop("SHOP-1").await.unwrap();
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn set_saves_the_command_once_it_runs_and_nothing_when_it_fails() {
    let w = TestService::start().await;
    let (_, port, agent) = in_dev(&w, 26_980..=26_999).await;
    let ran = agent
        .run(&format!(
            "'{CB}' run set --command {} --timeout 2",
            on_platform("'exit 4'", "\"exit 4\"")
        ))
        .await;
    assert_ne!(ran.status, 0, "{ran:?}");
    assert!(
        ran.stderr
            .contains("Nothing saved: SHOP-1's Run failed to start (exit 4)"),
        "{ran:?}"
    );
    assert_eq!(
        w.board.project_settings("SHOP").await.unwrap().run_command,
        ""
    );

    let listener = answer_once_starting(&w, "SHOP-1", port);
    let ran = agent
        .run(&format!(
            "'{CB}' run set --command 'sleep 30' --open-url 'http://localhost:$PORT/'"
        ))
        .await;
    assert_eq!(ran.status, 0, "{ran:?}");
    assert_eq!(
        ran.stdout.trim(),
        format!(
            "Saved as shop-web's run command (Set by Dev on SHOP-1).\nSHOP-1's Run: running :{port} (started by \
             Dev)\nhttp://localhost:{port}/"
        )
    );
    let ran = agent
        .run(&format!("'{CB}' run set --command 'pnpm dev'"))
        .await;
    assert_ne!(ran.status, 0);
    assert!(
        ran.stderr
            .contains("shop-web already has a run command (`sleep 30`)"),
        "{ran:?}"
    );
    agent.run(&format!("'{CB}' run stop")).await;
    drop(listener.await.unwrap());
    w.stop().await;
}
