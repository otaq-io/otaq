//! A child a sibling's merge freed waits for the epic lead's turn about that merge (T-0496; LEAD-04; d0124, d0083,
//! d0086, d0112): "Waiting for the lead's update" until the turn the wake-up was typed in ends (its Stop) or the lead
//! ends. A turn the user started while the wake-up waited isn't it. With no live lead, after a restart too, it starts
//! at once; a pause still wins. The lead's hooks are sent as `otaq hook` sends them, over the socket from inside its
//! session (an interactive `sh` stands in for `claude`); no test starts `claude`.

use std::path::Path;
use std::time::Duration;

use otaq_core::agent_tools::LaunchContext;
use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::agents::pause::PauseScope;
use otaq_core::board::tickets::{NewTicketInput, Ticket};
use otaq_core::domain::{Actor, Kind, Status};
use otaq_core::git::testing::{TempRepos, commit};
use otaq_core::project_agents::lead::Lead;
use otaq_core::review::LocalMergeOutcome;
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::{AgentShell, TestService, path_without_build_folders};
use serde_json::{Value, json};

fn me() -> Actor {
    Actor::User("me".into())
}

/// `claude` is an interactive `sh`, for the lead and for the children's agents.
fn with_sh(data: &Path) -> impl FnOnce(ServiceConfig) -> ServiceConfig {
    let script = format!(
        "export {}='{}'; export PS1=''; exec sh",
        otaq_core::data_dir::DataDir::env_var(),
        data.display()
    );
    move |config| ServiceConfig {
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
    }
}

/// The epic SHOP-1 Wishlist: SHOP-1.1 Wishlist API, and SHOP-1.2 Wishlist page, queued and waiting for 1.1.
struct World {
    w: TestService,
    api: Ticket,
    page: Ticket,
}

async fn world() -> World {
    let home = tempfile::TempDir::new().unwrap();
    let data = home.path().join("h");
    let w = TestService::start_in(home, TempRepos::new(), with_sh(&data)).await;
    w.add_project("shop-web").await;
    let epic = w
        .board
        .create_ticket(
            &NewTicketInput {
                project: "SHOP".into(),
                title: "Wishlist".into(),
                kind: Kind::Epic,
                ..Default::default()
            },
            &me(),
        )
        .await
        .unwrap();
    let child = async |title: &str| {
        w.board
            .create_ticket(
                &NewTicketInput {
                    project: "SHOP".into(),
                    title: title.into(),
                    epic: Some(epic.key.to_string()),
                    ..Default::default()
                },
                &me(),
            )
            .await
            .unwrap()
    };
    let api = child("Wishlist API").await;
    let page = child("Wishlist page").await;
    w.board
        .add_dependency(&page.key.to_string(), &api.key.to_string(), &me())
        .await
        .unwrap();
    w.board
        .queue_epic_child(&page.key.to_string(), &me())
        .await
        .unwrap();
    assert_eq!(w.status("SHOP-1.2").await, Status::Queued);
    World { w, api, page }
}

async fn open_lead(w: &TestService) -> (Lead, AgentShell) {
    let lead = w.project_agents.leads().open("SHOP-1").await.unwrap();
    let shell = w.shell_in(&session(&lead));
    (lead, shell)
}

fn session(lead: &Lead) -> String {
    lead.agent.session.as_ref().expect("a session").id.clone()
}

/// Works on SHOP-1.1 and Local-merges it into the epic branch: which tickets the merge started.
async fn merge_api(w: &World) -> Vec<String> {
    let wt = w.w.start_worktree(&w.api).await;
    commit(&wt, "api.ts", "api\n");
    w.w.force_status(&w.api, Status::WaitingForYou).await;
    match w
        .w
        .review
        .local_merge("SHOP-1.1", "SHOP-1.1: Wishlist API", false, &me())
        .await
        .unwrap()
    {
        LocalMergeOutcome::Merged { started, .. } => {
            started.into_iter().map(|k| k.to_string()).collect()
        }
        outcome => panic!("{outcome:?}"),
    }
}

const WAKE: &str = "SHOP-1.1 merged: Wishlist API";

/// A Claude Code hook as `otaq hook --tool claude <event>` sends it from inside the lead's session.
async fn hook(w: &TestService, lead: &AgentShell, event: &str, payload: Value) {
    let request = json!({"protocol_version": 1, "id": 1, "request": {"method": "hook", "params": {
        "tool": "claude", "event": event, "payload": payload,
    }}});
    let ran = lead
        .send_raw(&w.data.socket_path(), &request.to_string())
        .await;
    let answer: Value = serde_json::from_str(ran.stdout.lines().next().unwrap_or_default())
        .unwrap_or_else(|e| panic!("{e}: {ran:?}"));
    assert!(answer.get("error").is_none(), "{event}: {answer}");
}

/// The lead's prompt was submitted: its turn starts.
async fn prompt(w: &TestService, lead: &AgentShell, text: &str) {
    let mut payload = fixture("UserPromptSubmit");
    payload["prompt"] = text.into();
    hook(w, lead, "UserPromptSubmit", payload).await;
}

/// The lead's turn ended.
async fn stop(w: &TestService, lead: &AgentShell) {
    let mut payload = fixture("Stop-idle");
    payload["last_assistant_message"] = "Updated the description of SHOP-1.2.".into();
    hook(w, lead, "Stop", payload).await;
}

/// The lead's turn ended and the waiting wake-up is typed (before the next line goes into its shell).
async fn stop_and_type(w: &TestService, lead: &AgentShell) {
    stop(w, lead).await;
    eventually("the wake-up typed", async || {
        w.deliveries
            .pending(&lead.session)
            .await
            .unwrap()
            .is_empty()
    })
    .await;
    w.wait_for_output(&lead.session, WAKE).await;
}

