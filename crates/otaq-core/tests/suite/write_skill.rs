//! A stage's skill the Project agent writes (T-0472; FED-06; d0115): a new Plan stage with no skill has no problem
//! (Otaq's own skill for the stage runs). "Ask the agent to write it" saves into the project (T6): `flow.write_skill` with the
//! skill's name gives the agent a folder inside the project's checkout, `.agents/skills/<name>/` for a Codex agent and
//! `.claude/skills/<name>/` otherwise, and refuses one that is already there; handing it in names the skill on the
//! stage, with nothing copied, imported or committed. Undo and Stop delete the folder only while git says it is
//! untracked and it holds only what was written. A ticket's agent can't write one. The folders an older Otaq kept
//! skills in go at the service's start.

use std::path::{Path, PathBuf};

use otaq_core::agent_tools::Tool;
use otaq_core::agents::{AgentModel, FixedModels};
use otaq_core::flow::draft::DraftEdit;
use otaq_core::flow::proposal::ApplyChoices;
use otaq_core::flow::write_skill::saved_text;
use otaq_core::flow::{FlowFile, Stage, StageKind, yaml};
use otaq_core::git::testing::git;
use otaq_core::service::testing::TestService;
use serde_json::{Value, json};

fn models() -> FixedModels {
    FixedModels::sample()
}

/// The draft's document with a new Plan stage (An agent works, Developer, no skill) after Dev.
async fn with_plan(w: &TestService, key: &str) -> FlowFile {
    let draft = w.board.draft(key, &models()).await.unwrap();
    let mut file = yaml::read(&draft.proposal.yaml).unwrap();
    let dev = file.flow.position("dev").unwrap();
    file.flow.stages.insert(
        dev + 1,
        Stage {
            id: "plan".into(),
            name: "Plan".into(),
            kind: StageKind::Agent,
            agent: Some("Developer".into()),
            skill: None,
            options: Vec::new(),
            rounds: None,
            fail_to: None,
            description: None,
        },
    );
    w.board
        .update_draft(
            key,
            &DraftEdit::Document {
                yaml: yaml::write(&file),
            },
            &models(),
        )
        .await
        .unwrap();
    file
}

