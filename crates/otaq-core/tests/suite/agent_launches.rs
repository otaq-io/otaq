//! Sessions at launch (T-0405; AGM-05, AGM-03, ADP-04, SES-03; d0108, d0088, UX-65, UX-70): every new session runs on
//! who does its work (the Agent stage's model, the agent check's, "Noyan starts on" for a Noyan conversation, or the
//! conversation's own pick) and passes that model (`--model opus`, nothing for the tool default; `model: default`
//! resolved to the default model at start) and its effort; every resume passes the session's own stored model and
//! effort again, whatever the stage says now; the session stores what it runs for, the tool, the model as passed and
//! the effort, then the exact model the tool reports. `claude` stands in as `sh` printing its arguments: no test
//! starts claude.

use std::path::Path;
use std::time::Duration;

use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::agent_tools::claude::transcript::project_folder;
use otaq_core::agent_tools::{LaunchContext, Tool};
use otaq_core::agents::{About, AgentModel, ModelRef, NoyanStartsOn};
use otaq_core::board::comments::{NewComment, Side};
use otaq_core::board::moves::DropTarget;
use otaq_core::board::tickets::Ticket;
use otaq_core::caller::Caller;
use otaq_core::domain::{Actor, Status};
use otaq_core::git::testing::commit;
use otaq_core::project_agents::AgentState;
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::TestService;
use otaq_core::session::{SessionAgent, SessionInfo, SessionKind};
use otaq_core::skills::testing::claude_skill;
use serde_json::json;
use tempfile::TempDir;

/// Prints its arguments, then waits (reading what is typed into it).
const SCRIPT: &str = "printf 'ARGS %s\\n' \"$*\"; exec cat";

fn me() -> Actor {
    Actor::User("me".into())
}

struct World {
    w: TestService,
    /// The user's Claude folder (the answer path reads a conversation's file there to know it can be resumed).
    claude: TempDir,
}

impl World {
    /// Saves a new model on what the default flow's agent `name` did (an edit mid-ticket): Claude Code with `model`
    /// (`None`: its default). `Developer`: the Agent stages and the delivery; `Checker`: the checks; `Noyan`: "Noyan
    /// starts on".
    async fn set_model(&self, name: &str, model: Option<&str>) {
        self.set(name, claude(model), None).await;
    }

    /// Saves a new model and effort there.
    async fn set(&self, name: &str, model: AgentModel, effort: Option<&str>) {
        self.w
            .edit_agent(name, |page| {
                page.model = model;
                page.effort = effort.map(str::to_owned);
            })
            .await;
    }

    /// The model Dev's stage names now (`claude:opus`).
    async fn dev_model(&self) -> Option<String> {
        let flow = self.w.board.effective_flow("SHOP").await.unwrap();
        flow.stage("dev").unwrap().who.model.clone()
    }
}

fn claude(model: Option<&str>) -> AgentModel {
    AgentModel::Tool {
        tool: Tool::ALL[0],
        model: model.map(str::to_owned),
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

/// Project SHOP (`shop-web`, with an origin for Open PR) on the default flow, every stage on `default` with no default
/// model set yet (the new-session tool's own default), and Noyan on `default` too.
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
    World {
        w,
        claude: claude_home,
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

/// What a session of SHOP's stage `stage` stores it runs for, on Claude Code.
fn on_stage(stage: &str) -> Option<SessionAgent> {
    Some(SessionAgent {
        runs_for: About::Stage {
            project: "SHOP".into(),
            stage: stage.into(),
        }
        .scope(),
        name: Tool::Claude.display_name().into(),
    })
}

/// What a Noyan conversation's session stores it runs for, on Claude Code.
fn on_noyan() -> Option<SessionAgent> {
    Some(SessionAgent {
        runs_for: About::Noyan.scope(),
        name: Tool::Claude.display_name().into(),
    })
}

async fn create(w: &TestService, title: &str) -> Ticket {
    w.create("SHOP", title).await
}

/// waiting to begin → Dev: the start sequence runs and the agent works.
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

/// A ticket's Dev session runs on the Dev stage's model, with its effort: `--model opus --effort high`; on `default`
/// with no default model it runs the new-session tool's default (no `--model`); with a default model set, `default`
/// resolves to it at the start and that is what the session stores (AGM-03, AGM-05, UX-65).
#[tokio::test(flavor = "multi_thread")]
async fn a_dev_session_runs_on_its_stages_model() {
    let world = world().await;
    let w = &world.w;
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
            claude_skill("otaq:work-ticket")
        )
    );
    assert_eq!(
        stored_on(w, &session.id).await,
        (Some(Tool::Claude), Some("opus".into()), on_stage("dev"))
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
        format!("--session-id {id} {}", claude_skill("otaq:work-ticket")),
        "no default model yet: the tool's own default, no model passed"
    );
    assert_eq!(
        stored_on(w, &session.id).await,
        (Some(Tool::Claude), None, on_stage("dev"))
    );

    w.board
        .store()
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
            claude_skill("otaq:work-ticket")
        ),
        "default resolved at the start"
    );
    assert_eq!(
        stored_on(w, &session.id).await,
        (Some(Tool::Claude), Some("haiku".into()), on_stage("dev"))
    );
    let run: (Option<String>, Option<String>) = sqlx::query_as(
        "SELECT agent, model FROM stage_runs WHERE session_id = ? AND stage_id = 'dev'",
    )
    .bind(&session.id)
    .fetch_one(w.board.store().pool())
    .await
    .unwrap();
    assert_eq!(
        run,
        (Some("Claude Code".into()), Some("haiku".into())),
        "the stage run keeps the tool and the model"
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
            claude_skill("otaq:work-ticket")
        )
    );
    world.w.stop().await;
}

