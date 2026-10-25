//! Asking each tool for a flow, end to end (T-0441; ASK-10; d0108, d0056, d0079). A project added in Flow setup
//! refuses tickets; **Ask Claude Code** and **Ask OpenCode** start a flow agent on each; each stand-in checks its first
//! message is ASK-02's text for its own tool and proposes a flow with the real `otaq flow propose`; the Flow setup
//! shows both proposals; **Use this flow** on Claude Code's makes it the project's flow, OpenCode's agent gets "The user
//! chose another flow." and ends, Flow setup is cleared and `otaq create` works. Then no tool connected: asking either
//! tool is refused and **Use the default** settles the project on the default flow.
//!
//! Every tool is a stand-in (`sh` scripts), connected by hand; `OTAQ_HOME`, `HOME`, the Claude Code folder and the
//! repositories are temporary. No real agent tool, history or `gh` is touched.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::Duration;

use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::agent_tools::connected::AgentToolState;
use otaq_core::agent_tools::{LaunchContext, Tool};
use otaq_core::agents::FixedModels;
use otaq_core::board::projects::{AddProject, flow_not_set_up};
use otaq_core::client::{ClientError, request_once};
use otaq_core::data_dir::DataDir;
use otaq_core::flow::apply::ApplyParams;
use otaq_core::flow::proposal::ApplyChoices;
use otaq_core::git::testing::TempRepos;
use otaq_core::naming;
use otaq_core::project_agents::first_flow::{
    CHOSE_ANOTHER_FLOW, FlowSetup, FlowSetupRow, first_flow_message,
};
use otaq_core::project_agents::{AgentState, FLOW_COMMAND, ProjectAgent};
use otaq_core::protocol::{ErrorBody, OnboardingAskParams, ProjectParams, Reply, Request};
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::TestService;
use otaq_core::store::now_ms;
use otaq_core::tools::CheckEnv;
use serde_json::Value;

const CB: &str = env!("CARGO_BIN_EXE_otaq");

/// Claude Code's proposal: Grill first, and a Security check before the user's review.
const CLAUDE_FLOW: &str = "\
agents:
  - name: Griller
    skills: [grill]
    model: default
  - name: Developer
    skills: [work-ticket, commit]
    model: default
  - name: Validator
    skills: [validate]
    model: default
flow:
  - id: grill
    name: Grill
    kind: agent
    column: own
    agent: Griller
    skill: grill
    why: Most of your Claude Code sessions start with /grill
  - id: dev
    name: Dev
    kind: agent
    column: own
    agent: Developer
    skill: work-ticket
  - id: security
    name: Security
    kind: check
    by: agent
    column: own
    agents: [Validator]
    skill: validate
    rounds: 2
    why: CONTRIBUTING.md asks for a security review
  - id: review
    name: Review
    kind: check
    by: you
    column: own
  - id: deliver
    name: Done
    kind: delivery
    column: with-previous
    agent: Developer
    skill: open-pr
    mode: click
    delivers: either
";

/// OpenCode's proposal: Dev, the user's review, Done.
const OPENCODE_FLOW: &str = "\
agents:
  - name: Developer
    skills: [work-ticket, commit]
    model: default
flow:
  - id: dev
    name: Dev
    kind: agent
    column: own
    agent: Developer
    skill: work-ticket
    why: In OpenCode you review every change yourself
  - id: review
    name: Review
    kind: check
    by: you
    column: own
  - id: deliver
    name: Done
    kind: delivery
    column: with-previous
    agent: Developer
    skill: open-pr
    mode: click
    delivers: either
";

/// A stand-in's steps once it is started (its arguments in `$*`): checks the first message holds ASK-02's text for
/// its tool (`<name>.expected`), proposes `<name>.yaml` with the real `otaq flow propose` (the id lands in
/// `<name>.id`), then records each line typed into it in `<name>.read`; told "The user chose another flow." it ends, as
/// a real agent's session does once it has read that. A `STAND-IN …` line says each step is done.
fn steps(files: &Path, data: &Path, name: &str) -> String {
    let f = files.display();
    format!(
        "export {home}='{data}'; export HOME='{f}/home'\n\
         expected=$(cat '{f}/{name}.expected')\n\
         case \"$*\" in *\"$expected\"*) echo 'STAND-IN first message ok' ;; \
           *) echo 'STAND-IN wrong first message'; exec cat ;; esac\n\
         '{CB}' flow propose --stdin --silent < '{f}/{name}.yaml' > '{f}/{name}.id' 2>&1\n\
         echo 'STAND-IN proposed a flow'\n\
         while IFS= read -r line; do\n\
           printf '%s\\n' \"$line\" >> '{f}/{name}.read'\n\
           case \"$line\" in *'{CHOSE_ANOTHER_FLOW}'*) echo 'STAND-IN ends'; exit 0 ;; esac\n\
         done\n",
        home = DataDir::env_var(),
        data = data.display(),
    )
}

