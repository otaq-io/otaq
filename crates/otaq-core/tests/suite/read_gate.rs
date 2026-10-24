//! The read gate (T-0444; CMT-08; d0109, d0083): `stage next`, `stage done` and `check pass|fail` from a ticket's
//! session are refused until that session ran `prime` (or `prime --validate`) since its stage run started or was
//! last handed a new comment; the count names the comments posted since its last prime. Comments posted mid-run
//! don't block; the user's moves never are. Requests go through the socket from inside sessions (an ordinary `sh`,
//! never an agent tool); every data folder is temporary.
//!
//! The resume marker (`stage_runs.handed_over_at`) is set by hand here; `hand_overs.rs` checks that the real Send back,
//! PR Send back, drag back and failed-check resumes set it, and the CLI's `comment_path.rs` walks a failed check end to
//! end.

use std::time::Duration;

use otaq_core::board::ticket_comments::{CommentAuthor, TicketComment};
use otaq_core::board::tickets::Ticket;
use otaq_core::client::{Client, ClientConfig, ClientError};
use otaq_core::domain::{Actor, Status};
use otaq_core::protocol::Request;
use otaq_core::service::testing::{AgentShell, TestService};
use otaq_core::session::SessionKind;
use otaq_core::store::now_ms;
use serde_json::{Value, json};

