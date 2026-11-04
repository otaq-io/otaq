//! Run (T-0133, RUN-01): the project's run command in the ticket's worktree with `$PORT` = the Run port, only on
//! click; starting → running once the port answers; Stop ends the whole process group; an early exit is "failed to
//! start"; a port held by another program is "port in use"; Done and Close stop it with a toast; a restart leaves
//! "stopped by restart". Ordinary shell commands stand in for a project's dev server: `sh` syntax on macOS and
//! Linux, `cmd` syntax on Windows (where Run uses `cmd`, Q33; Git's `sleep` is on PATH in the checks). A Run of a
//! ticket whose build was freed at Review brings it back, and its end frees it again (T-0309, M19-c).

use crate::common::freed;

use std::net::{Ipv4Addr, SocketAddrV4, TcpListener};
use std::ops::RangeInclusive;
use std::time::Duration;

use otaq_core::board::Refusal;
use otaq_core::board::settings::ProjectSettings;
use otaq_core::board::tickets::Ticket;
use otaq_core::domain::{Actor, BuildState, Status};
use otaq_core::git::testing::commit;
use otaq_core::ports::PortPool;
use otaq_core::protocol::Event;
use otaq_core::service::testing::TestService;
use otaq_core::terminals::run::{RunInfo, RunState, StopCause};
use otaq_core::ticket_env::free::FreeOutcome;

/// The run command for this platform.
fn on_platform<'a>(unix: &'a str, windows: &'a str) -> &'a str {
    if cfg!(windows) { windows } else { unix }
}

