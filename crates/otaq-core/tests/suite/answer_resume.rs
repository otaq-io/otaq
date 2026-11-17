//! Finished tickets answer automatically (T-0152; MSG-03, MSG-04; d0030, d0018): the worktree is recreated at
//! the same path on the kept branch, the stored conversation is resumed answer-only (an ordinary command stands in
//! for `claude`), and the session ends after the answer; the worktree goes unless files changed. The plan limit
//! holds the question; a conversation that is gone refuses the ask.

use std::path::{Path, PathBuf};

use otaq_core::agent_tools::LaunchContext;
use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::agent_tools::claude::transcript::project_folder;
use otaq_core::agents::pause::PauseScope;
use otaq_core::board::tickets::Ticket;
use otaq_core::domain::{Actor, Status};
use otaq_core::git::testing::{commit, git};
use otaq_core::messages::{MessageState, Route};
use otaq_core::naming;
use otaq_core::service::testing::TestService;
use otaq_core::session::SessionKind;
use otaq_core::skills::testing::claude_skill;
use tempfile::TempDir;

const CONVERSATION: &str = "0199aaaa-bbbb-7ccc-8ddd-eeeeffff0000";

struct World {
    w: TestService,
    _claude: TempDir,
    parent: Ticket,
    child: Ticket,
    worktree: PathBuf,
}

fn agent(key: &str) -> Actor {
    Actor::Agent(key.parse().unwrap())
}

/// SHOP-1 merged and cleaned up (its conversation on disk), SHOP-2 depending on it and working.
async fn world(limit: bool) -> World {
    world_on(limit, None).await
}

/// [`world`], with `bin` first on the tools' `PATH` (stand-ins for the tools Otaq runs).
async fn world_on(limit: bool, bin: Option<&Path>) -> World {
    let claude = TempDir::new().unwrap();
    let claude_dir = claude.path().to_path_buf();
    let bin = bin.map(Path::to_path_buf);
    let w = TestService::start_with(move |mut c| {
        if let Some(bin) = bin {
            let mut path = std::ffi::OsString::from(bin);
            path.push(":");
            path.push(&c.tools.path);
            c.tools.path = path;
        }
        c.launch = Some(LaunchContext {
            bin_dir: PathBuf::from("/nonexistent"),
            plugin_dir: PathBuf::from("/nonexistent/plugin"),
            path: std::env::var_os("PATH").unwrap_or_default(),
            claude: ClaudeLaunch {
                program: vec![
                    "sh".into(),
                    "-c".into(),
                    "echo \"answer-session: $* in $(pwd)\"; exec cat".into(),
                    "sh".into(),
                ],
                config_dir: None,
            },
        });
        c.tools.claude_config_dir = Some(claude_dir);
        c
    })
    .await;
    w.add_project("shop-web").await;
    let parent = w.create("SHOP", "Checkout API").await;
    let child = w.create("SHOP", "Checkout page").await;
    w.board
        .add_dependency("SHOP-2", "SHOP-1", &Actor::User("me".into()))
        .await
        .unwrap();
    let worktree = w.start_worktree(&parent).await;
    commit(&worktree, "api.ts", "checkout\n");
    w.record_ended_agent(&parent, CONVERSATION).await;
    let folder = claude
        .path()
        .join("projects")
        .join(project_folder(&worktree));
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join(format!("{CONVERSATION}.jsonl")), "{}\n").unwrap();
    w.force_status(&parent, Status::Merged).await;
    let parent = w.board.ticket("SHOP-1").await.unwrap();
    w.review.finish(&parent).await.unwrap();
    assert!(!worktree.exists(), "Done cleanup removed it");
    w.force_status(&child, Status::Working).await;
    if limit {
        sqlx::query(
            "INSERT INTO active_limits (tool, kind, text, hit_at) VALUES ('claude', 'other', '', 1)",
        )
        .execute(w.board.store().pool())
        .await
        .unwrap();
    }
    World {
        w,
        _claude: claude,
        parent,
        child,
        worktree,
    }
}

