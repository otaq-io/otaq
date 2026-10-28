//! One reminder before Idle (d0042, refined 2026-10-28): a ticket's agent that ends its turn in an Agent stage without
//! `stage next`, `stage done` or `ask human` is reminded once, and the card stays Working; if the reminder's own turn
//! ends the same way, the ticket shows Idle. A turn the user's own typed message started gets no reminder. An ordinary
//! `sh` stands in for `claude`.

use otaq_core::agent_tools::Tool;
use otaq_core::board::tickets::Ticket;
use otaq_core::caller::Caller;
use otaq_core::domain::Status;
use otaq_core::helpers;
use otaq_core::service::testing::{AgentShell, TestService};
use serde_json::json;

fn agent(ticket: &Ticket, shell: &AgentShell) -> Caller {
    Caller::Agent {
        ticket: ticket.key.clone(),
        ticket_id: ticket.id.clone(),
        session: shell.session.clone(),
    }
}

/// SHOP-1 Working in its Dev stage (forcing Working opens its Agent stage run), with its agent's session.
async fn in_dev(w: &TestService) -> (Ticket, AgentShell) {
    w.add_project("shop-web").await;
    let t = w.create("SHOP", "Split user full name").await;
    w.force_status(&t, Status::Working).await;
    let shell = w.agent_shell(&t).await;
    (t, shell)
}

async fn hook(w: &TestService, caller: &Caller, event: &str, payload: serde_json::Value) {
    w.signals
        .hook(caller, Tool::Claude, event, &payload)
        .await
        .unwrap();
}

async fn stop(w: &TestService, caller: &Caller) {
    hook(
        w,
        caller,
        "Stop",
        json!({"hook_event_name": "Stop", "stop_hook_active": false,
               "last_assistant_message": "Done: the name is split and the tests pass.",
               "background_tasks": [], "session_crons": []}),
    )
    .await;
}

async fn new_turn(w: &TestService, caller: &Caller) {
    hook(
        w,
        caller,
        "UserPromptSubmit",
        json!({"hook_event_name": "UserPromptSubmit", "prompt": "…"}),
    )
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_stage_left_open_is_reminded_once_then_idle() {
    let w = TestService::start().await;
    let (t, shell) = in_dev(&w).await;
    let caller = agent(&t, &shell);

    stop(&w, &caller).await;
    assert_eq!(w.status("SHOP-1").await, Status::Working, "no Idle yet");
    w.wait_for_output(&shell.session, "your stage is still open")
        .await;

    // The reminder's turn ends the same way: Idle, and no second reminder.
    new_turn(&w, &caller).await;
    stop(&w, &caller).await;
    assert_eq!(w.status("SHOP-1").await, Status::Idle);

    // A later turn Otaq starts (an answer, a validation result) gets its own reminder.
    new_turn(&w, &caller).await;
    stop(&w, &caller).await;
    assert_eq!(w.status("SHOP-1").await, Status::Working);
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_turn_the_user_started_by_typing_gets_no_reminder() {
    let w = TestService::start().await;
    let (t, shell) = in_dev(&w).await;
    let caller = agent(&t, &shell);

    w.signals
        .enter(&t.id, Some("Why did you rename the column?"))
        .await
        .unwrap();
    new_turn(&w, &caller).await;
    stop(&w, &caller).await;
    assert_eq!(w.status("SHOP-1").await, Status::Idle);
    w.stop().await;
}

/// The agent in charge with a piece handed back but not merged gets no reminder: Idle as before (a helper still at
/// work makes it Working in background instead, `helpers::the_agent_in_charge_waiting_for_a_helper_isnt_idle`).
#[tokio::test(flavor = "multi_thread")]
async fn no_reminder_while_a_helper_isnt_merged() {
    let w = TestService::start().await;
    let (t, shell) = in_dev(&w).await;
    helpers::store::upsert(
        w.board.store().pool(),
        &helpers::store::NewHelper {
            ticket_id: &t.id,
            piece: 1,
            title: "Wishlist API",
            branch: "otaq/SHOP-1.1",
            worktree: "/tmp/SHOP-1.1",
            port: None,
        },
    )
    .await
    .unwrap();
    // Handed back: the agent in charge has it to review, nothing at work.
    sqlx::query("UPDATE helpers SET status = 'being_reviewed' WHERE ticket_id = ?")
        .bind(&t.id)
        .execute(w.board.store().pool())
        .await
        .unwrap();
    stop(&w, &agent(&t, &shell)).await;
    assert_eq!(w.status("SHOP-1").await, Status::Idle);
    w.stop().await;
}

/// A stage run that isn't an Agent stage's (here a Delivery's) gets no reminder.
#[tokio::test(flavor = "multi_thread")]
async fn no_reminder_outside_an_agent_stage() {
    let w = TestService::start().await;
    let (t, shell) = in_dev(&w).await;
    sqlx::query("UPDATE stage_runs SET kind = 'delivery' WHERE ticket_id = ? AND ended_at IS NULL")
        .bind(&t.id)
        .execute(w.board.store().pool())
        .await
        .unwrap();
    stop(&w, &agent(&t, &shell)).await;
    assert_eq!(w.status("SHOP-1").await, Status::Idle);
    w.stop().await;
}
