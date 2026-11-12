//! A message writer's prompt left open when the ticket leaves Review reads Needs you (re-test 7, NEW-4). On Codex the
//! writer, working in the ticket's own agent session, is often at a permission prompt when Approve & merge runs: in
//! Review that prompt keeps its calm (shown only in the Approve dialog). When the merge hits a conflict the ticket goes
//! back to Dev "Conflicts with main · fixing" in the same running session, and the prompt drawn in Review reads Needs
//! you there, naming its command, on the card, the pill and the strip; the restack waits behind it. "y" answers it, the
//! ticket reads "fixing" again, and once the writer's turn ends the restack is typed. Otaq types nothing into the
//! session to end the writer's turn. Send back over the open prompt in Review still names it and changes nothing (Review
//! stays calm), and Park over it in Dev stops the session and takes the prompt with it.
//!
//! The agent is an interactive `sh` with Codex as its tool. Its prompt arrives as Codex's own `PermissionRequest` hook,
//! the recorded payload (`tests/fixtures/codex/hooks/PermissionRequest.json`) with the writer's command; its turn end
//! is Codex's turn end as its hooks map it. "y" alone answers the prompt, as Codex takes it, and the line is then
//! cleared, as Codex's composer is empty after an approval.
#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::time::Duration;

use otaq_core::agent_tools::{Tool, ToolEvent};
use otaq_core::board::needs_you::NeedsYouGroup;
use otaq_core::board::tickets::Ticket;
use otaq_core::caller::Caller;
use otaq_core::client::{Client, ClientConfig};
use otaq_core::domain::{Actor, HandOffKind, Status};
use otaq_core::git::testing::commit;
use otaq_core::protocol::{InputParams, Request, encode_bytes};
use otaq_core::review::{LocalMergeOutcome, ReturnOutcome};
use otaq_core::service::testing::{TestService, said};
use otaq_core::session::{SessionKind, SessionTicket, SpawnSpec, Start, TerminalSize};

/// What the writer asks to run (re-test 7: SHOP-37's writer at `git log --oneline main..HEAD; …`).
const WRITER_COMMAND: &str = "git log --oneline main..HEAD";
const WRITER: &str = "Run command: git log --oneline main..HEAD";

fn me() -> Actor {
    Actor::User("me".into())
}

/// SHOP-1 waiting in Review with a commit of its own on `login.ts`, its agent's session (Codex) still running, as the
/// message writer works in it: the session's id.
async fn in_review(w: &TestService) -> (Ticket, String) {
    w.signals.set_stage_reminders(false);
    w.add_project("shop-web").await;
    let t = w.create("SHOP", "Pricing").await;
    let worktree = w.start_worktree(&t).await;
    commit(&worktree, "login.ts", "redirect\n");
    w.force_status(&t, Status::Working).await;
    let session = codex_session(w, &t).await;
    let key = t.key.to_string();
    w.board
        .submit_for_review(&key, said(&w.board, &key, "Prices.").await.as_ref(), &me())
        .await
        .unwrap();
    let t = w.board.ticket(&key).await.unwrap();
    assert_eq!(t.status, Status::WaitingForYou);
    (t, session)
}

/// An interactive `sh` as `ticket`'s agent session on Codex: its id.
async fn codex_session(w: &TestService, ticket: &Ticket) -> String {
    let project = w.board.project("SHOP").await.unwrap();
    w.sessions
        .spawn(SpawnSpec {
            kind: SessionKind::Agent,
            start: Start::User,
            project_id: project.id.clone(),
            project_key: project.key.clone(),
            ticket: Some(SessionTicket {
                id: ticket.id.clone(),
                key: ticket.key.clone(),
            }),
            program: "sh".into(),
            args: Vec::new(),
            cwd: PathBuf::from(&project.repo_path),
            env: vec![
                ("PS1".into(), String::new()),
                (
                    "PATH".into(),
                    otaq_core::service::testing::path_without_build_folders(),
                ),
            ],
            size: TerminalSize {
                cols: 160,
                rows: 40,
            },
            tool: Some(Tool::Codex),
            tool_session_id: Some("conversation-1".into()),
            model: None,
            agent: None,
            effort: None,
            theme: None,
        })
        .await
        .unwrap()
        .id
}

