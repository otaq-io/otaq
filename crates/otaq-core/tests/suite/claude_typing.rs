//! What Otaq types into the real Claude Code arrives whole: the message writer's request (one line, over 1 KB), lines
//! of 1,296, 1,824 and 3,000 bytes, a 3 KB message of several lines and a short line each reach Claude Code's
//! conversation as the user's turn, every byte of it; and a long line naming one of Otaq's skills
//! (`/otaq:address-review SHOP-1 …`, typed as a bracketed paste) still runs as that skill. The terminal hands a long
//! write to Claude Code in reads of about 1 KB, and Claude Code (2.1.322) takes a read over 800 characters as a paste
//! and a shorter read right after it as typing that replaces the line: unbracketed, only the last read arrived.
//!
//! A hand check against the real tool, run on purpose: set `OTAQ_REAL_CLAUDE` to the program (`claude`, found on
//! PATH), signed in, and run these ignored tests (`OTAQ_REAL_CLAUDE_CASES`, words of the cases' names separated by
//! commas, runs only those). Each case's turn is stopped (Esc) as soon as it is in Claude Code's saved conversation,
//! so the model barely runs. `otaq` is a stand-in on PATH that only prints its arguments, so a command the writer's
//! request or a skill names does nothing. Otaq's skills come from the repository's `plugin` folder, loaded with
//! `--plugin-dir` as Otaq loads its plugin.
//!
//! What arrived is read from Claude Code's own record of the conversation
//! ([`otaq_core::agent_tools::claude::transcript`]), not from the screen, which shows a long paste folded, with
//! Claude Code's paste tags taken out as Otaq's reader takes them out.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::common::Fixture;
use otaq_core::agent_tools::claude::transcript::{find_transcript, without_paste_tags};
use otaq_core::agent_tools::claude::trust::ClaudeConfig;
use otaq_core::agent_tools::setup::SetupOutcome;
use otaq_core::agent_tools::{Adapters, Tool};
use otaq_core::data_dir::DataDir;
use otaq_core::service::EventBus;
use otaq_core::session::deliveries::{Deliveries, DeliveryKind, IdleSignal};
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

pub(crate) struct Claude {
    _f: Fixture,
    _stub: tempfile::TempDir,
    _plugin: tempfile::TempDir,
    _repo: tempfile::TempDir,
    repo: PathBuf,
    pub(crate) sessions: Sessions,
    deliveries: Deliveries,
    pub(crate) session: String,
    conversation: String,
    config: ClaudeConfig,
}

