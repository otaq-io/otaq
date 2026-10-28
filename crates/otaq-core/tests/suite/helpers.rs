//! Helpers (T-0537; d0133 and its audit answers 2 and 4, d0082, d0018, d0020, d0016, d0038, d0048, d0118): the agent
//! in charge starts a helper per piece, each with its own branch from the ticket's branch's head, its own worktree,
//! port and session (`OTAQ_PIECE`); lists them, reads a helper's diff, sends it back with a note and merges its piece
//! (refused with uncommitted files, refused on a conflict with the note to send, squashed when clean). A helper hands
//! its piece back and may do nothing else. Two helpers take the ticket's one slot; a tool at its limit holds a helper;
//! tokens add up as "in charge · helpers · checks"; a merged helper's worktree goes, one left behind shows in Cleanup.
//!
//! The tools are `sh` printing what they were given (and the stand-in of `tests/fixtures/stand-in` where skills
//! matter); repositories, `OTAQ_HOME` and the tools' home are temporary.

use std::path::{Path, PathBuf};
use std::time::Duration;

#[cfg(unix)]
use crate::common::stand_in;
use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::agent_tools::{LaunchContext, Tool};
use otaq_core::board::BoardError;
use otaq_core::board::tickets::Ticket;
use otaq_core::caller::Caller;
use otaq_core::domain::Status;
use otaq_core::git::testing::{commit, git};
use otaq_core::helpers::{Helper, HelperStatus};
use otaq_core::policy::{Action, HELPER_ONLY_ITS_PIECE, TicketFacts, authorize};
use otaq_core::recovery::cleanup::CleanupGroup;
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::{AgentShell, TestService};
use otaq_core::session::SessionKind;
use tempfile::TempDir;

/// Keeps running, printing its arguments and the variables a helper is told.
const PRINTS_ITS_ENV: &str = "printf 'ARGS %s\\n' \"$*\"; \
     printf 'PIECE=%s TICKET=%s PORT=%s\\n' \"$OTAQ_PIECE\" \"$OTAQ_TICKET\" \"$PORT\"; exec sleep 60";

struct World {
    w: TestService,
    _dir: Option<TempDir>,
}

fn sh_launch(config: ServiceConfig) -> ServiceConfig {
    ServiceConfig {
        launch: Some(LaunchContext {
            bin_dir: "/nonexistent".into(),
            plugin_dir: "/nonexistent/plugin".into(),
            path: std::env::var_os("PATH").unwrap_or_default(),
            claude: ClaudeLaunch {
                program: vec!["sh".into(), "-c".into(), PRINTS_ITS_ENV.into(), "sh".into()],
                config_dir: None,
            },
        }),
        ..config
    }
}

/// A service whose helpers are `sh` printing their variables, with shop-web (SHOP).
async fn world() -> World {
    let w = TestService::start_with(sh_launch).await;
    w.add_project("shop-web").await;
    World { w, _dir: None }
}

/// A service whose tools are the stand-in (it lists the skills it loaded at start).
#[cfg(unix)]
async fn stand_in_world() -> World {
    let dir = TempDir::new().unwrap();
    let launch = stand_in::launch(&dir.path().join("bin"), dir.path());
    let w = TestService::start_with(move |config| ServiceConfig {
        launch: Some(launch),
        ..config
    })
    .await;
    w.add_project("shop-web").await;
    World { w, _dir: Some(dir) }
}

/// A ticket at work in Dev: its branch and worktree as the start makes them, and its agent in charge (an `sh` agent
/// session).
async fn at_work(w: &TestService, title: &str) -> (Ticket, PathBuf, AgentShell, Caller) {
    let t = w.create("SHOP", title).await;
    let worktree = w.start_worktree(&t).await;
    w.force_status(&t, Status::Working).await;
    let t = w.board.ticket(&t.key.to_string()).await.unwrap();
    let agent = w.agent_shell(&t).await;
    let caller = Caller::Agent {
        ticket: t.key.clone(),
        ticket_id: t.id.clone(),
        session: agent.session.clone(),
    };
    (t, worktree, agent, caller)
}

fn helper_caller(t: &Ticket, h: &Helper) -> Caller {
    Caller::Helper {
        ticket: t.key.clone(),
        ticket_id: t.id.clone(),
        session: h.session.clone().unwrap(),
    }
}

