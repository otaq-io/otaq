//! Flow onboarding end to end (T-0432; ONB-11; d0108, d0056, d0079; asking per tool since T-0433, ASK-02). Asking
//! Claude Code opens its flow agent for a project in Flow setup; a stand-in `claude` proposes a flow file with the
//! real `otaq flow propose`, gets the user's Ask for more changes typed in as its next message, proposes again, and
//! the user's Apply makes the second file the project's flow and ends Flow setup. Then no tool connected: asking is
//! refused and a preset applies instead. Otaq reads no tool's history (ASK-01); the Claude Code folder is an empty
//! temporary one, never the user's own; no real agent tool or `gh` runs.

use std::path::{Path, PathBuf};

use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::agent_tools::connected::AgentToolState;
use otaq_core::agent_tools::{LaunchContext, Tool};
use otaq_core::agents::FixedModels;
use otaq_core::board::projects::AddProject;
use otaq_core::client::{ClientError, request_once};
use otaq_core::data_dir::DataDir;
use otaq_core::flow::apply::{ApplyParams, ProposeFrom, ProposeFromParams};
use otaq_core::flow::presets::preset;
use otaq_core::flow::proposal::{ApplyChoices, ProposedBy, Source};
use otaq_core::flow::propose::AskForChangesParams;
use otaq_core::git::testing::TempRepos;
use otaq_core::onboarding::Sources;
use otaq_core::project_agents::ProjectAgent;
use otaq_core::protocol::{ErrorBody, OnboardingAskParams, ProjectParams, Reply, Request};
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::TestService;
use otaq_core::tools::CheckEnv;

const CB: &str = env!("CARGO_BIN_EXE_otaq");

/// The stand-in's first proposal: Grill first, and a Security check before the user's review.
const FIRST: &str = "\
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
    why: Most of your sessions start with /grill
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
    why: You asked for a security review before 4 of your last merges
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

/// After "Leave out Grill": the same without it.
const SECOND: &str = "\
agents:
  - name: Developer
    skills: [work-ticket, commit]
    model: default
  - name: Validator
    skills: [validate]
    model: default
flow:
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
    why: You asked for a security review before 4 of your last merges
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

/// The stand-in `claude`: started with the first-flow message, it proposes [`FIRST`]; each
/// "asks for more changes" typed into it then proposes [`SECOND`]. What it ran lands in `dir`, and a `STAND-IN …`
/// line says each step is done. Started any other way it is an interactive `sh`.
fn stand_in(dir: &Path, data: &Path) -> PathBuf {
    std::fs::write(dir.join("first.yaml"), FIRST).unwrap();
    std::fs::write(dir.join("second.yaml"), SECOND).unwrap();
    let d = dir.display();
    let script = format!(
        "export {home}='{data}'; export HOME='{d}/home'; export PS1=''\n\
         case \"$*\" in *\"first flow\"*) ;; *) exec sh ;; esac\n\
         '{CB}' flow propose --stdin --silent < '{d}/first.yaml' > '{d}/first.id' 2>&1\n\
         echo 'STAND-IN proposed the first flow'\n\
         while IFS= read -r line; do\n\
           case \"$line\" in *'asks for more changes'*)\n\
             '{CB}' flow propose --stdin --silent < '{d}/second.yaml' > '{d}/second.id' 2>&1\n\
             echo 'STAND-IN proposed the second flow' ;;\n\
           esac\n\
         done\n",
        home = DataDir::env_var(),
        data = data.display(),
    );
    let path = dir.join("claude-stand-in.sh");
    std::fs::write(&path, script).unwrap();
    path
}

/// The Claude Code folder (empty), the stand-in's files and an empty PATH folder (no `gh`).
struct Mac {
    _tmp: tempfile::TempDir,
    claude: PathBuf,
    bin: PathBuf,
    files: PathBuf,
}

fn mac() -> Mac {
    let tmp = tempfile::tempdir().unwrap();
    let claude = tmp.path().join("claude");
    let bin = tmp.path().join("bin");
    let files = tmp.path().join("stand-in");
    for dir in [&claude, &bin, &files, &files.join("home")] {
        std::fs::create_dir_all(dir).unwrap();
    }
    Mac {
        _tmp: tmp,
        claude,
        bin,
        files,
    }
}

