//! The Plan written and grown during the build stage (T4). The build stage's agent decides the split itself: on a
//! ticket that arrived with no Plan it writes one on its own started ticket and starts helpers for its pieces, and
//! while helpers run it adds pieces and rewrites a piece's text to what was already built. Only the agent in charge
//! changes the Plan; a ticket's agent still can't edit another started ticket.
//!
//! The requests go through the socket from inside the agent's session (an `sh` running `nc`), so they are judged as
//! that agent, as `otaq update` and `otaq helper start` are. Helpers are `sh` printing their variables.

use std::path::{Path, PathBuf};

use otaq_core::agent_tools::LaunchContext;
use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::board::tickets::{EventKind, Ticket, TicketUpdate};
use otaq_core::caller::Caller;
use otaq_core::client::{Client, ClientConfig};
use otaq_core::domain::Status;
use otaq_core::domain::plan::PlanBy;
use otaq_core::git::testing::commit;
use otaq_core::helpers::{Helper, HelperStatus};
use otaq_core::protocol::{Request, TicketUpdateParams};
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::{AgentShell, TestService};
use serde_json::{Value, json};

/// Keeps running, printing the variables a helper is told.
const PRINTS_ITS_ENV: &str =
    "printf 'PIECE=%s TICKET=%s\\n' \"$OTAQ_PIECE\" \"$OTAQ_TICKET\"; exec sleep 60";

/// The Plan the Dev agent writes up front, in the shape the skills write.
const PLAN: &str = "Why: the API lands first so the page has something to call.\n\n## Pieces\n\n\
                    1. Wishlist API \u{2014} waits for nothing\n\
                    2. Wishlist button \u{2014} waits for 1\n\
                    3. Wishlist page \u{2014} waits for 1\n";

async fn world() -> TestService {
    let w = TestService::start_with(|config| ServiceConfig {
        launch: Some(LaunchContext {
            bin_dir: "/nonexistent".into(),
            plugin_dir: "/nonexistent/plugin".into(),
            path: std::env::var_os("PATH").unwrap_or_default(),
            claude: ClaudeLaunch {
                program: vec!["sh".into(), "-c".into(), PRINTS_ITS_ENV.into(), "sh".into()],
                config_dir: None,
            },
        }),
        ..config
    })
    .await;
    w.add_project("shop-web").await;
    w
}

/// A ticket at work in Dev with no Plan: its branch and worktree, and its agent in charge (an `sh` agent session).
async fn at_work(w: &TestService, title: &str) -> (Ticket, PathBuf, AgentShell, Caller) {
    let t = w.create("SHOP", title).await;
    let worktree = w.start_worktree(&t).await;
    w.force_status(&t, Status::Working).await;
    let t = w.board.ticket(&t.key.to_string()).await.unwrap();
    let agent = w.agent_shell(&t).await;
    let caller = Caller::Agent {
        ticket: t.key.clone(),
        ticket_id: t.id.clone(),
        session: agent.session.clone(),
    };
    (t, worktree, agent, caller)
}

/// One request from inside `shell`'s session; the answer line.
async fn send(w: &TestService, shell: &AgentShell, method: &str, params: Value) -> Value {
    let request = json!({"protocol_version": otaq_core::protocol::PROTOCOL_VERSION, "id": 1,
                         "request": {"method": method, "params": params}});
    let ran = shell
        .send_raw(&w.data.socket_path(), &request.to_string())
        .await;
    serde_json::from_str(ran.stdout.lines().next().unwrap_or_default())
        .unwrap_or_else(|e| panic!("{e}: {ran:?}"))
}

/// `otaq update <key> --plan-file …` from the agent.
async fn write_plan(w: &TestService, shell: &AgentShell, key: &str, plan: &str) -> Value {
    send(w, shell, "ticket.update", json!({"key": key, "plan": plan})).await
}

/// `otaq helper start <n> --title …` from the agent.
async fn start_helper(w: &TestService, shell: &AgentShell, piece: u32, title: &str) -> Value {
    send(
        w,
        shell,
        "helper.start",
        json!({"key": "SHOP-1", "piece": piece, "title": title}),
    )
    .await
}

fn reply_text(answer: &Value) -> &str {
    assert!(answer.get("error").is_none(), "{answer}");
    answer["reply"]["helper"]["text"]
        .as_str()
        .or_else(|| answer["reply"]["text"].as_str())
        .unwrap_or_else(|| panic!("no text: {answer}"))
}

async fn told(w: &TestService, session: &str) -> Vec<String> {
    w.deliveries
        .pending(session)
        .await
        .unwrap()
        .into_iter()
        .map(|d| d.text)
        .collect()
}

