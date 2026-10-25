//! Several check agents in one Check stage (T-0527; d0130 and its row 17, d0108 second gap review rule 9, d0109,
//! d0063): on the test flow (`tests/flows/every-kind.yaml`) Security lists two check agents, Security reviewer and
//! Validator. Entering it starts both together, each a fresh read-only session in the ticket's worktree with the
//! stage's skill copied once; each verdict is its own agent's comment; the round ends when every agent has a verdict
//! (all passed: Pass; any failed: Fail with every report, counted once); the next round runs both again; a change
//! cancels the whole round. A stage with one agent (Validate) works as before. Every tool is the stand-in of
//! `tests/fixtures/stand-in`; repos, `OTAQ_HOME` and the tools' home are temporary.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::common::stand_in;
use otaq_core::board::hand_over::HandOver;
use otaq_core::board::tickets::Ticket;
use otaq_core::domain::{Actor, Status, ValidationOutcome, ValidationTrigger};
use otaq_core::flow::engine::RunResult;
use otaq_core::flow::{FlowFile, yaml};
use otaq_core::git::testing::git;
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::{TestService, said};
use otaq_core::session::{SessionInfo, SessionKind};
use otaq_core::signals::Signal;
use tempfile::TempDir;

struct World {
    w: TestService,
    _dir: TempDir,
}

/// A service whose tools are the stand-in, with shop-web (SHOP) on the test flow; Security runs the shipped
/// `validate` skill, so both of its checkers show they loaded it.
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
async fn in_dev(w: &TestService, title: &str) -> (Ticket, PathBuf) {
    let t = w.create("SHOP", title).await;
    let worktree = w.give_worktree(&t).await;
    w.force_stage(&t, "dev", Status::Working).await;
    (w.board.ticket(&t.key.to_string()).await.unwrap(), worktree)
}

async fn ticket(w: &TestService, t: &Ticket) -> Ticket {
    w.board.ticket(&t.key.to_string()).await.unwrap()
}

fn at(t: &Ticket) -> (Option<&str>, Status) {
    (t.place.stage(), t.status)
}

/// The agent's `stage next` from Dev: Validate runs.
async fn submit(w: &TestService, t: &Ticket) -> Ticket {
    w.validation
        .submit(
            &t.key.to_string(),
            said(&w.board, &t.key.to_string(), "Search works.")
                .await
                .as_ref(),
            &Actor::Agent(t.key.clone()),
            ValidationTrigger::Agent,
        )
        .await
        .unwrap()
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

/// `session`'s verdict as `otaq check pass|fail` gives it: its text is its agent's comment.
async fn verdict(
    w: &TestService,
    t: &Ticket,
    session: &SessionInfo,
    pass: bool,
    text: &str,
) -> Result<Ticket, String> {
    let key = t.key.to_string();
    let author = w.board.session_author(&session.id).await.unwrap();
    let hand_over: Option<HandOver> = w
        .board
        .stage_end_hand_over(&ticket(w, t).await, author, text, None, &[])
        .await
        .unwrap();
    w.validation
        .verdict(&key, pass, hand_over.as_ref(), &session.id)
        .await
        .map_err(|e| e.to_string())
}

/// Validate (one check agent) passes: the card is in Security, both of its checkers running.
async fn into_security(w: &TestService, t: &Ticket) -> Vec<SessionInfo> {
    submit(w, t).await;
    let validator = checkers(w, t, 1).await.remove(0);
    let security = verdict(w, t, &validator, true, "Validate: all good.")
        .await
        .unwrap();
    assert_eq!(at(&security), (Some("security"), Status::Checking));
    checkers(w, t, 2).await
}

/// The agents' names of `sessions`.
fn agents(sessions: &[SessionInfo]) -> Vec<&str> {
    sessions
        .iter()
        .map(|s| s.agent.as_ref().map_or("", |a| a.name.as_str()))
        .collect()
}

/// (stage, agent, result) of each run of `stage`, oldest first.
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

/// The ticket's comments by their authors' names, oldest first.
async fn comments(w: &TestService, t: &Ticket) -> Vec<(Option<String>, String)> {
    use otaq_core::board::ticket_comments::CommentAuthor;
    w.board
        .ticket_comments(&t.key.to_string())
        .await
        .unwrap()
        .comments
        .into_iter()
        .map(|c| {
            let name = match c.author {
                CommentAuthor::Agent { name, .. } => name,
                _ => None,
            };
            (name, c.body)
        })
        .collect()
}

fn some(name: &str) -> Option<String> {
    Some(name.to_owned())
}

/// Both check agents start together on their own agents, in the worktree, with the stage's skill they both loaded
/// (the copies shared, d0118); two passes are one Pass: the card moves on once, the round counted once, one run per
/// agent, one History record per agent with its report.
#[tokio::test(flavor = "multi_thread")]
async fn two_check_agents_start_together_and_two_passes_are_one_pass() {
    let world = world().await;
    let w = &world.w;
    let (t, _) = in_dev(w, "Search notes").await;
    let security = into_security(w, &t).await;
    assert_eq!(agents(&security), ["Security reviewer", "Validator"]);
    for checker in &security {
        w.wait_for_output(&checker.id, "RUNS otaq-validate").await;
    }

    let first = verdict(w, &t, &security[0], true, "No unescaped SQL.")
        .await
        .unwrap();
    assert_eq!(
        at(&first),
        (Some("security"), Status::Checking),
        "nothing moves before the last verdict"
    );
    assert_eq!(counter(w, &t, "security").await, (0, None));
    let passed = verdict(w, &t, &security[1], true, "Search matches.")
        .await
        .unwrap();
    assert_eq!(at(&passed), (Some("qa"), Status::WaitingForYou));
    assert_eq!(counter(w, &t, "security").await, (0, Some(1)));
    assert_eq!(
        runs_of(w, &t, "security").await,
        [
            (some("Security reviewer"), Some(RunResult::Passed)),
            (some("Validator"), Some(RunResult::Passed))
        ]
    );
    let records = w.board.validations(&t.key.to_string()).await.unwrap();
    // Newest round first: Security's two records, then Validate's one.
    let security_records: Vec<_> = records[..2]
        .iter()
        .map(|r| (r.round, r.agent.as_deref(), r.outcome, r.report.as_deref()))
        .collect();
    assert_eq!(records.len(), 3);
    assert_eq!(
        security_records,
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
                Some("Search matches.")
            ),
        ],
        "History: one report per agent per round"
    );
    let ended = w
        .board
        .history(&t.key.to_string())
        .await
        .unwrap()
        .into_iter()
        .filter(|e| e.kind == otaq_core::board::tickets::EventKind::ValidationEnded)
        .count();
    assert_eq!(ended, 2, "Validate's round and Security's: one each");
    world.w.stop().await;
}

