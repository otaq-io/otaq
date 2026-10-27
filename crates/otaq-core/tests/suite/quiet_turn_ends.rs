//! Turn ends no tool reports (T-0359; ADP-12, ADP-08, ADP-06, SES-07; d0086, d0088, d0048): a Codex, Copilot or
//! OpenCode turn end leaves the card Working in background while a command it started still runs, naming it, and
//! Idle once it ends; a new turn first cancels that; the tool's own helpers don't count; Claude Code reports its
//! own background work and is unchanged. A user's key at Copilot's own prompt is an answer, Otaq's deliveries never
//! are. A multi-line draft a tool can't clear is the user's to clear. The agent is an ordinary interactive `sh` in
//! a PTY with the session's tool set: no test starts a real tool.

use std::path::PathBuf;
use std::time::Duration;

use otaq_core::agent_tools::{Tool, ToolEvent};
use otaq_core::board::tickets::Ticket;
use otaq_core::caller::Caller;
use otaq_core::client::{Client, ClientConfig};
use otaq_core::domain::Status;
use otaq_core::protocol::{InputParams, Request, encode_bytes};
use otaq_core::service::testing::TestService;
use otaq_core::session::deliveries::Delivery;
use otaq_core::session::{SessionKind, SessionTicket, SpawnSpec, Start, TerminalSize};
use otaq_core::signals::background::BackgroundWork;
use otaq_core::signals::quiet::QuietTiming;

/// Short waits: a recheck every 100 ms, 300 ms after a key.
const TIMING: QuietTiming = QuietTiming {
    recheck: Duration::from_millis(100),
    answer_wait: Duration::from_millis(300),
};

async fn world() -> (TestService, Ticket) {
    let w = TestService::start().await;
    w.signals.set_quiet_timing(TIMING);
    // These are the tools' turn ends; the stage reminder before Idle (d0042 2026-10-28) has its own tests.
    w.signals.set_stage_reminders(false);
    w.add_project("shop-web").await;
    let ticket = w.create("SHOP", "Add coupon codes").await;
    w.force_status(&ticket, Status::Working).await;
    (w, ticket)
}

/// An interactive `sh` as `ticket`'s agent on `tool` (or `program` with `args`), and its caller.
async fn agent_on(
    w: &TestService,
    ticket: &Ticket,
    tool: Tool,
    program: &str,
    args: &[&str],
) -> Caller {
    let project = w.board.project("SHOP").await.unwrap();
    let info = w
        .sessions
        .spawn(SpawnSpec {
            kind: SessionKind::Agent,
            start: Start::User,
            project_id: project.id.clone(),
            project_key: project.key.clone(),
            ticket: Some(SessionTicket {
                id: ticket.id.clone(),
                key: ticket.key.clone(),
            }),
            program: program.into(),
            args: args.iter().map(|a| (*a).to_owned()).collect(),
            cwd: PathBuf::from(&project.repo_path),
            env: vec![("PS1".into(), String::new())],
            size: TerminalSize {
                cols: 200,
                rows: 50,
            },
            tool: Some(tool),
            tool_session_id: Some("conversation-1".into()),
            model: None,
            agent: None,
            effort: None,
            fallback: None,
            theme: None,
        })
        .await
        .unwrap();
    Caller::Agent {
        ticket: ticket.key.clone(),
        ticket_id: ticket.id.clone(),
        session: info.id,
    }
}

async fn sh_on(w: &TestService, ticket: &Ticket, tool: Tool) -> Caller {
    agent_on(w, ticket, tool, "sh", &[]).await
}

fn session_of(caller: &Caller) -> &str {
    match caller {
        Caller::Agent { session, .. } => session,
        other => panic!("{other:?}"),
    }
}

/// Types `command` into the agent's shell and waits until the shell has run it (it prints `marker`; the typed line's
/// echo spells it apart).
async fn run(w: &TestService, caller: &Caller, command: &str, marker: &str) {
    let session = session_of(caller);
    let (head, tail) = marker.split_at(1);
    w.sessions
        .write(
            session,
            format!("{command}\recho '{head}''{tail}'\r").as_bytes(),
        )
        .unwrap();
    w.wait_for_output(session, marker).await;
}

async fn event(w: &TestService, caller: &Caller, tool: Tool, event: ToolEvent) {
    w.signals
        .apply_events(caller, tool, &[event])
        .await
        .unwrap();
}

fn new_turn() -> ToolEvent {
    ToolEvent::NewTurn { prompt: None }
}

fn quiet_end() -> ToolEvent {
    ToolEvent::TurnEnded {
        background: BackgroundWork::default(),
        last_message: Some("Started the test run.".into()),
    }
}

/// The card's status and its line.
async fn card(w: &TestService) -> (Status, Option<String>) {
    let ticket = w.board.ticket("SHOP-1").await.unwrap();
    (ticket.status, ticket.signal.and_then(|s| s.text))
}

async fn until(w: &TestService, what: &str, want: (Status, Option<&str>)) {
    let reached = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let (status, text) = card(w).await;
            if status == want.0 && (want.1.is_none() || text.as_deref() == want.1) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await;
    assert!(reached.is_ok(), "never {what}: {:?}", card(w).await);
}

