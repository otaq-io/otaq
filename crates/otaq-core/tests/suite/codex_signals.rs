//! A Codex agent's session file, watched while it runs (T-0350; ADP-07, ADP-03, AGT-05, USG-09; d0086, d0088): the
//! watcher starts when Codex says its conversation id, reads only whole lines and stops with the session; the
//! saved session files (`tests/fixtures/codex/session_file/`), appended line by line, give a refused model (Start
//! failed, a validator's No result), the usage limit, the limit windows and the tokens; a file that can't be read
//! sets "limits not reported" and the quiet agent's card falls back to Idle. A stand-in `codex` (an `sh` script)
//! runs as the agent: no test starts codex, and Codex's home is a temporary folder.
//!
//! Unix only: the stand-in agent tools are `sh` scripts started by name, which Windows can't run (Q598).
#![cfg(unix)]

use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::agent_tools::{LaunchContext, Tool};
use otaq_core::agents::AgentModel;
use otaq_core::board::moves::DropTarget;
use otaq_core::caller::Caller;
use otaq_core::client::{Client, ClientConfig};
use otaq_core::domain::{Actor, StartStep, Status};
use otaq_core::protocol::{Reply, Request};
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::TestService;
use otaq_core::session::SessionKind;
use otaq_core::signals::watch::{LIMITS_NOT_REPORTED, Timing};
use otaq_core::usage::limit::ResetSource;
use otaq_core::usage::plan;
use serde_json::json;
use tempfile::TempDir;

/// The conversation id Codex reports.
const ID: &str = "01a14bae-dd63-7c03-b96e-8f7557586707";

fn me() -> Actor {
    Actor::User("me".into())
}

struct World {
    w: TestService,
    _bin: TempDir,
    /// The user's home folder of the tools (Codex's is `.codex` in it).
    home: TempDir,
}

impl World {
    /// The conversation's session file in Codex's home (not created).
    fn session_file(&self) -> PathBuf {
        self.home
            .path()
            .join(".codex/sessions/2026/10/18")
            .join(format!("rollout-2026-10-18T00-05-13-{ID}.jsonl"))
    }

    /// Creates the session file with Codex's first record.
    fn create_file(&self) -> PathBuf {
        let file = self.session_file();
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        let first = saved("model_refused.jsonl")
            .lines()
            .next()
            .unwrap()
            .to_owned();
        std::fs::write(&file, format!("{first}\n")).unwrap();
        file
    }
}

