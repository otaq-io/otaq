//! `otaq hook --tool opencode <type>` (T-0354; ADP-03, ADP-09, USG-09, AGT-05): the OpenCode plugin's saved events
//! (`otaq-core/tests/fixtures/opencode/events/`) fed to the command inside an agent session (an ordinary `sh`), as
//! Otaq's OpenCode plugin runs it.

use otaq_core::domain::Status;
use otaq_core::service::testing::{AgentShell, TestService};
use serde_json::Value;

const CB: &str = env!("CARGO_BIN_EXE_otaq");

/// A saved OpenCode plugin event.
fn fixture(name: &str) -> Value {
    let path = format!(
        "{}/../otaq-core/tests/fixtures/opencode/events/{name}.json",
        env!("CARGO_MANIFEST_DIR")
    );
    serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap()
}

/// Runs the plugin's hook for the saved event `name` (its type is the event's `type`) in `agent`'s session.
async fn event(agent: &AgentShell, name: &str) {
    let payload = fixture(name);
    let kind = payload["type"].as_str().unwrap();
    let ran = agent
        .run_with_stdin(
            &format!("'{CB}' hook --tool opencode {kind}"),
            &payload.to_string(),
        )
        .await;
    assert_eq!(ran.status, 0, "{ran:?}");
    assert_eq!(ran.stdout, "", "hooks print nothing");
}

#[tokio::test(flavor = "multi_thread")]
async fn every_opencode_event() {
    let w = TestService::start().await;
    // The tool's hook mapping; the stage reminder before Idle (d0042 2026-10-28) has its own tests.
    w.signals.set_stage_reminders(false);
    w.add_project("shop-web").await;
    let shop1 = w.create("SHOP", "Order totals rounding").await;
    w.force_status(&shop1, Status::Idle).await;
    let agent = w.agent_shell(&shop1).await;

    // A prompt arrives and the turn starts → Working.
    event(&agent, "session.inbox.enqueued").await;
    assert_eq!(w.status("SHOP-1").await, Status::Working);
    event(&agent, "session.execution.started").await;
    assert_eq!(w.status("SHOP-1").await, Status::Working);

    // The model that runs the step is the session's model that ran (AGT-05).
    event(&agent, "session.step.started").await;
    let session = w.sessions.stored(&agent.session).await.unwrap();
    assert_eq!(session.model_ran.as_deref(), Some("opencode/big-pickle"));

    // A permission prompt → Needs you; the same event again (a plugin set up twice) is ignored.
    event(&agent, "permission.asked").await;
    assert_eq!(w.status("SHOP-1").await, Status::NeedsYou);
    event(&agent, "session.inbox.enqueued").await;
    assert_eq!(
        w.status("SHOP-1").await,
        Status::NeedsYou,
        "a duplicate id changes nothing"
    );
    event(&agent, "session.execution.started").await;
    w.force_status(&shop1, Status::Working).await;

    // A question of the tool's own → handled like Needs you (as Copilot's; `ask human` is Asks you).
    event(&agent, "form.created").await;
    assert_eq!(w.status("SHOP-1").await, Status::NeedsYou);
    w.force_status(&shop1, Status::Working).await;

    // Tokens: the running totals are stored on the session (cache writes as input, the cost ignored); a later
    // report adds only what is new.
    event(&agent, "session.usage.updated").await;
    assert_eq!(w.status("SHOP-1").await, Status::Working);
    assert_eq!(usage(&w).await, (1350, 8000, 340, Some(56)));
    let mut later = fixture("session.usage.updated");
    later["id"] = "evt_later".into();
    later["data"]["tokens"]["output"] = 400.into();
    let ran = agent
        .run_with_stdin(
            &format!("'{CB}' hook --tool opencode session.usage.updated"),
            &later.to_string(),
        )
        .await;
    assert_eq!(ran.status, 0);
    assert_eq!(usage(&w).await, (1350, 8000, 400, Some(56)));

    // The turn ends → Idle.
    event(&agent, "session.execution.succeeded").await;
    assert_eq!(w.status("SHOP-1").await, Status::Idle);

    // A failed turn is logged; a retry that isn't a rate limit changes nothing.
    w.force_status(&shop1, Status::Working).await;
    event(&agent, "session.execution.failed").await;
    event(&agent, "session.retry.scheduled-transport").await;

    // A rate-limit retry → Waiting for limit reset.
    w.force_status(&shop1, Status::Working).await;
    event(&agent, "session.retry.scheduled-rate_limit").await;
    assert_eq!(w.status("SHOP-1").await, Status::WaitingForLimitReset);
    w.stop().await;
}

/// Today's stored tokens: input, cached, output, reasoning.
async fn usage(w: &TestService) -> (u64, u64, u64, Option<u64>) {
    let day = otaq_core::usage::local_day(otaq_core::store::now_ms());
    let t = w.usage.day_total(&day).await.unwrap();
    (t.input, t.cached, t.output, t.reasoning)
}