async fn send(w: &TestService, shell: &AgentShell, method: &str, params: Value) -> Value {
    let request =
        json!({"protocol_version": 1, "id": 1, "request": {"method": method, "params": params}});
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

async fn comment(w: &TestService, key: &str, body: &str) -> TicketComment {
    w.board
        .post_ticket_comment(
            key,
            &CommentAuthor::User { name: "me".into() },
            body,
            None,
            &Actor::User("me".into()),
        )
        .await
        .unwrap()
}

/// SHOP on the default flow without Validate, so `stage next` from Dev goes straight to Review; one ticket working
/// in Dev with a clean worktree and its agent session.
async fn world() -> (TestService, Ticket, AgentShell) {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    w.board
        .edit_flow_for_tests("SHOP", otaq_core::flow::default::testing::without_validate)
        .await;
    let t = w.create("SHOP", "Export orders as CSV").await;
    w.give_worktree(&t).await;
    let dev = w.agent_shell(&t).await;
    w.force_status(&t, Status::Working).await;
    let t = w.board.ticket(&t.key.to_string()).await.unwrap();
    (w, t, dev)
}

/// Lets the clock move on, so times recorded before and after compare strictly.
async fn tick() {
    tokio::time::sleep(Duration::from_millis(5)).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn stage_next_waits_for_prime_and_comments_posted_mid_run_dont_block() {
    let (w, t, dev) = world().await;
    let key = t.key.to_string();
    comment(&w, &key, "Agreed with you: CSV only, admins only.").await;
    comment(
        &w,
        &key,
        "Also keep the column order the same as the table.",
    )
    .await;
    let next = json!({"key": key, "summary": "Done."});

    // Refused before prime, naming the two comments; nothing moved.
    let answer = send(&w, &dev, "ticket.stage_next", next.clone()).await;
    assert_eq!(
        refused(&answer),
        "Run `otaq prime` first: SHOP-1 has 2 comments you haven't read."
    );
    assert_eq!(w.status(&key).await, Status::Working);

    // The user's prime doesn't count for the agent.
    ok(&send_as_user(&w, "ticket.prime", json!({"key": key})).await);
    let answer = send(&w, &dev, "ticket.stage_next", next.clone()).await;
    assert_eq!(
        refused(&answer),
        "Run `otaq prime` first: SHOP-1 has 2 comments you haven't read."
    );

    // The agent's prime: allowed, even with a comment the user posted after it (it is for the next agent).
    ok(&send(&w, &dev, "ticket.prime", json!({"key": key})).await);
    tick().await;
    comment(&w, &key, "Excel later, in another ticket.").await;
    ok(&send(&w, &dev, "ticket.stage_next", next).await);
    assert_eq!(w.status(&key).await, Status::WaitingForYou);
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_resume_with_a_new_comment_rearms_the_gate() {
    let (w, t, dev) = world().await;
    let key = t.key.to_string();
    let done = json!({"key": key, "summary": "Nothing to do."});

    // No comments: the short text.
    let answer = send(&w, &dev, "ticket.stage_done", done.clone()).await;
    assert_eq!(refused(&answer), "Run `otaq prime` first.");

    ok(&send(&w, &dev, "ticket.prime", json!({"key": key})).await);
    tick().await;
    // Otaq resumes the run with a new comment (a Send back, a drag back, a fail; T-0443 sets the marker).
    comment(&w, &key, "Validate failed round 2. Run `otaq prime`.").await;
    sqlx::query(
        "UPDATE stage_runs SET handed_over_at = ? WHERE ticket_id = ? AND ended_at IS NULL",
    )
    .bind(now_ms())
    .bind(&t.id)
    .execute(w.board.store().pool())
    .await
    .unwrap();
    let answer = send(&w, &dev, "ticket.stage_done", done.clone()).await;
    assert_eq!(
        refused(&answer),
        "Run `otaq prime` first: SHOP-1 has 1 comment you haven't read."
    );

    // Primed again: past the gate (stage done's own checks decide the rest).
    tick().await;
    ok(&send(&w, &dev, "ticket.prime", json!({"key": key})).await);
    let answer = send(&w, &dev, "ticket.stage_done", done).await;
    if let Some(message) = answer["error"]["message"].as_str() {
        assert!(!message.contains("prime"), "{answer}");
    }
    w.stop().await;
}

/// A prime recorded for an earlier run doesn't carry over: a new run of the stage starts unprimed.
#[tokio::test(flavor = "multi_thread")]
async fn a_new_run_starts_unprimed() {
    let (w, t, dev) = world().await;
    let key = t.key.to_string();
    ok(&send(&w, &dev, "ticket.prime", json!({"key": key})).await);
    // A prime older than the run's start (as if recorded before a restart of the run).
    sqlx::query(
        "UPDATE stage_runs SET primed_at = started_at - 1 WHERE ticket_id = ? AND ended_at IS NULL",
    )
    .bind(&t.id)
    .execute(w.board.store().pool())
    .await
    .unwrap();
    let answer = send(
        &w,
        &dev,
        "ticket.stage_next",
        json!({"key": key, "summary": "Done."}),
    )
    .await;
    assert_eq!(refused(&answer), "Run `otaq prime` first.");
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_checkers_verdict_waits_for_prime_validate() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let t = w.create("SHOP", "Export orders as CSV").await;
    let key = t.key.to_string();
    comment(&w, &key, "Done. Excel not added.").await;
    let checker = w.shell(&t, SessionKind::Validator).await;
    w.validating_with(&t, &checker.session).await;
    let verdict = json!({"key": key, "pass": true, "report": "All good."});

    let answer = send(&w, &checker, "validation.verdict", verdict.clone()).await;
    assert_eq!(
        refused(&answer),
        "Run `otaq prime` first: SHOP-1 has 1 comment you haven't read."
    );
    // Its plain prime counts as well as `--validate`; here `--validate`, as the validate skill runs it.
    ok(&send(&w, &checker, "ticket.prime_validate", json!({"key": key})).await);
    ok(&send(&w, &checker, "validation.verdict", verdict).await);
    assert_ne!(w.status(&key).await, Status::Checking);
    w.stop().await;
}

/// The user's moves are never gated: Move to next stage, and `stage next` from their own terminal.
#[tokio::test(flavor = "multi_thread")]
async fn the_users_moves_are_never_refused() {
    let (w, t, _dev) = world().await;
    let key = t.key.to_string();
    comment(&w, &key, "Also keep the column order.").await;
    w.force_status(&t, Status::Idle).await;
    ok(&send_as_user(&w, "ticket.submit_by_user", json!({"key": key})).await);
    assert_eq!(w.status(&key).await, Status::WaitingForYou);

    let other = w.create("SHOP", "Order list paging").await;
    w.give_worktree(&other).await;
    w.force_status(&other, Status::Working).await;
    let answer = send_as_user(
        &w,
        "ticket.stage_next",
        json!({"key": "SHOP-2", "summary": "Done."}),
    )
    .await;
    if let Some(message) = answer["error"]["message"].as_str() {
        assert!(!message.contains("prime"), "{answer}");
    }
    w.stop().await;
}

/// A new agent's first `prime` can arrive before its run opens (the start sets the ticket Working after the spawn):
/// it counts for the run the session then opens, and `stage next` goes through. A hand-over after it re-arms the gate.
#[tokio::test(flavor = "multi_thread")]
async fn a_prime_before_the_run_opens_counts_for_it() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    w.board
        .edit_flow_for_tests("SHOP", otaq_core::flow::default::testing::without_validate)
        .await;
    let t = w.create("SHOP", "Export orders as CSV").await;
    w.give_worktree(&t).await;
    let dev = w.agent_shell(&t).await;
    // Held stands for the start's wait before Working: no run is open, and the scheduler leaves it alone.
    w.force_status(&t, Status::Held).await;
    let key = t.key.to_string();
    let open: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM stage_runs WHERE ticket_id = ? AND ended_at IS NULL",
    )
    .bind(&t.id)
    .fetch_one(w.board.store().pool())
    .await
    .unwrap();
    assert_eq!(open, 0, "no run while it waits to start");

    ok(&send(&w, &dev, "ticket.prime", json!({"key": key})).await);
    tick().await;
    // The start's state change opens the run now.
    w.force_status(&t, Status::Working).await;

    // A hand-over after the prime (a comment Otaq handed it) needs a new prime.
    let next = json!({"key": key, "summary": "Done."});
    tick().await;
    sqlx::query(
        "UPDATE stage_runs SET handed_over_at = ? WHERE ticket_id = ? AND ended_at IS NULL",
    )
    .bind(now_ms())
    .bind(&t.id)
    .execute(w.board.store().pool())
    .await
    .unwrap();
    assert_eq!(
        refused(&send(&w, &dev, "ticket.stage_next", next.clone()).await),
        "Run `otaq prime` first."
    );
    sqlx::query("UPDATE stage_runs SET handed_over_at = NULL WHERE ticket_id = ?")
        .bind(&t.id)
        .execute(w.board.store().pool())
        .await
        .unwrap();

    ok(&send(&w, &dev, "ticket.stage_next", next).await);
    assert_eq!(w.status(&key).await, Status::WaitingForYou);
    w.stop().await;
}