async fn agent_prime(w: &TestService, key: &str) -> String {
    otaq_core::prime::render(
        &w.board
            .prime_context(key, w.adapters.for_new_session())
            .await
            .unwrap(),
    )
}

async fn helper(w: &TestService, piece: u32) -> Helper {
    let t = w.board.ticket("SHOP-1").await.unwrap();
    w.helpers
        .of_ticket(&t.id)
        .await
        .unwrap()
        .into_iter()
        .find(|h| h.piece == piece)
        .unwrap()
}

async fn helper_prime(w: &TestService, h: &Helper) -> String {
    let t = w.board.ticket("SHOP-1").await.unwrap();
    let caller = Caller::Helper {
        ticket: t.key.clone(),
        ticket_id: t.id.clone(),
        session: h.session.clone().unwrap(),
    };
    otaq_core::helpers::prime::render(&w.helpers.prime(&caller).await.unwrap())
}

/// Case A: the Dev agent writes a Plan on its own started ticket that had none; `prime` shows its Pieces and
/// `helper start` takes them. Its own edit queues no change note for itself. Another started ticket stays refused.
#[tokio::test(flavor = "multi_thread")]
async fn the_dev_agent_writes_a_plan_on_its_own_started_ticket_and_starts_helpers() {
    let w = world().await;
    let (_, _, agent, _) = at_work(&w, "Wishlist").await;
    assert!(!agent_prime(&w, "SHOP-1").await.contains("## Plan"));

    let answer = write_plan(&w, &agent, "SHOP-1", PLAN).await;
    assert!(answer.get("error").is_none(), "{answer}");
    let after = w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(after.plan, PLAN);
    assert_eq!(after.plan_by, Some(PlanBy::Stage("Dev".into())));
    assert!(
        told(&w, &agent.session).await.is_empty(),
        "an agent's own edit sends it no note"
    );

    let prime = agent_prime(&w, "SHOP-1").await;
    assert!(
        prime.contains(&format!(
            "## Plan\n\n{}\n\nPieces: 3 (1 \u{2192} 2, 3)\n",
            PLAN.trim()
        )),
        "{prime}"
    );

    let started = start_helper(&w, &agent, 1, "Wishlist API").await;
    let text = reply_text(&started);
    assert!(
        text.starts_with("Started 1.1's helper (Wishlist API) on "),
        "{text}"
    );
    let h = helper(&w, 1).await;
    assert_eq!(h.status, HelperStatus::Working);
    let out = w
        .wait_for_output(h.session.as_deref().unwrap(), "PIECE=")
        .await;
    assert!(out.contains("PIECE=1 TICKET=SHOP-1"), "{out}");
    let piece = helper_prime(&w, &h).await;
    assert!(
        piece.contains("## Your piece\n\n1.1 Wishlist API \u{2014} waits for nothing\n"),
        "{piece}"
    );

    // A ticket's agent still can't write the Plan of another started ticket.
    let other = w.create("SHOP", "Checkout").await;
    w.force_status(&other, Status::Working).await;
    let refused = write_plan(&w, &agent, "SHOP-2", PLAN).await;
    assert_eq!(refused["error"]["kind"], "refused", "{refused}");
    assert!(
        refused["error"]["message"]
            .as_str()
            .unwrap()
            .contains("SHOP-1's agent can only change tickets it created"),
        "{refused}"
    );
    assert_eq!(w.board.ticket("SHOP-2").await.unwrap().plan, "");
    w.stop().await;
}

