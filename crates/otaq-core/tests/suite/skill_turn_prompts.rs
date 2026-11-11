//! A permission prompt during a delivery skill turn reads Needs you while the ticket stays in Review: Push to PR's
//! skill, run in the ticket's live agent session, stops at a prompt; the ticket keeps its status and its column, its
//! card, item and PR dialog say the prompt, and the user's key in its terminal brings it back to Ready for you. Send
//! back, a drag back to Dev and Push to PR over the open prompt say a permission prompt waits (not "unsent text"), and
//! Approve & merge still goes through. The message writer's prompt keeps Review calm (shown only in the Approve
//! dialogs), also while an Open PR dialog shows its proposal.
//!
//! An `sh` stands in for the agent session. The skill's prompt arrives as Codex's own `PermissionRequest` hook, the
//! recorded payload (`tests/fixtures/codex/hooks/PermissionRequest.json`) with the push as its command; the screen
//! shows no dialog, so the only prompt open is the one on screen, as when Codex draws it as its hook comes. "y" alone
//! answers it, as Codex takes it. The writer's prompt is Claude Code's `NeedsYou` event, as its own test has it.
#![cfg(unix)]

use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::agent_tools::{LaunchContext, Tool, ToolEvent};
use otaq_core::board::moves::DropOn;
use otaq_core::board::needs_you::NeedsYouGroup;
use otaq_core::board::projects::AddProject;
use otaq_core::board::tickets::{NewTicketInput, Ticket};
use otaq_core::caller::Caller;
use otaq_core::client::{Client, ClientConfig};
use otaq_core::domain::{Actor, PromptTurn, Status};
use otaq_core::git::testing::commit;
use otaq_core::pr::service::PrOutcome;
use otaq_core::protocol::{InputParams, KeyParams, Reply, Request, encode_bytes};
use otaq_core::review::drag::DragAnswers;
use otaq_core::review::{LocalMergeOutcome, ReturnOutcome};
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::{AgentShell, TestService, said};
use otaq_core::session::SessionKind;

const PUSH_COMMAND: &str = "git push origin otaq/SHOP-1";
const PUSH: &str = "Run command: git push origin otaq/SHOP-1";

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
                    "exec sleep 60".into(),
                    "sh".into(),
                ],
                config_dir: None,
            },
        }),
        ..config
    })
    .await
}

