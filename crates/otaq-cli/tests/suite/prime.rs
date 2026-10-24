//! `otaq prime` from inside an agent session (T-0071): the current ticket's Markdown, `--json`. It is a read: it
//! clears no needs-you label and moves no last signal (d0119, T-0489).

use otaq_core::board::settings::WorkflowStep;
use otaq_core::domain::Status;
use otaq_core::service::testing::TestService;
use otaq_core::skills::testing::skill_folder;
use serde_json::Value;

const CB: &str = env!("CARGO_BIN_EXE_otaq");

#[tokio::test(flavor = "multi_thread")]
async fn prime_prints_the_context_and_changes_no_status() {
    let w = TestService::start().await;
    w.shop_sample().await;
    assert_eq!(w.status("SHOP-12").await, Status::AsksYou);
    let shop12 = w.board.ticket("SHOP-12").await.unwrap();
    let agent = w.agent_shell(&shop12).await;

    let ran = agent.run(&format!("'{CB}' prime")).await;
    assert_eq!(ran.status, 0, "{ran:?}");
    assert!(
        ran.stdout.starts_with("# SHOP-12 · Add dark mode toggle\n"),
        "{}",
        ran.stdout
    );
    assert!(ran.stdout.contains("stage next --stdin"));
    assert_eq!(
        w.status("SHOP-12").await,
        Status::AsksYou,
        "prime is a read"
    );

    let ran = agent.run(&format!("'{CB}' prime --json")).await;
    let context: Value = serde_json::from_str(&ran.stdout).unwrap();
    assert_eq!(context["ticket"]["key"], "SHOP-12");
    assert_eq!(context["commit_skill"], "/otaq-commit");
    w.stop().await;
}

/// AGM-07: Developer's imported Commit skill (its SKILL.md named `commit`, in place of the shipped one) is the skill
/// prime names, by its copy's name in the caller's tool's syntax, the same whether or not the copy is written yet.
#[tokio::test(flavor = "multi_thread")]
async fn prime_names_developers_imported_commit_skill() {
    let w = TestService::start().await;
    w.shop_sample().await;
    let folder = skill_folder(w.data.root(), "commit");
    w.import_step_skill(WorkflowStep::Commit, &folder).await;
    let shop12 = w.board.ticket("SHOP-12").await.unwrap();
    w.give_worktree(&shop12).await;
    let agent = w.agent_shell(&shop12).await;
    let ran = agent.run(&format!("'{CB}' prime")).await;
    assert_eq!(ran.status, 0, "{ran:?}");
    let rule = ran
        .stdout
        .lines()
        .find(|l| l.contains("before you end your stage"))
        .expect("the commit rule");
    assert_eq!(
        rule,
        "2. Commit everything with `/otaq-commit` before you end your stage. Never push and never open a PR: the \
         user does that."
    );
    let ran = agent.run(&format!("'{CB}' prime --json")).await;
    let context: Value = serde_json::from_str(&ran.stdout).unwrap();
    assert_eq!(context["commit_skill"], "/otaq-commit");
    w.stop().await;
}
