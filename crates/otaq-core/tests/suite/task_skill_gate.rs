//! The task skill gate (tuq decisions 081, 098): in a stage whose skill isn't Otaq's (`tuq:product-spec`), Otaq's
//! `work-ticket` primes in the first turn and the stage's task skill is typed once that turn ends. The agent's own
//! `stage next` / `stage done` before then is refused, telling it to end its turn: an agent that ignored "first turn:
//! prime, then end your turn" (seen on Copilot and Codex) would otherwise end the stage and the task skill would never
//! run. Nothing else changes: the commit it made stays, the user's moves end the stage as before (and drop the task
//! skill), stages with no task skill and an ended session are never refused, and a stage held for a missing tuq never
//! queued a task skill.
//!
//! The agent is an ordinary `sh` session sending requests through the socket (read_gate's pattern); its task skill is
//! queued as the start queues it ([`otaq_core::session::deliveries::Deliveries::queue_task_skill`] with the tool's
//! form), and a turn ends with Claude Code's real `Stop` hook. Unix only, as the requests go through `nc`.
#![cfg(unix)]

use std::path::Path;

use otaq_core::agent_tools::Tool;
use otaq_core::board::moves::DropOn;
use otaq_core::board::tickets::Ticket;
use otaq_core::caller::Caller;
use otaq_core::client::{Client, ClientConfig, ClientError};
use otaq_core::domain::{Actor, Status};
use otaq_core::git::testing::{commit, git};
use otaq_core::protocol::Request;
use otaq_core::review::drag::DragAnswers;
use otaq_core::service::testing::{AgentShell, TestService};
use otaq_core::session::SessionKind;
use serde_json::{Value, json};

async fn send(w: &TestService, shell: &AgentShell, method: &str, params: Value) -> Value {
    let request = json!({"protocol_version": otaq_core::protocol::PROTOCOL_VERSION, "id": 1, "request": {"method": method, "params": params}});
    let ran = shell
        .send_raw(&w.data.socket_path(), &request.to_string())
        .await;
    serde_json::from_str(ran.stdout.lines().next().unwrap_or_default())
        .unwrap_or_else(|e| panic!("{e}: {ran:?}"))
}

async fn send_as_user(w: &TestService, method: &str, params: Value) -> Value {
    let request: Request =
        serde_json::from_value(json!({"method": method, "params": params})).unwrap();
    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    match client.request(request).await {
        Ok(reply) => json!({"reply": serde_json::to_value(reply).unwrap()}),
        Err(ClientError::Service(e)) => json!({"error": serde_json::to_value(e).unwrap()}),
        Err(e) => panic!("{method}: {e}"),
    }
}

fn refused(answer: &Value) -> &str {
    assert_eq!(answer["error"]["kind"], "refused", "{answer}");
    answer["error"]["message"].as_str().unwrap()
}

fn ok(answer: &Value) -> &Value {
    assert!(answer.get("error").is_none(), "{answer}");
    &answer["reply"]
}

/// SHOP on the default flow without Validate (Dev → Review), its Dev stage naming `skill` (`None`: Otaq's own
/// `work-ticket` alone); one ticket working in Dev with a worktree and its agent session on `tool`.
async fn world(skill: Option<&str>, tool: Tool) -> (TestService, Ticket, AgentShell) {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let skill = skill.map(str::to_owned);
    w.board
        .edit_flow_for_tests("SHOP", move |file| {
            otaq_core::flow::default::testing::without_validate(file);
            for stage in &mut file.flow.stages {
                if stage.id == otaq_core::flow::default::DEV {
                    stage.skill = skill.clone();
                }
            }
        })
        .await;
    let t = w.create("SHOP", "Coupon codes").await;
    w.give_worktree(&t).await;
    let dev = w.shell_on(&t, SessionKind::Agent, Some(tool)).await;
    w.force_status(&t, Status::Working).await;
    let t = w.board.ticket(&t.key.to_string()).await.unwrap();
    (w, t, dev)
}

