//! A ticket an agent created (T-0500, #47; d0016, d0083, d0091): `otaq create` from a ticket's agent or an epic lead
//! (sent from inside its session, an ordinary `sh` running `nc`) tells the app with `created_by_agent`, naming the
//! creator and where the ticket landed, and the creating ticket's History logs it. The user's and a Project agent's
//! creations tell nothing (its pane lists them).

use otaq_core::agent_tools::LaunchContext;
use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::board::tickets::EventKind;
use otaq_core::client::{Client, ClientConfig};
use otaq_core::domain::{Actor, Status};
use otaq_core::git::testing::TempRepos;
use otaq_core::protocol::{Event, Request};
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::{AgentShell, TestService, path_without_build_folders};
use serde_json::{Value, json};
use tokio::sync::broadcast::Receiver;

/// The sample board, with `claude` an interactive `sh` in the service's data folder (for the lead and the Project
/// agent).
async fn world() -> TestService {
    let home = tempfile::TempDir::new().unwrap();
    let data = home.path().join("h");
    let script = format!(
        "export {}='{}'; export PS1=''; exec sh",
        otaq_core::data_dir::DataDir::env_var(),
        data.display()
    );
    let w = TestService::start_in(home, TempRepos::new(), |config| ServiceConfig {
        launch: Some(LaunchContext {
            bin_dir: "/nonexistent".into(),
            plugin_dir: "/nonexistent/plugin".into(),
            path: path_without_build_folders().into(),
            claude: ClaudeLaunch {
                program: vec!["sh".into(), "-c".into(), script, "sh".into()],
                config_dir: None,
            },
        }),
        ..config
    })
    .await;
    w.shop_sample().await;
    w
}

/// `ticket.create` sent from inside `shell`'s session; the new ticket's key.
async fn create_from(w: &TestService, shell: &AgentShell, params: Value) -> String {
    let request = json!({"protocol_version": 1, "id": 1,
                         "request": {"method": "ticket.create", "params": params}});
    let ran = shell
        .send_raw(&w.data.socket_path(), &request.to_string())
        .await;
    let answer: Value = serde_json::from_str(ran.stdout.lines().next().unwrap_or_default())
        .unwrap_or_else(|e| panic!("{e}: {ran:?}"));
    assert!(answer.get("error").is_none(), "{answer}");
    answer["reply"]["ticket"]["key"]
        .as_str()
        .unwrap()
        .to_owned()
}

/// The `created_by_agent` events published so far (the service publishes them before it answers).
fn created(events: &mut Receiver<Event>) -> Vec<(String, String, Status)> {
    let mut out = Vec::new();
    while let Ok(event) = events.try_recv() {
        if let Event::CreatedByAgent {
            key, by, status, ..
        } = event
        {
            out.push((key.to_string(), by, status));
        }
    }
    out
}

#[tokio::test(flavor = "multi_thread")]
async fn an_agent_s_and_a_lead_s_new_tickets_tell_the_app_the_user_s_and_a_project_agent_s_dont() {
    let w = world().await;
    let mut events = w.events.subscribe();

    // DEMO-7's developer files the fix ticket: the app is told, and SHOP-19's History says so.
    let creator = w.board.ticket("SHOP-19").await.unwrap();
    let shell = w.agent_shell(&creator).await;
    let key = create_from(
        &w,
        &shell,
        json!({"project": "SHOP", "title": "Fix the broken build"}),
    )
    .await;
    assert_eq!(
        created(&mut events),
        [(key.clone(), "SHOP-19".to_owned(), Status::Backlog)]
    );
    let new = w.board.ticket(&key).await.unwrap();
    assert_eq!(new.created_by, Actor::Agent(creator.key.clone()));
    let logged = w
        .board
        .history("SHOP-19")
        .await
        .unwrap()
        .into_iter()
        .find(|e| e.kind == EventKind::CreatedTicket)
        .expect("logged on the creator");
    assert_eq!(logged.actor, Actor::Agent(creator.key.clone()));
    assert_eq!(
        logged.payload,
        json!({"child": key, "title": "Fix the broken build", "to": "backlog"})
    );

    // An epic lead's new child: named by the lead.
    let lead = w.project_agents.leads().open("SHOP-40").await.unwrap();
    let lead_shell = w.shell_in(&lead.agent.session.as_ref().unwrap().id);
    let child = create_from(
        &w,
        &lead_shell,
        json!({"project": "SHOP", "title": "Wishlist empty state", "epic": "SHOP-40"}),
    )
    .await;
    assert_eq!(
        created(&mut events),
        [(child, "Wishlist lead".to_owned(), Status::Backlog)]
    );

    // A Project agent's: its pane's "Created" list has it, no toast.
    let agent = w
        .project_agents
        .open("SHOP", "Audit the footer")
        .await
        .unwrap();
    let agent_shell = w.shell_in(&agent.session.as_ref().unwrap().id);
    create_from(
        &w,
        &agent_shell,
        json!({"project": "SHOP", "title": "Footer links"}),
    )
    .await;
    assert_eq!(created(&mut events), []);

    // The user's own.
    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    let request: Request = serde_json::from_value(json!({"method": "ticket.create",
        "params": {"project": "SHOP", "title": "Made by me"}}))
    .unwrap();
    client.request(request).await.unwrap();
    assert_eq!(created(&mut events), []);
    w.stop().await;
}
