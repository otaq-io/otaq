//! "What this agent reads" (T-0440; AGR-01…AGR-04; d0108 rule 12, d0092, d0098): the reads text equals what the
//! agent is given when it starts: `prime`'s instructions, notes, stage skill and commit skill for a stage agent, and
//! the Project agent's and the epic lead's prime instructions and the skill their launch runs first. A skill part per
//! stage the agent runs (a ticket's choice doesn't move it), one for its role, the other skills with the commit skill
//! and the Project agent's `flow` marked; nothing for an agent in no stage.
//!
//! The plugin folder is the repository's (read only); copies go into temporary folders; no tool is started.

use std::path::Path;

use otaq_core::agent_reads::{AgentReads, RunsAs, SkillMark};
use otaq_core::agent_tools::claude::{ClaudeLaunch, find_plugin_dir};
use otaq_core::agent_tools::{EPIC_LEAD_SKILL, FirstPrompt, LaunchContext};
use otaq_core::agents::{Agent, Role};
use otaq_core::board::stage_choices::{ChoicePick, Chooser};
use otaq_core::client::{Client, ClientConfig};
use otaq_core::domain::{Actor, Kind, Status};
use otaq_core::flow::{FlowFile, yaml};
use otaq_core::project_agents::{FLOW_COMMAND, first_skill};
use otaq_core::protocol::{Reply, Request};
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::TestService;
use otaq_core::skills::copy::Marker;
use otaq_core::skills::testing::skill_folder;
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

/// The test flow `tests/flows/every-kind.yaml`: Grill → Dev (Rust developer; Quick developer an option) → Validate →
/// Security → QA → Done (Shipper).
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

/// The skill a launch of `agent` runs first for `name`, as the copies before it resolve it (AGM-07).
async fn launch_runs(w: &TestService, agent: &Agent, name: &str) -> String {
    let root = TempDir::new().unwrap();
    let (prompt, _) = w
        .board
        .copy_for_session(
            root.path(),
            w.adapters.shipped_skills(),
            Some(agent),
            Some(FirstPrompt::Skill {
                name: name.into(),
                message: None,
            }),
            Marker::Checkout,
        )
        .await;
    match prompt {
        Some(FirstPrompt::Skill { name, .. }) => name,
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
/// the default flow (Developer commits with `commit`) and on the test flow (Rust developer has no commit skill), with
/// an imported skill standing for the stage's.
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
    assert_eq!(prime.commit_skill, "/otaq-commit");
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
            (&RunsAs::Stage("Done".into()), "open-pr")
        ]
    );
    let others: Vec<(&str, Option<SkillMark>)> = r
        .others
        .iter()
        .map(|o| (o.skill.as_str(), o.mark))
        .collect();
    assert_eq!(others, [("commit", Some(SkillMark::Commits))]);

    // The test flow: Rust developer, its imported `work-ticket` standing for the stage's.
    w.board.use_flow_for_tests("SHOP", &every_kind()).await;
    let rust = agent(&w, "Rust developer").await;
    let folder = skill_folder(dir.path(), "work-ticket");
    let rust = otaq_core::agents::skills::import(&w.board, &rust.id, &folder)
        .await
        .unwrap();
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
    assert_eq!(
        dev.folder,
        Some(w.board.data().skill_path(&rust.imported[0].id).unwrap())
    );
    assert!(
        dev.text.as_deref().unwrap().contains("Do the step."),
        "{dev:?}"
    );
    assert_eq!(commit_mark(&r), None);
    assert_eq!(prime.commit_skill, "");
    assert_eq!(r.skills.len(), 1, "{r:#?}");
    let others: Vec<&str> = r.others.iter().map(|o| o.skill.as_str()).collect();
    assert_eq!(others, ["commit-compact"]);
    w.stop().await;
}

/// AGR-03, AGR-04: the Project agent reads its prime's instructions and notes, the skill its launch runs first ("as
/// Project agent"), and `flow` marked for flow changes, which its `/flow` start runs; the epic lead the same with its
/// own skill.
#[tokio::test(flavor = "multi_thread")]
async fn the_role_agents_read_what_their_launch_gives() {
    let dir = TempDir::new().unwrap();
    let w = world(&dir).await;
    w.edit_agent("Noyan", |a| {
        a.instructions = "Split epics into tickets of a day or less.".into();
    })
    .await;
    let planner = w
        .board
        .store()
        .role_agent(Role::ProjectAgent)
        .await
        .unwrap()
        .unwrap();
    let prime = w.board.project_prime("SHOP").await.unwrap();
    let told = prime.instructions.unwrap();
    let r = reads(&w, &planner, "SHOP").await;
    assert_eq!(
        (r.instructions.as_str(), r.notes.as_str()),
        (told.own.as_str(), told.notes.as_str())
    );
    let (first, _) = first_skill("Split SHOP-40");
    assert_eq!(r.skills.len(), 1, "in no stage: {r:#?}");
    assert_eq!(r.skills[0].runs, RunsAs::Role(Role::ProjectAgent));
    assert_eq!(r.skills[0].skill, launch_runs(&w, &planner, first).await);
    assert!(r.skills[0].text.is_some());
    let (flow, _) = first_skill(FLOW_COMMAND);
    let others: Vec<(&str, Option<SkillMark>)> = r
        .others
        .iter()
        .map(|o| (o.skill.as_str(), o.mark))
        .collect();
    assert_eq!(
        others,
        [(
            launch_runs(&w, &planner, flow).await.as_str(),
            Some(SkillMark::FlowChanges)
        )]
    );

    let lead = w
        .board
        .store()
        .role_agent(Role::EpicLead)
        .await
        .unwrap()
        .unwrap();
    let prime = otaq_core::epic_lead::prime::context(&w.board, "SHOP-40")
        .await
        .unwrap();
    let r = reads(&w, &lead, "SHOP").await;
    assert_eq!(r.instructions, "");
    assert_eq!(prime.instructions.unwrap().notes, r.notes);
    assert_eq!(r.skills.len(), 1);
    assert_eq!(r.skills[0].runs, RunsAs::Role(Role::EpicLead));
    assert_eq!(
        r.skills[0].skill,
        launch_runs(&w, &lead, EPIC_LEAD_SKILL).await
    );
    assert!(r.others.is_empty(), "{r:#?}");
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
            (Kind::Ticket, ""),
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
        .write_choices(&t, &writes, &Actor::User("me".into()), None)
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

    // Noyan, the Project agent, also runs Grill.
    w.board
        .use_flow_for_tests(
            "SHOP",
            &yaml::read(
                &std::fs::read_to_string(
                    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/flows/every-kind.yaml"),
                )
                .unwrap()
                .replace("    agent: Griller\n", "    agent: Noyan\n"),
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
            (&RunsAs::Stage("Grill".into()), "grill"),
            (&RunsAs::Role(Role::ProjectAgent), "noyan")
        ]
    );
    w.stop().await;
}