/// Queues the stage's task skill for the agent's session as the start does: in the tool's form, once.
async fn queue_task_skill(w: &TestService, dev: &AgentShell, skill: &str) {
    let text = w.deliveries.skill_text(&dev.session, skill, None);
    w.deliveries
        .queue_task_skill(&dev.session, &text)
        .await
        .unwrap();
}

/// The agent's turn ended (Claude Code's Stop hook, its real shape), and the service types what waits.
async fn turn_ended(w: &TestService, t: &Ticket, session: &str) {
    let me = Caller::Agent {
        ticket: t.key.clone(),
        ticket_id: t.id.clone(),
        session: session.to_owned(),
    };
    let payload: Value = serde_json::from_str(
        &std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/claude/hooks/Stop-idle.json"),
        )
        .unwrap(),
    )
    .unwrap();
    w.signals
        .hook(&me, Tool::Claude, "Stop", &payload)
        .await
        .unwrap();
    w.deliveries.deliver_if_ready(session).await.unwrap();
}

fn worktree_of(t: &Ticket) -> std::path::PathBuf {
    std::path::PathBuf::from(t.work.as_ref().unwrap().worktree.clone().unwrap())
}

const PRODUCT_SPEC: &str = "Your stage's skill `/tuq:product-spec` hasn't run yet: end your turn now, and Otaq types \
                            it next.";

/// The agent commits and runs `stage next` in its first turn: refused, before the read gate too; the ticket stays in
/// Dev and Working, the commit stays, and the task skill still waits. Its turn ends, the task skill is typed, and then
/// `stage next` ends the stage.
#[tokio::test(flavor = "multi_thread")]
async fn stage_next_in_the_first_turn_waits_for_the_task_skill() {
    let (w, t, dev) = world(Some("tuq:product-spec"), Tool::Claude).await;
    let key = t.key.to_string();
    queue_task_skill(&w, &dev, "tuq:product-spec").await;
    let worktree = worktree_of(&t);
    commit(&worktree, "coupons.ts", "export const coupons = [];\n");
    let head = git(&worktree, &["rev-parse", "HEAD"]);
    let next = json!({"key": key, "summary": "Coupon codes written."});

    // Before prime: the task skill's refusal comes first.
    assert_eq!(
        refused(&send(&w, &dev, "ticket.stage_next", next.clone()).await),
        PRODUCT_SPEC
    );
    ok(&send(&w, &dev, "ticket.prime", json!({"key": key})).await);
    assert_eq!(
        refused(&send(&w, &dev, "ticket.stage_next", next.clone()).await),
        PRODUCT_SPEC
    );
    let after = w.board.ticket(&key).await.unwrap();
    assert_eq!(after.status, Status::Working);
    assert_eq!(after.place.stage(), Some(otaq_core::flow::default::DEV));
    assert_eq!(
        git(&worktree, &["rev-parse", "HEAD"]),
        head,
        "the commit stays"
    );
    assert_eq!(
        crate::common::waiting_texts(&w.deliveries, &dev.session).await,
        ["/tuq:product-spec"]
    );

    // The turn ends: the task skill is typed, and nothing waits any more.
    turn_ended(&w, &t, &dev.session).await;
    w.wait_for_output(&dev.session, "/tuq:product-spec").await;
    assert!(w.deliveries.pending(&dev.session).await.unwrap().is_empty());

    // The task skill's turn ends the stage.
    ok(&send(&w, &dev, "ticket.prime", json!({"key": key})).await);
    ok(&send(&w, &dev, "ticket.stage_next", next).await);
    assert_eq!(w.status(&key).await, Status::WaitingForYou);
    w.stop().await;
}

