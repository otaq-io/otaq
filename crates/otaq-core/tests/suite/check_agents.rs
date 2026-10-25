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

// T-0528 (d0130 row 17, d0108's fallback paragraph and rule 9's Retry, 0063): the user's actions on a round of
// several check agents, holds and fallback per held agent, a restart mid-round.

use otaq_core::agent_tools::Tool;
use otaq_core::agents::{AgentHold, AgentModel, Fallbacks, HoldReason, ModelRef, SessionRole};
use otaq_core::domain::ValidationAction;
use otaq_core::usage::limit::{LimitHit, LimitKind};

/// The user's `action` on the ticket's check.
async fn act(w: &TestService, t: &Ticket, action: ValidationAction) -> Result<Ticket, String> {
    w.validation
        .user_action(&t.key.to_string(), action, "", &Actor::User("me".into()))
        .await
        .map_err(|e| e.to_string())
}

/// Until none of the ticket's checkers runs.
async fn no_checkers(w: &TestService, t: &Ticket) {
    for _ in 0..500 {
        if w.sessions
            .running_all_of(&t.id, SessionKind::Validator)
            .is_empty()
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("{}'s checkers still run", t.key);
}

/// How many checker sessions the agent `name` has had for the ticket.
async fn checker_runs(w: &TestService, t: &Ticket, name: &str) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM sessions WHERE ticket_id = ? AND kind = 'validator' AND agent_name = ?",
    )
    .bind(&t.id)
    .bind(name)
    .fetch_one(w.board.store().pool())
    .await
    .unwrap()
}

/// The ticket's checker holds: (check agent, reason, whether it offers Skip this check).
async fn check_holds(w: &TestService, t: &Ticket) -> Vec<(Option<String>, HoldReason, bool)> {
    w.board
        .store()
        .ticket_holds(&t.id)
        .await
        .unwrap()
        .into_iter()
        .filter(|h: &AgentHold| h.role == SessionRole::Validator)
        .map(|h| {
            let skip = otaq_core::validation::skip_at_limit(&h);
            (h.check_agent, h.reason, skip)
        })
        .collect()
}

