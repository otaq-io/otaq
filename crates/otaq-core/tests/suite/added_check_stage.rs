//! A check stage added in Edit flow runs: shop-web on the default flow (Dev · Validate · Review) gets the shipped
//! Security review template after Dev in its draft, the draft is saved (applied, at once: nothing is in the flow
//! yet), and a ticket started afterwards walks Dev, then Security review, where its checker runs the stage's skill.
//!
//! Every tool is the stand-in of `tests/fixtures/stand-in` (`crate::common::stand_in`). Unix only: the stand-in agent
//! tools are `sh` scripts started by name, which Windows can't run.
#![cfg(unix)]

use std::time::Duration;

use crate::common::stand_in;
use otaq_core::agents::FixedModels;
use otaq_core::board::moves::DropTarget;
use otaq_core::domain::{Actor, Place, Status};
use otaq_core::flow::draft::DraftEdit;
use otaq_core::flow::presets::presets;
use otaq_core::flow::proposal::ApplyChoices;
use otaq_core::flow::templates::{template, template_agents};
use otaq_core::flow::yaml;
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::TestService;
use otaq_core::session::{SessionKind, Start};
use otaq_core::start::StartOutcome;
use tempfile::TempDir;

fn me() -> Actor {
    Actor::User("me".into())
}

#[tokio::test(flavor = "multi_thread")]
async fn a_check_stage_added_and_saved_runs_for_the_next_ticket() {
    let dir = TempDir::new().unwrap();
    let launch = stand_in::launch(&dir.path().join("bin"), dir.path());
    let w = TestService::start_with(move |config| ServiceConfig {
        launch: Some(launch),
        ..config
    })
    .await;
    let repo = w.repos.clone_with_origin("shop-web", "main", 1);
    // The template's skill, committed in the project, where the tools read it.
    std::fs::create_dir_all(repo.join(".claude/skills/security-check")).unwrap();
    otaq_core::git::testing::commit(
        &repo,
        ".claude/skills/security-check/SKILL.md",
        "---\nname: security-check\ndescription: Looks for secrets and risky packages.\n---\n\nCheck it.\n",
    );
    w.board
        .add_project(&otaq_core::board::projects::AddProject {
            path: repo.display().to_string(),
            ..Default::default()
        })
        .await
        .unwrap();
    let default = presets()[0].read();
    w.board.use_flow_for_tests("SHOP", &default).await;

    // Edit flow: + after Dev, Security review; then Save flow.
    let mut file = default.clone();
    let security = template("security-review").unwrap().stage().clone();
    file.flow.stages.insert(1, security);
    for agent in template_agents() {
        if !file.agents.iter().any(|a| a.name == agent.name) {
            file.agents.push(agent.clone());
        }
    }
    let draft = w
        .board
        .update_draft(
            "SHOP",
            &DraftEdit::Document {
                yaml: yaml::write(&file),
            },
            &FixedModels::sample(),
        )
        .await
        .unwrap();
    assert!(
        draft.proposal.problems.is_empty(),
        "{:?}",
        draft.proposal.problems
    );
    w.board
        .apply_proposal(
            "SHOP",
            draft.proposal.id,
            &ApplyChoices::default(),
            &FixedModels::sample(),
        )
        .await
        .unwrap();
    let flow = w.board.effective_flow("SHOP").await.unwrap();
    let names: Vec<&str> = flow.stages.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["Dev", "Security review", "Validate", "Review"]);

    // A ticket started now goes through it.
    let t = w.create("SHOP", "Price table").await;
    let key = t.key.to_string();
    w.board
        .move_ticket(&key, DropTarget::ReadyForDev, true, &me())
        .await
        .unwrap();
    let outcome = w.starter.start(&key, &me(), Start::Auto).await.unwrap();
    assert!(
        matches!(outcome, StartOutcome::Started(_) | StartOutcome::Skipped),
        "{outcome:?}"
    );
    until(&w, &key, "dev", &[Status::Working]).await;
    // Dev's agent finishes its stage, between two turns.
    let dev = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            if let Some(s) = w.sessions.running_of(&t.id, SessionKind::Agent) {
                return s;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("Dev's agent runs");
    w.wait_for_output(&dev.id, "ARGS").await;
    sqlx::query("UPDATE tickets SET state = ?, state_since = ? WHERE id = ?")
        .bind(Status::Idle.as_str())
        .bind(otaq_core::store::now_ms())
        .bind(&t.id)
        .execute(w.board.store().pool())
        .await
        .unwrap();
    w.validation.submit_by_user(&key, "", &me()).await.unwrap();
    until(&w, &key, "security-review", &[Status::Checking]).await;
    let checker = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            if let [one] = w
                .sessions
                .running_all_of(&t.id, SessionKind::Validator)
                .as_slice()
            {
                return one.clone();
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("Security review's checker runs");
    let out = w.wait_for_output(&checker.id, "ARGS").await;
    assert!(out.contains("security-check"), "{out}");
    w.stop().await;
}

async fn until(w: &TestService, key: &str, stage: &str, statuses: &[Status]) {
    let reached = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let t = w.board.ticket(key).await.unwrap();
            if t.place == Place::stage_of(stage) && statuses.contains(&t.status) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await;
    assert!(reached.is_ok(), "{key} never reached {stage}");
}