/// A Validate round runs on the agent check's own model while Dev runs on Dev's: separate sessions, each on its
/// stage's model; the next round resolves the model again.
#[tokio::test(flavor = "multi_thread")]
async fn a_check_round_runs_its_own_agent() {
    let world = world().await;
    let w = &world.w;
    let t = create(w, "Coupon field").await;
    world.set_model("Developer", Some("opus")).await;
    world.set_model("Checker", Some("haiku")).await;
    let developer = start(w, &t).await;
    let validator = w
        .validation
        .start_validator(&t.id, otaq_core::session::Start::Auto)
        .await
        .unwrap()
        .session()
        .unwrap();
    let id = validator.tool_session_id.clone().unwrap();
    let prompt = w
        .adapters
        .get(Tool::Claude)
        .skill_reference("otaq:validate");
    assert_eq!(
        launched(w, &validator.id).await,
        format!("--session-id {id} --model haiku {prompt}")
    );
    assert_eq!(
        stored_on(w, &validator.id).await,
        (
            Some(Tool::Claude),
            Some("haiku".into()),
            on_stage("validate")
        )
    );
    let dev_id = developer.tool_session_id.clone().unwrap();
    assert_eq!(
        launched(w, &developer.id).await,
        format!(
            "--session-id {dev_id} --model opus {}",
            claude_skill("otaq:work-ticket")
        )
    );

    // Between rounds the stage's model may change (d0088): the next round runs on its model now.
    w.sessions.stop(&validator.id).await.unwrap();
    world.set_model("Checker", Some("sonnet")).await;
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

/// "Noyan starts on" is one app setting (UX-70): changed to Claude Code · opus · high, the next Noyan conversation
/// starts on it.
#[tokio::test(flavor = "multi_thread")]
async fn the_next_conversation_starts_on_noyans_setting() {
    let world = world().await;
    let w = &world.w;
    world.set("Noyan", claude(Some("opus")), Some("high")).await;
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
            "--session-id {id} --model opus --effort high {} Why is checkout slow?",
            claude_skill("otaq:noyan")
        )
    );
    assert_eq!(stored_on(w, &session.id).await.2, on_noyan());
    world.w.stop().await;
}

