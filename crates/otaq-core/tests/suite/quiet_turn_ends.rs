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
use otaq_core::protocol::{Event, InputParams, Request, encode_bytes};
use otaq_core::service::testing::TestService;
use otaq_core::session::deliveries::Delivery;
use otaq_core::session::{SessionKind, SessionTicket, SpawnSpec, Start, TerminalSize};
use otaq_core::signals::background::BackgroundWork;
use otaq_core::signals::hooks::Signal;
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
/// echo spells it apart) and what it started is settled ([`exec_settled`]).
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
    exec_settled(w, caller).await;
}

/// Windows: Git Bash's `&` forks the shell (a new `sh` process) and that `exec`s the command a moment later, which
/// starts it as a new process and ends the forked one another moment later; until then the command isn't running
/// yet, or looks like a child of that process. Waits until every process in the agent's tree is at its top and
/// none is a forked `sh` (no test leaves a shell running), as on macOS and Linux right away.
async fn exec_settled(w: &TestService, caller: &Caller) {
    if !cfg!(windows) {
        return;
    }
    let session = session_of(caller);
    let forked_sh = |p: &otaq_core::process::TreeProcess| {
        p.program
            .as_ref()
            .and_then(|p| p.file_name()?.to_str())
            .is_some_and(|n| n.eq_ignore_ascii_case("sh.exe") || n.eq_ignore_ascii_case("bash.exe"))
    };
    let settled = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let tree = w.sessions.process_tree(session);
            if tree
                .iter()
                .all(|p| tree.iter().all(|q| q.pid != p.parent) && !forked_sh(p))
            {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await;
    assert!(settled.is_ok(), "{:?}", w.sessions.process_tree(session));
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
/// recheck. The test ends each command itself (`kill`), so a slow machine can't end it before a look. The tool's helper (here `tail`, running before the turn) and a later one of the same program don't count.
#[tokio::test(flavor = "multi_thread")]
async fn a_quiet_turn_end_shows_what_still_runs_until_it_ends() {
    let (w, ticket) = world().await;
    let agent = sh_on(&w, &ticket, Tool::Codex).await;
    run(&w, &agent, "tail -f /dev/null &", "HELPER").await;
    event(&w, &agent, Tool::Codex, new_turn()).await;
    run(&w, &agent, "tail -f /dev/null & sleep 30 &", "STARTED").await;
    event(&w, &agent, Tool::Codex, quiet_end()).await;
    assert_eq!(
        card(&w).await,
        (
            Status::WorkingInBackground,
            Some("background: sleep 30".to_owned())
        )
    );
    // Once it ends: the turn's normal end, with the agent's last line.
    run(&w, &agent, "kill $!", "KILLED").await;
    until(&w, "Idle", (Status::Idle, Some("Started the test run."))).await;
    // The helpers still run and never counted.
    tokio::time::sleep(TIMING.recheck * 3).await;
    assert_eq!(card(&w).await.0, Status::Idle);
    w.stop().await;
}

/// SHOP-13 on Codex: Codex starts its code-mode host during the turn (after the turn's start, so it isn't one of the
/// helpers already running) and keeps it while it sits idle at its prompt. The turn end is Idle, not Working in
/// background, so the user is asked to act; a command the turn left running still counts beside it.
#[tokio::test(flavor = "multi_thread")]
async fn a_helper_codex_starts_mid_turn_is_not_background_work() {
    let (w, ticket) = world().await;
    let agent = sh_on(&w, &ticket, Tool::Codex).await;
    event(&w, &agent, Tool::Codex, new_turn()).await;
    // A program named as Codex's host (a copy of `sleep`, signed again on macOS, which kills an unsigned copy of a
    // system program), started in the turn and left running. Where `sleep` is a multi-call binary (uutils, Ubuntu
    // 26.04's coreutils), a copy under another name refuses to run at all, even run as `sleep` with `exec -a`
    // ("coreutils: unknown program"): there the host is a copy of `sh` waiting in its own `read` on a FIFO, so it
    // starts no command of its own.
    run(
        &w,
        &agent,
        "cp \"$(command -v sleep)\" ./codex-code-mode-host; \
         command -v codesign >/dev/null && codesign -f -s - ./codex-code-mode-host 2>/dev/null; \
         if ./codex-code-mode-host 0 2>/dev/null; then ./codex-code-mode-host 300 & \
         else cp \"$(command -v sh)\" ./codex-code-mode-host; mkfifo ./codex-code-mode-host.fifo; \
         ./codex-code-mode-host -c 'read x < ./codex-code-mode-host.fifo' & fi; echo $! > host.pid",
        "STARTED",
    )
    .await;
    // The background job runs the copy a moment after its shell forks.
    let host = |p: &otaq_core::process::TreeProcess| {
        p.program
            .as_ref()
            .and_then(|p| p.file_stem()?.to_str())
            .is_some_and(|n| n.eq_ignore_ascii_case("codex-code-mode-host"))
    };
    let running = tokio::time::timeout(Duration::from_secs(10), async {
        while !w.sessions.process_tree(session_of(&agent)).iter().any(host) {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await;
    assert!(
        running.is_ok(),
        "the host never ran: {:?}",
        w.sessions.process_tree(session_of(&agent))
    );
    event(&w, &agent, Tool::Codex, quiet_end()).await;
    assert_eq!(
        card(&w).await,
        (Status::Idle, Some("Started the test run.".to_owned()))
    );
    // The next turn leaves a command of its own running beside the host: only that one is named.
    event(&w, &agent, Tool::Codex, new_turn()).await;
    run(&w, &agent, "sleep 30 &", "SLEEPING").await;
    event(&w, &agent, Tool::Codex, quiet_end()).await;
    assert_eq!(
        card(&w).await,
        (
            Status::WorkingInBackground,
            Some("background: sleep 30".to_owned())
        )
    );
    run(&w, &agent, "kill $!", "KILLED").await;
    until(&w, "Idle", (Status::Idle, Some("Started the test run."))).await;
    // The `sh` host never ends by itself, and Stop reaches only the session's own process group.
    run(&w, &agent, "kill $(cat host.pid)", "HOST GONE").await;
    w.stop().await;
}

/// A command whose shell ended (`sh -c 'sleep … &'`) still runs in the agent's session: it counts.
#[tokio::test(flavor = "multi_thread")]
async fn a_command_whose_shell_ended_still_counts() {
    let (w, ticket) = world().await;
    let agent = sh_on(&w, &ticket, Tool::OpenCode).await;
    event(&w, &agent, Tool::OpenCode, new_turn()).await;
    run(
        &w,
        &agent,
        "sh -c 'sleep 30 & echo $! > sleep.pid'",
        "STARTED",
    )
    .await;
    event(&w, &agent, Tool::OpenCode, quiet_end()).await;
    assert_eq!(
        card(&w).await,
        (
            Status::WorkingInBackground,
            Some("background: sleep 30".to_owned())
        )
    );
    run(&w, &agent, "kill $(cat sleep.pid)", "KILLED").await;
    until(&w, "Idle", (Status::Idle, None)).await;
    w.stop().await;
}

/// A new turn before the command ends: the recheck is dropped, the card stays Working.
#[tokio::test(flavor = "multi_thread")]
async fn a_new_turn_cancels_the_recheck() {
    let (w, ticket) = world().await;
    let agent = sh_on(&w, &ticket, Tool::Codex).await;
    event(&w, &agent, Tool::Codex, new_turn()).await;
    run(&w, &agent, "sleep 30 &", "STARTED").await;
    event(&w, &agent, Tool::Codex, quiet_end()).await;
    assert_eq!(card(&w).await.0, Status::WorkingInBackground);
    event(&w, &agent, Tool::Codex, new_turn()).await;
    assert_eq!(card(&w).await.0, Status::Working);
    // A recheck that wasn't dropped would end the turn once the command ends.
    run(&w, &agent, "kill $!", "KILLED").await;
    let ended = tokio::time::timeout(Duration::from_secs(10), async {
        while !w.sessions.process_tree(session_of(&agent)).is_empty() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await;
    assert!(
        ended.is_ok(),
        "{:?}",
        w.sessions.process_tree(session_of(&agent))
    );
    tokio::time::sleep(TIMING.recheck * 5).await;
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

/// Codex takes "y" alone at its approval prompt and sends nothing until the command's turn goes on to its end or its
/// next prompt: the key itself makes the card Working at once, and it stays Working while the turn runs.
#[tokio::test(flavor = "multi_thread")]
async fn y_at_codexs_prompt_makes_it_working_at_once() {
    let (w, ticket) = world().await;
    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    let agent = sh_on(&w, &ticket, Tool::Codex).await;
    needs_you(&w, &agent, Tool::Codex).await;
    key(&client, &agent, b"y").await;
    assert_eq!(card(&w).await, (Status::Working, None));
    tokio::time::sleep(TIMING.answer_wait * 2).await;
    assert_eq!(card(&w).await.0, Status::Working);
    w.stop().await;
}

/// An interactive `sh` as Project agent `id` of SHOP on `tool`, and its caller.
async fn project_agent_on(w: &TestService, id: &str, tool: Tool) -> Caller {
    let project = w.board.project("SHOP").await.unwrap();
    sqlx::query(
        "INSERT INTO project_agents (id, project_id, tool, state, created_at) VALUES (?, ?, ?, 'running', 1)",
    )
    .bind(id)
    .bind(&project.id)
    .bind(tool.as_str())
    .execute(w.board.store().pool())
    .await
    .unwrap();
    let info = w
        .sessions
        .prepared(None)
        .for_project_agent(id)
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
            size: TerminalSize::default(),
            tool: Some(tool),
            tool_session_id: Some("conversation-1".into()),
            model: None,
            agent: None,
            effort: None,
            theme: None,
        })
        .await
        .unwrap();
    Caller::ProjectAgent {
        project: project.key.to_string(),
        agent: id.to_owned(),
        session: info.id,
    }
}

/// Codex's "y" at a Project agent's prompt (Noyan drafting a flow) answers it: it no longer waits for the user, so
/// Needs you gives way to the drafting line while the approved command runs, and its next prompt names the new
/// command. On Claude Code a key alone changes nothing (only Enter does).
#[tokio::test(flavor = "multi_thread")]
async fn y_at_a_codex_project_agents_prompt_answers_it() {
    let (w, _) = world().await;
    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    let waits = async |id: &str| w.project_agents.get(id).await.unwrap().needs_you;
    // The tool's prompt, as the service applies a Project agent's hook to it.
    let prompt = async |id: &str, command: &str| {
        let prompt = format!("Run command: {command}");
        w.project_agents
            .hook(id, &Signal::NeedsYou { prompt })
            .await
            .unwrap();
    };
    let input = async |caller: &Caller, bytes: &[u8]| {
        let Caller::ProjectAgent { session, .. } = caller else {
            panic!("{caller:?}")
        };
        client
            .request(Request::SessionInput(InputParams {
                session: session.clone(),
                data: encode_bytes(bytes),
                reply: false,
                if_empty: false,
            }))
            .await
            .unwrap();
    };

    let codex = project_agent_on(&w, "pa-codex", Tool::Codex).await;
    prompt("pa-codex", "Get-ChildItem .agents").await;
    assert_eq!(
        waits("pa-codex").await.as_deref(),
        Some("Run command: Get-ChildItem .agents")
    );
    input(&codex, b"y").await;
    assert_eq!(waits("pa-codex").await, None, "answered by the key");
    prompt("pa-codex", "Get-Content SKILL.md").await;
    assert_eq!(
        waits("pa-codex").await.as_deref(),
        Some("Run command: Get-Content SKILL.md")
    );

    let claude = project_agent_on(&w, "pa-claude", Tool::Claude).await;
    prompt("pa-claude", "npm test").await;
    input(&claude, b"1").await;
    assert!(
        waits("pa-claude").await.is_some(),
        "a key alone on Claude Code"
    );
    input(&claude, b"\r").await;
    assert_eq!(waits("pa-claude").await, None, "Enter answers it");
    w.stop().await;
}

/// An interactive `sh` as `ticket`'s session of `kind` on `tool`: its id.
async fn sh_of_kind(w: &TestService, ticket: &Ticket, kind: SessionKind, tool: Tool) -> String {
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
            size: TerminalSize::default(),
            tool: Some(tool),
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

/// The user's key in `session`'s terminal, as the app's terminal sends it.
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

fn permission_prompt() -> Signal {
    Signal::NeedsYou {
        prompt: "Run command: npm test".into(),
    }
}

/// Codex's "y" at a checker's prompt answers it: the ticket is Validating… again while the approved command runs. On
/// Claude Code a key alone changes nothing and Enter answers it. A key at a checker with no prompt open changes
/// nothing.
#[tokio::test(flavor = "multi_thread")]
async fn y_at_a_codex_validators_prompt_answers_it() {
    let (w, _) = world().await;
    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    for (tool, title, answer) in [
        (Tool::Codex, "Coupon field", &b"y"[..]),
        (Tool::Claude, "Gift cards", &b"\r"[..]),
    ] {
        let ticket = w.create("SHOP", title).await;
        let key = ticket.key.to_string();
        let validator = sh_of_kind(&w, &ticket, SessionKind::Validator, tool).await;
        w.validating_with(&ticket, &validator).await;
        key_in(&client, &validator, b"y").await;
        assert_eq!(
            w.status(&key).await,
            Status::Checking,
            "{tool:?}: no prompt"
        );
        w.validation
            .validator_signal(&ticket.id, &validator, &permission_prompt())
            .await
            .unwrap();
        assert_eq!(w.status(&key).await, Status::CheckerNeedsYou, "{tool:?}");
        if tool == Tool::Claude {
            key_in(&client, &validator, b"1").await;
            assert_eq!(
                w.status(&key).await,
                Status::CheckerNeedsYou,
                "a key alone on Claude Code"
            );
        }
        key_in(&client, &validator, answer).await;
        assert_eq!(w.status(&key).await, Status::Checking, "{tool:?}: answered");
    }
    w.stop().await;
}

/// Codex's "y" at a helper's prompt answers it: the helper no longer waits and the ticket is Working again. On Claude
/// Code a key alone changes nothing and Enter answers it. A key at a helper with no prompt open changes nothing.
#[tokio::test(flavor = "multi_thread")]
async fn y_at_a_codex_helpers_prompt_answers_it() {
    let (w, ticket) = world().await;
    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    let prompt_of = async |piece: u32| {
        w.helpers
            .of_ticket(&ticket.id)
            .await
            .unwrap()
            .into_iter()
            .find(|h| h.piece == piece)
            .unwrap()
            .prompt
    };
    for (piece, tool, answer) in [(1, Tool::Codex, &b"y"[..]), (2, Tool::Claude, &b"\r"[..])] {
        let session = sh_of_kind(&w, &ticket, SessionKind::Helper, tool).await;
        sqlx::query(
            "INSERT INTO helpers (id, ticket_id, piece, title, branch, worktree, session_id, status, started_at,
                                  updated_at)
             VALUES (?, ?, ?, 'A piece', 'otaq/SHOP-1-piece', '/nonexistent', ?, 'working', 1, 1)",
        )
        .bind(format!("helper-{piece}"))
        .bind(&ticket.id)
        .bind(piece)
        .bind(&session)
        .execute(w.board.store().pool())
        .await
        .unwrap();
        let caller = Caller::Helper {
            ticket: ticket.key.clone(),
            ticket_id: ticket.id.clone(),
            session: session.clone(),
        };
        key_in(&client, &session, b"y").await;
        assert_eq!(card(&w).await.0, Status::Working, "{tool:?}: no prompt");
        w.helpers
            .after_signals(&caller, &[permission_prompt()])
            .await
            .unwrap();
        assert_eq!(card(&w).await.0, Status::NeedsYou, "{tool:?}");
        if tool == Tool::Claude {
            key_in(&client, &session, b"1").await;
            assert!(
                prompt_of(piece).await.is_some(),
                "a key alone on Claude Code"
            );
            assert_eq!(card(&w).await.0, Status::NeedsYou);
        }
        key_in(&client, &session, answer).await;
        assert_eq!(prompt_of(piece).await, None, "{tool:?}: answered");
        assert_eq!(card(&w).await.0, Status::Working, "{tool:?}");
    }
    w.stop().await;
}

/// The events the app was sent since the last call: the ticket changes (by key) and the card statuses.
fn sent(events: &mut tokio::sync::broadcast::Receiver<Event>) -> (Vec<String>, Vec<Status>) {
    let (mut changed, mut statuses) = (Vec::new(), Vec::new());
    while let Ok(event) = events.try_recv() {
        match event {
            Event::TicketChanged { key, .. } => changed.push(key.to_string()),
            Event::CardStatusChanged { status, .. } => statuses.push(status),
            _ => {}
        }
    }
    (changed, statuses)
}

/// A checker's Codex prompt reads Needs you at once (re-test 3: the card and the sheet read "Checking" for about 30 s
/// while the prompt was drawn, until something else about the ticket changed): the app is told the moment its hook
/// comes, and again when "y" makes it Validating… while the approved command runs.
#[tokio::test(flavor = "multi_thread")]
async fn a_checkers_codex_prompt_tells_the_app_at_once() {
    let (w, _) = world().await;
    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    let ticket = w.create("SHOP", "Coupon field").await;
    let key = ticket.key.to_string();
    let validator = sh_of_kind(&w, &ticket, SessionKind::Validator, Tool::Codex).await;
    w.validating_with(&ticket, &validator).await;
    let mut events = w.events.subscribe();

    w.validation
        .validator_signal(&ticket.id, &validator, &permission_prompt())
        .await
        .unwrap();
    assert_eq!(w.status(&key).await, Status::CheckerNeedsYou);
    let (changed, statuses) = sent(&mut events);
    assert!(changed.contains(&key), "the prompt: {changed:?}");
    assert_eq!(statuses, [Status::CheckerNeedsYou]);

    key_in(&client, &validator, b"y").await;
    assert_eq!(w.status(&key).await, Status::Checking);
    let (changed, statuses) = sent(&mut events);
    assert!(changed.contains(&key), "the answer: {changed:?}");
    assert_eq!(statuses, [Status::Checking]);
    w.stop().await;
}

/// A helper's Codex prompt shows on its row at once while the ticket already shows another prompt (the agent in
/// charge's own, which it keeps), and its row reads working again at once after "y" (re-test 3: the row read
/// "working" for 4–14 s with the prompt drawn, and "needs you" for seconds after the answer, until something else
/// about the ticket changed).
#[tokio::test(flavor = "multi_thread")]
async fn a_helpers_codex_prompt_tells_the_app_at_once_behind_another_prompt() {
    let (w, ticket) = world().await;
    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    let in_charge = sh_on(&w, &ticket, Tool::Codex).await;
    let own = ToolEvent::NeedsYou {
        prompt: "Run command: git status".into(),
        helper_agent: false,
    };
    event(&w, &in_charge, Tool::Codex, own).await;
    assert_eq!(
        card(&w).await,
        (Status::NeedsYou, Some("Run command: git status".into()))
    );
    let session = sh_of_kind(&w, &ticket, SessionKind::Helper, Tool::Codex).await;
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
    let mut events = w.events.subscribe();

    w.helpers
        .after_signals(&helper, &[permission_prompt()])
        .await
        .unwrap();
    assert_eq!(
        prompt_of_helper().await.as_deref(),
        Some("Run command: npm test")
    );
    assert_eq!(
        card(&w).await,
        (Status::NeedsYou, Some("Run command: git status".into())),
        "the agent in charge's prompt still shows on the card"
    );
    assert_eq!(sent(&mut events).0, ["SHOP-1"], "the helper's prompt");

    key_in(&client, &session, b"y").await;
    assert_eq!(prompt_of_helper().await, None);
    assert_eq!(sent(&mut events).0, ["SHOP-1"], "the helper's answer");
    w.stop().await;
}

/// Codex's "y" at the prompt of a finished ticket's answer session answers it: the session no longer waits. On Claude
/// Code a key alone changes nothing and Enter answers it.
#[tokio::test(flavor = "multi_thread")]
async fn y_at_a_codex_answer_sessions_prompt_answers_it() {
    let (w, _) = world().await;
    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    for (tool, title, answer) in [
        (Tool::Codex, "Coupon field", &b"y"[..]),
        (Tool::Claude, "Gift cards", &b"\r"[..]),
    ] {
        let ticket = w.create("SHOP", title).await;
        w.force_status(&ticket, Status::Merged).await;
        let session = sh_of_kind(&w, &ticket, SessionKind::Answer, tool).await;
        let caller = Caller::Agent {
            ticket: ticket.key.clone(),
            ticket_id: ticket.id.clone(),
            session: session.clone(),
        };
        let prompt = ToolEvent::NeedsYou {
            prompt: "Run command: git log".into(),
            helper_agent: false,
        };
        event(&w, &caller, tool, prompt).await;
        assert!(w.signals.open_prompt(&session).is_some(), "{tool:?}");
        if tool == Tool::Claude {
            key_in(&client, &session, b"1").await;
            assert!(
                w.signals.open_prompt(&session).is_some(),
                "a key alone on Claude Code"
            );
        }
        key_in(&client, &session, answer).await;
        assert_eq!(w.signals.open_prompt(&session), None, "{tool:?}: answered");
    }
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
    run(&w, &agent, "sleep 30 &", "STARTED").await;
    key(&client, &agent, b"y").await;
    tokio::time::sleep(TIMING.answer_wait * 2).await;
    assert_eq!(card(&w).await.0, Status::Working);
    // Ctrl-U first: the key is still on the shell's line.
    run(&w, &agent, "\x15kill $!", "KILLED").await;
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