fn answer_session(w: &TestService, t: &Ticket) -> Option<String> {
    w.sessions
        .running_of(&t.id, SessionKind::Answer)
        .map(|s| s.id)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_merged_parent_is_resumed_answer_only_and_cleaned_up() {
    let World {
        w,
        _claude,
        parent,
        child,
        worktree,
    } = world(false).await;
    let asked = w
        .messages
        .ask(
            "SHOP-2",
            "SHOP-1",
            "Which status code on a declined card?",
            &agent("SHOP-2"),
        )
        .await
        .unwrap();
    assert_eq!(asked.route, Some(Route::Resumed));
    assert_eq!(asked.state(), MessageState::Delivered);
    let session = answer_session(&w, &parent).expect("an answer session");
    let printed = w.wait_for_output(&session, "answer-session:").await;
    assert!(
        printed.contains(&format!("--resume {CONVERSATION}")),
        "{printed}"
    );
    assert!(
        printed.contains(&claude_skill("otaq:answer-question")),
        "{printed}"
    );
    assert!(worktree.is_dir(), "recreated at the same path");
    assert_eq!(
        git(&worktree, &["branch", "--show-current"]).trim(),
        naming::branch("SHOP-1"),
        "on the kept branch"
    );
    let stored = w.sessions.stored(&session).await.unwrap();
    assert_eq!(stored.tool_session_id.as_deref(), Some(CONVERSATION));
    assert_eq!(
        (stored.tool, stored.model.as_deref()),
        (Some(otaq_core::agent_tools::Tool::Claude), None)
    );
    let card = w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(
        card.flags.answering,
        Some(child.key.clone()),
        "answering SHOP-2"
    );

    let answered = w
        .messages
        .answer(&asked.id, "402 with {code, message}.", &agent("SHOP-1"))
        .await
        .unwrap();
    assert_eq!(answered.state(), MessageState::Answered);
    assert!(answer_session(&w, &parent).is_none(), "the session ended");
    assert!(!worktree.exists(), "clean: removed");
    assert_eq!(
        w.board.ticket("SHOP-1").await.unwrap().flags.answering,
        None
    );

    // Files changed during the next answer: the worktree is kept and listed in Cleanup.
    let again = w
        .messages
        .ask(
            "SHOP-2",
            "SHOP-1",
            "And for an expired card?",
            &agent("SHOP-2"),
        )
        .await
        .unwrap();
    assert!(answer_session(&w, &parent).is_some());
    std::fs::write(worktree.join("notes.md"), "scratch").unwrap();
    w.messages
        .answer(&again.id, "Also 402.", &agent("SHOP-1"))
        .await
        .unwrap();
    assert!(worktree.join("notes.md").exists(), "kept");
    let scan = w.cleanup.scan().await.unwrap();
    assert!(format!("{scan:?}").contains("SHOP-1"), "{scan:?}");
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_plan_limit_holds_the_question_and_a_lost_conversation_refuses() {
    let World {
        w,
        parent,
        _claude: _keep,
        ..
    } = world(true).await;
    let asked = w
        .messages
        .ask("SHOP-2", "SHOP-1", "Which status code?", &agent("SHOP-2"))
        .await
        .unwrap();
    assert_eq!(asked.route, None, "waits while the limit is active");
    assert!(answer_session(&w, &parent).is_none());
    sqlx::query("DELETE FROM active_limits")
        .execute(w.board.store().pool())
        .await
        .unwrap();
    w.messages.route_waiting().await.unwrap();
    assert!(
        answer_session(&w, &parent).is_some(),
        "resumed once the limit reset"
    );
    w.stop().await;

    // The conversation file is gone: the ask is refused (MSG-04).
    let World { w, _claude, .. } = world(false).await;
    std::fs::remove_dir_all(_claude.path().join("projects")).unwrap();
    let refused = w
        .messages
        .ask("SHOP-2", "SHOP-1", "Which status code?", &agent("SHOP-2"))
        .await
        .unwrap_err();
    assert_eq!(
        refused.to_string(),
        format!(
            "SHOP-1's conversation is no longer available. Ask the human instead (`{} ask human`).",
            naming::COMMAND_NAME
        )
    );
    let _ = Path::new("");
    w.stop().await;
}

/// T-0456 (PAU-04; d0030, d0112): while the target's project is paused the question waits, the route's tick
/// included; the project's Resume routes it, and the answer session starts once.
#[tokio::test(flavor = "multi_thread")]
async fn a_paused_projects_finished_ticket_answers_once_it_resumes() {
    let World {
        w, _claude, parent, ..
    } = world(false).await;
    let shop = PauseScope::Project {
        project: "SHOP".into(),
    };
    w.pause.pause(&shop).await.unwrap();
    let asked = w
        .messages
        .ask("SHOP-2", "SHOP-1", "Which status code?", &agent("SHOP-2"))
        .await
        .unwrap();
    assert_eq!(asked.route, None, "waits while paused");
    w.messages.route_waiting().await.unwrap();
    assert!(answer_session(&w, &parent).is_none(), "kept, not started");

    let me = Actor::User("me".into());
    w.pause.resume(&shop, &me).await.unwrap();
    let session = answer_session(&w, &parent).expect("Resume routed it");
    w.pause.resume(&shop, &me).await.unwrap();
    w.messages.route_waiting().await.unwrap();
    assert_eq!(answer_session(&w, &parent), Some(session), "once");
    w.stop().await;
}

/// T-0153 (MSG-05): with "Let finished tickets answer questions" off, the question goes to the user's Needs you, first
/// group; answering there records it; "Resume SHOP-1" lets the finished ticket answer instead.
#[tokio::test(flavor = "multi_thread")]
async fn with_the_setting_off_the_question_goes_to_needs_you() {
    let World {
        w,
        parent,
        _claude: _keep,
        ..
    } = world(false).await;
    let mut settings = w.board.project_settings("SHOP").await.unwrap();
    assert!(settings.let_finished_tickets_answer, "on by default");
    settings.let_finished_tickets_answer = false;
    w.board
        .set_project_settings("SHOP", &settings)
        .await
        .unwrap();

    let asked = w
        .messages
        .ask(
            "SHOP-2",
            "SHOP-1",
            "Which Stripe API version?",
            &agent("SHOP-2"),
        )
        .await
        .unwrap();
    assert_eq!(asked.route, Some(Route::NeedsYou));
    assert_eq!(asked.state(), MessageState::NeedsYou);
    assert!(answer_session(&w, &parent).is_none(), "no automatic resume");
    let items = w.board.needs_you_items(i64::MAX / 2).await.unwrap();
    let item = items
        .iter()
        .find(|i| i.message.is_some())
        .expect("an needs_you item");
    assert_eq!(item.key.to_string(), "SHOP-2");
    assert_eq!(
        item.reason,
        "asks SHOP-1 (Merged): Which Stripe API version?"
    );
    assert_eq!(
        item.group,
        otaq_core::board::needs_you::NeedsYouGroup::Family
    );
    assert_eq!(item.id, asked.id);

    // Answered from Needs you (free text, by the user): recorded, the item is gone.
    let answered = w
        .messages
        .answer(&asked.id, "2024-06-20.", &Actor::User("me".into()))
        .await
        .unwrap();
    assert_eq!(answered.answer.as_deref(), Some("2024-06-20."));
    let items = w.board.needs_you_items(i64::MAX / 2).await.unwrap();
    assert!(items.iter().all(|i| i.message.is_none()));

    // Resume SHOP-1 lets the finished ticket answer (the user's click).
    let again = w
        .messages
        .ask(
            "SHOP-2",
            "SHOP-1",
            "And the webhook secret name?",
            &agent("SHOP-2"),
        )
        .await
        .unwrap();
    assert_eq!(again.route, Some(Route::NeedsYou));
    let resumed = w.messages.resume_target(&again.id).await.unwrap();
    assert_eq!(resumed.route, Some(Route::Resumed));
    assert!(answer_session(&w, &parent).is_some());
    w.stop().await;
}

/// Gives `tool` an active usage limit, its reset seen or not.
async fn limit_on(w: &TestService, tool: &str, reset_seen: bool) {
    sqlx::query(
        "INSERT INTO active_limits (tool, kind, text, hit_at, reset_seen_at) VALUES (?, 'other', '', 1, ?)",
    )
    .bind(tool)
    .bind(reset_seen.then_some(2_i64))
    .execute(w.board.store().pool())
    .await
    .unwrap();
}

/// F-F-1 (USG-10): a limit holds only the answer sessions of its own tool. SHOP-1's conversation ran on Claude Code,
/// so OpenCode's and Codex's limits don't hold its answer.
#[tokio::test(flavor = "multi_thread")]
async fn another_tools_limit_doesnt_hold_the_answer_session() {
    let World {
        w, _claude, parent, ..
    } = world(false).await;
    limit_on(&w, "opencode", false).await;
    limit_on(&w, "codex", false).await;
    let asked = w
        .messages
        .ask("SHOP-2", "SHOP-1", "Which status code?", &agent("SHOP-2"))
        .await
        .unwrap();
    assert_eq!(asked.route, Some(Route::Resumed));
    assert!(answer_session(&w, &parent).is_some());
    w.stop().await;
}

/// d0020: the target's own tool's limit holds its answer until that tool's Resume, not at the reset alone.
#[tokio::test(flavor = "multi_thread")]
async fn the_own_tools_limit_holds_after_its_reset_was_seen() {
    let World {
        w, _claude, parent, ..
    } = world(false).await;
    limit_on(&w, "claude", true).await;
    let asked = w
        .messages
        .ask("SHOP-2", "SHOP-1", "Which status code?", &agent("SHOP-2"))
        .await
        .unwrap();
    assert_eq!(asked.route, None, "waits for Claude Code's Resume");
    w.messages.route_waiting().await.unwrap();
    assert!(answer_session(&w, &parent).is_none());
    w.stop().await;
}

/// MSG-05's **Resume SHOP-1** is refused only by the limit of the tool SHOP-1's conversation ran on, named.
#[tokio::test(flavor = "multi_thread")]
async fn resume_target_is_refused_only_by_the_conversations_tool() {
    let World {
        w, _claude, parent, ..
    } = world(false).await;
    let mut settings = w.board.project_settings("SHOP").await.unwrap();
    settings.let_finished_tickets_answer = false;
    w.board
        .set_project_settings("SHOP", &settings)
        .await
        .unwrap();
    let asked = w
        .messages
        .ask("SHOP-2", "SHOP-1", "Which status code?", &agent("SHOP-2"))
        .await
        .unwrap();
    assert_eq!(asked.route, Some(Route::NeedsYou));

    limit_on(&w, "claude", false).await;
    let refused = w.messages.resume_target(&asked.id).await.unwrap_err();
    assert_eq!(
        refused.to_string(),
        "Claude Code's usage limit is active: SHOP-1 answers once it resets"
    );
    assert!(answer_session(&w, &parent).is_none());

    sqlx::query("DELETE FROM active_limits")
        .execute(w.board.store().pool())
        .await
        .unwrap();
    limit_on(&w, "opencode", false).await;
    let resumed = w.messages.resume_target(&asked.id).await.unwrap();
    assert_eq!(resumed.route, Some(Route::Resumed));
    assert!(answer_session(&w, &parent).is_some());
    w.stop().await;
}

/// An OpenCode target held by OpenCode's limit: the route's tick reads the conversation's tool from history and
/// never runs `opencode` to ask whether the conversation is still there. Unix only: the stand-in `opencode` is a shell
/// script.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn a_held_opencode_target_doesnt_run_opencode_on_the_tick() {
    let bin = TempDir::new().unwrap();
    let calls = bin.path().join("calls");
    let opencode = bin.path().join("opencode");
    std::fs::write(
        &opencode,
        format!(
            "#!/bin/sh\necho \"$*\" >> '{}'\nprintf '{{\"data\":{{\"id\":\"ses_answer\"}}}}'\n",
            calls.display()
        ),
    )
    .unwrap();
    let mut permissions = std::fs::metadata(&opencode).unwrap().permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut permissions, 0o755);
    std::fs::set_permissions(&opencode, permissions).unwrap();
    let World {
        w, _claude, parent, ..
    } = world_on(false, Some(bin.path())).await;
    // SHOP-1's newest conversation ran on OpenCode.
    sqlx::query(
        "INSERT INTO sessions (id, project_id, ticket_id, kind, tool, tool_session_id, state, started_at, ended_at)
         SELECT 'opencode-run', project_id, ticket_id, 'agent', 'opencode', 'ses_answer', 'ended', started_at + 1,
                ended_at + 1
         FROM sessions WHERE ticket_id = ? AND kind = 'agent'",
    )
    .bind(&parent.id)
    .execute(w.board.store().pool())
    .await
    .unwrap();
    limit_on(&w, "opencode", false).await;
    // The conversation checks only (`opencode api … /api/session/<id>`), not the service's other looks at OpenCode.
    let read_calls = || {
        std::fs::read_to_string(&calls)
            .unwrap_or_default()
            .lines()
            .filter(|l| l.contains("/api/session/ses_answer"))
            .count()
    };

    let asked = w
        .messages
        .ask("SHOP-2", "SHOP-1", "Which status code?", &agent("SHOP-2"))
        .await
        .unwrap();
    assert_eq!(asked.route, None, "waits for OpenCode's Resume");
    let at_ask = read_calls();
    assert_eq!(at_ask, 1, "the ask checks the conversation once (MSG-04)");
    w.messages.route_waiting().await.unwrap();
    w.messages.route_waiting().await.unwrap();
    assert_eq!(read_calls(), at_ask, "the tick doesn't run opencode");
    assert!(answer_session(&w, &parent).is_none());
    w.stop().await;
}
