//! A helper's questions, prompts and idle, and the ticket as one (T-0538; d0133 audit answers 3 and 5, d0048, d0030,
//! d0011, d0016, d0042, d0119, d0112, d0052, d0108, d0018, d0045, d0122).
//!
//! - A helper's `ask human` goes to the agent in charge as a note; the card stays Working. `helper answer` types the
//!   answer into the helper; `helper pass-on` turns the card turquoise ("SHOP-1 · 1.2 asks", the note) and the user's
//!   answer (the answer box, or Enter in the helper's terminal) reaches the helper.
//! - A helper's permission prompt makes the ticket Needs you naming the piece, with its session for ⌘J and the chip;
//!   its Idle is a note to the agent in charge only.
//! - Park and a drag back stop every helper and keep the worktrees; a restart makes one Interrupted item, and Resume
//!   continues the agent in charge and the working helpers only.
//!
//! Agents and helpers are `sh` printing their arguments (no agent tool runs); hooks are Claude Code's real-shaped JSON
//! (`tests/fixtures/claude/hooks`); repositories, `OTAQ_HOME` and the tools' home are temporary.

use std::path::{Path, PathBuf};
use std::time::Duration;

use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::agent_tools::{LaunchContext, Tool};
use otaq_core::board::moves::{DropOn, DropTarget};
use otaq_core::board::questions::Choice;
use otaq_core::board::tickets::{EventKind, Ticket};
use otaq_core::caller::Caller;
use otaq_core::domain::{Actor, PieceWaitKind, Status};
use otaq_core::helpers::questions::HelperQuestionState;
use otaq_core::helpers::{Helper, HelperStatus};
use otaq_core::review::drag::DragAnswers;
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::TestService;
use otaq_core::session::SessionKind;
use otaq_core::session::deliveries::Delivery;
use serde_json::Value;

/// Keeps running, printing its arguments. On Windows it reads what is typed into it (`cat`): a Windows console shows
/// typed text only while a program reads it, where a Unix terminal echoes it whatever runs.
const PRINTS_ITS_ARGS: &str = if cfg!(windows) {
    "printf 'ARGS %s\\n' \"$*\"; exec cat"
} else {
    "printf 'ARGS %s\\n' \"$*\"; exec sleep 60"
};

fn me() -> Actor {
    Actor::User("me".into())
}

fn launch() -> LaunchContext {
    LaunchContext {
        bin_dir: "/nonexistent".into(),
        plugin_dir: "/nonexistent/plugin".into(),
        path: std::env::var_os("PATH").unwrap_or_default(),
        claude: ClaudeLaunch {
            program: vec![
                "sh".into(),
                "-c".into(),
                PRINTS_ITS_ARGS.into(),
                "sh".into(),
            ],
            config_dir: None,
        },
    }
}

async fn world() -> TestService {
    let w = TestService::start_with(|config| ServiceConfig {
        launch: Some(launch()),
        ..config
    })
    .await;
    w.add_project("shop-web").await;
    w
}

/// A ticket at work in Dev with its branch and worktree, and its agent in charge (an `sh` agent session).
async fn at_work(w: &TestService, title: &str) -> (Ticket, Caller) {
    let t = w.create("SHOP", title).await;
    w.start_worktree(&t).await;
    w.force_status(&t, Status::Working).await;
    let t = w.board.ticket(&t.key.to_string()).await.unwrap();
    let agent = w.agent_shell(&t).await;
    let caller = Caller::Agent {
        ticket: t.key.clone(),
        ticket_id: t.id.clone(),
        session: agent.session.clone(),
    };
    (t, caller)
}

fn helper_caller(t: &Ticket, h: &Helper) -> Caller {
    Caller::Helper {
        ticket: t.key.clone(),
        ticket_id: t.id.clone(),
        session: h.session.clone().unwrap(),
    }
}

fn session_of(caller: &Caller) -> String {
    match caller {
        Caller::Agent { session, .. } | Caller::Helper { session, .. } => session.clone(),
        other => panic!("no session: {other:?}"),
    }
}

