//! Hand-offs reach the agent (T-0487; d0118, d0092, d0098, d0048): a ticket's agent session starts with every
//! hand-off skill in its worktree (`otaq-address-review`, `otaq-restack`, the Delivery's Open PR and Push skills), so
//! Send back, a merge conflict and Open PR typed into it run a skill its tool loaded at start. A skill it didn't start
//! with is named by its file, never typed as a command. A gone session gets the hand-off as its first prompt.
//!
//! Every tool is the stand-in of `tests/fixtures/stand-in` (`crate::common::stand_in`): it reads its skill folders
//! only at start and prints `Unknown command` for a skill it didn't load, as the real tools do (the race of finding
//! 19). Repos, `OTAQ_HOME` and the tools' home are temporary: no test starts a real tool.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::common::stand_in;
use otaq_core::agent_tools::Tool;
use otaq_core::agents::AgentModel;
use otaq_core::board::moves::DropTarget;
use otaq_core::board::settings::WorkflowStep;
use otaq_core::board::tickets::Ticket;
use otaq_core::caller::Caller;
use otaq_core::domain::{Actor, Status};
use otaq_core::git::testing::{commit, git};
use otaq_core::pr::service::PrOutcome;
use otaq_core::review::LocalMergeOutcome;
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::{TestService, said};
use otaq_core::session::deliveries::handoff_sentence;
use otaq_core::session::{SessionInfo, SessionKind, Start};
use tempfile::TempDir;

/// Every hand-off skill and the Developer's own, as the stand-in lists what it loaded.
const LOADED: &str = "loaded: otaq-address-review otaq-commit otaq-open-pr otaq-push otaq-restack otaq-work-ticket \r";

fn me() -> Actor {
    Actor::User("me".into())
}

struct World {
    w: TestService,
    repo: PathBuf,
    _dir: TempDir,
}

/// A service whose four tools are the stand-in, with shop-web (a local origin).
async fn world() -> World {
    let dir = TempDir::new().unwrap();
    let launch = stand_in::launch(&dir.path().join("bin"), dir.path());
    let w = TestService::start_with(move |config| ServiceConfig {
        launch: Some(launch),
        ..config
    })
    .await;
    let repo = w.repos.clone_with_origin("shop-web", "main", 1);
    w.board
        .add_project(&otaq_core::board::projects::AddProject {
            path: repo.display().to_string(),
            ..Default::default()
        })
        .await
        .unwrap();
    World { w, repo, _dir: dir }
}

fn worktree_of(t: &Ticket) -> PathBuf {
    PathBuf::from(t.work.as_ref().unwrap().worktree.clone().unwrap())
}

/// Starts a new ticket on `tool` from Ready for Dev; returns it and its agent session once the stand-in said what
/// it loaded.
async fn started(w: &TestService, tool: Tool, title: &str) -> (Ticket, SessionInfo) {
    w.set_agent_model("Developer", AgentModel::Tool { tool, model: None })
        .await;
    let t = w.create("SHOP", title).await;
    let key = t.key.to_string();
    w.board
        .move_ticket(&key, DropTarget::ReadyForDev, true, &me())
        .await
        .unwrap();
    w.starter.start(&key, &me(), Start::Auto).await.unwrap();
    let session = running(w, &t).await;
    w.wait_for_output(&session.id, "STAND-IN").await;
    (w.board.ticket(&key).await.unwrap(), session)
}

async fn running(w: &TestService, t: &Ticket) -> SessionInfo {
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            if let Some(s) = w.sessions.running_of(&t.id, SessionKind::Agent) {
                return s;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("{}'s agent never ran", t.key))
}

/// The started ticket with a commit of its own on `file`, submitted: waiting in Review with its session live.
async fn in_review(w: &TestService, t: &Ticket, file: &str) {
    commit(&worktree_of(t), file, "redirect\n");
    w.force_status(t, Status::Working).await;
    let key = t.key.to_string();
    w.board
        .submit_for_review(
            &key,
            said(&w.board, &key, "Redirects after login.")
                .await
                .as_ref(),
            &me(),
        )
        .await
        .unwrap();
}

