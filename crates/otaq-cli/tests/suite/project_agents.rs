//! Project agents from inside their session (T-0199; PAG-01, PAG-04; d0068): `claude` stands in as an ordinary
//! `sh` started the way a Project agent is (the role, no ticket), so `otaq agent name` and `otaq hook …`
//! typed into it reach the service as that Project agent (CLI-01). No test starts claude.

use std::process::Command;

use otaq_core::agent_tools::LaunchContext;
use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::data_dir::DataDir;
use otaq_core::git::testing::TempRepos;
use otaq_core::naming;
use otaq_core::project_agents::{NEW_AGENT, NameSource, ONLY_A_PROJECT_AGENT};
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::{AgentShell, TestService};
use serde_json::{Value, json};

const CB: &str = env!("CARGO_BIN_EXE_otaq");

/// A service whose `claude` is an interactive `sh` in the data folder of `home`.
async fn world() -> TestService {
    let home = tempfile::TempDir::new().unwrap();
    let data = home.path().join("h");
    let script = format!(
        "export {}='{}'; export PS1=''; exec sh",
        DataDir::env_var(),
        data.display()
    );
    let w = TestService::start_in(home, TempRepos::new(), |config| ServiceConfig {
        launch: Some(LaunchContext {
            bin_dir: "/nonexistent".into(),
            plugin_dir: "/nonexistent/plugin".into(),
            path: std::env::var_os("PATH").unwrap_or_default(),
            claude: ClaudeLaunch {
                program: vec!["sh".into(), "-c".into(), script, "sh".into()],
                config_dir: None,
            },
        }),
        ..config
    })
    .await;
    w.add_project("shop-web").await;
    w
}

