//! A check stage runs one agent, like an Agent stage: on the test flow (`tests/flows/every-kind.yaml`) Validate's
//! agent is Validator and Security's is Security reviewer, two checks one after the other. Entering a check starts one
//! read-only checker on the check's agent, with the stage's skill; its verdict ends the round; each check counts its
//! own rounds, and a validator's `otaq prime --validate` reads its own stage's round. A checker held for its tool's
//! limit is the ticket's hold, one per ticket. Every tool is the stand-in of `tests/fixtures/stand-in`; repos,
//! `OTAQ_HOME` and the tools' home are temporary.
//!
//! Unix only: the stand-in agent tools are `sh` scripts started by name, which Windows can't run (Q598).
#![cfg(unix)]

use std::path::Path;
use std::time::Duration;

use crate::common::stand_in;
use otaq_core::agent_tools::Tool;
use otaq_core::agents::{AgentModel, HoldReason, SessionRole};
use otaq_core::board::hand_over::HandOver;
use otaq_core::board::tickets::Ticket;
use otaq_core::domain::{Actor, Status, ValidationOutcome, ValidationTrigger};
use otaq_core::flow::engine::RunResult;
use otaq_core::flow::{FlowFile, yaml};
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::{TestService, said};
use otaq_core::session::{SessionInfo, SessionKind};
use otaq_core::usage::limit::{LimitHit, LimitKind};
use tempfile::TempDir;

struct World {
    w: TestService,
    _dir: TempDir,
}

/// A service whose tools are the stand-in, with shop-web (SHOP) on the test flow; Security runs the shipped
/// `validate` skill, so its checker shows it loaded it.
async fn world() -> World {
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
    World { w, _dir: dir }
}

/// A ticket at work in Dev with a committed worktree.
async fn in_dev(w: &TestService, title: &str) -> Ticket {
    let t = w.create("SHOP", title).await;
    w.give_worktree(&t).await;
    w.force_stage(&t, "dev", Status::Working).await;
    ticket(w, &t).await
}

async fn ticket(w: &TestService, t: &Ticket) -> Ticket {
    w.board.ticket(&t.key.to_string()).await.unwrap()
}

fn at(t: &Ticket) -> (Option<&str>, Status) {
    (t.place.stage(), t.status)
}

/// The agent's `stage next` from Dev: Validate runs.
async fn submit(w: &TestService, t: &Ticket) -> Ticket {
    let key = t.key.to_string();
    w.validation
        .submit(
            &key,
            said(&w.board, &key, "Search works.").await.as_ref(),
            &Actor::Agent(t.key.clone()),
            ValidationTrigger::Agent,
        )
        .await
        .unwrap()
}

