//! Send back (T-0086): the comments go to the agent's live session, a resumed one, or the ticket waits in Queued
//! while the agent limit is full. Sessions are an ordinary `sh`; no test starts claude. A build freed at Review comes
//! back before the agent works again (T-0309, M19-c).

use crate::common::freed;
use otaq_core::service::testing::said;

use otaq_core::agent_tools::LaunchContext;
use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::board::Refusal;
use otaq_core::board::comments::{NewComment, Side};
use otaq_core::board::settings::{AgentLimits, GlobalSettings};
use otaq_core::board::tickets::{EventKind, Ticket};
use otaq_core::domain::{Actor, BuildState, SentBack, Status};
use otaq_core::naming;
use otaq_core::review::ReturnOutcome;
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::TestService;
use otaq_core::session::SessionKind;
use otaq_core::session::deliveries::handoff_sentence;

fn me() -> Actor {
    Actor::User("me".into())
}

fn skill() -> String {
    format!("/{}", naming::copy_name("address-review"))
}

async fn in_review_with_comments(world: &TestService, comments: usize) -> Ticket {
    let t = world.create("SHOP", "Search results paging").await;
    world.force_status(&t, Status::WaitingForYou).await;
    for n in 0..comments {
        world
            .board
            .add_comment(&NewComment {
                key: t.key.to_string(),
                file: "lib/search.ts".into(),
                line: u32::try_from(n + 1).unwrap(),
                side: Side::New,
                body: format!("comment {n}"),
            })
            .await
            .unwrap();
    }
    t
}

fn done(outcome: ReturnOutcome) -> Ticket {
    match outcome {
        ReturnOutcome::Done { ticket } => *ticket,
        other => panic!("expected done, got {other:?}"),
    }
}

#[tokio::test]
async fn a_live_session_gets_the_skill_and_the_ticket_works_again() {
    let world = TestService::start().await;
    world.add_project("shop-web").await;
    let t = in_review_with_comments(&world, 3).await;
    let agent = world.agent_shell(&t).await;

    let after = done(
        world
            .review
            .fail("SHOP-1", "Also check page 0", None, false, &me())
            .await
            .unwrap(),
    );
    assert_eq!(after.status, Status::Working);
    assert_eq!(
        after.flags.sent_back,
        Some(SentBack {
            comments: 3,
            round: 1,
            decisions: 0
        })
    );
    world.wait_for_output(&agent.session, &skill()).await;

    let comments = world.board.comments("SHOP-1", None).await.unwrap();
    assert_eq!(comments.pending, 0);
    assert!(comments.comments.iter().all(|c| c.sent_at.is_some()));
    assert_eq!(comments.round, 2);
    assert_eq!(
        world.board.latest_send_back_round(&t.id).await.unwrap(),
        Some(1)
    );
    assert_eq!(
        crate::common::last_comment(&world.board, "SHOP-1")
            .await
            .map(|c| c.body)
            .as_deref(),
        Some("Also check page 0")
    );
    let history = world.board.history("SHOP-1").await.unwrap();
    assert!(history.iter().any(|e| e.kind == EventKind::SentBack));

    // The flag stays until the ticket is back in Review.
    world.give_worktree(&t).await;
    world
        .board
        .submit_for_review(
            "SHOP-1",
            said(&world.board, "SHOP-1", "fixed").await.as_ref(),
            &Actor::Agent(t.key.clone()),
        )
        .await
        .unwrap();
    assert_eq!(
        world.board.ticket("SHOP-1").await.unwrap().flags.sent_back,
        None
    );
    world.stop().await;
}

#[tokio::test]
async fn unsent_text_asks_first_and_clear_it_and_send_goes_on() {
    let world = TestService::start().await;
    world.add_project("shop-web").await;
    let t = in_review_with_comments(&world, 1).await;
    let agent = world.agent_shell(&t).await;
    world.sessions.input(&agent.session, b"my draft").unwrap();

    let first = world
        .review
        .fail("SHOP-1", "", None, false, &me())
        .await
        .unwrap();
    assert_eq!(
        first,
        ReturnOutcome::NeedsConfirmation {
            ticket: t.key.clone(),
            unclearable_by: None,
        }
    );
    assert_eq!(world.status("SHOP-1").await, Status::WaitingForYou);
    assert_eq!(
        world.board.comments("SHOP-1", None).await.unwrap().pending,
        1
    );

    let after = done(
        world
            .review
            .fail("SHOP-1", "", None, true, &me())
            .await
            .unwrap(),
    );
    assert_eq!(after.status, Status::Working);
    world.wait_for_output(&agent.session, &skill()).await;
    world.stop().await;
}