fn fixture(file: &str) -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/claude/hooks")
        .join(format!("{file}.json"));
    serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap()
}

async fn eventually(what: &str, mut check: impl AsyncFnMut() -> bool) {
    tokio::time::timeout(Duration::from_secs(20), async {
        while !check().await {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("{what} never happened"));
}

/// SHOP-1.2 waits: a pass now starts nothing, and its card says why (`lead`: for the lead's update).
async fn page_waits(w: &World, lead: bool) {
    let plan = w.w.scheduler.pass().await.unwrap();
    assert!(plan.start.is_empty(), "{plan:?}");
    let page = w.w.board.ticket("SHOP-1.2").await.unwrap();
    assert_eq!(page.status, Status::Queued);
    assert_eq!(page.flags.waiting_for_lead, lead);
    assert!(
        otaq_core::board::readiness::readiness(&page, &w.w.board.tickets("SHOP").await.unwrap())
            .is_ready()
    );
}

async fn page_starts(w: &World) {
    eventually("SHOP-1.2 starts", async || {
        w.w.status("SHOP-1.2").await == Status::Working
    })
    .await;
    assert!(
        !w.w.board
            .ticket("SHOP-1.2")
            .await
            .unwrap()
            .flags
            .waiting_for_lead
    );
}

async fn stop_all(w: TestService) {
    for s in w
        .sessions
        .in_memory()
        .into_iter()
        .filter(|s| s.is_running())
    {
        let _ = w.sessions.stop(&s.id).await;
    }
    w.stop().await;
}

/// The lead is busy with the user's turn when 1.1 merges: 1.2 waits through the end of that turn (the wake-up is
/// typed then), through the merge's own turn, and starts when that turn's Stop arrives.
#[tokio::test(flavor = "multi_thread")]
async fn a_freed_child_waits_for_the_leads_turn_about_the_merge() {
    let w = world().await;
    let (_lead, shell) = open_lead(&w.w).await;
    prompt(&w.w, &shell, "Plan it").await;

    assert!(merge_api(&w).await.is_empty(), "nothing started");
    page_waits(&w, true).await;

    // The user's turn ends: the wake-up is typed, but that turn wasn't about the merge.
    stop_and_type(&w.w, &shell).await;
    page_waits(&w, true).await;

    // The merge's turn: still waiting while it runs, and its Stop lets 1.2 start.
    prompt(&w.w, &shell, WAKE).await;
    page_waits(&w, true).await;
    stop(&w.w, &shell).await;
    page_starts(&w).await;
    let _ = &w.page;
    stop_all(w.w).await;
}

/// No lead: the merge starts 1.2 at once, as before.
#[tokio::test(flavor = "multi_thread")]
async fn with_no_lead_the_freed_child_starts_at_once() {
    let w = world().await;
    assert_eq!(merge_api(&w).await, ["SHOP-1.2"]);
    page_starts(&w).await;
    stop_all(w.w).await;
}

/// The lead ends while 1.2 waits: 1.2 starts.
#[tokio::test(flavor = "multi_thread")]
async fn a_lead_that_ends_releases_the_waiting_child() {
    let w = world().await;
    let (lead, shell) = open_lead(&w.w).await;
    prompt(&w.w, &shell, "Plan it").await;
    assert!(merge_api(&w).await.is_empty());
    page_waits(&w, true).await;
    w.w.sessions.stop(&session(&lead)).await.unwrap();
    page_starts(&w).await;
    stop_all(w.w).await;
}

/// A restart ends the lead ("Ended by restart"): nothing waits for it, and 1.2 starts.
#[tokio::test(flavor = "multi_thread")]
async fn after_a_restart_nothing_waits_for_the_lead() {
    let World { w, api, page } = world().await;
    let (_lead, shell) = open_lead(&w).await;
    prompt(&w, &shell, "Plan it").await;
    let world = World { w, api, page };
    assert!(merge_api(&world).await.is_empty());
    page_waits(&world, true).await;
    let World { w, api, page } = world;
    let data = w.data.root().to_path_buf();
    let w = w.restart_with(with_sh(&data)).await;
    let world = World { w, api, page };
    page_starts(&world).await;
    stop_all(world.w).await;
}

/// Paused: the lead's Stop releases 1.2 from the lead, and the pause still holds it until the user's Resume.
#[tokio::test(flavor = "multi_thread")]
async fn a_pause_wins_over_the_leads_turn() {
    let w = world().await;
    let shop = PauseScope::Project {
        project: "SHOP".into(),
    };
    w.w.pause.pause(&shop).await.unwrap();
    // The user's click opens the lead while paused.
    let (_lead, shell) = open_lead(&w.w).await;
    prompt(&w.w, &shell, "Plan it").await;
    assert!(merge_api(&w).await.is_empty());
    // A paused project's card says it is paused, not what the lead does.
    page_waits(&w, false).await;
    stop_and_type(&w.w, &shell).await;
    prompt(&w.w, &shell, WAKE).await;
    stop(&w.w, &shell).await;
    eventually("the lead's turn ended", async || {
        w.w.project_agents
            .leads()
            .lead_waits()
            .await
            .unwrap()
            .iter()
            .all(|wait| wait.freed.is_empty() && !wait.released.is_empty())
    })
    .await;
    page_waits(&w, false).await;

    w.w.pause.resume(&shop, &me()).await.unwrap();
    page_starts(&w).await;
    stop_all(w.w).await;
}