/// The ticket's one running checker, once it runs; never two.
async fn checker(w: &TestService, t: &Ticket) -> SessionInfo {
    for _ in 0..500 {
        let running = w.sessions.running_all_of(&t.id, SessionKind::Validator);
        assert!(
            running.len() < 2,
            "{} runs {} checkers",
            t.key,
            running.len()
        );
        if let [one] = running.as_slice() {
            return one.clone();
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("{} has no checker", t.key);
}

/// `session`'s verdict as `otaq check pass|fail` gives it: its text is its agent's comment.
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
        .stage_end_hand_over(&ticket(w, t).await, author, text, Vec::new(), &[])
        .await
        .unwrap();
    w.validation
        .verdict(&t.key.to_string(), pass, hand_over.as_ref(), &session.id)
        .await
        .unwrap()
}

fn agent_of(session: &SessionInfo) -> &str {
    session.agent.as_ref().map_or("", |a| a.name.as_str())
}

/// (agent, result) of each run of `stage`, oldest first.
async fn runs_of(
    w: &TestService,
    t: &Ticket,
    stage: &str,
) -> Vec<(Option<String>, Option<RunResult>)> {
    w.board
        .stage_runs(&t.key.to_string())
        .await
        .unwrap()
        .into_iter()
        .filter(|r| r.stage == stage)
        .map(|r| (r.agent, r.result))
        .collect()
}

/// (fails in a row, passed round) of the check `stage`.
async fn counter(w: &TestService, t: &Ticket, stage: &str) -> (u32, Option<u32>) {
    let count = w.board.check_counts(&t.id).await.unwrap().of(stage);
    (count.fails, count.passed_round)
}

/// The round `otaq prime --validate` gives the ticket's validator now.
async fn primed_round(w: &TestService, t: &Ticket) -> u32 {
    w.board
        .validate_prime(&t.key.to_string())
        .await
        .unwrap()
        .round
}

fn some(name: &str) -> Option<String> {
    Some(name.to_owned())
}

/// A flow with Validate (agent: Validator) runs one validator per round; Security, the check after it, runs its own
/// one agent. Each round is one checker, one run and one History record; the pass moves the card on once.
#[tokio::test(flavor = "multi_thread")]
async fn each_check_runs_one_checker_on_its_own_agent() {
    let world = world().await;
    let w = &world.w;
    let t = in_dev(w, "Search notes").await;
    submit(w, &t).await;
    let validator = checker(w, &t).await;
    assert_eq!(agent_of(&validator), "Validator");
    let security = verdict(w, &t, &validator, true, "Validate: all good.").await;
    assert_eq!(at(&security), (Some("security"), Status::Checking));

    let reviewer = checker(w, &t).await;
    assert_eq!(agent_of(&reviewer), "Security reviewer");
    w.wait_for_output(&reviewer.id, "RUNS otaq:validate").await;
    let passed = verdict(w, &t, &reviewer, true, "No unescaped SQL.").await;
    assert_eq!(at(&passed), (Some("qa"), Status::WaitingForYou));

    assert_eq!(
        runs_of(w, &t, "validate").await,
        [(some("Validator"), Some(RunResult::Passed))]
    );
    assert_eq!(
        runs_of(w, &t, "security").await,
        [(some("Security reviewer"), Some(RunResult::Passed))]
    );
    let records = w.board.validations(&t.key.to_string()).await.unwrap();
    let records: Vec<_> = records
        .iter()
        .map(|r| (r.round, r.agent.as_deref(), r.outcome, r.report.as_deref()))
        .collect();
    assert_eq!(
        records,
        [
            (
                1,
                Some("Security reviewer"),
                Some(ValidationOutcome::Pass),
                Some("No unescaped SQL.")
            ),
            (
                1,
                Some("Validator"),
                Some(ValidationOutcome::Pass),
                Some("Validate: all good.")
            ),
        ],
        "one record per round, newest first"
    );
    assert_eq!(counter(w, &t, "validate").await, (0, Some(1)));
    assert_eq!(counter(w, &t, "security").await, (0, Some(1)));
    world.w.stop().await;
}

/// Two check stages count their rounds apart: Security's fail sends the card back to Dev and counts one round on
/// Security only; the walk again is Validate's round 1 and Security's round 2. A validator's `otaq prime --validate`
/// reads its own stage's round, and Dev's agent is told the one check's fail.
#[tokio::test(flavor = "multi_thread")]
async fn two_checks_count_their_rounds_apart_and_prime_reads_its_own() {
    let world = world().await;
    let w = &world.w;
    let t = in_dev(w, "Search notes").await;
    let agent = w.agent_shell(&t).await;
    submit(w, &t).await;
    let validator = checker(w, &t).await;
    assert_eq!(primed_round(w, &t).await, 1);
    verdict(w, &t, &validator, true, "Validate: all good.").await;
    let reviewer = checker(w, &t).await;
    assert_eq!(primed_round(w, &t).await, 1);
    let back = verdict(w, &t, &reviewer, false, "Fail: raw SQL in search.").await;
    assert_eq!(at(&back), (Some("dev"), Status::Working));
    assert_eq!(counter(w, &t, "security").await, (1, None));
    assert_eq!(counter(w, &t, "validate").await.0, 0);
    let told = w.deliveries.pending(&agent.session).await.unwrap();
    assert_eq!(
        told.last().map(|p| p.text.as_str()),
        Some("Security failed round 1. Run `otaq prime`.")
    );

    submit(w, &t).await;
    let validator = checker(w, &t).await;
    assert_eq!(agent_of(&validator), "Validator");
    assert_eq!(primed_round(w, &t).await, 1, "Validate's own round");
    verdict(w, &t, &validator, true, "Validate: all good.").await;
    let reviewer = checker(w, &t).await;
    assert_eq!(agent_of(&reviewer), "Security reviewer");
    assert_eq!(primed_round(w, &t).await, 2, "Security's own round");
    let passed = verdict(w, &t, &reviewer, true, "No unescaped SQL.").await;
    assert_eq!(at(&passed), (Some("qa"), Status::WaitingForYou));
    world.w.stop().await;
}

/// A check whose agent's tool is at its limit waits: its checker is held before its launch, the card says "Waiting
/// for limit reset", and the hold is the ticket's (one, with no agent of its own); the tool's Resume starts the
/// check's one checker on that tool.
#[tokio::test(flavor = "multi_thread")]
async fn a_check_at_its_tools_limit_waits_as_the_tickets_hold() {
    let world = world().await;
    let w = &world.w;
    let t = in_dev(w, "Search notes").await;
    submit(w, &t).await;
    let validator = checker(w, &t).await;
    w.set_agent_model(
        "Security reviewer",
        AgentModel::Tool {
            tool: Tool::Codex,
            model: None,
        },
    )
    .await;
    let hit = LimitHit {
        kind: LimitKind::Session,
        text: "You've hit your usage limit.".into(),
    };
    w.plan_limit
        .record_hit(Tool::Codex, &hit, otaq_core::store::now_ms())
        .await
        .unwrap();
    let waits = verdict(w, &t, &validator, true, "Validate: all good.").await;
    assert_eq!(at(&waits), (Some("security"), Status::WaitingForLimitReset));
    assert!(
        w.sessions
            .running_all_of(&t.id, SessionKind::Validator)
            .is_empty()
    );
    let holds: Vec<_> = w
        .board
        .store()
        .ticket_holds(&t.id)
        .await
        .unwrap()
        .into_iter()
        .filter(|h| h.role == SessionRole::Validator)
        .map(|h| (h.reason, h.tool))
        .collect();
    assert_eq!(holds, [(HoldReason::ToolLimit, Tool::Codex)]);

    otaq_core::agents::limits::resume_tool(&w.plan_limit, &w.validation, &w.gate, Tool::Codex)
        .await
        .unwrap();
    let reviewer = checker(w, &t).await;
    assert_eq!(agent_of(&reviewer), "Security reviewer");
    assert_eq!(reviewer.tool, Some(Tool::Codex));
    for _ in 0..500 {
        if ticket(w, &t).await.status == Status::Checking {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let passed = verdict(w, &t, &reviewer, true, "No unescaped SQL.").await;
    assert_eq!(at(&passed), (Some("qa"), Status::WaitingForYou));
    assert!(
        w.board
            .store()
            .ticket_holds(&t.id)
            .await
            .unwrap()
            .is_empty()
    );
    world.w.stop().await;
}

/// A round stored from when a check could have several agents still reads as it was: one round, a record per
/// agent's row, each with its agent.
#[tokio::test(flavor = "multi_thread")]
async fn a_round_stored_with_several_check_agents_reads_as_it_was() {
    let world = world().await;
    let w = &world.w;
    let t = in_dev(w, "Search notes").await;
    w.force_stage(&t, "security", Status::Checking).await;
    w.validating_with_agents(
        &t,
        &[("Security reviewer", "s-old-1"), ("Validator", "s-old-2")],
    )
    .await;
    let records = w.board.validations(&t.key.to_string()).await.unwrap();
    let mut rows: Vec<_> = records
        .iter()
        .map(|r| (r.round, r.agent.clone(), r.session.clone()))
        .collect();
    rows.sort();
    assert_eq!(
        rows,
        [
            (1, some("Security reviewer"), some("s-old-1")),
            (1, some("Validator"), some("s-old-2")),
        ]
    );
    world.w.stop().await;
}