/// The output without terminal control sequences (a Windows console starts with a few).
fn visible(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\x1b' {
            out.push(c);
            continue;
        }
        match chars.next() {
            // CSI: parameters up to a final letter (or `~`).
            Some('[') => {
                for c in chars.by_ref() {
                    if c.is_ascii_alphabetic() || c == '~' {
                        break;
                    }
                }
            }
            // OSC: up to BEL or ST.
            Some(']') => {
                while let Some(c) = chars.next() {
                    if c == '\x07' || (c == '\x1b' && chars.next_if_eq(&'\\').is_some()) {
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    out
}

fn me() -> Actor {
    Actor::User("me".into())
}

async fn set_run_command(w: &TestService, command: &str) {
    let settings = ProjectSettings {
        apps: otaq_core::board::settings::RunApp::only(command, "http://localhost:$PORT/swagger"),
        ..Default::default()
    };
    w.board
        .set_project_settings("SHOP", &settings)
        .await
        .unwrap();
}

/// A started ticket: a worktree and its two ports from `range`.
async fn started(w: &TestService, title: &str, range: RangeInclusive<u16>) -> (Ticket, u16) {
    let t = w.create("SHOP", title).await;
    w.start_worktree(&t).await;
    let ports = PortPool::with_range(w.board.store().clone(), range)
        .allocate(&t.id, &[otaq_core::board::settings::DEFAULT_APP.to_owned()])
        .await
        .unwrap();
    w.force_status(&t, Status::Working).await;
    (
        w.board.ticket(&t.key.to_string()).await.unwrap(),
        ports.apps[0].1,
    )
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

fn output(w: &TestService, session: &str) -> String {
    String::from_utf8_lossy(&w.sessions.attach(session).unwrap().scrollback).into_owned()
}

fn alive(pid: i32) -> bool {
    u32::try_from(pid).is_ok_and(otaq_core::process::alive)
}

#[tokio::test(flavor = "multi_thread")]
async fn refused_without_a_run_command_or_a_worktree() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let (t, _) = started(&w, "Price table", 23_000..=23_099).await;
    let err = w.runs.start(&t.key.to_string(), None).await.unwrap_err();
    assert_eq!(err.refusal(), Some(Refusal::NoRunCommand));
    assert_eq!(err.to_string(), "shop-web has no run command yet");

    set_run_command(&w, "sleep 30").await;
    let backlog = w.create("SHOP", "Not started").await;
    let err = w
        .runs
        .start(&backlog.key.to_string(), None)
        .await
        .unwrap_err();
    assert_eq!(err.refusal(), Some(Refusal::NoWorktree));
    let info = w.runs.info("SHOP-2", None).await.unwrap();
    assert_eq!(info.state, RunState::NotStarted);
    assert_eq!(info.command, "sleep 30");
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn runs_in_the_worktree_with_the_run_port_and_stop_ends_the_process_group() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let (t, port) = started(&w, "Price table", 23_100..=23_199).await;
    let worktree = t.work.as_ref().unwrap().worktree.clone().unwrap();
    set_run_command(
        &w,
        on_platform(
            "echo ready; echo \"port=$PORT ticket=$OTAQ_TICKET\"; pwd; sleep 30 & echo $! > child.pid; wait",
            "echo ready& echo port=%PORT% ticket=%OTAQ_TICKET%& cd& powershell -NoProfile -Command \
             \"$p = Start-Process sleep -ArgumentList 30 -PassThru -NoNewWindow; Set-Content child.pid $p.Id; \
             $p.WaitForExit()\"",
        ),
    )
    .await;
    let key = t.key.to_string();

    let info = w.runs.start(&key, None).await.unwrap();
    assert_eq!(info.state, RunState::Starting, "nothing listens yet");
    assert_eq!(info.port, Some(port));
    assert_eq!(
        info.url().as_deref(),
        Some(format!("http://localhost:{port}/swagger").as_str())
    );
    let session = info.session.clone().unwrap();
    // The first line shows the command (which itself contains "ticket="), so wait for the command's own output.
    let text = visible(
        &w.wait_for_output(&session, &format!("port={port} ticket={key}"))
            .await,
    );
    assert!(
        text.starts_with("$ echo ready"),
        "first line shows the command: {text:?}"
    );
    assert!(text.contains("ready"));
    assert!(
        text.contains(&format!("port={port} ticket={key}")),
        "{text}"
    );
    if cfg!(windows) {
        // `cd` prints the folder as the program got it (with short 8.3 names from %TEMP%, unlike canonical).
        w.wait_for_output(&session, &worktree).await;
    } else {
        let canonical = otaq_core::paths::canonical(&worktree).unwrap();
        w.wait_for_output(&session, canonical.to_str().unwrap())
            .await;
    }
    // The card shows ▶ :port while it runs.
    assert_eq!(
        w.board
            .ticket(&key)
            .await
            .unwrap()
            .flags
            .run_apps
            .first()
            .map(|a| a.port),
        Some(port)
    );
    // Starting it again leaves it alone.
    assert_eq!(
        w.runs.start(&key, None).await.unwrap().session,
        Some(session.clone())
    );
    assert_eq!(w.runs.running().await.len(), 1);

    let pid_file = std::path::Path::new(&worktree).join("child.pid");
    let child: i32 = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Ok(text) = std::fs::read_to_string(&pid_file)
                && let Ok(pid) = text.trim().parse()
            {
                return pid;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    assert!(alive(child));

    let info = w.runs.stop(&key, None).await.unwrap();
    assert_eq!(info.state, RunState::Stopped);
    assert_eq!(info.stopped_by, Some(StopCause::You));
    assert!(output(&w, &session).contains("] stopped by you"));
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(!alive(child), "the whole process group stopped");
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
    assert!(w.runs.running().await.is_empty());
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn running_once_the_port_answers() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let (t, port) = started(&w, "Price table", 23_200..=23_299).await;
    set_run_command(&w, "echo listening soon; sleep 30").await;
    let key = t.key.to_string();
    let info = w.runs.start(&key, None).await.unwrap();
    assert_eq!(info.state, RunState::Starting);
    // Something now listens on the Run port (the dev server came up).
    let listener = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port)).unwrap();
    let info = wait_state(&w, &key, RunState::Running).await;
    assert_eq!(info.port, Some(port));
    drop(listener);
    w.runs.stop(&key, None).await.unwrap();
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_early_exit_is_failed_to_start_with_the_log_kept() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let (t, _) = started(&w, "Price table", 23_300..=23_399).await;
    set_run_command(
        &w,
        on_platform(
            "echo 'ERR_PNPM_NO_SCRIPT  Missing script: dev' >&2; exit 1",
            "echo ERR_PNPM_NO_SCRIPT  Missing script: dev 1>&2& exit 1",
        ),
    )
    .await;
    let key = t.key.to_string();
    let info = w.runs.start(&key, None).await.unwrap();
    let session = info.session.unwrap();
    let info = wait_state(&w, &key, RunState::FailedToStart).await;
    assert_eq!(info.exit_code, Some(1));
    assert_eq!(info.stopped_by, None);
    let text = output(&w, &session);
    assert!(text.contains("Missing script: dev"), "{text}");
    assert!(text.contains("] exited with code 1"), "{text}");
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

    // A program that ran and finished cleanly is just stopped.
    set_run_command(&w, "echo done").await;
    w.runs.start(&key, None).await.unwrap();
    let info = wait_state(&w, &key, RunState::Stopped).await;
    assert_eq!(info.exit_code, Some(0));
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_port_held_by_another_program_is_port_in_use() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let (t, port) = started(&w, "Price table", 23_400..=23_499).await;
    set_run_command(&w, "sleep 30").await;
    let held = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port)).unwrap();
    let info = w.runs.start(&t.key.to_string(), None).await.unwrap();
    assert_eq!(info.state, RunState::PortInUse);
    assert_eq!(info.port, Some(port));
    assert_eq!(info.session, None, "nothing started");
    drop(held);
    let info = w.runs.start(&t.key.to_string(), None).await.unwrap();
    assert_eq!(info.state, RunState::Starting);
    w.runs.stop(&t.key.to_string(), None).await.unwrap();
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn close_stops_it_with_a_toast_and_a_restart_leaves_stopped_by_restart() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    set_run_command(&w, "sleep 30").await;
    let (closing, _) = started(&w, "Won't do", 23_500..=23_599).await;
    let (other, _) = started(&w, "Price table", 23_500..=23_599).await;
    let (closing_key, other_key) = (closing.key.to_string(), other.key.to_string());
    let session = w
        .runs
        .start(&closing_key, None)
        .await
        .unwrap()
        .session
        .unwrap();
    w.runs.start(&other_key, None).await.unwrap();
    let mut listener = w.events.listen();

    w.review.close(&closing_key, true, &me()).await.unwrap();
    let info = w.runs.info(&closing_key, None).await.unwrap();
    assert_eq!(info.state, RunState::Stopped);
    assert_eq!(
        info.stopped_by,
        Some(StopCause::Finished {
            status: Status::ClosedNotMerged
        })
    );
    assert!(!w.sessions.info(&session).unwrap().is_running());
    assert!(output(&w, &session).contains(&format!("] stopped · {closing_key} is closed")));
    let toast = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(Event::RunStopped { message, .. }) = listener.receiver.recv().await {
                return message;
            }
        }
    })
    .await
    .unwrap();
    assert_eq!(toast, format!("Run stopped · {closing_key} is closed"));

    // A restart ends the other one; it shows "stopped by restart" and nothing starts it again.
    let w = w.restart().await;
    let info = wait_state(&w, &other_key, RunState::StoppedByRestart).await;
    assert_eq!(info.stopped_by, Some(StopCause::Restart));
    assert!(w.runs.running().await.is_empty());
    assert_eq!(
        w.board
            .ticket(&other_key)
            .await
            .unwrap()
            .flags
            .run_apps
            .first()
            .map(|a| a.port),
        None
    );
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_local_merge_stops_it_as_done() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    set_run_command(&w, "sleep 30").await;
    let (t, _) = started(&w, "Checkout API", 23_600..=23_699).await;
    let worktree = t.work.as_ref().unwrap().worktree.clone().unwrap();
    commit(std::path::Path::new(&worktree), "api.ts", "api\n");
    w.force_status(&t, Status::WaitingForYou).await;
    let key = t.key.to_string();
    w.runs.start(&key, None).await.unwrap();
    w.review.local_merge(&key, "m", false, &me()).await.unwrap();
    let info = w.runs.info(&key, None).await.unwrap();
    assert_eq!(info.state, RunState::Stopped);
    assert_eq!(
        info.stopped_by,
        Some(StopCause::Finished {
            status: Status::Merged
        })
    );
    w.stop().await;
}