/// One fails: nothing moves until the other's verdict, then Fail with both comments in the stage's order; Dev's
/// agent is told each verdict; the fail counts one round; the next round starts both again, the one that passed too.
#[tokio::test(flavor = "multi_thread")]
async fn one_fail_waits_for_the_other_then_sends_every_report() {
    let world = world().await;
    let w = &world.w;
    let (t, _) = in_dev(w, "Search notes").await;
    let agent = w.agent_shell(&t).await;
    let security = into_security(w, &t).await;

    let first = verdict(
        w,
        &t,
        &security[0],
        false,
        "- src/search.rs:3 unescaped SQL",
    )
    .await
    .unwrap();
    assert_eq!(at(&first), (Some("security"), Status::Checking));
    assert!(
        w.sessions.info(&security[1].id).unwrap().is_running(),
        "the other check agent runs on"
    );
    let back = verdict(w, &t, &security[1], true, "Search matches.")
        .await
        .unwrap();
    assert_eq!(at(&back), (Some("dev"), Status::Working));
    assert_eq!(counter(w, &t, "security").await, (1, None), "one fail");
    let all = comments(w, &t).await;
    assert_eq!(
        all[all.len() - 2..],
        [
            (
                some("Security reviewer"),
                "- src/search.rs:3 unescaped SQL".to_owned()
            ),
            (some("Validator"), "Search matches.".to_owned())
        ]
    );
    let told = w.deliveries.pending(&agent.session).await.unwrap();
    assert_eq!(
        told.last().map(|p| p.text.as_str()),
        Some(
            "Security failed round 1: Security reviewer failed, Validator passed. Run `otaq prime`."
        )
    );

    // The next round runs both again.
    let again = into_security(w, &t).await;
    assert_eq!(agents(&again), ["Security reviewer", "Validator"]);
    assert!(
        again
            .iter()
            .all(|s| security.iter().all(|old| old.id != s.id))
    );
    assert_eq!(
        ticket(w, &t)
            .await
            .flags
            .validating_round
            .map(|r| (r.round, r.of)),
        Some((2, 2))
    );
    world.w.stop().await;
}

