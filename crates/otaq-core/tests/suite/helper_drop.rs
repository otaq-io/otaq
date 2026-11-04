//! `otaq helper drop <n> --stdin` (T10): the agent in charge drops a piece with a reason. Its helper's session stops,
//! its worktree and branch are removed, it is `dropped` with the reason in History, and it can't start again. Only
//! the agent in charge drops: the user, a helper, Noyan and another ticket's agent are refused, and so are a merged
//! piece, a piece dropped already, a piece with no helper and an empty reason.
//!
//! The agent's requests go through the socket from inside its session (an `sh` running `nc`), so they are judged as
//! that agent, as the CLI's are. Helpers are `sh` printing their variables.

use std::path::Path;

use otaq_core::agent_tools::LaunchContext;
use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::board::tickets::{EventKind, Ticket};
use otaq_core::caller::Caller;
use otaq_core::client::{Client, ClientConfig};
use otaq_core::domain::Status;
use otaq_core::git::testing::{commit, git};
use otaq_core::helpers::{Helper, HelperStatus};
use otaq_core::policy::{Action, BACKLOG_ONLY, HELPER_ONLY_ITS_PIECE, TicketFacts, authorize};
use otaq_core::protocol::{HelperNoteParams, Request};
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::{AgentShell, TestService};
use serde_json::{Value, json};

const PRINTS_ITS_ENV: &str =
    "printf 'PIECE=%s TICKET=%s\\n' \"$OTAQ_PIECE\" \"$OTAQ_TICKET\"; exec sleep 60";

const PLAN: &str = "## Pieces\n\n\
                    1. Wishlist API \u{2014} waits for nothing\n\
                    2. Wishlist button \u{2014} waits for 1\n\
                    3. Wishlist page \u{2014} waits for 1\n";

const IN_CHARGE: &str =
    "Only the agent in charge of SHOP-1 starts, sends back and merges its helpers";

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