async fn start(w: &TestService, t: &Ticket, caller: &Caller, piece: u32, title: &str) -> Helper {
    w.helpers
        .start(caller, &t.key.to_string(), piece, title, None)
        .await
        .unwrap()
        .helper
}

async fn helper(w: &TestService, t: &Ticket, piece: u32) -> Helper {
    w.helpers
        .of_ticket(&t.id)
        .await
        .unwrap()
        .into_iter()
        .find(|h| h.piece == piece)
        .unwrap()
}

/// What waits to be typed into `session` (it never ended a turn, so nothing went yet).
async fn pending(w: &TestService, session: &str) -> Vec<String> {
    sqlx::query_scalar("SELECT text FROM pending_deliveries WHERE session_id = ? ORDER BY rowid")
        .bind(session)
        .fetch_all(w.board.store().pool())
        .await
        .unwrap()
}

fn fixture(file: &str) -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/claude/hooks")
        .join(format!("{file}.json"));
    serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap()
}

/// A hook from `who`, as the service applies it: the signals, then what follows a helper's.
async fn hook(w: &TestService, who: &Caller, event: &str, file: &str) {
    let signals = w
        .signals
        .hook(who, Tool::Claude, event, &fixture(file))
        .await
        .unwrap();
    w.helpers.after_signals(who, &signals).await.unwrap();
}

/// The helper entries of History, oldest first.
async fn helper_history(w: &TestService, key: &str) -> Vec<(String, Option<String>)> {
    let mut entries: Vec<(String, Option<String>)> = w
        .board
        .history(key)
        .await
        .unwrap()
        .into_iter()
        .filter(|e| e.kind == EventKind::Helper)
        .map(|e| {
            (
                e.payload["what"].as_str().unwrap_or_default().to_owned(),
                e.payload["note"].as_str().map(str::to_owned),
            )
        })
        .collect();
    entries.reverse();
    entries
}