/// A commit mid-round cancels both sessions and counts nothing (0108 rule 9); a verdict arriving after it is
/// refused; a session that isn't one of the round's can't give one.
#[tokio::test(flavor = "multi_thread")]
async fn a_commit_mid_round_cancels_both_and_late_verdicts_are_refused() {
    let world = world().await;
    let w = &world.w;
    let (t, worktree) = in_dev(w, "Search notes").await;
    let agent = w.agent_shell(&t).await;
    let security = into_security(w, &t).await;
    verdict(w, &t, &security[0], true, "No unescaped SQL.")
        .await
        .unwrap();

    let e = verdict(
        w,
        &t,
        &agent_info(w, &agent.session),
        true,
        "I pass myself.",
    )
    .await
    .unwrap_err();
    assert!(e.contains("This isn't SHOP-1's running check"), "{e}");

    w.validation.agent_working(&t.id).await;
    std::fs::write(worktree.join("file1.txt"), "the agent's fix\n").unwrap();
    git(&worktree, &["commit", "-qam", "agent fix"]);
    w.validation.agent_turn_ended(&t.id).await.unwrap();
    let back = ticket(w, &t).await;
    assert_eq!(at(&back), (Some("dev"), Status::Working));
    for _ in 0..200 {
        if w.sessions
            .running_all_of(&t.id, SessionKind::Validator)
            .is_empty()
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(
        w.sessions
            .running_all_of(&t.id, SessionKind::Validator)
            .is_empty(),
        "every checker of the round stopped"
    );
    assert_eq!(
        counter(w, &t, "security").await,
        (0, None),
        "nothing counted"
    );
    assert_eq!(
        runs_of(w, &t, "security").await,
        [
            (some("Security reviewer"), Some(RunResult::Passed)),
            (some("Validator"), Some(RunResult::Cancelled))
        ]
    );
    let late = verdict(w, &t, &security[1], false, "Too late.")
        .await
        .unwrap_err();
    assert!(late.contains("This isn't SHOP-1's running check"), "{late}");
    assert_eq!(at(&ticket(w, &t).await), (Some("dev"), Status::Working));
    world.w.stop().await;
}

/// A check agent whose turn ends without a verdict leaves its row without one; the round waits for the other, then
/// ends "ended without a verdict" naming it (not a round). With a fail among the verdicts the round fails instead.
#[tokio::test(flavor = "multi_thread")]
async fn a_check_agent_without_a_verdict() {
    let world = world().await;
    let w = &world.w;
    let (t, _) = in_dev(w, "Search notes").await;
    let security = into_security(w, &t).await;
    w.validation
        .validator_signal(&t.id, &security[0].id, &Signal::TurnEnded)
        .await
        .unwrap();
    assert_eq!(
        at(&ticket(w, &t).await),
        (Some("security"), Status::Checking)
    );
    let ended = verdict(w, &t, &security[1], true, "Search matches.")
        .await
        .unwrap();
    assert_eq!(at(&ended), (Some("security"), Status::CheckNoResult));
    let signal: Option<String> = sqlx::query_scalar("SELECT signal_text FROM tickets WHERE id = ?")
        .bind(&t.id)
        .fetch_one(w.board.store().pool())
        .await
        .unwrap();
    assert_eq!(
        signal.as_deref(),
        Some("Security reviewer ended without a verdict")
    );
    assert_eq!(counter(w, &t, "security").await, (0, None), "not a round");
    assert_eq!(
        runs_of(w, &t, "security").await,
        [
            (some("Security reviewer"), Some(RunResult::NoResult)),
            (some("Validator"), Some(RunResult::Passed))
        ]
    );

    // The next round: a fail and no verdict fail the round, every report together.
    w.force_stage(&t, "dev", Status::Working).await;
    let again = into_security(w, &t).await;
    verdict(w, &t, &again[1], false, "- search ignores titles")
        .await
        .unwrap();
    w.validation
        .validator_signal(&t.id, &again[0].id, &Signal::TurnEnded)
        .await
        .unwrap();
    let back = ticket(w, &t).await;
    assert_eq!(at(&back), (Some("dev"), Status::Working));
    assert_eq!(counter(w, &t, "security").await, (1, None));
    world.w.stop().await;
}

/// One check agent (Validate) reads as before: its fail is today's text and the round's one record.
#[tokio::test(flavor = "multi_thread")]
async fn one_check_agent_works_as_before() {
    let world = world().await;
    let w = &world.w;
    let (t, _) = in_dev(w, "Search notes").await;
    let agent = w.agent_shell(&t).await;
    submit(w, &t).await;
    let validator = checkers(w, &t, 1).await.remove(0);
    let back = verdict(w, &t, &validator, false, "- cart total wrong")
        .await
        .unwrap();
    assert_eq!(at(&back), (Some("dev"), Status::Working));
    let told = w.deliveries.pending(&agent.session).await.unwrap();
    assert_eq!(
        told.last().map(|p| p.text.as_str()),
        Some("Validate failed round 1. Run `otaq prime`.")
    );
    let records = w.board.validations(&t.key.to_string()).await.unwrap();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].agent.as_deref(), Some("Validator"));
    world.w.stop().await;
}

/// The session behind `id`, for a verdict from a session that isn't a checker.
fn agent_info(w: &TestService, id: &str) -> SessionInfo {
    w.sessions.info(id).unwrap()
}
