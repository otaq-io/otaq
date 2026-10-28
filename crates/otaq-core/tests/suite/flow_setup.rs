//! T-0433 (ASK-02, ASK-03, ASK-04, ASK-10; d0108, d0086, d0068): asking a tool for a flow, several proposals and
//! Flow setup, through the service. **Ask <Tool>** starts a flow agent on that tool with no model option and ASK-02's
//! first message; one per tool; refused at the tool's limit, for a tool that isn't connected and for a project not in
//! Flow setup. During Flow setup each flow agent keeps one open proposal; Use this flow closes the others (their
//! agents told "The user chose another flow." and ended), as do Use the default and a preset applied. Outside Flow
//! setup a new proposal still replaces the old one (FLW-03). Flow setup is set on Add project, kept across a restart,
//! and refuses tickets.
//!
//! Every tool is a stand-in: `claude` is `sh` printing its arguments, `opencode` an `sh` script in Otaq's bin folder
//! printing its arguments and launch config; both then echo what is typed. Tools are marked connected by hand (no
//! detect runs). `OTAQ_HOME`, the repositories and the tools' folders are temporary; no real tool or history is read.
//!
//! Unix only: the stand-in agent tools are `sh` scripts started by name, which Windows can't run (Q598).
#![cfg(unix)]

use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::time::Duration;

use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::agent_tools::connected::AgentToolState;
use otaq_core::agent_tools::{LaunchContext, Tool};
use otaq_core::agents::{AgentModel, FixedModels, Role};
use otaq_core::board::batch::parse_markdown;
use otaq_core::board::projects::{AddProject, flow_not_set_up};
use otaq_core::board::tickets::NewTicketInput;
use otaq_core::client::{ClientError, request_once};
use otaq_core::domain::Actor;
use otaq_core::flow::apply::{ApplyParams, ProposeFrom, ProposeFromParams};
use otaq_core::flow::presets::{SOLO_AUTOPILOT_FILE, SPEC_FIRST_FILE, TEAM_PR_FILE, preset};
use otaq_core::flow::proposal::{ApplyChoices, Proposal, ProposedBy};
use otaq_core::flow::propose::Proposed;
use otaq_core::project_agents::first_flow::{
    CHOSE_ANOTHER_FLOW, FlowSetup, FlowSetupRow, NOT_SIGNED_IN, first_flow_message,
    flow_agent_name, not_in_flow_setup,
};
use otaq_core::project_agents::{AgentState, LIMIT_REFUSAL, ProjectAgent};
use otaq_core::protocol::{ErrorBody, Event, OnboardingAskParams, ProjectParams, Reply, Request};
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::TestService;
use otaq_core::store::now_ms;
use otaq_core::usage::limit::{LimitHit, LimitKind};
use tempfile::TempDir;

/// Prints its arguments and OpenCode's launch config, then echoes what is typed.
const SCRIPT: &str = "printf 'ARGS %s\\n' \"$*\"; \
                      printf 'CONFIG %s\\n' \"${OPENCODE_CONFIG_CONTENT-none}\"; exec cat";

fn me() -> Actor {
    Actor::User("me".into())
}

/// The `opencode` stand-in: answers the conversation made before the launch, then runs [`SCRIPT`].
fn opencode(bin: &Path) {
    let file = bin.join(Tool::OpenCode.command_name());
    std::fs::write(
        &file,
        format!("#!/bin/sh\ncase \"$1\" in api) exit 0 ;; esac\n{SCRIPT}\n"),
    )
    .unwrap();
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).unwrap();
}

struct World {
    w: TestService,
    _tools: TempDir,
}

/// The `claude` stand-in: prints like [`SCRIPT`], then only reads its input, one line at a time, and records each
/// line in `heard` (the terminal's echo isn't reading).
fn reader(heard: &Path) -> String {
    format!(
        "printf 'ARGS %s\\n' \"$*\"; printf 'CONFIG none\\n'; \
         while IFS= read -r line; do printf '%s\\n' \"$line\" >> '{}'; done",
        heard.display()
    )
}