/// `stage done` is refused the same way, with the skill in the session's tool's form (Copilot's
/// `/tuq:tuq-product-spec`); the ticket stays where it was.
#[tokio::test(flavor = "multi_thread")]
async fn stage_done_waits_for_the_task_skill_in_the_tools_form() {
    let (w, t, dev) = world(Some("tuq:product-spec"), Tool::Copilot).await;
    let key = t.key.to_string();
    queue_task_skill(&w, &dev, "tuq:product-spec").await;
    let typed = Tool::Copilot.skill_reference("tuq:product-spec");
    assert_eq!(typed, "/tuq:tuq-product-spec");
    ok(&send(&w, &dev, "ticket.prime", json!({"key": key})).await);
    assert_eq!(
        refused(
            &send(
                &w,
                &dev,
                "ticket.stage_done",
                json!({"key": key, "summary": "Nothing to do."})
            )
            .await
        ),
        format!(
            "Your stage's skill `{typed}` hasn't run yet: end your turn now, and Otaq types it next."
        )
    );
    let after = w.board.ticket(&key).await.unwrap();
    assert_eq!(after.status, Status::Working);
    assert_eq!(after.place.stage(), Some(otaq_core::flow::default::DEV));
    assert_eq!(w.deliveries.pending(&dev.session).await.unwrap().len(), 1);
    w.stop().await;
}

/// A stage with no task skill (Otaq's `work-ticket` alone) isn't gated: `stage next` in the first turn ends it.
#[tokio::test(flavor = "multi_thread")]
async fn a_stage_without_a_task_skill_isnt_gated() {
    let (w, t, dev) = world(None, Tool::Claude).await;
    let key = t.key.to_string();
    ok(&send(&w, &dev, "ticket.prime", json!({"key": key})).await);
    ok(&send(
        &w,
        &dev,
        "ticket.stage_next",
        json!({"key": key, "summary": "Done."}),
    )
    .await);
    assert_eq!(w.status(&key).await, Status::WaitingForYou);
    w.stop().await;
}

/// The user's own moves end the stage while the task skill waits, and drop it: `stage next` from their terminal, and
/// a drag.
#[tokio::test(flavor = "multi_thread")]
async fn the_users_moves_end_the_stage_and_drop_the_task_skill() {
    let (w, t, dev) = world(Some("tuq:product-spec"), Tool::Claude).await;
    let key = t.key.to_string();
    queue_task_skill(&w, &dev, "tuq:product-spec").await;
    ok(&send_as_user(
        &w,
        "ticket.stage_next",
        json!({"key": key, "summary": "Moving on."}),
    )
    .await);
    assert_eq!(w.status(&key).await, Status::WaitingForYou);
    assert!(w.deliveries.pending(&dev.session).await.unwrap().is_empty());

    let (w2, t2, dev2) = world(Some("tuq:product-spec"), Tool::Claude).await;
    let key2 = t2.key.to_string();
    queue_task_skill(&w2, &dev2, "tuq:product-spec").await;
    // A drag waits for a turn to end (its own rule); the task skill still waits then, say behind the user's unsent
    // text.
    w2.force_status(&t2, Status::Idle).await;
    let answers = DragAnswers {
        note: "",
        attachments: Default::default(),
        confirmed: true,
        clear_first: false,
    };
    w2.review
        .drag(
            &key2,
            &DropOn::stage(otaq_core::flow::default::REVIEW),
            answers,
            &w2.validation,
            &Actor::User("me".into()),
        )
        .await
        .unwrap();
    assert_eq!(
        w2.board.ticket(&key2).await.unwrap().place.stage(),
        Some(otaq_core::flow::default::REVIEW)
    );
    assert!(
        w2.deliveries
            .pending(&dev2.session)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        w2.board
            .task_skill_waiting(&key2, &dev2.session)
            .await
            .unwrap(),
        None
    );
    w.stop().await;
    w2.stop().await;
}

/// A session that ended takes its task skill with it: nothing is left for the gate to refuse.
#[tokio::test(flavor = "multi_thread")]
async fn an_ended_session_leaves_no_task_skill_to_wait_for() {
    let (w, t, dev) = world(Some("tuq:product-spec"), Tool::Claude).await;
    let key = t.key.to_string();
    queue_task_skill(&w, &dev, "tuq:product-spec").await;
    assert_eq!(
        w.board
            .task_skill_waiting(&key, &dev.session)
            .await
            .unwrap(),
        Some(Some("tuq:product-spec".to_owned()))
    );
    w.sessions.stop(&dev.session).await.unwrap();
    let gone = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        while w
            .board
            .task_skill_waiting(&key, &dev.session)
            .await
            .unwrap()
            .is_some()
        {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await;
    assert!(gone.is_ok(), "the ended session's task skill still waits");
    w.stop().await;
}
