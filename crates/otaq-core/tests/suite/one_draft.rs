//! A project keeps one flow draft (FLW-03, FED-02): a Project agent's proposal lands in it, and a second one, even
//! from another tool, replaces the first's content.
//!
//! The `claude` stand-in is `sh` echoing what is typed; `OTAQ_HOME` and the repositories are temporary.
//!
//! Unix only: the stand-in agent tool is an `sh` script started by name, which Windows can't run (Q598).
#![cfg(unix)]

use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::agent_tools::{LaunchContext, Tool};
use otaq_core::agents::FixedModels;
use otaq_core::flow::presets::{SPEC_FIRST_FILE, TEAM_PR_FILE, preset};
use otaq_core::flow::proposal::{Proposal, ProposedBy};
use otaq_core::flow::propose::Proposed;
use otaq_core::project_agents::ProjectAgent;
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::TestService;
use tempfile::TempDir;

fn config(bin: &std::path::Path) -> impl FnOnce(ServiceConfig) -> ServiceConfig + use<> {
    let bin = bin.to_path_buf();
    move |config| ServiceConfig {
        launch: Some(LaunchContext {
            bin_dir: bin,
            plugin_dir: "/nonexistent/plugin".into(),
            path: std::env::var_os("PATH").unwrap_or_default(),
            claude: ClaudeLaunch {
                program: vec!["sh".into(), "-c".into(), "exec cat".into(), "sh".into()],
                config_dir: None,
            },
        }),
        ..config
    }
}

/// `agent` proposes the preset `file`'s flow, as `otaq flow propose --stdin` from its session does.
async fn propose(w: &TestService, project: &str, agent: &ProjectAgent, file: &str) -> Proposal {
    let by = ProposedBy::ProjectAgent {
        agent: agent.id.clone(),
        session: agent.session.as_ref().expect("a session").id.clone(),
        name: agent.name.clone(),
        tool: agent.tool,
    };
    w.board
        .propose(
            project,
            &Proposed::Document {
                yaml: preset(file).unwrap().yaml.to_owned(),
                file: None,
            },
            &by,
            &FixedModels::sample(),
        )
        .await
        .unwrap_or_else(|e| panic!("{file}: {e}"))
}

#[tokio::test(flavor = "multi_thread")]
async fn a_new_proposal_replaces_the_old_in_the_one_draft() {
    let tools = TempDir::new().unwrap();
    let bin = tools.path().join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let w = TestService::start_with(config(&bin)).await;
    let shop = w.add_project("shop-web").await.key;
    let a = w
        .project_agents
        .open(&shop, "/flow add a Product spec stage")
        .await
        .unwrap();
    let mut b = w
        .project_agents
        .open(&shop, "/flow try the team flow")
        .await
        .unwrap();
    // Even from another tool.
    b.tool = Tool::OpenCode;
    let first = propose(&w, &shop, &a, SPEC_FIRST_FILE).await;
    let second = propose(&w, &shop, &b, TEAM_PR_FILE).await;
    // Both land in the project's one draft: the second replaces the first's content, the agent waiting for the answer
    // is the second's.
    assert_eq!(first.id, second.id);
    assert!(
        matches!(&second.proposed_by, ProposedBy::ProjectAgent { agent, .. } if *agent == b.id)
    );
    let ids: Vec<u32> = w
        .board
        .proposals(&shop, &FixedModels::sample())
        .await
        .unwrap()
        .iter()
        .map(|p| p.id)
        .collect();
    assert_eq!(ids, [second.id]);
    w.stop().await;
}