fn config(bin: &Path) -> impl FnOnce(ServiceConfig) -> ServiceConfig + use<> {
    let bin = bin.to_path_buf();
    let script = reader(&heard(&bin));
    move |config| ServiceConfig {
        launch: Some(LaunchContext {
            bin_dir: bin,
            plugin_dir: "/nonexistent/plugin".into(),
            path: std::env::var_os("PATH").unwrap_or_default(),
            claude: ClaudeLaunch {
                program: vec!["sh".into(), "-c".into(), script, "sh".into()],
                config_dir: None,
            },
        }),
        ..config
    }
}

/// Where the `claude` stand-ins of a world record what they read.
fn heard(bin: &Path) -> std::path::PathBuf {
    bin.with_file_name("heard")
}

/// Claude Code and OpenCode connected, Copilot found but signed out, Codex not installed; the Project agent role's
/// agent on Claude Code's opus at high effort.
async fn world() -> World {
    let tools = TempDir::new().unwrap();
    let bin = tools.path().join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    opencode(&bin);
    let w = TestService::start_with(config(&bin)).await;
    connect(&w).await;
    let role = w
        .board
        .store()
        .role_agent(Role::ProjectAgent)
        .await
        .unwrap()
        .expect("a Project agent");
    w.edit_agent(&role.name, |page| {
        page.model = AgentModel::Tool {
            tool: Tool::Claude,
            model: Some("opus".into()),
        };
        page.effort = Some("high".into());
    })
    .await;
    // Stand-ins report no turn's end: an unchosen flow agent ends after a short wait here.
    w.project_agents
        .set_answer_grace(Duration::from_millis(500));
    World { w, _tools: tools }
}

