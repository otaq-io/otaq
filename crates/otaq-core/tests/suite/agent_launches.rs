//! Agents at launch (T-0405; AGM-05, AGM-03, ADP-04, SES-03; d0108, d0088): every new session runs its agent (the
//! Agent stage's, the agent check's, the role agents for the epic lead and the Project agent) and passes the agent's
//! model (`--model opus`, nothing for the tool default; `model: default` resolved to the default model at start)
//! and its effort; every resume passes the session's own stored model and effort again, whatever the agent is set to
//! now; the session stores the agent, the tool, the model as passed and the effort, then the exact model the tool
//! reports. `claude` stands in as `sh` printing its arguments: no test starts claude.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::agent_tools::claude::transcript::project_folder;
use otaq_core::agent_tools::{LaunchContext, Tool};
use otaq_core::agents::{Agent, AgentFields, AgentModel, Agents, FixedModels, ModelRef};
use otaq_core::board::comments::{NewComment, Side};
use otaq_core::board::moves::DropTarget;
use otaq_core::board::tickets::{NewTicketInput, Ticket};
use otaq_core::caller::Caller;
use otaq_core::domain::{Actor, Kind, Status};
use otaq_core::git::testing::commit;
use otaq_core::naming;
use otaq_core::project_agents::AgentState;
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::TestService;
use otaq_core::session::{SessionAgent, SessionInfo, SessionKind};
use serde_json::json;
use tempfile::TempDir;

/// Prints its arguments, then waits (reading what is typed into it).
const SCRIPT: &str = "printf 'ARGS %s\\n' \"$*\"; exec cat";

/// A skill as Claude Code runs it: `/otaq-work-ticket` (AGM-07).
fn claude_skill(name: &str) -> String {
    format!("/{}", naming::copy_name(name))
}

fn me() -> Actor {
    Actor::User("me".into())
}

struct World {
    w: TestService,
    /// The user's Claude folder (the answer path reads a conversation's file there to know it can be resumed).
    claude: TempDir,
    rules: Agents,
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

    /// Saves a new model on an agent (an edit mid-ticket): Claude Code with `model` (`None`: its default).
    async fn set_model(&self, name: &str, model: Option<&str>) {
        self.set(name, claude(model), None).await;
    }

    /// Saves a new model and effort on an agent.
    async fn set(&self, name: &str, model: AgentModel, effort: Option<&str>) {
        let agent = self.agent(name).await;
        self.rules
            .edit(&agent.id, &fields(&agent, model, effort))
            .await
            .unwrap();
    }
}

fn claude(model: Option<&str>) -> AgentModel {
    AgentModel::Tool {
        tool: Tool::ALL[0],
        model: model.map(str::to_owned),
    }
}

/// `agent`'s page with another model and effort.
fn fields(agent: &Agent, model: AgentModel, effort: Option<&str>) -> AgentFields {
    AgentFields {
        name: agent.name.clone(),
        instructions: agent.instructions.clone(),
        skills: agent.skills.clone(),
        model,
        effort: effort.map(str::to_owned),
    }
}

/// The model text an agent passes (its own; `None` for the tool default and for `default`).
fn model_of(agent: &Agent) -> Option<String> {
    match &agent.model {
        AgentModel::Tool { model, .. } => model.clone(),
        AgentModel::Default => None,
    }
}

fn launch_config(claude: &Path) -> impl FnOnce(ServiceConfig) -> ServiceConfig {
    let claude = claude.to_owned();
    move |mut config| {
        config.launch = Some(LaunchContext {
            bin_dir: "/nonexistent".into(),
            plugin_dir: "/nonexistent/plugin".into(),
            path: std::env::var_os("PATH").unwrap_or_default(),
            claude: ClaudeLaunch {
                program: vec!["sh".into(), "-c".into(), SCRIPT.into(), "sh".into()],
                config_dir: None,
            },
        });
        config.tools.claude_config_dir = Some(claude);
        config
    }
}