fn caller(t: &Ticket, session: &str) -> Caller {
    Caller::Agent {
        ticket: t.key.clone(),
        ticket_id: t.id.clone(),
        session: session.to_owned(),
    }
}

/// Codex in the session asks to run `command`: its recorded `PermissionRequest` hook.
async fn codex_asks(w: &TestService, t: &Ticket, session: &str, command: &str) {
    let mut payload: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/codex/hooks/PermissionRequest.json"),
        )
        .unwrap(),
    )
    .unwrap();
    payload["tool_input"]["command"] = command.into();
    w.signals
        .hook(
            &caller(t, session),
            Tool::Codex,
            "PermissionRequest",
            &payload,
        )
        .await
        .unwrap();
}

/// The ticket's Needs you items: (group, status, reason).
async fn items(w: &TestService, t: &Ticket) -> Vec<(NeedsYouGroup, Status, String)> {
    w.board
        .needs_you_items(0)
        .await
        .unwrap()
        .into_iter()
        .filter(|i| i.key == t.key)
        .map(|i| (i.group, i.status, i.reason))
        .collect()
}

/// "y" typed into the session's terminal, as the app sends it; then the line is empty, as Codex's composer is after
/// an approval.
async fn y(w: &TestService, session: &str) {
    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    client
        .request(Request::SessionInput(InputParams {
            session: session.to_owned(),
            data: encode_bytes(b"y"),
            reply: false,
            if_empty: false,
        }))
        .await
        .unwrap();
    w.sessions.clear_line(session, b"\x15").unwrap();
}