fn refusal(e: BoardError) -> String {
    assert!(e.refusal().is_some(), "{e:?}");
    e.to_string()
}

async fn start(w: &TestService, t: &Ticket, caller: &Caller, piece: u32, title: &str) -> Helper {
    w.helpers
        .start(caller, &t.key.to_string(), piece, title, None)
        .await
        .unwrap()
        .helper
}

fn head(dir: &Path, rev: &str) -> String {
    git(dir, &["rev-parse", rev])
}

/// Start makes the helper's branch from the ticket's branch's head and its own worktree, and a helper session there
/// with `OTAQ_PIECE`, the ticket and a port of its own; the note becomes the comment "To 1.3: …"; History records it.
#[tokio::test(flavor = "multi_thread")]
async fn start_makes_the_branch_the_worktree_and_the_session() {
    let world = world().await;
    let w = &world.w;
    let (_, worktree, _agent, caller) = at_work(w, "Wishlist").await;
    commit(&worktree, "api.rs", "fn api() {}\n");
    let ticket_head = head(&worktree, "HEAD");

    let reply = w
        .helpers
        .start(
            &caller,
            "SHOP-1",
            3,
            "Wishlist page",
            Some("Use the API from piece 1."),
        )
        .await
        .unwrap();
    let helper = reply.helper;
    assert_eq!(helper.status, HelperStatus::Working);
    assert_eq!(helper.label(), "1.3");
    assert_eq!(helper.branch, otaq_core::naming::branch("SHOP-1.3"));
    let expected = w.data.worktree_path("shop-web", "SHOP-1.3").unwrap();
    assert_eq!(PathBuf::from(&helper.worktree), expected);
    assert!(expected.join(".git").exists());
    assert_eq!(
        head(&expected, "HEAD"),
        ticket_head,
        "from the ticket's head"
    );
    assert_eq!(git(&expected, &["branch", "--show-current"]), helper.branch);
    assert!(
        reply
            .text
            .starts_with("Started 1.3's helper (Wishlist page)"),
        "{}",
        reply.text
    );

    let session = w.sessions.info(helper.session.as_deref().unwrap()).unwrap();
    assert_eq!(session.kind, SessionKind::Helper);
    assert!(session.is_running());
    let out = w.wait_for_output(&session.id, "PIECE=").await;
    assert!(out.contains("PIECE=3 TICKET=SHOP-1 PORT="), "{out}");
    let port = helper.port.unwrap();
    assert!(out.contains(&format!("PORT={port}")), "{out}");
    let ticket_ports = w.board.ticket("SHOP-1").await.unwrap().work.unwrap();
    assert_ne!(Some(port), ticket_ports.agent_port);
    assert!(
        out.contains("You are the helper for piece 3 of SHOP-1, Wishlist page."),
        "{out}"
    );

    let comments = w.board.ticket_comments("SHOP-1").await.unwrap().comments;
    assert_eq!(
        comments.last().unwrap().body,
        "To 1.3: Use the API from piece 1."
    );
    let history = w.board.history("SHOP-1").await.unwrap();
    assert!(
        history
            .iter()
            .any(|e| e.kind == otaq_core::board::tickets::EventKind::Helper
                && e.payload["what"] == "started")
    );

    // A second start of a working helper is refused; a piece 0 and an empty title too.
    let again = w
        .helpers
        .start(&caller, "SHOP-1", 3, "Wishlist page", None)
        .await
        .unwrap_err();
    assert!(refusal(again).contains("already working"));
    assert!(
        w.helpers
            .start(&caller, "SHOP-1", 0, "x", None)
            .await
            .is_err()
    );
    assert!(
        w.helpers
            .start(&caller, "SHOP-1", 4, "  ", None)
            .await
            .is_err()
    );
    world.w.stop().await;
}

