//! What Otaq types into the real Codex starts a turn: a skill typed alone (`$otaq:push`, `$otaq:restack`) and a
//! multi-line message, each with the one Enter Otaq sends; and a stuck line ending in a space still reads as text.
//! A hand check against the real tool, run on purpose: set `OTAQ_REAL_CODEX` to the program (`codex`, found on
//! PATH), signed in, with Otaq's skills where Codex finds them (Otaq set up for Codex), and run this ignored test.
//! Each case asks the model for a short turn. `otaq` is a stand-in on PATH that only prints its arguments, so the
//! skills' own commands do nothing. The test's terminal answers Codex's cursor questions as a terminal attached in
//! the app does.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::common::Fixture;
use otaq_core::agent_tools::{Adapters, Tool};
use otaq_core::data_dir::DataDir;
use otaq_core::domain::HandOffKind;
use otaq_core::service::EventBus;
use otaq_core::session::deliveries::{Deliveries, Delivery, DeliveryKind, IdleSignal};
use otaq_core::session::hand_offs::HandOff;
use otaq_core::session::{SessionKind, SessionTicket, Sessions, SpawnSpec, Start, TerminalSize};

struct Idle(AtomicBool);

impl IdleSignal for Idle {
    fn is_idle(&self, _session: &str) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

const SIZE: TerminalSize = TerminalSize {
    cols: 120,
    rows: 40,
};

struct Codex {
    _f: Fixture,
    _stub: tempfile::TempDir,
    _repo: tempfile::TempDir,
    sessions: Sessions,
    deliveries: Deliveries,
    session: String,
}

impl Codex {
    /// The real Codex in a session on `Tool::Codex`, idle at its empty input line.
    async fn start(program: &str) -> Self {
        let f = Fixture::shop().await;
        let t = f.create("Real Codex typing").await;
        let repo_dir = trusted_repo();
        let repo = repo_dir.path().to_owned();
        let stub = otaq_stand_in();
        let path = std::env::join_paths(std::iter::once(stub.path().to_owned()).chain(
            std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()),
        ))
        .unwrap()
        .into_string()
        .unwrap();
        let bus = EventBus::new(256);
        let sessions = Sessions::new(f.board.store().clone(), bus.clone());
        let adapters = Adapters::launching_only(None, DataDir::new(f.home.path().join("home")));
        let deliveries = Deliveries::new(
            f.board.store().clone(),
            sessions.clone(),
            bus,
            Arc::new(Idle(AtomicBool::new(true))),
            adapters,
        );
        let info = sessions
            .spawn(SpawnSpec {
                kind: SessionKind::Agent,
                start: Start::User,
                project_id: f.board.project("SHOP").await.unwrap().id,
                project_key: "SHOP".into(),
                ticket: Some(SessionTicket {
                    id: t.id.clone(),
                    key: t.key.clone(),
                }),
                program: program.into(),
                args: vec![
                    "--no-daemon".into(),
                    "-c".into(),
                    "check_for_update_on_startup=false".into(),
                    "-c".into(),
                    format!("projects.{}.trust_level=\"trusted\"", repo.display()),
                ],
                cwd: repo,
                env: vec![("PATH".into(), path)],
                size: SIZE,
                tool: Some(Tool::Codex),
                tool_session_id: None,
                model: None,
                agent: None,
                effort: None,
                theme: None,
            })
            .await
            .unwrap();
        answer_cursor_questions(&sessions, &info.id);
        let codex = Self {
            _f: f,
            _stub: stub,
            _repo: repo_dir,
            sessions,
            deliveries,
            session: info.id,
        };
        codex
            .until("its input line", Duration::from_secs(90), |s| {
                s.contains("for shortcuts") || s.contains("% left") || s.contains("context left")
            })
            .await;
        // Codex finishes drawing (and loading its skills) after the composer first shows.
        tokio::time::sleep(Duration::from_secs(3)).await;
        let screen = codex.screen();
        assert!(!screen.contains("Trust this folder"), "{screen}");
        codex
    }