async fn eventually(what: &str, mut check: impl AsyncFnMut() -> bool) {
    tokio::time::timeout(Duration::from_secs(10), async {
        while !check().await {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("{what} never happened"));
}

/// The writer's prompt is open in Review (calm), and Approve & merge hits a conflict with a moved main: the ticket is
/// in Dev to be fixed, the restack waiting behind the writer's turn.
async fn conflict_over_the_writers_prompt(w: &TestService, t: &Ticket, session: &str) {
    let key = t.key.to_string();
    let calm = items(w, t).await;
    codex_asks(w, t, session, WRITER_COMMAND).await;
    let ticket = w.board.ticket(&key).await.unwrap();
    assert_eq!(ticket.status, Status::WaitingForYou, "Review stays calm");
    assert_eq!(ticket.session_prompt(), None);
    assert_eq!(items(w, t).await, calm);
    assert_eq!(
        ticket.flags.prompts_drawn_in,
        [session.to_owned()],
        "only its session's header says it"
    );

    let repo = w.board.project("SHOP").await.unwrap().repo_path;
    commit(Path::new(&repo), "login.ts", "main\n");
    let merged = w
        .review
        .local_merge(&key, "SHOP-1: Pricing", false, &me())
        .await
        .unwrap();
    assert!(
        matches!(merged, LocalMergeOutcome::Conflict { .. }),
        "{merged:?}"
    );
}

/// Re-test 7 NEW-4: the writer's prompt drawn in Review reads Needs you once the conflict sends the ticket to Dev, on
/// the card, the pill and the strip, naming its command; the restack waits behind it. "y" brings back "fixing", and
/// the writer's turn end lets the restack go out.
#[tokio::test(flavor = "multi_thread")]
async fn a_writer_prompt_left_open_reads_needs_you_once_a_conflict_sends_the_ticket_to_dev() {
    let w = TestService::start().await;
    let (t, session) = in_review(&w).await;
    let key = t.key.to_string();
    conflict_over_the_writers_prompt(&w, &t, &session).await;

    let ticket = w.board.ticket(&key).await.unwrap();
    assert_eq!(
        ticket.board_group(),
        otaq_core::domain::StatusSet::AgentStage
    );
    assert_eq!(
        ticket.status,
        Status::NeedsYou,
        "the card and the pill read Needs you"
    );
    assert_eq!(
        ticket.signal.as_ref().and_then(|s| s.text.as_deref()),
        Some(WRITER),
        "naming it"
    );
    assert_eq!(ticket.flags.conflicts_with.as_deref(), Some("main"));
    assert_eq!(ticket.flags.messages_waiting, 1, "the restack waits");
    assert_eq!(ticket.flags.hand_off, None, "nothing typed over the prompt");
    assert_eq!(
        items(&w, &t).await,
        [(
            NeedsYouGroup::Family,
            Status::NeedsYou,
            format!("permission: {WRITER}")
        )],
        "the strip"
    );

    y(&w, &session).await;
    let ticket = w.board.ticket(&key).await.unwrap();
    assert_eq!(ticket.status, Status::MergeConflictFixing, "fixing again");
    assert_eq!(w.signals.open_prompt(&session), None);
    assert!(items(&w, &t).await.is_empty());
    assert_eq!(ticket.flags.messages_waiting, 1, "still the writer's turn");

    // The writer's turn ends: the restack goes out, and the ticket stays fixing.
    let ended = ToolEvent::TurnEnded {
        background: Default::default(),
        last_message: None,
    };
    w.tool_events(&caller(&t, &session), Tool::Codex, vec![ended])
        .await;
    eventually("the restack typed", async || {
        w.board.ticket(&key).await.unwrap().flags.hand_off == Some(HandOffKind::MergeConflict)
    })
    .await;
    eventually("the waiting message gone", async || {
        w.board.ticket(&key).await.unwrap().flags.messages_waiting == 0
    })
    .await;
    assert_eq!(w.status(&key).await, Status::MergeConflictFixing);
    w.stop().await;
}

/// Decision kept: in Review the writer's prompt stays calm, and Send back over it names the prompt and changes nothing
/// (answer it in the terminal first); the ticket is still calm in Review afterwards.
#[tokio::test(flavor = "multi_thread")]
async fn send_back_over_an_open_writer_prompt_names_it_and_review_stays_calm() {
    let w = TestService::start().await;
    let (t, session) = in_review(&w).await;
    let key = t.key.to_string();
    let calm = items(&w, &t).await;
    codex_asks(&w, &t, &session, WRITER_COMMAND).await;

    let sent_back = w
        .review
        .fail(&key, "Keep the old prices.", Vec::new(), false, &me())
        .await
        .unwrap();
    assert_eq!(
        sent_back,
        ReturnOutcome::NeedsConfirmation {
            ticket: t.key.clone(),
            unclearable_by: None,
            prompt: Some(WRITER.to_owned()),
        }
    );
    let ticket = w.board.ticket(&key).await.unwrap();
    assert_eq!(ticket.status, Status::WaitingForYou, "nothing changed");
    assert_eq!(ticket.session_prompt(), None, "still calm");
    assert_eq!(items(&w, &t).await, calm);
    assert_eq!(w.signals.open_prompt(&session).as_deref(), Some(WRITER));
    w.stop().await;
}

/// Park over the writer's prompt now reading Needs you in Dev stops the agent's session, and the prompt goes with it:
/// the parked ticket reads Parked, with nothing left in Needs you.
#[tokio::test(flavor = "multi_thread")]
async fn park_over_a_writer_prompt_left_open_takes_it_away() {
    let w = TestService::start().await;
    let (t, session) = in_review(&w).await;
    let key = t.key.to_string();
    conflict_over_the_writers_prompt(&w, &t, &session).await;
    assert_eq!(w.status(&key).await, Status::NeedsYou);

    let parked = w.review.park(&key, true, &me()).await.unwrap();
    assert_eq!(parked.status, Status::Parked);
    assert!(!w.sessions.info(&session).unwrap().is_running());
    eventually("the prompt gone with its session", async || {
        let ticket = w.board.ticket(&key).await.unwrap();
        ticket.flags.prompts_drawn_in.is_empty() && ticket.session_prompt().is_none()
    })
    .await;
    assert_eq!(w.status(&key).await, Status::Parked);
    assert!(items(&w, &t).await.is_empty());
    w.stop().await;
}