async fn eventually(what: &str, mut check: impl AsyncFnMut() -> bool) {
    tokio::time::timeout(Duration::from_secs(20), async {
        while !check().await {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("{what} never happened"));
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

/// A helper's question goes to the agent in charge as a note; the ticket stays Working with no Needs you item, the
/// helper's row reads "asked · in charge is answering", and History logs it. `helper answer` types the answer into
/// the helper in 0061's form, logged as the agent in charge's.
#[tokio::test(flavor = "multi_thread")]
async fn a_helpers_question_goes_to_the_agent_in_charge_who_answers_it() {
    let w = world().await;
    let (t, caller) = at_work(&w, "Wishlist").await;
    let two = start(&w, &t, &caller, 2, "Wishlist button").await;
    let reply = w
        .helpers
        .ask(
            &helper_caller(&t, &two),
            "Can guests save items?",
            &["Logged-in users only".into(), "Guests too".into()],
            &[],
            Some(1),
        )
        .await
        .unwrap();
    assert!(
        reply.text.starts_with("Asked the agent in charge"),
        "{}",
        reply.text
    );
    assert_eq!(reply.helper.asked, Some(HelperQuestionState::WithInCharge));
    assert_eq!(
        reply.helper.row_status(),
        "asked \u{b7} in charge is answering"
    );
    let open = otaq_core::helpers::questions::open_of(w.board.store().pool(), &two.id)
        .await
        .unwrap()
        .unwrap();
    let note = pending(&w, &session_of(&caller)).await;
    assert_eq!(
        note,
        [format!(
            "Piece 2's helper asks: Can guests save items? (options: 1. Logged-in users only, 2. Guests too). Answer \
             with `otaq helper answer 2 --stdin`, or pass it on with `otaq helper pass-on {}`.",
            open.question.id
        )]
    );
    let after = w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(after.status, Status::Working);
    assert!(after.question.is_none() && after.flags.piece_wait.is_none());
    assert!(w.needs_you.list().await.unwrap().items.is_empty());

    // Someone else can't answer it; the agent in charge does.
    assert!(
        w.helpers
            .answer(&Caller::User, "SHOP-1", 2, "Logged-in users only")
            .await
            .is_err()
    );
    let answered = w
        .helpers
        .answer(
            &caller,
            "SHOP-1",
            2,
            "Logged-in users only: PAY-123 says so.",
        )
        .await
        .unwrap();
    assert_eq!(answered.helper.asked, None);
    assert_eq!(answered.helper.row_status(), "working");
    assert_eq!(
        pending(&w, two.session.as_deref().unwrap()).await,
        ["Answer to your question: Logged-in users only: PAY-123 says so."]
    );
    assert_eq!(w.status("SHOP-1").await, Status::Working);
    let again = w
        .helpers
        .answer(&caller, "SHOP-1", 2, "again")
        .await
        .unwrap_err();
    assert!(
        again.to_string().contains("no question waiting for you"),
        "{again}"
    );
    assert_eq!(
        helper_history(&w, "SHOP-1").await[1..],
        [
            (
                "asked".to_owned(),
                Some("Can guests save items?".to_owned())
            ),
            (
                "answered".to_owned(),
                Some("Logged-in users only: PAY-123 says so.".to_owned())
            ),
        ]
    );
    // Answered by the agent in charge, it isn't the user's: never in Messages.
    assert!(
        w.board
            .answered_human_questions(&t.id)
            .await
            .unwrap()
            .is_empty()
    );
    w.stop().await;
}

/// `pass-on` makes the question the user's: the card turns turquoise with the piece's line, the chip and the agent in
/// charge's note, and the agent in charge's own hooks don't clear it (d0119). The user's answer from the answer box is
/// typed into the helper's session, closes it, and the card is Working again; History says the user answered.
#[tokio::test(flavor = "multi_thread")]
async fn pass_on_turns_the_card_turquoise_and_the_users_answer_reaches_the_helper() {
    let w = world().await;
    let (t, caller) = at_work(&w, "Wishlist").await;
    let two = start(&w, &t, &caller, 2, "Wishlist button").await;
    w.helpers
        .ask(
            &helper_caller(&t, &two),
            "Can guests save items?",
            &["Logged-in users only".into(), "Guests too".into()],
            &[],
            None,
        )
        .await
        .unwrap();
    let id = otaq_core::helpers::questions::open_of(w.board.store().pool(), &two.id)
        .await
        .unwrap()
        .unwrap()
        .question
        .id;
    // Not the user's to pass on, nor another ticket's agent's.
    assert!(
        w.helpers
            .pass_on(&Caller::User, "SHOP-1", &id, None)
            .await
            .is_err()
    );
    let passed = w
        .helpers
        .pass_on(
            &caller,
            "SHOP-1",
            &id,
            Some("it's a product question the plan doesn't answer"),
        )
        .await
        .unwrap();
    assert_eq!(passed.helper.row_status(), "asked \u{b7} waiting for you");

    let card = w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(card.status, Status::AsksYou);
    assert_eq!(
        card.question.as_ref().map(|q| q.text.as_str()),
        Some("Can guests save items?")
    );
    let wait = card.flags.piece_wait.clone().unwrap();
    assert_eq!(wait.kind, PieceWaitKind::Asks);
    assert_eq!(wait.label, "1.2");
    assert_eq!(wait.chip, "SHOP-1 \u{b7} 1.2 asks");
    assert_eq!(wait.line, "1.2 asks: Can guests save items?");
    assert_eq!(
        wait.note.as_deref(),
        Some("passed this to you: it's a product question the plan doesn't answer")
    );
    assert_eq!(wait.session, two.session);
    let items = w.needs_you.list().await.unwrap().items;
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].reason, "1.2 asks: Can guests save items?");
    assert_eq!(items[0].piece.as_ref(), Some(&wait));

    // The agent in charge's own turn goes on: the question still holds Asks you.
    hook(&w, &caller, "UserPromptSubmit", "UserPromptSubmit").await;
    assert_eq!(w.status("SHOP-1").await, Status::AsksYou);

    let delivery = w
        .needs_you
        .answer("SHOP-1", &Choice::Option(2), false, &me())
        .await
        .unwrap();
    assert!(matches!(delivery, Delivery::Sent { .. }), "{delivery:?}");
    w.wait_for_output(
        two.session.as_deref().unwrap(),
        "Answer to your question: Guests too (option 2).",
    )
    .await;
    let card = w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(card.status, Status::Working);
    assert!(card.flags.piece_wait.is_none());
    assert!(w.needs_you.list().await.unwrap().items.is_empty());
    assert_eq!(helper(&w, &t, 2).await.asked, None);
    let history = helper_history(&w, "SHOP-1").await;
    assert_eq!(
        history
            .iter()
            .map(|(what, _)| what.as_str())
            .collect::<Vec<_>>(),
        ["started", "asked", "passed_on", "you_answered"]
    );
    // The user's answer is logged with the question in Messages (d0030).
    let logged = w.board.answered_human_questions(&t.id).await.unwrap();
    assert_eq!(logged.len(), 1);
    assert_eq!(
        logged[0].answer,
        "Answer to your question: Guests too (option 2)."
    );
    w.stop().await;
}

/// Enter on a line in the helper's terminal answers its passed-on question (d0119), and only that one.
#[tokio::test(flavor = "multi_thread")]
async fn enter_in_the_helpers_terminal_answers_its_passed_on_question() {
    let w = world().await;
    let (t, caller) = at_work(&w, "Wishlist").await;
    let two = start(&w, &t, &caller, 2, "Wishlist button").await;
    let session = two.session.clone().unwrap();
    w.helpers
        .ask(&helper_caller(&t, &two), "Which icon?", &[], &[], None)
        .await
        .unwrap();
    // While the agent in charge has it, a line in the helper's terminal answers nothing.
    w.helpers.enter(&session, Some("a heart")).await.unwrap();
    assert_eq!(
        helper(&w, &t, 2).await.asked,
        Some(HelperQuestionState::WithInCharge)
    );
    let id = otaq_core::helpers::questions::open_of(w.board.store().pool(), &two.id)
        .await
        .unwrap()
        .unwrap()
        .question
        .id;
    w.helpers
        .pass_on(&caller, "SHOP-1", &id, None)
        .await
        .unwrap();
    assert_eq!(w.status("SHOP-1").await, Status::AsksYou);
    assert_eq!(
        w.board
            .ticket("SHOP-1")
            .await
            .unwrap()
            .flags
            .piece_wait
            .unwrap()
            .note
            .as_deref(),
        Some("passed this to you")
    );
    // A line sent in the agent in charge's own terminal is its own: the helper's question stays open.
    w.signals.enter(&t.id, Some("a star")).await.unwrap();
    assert_eq!(w.status("SHOP-1").await, Status::AsksYou);
    // An empty line answers nothing; a line does.
    w.helpers.enter(&session, Some("  ")).await.unwrap();
    assert_eq!(w.status("SHOP-1").await, Status::AsksYou);
    w.helpers.enter(&session, Some("A heart")).await.unwrap();
    assert_eq!(w.status("SHOP-1").await, Status::Working);
    let history = helper_history(&w, "SHOP-1").await;
    assert_eq!(
        history.last().unwrap(),
        &("you_answered".to_owned(), Some("A heart".to_owned()))
    );
    w.stop().await;
}

/// A helper's permission prompt goes straight to the user: the ticket is Needs you with "1.3 needs you: …", the chip
/// names the piece and ⌘J opens the helper's session; the agent in charge's hooks don't clear it, and no Allow / Deny
/// is offered (the answer box refuses it). Enter in the helper's terminal (the user answered in the tool) clears it.
#[tokio::test(flavor = "multi_thread")]
async fn a_helpers_permission_prompt_makes_the_ticket_need_you_naming_the_piece() {
    let w = world().await;
    let (t, caller) = at_work(&w, "Wishlist").await;
    let three = start(&w, &t, &caller, 3, "Wishlist page").await;
    let session = three.session.clone().unwrap();
    let helper3 = helper_caller(&t, &three);
    hook(
        &w,
        &helper3,
        "Notification",
        "Notification-permission_prompt",
    )
    .await;

    let card = w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(card.status, Status::NeedsYou);
    let line = "1.3 needs you: Claude needs your permission to use Bash";
    assert_eq!(
        card.signal.as_ref().and_then(|s| s.text.as_deref()),
        Some(line)
    );
    let wait = card.flags.piece_wait.clone().unwrap();
    assert_eq!(wait.kind, PieceWaitKind::NeedsYou);
    assert_eq!(wait.chip, "SHOP-1 \u{b7} 1.3 needs you");
    assert_eq!(wait.line, line);
    assert_eq!(wait.session.as_deref(), Some(session.as_str()));
    assert_eq!(helper(&w, &t, 3).await.row_status(), "needs you");
    let items = w.needs_you.list().await.unwrap().items;
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].reason, line);
    assert_eq!(items[0].piece.as_ref(), Some(&wait));
    // Answered in the helper's own terminal, never through Otaq (d0011, d0016).
    assert!(
        w.needs_you
            .answer("SHOP-1", &Choice::Other("yes".into()), false, &me())
            .await
            .is_err()
    );

    // The agent in charge works on: the helper's prompt still shows.
    hook(&w, &caller, "UserPromptSubmit", "UserPromptSubmit").await;
    assert_eq!(w.status("SHOP-1").await, Status::NeedsYou);
    // The agent in charge's own prompt shows first; once it is answered the helper's shows again.
    hook(
        &w,
        &caller,
        "Notification",
        "Notification-permission_prompt",
    )
    .await;
    let own = w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(
        own.signal.as_ref().and_then(|s| s.text.as_deref()),
        Some("Claude needs your permission to use Bash")
    );
    assert!(own.flags.piece_wait.is_none());
    w.signals.enter(&t.id, None).await.unwrap();
    let card = w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(
        card.signal.as_ref().and_then(|s| s.text.as_deref()),
        Some(line)
    );

    // The user answers in the helper's terminal.
    w.helpers.enter(&session, None).await.unwrap();
    let card = w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(card.status, Status::Working);
    assert!(card.flags.piece_wait.is_none());
    assert_eq!(helper(&w, &t, 3).await.prompt, None);

    // A new prompt closes with the helper's next turn too.
    hook(
        &w,
        &helper3,
        "Notification",
        "Notification-permission_prompt",
    )
    .await;
    assert_eq!(w.status("SHOP-1").await, Status::NeedsYou);
    hook(&w, &helper3, "UserPromptSubmit", "UserPromptSubmit").await;
    assert_eq!(w.status("SHOP-1").await, Status::Working);
    w.stop().await;
}

