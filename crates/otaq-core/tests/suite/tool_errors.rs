//! A tool stopped by a provider's error (F10): a usage or quota limit goes through the limit handling (only that
//! tool's agents wait, Waiting for limit reset, the tool's limit with the error's retry time); any other error stops
//! the card on Idle with the tool's own message. A message waiting for the session stays held either way, and goes
//! once the session continues. The hooks come from inside an `sh` standing in for the agent, as the plugins send
//! them; the payloads are the adapters' fixtures (`tests/fixtures/`).

use std::path::Path;
use std::time::Duration;

use otaq_core::agent_tools::Tool;
use otaq_core::board::tickets::Ticket;
use otaq_core::caller::Caller;
use otaq_core::domain::Status;
use otaq_core::service::testing::{AgentShell, TestService};
use otaq_core::session::deliveries::DeliveryKind;
use otaq_core::store::now_ms;
use otaq_core::usage::limit::{LimitKind, ResetSource};
use serde_json::{Value, json};

fn fixture(path: &str) -> Value {
    let path = format!("{}/tests/fixtures/{path}", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// OpenCode's plugin event `name`, as `otaq hook --tool opencode <type>` sends it from inside the session.
async fn opencode_event(w: &TestService, agent: &AgentShell, name: &str, created: Option<i64>) {
    let mut event = fixture(&format!("opencode/events/{name}.json"));
    if let Some(created) = created {
        event["created"] = json!(created);
    }
    let kind = event["type"].as_str().unwrap().to_owned();
    let request = json!({"protocol_version": otaq_core::protocol::PROTOCOL_VERSION, "id": 1, "request":
        {"method": "hook", "params": {"tool": "opencode", "event": kind, "payload": event}}});
    let ran = agent
        .send_raw(&w.data.socket_path(), &request.to_string())
        .await;
    assert!(ran.stdout.contains("\"reply\":\"ok\""), "{name}: {ran:?}");
}

async fn eventually(what: &str, mut check: impl AsyncFnMut() -> bool) {
    tokio::time::timeout(Duration::from_secs(10), async {
        while !check().await {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("{what} never happened"));
}

/// A working SHOP ticket, its agent's `sh`, and a message waiting for it (a ticket change that would leave
/// `marker` once typed).
async fn working_with_a_waiting_message(
    w: &TestService,
    title: &str,
    marker: &Path,
) -> (Ticket, AgentShell) {
    let ticket = w.create("SHOP", title).await;
    w.force_status(&ticket, Status::Working).await;
    let agent = w.agent_shell(&ticket).await;
    w.deliveries
        .queue_delivery(
            &agent.session,
            DeliveryKind::TicketChange,
            &format!("touch '{}'", marker.display()),
        )
        .await
        .unwrap();
    (ticket, agent)
}

/// The tool's message an Idle a failed turn left shows.
async fn idle_error(w: &TestService, key: &str) -> Option<String> {
    let text = w.board.ticket(key).await.unwrap().signal?.text?;
    otaq_core::signals::tool_error(&text).map(str::to_owned)
}

#[tokio::test(flavor = "multi_thread")]
async fn an_opencode_quota_error_waits_for_the_limit_reset() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let marker = w.home.path().join("delivered");
    let (_ticket, agent) = working_with_a_waiting_message(&w, "Order email", &marker).await;
    let other = w.create("SHOP", "Coupon field").await;
    w.force_status(&other, Status::Working).await;

    // Gemini's daily quota, as OpenCode 2.2.5 stops on it: `session.execution.failed` with `provider.quota`.
    let at = now_ms();
    opencode_event(&w, &agent, "session.execution.failed-quota", Some(at)).await;
    eventually("the limit recorded", async || {
        otaq_core::usage::plan::limit_of(w.board.store(), Tool::OpenCode)
            .await
            .unwrap()
            .is_some()
    })
    .await;
    assert_eq!(w.status("SHOP-1").await, Status::WaitingForLimitReset);
    let limit = otaq_core::usage::plan::limit_of(w.board.store(), Tool::OpenCode)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(limit.kind, LimitKind::Other);
    assert!(limit.text.contains("Quota exceeded"), "{}", limit.text);
    // The reset is the provider's retry delay after the event.
    assert_eq!(
        (limit.resets_at, limit.reset_source),
        (Some(at + 30_285_000), Some(ResetSource::ToolSignal))
    );
    // Only OpenCode is at its limit; the other agent keeps working, and the stopped one raises no item.
    assert!(
        otaq_core::usage::plan::limit_of(w.board.store(), Tool::Claude)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(w.status("SHOP-2").await, Status::Working);
    assert!(w.needs_you.list().await.unwrap().items.is_empty());
    assert_eq!(
        w.plan_limit.stopped(Tool::OpenCode).await.unwrap(),
        1,
        "the banner's agents stopped"
    );
    // The waiting message stays held: typing it would only hit the limit again.
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(w.deliveries.pending(&agent.session).await.unwrap().len(), 1);
    assert!(!marker.exists());
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_opencode_provider_error_stops_the_card_on_idle_with_its_message() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let marker = w.home.path().join("delivered");
    let (_ticket, agent) = working_with_a_waiting_message(&w, "Order email", &marker).await;

    // A refused key (`provider.auth`): not a limit.
    opencode_event(&w, &agent, "session.execution.failed", None).await;
    assert_eq!(w.status("SHOP-1").await, Status::Idle);
    assert_eq!(
        idle_error(&w, "SHOP-1").await.as_deref(),
        Some("Invalid API key")
    );
    assert!(
        otaq_core::usage::plan::limit_of(w.board.store(), Tool::OpenCode)
            .await
            .unwrap()
            .is_none()
    );
    let items = w.needs_you.list().await.unwrap().items;
    assert_eq!(
        items
            .iter()
            .map(|i| (i.key.to_string(), i.reason.clone()))
            .collect::<Vec<_>>(),
        [(
            "SHOP-1".to_owned(),
            "idle · stopped on an error: “Invalid API key”".to_owned()
        )]
    );
    // The waiting message stays held with the card: not lost, not typed.
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(w.deliveries.pending(&agent.session).await.unwrap().len(), 1);
    assert!(!marker.exists());

    // The session continues (the user typed into it): its next turn ends, and the message goes then.
    opencode_event(&w, &agent, "session.execution.started", None).await;
    assert_eq!(w.status("SHOP-1").await, Status::Working);
    opencode_event(&w, &agent, "session.execution.succeeded", None).await;
    eventually("the message typed", async || marker.exists()).await;
    eventually("the message no longer waiting", async || {
        w.deliveries
            .pending(&agent.session)
            .await
            .unwrap()
            .is_empty()
    })
    .await;
    w.stop().await;
}

/// Claude Code's `StopFailure` and Codex's failed turn in its session file stop the card the same way.
#[tokio::test(flavor = "multi_thread")]
async fn claude_code_and_codex_errors_stop_the_card_on_idle() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let marker = w.home.path().join("delivered");
    let (claude, agent) = working_with_a_waiting_message(&w, "Order email", &marker).await;
    let caller = Caller::Agent {
        ticket: claude.key.clone(),
        ticket_id: claude.id.clone(),
        session: agent.session.clone(),
    };
    let failure = fixture("claude/hooks/StopFailure-other.json");
    w.signals
        .hook(&caller, Tool::Claude, "StopFailure", &failure)
        .await
        .unwrap();
    assert_eq!(w.status("SHOP-1").await, Status::Idle);
    assert_eq!(
        idle_error(&w, "SHOP-1").await.as_deref(),
        Some("API Error: 500")
    );
    assert!(!w.signals.is_idle_between_turns(&agent.session));
    assert_eq!(w.deliveries.pending(&agent.session).await.unwrap().len(), 1);

    let codex = w.create("SHOP", "Coupon field").await;
    w.force_status(&codex, Status::Working).await;
    let shell = w.agent_shell(&codex).await;
    let caller = Caller::Agent {
        ticket: codex.key.clone(),
        ticket_id: codex.id.clone(),
        session: shell.session.clone(),
    };
    let lines = std::fs::read_to_string(format!(
        "{}/tests/fixtures/codex/session_file/failed_signed_out.jsonl",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap();
    let events = w
        .adapters
        .get(Tool::Codex)
        .map_watched_lines(&lines, Default::default());
    w.signals
        .apply_events(&caller, Tool::Codex, &events)
        .await
        .unwrap();
    assert_eq!(w.status("SHOP-2").await, Status::Idle);
    let error = idle_error(&w, "SHOP-2").await.unwrap();
    assert!(
        error.starts_with("unexpected status 401 Unauthorized"),
        "{error}"
    );
    w.stop().await;
}