/// Only the agent in charge runs helper commands: not the user, not another session of the ticket, not a helper.
#[tokio::test(flavor = "multi_thread")]
async fn only_the_agent_in_charge_runs_its_helpers() {
    let world = world().await;
    let w = &world.w;
    let (t, _, _agent, caller) = at_work(w, "Wishlist").await;
    let helper = start(w, &t, &caller, 1, "Wishlist API").await;
    let in_charge = "Only the agent in charge of SHOP-1 starts, sends back and merges its helpers";
    let other = Caller::Agent {
        ticket: t.key.clone(),
        ticket_id: t.id.clone(),
        session: "not-the-live-one".into(),
    };
    for who in [Caller::User, other, helper_caller(&t, &helper)] {
        let e = w.helpers.list(&who, "SHOP-1").await.unwrap_err();
        assert_eq!(refusal(e), in_charge, "{who:?}");
        let e = w
            .helpers
            .start(&who, "SHOP-1", 2, "Wishlist store", None)
            .await
            .unwrap_err();
        assert_eq!(refusal(e), in_charge, "{who:?}");
    }
    world.w.stop().await;
}

/// `list` and `diff`: the helper's piece, title, status and branch; its branch against the ticket's.
#[tokio::test(flavor = "multi_thread")]
async fn list_and_diff() {
    let world = world().await;
    let w = &world.w;
    let (t, _, _agent, caller) = at_work(w, "Wishlist").await;
    let one = start(w, &t, &caller, 1, "Wishlist API").await;
    start(w, &t, &caller, 2, "Wishlist page").await;
    commit(Path::new(&one.worktree), "wishlist.rs", "pub fn add() {}\n");

    let list = w.helpers.list(&caller, "SHOP-1").await.unwrap();
    let rows: Vec<(String, &str, HelperStatus)> = list
        .iter()
        .map(|h| (h.label(), h.title.as_str(), h.status))
        .collect();
    assert_eq!(
        rows,
        [
            ("1.1".to_owned(), "Wishlist API", HelperStatus::Working),
            ("1.2".to_owned(), "Wishlist page", HelperStatus::Working),
        ]
    );

    let diff = w.helpers.diff(&caller, "SHOP-1", 1).await.unwrap();
    assert_eq!(diff.against, otaq_core::naming::branch("SHOP-1"));
    let files: Vec<&str> = diff.files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(files, ["wishlist.rs"]);
    assert!(diff.patch.contains("+pub fn add() {}"), "{}", diff.patch);
    let none = w.helpers.diff(&caller, "SHOP-1", 2).await.unwrap();
    assert!(none.files.is_empty());
    assert!(w.helpers.diff(&caller, "SHOP-1", 9).await.is_err());
    world.w.stop().await;
}

/// `back` queues the note for the helper's next idle moment (never into a running turn, d0048) and sets the status.
#[tokio::test(flavor = "multi_thread")]
async fn back_queues_the_note_and_sets_the_status() {
    let world = world().await;
    let w = &world.w;
    let (t, _, _agent, caller) = at_work(w, "Wishlist").await;
    let helper = start(w, &t, &caller, 1, "Wishlist API").await;
    let reply = w
        .helpers
        .back(
            &caller,
            "SHOP-1",
            1,
            "Add a test for an empty wishlist.\nThen hand back.",
        )
        .await
        .unwrap();
    assert_eq!(reply.helper.status, HelperStatus::SentBack);
    assert_eq!(
        reply.helper.note.as_deref(),
        Some("Add a test for an empty wishlist.\nThen hand back.")
    );
    // The helper's turn never ended (no hook): the note waits.
    let pending: Vec<String> =
        sqlx::query_scalar("SELECT text FROM pending_deliveries WHERE session_id = ?")
            .bind(helper.session.as_deref().unwrap())
            .fetch_all(w.board.store().pool())
            .await
            .unwrap();
    assert_eq!(
        pending,
        ["Add a test for an empty wishlist.\nThen hand back."]
    );
    let comments = w.board.ticket_comments("SHOP-1").await.unwrap().comments;
    assert!(
        comments
            .last()
            .unwrap()
            .body
            .starts_with("To 1.1: Add a test")
    );
    world.w.stop().await;
}

