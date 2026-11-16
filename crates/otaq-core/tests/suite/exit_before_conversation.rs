//! A new conversation whose tool exits before the conversation exists is Start failed with the tool's own last words,
//! not Interrupted ("Stopped by a restart") (F-E2-1 b): Copilot refusing an argument at start (it makes no
//! `session-state/<id>/`), Claude Code exiting at start (no transcript). A tool that exits after its conversation
//! exists still interrupts its ticket (d0018), a resume is never taken for a failed start (REC-03), a later Agent stage
//! keeps its place, and Retry start launches on the agent's current model.
//!
//! Every tool is a stand-in: `copilot` is an `sh` script in Otaq's bin folder that does what a control file says, and
//! `claude` is `sh`. Repos, `OTAQ_HOME` and the tools' home are temporary: no test starts a real tool. Unix only: the
//! stand-ins are `sh` scripts started by name, which Windows can't run (Q598).
#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::common::stand_in::plugin;
use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::agent_tools::{LaunchContext, Tool};
use otaq_core::agents::AgentModel;
use otaq_core::board::moves::DropTarget;
use otaq_core::board::tickets::EventKind;
use otaq_core::domain::{Actor, Place, StartStep, Status};
use otaq_core::flow::{FlowFile, yaml};
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::TestService;
use otaq_core::session::SessionKind;
use tempfile::TempDir;

/// What the stand-in Copilot prints when it refuses its arguments, as Copilot 1.0.112 refuses `--auto-tier
/// efficiency`.
const REFUSED: &str = "Error: The efficiency Auto profile is no longer available.";
const REFUSED_HOW: &str = "Set --auto-tier to balance or intelligence.";
/// A frame row under it, `╭─╮` in `printf`'s octal: never part of the message.
const FRAME: &str = "\\342\\225\\255\\342\\224\\200\\342\\225\\256";

fn me() -> Actor {
    Actor::User("me".into())
}

struct World {
    w: TestService,
    /// The stand-in Copilot's control file: `refuse` (exit at once, no conversation), `crash` (make the conversation,
    /// then exit), `lost` (exit with no conversation and no text Otaq knows), `notfound` (exit with Copilot's text
    /// for an unknown id), else make it and keep running.
    mode: PathBuf,
    /// Copilot's `session-state` folder in the tools' home.
    state: PathBuf,
    _dir: TempDir,
}

impl World {
    fn set_mode(&self, mode: &str) {
        std::fs::write(&self.mode, mode).unwrap();
    }
}

/// A service whose `copilot` is the stand-in, with project SHOP and ticket SHOP-1 whose Developer runs on Copilot,
/// waiting to begin.
async fn world(mode: &str) -> World {
    let dir = TempDir::new().unwrap();
    let bin = dir.path().join("bin");
    let home = dir.path().join("home");
    let state = home.join(".copilot").join("session-state");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::create_dir_all(&state).unwrap();
    let control = dir.path().join("mode");
    std::fs::write(&control, mode).unwrap();
    let copilot = bin.join(Tool::Copilot.command_name());
    let script = format!(
        "#!/bin/sh\n\
         case \"$1\" in --session-id|--resume) id=\"$2\" ;; *) exit 0 ;; esac\n\
         printf 'ARGS %s\\n' \"$*\"\n\
         case \"$(cat '{control}')\" in\n\
         refuse) printf '{REFUSED}\\n{REFUSED_HOW}\\n{FRAME}\\n'; sleep 1; exit 1 ;;\n\
         crash) mkdir -p '{state}'/\"$id\"; printf 'crashed mid-work\\n'; sleep 1; exit 1 ;;\n\
         lost) printf 'went away\\n'; sleep 1; exit 1 ;;\n\
         notfound) printf 'Error: No session, task, or name matched %s\\n' \"$id\"; sleep 1; exit 1 ;;\n\
         *) mkdir -p '{state}'/\"$id\"; exec cat ;;\n\
         esac\n",
        control = control.display(),
        state = state.display(),
    );
    std::fs::write(&copilot, script).unwrap();
    std::fs::set_permissions(&copilot, std::fs::Permissions::from_mode(0o755)).unwrap();
    let plugin_dir = plugin(dir.path());
    let w = TestService::start_with(move |config| ServiceConfig {
        tool_home: Some(home),
        launch: Some(LaunchContext {
            bin_dir: bin,
            plugin_dir,
            path: std::env::var_os("PATH").unwrap_or_default(),
            claude: ClaudeLaunch {
                program: vec!["false".into()],
                config_dir: None,
            },
        }),
        ..config
    })
    .await;
    w.add_project("shop-web").await;
    w.create("SHOP", "Coupon codes").await;
    w.set_agent_model("Developer", copilot_on(None)).await;
    w.board
        .move_ticket("SHOP-1", DropTarget::ReadyForDev, true, &me())
        .await
        .unwrap();
    World {
        w,
        mode: control,
        state,
        _dir: dir,
    }
}

