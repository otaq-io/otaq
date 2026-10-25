//! `otaq attach` (T-0055) against a service running in the test, with `cat` as the session's program and
//! the command itself inside a PTY: scrollback then live output, typing, resizing, detaching (the session keeps
//! running), `--stage` (FLW-20) on the test flow, and the error cases.

use std::io::{Read, Write};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use otaq_core::board::tickets::Ticket;
use otaq_core::data_dir::DataDir;
use otaq_core::domain::Status;
use otaq_core::flow::yaml;
use otaq_core::naming;
use otaq_core::service::testing::TestService;
use otaq_core::session::{SessionKind, SessionTicket, SpawnSpec, Start, TerminalSize};
use portable_pty::{CommandBuilder, PtySize, native_pty_system};

/// shop-web on the test flow (`otaq-core/tests/flows/every-kind.yaml`), SHOP-1 with an agent session and a check
/// session (both `cat`), SHOP-2 with none.
struct World {
    service: TestService,
    shop1: Ticket,
    session: String,
    /// SHOP-1's check (d0063), also `cat`.
    validator: String,
}

impl World {
    async fn new() -> Self {
        let service = TestService::start().await;
        let project = service.add_project("shop-web").await;
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../otaq-core/tests/flows/every-kind.yaml");
        let file = yaml::read(&std::fs::read_to_string(path).unwrap()).unwrap();
        service.board.use_flow_for_tests("SHOP", &file).await;
        let shop1 = service.create("SHOP", "with a session").await;
        service.create("SHOP", "without one").await;
        let spawn = |kind| SpawnSpec {
            kind,
            start: Start::User,
            project_id: project.id.clone(),
            project_key: "SHOP".into(),
            ticket: Some(SessionTicket {
                id: shop1.id.clone(),
                key: shop1.key.clone(),
            }),
            program: "cat".into(),
            args: vec![],
            cwd: project.repo_path.clone().into(),
            env: vec![],
            size: TerminalSize::default(),
            tool: None,
            tool_session_id: None,
            model: None,
            agent: None,
            effort: None,
            fallback: None,
            theme: None,
        };
        let info = service
            .sessions
            .spawn(spawn(SessionKind::Agent))
            .await
            .unwrap();
        let validator = service
            .sessions
            .spawn(spawn(SessionKind::Validator))
            .await
            .unwrap();
        Self {
            service,
            shop1,
            session: info.id,
            validator: validator.id,
        }
    }

    fn home(&self) -> &std::path::Path {
        self.service.data.root()
    }

    fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_otaq"));
        command
            .args(args)
            .env(DataDir::env_var(), self.home())
            .env_remove(naming::ticket_env_var());
        command
    }

    async fn stop(self) {
        self.service.stop().await;
    }
}

/// Waits until `seen` contains `needle`.
async fn wait_for(seen: &Arc<Mutex<Vec<u8>>>, needle: &str) {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if String::from_utf8_lossy(&seen.lock().unwrap()).contains(needle) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap_or_else(|_| {
        panic!(
            "{needle:?} not seen in {:?}",
            String::from_utf8_lossy(&seen.lock().unwrap())
        )
    });
}