/// `merge`: refused while either worktree has uncommitted files; refused on a conflict with the note to send (nothing
/// merged, both branches as they were); clean, one squash commit on the ticket's branch with the files recorded, the
/// helper's session ended, its worktree removed and its branch kept.
#[tokio::test(flavor = "multi_thread")]
async fn merge_refuses_then_squashes() {
    let world = world().await;
    let w = &world.w;
    let (t, worktree, _agent, caller) = at_work(w, "Wishlist").await;
    let helper = start(w, &t, &caller, 1, "Wishlist API").await;
    let helper_wt = PathBuf::from(&helper.worktree);
    commit(&helper_wt, "wishlist.rs", "pub fn add() {}\n");
    commit(&helper_wt, "README.md", "Wishlists.\n");

    std::fs::write(worktree.join("scratch.txt"), "x").unwrap();
    let e = w.helpers.merge(&caller, "SHOP-1", 1).await.unwrap_err();
    assert_eq!(refusal(e), "SHOP-1 has 1 uncommitted file");
    std::fs::remove_file(worktree.join("scratch.txt")).unwrap();
    std::fs::write(helper_wt.join("wip.txt"), "x").unwrap();
    let e = w.helpers.merge(&caller, "SHOP-1", 1).await.unwrap_err();
    assert_eq!(refusal(e), "1.1 has 1 uncommitted file");
    std::fs::remove_file(helper_wt.join("wip.txt")).unwrap();

    // The ticket's branch changed README.md meanwhile: a conflict merges nothing.
    commit(&worktree, "README.md", "Shop.\n");
    let ticket_before = head(&worktree, "HEAD");
    let helper_before = head(&helper_wt, "HEAD");
    let e = w.helpers.merge(&caller, "SHOP-1", 1).await.unwrap_err();
    let text = refusal(e);
    assert!(
        text.starts_with("1.1 conflicts with ") && text.contains("README.md: nothing was merged"),
        "{text}"
    );
    assert!(text.contains("helper back 1 --stdin"), "{text}");
    assert_eq!(head(&worktree, "HEAD"), ticket_before);
    assert_eq!(head(&helper_wt, "HEAD"), helper_before);
    assert!(git(&helper_wt, &["status", "--porcelain"]).is_empty());

    // The helper resolves it its own way, without its README: now it merges clean.
    git(&helper_wt, &["reset", "--quiet", "--hard", "HEAD~1"]);
    let reply = w.helpers.merge(&caller, "SHOP-1", 1).await.unwrap();
    assert_eq!(reply.helper.status, HelperStatus::Merged);
    assert_eq!(reply.helper.files, ["wishlist.rs"]);
    assert!(
        reply.text.starts_with("Merged 1.1 (Wishlist API) into "),
        "{}",
        reply.text
    );
    assert_eq!(
        git(&worktree, &["log", "-1", "--format=%s"]),
        "Wishlist API (1.1)"
    );
    assert_eq!(
        head(&worktree, "HEAD~1"),
        ticket_before,
        "one squash commit"
    );
    assert!(worktree.join("wishlist.rs").exists());
    assert!(!helper_wt.exists(), "its worktree is removed");
    let repo = w.repos.root().join("shop-web");
    assert!(
        !git(&repo, &["branch", "--list", &helper.branch]).is_empty(),
        "its branch stays"
    );
    let session = w.sessions.info(helper.session.as_deref().unwrap()).unwrap();
    assert!(!session.is_running());
    assert!(w.helpers.merge(&caller, "SHOP-1", 1).await.is_err());

    // The Files tab tags the file with its piece.
    let changes = w.board.files_changes("SHOP-1").await.unwrap();
    let tagged = changes
        .files
        .iter()
        .find(|f| f.file.path == "wishlist.rs")
        .expect("wishlist.rs changed");
    assert_eq!(tagged.tags.len(), 1);
    assert_eq!(tagged.tags[0].piece, 1);
    world.w.stop().await;
}

