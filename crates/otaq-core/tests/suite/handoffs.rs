//! Hand-offs reach the agent (T-0487, T2; d0118, d0048): Otaq writes no skill into a worktree. Every one of Otaq's
//! own skills reaches every session from its start through Otaq's plugin, so Send back, a merge conflict and Open PR
//! typed into a live session run a skill its tool has (`/otaq:address-review`). A skill a stage names that isn't
//! Otaq's is typed by its own name; the tool finds it itself, or says it doesn't know it. A gone session gets the
//! hand-off as its first prompt.
//!
//! Every tool is the stand-in of `tests/fixtures/stand-in` (`crate::common::stand_in`): Otaq's skills always run
//! by their namespaced name, a project skill runs when it was in the folder at start, and any other answers `Unknown
//! command`, as the real tools do. Repos, `OTAQ_HOME` and the tools' home are temporary: no test starts a real tool.
//!
//! Unix only: the stand-in agent tools are `sh` scripts started by name, which Windows can't run (Q598).
#![cfg(unix)]

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
use otaq_core::session::{SessionInfo, SessionKind, Start};
use otaq_core::skills::testing::claude_skill;
use tempfile::TempDir;

/// What the stand-in found in the worktree at start: no skill, since Otaq writes none there.
const LOADED: &str = "loaded: \r";

/// What the stand-in prints when it runs Otaq's skill `name` as `tool` names it (OpenCode has no namespace).
fn runs_own(tool: Tool, name: &str) -> String {
    match tool {
        Tool::OpenCode => format!("RUNS {name}"),
        _ => format!("RUNS {}:{name}", otaq_core::naming::PLUGIN_NAMESPACE),
    }
}

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

/// Send back on a live session, on each tool: nothing was written into the worktree, and the line typed runs Otaq's
/// `address-review` from the plugin; the stand-in never says `Unknown command`.
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
        // A line comment: a code review, so the skill goes (d0131 Q514).
        crate::common::line_comment(&w.board, &key).await;
        w.review
            .fail(&key, "Keep the old URL working.", Vec::new(), false, &me())
            .await
            .unwrap();
        let out = w
            .wait_for_output(&session.id, &runs_own(tool, "address-review"))
            .await;
        assert!(!out.contains("Unknown command"), "{tool:?}: {out}");
        assert_eq!(w.status(&key).await, Status::Working, "{tool:?}");
        assert!(!worktree.join(".agents").exists(), "{tool:?}");
        assert!(!worktree.join(".claude").exists(), "{tool:?}");
    }
    w.stop().await;
}

/// A merge conflict at Local merge goes back to the live session with restack, typed once its turn ended: Otaq's
/// `restack` is in every session, so it runs.
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
    let out = w
        .wait_for_output(&session.id, &runs_own(Tool::Claude, "restack"))
        .await;
    assert!(!out.contains("Unknown command"), "{out}");
    w.stop().await;
}

/// Open PR and Push to PR clicked into the live session that delivers (the Developer is the Delivery's agent): the
/// Delivery stage names the team's `my-pr`, a project skill committed to the repository (so git puts it in the
/// worktree), and it is typed by its own name; Push to PR types Otaq's `push`. Both run.
#[tokio::test(flavor = "multi_thread")]
async fn open_pr_and_push_run_in_the_live_session() {
    let World { w, repo, _dir } = world().await;
    std::fs::create_dir_all(repo.join(".claude/skills/my-pr")).unwrap();
    commit(
        &repo,
        ".claude/skills/my-pr/SKILL.md",
        "---\nname: my-pr\ndescription: Opens the team's PR.\n---\nOpen the PR.\n",
    );
    git(&repo, &["push", "-q", "origin", "main"]);
    w.name_stage_skill(WorkflowStep::OpenPr, Some("my-pr"))
        .await;
    let (t, session) = started(&w, Tool::Claude, "Login redirect").await;
    let out = w.wait_for_output(&session.id, "ARGS").await;
    assert!(out.contains("loaded: my-pr "), "{out}");
    in_review(&w, &t, "login.ts").await;
    let key = t.key.to_string();

    let outcome = w.prs.open(&key, false, false, &me()).await.unwrap();
    assert!(matches!(outcome, PrOutcome::Started { .. }), "{outcome:?}");
    w.wait_for_output(&session.id, "RUNS my-pr").await;
    w.prs.cancel(&key, &me()).await.unwrap();

    sqlx::query("UPDATE tickets SET pr_number = 481 WHERE id = ?")
        .bind(&t.id)
        .execute(w.board.store().pool())
        .await
        .unwrap();
    let outcome = w.prs.open(&key, true, false, &me()).await.unwrap();
    assert!(matches!(outcome, PrOutcome::Started { .. }), "{outcome:?}");
    let out = w
        .wait_for_output(&session.id, &runs_own(Tool::Claude, "push"))
        .await;
    assert!(!out.contains("Unknown command"), "{out}");
    w.stop().await;
}