/// A helper that stops without handing back is reported to the agent in charge with its last line, never to the user;
/// one that asked or handed back isn't.
#[tokio::test(flavor = "multi_thread")]
async fn a_helpers_idle_notes_the_agent_in_charge_only() {
    let w = world().await;
    let (t, caller) = at_work(&w, "Wishlist").await;
    let three = start(&w, &t, &caller, 3, "Wishlist page").await;
    let helper3 = helper_caller(&t, &three);
    hook(&w, &helper3, "Stop", "Stop-idle").await;
    assert_eq!(
        pending(&w, &session_of(&caller)).await,
        [
            "Piece 3's helper stopped without handing back; its last line: Should I also update the tests?"
        ]
    );
    assert_eq!(w.status("SHOP-1").await, Status::Working);
    assert!(w.needs_you.list().await.unwrap().items.is_empty());

    // After its question, its Stop isn't Idle (the service marks the turn, as for a ticket's agent).
    hook(&w, &helper3, "UserPromptSubmit", "UserPromptSubmit").await;
    w.helpers
        .ask(&helper3, "Which icon?", &[], &[], None)
        .await
        .unwrap();
    w.signals.mark_submitted_or_asked(&session_of(&helper3));
    hook(&w, &helper3, "Stop", "Stop-idle").await;
    let notes = pending(&w, &session_of(&caller)).await;
    assert_eq!(notes.len(), 2, "{notes:?}");
    assert!(notes[1].starts_with("Piece 3's helper asks: Which icon?"));
    w.stop().await;
}