/// A new conversation picks its own model and effort before its first message (UX-70): it starts on them and stores
/// them; a model that doesn't read is refused.
#[tokio::test(flavor = "multi_thread")]
async fn a_project_agent_opens_on_the_model_asked_for() {
    let world = world().await;
    let w = &world.w;
    let pick = NoyanStartsOn {
        model: Some("claude:opus".into()),
        effort: None,
    };
    let agent = w
        .project_agents
        .open_on("SHOP", "Why is checkout slow?", Some(&pick))
        .await
        .unwrap();
    let session = agent.session.clone().unwrap();
    let id = session.tool_session_id.clone().unwrap();
    assert_eq!(
        launched(w, &session.id).await,
        format!(
            "--session-id {id} --model opus {} Why is checkout slow?",
            claude_skill("otaq:noyan")
        )
    );
    assert_eq!(stored_on(w, &session.id).await.1.as_deref(), Some("opus"));
    // Switched to sonnet · medium in the panel's header before typing (canvas A-6): it runs on both.
    let sonnet = NoyanStartsOn {
        model: Some("claude:sonnet".into()),
        effort: Some("medium".into()),
    };
    let agent = w
        .project_agents
        .open_on("SHOP", "Which avatar sizes?", Some(&sonnet))
        .await
        .unwrap();
    let session = agent.session.clone().unwrap();
    let id = session.tool_session_id.clone().unwrap();
    assert_eq!(
        launched(w, &session.id).await,
        format!(
            "--session-id {id} --model sonnet --effort medium {} Which avatar sizes?",
            claude_skill("otaq:noyan")
        )
    );
    assert_eq!(
        w.sessions
            .stored(&session.id)
            .await
            .unwrap()
            .effort
            .as_deref(),
        Some("medium")
    );
    let bad = NoyanStartsOn {
        model: Some("gpt-9".into()),
        effort: None,
    };
    assert!(
        w.project_agents
            .open_on("SHOP", "Hello", Some(&bad))
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
}

impl Resume {
    /// Whose model the path's session runs on: the default flow's Developer (Dev, the delivery), or Noyan.
    fn agent(self) -> &'static str {
        match self {
            Self::ProjectAgent => "Noyan",
            _ => "Developer",
        }
    }

    /// What its session stores it runs for.
    fn runs_for(self) -> Option<SessionAgent> {
        match self {
            Self::ProjectAgent => on_noyan(),
            _ => on_stage("dev"),
        }
    }
}

const EVERY_RESUME: [Resume; 6] = [
    Resume::SendBack,
    Resume::Park,
    Resume::AfterRestart,
    Resume::OpenPr,
    Resume::Answer,
    Resume::ProjectAgent,
];

const CONVERSATION: &str = "0199d2a4-3b1c-7c3e-9f00-123456789abc";

/// Records an ended Dev session of `t` on Claude Code with `model` (as passed), as an earlier start would have.
async fn record_ended_on(w: &TestService, t: &Ticket, model: Option<&str>) {
    w.record_ended_agent(t, CONVERSATION).await;
    let on = on_stage("dev").unwrap();
    sqlx::query(
        "UPDATE sessions SET model = ?, runs_for = ?, agent_name = ? WHERE tool_session_id = ?",
    )
    .bind(model)
    .bind(&on.runs_for)
    .bind(&on.name)
    .bind(CONVERSATION)
    .execute(w.board.store().pool())
    .await
    .unwrap();
}

/// Runs `path` for a session first launched with its stage on `model` (then edited to `edited_to`): returns the
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
    let flag = model_flag(model);
    let (session, expected) = match path {
        Resume::SendBack => {
            let w = &world.w;
            let t = create(w, "Fix flaky login redirect").await;
            w.start_worktree(&t).await;
            record_ended_on(w, &t, model).await;
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
                .fail("SHOP-1", "", Vec::new(), false, &me())
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
                    claude_skill("otaq:address-review")
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
            (
                s,
                format!(
                    "--resume {conversation}{flag} {}",
                    otaq_core::recovery::carry_on_after_park_text()
                ),
            )
        }
        Resume::AfterRestart => {
            let t = create(&world.w, "Upgrade to Next.js 15").await;
            let first = start(&world.w, &t).await;
            let conversation = first.tool_session_id.clone().unwrap();
            world.set_model(on, edited_to).await;
            let World { w, claude, .. } = world;
            let w = w.restart_with(launch_config(claude.path())).await;
            world = World { w, claude };
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
            record_ended_on(w, &t, model).await;
            world.set_model(on, edited_to).await;
            w.force_status(&t, Status::WaitingForYou).await;
            // A local origin has no PR provider: the user's Open PR skill the Delivery stage names runs instead.
            let folder = otaq_core::skills::testing::skill_folder(w.data.root(), "my-pr");
            name_open_pr_skill(&world, &folder).await;
            // The dialog opens at once; the confirm resumes the conversation with the skill.
            w.prs.open("SHOP-1", true, false, &me()).await.unwrap();
            let proposal = w
                .board
                .pr_flow(&t.id)
                .await
                .unwrap()
                .unwrap()
                .proposal
                .unwrap();
            w.prs
                .confirm("SHOP-1", &proposal, false, &me())
                .await
                .unwrap();
            let s = w
                .sessions
                .running_of(&t.id, SessionKind::Agent)
                .expect("resumed");
            // Run by its own name, told the user confirmed; nothing is written into the worktree (T2).
            assert!(!worktree.join(".agents").exists());
            let prompt = w.adapters.get(Tool::Claude).skill_reference("my-pr");
            let confirmed =
                otaq_core::pr::service::confirmed_instruction(otaq_core::pr::flow::FlowKind::Open);
            (
                s,
                format!("--resume {CONVERSATION}{flag} {prompt} {confirmed}"),
            )
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
            record_ended_on(w, &parent, model).await;
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
                    claude_skill("otaq:answer-question")
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
    };
    let args = launched(&world.w, &session.id).await;
    (world, args, expected, session)
}