/// Otaq's agents (Planner, Epic lead, Developer, Validator; all on `default`, with no default model set yet: the
/// new-session tool's own default) and Strong on `opus`; project SHOP (`shop-web`, with an origin for Open PR).
async fn world() -> World {
    let claude_home = TempDir::new().unwrap();
    let w = TestService::start_with(launch_config(claude_home.path())).await;
    let repo = w.repos.clone_with_origin("shop-web", "main", 1);
    w.board
        .add_project(&otaq_core::board::projects::AddProject {
            path: repo.display().to_string(),
            ..Default::default()
        })
        .await
        .unwrap();
    let rules = Agents::new(w.board.clone(), Arc::new(FixedModels::sample()));
    rules
        .add(&AgentFields {
            name: "Strong".into(),
            instructions: String::new(),
            skills: Vec::new(),
            model: claude(Some("opus")),
            effort: None,
        })
        .await
        .unwrap();
    World {
        w,
        claude: claude_home,
        rules,
    }
}

async fn until_status(w: &TestService, key: &str, want: Status) {
    let reached = tokio::time::timeout(Duration::from_secs(20), async {
        while w.status(key).await != want {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await;
    if reached.is_err() {
        let t = w.board.ticket(key).await.unwrap();
        panic!(
            "{key} never became {want}: {} {:?}",
            t.status, t.start_problem
        );
    }
}

/// The command line the session's `claude` got, from its conversation's flag on: `--session-id <id> …` or
/// `--resume <id> …` (what comes before is the same for every launch: the plugin, the one pre-approval and the
/// status line, tested in the adapter).
async fn launched(w: &TestService, session: &str) -> String {
    let out = w.wait_for_output(session, "ARGS").await;
    let line = out
        .lines()
        .find(|l| l.contains("ARGS "))
        .expect("the arguments line")
        .trim_end();
    let at = line
        .find("--session-id ")
        .or_else(|| line.find("--resume "))
        .unwrap_or_else(|| panic!("no conversation flag: {line}"));
    assert_eq!(
        line.matches("--model").count(),
        usize::from(line.contains("--model ")),
        "--model at most once: {line}"
    );
    line[at..].to_owned()
}

/// ` --model <m>` for a model, nothing for the tool default.
fn model_flag(model: Option<&str>) -> String {
    model.map(|m| format!(" --model {m}")).unwrap_or_default()
}

fn named(agent: &Agent) -> Option<SessionAgent> {
    Some(SessionAgent {
        id: agent.id.clone(),
        name: agent.name.clone(),
    })
}

async fn create(w: &TestService, title: &str) -> Ticket {
    w.create("SHOP", title).await
}

/// Ready for Dev → Dev: the start sequence runs and the agent works.
async fn start(w: &TestService, t: &Ticket) -> SessionInfo {
    let key = t.key.to_string();
    w.board
        .move_ticket(&key, DropTarget::ReadyForDev, true, &me())
        .await
        .unwrap();
    w.starter
        .start(&key, &me(), otaq_core::session::Start::Auto)
        .await
        .unwrap();
    until_status(w, &key, Status::Working).await;
    w.sessions
        .running_of(&t.id, SessionKind::Agent)
        .expect("the agent runs")
}

/// What the session's row says it ran on: the tool, the model as passed and the agent.
async fn stored_on(
    w: &TestService,
    session: &str,
) -> (Option<Tool>, Option<String>, Option<SessionAgent>) {
    let s = w.sessions.stored(session).await.unwrap();
    (s.tool, s.model, s.agent)
}

/// A ticket's Dev session runs Developer (the Dev stage's agent) on its model, with its effort: `--model opus
/// --effort high`; on `default` with no default model it runs the new-session tool's default (no `--model`); with a
/// default model set, `default` resolves to it at the start and that is what the session stores (AGM-03, AGM-05).
#[tokio::test(flavor = "multi_thread")]
async fn a_dev_session_runs_developer_on_its_model() {
    let world = world().await;
    let w = &world.w;
    let developer = world.agent("Developer").await;
    let price = create(w, "Price table").await;
    world
        .set("Developer", claude(Some("opus")), Some("high"))
        .await;
    let session = start(w, &price).await;
    let id = session.tool_session_id.clone().unwrap();
    assert_eq!(
        launched(w, &session.id).await,
        format!(
            "--session-id {id} --model opus --effort high {}",
            claude_skill("work-ticket")
        )
    );
    assert_eq!(
        stored_on(w, &session.id).await,
        (Some(Tool::Claude), Some("opus".into()), named(&developer))
    );
    assert_eq!(
        w.sessions
            .stored(&session.id)
            .await
            .unwrap()
            .effort
            .as_deref(),
        Some("high")
    );

    world.set("Developer", AgentModel::Default, None).await;
    let footer = create(w, "Update footer links").await;
    let session = start(w, &footer).await;
    let id = session.tool_session_id.clone().unwrap();
    assert_eq!(
        launched(w, &session.id).await,
        format!("--session-id {id} {}", claude_skill("work-ticket")),
        "no default model yet: the tool's own default, no model passed"
    );
    assert_eq!(
        stored_on(w, &session.id).await,
        (Some(Tool::Claude), None, named(&developer))
    );

    world
        .rules
        .set_default_model(&ModelRef {
            tool: Tool::ALL[0],
            model: Some("haiku".into()),
        })
        .await
        .unwrap();
    let links = create(w, "Link checker").await;
    let session = start(w, &links).await;
    let id = session.tool_session_id.clone().unwrap();
    assert_eq!(
        launched(w, &session.id).await,
        format!(
            "--session-id {id} --model haiku {}",
            claude_skill("work-ticket")
        ),
        "default resolved at the start"
    );
    assert_eq!(
        stored_on(w, &session.id).await,
        (Some(Tool::Claude), Some("haiku".into()), named(&developer))
    );
    let run: (Option<String>, Option<String>) = sqlx::query_as(
        "SELECT agent_id, model FROM stage_runs WHERE session_id = ? AND stage_id = 'dev'",
    )
    .bind(&session.id)
    .fetch_one(w.board.store().pool())
    .await
    .unwrap();
    assert_eq!(
        run,
        (Some(developer.id.clone()), Some("haiku".into())),
        "the stage run keeps the agent and the model"
    );
    world.w.stop().await;
}

/// Retry start resolves again: a model changed after the start stopped gives the new model (Q-B relies on it).
#[tokio::test(flavor = "multi_thread")]
async fn retry_start_after_the_model_changed_runs_the_new_model() {
    let world = world().await;
    let w = &world.w;
    let mut settings = w.board.project_settings("SHOP").await.unwrap();
    settings.setup_command = "exit 3".into();
    w.board
        .set_project_settings("SHOP", &settings)
        .await
        .unwrap();
    let t = create(w, "Fix README typo").await;
    world.set_model("Developer", Some("opus")).await;
    w.board
        .move_ticket("SHOP-1", DropTarget::ReadyForDev, true, &me())
        .await
        .unwrap();
    w.starter
        .start("SHOP-1", &me(), otaq_core::session::Start::Auto)
        .await
        .unwrap();
    until_status(w, "SHOP-1", Status::StartInterrupted).await;

    settings.setup_command = String::new();
    w.board
        .set_project_settings("SHOP", &settings)
        .await
        .unwrap();
    world.set_model("Developer", Some("haiku")).await;
    w.starter.retry("SHOP-1", &me()).await.unwrap();
    w.starter
        .start("SHOP-1", &me(), otaq_core::session::Start::Auto)
        .await
        .unwrap();
    until_status(w, "SHOP-1", Status::Working).await;
    let session = w.sessions.running_of(&t.id, SessionKind::Agent).unwrap();
    let id = session.tool_session_id.clone().unwrap();
    assert_eq!(
        launched(w, &session.id).await,
        format!(
            "--session-id {id} --model haiku {}",
            claude_skill("work-ticket")
        )
    );
    world.w.stop().await;
}

/// A Validate round runs Validator (the agent check's agent) on its own model while Dev runs Developer's: separate
/// sessions, each with its own agent; the next round resolves the model again.
#[tokio::test(flavor = "multi_thread")]
async fn a_check_round_runs_its_own_agent() {
    let world = world().await;
    let w = &world.w;
    let t = create(w, "Coupon field").await;
    world.set_model("Developer", Some("opus")).await;
    world.set_model("Validator", Some("haiku")).await;
    let developer = start(w, &t).await;
    let validator = w
        .validation
        .start_validator(&t.id, otaq_core::session::Start::Auto)
        .await
        .unwrap()
        .session()
        .unwrap();
    let id = validator.tool_session_id.clone().unwrap();
    let prompt = w.adapters.get(Tool::Claude).skill_reference("validate");
    assert_eq!(
        launched(w, &validator.id).await,
        format!("--session-id {id} --model haiku {prompt}")
    );
    assert_eq!(
        stored_on(w, &validator.id).await,
        (
            Some(Tool::Claude),
            Some("haiku".into()),
            named(&world.agent("Validator").await)
        )
    );
    let dev_id = developer.tool_session_id.clone().unwrap();
    assert_eq!(
        launched(w, &developer.id).await,
        format!(
            "--session-id {dev_id} --model opus {}",
            claude_skill("work-ticket")
        )
    );

    // Between rounds the agent may change (d0088): the next round runs on its model now.
    w.sessions.stop(&validator.id).await.unwrap();
    world.set_model("Validator", Some("sonnet")).await;
    let next = w
        .validation
        .start_validator(&t.id, otaq_core::session::Start::Auto)
        .await
        .unwrap()
        .session()
        .unwrap();
    let id = next.tool_session_id.clone().unwrap();
    assert_eq!(
        launched(w, &next.id).await,
        format!("--session-id {id} --model sonnet {prompt}")
    );
    world.w.stop().await;
}

/// An epic's lead runs the agent with the Epic lead role and a Project agent the agent with the Project agent role
/// (AGM-05): both launch with their agent's model, and the Project agent's row stores it.
#[tokio::test(flavor = "multi_thread")]
async fn the_lead_and_the_project_agent_run_the_role_agents() {
    let world = world().await;
    let w = &world.w;
    let wishlist = w
        .board
        .create_ticket(
            &NewTicketInput {
                project: "SHOP".into(),
                title: "Wishlist".into(),
                kind: Kind::Epic,
                ..Default::default()
            },
            &me(),
        )
        .await
        .unwrap();
    world.set_model("Epic lead", Some("opus")).await;
    let lead = w.project_agents.leads().open("SHOP-1").await.unwrap();
    let session = lead.agent.session.clone().unwrap();
    let id = session.tool_session_id.clone().unwrap();
    assert_eq!(
        launched(w, &session.id).await,
        format!(
            "--session-id {id} --model opus {}",
            claude_skill("epic-lead")
        )
    );
    assert_eq!(
        stored_on(w, &session.id).await,
        (
            Some(Tool::Claude),
            Some("opus".into()),
            named(&world.agent("Epic lead").await)
        )
    );
    assert_eq!(lead.agent.model.as_deref(), Some("opus"));
    assert_eq!(lead.epic, wishlist.key);

    world.set_model("Planner", Some("haiku")).await;
    let agent = w
        .project_agents
        .open("SHOP", "Why is checkout slow?")
        .await
        .unwrap();
    let session = agent.session.clone().unwrap();
    let id = session.tool_session_id.clone().unwrap();
    assert_eq!(
        launched(w, &session.id).await,
        format!(
            "--session-id {id} --model haiku {} Why is checkout slow?",
            claude_skill("project-agent")
        )
    );
    assert_eq!(agent.model.as_deref(), Some("haiku"));
    assert_eq!(
        stored_on(w, &session.id).await.2,
        named(&world.agent("Planner").await)
    );
    world.w.stop().await;
}

/// "New agent on Strong" (M23, CHANGES-v §4): a Project agent opened on another agent than the role's starts on that
/// agent and stores it; an unknown one is refused.
#[tokio::test(flavor = "multi_thread")]
async fn a_project_agent_opens_on_the_agent_asked_for() {
    let world = world().await;
    let w = &world.w;
    let strong = world.agent("Strong").await;
    let agent = w
        .project_agents
        .open_on("SHOP", "Why is checkout slow?", Some(&strong.id))
        .await
        .unwrap();
    let session = agent.session.clone().unwrap();
    let id = session.tool_session_id.clone().unwrap();
    assert_eq!(
        launched(w, &session.id).await,
        format!(
            "--session-id {id} --model opus {} Why is checkout slow?",
            claude_skill("project-agent")
        )
    );
    assert_eq!(stored_on(w, &session.id).await.2, named(&strong));
    assert!(
        w.project_agents
            .open_on("SHOP", "Hello", Some("no-such-agent"))
            .await
            .is_err()
    );
    world.w.stop().await;
}

/// The resume paths (the models report, "Two adapter rules" 1; d0088 "Resume reopens the same tool and model").
#[derive(Debug, Clone, Copy)]
enum Resume {
    SendBack,
    Park,
    AfterRestart,
    OpenPr,
    Answer,
    ProjectAgent,
    Lead,
}

impl Resume {
    /// The agent whose session the path resumes.
    fn agent(self) -> &'static str {
        match self {
            Self::ProjectAgent => "Planner",
            Self::Lead => "Epic lead",
            _ => "Developer",
        }
    }
}

const EVERY_RESUME: [Resume; 7] = [
    Resume::SendBack,
    Resume::Park,
    Resume::AfterRestart,
    Resume::OpenPr,
    Resume::Answer,
    Resume::ProjectAgent,
    Resume::Lead,
];

const CONVERSATION: &str = "0199d2a4-3b1c-7c3e-9f00-123456789abc";

/// Records an ended agent session of `t` on `agent` with the model it launched with, as an earlier start would have.
async fn record_ended_on(w: &TestService, t: &Ticket, agent: &Agent) {
    w.record_ended_agent(t, CONVERSATION).await;
    sqlx::query(
        "UPDATE sessions SET model = ?, agent_id = ?, agent_name = ? WHERE tool_session_id = ?",
    )
    .bind(model_of(agent))
    .bind(&agent.id)
    .bind(&agent.name)
    .bind(CONVERSATION)
    .execute(w.board.store().pool())
    .await
    .unwrap();
}

/// Runs `path` for a session first launched with its agent on `model` (then edited to `edited_to`): returns the
/// resumed session's arguments, what it stored, and the arguments expected with `model` passed.
async fn resume(
    world: World,
    path: Resume,
    model: Option<&str>,
    edited_to: Option<&str>,
) -> (World, String, String, SessionInfo) {
    let mut world = world;
    let on = path.agent();
    world.set_model(on, model).await;
    let agent = world.agent(on).await;
    let flag = model_flag(model);
    let (session, expected) = match path {
        Resume::SendBack => {
            let w = &world.w;
            let t = create(w, "Fix flaky login redirect").await;
            w.start_worktree(&t).await;
            record_ended_on(w, &t, &agent).await;
            world.set_model(on, edited_to).await;
            w.force_status(&t, Status::WaitingForYou).await;
            w.board
                .add_comment(&NewComment {
                    key: "SHOP-1".into(),
                    file: "login.ts".into(),
                    line: 1,
                    side: Side::New,
                    body: "Keep the query string".into(),
                })
                .await
                .unwrap();
            w.review
                .fail("SHOP-1", "", None, false, &me())
                .await
                .unwrap();
            let s = w
                .sessions
                .running_of(&t.id, SessionKind::Agent)
                .expect("resumed");
            (
                s,
                format!(
                    "--resume {CONVERSATION}{flag} {} SHOP-1 was sent back to Dev with 1 line comment. Run `otaq \
                     prime`, then `otaq review feedback`.",
                    claude_skill("address-review")
                ),
            )
        }
        Resume::Park => {
            let w = &world.w;
            let t = create(w, "Price table").await;
            let first = start(w, &t).await;
            let conversation = first.tool_session_id.clone().unwrap();
            world.set_model(on, edited_to).await;
            w.review.park("SHOP-1", true, &me()).await.unwrap();
            let s = start(w, &t).await;
            assert_ne!(s.id, first.id);
            (s, format!("--resume {conversation}{flag}"))
        }
        Resume::AfterRestart => {
            let t = create(&world.w, "Upgrade to Next.js 15").await;
            let first = start(&world.w, &t).await;
            let conversation = first.tool_session_id.clone().unwrap();
            world.set_model(on, edited_to).await;
            let World { w, claude, .. } = world;
            let w = w.restart_with(launch_config(claude.path())).await;
            let rules = Agents::new(w.board.clone(), Arc::new(FixedModels::sample()));
            world = World { w, claude, rules };
            let w = &world.w;
            assert_eq!(w.status("SHOP-1").await, Status::Interrupted);
            w.review.resume("SHOP-1", &me()).await.unwrap();
            let s = w
                .sessions
                .running_of(&t.id, SessionKind::Agent)
                .expect("resumed");
            (
                s,
                format!(
                    "--resume {conversation}{flag} {}",
                    otaq_core::recovery::carry_on_text()
                ),
            )
        }
        Resume::OpenPr => {
            let w = &world.w;
            let t = create(w, "Checkout API").await;
            let worktree = w.start_worktree(&t).await;
            commit(&worktree, "api.ts", "checkout\n");
            record_ended_on(w, &t, &agent).await;
            world.set_model(on, edited_to).await;
            w.force_status(&t, Status::WaitingForYou).await;
            // A local origin has no PR provider: Developer's own imported Open PR skill runs instead.
            let folder = otaq_core::skills::testing::skill_folder(w.data.root(), "my-pr");
            import_open_pr(&world, &folder).await;
            w.prs.open("SHOP-1", true, false, &me()).await.unwrap();
            let s = w
                .sessions
                .running_of(&t.id, SessionKind::Agent)
                .expect("resumed");
            // Written into the worktree before the resume, and run by its copy's name (AGM-07).
            assert!(
                worktree
                    .join(".agents/skills/otaq-my-pr/SKILL.md")
                    .is_file()
            );
            let prompt = w.adapters.get(Tool::Claude).skill_reference("my-pr");
            (s, format!("--resume {CONVERSATION}{flag} {prompt}"))
        }
        Resume::Answer => {
            let w = &world.w;
            let parent = create(w, "Checkout API").await;
            let child = create(w, "Checkout page").await;
            w.board
                .add_dependency("SHOP-2", "SHOP-1", &me())
                .await
                .unwrap();
            let worktree = w.start_worktree(&parent).await;
            commit(&worktree, "api.ts", "checkout\n");
            record_ended_on(w, &parent, &agent).await;
            let folder = world
                .claude
                .path()
                .join("projects")
                .join(project_folder(&worktree));
            std::fs::create_dir_all(&folder).unwrap();
            std::fs::write(folder.join(format!("{CONVERSATION}.jsonl")), "{}\n").unwrap();
            w.force_status(&parent, Status::Merged).await;
            let merged = w.board.ticket("SHOP-1").await.unwrap();
            w.review.finish(&merged).await.unwrap();
            w.force_status(&child, Status::Working).await;
            world.set_model(on, edited_to).await;
            w.messages
                .ask(
                    "SHOP-2",
                    "SHOP-1",
                    "Which status code on a declined card?",
                    &Actor::Agent(child.key.clone()),
                )
                .await
                .unwrap();
            let s = w
                .sessions
                .running_of(&parent.id, SessionKind::Answer)
                .expect("an answer session");
            (
                s,
                format!(
                    "--resume {CONVERSATION}{flag} {}",
                    claude_skill("answer-question")
                ),
            )
        }
        Resume::ProjectAgent => {
            let w = &world.w;
            let opened = w.project_agents.open("SHOP", "Footer audit").await.unwrap();
            let first = opened.session.clone().unwrap();
            w.sessions.stop(&first.id).await.unwrap();
            wait_for_agent(w, &opened.id, AgentState::Ended).await;
            world.set_model(on, edited_to).await;
            let resumed = w.project_agents.resume(&opened.id).await.unwrap();
            let s = resumed.session.unwrap();
            let conversation = opened.tool_session_id.unwrap();
            (s, format!("--resume {conversation}{flag}"))
        }
        Resume::Lead => {
            let w = &world.w;
            w.board
                .create_ticket(
                    &NewTicketInput {
                        project: "SHOP".into(),
                        title: "Wishlist".into(),
                        kind: Kind::Epic,
                        ..Default::default()
                    },
                    &me(),
                )
                .await
                .unwrap();
            let leads = w.project_agents.leads();
            let opened = leads.open("SHOP-1").await.unwrap();
            let first = opened.agent.session.clone().unwrap();
            w.sessions.stop(&first.id).await.unwrap();
            wait_for_agent(w, &opened.agent.id, AgentState::Ended).await;
            world.set_model(on, edited_to).await;
            let resumed = leads.resume("SHOP-1").await.unwrap();
            let s = resumed.agent.session.unwrap();
            let conversation = opened.agent.tool_session_id.unwrap();
            (s, format!("--resume {conversation}{flag}"))
        }
    };
    let args = launched(&world.w, &session.id).await;
    (world, args, expected, session)
}

/// Imports the test skill folder `my-pr` onto Developer, and the flow's Delivery names it as its Open PR skill.
async fn import_open_pr(world: &World, folder: &Path) {
    world
        .w
        .import_step_skill(otaq_core::board::settings::WorkflowStep::OpenPr, folder)
        .await;
}

async fn wait_for_agent(w: &TestService, id: &str, state: AgentState) {
    tokio::time::timeout(Duration::from_secs(10), async {
        while w.project_agents.get(id).await.unwrap().state != state {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("agent {id} never became {state:?}"));
}

/// Every path that resumes a conversation launches with the session's own stored model, never the agent's current
/// one (AGM-03: "Resume reopens the same tool and model"): a session started on `sonnet` resumes with `--model
/// sonnet` after its agent was edited to `opus`, and the resumed session's row names the agent with `sonnet` again.
#[tokio::test(flavor = "multi_thread")]
async fn every_resume_passes_the_sessions_own_model() {
    for path in EVERY_RESUME {
        let (world, args, expected, session) =
            resume(world().await, path, Some("sonnet"), Some("opus")).await;
        assert_eq!(args, expected, "{path:?}");
        let agent = world.agent(path.agent()).await;
        assert_eq!(
            model_of(&agent).as_deref(),
            Some("opus"),
            "edited mid-ticket"
        );
        assert_eq!(
            stored_on(&world.w, &session.id).await,
            (Some(Tool::Claude), Some("sonnet".into()), named(&agent)),
            "{path:?}: the new session row copies the one it resumes"
        );
        world.w.stop().await;
    }
}

/// A session that ran on the tool default resumes with no `--model`, even after its agent got one.
#[tokio::test(flavor = "multi_thread")]
async fn every_resume_of_a_tool_default_session_passes_no_model() {
    for path in EVERY_RESUME {
        let (world, args, expected, session) =
            resume(world().await, path, None, Some("haiku")).await;
        assert_eq!(args, expected, "{path:?}");
        assert!(!args.contains("--model"), "{path:?}: {args}");
        assert_eq!(stored_on(&world.w, &session.id).await.1, None, "{path:?}");
        world.w.stop().await;
    }
}

/// An edit to an agent mid-ticket changes only new sessions (0088): SHOP-1's Send back still passes `sonnet`, a new
/// ticket starts with the new `opus`.
#[tokio::test(flavor = "multi_thread")]
async fn an_edited_agent_changes_only_new_sessions() {
    let (world, args, _, _) = resume(
        world().await,
        Resume::SendBack,
        Some("sonnet"),
        Some("opus"),
    )
    .await;
    assert!(args.contains("--model sonnet"), "{args}");
    let w = &world.w;
    let t = create(w, "Wishlist share link").await;
    let session = start(w, &t).await;
    let id = session.tool_session_id.clone().unwrap();
    assert_eq!(
        launched(w, &session.id).await,
        format!(
            "--session-id {id} --model opus {}",
            claude_skill("work-ticket")
        )
    );
    world.w.stop().await;
}

/// A Resume passes the session's own effort again, whatever its agent's effort is now (AGM-01, AGM-03).
#[tokio::test(flavor = "multi_thread")]
async fn a_resume_passes_the_sessions_own_effort() {
    let world = world().await;
    let w = &world.w;
    let t = create(w, "Price table").await;
    world
        .set("Developer", claude(Some("opus")), Some("high"))
        .await;
    let first = start(w, &t).await;
    let conversation = first.tool_session_id.clone().unwrap();
    world
        .set("Developer", claude(Some("opus")), Some("low"))
        .await;
    w.review.park("SHOP-1", true, &me()).await.unwrap();
    let s = start(w, &t).await;
    assert_eq!(
        launched(w, &s.id).await,
        format!("--resume {conversation} --model opus --effort high")
    );
    assert_eq!(
        w.sessions.stored(&s.id).await.unwrap().effort.as_deref(),
        Some("high")
    );
    world.w.stop().await;
}

/// The exact model the tool reports is written once on the session (the first value only), from Claude Code's
/// `SessionStart` hook; a later different report (a `/model` typed into the pane) is only logged.
#[tokio::test(flavor = "multi_thread")]
async fn the_model_that_ran_is_written_once() {
    let world = world().await;
    let w = &world.w;
    let t = create(w, "Price table").await;
    world.set_model("Developer", Some("opus")).await;
    let session = start(w, &t).await;
    let caller = Caller::Agent {
        ticket: t.key.clone(),
        ticket_id: t.id.clone(),
        session: session.id.clone(),
    };
    let payload = |model: &str| {
        json!({"hook_event_name": "SessionStart", "source": "startup", "model": model,
               "session_id": session.tool_session_id})
    };
    w.signals
        .hook(
            &caller,
            Tool::Claude,
            "SessionStart",
            &payload("claude-opus-5-5"),
        )
        .await
        .unwrap();
    let stored = w.sessions.stored(&session.id).await.unwrap();
    assert_eq!(stored.model_ran.as_deref(), Some("claude-opus-5-5"));
    assert_eq!(
        stored.model.as_deref(),
        Some("opus"),
        "the model as passed stays"
    );
    assert_eq!(
        w.sessions.info(&session.id).unwrap().model_ran.as_deref(),
        Some("claude-opus-5-5"),
        "the running session's info follows"
    );
    w.signals
        .hook(
            &caller,
            Tool::Claude,
            "SessionStart",
            &payload("claude-sonnet-5-5"),
        )
        .await
        .unwrap();
    assert_eq!(
        w.sessions
            .stored(&session.id)
            .await
            .unwrap()
            .model_ran
            .as_deref(),
        Some("claude-opus-5-5"),
        "the first value wins"
    );
    // An agent's model is never read back from the pane: Developer stays on `opus`.
    assert_eq!(
        model_of(&world.agent("Developer").await).as_deref(),
        Some("opus")
    );
    world.w.stop().await;
}

/// A Project agent's exact model is saved on its session and its row.
#[tokio::test(flavor = "multi_thread")]
async fn a_project_agents_model_that_ran_is_on_its_row() {
    let world = world().await;
    let w = &world.w;
    let agent = w.project_agents.open("SHOP", "Footer audit").await.unwrap();
    let session = agent.session.clone().unwrap();
    let caller = Caller::ProjectAgent {
        project: "SHOP".into(),
        agent: agent.id.clone(),
        session: session.id.clone(),
    };
    w.signals
        .hook(
            &caller,
            Tool::Claude,
            "SessionStart",
            &json!({"hook_event_name": "SessionStart", "source": "startup", "model": "claude-sonnet-5-5"}),
        )
        .await
        .unwrap();
    let row: Option<String> =
        sqlx::query_scalar("SELECT model_ran FROM project_agents WHERE id = ?")
            .bind(&agent.id)
            .fetch_one(w.board.store().pool())
            .await
            .unwrap();
    assert_eq!(row.as_deref(), Some("claude-sonnet-5-5"));
    assert_eq!(
        w.sessions
            .stored(&session.id)
            .await
            .unwrap()
            .model_ran
            .as_deref(),
        Some("claude-sonnet-5-5")
    );
    world.w.stop().await;
}
