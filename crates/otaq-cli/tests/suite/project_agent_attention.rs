//! A Project agent waiting for the user (T-0201; PAG-07, ATT-03, ATT-06, ATT-10, ATT-11, USG-01; d0068, d0016,
//! d0042, d0020): real-shaped hook JSON fed to `otaq hook` inside its session (an ordinary `sh` started with the
//! Project agent role and no ticket). A permission prompt and `ask human` become group 1 inbox items with its name,
//! counted and notified once; never Idle; focus hides them; answering inline types into its session (tokens:
//! `otaq-core/tests/suite/project_agents.rs`). A recording notifier stands in for the OS.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use otaq_core::agent_tools::LaunchContext;
use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::attention::notify::pace::{FakeClock, QUIET};
use otaq_core::attention::notify::{Notifier, OnClick, OsNotification};
use otaq_core::board::questions::Choice;
use otaq_core::client::{Client, ClientConfig};
use otaq_core::data_dir::DataDir;
use otaq_core::domain::Status;
use otaq_core::git::testing::TempRepos;
use otaq_core::protocol::{FocusParams, Request};
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::{AgentShell, TestService};
use serde_json::{Value, json};

const CB: &str = env!("CARGO_BIN_EXE_otaq");

#[derive(Default)]
struct Recorder(Mutex<Vec<OsNotification>>);

impl Notifier for Recorder {
    fn post(&self, n: &OsNotification, _: OnClick) {
        self.0.lock().unwrap().push(n.clone());
    }
}

async fn world(recorder: Arc<Recorder>, clock: Arc<FakeClock>) -> TestService {
    let home = tempfile::TempDir::new().unwrap();
    let data = home.path().join("h");
    let script = format!(
        "export {}='{}'; export PS1=''; exec sh",
        DataDir::env_var(),
        data.display()
    );
    let notifier: Arc<dyn Notifier> = recorder;
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
        notifier,
        clock,
        ..config
    })
    .await;
    w.add_project("shop-web").await;
    w
}

async fn hook(agent: &AgentShell, event: &str, fields: Value) {
    let mut payload = json!({"session_id": "0199d2a4-3b1c-7c3e-9f00-123456789abc", "cwd": "/src/shop-web",
                             "permission_mode": "default", "hook_event_name": event});
    for (k, v) in fields.as_object().unwrap() {
        payload[k] = v.clone();
    }
    let ran = agent
        .run_with_stdin(
            &format!("'{CB}' hook --tool claude {event}"),
            &payload.to_string(),
        )
        .await;
    assert_eq!(ran.status, 0, "{ran:?}");
}

/// Waits until the inbox's Project agent items match `want` (statuses), 5 s at most.
async fn agent_items(w: &TestService, want: &[Status]) -> otaq_core::attention::inbox::InboxList {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let list = w.inbox.list().await.unwrap();
            let got: Vec<Status> = list.project_agents.iter().map(|i| i.status).collect();
            if got == want {
                return list;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("the inbox never showed {want:?}"))
}

async fn titles(recorder: &Recorder, n: usize) -> Vec<(String, String, String)> {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let got: Vec<(String, String, String)> = recorder
                .0
                .lock()
                .unwrap()
                .iter()
                .map(|n| (n.title.clone(), n.subtitle.clone(), n.body.clone()))
                .collect();
            if got.len() >= n {
                return got;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("{n} notifications never came"))
}