/// SHOP-1 at work in Dev with its Plan, its branch and worktree, and its agent in charge (an `sh` agent session).
async fn at_work(w: &TestService) -> (Ticket, AgentShell, Caller) {
    let t = w.create("SHOP", "Wishlist").await;
    w.start_worktree(&t).await;
    w.force_status(&t, Status::Working).await;
    let t = w.board.ticket(&t.key.to_string()).await.unwrap();
    let agent = w.agent_shell(&t).await;
    let caller = Caller::Agent {
        ticket: t.key.clone(),
        ticket_id: t.id.clone(),
        session: agent.session.clone(),
    };
    let planned = send(
        w,
        &agent,
        "ticket.update",
        json!({"key": "SHOP-1", "plan": PLAN}),
    )
    .await;
    assert!(planned.get("error").is_none(), "{planned}");
    (t, agent, caller)
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

async fn drop_piece(
    w: &TestService,
    shell: &AgentShell,
    key: &str,
    piece: u32,
    reason: &str,
) -> Value {
    send(
        w,
        shell,
        "helper.drop",
        json!({"key": key, "piece": piece, "text": reason}),
    )
    .await
}

async fn start_helper(w: &TestService, shell: &AgentShell, piece: u32, title: &str) -> Value {
    send(
        w,
        shell,
        "helper.start",
        json!({"key": "SHOP-1", "piece": piece, "title": title}),
    )
    .await
}

fn refused(answer: &Value) -> &str {
    assert_eq!(answer["error"]["kind"], "refused", "{answer}");
    answer["error"]["message"].as_str().unwrap()
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

fn has_branch(repo: &Path, branch: &str) -> bool {
    !git(repo, &["branch", "--list", branch]).trim().is_empty()
}

/// Dropping a working piece: its session stops, its worktree (with what it held) and branch go, it reads `dropped`
/// with the reason in History, and `helper start` refuses it. The other helper runs on.
#[tokio::test(flavor = "multi_thread")]
async fn the_agent_in_charge_drops_a_working_piece() {
    let w = world().await;
    let (_, agent, _) = at_work(&w).await;
    assert!(
        start_helper(&w, &agent, 2, "Wishlist button")
            .await
            .get("error")
            .is_none()
    );
    assert!(
        start_helper(&w, &agent, 3, "Wishlist page")
            .await
            .get("error")
            .is_none()
    );
    let page = helper(&w, 3).await;
    let session = page.session.clone().unwrap();
    assert!(w.sessions.info(&session).unwrap().is_running());
    let worktree = Path::new(&page.worktree).to_path_buf();
    commit(&worktree, "page.rs", "pub fn page() {}\n");
    std::fs::write(worktree.join("draft.tmp"), "half done").unwrap();
    let project = w.board.project("SHOP").await.unwrap();
    let repo = Path::new(&project.repo_path);
    assert!(has_branch(repo, &page.branch));

    let answer = drop_piece(&w, &agent, "SHOP-1", 3, "The page moved to its own ticket.").await;
    assert!(answer.get("error").is_none(), "{answer}");
    assert_eq!(
        answer["reply"]["helper"]["text"],
        "Dropped 1.3 (Wishlist page): its helper stopped and its branch is gone. Take it out of the Plan with \
         `otaq update --plan-stdin`."
    );
    assert_eq!(answer["reply"]["helper"]["helper"]["status"], "dropped");

    let dropped = helper(&w, 3).await;
    assert_eq!(dropped.status, HelperStatus::Dropped);
    assert_eq!(dropped.row_status(), "dropped");
    assert_eq!(
        dropped.note.as_deref(),
        Some("The page moved to its own ticket.")
    );
    assert_eq!(dropped.port, None);
    assert!(!w.sessions.info(&session).unwrap().is_running());
    assert!(
        !worktree.exists(),
        "the worktree is removed, uncommitted files and all"
    );
    assert!(!has_branch(repo, &page.branch), "the branch is deleted");
    // The other piece runs on.
    let button = helper(&w, 2).await;
    assert_eq!(button.status, HelperStatus::Working);
    assert!(
        w.sessions
            .info(button.session.as_deref().unwrap())
            .unwrap()
            .is_running()
    );

    let history = w.board.history("SHOP-1").await.unwrap();
    let event = history
        .iter()
        .rfind(|e| e.kind == EventKind::Helper && e.payload["what"] == "dropped")
        .expect("a dropped event");
    assert_eq!(event.payload["label"], "1.3");
    assert_eq!(event.payload["title"], "Wishlist page");
    assert_eq!(event.payload["note"], "The page moved to its own ticket.");

    // It can't start again, be dropped again, sent back or merged.
    assert_eq!(
        refused(&start_helper(&w, &agent, 3, "Wishlist page").await),
        "1.3 is dropped already"
    );
    assert_eq!(
        refused(&drop_piece(&w, &agent, "SHOP-1", 3, "Again.").await),
        "1.3 is dropped already"
    );
    let back = send(
        &w,
        &agent,
        "helper.back",
        json!({"key": "SHOP-1", "piece": 3, "text": "Fix it."}),
    )
    .await;
    assert_eq!(refused(&back), "1.3 is dropped already");
    let merge = send(
        &w,
        &agent,
        "helper.merge",
        json!({"key": "SHOP-1", "piece": 3}),
    )
    .await;
    assert_eq!(refused(&merge), "1.3 is dropped already");
    w.stop().await;
}

/// Refusals: an empty reason, a piece with no helper, a merged piece; the user, a helper, Noyan and another ticket's
/// agent.
#[tokio::test(flavor = "multi_thread")]
async fn only_the_agent_in_charge_drops_a_piece_it_can_drop() {
    let w = world().await;
    let (t, agent, _) = at_work(&w).await;
    assert!(
        start_helper(&w, &agent, 1, "Wishlist API")
            .await
            .get("error")
            .is_none()
    );
    let api = helper(&w, 1).await;

    // An empty reason; a piece with no helper.
    assert_eq!(
        refused(&drop_piece(&w, &agent, "SHOP-1", 1, "  \n").await),
        "Say why 1.1 is dropped, on stdin: `otaq helper drop 1 --stdin`"
    );
    let unknown = drop_piece(&w, &agent, "SHOP-1", 9, "Not needed.").await;
    assert_eq!(
        unknown["error"]["message"], "SHOP-1 has no helper for piece 9",
        "{unknown}"
    );

    // The user (the app's own connection).
    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    let e = client
        .request(Request::HelperDrop(HelperNoteParams {
            key: "SHOP-1".into(),
            piece: 1,
            text: "Not needed.".into(),
        }))
        .await
        .unwrap_err();
    assert!(e.to_string().contains(IN_CHARGE), "{e}");

    // A helper: the policy keeps it to its piece; the handler refuses it too.
    let as_helper = Caller::Helper {
        ticket: t.key.clone(),
        ticket_id: t.id.clone(),
        session: api.session.clone().unwrap(),
    };
    let facts = TicketFacts {
        id: t.id.clone(),
        label: "SHOP-1".into(),
        created_by: t.created_by.clone(),
        backlog: false,
        never_started: false,
    };
    let e = authorize(&as_helper, &Action::Helpers(facts.clone())).unwrap_err();
    assert_eq!(e.to_string(), HELPER_ONLY_ITS_PIECE);
    let e = w
        .helpers
        .drop_piece(&as_helper, "SHOP-1", 1, "Not needed.")
        .await
        .unwrap_err();
    assert_eq!(e.to_string(), IN_CHARGE);

    // Noyan.
    let noyan = Caller::ProjectAgent {
        project: "SHOP".into(),
        agent: "pa-1".into(),
        session: "noyan-session".into(),
    };
    let e = authorize(&noyan, &Action::Helpers(facts)).unwrap_err();
    assert_eq!(e.to_string(), BACKLOG_ONLY);
    let e = w
        .helpers
        .drop_piece(&noyan, "SHOP-1", 1, "Not needed.")
        .await
        .unwrap_err();
    assert_eq!(e.to_string(), IN_CHARGE);

    // Another ticket's agent.
    let other = w.create("SHOP", "Coupons").await;
    let theirs = w.agent_shell(&other).await;
    assert_eq!(
        refused(&drop_piece(&w, &theirs, "SHOP-1", 1, "Not needed.").await),
        "SHOP-2's agent can't run SHOP-1's helpers"
    );
    assert_eq!(helper(&w, 1).await.status, HelperStatus::Working);

    // A merged piece.
    commit(Path::new(&api.worktree), "wishlist.rs", "pub fn add() {}\n");
    let merged = send(
        &w,
        &agent,
        "helper.merge",
        json!({"key": "SHOP-1", "piece": 1}),
    )
    .await;
    assert!(merged.get("error").is_none(), "{merged}");
    assert_eq!(
        refused(&drop_piece(&w, &agent, "SHOP-1", 1, "Not needed.").await),
        "1.1 is merged already"
    );
    w.stop().await;
}