/// The session's output so far.
fn output(w: &TestService, session: &str) -> String {
    String::from_utf8_lossy(&w.sessions.attach(session).unwrap().scrollback).into_owned()
}

/// The agent's turn ended (Claude Code's Stop hook, its real shape): the session is idle between turns.
async fn turn_ended(w: &TestService, t: &Ticket, session: &str) {
    let me = Caller::Agent {
        ticket: t.key.clone(),
        ticket_id: t.id.clone(),
        session: session.to_owned(),
    };
    let payload: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/claude/hooks/Stop-idle.json"),
        )
        .unwrap(),
    )
    .unwrap();
    w.signals
        .hook(&me, Tool::Claude, "Stop", &payload)
        .await
        .unwrap();
}

/// Send back on a live session, on each tool: the session started with `otaq-address-review` (with the other
/// hand-off skills, git sees none of them), so the line typed runs it and the stand-in never says `Unknown command`.
#[tokio::test(flavor = "multi_thread")]
async fn send_back_into_a_live_session_runs_address_review_on_each_tool() {
    let World { w, _dir, .. } = world().await;
    for (n, tool) in [Tool::Claude, Tool::Codex, Tool::Copilot, Tool::OpenCode]
        .into_iter()
        .enumerate()
    {
        let (t, session) = started(&w, tool, &format!("Login redirect {n}")).await;
        let out = w.wait_for_output(&session.id, "ARGS").await;
        assert!(out.contains(LOADED), "{tool:?}: {out}");
        let worktree = worktree_of(&t);
        assert_eq!(
            git(
                &worktree,
                &["status", "--porcelain", "--untracked-files=all"]
            ),
            "",
            "{tool:?}"
        );
        in_review(&w, &t, "login.ts").await;
        let key = t.key.to_string();
        w.review
            .fail(&key, "Keep the old URL working.", None, false, &me())
            .await
            .unwrap();
        let out = w
            .wait_for_output(&session.id, "RUNS otaq-address-review")
            .await;
        assert!(!out.contains("Unknown command"), "{tool:?}: {out}");
        assert_eq!(w.status(&key).await, Status::Working, "{tool:?}");
    }
    w.stop().await;
}

/// A merge conflict at Local merge goes back to the live session with restack, typed once its turn ended: the
/// session started with `otaq-restack`, so it runs.
#[tokio::test(flavor = "multi_thread")]
async fn a_merge_conflict_runs_restack_in_the_live_session() {
    let World { w, repo, _dir } = world().await;
    let (t, session) = started(&w, Tool::Claude, "Pricing").await;
    in_review(&w, &t, "lib.ts").await;
    commit(&repo, "lib.ts", "main\n");
    turn_ended(&w, &t, &session.id).await;
    let key = t.key.to_string();
    let merged = w.review.local_merge(&key, "m", false, &me()).await.unwrap();
    assert!(
        matches!(merged, LocalMergeOutcome::Conflict { .. }),
        "{merged:?}"
    );
    assert_eq!(w.status(&key).await, Status::MergeConflictFixing);
    let out = w.wait_for_output(&session.id, "RUNS otaq-restack").await;
    assert!(!out.contains("Unknown command"), "{out}");
    w.stop().await;
}

/// Open PR and Push to PR clicked into the live session that delivers (the Developer is the Delivery's agent): the
/// Delivery's imported Open PR skill (`my-pr`) and Otaq's `push` were copied at its start, so both run.
#[tokio::test(flavor = "multi_thread")]
async fn open_pr_and_push_run_in_the_live_session() {
    let World { w, _dir, .. } = world().await;
    let folder = otaq_core::skills::testing::skill_folder(w.data.root(), "my-pr");
    w.import_step_skill(WorkflowStep::OpenPr, &folder).await;
    let (t, session) = started(&w, Tool::Claude, "Login redirect").await;
    let out = w.wait_for_output(&session.id, "ARGS").await;
    assert!(out.contains("otaq-my-pr "), "{out}");
    in_review(&w, &t, "login.ts").await;
    let key = t.key.to_string();

    let outcome = w.prs.open(&key, false, false, &me()).await.unwrap();
    assert!(matches!(outcome, PrOutcome::Started { .. }), "{outcome:?}");
    w.wait_for_output(&session.id, "RUNS otaq-my-pr").await;
    w.prs.cancel(&key, &me()).await.unwrap();

    sqlx::query("UPDATE tickets SET pr_number = 481 WHERE id = ?")
        .bind(&t.id)
        .execute(w.board.store().pool())
        .await
        .unwrap();
    let outcome = w.prs.open(&key, true, false, &me()).await.unwrap();
    assert!(matches!(outcome, PrOutcome::Started { .. }), "{outcome:?}");
    let out = w.wait_for_output(&session.id, "RUNS otaq-push").await;
    assert!(!out.contains("Unknown command"), "{out}");
    w.stop().await;
}