/// Park stops the agent in charge and every helper, keeps every worktree and the helpers' statuses, and frees their
/// ports; the parked card says which helpers come back. A drag back out of the stage stops them all too.
#[tokio::test(flavor = "multi_thread")]
async fn park_and_drag_back_stop_every_session_and_keep_the_worktrees() {
    let w = world().await;
    let (t, caller) = at_work(&w, "Wishlist").await;
    let one = start(&w, &t, &caller, 1, "Wishlist API").await;
    let two = start(&w, &t, &caller, 2, "Wishlist page").await;
    w.helpers
        .back(&caller, "SHOP-1", 2, "Add the empty state.")
        .await
        .unwrap();
    hook(
        &w,
        &helper_caller(&t, &one),
        "Notification",
        "Notification-permission_prompt",
    )
    .await;
    assert_eq!(w.status("SHOP-1").await, Status::NeedsYou);

    let parked = w.review.park("SHOP-1", true, &me()).await.unwrap();
    assert_eq!(parked.status, Status::Parked);
    let running = w.helpers.running_sessions(&t.id);
    assert!(running.is_empty(), "{running:#?}");
    assert!(w.sessions.running_of(&t.id, SessionKind::Agent).is_none());
    let helpers = w.helpers.of_ticket(&t.id).await.unwrap();
    for h in &helpers {
        assert!(PathBuf::from(&h.worktree).join(".git").exists(), "{h:?}");
        assert_eq!(h.port, None, "ports freed");
        assert_eq!(h.prompt, None, "its prompt ended with its session");
    }
    assert_eq!(
        helpers.iter().map(|h| h.status).collect::<Vec<_>>(),
        [HelperStatus::Working, HelperStatus::SentBack]
    );
    assert_eq!(parked.flags.helpers_to_resume, ["1.1", "1.2"]);
    let _ = two;

    // A drag back out of the stage (on a flow with an earlier Agent stage) stops them all as well.
    let w2 = world().await;
    let file = otaq_core::flow::yaml::read(
        &std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/flows/every-kind.yaml"),
        )
        .unwrap(),
    )
    .unwrap();
    w2.board.use_flow_for_tests("SHOP", &file).await;
    let (t2, caller2) = at_work(&w2, "Saved carts").await;
    sqlx::query("UPDATE tickets SET stage_id = 'dev' WHERE id = ?")
        .bind(&t2.id)
        .execute(w2.board.store().pool())
        .await
        .unwrap();
    let h = start(&w2, &t2, &caller2, 1, "Cart API").await;
    w2.review
        .drag(
            "SHOP-1",
            &DropOn::stage("product-spec"),
            DragAnswers {
                note: "Ask about shared carts.",
                report: None,
                confirmed: true,
                clear_first: false,
            },
            &w2.validation,
            &me(),
        )
        .await
        .unwrap();
    assert!(
        !w2.sessions
            .info(h.session.as_deref().unwrap())
            .unwrap()
            .is_running(),
        "the helper stopped with the drag"
    );
    assert!(PathBuf::from(&h.worktree).join(".git").exists());
    assert_eq!(helper(&w2, &t2, 1).await.status, HelperStatus::Working);
    w2.stop().await;
    w.stop().await;
}

