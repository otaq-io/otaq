//! "What this agent reads" (T-0440; AGR-01…AGR-04; d0108 rule 12, d0092, d0098): the reads text equals what the
//! agent is given when it starts: `prime`'s instructions, notes, stage skill and commit skill for a stage agent, and
//! the Project agent's prime instructions and the skill its launch runs first. A skill part per
//! stage the agent runs (a ticket's choice doesn't move it), one for its role, the other skills with the commit skill
//! and the Project agent's `flow` marked; nothing for an agent in no stage.
//!
//! The plugin folder is the repository's (read only); no tool is started.

use std::path::Path;

use otaq_core::agent_reads::{AgentReads, RunsAs, SkillMark};
use otaq_core::agent_tools::LaunchContext;
use otaq_core::agent_tools::claude::{ClaudeLaunch, find_plugin_dir};
use otaq_core::agents::Agent;
use otaq_core::board::stage_choices::{ChoicePick, Chooser};
use otaq_core::client::{Client, ClientConfig};
use otaq_core::domain::{Actor, Status};
use otaq_core::flow::{FlowFile, yaml};
use otaq_core::project_agents::{FLOW_COMMAND, first_skill};
use otaq_core::protocol::{Reply, Request};
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::TestService;
use otaq_core::skills::testing::claude_skill;
use tempfile::TempDir;

/// The sample board on a service whose plugin folder is the repository's, with shop-web's notes.
async fn world(dir: &TempDir) -> TestService {
    let bin = dir.path().join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let plugin_dir = find_plugin_dir(Path::new("/nonexistent")).expect("the repository's plugin");
    let w = TestService::start_with(move |config| ServiceConfig {
        launch: Some(LaunchContext {
            bin_dir: bin,
            plugin_dir,
            path: std::env::var_os("PATH").unwrap_or_default(),
            claude: ClaudeLaunch {
                program: vec!["true".into()],
                config_dir: None,
            },
        }),
        ..config
    })
    .await;
    w.shop_sample().await;
    let mut settings = w.board.project_settings("SHOP").await.unwrap();
    settings.project_notes = "Use pnpm, not npm. Run cargo fmt before you commit.".into();
    w.board
        .set_project_settings("SHOP", &settings)
        .await
        .unwrap();
    w
}

/// The test flow `tests/flows/every-kind.yaml`: Product spec → Dev (Rust developer; Quick developer an option) →
/// Validate → Security → QA → Done (Shipper).
fn every_kind() -> FlowFile {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/flows/every-kind.yaml");
    yaml::read(&std::fs::read_to_string(path).unwrap()).unwrap()
}

async fn agent(w: &TestService, name: &str) -> Agent {
    w.board.store().agent_named(name).await.unwrap().unwrap()
}

/// `agents.reads` over the socket, as the app asks.
async fn reads(w: &TestService, agent: &Agent, project: &str) -> AgentReads {
    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    match client
        .request(Request::AgentsReads(
            otaq_core::agent_reads::AgentReadsParams {
                agent: agent.id.clone(),
                project: project.into(),
            },
        ))
        .await
        .unwrap()
    {
        Reply::AgentReads(r) => *r,
        other => panic!("{other:?}"),
    }
}

/// The part running in `stage`.
fn stage_part<'r>(reads: &'r AgentReads, stage: &str) -> &'r otaq_core::agent_reads::SkillPart {
    reads
        .skills
        .iter()
        .find(|p| p.runs == RunsAs::Stage(stage.into()))
        .unwrap_or_else(|| panic!("no {stage} part in {reads:#?}"))
}

fn commit_mark(reads: &AgentReads) -> Option<&str> {
    reads
        .others
        .iter()
        .find(|o| o.mark == Some(SkillMark::Commits))
        .map(|o| o.skill.as_str())
}