/// A helper's `done` hands its piece back (refused with uncommitted files, as `stage next` is), its text its comment;
/// every command that moves the ticket is refused to it.
#[tokio::test(flavor = "multi_thread")]
async fn a_helper_hands_back_and_moves_nothing() {
    let world = world().await;
    let w = &world.w;
    let (t, _, _agent, caller) = at_work(w, "Wishlist").await;
    let helper = start(w, &t, &caller, 2, "Wishlist page").await;
    let me = helper_caller(&t, &helper);
    std::fs::write(Path::new(&helper.worktree).join("page.tsx"), "x").unwrap();
    let e = w.helpers.done(&me, "Page done.").await.unwrap_err();
    assert_eq!(
        refusal(e),
        "1 file is not committed. Commit it, or discard it if it's a scratch file"
    );
    commit(Path::new(&helper.worktree), "page.tsx", "x");
    let reply = w.helpers.done(&me, "Page done, with tests.").await.unwrap();
    assert_eq!(reply.helper.status, HelperStatus::BeingReviewed);
    let last = w.board.ticket_comments("SHOP-1").await.unwrap().comments;
    let last = last.last().unwrap();
    assert_eq!(last.body, "Page done, with tests.");
    assert!(
        last.author_label().ends_with("1.2"),
        "{}",
        last.author_label()
    );
    // The agent in charge hears it when it is idle (its turn never ended here: it waits).
    let Caller::Agent { session, .. } = &caller else {
        unreachable!()
    };
    let told: Vec<String> =
        sqlx::query_scalar("SELECT text FROM pending_deliveries WHERE session_id = ?")
            .bind(session)
            .fetch_all(w.board.store().pool())
            .await
            .unwrap();
    assert_eq!(
        told,
        [otaq_core::helpers::handed_back_text(
            "1.2",
            2,
            "Page done, with tests."
        )]
    );
    assert!(
        told[0].starts_with("1.2 handed its piece back: Page done, with tests. Read it with "),
        "{told:?}"
    );
    // The agent in charge can't hand back.
    assert!(w.helpers.done(&caller, "x").await.is_err());

    // Its prime: its piece first, then the notes, the ticket and the whole Plan.
    sqlx::query("UPDATE tickets SET plan = ? WHERE id = ?")
        .bind(
            "Wishlists in two steps.\n\n## Pieces\n\n1. Wishlist API \u{2014} waits for nothing\n\
             2. Wishlist page \u{2014} waits for 1\n   The page lists the saved products.\n",
        )
        .bind(&t.id)
        .execute(w.board.store().pool())
        .await
        .unwrap();
    let prime = w.helpers.prime(&me).await.unwrap();
    let text = otaq_core::helpers::prime::render(&prime);
    assert!(text.starts_with("# 1.2 \u{b7} Wishlist page"), "{text}");
    let piece = text.find("## Your piece\n\n1.2 Wishlist page \u{2014} waits for 1.1\n\nThe page lists the saved products.");
    let plan = text.find("## Plan\n\nWishlists in two steps.");
    assert!(piece.is_some() && plan.is_some() && piece < plan, "{text}");
    assert!(
        text.contains("## Notes between you and the agent in charge"),
        "{text}"
    );

    let facts = TicketFacts {
        id: t.id.clone(),
        label: "SHOP-1".into(),
        created_by: t.created_by.clone(),
        backlog: false,
        never_started: false,
    };
    for action in [
        Action::ChangeTicket(facts.clone()),
        Action::Verdict(facts.clone()),
        Action::Helpers(facts.clone()),
        Action::EditTicket(facts.clone()),
        Action::CreateTicket,
    ] {
        let e = authorize(&me, &action).unwrap_err();
        assert_eq!(e.to_string(), HELPER_ONLY_ITS_PIECE, "{action:?}");
    }
    assert!(authorize(&me, &Action::Read).is_ok());
    assert!(authorize(&me, &Action::HandBack(facts.clone())).is_ok());
    assert!(authorize(&caller, &Action::HandBack(facts)).is_err());
    world.w.stop().await;
}

/// Two helpers at once: the ticket still takes one slot (d0133 audit 4).
#[tokio::test(flavor = "multi_thread")]
async fn two_helpers_take_one_slot() {
    let world = world().await;
    let w = &world.w;
    let (t, _, _agent, caller) = at_work(w, "Wishlist").await;
    let (a, b) = tokio::join!(
        w.helpers.start(&caller, "SHOP-1", 1, "Wishlist API", None),
        w.helpers.start(&caller, "SHOP-1", 2, "Wishlist page", None),
    );
    assert_eq!(a.unwrap().helper.status, HelperStatus::Working);
    assert_eq!(b.unwrap().helper.status, HelperStatus::Working);
    assert_eq!(w.helpers.running_sessions(&t.id).len(), 2);
    assert_eq!(w.scheduler.status().await.unwrap().holding, 1);
    world.w.stop().await;
}