/// Pause stops the helpers with the agent in charge; the ticket is one Interrupted item. After a restart too: one
/// Interrupted item, "2 helpers were working"; Resume continues the agent in charge with "Carry on" and each helper
/// that was working or sent back in its own conversation with "Carry on with piece N"; one being reviewed isn't.
#[tokio::test(flavor = "multi_thread")]
async fn a_restart_makes_one_interrupted_item_and_resume_brings_back_the_working_helpers() {
    let w = world().await;
    let t = w.create("SHOP", "Wishlist").await;
    w.board
        .move_ticket("SHOP-1", DropTarget::ReadyForDev, true, &me())
        .await
        .unwrap();
    w.starter
        .start("SHOP-1", &me(), otaq_core::session::Start::Auto)
        .await
        .unwrap();
    until_status(&w, "SHOP-1", Status::Working).await;
    let in_charge = w.sessions.running_of(&t.id, SessionKind::Agent).unwrap();
    let caller = Caller::Agent {
        ticket: t.key.clone(),
        ticket_id: t.id.clone(),
        session: in_charge.id.clone(),
    };
    let one = start(&w, &t, &caller, 1, "Wishlist API").await;
    let two = start(&w, &t, &caller, 2, "Wishlist page").await;
    let three = start(&w, &t, &caller, 3, "Wishlist button").await;
    w.helpers
        .back(&caller, "SHOP-1", 2, "Add the empty state.")
        .await
        .unwrap();
    w.helpers
        .done(&helper_caller(&t, &three), "Button done.")
        .await
        .unwrap();
    let conversation = |h: &Helper| {
        w.sessions
            .info(h.session.as_deref().unwrap())
            .unwrap()
            .tool_session_id
            .unwrap()
    };
    let (conv1, conv2) = (conversation(&one), conversation(&two));

    // Pause stops every session of the ticket; it is one Interrupted item.
    w.pause
        .pause(&otaq_core::agents::pause::PauseScope::Project {
            project: "SHOP".into(),
        })
        .await
        .unwrap();
    assert!(w.helpers.running_sessions(&t.id).is_empty());
    assert_eq!(w.status("SHOP-1").await, Status::Interrupted);
    w.pause
        .resume(
            &otaq_core::agents::pause::PauseScope::Project {
                project: "SHOP".into(),
            },
            &me(),
        )
        .await
        .unwrap();
    until_status(&w, "SHOP-1", Status::Working).await;
    eventually("the helpers back after the pause", async || {
        w.helpers.running_sessions(&t.id).len() == 2
    })
    .await;

    let w = w
        .restart_with(|config| ServiceConfig {
            launch: Some(launch()),
            ..config
        })
        .await;
    assert_eq!(w.status("SHOP-1").await, Status::Interrupted);
    assert!(w.helpers.running_sessions(&t.id).is_empty());
    let items = w.needs_you.list().await.unwrap().items;
    assert_eq!(items.len(), 1, "one item for the ticket: {items:?}");
    assert_eq!(
        items[0].reason,
        "session interrupted \u{b7} 2 helpers were working"
    );
    assert_eq!(items[0].helpers_to_resume, ["1.1", "1.2"]);
    assert_eq!(
        otaq_core::helpers::restart::interrupted_line(items[0].helpers_to_resume.len()),
        "Interrupted \u{b7} 2 helpers were working"
    );

    w.review.resume("SHOP-1", &me()).await.unwrap();
    until_status(&w, "SHOP-1", Status::Working).await;
    let resumed = w.sessions.running_of(&t.id, SessionKind::Agent).unwrap();
    let out = w.wait_for_output(&resumed.id, "ARGS").await;
    assert!(
        out.contains("You were interrupted by a restart. Carry on"),
        "{out}"
    );
    eventually("two helpers back", async || {
        w.helpers.running_sessions(&t.id).len() == 2
    })
    .await;
    for (piece, conv) in [(1, &conv1), (2, &conv2)] {
        let h = helper(&w, &t, piece).await;
        let out = w
            .wait_for_output(h.session.as_deref().unwrap(), "ARGS")
            .await;
        assert!(
            out.contains(&format!("--resume {conv}")),
            "piece {piece}: {conv} (1: {conv1}, 2: {conv2}) {h:?} {out}"
        );
        assert!(
            out.contains(&format!(
                "You were interrupted by a restart. Carry on with piece {piece}."
            )),
            "{out}"
        );
    }
    let statuses: Vec<HelperStatus> = w
        .helpers
        .of_ticket(&t.id)
        .await
        .unwrap()
        .iter()
        .map(|h| h.status)
        .collect();
    assert_eq!(
        statuses,
        [
            HelperStatus::Working,
            HelperStatus::SentBack,
            HelperStatus::BeingReviewed
        ]
    );
    let three = helper(&w, &t, 3).await;
    assert!(
        !w.sessions
            .info(three.session.as_deref().unwrap())
            .map(|s| s.is_running())
            .unwrap_or(false),
        "the helper being reviewed isn't resumed"
    );
    w.stop().await;
}