async fn world(mac: &Mac) -> TestService {
    let home = tempfile::TempDir::new().unwrap();
    let program = stand_in(&mac.files, &home.path().join("h"));
    let env = CheckEnv {
        path: mac.bin.as_os_str().to_owned(),
        claude_config_dir: Some(mac.claude.clone()),
        managed_settings_dir: mac.claude.join("managed"),
    };
    TestService::start_in(home, TempRepos::new(), move |config| ServiceConfig {
        tools: env,
        launch: Some(LaunchContext {
            bin_dir: "/nonexistent".into(),
            plugin_dir: "/nonexistent/plugin".into(),
            path: std::env::var_os("PATH").unwrap_or_default(),
            claude: ClaudeLaunch {
                program: vec!["sh".into(), program.display().to_string()],
                config_dir: None,
            },
        }),
        ..config
    })
    .await
}

/// `request` as the app sends it.
async fn ask(w: &TestService, request: Request) -> Result<Reply, ErrorBody> {
    match request_once(&w.data.socket_path(), request).await {
        Ok(reply) => Ok(reply),
        Err(ClientError::Service(body)) => Err(body),
        Err(e) => panic!("{e}"),
    }
}

async fn sources(w: &TestService, project: &str) -> Sources {
    let request = Request::OnboardingSources(ProjectParams {
        project: project.into(),
    });
    match ask(w, request).await.unwrap() {
        Reply::OnboardingSources(s) => s,
        other => panic!("{other:?}"),
    }
}

async fn start_flow(w: &TestService, project: &str) -> Result<ProjectAgent, ErrorBody> {
    let request = Request::OnboardingAsk(OnboardingAskParams {
        project: project.into(),
        tool: Tool::Claude,
    });
    ask(w, request).await.map(|reply| match reply {
        Reply::ProjectAgent(agent) => *agent,
        other => panic!("{other:?}"),
    })
}