#[tokio::test(flavor = "multi_thread")]
async fn attach_shows_scrollback_relays_typing_and_detaches() {
    let w = World::new().await;
    w.service
        .sessions
        .write(&w.session, b"before attach\n")
        .unwrap();

    let pty = native_pty_system()
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .unwrap();
    let mut cmd = CommandBuilder::new(env!("CARGO_BIN_EXE_otaq"));
    cmd.args(["attach", "SHOP-1"]);
    cmd.env(DataDir::env_var(), w.home());
    let mut child = pty.slave.spawn_command(cmd).unwrap();
    drop(pty.slave);
    let mut reader = pty.master.try_clone_reader().unwrap();
    let mut writer = pty.master.take_writer().unwrap();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = seen.clone();
    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        while let Ok(n) = reader.read(&mut buf) {
            if n == 0 {
                break;
            }
            sink.lock().unwrap().extend_from_slice(&buf[..n]);
        }
    });

    // Windows: ConPTY first asks the terminal (this test) where the cursor is, and waits for the answer.
    if cfg!(windows) {
        wait_for(&seen, "\x1b[6n").await;
        writer.write_all(b"\x1b[1;1R").unwrap();
    }
    // The stored scrollback first.
    wait_for(&seen, "before attach").await;
    // Attached counts as looking at the ticket (ATT-03).
    tokio::time::timeout(Duration::from_secs(10), async {
        while !w.service.sessions.is_focused(&w.session).unwrap() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("attach holds focus");
    // Typing reaches the session (cat prints the line back).
    writer.write_all(b"typed in attach\r").unwrap();
    wait_for(&seen, "typed in attach").await;
    let session_out =
        String::from_utf8_lossy(&w.service.sessions.attach(&w.session).unwrap().scrollback)
            .into_owned();
    assert!(session_out.contains("typed in attach"), "{session_out:?}");

    // Resizing the terminal resizes the PTY.
    pty.master
        .resize(PtySize {
            rows: 30,
            cols: 100,
            pixel_width: 0,
            pixel_height: 0,
        })
        .unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        while w.service.sessions.info(&w.session).unwrap().size
            != Some(TerminalSize {
                cols: 100,
                rows: 30,
            })
        {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the session follows the terminal's size");

    // Ctrl+] detaches; the session keeps running. A test's pseudo console on Windows doesn't pass Ctrl+] on as a
    // key (neither as the byte nor as a win32-input-mode key event), so there it is checked by hand in Windows
    // Terminal (review-M11, T-0156).
    if cfg!(windows) {
        let _ = child.kill();
        w.stop().await;
        return;
    }
    writer.write_all(&[0x1d]).unwrap();
    let mut killer = child.clone_killer();
    let waited = tokio::time::timeout(
        Duration::from_secs(10),
        tokio::task::spawn_blocking(move || child.wait()),
    )
    .await;
    let Ok(status) = waited else {
        let _ = killer.kill();
        panic!(
            "attach didn't end after Ctrl+]: {:?}",
            String::from_utf8_lossy(&seen.lock().unwrap())
        );
    };
    let status = status.unwrap().unwrap();
    assert!(status.success(), "{status:?}");
    wait_for(&seen, "detached from SHOP-1").await;
    assert!(w.service.sessions.info(&w.session).unwrap().is_running());
    assert!(
        !w.service.sessions.is_focused(&w.session).unwrap(),
        "detaching lets go"
    );
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn errors_name_the_ticket_and_exit_non_zero() {
    let w = World::new().await;
    let check = |mut command: Command, expected: &str| {
        let out = command.output().unwrap();
        assert!(!out.status.success(), "{out:?}");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(stderr.contains(expected), "{stderr}");
    };
    let none = "SHOP-2 has no live session";
    let w2 = &w;
    tokio::task::spawn_blocking({
        let commands = vec![
            (w2.command(&["attach", "SHOP-2"]), none),
            (w2.command(&["-P", "SHOP", "attach", "2"]), none),
            (
                {
                    let mut c = w2.command(&["attach"]);
                    c.env(naming::ticket_env_var(), "SHOP-2");
                    c
                },
                none,
            ),
            (
                {
                    let mut c = w2.command(&["attach", "2"]);
                    c.env(naming::ticket_env_var(), "SHOP-1");
                    c
                },
                none,
            ),
            (w2.command(&["attach"]), "name a ticket"),
            // FLW-20: a stage the ticket isn't in, a stage the flow hasn't, and the removed flag.
            (
                w2.command(&["attach", "SHOP-2", "--stage", "security"]),
                "SHOP-2 isn't in Security",
            ),
            (
                w2.command(&["attach", "SHOP-2", "--stage", "nope"]),
                "SHOP's flow has no stage nope (its stages: Grill, Dev, Validate, Security, QA, Done)",
            ),
            (
                w2.command(&["attach", "SHOP-2", "--validator"]),
                "unexpected argument '--validator'",
            ),
            (w2.command(&["attach", "SHOP-99"]), "SHOP-99"),
        ];
        move || {
            for (command, expected) in commands {
                check(command, expected);
            }
        }
    })
    .await
    .unwrap();
    w.stop().await;
}

#[test]
fn without_a_service_attach_says_so() {
    let home = tempfile::tempdir().unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_otaq"))
        .args(["attach", "SHOP-1"])
        .env(DataDir::env_var(), home.path())
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("isn't running"),
        "{out:?}"
    );
}

/// `otaq attach <KEY> --stage <name>` (FLW-20; was T-0183's `--validator`) opens the session of the agent running
/// that stage: in Dev a Validate check is refused; in Validate (one check agent) it opens the check's terminal, not
/// the agent's.
#[tokio::test(flavor = "multi_thread")]
async fn attach_stage_reaches_the_check_running_it() {
    let w = World::new().await;
    w.service.force_status(&w.shop1, Status::Working).await;
    let out = w
        .command(&["attach", "SHOP-1", "--stage", "validate"])
        .output()
        .unwrap();
    assert!(!out.status.success(), "{out:?}");
    assert_eq!(
        String::from_utf8_lossy(&out.stderr),
        "error: SHOP-1 isn't in Validate\n"
    );
    w.service
        .force_stage(&w.shop1, "validate", Status::Checking)
        .await;
    w.service
        .sessions
        .write(&w.session, b"the agent\n")
        .unwrap();
    w.service
        .sessions
        .write(&w.validator, b"the validator\n")
        .unwrap();
    let pty = native_pty_system()
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .unwrap();
    let mut cmd = CommandBuilder::new(env!("CARGO_BIN_EXE_otaq"));
    cmd.args(["attach", "SHOP-1", "--stage", "Validate"]);
    cmd.env(DataDir::env_var(), w.home());
    let mut child = pty.slave.spawn_command(cmd).unwrap();
    drop(pty.slave);
    let mut reader = pty.master.try_clone_reader().unwrap();
    let mut writer = pty.master.take_writer().unwrap();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = seen.clone();
    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        while let Ok(n) = reader.read(&mut buf) {
            if n == 0 {
                break;
            }
            sink.lock().unwrap().extend_from_slice(&buf[..n]);
        }
    });
    if cfg!(windows) {
        wait_for(&seen, "\x1b[6n").await;
        writer.write_all(b"\x1b[1;1R").unwrap();
    }
    wait_for(&seen, "the validator").await;
    assert!(
        !String::from_utf8_lossy(&seen.lock().unwrap()).contains("the agent"),
        "the validator's terminal, not the agent's"
    );
    tokio::time::timeout(Duration::from_secs(10), async {
        while !w.service.sessions.is_focused(&w.validator).unwrap() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("attaching holds the validator's focus");
    child.kill().unwrap();
    let _ = child.wait();
    w.stop().await;
}

/// d0130 row 17: Security has two check agents. `--stage security` alone is refused with the count; `--agent` (any
/// case) opens that one's checker; a name the stage hasn't is refused with its check agents.
#[tokio::test(flavor = "multi_thread")]
async fn attach_agent_picks_one_check_agent_of_several() {
    let w = World::new().await;
    let project = w.service.board.project("SHOP").await.unwrap();
    let reviewer = w
        .service
        .sessions
        .spawn(SpawnSpec {
            kind: SessionKind::Validator,
            start: Start::User,
            project_id: project.id.clone(),
            project_key: "SHOP".into(),
            ticket: Some(SessionTicket {
                id: w.shop1.id.clone(),
                key: w.shop1.key.clone(),
            }),
            program: "cat".into(),
            args: vec![],
            cwd: project.repo_path.clone().into(),
            env: vec![],
            size: TerminalSize::default(),
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
    w.service
        .validating_with_agents(
            &w.shop1,
            &[
                ("Security reviewer", &reviewer.id),
                ("Validator", &w.validator),
            ],
        )
        .await;
    w.service
        .force_stage(&w.shop1, "security", Status::Checking)
        .await;
    let refused = |args: &[&str], expected: &str| {
        let out = w.command(args).output().unwrap();
        assert!(!out.status.success(), "{out:?}");
        assert_eq!(String::from_utf8_lossy(&out.stderr), expected);
    };
    refused(
        &["attach", "SHOP-1", "--stage", "security"],
        "error: Security has 2 check agents; add --agent\n",
    );
    refused(
        &[
            "attach",
            "SHOP-1",
            "--stage",
            "security",
            "--agent",
            "Performance",
        ],
        "error: Security has no check agent Performance (its check agents: Security reviewer, Validator)\n",
    );
    w.service
        .sessions
        .write(&reviewer.id, b"the reviewer\n")
        .unwrap();
    w.service
        .sessions
        .write(&w.validator, b"the validator\n")
        .unwrap();
    let pty = native_pty_system()
        .openpty(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .unwrap();
    let mut cmd = CommandBuilder::new(env!("CARGO_BIN_EXE_otaq"));
    cmd.args([
        "attach",
        "SHOP-1",
        "--stage",
        "security",
        "--agent",
        "security reviewer",
    ]);
    cmd.env(DataDir::env_var(), w.home());
    let mut child = pty.slave.spawn_command(cmd).unwrap();
    drop(pty.slave);
    let mut reader = pty.master.try_clone_reader().unwrap();
    let mut writer = pty.master.take_writer().unwrap();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = seen.clone();
    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        while let Ok(n) = reader.read(&mut buf) {
            if n == 0 {
                break;
            }
            sink.lock().unwrap().extend_from_slice(&buf[..n]);
        }
    });
    if cfg!(windows) {
        wait_for(&seen, "\x1b[6n").await;
        writer.write_all(b"\x1b[1;1R").unwrap();
    }
    wait_for(&seen, "the reviewer").await;
    assert!(
        !String::from_utf8_lossy(&seen.lock().unwrap()).contains("the validator"),
        "the Security reviewer's terminal, not the Validator's"
    );
    child.kill().unwrap();
    let _ = child.wait();
    w.stop().await;
}
