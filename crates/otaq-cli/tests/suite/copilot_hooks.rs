//! `otaq hook --tool copilot <event>` (T-0352; ADP-03, ADP-08, d0088): the saved Copilot payloads
//! (`otaq-core/tests/fixtures/copilot/`) fed to the command inside an agent session (an ordinary `sh`), as
//! `plugin/copilot/hooks/hooks.json` runs it. An `errorOccurred` reads the session's `events.jsonl` in Copilot's home,
//! here a temporary one.

use std::path::{Path, PathBuf};

use otaq_core::domain::Status;
use otaq_core::service::testing::{AgentShell, TestService};
use serde_json::Value;

const CB: &str = env!("CARGO_BIN_EXE_otaq");

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../otaq-core/tests/fixtures/copilot")
}

/// A saved Copilot hook payload.
fn fixture(name: &str) -> Value {
    let path = fixtures().join("hooks").join(name);
    serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap()
}

/// Runs Copilot's hook `event` with the saved payload `name` in `agent`'s session.
async fn hook(agent: &AgentShell, event: &str, name: &str) {
    let ran = agent
        .run_with_stdin(
            &format!("'{CB}' hook --tool copilot {event}"),
            &fixture(name).to_string(),
        )
        .await;
    assert_eq!(ran.status, 0, "{ran:?}");
    assert_eq!(ran.stdout, "", "hooks print nothing");
}

#[tokio::test(flavor = "multi_thread")]
async fn every_copilot_hook() {
    // Copilot's home is `<home>/.copilot` unless COPILOT_HOME says otherwise: never write into that one.
    if std::env::var_os("COPILOT_HOME").is_some_and(|v| !v.is_empty()) {
        eprintln!("COPILOT_HOME is set: skipped, so the test never writes into it");
        return;
    }
    let tool_home = tempfile::tempdir().unwrap();
    let home = tool_home.path().to_path_buf();
    let w = TestService::start_with(move |mut config| {
        config.tool_home = Some(home);
        config
    })
    .await;
    // The tool's hook mapping; the stage reminder before Idle (d0042 2026-10-28) has its own tests.
    w.signals.set_stage_reminders(false);
    w.add_project("shop-web").await;
    let shop1 = w.create("SHOP", "Coupon field").await;
    w.force_status(&shop1, Status::Working).await;
    let agent = w.agent_shell(&shop1).await;

    // A permission prompt (camelCase `sessionId` only) → Needs you; the next prompt → working.
    hook(
        &agent,
        "Notification",
        "Notification-permission_prompt.json",
    )
    .await;
    assert_eq!(w.status("SHOP-1").await, Status::NeedsYou);
    hook(&agent, "UserPromptSubmit", "UserPromptSubmit.json").await;
    assert_eq!(w.status("SHOP-1").await, Status::Working);

    // A question Copilot shows → Needs you, with its prompt.
    hook(
        &agent,
        "Notification",
        "Notification-elicitation_dialog.json",
    )
    .await;
    assert_eq!(w.status("SHOP-1").await, Status::NeedsYou);
    hook(&agent, "UserPromptSubmit", "UserPromptSubmit.json").await;

    // Other notifications, Copilot's own start, and its permission check (fired for pre-approved commands too)
    // change nothing.
    hook(&agent, "Notification", "Notification-shell_completed.json").await;
    hook(&agent, "SessionStart", "SessionStart.json").await;
    hook(&agent, "permissionRequest", "permissionRequest.json").await;
    assert_eq!(w.status("SHOP-1").await, Status::Working);

    // Stop → the turn ended: Idle.
    hook(&agent, "Stop", "Stop.json").await;
    assert_eq!(w.status("SHOP-1").await, Status::Idle);
    hook(&agent, "UserPromptSubmit", "UserPromptSubmit.json").await;

    // A retry's error with no `session.error` written → nothing; one that isn't recoverable → failed (logged,
    // the status stays).
    hook(&agent, "errorOccurred", "errorOccurred-rate_limit.json").await;
    assert_eq!(w.status("SHOP-1").await, Status::Working);
    hook(&agent, "errorOccurred", "errorOccurred-unrecoverable.json").await;
    assert_eq!(w.status("SHOP-1").await, Status::Working);

    // The last retry's error, with the `session.error` of type `rate_limit` in the session's events file →
    // Waiting for limit reset.
    let error = fixture("errorOccurred-rate_limit.json");
    let folder = tool_home
        .path()
        .join(".copilot/session-state")
        .join(error["sessionId"].as_str().unwrap());
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::copy(
        fixtures().join("events/rate_limit.jsonl"),
        folder.join("events.jsonl"),
    )
    .unwrap();
    hook(&agent, "errorOccurred", "errorOccurred-rate_limit.json").await;
    assert_eq!(w.status("SHOP-1").await, Status::WaitingForLimitReset);
    w.stop().await;
}
