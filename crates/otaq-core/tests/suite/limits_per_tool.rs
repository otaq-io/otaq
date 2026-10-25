//! Limits per tool (T-0358, T-0405; USG-10, USG-06, USG-07, AGM-03, ATT-09; d0086, d0108, d0020, d0046, d0018): one
//! banner line per tool, Resume per tool (only that tool's agents continue, only its holds lift), "This is the usage
//! limit" per tool, a tool that isn't available (its line and one notification, no needs-you item per ticket), Use
//! fallback now (the tool's fallback, only on the click), and the limit signal's own reset.
//!
//! The tools are stand-ins (scripts printing a version: no tool runs), as in `launch_gate.rs`; `claude` is `sh`
//! printing its arguments, a Codex launch runs a script that exits at once. Agents stopped mid-turn are `sh`
//! sessions marked with their tool. SHOP runs the default flow on Developer (the default model: Claude Code); BILL
//! runs it on "Codex developer" (Codex).

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::agent_tools::models::{ModelEntry, ModelKind, ModelList};
use otaq_core::agent_tools::{LaunchContext, Tool};
use otaq_core::agents::limits::{Situation, ToolLine, ToolLines};
use otaq_core::agents::{
    Agent, AgentFields, AgentHold, AgentModel, Agents, FallbackFrom, FixedModels, HoldReason,
    ModelRef, ToolModel,
};
use otaq_core::attention::notify::pace::{FakeClock, QUIET};
use otaq_core::attention::notify::{Notifier, OnClick, OsNotification};
use otaq_core::board::moves::DropTarget;
use otaq_core::board::tickets::{EventKind, Ticket};
use otaq_core::client::{Client, ClientConfig};
use otaq_core::data_dir::DataDir;
use otaq_core::domain::{Actor, Status};
use otaq_core::flow::FlowAgent;
use otaq_core::naming;
use otaq_core::protocol::{KeyParams, Reply, Request, ToolParams, UsageParams};
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::{TestService, path_without_build_folders};
use otaq_core::session::{SessionKind, SessionTicket, SpawnSpec, Start, TerminalSize};
use otaq_core::store::now_ms;
use otaq_core::usage::limit::{CONTINUE, LimitHit, LimitKind, ResetSource};
use tempfile::TempDir;

const A: Tool = Tool::Claude;
const B: Tool = Tool::Codex;

fn me() -> Actor {
    Actor::User("me".into())
}

/// Records what is posted.
#[derive(Default)]
struct Recorder(Mutex<Vec<OsNotification>>);

impl Notifier for Recorder {
    fn post(&self, n: &OsNotification, _on_click: OnClick) {
        self.0.lock().unwrap().push(n.clone());
    }
}

impl Recorder {
    fn unavailable(&self) -> Vec<(String, String)> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .filter(|n| n.title.contains("available"))
            .map(|n| (n.title.clone(), n.body.clone()))
            .collect()
    }
}

struct World {
    w: TestService,
    rules: Agents,
    /// BILL's key: its Agent stage runs "Codex developer".
    bill: String,
    recorder: Arc<Recorder>,
    /// The notifications' quiet period counts in it (d0090).
    clock: Arc<FakeClock>,
    _tools: TempDir,
}

impl World {
    async fn agent(&self, name: &str) -> Agent {
        self.rules
            .list()
            .await
            .unwrap()
            .agents
            .into_iter()
            .find(|a| a.name == name)
            .unwrap_or_else(|| panic!("no agent {name}"))
    }

    /// Saves `model` on the agent `name` (Settings → Agents).
    async fn set_model(&self, name: &str, model: AgentModel) {
        let agent = self.agent(name).await;
        self.rules
            .edit(
                &agent.id,
                &AgentFields {
                    name: agent.name.clone(),
                    instructions: agent.instructions.clone(),
                    skills: agent.skills.clone(),
                    model,
                    effort: None,
                },
            )
            .await
            .unwrap();
    }

    /// The `n`th BILL ticket's key.
    fn bill(&self, n: u32) -> String {
        format!("{}-{n}", self.bill)
    }

    async fn hide(&self, tool: Tool, hidden: bool) {
        self.w
            .connected
            .set_hidden(self.w.board.store(), tool, hidden)
            .await
            .unwrap();
    }

    async fn client(&self) -> Client {
        Client::connect(ClientConfig::new(&self.w.data))
            .await
            .unwrap()
    }