/// A hand-off whose skill the session didn't start with (Open PR's skill imported while the agent ran): it is copied
/// and named by its file in the folder the tool reads (`.claude/skills/…` for Claude Code, `.agents/skills/…` for the
/// other three), never typed as a command; the stand-in follows it.
#[tokio::test(flavor = "multi_thread")]
async fn a_skill_the_session_didnt_start_with_is_named_by_its_file() {
    let World { w, _dir, .. } = world().await;
    for (n, (tool, root)) in [
        (Tool::Claude, ".claude/skills"),
        (Tool::Codex, ".agents/skills"),
        (Tool::Copilot, ".agents/skills"),
        (Tool::OpenCode, ".agents/skills"),
    ]
    .into_iter()
    .enumerate()
    {
        let (t, session) = started(&w, tool, &format!("Login redirect {n}")).await;
        let name = format!("team-pr-{n}");
        let folder = otaq_core::skills::testing::skill_folder(w.data.root(), &name);
        w.import_step_skill(WorkflowStep::OpenPr, &folder).await;
        in_review(&w, &t, "login.ts").await;
        let key = t.key.to_string();
        let outcome = w.prs.open(&key, false, false, &me()).await.unwrap();
        assert!(matches!(outcome, PrOutcome::Started { .. }), "{outcome:?}");
        let sentence = handoff_sentence(root, &name, None);
        assert_eq!(
            sentence,
            format!("Follow the skill in `{root}/otaq-team-pr-{n}/SKILL.md`.")
        );
        let out = w
            .wait_for_output(
                &session.id,
                &format!("FOLLOWS {root}/otaq-team-pr-{n}/SKILL.md"),
            )
            .await;
        assert!(out.contains(&sentence), "{tool:?}: {out}");
        assert!(!out.contains("Unknown command"), "{tool:?}: {out}");
        w.prs.cancel(&key, &me()).await.unwrap();
    }
    w.stop().await;
}

/// Send back to a ticket whose session is gone: the conversation is resumed with the hand-off as its first prompt,
/// the skill copied before the tool starts, as before d0118.
#[tokio::test(flavor = "multi_thread")]
async fn a_gone_session_gets_the_handoff_as_its_first_prompt() {
    let World { w, _dir, .. } = world().await;
    let (t, session) = started(&w, Tool::Claude, "Login redirect").await;
    in_review(&w, &t, "login.ts").await;
    w.sessions.stop(&session.id).await.unwrap();
    let key = t.key.to_string();
    w.review
        .fail(&key, "Keep the old URL working.", None, false, &me())
        .await
        .unwrap();
    let resumed = running(&w, &t).await;
    assert_ne!(resumed.id, session.id);
    let out = w
        .wait_for_output(&resumed.id, "RUNS otaq-address-review")
        .await;
    let args = out.lines().find(|l| l.starts_with("ARGS")).unwrap();
    assert!(args.contains("--resume"), "{args}");
    assert!(args.contains("/otaq-address-review "), "{args}");
    assert!(out.contains(LOADED), "{out}");
    assert!(!output(&w, &resumed.id).contains("Unknown command"));
    // A first prompt isn't typed in: nothing waits for the agent to pick it up (d0118).
    assert_eq!(w.board.ticket(&key).await.unwrap().flags.hand_off, None);
    w.stop().await;
}