/// What the agent writes in the folder Otaq gives it.
fn write_plan_skill(folder: &Path) {
    std::fs::write(
        folder.join("SKILL.md"),
        "---\nname: plan-ticket\ndescription: Breaks the ticket into steps and checks them with you before Dev.\n---\n\
         # Plan the ticket\n\nList the steps in a comment and wait for the user's OK.\n",
    )
    .unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_written_skill_is_saved_in_the_project_for_a_claude_code_agent() {
    let w = TestService::start().await;
    let project = w.add_project("shop-web").await;
    let key = project.key.clone();
    let checkout = PathBuf::from(&project.repo_path);
    w.set_agent_model(
        "Developer",
        AgentModel::Tool {
            tool: Tool::Claude,
            model: None,
        },
    )
    .await;

    // A new Agent stage with no skill: the draft holds it with no problem, since Otaq's own skill for it runs.
    with_plan(&w, &key).await;
    let draft = w.board.draft(&key, &models()).await.unwrap();
    let messages: Vec<&str> = draft
        .proposal
        .problems
        .iter()
        .map(|p| p.message.as_str())
        .collect();
    assert!(messages.is_empty(), "{messages:?}");

    // Ask the agent to write it: being written, by the agent the user asked.
    let draft = w
        .board
        .update_draft(
            &key,
            &DraftEdit::SkillWriting {
                stage: "plan".into(),
                agent: Some("pa-1".into()),
            },
            &models(),
        )
        .await
        .unwrap();
    assert_eq!(draft.writing.len(), 1);

    // The skill needs a name first, and one of Otaq's own names is refused.
    let unnamed = w
        .board
        .skill_folder(&key, "Plan", None, None)
        .await
        .unwrap_err();
    assert!(unnamed.to_string().contains("--name"), "{unnamed}");
    let own = w
        .board
        .skill_folder(&key, "Plan", Some("validate"), None)
        .await
        .unwrap_err();
    assert!(own.to_string().contains("own skills"), "{own}");
    let odd = w
        .board
        .skill_folder(&key, "Plan", Some("../x"), None)
        .await
        .unwrap_err();
    assert!(odd.to_string().contains("can't name a skill"), "{odd}");

    // The folder is in the checkout, where Claude Code reads skills.
    let given = w
        .board
        .skill_folder(&key, "Plan", Some("plan-ticket"), None)
        .await
        .unwrap();
    let folder = PathBuf::from(&given.folder);
    assert_eq!(
        folder,
        checkout.join(".claude").join("skills").join("plan-ticket")
    );
    assert!(folder.is_dir());
    assert!(given.text.contains("name: plan-ticket"), "{}", given.text);
    assert!(
        given.text.contains("flow write-skill plan --done"),
        "{}",
        given.text
    );
    let draft = w.board.draft(&key, &models()).await.unwrap();
    assert_eq!(
        draft.writing[0].agent.as_deref(),
        Some("pa-1"),
        "still the agent asked"
    );
    assert_eq!(
        draft.writing[0].folder.as_deref(),
        Some(".claude/skills/plan-ticket")
    );
    // Asking again keeps the folder this write made.
    w.board
        .skill_folder(&key, "plan", None, None)
        .await
        .unwrap();

    // Handing in an empty folder is refused, and the folder stays for the agent to fix.
    let empty = w
        .board
        .skill_written(&key, "plan", "Checkout slowness")
        .await
        .unwrap_err();
    assert!(empty.to_string().contains("SKILL.md"), "{empty}");
    assert!(folder.is_dir());

    write_plan_skill(&folder);
    let written = w
        .board
        .skill_written(&key, "plan", "Checkout slowness")
        .await
        .unwrap();
    assert_eq!(written.skill.as_deref(), Some("plan-ticket"));
    assert!(
        written
            .text
            .contains(&saved_text(".claude/skills/plan-ticket")),
        "{}",
        written.text
    );
    assert_eq!(
        saved_text(".claude/skills/plan-ticket"),
        "Saved in `.claude/skills/plan-ticket` in this project. Commit it so agents get it in their worktrees."
    );
    assert!(
        folder.join("SKILL.md").is_file(),
        "the skill stays where it was written"
    );
    assert!(
        !w.data.root().join("skills").exists(),
        "nothing is copied into Otaq's data folder"
    );
    let status = git(
        &checkout,
        &["status", "--porcelain", "--untracked-files=all"],
    );
    assert!(
        status.contains("?? .claude/skills/plan-ticket/SKILL.md"),
        "not committed: {status}"
    );

    // The draft names it, tinted as written, with its folder; Apply is on.
    let draft = w.board.draft(&key, &models()).await.unwrap();
    assert!(draft.writing.is_empty());
    assert_eq!(draft.written.len(), 1);
    assert_eq!(draft.written[0].skill, "plan-ticket");
    assert_eq!(draft.written[0].by, "Checkout slowness");
    assert_eq!(draft.written[0].folder, ".claude/skills/plan-ticket");
    let file = yaml::read(&draft.proposal.yaml).unwrap();
    assert_eq!(
        file.flow.stage("plan").unwrap().skill.as_deref(),
        Some("plan-ticket")
    );
    assert!(
        draft.proposal.problems.is_empty(),
        "{:?}",
        draft.proposal.problems
    );
    assert!(
        draft
            .proposal
            .diff
            .warnings
            .iter()
            .all(|w| !w.text.contains("plan-ticket")),
        "the user has it now: {:?}",
        draft.proposal.diff.warnings
    );

    // Apply: the stage runs it by its name; no agent gets a skill.
    w.board
        .apply_proposal(&key, draft.proposal.id, &ApplyChoices::default(), &models())
        .await
        .unwrap();
    let flow = w.board.effective_flow(&key).await.unwrap();
    assert_eq!(
        flow.stage("plan").unwrap().skill.as_deref(),
        Some("plan-ticket")
    );

    // Another write can't take a folder that is already there.
    let taken = w
        .board
        .skill_folder(&key, "dev", Some("plan-ticket"), None)
        .await
        .unwrap_err();
    assert!(
        taken.to_string().contains("already in this project"),
        "{taken}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_codex_agents_skill_goes_in_the_agents_folder() {
    let w = TestService::start().await;
    let project = w.add_project("shop-web").await;
    let key = project.key.clone();
    w.set_agent_model(
        "Developer",
        AgentModel::Tool {
            tool: Tool::Codex,
            model: None,
        },
    )
    .await;
    with_plan(&w, &key).await;
    let given = w
        .board
        .skill_folder(&key, "plan", Some("plan-ticket"), None)
        .await
        .unwrap();
    assert_eq!(
        PathBuf::from(&given.folder),
        PathBuf::from(&project.repo_path)
            .join(".agents")
            .join("skills")
            .join("plan-ticket")
    );
    write_plan_skill(Path::new(&given.folder));
    w.board.skill_written(&key, "plan", "you").await.unwrap();
    let draft = w.board.draft(&key, &models()).await.unwrap();
    assert_eq!(draft.written[0].folder, ".agents/skills/plan-ticket");
    let file = yaml::read(&draft.proposal.yaml).unwrap();
    assert_eq!(
        file.flow.stage("plan").unwrap().skill.as_deref(),
        Some("plan-ticket")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn undo_deletes_an_untracked_folder_and_keeps_a_committed_one() {
    let w = TestService::start().await;
    let project = w.add_project("shop-web").await;
    let key = project.key.clone();
    let checkout = PathBuf::from(&project.repo_path);
    with_plan(&w, &key).await;
    let undo = |w: &TestService| {
        let key = key.clone();
        let board = w.board.clone();
        async move {
            board
                .update_draft(
                    &key,
                    &DraftEdit::UndoSkill {
                        stage: "plan".into(),
                    },
                    &models(),
                )
                .await
                .unwrap()
        }
    };

    // Untracked and as written: Undo deletes the folder and Plan has no skill again.
    let given = w
        .board
        .skill_folder(&key, "plan", Some("plan-ticket"), None)
        .await
        .unwrap();
    write_plan_skill(Path::new(&given.folder));
    w.board.skill_written(&key, "plan", "you").await.unwrap();
    let draft = undo(&w).await;
    assert!(draft.written.is_empty());
    assert!(
        !Path::new(&given.folder).exists(),
        "the untracked folder goes"
    );
    let file = yaml::read(&draft.proposal.yaml).unwrap();
    assert_eq!(file.flow.stage("plan").unwrap().skill, None);
    assert!(
        draft.proposal.problems.is_empty(),
        "{:?}",
        draft.proposal.problems
    );

    // Committed: Undo only clears the field.
    let given = w
        .board
        .skill_folder(&key, "plan", Some("plan-steps"), None)
        .await
        .unwrap();
    write_plan_skill(Path::new(&given.folder));
    w.board.skill_written(&key, "plan", "you").await.unwrap();
    git(&checkout, &["add", ".claude"]);
    git(&checkout, &["commit", "-q", "-m", "Plan skill"]);
    let draft = undo(&w).await;
    assert!(draft.written.is_empty());
    assert!(
        Path::new(&given.folder).join("SKILL.md").is_file(),
        "a committed folder stays"
    );
    let file = yaml::read(&draft.proposal.yaml).unwrap();
    assert_eq!(file.flow.stage("plan").unwrap().skill, None);

    // Untracked but changed since it was handed in: Undo keeps it too.
    let given = w
        .board
        .skill_folder(&key, "plan", Some("plan-later"), None)
        .await
        .unwrap();
    write_plan_skill(Path::new(&given.folder));
    w.board.skill_written(&key, "plan", "you").await.unwrap();
    std::fs::write(Path::new(&given.folder).join("notes.md"), "mine").unwrap();
    undo(&w).await;
    assert!(
        Path::new(&given.folder).join("notes.md").is_file(),
        "the user's edit stays"
    );

    // Stop: the field is empty again and the untracked folder being written goes; handing in then has nothing.
    let given = w
        .board
        .skill_folder(&key, "plan", Some("plan-stopped"), Some("pa-1"))
        .await
        .unwrap();
    let draft = w.board.draft(&key, &models()).await.unwrap();
    assert_eq!(draft.writing.len(), 1, "asking for the folder marks it");
    let draft = w
        .board
        .update_draft(
            &key,
            &DraftEdit::StopWriting {
                stage: "plan".into(),
            },
            &models(),
        )
        .await
        .unwrap();
    assert!(draft.writing.is_empty());
    assert!(!Path::new(&given.folder).exists());
    let late = w
        .board
        .skill_written(&key, "plan", "you")
        .await
        .unwrap_err();
    assert!(late.to_string().contains("Nothing to hand in"), "{late}");

    // A stage no agent runs has no skill to write.
    let none = w
        .board
        .skill_folder(&key, "review", Some("x"), None)
        .await
        .unwrap_err();
    assert!(none.to_string().contains("runs no agent"), "{none}");
}

/// Undo and Stop also take away the folders above the skill's that the write made (`.claude/`, `.claude/skills/`), once
/// nothing else is in them; a folder that was there before the write stays, empty or not.
#[tokio::test(flavor = "multi_thread")]
async fn undo_and_stop_leave_no_empty_folders_the_write_made() {
    let w = TestService::start().await;
    let project = w.add_project("shop-web").await;
    let key = project.key.clone();
    let checkout = PathBuf::from(&project.repo_path);
    with_plan(&w, &key).await;
    let claude = checkout.join(".claude");
    let skills = claude.join("skills");
    assert!(!claude.exists(), "the checkout starts without .claude");
    let edit = |edit: DraftEdit| {
        let key = key.clone();
        let board = w.board.clone();
        async move { board.update_draft(&key, &edit, &models()).await.unwrap() }
    };
    let undo = || {
        edit(DraftEdit::UndoSkill {
            stage: "plan".into(),
        })
    };
    let stop = || {
        edit(DraftEdit::StopWriting {
            stage: "plan".into(),
        })
    };

    // Written and undone: the skill's folder, `.claude/skills` and `.claude` all go.
    let given = w
        .board
        .skill_folder(&key, "plan", Some("plan-ticket"), None)
        .await
        .unwrap();
    write_plan_skill(Path::new(&given.folder));
    w.board.skill_written(&key, "plan", "you").await.unwrap();
    undo().await;
    assert!(
        !claude.exists(),
        ".claude was made by the write and is empty"
    );

    // Asked for a folder, nothing written, stopped: no empty folder stays behind.
    w.board
        .skill_folder(&key, "plan", Some("plan-ticket"), None)
        .await
        .unwrap();
    assert!(skills.join("plan-ticket").is_dir());
    stop().await;
    assert!(
        !claude.exists(),
        "nothing is left of the folder made for it"
    );

    // Renamed before anything was written: the first name's folder goes, the folders above stay for the new one.
    w.board
        .skill_folder(&key, "plan", Some("plan-a"), None)
        .await
        .unwrap();
    w.board
        .skill_folder(&key, "plan", Some("plan-b"), None)
        .await
        .unwrap();
    assert!(!skills.join("plan-a").exists());
    assert!(skills.join("plan-b").is_dir());
    stop().await;
    assert!(!claude.exists(), "the renamed write made them, so they go");

    // `.claude` was there before (empty): only `.claude/skills`, which the write made, goes.
    std::fs::create_dir(&claude).unwrap();
    let given = w
        .board
        .skill_folder(&key, "plan", Some("plan-ticket"), None)
        .await
        .unwrap();
    write_plan_skill(Path::new(&given.folder));
    w.board.skill_written(&key, "plan", "you").await.unwrap();
    undo().await;
    assert!(!skills.exists(), ".claude/skills was made by the write");
    assert!(
        claude.is_dir(),
        "the .claude that was there stays, though empty"
    );

    // Something else landed in `.claude/skills` meanwhile: it stays, and so does `.claude`.
    std::fs::remove_dir(&claude).unwrap();
    let given = w
        .board
        .skill_folder(&key, "plan", Some("plan-ticket"), None)
        .await
        .unwrap();
    std::fs::create_dir_all(skills.join("mine")).unwrap();
    std::fs::write(
        skills.join("mine").join("SKILL.md"),
        "---\nname: mine\n---\n",
    )
    .unwrap();
    stop().await;
    assert!(!Path::new(&given.folder).exists());
    assert!(
        skills.join("mine").join("SKILL.md").is_file(),
        "not empty, so kept"
    );
}

/// Otaq keeps no skills: the folders an older Otaq kept them in (`$OTAQ_HOME/skills/`, `skill-writes/`) are gone after
/// the service starts.
#[tokio::test(flavor = "multi_thread")]
async fn the_old_skill_folders_go_at_the_services_start() {
    let w = TestService::start().await;
    let old = w.data.root().join("skills").join("0199-copy");
    let writes = w.data.root().join("skill-writes").join("SHOP").join("plan");
    for dir in [&old, &writes] {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("SKILL.md"), "---\nname: x\n---\n").unwrap();
    }
    let w = w.restart().await;
    assert!(!w.data.root().join("skills").exists());
    assert!(!w.data.root().join("skill-writes").exists());
    w.stop().await;
}

/// Sends one raw request from inside the agent shell and returns the answer.
async fn send(
    w: &TestService,
    agent: &otaq_core::service::testing::AgentShell,
    method: &str,
    params: Value,
) -> Value {
    let request = json!({"protocol_version": otaq_core::protocol::PROTOCOL_VERSION, "id": 1, "request": {"method": method, "params": params}});
    let ran = agent
        .send_raw(&w.data.socket_path(), &request.to_string())
        .await;
    serde_json::from_str(ran.stdout.lines().next().unwrap_or_default())
        .unwrap_or_else(|e| panic!("{e}: {ran:?}"))
}

/// Writing a stage's skill is changing the flow: a ticket's agent is refused, and nothing is made for it.
#[tokio::test(flavor = "multi_thread")]
async fn a_tickets_agent_cant_write_a_stages_skill() {
    let w = TestService::start().await;
    let key = w.add_project("shop-web").await.key;
    let ticket = w.create(&key, "Split user full name").await;
    let agent = w.agent_shell(&ticket).await;
    let answer = send(
        &w,
        &agent,
        "flow.write_skill",
        json!({"project": key, "stage": "dev"}),
    )
    .await;
    assert_eq!(answer["error"]["kind"], "refused", "{answer}");
    assert!(
        answer["error"]["message"]
            .as_str()
            .unwrap()
            .contains("Only Noyan or you"),
        "{answer}"
    );
    assert!(!w.data.root().join("skill-writes").exists());
    let draft = w.board.draft(&key, &models()).await.unwrap();
    assert!(draft.writing.is_empty(), "nothing is being written");
}