async fn hook(agent: &AgentShell, event: &str, payload: Value) {
    let ran = agent
        .run_with_stdin(
            &format!("'{CB}' hook --tool claude {event}"),
            &payload.to_string(),
        )
        .await;
    assert_eq!(ran.status, 0, "{ran:?}");
    assert_eq!(ran.stdout, "", "hooks print nothing");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_project_agent_names_itself_once_and_nobody_else_can() {
    let w = world().await;
    let opened = w
        .project_agents
        .open("SHOP", "Look around the checkout code")
        .await
        .unwrap();
    let agent = w.shell_in(&opened.session.as_ref().unwrap().id);

    let ran = agent
        .run(&format!("'{CB}' agent name 'Checkout slowness'"))
        .await;
    assert_eq!(ran.status, 0, "{ran:?}");
    assert_eq!(ran.stdout.trim(), "Named \"Checkout slowness\".");
    let named = w.project_agents.get(&opened.id).await.unwrap();
    assert_eq!(named.name, "Checkout slowness");
    assert_eq!(named.name_source, Some(NameSource::Agent));

    let ran = agent
        .run(&format!("'{CB}' agent name 'Other words' --json"))
        .await;
    assert_ne!(ran.status, 0);
    let e: Value = serde_json::from_str(&ran.stdout).unwrap();
    assert!(
        e["error"]["message"]
            .as_str()
            .unwrap()
            .starts_with("This Project agent is already named \"Checkout slowness\""),
        "{e}"
    );
    let ran = agent
        .run(&format!("'{CB}' agent name 'Checkout queries' --rename"))
        .await;
    assert_eq!(ran.status, 0, "{ran:?}");

    // The user's own terminal and a ticket's agent can't.
    let out = Command::new(CB)
        .args(["agent", "name", "Mine", "--json"])
        .env(DataDir::env_var(), w.data.root())
        .env_remove(naming::ticket_env_var())
        .env_remove(naming::role_env_var())
        .current_dir(w.home.path())
        .output()
        .unwrap();
    assert!(!out.status.success());
    let e: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(e["error"]["message"], ONLY_A_PROJECT_AGENT);
    let t = w.create("SHOP", "Cart badge").await;
    let ticket_agent = w.agent_shell(&t).await;
    let ran = ticket_agent
        .run(&format!("'{CB}' agent name 'Mine' --json"))
        .await;
    assert_ne!(ran.status, 0);
    let e: Value = serde_json::from_str(&ran.stdout).unwrap();
    assert_eq!(e["error"]["message"], ONLY_A_PROJECT_AGENT);
    assert_eq!(
        w.project_agents.get(&opened.id).await.unwrap().name,
        "Checkout queries"
    );
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn its_hooks_keep_the_first_request_and_name_it_after_the_first_turn() {
    let w = world().await;
    let opened = w
        .project_agents
        .open("SHOP", "Import the GitHub issues labelled ready")
        .await
        .unwrap();
    let agent = w.shell_in(&opened.session.as_ref().unwrap().id);
    let common = json!({"session_id": opened.tool_session_id, "cwd": "/src/shop-web",
                        "permission_mode": "default"});
    let with = |event: &str, fields: Value| {
        let mut payload = common.clone();
        payload["hook_event_name"] = json!(event);
        for (k, v) in fields.as_object().unwrap() {
            payload[k] = v.clone();
        }
        payload
    };
    // Its first turn: the skill with the message it opened with (d0087), the first request since open.
    let a = w.project_agents.get(&opened.id).await.unwrap();
    assert_eq!((a.name.as_str(), a.got_request), (NEW_AGENT, true));
    hook(
        &agent,
        "UserPromptSubmit",
        with(
            "UserPromptSubmit",
            json!({"prompt": format!(
                "{} Import the GitHub issues labelled ready",
                format!("/{}", naming::copy_name("noyan"))
            )}),
        ),
    )
    .await;
    let a = w.project_agents.get(&opened.id).await.unwrap();
    assert!(a.got_request && a.mid_turn);
    hook(
        &agent,
        "Stop",
        with(
            "Stop",
            json!({"last_assistant_message": "Created 3 tickets."}),
        ),
    )
    .await;
    let a = w.project_agents.get(&opened.id).await.unwrap();
    assert_eq!(a.name, "Import the GitHub issues");
    assert_eq!(a.name_source, Some(NameSource::Fallback));
    assert!(!a.mid_turn);
    w.stop().await;
}

/// T-0200 (PAG-02, PAG-03, CLI-06): `prime --project`, `create`, and `ask human` from inside a Project agent.
#[tokio::test(flavor = "multi_thread")]
async fn a_project_agent_primes_creates_and_asks_the_user() {
    let w = world().await;
    let opened = w
        .project_agents
        .open("SHOP", "Look around the checkout code")
        .await
        .unwrap();
    let agent = w.shell_in(&opened.session.as_ref().unwrap().id);

    let ran = agent.run(&format!("'{CB}' prime --project")).await;
    assert_eq!(ran.status, 0, "{ran:?}");
    assert!(
        ran.stdout
            .starts_with("# Project agent · shop-web (SHOP)\n"),
        "{ran:?}"
    );

    let ran = agent
        .run_with_stdin(
            &format!("'{CB}' create 'Slow checkout query' --stdin --json"),
            "The cart query runs once per item.",
        )
        .await;
    assert_eq!(ran.status, 0, "{ran:?}");
    let created: Value = serde_json::from_str(&ran.stdout).unwrap();
    assert_eq!(
        created["created_by"],
        format!("project-agent:{}", opened.id),
        "{created}"
    );

    let ran = agent
        .run_with_stdin(
            &format!("'{CB}' ask human --stdin --option Yes --option No --recommended 2"),
            "Import closed issues too?",
        )
        .await;
    assert_eq!(ran.status, 0, "{ran:?}");
    assert_eq!(
        ran.stdout.trim(),
        "The user is asked. End your turn and wait for the answer."
    );
    let question = w
        .project_agents
        .get(&opened.id)
        .await
        .unwrap()
        .question
        .unwrap();
    assert_eq!(question.options, ["Yes", "No"]);
    assert_eq!(question.recommended, Some(2));
    w.stop().await;
}