fn copilot_on(model: Option<&str>) -> AgentModel {
    AgentModel::Tool {
        tool: Tool::Copilot,
        model: model.map(str::to_owned),
    }
}

async fn until_status(w: &TestService, key: &str, want: Status) {
    let reached = tokio::time::timeout(Duration::from_secs(20), async {
        while w.status(key).await != want {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await;
    if reached.is_err() {
        let t = w.board.ticket(key).await.unwrap();
        panic!(
            "{key} never became {want}: {} {:?}",
            t.status, t.start_problem
        );
    }
}

/// Starts SHOP-1 and returns its agent session's output once the stand-in printed its arguments.
async fn started(w: &TestService) -> String {
    w.starter
        .start("SHOP-1", &me(), otaq_core::session::Start::Auto)
        .await
        .unwrap();
    let ticket = w.board.ticket("SHOP-1").await.unwrap();
    let session = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            if let Some(s) = w.sessions.latest_of(&ticket.id, SessionKind::Agent) {
                return s;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the agent ran");
    assert_eq!(session.tool, Some(Tool::Copilot));
    w.wait_for_output(&session.id, "ARGS").await
}

/// Copilot refusing its arguments at start: Start failed at the launch with Copilot's own words (its frame rows
/// left out), not Interrupted. With the agent's model changed, Retry start launches on the new model.
#[tokio::test(flavor = "multi_thread")]
async fn a_copilot_that_exits_before_its_conversation_is_start_failed_with_its_words() {
    let w = world("refuse").await;
    started(&w.w).await;
    until_status(&w.w, "SHOP-1", Status::StartInterrupted).await;
    let ticket = w.w.board.ticket("SHOP-1").await.unwrap();
    let problem = ticket.start_problem.unwrap();
    assert_eq!(problem.step, StartStep::Launch);
    assert!(
        problem.message.starts_with(
            "GitHub Copilot stopped before its conversation started, so there is nothing to resume. It said:\n"
        ),
        "{}",
        problem.message
    );
    assert!(
        problem
            .message
            .ends_with(&format!("\n{REFUSED}\n{REFUSED_HOW}")),
        "{}",
        problem.message
    );
    assert_eq!(ticket.place, Place::Waiting(None), "waiting to begin");
    let history = w.w.board.history("SHOP-1").await.unwrap();
    assert!(
        history.iter().all(|e| e.kind != EventKind::Interrupted),
        "not a restart"
    );
    let failed = history
        .iter()
        .find(|e| e.kind == EventKind::StartInterrupted)
        .unwrap();
    assert_eq!(failed.payload["message"], problem.message.as_str());

    // The user picks another model; Retry start launches a new conversation on it.
    w.w.set_agent_model("Developer", copilot_on(Some("gpt-9")))
        .await;
    w.set_mode("run");
    w.w.starter.retry("SHOP-1", &me()).await.unwrap();
    let out = started(&w.w).await;
    until_status(&w.w, "SHOP-1", Status::Working).await;
    assert!(out.contains("--model gpt-9"), "{out}");
    assert!(out.contains("--session-id"), "a new conversation: {out}");
    w.w.stop().await;
}

/// A Copilot whose conversation exists when it exits (a crash mid-work) still interrupts its ticket (d0018), and
/// its Resume, which then ends with no conversation and no text Otaq knows, stays Interrupted as before: a resume is
/// never taken for a failed start (REC-03).
#[tokio::test(flavor = "multi_thread")]
async fn an_existing_conversation_interrupts_and_a_resume_is_never_a_failed_start() {
    let w = world("crash").await;
    started(&w.w).await;
    until_status(&w.w, "SHOP-1", Status::Interrupted).await;
    assert_eq!(
        w.w.board.ticket("SHOP-1").await.unwrap().start_problem,
        None
    );

    // The conversation is gone by the Resume, and Copilot says nothing Otaq recognises.
    std::fs::remove_dir_all(&w.state).unwrap();
    w.set_mode("lost");
    w.w.review.resume("SHOP-1", &me()).await.unwrap();
    let ticket = w.w.board.ticket("SHOP-1").await.unwrap();
    let resumed = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            if let Some(s) = w.w.sessions.running_of(&ticket.id, SessionKind::Agent) {
                return s;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the resume ran");
    let out = w.w.wait_for_output(&resumed.id, "went away").await;
    assert!(out.contains("--resume"), "{out}");
    until_status(&w.w, "SHOP-1", Status::Interrupted).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    let after = w.w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(
        (after.status, after.start_problem),
        (Status::Interrupted, None)
    );
    w.w.stop().await;
}

/// Copilot's own "conversation not found" text on a resume is still Resume failed (REC-03).
#[tokio::test(flavor = "multi_thread")]
async fn a_resume_copilot_cant_find_stays_resume_failed() {
    let w = world("crash").await;
    started(&w.w).await;
    until_status(&w.w, "SHOP-1", Status::Interrupted).await;
    std::fs::remove_dir_all(&w.state).unwrap();
    w.set_mode("notfound");
    w.w.review.resume("SHOP-1", &me()).await.unwrap();
    until_status(&w.w, "SHOP-1", Status::ResumeFailed).await;
    assert_eq!(
        w.w.board.ticket("SHOP-1").await.unwrap().start_problem,
        None
    );
    w.w.stop().await;
}

/// Only Claude Code and Copilot are asked whether a new conversation came to exist: an OpenCode or Codex session
/// that ends on its own keeps today's way (OpenCode's check runs `opencode` and can fail for other reasons, so a real
/// conversation would lose its resume; Codex's id-after-start rule covers it).
#[tokio::test(flavor = "multi_thread")]
async fn only_claude_code_and_copilot_check_a_new_conversation() {
    let w = TestService::start().await;
    let checks = |tool: Tool| w.adapters.get(tool).checks_new_conversation();
    assert!(checks(Tool::Claude) && checks(Tool::Copilot));
    assert!(!checks(Tool::OpenCode) && !checks(Tool::Codex));
    w.stop().await;
}

/// `claude` as an `sh` running `script`.
fn claude_stand_in(script: &str) -> LaunchContext {
    LaunchContext {
        bin_dir: "/nonexistent".into(),
        plugin_dir: "/nonexistent/plugin".into(),
        path: std::env::var_os("PATH").unwrap_or_default(),
        claude: ClaudeLaunch {
            program: vec!["sh".into(), "-c".into(), script.into(), "sh".into()],
            config_dir: None,
        },
    }
}

fn every_kind_file() -> FlowFile {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/flows/every-kind.yaml");
    yaml::read(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// A Claude Code that exits at the start of a later Agent stage (Dev, after Product spec), with no conversation
/// saved: Start failed, waiting again before Dev (as a refused model's start), not before the flow's first stage.
#[tokio::test(flavor = "multi_thread")]
async fn a_failed_start_of_a_later_agent_stage_waits_before_that_stage() {
    let w = TestService::start_with(|config| ServiceConfig {
        launch: Some(claude_stand_in(
            "printf 'Error: unknown option --bogus\\n'; sleep 1; exit 1",
        )),
        ..config
    })
    .await;
    w.add_project("shop-web").await;
    w.board.use_flow_for_tests("SHOP", &every_kind_file()).await;
    let t = w.create("SHOP", "Sort notes by date").await;
    // Product spec finished: waiting before Dev.
    sqlx::query("UPDATE tickets SET stage_id = 'dev', state = ? WHERE id = ?")
        .bind(Status::Queued.as_str())
        .bind(&t.id)
        .execute(w.board.store().pool())
        .await
        .unwrap();
    w.starter
        .start("SHOP-1", &me(), otaq_core::session::Start::Auto)
        .await
        .unwrap();
    until_status(&w, "SHOP-1", Status::StartInterrupted).await;
    let ticket = w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(ticket.place, Place::Waiting(Some("dev".into())));
    let problem = ticket.start_problem.unwrap();
    assert_eq!(problem.step, StartStep::Launch);
    assert!(
        problem
            .message
            .starts_with("Claude Code stopped before its conversation started")
            && problem.message.ends_with("\nError: unknown option --bogus"),
        "{}",
        problem.message
    );
    w.stop().await;
}
