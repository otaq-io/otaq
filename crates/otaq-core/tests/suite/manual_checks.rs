//! Manual checks for you (T-0498, d0125): `--manual-check` on `stage next`, `stage done` and `check pass|fail` stores
//! one line per item on the ticket with who added it, in which stage and round, in the stage end's transaction;
//! `prime` lists them; a new round of the same stage replaces that stage's unticked items (ticked ones stay); ticking
//! is the user's, logged in History. Sessions are an ordinary `sh` (requests through the socket from inside them); no
//! agent tool runs.

use otaq_core::board::manual_checks::{MANUAL_CHECK_ONE_LINE, ManualCheck, ManualChecks};
use otaq_core::board::ticket_comments::CommentAuthor;
use otaq_core::board::tickets::{EventKind, Ticket};
use otaq_core::client::{Client, ClientConfig};
use otaq_core::domain::Status;
use otaq_core::protocol::{KeyParams, ManualCheckTickParams, Reply, Request};
use otaq_core::service::testing::{AgentShell, TestService};
use otaq_core::session::SessionKind;
use serde_json::{Value, json};

async fn world() -> TestService {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    w
}

/// A ticket working in Dev with a committed worktree, its agent's session running "Rust developer"; SHOP's flow has
/// no Validate, so `stage next` goes to Review.
async fn in_dev(w: &TestService, title: &str) -> (Ticket, AgentShell) {
    w.board
        .edit_flow_for_tests("SHOP", otaq_core::flow::default::testing::without_validate)
        .await;
    let t = w.create("SHOP", title).await;
    w.give_worktree(&t).await;
    w.force_status(&t, Status::Working).await;
    let dev = w.agent_shell(&t).await;
    w.run_as_agent(&dev, "rust-dev", "Rust developer").await;
    (t, dev)
}

/// One raw request from inside a session (no `'` in it: it goes through `sh`).
async fn send(w: &TestService, shell: &AgentShell, method: &str, params: Value) -> Value {
    let request =
        json!({"protocol_version": 1, "id": 1, "request": {"method": method, "params": params}});
    let ran = shell
        .send_raw(&w.data.socket_path(), &request.to_string())
        .await;
    serde_json::from_str(ran.stdout.lines().next().unwrap_or_default())
        .unwrap_or_else(|e| panic!("{e}: {ran:?}"))
}

/// `prime` first, as every stage end needs (CMT-08), then `method`.
async fn end_stage(w: &TestService, shell: &AgentShell, method: &str, params: Value) -> Value {
    let prime = match method {
        "validation.verdict" => "ticket.prime_validate",
        _ => "ticket.prime",
    };
    send(w, shell, prime, json!({ "key": params["key"] })).await;
    send(w, shell, method, params).await
}

async fn checks(w: &TestService) -> ManualChecks {
    w.board.manual_checks("SHOP-1").await.unwrap()
}

fn listed(checks: &ManualChecks) -> Vec<(u32, &str, bool)> {
    checks
        .items
        .iter()
        .map(|i| (i.n, i.text.as_str(), i.ticked()))
        .collect()
}

/// The user's own request over the socket (the app).
async fn tick(w: &TestService, n: u32, ticked: bool) -> ManualChecks {
    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    match client
        .request(Request::TicketManualCheckTick(ManualCheckTickParams {
            key: "SHOP-1".into(),
            n,
            ticked,
        }))
        .await
        .unwrap()
    {
        Reply::ManualChecks(checks) => checks,
        other => panic!("{other:?}"),
    }
}