/// SHOP-1 with one commit, its agent's session live, submitted to Review.
async fn in_review(w: &TestService, pr: Option<i64>) -> (Ticket, AgentShell) {
    let repo = w.repos.clone_with_origin("shop-web", "main", 1);
    w.board
        .add_project(&AddProject {
            path: repo.display().to_string(),
            ..Default::default()
        })
        .await
        .unwrap();
    let t = w
        .board
        .create_ticket(
            &NewTicketInput {
                project: "SHOP".into(),
                title: "Checkout timeout".into(),
                ..Default::default()
            },
            &me(),
        )
        .await
        .unwrap();
    let worktree = w.start_worktree(&t).await;
    commit(&worktree, "api.ts", "retry\n");
    if let Some(n) = pr {
        sqlx::query("UPDATE tickets SET pr_number = ? WHERE id = ?")
            .bind(n)
            .bind(&t.id)
            .execute(w.board.store().pool())
            .await
            .unwrap();
    }
    let shell = w.agent_shell(&t).await;
    w.force_status(&t, Status::Working).await;
    let key = t.key.to_string();
    w.board
        .submit_for_review(&key, said(&w.board, &key, "done").await.as_ref(), &me())
        .await
        .unwrap();
    let t = w.board.ticket(&key).await.unwrap();
    assert_eq!(t.status, Status::WaitingForYou);
    (t, shell)
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
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
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

/// The session's tool stops at `prompt`.
async fn asks(w: &TestService, t: &Ticket, session: &str, prompt: &str) {
    w.signals
        .apply_events(
            &caller(t, session),
            Tool::Claude,
            &[ToolEvent::NeedsYou {
                prompt: prompt.into(),
                helper_agent: false,
            }],
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

async fn client(w: &TestService) -> Client {
    Client::connect(ClientConfig::new(&w.data)).await.unwrap()
}

async fn pr_flow(w: &TestService, t: &Ticket) -> otaq_core::pr::flow::PrFlowView {
    let key = KeyParams {
        key: t.key.to_string(),
    };
    match client(w)
        .await
        .request(Request::TicketPrFlow(key))
        .await
        .unwrap()
    {
        Reply::PrFlow(v) => *v,
        other => panic!("{other:?}"),
    }
}

/// A key typed into the session's terminal, as the app sends it.
async fn key(w: &TestService, session: &str, bytes: &'static [u8]) {
    client(w)
        .await
        .request(Request::SessionInput(InputParams {
            session: session.to_owned(),
            data: encode_bytes(bytes),
            reply: false,
            if_empty: false,
        }))
        .await
        .unwrap();
}

/// Push to PR on a ticket with a PR, in its live agent session.
async fn push_started(w: &TestService, t: &Ticket) {
    let outcome = w
        .prs
        .open(&t.key.to_string(), true, false, &me())
        .await
        .unwrap();
    assert!(matches!(outcome, PrOutcome::Started { .. }), "{outcome:?}");
}

/// Push to PR's skill stops at a prompt: Needs you on the item (naming the command), the ticket in Review with its
/// status, the PR dialog says it with the session to answer in; "y" in the terminal brings Ready for you back.
#[tokio::test(flavor = "multi_thread")]
async fn a_push_skills_prompt_reads_needs_you_while_the_ticket_stays_in_review() {
    let w = world().await;
    let (t, shell) = in_review(&w, Some(481)).await;
    let calm = items(&w, &t).await;
    assert_eq!(calm.len(), 1);
    assert_eq!(calm[0].0, NeedsYouGroup::Review);
    push_started(&w, &t).await;

    codex_asks(&w, &t, &shell.session, PUSH_COMMAND).await;
    let ticket = w.board.ticket(&t.key.to_string()).await.unwrap();
    assert_eq!(
        ticket.status,
        Status::WaitingForYou,
        "the status never changes"
    );
    assert_eq!(ticket.place, t.place, "nor its column");
    let waiting = ticket.session_prompt().expect("the mark");
    assert_eq!(waiting.prompt, PUSH);
    assert_eq!(waiting.session, shell.session);
    assert_eq!(waiting.turn, PromptTurn::PushToPr);
    assert!(ticket.shows_needs_you() && ticket.needs_user());
    assert_eq!(
        ticket.flags.prompts_drawn_in,
        std::slice::from_ref(&shell.session)
    );
    assert_eq!(
        items(&w, &t).await,
        [(
            NeedsYouGroup::Family,
            Status::NeedsYou,
            format!("permission: {PUSH}")
        )]
    );
    let flow = pr_flow(&w, &t).await;
    assert_eq!(flow.waits.as_ref().map(|p| p.prompt.as_str()), Some(PUSH));
    assert_eq!(flow.asks_in, None, "not the writer's");

    // An arrow key (an escape sequence) answers nothing; "y" does.
    key(&w, &shell.session, b"\x1b[B").await;
    assert!(
        w.board
            .ticket(&t.key.to_string())
            .await
            .unwrap()
            .session_prompt()
            .is_some()
    );
    key(&w, &shell.session, b"y").await;
    let ticket = w.board.ticket(&t.key.to_string()).await.unwrap();
    assert_eq!(ticket.session_prompt(), None);
    assert!(ticket.flags.prompts_drawn_in.is_empty());
    assert_eq!(ticket.status, Status::WaitingForYou);
    assert_eq!(items(&w, &t).await, calm, "Ready for you again");
    assert_eq!(pr_flow(&w, &t).await.waits, None);
    w.stop().await;
}

/// Any other signal of the tool that moves its turn on, or the session ending, takes the mark away.
#[tokio::test(flavor = "multi_thread")]
async fn the_mark_goes_when_the_turn_moves_on_or_the_session_ends() {
    let w = world().await;
    let (t, shell) = in_review(&w, Some(481)).await;
    push_started(&w, &t).await;
    codex_asks(&w, &t, &shell.session, PUSH_COMMAND).await;
    w.signals
        .apply_events(
            &caller(&t, &shell.session),
            Tool::Claude,
            &[ToolEvent::NewTurn { prompt: None }],
        )
        .await
        .unwrap();
    let key = t.key.to_string();
    assert_eq!(w.board.ticket(&key).await.unwrap().session_prompt(), None);

    codex_asks(&w, &t, &shell.session, PUSH_COMMAND).await;
    assert!(
        w.board
            .ticket(&key)
            .await
            .unwrap()
            .session_prompt()
            .is_some()
    );
    w.sessions.stop(&shell.session).await.unwrap();
    let gone = tokio::time::timeout(std::time::Duration::from_secs(20), async {
        while w
            .board
            .ticket(&key)
            .await
            .unwrap()
            .session_prompt()
            .is_some()
        {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await;
    assert!(gone.is_ok(), "the session's end takes its prompt with it");
    w.stop().await;
}

/// Send back, a drag back to Dev and Push to PR over the open prompt hold back, naming the prompt rather than unsent
/// text; nothing changed.
#[tokio::test(flavor = "multi_thread")]
async fn clicks_over_the_open_prompt_name_the_prompt() {
    let w = world().await;
    let (t, shell) = in_review(&w, Some(481)).await;
    push_started(&w, &t).await;
    codex_asks(&w, &t, &shell.session, PUSH_COMMAND).await;
    let key = t.key.to_string();
    let named = ReturnOutcome::NeedsConfirmation {
        ticket: t.key.clone(),
        unclearable_by: None,
        prompt: Some(PUSH.to_owned()),
    };

    let sent_back = w
        .review
        .fail(&key, "Keep the old timeout.", Vec::new(), false, &me())
        .await
        .unwrap();
    assert_eq!(sent_back, named, "Send back");
    let dragged = w
        .review
        .drag(
            &key,
            &DropOn::stage("dev"),
            DragAnswers {
                note: "Keep the old timeout.",
                confirmed: true,
                ..DragAnswers::default()
            },
            &w.validation,
            &me(),
        )
        .await
        .unwrap();
    assert_eq!(dragged, named, "drag back to Dev");
    let pushed = w.prs.retry(&key, false).await.unwrap();
    assert!(
        matches!(&pushed, PrOutcome::NeedsConfirmation { prompt: Some(p), .. } if p == PUSH),
        "Push to PR again: {pushed:?}"
    );
    assert_eq!(
        w.status(&key).await,
        Status::WaitingForYou,
        "nothing changed"
    );
    assert!(
        w.board
            .ticket(&key)
            .await
            .unwrap()
            .session_prompt()
            .is_some()
    );
    w.stop().await;
}

/// Approve & merge goes through over the open prompt: the status was never changed, so nothing refuses it, and the
/// merged ticket reads Merged, not Needs you.
#[tokio::test(flavor = "multi_thread")]
async fn approve_and_merge_over_the_open_prompt() {
    let w = world().await;
    let (t, shell) = in_review(&w, Some(481)).await;
    push_started(&w, &t).await;
    codex_asks(&w, &t, &shell.session, PUSH_COMMAND).await;
    let key = t.key.to_string();
    assert!(
        w.board
            .ticket(&key)
            .await
            .unwrap()
            .session_prompt()
            .is_some()
    );
    let merged = w
        .review
        .local_merge(&key, "SHOP-1: Checkout timeout", true, &me())
        .await
        .unwrap();
    assert!(
        matches!(merged, LocalMergeOutcome::Merged { .. }),
        "{merged:?}"
    );
    let ticket = w.board.ticket(&key).await.unwrap();
    assert_eq!(ticket.status, Status::Merged);
    assert_eq!(ticket.session_prompt(), None);
    assert!(items(&w, &t).await.is_empty());
    w.stop().await;
}

/// The message writer keeps Review calm while a clicked Open PR's dialog shows its proposal: its prompt shows in the
/// dialog (with its session), never as Needs you.
#[tokio::test(flavor = "multi_thread")]
async fn the_writers_prompt_under_an_open_pr_dialog_stays_in_the_dialog() {
    let w = world().await;
    let (t, _shell) = in_review(&w, None).await;
    // The remote is a local folder, so the PR is opened by the team's own skill, which the writer's tool has.
    w.board
        .edit_flow_for_tests("SHOP", |file| {
            file.flow.delivery.agent = Some("Checker".into());
            file.flow.delivery.skill = Some("my-pr".into());
        })
        .await;
    w.claude_has_skill("my-pr").await;
    let _ = w.prs.prepare_message(&t.id).await.unwrap();
    let writer = w
        .sessions
        .running_of(&t.id, SessionKind::Delivery)
        .expect("the writer's own delivery session");
    let calm = items(&w, &t).await;
    let opened = w
        .prs
        .open(&t.key.to_string(), true, false, &me())
        .await
        .unwrap();
    assert!(matches!(opened, PrOutcome::Started { .. }), "{opened:?}");

    asks(
        &w,
        &t,
        &writer.id,
        "Claude needs your permission to use Bash",
    )
    .await;
    let ticket = w.board.ticket(&t.key.to_string()).await.unwrap();
    assert_eq!(ticket.status, Status::WaitingForYou);
    assert_eq!(ticket.session_prompt(), None, "Review stays calm");
    assert_eq!(
        ticket.flags.prompts_drawn_in,
        std::slice::from_ref(&writer.id),
        "its header says it"
    );
    assert_eq!(items(&w, &t).await, calm);
    let flow = pr_flow(&w, &t).await;
    assert_eq!(flow.asks_in.as_deref(), Some(writer.id.as_str()));
    assert_eq!(flow.waits, None);
    w.stop().await;
}