/// AGR-04: a stage agent's reads text equals its ticket's prime (instructions, notes, stage skill, commit skill), on
/// the default flow (Developer commits with `commit`) and on the test flow. Sessions don't read an agent's skills
/// (T2): the stage's skill is Otaq's own, and every session commits with Otaq's `commit`.
#[tokio::test(flavor = "multi_thread")]
async fn a_stage_agent_reads_what_prime_gives() {
    let dir = TempDir::new().unwrap();
    let w = world(&dir).await;
    let tool = w.adapters.for_new_session();
    w.edit_agent("Developer", |a| {
        a.instructions = "Work one ticket at a time on its own branch.".into();
    })
    .await;
    let developer = agent(&w, "Developer").await;

    let prime = w.board.prime_context("SHOP-20", tool).await.unwrap();
    let told = prime.instructions.clone().unwrap();
    let r = reads(&w, &developer, "SHOP").await;
    assert_eq!(
        (r.agent.as_str(), r.project_name.as_str()),
        ("Developer", "shop-web")
    );
    assert_eq!(r.instructions, told.own);
    assert_eq!(r.notes, told.notes);
    let stage = prime.stage.unwrap();
    let dev = stage_part(&r, &stage.name);
    assert_eq!(Some(tool.skill_reference(&dev.skill)), stage.skill);
    assert_eq!(
        commit_mark(&r)
            .map(|s| tool.skill_reference(s))
            .unwrap_or_default(),
        prime.commit_skill
    );
    assert_eq!(prime.commit_skill, claude_skill("commit"));
    // The shipped SKILL.md in full, from the bundle; Developer also opens PRs in the default flow's Done.
    let shipped = w.adapters.shipped_skills().unwrap().join("work-ticket");
    assert_eq!(dev.folder.as_deref(), Some(shipped.as_path()));
    assert_eq!(
        dev.text.as_deref(),
        Some(
            std::fs::read_to_string(shipped.join("SKILL.md"))
                .unwrap()
                .as_str()
        )
    );
    let parts: Vec<(&RunsAs, &str)> = r
        .skills
        .iter()
        .map(|p| (&p.runs, p.skill.as_str()))
        .collect();
    assert_eq!(
        parts,
        [
            (&RunsAs::Stage("Dev".into()), "work-ticket"),
            (&RunsAs::Stage("Delivery".into()), "open-pr")
        ]
    );
    let others: Vec<(&str, Option<SkillMark>)> = r
        .others
        .iter()
        .map(|o| (o.skill.as_str(), o.mark))
        .collect();
    assert_eq!(others, [("commit", Some(SkillMark::Commits))]);

    // The test flow: Rust developer runs Otaq's `work-ticket` in Dev.
    w.board.use_flow_for_tests("SHOP", &every_kind()).await;
    let rust = agent(&w, "Rust developer").await;
    let t = w.create("SHOP", "Sort notes by date").await;
    w.force_status(&t, Status::Working).await; // in Dev
    let prime = w
        .board
        .prime_context(&t.key.to_string(), tool)
        .await
        .unwrap();
    let r = reads(&w, &rust, "SHOP").await;
    let told = prime.instructions.unwrap();
    assert_eq!(
        (r.instructions.as_str(), r.notes.as_str()),
        (told.own.as_str(), told.notes.as_str())
    );
    let dev = stage_part(&r, "Dev");
    assert_eq!(
        Some(tool.skill_reference(&dev.skill)),
        prime.stage.unwrap().skill
    );
    assert_eq!(dev.folder.as_deref(), Some(shipped.as_path()));
    assert_eq!(prime.commit_skill, claude_skill("commit"));
    assert_eq!(r.skills.len(), 1, "{r:#?}");
    // An agent has no skill list of its own (T6): only the commit skill prime names.
    let others: Vec<&str> = r.others.iter().map(|o| o.skill.as_str()).collect();
    assert_eq!(others, ["commit"]);
    w.stop().await;
}

/// AGR-03, AGR-04: the Project agent reads its prime's instructions and notes, the skill its launch runs first ("as
/// Project agent"), and `flow` marked for flow changes, which its `/flow` start runs.
#[tokio::test(flavor = "multi_thread")]
async fn the_agent_noyan_uses_reads_what_its_launch_gives() {
    let dir = TempDir::new().unwrap();
    let w = world(&dir).await;
    w.edit_agent("Noyan", |a| {
        a.instructions = "Split big work into tickets of a day or less.".into();
    })
    .await;
    let planner = w.board.store().noyan_agent().await.unwrap().unwrap();
    let prime = w.board.project_prime("SHOP", None).await.unwrap();
    let told = prime.instructions.unwrap();
    let r = reads(&w, &planner, "SHOP").await;
    assert_eq!(
        (r.instructions.as_str(), r.notes.as_str()),
        (told.own.as_str(), told.notes.as_str())
    );
    let (first, _) = first_skill("Split SHOP-40");
    assert_eq!(r.skills.len(), 1, "in no stage: {r:#?}");
    assert_eq!(r.skills[0].runs, RunsAs::Noyan);
    assert_eq!(r.skills[0].skill, first);
    assert!(r.skills[0].text.is_some());
    let (flow, _) = first_skill(FLOW_COMMAND);
    let others: Vec<(&str, Option<SkillMark>)> = r
        .others
        .iter()
        .map(|o| (o.skill.as_str(), o.mark))
        .collect();
    assert_eq!(others, [(flow, Some(SkillMark::FlowChanges))]);

    w.stop().await;
}

