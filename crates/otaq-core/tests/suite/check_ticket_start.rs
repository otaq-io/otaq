//! A check stage runs Otaq's `check-ticket`, then its task skill (tuq decisions 113, 081; 0.3 regression F-U2-2): a
//! check stage whose skill is the user's (`team-validate`) starts its checker with `/otaq:check-ticket`, and the
//! stage's skill waits to be typed once that first turn ends. The first turn's end isn't a check with no result; a
//! verdict before the task skill was typed is refused, telling the checker to end its turn; once typed, the verdict
//! lands. The task skill waits from the session's first instant, so a first turn that ends at once still gets it.
//! Otaq's own `validate` stage starts alone, as before (validation.rs), and work stages are unchanged
//! (task_skill_gate.rs, handoffs.rs).
//!
//! `claude` stands in as `sh`: it prints its arguments, then is a shell the test types into (the checker's requests
//! go through the socket with `nc`, its turn ends with Claude Code's real `Stop` hook). Unix only.
#![cfg(unix)]

use std::path::Path;

use otaq_core::agent_tools::LaunchContext;
use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::board::settings::WorkflowStep;
use otaq_core::board::tickets::Ticket;
use otaq_core::domain::{Actor, Status, ValidationTrigger};
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::{AgentShell, TestService, said};
use otaq_core::session::SessionKind;
use otaq_core::skills::testing::claude_skill;
use serde_json::{Value, json};

/// The hook's payload as Claude Code sends it at a turn's end.
fn stop_payload() -> Value {
    serde_json::from_str(
        &std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/claude/hooks/Stop-idle.json"),
        )
        .unwrap(),
    )
    .unwrap()
}

fn request(method: &str, params: Value) -> String {
    json!({"protocol_version": otaq_core::protocol::PROTOCOL_VERSION, "id": 1,
        "request": {"method": method, "params": params}})
    .to_string()
}

/// A service whose `claude` prints its arguments, runs `first` (shell syntax; the first turn), then stays a shell.
async fn world(first: impl FnOnce(&ServiceConfig) -> String) -> TestService {
    TestService::start_with(|config| {
        let script = format!("printf 'ARGS %s\\n' \"$*\"; {}; exec sh", first(&config));
        ServiceConfig {
            launch: Some(LaunchContext {
                bin_dir: "/nonexistent".into(),
                plugin_dir: "/nonexistent/plugin".into(),
                path: std::env::var_os("PATH").unwrap_or_default(),
                claude: ClaudeLaunch {
                    program: vec!["sh".into(), "-c".into(), script, "sh".into()],
                    config_dir: None,
                },
            }),
            ..config
        }
    })
    .await
}

/// SHOP whose Validate stage names the user's `team-validate` (Claude Code has it), and a working ticket submitted:
/// its checker runs.
async fn submitted(w: &TestService) -> (Ticket, String) {
    w.add_project("shop-web").await;
    let folder = otaq_core::skills::testing::skill_folder(w.data.root(), "team-validate");
    w.use_step_skill(WorkflowStep::Validate, &folder).await;
    let t = w.create("SHOP", "Cart totals").await;
    w.give_worktree(&t).await;
    w.force_status(&t, Status::Working).await;
    let key = t.key.to_string();
    w.validation
        .submit(
            &key,
            said(&w.board, &key, "Cart totals add up.").await.as_ref(),
            &Actor::Agent(t.key.clone()),
            ValidationTrigger::Agent,
        )
        .await
        .unwrap();
    let checker = w
        .sessions
        .running_of(&t.id, SessionKind::Validator)
        .expect("a checker runs");
    (w.board.ticket(&key).await.unwrap(), checker.id)
}

async fn send(w: &TestService, shell: &AgentShell, method: &str, params: Value) -> Value {
    let ran = shell
        .send_raw(&w.data.socket_path(), &request(method, params))
        .await;
    serde_json::from_str(ran.stdout.lines().next().unwrap_or_default())
        .unwrap_or_else(|e| panic!("{e}: {ran:?}"))
}

