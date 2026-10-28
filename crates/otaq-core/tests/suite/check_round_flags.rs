//! The card's facts for a Check stage with several check agents (T-0531; d0130): on the test flow
//! (`tests/flows/every-kind.yaml`) Security lists Security reviewer and Validator. While the card is in the check, its
//! flags carry each agent's latest row of the round with its tool; once the round failed and the card is back in Dev,
//! the round says so with each verdict, and the card's one-verdict line is left out. Validate (one agent) has none.
//! Every tool is the stand-in of `tests/fixtures/stand-in`; repos, `OTAQ_HOME` and the tools' home are temporary.
//!
//! Unix only: the stand-in agent tools are `sh` scripts started by name, which Windows can't run (Q598).
#![cfg(unix)]

use std::path::Path;
use std::time::Duration;

use crate::common::stand_in;
use otaq_core::board::hand_over::HandOver;
use otaq_core::board::tickets::Ticket;
use otaq_core::domain::{Actor, Status, ValidationOutcome, ValidationTrigger};
use otaq_core::flow::{FlowFile, yaml};
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::{TestService, said};
use otaq_core::session::{SessionInfo, SessionKind};
use tempfile::TempDir;

async fn world() -> (TestService, TempDir) {
    let dir = TempDir::new().unwrap();
    let launch = stand_in::launch(&dir.path().join("bin"), dir.path());
    let w = TestService::start_with(move |config| ServiceConfig {
        launch: Some(launch),
        ..config
    })
    .await;
    w.add_project("shop-web").await;
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/flows/every-kind.yaml");
    let mut file: FlowFile = yaml::read(&std::fs::read_to_string(path).unwrap()).unwrap();
    let security = file.flow.stages.iter_mut().find(|s| s.id == "security");
    security.unwrap().skill = Some("validate".into());
    w.board.use_flow_for_tests("SHOP", &file).await;
    (w, dir)
}

async fn ticket(w: &TestService, t: &Ticket) -> Ticket {
    w.board.ticket(&t.key.to_string()).await.unwrap()
}

/// The `n` running checkers of the ticket, oldest first (the stage's order).
async fn checkers(w: &TestService, t: &Ticket, n: usize) -> Vec<SessionInfo> {
    for _ in 0..500 {
        let running = w.sessions.running_all_of(&t.id, SessionKind::Validator);
        if running.len() == n {
            return running;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("{} has no {n} checkers", t.key);
}

async fn verdict(
    w: &TestService,
    t: &Ticket,
    session: &SessionInfo,
    pass: bool,
    text: &str,
) -> Ticket {
    let author = w.board.session_author(&session.id).await.unwrap();
    let hand_over: Option<HandOver> = w
        .board
        .stage_end_hand_over(&ticket(w, t).await, author, text, None, &[])
        .await
        .unwrap();
    w.validation
        .verdict(&t.key.to_string(), pass, hand_over.as_ref(), &session.id)
        .await
        .unwrap()
}

/// Each agent's (name, outcome) of the card's round, and whether its tool is known.
fn round_of(t: &Ticket) -> Option<Vec<(String, Option<ValidationOutcome>, bool)>> {
    let round = t.flags.check_round.as_ref()?;
    Some(
        round
            .agents
            .iter()
            .map(|a| (a.name.clone(), a.outcome, a.tool.is_some()))
            .collect(),
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn the_card_carries_each_check_agents_row_while_in_the_check_and_back_from_it() {
    let (w, _dir) = world().await;
    let t = w.create("SHOP", "Search notes").await;
    w.give_worktree(&t).await;
    w.force_stage(&t, "dev", Status::Working).await;
    let key = t.key.to_string();
    w.validation
        .submit(
            &key,
            said(&w.board, &key, "Search works.").await.as_ref(),
            &Actor::Agent(t.key.clone()),
            ValidationTrigger::Agent,
        )
        .await
        .unwrap();
    // Validate has one check agent: the card reads as today.
    let validator = checkers(&w, &t, 1).await.remove(0);
    assert_eq!(ticket(&w, &t).await.flags.check_round, None);
    verdict(&w, &t, &validator, true, "Validate: all good.").await;

    // Security's two check agents run.
    let security = checkers(&w, &t, 2).await;
    let running = ticket(&w, &t).await;
    assert_eq!(running.status, Status::Checking);
    let named = |name: &str, outcome| (name.to_owned(), outcome, true);
    assert_eq!(
        round_of(&running),
        Some(vec![
            named("Security reviewer", None),
            named("Validator", None)
        ])
    );
    let first = verdict(&w, &t, &security[0], false, "Fail: raw SQL in search.").await;
    assert_eq!(
        round_of(&first),
        Some(vec![
            named("Security reviewer", Some(ValidationOutcome::Fail)),
            named("Validator", None)
        ])
    );
    // The last verdict: back in Dev with both, the round naming where it is; no single verdict line.
    let back = verdict(&w, &t, &security[1], true, "Search matches.").await;
    assert_eq!(
        (back.place.stage(), back.status),
        (Some("dev"), Status::Working)
    );
    let round = back
        .flags
        .check_round
        .as_ref()
        .expect("back from the round");
    assert_eq!(round.back_in.as_deref(), Some("Dev"));
    assert_eq!(round.round, 1);
    assert!(!round.again);
    assert_eq!(
        round_of(&back),
        Some(vec![
            named("Security reviewer", Some(ValidationOutcome::Fail)),
            named("Validator", Some(ValidationOutcome::Pass))
        ])
    );
    assert_eq!(back.flags.verdict, None);
}
