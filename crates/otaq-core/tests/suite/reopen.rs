//! Reopen (T-0101, CLS-03, GIT-06): only Closed tickets reopen; on the way back to Dev the old branch is rebased
//! onto the current base with no tokens and a NEW agent session starts (never `--resume`); a conflict is
//! announced through `otaq prime`. `claude` stands in as an `sh` printing its arguments. Where builds are renamed, the
//! reopened ticket's new worktree takes the build nearest to its rebased commit (T-0309, M19-c).

use crate::common::freed;
use otaq_core::service::testing::said;

use std::path::PathBuf;
use std::time::Duration;

use otaq_core::agent_tools::LaunchContext;
use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::board::conflicts::ConflictReason;
use otaq_core::board::moves::DropTarget;
use otaq_core::board::tickets::Ticket;
use otaq_core::domain::{Actor, Status};
use otaq_core::git::testing::{commit, git};
use otaq_core::naming;
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::TestService;
use otaq_core::session::SessionKind;

fn me() -> Actor {
    Actor::User("me".into())
}

async fn world() -> TestService {
    TestService::start_with(|config| ServiceConfig {
        launch: Some(LaunchContext {
            bin_dir: "/nonexistent".into(),
            plugin_dir: "/nonexistent/plugin".into(),
            path: std::env::var_os("PATH").unwrap_or_default(),
            claude: ClaudeLaunch {
                program: vec![
                    "sh".into(),
                    "-c".into(),
                    "printf 'ARGS %s\\n' \"$*\"; exec sleep 30".into(),
                    "sh".into(),
                ],
                config_dir: None,
            },
        }),
        ..config
    })
    .await
}

/// A ticket that worked on `file`, had a session, and was closed (clean worktree: removed).
async fn closed(w: &TestService, file: &str) -> (Ticket, PathBuf) {
    let t = w.create("SHOP", "Coupon codes").await;
    let wt = w.start_worktree(&t).await;
    commit(&wt, file, "ticket\n");
    w.record_ended_agent(&t, "0199-old-conversation").await;
    w.force_status(&t, Status::Working).await;
    w.review.close("SHOP-1", true, &me()).await.unwrap();
    assert!(!wt.exists());
    (t, wt)
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

#[tokio::test]
async fn a_reopened_ticket_rebases_and_starts_a_new_session() {
    let w = world().await;
    let project = w.add_project("shop-web").await;
    let repo = PathBuf::from(&project.repo_path);
    let (t, wt) = closed(&w, "coupon.ts").await;

    let reopened = w.board.reopen("SHOP-1", &me()).await.unwrap();
    assert_eq!(reopened.status, Status::Backlog);
    commit(&repo, "moved.ts", "main moved\n");
    let main = git(&repo, &["rev-parse", "main"]);

    w.board
        .move_ticket("SHOP-1", DropTarget::ReadyForDev, true, &me())
        .await
        .unwrap();
    // What the scheduler does once it sees the ticket in Queued.
    w.starter
        .start("SHOP-1", &me(), otaq_core::session::Start::Auto)
        .await
        .unwrap();
    until_status(&w, "SHOP-1", Status::Working).await;

    assert!(
        wt.join("coupon.ts").is_file(),
        "worktree recreated on the old branch"
    );
    assert!(
        wt.join("moved.ts").is_file(),
        "rebased onto the current main"
    );
    assert_eq!(git(&wt, &["rev-parse", "HEAD~1"]), main);
    let row = w.board.store().ticket(&t.id).await.unwrap().unwrap();
    assert_eq!(row.base_sha.as_deref(), Some(main.as_str()));
    assert!(!row.reopened);
    let session = w.sessions.running_of(&t.id, SessionKind::Agent).unwrap();
    let out = w.wait_for_output(&session.id, "ARGS").await;
    assert!(out.contains("--session-id"), "{out}");
    assert!(!out.contains("--resume"), "never a resume: {out}");
    assert!(
        out.contains(&format!("/{}", naming::copy_name("work-ticket"))),
        "{out}"
    );
    assert_ne!(
        session.tool_session_id.as_deref(),
        Some("0199-old-conversation")
    );
    let kept: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sessions WHERE ticket_id = ? AND tool_session_id = '0199-old-conversation'",
    )
    .bind(&t.id)
    .fetch_one(w.board.store().pool())
    .await
    .unwrap();
    assert_eq!(kept, 1, "the previous session ID stays in History");
    let prime = otaq_core::prime::render(
        &w.board
            .prime_context("SHOP-1", w.adapters.for_new_session())
            .await
            .unwrap(),
    );
    assert!(!prime.contains("Resolve first"), "{prime}");
    w.sessions.stop(&session.id).await.unwrap();
    w.stop().await;
}