/// Marks the tools' states by hand, once the service's own check at start has recorded what it found (nothing).
async fn connect(w: &TestService) {
    tokio::time::timeout(Duration::from_secs(20), async {
        while !Tool::ALL
            .iter()
            .all(|&t| w.connected.status(t).checked_at.is_some())
        {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the check at start");
    let connected = || AgentToolState::Connected {
        version: None,
        account: None,
    };
    w.connected
        .set_detected(Tool::Claude, connected(), now_ms());
    w.connected
        .set_detected(Tool::OpenCode, connected(), now_ms());
    w.connected.set_detected(
        Tool::Copilot,
        AgentToolState::FoundNotSignedIn { version: None },
        now_ms(),
    );
}

async fn send(w: &TestService, request: Request) -> Result<Reply, ErrorBody> {
    match request_once(&w.data.socket_path(), request).await {
        Ok(reply) => Ok(reply),
        Err(ClientError::Service(body)) => Err(body),
        Err(e) => panic!("{e}"),
    }
}

/// Adds the repository `name` as the app's Add project does (in Flow setup); its key.
async fn add_in_flow_setup(w: &TestService, name: &str) -> String {
    let repo = w.repos.repo(name, 1);
    let reply = send(
        w,
        Request::ProjectAdd(AddProject {
            path: repo.display().to_string(),
            flow_setup: true,
            ..Default::default()
        }),
    )
    .await
    .unwrap();
    let Reply::ProjectAdded { project, .. } = reply else {
        panic!("{reply:?}")
    };
    assert!(project.flow_setup_since.is_some(), "{project:?}");
    project.key
}

async fn ask(w: &TestService, project: &str, tool: Tool) -> Result<ProjectAgent, ErrorBody> {
    let request = Request::OnboardingAsk(OnboardingAskParams {
        project: project.into(),
        tool,
    });
    send(w, request).await.map(|reply| match reply {
        Reply::ProjectAgent(agent) => *agent,
        other => panic!("{other:?}"),
    })
}

async fn setup(w: &TestService, project: &str) -> FlowSetup {
    let request = Request::OnboardingFlowSetup(ProjectParams {
        project: project.into(),
    });
    match send(w, request).await.unwrap() {
        Reply::FlowSetup(setup) => *setup,
        other => panic!("{other:?}"),
    }
}

fn row(setup: &FlowSetup, tool: Tool) -> Option<&FlowSetupRow> {
    setup.tools.iter().find(|t| t.tool == tool).map(|t| &t.row)
}

async fn in_flow_setup(w: &TestService, project: &str) -> bool {
    w.board
        .project(project)
        .await
        .unwrap()
        .flow_setup_since
        .is_some()
}

async fn agents(w: &TestService) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM project_agents")
        .fetch_one(w.board.store().pool())
        .await
        .unwrap()
}

/// The first `ARGS` / `CONFIG` line the session printed.
async fn printed(w: &TestService, agent: &ProjectAgent, what: &str) -> String {
    let session = agent.session.as_ref().expect("a session");
    let out = w.wait_for_output(&session.id, "CONFIG ").await;
    out.lines()
        .find(|l| l.starts_with(what))
        .unwrap_or_default()
        .trim_end()
        .to_owned()
}

/// `agent` proposes the preset `file`'s flow, as `otaq flow propose --stdin` from its session does.
async fn propose(w: &TestService, project: &str, agent: &ProjectAgent, file: &str) -> Proposal {
    let by = ProposedBy::ProjectAgent {
        agent: agent.id.clone(),
        session: agent.session.as_ref().expect("a session").id.clone(),
        name: agent.name.clone(),
        tool: agent.tool,
    };
    w.board
        .propose(
            project,
            &Proposed::Document {
                yaml: preset(file).unwrap().yaml.to_owned(),
                file: None,
            },
            &by,
            &FixedModels::sample(),
        )
        .await
        .unwrap_or_else(|e| panic!("{file}: {e}"))
}

async fn open_proposals(w: &TestService, project: &str) -> Vec<Proposal> {
    w.board
        .proposals(project, &FixedModels::sample())
        .await
        .unwrap()
}

/// Waits until the agent `id` has ended and left the switcher (Recently ended).
async fn ended(w: &TestService, id: &str) -> ProjectAgent {
    tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let agent = w.project_agents.get(id).await.unwrap();
            if agent.state == AgentState::Ended && agent.closed {
                return agent;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("{id} never ended"))
}

#[tokio::test(flavor = "multi_thread")]
async fn asking_starts_a_flow_agent_on_the_tool_default_one_per_tool() {
    let World { w, _tools } = world().await;
    let shop = add_in_flow_setup(&w, "shop-web").await;

    // An ordinary Project agent runs on the role's model; a flow agent on the asked tool with no model option.
    let ordinary = w
        .project_agents
        .open(&shop, "Why is checkout slow?")
        .await
        .unwrap();
    let args = printed(&w, &ordinary, "ARGS ").await;
    assert!(args.contains("--model opus"), "{args}");

    let claude = ask(&w, &shop, Tool::Claude).await.unwrap();
    assert_eq!(claude.state, AgentState::Running);
    assert_eq!((claude.tool, claude.model.as_deref()), (Tool::Claude, None));
    assert_eq!(claude.name, flow_agent_name(Tool::Claude));
    let args = printed(&w, &claude, "ARGS ").await;
    assert!(!args.contains("--model"), "no model option: {args}");
    assert!(!args.contains("--effort"), "no effort either: {args}");
    let stored: (Option<String>, Option<String>, Option<String>) =
        sqlx::query_as("SELECT model, effort, first_request FROM project_agents WHERE id = ?")
            .bind(&claude.id)
            .fetch_one(w.board.store().pool())
            .await
            .unwrap();
    let message = first_flow_message("shop-web", Tool::Claude);
    assert_eq!(stored, (None, None, Some(message.clone())));
    // ASK-02's first message: the tool and the project, its own history and the repo, nothing else.
    for words in [
        "This is shop-web's first flow. Look at how the user works with ",
        "your own history on this Mac (find it yourself; read only what the user typed, never login or \
         credential data)",
        "Nothing else: not GitHub or GitLab, not skills or plugins in the home folder. Then propose a whole flow.",
    ] {
        assert!(message.contains(words), "{message}");
        assert!(args.contains(words), "{args}");
    }
    assert!(args.contains(&format!("works with {}:", Tool::Claude.display_name())));

    let opencode = ask(&w, &shop, Tool::OpenCode).await.unwrap();
    assert_eq!(
        (opencode.tool, opencode.model.as_deref()),
        (Tool::OpenCode, None)
    );
    let config = printed(&w, &opencode, "CONFIG ").await;
    assert!(config.starts_with("CONFIG {"), "{config}");
    assert!(
        !config.contains("\"model\""),
        "no model in the launch: {config}"
    );
    assert!(
        printed(&w, &opencode, "ARGS ")
            .await
            .contains(&format!("works with {}:", Tool::OpenCode.display_name()))
    );

    // One per tool: asking again returns the running one.
    assert_eq!(ask(&w, &shop, Tool::Claude).await.unwrap().id, claude.id);
    assert_eq!(
        ask(&w, &shop, Tool::OpenCode).await.unwrap().id,
        opencode.id
    );
    assert_eq!(agents(&w).await, 3);
    // Once it ended, asking starts a new one.
    w.project_agents.close(&claude.id, true).await.unwrap();
    let again = ask(&w, &shop, Tool::Claude).await.unwrap();
    assert_ne!(again.id, claude.id);
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn asking_is_refused_at_the_limit_for_a_tool_not_connected_and_outside_flow_setup() {
    let World { w, _tools } = world().await;
    let shop = add_in_flow_setup(&w, "shop-web").await;

    let hit = LimitHit {
        kind: LimitKind::Session,
        text: "You've hit your session limit · resets 3:45pm".into(),
    };
    w.plan_limit
        .record_hit(Tool::Claude, &hit, now_ms())
        .await
        .unwrap();
    let e = ask(&w, &shop, Tool::Claude).await.unwrap_err();
    assert_eq!(e.message, LIMIT_REFUSAL);
    // Another tool's limit doesn't hold it.
    ask(&w, &shop, Tool::OpenCode).await.unwrap();
    assert!(matches!(
        row(&setup(&w, &shop).await, Tool::Claude),
        Some(FlowSetupRow::PlanLimit { agent: None, .. })
    ));
    w.plan_limit.resume_tool(Tool::Claude).await.unwrap();
    ask(&w, &shop, Tool::Claude).await.unwrap();

    let e = ask(&w, &shop, Tool::Copilot).await.unwrap_err();
    assert_eq!(e.message, NOT_SIGNED_IN);
    let e = ask(&w, &shop, Tool::Codex).await.unwrap_err();
    assert!(
        e.message.starts_with(&format!(
            "{} isn\u{2019}t available",
            Tool::Codex.display_name()
        )),
        "{e:?}"
    );
    assert_eq!(agents(&w).await, 2, "nothing stored for the refusals");

    // A project on its flow already (not added through Add project).
    let bill = w.add_project("billing-api").await.key;
    let e = ask(&w, &bill, Tool::Claude).await.unwrap_err();
    assert_eq!(e.message, not_in_flow_setup("billing-api"));
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_flow_setup_rows_follow_each_tool() {
    let World { w, _tools } = world().await;
    let shop = add_in_flow_setup(&w, "shop-web").await;
    let mut events = w.events.subscribe();

    let s = setup(&w, &shop).await;
    assert_eq!(
        (s.project.as_str(), s.project_name.as_str()),
        (shop.as_str(), "shop-web")
    );
    assert!(s.since.is_some());
    let tools: Vec<Tool> = s.tools.iter().map(|t| t.tool).collect();
    assert_eq!(
        tools,
        [Tool::Claude, Tool::Copilot, Tool::OpenCode],
        "connected and signed out"
    );
    assert_eq!(row(&s, Tool::Claude), Some(&FlowSetupRow::NotAsked));
    assert_eq!(row(&s, Tool::Copilot), Some(&FlowSetupRow::NotSignedIn));

    let claude = ask(&w, &shop, Tool::Claude).await.unwrap();
    let opencode = ask(&w, &shop, Tool::OpenCode).await.unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Ok(Event::FlowSetupChanged { project }) = events.recv().await
                && project == shop
            {
                return;
            }
        }
    })
    .await
    .expect("flow_setup_changed pushed");
    let s = setup(&w, &shop).await;
    assert!(
        matches!(row(&s, Tool::Claude), Some(FlowSetupRow::Working { agent, .. }) if *agent == claude.id),
        "{s:?}"
    );

    let proposal = propose(&w, &shop, &claude, SPEC_FIRST_FILE).await;
    w.project_agents.close(&opencode.id, true).await.unwrap();
    let s = setup(&w, &shop).await;
    assert_eq!(
        row(&s, Tool::Claude),
        Some(&FlowSetupRow::Proposed {
            agent: claude.id.clone(),
            proposal: proposal.id
        })
    );
    assert_eq!(
        row(&s, Tool::OpenCode),
        Some(&FlowSetupRow::Ended {
            agent: opencode.id.clone()
        })
    );
    assert_eq!(s.proposals.len(), 1);
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn two_proposals_and_use_this_flow_closes_the_other_and_ends_its_agent() {
    let World { w, _tools } = world().await;
    let shop = add_in_flow_setup(&w, "shop-web").await;
    let claude = ask(&w, &shop, Tool::Claude).await.unwrap();
    let opencode = ask(&w, &shop, Tool::OpenCode).await.unwrap();

    let first = propose(&w, &shop, &claude, SPEC_FIRST_FILE).await;
    let theirs = propose(&w, &shop, &opencode, TEAM_PR_FILE).await;
    let open = open_proposals(&w, &shop).await;
    let by: Vec<(u32, Option<Tool>)> = open.iter().map(|p| (p.id, p.proposed_by.tool())).collect();
    assert_eq!(
        by,
        [
            (first.id, Some(Tool::Claude)),
            (theirs.id, Some(Tool::OpenCode))
        ]
    );
    // A new proposal from the same agent replaces its own only.
    let mine = propose(&w, &shop, &claude, SOLO_AUTOPILOT_FILE).await;
    let ids: Vec<u32> = open_proposals(&w, &shop)
        .await
        .iter()
        .map(|p| p.id)
        .collect();
    assert_eq!(ids, [theirs.id, mine.id]);

    // Use this flow on Claude Code's: it applies, OpenCode's closes, its agent is told and ends.
    let reply = send(
        &w,
        Request::FlowApply(ApplyParams {
            project: shop.clone(),
            proposal: mine.id,
            choices: ApplyChoices::default(),
        }),
    )
    .await
    .unwrap();
    assert!(matches!(reply, Reply::FlowApplied(_)), "{reply:?}");
    assert!(open_proposals(&w, &shop).await.is_empty());
    assert!(!in_flow_setup(&w, &shop).await);
    let told = opencode.session.as_ref().unwrap().id.clone();
    w.wait_for_output(&told, CHOSE_ANOTHER_FLOW).await;
    ended(&w, &opencode.id).await;
    // The chosen agent stays, its answer waiting to be typed.
    let chosen = w.project_agents.get(&claude.id).await.unwrap();
    assert_eq!(chosen.state, AgentState::Running);
    let pending = w
        .deliveries
        .pending(&claude.session.as_ref().unwrap().id)
        .await
        .unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(
        pending[0].text,
        format!("The user applied Proposal {}.", mine.id)
    );
    // The board takes tickets now.
    w.create(&shop, "Rate limit").await;
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn use_the_default_and_a_preset_close_every_proposal_and_agent() {
    let World { w, _tools } = world().await;
    let shop = add_in_flow_setup(&w, "shop-web").await;
    let claude = ask(&w, &shop, Tool::Claude).await.unwrap();
    let opencode = ask(&w, &shop, Tool::OpenCode).await.unwrap();
    propose(&w, &shop, &claude, SPEC_FIRST_FILE).await;
    let before = w.board.effective_flow(&shop).await.unwrap();
    let reply = send(
        &w,
        Request::OnboardingUseDefault(ProjectParams {
            project: shop.clone(),
        }),
    )
    .await
    .unwrap();
    assert!(matches!(reply, Reply::Ok), "{reply:?}");
    assert!(open_proposals(&w, &shop).await.is_empty());
    assert!(!in_flow_setup(&w, &shop).await);
    assert_eq!(
        w.board.effective_flow(&shop).await.unwrap(),
        before,
        "the default flow stays"
    );
    for agent in [&claude, &opencode] {
        w.wait_for_output(&agent.session.as_ref().unwrap().id, CHOSE_ANOTHER_FLOW)
            .await;
        ended(&w, &agent.id).await;
    }

    // A preset applied (the user's own proposal over the Flow setup page) settles it the same way.
    let api = add_in_flow_setup(&w, "pocket-api").await;
    let opencode = ask(&w, &api, Tool::OpenCode).await.unwrap();
    let theirs = propose(&w, &api, &opencode, TEAM_PR_FILE).await;
    let reply = send(
        &w,
        Request::FlowProposeFrom(ProposeFromParams {
            project: api.clone(),
            source: ProposeFrom::Preset {
                file: SPEC_FIRST_FILE.into(),
            },
        }),
    )
    .await
    .unwrap();
    let Reply::FlowProposal(Some(users)) = reply else {
        panic!("{reply:?}")
    };
    let ids: Vec<u32> = open_proposals(&w, &api)
        .await
        .iter()
        .map(|p| p.id)
        .collect();
    assert_eq!(ids, [theirs.id, users.id], "the user's beside the agent's");
    send(
        &w,
        Request::FlowApply(ApplyParams {
            project: api.clone(),
            proposal: users.id,
            choices: ApplyChoices::default(),
        }),
    )
    .await
    .unwrap();
    assert!(open_proposals(&w, &api).await.is_empty());
    assert!(!in_flow_setup(&w, &api).await);
    assert_eq!(
        w.board.effective_flow(&api).await.unwrap(),
        preset(SPEC_FIRST_FILE).unwrap().read().flow
    );
    w.wait_for_output(&opencode.session.as_ref().unwrap().id, CHOSE_ANOTHER_FLOW)
        .await;
    ended(&w, &opencode.id).await;
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn outside_flow_setup_a_new_proposal_still_replaces_the_old() {
    let World { w, _tools } = world().await;
    let shop = w.add_project("shop-web").await.key;
    let a = w
        .project_agents
        .open(&shop, "/flow add a Product spec stage")
        .await
        .unwrap();
    let mut b = w
        .project_agents
        .open(&shop, "/flow try the team flow")
        .await
        .unwrap();
    // Even from another tool.
    b.tool = Tool::OpenCode;
    let first = propose(&w, &shop, &a, SPEC_FIRST_FILE).await;
    let second = propose(&w, &shop, &b, TEAM_PR_FILE).await;
    // Both land in the project's one draft (FED-02): the second replaces the first's content, the agent waiting for
    // the answer is the second's.
    assert_eq!(first.id, second.id);
    assert!(
        matches!(&second.proposed_by, ProposedBy::ProjectAgent { agent, .. } if *agent == b.id)
    );
    let ids: Vec<u32> = open_proposals(&w, &shop)
        .await
        .iter()
        .map(|p| p.id)
        .collect();
    assert_eq!(ids, [second.id]);
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn flow_setup_is_kept_across_a_restart_and_refuses_tickets() {
    let World { w, _tools } = world().await;
    let shop = add_in_flow_setup(&w, "shop-web").await;
    let reply = send(&w, Request::ProjectList).await.unwrap();
    let Reply::Projects(projects) = reply else {
        panic!("{reply:?}")
    };
    let since = projects[0].flow_setup_since.expect("the list carries it");

    let input = NewTicketInput {
        project: shop.clone(),
        title: "Rate limit".into(),
        ..Default::default()
    };
    let e = w.board.create_ticket(&input, &me()).await.unwrap_err();
    assert_eq!(e.to_string(), flow_not_set_up("shop-web"));
    // An import goes through the same door.
    let e = w
        .board
        .create_batch(
            &shop,
            &parse_markdown("### Rate limit\n\n### Retry\n").unwrap(),
            &me(),
        )
        .await
        .unwrap_err();
    assert_eq!(e.to_string(), flow_not_set_up("shop-web"));

    let tools = _tools;
    let bin = tools.path().join("bin");
    let w = w.restart_with(config(&bin)).await;
    connect(&w).await;
    assert_eq!(
        w.board.project(&shop).await.unwrap().flow_setup_since,
        Some(since)
    );
    let s = setup(&w, &shop).await;
    assert_eq!(s.since, Some(since));
    assert!(w.board.create_ticket(&input, &me()).await.is_err());

    send(
        &w,
        Request::OnboardingUseDefault(ProjectParams {
            project: shop.clone(),
        }),
    )
    .await
    .unwrap();
    let w = w.restart_with(config(&bin)).await;
    assert!(!in_flow_setup(&w, &shop).await);
    w.board.create_ticket(&input, &me()).await.unwrap();
    w.stop().await;
}

/// A hook of the flow agent `agent`, as the service handles it: the signals map it, the Project agents apply it.
async fn hook(w: &TestService, agent: &ProjectAgent, event: &str, payload: serde_json::Value) {
    let caller = otaq_core::caller::Caller::ProjectAgent {
        project: agent.project.clone(),
        agent: agent.id.clone(),
        session: agent.session.as_ref().expect("a session").id.clone(),
    };
    let signals = w
        .signals
        .hook(&caller, agent.tool, event, &payload)
        .await
        .unwrap();
    for signal in &signals {
        w.project_agents.hook(&agent.id, signal).await.unwrap();
    }
}

/// ASK-03 (the fix): an unchosen flow agent reads "The user chose another flow." as its answer, then ends when that
/// turn ends; Flow setup is over at once. `claude` here only reads its input and records it.
#[tokio::test(flavor = "multi_thread")]
async fn an_unchosen_agent_reads_its_answer_and_ends_with_its_turn() {
    let World { w, _tools } = world().await;
    w.project_agents.set_answer_grace(Duration::from_secs(60));
    let shop = add_in_flow_setup(&w, "shop-web").await;
    let claude = ask(&w, &shop, Tool::Claude).await.unwrap();
    printed(&w, &claude, "ARGS ").await;
    // Its first turn ended: it waits for the user.
    hook(
        &w,
        &claude,
        "UserPromptSubmit",
        serde_json::json!({ "prompt": "/flow" }),
    )
    .await;
    hook(
        &w,
        &claude,
        "Stop",
        serde_json::json!({ "last_assistant_message": "Proposed." }),
    )
    .await;

    send(
        &w,
        Request::OnboardingUseDefault(ProjectParams {
            project: shop.clone(),
        }),
    )
    .await
    .unwrap();
    // Flow setup is over at once: the board takes tickets.
    assert!(!in_flow_setup(&w, &shop).await);
    w.create(&shop, "Rate limit").await;
    // The agent read its answer and still runs: its turn on it hasn't ended.
    let heard = heard(&_tools.path().join("bin"));
    tokio::time::timeout(Duration::from_secs(20), async {
        while !std::fs::read_to_string(&heard).is_ok_and(|h| h.contains(CHOSE_ANOTHER_FLOW)) {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the stand-in read the answer");
    let reading = w.project_agents.get(&claude.id).await.unwrap();
    assert_eq!(
        (reading.state, reading.closed),
        (AgentState::Running, false)
    );
    // Its turn ends: it ends to Recently ended, long before the wait would have run out.
    hook(
        &w,
        &claude,
        "UserPromptSubmit",
        serde_json::json!({ "prompt": CHOSE_ANOTHER_FLOW }),
    )
    .await;
    hook(
        &w,
        &claude,
        "Stop",
        serde_json::json!({ "last_assistant_message": "Done." }),
    )
    .await;
    let started = std::time::Instant::now();
    ended(&w, &claude.id).await;
    assert!(started.elapsed() < Duration::from_secs(10));
    w.stop().await;
}

/// An unchosen flow agent that never reports its turn's end ends once the wait runs out; one that isn't running
/// ends at once.
#[tokio::test(flavor = "multi_thread")]
async fn an_unchosen_agent_that_never_ends_its_turn_ends_after_the_wait() {
    let World { w, _tools } = world().await;
    w.project_agents.set_answer_grace(Duration::from_secs(2));
    let shop = add_in_flow_setup(&w, "shop-web").await;
    let opencode = ask(&w, &shop, Tool::OpenCode).await.unwrap();
    printed(&w, &opencode, "CONFIG ").await;
    let claude = ask(&w, &shop, Tool::Claude).await.unwrap();
    w.project_agents.close(&claude.id, true).await.unwrap();
    // The ended one is reopened in the tabs, as after a restart, to show it ends at once.
    sqlx::query("UPDATE project_agents SET closed_at = NULL WHERE id = ?")
        .bind(&claude.id)
        .execute(w.board.store().pool())
        .await
        .unwrap();

    let started = std::time::Instant::now();
    send(
        &w,
        Request::OnboardingUseDefault(ProjectParams {
            project: shop.clone(),
        }),
    )
    .await
    .unwrap();
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "the click returns at once"
    );
    assert!(
        w.project_agents.get(&claude.id).await.unwrap().closed,
        "not running: at once"
    );
    w.wait_for_output(&opencode.session.as_ref().unwrap().id, CHOSE_ANOTHER_FLOW)
        .await;
    assert!(
        !w.project_agents.get(&opencode.id).await.unwrap().closed,
        "it has its wait"
    );
    ended(&w, &opencode.id).await;
    assert!(
        started.elapsed() >= Duration::from_secs(2),
        "{:?}",
        started.elapsed()
    );
    w.stop().await;
}
