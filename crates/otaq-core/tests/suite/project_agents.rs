//! Project agents (T-0199, T-0318; PAG-01, PAG-04, PAG-06, PAG-07, SCH-03, SCH-04, REC-01; d0068, d0087): opening
//! one with the user's first message starts `claude` with the role, the skill and the message in the project's
//! folder; naming (the agent's own once, the fallback after the first turn, the user's rename wins); close (End
//! agent), Close ended agents, Ended by restart · Resume and the recent list; never counted toward the agent limit;
//! none opens during the usage limit, and limit-stopped ones continue on their tool's Resume.
//! `claude` stands in as `sh` printing its arguments and environment, then `cat` echoing what is typed into it: no
//! test starts claude. (On Windows Git's `sh` prints the folder with `pwd -W`, as Windows writes it, and the console
//! echoes only what a program reads, hence `cat` rather than `sleep`.)

use otaq_core::agent_tools::LaunchContext;
use otaq_core::agent_tools::Tool;
use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::board::Refusal;
use otaq_core::caller::Caller;
use otaq_core::client::{Client, ClientConfig};
use otaq_core::naming;
use otaq_core::project_agents::{AgentState, EMPTY_MESSAGE, NEW_AGENT, NameSource, ProjectAgent};
use otaq_core::protocol::{Reply, Request};
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::TestService;
use otaq_core::session::SessionKind;
use otaq_core::store::now_ms;
use otaq_core::usage::limit::{LimitHit, LimitKind};
use serde_json::{Value, json};

/// A skill as Claude Code runs it: `/otaq-work-ticket` (AGM-07).
fn claude_skill(name: &str) -> String {
    format!("/{}", naming::copy_name(name))
}

fn fake_launch() -> LaunchContext {
    let script = format!(
        "printf 'ARGS %s\\n' \"$*\"; printf 'ROLE=%s TICKET=%s PORT=%s\\n' \"${role}\" \"${{{ticket}-none}}\" \
         \"${{PORT-none}}\"; printf 'CWD=%s\\n' \"$(pwd -W 2>/dev/null || pwd -P)\"; exec cat",
        role = naming::role_env_var(),
        ticket = naming::ticket_env_var(),
    );
    LaunchContext {
        bin_dir: "/nonexistent".into(),
        plugin_dir: "/nonexistent/plugin".into(),
        path: std::env::var_os("PATH").unwrap_or_default(),
        claude: ClaudeLaunch {
            program: vec!["sh".into(), "-c".into(), script, "sh".into()],
            config_dir: None,
        },
    }
}