#[tokio::test(flavor = "multi_thread")]
async fn its_permission_prompt_and_question_are_group_one_items_with_its_name() {
    let recorder = Arc::new(Recorder::default());
    let clock = Arc::new(FakeClock::new());
    let w = world(recorder.clone(), clock.clone()).await;
    let opened = w
        .project_agents
        .open("SHOP", "Look around the checkout code")
        .await
        .unwrap();
    w.project_agents
        .rename(&opened.id, "Import GitHub issues")
        .await
        .unwrap();
    let agent = w.shell_in(&opened.session.as_ref().unwrap().id);

    // A turn ends: never Idle, no item.
    hook(
        &agent,
        "UserPromptSubmit",
        json!({"prompt": "Import the issues labelled ready"}),
    )
    .await;
    hook(&agent, "Stop", json!({"last_assistant_message": "Done."})).await;
    agent_items(&w, &[]).await;

    // A permission prompt: Needs you, with the project and its name; counted; notified once.
    hook(&agent, "UserPromptSubmit", json!({"prompt": "go on"})).await;
    hook(
        &agent,
        "Notification",
        json!({"notification_type": "permission_prompt", "message": "Allow gh issue list?",
               "title": "Claude needs your permission"}),
    )
    .await;
    let list = agent_items(&w, &[Status::NeedsYou]).await;
    let item = &list.project_agents[0];
    assert_eq!(
        (
            item.project.as_str(),
            item.project_name.as_str(),
            item.name.as_str()
        ),
        ("SHOP", "shop-web", "Import GitHub issues")
    );
    assert_eq!(item.reason, "permission: Allow gh issue list?");
    assert_eq!(list.counts.total, 1);
    assert_eq!(list.counts.of_project("SHOP"), 1);
    assert_eq!(
        titles(&recorder, 1).await,
        [(
            "Project agent needs permission".to_owned(),
            "shop-web · Import GitHub issues".to_owned(),
            "Allow gh issue list?".to_owned()
        )]
    );

    // A read is not it acting (d0119): the item stays. Its turn moving on means the prompt was answered: the item
    // goes.
    let ran = agent.run(&format!("'{CB}' prime --project")).await;
    assert_eq!(ran.status, 0, "{ran:?}");
    agent_items(&w, &[Status::NeedsYou]).await;
    hook(
        &agent,
        "Stop",
        json!({"last_assistant_message": "Listed them."}),
    )
    .await;
    agent_items(&w, &[]).await;

    // `ask human` after the quiet period (d0090): Asks you with the question; answered inline, typed into its
    // session.
    clock.advance(QUIET);
    let ran = agent
        .run_with_stdin(
            &format!("'{CB}' ask human --stdin --option Yes --option No"),
            "Import closed issues too?",
        )
        .await;
    assert_eq!(ran.status, 0, "{ran:?}");
    let list = agent_items(&w, &[Status::AsksYou]).await;
    assert_eq!(
        list.project_agents[0].reason,
        "asks: Import closed issues too?"
    );
    assert_eq!(
        list.project_agents[0].question.as_ref().unwrap().options,
        ["Yes", "No"]
    );
    let all = titles(&recorder, 2).await;
    assert_eq!(
        all[1],
        (
            "Project agent asks a question".to_owned(),
            "shop-web · Import GitHub issues".to_owned(),
            "Import closed issues too?".to_owned()
        )
    );
    w.project_agents
        .answer(&opened.id, &Choice::Option(2), false)
        .await
        .unwrap();
    w.wait_for_output(
        &opened.session.as_ref().unwrap().id,
        "Answer to your question: No (option 2).",
    )
    .await;
    agent_items(&w, &[]).await;
    assert_eq!(
        recorder.0.lock().unwrap().len(),
        2,
        "each wait notified once"
    );
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn focus_hides_its_items() {
    let recorder = Arc::new(Recorder::default());
    let clock = Arc::new(FakeClock::new());
    let w = world(recorder.clone(), clock.clone()).await;
    let opened = w
        .project_agents
        .open("SHOP", "Look around the checkout code")
        .await
        .unwrap();
    let session = opened.session.as_ref().unwrap().id.clone();
    let agent = w.shell_in(&session);

    // Its terminal has focus: the prompt raises no item and no notification (ATT-03).
    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    client
        .request(Request::SessionFocus(FocusParams {
            session: session.clone(),
            focused: true,
        }))
        .await
        .unwrap();
    assert!(w.project_agents.get(&opened.id).await.unwrap().focused);
    hook(
        &agent,
        "Notification",
        json!({"notification_type": "permission_prompt", "message": "Allow curl?"}),
    )
    .await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    agent_items(&w, &[]).await;
    assert!(recorder.0.lock().unwrap().is_empty());
    // It leaves the terminal: the item appears.
    client
        .request(Request::SessionFocus(FocusParams {
            session: session.clone(),
            focused: false,
        }))
        .await
        .unwrap();
    agent_items(&w, &[Status::NeedsYou]).await;
    titles(&recorder, 1).await;

    w.stop().await;
}