    fn screen(&self) -> String {
        self.sessions
            .visible_screen(&self.session)
            .map(|s| s.to_string())
            .unwrap_or_default()
    }

    async fn until(&self, what: &str, within: Duration, check: impl Fn(&str) -> bool) -> String {
        let start = Instant::now();
        loop {
            let screen = self.screen();
            if check(&screen) {
                return screen;
            }
            if start.elapsed() >= within {
                let info = self.sessions.info(&self.session);
                let output = self
                    .sessions
                    .attach(&self.session)
                    .map(|a| a.scrollback)
                    .unwrap_or_default();
                let tail = String::from_utf8_lossy(&output[output.len().saturating_sub(3000)..]);
                panic!("{what} never showed:\n{screen}\nsession: {info:?}\noutput's end: {tail:?}");
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }

    /// Types `text` as Otaq types an automatic message, on Codex's empty input line.
    async fn deliver(&self, text: &str) {
        assert!(
            !self.sessions.has_unsent_input(&self.session).unwrap(),
            "the line reads empty:\n{}",
            self.screen()
        );
        self.deliveries
            .queue_delivery(&self.session, DeliveryKind::AgentMessage, text)
            .await
            .unwrap();
        assert!(
            self.deliveries
                .pending(&self.session)
                .await
                .unwrap()
                .is_empty(),
            "typed at once"
        );
    }

    /// The turn `text` started is running (Codex's "Working … esc to interrupt").
    async fn turn_starts(&self, text: &str) -> String {
        self.until(
            &format!("the turn {text:?} started"),
            Duration::from_secs(30),
            working,
        )
        .await
    }

    async fn stop(self) {
        self.sessions.write(&self.session, b"\x1b").unwrap();
        tokio::time::sleep(Duration::from_millis(1500)).await;
        self.sessions.shutdown().await;
    }
}

fn working(screen: &str) -> bool {
    screen.contains("esc to interrupt") || screen.contains("Esc to interrupt")
}

/// What the session's terminal answers in the app: the cursor's position when Codex asks for it.
fn answer_cursor_questions(sessions: &Sessions, session: &str) {
    let mut attached = sessions.attach(session).unwrap();
    let sessions = sessions.clone();
    let session = session.to_owned();
    tokio::spawn(async move {
        let mut parser = vt100::Parser::new(SIZE.rows, SIZE.cols, 0);
        parser.process(&attached.scrollback);
        while let Ok(chunk) = attached.output.recv().await {
            parser.process(&chunk.data);
            if chunk.data.windows(4).any(|w| w == b"\x1b[6n") {
                let (row, col) = parser.screen().cursor_position();
                let answer = format!("\x1b[{};{}R", row + 1, col + 1);
                if sessions.write(&session, answer.as_bytes()).is_err() {
                    break;
                }
            }
        }
    });
}

/// A git repository Codex trusts through `-c projects.<path>.trust_level`: a folder whose path has no dot, since the
/// key is split at dots (the temporary folder's usual `.tmp` name would make Codex ask "Trust this folder?").
fn trusted_repo() -> tempfile::TempDir {
    let dir = tempfile::Builder::new()
        .prefix("otaq-codex-typing-")
        .rand_bytes(6)
        .tempdir()
        .unwrap();
    assert!(
        !dir.path().to_string_lossy().contains('.'),
        "{}",
        dir.path().display()
    );
    std::fs::write(dir.path().join("README.md"), "# typing\n").unwrap();
    let git = |args: &[&str]| {
        let status = std::process::Command::new("git")
            .args(args)
            .current_dir(dir.path())
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?}");
    };
    git(&["init", "-q"]);
    git(&["add", "."]);
    git(&[
        "-c",
        "user.name=Otaq test",
        "-c",
        "user.email=test@example.invalid",
        "commit",
        "-qm",
        "init",
    ]);
    dir
}

/// A folder with an `otaq` that prints its arguments and does nothing else.
fn otaq_stand_in() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    write_stand_in(dir.path());
    dir
}