/// The flow's Delivery names the test skill `my-pr` as its Open PR skill; nothing is copied or imported.
async fn name_open_pr_skill(world: &World, folder: &Path) {
    world
        .w
        .use_step_skill(otaq_core::board::settings::WorkflowStep::OpenPr, folder)
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

/// Every path that resumes a conversation launches with the session's own stored model, never the stage's current
/// one (AGM-03: "Resume reopens the same tool and model"): a session started on `sonnet` resumes with `--model
/// sonnet` after its stage was edited to `opus`, and the resumed session's row says what it runs for with `sonnet`
/// again.
#[tokio::test(flavor = "multi_thread")]
async fn every_resume_passes_the_sessions_own_model() {
    for path in EVERY_RESUME {
        let (world, args, expected, session) =
            resume(world().await, path, Some("sonnet"), Some("opus")).await;
        assert_eq!(args, expected, "{path:?}");
        let now = match path {
            Resume::ProjectAgent => world.w.board.store().noyan_starts_on().await.unwrap().model,
            _ => world.dev_model().await,
        };
        assert_eq!(now.as_deref(), Some("claude:opus"), "edited mid-ticket");
        assert_eq!(
            stored_on(&world.w, &session.id).await,
            (Some(Tool::Claude), Some("sonnet".into()), path.runs_for()),
            "{path:?}: the new session row copies the one it resumes"
        );
        world.w.stop().await;
    }
}

/// A session that ran on the tool default resumes with no `--model`, even after its stage got one.
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

/// An edit to a stage's model mid-ticket changes only new sessions (0088): SHOP-1's Send back still passes `sonnet`, a new
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
            claude_skill("otaq:work-ticket")
        )
    );
    world.w.stop().await;
}

/// A Resume passes the session's own effort again, whatever its stage's effort is now (AGM-01, AGM-03).
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
        format!(
            "--resume {conversation} --model opus --effort high {}",
            otaq_core::recovery::carry_on_after_park_text()
        )
    );
    assert_eq!(
        w.sessions.stored(&s.id).await.unwrap().effort.as_deref(),
        Some("high")
    );
    world.w.stop().await;
}

/// The exact model the tool reports is written once on the session (the first value only), from Claude Code's
/// `SessionStart` hook; a later different report (a `/model` typed into the agent panel) is only logged.
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
    // A stage's model is never read back from the agent panel: Dev stays on `opus`.
    assert_eq!(world.dev_model().await.as_deref(), Some("claude:opus"));
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

/// Noyan has no instructions of its own (UX-74): its conversations' `prime --project`, and one read by hand, give only
/// the project's notes, whatever model the conversation runs on.
#[tokio::test(flavor = "multi_thread")]
async fn a_conversations_prime_gives_only_the_projects_notes() {
    let world = world().await;
    let w = &world.w;
    let mut settings = w.board.project_settings("SHOP").await.unwrap();
    settings.project_notes = "Use pnpm.".into();
    w.board
        .set_project_settings("SHOP", &settings)
        .await
        .unwrap();
    let pick = NoyanStartsOn {
        model: Some("claude:opus".into()),
        effort: None,
    };
    let conversation = w
        .project_agents
        .open_on("SHOP", "Why is checkout slow?", Some(&pick))
        .await
        .unwrap();
    let told = |prime: otaq_core::project_agents::prime::ProjectPrime| {
        let i = prime.instructions.unwrap();
        (i.agent, i.own, i.notes)
    };
    let notes_only = (None, String::new(), "Use pnpm.".to_owned());
    assert_eq!(
        told(
            w.board
                .project_prime("SHOP", Some(&conversation.id))
                .await
                .unwrap()
        ),
        notes_only
    );
    assert_eq!(
        told(w.board.project_prime("SHOP", None).await.unwrap()),
        notes_only
    );
    for s in w
        .sessions
        .in_memory()
        .into_iter()
        .filter(|s| s.is_running())
    {
        let _ = w.sessions.stop(&s.id).await;
    }
    world.w.stop().await;
}