/// A skill runs only on a tool that has it: Open PR into the live Dev session, with a Delivery naming a skill nobody
/// has, types nothing. The click is refused with "<tool> doesn't have the skill `/team-pr-0`", no PR flow is under
/// way, and the session never sees the skill's name, on every tool.
#[tokio::test(flavor = "multi_thread")]
async fn a_skill_the_sessions_tool_doesnt_have_is_never_typed() {
    let World { w, _dir, .. } = world().await;
    for (n, tool) in [Tool::Claude, Tool::Codex, Tool::Copilot, Tool::OpenCode]
        .into_iter()
        .enumerate()
    {
        let (t, session) = started(&w, tool, &format!("Login redirect {n}")).await;
        let name = format!("team-pr-{n}");
        w.name_stage_skill(WorkflowStep::OpenPr, Some(&name)).await;
        in_review(&w, &t, "login.ts").await;
        let key = t.key.to_string();
        let refused = w.prs.open(&key, false, false, &me()).await.unwrap_err();
        assert_eq!(
            refused.to_string(),
            otaq_core::flow::proposal::missing_skill_text(tool, &name),
            "{tool}"
        );
        assert!(w.board.pr_flow(&t.id).await.unwrap().is_none(), "{tool}");
        tokio::time::sleep(Duration::from_millis(200)).await;
        let typed = w.adapters.get(tool).skill_reference(&name);
        let screen = w.wait_for_output(&session.id, "ARGS").await;
        assert!(!screen.contains(&typed), "{tool}: {screen}");
    }
    w.stop().await;
}

/// Send back to a ticket whose session is gone: the conversation is resumed with the hand-off as its first prompt,
/// Otaq's `address-review` by its namespaced name, nothing written into the worktree.
#[tokio::test(flavor = "multi_thread")]
async fn a_gone_session_gets_the_handoff_as_its_first_prompt() {
    let World { w, _dir, .. } = world().await;
    let (t, session) = started(&w, Tool::Claude, "Login redirect").await;
    in_review(&w, &t, "login.ts").await;
    w.sessions.stop(&session.id).await.unwrap();
    let key = t.key.to_string();
    crate::common::line_comment(&w.board, &key).await;
    w.review
        .fail(&key, "Keep the old URL working.", Vec::new(), false, &me())
        .await
        .unwrap();
    let resumed = running(&w, &t).await;
    assert_ne!(resumed.id, session.id);
    let out = w
        .wait_for_output(&resumed.id, &runs_own(Tool::Claude, "address-review"))
        .await;
    let args = out.lines().find(|l| l.starts_with("ARGS")).unwrap();
    assert!(args.contains("--resume"), "{args}");
    assert!(
        args.contains(&format!("{} ", claude_skill("address-review"))),
        "{args}"
    );
    assert!(out.contains(LOADED), "{out}");
    assert!(!output(&w, &resumed.id).contains("Unknown command"));
    // A first prompt isn't typed in: nothing waits for the agent to pick it up (d0118).
    assert_eq!(w.board.ticket(&key).await.unwrap().flags.hand_off, None);
    w.stop().await;
}
