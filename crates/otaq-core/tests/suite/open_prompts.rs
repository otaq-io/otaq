//! Every prompt an agent shows reads Needs you, one at a time (re-test 5: a checker's second Codex prompt, queued
//! behind the first, stayed "Checking" for 30 s+, and the card once named the queued command while the first was on
//! screen). Codex fires the hooks of the commands it runs together at once and draws their dialogs one after another:
//! the first reads Needs you naming its command, "y" moves Needs you to the second once it is drawn, naming it, and "y"
//! again clears it. For a checker, a helper and a ticket's agent; the agent is an ordinary `sh` with Codex as its tool,
//! and a "dialog" is Codex's text printed on its screen.

use std::path::PathBuf;
use std::time::Duration;

use otaq_core::agent_tools::{Tool, ToolEvent};
use otaq_core::board::tickets::Ticket;
use otaq_core::caller::Caller;
use otaq_core::client::{Client, ClientConfig};
use otaq_core::domain::Status;
use otaq_core::protocol::{InputParams, Request, encode_bytes};
use otaq_core::service::testing::TestService;
use otaq_core::session::{SessionKind, SessionTicket, SpawnSpec, Start, TerminalSize};
use otaq_core::signals::asks::LOOK_EVERY;

const FIRST: &str = "Get-Content package.json; Get-Content docs/one.txt";
const SECOND: &str = "git show --stat --oneline 718a765";

async fn world() -> (TestService, Ticket, Client) {
    let w = TestService::start().await;
    w.signals.set_stage_reminders(false);
    w.add_project("shop-web").await;
    let ticket = w.create("SHOP", "Add coupon codes").await;
    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    (w, ticket, client)
}