/// SHOP-18 on Codex: `sleep … &` and the turn ends → Working in background naming it; it ends → Idle within a
/// recheck. The tool's helper (here `tail`, running before the turn) and a later one of the same program don't count.
#[tokio::test(flavor = "multi_thread")]
async fn a_quiet_turn_end_shows_what_still_runs_until_it_ends() {
    let (w, ticket) = world().await;
    let agent = sh_on(&w, &ticket, Tool::Codex).await;
    run(&w, &agent, "tail -f /dev/null &", "HELPER").await;
    event(&w, &agent, Tool::Codex, new_turn()).await;
    run(&w, &agent, "tail -f /dev/null & sleep 1 &", "STARTED").await;
    event(&w, &agent, Tool::Codex, quiet_end()).await;
    assert_eq!(
        card(&w).await,
        (
            Status::WorkingInBackground,
            Some("background: sleep 1".to_owned())
        )
    );
    // Gone after its second: the turn's normal end, with the agent's last line.
    until(&w, "Idle", (Status::Idle, Some("Started the test run."))).await;
    // The helpers still run and never counted.
    tokio::time::sleep(TIMING.recheck * 3).await;
    assert_eq!(card(&w).await.0, Status::Idle);
    w.stop().await;
}

/// A command whose shell ended (`sh -c 'sleep … &'`) still runs in the agent's session: it counts.
#[tokio::test(flavor = "multi_thread")]
async fn a_command_whose_shell_ended_still_counts() {
    let (w, ticket) = world().await;
    let agent = sh_on(&w, &ticket, Tool::OpenCode).await;
    event(&w, &agent, Tool::OpenCode, new_turn()).await;
    run(&w, &agent, "sh -c 'sleep 1 &'", "STARTED").await;
    event(&w, &agent, Tool::OpenCode, quiet_end()).await;
    assert_eq!(
        card(&w).await,
        (
            Status::WorkingInBackground,
            Some("background: sleep 1".to_owned())
        )
    );
    until(&w, "Idle", (Status::Idle, None)).await;
    w.stop().await;
}

/// A new turn before the command ends: the recheck is dropped, the card stays Working.
#[tokio::test(flavor = "multi_thread")]
async fn a_new_turn_cancels_the_recheck() {
    let (w, ticket) = world().await;
    let agent = sh_on(&w, &ticket, Tool::Codex).await;
    event(&w, &agent, Tool::Codex, new_turn()).await;
    run(&w, &agent, "sleep 1 &", "STARTED").await;
    event(&w, &agent, Tool::Codex, quiet_end()).await;
    assert_eq!(card(&w).await.0, Status::WorkingInBackground);
    event(&w, &agent, Tool::Codex, new_turn()).await;
    assert_eq!(card(&w).await.0, Status::Working);
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert_eq!(card(&w).await, (Status::Working, None), "no late Idle");
    w.stop().await;
}

/// Claude Code says what its turn left running: an empty report is Idle at once, whatever runs.
#[tokio::test(flavor = "multi_thread")]
async fn claude_code_turn_ends_are_unchanged() {
    let (w, ticket) = world().await;
    let agent = sh_on(&w, &ticket, Tool::Claude).await;
    event(&w, &agent, Tool::Claude, new_turn()).await;
    run(&w, &agent, "sleep 2 &", "STARTED").await;
    event(&w, &agent, Tool::Claude, quiet_end()).await;
    assert_eq!(
        card(&w).await,
        (Status::Idle, Some("Started the test run.".to_owned()))
    );
    w.stop().await;
}

async fn needs_you(w: &TestService, caller: &Caller, tool: Tool) {
    event(w, caller, tool, new_turn()).await;
    let prompt = ToolEvent::NeedsYou {
        prompt: "Run command: curl https://example.com".into(),
        helper_agent: false,
    };
    event(w, caller, tool, prompt).await;
    assert_eq!(card(w).await.0, Status::NeedsYou);
}

/// The user's key, as the app's terminal sends it.
async fn key(client: &Client, caller: &Caller, bytes: &[u8]) {
    client
        .request(Request::SessionInput(InputParams {
            session: session_of(caller).to_owned(),
            data: encode_bytes(bytes),
            reply: false,
            if_empty: false,
        }))
        .await
        .unwrap();
}

/// Copilot sends nothing after Esc at its prompt: the key makes the card Working, and with no signal and nothing
/// running the turn ends (Idle) after the wait. A hook during the wait keeps it Working.
#[tokio::test(flavor = "multi_thread")]
async fn a_key_at_copilots_prompt_answers_it() {
    let (w, ticket) = world().await;
    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    let agent = sh_on(&w, &ticket, Tool::Copilot).await;
    needs_you(&w, &agent, Tool::Copilot).await;
    key(&client, &agent, b"\x1b").await;
    assert_eq!(card(&w).await.0, Status::Working);
    until(&w, "Idle after the wait", (Status::Idle, None)).await;

    // A signal within the wait: the tool goes on, the card stays Working.
    needs_you(&w, &agent, Tool::Copilot).await;
    key(&client, &agent, b"\x1b[B").await;
    assert_eq!(card(&w).await.0, Status::Working);
    w.signals
        .hook(&agent, Tool::Copilot, "postToolUse", &serde_json::json!({}))
        .await
        .unwrap();
    tokio::time::sleep(TIMING.answer_wait * 3).await;
    assert_eq!(card(&w).await.0, Status::Working);
    w.stop().await;
}

