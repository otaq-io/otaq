//! The check before every launch (T-0357, T-0405; SCH-04 `[M23]`, AGM-05, USG-10; d0108, d0086, d0088, d0018): a
//! tool that isn't available or is at its limit holds only the agents that would use it, a model no longer listed
//! holds with "Model unavailable", **a held agent waits for its own tool** (nothing ever moves to another tool),
//! resumes are never checked, and a hold lifts once when its cause goes, never on a timer.
//!
//! The decision is tested as a table (each reason × another tool free or blocked); the service's side with stand-in
//! tool states (the tools' "programs" are scripts printing a version: no tool runs) and lists recorded by hand.
//! `claude` stands in as `sh` printing its arguments; a Codex launch runs a script that exits at once. SHOP runs the
//! default flow (Developer, on the default model: Claude Code); BILL runs it with its Agent stage on "Codex
//! developer" (Codex), so each tool has tickets of its own.

use otaq_core::service::testing::said;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::agent_tools::connected::AgentToolState;
use otaq_core::agent_tools::models::{ModelCopy, ModelEntry, ModelKind, ModelList};
use otaq_core::agent_tools::{LaunchContext, Tool};
use otaq_core::agents::gate::{Blocked, Decision, Facts, check, decide};
use otaq_core::agents::{
    Agent, AgentFields, AgentHold, AgentModel, Agents, CameFrom, FixedModels, HoldReason, ModelRef,
    SessionRole, ToolModel,
};
use otaq_core::board::moves::DropTarget;
use otaq_core::board::tickets::{EventKind, Ticket};
use otaq_core::domain::{Actor, StartStep, Status};
use otaq_core::flow::FlowAgent;
use otaq_core::protocol::Event;
use otaq_core::service::testing::TestService;
use otaq_core::session::SessionKind;
use otaq_core::store::now_ms;
use otaq_core::usage::limit::{LimitHit, LimitKind};
use tempfile::TempDir;

// The decision, as a table.

const A: Tool = Tool::Claude;
const B: Tool = Tool::Codex;

fn agent(id: &str, model: AgentModel) -> Agent {
    Agent {
        id: id.into(),
        name: id.into(),
        instructions: String::new(),
        model,
        effort: None,
        came_from: CameFrom::You,
        mark: None,
        created_at: 0,
        updated_at: 0,
    }
}

fn on(tool: Tool, model: Option<&str>) -> ModelRef {
    ModelRef {
        tool,
        model: model.map(str::to_owned),
    }
}