/// Case B: with helpers running, the agent in charge merges a piece handed back early, rewrites that piece's text to
/// what was built, edits a running piece's text and adds pieces for the rest. The running helper is left alone (same
/// session, same status, no note); the new pieces start; a merged piece can't start again. The agent's own edit sends
/// no note to anyone; a user's Plan edit reaches the agent in charge only.
#[tokio::test(flavor = "multi_thread")]
async fn the_agent_in_charge_grows_the_plan_while_helpers_run() {
    let w = world().await;
    let (t, _, agent, _) = at_work(&w, "Wishlist").await;
    assert!(
        write_plan(&w, &agent, "SHOP-1", PLAN)
            .await
            .get("error")
            .is_none()
    );
    reply_text(&start_helper(&w, &agent, 1, "Wishlist API").await);
    reply_text(&start_helper(&w, &agent, 3, "Wishlist page").await);
    let running = helper(&w, 3).await;
    let running_session = running.session.clone().unwrap();

    // Piece 1 turned out bigger: its helper commits what it built and hands back with a suggested split.
    let first = helper(&w, 1).await;
    commit(
        Path::new(&first.worktree),
        "wishlist.rs",
        "pub fn add() {}\n",
    );
    let helper_one = Caller::Helper {
        ticket: t.key.clone(),
        ticket_id: t.id.clone(),
        session: first.session.clone().unwrap(),
    };
    w.helpers
        .done(
            &helper_one,
            "Add and list are done; export is left.\n\nSuggest: 4 export API, 5 export button.",
        )
        .await
        .unwrap();
    assert_eq!(
        told(&w, &agent.session).await,
        [otaq_core::helpers::handed_back_text(
            "1.1",
            1,
            "Add and list are done; export is left."
        )]
    );
    // The message carries only the summary's first line; the suggested split is in the comment `prime` shows.
    assert!(
        agent_prime(&w, "SHOP-1")
            .await
            .contains("Suggest: 4 export API, 5 export button.")
    );
    let merged = send(
        &w,
        &agent,
        "helper.merge",
        json!({"key": "SHOP-1", "piece": 1}),
    )
    .await;
    assert!(merged.get("error").is_none(), "{merged}");

    // The agent in charge grows the Plan: piece 1 shrunk to what was built, piece 3's text edited, 4 and 5 added.
    let grown = "Why: the API lands first so the page has something to call.\n\n## Pieces\n\n\
                 1. Wishlist API \u{2014} waits for nothing\n   \
                 Built: add and list.\n\
                 2. Wishlist button \u{2014} waits for 1\n\
                 3. Wishlist page \u{2014} waits for 1\n   \
                 Show the list only; export comes with 5.\n\
                 4. Export API \u{2014} waits for 1\n\
                 5. Export button \u{2014} waits for 3, 4\n";
    let answer = write_plan(&w, &agent, "SHOP-1", grown).await;
    assert!(answer.get("error").is_none(), "{answer}");
    let after = w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(after.plan, grown);
    assert_eq!(after.plan_by, Some(PlanBy::Stage("Dev".into())));
    let history = w.board.history("SHOP-1").await.unwrap();
    assert_eq!(
        history
            .iter()
            .filter(|e| e.kind == EventKind::Updated)
            .filter(|e| e.payload["changes"][0]["field"] == "plan")
            .count(),
        2,
        "both versions are in History"
    );
    assert!(
        agent_prime(&w, "SHOP-1")
            .await
            .contains("Pieces: 5 (1 \u{2192} 2, 3, 4 \u{2192} 5)\n")
    );

    // No note for the agent's own edit, to itself or to the running helper.
    assert_eq!(
        told(&w, &agent.session).await.len(),
        1,
        "only the hand-back"
    );
    assert!(told(&w, &running_session).await.is_empty());

    // The running piece is left alone; its next prime has the new text.
    let still = helper(&w, 3).await;
    assert_eq!(still.status, HelperStatus::Working);
    assert_eq!(still.session.as_deref(), Some(running_session.as_str()));
    assert!(w.sessions.info(&running_session).unwrap().is_running());
    let piece = helper_prime(&w, &still).await;
    assert!(
        piece.contains(
            "1.3 Wishlist page \u{2014} waits for 1.1\n\nShow the list only; export comes with 5.\n"
        ),
        "{piece}"
    );

    // The new pieces start; the merged one can't start again.
    let started = start_helper(&w, &agent, 4, "Export API").await;
    assert!(
        reply_text(&started).starts_with("Started 1.4's helper (Export API) on "),
        "{started}"
    );
    let fourth = helper(&w, 4).await;
    assert!(
        helper_prime(&w, &fourth)
            .await
            .contains("## Your piece\n\n1.4 Export API \u{2014} waits for 1.1\n")
    );
    let again = start_helper(&w, &agent, 1, "Wishlist API").await;
    assert_eq!(again["error"]["kind"], "refused", "{again}");
    assert_eq!(again["error"]["message"], "1.1 is merged already");

    // A user's Plan edit reaches the agent in charge, naming the pieces; the helpers hear nothing.
    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    client
        .request(Request::TicketUpdate(TicketUpdateParams {
            key: "SHOP-1".into(),
            update: TicketUpdate {
                plan: Some(grown.replace("5. Export button \u{2014} waits for 3, 4\n", "")),
                ..Default::default()
            },
        }))
        .await
        .unwrap();
    assert_eq!(
        told(&w, &agent.session).await.last().unwrap(),
        "The ticket was updated by the user. The plan changed: 1.5 removed. Run `otaq prime` for the full \
         ticket."
    );
    assert!(told(&w, &running_session).await.is_empty());
    w.stop().await;
}
