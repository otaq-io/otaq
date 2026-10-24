//! An open question holds Needs you (T-0489; ATT-01, ATT-02; d0119): the dogfood run's sequence with the real
//! commands inside an agent session (an ordinary `sh`, never `claude`): `otaq ask human`, then Claude Code's status
//! line (`otaq statusline` with the saved payload, rate limits included), `otaq prime`, and the saved hook payloads
//! through `otaq hook --tool claude`. The ticket stays Asks you; a permission prompt shows on top and gives way to
//! the question at the next Stop.

use otaq_core::domain::Status;
use otaq_core::service::testing::{AgentShell, TestService};
use serde_json::{Value, json};

const CB: &str = env!("CARGO_BIN_EXE_otaq");
const QUESTION: &str = "Question 1 of about 4: which email provider?";

fn fixture(path: &str) -> Value {
    let path = format!(
        "{}/../otaq-core/tests/fixtures/claude/{path}",
        env!("CARGO_MANIFEST_DIR")
    );
    serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap()
}

/// Runs the hook of the saved payload `name` in `agent`'s session.
async fn hook(agent: &AgentShell, name: &str) {
    let payload = fixture(&format!("hooks/{name}"));
    let event = payload["hook_event_name"].as_str().unwrap();
    let ran = agent
        .run_with_stdin(
            &format!("'{CB}' hook --tool claude {event}"),
            &payload.to_string(),
        )
        .await;
    assert_eq!(ran.status, 0, "{ran:?}");
}

/// Claude Code running the status line in `agent`'s session, with an empty Claude Code folder (no command of the
/// user's runs).
async fn status_line(agent: &AgentShell, claude: &std::path::Path) {
    let mut payload = fixture("statusline/with_rate_limits.json");
    let wt = "/nonexistent/wt/shop-web/SHOP-1";
    payload["cwd"] = json!(wt);
    payload["workspace"] = json!({"current_dir": wt, "project_dir": wt});
    let ran = agent
        .run_with_stdin(
            &format!("CLAUDE_CONFIG_DIR='{}' '{CB}' statusline", claude.display()),
            &payload.to_string(),
        )
        .await;
    assert_eq!(ran.status, 0, "{ran:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_status_line_and_hooks_keep_the_question() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let t = w.create("SHOP", "Order confirmation email").await;
    w.force_status(&t, Status::Working).await;
    let agent = w.agent_shell(&t).await;
    let claude = tempfile::tempdir().unwrap();

    hook(&agent, "UserPromptSubmit.json").await;
    let ran = agent
        .run_with_stdin(
            &format!("'{CB}' ask human --option Postmark --option SES --recommended 1 --stdin"),
            QUESTION,
        )
        .await;
    assert_eq!(ran.status, 0, "{ran:?}");
    assert_eq!(w.status("SHOP-1").await, Status::AsksYou);

    // The status line re-renders right after the tool call (#37's ~250 ms), and the agent reads its context.
    status_line(&agent, claude.path()).await;
    assert_eq!(w.status("SHOP-1").await, Status::AsksYou, "the status line");
    let ran = agent.run(&format!("'{CB}' prime")).await;
    assert_eq!(ran.status, 0, "{ran:?}");
    assert_eq!(w.status("SHOP-1").await, Status::AsksYou, "a read");

    // It ends its turn as told.
    hook(&agent, "Stop-idle.json").await;
    status_line(&agent, claude.path()).await;
    let card = w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(card.status, Status::AsksYou, "the turn's Stop");
    assert_eq!(card.question.map(|q| q.text).as_deref(), Some(QUESTION));

    // A permission prompt on top, then the question again at the next Stop.
    hook(&agent, "UserPromptSubmit.json").await;
    hook(&agent, "Notification-permission_prompt.json").await;
    assert_eq!(w.status("SHOP-1").await, Status::NeedsYou);
    hook(&agent, "Stop-idle.json").await;
    let card = w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(card.status, Status::AsksYou);
    assert_eq!(card.signal.and_then(|s| s.text).as_deref(), Some(QUESTION));
    let strip = w.inbox.list().await.unwrap().items;
    assert_eq!(
        strip.iter().map(|i| i.reason.as_str()).collect::<Vec<_>>(),
        [format!("asks: {QUESTION}")]
    );
    w.stop().await;
}