/// `stage next --manual-check …`: stored with the agent, its stage and the comment it came with; listed in `prime`
/// and read by the app with the count not ticked. A line break refuses the whole stage end: nothing moves.
#[tokio::test(flavor = "multi_thread")]
async fn stage_next_lists_manual_checks_for_the_user() {
    let w = world().await;
    let (_t, dev) = in_dev(&w, "Offline pages").await;

    let two_lines = end_stage(
        &w,
        &dev,
        "ticket.stage_next",
        json!({"key": "SHOP-1", "summary": "Done.", "manual_checks": ["Reload offline\nand on a phone"]}),
    )
    .await;
    assert_eq!(
        (
            two_lines["error"]["data"]["refusal"].as_str(),
            two_lines["error"]["message"].as_str()
        ),
        (Some("multi_line"), Some(MANUAL_CHECK_ONE_LINE))
    );
    assert_eq!(w.status("SHOP-1").await, Status::Working, "nothing moved");
    assert!(
        w.board
            .ticket_comments("SHOP-1")
            .await
            .unwrap()
            .comments
            .is_empty()
    );

    let ended = end_stage(
        &w,
        &dev,
        "ticket.stage_next",
        json!({"key": "SHOP-1", "summary": "Pages cache for offline use.", "manual_checks": [
            "Production build: stop the server and reload a visited page",
            "  Open a never-visited detail page offline  ",
        ]}),
    )
    .await;
    assert!(ended["reply"]["stage_ended"].is_object(), "{ended}");
    assert_eq!(w.status("SHOP-1").await, Status::WaitingForYou);
    let now = checks(&w).await;
    assert_eq!(
        listed(&now),
        [
            (
                1,
                "Production build: stop the server and reload a visited page",
                false
            ),
            (2, "Open a never-visited detail page offline", false),
        ]
    );
    assert_eq!(now.unticked, 2);
    let first: &ManualCheck = &now.items[0];
    assert_eq!(
        first.author,
        CommentAuthor::Agent {
            id: Some("rust-dev".into()),
            name: Some("Rust developer".into()),
            tool: Some(otaq_core::agent_tools::Tool::Claude),
            stage: Some("dev".into()),
            round: None,
        }
    );
    assert_eq!(
        (
            first.stage_name.as_deref(),
            first.comment,
            first.source().as_str()
        ),
        (Some("Dev"), Some(1), "Rust developer \u{b7} Dev")
    );

    // The app's read, as the review's checklist uses it.
    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    let read = client
        .request(Request::TicketManualChecks(KeyParams {
            key: "SHOP-1".into(),
        }))
        .await
        .unwrap();
    assert_eq!(read, Reply::ManualChecks(now.clone()));

    let prime = w
        .board
        .prime_context("SHOP-1", w.adapters.for_new_session())
        .await
        .unwrap();
    let text = otaq_core::prime::render(&prime);
    assert!(
        text.contains(
            "## Manual checks for the user\n\nWhat agents couldn't check themselves; the user tries them at review."
        ),
        "{text}"
    );
    assert!(
        text.contains(
            "- [ ] Open a never-visited detail page offline \u{b7} Rust developer \u{b7} Dev\n"
        ),
        "{text}"
    );
    assert!(
        text.contains("Never leave a check to \"the PR\": there may be no PR."),
        "the rule: {text}"
    );
    w.stop().await;
}