#[cfg(windows)]
fn write_stand_in(dir: &Path) {
    std::fs::write(dir.join("otaq.cmd"), "@echo stub otaq %*\r\n").unwrap();
}

#[cfg(unix)]
fn write_stand_in(dir: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join("otaq");
    std::fs::write(&path, "#!/bin/sh\necho stub otaq \"$@\"\n").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

fn real_codex() -> String {
    std::env::var("OTAQ_REAL_CODEX").expect("OTAQ_REAL_CODEX: the real Codex to run")
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "runs the real Codex: set OTAQ_REAL_CODEX"]
async fn what_otaq_types_starts_codexs_turn() {
    let program = real_codex();

    // A skill alone: Push to PR and Retry push type `push` as the user's click, a merge conflict queues `restack`.
    // The skill's turn runs its first command, `otaq prime` (the stand-in).
    for (skill, kind) in [
        ("push", HandOffKind::Push),
        ("restack", HandOffKind::MergeConflict),
    ] {
        let codex = Codex::start(&program).await;
        let text = codex.deliveries.skill_text(&codex.session, skill, None);
        assert_eq!(text, format!("$otaq:{skill}"));
        assert!(!codex.sessions.has_unsent_input(&codex.session).unwrap());
        let hand_off = HandOff::new(kind, text.clone());
        if kind == HandOffKind::Push {
            let sent = codex
                .deliveries
                .hand_off(&codex.session, hand_off, false)
                .await
                .unwrap();
            assert_eq!(sent, Delivery::Sent { count: 1 });
        } else {
            codex
                .deliveries
                .queue_hand_off(&codex.session, hand_off)
                .await
                .unwrap();
            assert!(
                codex
                    .deliveries
                    .pending(&codex.session)
                    .await
                    .unwrap()
                    .is_empty()
            );
        }
        codex.turn_starts(&text).await;
        let screen = codex
            .until("the skill's first command", Duration::from_secs(60), |s| {
                s.contains("stub otaq prime")
            })
            .await;
        println!("{text}:\n{screen}");
        codex.stop().await;
    }

    // A multi-line message goes whole, as one message.
    let codex = Codex::start(&program).await;
    let text = "First line: this message has two lines.\nSecond line: do nothing; reply only with the word PONG.";
    codex.deliver(text).await;
    let screen = codex
        .until("Codex's answer", Duration::from_secs(90), |s| {
            s.contains("PONG\n") || s.lines().any(|l| l.trim_end().ends_with("PONG"))
        })
        .await;
    println!("multi-line:\n{screen}");
    assert!(
        screen.contains("First line: this message has two lines."),
        "{screen}"
    );
    assert!(screen.contains("Second line: do nothing"), "{screen}");
    codex.stop().await;

    // A line left typed and ending in a space still reads as text: nothing is typed over it.
    let codex = Codex::start(&program).await;
    codex
        .sessions
        .write(&codex.session, b"$otaq:push ")
        .unwrap();
    let screen = codex
        .until("the typed line", Duration::from_secs(10), |s| {
            s.lines()
                .any(|l| l.trim_start().starts_with('\u{203a}') && l.contains("$otaq:push"))
        })
        .await;
    tokio::time::sleep(Duration::from_millis(700)).await;
    assert!(
        codex.sessions.has_unsent_input(&codex.session).unwrap(),
        "{screen}"
    );
    codex
        .deliveries
        .queue_delivery(&codex.session, DeliveryKind::AgentMessage, "from SHOP-18")
        .await
        .unwrap();
    assert_eq!(
        codex
            .deliveries
            .pending(&codex.session)
            .await
            .unwrap()
            .len(),
        1,
        "held behind the typed line"
    );
    assert!(!working(&codex.screen()), "nothing sent");
    // Clear the line (Ctrl+E, Ctrl+U) so nothing is sent as Codex quits.
    codex.sessions.write(&codex.session, b"\x05\x15").unwrap();
    codex.stop().await;
}
