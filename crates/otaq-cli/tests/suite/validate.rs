//! `otaq check pass|fail --stdin` and `otaq prime --validate` (T-0184, T-0406; VAL-03, CLI-05, AGM-10; d0063), from
//! a validator session (an ordinary `sh` started as one), from the ticket's agent and from the user.

use std::process::Command;

use otaq_core::data_dir::DataDir;
use otaq_core::domain::Status;
use otaq_core::naming;
use otaq_core::service::testing::TestService;
use otaq_core::session::SessionKind;
use serde_json::{Value, json};

const CB: &str = env!("CARGO_BIN_EXE_otaq");

#[tokio::test(flavor = "multi_thread")]
async fn only_the_validator_reports_a_verdict() {
    let w = TestService::start().await;
    w.shop_sample().await;
    let shop19 = w.board.ticket("SHOP-19").await.unwrap();
    let validator = w.shell(&shop19, SessionKind::Validator).await;
    let agent = w.agent_shell(&shop19).await;
    w.validating_with(&shop19, &validator.session).await;
    let only = "Only an agent check can give a verdict";

    // The user, from their own terminal.
    let out = Command::new(CB)
        .args(["check", "pass", "--stdin", "--json"])
        .env(DataDir::env_var(), w.data.root())
        .env(naming::ticket_env_var(), "SHOP-19")
        .current_dir(w.home.path())
        .output()
        .unwrap();
    assert!(!out.status.success());
    let e: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(e["error"]["message"], only);
    // The ticket's agent: its Dev session never gives a verdict (Q269 names the stage).
    let ran = agent
        .run_with_stdin(&format!("'{CB}' check pass --stdin --json"), "looks fine")
        .await;
    assert_ne!(ran.status, 0);
    let e: Value = serde_json::from_str(&ran.stdout).unwrap();
    assert_eq!(
        e["error"]["message"],
        "SHOP-19's Dev session can't give a check verdict"
    );
    assert_eq!(w.status("SHOP-19").await, Status::Checking);

    // The validator: it reads its context, then fails the round; the findings go back to the agent.
    let ran = validator.run(&format!("'{CB}' prime --validate")).await;
    assert_eq!(ran.status, 0, "{ran:?}");
    assert!(
        ran.stdout.starts_with("# Validate SHOP-19 · Price table"),
        "{ran:?}"
    );
    // A missing `--report-file` is refused before anything is read or sent (CMT-06): the round stays open.
    let missing = validator
        .run_with_stdin(
            &format!("'{CB}' check fail --stdin --report-file /nonexistent/test-run.txt"),
            "src/prices.ts:3 not sorted",
        )
        .await;
    assert_ne!(missing.status, 0, "{missing:?}");
    assert_eq!(w.status("SHOP-19").await, Status::Checking);
    let folder = tempfile::TempDir::new().unwrap();
    let run = folder.path().join("test-run.txt");
    std::fs::write(&run, "FAILED prices::sorted\n").unwrap();
    let ran = validator
        .run_with_stdin(
            &format!(
                "'{CB}' check fail --stdin --report-file '{}'",
                run.display()
            ),
            "src/prices.ts:3 not sorted",
        )
        .await;
    assert_eq!(
        (ran.status, ran.stdout.trim()),
        (0, "SHOP-19: Validate failed. End your turn."),
        "{ran:?}"
    );
    assert_eq!(w.status("SHOP-19").await, Status::Working);
    // The findings are the checker's comment with its report; the agent is told only the round (CMT-05).
    let comment = w.board.ticket_comments("SHOP-19").await.unwrap().comments;
    assert_eq!(comment[0].body, "src/prices.ts:3 not sorted");
    assert_eq!(comment[0].report.map(|r| r.lines), Some(1));
    let pending = w.deliveries.pending(&agent.session).await.unwrap();
    assert_eq!(
        pending[0].text, "Validate failed round 1. Run `otaq prime`.",
        "{pending:?}"
    );

    // Its turn ends: the validator is done.
    // Typed, not awaited: the service ends the session the command runs in.
    let payload = json!({"hook_event_name": "Stop", "background_tasks": [], "session_crons": []});
    let line = format!("printf '%s' '{payload}' | '{CB}' hook --tool claude Stop\r");
    w.sessions
        .input(&validator.session, line.as_bytes())
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        while w
            .sessions
            .running_of(&shop19.id, SessionKind::Validator)
            .is_some()
        {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the validator ends after its verdict turn");
    w.stop().await;
}

/// Two check agents in one round (T-0527, d0130): each verdict records its own session's and its reply names the
/// check as with one ("SHOP-19: Validate passed. End your turn."); the round moves only on the last one. A third
/// checker that isn't in the round is refused.
#[tokio::test(flavor = "multi_thread")]
async fn each_check_agent_gives_its_own_verdict() {
    let w = TestService::start().await;
    w.shop_sample().await;
    let shop19 = w.board.ticket("SHOP-19").await.unwrap();
    let first = w.shell(&shop19, SessionKind::Validator).await;
    let second = w.shell(&shop19, SessionKind::Validator).await;
    let stranger = w.shell(&shop19, SessionKind::Validator).await;
    let _agent = w.agent_shell(&shop19).await;
    w.validating_with_agents(
        &shop19,
        &[
            ("Security reviewer", &first.session),
            ("Validator", &second.session),
        ],
    )
    .await;
    for checker in [&first, &second, &stranger] {
        let ran = checker.run(&format!("'{CB}' prime --validate")).await;
        assert_eq!(ran.status, 0, "{ran:?}");
    }

    let ran = stranger
        .run_with_stdin(&format!("'{CB}' check pass --stdin --json"), "fine")
        .await;
    assert_ne!(ran.status, 0);
    let e: Value = serde_json::from_str(&ran.stdout).unwrap();
    assert_eq!(e["error"]["message"], "This isn't SHOP-19's running check");

    let ran = first
        .run_with_stdin(&format!("'{CB}' check pass --stdin"), "no unescaped SQL")
        .await;
    assert_eq!(
        (ran.status, ran.stdout.trim()),
        (0, "SHOP-19: Validate passed. End your turn."),
        "{ran:?}"
    );
    assert_eq!(
        w.status("SHOP-19").await,
        Status::Checking,
        "the other checks on"
    );
    let again = first
        .run_with_stdin(&format!("'{CB}' check fail --stdin"), "changed my mind")
        .await;
    assert_ne!(again.status, 0, "one verdict per check agent: {again:?}");

    let ran = second
        .run_with_stdin(&format!("'{CB}' check fail --stdin"), "prices not sorted")
        .await;
    assert_eq!(
        (ran.status, ran.stdout.trim()),
        (0, "SHOP-19: Validate failed. End your turn."),
        "{ran:?}"
    );
    assert_eq!(w.status("SHOP-19").await, Status::Working);
    w.stop().await;
}
