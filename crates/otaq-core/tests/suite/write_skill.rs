//! A stage's skill the Project agent writes (T-0472; FED-06; d0115, d0092, d0023): a new Plan stage with no skill
//! keeps Apply off ("Plan needs a skill"); `flow.write_skill` gives the agent a folder in Otaq's own data folder, and
//! handing it in imports it as Otaq's copy under `$OTAQ_HOME/skills/`, with nothing written in the project's checkout
//! or a worktree; Apply gives the copy to the stage's agent without copying it again; Undo deletes it, Stop drops the
//! folder; a ticket's agent can't write one.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use otaq_core::agents::{FixedModels, SkillRef};
use otaq_core::flow::draft::DraftEdit;
use otaq_core::flow::proposal::ApplyChoices;
use otaq_core::flow::{FlowFile, Stage, StageKind, yaml};
use otaq_core::service::testing::TestService;
use serde_json::{Value, json};

fn models() -> FixedModels {
    FixedModels::sample()
}

/// Every file under `root` with its contents (`.git` included): what a test compares before and after.
fn files(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut todo = vec![root.to_owned()];
    while let Some(dir) = todo.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                todo.push(path);
            } else {
                out.insert(path.clone(), std::fs::read(&path).unwrap_or_default());
            }
        }
    }
    out
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
            column: Default::default(),
            agent: Some("Developer".into()),
            skill: None,
            options: Vec::new(),
            rounds: None,
            delivery: None,
            why: None,
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