/// Something running under Copilot after the key (the command it was allowed to run): the card stays Working until
/// it ends.
#[tokio::test(flavor = "multi_thread")]
async fn after_a_key_a_running_command_keeps_the_turn() {
    let (w, ticket) = world().await;
    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    let agent = sh_on(&w, &ticket, Tool::Copilot).await;
    needs_you(&w, &agent, Tool::Copilot).await;
    run(&w, &agent, "sleep 1 &", "STARTED").await;
    key(&client, &agent, b"y").await;
    tokio::time::sleep(TIMING.answer_wait * 2).await;
    assert_eq!(card(&w).await.0, Status::Working);
    until(&w, "Idle once it ended", (Status::Idle, None)).await;
    w.stop().await;
}

/// Otaq's own typing never answers; on Claude Code a key at Needs you changes nothing (only Enter does).
#[tokio::test(flavor = "multi_thread")]
async fn deliveries_and_other_tools_dont_count() {
    let (w, ticket) = world().await;
    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    let agent = sh_on(&w, &ticket, Tool::Copilot).await;
    needs_you(&w, &agent, Tool::Copilot).await;
    let sent = w
        .deliveries
        .deliver_from_click(session_of(&agent), "echo typed by Otaq")
        .await
        .unwrap();
    assert_eq!(sent, Delivery::Sent { count: 1 });
    tokio::time::sleep(TIMING.answer_wait * 2).await;
    assert_eq!(card(&w).await.0, Status::NeedsYou);
    w.sessions.stop(session_of(&agent)).await.unwrap();

    let agent = sh_on(&w, &ticket, Tool::Claude).await;
    needs_you(&w, &agent, Tool::Claude).await;
    key(&client, &agent, b"\x1b").await;
    tokio::time::sleep(TIMING.answer_wait * 2).await;
    assert_eq!(card(&w).await.0, Status::NeedsYou);
    w.stop().await;
}

/// SES-07 on Codex: a multi-line draft offers only Keep it and cancel ("Clear it and send" refuses too, never
/// Ctrl+C); a one-line draft clears with Ctrl+E Ctrl+U, then the text goes.
#[tokio::test(flavor = "multi_thread")]
async fn a_multi_line_draft_is_the_users_to_clear() {
    let (w, ticket) = world().await;
    let raw = ["-c", "stty -icanon -echo; echo 'REA''DY'; exec cat -v"];
    let agent = agent_on(&w, &ticket, Tool::Codex, "sh", &raw).await;
    let session = session_of(&agent).to_owned();
    w.wait_for_output(&session, "READY").await;
    w.sessions.input(&session, b"first line\nsecond").unwrap();
    let unclearable = Delivery::NeedsConfirmation {
        ticket: Some(ticket.key.clone()),
        unclearable_by: Some("Codex".into()),
    };
    assert_eq!(
        w.deliveries
            .deliver_from_click(&session, "address the review")
            .await
            .unwrap(),
        unclearable
    );
    assert_eq!(
        w.deliveries
            .clear_and_send(&session, Some("address the review"))
            .await
            .unwrap(),
        unclearable
    );
    let output = w.wait_for_output(&session, "second").await;
    assert!(!output.contains("address the review"), "{output}");
    assert!(!output.contains("^C"), "never Ctrl+C: {output}");
    w.sessions.stop(&session).await.unwrap();

    let agent = agent_on(&w, &ticket, Tool::Codex, "sh", &raw).await;
    let session = session_of(&agent).to_owned();
    w.wait_for_output(&session, "READY").await;
    w.sessions.input(&session, b"one line").unwrap();
    assert_eq!(
        w.deliveries
            .deliver_from_click(&session, "address the review")
            .await
            .unwrap(),
        Delivery::NeedsConfirmation {
            ticket: Some(ticket.key.clone()),
            unclearable_by: None,
        }
    );
    assert_eq!(
        w.deliveries
            .clear_and_send(&session, Some("address the review"))
            .await
            .unwrap(),
        Delivery::Sent { count: 1 }
    );
    let output = w.wait_for_output(&session, "address the review").await;
    assert!(
        output.contains("one line^E^Uaddress the review"),
        "{output}"
    );

    // Claude Code clears a multi-line draft itself.
    w.sessions.stop(&session).await.unwrap();
    let agent = agent_on(&w, &ticket, Tool::Claude, "sh", &raw).await;
    let session = session_of(&agent).to_owned();
    w.wait_for_output(&session, "READY").await;
    w.sessions.input(&session, b"first line\nsecond").unwrap();
    assert_eq!(
        w.deliveries
            .clear_and_send(&session, Some("address the review"))
            .await
            .unwrap(),
        Delivery::Sent { count: 1 }
    );
    w.stop().await;
}