    async fn lines(&self) -> ToolLines {
        let reply = self
            .client()
            .await
            .request(Request::UsageSummary(UsageParams::default()))
            .await
            .unwrap();
        let Reply::Usage(summary) = reply else {
            panic!("{reply:?}")
        };
        summary.tool_lines
    }

    async fn line(&self, tool: Tool) -> ToolLine {
        self.lines()
            .await
            .lines
            .into_iter()
            .find(|l| l.tool == tool)
            .unwrap_or_else(|| panic!("no line for {tool}"))
    }

    async fn request(&self, request: Request) -> Result<Reply, otaq_core::protocol::ErrorBody> {
        match self.client().await.request(request).await {
            Ok(reply) => Ok(reply),
            Err(otaq_core::client::ClientError::Service(body)) => Err(body),
            Err(e) => panic!("{e}"),
        }
    }
}

const ARGS: &str = "printf 'ARGS %s\\n' \"$*\"; exec cat";

/// Claude Code and Codex connected (stand-ins printing a version): the default model is Claude Code's own. Claude
/// Code's fallback is Codex · GPT Sol (always the newest), Codex's is Claude Code · sonnet. Project SHOP (`shop-web`,
/// Developer) and BILL (`billing-api`, "Codex developer" on Codex's default). OS notifications are recorded.
async fn world() -> World {
    let tools = TempDir::new().unwrap();
    let bin = tools.path().join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::create_dir_all(tools.path().join("claude")).unwrap();
    otaq_core::process::testing::stub_command(&bin, "claude", r#"echo "2.1.4 (Claude Code)""#);
    otaq_core::process::testing::stub_command(&bin, "codex", r#"echo "codex-cli 0.157.1""#);
    let root = tools.path().to_path_buf();
    let agent_path = std::env::join_paths([bin.clone(), "/usr/bin".into(), "/bin".into()]).unwrap();
    let recorder = Arc::new(Recorder::default());
    let notifier: Arc<dyn Notifier> = recorder.clone();
    let clock = Arc::new(FakeClock::new());
    let time = clock.clone();
    let w = TestService::start_with(move |mut config: ServiceConfig| {
        config.clock = time;
        config.tools.path = bin.into_os_string();
        config.tools.claude_config_dir = Some(root.join("claude"));
        config.tools.managed_settings_dir = root.join("managed");
        config.check_tool_states = true;
        config.scheduler_tick = Duration::from_millis(50);
        config.notifier = notifier;
        config.launch = Some(LaunchContext {
            bin_dir: "/nonexistent".into(),
            plugin_dir: "/nonexistent/plugin".into(),
            // Only the stand-ins: a Codex launch runs the script, never a real `codex`.
            path: agent_path,
            claude: ClaudeLaunch {
                program: vec!["sh".into(), "-c".into(), ARGS.into(), "sh".into()],
                config_dir: None,
            },
        });
        config
    })
    .await;
    until("both tools connected", || async {
        [A, B]
            .iter()
            .all(|&t| w.connected.status(t).state.is_connected())
    })
    .await;
    until("the default model", || async {
        w.board.store().default_model().await.unwrap().is_some()
    })
    .await;
    w.add_project("shop-web").await;
    let bill = w.add_project("billing-api").await.key;
    let mut file = otaq_core::flow::default_file().clone();
    for stage in &mut file.flow.stages {
        if stage.agent.as_deref() == Some("Developer") {
            stage.agent = Some("Codex developer".into());
        }
    }
    file.agents = vec![FlowAgent {
        name: "Codex developer".into(),
        role: None,
        instructions: String::new(),
        skills: vec!["work-ticket".into(), "commit".into()],
        model: "codex".into(),
        why: None,
    }];
    w.board.use_flow_for_tests(&bill, &file).await;
    let rules = Agents::new(
        w.board.clone(),
        Arc::new(FixedModels::new(vec![
            ToolModel::ToolDefault,
            ToolModel::Follow("sonnet".into()),
            ToolModel::Follow("newest:gpt-sol".into()),
        ])),
    );
    let model = |tool, model: &str| ModelRef {
        tool,
        model: Some(model.into()),
    };
    rules
        .set_fallback(A, Some(&model(B, "newest:gpt-sol")))
        .await
        .unwrap();
    rules
        .set_fallback(B, Some(&model(A, "sonnet")))
        .await
        .unwrap();
    // Codex's list, with GPT Sol's newest model.
    let entry = |id: &str, kind, resolves_to: Option<&str>| ModelEntry {
        id: id.into(),
        display_name: id.into(),
        description: None,
        resolves_to: resolves_to.map(str::to_owned),
        kind,
        family: Some("gpt-sol".into()),
        released_at: None,
    };
    w.models
        .record(
            B,
            Ok(ModelList {
                entries: vec![
                    entry("newest:gpt-sol", ModelKind::Follow, Some("gpt-6-sol")),
                    entry("gpt-6-sol", ModelKind::Exact, None),
                ],
            }),
            now_ms(),
        )
        .await;
    World {
        w,
        rules,
        bill,
        recorder,
        clock,
        _tools: tools,
    }
}

async fn until<F, Fut>(what: &str, done: F)
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let reached = tokio::time::timeout(Duration::from_secs(20), async {
        while !done().await {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await;
    assert!(reached.is_ok(), "never: {what}");
}

async fn ticket(w: &TestService, key: &str) -> Ticket {
    w.board.ticket(key).await.unwrap()
}

async fn queue(w: &TestService, key: &str) {
    w.board
        .move_ticket(key, DropTarget::ReadyForDev, true, &me())
        .await
        .unwrap();
}

async fn until_status(w: &TestService, key: &str, want: Status) {
    until(&format!("{key} {want}"), || async {
        w.status(key).await == want
    })
    .await;
}

async fn until_started(w: &TestService, key: &str) {
    until(&format!("{key} started"), || async {
        !matches!(
            w.status(key).await,
            Status::Held | Status::Queued | Status::WaitingForSlot
        )
    })
    .await;
}

async fn holds(w: &TestService, key: &str) -> Vec<AgentHold> {
    let t = ticket(w, key).await;
    w.board.store().ticket_holds(&t.id).await.unwrap()
}

/// An `sh` running as `ticket`'s agent on `tool`: an agent the limit can stop mid-turn.
async fn shell_on(w: &TestService, ticket: &Ticket, tool: Tool) -> String {
    let project = w.board.project(ticket.key.project.as_str()).await.unwrap();
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
            args: vec![],
            cwd: PathBuf::from(&project.repo_path),
            env: vec![
                (naming::ticket_env_var(), ticket.key.to_string()),
                (DataDir::env_var(), w.data.root().display().to_string()),
                ("PS1".into(), String::new()),
                ("PATH".into(), path_without_build_folders()),
            ],
            size: TerminalSize {
                cols: 400,
                rows: 50,
            },
            tool: Some(tool),
            tool_session_id: None,
            model: None,
            agent: None,
            effort: None,
            fallback: None,
            theme: None,
        })
        .await
        .unwrap()
        .id
}

fn refusal(e: &otaq_core::protocol::ErrorBody) -> Option<&str> {
    e.data.as_ref()?.get("refusal")?.as_str()
}

fn hit(kind: LimitKind, text: &str) -> LimitHit {
    LimitHit {
        kind,
        text: text.into(),
    }
}

async fn wait_for_output(w: &TestService, session: &str, needle: &str) {
    let found = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let out = String::from_utf8_lossy(&w.sessions.attach(session).unwrap().scrollback)
                .into_owned();
            if out.contains(needle) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await;
    assert!(found.is_ok(), "{needle:?} never typed");
}

#[tokio::test(flavor = "multi_thread")]
async fn two_tools_at_their_limits_get_two_lines_and_resume_is_per_tool() {
    let world = world().await;
    let w = &world.w;
    let bill = world.bill(1);
    w.create("SHOP", "Checkout flow").await;
    w.create(&world.bill, "Add coupon codes").await;
    let order_email = w.create("SHOP", "Order email").await;
    let coupon_field = w.create("SHOP", "Coupon field").await;

    // SHOP-2 (Claude Code) and SHOP-3 (Codex) are stopped mid-turn by their tools' limits. Claude Code's has no
    // reset time (an API-key user: no windows, none in the text); Codex's signal carried its reset.
    let claude_agent = shell_on(w, &order_email, A).await;
    let codex_agent = shell_on(w, &coupon_field, B).await;
    for t in [&order_email, &coupon_field] {
        w.force_status(t, Status::WaitingForLimitReset).await;
    }
    // Claude Code's turn ended on its limit (its StopFailure hook): idle between turns.
    let caller = otaq_core::caller::Caller::Agent {
        ticket: order_email.key.clone(),
        ticket_id: order_email.id.clone(),
        session: claude_agent.clone(),
    };
    let failure = serde_json::json!({"hook_event_name": "StopFailure", "error": "rate_limit",
        "last_assistant_message": "You've hit your session limit"});
    let signal = w
        .signals
        .hook(&caller, A, "StopFailure", &failure)
        .await
        .unwrap();
    assert!(matches!(
        signal[..],
        [otaq_core::signals::Signal::LimitReached { .. }]
    ));
    let codex_reset = now_ms() + 2 * 60 * 60 * 1000;
    w.plan_limit
        .hit(
            A,
            &hit(LimitKind::Session, "Claude usage limit reached"),
            None,
            &order_email.id,
            now_ms(),
        )
        .await
        .unwrap();
    w.plan_limit
        .hit(
            B,
            &hit(LimitKind::Weekly, "You've hit your usage limit."),
            Some(codex_reset),
            &coupon_field.id,
            now_ms(),
        )
        .await
        .unwrap();
    let limits = otaq_core::usage::plan::limits(w.board.store())
        .await
        .unwrap();
    let codex_limit = limits.iter().find(|l| l.tool == B).unwrap();
    assert_eq!(
        (codex_limit.resets_at, codex_limit.reset_source),
        (Some(codex_reset), Some(ResetSource::ToolSignal)),
        "the signal's reset comes first"
    );

    // Both waiting tickets are held, each on its own tool.
    queue(w, "SHOP-1").await;
    queue(w, &bill).await;
    until_status(w, "SHOP-1", Status::Held).await;
    until_status(w, &bill, Status::Held).await;

    // Two lines.
    let lines = world.lines().await;
    assert!(lines.several_tools);
    let tools: Vec<Tool> = lines.lines.iter().map(|l| l.tool).collect();
    assert_eq!(tools, [A, B]);
    let claude = &lines.lines[0];
    assert_eq!(
        claude.situation,
        Situation::Limit {
            kind: LimitKind::Session,
            window_minutes: None,
            resets_at: None,
            reset_seen: false,
            hit_at: limits.iter().find(|l| l.tool == A).unwrap().hit_at,
        }
    );
    assert_eq!((claude.stopped, claude.waiting), (1, 1));
    assert!(claude.resume_enabled, "an unknown reset: Resume always");
    let codex = &lines.lines[1];
    assert!(
        matches!(codex.situation, Situation::Limit { kind: LimitKind::Weekly, resets_at: Some(at), .. } if at == codex_reset)
    );
    assert_eq!((codex.stopped, codex.waiting), (1, 1));
    assert!(!codex.resume_enabled, "a known reset that hasn't come");
    // Each tool's fallback is at its limit too: Use fallback now can't start them.
    assert_eq!(
        claude.fallback.as_ref().map(|f| f.tool),
        Some(B),
        "Claude Code's fallback"
    );
    assert!(!claude.fallback_can_start && !codex.fallback_can_start);

    // Only the tool's own limit-stopped agents are out of Needs you (ATT-09): both are, for now.
    assert!(w.needs_you.list().await.unwrap().items.is_empty());

    // Resume Claude Code: only its agent continues, only its holds lift.
    let Reply::Resumed(keys) = world
        .request(Request::LimitsResumeTool(ToolParams { tool: A }))
        .await
        .unwrap()
    else {
        panic!("resumed")
    };
    let keys: Vec<String> = keys.iter().map(ToString::to_string).collect();
    assert_eq!(keys, ["SHOP-2"]);
    wait_for_output(w, &claude_agent, CONTINUE).await;
    assert!(w.deliveries.pending(&codex_agent).await.unwrap().is_empty());
    until_status(w, "SHOP-1", Status::Working).await;
    assert!(holds(w, "SHOP-1").await.is_empty());
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(w.status(&bill).await, Status::Held);
    assert_eq!(holds(w, &bill).await[0].reason, HoldReason::ToolLimit);
    let tools: Vec<Tool> = world.lines().await.lines.iter().map(|l| l.tool).collect();
    assert_eq!(tools, [B]);
    // Codex's line now offers its fallback, Claude Code · sonnet, which can start.
    let codex = world.line(B).await;
    assert_eq!(
        codex.fallback,
        Some(ModelRef {
            tool: A,
            model: Some("sonnet".into())
        })
    );
    assert!(codex.fallback_can_start);

    // Needs you: Claude Code's agent raises its item again; Codex's still doesn't.
    w.force_status(&order_email, Status::Idle).await;
    w.force_status(&coupon_field, Status::Idle).await;
    let items: Vec<String> = w
        .needs_you
        .list()
        .await
        .unwrap()
        .items
        .iter()
        .map(|i| i.key.to_string())
        .collect();
    assert_eq!(items, ["SHOP-2"]);
    // There is no Resume all.
    let resume_all = serde_json::from_value::<Request>(
        serde_json::json!({"method": "usage.resume_all", "params": {}}),
    );
    assert!(resume_all.is_err());
    world.w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn this_is_the_usage_limit_folds_only_the_clicked_agents_tool() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let shop1 = w.create("SHOP", "Price table").await;
    let shop2 = w.create("SHOP", "Coupon field").await;
    let shop3 = w.create("SHOP", "Order email").await;
    let default = w.adapters.for_new_session().tool();
    let other = *Tool::ALL.iter().find(|&&t| t != default).unwrap();
    shell_on(&w, &shop1, default).await;
    shell_on(&w, &shop2, other).await;
    for t in [&shop1, &shop2, &shop3] {
        w.force_status(t, Status::Idle).await;
    }
    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    let mark = |key: &str| {
        Request::UsageMarkLimit(KeyParams {
            key: key.to_owned(),
        })
    };
    // SHOP-1's tool: SHOP-1 and SHOP-3 (no session: the new-session tool) fold; SHOP-2 keeps its item.
    let Reply::LimitMarked(keys) = client.request(mark("SHOP-1")).await.unwrap() else {
        panic!("marked")
    };
    let keys: Vec<String> = keys.iter().map(ToString::to_string).collect();
    assert_eq!(keys, ["SHOP-1", "SHOP-3"]);
    assert_eq!(w.status("SHOP-2").await, Status::Idle);
    let items: Vec<String> = w
        .needs_you
        .list()
        .await
        .unwrap()
        .items
        .iter()
        .map(|i| i.key.to_string())
        .collect();
    assert_eq!(items, ["SHOP-2"]);
    let tools: Vec<Tool> = otaq_core::usage::plan::limits(w.board.store())
        .await
        .unwrap()
        .iter()
        .map(|l| l.tool)
        .collect();
    assert_eq!(tools, [default]);

    // SHOP-2's click folds its own tool's.
    let Reply::LimitMarked(keys) = client.request(mark("SHOP-2")).await.unwrap() else {
        panic!("marked")
    };
    assert_eq!(keys, std::slice::from_ref(&shop2.key));
    assert!(w.needs_you.list().await.unwrap().items.is_empty());
    assert!(
        otaq_core::usage::plan::limit_of(w.board.store(), other)
            .await
            .unwrap()
            .is_some()
    );
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_tool_that_isnt_available_gets_its_line_and_one_notification() {
    let world = world().await;
    let w = &world.w;
    let [first, second, third] = [1, 2, 3].map(|n| world.bill(n));
    for title in ["Add coupon codes", "Coupon field", "Order totals rounding"] {
        w.create(&world.bill, title).await;
    }
    world.hide(B, true).await;
    queue(w, &first).await;
    until_status(w, &first, Status::Held).await;
    until("the notification", || async {
        !world.recorder.unavailable().is_empty()
    })
    .await;
    assert_eq!(
        world.recorder.unavailable(),
        [(
            "Codex isn\u{2019}t available".to_owned(),
            "1 ticket waiting".to_owned()
        )]
    );

    let line = world.line(B).await;
    let Situation::Unavailable {
        reason,
        sign_in,
        hidden,
        ..
    } = &line.situation
    else {
        panic!("{line:?}")
    };
    assert_eq!(
        (reason.as_str(), *sign_in, *hidden),
        ("hidden in Settings", false, true)
    );
    assert_eq!(
        (line.waiting, line.stopped, line.resume_enabled),
        (1, 0, false)
    );
    // Codex hidden leaves one tool connected: no fallback offered (it reads as M22's banner).
    assert_eq!(line.fallback, None);
    assert!(!world.lines().await.several_tools);

    // A second held ticket: the line counts it, no second notification, no needs-you item for either.
    queue(w, &second).await;
    until_status(w, &second, Status::Held).await;
    until("two waiting", || async { world.line(B).await.waiting == 2 }).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(world.recorder.unavailable().len(), 1);
    assert!(w.needs_you.list().await.unwrap().items.is_empty());

    // Shown again: the holds lift, the line goes.
    world.hide(B, false).await;
    until_started(w, &first).await;
    until_started(w, &second).await;
    until("the line gone", || async {
        world.lines().await.lines.iter().all(|l| l.tool != B)
    })
    .await;

    // Hidden again with a new waiting ticket, after the quiet period: a new situation, a new notification.
    world.clock.advance(QUIET);
    world.hide(B, true).await;
    queue(w, &third).await;
    until_status(w, &third, Status::Held).await;
    until("the second notification", || async {
        world.recorder.unavailable().len() == 2
    })
    .await;
    world.w.stop().await;
}

/// Use fallback now (AGM-03): nothing moves on its own; the click starts every ticket waiting on Claude Code on its
/// fallback (Codex · GPT Sol, resolved to its newest listed model), saved on each session, and the line counts them;
/// with the fallback blocked it is refused and nothing moves.
#[tokio::test(flavor = "multi_thread")]
async fn use_fallback_now_starts_the_tools_waiting_tickets_on_its_fallback() {
    let world = world().await;
    let w = &world.w;
    w.create("SHOP", "Checkout flow").await;
    w.create("SHOP", "Update footer links").await;
    w.create("SHOP", "Order email").await;
    w.plan_limit
        .record_hit(
            A,
            &hit(LimitKind::Session, "Claude usage limit reached"),
            now_ms(),
        )
        .await
        .unwrap();
    for key in ["SHOP-1", "SHOP-2"] {
        queue(w, key).await;
    }
    until_status(w, "SHOP-1", Status::Held).await;
    until_status(w, "SHOP-2", Status::Held).await;
    // The line offers the fallback, which can start; nothing moves without the click.
    let line = world.line(A).await;
    assert_eq!(
        (
            line.fallback.as_ref().map(|f| f.tool),
            line.fallback_can_start
        ),
        (Some(B), true)
    );
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(w.status("SHOP-1").await, Status::Held);
    assert_eq!(line.on_fallback, 0);

    let Reply::StartedOnFallback(keys) = world
        .request(Request::LimitsUseFallbackNow(ToolParams { tool: A }))
        .await
        .unwrap()
    else {
        panic!("started")
    };
    let keys: Vec<String> = keys.iter().map(ToString::to_string).collect();
    assert_eq!(keys, ["SHOP-1", "SHOP-2"]);
    for key in ["SHOP-1", "SHOP-2"] {
        until_started(w, key).await;
        let t = ticket(w, key).await;
        // Started on Codex, with GPT Sol resolved to its newest listed model, and the fallback on the session.
        let (tool, model, from, reason): (
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
        ) = sqlx::query_as(
            "SELECT tool, model, fallback_from, fallback_reason FROM sessions
             WHERE ticket_id = ? AND kind = 'agent' ORDER BY started_at DESC",
        )
        .bind(&t.id)
        .fetch_one(w.board.store().pool())
        .await
        .unwrap();
        assert_eq!(tool.as_deref(), Some(B.as_str()), "{key}");
        assert_eq!(model.as_deref(), Some("gpt-6-sol"), "{key}");
        assert_eq!(
            (from.as_deref(), reason.as_deref()),
            (Some(A.as_str()), Some(HoldReason::ToolLimit.as_str())),
            "{key}"
        );
        let history = w.board.history(key).await.unwrap();
        assert!(
            history.iter().any(|e| e.kind == EventKind::FallbackUsed
                && e.payload["from_tool"] == A.as_str()
                && e.payload["tool"] == B.as_str()),
            "{key}: {history:?}"
        );
    }
    until("the line counts them", || async {
        world.line(A).await.on_fallback == 2
    })
    .await;
    assert_eq!(world.line(A).await.waiting, 0);

    // With Codex hidden the fallback can't start: refused, and a newly held ticket stays waiting.
    world.hide(B, true).await;
    queue(w, "SHOP-3").await;
    until_status(w, "SHOP-3", Status::Held).await;
    let blocked = world
        .request(Request::LimitsUseFallbackNow(ToolParams { tool: A }))
        .await
        .unwrap_err();
    assert_eq!(refusal(&blocked), Some("move_refused"), "{blocked:?}");
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(w.status("SHOP-3").await, Status::Held);
    world.w.stop().await;
}

/// A tool that isn't available keeps its line for the tickets started on its fallback: "Codex isn't available ·
/// hidden in Settings · 2 tickets on the fallback", with one notification; nothing moves back once it is back.
#[tokio::test(flavor = "multi_thread")]
async fn a_tool_that_isnt_available_keeps_its_line_for_the_tickets_on_its_fallback() {
    let world = world().await;
    let w = &world.w;
    let (first, second) = (world.bill(1), world.bill(2));
    w.create(&world.bill, "Add coupon codes").await;
    w.create(&world.bill, "Coupon field").await;
    world.hide(B, true).await;
    queue(w, &first).await;
    queue(w, &second).await;
    until_status(w, &first, Status::Held).await;
    until_status(w, &second, Status::Held).await;
    w.gate.use_fallback_now(B).await.unwrap();
    until_status(w, &first, Status::Working).await;
    until_status(w, &second, Status::Working).await;
    assert!(holds(w, &first).await.is_empty() && holds(w, &second).await.is_empty());
    let t = ticket(w, &first).await;
    let session = w
        .sessions
        .running_of(&t.id, SessionKind::Agent)
        .expect("started");
    assert_eq!(
        w.sessions.stored(&session.id).await.unwrap().fallback,
        Some(FallbackFrom {
            tool: B,
            reason: HoldReason::ToolUnavailable
        })
    );

    // The line stays and says so.
    until("both counted", || async {
        world.line(B).await.on_fallback == 2
    })
    .await;
    let line = world.line(B).await;
    assert!(
        matches!(&line.situation, Situation::Unavailable { reason, .. } if reason == "hidden in Settings"),
        "{line:?}"
    );
    assert_eq!(line.waiting, 0);
    // One notification, when the first ticket was held.
    tokio::time::sleep(Duration::from_millis(300)).await;
    let posted = world.recorder.unavailable();
    assert_eq!(posted.len(), 1, "{posted:?}");
    assert_eq!(posted[0].0, "Codex isn\u{2019}t available");

    // Codex is back: the line goes (nothing moves back).
    world.hide(B, false).await;
    until("the line gone", || async {
        world.lines().await.lines.iter().all(|l| l.tool != B)
    })
    .await;
    assert_eq!(
        w.sessions
            .running_of(&t.id, SessionKind::Agent)
            .and_then(|s| s.tool),
        Some(A)
    );
    world.w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn another_tools_limit_doesnt_hold_a_project_agent() {
    let world = world().await;
    let w = &world.w;
    w.plan_limit
        .record_hit(
            A,
            &hit(LimitKind::Session, "Claude usage limit reached"),
            now_ms(),
        )
        .await
        .unwrap();
    // The Project agent role's agent on Codex: it opens.
    world
        .set_model(
            "Noyan",
            AgentModel::Tool {
                tool: B,
                model: None,
            },
        )
        .await;
    let codex = w
        .project_agents
        .open("SHOP", "Why is checkout slow?")
        .await
        .unwrap();
    assert_eq!(codex.session.as_ref().and_then(|s| s.tool), Some(B));
    // On Claude Code: held with its tool's message.
    world
        .set_model(
            "Noyan",
            AgentModel::Tool {
                tool: A,
                model: Some("sonnet".into()),
            },
        )
        .await;
    let held = w
        .project_agents
        .open("SHOP", "What changed in billing?")
        .await
        .unwrap_err();
    assert_eq!(held.to_string(), "Claude Code limit reached");

    // Resuming the ended Codex agent isn't held by Claude Code's limit either.
    sqlx::query("UPDATE project_agents SET tool_session_id = ? WHERE id = ?")
        .bind("0199d2a4-3b1c-7c3e-9f00-123456789abc")
        .bind(&codex.id)
        .execute(w.board.store().pool())
        .await
        .unwrap();
    until("the Codex agent ended", || async {
        w.project_agents.get(&codex.id).await.unwrap().state
            != otaq_core::project_agents::AgentState::Running
    })
    .await;
    let resumed = w.project_agents.resume(&codex.id).await.unwrap();
    assert_eq!(resumed.session.as_ref().and_then(|s| s.tool), Some(B));
    world.w.stop().await;
}