async fn until_status(w: &TestService, t: &Ticket, want: Status) -> Ticket {
    for _ in 0..500 {
        let now = ticket(w, t).await;
        if now.status == want {
            return now;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!(
        "{} never became {want}: {}",
        t.key,
        ticket(w, t).await.status
    );
}

/// Security reviewer runs on Codex, at its limit: its checker is held before its launch, the Validator's runs.
async fn codex_at_its_limit(w: &TestService) {
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
}

async fn fallbacks(w: &TestService, fallbacks: Fallbacks) {
    w.board.store().set_fallbacks(&fallbacks).await.unwrap();
}

/// Retry after "ended without a verdict" re-runs only the agent without one; the other's pass stands, and the stage
/// passes once the re-run passes (one round, counted once).
#[tokio::test(flavor = "multi_thread")]
async fn retry_reruns_only_the_check_agent_without_a_verdict() {
    let world = world().await;
    let w = &world.w;
    let (t, _) = in_dev(w, "Search notes").await;
    let security = into_security(w, &t).await;
    w.validation
        .validator_signal(&t.id, &security[0].id, &Signal::TurnEnded)
        .await
        .unwrap();
    let ended = verdict(w, &t, &security[1], true, "Search matches.")
        .await
        .unwrap();
    assert_eq!(at(&ended), (Some("security"), Status::CheckNoResult));

    let retried = act(w, &t, ValidationAction::Retry).await.unwrap();
    assert_eq!(at(&retried), (Some("security"), Status::Checking));
    let again = checkers(w, &t, 1).await;
    assert_eq!(
        agents(&again),
        ["Security reviewer"],
        "only the one without a verdict"
    );
    assert_eq!(
        checker_runs(w, &t, "Validator").await,
        2,
        "Validate's and Security's"
    );
    let passed = verdict(w, &t, &again[0], true, "No unescaped SQL.")
        .await
        .unwrap();
    assert_eq!(at(&passed), (Some("qa"), Status::WaitingForYou));
    assert_eq!(counter(w, &t, "security").await, (0, Some(1)));
    let records = w.board.validations(&t.key.to_string()).await.unwrap();
    let security_records: Vec<_> = records
        .iter()
        .filter(|r| r.round == 1 && r.agent.is_some())
        .take(3)
        .map(|r| (r.agent.as_deref(), r.outcome))
        .collect();
    assert_eq!(
        security_records,
        [
            (Some("Security reviewer"), Some(ValidationOutcome::Pass)),
            (Some("Security reviewer"), Some(ValidationOutcome::NoResult)),
            (Some("Validator"), Some(ValidationOutcome::Pass)),
        ],
        "History: the re-run beside the round's two"
    );
    world.w.stop().await;
}

/// Stop this check stops every checker of the round, nothing counted; Skip this check skips the whole stage
/// (Delivery in click mode after).
#[tokio::test(flavor = "multi_thread")]
async fn stop_stops_both_and_skip_skips_the_stage() {
    let world = world().await;
    let w = &world.w;
    let (t, _) = in_dev(w, "Search notes").await;
    let security = into_security(w, &t).await;
    verdict(w, &t, &security[0], true, "No unescaped SQL.")
        .await
        .unwrap();
    let stopped = act(w, &t, ValidationAction::Stop).await.unwrap();
    no_checkers(w, &t).await;
    assert_ne!(stopped.status, Status::Checking);
    assert_eq!(counter(w, &t, "security").await, (0, None), "not counted");

    w.force_stage(&t, "dev", Status::Working).await;
    into_security(w, &t).await;
    let skipped = act(w, &t, ValidationAction::Skip).await.unwrap();
    no_checkers(w, &t).await;
    assert_eq!(at(&skipped), (Some("qa"), Status::WaitingForYou));
    assert!(
        w.board
            .forced_click_reason(&skipped)
            .await
            .unwrap()
            .is_some(),
        "Delivery in click mode after a skip"
    );
    world.w.stop().await;
}

/// d0130 row 17: one check agent held for its tool's limit, the other runs on and passes; the round waits for the
/// held one ("Waiting for limit reset"), Use fallback moves only it, and the stage passes on its verdict. The ticket
/// holds one slot however many checkers it runs.
#[tokio::test(flavor = "multi_thread")]
async fn one_held_the_other_passes_and_use_fallback_moves_only_the_held_one() {
    let world = world().await;
    let w = &world.w;
    let (t, _) = in_dev(w, "Search notes").await;
    submit(w, &t).await;
    let validator = checkers(w, &t, 1).await.remove(0);
    codex_at_its_limit(w).await;
    fallbacks(
        w,
        [(
            Tool::Codex,
            ModelRef {
                tool: Tool::Claude,
                model: None,
            },
        )]
        .into(),
    )
    .await;
    verdict(w, &t, &validator, true, "Validate: all good.")
        .await
        .unwrap();
    let running = checkers(w, &t, 1).await;
    assert_eq!(
        agents(&running),
        ["Validator"],
        "the held one waits, the other runs"
    );
    assert_eq!(
        check_holds(w, &t).await,
        [(
            Some("Security reviewer".into()),
            HoldReason::ToolLimit,
            false
        )]
    );
    assert_eq!(
        at(&ticket(w, &t).await),
        (Some("security"), Status::Checking)
    );
    assert_eq!(w.scheduler.status().await.unwrap().holding, 1, "one slot");

    let waits = verdict(w, &t, &running[0], true, "Search matches.")
        .await
        .unwrap();
    assert_eq!(at(&waits), (Some("security"), Status::WaitingForLimitReset));
    let e = act(w, &t, ValidationAction::Skip).await.unwrap_err();
    assert!(e.contains("use the fallback instead"), "{e}");
    let e = w
        .gate
        .use_fallback(&t.key.to_string(), Some("Validator"))
        .await
        .unwrap_err();
    assert!(
        e.to_string().contains("SHOP-1's Validator isn't waiting"),
        "{e}"
    );

    w.gate
        .use_fallback(&t.key.to_string(), Some("security reviewer"))
        .await
        .unwrap();
    let moved = checkers(w, &t, 1).await;
    assert_eq!(agents(&moved), ["Security reviewer"]);
    assert_eq!(moved[0].tool, Some(Tool::Claude), "on its tool's fallback");
    assert_eq!(
        checker_runs(w, &t, "Validator").await,
        2,
        "the other isn't run again"
    );
    assert_eq!(
        until_status(w, &t, Status::Checking).await.place.stage(),
        Some("security")
    );
    let passed = verdict(w, &t, &moved[0], true, "No unescaped SQL.")
        .await
        .unwrap();
    assert_eq!(at(&passed), (Some("qa"), Status::WaitingForYou));
    assert!(check_holds(w, &t).await.is_empty());
    world.w.stop().await;
}

/// A held check agent with no usable fallback offers Skip this check (0108's fallback paragraph); the round ends
/// skipped, the card moves past the stage.
#[tokio::test(flavor = "multi_thread")]
async fn held_with_no_fallback_offers_skip() {
    let world = world().await;
    let w = &world.w;
    let (t, _) = in_dev(w, "Search notes").await;
    submit(w, &t).await;
    let validator = checkers(w, &t, 1).await.remove(0);
    codex_at_its_limit(w).await;
    verdict(w, &t, &validator, true, "Validate: all good.")
        .await
        .unwrap();
    let running = checkers(w, &t, 1).await;
    assert_eq!(
        check_holds(w, &t).await,
        [(
            Some("Security reviewer".into()),
            HoldReason::ToolLimit,
            true
        )]
    );
    let e = w
        .gate
        .use_fallback(&t.key.to_string(), Some("Security reviewer"))
        .await
        .unwrap_err();
    assert!(e.to_string().contains("has no fallback"), "{e}");
    verdict(w, &t, &running[0], true, "Search matches.")
        .await
        .unwrap();
    let skipped = act(w, &t, ValidationAction::Skip).await.unwrap();
    assert_eq!(at(&skipped), (Some("qa"), Status::WaitingForYou));
    assert!(check_holds(w, &t).await.is_empty());
    let records = w.board.validations(&t.key.to_string()).await.unwrap();
    assert_eq!(
        records[0].outcome,
        Some(ValidationOutcome::Skipped),
        "the round ends skipped"
    );
    world.w.stop().await;
}

/// A checker that hits its tool's limit mid-round while the other runs on waits held for it (not Waiting for limit
/// reset while the other runs, nor an interrupted round); its Use fallback starts a fresh checker for it only.
#[tokio::test(flavor = "multi_thread")]
async fn a_limit_mid_round_holds_only_that_check_agent() {
    let world = world().await;
    let w = &world.w;
    let (t, _) = in_dev(w, "Search notes").await;
    let security = into_security(w, &t).await;
    fallbacks(
        w,
        [(
            Tool::Claude,
            ModelRef {
                tool: Tool::Codex,
                model: None,
            },
        )]
        .into(),
    )
    .await;
    let hit = LimitHit {
        kind: LimitKind::Session,
        text: "You've hit your session limit".into(),
    };
    w.validation
        .validator_signal(
            &t.id,
            &security[0].id,
            &Signal::LimitReached {
                hit: hit.clone(),
                resets_at: None,
            },
        )
        .await
        .unwrap();
    // The service records the tool's limit after the checker's signal.
    w.plan_limit
        .record_hit(Tool::Claude, &hit, otaq_core::store::now_ms())
        .await
        .unwrap();
    let left = checkers(w, &t, 1).await;
    assert_eq!(left[0].id, security[1].id, "the other runs on");
    assert_eq!(
        at(&ticket(w, &t).await),
        (Some("security"), Status::Checking)
    );
    assert_eq!(
        check_holds(w, &t).await,
        [(
            Some("Security reviewer".into()),
            HoldReason::ToolLimit,
            false
        )]
    );
    w.gate
        .use_fallback(&t.key.to_string(), Some("Security reviewer"))
        .await
        .unwrap();
    let both = checkers(w, &t, 2).await;
    let fresh = both.iter().find(|s| s.id != security[1].id).unwrap();
    assert_eq!(fresh.tool, Some(Tool::Codex));
    verdict(w, &t, fresh, true, "No unescaped SQL.")
        .await
        .unwrap();
    let passed = verdict(w, &t, &security[1], true, "Search matches.")
        .await
        .unwrap();
    assert_eq!(at(&passed), (Some("qa"), Status::WaitingForYou));
    world.w.stop().await;
}

/// One checker's Working doesn't clear another's Needs you (the turquoise stays while one still asks).
#[tokio::test(flavor = "multi_thread")]
async fn one_checkers_work_leaves_the_others_question() {
    let world = world().await;
    let w = &world.w;
    let (t, _) = in_dev(w, "Search notes").await;
    let security = into_security(w, &t).await;
    let asks = Signal::NeedsYou {
        prompt: "Allow Bash: cargo test?".into(),
    };
    let works = Signal::Working { prompt: None };
    w.validation
        .validator_signal(&t.id, &security[0].id, &asks)
        .await
        .unwrap();
    assert_eq!(ticket(w, &t).await.status, Status::CheckerNeedsYou);
    w.validation
        .validator_signal(&t.id, &security[1].id, &works)
        .await
        .unwrap();
    assert_eq!(
        ticket(w, &t).await.status,
        Status::CheckerNeedsYou,
        "the other checker's Working leaves the question"
    );
    w.validation
        .validator_signal(&t.id, &security[0].id, &works)
        .await
        .unwrap();
    assert_eq!(ticket(w, &t).await.status, Status::Checking);
    world.w.stop().await;
}

/// A restart mid-round interrupts it; Resume restarts every check agent fresh, the verdict given before the restart
/// not kept (0063).
#[tokio::test(flavor = "multi_thread")]
async fn a_restart_mid_round_restarts_every_check_agent() {
    let World { w, _dir: dir } = world().await;
    let (t, _) = in_dev(&w, "Search notes").await;
    let security = into_security(&w, &t).await;
    verdict(&w, &t, &security[0], true, "No unescaped SQL.")
        .await
        .unwrap();
    let launch = stand_in::launch(&dir.path().join("bin"), dir.path());
    let w = w
        .restart_with(move |config| ServiceConfig {
            launch: Some(launch),
            ..config
        })
        .await;
    let interrupted = ticket(&w, &t).await;
    assert_eq!(
        (interrupted.status, interrupted.interrupted_from),
        (Status::Interrupted, Some(Status::Checking))
    );
    w.review
        .resume(&t.key.to_string(), &Actor::User("me".into()))
        .await
        .unwrap();
    let fresh = checkers(&w, &t, 2).await;
    assert_eq!(agents(&fresh), ["Security reviewer", "Validator"]);
    verdict(&w, &t, &fresh[1], true, "Search matches.")
        .await
        .unwrap();
    assert_eq!(
        at(&ticket(&w, &t).await),
        (Some("security"), Status::Checking),
        "the pass before the restart isn't kept"
    );
    let passed = verdict(&w, &t, &fresh[0], true, "No unescaped SQL.")
        .await
        .unwrap();
    assert_eq!(at(&passed), (Some("qa"), Status::WaitingForYou));
    w.stop().await;
    drop(dir);
}