/// A saved session file.
fn saved(name: &str) -> String {
    let path = format!(
        "{}/tests/fixtures/codex/session_file/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read_to_string(path).unwrap()
}

/// The lines of the saved session file `name` whose record is of `kind` (`task_complete`, `token_count`).
fn saved_lines(name: &str, kind: &str) -> Vec<String> {
    saved(name)
        .lines()
        .filter(|l| l.contains(&format!("\"type\":\"{kind}\"")))
        .map(str::to_owned)
        .collect()
}

fn append(file: &Path, text: &str) {
    std::fs::OpenOptions::new()
        .append(true)
        .open(file)
        .unwrap()
        .write_all(text.as_bytes())
        .unwrap();
}

/// A service whose `codex` prints its arguments and waits, with Codex's home in a temporary folder, short watch
/// times, and project SHOP with ticket SHOP-1 whose Developer agent runs on Codex, in Ready for Dev.
async fn world() -> World {
    let bin = TempDir::new().unwrap();
    let codex = bin.path().join(Tool::Codex.command_name());
    std::fs::write(&codex, "#!/bin/sh\nprintf 'ARGS %s\\n' \"$*\"; exec cat\n").unwrap();
    std::fs::set_permissions(&codex, std::fs::Permissions::from_mode(0o755)).unwrap();
    let home = TempDir::new().unwrap();
    let (bin_dir, tool_home) = (bin.path().to_owned(), home.path().to_owned());
    let w = TestService::start_with(move |config| ServiceConfig {
        launch: Some(LaunchContext {
            bin_dir,
            plugin_dir: "/nonexistent/plugin".into(),
            path: std::env::var_os("PATH").unwrap_or_default(),
            claude: ClaudeLaunch {
                program: vec!["false".into()],
                config_dir: None,
            },
        }),
        tool_home: Some(tool_home),
        ..config
    })
    .await;
    w.signals.watcher().unwrap().set_timing(Timing {
        poll: Duration::from_millis(50),
        missing_after: Duration::from_millis(300),
        quiet_after: Duration::from_millis(500),
    });
    w.add_project("shop-web").await;
    w.create("SHOP", "Add coupon codes").await;
    // Developer (the Dev stage's agent) runs on Codex.
    w.set_agent_model(
        "Developer",
        AgentModel::Tool {
            tool: Tool::Codex,
            model: None,
        },
    )
    .await;
    w.board
        .move_ticket("SHOP-1", DropTarget::ReadyForDev, true, &me())
        .await
        .unwrap();
    World { w, _bin: bin, home }
}

async fn until<F: std::future::Future<Output = bool>>(what: &str, mut done: impl FnMut() -> F) {
    let reached = tokio::time::timeout(Duration::from_secs(20), async {
        while !done().await {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await;
    assert!(reached.is_ok(), "never: {what}");
}

/// Starts SHOP-1 and returns its agent's caller, once Codex printed its arguments.
async fn started(w: &TestService) -> Caller {
    w.starter
        .start("SHOP-1", &me(), otaq_core::session::Start::Auto)
        .await
        .unwrap();
    until("SHOP-1 works", || async {
        w.status("SHOP-1").await == Status::Working
    })
    .await;
    let ticket = w.board.ticket("SHOP-1").await.unwrap();
    let session = w
        .sessions
        .running_of(&ticket.id, SessionKind::Agent)
        .unwrap();
    w.wait_for_output(&session.id, "ARGS").await;
    Caller::Agent {
        ticket: ticket.key,
        ticket_id: ticket.id,
        session: session.id,
    }
}

fn session_of(caller: &Caller) -> &str {
    match caller {
        Caller::Agent { session, .. } | Caller::Validator { session, .. } => session,
        other => panic!("{other:?}"),
    }
}

/// Codex's `SessionStart` hook: its conversation id and model.
async fn session_start(w: &TestService, caller: &Caller) {
    let payload = json!({"session_id": ID, "hook_event_name": "SessionStart", "model": "gpt-6-luna",
        "source": "startup", "permission_mode": "default"});
    w.signals
        .hook(caller, Tool::Codex, "SessionStart", &payload)
        .await
        .unwrap();
}

async fn windows(w: &TestService) -> Vec<(String, f64)> {
    plan::windows(w.board.store(), Tool::Codex)
        .await
        .unwrap()
        .into_iter()
        .map(|w| (w.window.key, w.window.used_percent))
        .collect()
}

async fn notes(client: &Client) -> Vec<String> {
    match client.request(Request::AgentToolsList).await.unwrap() {
        Reply::AgentTools(list) => list
            .tools
            .iter()
            .find(|t| t.tool == Tool::Codex)
            .unwrap()
            .notes
            .clone(),
        other => panic!("{other:?}"),
    }
}

/// The watcher starts on the tool id, waits for half a line, replaces the windows per report, gives the limit
/// with its reset and stops with the session; the tokens are read from the same file.
#[tokio::test(flavor = "multi_thread")]
async fn the_watcher_reads_whole_lines_from_the_tool_id_to_the_end_of_the_session() {
    let world = world().await;
    let w = &world.w;
    let caller = started(w).await;
    let session = session_of(&caller).to_owned();
    let watcher = w.signals.watcher().unwrap().clone();
    let file = world.create_file();
    assert!(!watcher.is_watching(&session), "not before the id");

    session_start(w, &caller).await;
    assert!(watcher.is_watching(&session));
    assert_eq!(
        w.sessions
            .info(&session)
            .unwrap()
            .tool_session_id
            .as_deref(),
        Some(ID)
    );

    // Half a line waits for the rest.
    let counts = saved_lines("turns.jsonl", "token_count");
    let (head, tail) = counts[0].split_at(60);
    append(&file, head);
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(windows(w).await, []);
    append(&file, &format!("{tail}\n"));
    until("the weekly window", || async {
        windows(w).await == [("weekly".to_owned(), 0.0)]
    })
    .await;

    // A later report replaces the set: Pro's 5-hour and weekly windows.
    let pro = json!({"timestamp": "2026-10-18T09:00:00.000Z", "ordinal": 30, "type": "event_msg",
        "payload": {"type": "token_count", "info": null, "rate_limits": {
            "primary": {"used_percent": 40.0, "window_minutes": 300, "resets_at": 1_892_807_562_i64},
            "secondary": {"used_percent": 7.0, "window_minutes": 10080, "resets_at": 1_892_807_562_i64}}}});
    append(&file, &format!("{pro}\n"));
    until("two windows", || async {
        windows(w).await == [("session".to_owned(), 40.0), ("weekly".to_owned(), 7.0)]
    })
    .await;
    for line in &counts[1..] {
        append(&file, &format!("{line}\n"));
    }

    // The usage limit: Waiting for limit reset, and Codex's limit with the reset in its message.
    for line in saved_lines("limit.jsonl", "token_count")
        .into_iter()
        .chain(saved_lines("limit.jsonl", "task_complete"))
    {
        append(&file, &format!("{line}\n"));
    }
    until("the limit", || async {
        w.status("SHOP-1").await == Status::WaitingForLimitReset
    })
    .await;
    // The limit row is written just after the status, as for every tool's limit (the service's after-signal step).
    until("the limit row", || async {
        plan::limit_of(w.board.store(), Tool::Codex)
            .await
            .unwrap()
            .is_some()
    })
    .await;
    let limit = plan::limit_of(w.board.store(), Tool::Codex)
        .await
        .unwrap()
        .unwrap();
    assert!(limit.text.starts_with("You've hit your usage limit."));
    assert_eq!(limit.reset_source, Some(ResetSource::LimitText));
    assert!(limit.resets_at.is_some());

    // Tokens: every count, cached input apart.
    w.usage.read_now().await.unwrap();
    let ticket = w.board.ticket("SHOP-1").await.unwrap();
    let tokens = w.usage.sessions_of(&ticket.id).await.unwrap()[0]
        .tokens
        .unwrap();
    assert_eq!(
        (tokens.input, tokens.cached, tokens.output, tokens.reasoning),
        (
            153_719 - 136_320 + 15_603,
            136_320,
            300 - 41 + 124,
            Some(41)
        )
    );

    w.sessions.stop(&session).await.unwrap();
    until("the watch stops", || async {
        !watcher.is_watching(&session)
    })
    .await;
    world.w.stop().await;
}

/// A Developer's first turn on a model Codex refuses (the TUI's captured lines): Start failed at the model step,
/// as on Claude Code (M22).
#[tokio::test(flavor = "multi_thread")]
async fn a_refused_model_on_the_first_turn_is_start_failed() {
    let world = world().await;
    let w = &world.w;
    let caller = started(w).await;
    let file = world.create_file();
    session_start(w, &caller).await;
    for line in saved_lines("model_refused.jsonl", "event_msg") {
        append(&file, &format!("{line}\n"));
    }
    until("Start failed", || async {
        w.status("SHOP-1").await == Status::StartInterrupted
    })
    .await;
    let problem = w
        .board
        .ticket("SHOP-1")
        .await
        .unwrap()
        .start_problem
        .unwrap();
    assert_eq!(problem.step, StartStep::Model);
    assert_eq!(
        problem.message,
        "The 'no-such-model-xyz' model is not supported when using Codex with a ChatGPT account."
    );
    world.w.stop().await;
}

/// A validator on Codex whose model is refused: No result, not a round.
#[tokio::test(flavor = "multi_thread")]
async fn a_validators_refused_model_is_no_result() {
    let world = world().await;
    let w = &world.w;
    let ticket = w.create("SHOP", "Coupon field").await;
    w.give_worktree(&ticket).await;
    let validator = w.shell(&ticket, SessionKind::Validator).await;
    w.validating_with(&ticket, &validator.session).await;
    let caller = Caller::Validator {
        ticket: ticket.key.clone(),
        ticket_id: ticket.id.clone(),
        session: validator.session.clone(),
    };
    let file = world.create_file();
    session_start(w, &caller).await;
    for line in saved_lines("model_refused.jsonl", "event_msg") {
        append(&file, &format!("{line}\n"));
    }
    until("No result", || async {
        w.status("SHOP-2").await == Status::CheckNoResult
    })
    .await;
    assert_eq!(w.board.ticket("SHOP-2").await.unwrap().validation_round, 0);
    world.w.stop().await;
}

/// No file, or a line Codex's adapter can't read: the note "limits not reported", and the quiet agent's card
/// falls back to Idle; a readable line clears the note.
#[tokio::test(flavor = "multi_thread")]
async fn an_unreadable_file_sets_the_note_and_a_quiet_agent_goes_idle() {
    let world = world().await;
    let w = &world.w;
    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    let caller = started(w).await;
    session_start(w, &caller).await;
    until("the note", || async {
        notes(&client).await == [LIMITS_NOT_REPORTED]
    })
    .await;
    until("Idle once quiet", || async {
        w.status("SHOP-1").await == Status::Idle
    })
    .await;

    let file = world.create_file();
    let count = saved_lines("turns.jsonl", "token_count").remove(0);
    append(&file, &format!("{count}\n"));
    until("the note clears", || async {
        notes(&client).await.is_empty()
    })
    .await;

    append(&file, "{\"not\": \"a record\"}\n");
    until("the note again", || async {
        notes(&client).await == [LIMITS_NOT_REPORTED]
    })
    .await;
    world.w.stop().await;
}

/// After a restart of the service's watches, a running session is watched again from its file's end: what was
/// written before isn't applied again.
#[tokio::test(flavor = "multi_thread")]
async fn running_sessions_are_watched_again_from_the_end() {
    let world = world().await;
    let w = &world.w;
    let caller = started(w).await;
    let session = session_of(&caller).to_owned();
    let file = world.create_file();
    // The id was stored before (the service that captured it is gone); the file already has the limit.
    w.signals.tool_id_known(&session, ID).await.unwrap();
    for line in saved_lines("limit.jsonl", "event_msg") {
        append(&file, &format!("{line}\n"));
    }
    let watcher = w.signals.watcher().unwrap();
    assert!(!watcher.is_watching(&session));

    w.signals.rewatch_running();
    assert!(watcher.is_watching(&session));
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(w.status("SHOP-1").await, Status::Working, "old lines");
    assert_eq!(windows(w).await, []);

    let count = saved_lines("turns.jsonl", "token_count").remove(0);
    append(&file, &format!("{count}\n"));
    until("a new line is read", || async {
        windows(w).await == [("weekly".to_owned(), 0.0)]
    })
    .await;
    world.w.stop().await;
}

/// A Codex Project agent's own row keeps the id Codex reports, so its Resume continues the conversation.
#[tokio::test(flavor = "multi_thread")]
async fn a_project_agents_row_keeps_the_reported_id() {
    let world = world().await;
    let w = &world.w;
    let project = w.board.project("SHOP").await.unwrap();
    let pool = w.board.store().pool();
    sqlx::query(
        "INSERT INTO project_agents (id, project_id, tool, state, created_at)
         VALUES ('pa-1', ?, 'codex', 'running', 1)",
    )
    .bind(&project.id)
    .execute(pool)
    .await
    .unwrap();
    let shell = w.create("SHOP", "stand-in").await;
    let session = w.agent_shell(&shell).await.session;
    let caller = Caller::ProjectAgent {
        project: "SHOP".into(),
        agent: "pa-1".into(),
        session,
    };
    session_start(w, &caller).await;
    let kept: Option<String> =
        sqlx::query_scalar("SELECT tool_session_id FROM project_agents WHERE id = 'pa-1'")
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(kept.as_deref(), Some(ID));
    // A later report never replaces it.
    let other = json!({"session_id": "01a1-other", "model": "gpt-6-luna"});
    w.signals
        .hook(&caller, Tool::Codex, "SessionStart", &other)
        .await
        .unwrap();
    let kept: Option<String> =
        sqlx::query_scalar("SELECT tool_session_id FROM project_agents WHERE id = 'pa-1'")
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(kept.as_deref(), Some(ID));
    world.w.stop().await;
}