/// Q30: Park frees the ports (d0052), so the Run stops first: its process is gone and the Run port is free.
#[tokio::test(flavor = "multi_thread")]
async fn park_stops_the_run_and_frees_its_port() {
    let w = TestService::start_with(|mut c| {
        c.terminals.shell = "sh".into();
        c.terminals.shell_args = Vec::new();
        c
    })
    .await;
    w.add_project("shop-web").await;
    let (t, port) = started(&w, "Price table", 23_700..=23_799).await;
    let worktree = t.work.as_ref().unwrap().worktree.clone().unwrap();
    // A dev server that holds the Run port.
    set_run_command(
        &w,
        on_platform(
            "echo $$ > run.pid; exec nc -lk $PORT",
            "powershell -NoProfile -Command \"Set-Content run.pid $PID; \
             $l = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Any, [int]$env:PORT); \
             $l.Start(); while ($true) { $l.AcceptTcpClient().Close() }\"",
        ),
    )
    .await;
    let key = t.key.to_string();
    let session = w.runs.start(&key, None).await.unwrap().session.unwrap();
    let pid_file = std::path::Path::new(&worktree).join("run.pid");
    let pid: i32 = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Ok(text) = std::fs::read_to_string(&pid_file)
                && let Ok(pid) = text.trim().parse()
            {
                return pid;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    assert!(alive(pid));
    wait_state(&w, &key, RunState::Running).await;
    assert!(
        !otaq_core::ports::is_free_on_machine(port),
        "the Run holds its port"
    );
    // The Shell holds no port: it stays open.
    let shell = w.shells.open_shell(&key, false).await.unwrap();

    w.review.park(&key, true, &me()).await.unwrap();
    let info = w.runs.info(&key, None).await.unwrap();
    assert_eq!(info.state, RunState::Stopped);
    assert_eq!(info.stopped_by, Some(StopCause::Parked));
    assert!(!w.sessions.info(&session).unwrap().is_running());
    assert!(output(&w, &session).contains(&format!("] stopped · {key} was parked")));
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(!alive(pid), "the Run's process is gone");
    let row = w.board.store().ticket(&t.id).await.unwrap().unwrap();
    assert_eq!((row.agent_port, row.run_port), (None, None), "ports freed");
    assert!(
        otaq_core::ports::is_free_on_machine(port),
        "nothing listens on the freed Run port"
    );
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
    assert!(w.runs.running().await.is_empty());
    assert!(
        w.sessions.info(&shell.id).unwrap().is_running(),
        "the Shell stays"
    );
    w.stop().await;
}

/// The acceptance "when the Run closes, the build is freed again": a Run of a ticket in Review brings its freed build back
/// before it starts, holds it while it runs, and when it ends the build is freed again.
#[tokio::test(flavor = "multi_thread")]
async fn run_in_review_brings_the_build_back_and_frees_it_when_it_ends() {
    let w = freed::world().await;
    let project = freed::shop_web(&w, |s| {
        s.apps = otaq_core::board::settings::RunApp::only("sleep 30", "")
    })
    .await;
    let (t, wt) = freed::freed(&w, "Price table", "price.ts").await;
    PortPool::with_range(w.board.store().clone(), 23_800..=23_899)
        .allocate(&t.id, &[otaq_core::board::settings::DEFAULT_APP.to_owned()])
        .await
        .unwrap();
    let key = t.key.to_string();

    let info = w.runs.start(&key, None).await.unwrap();

    assert!(info.state.is_live(), "{:?}", info.state);
    assert_eq!(freed::app_bin(&wt).as_deref(), Some("Price table"));
    assert_eq!(freed::state(&w, &key).await, BuildState::Present);
    assert_eq!(freed::entries(&w, &project), Vec::<u64>::new());
    assert_eq!(w.status(&key).await, Status::WaitingForYou);
    assert_eq!(
        w.ticket_env.free(&t.key).await,
        FreeOutcome::Waiting,
        "the Run holds the build"
    );

    w.runs.stop(&key, None).await.unwrap();
    freed::until_state(&w, &key, BuildState::Freed).await;
    assert_eq!(freed::app_bin(&wt), None);
    assert_eq!(freed::entries(&w, &project).len(), 1, "back in the stock");
    assert_eq!(freed::porcelain(&wt), "");
    w.stop().await;
}

/// A setup that fails while the build comes back never stops the Run, and never leaves the ticket `fetching`: what
/// came back is taken as present, so the Run's end frees it again (review fix, T-0315).
#[tokio::test(flavor = "multi_thread")]
async fn a_failed_setup_still_frees_the_build_when_the_run_ends() {
    let w = freed::world().await;
    let project = freed::shop_web(&w, |s| {
        s.apps = otaq_core::board::settings::RunApp::only("sleep 30", "");
        s.setup_command = "exit 3".into();
    })
    .await;
    let (t, wt) = freed::freed(&w, "Price badge", "badge.ts").await;
    PortPool::with_range(w.board.store().clone(), 23_700..=23_799)
        .allocate(&t.id, &[otaq_core::board::settings::DEFAULT_APP.to_owned()])
        .await
        .unwrap();
    let key = t.key.to_string();

    let info = w.runs.start(&key, None).await.unwrap();

    assert!(info.state.is_live(), "{:?}", info.state);
    assert_eq!(freed::app_bin(&wt).as_deref(), Some("Price badge"));
    assert_eq!(freed::state(&w, &key).await, BuildState::Present);

    w.runs.stop(&key, None).await.unwrap();
    freed::until_state(&w, &key, BuildState::Freed).await;
    assert_eq!(freed::app_bin(&wt), None);
    assert_eq!(freed::entries(&w, &project).len(), 1, "back in the stock");
    w.stop().await;
}

/// Two Run clicks while the freed build comes back start one Run: the second waits for the first and shows it.
#[tokio::test(flavor = "multi_thread")]
async fn a_second_click_while_the_build_comes_back_starts_no_second_run() {
    let w = freed::world().await;
    freed::shop_web(&w, |s| {
        s.apps = otaq_core::board::settings::RunApp::only("sleep 30", "");
        s.setup_command = format!("sleep 1 && echo {}", freed::SETUP_RAN);
    })
    .await;
    let (t, _wt) = freed::freed(&w, "Price filter", "filter.ts").await;
    PortPool::with_range(w.board.store().clone(), 23_900..=23_999)
        .allocate(&t.id, &[otaq_core::board::settings::DEFAULT_APP.to_owned()])
        .await
        .unwrap();
    let key = t.key.to_string();

    let (a, b) = tokio::join!(w.runs.start(&key, None), w.runs.start(&key, None));
    let (a, b) = (a.unwrap(), b.unwrap());

    assert!(a.state.is_live(), "{:?}", a.state);
    assert!(b.state.is_live(), "{:?}", b.state);
    assert!(a.session.is_some());
    assert_eq!(a.session, b.session, "one Run");
    w.runs.stop(&key, None).await.unwrap();
    w.stop().await;
}