fn copy(tool: Tool, ids: &[&str]) -> ModelCopy {
    ModelCopy {
        tool,
        list: ModelList {
            entries: ids
                .iter()
                .map(|id| ModelEntry {
                    id: (*id).into(),
                    display_name: (*id).into(),
                    description: None,
                    resolves_to: None,
                    kind: ModelKind::Exact,
                    family: None,
                    released_at: None,
                })
                .collect(),
        },
        read_at: 0,
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Reason {
    Unavailable,
    Limit,
    Model,
}

/// What blocks A's model `m1` for `reason`, with the other tool (B) blocked too or free.
fn facts(reason: Reason, other_blocked: bool) -> Facts {
    let mut facts = Facts::default();
    match reason {
        Reason::Unavailable => {
            facts.unavailable.insert(A, "signed out".into());
        }
        Reason::Limit => {
            facts.at_limit.insert(A);
        }
        Reason::Model => {
            facts.lists.insert(A, copy(A, &["m2"]));
        }
    }
    if other_blocked {
        facts.at_limit.insert(B);
    }
    facts
}

/// A held agent waits for its own tool, whatever the reason and whether another tool could run it.
#[test]
fn every_reason_holds_on_its_own_tool_whatever_other_tools_can_do() {
    let model = AgentModel::Tool {
        tool: A,
        model: Some("m1".into()),
    };
    let developer = agent("developer", model);
    for reason in [Reason::Unavailable, Reason::Limit, Reason::Model] {
        for other_blocked in [false, true] {
            let decision = decide(
                &developer,
                &on(A, Some("m1")),
                &facts(reason, other_blocked),
            );
            let case = format!("{reason:?} other_blocked={other_blocked}");
            let expect_reason = match reason {
                Reason::Unavailable => HoldReason::ToolUnavailable,
                Reason::Limit => HoldReason::ToolLimit,
                Reason::Model => HoldReason::ModelUnavailable,
            };
            let Decision::Hold(held) = decision else {
                panic!("{case}: {decision:?}")
            };
            assert_eq!(held.blocked.reason(), expect_reason, "{case}");
            assert_eq!(held.agent.id, "developer", "{case}");
            assert_eq!(held.on, on(A, Some("m1")), "{case}");
        }
    }
}

#[test]
fn the_checks_run_in_order_and_pass_on_an_empty_model_or_a_list_never_read() {
    let strong = on(A, Some("m1"));
    let mut all = facts(Reason::Model, false);
    all.at_limit.insert(A);
    all.unavailable.insert(A, "not installed".into());
    assert_eq!(
        check(&strong, &all),
        Err(Blocked::Unavailable {
            reason: "not installed".into()
        })
    );
    all.unavailable.clear();
    assert_eq!(check(&strong, &all), Err(Blocked::AtLimit));
    all.at_limit.clear();
    assert_eq!(check(&strong, &all), Err(Blocked::ModelUnlisted));
    // A listed model passes as it is; an empty one passes no model; a list never read never blocks.
    all.lists.insert(A, copy(A, &["m1"]));
    assert_eq!(check(&strong, &all), Ok(Some("m1".into())));
    assert_eq!(check(&on(A, None), &facts(Reason::Model, false)), Ok(None));
    assert_eq!(check(&strong, &Facts::default()), Ok(Some("m1".into())));
    // Another tool's problem changes nothing.
    assert_eq!(
        check(&on(B, Some("gpt-5.5")), &facts(Reason::Unavailable, false)),
        Ok(Some("gpt-5.5".into()))
    );
}

#[test]
fn a_family_choice_resolves_to_its_newest_listed_model() {
    let mut list = copy(B, &["gpt-6-sol"]);
    list.list.entries.insert(
        0,
        ModelEntry {
            id: "newest:gpt-sol".into(),
            display_name: "GPT Sol".into(),
            description: None,
            resolves_to: Some("gpt-6-sol".into()),
            kind: ModelKind::Follow,
            family: Some("gpt-sol".into()),
            released_at: None,
        },
    );
    let facts = Facts {
        lists: HashMap::from([(B, list)]),
        ..Facts::default()
    };
    assert_eq!(
        check(&on(B, Some("newest:gpt-sol")), &facts),
        Ok(Some("gpt-6-sol".into()))
    );
    assert_eq!(
        check(&on(B, Some("newest:gpt-astra")), &facts),
        Err(Blocked::ModelUnlisted)
    );
}

#[test]
fn the_hand_check_seam_is_debug_only() {
    let parsed = otaq_core::agent_tools::models::dev_drop_model(Some("codex:gpt-5.4"));
    assert_eq!(parsed.is_some(), cfg!(debug_assertions));
    if cfg!(debug_assertions) {
        assert_eq!(parsed, Some((B, "gpt-5.4".into())));
    }
    for bad in [
        None,
        Some(""),
        Some("codex"),
        Some("codex:"),
        Some("nope:gpt-5.4"),
    ] {
        assert_eq!(otaq_core::agent_tools::models::dev_drop_model(bad), None);
    }
}

// The service.

fn me() -> Actor {
    Actor::User("me".into())
}

struct World {
    w: TestService,
    rules: Agents,
    /// BILL's key: its Agent stage runs "Codex developer".
    bill: String,
    /// The stand-in tools' folder (their scripts in `bin`).
    tools: TempDir,
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

    /// Saves `model` on the agent `name` as Settings → Agents does (its list check aside: the tests read no list) and
    /// says so as `agents.edit` does (`agents_changed`: the holds are checked again).
    async fn set_model(&self, name: &str, model: AgentModel) {
        self.w.set_agent_model(name, model).await;
        self.w.events.publish(Event::AgentsChanged);
    }

    async fn hide(&self, tool: Tool, hidden: bool) {
        self.w
            .connected
            .set_hidden(self.w.board.store(), tool, hidden)
            .await
            .unwrap();
    }

    async fn limit(&self, tool: Tool) {
        let hit = LimitHit {
            kind: LimitKind::Session,
            text: "You've hit your session limit · resets 3:45pm".into(),
        };
        self.w
            .plan_limit
            .record_hit(tool, &hit, now_ms())
            .await
            .unwrap();
    }

    fn bin(&self) -> PathBuf {
        self.tools.path().join("bin")
    }

    /// The `n`th BILL ticket's key.
    fn bill(&self, n: u32) -> String {
        format!("{}-{n}", self.bill)
    }
}

const ARGS: &str = "printf 'ARGS %s\\n' \"$*\"; exec cat";

/// Claude Code and Codex found and connected (stand-ins printing a version), so the default model is Claude Code's
/// own. Project SHOP (`shop-web`, the default flow) and BILL
/// (`billing-api`, its Agent stage on "Codex developer", Codex's default model).
async fn world() -> World {
    let tools = TempDir::new().unwrap();
    let bin = tools.path().join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::create_dir_all(tools.path().join("claude")).unwrap();
    otaq_core::process::testing::stub_command(&bin, "claude", r#"echo "2.1.4 (Claude Code)""#);
    otaq_core::process::testing::stub_command(&bin, "codex", r#"echo "codex-cli 0.157.1""#);
    let root = tools.path().to_path_buf();
    let agent_path = std::env::join_paths(
        std::iter::once(bin.clone()).chain(otaq_core::process::testing::sh_dirs()),
    )
    .unwrap();
    let w = TestService::start_with(move |mut config| {
        config.tools.path = bin.into_os_string();
        config.tools.claude_config_dir = Some(root.join("claude"));
        config.tools.managed_settings_dir = root.join("managed");
        config.check_tool_states = true;
        config.scheduler_tick = Duration::from_millis(50);
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
        w.board.store().default_model().await.unwrap() == Some(on(A, None))
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
        instructions: String::new(),
        model: "codex".into(),
        why: None,
    }];
    w.board.use_flow_for_tests(&bill, &file).await;
    let rules = Agents::new(
        w.board.clone(),
        Arc::new(FixedModels::new(vec![
            ToolModel::ToolDefault,
            ToolModel::Follow("sonnet".into()),
            ToolModel::Exact("gpt-5.3".into()),
            ToolModel::Exact("gpt-5.4".into()),
            ToolModel::Exact("gpt-5.5".into()),
        ])),
    );
    World {
        w,
        rules,
        bill,
        tools,
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

/// The user's drag to waiting to begin; the scheduler starts it.
async fn queue(w: &TestService, key: &str) {
    w.board
        .move_ticket(key, DropTarget::ReadyForDev, true, &me())
        .await
        .unwrap();
}

async fn until_status(w: &TestService, key: &str, want: Status) {
    let reached = tokio::time::timeout(Duration::from_secs(20), async {
        while w.status(key).await != want {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await;
    if reached.is_err() {
        let t = ticket(w, key).await;
        panic!(
            "{key} never became {want}: {} {:?}",
            t.status, t.start_problem
        );
    }
}

/// The ticket left waiting to begin's waiting states: its agent started (or its start was tried and stopped).
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

async fn count(w: &TestService, key: &str, kind: EventKind) -> usize {
    w.board
        .history(key)
        .await
        .unwrap()
        .iter()
        .filter(|e| e.kind == kind)
        .count()
}

/// How many agent sessions the ticket has had.
async fn sessions(w: &TestService, key: &str) -> i64 {
    let t = ticket(w, key).await;
    sqlx::query_scalar("SELECT COUNT(*) FROM sessions WHERE ticket_id = ?")
        .bind(&t.id)
        .fetch_one(w.board.store().pool())
        .await
        .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_tool_that_isnt_available_holds_only_its_tickets_until_it_is_back() {
    let world = world().await;
    let w = &world.w;
    let bill = world.bill(1);
    w.create(&world.bill, "Add coupon codes").await;
    w.create("SHOP", "Update footer links").await;
    world.hide(B, true).await;
    queue(w, &bill).await;
    queue(w, "SHOP-1").await;

    // SHOP-1 runs on Claude Code; the BILL ticket waits waiting to begin, nothing made for it.
    until_status(w, "SHOP-1", Status::Working).await;
    until_status(w, &bill, Status::Held).await;
    let held = ticket(w, &bill).await;
    let problem = held.start_problem.clone().unwrap();
    assert_eq!(problem.step, StartStep::ToolUnavailable);
    assert_eq!(
        problem.message,
        "Codex isn\u{2019}t available \u{b7} hidden in Settings"
    );
    assert!(
        held.work
            .as_ref()
            .and_then(|w| w.worktree.as_ref())
            .is_none()
    );
    let codex_developer = world.agent("Codex developer").await;
    let [hold] = &holds(w, &bill).await[..] else {
        panic!("one hold")
    };
    assert_eq!(
        (hold.role, hold.reason, hold.tool),
        (SessionRole::Developer, HoldReason::ToolUnavailable, B)
    );
    assert_eq!(hold.agent_id.as_deref(), Some(codex_developer.id.as_str()));
    assert!(
        !w.board.store().developer_started(&held.id).await.unwrap(),
        "nothing locks"
    );

    // Nothing retries on a timer, and nothing moves to another tool: many scheduler passes later it is held once.
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert_eq!(w.status(&bill).await, Status::Held);
    assert_eq!(count(w, &bill, EventKind::Held).await, 1);
    assert_eq!(sessions(w, &bill).await, 0);

    // Shown again: the start runs once, as the drag asked for it.
    world.hide(B, false).await;
    until_started(w, &bill).await;
    assert!(holds(w, &bill).await.is_empty());
    assert_eq!(count(w, &bill, EventKind::Held).await, 1);
    assert_eq!(sessions(w, &bill).await, 1, "one launch on Codex");
    world.w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_tool_at_its_limit_holds_its_agents_while_other_tools_keep_starting() {
    let world = world().await;
    let w = &world.w;
    let bill = world.bill(1);
    w.create("SHOP", "Checkout flow").await;
    w.create(&world.bill, "Add coupon codes").await;
    world.limit(A).await;
    queue(w, "SHOP-1").await;
    queue(w, &bill).await;
    until_status(w, "SHOP-1", Status::Held).await;
    let problem = ticket(w, "SHOP-1").await.start_problem.unwrap();
    assert_eq!(
        (problem.step, problem.message.as_str()),
        (StartStep::ToolLimit, "Claude Code limit reached")
    );
    let [hold] = &holds(w, "SHOP-1").await[..] else {
        panic!("one hold")
    };
    assert_eq!((hold.reason, hold.tool), (HoldReason::ToolLimit, A));
    // The Codex ticket started meanwhile.
    until_started(w, &bill).await;
    assert!(holds(w, &bill).await.is_empty());

    // The limit is resumed: the held start runs.
    w.plan_limit.resume_tool(A).await.unwrap();
    until_status(w, "SHOP-1", Status::Working).await;
    assert!(holds(w, "SHOP-1").await.is_empty());
    world.w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_model_no_longer_listed_holds_marks_and_lifts_on_the_list_or_an_edit() {
    let world = world().await;
    let w = &world.w;
    let (first, second) = (world.bill(1), world.bill(2));
    // Codex's list without gpt-5.4 (as a refresh with the hand-check seam would leave it).
    w.models
        .record(B, Ok(copy(B, &["gpt-5.5"]).list), now_ms())
        .await;
    world
        .set_model(
            "Codex developer",
            AgentModel::Tool {
                tool: B,
                model: Some("gpt-5.4".into()),
            },
        )
        .await;
    w.create(&world.bill, "Export orders as CSV").await;
    queue(w, &first).await;
    until_status(w, &first, Status::Held).await;
    let problem = ticket(w, &first).await.start_problem.unwrap();
    assert_eq!(problem.step, StartStep::ModelUnavailable);
    assert_eq!(
        problem.message,
        "Model unavailable \u{b7} Codex no longer lists gpt-5.4"
    );
    let [hold] = &holds(w, &first).await[..] else {
        panic!("one hold")
    };
    assert_eq!(hold.reason, HoldReason::ModelUnavailable);
    let codex_developer = world.agent("Codex developer").await;
    let mark = codex_developer.mark.clone().expect("marked");
    assert_eq!(mark.text, "Codex no longer lists gpt-5.4");
    assert_eq!(mark.model.as_deref(), Some("gpt-5.4"));
    // The card names the agent to open (Open agent…, AGM-03).
    let cards = otaq_core::agents::facts::board_agents(&w.board, &world.bill)
        .await
        .unwrap();
    let card = cards
        .cards
        .iter()
        .find(|c| c.key.to_string() == first)
        .unwrap();
    assert_eq!(
        card.holds[0].agent_id.as_deref(),
        Some(codex_developer.id.as_str())
    );

    // The list has the model again: the mark clears and the start runs.
    w.models
        .record(B, Ok(copy(B, &["gpt-5.5", "gpt-5.4"]).list), now_ms())
        .await;
    until_started(w, &first).await;
    until("the mark cleared", || async {
        world.agent("Codex developer").await.mark.is_none()
    })
    .await;

    // Open agent…: another model saved on the agent lifts the next hold.
    world
        .set_model(
            "Codex developer",
            AgentModel::Tool {
                tool: B,
                model: Some("gpt-5.3".into()),
            },
        )
        .await;
    w.models
        .record(B, Ok(copy(B, &["gpt-5.5"]).list), now_ms())
        .await;
    w.create(&world.bill, "Order totals rounding").await;
    queue(w, &second).await;
    until_status(w, &second, Status::Held).await;
    world
        .set_model(
            "Codex developer",
            AgentModel::Tool {
                tool: B,
                model: Some("gpt-5.5".into()),
            },
        )
        .await;
    until_started(w, &second).await;
    world.w.stop().await;
}

/// A ticket held for Claude Code's limit waits, with Codex free, and starts on Claude Code once the limit resets:
/// nothing moves it to another tool, before or after.
#[tokio::test(flavor = "multi_thread")]
async fn a_ticket_held_for_a_limit_waits_and_starts_on_its_own_tool_after_the_reset() {
    let world = world().await;
    let w = &world.w;
    w.create("SHOP", "Checkout flow").await;
    let hit = LimitHit {
        kind: LimitKind::Session,
        text: "You've hit your session limit".into(),
    };
    let now = now_ms();
    let resets_at = now + 60_000;
    w.plan_limit
        .record_hit_reset(A, &hit, Some(resets_at), now)
        .await
        .unwrap();
    queue(w, "SHOP-1").await;
    until_status(w, "SHOP-1", Status::Held).await;
    // Codex can run, yet it waits: many scheduler passes later it is still held, with no session.
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert_eq!(w.status("SHOP-1").await, Status::Held);
    assert_eq!(sessions(w, "SHOP-1").await, 0);
    let [hold] = &holds(w, "SHOP-1").await[..] else {
        panic!("one hold")
    };
    assert_eq!((hold.reason, hold.tool), (HoldReason::ToolLimit, A));

    // The limit resets: the held start runs, on Claude Code.
    assert!(w.plan_limit.check_reset(resets_at + 1).await.unwrap());
    until_status(w, "SHOP-1", Status::Working).await;
    assert!(holds(w, "SHOP-1").await.is_empty());
    let t = ticket(w, "SHOP-1").await;
    let session = w.sessions.running_of(&t.id, SessionKind::Agent).unwrap();
    assert_eq!(session.tool, Some(A));
    assert_eq!(sessions(w, "SHOP-1").await, 1);
    world.w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_launch_that_fails_because_of_its_tool_rechecks_it_and_holds() {
    let world = world().await;
    let w = &world.w;
    let bill = world.bill(1);
    w.create(&world.bill, "Add coupon codes").await;
    // Codex went away since the last check: the launch can't find it.
    std::fs::remove_file(otaq_core::process::testing::stub_file(
        &world.bin(),
        "codex",
    ))
    .unwrap();
    queue(w, &bill).await;
    until_status(w, &bill, Status::Held).await;
    let problem = ticket(w, &bill).await.start_problem.unwrap();
    assert_eq!(problem.step, StartStep::ToolUnavailable);
    assert_eq!(
        problem.message,
        "Codex isn\u{2019}t available \u{b7} not installed"
    );
    assert_eq!(w.connected.status(B).state, AgentToolState::NotInstalled);
    world.w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn resumes_are_never_checked() {
    let world = world().await;
    let w = &world.w;
    w.create("SHOP", "Price table").await;
    queue(w, "SHOP-1").await;
    until_status(w, "SHOP-1", Status::Working).await;
    w.review.park("SHOP-1", true, &me()).await.unwrap();
    // Claude Code at its limit and hidden: the parked conversation still resumes on it.
    world.limit(A).await;
    world.hide(A, true).await;
    queue(w, "SHOP-1").await;
    until_status(w, "SHOP-1", Status::Working).await;
    assert_eq!(count(w, "SHOP-1", EventKind::Held).await, 0);
    assert!(holds(w, "SHOP-1").await.is_empty());
    world.w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_held_validator_waits_in_dev_and_starts_once_its_tool_is_back() {
    let world = world().await;
    let w = &world.w;
    let t = w.create("SHOP", "Cart totals").await;
    world
        .set_model(
            "Validator",
            AgentModel::Tool {
                tool: A,
                model: Some("sonnet".into()),
            },
        )
        .await;
    w.give_worktree(&t).await;
    w.force_status(&t, Status::Working).await;
    world.hide(A, true).await;
    let after = w
        .validation
        .submit(
            "SHOP-1",
            said(&w.board, "SHOP-1", "Cart totals add up.")
                .await
                .as_ref(),
            &Actor::Agent(t.key.clone()),
            otaq_core::domain::ValidationTrigger::Agent,
        )
        .await
        .unwrap();
    assert_eq!(after.status, Status::Checking);
    assert!(
        w.sessions
            .running_of(&t.id, SessionKind::Validator)
            .is_none()
    );
    let [hold] = &holds(w, "SHOP-1").await[..] else {
        panic!("one hold")
    };
    assert_eq!(
        (hold.role, hold.reason),
        (SessionRole::Validator, HoldReason::ToolUnavailable)
    );
    world.hide(A, false).await;
    until("the validator started", || async {
        w.sessions
            .running_of(&t.id, SessionKind::Validator)
            .is_some()
    })
    .await;
    assert!(holds(w, "SHOP-1").await.is_empty());
    world.w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_project_agent_is_held_too() {
    let world = world().await;
    let w = &world.w;
    // The agent Noyan uses on Codex, with Claude Code free: it is held all the same.
    world.hide(B, true).await;
    world
        .set_model(
            "Noyan",
            AgentModel::Tool {
                tool: B,
                model: None,
            },
        )
        .await;
    let e = w
        .project_agents
        .open("SHOP", "Why is checkout slow?")
        .await
        .unwrap_err();
    assert_eq!(
        e.to_string(),
        "Codex isn\u{2019}t available \u{b7} hidden in Settings"
    );
    let project = w.board.project("SHOP").await.unwrap();
    let [hold] = &w.board.store().project_holds(&project.id).await.unwrap()[..] else {
        panic!("one hold")
    };
    assert_eq!((hold.role, hold.tool), (SessionRole::ProjectAgent, B));

    // Back: the Project agent's message waits for a send.
    world.hide(B, false).await;
    until("the project hold went", || async {
        w.board
            .store()
            .project_holds(&project.id)
            .await
            .unwrap()
            .is_empty()
    })
    .await;
    assert!(w.project_agents.list("SHOP").await.unwrap().open.is_empty());
    world.w.stop().await;
}

/// A swap (AGT-05, Q-J): on a tool that quietly runs another model (OpenCode here), only an exact model asked for
/// and another one run marks the agent; a follow choice choosing a model never does (a tool that refuses instead is
/// `mark.rs`'s unit test).
#[tokio::test(flavor = "multi_thread")]
async fn the_swap_mark_is_only_for_an_exact_model_on_a_swapping_tool() {
    let world = world().await;
    let w = &world.w;
    let payload: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/opencode/events/session.step.started.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let mut marks = Vec::new();
    for (n, (tool, asked)) in [
        (Tool::OpenCode, "opencode/gpt-5.5"),
        (Tool::OpenCode, "newest:big"),
    ]
    .into_iter()
    .enumerate()
    {
        let name = format!("OC {n}");
        let agent = world
            .rules
            .add(&AgentFields {
                name: name.clone(),
                instructions: String::new(),
                model: AgentModel::Tool { tool, model: None },
                effort: None,
            })
            .await
            .unwrap();
        let t = w.create("SHOP", "Order totals rounding").await;
        w.force_status(&t, Status::Working).await;
        let shell = w.agent_shell(&t).await;
        sqlx::query(
            "UPDATE sessions SET tool = ?, model = ?, agent_id = ?, agent_name = ? WHERE id = ?",
        )
        .bind(tool.as_str())
        .bind(asked)
        .bind(&agent.id)
        .bind(&name)
        .bind(&shell.session)
        .execute(w.board.store().pool())
        .await
        .unwrap();
        let caller = otaq_core::caller::Caller::Agent {
            ticket: t.key.clone(),
            ticket_id: t.id.clone(),
            session: shell.session.clone(),
        };
        w.signals
            .hook(&caller, tool, "session.step.started", &payload)
            .await
            .unwrap();
        marks.push(world.agent(&name).await.mark.map(|m| m.text));
    }
    assert_eq!(
        marks,
        [
            Some(
                "OpenCode ran opencode/big-pickle instead of opencode/gpt-5.5 on SHOP-1".to_owned()
            ),
            None
        ]
    );
    world.w.stop().await;
}

/// Change agent… on a card held for its tool's limit: the held Dev hasn't started, so its choice isn't locked and the
/// user's pick of an agent on a tool that can run (`ticket.set_choice`) starts it on that agent at once; the agents
/// offered are those whose tool has Dev's skill. The ticket's own agent can't make that pick.
#[tokio::test(flavor = "multi_thread")]
async fn change_agent_on_a_card_held_for_a_limit_starts_it_on_the_chosen_agent() {
    use otaq_core::client::{Client, ClientConfig};
    use otaq_core::protocol::{Reply, Request, SetChoiceParams};
    let world = world().await;
    let w = &world.w;
    w.create("SHOP", "Checkout flow").await;
    world.limit(A).await;
    queue(w, "SHOP-1").await;
    until_status(w, "SHOP-1", Status::Held).await;
    let held = w.board.ticket_choices("SHOP-1").await.unwrap();
    let dev = held.stages.iter().find(|s| s.stage_id == "dev").unwrap();
    assert!(dev.held && !dev.locked, "{dev:?}");
    let codex = dev
        .agents
        .iter()
        .find(|a| a.name == "Codex developer")
        .expect("an agent on Codex is offered: Dev runs one of Otaq's own skills");
    assert_eq!(codex.tool, Some(B));

    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    let reply = client
        .request(Request::TicketSetChoice(SetChoiceParams {
            key: "SHOP-1".into(),
            stage: "Dev".into(),
            choice: "Codex developer".into(),
        }))
        .await
        .unwrap();
    let Reply::TicketChoices(after) = reply else {
        panic!("{reply:?}")
    };
    let dev = after.stages.iter().find(|s| s.stage_id == "dev").unwrap();
    assert_eq!(dev.label(), "Codex developer");
    // It starts on Codex (whose stand-in exits at once, so the card then stops as an interrupted start).
    until_started(w, "SHOP-1").await;
    assert!(holds(w, "SHOP-1").await.is_empty());
    let t = ticket(w, "SHOP-1").await;
    let started: Vec<(Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT agent_name, tool FROM sessions WHERE ticket_id = ? AND kind = 'agent'",
    )
    .bind(&t.id)
    .fetch_all(w.board.store().pool())
    .await
    .unwrap();
    assert_eq!(
        started,
        [(
            Some("Codex developer".to_owned()),
            Some(B.as_str().to_owned())
        )]
    );
    // Started now: the choice is locked.
    let locked = w.board.ticket_choices("SHOP-1").await.unwrap();
    let dev = locked.stages.iter().find(|s| s.stage_id == "dev").unwrap();
    assert!(dev.locked && !dev.held, "{dev:?}");
    world.w.stop().await;
}