/// AGR-02, AGR-03: an agent with a role and a stage gets a part for each, the stage's first; an agent in no stage
/// (an option of Dev only) gets none; a ticket's choice of Quick developer for Dev doesn't move Dev's part, though
/// that ticket's prime names Quick developer.
#[tokio::test(flavor = "multi_thread")]
async fn parts_follow_the_flow_not_a_tickets_choice() {
    let dir = TempDir::new().unwrap();
    let w = world(&dir).await;
    w.board.use_flow_for_tests("SHOP", &every_kind()).await;
    let t = w.create("SHOP", "Fix README typo").await;
    let key = t.key.to_string();
    let writes = w
        .board
        .check_choices(
            "SHOP",
            Some(&t),
            "",
            &[ChoicePick {
                stage: "Dev".into(),
                choice: "Quick developer".into(),
                reason: None,
            }],
            &Chooser::User,
            false,
        )
        .await
        .unwrap();
    w.board
        .write_choices(&t, &writes, &Actor::User("me".into()))
        .await
        .unwrap();
    w.force_status(&t, Status::Working).await;
    let prime = w
        .board
        .prime_context(&key, w.adapters.for_new_session())
        .await
        .unwrap();
    assert_eq!(
        prime.stage.unwrap().agent.as_deref(),
        Some("Quick developer")
    );

    let quick = reads(&w, &agent(&w, "Quick developer").await, "SHOP").await;
    assert!(quick.skills.is_empty(), "{quick:#?}");
    let rust = reads(&w, &agent(&w, "Rust developer").await, "SHOP").await;
    assert_eq!(rust.skills.len(), 1);
    assert_eq!(stage_part(&rust, "Dev").skill, "work-ticket");
    // Validator runs Validate; Security has its own agent.
    let validator = reads(&w, &agent(&w, "Validator").await, "SHOP").await;
    let parts: Vec<(&RunsAs, &str)> = validator
        .skills
        .iter()
        .map(|p| (&p.runs, p.skill.as_str()))
        .collect();
    assert_eq!(parts, [(&RunsAs::Stage("Validate".into()), "validate")]);

    // Noyan, the Project agent, also runs Product spec.
    w.board
        .use_flow_for_tests(
            "SHOP",
            &yaml::read(
                &std::fs::read_to_string(
                    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/flows/every-kind.yaml"),
                )
                .unwrap()
                .replace("    agent: Spec writer\n", "    agent: Noyan\n"),
            )
            .unwrap(),
        )
        .await;
    let planner = reads(&w, &agent(&w, "Noyan").await, "SHOP").await;
    let parts: Vec<(&RunsAs, &str)> = planner
        .skills
        .iter()
        .map(|p| (&p.runs, p.skill.as_str()))
        .collect();
    assert_eq!(
        parts,
        [
            (&RunsAs::Stage("Product spec".into()), "product-spec"),
            (&RunsAs::Noyan, "noyan")
        ]
    );
    w.stop().await;
}

/// A user's stage skill is shown from where the skill finder found it: with `team-validate` in the personal
/// `~/.claude/skills` on Validate, a Claude Code validator's part is that SKILL.md, and its folder (what Open skill
/// folder → opens) is `~/.claude/skills/team-validate`, not a copy. A Codex validator's tool doesn't have it (Codex
/// doesn't read `~/.claude/skills`): the part names Codex and shows no text or folder.
#[tokio::test(flavor = "multi_thread")]
async fn a_users_stage_skill_is_read_from_its_own_folder() {
    use otaq_core::agent_tools::Tool;
    use otaq_core::agents::AgentModel;

    let tool_home = TempDir::new().unwrap();
    let home = tool_home.path().to_path_buf();
    let w = TestService::start_with(move |mut config| {
        config.tool_home = Some(home);
        config
    })
    .await;
    w.shop_sample().await;
    let folder = tool_home.path().join(".claude/skills/team-validate");
    std::fs::create_dir_all(&folder).unwrap();
    let text = "---\nname: team-validate\ndescription: Our checks.\n---\n\nRun our checks.\n";
    std::fs::write(folder.join("SKILL.md"), text).unwrap();
    let mut flow = every_kind();
    for stage in &mut flow.flow.stages {
        if stage.name == "Validate" {
            stage.skill = Some("team-validate".into());
        }
    }
    w.board.use_flow_for_tests("SHOP", &flow).await;

    let on = |tool| {
        move |a: &mut otaq_core::agents::store::NewAgent| {
            a.model = AgentModel::Tool { tool, model: None };
        }
    };
    w.edit_agent("Validator", on(Tool::Claude)).await;
    let validator = agent(&w, "Validator").await;
    let r = reads(&w, &validator, "SHOP").await;
    let part = stage_part(&r, "Validate");
    assert_eq!(part.skill, "team-validate");
    assert_eq!(part.text.as_deref(), Some(text));
    assert_eq!(part.folder.as_deref(), Some(folder.as_path()));
    assert_eq!(part.missing_in, None);

    w.edit_agent("Validator", on(Tool::Codex)).await;
    let r = reads(&w, &validator, "SHOP").await;
    let part = stage_part(&r, "Validate");
    assert_eq!(part.skill, "team-validate");
    assert_eq!((&part.text, &part.folder), (&None, &None));
    assert_eq!(part.missing_in, Some(Tool::Codex));

    // A copy Codex reads (`~/.agents/skills`) is the one its validator's page shows.
    let shared = tool_home.path().join(".agents/skills/team-validate");
    std::fs::create_dir_all(&shared).unwrap();
    std::fs::write(shared.join("SKILL.md"), "Shared copy.\n").unwrap();
    let r = reads(&w, &validator, "SHOP").await;
    let part = stage_part(&r, "Validate");
    assert_eq!(part.text.as_deref(), Some("Shared copy.\n"));
    assert_eq!(part.folder.as_deref(), Some(shared.as_path()));
    assert_eq!(part.missing_in, None);
    w.stop().await;
}