/// An interactive `sh` as `ticket`'s session of `kind` on Codex: its id.
async fn codex_session(w: &TestService, ticket: &Ticket, kind: SessionKind) -> String {
    let project = w.board.project("SHOP").await.unwrap();
    w.sessions
        .spawn(SpawnSpec {
            kind,
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
            env: vec![("PS1".into(), String::new())],
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

/// Codex's approval dialog for `command`, drawn on a cleared screen under the two commands' "Running" lines (Ctrl-U
/// first: the user's "y" is still on the shell's line).
async fn draw(w: &TestService, session: &str, command: &str) {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("dialog.txt");
    std::fs::write(
        &file,
        format!(
            "\x1b[2J\x1b[H\
             \u{2022} Running {FIRST}\r\n\
             \u{2022} Running {SECOND}\r\n\r\n\
             \x20 Would you like to run the following command?\r\n\r\n\
             \x20 Environment: local\r\n\r\n\
             \x20 $ {command}\r\n\r\n\
             \u{203a} 1. Yes, proceed (y)\r\n\
             \x20 2. Yes, and don't ask again for commands that start with `{command}` (p)\r\n\
             \x20 3. No, and tell Codex what to do differently (esc)\r\n\r\n\
             \x20 Press enter to confirm or esc to cancel\r\n"
        ),
    )
    .unwrap();
    let path = file.display().to_string().replace('\\', "/");
    w.sessions
        .write(session, format!("\x15cat '{path}'\r").as_bytes())
        .unwrap();
    let drawn = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if w.sessions
                .visible_screen(session)
                .is_some_and(|s| s.contains(&format!("$ {command}")))
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await;
    assert!(
        drawn.is_ok(),
        "never drawn: {:?}",
        w.sessions.visible_screen(session)
    );
}

/// Both commands' hooks, in the same moment, the second's first (Codex sends them in no set order).
async fn both_asked(w: &TestService, caller: &Caller) {
    for command in [SECOND, FIRST] {
        let prompt = ToolEvent::NeedsYou {
            prompt: format!("Run command: {command}"),
            helper_agent: false,
        };
        w.tool_events(caller, Tool::Codex, vec![prompt]).await;
    }
}

async fn key_in(client: &Client, session: &str, bytes: &[u8]) {
    client
        .request(Request::SessionInput(InputParams {
            session: session.to_owned(),
            data: encode_bytes(bytes),
            reply: false,
            if_empty: false,
        }))
        .await
        .unwrap();
}

/// The ticket's status and its line.
async fn card(w: &TestService, ticket: &Ticket) -> (Status, Option<String>) {
    let ticket = w.board.ticket(&ticket.key.to_string()).await.unwrap();
    (ticket.status, ticket.signal.and_then(|s| s.text))
}

/// Waits until the card reads `status`, its line naming `command` when given.
async fn until(w: &TestService, ticket: &Ticket, status: Status, command: Option<&str>) {
    let reached = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let (now, text) = card(w, ticket).await;
            if now == status
                && command.is_none_or(|c| text.as_deref().is_some_and(|t| t.contains(c)))
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await;
    assert!(
        reached.is_ok(),
        "never {status:?} naming {command:?}: {:?}",
        card(w, ticket).await
    );
}

/// While the answered dialog still shows (and while the approved command would run), the second isn't Needs you yet.
async fn stays(w: &TestService, ticket: &Ticket, status: Status) {
    tokio::time::sleep(LOOK_EVERY * 4).await;
    assert_eq!(
        card(w, ticket).await.0,
        status,
        "the second isn't drawn yet"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_checkers_two_prompts_read_needs_you_one_at_a_time() {
    let (w, ticket, client) = world().await;
    let session = codex_session(&w, &ticket, SessionKind::Validator).await;
    w.validating_with(&ticket, &session).await;
    let checker = Caller::Validator {
        ticket: ticket.key.clone(),
        ticket_id: ticket.id.clone(),
        session: session.clone(),
    };

    draw(&w, &session, FIRST).await;
    both_asked(&w, &checker).await;
    until(&w, &ticket, Status::CheckerNeedsYou, Some(FIRST)).await;
    let open = w.signals.open_prompts(&session).unwrap();
    assert_eq!(open.open().len(), 2, "{open:?}");

    key_in(&client, &session, b"y").await;
    until(&w, &ticket, Status::Checking, None).await;
    stays(&w, &ticket, Status::Checking).await;
    draw(&w, &session, SECOND).await;
    until(&w, &ticket, Status::CheckerNeedsYou, Some(SECOND)).await;

    key_in(&client, &session, b"y").await;
    until(&w, &ticket, Status::Checking, None).await;
    assert_eq!(w.signals.open_prompt(&session), None);
    assert!(w.signals.open_prompts(&session).unwrap().open().is_empty());
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_helpers_two_prompts_read_needs_you_one_at_a_time() {
    let (w, ticket, client) = world().await;
    w.force_status(&ticket, Status::Working).await;
    let session = codex_session(&w, &ticket, SessionKind::Helper).await;
    sqlx::query(
        "INSERT INTO helpers (id, ticket_id, piece, title, branch, worktree, session_id, status, started_at,
                              updated_at)
         VALUES ('helper-1', ?, 1, 'A piece', 'otaq/SHOP-1.1', '/nonexistent', ?, 'working', 1, 1)",
    )
    .bind(&ticket.id)
    .bind(&session)
    .execute(w.board.store().pool())
    .await
    .unwrap();
    let helper = Caller::Helper {
        ticket: ticket.key.clone(),
        ticket_id: ticket.id.clone(),
        session: session.clone(),
    };
    let prompt_of_helper = async || {
        w.helpers.of_ticket(&ticket.id).await.unwrap()[0]
            .prompt
            .clone()
    };

    draw(&w, &session, FIRST).await;
    both_asked(&w, &helper).await;
    until(&w, &ticket, Status::NeedsYou, Some(FIRST)).await;
    assert_eq!(
        prompt_of_helper().await,
        Some(format!("Run command: {FIRST}"))
    );

    key_in(&client, &session, b"y").await;
    until(&w, &ticket, Status::Working, None).await;
    assert_eq!(prompt_of_helper().await, None);
    stays(&w, &ticket, Status::Working).await;
    draw(&w, &session, SECOND).await;
    until(&w, &ticket, Status::NeedsYou, Some(SECOND)).await;
    assert_eq!(
        prompt_of_helper().await,
        Some(format!("Run command: {SECOND}"))
    );

    key_in(&client, &session, b"y").await;
    until(&w, &ticket, Status::Working, None).await;
    assert_eq!(prompt_of_helper().await, None);
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_tickets_agents_two_prompts_read_needs_you_one_at_a_time() {
    let (w, ticket, client) = world().await;
    w.force_status(&ticket, Status::Working).await;
    let session = codex_session(&w, &ticket, SessionKind::Agent).await;
    let agent = Caller::Agent {
        ticket: ticket.key.clone(),
        ticket_id: ticket.id.clone(),
        session: session.clone(),
    };

    draw(&w, &session, FIRST).await;
    both_asked(&w, &agent).await;
    until(&w, &ticket, Status::NeedsYou, Some(FIRST)).await;
    assert_eq!(
        w.signals.open_prompt(&session),
        Some(format!("Run command: {FIRST}"))
    );

    key_in(&client, &session, b"y").await;
    until(&w, &ticket, Status::Working, None).await;
    stays(&w, &ticket, Status::Working).await;
    assert_eq!(w.signals.open_prompt(&session), None);
    draw(&w, &session, SECOND).await;
    until(&w, &ticket, Status::NeedsYou, Some(SECOND)).await;

    key_in(&client, &session, b"y").await;
    until(&w, &ticket, Status::Working, None).await;
    assert_eq!(w.signals.open_prompt(&session), None);
    w.stop().await;
}

/// A Project agent (Noyan) keeps its prompts the same way: the second reads Needs you once drawn.
#[tokio::test(flavor = "multi_thread")]
async fn a_project_agents_second_prompt_reads_needs_you_once_drawn() {
    let (w, _, client) = world().await;
    let project = w.board.project("SHOP").await.unwrap();
    sqlx::query(
        "INSERT INTO project_agents (id, project_id, tool, state, created_at) VALUES ('pa-1', ?, 'codex', 'running', 1)",
    )
    .bind(&project.id)
    .execute(w.board.store().pool())
    .await
    .unwrap();
    let session = w
        .sessions
        .prepared(None)
        .for_project_agent("pa-1")
        .spawn(SpawnSpec {
            kind: SessionKind::ProjectAgent,
            start: Start::User,
            project_id: project.id.clone(),
            project_key: project.key.clone(),
            ticket: None,
            program: "sh".into(),
            args: Vec::new(),
            cwd: PathBuf::from(&project.repo_path),
            env: vec![("PS1".into(), String::new())],
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
        .id;
    let caller = Caller::ProjectAgent {
        project: project.key.to_string(),
        agent: "pa-1".into(),
        session: session.clone(),
    };
    let waits = async || w.project_agents.get("pa-1").await.unwrap().needs_you;
    let until_waits = async |want: Option<&str>| {
        let reached = tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let now = waits().await;
                if now.as_deref().map(|p| p.contains(want.unwrap_or("\u{0}"))) == want.map(|_| true)
                {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await;
        assert!(reached.is_ok(), "never {want:?}: {:?}", waits().await);
    };

    draw(&w, &session, FIRST).await;
    both_asked(&w, &caller).await;
    until_waits(Some(FIRST)).await;
    key_in(&client, &session, b"y").await;
    until_waits(None).await;
    tokio::time::sleep(LOOK_EVERY * 4).await;
    assert_eq!(waits().await, None, "the second isn't drawn yet");
    draw(&w, &session, SECOND).await;
    until_waits(Some(SECOND)).await;
    key_in(&client, &session, b"y").await;
    until_waits(None).await;
    w.stop().await;
}