#[tokio::test]
async fn a_conflicting_rebase_is_announced_through_prime_and_the_agent_still_starts() {
    let w = world().await;
    let project = w.add_project("shop-web").await;
    let repo = PathBuf::from(&project.repo_path);
    let (t, wt) = closed(&w, "lib.ts").await;
    w.board.reopen("SHOP-1", &me()).await.unwrap();
    commit(&repo, "lib.ts", "main\n");
    let branch = naming::branch("SHOP-1");
    let before = git(&repo, &["rev-parse", &branch]);

    w.board
        .move_ticket("SHOP-1", DropTarget::ReadyForDev, true, &me())
        .await
        .unwrap();
    // What the scheduler does once it sees the ticket in Queued.
    w.starter
        .start("SHOP-1", &me(), otaq_core::session::Start::Auto)
        .await
        .unwrap();
    until_status(&w, "SHOP-1", Status::Working).await;

    assert_eq!(
        git(&repo, &["rev-parse", &branch]),
        before,
        "the branch as it was"
    );
    assert_eq!(
        std::fs::read_to_string(wt.join("lib.ts")).unwrap(),
        "ticket\n"
    );
    let conflict = w.board.conflict(&t.id).await.unwrap().unwrap();
    assert_eq!(conflict.reason, ConflictReason::Reopen);
    assert_eq!(conflict.files, ["lib.ts"]);
    let prime = otaq_core::prime::render(
        &w.board
            .prime_context("SHOP-1", w.adapters.for_new_session())
            .await
            .unwrap(),
    );
    assert!(
        prime.contains(
            "Reopened: the rebase onto `main` conflicted. Resolve it first with the restack skill"
        ),
        "{prime}"
    );
    assert!(prime.contains("- lib.ts"));
    // The fact clears once the agent submits again.
    std::fs::write(wt.join("lib.ts"), "main\nticket\n").unwrap();
    git(&wt, &["commit", "--quiet", "-am", "resolve"]);
    w.board
        .submit_for_review(
            "SHOP-1",
            said(&w.board, "SHOP-1", "resolved").await.as_ref(),
            &Actor::Agent(t.key.clone()),
        )
        .await
        .unwrap();
    assert_eq!(w.board.conflict(&t.id).await.unwrap(), None);
    if let Some(s) = w.sessions.running_of(&t.id, SessionKind::Agent) {
        w.sessions.stop(&s.id).await.unwrap();
    }
    w.stop().await;
}

/// Prepare on reopen: the new worktree is seeded after the rebase, so the build nearest to the rebased commit is
/// taken (one built at the current `main`), not the ticket's own build of its old commit.
#[tokio::test(flavor = "multi_thread")]
async fn a_reopened_ticket_takes_the_nearest_build_after_the_rebase() {
    use otaq_core::board::settings::{AgentLimits, GlobalSettings};
    use otaq_core::warm::store::WarmRecord;

    let w = freed::world().await;
    let project = freed::shop_web(&w, |s| s.env_files.clear()).await;
    let repo = PathBuf::from(&project.repo_path);
    // The agent limit is full, so the scheduler starts nothing; the tickets waiting for a slot keep the stock at two.
    w.board
        .set_global_settings(&GlobalSettings {
            agent_limits: AgentLimits {
                enabled: true,
                max_concurrency: 1,
                back_pressure: 3,
            },
            ..Default::default()
        })
        .await
        .unwrap();
    let t = w.create("SHOP", "Coupon codes").await;
    let wt = w.start_worktree(&t).await;
    commit(&wt, "coupon.ts", "ticket\n");
    freed::build(&wt, "own build");
    w.force_status(&t, Status::Working).await;
    w.review.close("SHOP-1", true, &me()).await.unwrap();
    assert!(!wt.exists());
    assert_eq!(freed::entries(&w, &project).len(), 1, "its build was kept");
    let busy = w.create("SHOP", "busy").await;
    w.force_status(&busy, Status::Working).await;
    let other = w.create("SHOP", "waiting").await;
    w.force_status(&other, Status::WaitingForSlot).await;

    w.board.reopen("SHOP-1", &me()).await.unwrap();
    commit(&repo, "moved.ts", "main moved\n");
    let main = git(&repo, &["rev-parse", "main"]);
    w.board
        .move_ticket("SHOP-1", DropTarget::ReadyForDev, true, &me())
        .await
        .unwrap();
    w.force_status(&t, Status::WaitingForSlot).await;
    // A build of the current main, kept from SHOP-9.
    let incoming = w.warm.incoming(&project).await.unwrap();
    std::fs::create_dir_all(incoming.path().join("out").join("deep")).unwrap();
    std::fs::write(incoming.path().join("out/deep/app.bin"), "main build").unwrap();
    let record = WarmRecord {
        built_at: main.clone(),
        base: main.clone(),
        from: "SHOP-9".into(),
        kept_at: otaq_core::store::now_ms(),
        project: String::new(),
    };
    w.ticket_env
        .stock()
        .add(&project, incoming, record)
        .await
        .unwrap();
    assert_eq!(freed::entries(&w, &project).len(), 2);

    w.starter
        .start("SHOP-1", &me(), otaq_core::session::Start::Auto)
        .await
        .unwrap();
    until_status(&w, "SHOP-1", Status::Working).await;

    assert_eq!(git(&wt, &["rev-parse", "HEAD~1"]), main, "rebased");
    assert_eq!(
        freed::app_bin(&wt).as_deref(),
        Some("main build"),
        "the build nearest to the rebased commit"
    );
    let log = freed::setup_log(&w, "SHOP-1");
    assert!(log.starts_with("Took shop-web's build"), "{log}");
    assert!(log.contains("(SHOP-9"), "{log}");
    assert_eq!(freed::entries(&w, &project).len(), 1, "its own build stays");
    let session = w.sessions.running_of(&t.id, SessionKind::Agent).unwrap();
    w.sessions.stop(&session.id).await.unwrap();
    w.stop().await;
}