/// A tool at its limit holds a helper's start ("Waiting for limit reset"): nothing is made; the reply tells the agent
/// in charge. Once the limit goes, it starts.
#[tokio::test(flavor = "multi_thread")]
async fn the_plan_limit_holds_a_helper() {
    let world = world().await;
    let w = &world.w;
    let (t, _, _agent, caller) = at_work(w, "Wishlist").await;
    let hit = otaq_core::usage::limit::LimitHit {
        kind: otaq_core::usage::limit::LimitKind::Session,
        text: "5-hour limit reached".into(),
    };
    w.plan_limit
        .record_hit_reset(Tool::Claude, &hit, None, otaq_core::store::now_ms())
        .await
        .unwrap();
    let reply = w
        .helpers
        .start(&caller, "SHOP-1", 1, "Wishlist API", None)
        .await
        .unwrap();
    assert_eq!(reply.helper.status, HelperStatus::Held);
    assert_eq!(
        reply.helper.held_reason.as_deref(),
        Some("Claude Code limit reached")
    );
    assert!(
        reply.text.contains("waiting for limit reset"),
        "{}",
        reply.text
    );
    assert!(!Path::new(&reply.helper.worktree).exists(), "nothing made");
    assert!(w.helpers.running_sessions(&t.id).is_empty());

    w.plan_limit.resume_tool(Tool::Claude).await.unwrap();
    // The lift starts it (the gate's restarts) or this call does; whichever comes second finds nothing left to start,
    // never a second session beside the first (T-0538).
    let started = w.helpers.start_held().await.unwrap();
    assert!(started <= 1, "{started}");
    tokio::time::timeout(Duration::from_secs(20), async {
        while w.helpers.running_sessions(&t.id).is_empty() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the held helper starts");
    assert_eq!(w.helpers.start_held().await.unwrap(), 0);
    assert_eq!(w.helpers.running_sessions(&t.id).len(), 1);
    let now = &w.helpers.of_ticket(&t.id).await.unwrap()[0];
    assert_eq!(now.status, HelperStatus::Working);
    assert!(Path::new(&now.worktree).exists());
    world.w.stop().await;
}

/// Tokens add up on the ticket, split "in charge · helpers · checks".
#[tokio::test(flavor = "multi_thread")]
async fn tokens_add_up_by_who_spent_them() {
    let world = world().await;
    let w = &world.w;
    let t = w.create("SHOP", "Wishlist").await;
    let project = w.board.project("SHOP").await.unwrap();
    for (id, kind, input) in [
        ("s-agent", "agent", 1000),
        ("s-helper-1", "helper", 300),
        ("s-helper-2", "helper", 200),
        ("s-check", "validator", 50),
    ] {
        sqlx::query(
            "INSERT INTO sessions (id, project_id, ticket_id, kind, tool, tool_session_id, state, started_at)
             VALUES (?, ?, ?, ?, 'claude', ?, 'ended', 0)",
        )
        .bind(id)
        .bind(&project.id)
        .bind(&t.id)
        .bind(kind)
        .bind(format!("tool-{id}"))
        .execute(w.board.store().pool())
        .await
        .unwrap();
        sqlx::query("INSERT INTO usage (session_id, day, input, cached, output) VALUES (?, '2026-10-25', ?, 0, 0)")
            .bind(id)
            .bind(input)
            .execute(w.board.store().pool())
            .await
            .unwrap();
    }
    let usage = w.usage.tickets("SHOP").await.unwrap();
    let usage = usage.iter().find(|u| u.id == t.id).unwrap();
    assert_eq!(usage.tokens.unwrap().input, 1550);
    assert_eq!(usage.helpers.unwrap().input, 500);
    assert_eq!(usage.validation.unwrap().input, 50);
    assert_eq!(usage.in_charge().unwrap().input, 1000);
    assert_eq!(usage.agent().unwrap().input, 1500);
    assert_eq!(w.usage.day_total("2026-10-25").await.unwrap().input, 1550);
    world.w.stop().await;
}

/// A helper's worktree is in use while it works; left behind on a finished ticket, Cleanup lists it under its ticket;
/// Done removes what is left (worktree and branch).
#[tokio::test(flavor = "multi_thread")]
async fn a_worktree_left_behind_shows_in_cleanup_and_done_removes_it() {
    let world = world().await;
    let w = &world.w;
    let (t, _, _agent, caller) = at_work(w, "Wishlist").await;
    let helper = start(w, &t, &caller, 1, "Wishlist API").await;
    let listed = |items: &[otaq_core::recovery::cleanup::CleanupItem]| {
        items
            .iter()
            .filter(|i| {
                i.path.as_deref() == Some(helper.worktree.as_str())
                    || i.label.contains(&helper.branch)
            })
            .map(|i| (i.group, i.ticket.as_ref().map(ToString::to_string)))
            .collect::<Vec<_>>()
    };
    assert!(
        listed(&w.cleanup.scan().await.unwrap()).is_empty(),
        "in use"
    );

    w.helpers.stop_all(&t.id).await.unwrap();
    w.force_status(&t, Status::Merged).await;
    let items = w.cleanup.scan().await.unwrap();
    let rows = listed(&items);
    assert!(
        rows.contains(&(CleanupGroup::Worktrees, Some("SHOP-1".to_owned()))),
        "{rows:?}"
    );

    w.helpers.finished(&t.id).await.unwrap();
    assert!(!Path::new(&helper.worktree).exists());
    let repo = w.repos.root().join("shop-web");
    assert!(git(&repo, &["branch", "--list", &helper.branch]).is_empty());
    assert!(listed(&w.cleanup.scan().await.unwrap()).is_empty());
    world.w.stop().await;
}

/// Two helpers starting side by side, several times: each worktree gets its own skill copies before its tool starts,
/// so neither tool ever lists a copy the other's start is replacing (M44's lesson, T-0527).
// Unix only: it runs the `sh` stand-in agent tool (`common::stand_in`).
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn two_helpers_starting_together_each_load_their_skills() {
    let world = stand_in_world().await;
    let w = &world.w;
    for round in 0..3 {
        let (t, _, agent, caller) = at_work(w, &format!("Wishlist {round}")).await;
        let key = t.key.to_string();
        let (a, b) = tokio::join!(
            w.helpers.start(&caller, &key, 1, "Wishlist API", None),
            w.helpers.start(&caller, &key, 2, "Wishlist page", None),
        );
        let mut loaded = Vec::new();
        for helper in [a.unwrap().helper, b.unwrap().helper] {
            let out = w
                .wait_for_output(
                    helper.session.as_deref().unwrap(),
                    "STAND-IN claude loaded:",
                )
                .await;
            let line = out
                .lines()
                .find(|l| l.starts_with("STAND-IN claude loaded:"))
                .unwrap()
                .to_owned();
            assert!(line.contains(" otaq-restack "), "round {round}: {line}");
            // The stage's skill too: the helper runs it, with OTAQ_PIECE set (T-0539).
            assert!(line.contains(" otaq-work-ticket "), "round {round}: {line}");
            loaded.push(line);
        }
        assert_eq!(loaded[0], loaded[1], "round {round}");
        w.helpers.stop_all(&t.id).await.unwrap();
        w.sessions.stop(&agent.session).await.unwrap();
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    world.w.stop().await;
}

/// `board.helpers` (T-0541): the user reads every helper of the project's tickets, by ticket then piece, for the card's
/// piece rows and the sheet's Session list; `helper.list` stays the agent in charge's.
#[tokio::test(flavor = "multi_thread")]
async fn the_user_reads_the_projects_helpers() {
    use otaq_core::client::{Client, ClientConfig};
    use otaq_core::protocol::{ProjectParams, Reply, Request};
    let world = world().await;
    let w = &world.w;
    let (first, _, _a, first_caller) = at_work(w, "Wishlist").await;
    let (second, _, _b, second_caller) = at_work(w, "Checkout").await;
    start(w, &second, &second_caller, 1, "Address step").await;
    start(w, &first, &first_caller, 2, "Wishlist button").await;
    start(w, &first, &first_caller, 1, "Wishlist API").await;

    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    let Reply::Helpers(helpers) = client
        .request(Request::BoardHelpers(ProjectParams {
            project: "SHOP".into(),
        }))
        .await
        .unwrap()
    else {
        panic!("not helpers");
    };
    let seen: Vec<String> = helpers
        .iter()
        .map(|h| format!("{} {}", h.label(), h.title))
        .collect();
    assert_eq!(
        seen,
        [
            "1.1 Wishlist API",
            "1.2 Wishlist button",
            "2.1 Address step"
        ]
    );
    assert!(
        client
            .request(Request::HelperList(otaq_core::protocol::KeyParams {
                key: first.key.to_string(),
            }))
            .await
            .is_err(),
        "helper.list stays the agent in charge's"
    );
}