/// A folder as the fake `claude` prints it: on Windows without the `\\?\` prefix and with forward slashes, as
/// `pwd -W` writes it.
fn folder_text(folder: &std::path::Path) -> String {
    let text = folder.display().to_string();
    if cfg!(windows) {
        text.trim_start_matches(r"\\?\").replace('\\', "/")
    } else {
        text
    }
}

fn with_fake_claude(config: ServiceConfig) -> ServiceConfig {
    ServiceConfig {
        launch: Some(fake_launch()),
        ..config
    }
}

async fn world() -> TestService {
    let w = TestService::start_with(with_fake_claude).await;
    w.add_project("shop-web").await;
    w
}

fn caller(agent: &ProjectAgent) -> Caller {
    Caller::ProjectAgent {
        project: agent.project.clone(),
        agent: agent.id.clone(),
        session: agent.session.as_ref().expect("a session").id.clone(),
    }
}

/// A hook of the agent, as the service handles it: the signals map it, the Project agents apply it.
async fn hook(w: &TestService, agent: &ProjectAgent, event: &str, payload: Value) {
    let signals = w
        .signals
        .hook(&caller(agent), Tool::Claude, event, &payload)
        .await
        .unwrap();
    for signal in &signals {
        w.project_agents.hook(&agent.id, signal).await.unwrap();
    }
}

async fn prompt(w: &TestService, agent: &ProjectAgent, text: &str) {
    hook(w, agent, "UserPromptSubmit", json!({ "prompt": text })).await;
}

async fn stop(w: &TestService, agent: &ProjectAgent) {
    hook(
        w,
        agent,
        "Stop",
        json!({ "last_assistant_message": "Done." }),
    )
    .await;
}

fn refusal(e: otaq_core::board::BoardError) -> (Option<Refusal>, String) {
    (e.refusal(), e.to_string())
}

/// Opens a Project agent with the user's first message.
async fn open(w: &TestService, message: &str) -> ProjectAgent {
    w.project_agents.open("SHOP", message).await.unwrap()
}

/// The first request stored for the agent `id`.
async fn first_request(w: &TestService, id: &str) -> Option<String> {
    sqlx::query_scalar("SELECT first_request FROM project_agents WHERE id = ?")
        .bind(id)
        .fetch_one(w.board.store().pool())
        .await
        .unwrap()
}

async fn wait_for_state(w: &TestService, id: &str, state: AgentState) -> ProjectAgent {
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let a = w.project_agents.get(id).await.unwrap();
            if a.state == state {
                return a;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("the agent became {state:?}"))
}

#[tokio::test(flavor = "multi_thread")]
async fn opening_starts_claude_with_the_role_the_skill_and_the_message_several_at_once() {
    let w = world().await;
    let repo = folder_text(
        &std::fs::canonicalize(w.board.project("SHOP").await.unwrap().repo_path).unwrap(),
    );
    let a = open(&w, "  List the tickets that wait on NOTE-5 ").await;
    let b = open(&w, "Why is checkout slow?").await;
    for (agent, message) in [
        (&a, "List the tickets that wait on NOTE-5"),
        (&b, "Why is checkout slow?"),
    ] {
        assert_eq!(agent.name, NEW_AGENT, "named only after its first turn");
        assert_eq!(agent.name_source, None);
        assert_eq!(agent.state, AgentState::Running);
        // The message is its first request from the start (d0087).
        assert!(agent.got_request);
        assert_eq!(first_request(&w, &agent.id).await.as_deref(), Some(message));
        let session = agent.session.as_ref().unwrap();
        assert_eq!(session.kind, SessionKind::ProjectAgent);
        assert_eq!(session.ticket, None);
        assert_eq!(
            session.tool_session_id.as_deref(),
            agent.tool_session_id.as_deref()
        );
        assert_eq!(
            (session.tool, session.model.as_deref()),
            (Some(otaq_core::agent_tools::Tool::Claude), None)
        );
        assert_eq!(
            (agent.tool, agent.model.as_deref()),
            (otaq_core::agent_tools::Tool::Claude, None)
        );
        let out = w.wait_for_output(&session.id, "CWD=").await;
        // The skill and the message are one first prompt: the last argument.
        let args = out
            .lines()
            .find(|l| l.starts_with("ARGS "))
            .unwrap_or_default()
            .trim_end();
        assert!(
            args.ends_with(&format!(
                "--session-id {} /{}-project-agent {message}",
                agent.tool_session_id.as_deref().unwrap(),
                naming::COMMAND_NAME
            )),
            "{out}"
        );
        assert!(
            out.contains(&format!("--allowedTools Bash({}:*)", naming::COMMAND_NAME)),
            "{out}"
        );
        assert!(!out.contains("permission"), "{out}");
        assert!(
            out.contains(&format!(
                "ROLE={} TICKET=none PORT=none",
                naming::ROLE_PROJECT_AGENT
            )),
            "{out}"
        );
        assert!(out.contains(&format!("CWD={repo}")), "{out}");
    }
    assert_ne!(a.tool_session_id, b.tool_session_id);
    let list = w.project_agents.list("SHOP").await.unwrap();
    assert_eq!(
        list.open.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(),
        [a.id.as_str(), b.id.as_str()]
    );
    assert!(
        list.open
            .iter()
            .all(|a| a.session.as_ref().unwrap().is_running())
    );
    assert!(list.recent.is_empty());
    w.stop().await;
}

/// d0087: no empty session. A message without words is refused and nothing is stored or started.
#[tokio::test(flavor = "multi_thread")]
async fn an_empty_message_opens_nothing() {
    let w = world().await;
    for message in ["", "   \n\t "] {
        let (code, text) = refusal(w.project_agents.open("SHOP", message).await.unwrap_err());
        assert_eq!(code, Some(Refusal::EmptyField));
        assert_eq!(text, EMPTY_MESSAGE);
    }
    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM project_agents")
        .fetch_one(w.board.store().pool())
        .await
        .unwrap();
    assert_eq!(rows, 0);
    let sessions: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sessions")
        .fetch_one(w.board.store().pool())
        .await
        .unwrap();
    assert_eq!(sessions, 0);
    // The same over the socket.
    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    let e = client
        .request(Request::ProjectAgentOpen(
            otaq_core::protocol::ProjectAgentOpenParams {
                project: "SHOP".into(),
                message: " ".into(),
                agent: None,
            },
        ))
        .await
        .unwrap_err();
    assert!(e.to_string().contains(EMPTY_MESSAGE), "{e}");
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_agent_names_itself_once_and_the_users_rename_wins() {
    let w = world().await;
    let a = open(&w, "why is checkout slow?").await;
    let named = w
        .project_agents
        .name_by_agent(&a.id, "Checkout slowness", false)
        .await
        .unwrap();
    assert_eq!(named.name, "Checkout slowness");
    assert_eq!(named.name_source, Some(NameSource::Agent));

    // Once: a second call needs --rename (the user asked for it).
    let (code, text) = refusal(
        w.project_agents
            .name_by_agent(&a.id, "Something else", false)
            .await
            .unwrap_err(),
    );
    assert_eq!(code, Some(Refusal::NotAllowed));
    assert!(
        text.starts_with("This Project agent is already named \"Checkout slowness\""),
        "{text}"
    );
    let renamed = w
        .project_agents
        .name_by_agent(&a.id, "Checkout queries", true)
        .await
        .unwrap();
    assert_eq!(renamed.name, "Checkout queries");

    // The user's rename always wins.
    let mine = w
        .project_agents
        .rename(&a.id, "Slow checkout")
        .await
        .unwrap();
    assert_eq!(mine.name_source, Some(NameSource::User));
    for rename in [false, true] {
        let (code, text) = refusal(
            w.project_agents
                .name_by_agent(&a.id, "Agent's choice", rename)
                .await
                .unwrap_err(),
        );
        assert_eq!(code, Some(Refusal::NotAllowed));
        assert_eq!(
            text,
            "The user named this Project agent \"Slow checkout\"; their name stays"
        );
    }
    // Nor does the first turn's fallback overwrite it.
    prompt(&w, &mine, "why is checkout slow?").await;
    stop(&w, &mine).await;
    assert_eq!(
        w.project_agents.get(&a.id).await.unwrap().name,
        "Slow checkout"
    );
    let (code, _) = refusal(w.project_agents.rename(&a.id, "  ").await.unwrap_err());
    assert_eq!(code, Some(Refusal::EmptyField));
    w.stop().await;
}

/// SHB-03 (d0113): the Ask bar's Continue types the user's message into a running conversation, and a turn's end is
/// kept for its "New reply"; nothing is typed without words or into an ended one.
#[tokio::test(flavor = "multi_thread")]
async fn continue_types_into_a_running_agent_and_its_turn_end_is_kept() {
    let w = world().await;
    let a = open(&w, "why is checkout slow?").await;
    assert_eq!(a.turn_ended_at, None);
    prompt(&w, &a, "why is checkout slow?").await;
    let before = now_ms();
    stop(&w, &a).await;
    let ended = w.project_agents.get(&a.id).await.unwrap();
    assert!(ended.turn_ended_at.is_some_and(|at| at >= before));

    let session = a.session.clone().unwrap();
    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    let sent = client
        .request(Request::ProjectAgentSend(
            otaq_core::protocol::ProjectAgentSendParams {
                id: a.id.clone(),
                message: "and the cart page?".into(),
            },
        ))
        .await
        .unwrap();
    assert!(
        matches!(
            sent,
            Reply::Delivery(otaq_core::session::deliveries::Delivery::Sent { count: 1 })
        ),
        "{sent:?}"
    );
    w.wait_for_output(&session.id, "and the cart page?").await;

    let (code, _) = refusal(w.project_agents.send(&a.id, "  ").await.unwrap_err());
    assert_eq!(code, Some(Refusal::EmptyField));
    w.sessions.stop(&session.id).await.unwrap();
    wait_for_state(&w, &a.id, AgentState::Ended).await;
    let (code, _) = refusal(
        w.project_agents
            .send(&a.id, "still there?")
            .await
            .unwrap_err(),
    );
    assert_eq!(code, Some(Refusal::SessionEnded));
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn without_a_name_the_first_words_of_the_message_name_it_after_its_first_turn() {
    let w = world().await;
    let message = "Why is checkout so slow? Make tickets for what you find";
    let a = open(&w, message).await;
    // Its first turn is the skill with the message (the launch's first prompt).
    prompt(
        &w,
        &a,
        &format!("{} {message}", claude_skill("project-agent")),
    )
    .await;
    assert_eq!(first_request(&w, &a.id).await.as_deref(), Some(message));
    let a = w.project_agents.get(&a.id).await.unwrap();
    assert!(a.got_request);
    assert!(a.mid_turn);
    assert_eq!(a.name, NEW_AGENT, "only at the end of the turn");
    stop(&w, &a).await;
    let a = w.project_agents.get(&a.id).await.unwrap();
    assert_eq!(a.name, "Why is checkout so");
    assert_eq!(a.name_source, Some(NameSource::Fallback));
    assert!(!a.mid_turn);

    // A later request changes nothing; the agent's own name may still replace the fallback.
    prompt(&w, &a, "Now look at the cart").await;
    stop(&w, &a).await;
    assert_eq!(
        w.project_agents.get(&a.id).await.unwrap().name,
        "Why is checkout so"
    );
    let named = w
        .project_agents
        .name_by_agent(&a.id, "Checkout slowness", false)
        .await
        .unwrap();
    assert_eq!(named.name, "Checkout slowness");
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn end_agent_ends_it_into_recently_ended_and_asks_first_mid_turn() {
    let w = world().await;
    // Opened with its message, it works on it from the start: closing asks first, then it ends and is listed in
    // Recently ended (never discarded).
    let first = open(&w, "Audit the footer").await;
    let (code, _) = refusal(w.project_agents.close(&first.id, false).await.unwrap_err());
    assert_eq!(code, Some(Refusal::NeedsConfirmation));
    w.project_agents.close(&first.id, true).await.unwrap();
    let first = w.project_agents.get(&first.id).await.unwrap();
    assert_eq!(first.state, AgentState::Ended);
    assert!(first.closed);
    assert!(
        !w.sessions
            .info(&first.session.as_ref().unwrap().id)
            .unwrap()
            .is_running()
    );
    let list = w.project_agents.list("SHOP").await.unwrap();
    assert!(list.open.is_empty());
    assert_eq!(
        list.recent
            .iter()
            .map(|a| a.id.as_str())
            .collect::<Vec<_>>(),
        [first.id.as_str()]
    );

    // Mid-turn: needs the user's "Stop and close".
    let busy = open(&w, "Find slow queries").await;
    prompt(&w, &busy, "Find slow queries").await;
    let (code, text) = refusal(w.project_agents.close(&busy.id, false).await.unwrap_err());
    assert_eq!(code, Some(Refusal::NeedsConfirmation));
    assert_eq!(
        text,
        "New agent is in the middle of a turn. Stop it and close it?"
    );
    w.project_agents.close(&busy.id, true).await.unwrap();
    let busy = w.project_agents.get(&busy.id).await.unwrap();
    assert_eq!(busy.state, AgentState::Ended);
    assert!(busy.closed);
    assert!(!busy.session.as_ref().unwrap().is_running());
    let list = w.project_agents.list("SHOP").await.unwrap();
    assert!(list.open.is_empty());
    assert_eq!(list.recent.len(), 2);

    // Between turns it closes without asking.
    let idle = open(&w, "Audit the footer").await;
    prompt(&w, &idle, "Audit the footer").await;
    stop(&w, &idle).await;
    w.project_agents.close(&idle.id, false).await.unwrap();
    assert_eq!(
        w.project_agents.get(&idle.id).await.unwrap().state,
        AgentState::Ended
    );
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_agent_that_exits_ends_and_resume_continues_its_conversation() {
    let w = world().await;
    let a = open(&w, "Footer audit").await;
    let first = a.session.clone().unwrap();
    w.sessions.stop(&first.id).await.unwrap();
    let ended = wait_for_state(&w, &a.id, AgentState::Ended).await;
    assert!(!ended.closed, "its tab stays until closed");
    assert!(ended.ended_at.is_some());

    let resumed = w.project_agents.resume(&a.id).await.unwrap();
    assert_eq!(resumed.state, AgentState::Running);
    let session = resumed.session.clone().unwrap();
    assert_ne!(session.id, first.id);
    assert_eq!(
        session.tool_session_id.as_deref(),
        a.tool_session_id.as_deref()
    );
    assert_eq!(
        (session.tool, session.model.as_deref()),
        (Some(otaq_core::agent_tools::Tool::Claude), None)
    );
    let out = w.wait_for_output(&session.id, "CWD=").await;
    assert!(
        out.contains(&format!(
            "--resume {}",
            a.tool_session_id.as_deref().unwrap()
        )),
        "{out}"
    );
    assert!(
        !out.contains(&claude_skill("project-agent")),
        "no skill on resume: {out}"
    );
    assert!(!out.contains("Carry on"), "it ended, not a restart: {out}");
    // Resuming a running agent changes nothing.
    let again = w.project_agents.resume(&a.id).await.unwrap();
    assert_eq!(again.session.unwrap().id, session.id);
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn after_a_restart_agents_are_ended_by_restart_and_resume() {
    let w = world().await;
    let used = open(&w, "Import the GitHub issues labelled ready").await;
    prompt(
        &w,
        &used,
        &format!(
            "{} Import the GitHub issues labelled ready",
            claude_skill("project-agent")
        ),
    )
    .await;
    // Opened with its message, no hook seen yet: still a request, so not dropped.
    let fresh = open(&w, "Audit the footer").await;
    let w = w.restart_with(with_fake_claude).await;

    let list = w.project_agents.list("SHOP").await.unwrap();
    assert_eq!(
        list.open.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(),
        [used.id.as_str(), fresh.id.as_str()]
    );
    for after in &list.open {
        assert_eq!(after.state, AgentState::EndedByRestart);
        assert!(!after.mid_turn);
    }
    assert_eq!(list.recent.len(), 2);

    let resumed = w.project_agents.resume(&used.id).await.unwrap();
    assert_eq!(resumed.state, AgentState::Running);
    let out = w
        .wait_for_output(&resumed.session.unwrap().id, "CWD=")
        .await;
    // Told to carry on as its first prompt (d0122).
    assert!(
        out.contains(&format!(
            "--resume {} {}",
            used.tool_session_id.as_deref().unwrap(),
            otaq_core::recovery::CARRY_ON_PROJECT_AGENT
        )),
        "{out}"
    );
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_recent_list_holds_the_ten_newest_ended_agents() {
    let w = world().await;
    let project = w.board.project("SHOP").await.unwrap();
    for n in 0..12 {
        sqlx::query(
            "INSERT INTO project_agents (id, project_id, name, name_source, tool, tool_session_id, state,
                                         first_request, created_at, ended_at, closed_at)
             VALUES (?, ?, ?, 'agent', 'claude', ?, 'ended', 'r', ?, ?, ?)",
        )
        .bind(format!("a{n:02}"))
        .bind(&project.id)
        .bind(format!("Agent {n}"))
        .bind(format!("c{n}"))
        .bind(n)
        .bind(1000 + n)
        .bind(1000 + n)
        .execute(w.board.store().pool())
        .await
        .unwrap();
    }
    // An ended one that never got a request isn't listed.
    sqlx::query(
        "INSERT INTO project_agents (id, project_id, tool, tool_session_id, state, created_at, ended_at)
         VALUES ('unused', ?, 'claude', 'cu', 'ended', 0, 5000)",
    )
    .bind(&project.id)
    .execute(w.board.store().pool())
    .await
    .unwrap();
    let list = w.project_agents.list("SHOP").await.unwrap();
    assert!(list.open.iter().all(|a| a.id == "unused"));
    let names: Vec<&str> = list.recent.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "Agent 11", "Agent 10", "Agent 9", "Agent 8", "Agent 7", "Agent 6", "Agent 5",
            "Agent 4", "Agent 3", "Agent 2"
        ]
    );
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn agents_never_count_toward_the_limit_and_none_opens_during_the_usage_limit() {
    let w = world().await;
    let a = open(&w, "Find slow queries").await;
    open(&w, "Audit the footer").await;
    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    match client.request(Request::SchedulerStatus).await.unwrap() {
        Reply::SchedulerStatus(s) => assert_eq!(s.holding, 0),
        other => panic!("{other:?}"),
    }
    match client.request(Request::ServiceStatus).await.unwrap() {
        Reply::ServiceStatus(s) => assert_eq!((s.agents_working, s.agents_waiting), (0, 0)),
        other => panic!("{other:?}"),
    }

    // The limit stops the agent: it waits for its tool's Resume, and no new one opens meanwhile.
    prompt(&w, &a, "Find slow queries").await;
    let hit = LimitHit {
        kind: LimitKind::Session,
        text: "You've hit your session limit · resets 3:45pm".into(),
    };
    hook(
        &w,
        &a,
        "StopFailure",
        json!({ "error": "rate_limit", "last_assistant_message": hit.text }),
    )
    .await;
    w.plan_limit
        .record_hit(Tool::Claude, &hit, now_ms())
        .await
        .unwrap();
    assert!(w.project_agents.get(&a.id).await.unwrap().limit_stopped);
    let agents_before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM project_agents")
        .fetch_one(w.board.store().pool())
        .await
        .unwrap();
    let (code, text) = refusal(
        w.project_agents
            .open("SHOP", "Why is checkout slow?")
            .await
            .unwrap_err(),
    );
    assert_eq!(code, Some(Refusal::MoveRefused));
    // Held by the check before every launch: its tool is at its limit (T-0357).
    assert_eq!(
        text,
        format!("{} limit reached", Tool::Claude.display_name())
    );
    let other = w.project_agents.open("SHOP", "Audit the footer").await;
    assert!(other.is_err());
    let agents_after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM project_agents")
        .fetch_one(w.board.store().pool())
        .await
        .unwrap();
    assert_eq!(agents_before, agents_after, "nothing stored");

    // Resume all: "continue" is typed into its session (its turn has ended), and new agents open again.
    w.plan_limit.resume_tool(Tool::Claude).await.unwrap();
    let session = a.session.as_ref().unwrap().id.clone();
    let delivered: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pending_deliveries WHERE session_id = ? AND kind = 'continue_after_limit'",
    )
    .bind(&session)
    .fetch_one(w.board.store().pool())
    .await
    .unwrap();
    let typed = w
        .sessions
        .attach(&session)
        .map(|s| String::from_utf8_lossy(&s.scrollback).contains("continue"))
        .unwrap_or(false);
    assert!(delivered == 1 || typed, "continue queued or typed");
    prompt(&w, &a, "continue").await;
    assert!(!w.project_agents.get(&a.id).await.unwrap().limit_stopped);
    open(&w, "Why is checkout slow?").await;
    w.stop().await;
}

/// T-0201 (PAG-07, USG-01): a Project agent's tokens count per agent and in today's total.
#[tokio::test(flavor = "multi_thread")]
async fn tokens_count_per_agent_and_in_todays_total() {
    let w = world().await;
    let a = open(&w, "Find slow queries").await;
    let b = open(&w, "Audit the footer").await;
    let day = w.usage.summary(None).await.unwrap().day;
    for (agent, n) in [(&a, 1000), (&b, 10)] {
        sqlx::query(
            "INSERT INTO usage (session_id, day, input, cached, output) VALUES (?, ?, ? + 100, 5000, 300)",
        )
        .bind(&agent.session.as_ref().unwrap().id)
        .bind(&day)
        .bind(n)
        .execute(w.board.store().pool())
        .await
        .unwrap();
    }
    assert_eq!(
        w.project_agents.get(&a.id).await.unwrap().tokens.shown(),
        1400
    );
    assert_eq!(
        w.project_agents.get(&b.id).await.unwrap().tokens.shown(),
        410
    );
    assert_eq!(w.usage.summary(None).await.unwrap().today.shown(), 1810);
    w.stop().await;
}

/// T-0205 (REC-01, REC-06; d0063, d0068): what the after-restart summary reads (`service.startup_report`): the
/// Project agents the restart ended (each resumable; since d0087 every one opened with a request, so none is
/// discarded), and which interrupted tickets were validating (their Resume starts a fresh validation:
/// `tests/suite/validation.rs`).
#[tokio::test(flavor = "multi_thread")]
async fn the_startup_report_names_ended_agents_and_validating_tickets() {
    let w = world().await;
    let used = open(&w, "Audit the footer").await;
    let other = open(&w, "Find slow queries").await;
    let validating = w.create("SHOP", "Cart totals").await;
    w.force_status(&validating, otaq_core::domain::Status::Checking)
        .await;
    let w = w.restart_with(with_fake_claude).await;

    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    let report = match client.request(Request::StartupReport).await.unwrap() {
        Reply::StartupReport(r) => *r,
        other => panic!("{other:?}"),
    };
    let mut ended = report.project_agents_ended.clone();
    ended.sort();
    let mut expected = vec![used.id.clone(), other.id.clone()];
    expected.sort();
    assert_eq!(ended, expected);
    assert_eq!(report.new_agents_discarded, 0);
    assert_eq!(report.was_validating, std::slice::from_ref(&validating.key));
    assert!(report.interrupted.contains(&validating.key));
    let after = w.project_agents.get(&used.id).await.unwrap();
    assert_eq!(after.state, AgentState::EndedByRestart);
    // Resume from the summary continues its conversation.
    let resumed = w.project_agents.resume(&used.id).await.unwrap();
    assert_eq!(resumed.state, AgentState::Running);
    w.stop().await;
}

/// T-0318 (SHL-04, d0087): **Close ended agents** closes every ended (Ended, Ended by restart), not yet closed Project
/// agent of the project; they stay in Recently ended. Running agents, epic leads and other projects are left alone.
#[tokio::test(flavor = "multi_thread")]
async fn close_ended_closes_only_the_projects_ended_agents() {
    let w = world().await;
    let running = open(&w, "Find slow queries").await;
    let exited = open(&w, "Audit the footer").await;
    w.sessions
        .stop(&exited.session.as_ref().unwrap().id)
        .await
        .unwrap();
    wait_for_state(&w, &exited.id, AgentState::Ended).await;
    let project = w.board.project("SHOP").await.unwrap();
    let epic = w.create("SHOP", "Wishlist").await;
    let other = w.add_project("notes-api").await;
    for (id, project_id, state, epic_id) in [
        ("by-restart", &project.id, "ended_by_restart", None),
        ("lead", &project.id, "ended", Some(&epic.id)),
        ("elsewhere", &other.id, "ended", None),
    ] {
        sqlx::query(
            "INSERT INTO project_agents (id, project_id, tool, tool_session_id, state, first_request, created_at,
                                         ended_at, epic_id)
             VALUES (?, ?, 'claude', ?, ?, 'r', 1, 2, ?)",
        )
        .bind(id)
        .bind(project_id)
        .bind(format!("c-{id}"))
        .bind(state)
        .bind(epic_id)
        .execute(w.board.store().pool())
        .await
        .unwrap();
    }

    let list = w.project_agents.close_ended("SHOP").await.unwrap();
    assert_eq!(
        list.open.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(),
        [running.id.as_str()]
    );
    let closed = |id: &str| {
        let w = &w;
        let id = id.to_owned();
        async move { w.project_agents.get(&id).await.unwrap().closed }
    };
    assert!(closed(&exited.id).await);
    assert!(closed("by-restart").await);
    assert!(!closed(&running.id).await, "a running agent stays");
    assert!(!closed("lead").await, "epic leads are left alone");
    assert!(!closed("elsewhere").await, "another project's stay");
    let running = w.project_agents.get(&running.id).await.unwrap();
    assert_eq!(running.state, AgentState::Running);
    assert!(running.session.unwrap().is_running());
    // Both stay in Recently ended, each resumable.
    let mut recent: Vec<&str> = list.recent.iter().map(|a| a.id.as_str()).collect();
    recent.sort_unstable();
    let mut expected = vec![exited.id.as_str(), "by-restart"];
    expected.sort_unstable();
    assert_eq!(recent, expected);
    let resumed = w.project_agents.resume("by-restart").await.unwrap();
    assert_eq!(resumed.state, AgentState::Running);
    assert!(!resumed.closed);

    // Over the socket, as the pane's menu sends it: nothing more to close.
    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    match client
        .request(Request::ProjectAgentCloseEnded(
            otaq_core::protocol::ProjectParams {
                project: "SHOP".into(),
            },
        ))
        .await
        .unwrap()
    {
        Reply::ProjectAgents(list) => assert_eq!(list.open.len(), 2, "running and resumed"),
        other => panic!("{other:?}"),
    }
    w.stop().await;
}

/// M23 (T-0363): a first message held before its start comes with the project's list, for the pane's notice.
#[tokio::test(flavor = "multi_thread")]
async fn the_list_carries_a_held_first_message() {
    use otaq_core::agents::{AgentHold, HoldReason, SessionRole};
    let w = world().await;
    let project = w.board.project("SHOP").await.unwrap();
    assert!(
        w.project_agents
            .list("SHOP")
            .await
            .unwrap()
            .holds
            .is_empty()
    );
    let hold = AgentHold {
        id: "h1".into(),
        ticket_id: None,
        project_id: Some(project.id.clone()),
        role: SessionRole::ProjectAgent,
        reason: HoldReason::ModelUnavailable,
        tool: Tool::ALL[1],
        model: Some("gpt-5.4".into()),
        agent_id: None,
        fallback_tool: None,
        fallback_held: false,
        fallback_asked: false,
        since: 1,
    };
    w.board.store().set_agent_hold(&hold).await.unwrap();
    assert_eq!(w.project_agents.list("SHOP").await.unwrap().holds, [hold]);
    w.stop().await;
}