async fn until(what: &str, mut check: impl AsyncFnMut() -> bool) {
    tokio::time::timeout(std::time::Duration::from_secs(30), async {
        while !check().await {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("never: {what}"));
}

const UNTYPED: &str = "Your stage's skill `/team-validate` hasn't run yet: end your turn now, and Otaq types it next.";

/// `/otaq:check-ticket` first, `/team-validate` waiting; a verdict in the first turn is refused (before the read
/// gate); the turn's end types the task skill and is no "gave no result"; then the verdict lands.
#[tokio::test(flavor = "multi_thread")]
async fn check_ticket_starts_then_the_task_skill_then_the_verdict_lands() {
    let w = world(|_| ":".to_owned()).await;
    let (t, checker) = submitted(&w).await;
    let key = t.key.to_string();
    let out = w
        .wait_for_output(&checker, &claude_skill("otaq:check-ticket"))
        .await;
    assert!(!out.contains("team-validate"), "{out}");
    assert_eq!(
        crate::common::waiting_texts(&w.deliveries, &checker).await,
        ["/team-validate"]
    );
    let shell = w.shell_in(&checker);

    // A verdict in the first turn: refused, the task skill's refusal first; the check goes on.
    let early = send(
        &w,
        &shell,
        "validation.verdict",
        json!({"key": key, "pass": true, "report": "Pass."}),
    )
    .await;
    assert_eq!(early["error"]["kind"], "refused", "{early}");
    assert_eq!(early["error"]["message"], UNTYPED, "{early}");
    let primed = send(&w, &shell, "ticket.prime_validate", json!({"key": key})).await;
    assert!(primed.get("error").is_none(), "{primed}");
    let early = send(
        &w,
        &shell,
        "validation.verdict",
        json!({"key": key, "pass": true, "report": "Pass."}),
    )
    .await;
    assert_eq!(early["error"]["message"], UNTYPED, "{early}");
    assert_eq!(w.status(&key).await, Status::Checking);

    // The first turn ends: the task skill is typed, and the check goes on (no "gave no result").
    let stop = send(
        &w,
        &shell,
        "hook",
        json!({"tool": "claude", "event": "Stop", "payload": stop_payload()}),
    )
    .await;
    assert!(stop.get("error").is_none(), "{stop}");
    w.wait_for_output(&checker, "/team-validate").await;
    until("the task skill typed", async || {
        w.deliveries.pending(&checker).await.unwrap().is_empty()
    })
    .await;
    assert_eq!(w.status(&key).await, Status::Checking);
    assert!(w.sessions.info(&checker).unwrap().is_running());

    // The task skill's turn gives the verdict: it lands.
    let verdict = send(
        &w,
        &shell,
        "validation.verdict",
        json!({"key": key, "pass": true, "report": "Pass: `node test.js` printed ok."}),
    )
    .await;
    assert!(verdict.get("error").is_none(), "{verdict}");
    until("the check passed", async || {
        w.status(&key).await != Status::Checking
    })
    .await;
    let after = w.board.ticket(&key).await.unwrap();
    assert_ne!(after.status, Status::CheckNoResult);
    assert_eq!(after.status, Status::WaitingForYou);
    w.stop().await;
}

/// The start race: the task skill is stored before the program starts, so a first turn that ends at once (before the
/// start has even recorded the checker) still gets it typed, and isn't a check with no result.
#[tokio::test(flavor = "multi_thread")]
async fn a_first_turn_that_ends_at_once_still_gets_the_task_skill() {
    let w = world(|config| {
        let stop = request(
            "hook",
            json!({"tool": "claude", "event": "Stop", "payload": stop_payload()}),
        );
        format!(
            "printf '%s\\n' '{stop}' | nc -U -w 5 '{}' > /dev/null",
            config.data.socket_path().display()
        )
    })
    .await;
    let (t, checker) = submitted(&w).await;
    w.wait_for_output(&checker, "/team-validate").await;
    until("the task skill typed", async || {
        w.deliveries.pending(&checker).await.unwrap().is_empty()
    })
    .await;
    assert_eq!(w.status(&t.key.to_string()).await, Status::Checking);
    assert!(w.sessions.info(&checker).unwrap().is_running());
    w.stop().await;
}

/// A checker of Otaq's own `validate` is as before: nothing waits for it, and its turn's end without a verdict is
/// still a check with no result.
#[tokio::test(flavor = "multi_thread")]
async fn otaqs_validate_starts_alone_and_a_turn_without_a_verdict_is_no_result() {
    let w = world(|_| ":".to_owned()).await;
    w.add_project("shop-web").await;
    let t = w.create("SHOP", "Cart totals").await;
    w.give_worktree(&t).await;
    w.force_status(&t, Status::Working).await;
    let key = t.key.to_string();
    w.validation
        .submit(
            &key,
            said(&w.board, &key, "Cart totals add up.").await.as_ref(),
            &Actor::Agent(t.key.clone()),
            ValidationTrigger::Agent,
        )
        .await
        .unwrap();
    let checker = w
        .sessions
        .running_of(&t.id, SessionKind::Validator)
        .unwrap()
        .id;
    let out = w
        .wait_for_output(&checker, &claude_skill("otaq:validate"))
        .await;
    assert!(!out.contains("check-ticket"), "{out}");
    assert!(w.deliveries.pending(&checker).await.unwrap().is_empty());
    let shell = w.shell_in(&checker);
    send(
        &w,
        &shell,
        "hook",
        json!({"tool": "claude", "event": "Stop", "payload": stop_payload()}),
    )
    .await;
    until("no result", async || {
        w.status(&key).await == Status::CheckNoResult
    })
    .await;
    w.stop().await;
}