/// Ticking is the user's and goes in History; an agent can't tick. Dev's next round replaces Dev's unticked items
/// and keeps the ticked one; numbers aren't reused.
#[tokio::test(flavor = "multi_thread")]
async fn a_new_round_replaces_the_stages_unticked_items() {
    let w = world().await;
    let (t, dev) = in_dev(&w, "Offline pages").await;
    end_stage(
        &w,
        &dev,
        "ticket.stage_next",
        json!({"key": "SHOP-1", "summary": "Round one.", "manual_checks": ["Reload offline", "Install as an app"]}),
    )
    .await;

    let by_agent = send(
        &w,
        &dev,
        "ticket.manual_check_tick",
        json!({"key": "SHOP-1", "n": 1, "ticked": true}),
    )
    .await;
    assert_eq!(
        by_agent["error"]["data"]["refusal"].as_str(),
        Some("not_allowed")
    );
    let ticked = tick(&w, 1, true).await;
    assert_eq!(
        listed(&ticked),
        [(1, "Reload offline", true), (2, "Install as an app", false)]
    );
    assert_eq!(ticked.unticked, 1);
    assert_eq!(ticked.items[0].ticked_by.as_deref(), Some(&*whoami()));
    // Ticking again changes nothing and logs nothing.
    tick(&w, 1, true).await;
    let history = w.board.history("SHOP-1").await.unwrap();
    let ticks: Vec<&Value> = history
        .iter()
        .filter(|e| e.kind == EventKind::ManualCheckTicked)
        .map(|e| &e.payload)
        .collect();
    assert_eq!(
        ticks,
        [&json!({"n": 1, "text": "Reload offline", "ticked": true})]
    );

    // Sent back to Dev: its next stage end lists again.
    w.force_status(&t, Status::Working).await;
    end_stage(
        &w,
        &dev,
        "ticket.stage_next",
        json!({"key": "SHOP-1", "summary": "Round two.", "manual_checks": ["Install on a phone"]}),
    )
    .await;
    let now = checks(&w).await;
    assert_eq!(
        listed(&now),
        [
            (1, "Reload offline", true),
            (3, "Install on a phone", false)
        ]
    );
    assert_eq!(now.items[1].comment, Some(2));

    // A round with none clears the stage's unticked ones.
    w.force_status(&t, Status::Working).await;
    end_stage(
        &w,
        &dev,
        "ticket.stage_next",
        json!({"key": "SHOP-1", "summary": "Round three."}),
    )
    .await;
    assert_eq!(listed(&checks(&w).await), [(1, "Reload offline", true)]);

    // Unticking is logged too.
    let unticked = tick(&w, 1, false).await;
    assert_eq!(unticked.unticked, 1);
    let history = w.board.history("SHOP-1").await.unwrap();
    assert!(
        history
            .iter()
            .any(|e| e.kind == EventKind::ManualCheckTicked && e.payload["ticked"] == json!(false))
    );
    w.stop().await;
}

/// A check's verdict lists its own, by the checker with its stage and round; Dev's next stage end leaves them.
#[tokio::test(flavor = "multi_thread")]
async fn a_verdict_lists_the_checks_items() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let t = w.create("SHOP", "Offline pages").await;
    w.give_worktree(&t).await;
    w.force_status(&t, Status::Working).await;
    let dev = w.agent_shell(&t).await;
    w.run_as_agent(&dev, "rust-dev", "Rust developer").await;
    let checker = w.shell(&t, SessionKind::Validator).await;
    w.run_as_agent(&checker, "validator", "Validator").await;
    w.validating_with(&t, &checker.session).await;

    let failed = end_stage(
        &w,
        &checker,
        "validation.verdict",
        json!({"key": "SHOP-1", "pass": false, "report": "Fail: no offline page.",
               "manual_checks": ["In Safari, reload with the network off"]}),
    )
    .await;
    assert!(failed["reply"]["ticket"].is_object(), "{failed}");
    let now = checks(&w).await;
    assert_eq!(
        listed(&now),
        [(1, "In Safari, reload with the network off", false)]
    );
    assert_eq!(
        now.items[0].source(),
        "Validator \u{b7} Validate \u{b7} round 1"
    );

    // Dev ends its next round with its own: the checker's item stays.
    let ended = end_stage(
        &w,
        &dev,
        "ticket.stage_next",
        json!({"key": "SHOP-1", "summary": "Added the offline page.", "manual_checks": ["Install as an app"]}),
    )
    .await;
    assert!(ended.get("reply").is_some(), "{ended}");
    let now = checks(&w).await;
    assert_eq!(
        listed(&now),
        [
            (1, "In Safari, reload with the network off", false),
            (2, "Install as an app", false)
        ]
    );
    let validate = w.board.validate_prime("SHOP-1").await.unwrap();
    let text = otaq_core::prime::render_validate(&validate);
    assert!(
        text.contains("- [ ] In Safari, reload with the network off \u{b7} Validator \u{b7} Validate \u{b7} round 1"),
        "{text}"
    );
    assert!(
        text.contains("`--manual-check \"<what to check>\"` on `otaq check pass|fail`"),
        "{text}"
    );
    w.stop().await;
}

/// The OS user, as the service records the user's actor.
fn whoami() -> String {
    otaq_core::domain::Actor::current_user().to_string()
}