/// Adds the repository `name` (with `commits`) as the user's Add project does: in Flow setup.
async fn add_in_flow_setup(w: &TestService, name: &str, key: &str, commits: usize) -> String {
    let repo = w.repos.repo(name, commits);
    w.board
        .add_project(&AddProject {
            path: repo.display().to_string(),
            key: Some(key.into()),
            base_branch: Some("main".into()),
            flow_setup: true,
        })
        .await
        .unwrap()
        .key
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

fn read(path: PathBuf) -> String {
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

#[tokio::test(flavor = "multi_thread")]
async fn the_flow_agent_proposes_and_the_second_proposal_applies() {
    let mac = mac();
    let w = world(&mac).await;
    // Claude Code connected by hand, once the service's own check at start recorded what it found (nothing).
    tokio::time::timeout(std::time::Duration::from_secs(20), async {
        while w.connected.status(Tool::Claude).checked_at.is_none() {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the check at start");
    let connected = AgentToolState::Connected {
        version: None,
        account: None,
    };
    w.connected.set_detected(Tool::Claude, connected, 0);
    let shop = add_in_flow_setup(&w, "shop-web", "SHOP", 1).await;
    let s = sources(&w, &shop).await;
    assert_eq!(s.tools, [Tool::Claude], "{s:?}");

    let agent = start_flow(&w, &shop).await.unwrap();
    let session = agent.session.as_ref().expect("a session").id.clone();
    w.wait_for_output(&session, "STAND-IN proposed the first flow")
        .await;

    // The first proposal: from the agent's document, its why lines, nothing to fix.
    let first: u32 = read(mac.files.join("first.id")).trim().parse().unwrap();
    let open = w
        .board
        .proposal(&shop, &FixedModels::sample())
        .await
        .unwrap()
        .expect("the first proposal");
    assert_eq!(open.id, first);
    assert_eq!(open.source, Source::Document { file: None });
    assert!(
        matches!(&open.proposed_by, ProposedBy::ProjectAgent { session: s, .. } if *s == session),
        "{:?}",
        open.proposed_by
    );
    assert!(open.problems.is_empty(), "{:?}", open.problems);
    let why: Vec<&str> = open.why.iter().map(|l| l.text.as_str()).collect();
    assert_eq!(
        why,
        [
            "Most of your sessions start with /grill",
            "You asked for a security review before 4 of your last merges"
        ]
    );
    // Nothing applies until the user says so.
    assert_eq!(
        stage_names(&w, &shop).await,
        ["Dev", "Validate", "Review", "Done"]
    );

    // Ask for more changes reaches the agent as its next message; it proposes again.
    let changes = "Leave out Grill: I write my own tickets";
    let reply = ask(
        &w,
        Request::FlowAskForChanges(AskForChangesParams {
            project: shop.clone(),
            proposal: first,
            text: changes.into(),
        }),
    )
    .await
    .unwrap();
    assert!(matches!(reply, Reply::Ok), "{reply:?}");
    w.deliveries.deliver_now(&session).await.unwrap();
    let out = w
        .wait_for_output(&session, "STAND-IN proposed the second flow")
        .await;
    assert!(
        out.contains(&format!(
            "The user asks for more changes to Proposal {first}: {changes}"
        )),
        "{out}"
    );
    let second: u32 = read(mac.files.join("second.id")).trim().parse().unwrap();
    assert_ne!(second, first);

    // Apply: the board is quiet, so the second file is the project's flow at once, and the agent hears so.
    let reply = ask(
        &w,
        Request::FlowApply(ApplyParams {
            project: shop.clone(),
            proposal: second,
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
        ["Dev", "Security", "Review", "Done"]
    );
    let file = otaq_core::flow::yaml::read(SECOND).unwrap();
    let applied_flow = w.board.effective_flow(&shop).await.unwrap();
    let security = applied_flow
        .stages
        .iter()
        .find(|s| s.id == "security")
        .unwrap();
    assert_eq!(
        Some(security),
        file.flow.stages.iter().find(|s| s.id == "security")
    );
    w.deliveries.deliver_now(&session).await.unwrap();
    w.wait_for_output(&session, &format!("The user applied Proposal {second}."))
        .await;
    assert!(
        w.board
            .proposal(&shop, &FixedModels::sample())
            .await
            .unwrap()
            .is_none()
    );
    assert!(w.deliveries.pending(&session).await.unwrap().is_empty());
    assert!(!in_flow_setup(&w, &shop).await, "the flow is settled");
    w.stop().await;
}

/// No tool connected: asking is refused, no agent opens, and a preset applies instead, ending Flow setup.
#[tokio::test(flavor = "multi_thread")]
async fn with_no_tool_connected_a_preset_applies_instead() {
    let mac = mac();
    let w = world(&mac).await;
    add_in_flow_setup(&w, "empty-api", "API", 0).await;

    let s = sources(&w, "API").await;
    assert!(!s.any(), "{s:?}");
    let e = start_flow(&w, "API").await.unwrap_err();
    assert!(e.message.contains("available"), "{e:?}");
    let agents = w.project_agents.list("API").await.unwrap();
    assert!(
        agents.open.is_empty() && agents.recent.is_empty(),
        "{agents:?}"
    );
    assert!(!mac.files.join("first.id").exists());

    let file = "grill-first.yaml";
    let reply = ask(
        &w,
        Request::FlowProposeFrom(ProposeFromParams {
            project: "API".into(),
            source: ProposeFrom::Preset { file: file.into() },
        }),
    )
    .await
    .unwrap();
    let Reply::FlowProposal(Some(proposal)) = reply else {
        panic!("{reply:?}")
    };
    assert_eq!(proposal.source, Source::Preset { file: file.into() });
    assert!(proposal.problems.is_empty(), "{:?}", proposal.problems);
    let reply = ask(
        &w,
        Request::FlowApply(ApplyParams {
            project: "API".into(),
            proposal: proposal.id,
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
        w.board.effective_flow("API").await.unwrap(),
        preset(file).unwrap().read().flow
    );
    assert!(!in_flow_setup(&w, "API").await, "the flow is settled");
    w.stop().await;
}