#[tokio::test]
async fn a_full_agent_limit_queues_the_ticket_with_the_skill_to_run() {
    let world = TestService::start().await;
    world.add_project("shop-web").await;
    world
        .board
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
    let busy = world.create("SHOP", "busy").await;
    world.force_status(&busy, Status::Working).await;
    let t = in_review_with_comments(&world, 2).await;
    let agent = world.agent_shell(&t).await;
    // Unsent text doesn't matter now: nothing is typed until a slot frees.
    world.sessions.input(&agent.session, b"draft").unwrap();

    let after = done(
        world
            .review
            .fail("SHOP-2", "", None, false, &me())
            .await
            .unwrap(),
    );
    assert!(after.status.is_queued(), "{}", after.status);
    assert!(after.queued_at.is_some());
    let row = world.board.store().ticket(&t.id).await.unwrap().unwrap();
    assert_eq!(row.resume_skill.as_deref(), Some("address-review"));
    assert_eq!(
        after.flags.sent_back,
        Some(SentBack {
            comments: 2,
            round: 1,
            decisions: 0
        })
    );
    world.stop().await;
}

#[tokio::test]
async fn a_gone_session_is_resumed_with_the_skill_in_the_same_click() {
    // `claude` stands in as `sh` printing its arguments, so the test reads the command line it got.
    let world = TestService::start_with(|config| ServiceConfig {
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
    .await;
    world.add_project("shop-web").await;
    let t = in_review_with_comments(&world, 1).await;
    let worktree = world.start_worktree(&t).await;
    world.record_ended_agent(&t, "0199-old-conversation").await;

    let after = done(
        world
            .review
            .fail("SHOP-1", "", None, false, &me())
            .await
            .unwrap(),
    );
    assert_eq!(after.status, Status::Working, "{:?}", after.start_problem);
    let session = world
        .sessions
        .running_of(&t.id, otaq_core::session::SessionKind::Agent)
        .expect("resumed");
    let out = world.wait_for_output(&session.id, "ARGS").await;
    assert!(
        out.contains(&format!("--resume 0199-old-conversation {}", skill())),
        "{out}"
    );
    assert_eq!(
        session.tool_session_id.as_deref(),
        Some("0199-old-conversation")
    );
    assert_eq!(
        (session.tool, session.model.as_deref()),
        (Some(otaq_core::agent_tools::Tool::Claude), None)
    );
    assert!(worktree.is_dir());
    // Kept until the agent shows the resumed conversation got it (a failed resume must send it again).
    let row = world.board.store().ticket(&t.id).await.unwrap().unwrap();
    assert_eq!(row.resume_skill.as_deref(), Some("address-review"));
    let agent = otaq_core::caller::Caller::Agent {
        ticket: t.key.clone(),
        ticket_id: t.id.clone(),
        session: session.id.clone(),
    };
    world
        .signals
        .hook(
            &agent,
            otaq_core::agent_tools::Tool::Claude,
            "UserPromptSubmit",
            &serde_json::json!({"hook_event_name": "UserPromptSubmit", "prompt": skill()}),
        )
        .await
        .unwrap();
    let row = world.board.store().ticket(&t.id).await.unwrap().unwrap();
    assert_eq!(row.resume_skill, None, "the skill ran");
    world.sessions.stop(&session.id).await.unwrap();
    world.stop().await;
}

#[tokio::test]
async fn send_back_is_refused_on_epics_and_outside_review() {
    let world = TestService::start().await;
    world.add_project("shop-web").await;
    let t = world.create("SHOP", "Working").await;
    world.force_status(&t, Status::Working).await;
    let err = world
        .review
        .fail("SHOP-1", "", None, false, &me())
        .await
        .unwrap_err();
    assert_eq!(err.refusal(), Some(Refusal::MoveRefused));

    let epic = world
        .board
        .create_ticket(
            &otaq_core::board::tickets::NewTicketInput {
                project: "SHOP".into(),
                title: "Wishlist".into(),
                kind: otaq_core::domain::Kind::Epic,
                ..Default::default()
            },
            &me(),
        )
        .await
        .unwrap();
    world.force_status(&epic, Status::WaitingForYou).await;
    let err = world
        .review
        .fail(&epic.key.to_string(), "", None, false, &me())
        .await
        .unwrap_err();
    assert_eq!(err.to_string(), "Add or reopen a child ticket to change it");
    world.stop().await;
}

/// T-0090, FLW-17 (parity change 2 of M33): a click Local merge's conflict flags the ticket and sends it to Dev at
/// once, no click: the restack skill is queued for its session ("Merge conflict · fixing") and prime names the target.
#[tokio::test]
async fn a_merge_conflict_goes_to_the_agent_with_the_restack_skill() {
    use otaq_core::git::testing::commit;
    use otaq_core::review::LocalMergeOutcome;

    let world = TestService::start().await;
    let project = world.add_project("shop-web").await;
    let repo = std::path::PathBuf::from(&project.repo_path);
    let t = world.create("SHOP", "Pricing").await;
    let wt = world.start_worktree(&t).await;
    commit(&wt, "lib.ts", "ticket\n");
    commit(&repo, "lib.ts", "main\n");
    world.force_status(&t, Status::WaitingForYou).await;
    let agent = world.agent_shell(&t).await;

    let merged = world
        .review
        .local_merge("SHOP-1", "m", false, &me())
        .await
        .unwrap();
    assert!(
        matches!(merged, LocalMergeOutcome::Conflict { .. }),
        "{merged:?}"
    );
    let after = world.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(after.status, Status::MergeConflictFixing);
    assert_eq!(
        after.place,
        otaq_core::domain::Place::stage_of(otaq_core::flow::default::DEV)
    );
    assert_eq!(after.auto_merge_conflicts, 1);
    assert_eq!(
        after.flags.conflicts_with.as_deref(),
        Some("main"),
        "until it is back in Review"
    );
    let pending = world.deliveries.pending(&agent.session).await.unwrap();
    assert_eq!(
        pending.last().unwrap().text,
        handoff_sentence(
            world.adapters.of_session(None).skill_root(),
            "restack",
            None
        )
    );
    let prime = otaq_core::prime::render(
        &world
            .board
            .prime_context("SHOP-1", world.adapters.for_new_session())
            .await
            .unwrap(),
    );
    assert!(
        prime.contains("This branch conflicts with the latest `main`. Rebase it onto `main`"),
        "{prime}"
    );
    assert!(prime.contains("- lib.ts"), "{prime}");
    assert!(
        world
            .board
            .history("SHOP-1")
            .await
            .unwrap()
            .iter()
            .any(|e| e.kind == EventKind::SentToFix)
    );

    world
        .board
        .submit_for_review(
            "SHOP-1",
            said(&world.board, "SHOP-1", "rebased").await.as_ref(),
            &Actor::Agent(t.key.clone()),
        )
        .await
        .unwrap();
    let back = world.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(back.flags.conflicts_with, None);
    world.stop().await;
}

// Bringing a freed build back (T-0309, M19-c). The disk renames builds, so a freed build sits in the stock.

/// The M19-c acceptance: a freed ticket sent back to its live session gets its build back first, then the skill.
/// The session and the worktree are the same; the build left the stock.
#[tokio::test(flavor = "multi_thread")]
async fn a_live_session_gets_the_build_back_then_the_skill() {
    let w = freed::world().await;
    let project = freed::shop_web(&w, |_| {}).await;
    let (t, wt) = freed::freed(&w, "Checkout API", "api.ts").await;
    let key = t.key.to_string();
    assert_eq!(freed::entries(&w, &project), [0]);
    let agent = w.agent_shell(&t).await;
    let worktree = t.work.clone().unwrap().worktree;

    let after = done(w.review.fail(&key, "", None, false, &me()).await.unwrap());

    assert_eq!(after.status, Status::Working);
    assert_eq!(freed::app_bin(&wt).as_deref(), Some("Checkout API"));
    assert_eq!(freed::state(&w, &key).await, BuildState::Present);
    assert_eq!(
        freed::entries(&w, &project),
        Vec::<u64>::new(),
        "its build left the stock"
    );
    assert_eq!(after.work.unwrap().worktree, worktree, "the same worktree");
    assert_eq!(
        w.sessions
            .running_of(&t.id, SessionKind::Agent)
            .expect("the agent still runs")
            .id,
        agent.session,
        "the same session"
    );
    let log = freed::setup_log(&w, &key);
    assert!(log.starts_with("Took shop-web's build"), "{log}");
    assert!(log.contains(freed::SETUP_RAN), "{log}");
    assert_eq!(freed::porcelain(&wt), "", "the worktree stays clean");
    w.wait_for_output(&agent.session, &skill()).await;
    w.stop().await;
}

/// A freed ticket whose session is gone: the same click resumes the conversation, in the same worktree, with its
/// build back (the start brings it).
#[tokio::test(flavor = "multi_thread")]
async fn a_gone_session_is_resumed_with_its_build() {
    let w = freed::world().await;
    let project = freed::shop_web(&w, |_| {}).await;
    let (t, wt) = freed::freed(&w, "Search paging", "search.ts").await;
    let key = t.key.to_string();
    w.record_ended_agent(&t, "0199-old-conversation").await;

    let after = done(w.review.fail(&key, "", None, false, &me()).await.unwrap());

    assert_eq!(after.status, Status::Working, "{:?}", after.start_problem);
    assert_eq!(freed::app_bin(&wt).as_deref(), Some("Search paging"));
    assert_eq!(freed::state(&w, &key).await, BuildState::Present);
    assert_eq!(freed::entries(&w, &project), Vec::<u64>::new());
    let session = w
        .sessions
        .running_of(&t.id, SessionKind::Agent)
        .expect("resumed");
    assert_eq!(
        after.work.unwrap().worktree,
        t.work.clone().unwrap().worktree,
        "in the same worktree"
    );
    let out = w.wait_for_output(&session.id, "ARGS").await;
    assert!(
        out.contains(&format!("--resume 0199-old-conversation {}", skill())),
        "{out}"
    );
    w.sessions.stop(&session.id).await.unwrap();
    w.stop().await;
}

/// With the agent limit full the ticket waits in Queued and its build stays in the stock; the start that comes
/// when a slot frees brings it back.
#[tokio::test(flavor = "multi_thread")]
async fn a_full_limit_queues_and_the_start_brings_the_build_back() {
    let w = freed::world().await;
    let project = freed::shop_web(&w, |_| {}).await;
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
    let busy = w.create("SHOP", "busy").await;
    w.force_status(&busy, Status::Working).await;
    let (t, wt) = freed::freed(&w, "Cart totals", "cart.ts").await;
    let key = t.key.to_string();
    w.record_ended_agent(&t, "0199-cart").await;

    let after = done(w.review.fail(&key, "", None, false, &me()).await.unwrap());
    assert!(after.status.is_queued(), "{}", after.status);
    assert_eq!(freed::state(&w, &key).await, BuildState::Freed);
    assert_eq!(
        freed::app_bin(&wt),
        None,
        "nothing comes back while it waits"
    );
    assert_eq!(freed::entries(&w, &project), [0]);

    // A slot frees.
    w.force_status(&busy, Status::Backlog).await;
    w.starter
        .start(&key, &me(), otaq_core::session::Start::Auto)
        .await
        .unwrap();
    for _ in 0..400 {
        if w.status(&key).await == Status::Working {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    assert_eq!(w.status(&key).await, Status::Working);
    assert_eq!(freed::app_bin(&wt).as_deref(), Some("Cart totals"));
    assert_eq!(freed::state(&w, &key).await, BuildState::Present);
    assert_eq!(freed::entries(&w, &project), Vec::<u64>::new());
    w.stop().await;
}

/// A click Local merge's conflict brings the build back before the restack skill is queued (FLW-17, no click).
#[tokio::test(flavor = "multi_thread")]
async fn a_merge_conflict_brings_the_build_back() {
    use otaq_core::review::LocalMergeOutcome;

    let w = freed::world().await;
    let project = freed::shop_web(&w, |_| {}).await;
    let repo = std::path::PathBuf::from(&project.repo_path);
    let (t, wt) = freed::freed(&w, "Pricing", "lib.ts").await;
    let key = t.key.to_string();
    otaq_core::git::testing::commit(&repo, "lib.ts", "main\n");
    let agent = w.agent_shell(&t).await;
    let merged = w.review.local_merge(&key, "m", false, &me()).await.unwrap();
    assert!(
        matches!(merged, LocalMergeOutcome::Conflict { .. }),
        "{merged:?}"
    );

    assert_eq!(w.status(&key).await, Status::MergeConflictFixing);
    assert_eq!(freed::app_bin(&wt).as_deref(), Some("Pricing"));
    assert_eq!(freed::state(&w, &key).await, BuildState::Present);
    assert_eq!(freed::entries(&w, &project), Vec::<u64>::new());
    let pending = w.deliveries.pending(&agent.session).await.unwrap();
    assert_eq!(
        pending.last().unwrap().text,
        handoff_sentence(w.adapters.of_session(None).skill_root(), "restack", None)
    );
    w.stop().await;
}

/// A ticket whose own build left the stock takes the nearest one there; with none left, the setup command builds
/// from nothing.
#[tokio::test(flavor = "multi_thread")]
async fn without_its_own_build_the_nearest_is_taken_or_setup_builds() {
    let w = freed::world().await;
    let project = freed::shop_web(&w, |_| {}).await;
    let (a, a_wt) = freed::freed(&w, "Wishlist", "wishlist.ts").await;
    // B's build was freed too, but its entry is gone from the stock (evicted, say).
    let (b, b_wt) = freed::in_review(&w, "Footer", "footer.ts").await;
    std::fs::remove_dir_all(b_wt.join("out")).unwrap();
    w.board
        .store()
        .set_build_state(&b.id, BuildState::Freed, None)
        .await
        .unwrap();
    let a_agent = w.agent_shell(&a).await;
    let b_agent = w.agent_shell(&b).await;

    done(
        w.review
            .fail(&b.key.to_string(), "", None, false, &me())
            .await
            .unwrap(),
    );
    assert_eq!(
        freed::app_bin(&b_wt).as_deref(),
        Some("Wishlist"),
        "the nearest build: A's"
    );
    let log = freed::setup_log(&w, &b.key.to_string());
    assert!(log.starts_with("Took shop-web's build"), "{log}");
    assert!(log.contains(&a.key.to_string()), "{log}");
    assert_eq!(freed::entries(&w, &project), Vec::<u64>::new());
    w.wait_for_output(&b_agent.session, &skill()).await;

    let a_key = a.key.to_string();
    done(w.review.fail(&a_key, "", None, false, &me()).await.unwrap());
    assert_eq!(freed::app_bin(&a_wt), None, "no build to take");
    assert_eq!(freed::state(&w, &a_key).await, BuildState::Present);
    let log = freed::setup_log(&w, &a_key);
    assert!(!log.contains("Took"), "{log}");
    assert!(log.contains(freed::SETUP_RAN), "the setup built it: {log}");
    w.wait_for_output(&a_agent.session, &skill()).await;
    w.stop().await;
}

/// A setup that fails while the build comes back never refuses the click: the skill is typed, the build is taken
/// as present (the agent runs the setup itself) and a start note says why.
#[tokio::test(flavor = "multi_thread")]
async fn a_failed_setup_on_send_back_still_delivers_the_skill() {
    let w = freed::world().await;
    freed::shop_web(&w, |s| s.setup_command = "exit 3".into()).await;
    let (t, wt) = freed::freed(&w, "Coupons", "coupon.ts").await;
    let key = t.key.to_string();
    let agent = w.agent_shell(&t).await;

    let after = done(w.review.fail(&key, "", None, false, &me()).await.unwrap());

    assert_eq!(after.status, Status::Working);
    w.wait_for_output(&agent.session, &skill()).await;
    assert_eq!(
        freed::app_bin(&wt).as_deref(),
        Some("Coupons"),
        "the build was taken before the setup failed"
    );
    assert_eq!(freed::state(&w, &key).await, BuildState::Present);
    let history = w.board.history(&key).await.unwrap();
    let note = history
        .iter()
        .find(|e| e.kind == EventKind::StartNote && e.payload["note"] == "build_not_restored")
        .expect("a start note");
    assert!(
        note.payload["message"]
            .as_str()
            .unwrap()
            .contains("setup command failed (exit 3)"),
        "{note:?}"
    );
    w.stop().await;
}