fn skill_copies(w: &TestService) -> Vec<String> {
    let mut ids: Vec<String> = std::fs::read_dir(w.data.skills_dir())
        .map(|entries| {
            entries
                .flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    ids.sort();
    ids
}

#[tokio::test(flavor = "multi_thread")]
async fn a_written_skill_is_otaqs_copy_never_in_the_repo() {
    let w = TestService::start().await;
    let project = w.add_project("shop-web").await;
    let key = project.key.clone();
    let checkout = PathBuf::from(&project.repo_path);
    let before = files(&checkout);

    // A new Agent stage with no skill: the draft holds it, Apply stays off with the reason.
    with_plan(&w, &key).await;
    let draft = w.board.draft(&key, &models()).await.unwrap();
    let messages: Vec<&str> = draft
        .proposal
        .problems
        .iter()
        .map(|p| p.message.as_str())
        .collect();
    assert_eq!(messages, ["Plan needs a skill"]);
    let refused = w
        .board
        .apply_proposal(&key, draft.proposal.id, &ApplyChoices::default(), &models())
        .await
        .unwrap_err();
    assert!(
        refused.to_string().contains("Plan needs a skill"),
        "{refused}"
    );

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
    assert_eq!(draft.writing[0].agent.as_deref(), Some("pa-1"));

    // The agent's folder is Otaq's own, outside the checkout.
    let given = w.board.skill_folder(&key, "Plan", None).await.unwrap();
    let folder = PathBuf::from(&given.folder);
    assert!(folder.starts_with(w.data.root()), "{}", folder.display());
    assert!(!folder.starts_with(&checkout));
    assert!(folder.is_dir());
    assert!(
        given.text.contains("flow write-skill plan --done"),
        "{}",
        given.text
    );

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
    assert!(!folder.exists(), "the folder the agent wrote in goes");
    let copies = skill_copies(&w);
    assert_eq!(copies.len(), 1, "one copy, Otaq's");
    let copy = w.data.skill_path(&copies[0]).unwrap();
    assert!(copy.join("SKILL.md").is_file());

    // The draft names it, tinted as written; the stage's agent brings the copy, and Apply is on.
    let draft = w.board.draft(&key, &models()).await.unwrap();
    assert!(draft.writing.is_empty());
    assert_eq!(draft.written.len(), 1);
    assert_eq!(draft.written[0].skill, "plan-ticket");
    assert_eq!(draft.written[0].by, "Checkout slowness");
    assert_eq!(draft.written[0].copy, copies[0]);
    let file = yaml::read(&draft.proposal.yaml).unwrap();
    assert_eq!(
        file.flow.stage("plan").unwrap().skill.as_deref(),
        Some("plan-ticket")
    );
    let developer = file.agents.iter().find(|a| a.name == "Developer").unwrap();
    assert!(
        developer.skills.contains(&copy.display().to_string()),
        "{developer:?}"
    );
    assert!(
        draft.proposal.problems.is_empty(),
        "{:?}",
        draft.proposal.problems
    );

    // Nothing was written in the checkout, and no worktree exists for it.
    assert_eq!(files(&checkout), before, "the checkout is untouched");
    assert!(
        files(&w.data.worktrees_dir()).is_empty(),
        "no worktree holds it"
    );

    // A restart's clean-up keeps the copy the open draft names.
    otaq_core::agents::skills::remove_unreferenced(&w.board)
        .await
        .unwrap();
    assert_eq!(skill_copies(&w), copies);

    // Apply: the Developer holds that very copy (not copied again), and the stage runs it.
    let applied = w
        .board
        .apply_proposal(&key, draft.proposal.id, &ApplyChoices::default(), &models())
        .await
        .unwrap();
    assert!(
        applied.agents.contains(&"Developer".to_owned()),
        "{applied:?}"
    );
    assert_eq!(skill_copies(&w), copies, "the same copy, no second one");
    let developer = w
        .board
        .store()
        .agent_named("Developer")
        .await
        .unwrap()
        .unwrap();
    assert!(
        developer
            .skills
            .contains(&SkillRef::Imported(copies[0].clone())),
        "{developer:?}"
    );
    assert_eq!(
        developer.imported_skill(&copies[0]).unwrap().name,
        "plan-ticket"
    );
    let flow = w.board.effective_flow(&key).await.unwrap();
    assert_eq!(
        flow.stage("plan").unwrap().skill.as_deref(),
        Some("plan-ticket")
    );
    assert_eq!(files(&checkout), before, "still untouched");
}

#[tokio::test(flavor = "multi_thread")]
async fn undo_deletes_the_copy_and_stop_drops_the_folder() {
    let w = TestService::start().await;
    let key = w.add_project("shop-web").await.key;
    with_plan(&w, &key).await;

    let given = w.board.skill_folder(&key, "plan", None).await.unwrap();
    write_plan_skill(Path::new(&given.folder));
    w.board.skill_written(&key, "plan", "you").await.unwrap();
    assert_eq!(skill_copies(&w).len(), 1);

    // Undo: Plan has no skill again, the copy is gone, Apply waits again.
    let draft = w
        .board
        .update_draft(
            &key,
            &DraftEdit::UndoSkill {
                stage: "plan".into(),
            },
            &models(),
        )
        .await
        .unwrap();
    assert!(draft.written.is_empty());
    assert!(skill_copies(&w).is_empty(), "Otaq's copy is deleted");
    let file = yaml::read(&draft.proposal.yaml).unwrap();
    assert_eq!(file.flow.stage("plan").unwrap().skill, None);
    assert!(
        file.agents
            .iter()
            .all(|a| a.skills.iter().all(|s| !s.starts_with('/'))),
        "no agent brings it"
    );
    assert_eq!(draft.proposal.problems[0].message, "Plan needs a skill");

    // Stop: the field is empty again and the agent's folder goes; handing in then has nothing to import.
    let given = w
        .board
        .skill_folder(&key, "plan", Some("pa-1"))
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
        .skill_folder(&key, "review", None)
        .await
        .unwrap_err();
    assert!(none.to_string().contains("runs no agent"), "{none}");
}

/// Sends one raw request from inside the agent shell and returns the answer.
async fn send(
    w: &TestService,
    agent: &otaq_core::service::testing::AgentShell,
    method: &str,
    params: Value,
) -> Value {
    let request =
        json!({"protocol_version": 1, "id": 1, "request": {"method": method, "params": params}});
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
}