/// The temporary Mac: the Claude Code folder (empty), an empty tools PATH (no tool is found by a check), Otaq's bin
/// folder holding the `opencode` stand-in, and the stand-ins' files.
struct Mac {
    _tmp: tempfile::TempDir,
    claude: PathBuf,
    path: PathBuf,
    bin: PathBuf,
    files: PathBuf,
}

fn mac() -> Mac {
    let tmp = tempfile::tempdir().unwrap();
    let claude = tmp.path().join("claude");
    let path = tmp.path().join("path");
    let bin = tmp.path().join("bin");
    let files = tmp.path().join("stand-in");
    for dir in [&claude, &path, &bin, &files, &files.join("home")] {
        std::fs::create_dir_all(dir).unwrap();
    }
    Mac {
        _tmp: tmp,
        claude,
        path,
        bin,
        files,
    }
}

/// The service with both stand-ins: `claude` is `sh claude.sh`; `opencode` is the script in Otaq's bin folder, which
/// answers the conversation OpenCode's launch makes first (`opencode api …`).
async fn world(mac: &Mac) -> TestService {
    let home = tempfile::TempDir::new().unwrap();
    let data = home.path().join("h");
    for (name, yaml) in [("claude", CLAUDE_FLOW), ("opencode", OPENCODE_FLOW)] {
        std::fs::write(mac.files.join(format!("{name}.yaml")), yaml).unwrap();
    }
    let claude = mac.files.join("claude.sh");
    std::fs::write(&claude, steps(&mac.files, &data, "claude")).unwrap();
    let opencode = mac.bin.join(Tool::OpenCode.command_name());
    std::fs::write(
        &opencode,
        format!(
            "#!/bin/sh\ncase \"$1\" in api) exit 0 ;; esac\n{}",
            steps(&mac.files, &data, "opencode")
        ),
    )
    .unwrap();
    std::fs::set_permissions(&opencode, std::fs::Permissions::from_mode(0o755)).unwrap();
    let env = CheckEnv {
        path: mac.path.as_os_str().to_owned(),
        claude_config_dir: Some(mac.claude.clone()),
        managed_settings_dir: mac.claude.join("managed"),
    };
    let bin = mac.bin.clone();
    TestService::start_in(home, TempRepos::new(), move |config| ServiceConfig {
        tools: env,
        launch: Some(LaunchContext {
            bin_dir: bin,
            plugin_dir: "/nonexistent/plugin".into(),
            path: std::env::var_os("PATH").unwrap_or_default(),
            claude: ClaudeLaunch {
                program: vec!["sh".into(), claude.display().to_string()],
                config_dir: None,
            },
        }),
        ..config
    })
    .await
}

/// Waits for the service's own check at start (it finds nothing on the empty PATH), then marks `tools` connected.
async fn connect(w: &TestService, tools: &[Tool]) {
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
    for &tool in tools {
        let connected = AgentToolState::Connected {
            version: None,
            account: None,
        };
        w.connected.set_detected(tool, connected, now_ms());
    }
}

/// `request` as the app sends it.
async fn send(w: &TestService, request: Request) -> Result<Reply, ErrorBody> {
    match request_once(&w.data.socket_path(), request).await {
        Ok(reply) => Ok(reply),
        Err(ClientError::Service(body)) => Err(body),
        Err(e) => panic!("{e}"),
    }
}