impl Claude {
    /// The real Claude Code in a session on `Tool::Claude`, idle at its empty input line, in a repository it
    /// trusts as Otaq records it for a project.
    pub(crate) async fn start(program: &str) -> Self {
        let f = Fixture::shop().await;
        let t = f.create("Real Claude Code typing").await;
        let repo_dir = repo();
        let repo = otaq_core::paths::canonical(repo_dir.path()).unwrap();
        let config = ClaudeConfig::from_env().expect("Claude Code's config file");
        assert!(
            matches!(
                config.record_trust(&repo),
                SetupOutcome::Recorded | SetupOutcome::AlreadyTrusted
            ),
            "trust for {}",
            repo.display()
        );
        let stub = otaq_stand_in();
        let plugin = otaq_plugin();
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
        let conversation = uuid::Uuid::now_v7().to_string();
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
                    "--session-id".into(),
                    conversation.clone(),
                    "--plugin-dir".into(),
                    plugin.path().to_string_lossy().into_owned(),
                ],
                cwd: repo.clone(),
                env: vec![("PATH".into(), path)],
                size: SIZE,
                tool: Some(Tool::Claude),
                tool_session_id: Some(conversation.clone()),
                model: None,
                agent: None,
                effort: None,
                theme: None,
            })
            .await
            .unwrap();
        let claude = Self {
            _f: f,
            _stub: stub,
            _plugin: plugin,
            _repo: repo_dir,
            repo,
            sessions,
            deliveries,
            session: info.id,
            conversation,
            config,
        };
        claude
            .until("its input line", Duration::from_secs(90), |s| {
                s.contains("for shortcuts")
                    || s.lines().any(|l| l.trim_start().starts_with('\u{276f}'))
            })
            .await;
        // Claude Code finishes starting (its hooks, plugins and the first status line) after the line first shows.
        tokio::time::sleep(Duration::from_secs(4)).await;
        let screen = claude.screen();
        assert!(!screen.contains("trust this folder"), "{screen}");
        claude
    }

    pub(crate) fn screen(&self) -> String {
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
                panic!("{what} never showed:\n{screen}");
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }

    /// Types `text` as Otaq types an automatic message, on Claude Code's empty input line.
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

    /// The user's turns in Claude Code's saved conversation, once there is one; the turn is then stopped.
    pub(crate) async fn turns(&self) -> Vec<String> {
        let config_dir = self.config.file.parent().unwrap().join(".claude");
        let config_dir = std::env::var_os("CLAUDE_CONFIG_DIR")
            .filter(|d| !d.is_empty())
            .map_or(config_dir, PathBuf::from);
        let start = Instant::now();
        loop {
            if let Some(file) = find_transcript(&config_dir, &self.repo, &self.conversation) {
                let turns = user_turns(&file);
                if !turns.is_empty() {
                    self.sessions.write(&self.session, b"\x1b").unwrap();
                    return turns;
                }
            }
            if start.elapsed() >= Duration::from_secs(30) {
                panic!("no turn in the saved conversation:\n{}", self.screen());
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
    }

    pub(crate) async fn stop(self) {
        self.sessions.write(&self.session, b"\x1b").unwrap();
        tokio::time::sleep(Duration::from_millis(1500)).await;
        self.sessions.shutdown().await;
    }
}

/// The text of every user record that isn't Claude Code's own (meta) record.
fn user_turns(file: &Path) -> Vec<String> {
    let text = std::fs::read_to_string(file).unwrap_or_default();
    text.lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .filter(|r| r["type"] == "user" && r["isMeta"] != true)
        .filter_map(|r| match &r["message"]["content"] {
            serde_json::Value::String(s) => Some(s.clone()),
            serde_json::Value::Array(blocks) => Some(
                blocks
                    .iter()
                    .filter_map(|b| b["text"].as_str())
                    .collect::<Vec<_>>()
                    .join(""),
            ),
            _ => None,
        })
        .filter(|t| !t.is_empty())
        .collect()
}

/// A message of `len` bytes whose every 10 bytes name their own place ("<00039>___"), so a cut shows where it is;
/// with `lines` above one, line breaks spread through it.
pub(crate) fn numbered(len: usize, lines: usize) -> String {
    let mut s = String::from("Do nothing; reply with only the word OK. ");
    while s.len() < len {
        s.push_str(&format!("<{:05}>___", s.len()));
    }
    s.truncate(len);
    let mut bytes = s.into_bytes();
    for i in 1..lines {
        bytes[i * len / lines] = b'\n';
    }
    String::from_utf8(bytes).unwrap()
}

/// A git repository with one commit.
fn repo() -> tempfile::TempDir {
    let dir = tempfile::Builder::new()
        .prefix("otaq-claude-typing-")
        .tempdir()
        .unwrap();
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

/// Otaq's Claude Code plugin as the app copies it: the plugin's manifest and Otaq's skills (its hooks left out: they
/// report to an Otaq that isn't running).
fn otaq_plugin() -> tempfile::TempDir {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugin");
    let dir = tempfile::tempdir().unwrap();
    copy_dir(
        &source.join("claude/.claude-plugin"),
        &dir.path().join(".claude-plugin"),
    );
    copy_dir(&source.join("skills"), &dir.path().join("skills"));
    dir
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
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

pub(crate) fn real_claude() -> String {
    std::env::var("OTAQ_REAL_CLAUDE").expect("OTAQ_REAL_CLAUDE: the real Claude Code to run")
}

fn wanted(what: &str) -> bool {
    std::env::var("OTAQ_REAL_CLAUDE_CASES")
        .ok()
        .filter(|c| !c.is_empty())
        .is_none_or(|cases| cases.split(',').any(|c| what.contains(c.trim())))
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "runs the real Claude Code: set OTAQ_REAL_CLAUDE"]
async fn a_long_message_reaches_claude_code_whole() {
    let program = real_claude();
    let writer = otaq_core::pr::message::prompt("SHOP-1", "main", false, true);
    assert!(
        writer.len() > 1024 && !writer.contains('\n'),
        "{}",
        writer.len()
    );
    for (what, text) in [
        ("the message writer's request", writer),
        ("a 1,296-byte line", numbered(1296, 1)),
        ("a 1,824-byte line", numbered(1824, 1)),
        ("a 3,000-byte line", numbered(3000, 1)),
        ("a 3,000-byte message of 6 lines", numbered(3000, 6)),
        ("a short line", numbered(300, 1)),
    ] {
        if !wanted(what) {
            continue;
        }
        let claude = Claude::start(&program).await;
        claude.deliver(&text).await;
        let turns = claude.turns().await;
        println!(
            "RESULT {what}: {} bytes typed, turns {:?}, paste blocks {}",
            text.len(),
            turns.iter().map(String::len).collect::<Vec<_>>(),
            turns
                .iter()
                .map(|t| t.matches("<pasted_content").count())
                .sum::<usize>()
        );
        assert_eq!(turns.len(), 1, "{what}: {turns:?}");
        assert_eq!(without_paste_tags(&turns[0]), text, "{what}");
        claude.stop().await;
    }
}

/// A hand-off's line is the skill and what came back (`/otaq:address-review SHOP-1 was sent back…`): over the
/// bracketing length or with line breaks it goes in as the skill's name typed and the rest pasted, and Claude Code
/// runs it as Otaq's skill with the rest whole as its arguments (the whole line pasted, it was sent as a plain
/// message). A short one, typed as before, is the control.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "runs the real Claude Code: set OTAQ_REAL_CLAUDE"]
async fn a_long_skill_line_still_runs_the_skill() {
    let program = real_claude();
    let told = |len: usize, lines: usize| {
        format!(
            "SHOP-1 was sent back to Dev with a comment. Do nothing and run nothing; reply with only the word OK. {}",
            numbered(len, lines)
        )
    };
    for (what, told) in [
        ("a long skill line", told(900, 1)),
        ("a 2 KB skill line", told(2000, 1)),
        ("a multi-line skill line", told(1500, 4)),
        (
            "a short skill line",
            "SHOP-1 was sent back to Dev.".to_owned(),
        ),
    ] {
        if !wanted(what) {
            continue;
        }
        let claude = Claude::start(&program).await;
        let line = claude
            .deliveries
            .skill_text(&claude.session, "address-review", Some(&told));
        assert!(line.starts_with("/otaq:address-review "), "{line}");
        claude.deliver(&line).await;
        let turns = claude.turns().await;
        println!("RESULT {what}: {} bytes typed, turns {turns:?}", line.len());
        let turn = without_paste_tags(&turns[0]);
        assert!(
            turn.contains("<command-name>/otaq:address-review</command-name>"),
            "{what} ran as the skill: {turn}"
        );
        let args = turn
            .split_once("<command-args>")
            .and_then(|(_, rest)| rest.split_once("</command-args>"))
            .map(|(args, _)| args.trim().to_owned());
        assert_eq!(args.as_deref(), Some(told.as_str()), "{what}: {turn}");
        claude.stop().await;
    }
}

/// The row of Claude Code's input line (its `❯` marker), as the screen shows it.
fn input_row(screen: &str) -> String {
    screen
        .lines()
        .rfind(|l| l.trim_start().starts_with('\u{276f}'))
        .unwrap_or_default()
        .to_owned()
}

/// Ctrl+V reaches Claude Code as the raw key where the terminal has no text to paste (an image or nothing on the
/// clipboard); finding no image to paste, Claude Code leaves its line as it was, and the line keeps reading empty
/// for Otaq's notes and answers. A history entry of several lines (↑) and an image pasted with Alt+V
/// (`[Image #1]`) still count as unsent text. The "image" case wants an image on the clipboard, set before the run
/// (from PowerShell on Windows); "raw ctrl+v" wants none.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "runs the real Claude Code: set OTAQ_REAL_CLAUDE"]
async fn a_ctrl_v_that_pastes_nothing_leaves_claude_codes_line_empty() {
    let program = real_claude();
    if wanted("raw ctrl+v") {
        let claude = Claude::start(&program).await;
        // A turn of two lines first, so ↑ has an entry of several lines. (Stopped with Esc, Claude Code would put
        // the message back in its line.) Then the bare `❯` Claude Code shows once its suggestion is gone.
        claude
            .deliver("Do nothing and run nothing; reply with only the word PINEAPPLE.\nThis is the second line.")
            .await;
        claude
            .until("the reply", Duration::from_secs(120), |s| {
                s.matches("PINEAPPLE").count() >= 2
            })
            .await;
        let bare = |s: &str| input_row(s).trim() == "\u{276f}";
        let start = Instant::now();
        while !bare(&claude.screen()) && start.elapsed() < Duration::from_secs(180) {
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
        let screen = claude.screen();
        println!("RESULT before: {:?}", input_row(&screen));
        assert!(bare(&screen), "Claude Code's bare line:\n{screen}");
        claude.sessions.input(&claude.session, b"\x16").unwrap();
        tokio::time::sleep(Duration::from_secs(2)).await;
        let screen = claude.screen();
        println!("RESULT raw ctrl+v: {:?}", input_row(&screen));
        assert!(
            !claude.sessions.has_unsent_input(&claude.session).unwrap(),
            "{screen}"
        );
        claude.sessions.input(&claude.session, b"\x1b[A").unwrap();
        tokio::time::sleep(Duration::from_secs(2)).await;
        let screen = claude.screen();
        println!("RESULT history: {:?}\n{screen}", input_row(&screen));
        assert!(
            claude.sessions.has_unsent_input(&claude.session).unwrap(),
            "{screen}"
        );
        claude.stop().await;
    }
    if wanted("image") {
        let claude = Claude::start(&program).await;
        claude.sessions.input(&claude.session, b"\x1bv").unwrap();
        let screen = claude
            .until("the pasted image", Duration::from_secs(10), |s| {
                s.contains("[Image #1]")
            })
            .await;
        tokio::time::sleep(Duration::from_secs(1)).await;
        println!("RESULT image: {:?}", input_row(&screen));
        assert!(
            claude.sessions.has_unsent_input(&claude.session).unwrap(),
            "{screen}"
        );
        claude.stop().await;
    }
}