/// Adds the repository `name` as the app's Add project does: in Flow setup.
async fn add_in_flow_setup(w: &TestService, name: &str, key: &str) -> String {
    let repo = w.repos.repo(name, 1);
    let reply = send(
        w,
        Request::ProjectAdd(AddProject {
            path: repo.display().to_string(),
            key: Some(key.into()),
            base_branch: Some("main".into()),
            flow_setup: true,
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

async fn flow_setup(w: &TestService, project: &str) -> FlowSetup {
    let request = Request::OnboardingFlowSetup(ProjectParams {
        project: project.into(),
    });
    match send(w, request).await.unwrap() {
        Reply::FlowSetup(setup) => *setup,
        other => panic!("{other:?}"),
    }
}

async fn in_flow_setup(w: &TestService, project: &str) -> bool {
    w.board
        .project(project)
        .await
        .unwrap()
        .flow_setup_since
        .is_some()
}

async fn stage_names(w: &TestService, project: &str) -> Vec<String> {
    let flow = w.board.effective_flow(project).await.unwrap();
    flow.stages.into_iter().map(|s| s.name).collect()
}

/// `otaq -P <project> create <title> --json`, as the user types it in a terminal.
fn create(w: &TestService, project: &str, title: &str) -> Output {
    Command::new(CB)
        .args(["-P", project, "create", title, "--json"])
        .env(DataDir::env_var(), w.data.root())
        .env_remove(naming::ticket_env_var())
        .current_dir(w.home.path())
        .output()
        .unwrap()
}

fn refused(out: &Output) -> String {
    assert!(
        !out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    let body: Value = serde_json::from_slice(&out.stdout).unwrap();
    body["error"]["message"]
        .as_str()
        .unwrap_or_default()
        .to_owned()
}

fn created(out: &Output) -> String {
    assert!(
        out.status.success(),
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let ticket: Value = serde_json::from_slice(&out.stdout).unwrap();
    ticket["key"].as_str().unwrap().to_owned()
}

/// The proposal id a stand-in's `otaq flow propose --silent` printed.
fn proposal_id(mac: &Mac, name: &str) -> u32 {
    let path = mac.files.join(format!("{name}.id"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    text.trim()
        .parse()
        .unwrap_or_else(|_| panic!("{name}: {text}"))
}

/// Waits until the agent `id` has ended and left the switcher.
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
async fn both_tools_propose_and_use_this_flow_settles_the_project() {
    let mac = mac();
    let w = world(&mac).await;
    connect(&w, &[Tool::Claude, Tool::OpenCode]).await;
    let shop = add_in_flow_setup(&w, "shop-web", "SHOP").await;
    for (name, tool) in [("claude", Tool::Claude), ("opencode", Tool::OpenCode)] {
        // The text after the skill (each tool names a skill its own way).
        let message = first_flow_message("shop-web", tool);
        let asks = message.strip_prefix(FLOW_COMMAND).unwrap().trim_start();
        std::fs::write(mac.files.join(format!("{name}.expected")), asks).unwrap();
    }

    // No board yet: the CLI can't create a ticket.
    let e = refused(&create(&w, &shop, "Rate limit"));
    assert!(e.contains(&flow_not_set_up("shop-web")), "{e}");

    // Ask both tools: each stand-in finds its own tool's first message and proposes through the CLI.
    let claude = ask(&w, &shop, Tool::Claude).await.unwrap();
    let opencode = ask(&w, &shop, Tool::OpenCode).await.unwrap();
    assert_eq!((claude.tool, opencode.tool), (Tool::Claude, Tool::OpenCode));
    let session = |a: &ProjectAgent| a.session.as_ref().expect("a session").id.clone();
    let (claude_session, opencode_session) = (session(&claude), session(&opencode));
    for id in [&claude_session, &opencode_session] {
        let out = w.wait_for_output(id, "STAND-IN proposed a flow").await;
        assert!(out.contains("STAND-IN first message ok"), "{out}");
    }
    let (theirs_claude, theirs_opencode) =
        (proposal_id(&mac, "claude"), proposal_id(&mac, "opencode"));
    assert_ne!(theirs_claude, theirs_opencode);

    // Two open proposals, each labelled with its tool and carrying its own why lines; both rows proposed.
    let setup = flow_setup(&w, &shop).await;
    let by: Vec<(u32, Option<Tool>)> = setup
        .proposals
        .iter()
        .map(|p| (p.id, p.proposed_by.tool()))
        .collect();
    assert_eq!(
        by,
        [
            (theirs_claude, Some(Tool::Claude)),
            (theirs_opencode, Some(Tool::OpenCode))
        ]
    );
    let why = |id: u32| -> Vec<String> {
        let p = setup.proposals.iter().find(|p| p.id == id).unwrap();
        assert!(p.problems.is_empty(), "{:?}", p.problems);
        p.why.iter().map(|l| l.text.clone()).collect()
    };
    assert_eq!(
        why(theirs_claude),
        [
            "Most of your Claude Code sessions start with /grill",
            "CONTRIBUTING.md asks for a security review"
        ]
    );
    assert_eq!(
        why(theirs_opencode),
        ["In OpenCode you review every change yourself"]
    );
    let row = |tool: Tool| {
        setup
            .tools
            .iter()
            .find(|t| t.tool == tool)
            .map(|t| t.row.clone())
    };
    assert_eq!(
        row(Tool::Claude),
        Some(FlowSetupRow::Proposed {
            agent: claude.id.clone(),
            proposal: theirs_claude
        })
    );
    assert_eq!(
        row(Tool::OpenCode),
        Some(FlowSetupRow::Proposed {
            agent: opencode.id.clone(),
            proposal: theirs_opencode
        })
    );
    // Nothing applies until the user picks; still no board.
    assert!(in_flow_setup(&w, &shop).await);
    refused(&create(&w, &shop, "Rate limit"));

    // Use this flow on Claude Code's.
    let reply = send(
        &w,
        Request::FlowApply(ApplyParams {
            project: shop.clone(),
            proposal: theirs_claude,
            choices: ApplyChoices::default(),
        }),
    )
    .await
    .unwrap();
    let Reply::FlowApplied(applied) = reply else {
        panic!("{reply:?}")
    };
    assert!(applied.waiting.is_none(), "a quiet board applies at once");
    assert_eq!(
        stage_names(&w, &shop).await,
        ["Grill", "Dev", "Security", "Review", "Done"]
    );
    assert!(!in_flow_setup(&w, &shop).await, "Flow setup is cleared");
    assert!(
        w.board
            .proposals(&shop, &FixedModels::sample())
            .await
            .unwrap()
            .is_empty(),
        "OpenCode's proposal is closed"
    );

    // OpenCode's agent reads the answer and ends (its session ends, well before the 30 s wait runs out); Claude
    // Code's stays, its answer waiting to be typed.
    ended(&w, &opencode.id).await;
    assert_eq!(
        std::fs::read_to_string(mac.files.join("opencode.read")).unwrap(),
        format!("{CHOSE_ANOTHER_FLOW}\n"),
        "the stand-in read the answer"
    );
    assert!(
        !mac.files.join("claude.read").exists(),
        "nothing typed into Claude Code's yet"
    );
    assert_eq!(
        w.project_agents.get(&claude.id).await.unwrap().state,
        AgentState::Running
    );
    let pending = w.deliveries.pending(&claude_session).await.unwrap();
    assert_eq!(
        pending.iter().map(|d| d.text.as_str()).collect::<Vec<_>>(),
        [format!("The user applied Proposal {theirs_claude}.")]
    );

    // The board takes tickets now.
    assert_eq!(created(&create(&w, &shop, "Rate limit")), "SHOP-1");
    w.stop().await;
}

/// No tool connected: neither tool can be asked, nothing starts, and Use the default puts the project on the default
/// flow.
#[tokio::test(flavor = "multi_thread")]
async fn with_no_tool_connected_use_the_default_settles_the_project() {
    let mac = mac();
    let w = world(&mac).await;
    connect(&w, &[]).await;
    let api = add_in_flow_setup(&w, "pocket-api", "API").await;

    let reply = send(
        &w,
        Request::OnboardingSources(ProjectParams {
            project: api.clone(),
        }),
    )
    .await
    .unwrap();
    let Reply::OnboardingSources(sources) = reply else {
        panic!("{reply:?}")
    };
    assert!(sources.tools.is_empty(), "{sources:?}");
    assert!(flow_setup(&w, &api).await.tools.is_empty());
    for tool in [Tool::Claude, Tool::OpenCode] {
        let e = ask(&w, &api, tool).await.unwrap_err();
        assert!(
            e.message
                .starts_with(&format!("{} isn\u{2019}t available", tool.display_name())),
            "{e:?}"
        );
    }
    let agents = w.project_agents.list(&api).await.unwrap();
    assert!(
        agents.open.is_empty() && agents.recent.is_empty(),
        "{agents:?}"
    );
    assert!(!mac.files.join("claude.id").exists());
    assert!(!mac.files.join("opencode.id").exists());
    let e = refused(&create(&w, &api, "Rate limit"));
    assert!(e.contains(&flow_not_set_up("pocket-api")), "{e}");

    let reply = send(
        &w,
        Request::OnboardingUseDefault(ProjectParams {
            project: api.clone(),
        }),
    )
    .await
    .unwrap();
    assert!(matches!(reply, Reply::Ok), "{reply:?}");
    assert!(!in_flow_setup(&w, &api).await, "Flow setup is cleared");
    assert_eq!(
        stage_names(&w, &api).await,
        ["Dev", "Validate", "Review", "Done"]
    );
    assert_eq!(created(&create(&w, &api, "Rate limit")), "API-1");
    w.stop().await;
}
