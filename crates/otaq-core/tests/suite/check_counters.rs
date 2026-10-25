//! Agent checks in any flow (T-0417; FLW-13, Q259, Q277; d0108, d0063, d0083): on the test flow
//! (`tests/flows/every-kind.yaml`: Grill · Dev · Validate · Security · QA · Done), each agent check counts its own fails
//! in a row and stops at its own rounds; a pass into the next check starts that check's own checker; a human Fail
//! resets every counter; Move on anyway and Skip this check take the card to the engine's next stage, and its
//! Delivery is then in click mode with the reason. Skip this check is offered on a check held for the plan limit only
//! when no fallback can take it, never on a held Agent stage. `claude` stands in as `sh` printing its arguments.

use otaq_core::service::testing::said;
use std::path::Path;
use std::time::Duration;

use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::agent_tools::{LaunchContext, Tool};
use otaq_core::agents::{AgentHold, HoldReason, SessionRole};
use otaq_core::board::tickets::Ticket;
use otaq_core::domain::{Actor, MovedPast, Place, Status, ValidationAction, ValidationTrigger};
use otaq_core::flow::engine::RunResult;
use otaq_core::flow::{FlowFile, yaml};
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::TestService;
use otaq_core::session::{SessionInfo, SessionKind};
use otaq_core::store::{new_id, now_ms};

/// Keeps running, printing its arguments.
const KEEPS_RUNNING: &str = "printf 'ARGS %s\\n' \"$*\"; exec sleep 30";

fn me() -> Actor {
    Actor::User("me".into())
}

fn every_kind_file() -> FlowFile {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/flows/every-kind.yaml");
    yaml::read(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// A service whose `claude` keeps running, with shop-web (SHOP) on the test flow, edited by `edit`.
async fn world(edit: impl FnOnce(&mut FlowFile)) -> TestService {
    let w = TestService::start_with(|config| ServiceConfig {
        launch: Some(LaunchContext {
            bin_dir: "/nonexistent".into(),
            plugin_dir: "/nonexistent/plugin".into(),
            path: std::env::var_os("PATH").unwrap_or_default(),
            claude: ClaudeLaunch {
                program: vec!["sh".into(), "-c".into(), KEEPS_RUNNING.into(), "sh".into()],
                config_dir: None,
            },
        }),
        ..config
    })
    .await;
    w.add_project("shop-web").await;
    let mut file = every_kind_file();
    edit(&mut file);
    w.board.use_flow_for_tests("SHOP", &file).await;
    w
}

/// A ticket at work in Dev with a committed worktree.
async fn in_dev(w: &TestService, title: &str) -> Ticket {
    let t = w.create("SHOP", title).await;
    w.give_worktree(&t).await;
    w.force_stage(&t, "dev", Status::Working).await;
    w.board.ticket(&t.key.to_string()).await.unwrap()
}

async fn ticket(w: &TestService, t: &Ticket) -> Ticket {
    w.board.ticket(&t.key.to_string()).await.unwrap()
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

/// The running checker of the ticket's check.
async fn checker(w: &TestService, t: &Ticket) -> SessionInfo {
    for _ in 0..500 {
        if let Some(session) = w.sessions.running_of(&t.id, SessionKind::Validator) {
            return session;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("{} has no checker", t.key);
}

/// The running checkers of the ticket's check, one per check agent (Security lists two, d0130), in the stage's order.
async fn checkers(w: &TestService, t: &Ticket) -> Vec<SessionInfo> {
    let stage = ticket(w, t).await.place.stage().unwrap().to_owned();
    let flow = w.board.effective_flow("SHOP").await.unwrap();
    let agents = flow.stage(&stage).unwrap().check_agents().len();
    for _ in 0..500 {
        let running = w.sessions.running_all_of(&t.id, SessionKind::Validator);
        if running.len() == agents {
            return running;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("{} has no {agents} checkers", t.key);
}

/// The round's verdict: every checker passes, or the first fails and the others pass.
async fn verdict(w: &TestService, t: &Ticket, pass: bool) -> Ticket {
    let mut after = None;
    for (i, session) in checkers(w, t).await.into_iter().enumerate() {
        after = Some(
            w.validation
                .verdict(
                    &t.key.to_string(),
                    pass || i > 0,
                    said(
                        &w.board,
                        &t.key.to_string(),
                        "- src/search.rs:3 unescaped SQL",
                    )
                    .await
                    .as_ref(),
                    &session.id,
                )
                .await
                .unwrap(),
        );
    }
    after.unwrap()
}

/// (fails in a row, passed round) of each check that counted, by stage id.
async fn counters(w: &TestService, t: &Ticket) -> Vec<(String, u32, Option<u32>)> {
    w.board
        .check_counts(&t.id)
        .await
        .unwrap()
        .0
        .into_iter()
        .filter(|c| c.fails > 0 || c.passed_round.is_some())
        .map(|c| (c.stage, c.fails, c.passed_round))
        .collect()
}

fn at(t: &Ticket) -> (Option<&str>, Status) {
    (t.place.stage(), t.status)
}

async fn action(w: &TestService, t: &Ticket, action: ValidationAction) -> Result<Ticket, String> {
    w.validation
        .user_action(&t.key.to_string(), action, "", &me())
        .await
        .map_err(|e| e.to_string())
}

/// The results of the ticket's runs of `stage`, oldest first.
async fn runs_of(w: &TestService, t: &Ticket, stage: &str) -> Vec<Option<RunResult>> {
    w.board
        .stage_runs(&t.key.to_string())
        .await
        .unwrap()
        .into_iter()
        .filter(|r| r.stage == stage)
        .map(|r| r.result)
        .collect()
}

/// Validate and Security count separately: a Validate fail is Validate's; a pass into Security starts Security's own
/// checker at its round 1; a Security fail goes back to Dev counted on Security ("Security · round 1 of 2"), and
/// Validate's earlier fail no longer counts after its pass. Security's 2 rounds stop only Security.
#[tokio::test(flavor = "multi_thread")]
async fn two_checks_in_a_row_count_separately() {
    let w = world(|_| {}).await;
    let t = in_dev(&w, "Search notes").await;

    assert_eq!(
        at(&submit(&w, &t).await),
        (Some("validate"), Status::Checking)
    );
    let back = verdict(&w, &t, false).await;
    assert_eq!(at(&back), (Some("dev"), Status::Working));
    assert_eq!(counters(&w, &t).await, [("validate".into(), 1, None)]);
    assert_eq!(back.flags.check.as_deref(), Some("Validate"));
    assert_eq!(
        back.flags.validation_round.unwrap().to_string(),
        "Validation failed · round 1 of 3"
    );

    let checking = submit(&w, &t).await;
    assert_eq!(
        checking.flags.validating_round.map(|r| (r.round, r.of)),
        Some((2, 3))
    );
    let validate_checker = checker(&w, &t).await;
    let security = verdict(&w, &t, true).await;
    assert_eq!(at(&security), (Some("security"), Status::Checking));
    let security_checkers = checkers(&w, &t).await;
    assert!(
        security_checkers
            .iter()
            .all(|s| s.id != validate_checker.id),
        "Security's own checkers"
    );
    assert_eq!(
        security_checkers
            .iter()
            .map(|s| s.agent.as_ref().map(|a| a.name.as_str()))
            .collect::<Vec<_>>(),
        [Some("Security reviewer"), Some("Validator")],
        "one per check agent (d0130)"
    );
    assert_eq!(
        security.flags.validating_round.map(|r| (r.round, r.of)),
        Some((1, 2)),
        "Security's round 1 of its 2"
    );
    assert_eq!(counters(&w, &t).await, [("validate".into(), 0, Some(2))]);

    let back = verdict(&w, &t, false).await;
    assert_eq!(at(&back), (Some("dev"), Status::Working));
    assert_eq!(
        counters(&w, &t).await,
        [
            ("security".into(), 1, None),
            ("validate".into(), 0, Some(2))
        ]
    );
    assert_eq!(back.flags.check.as_deref(), Some("Security"));
    assert_eq!(
        back.flags.validation_round.map(|r| (r.round, r.of)),
        Some((1, 2))
    );

    // Every later stage again: Validate (round 1: its pass ended its fails), then Security's second fail stops it.
    let validate = submit(&w, &t).await;
    assert_eq!(
        validate.flags.validating_round.map(|r| (r.round, r.of)),
        Some((1, 3))
    );
    verdict(&w, &t, true).await;
    let stopped = verdict(&w, &t, false).await;
    assert_eq!(at(&stopped), (Some("security"), Status::CheckFailed));
    assert_eq!(stopped.validation_round, 2, "Failed Security 2 times");
    assert_eq!(
        counters(&w, &t).await,
        [
            ("security".into(), 2, None),
            ("validate".into(), 0, Some(1))
        ],
        "Security's limit stops only Security"
    );
    // One run per check agent per round: Security reviewer failed, Validator passed (d0130).
    assert_eq!(
        runs_of(&w, &t, "security").await,
        [
            Some(RunResult::Failed),
            Some(RunResult::Passed),
            Some(RunResult::Failed),
            Some(RunResult::Passed)
        ]
    );
    w.stop().await;
}

/// A human Fail (Fail on QA) resets both checks' counters, and the marks of checks the card was moved past.
#[tokio::test(flavor = "multi_thread")]
async fn a_human_fail_resets_both_checks_counters() {
    let w = world(|_| {}).await;
    let t = in_dev(&w, "Search notes").await;
    submit(&w, &t).await;
    verdict(&w, &t, false).await;
    submit(&w, &t).await;
    verdict(&w, &t, true).await;
    verdict(&w, &t, false).await;
    assert_eq!(
        counters(&w, &t).await,
        [
            ("security".into(), 1, None),
            ("validate".into(), 0, Some(2))
        ]
    );
    // Validate fails once more, then the user moves the card on to QA past Security.
    submit(&w, &t).await;
    verdict(&w, &t, false).await;
    w.force_stage(&t, "security", Status::CheckFailed).await;
    let qa = action(&w, &t, ValidationAction::MoveOnAnyway)
        .await
        .unwrap();
    assert_eq!(at(&qa), (Some("qa"), Status::WaitingForYou));
    assert!(qa.flags.moved_past_check.is_some());

    w.review
        .fail(&t.key.to_string(), "Escape the query", None, false, &me())
        .await
        .unwrap();
    assert!(w.board.check_counts(&t.id).await.unwrap().0.is_empty());
    let back = ticket(&w, &t).await;
    assert_eq!(back.flags.moved_past_check, None);
    assert_eq!(back.validation_round, 0);
    assert_eq!(
        w.board.forced_click_reason(&back).await.unwrap(),
        None,
        "the card walks every check again"
    );
    w.stop().await;
}

/// Security's third fail (its rounds set to 3): Move on anyway goes to the engine's next stage (QA, a check by you,
/// not "the next human check" by luck: without QA it is the Delivery) and says why the Delivery waits for a click.
#[tokio::test(flavor = "multi_thread")]
async fn move_on_anyway_from_failed_three_times_at_security() {
    let three = |file: &mut FlowFile| {
        let security = file.flow.stages.iter_mut().find(|s| s.id == "security");
        security.unwrap().rounds = Some(3);
    };
    let w = world(three).await;
    let t = in_dev(&w, "Search notes").await;
    submit(&w, &t).await;
    verdict(&w, &t, true).await;
    for _ in 0..2 {
        verdict(&w, &t, false).await;
        submit(&w, &t).await;
        verdict(&w, &t, true).await;
    }
    let failed = verdict(&w, &t, false).await;
    assert_eq!(at(&failed), (Some("security"), Status::CheckFailed));
    assert_eq!(failed.validation_round, 3);

    let moved = action(&w, &t, ValidationAction::MoveOnAnyway)
        .await
        .unwrap();
    assert_eq!(at(&moved), (Some("qa"), Status::WaitingForYou));
    let reason = "Failed Security 3 times · moved by you";
    assert_eq!(moved.flags.moved_past_check.as_deref(), Some(reason));
    assert_eq!(
        w.board
            .forced_click_reason(&moved)
            .await
            .unwrap()
            .as_deref(),
        Some(reason)
    );
    w.stop().await;

    // Without QA the next stage is the Delivery: the card waits there for a click (its mode is automatic).
    let w = world(|file| {
        three(file);
        file.flow.stages.retain(|s| s.id != "qa");
    })
    .await;
    let t = in_dev(&w, "Search notes").await;
    w.force_stage(&t, "security", Status::CheckFailed).await;
    sqlx::query(
        "INSERT INTO check_counters (ticket_id, stage_id, fails, updated_at) VALUES (?, 'security', 3, 1)",
    )
    .bind(&t.id)
    .execute(w.board.store().pool())
    .await
    .unwrap();
    let moved = action(&w, &t, ValidationAction::MoveOnAnyway)
        .await
        .unwrap();
    assert_eq!(at(&moved), (Some("ship"), Status::ToDeliver));
    assert_eq!(
        w.board
            .forced_click_reason(&moved)
            .await
            .unwrap()
            .as_deref(),
        Some(reason)
    );
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(
        w.status(&t.key.to_string()).await,
        Status::ToDeliver,
        "nothing delivers by itself"
    );
    w.stop().await;
}

/// Skip this check on a running Security: its checker stops, its run is skipped, the card goes to QA, and its
/// Delivery will wait for a click ("Skipped Security · by you").
#[tokio::test(flavor = "multi_thread")]
async fn a_running_check_is_skipped() {
    let w = world(|_| {}).await;
    let t = in_dev(&w, "Search notes").await;
    submit(&w, &t).await;
    verdict(&w, &t, true).await;
    checker(&w, &t).await;

    let skipped = action(&w, &t, ValidationAction::Skip).await.unwrap();
    assert_eq!(at(&skipped), (Some("qa"), Status::WaitingForYou));
    assert!(
        w.sessions
            .running_of(&t.id, SessionKind::Validator)
            .is_none(),
        "its checker stopped"
    );
    assert_eq!(
        runs_of(&w, &t, "security").await,
        [Some(RunResult::Skipped), Some(RunResult::Skipped)],
        "both check agents' runs"
    );
    assert_eq!(
        skipped.flags.moved_past_check.as_deref(),
        Some("Skipped Security · by you")
    );
    w.stop().await;
}

/// Puts the ticket in `stage`, held for its tool's plan limit with an open round and no checker (as
/// `Validation::hold_round` leaves it), with the hold of `role` and the given fallback.
async fn held_at_limit(
    w: &TestService,
    t: &Ticket,
    stage: &str,
    role: SessionRole,
    fallback: Option<Tool>,
) {
    w.force_stage(t, stage, Status::WaitingForLimitReset).await;
    let checking = role == SessionRole::Validator;
    sqlx::query("UPDATE tickets SET interrupted_from = ? WHERE id = ?")
        .bind(checking.then_some(Status::Checking.as_str()))
        .bind(&t.id)
        .execute(w.board.store().pool())
        .await
        .unwrap();
    if checking {
        sqlx::query(
            "INSERT INTO validations (id, ticket_id, round, trigger, started_at, head_sha)
             VALUES (?, ?, 1, 'agent', ?, NULL)",
        )
        .bind(new_id())
        .bind(&t.id)
        .bind(now_ms())
        .execute(w.board.store().pool())
        .await
        .unwrap();
    }
    w.board
        .store()
        .set_agent_hold(&AgentHold {
            id: new_id(),
            ticket_id: Some(t.id.clone()),
            project_id: None,
            check_agent: None,
            role,
            reason: HoldReason::ToolLimit,
            tool: Tool::Claude,
            model: None,
            agent_id: None,
            fallback_tool: fallback,
            fallback_held: false,
            fallback_asked: false,
            since: now_ms(),
        })
        .await
        .unwrap();
}

async fn hold_of(w: &TestService, t: &Ticket) -> Option<AgentHold> {
    w.board
        .store()
        .ticket_holds(&t.id)
        .await
        .unwrap()
        .into_iter()
        .next()
}

/// Q277: a check held for the plan limit with no fallback set offers Skip this check (the card's hold says so); the
/// skip ends its round as skipped, lifts the hold and goes on to QA.
#[tokio::test(flavor = "multi_thread")]
async fn a_held_check_with_no_fallback_is_skipped() {
    let w = world(|_| {}).await;
    let t = in_dev(&w, "Search notes").await;
    held_at_limit(&w, &t, "security", SessionRole::Validator, None).await;
    let hold = hold_of(&w, &t).await.unwrap();
    assert!(otaq_core::validation::skip_at_limit(&hold));
    let card = otaq_core::agents::facts::CardHold::of(&hold, &[], &Default::default());
    assert!(card.can_skip_check && !card.can_use_fallback);

    let skipped = action(&w, &t, ValidationAction::Skip).await.unwrap();
    assert_eq!(at(&skipped), (Some("qa"), Status::WaitingForYou));
    assert_eq!(skipped.interrupted_from, None);
    assert!(hold_of(&w, &t).await.is_none(), "the hold is lifted");
    assert_eq!(
        runs_of(&w, &t, "security").await.last(),
        Some(&Some(RunResult::Skipped))
    );
    assert_eq!(
        w.board
            .forced_click_reason(&skipped)
            .await
            .unwrap()
            .as_deref(),
        Some("Skipped Security · by you")
    );
    let counts = w.board.check_counts(&t.id).await.unwrap();
    assert_eq!(counts.of("security").moved_past, Some(MovedPast::Skipped));
    w.stop().await;
}

/// With a fallback that can take it, a held check offers Use fallback, not Skip this check; an Agent stage held for
/// the limit never offers the skip.
#[tokio::test(flavor = "multi_thread")]
async fn no_skip_with_a_fallback_nor_on_a_held_agent_stage() {
    let w = world(|_| {}).await;
    let t = in_dev(&w, "Search notes").await;
    held_at_limit(
        &w,
        &t,
        "security",
        SessionRole::Validator,
        Some(Tool::Codex),
    )
    .await;
    let hold = hold_of(&w, &t).await.unwrap();
    assert!(!otaq_core::validation::skip_at_limit(&hold));
    let e = action(&w, &t, ValidationAction::Skip).await.unwrap_err();
    assert!(e.contains("fallback"), "{e}");
    let still = ticket(&w, &t).await;
    assert_eq!(at(&still), (Some("security"), Status::WaitingForLimitReset));

    let dev = in_dev(&w, "Export notes").await;
    held_at_limit(&w, &dev, "dev", SessionRole::Developer, None).await;
    let hold = hold_of(&w, &dev).await.unwrap();
    assert!(!otaq_core::validation::skip_at_limit(&hold));
    assert!(action(&w, &dev, ValidationAction::Skip).await.is_err());
    assert_eq!(
        at(&ticket(&w, &dev).await),
        (Some("dev"), Status::WaitingForLimitReset)
    );
    assert_eq!(ticket(&w, &dev).await.place, Place::stage_of("dev"));
    w.stop().await;
}

/// 0063's rules hold on any check, whatever its skill (d0108 second gap review rule 9): a description edit cancels
/// Security (back to Dev, not a round), and Stop this check stops it without counting either.
#[tokio::test(flavor = "multi_thread")]
async fn a_cancel_and_stop_this_check_on_security_count_nothing() {
    let w = world(|_| {}).await;
    let t = in_dev(&w, "Search notes").await;
    submit(&w, &t).await;
    let before = verdict(&w, &t, true).await;
    assert_eq!(at(&before), (Some("security"), Status::Checking));
    let mut after = before.clone();
    after.body = "Search titles too.".into();
    w.validation.ticket_edited(&before, &after).await.unwrap();
    let back = ticket(&w, &t).await;
    assert_eq!(at(&back), (Some("dev"), Status::Working));
    assert_eq!(counters(&w, &t).await, [("validate".into(), 0, Some(1))]);
    assert_eq!(
        runs_of(&w, &t, "security").await,
        [Some(RunResult::Cancelled), Some(RunResult::Cancelled)],
        "both check agents' runs"
    );

    submit(&w, &t).await;
    verdict(&w, &t, true).await;
    checker(&w, &t).await;
    let stopped = action(&w, &t, ValidationAction::Stop).await.unwrap();
    assert_eq!(at(&stopped), (Some("dev"), Status::Working));
    assert_eq!(
        counters(&w, &t).await,
        [("validate".into(), 0, Some(1))],
        "a stop isn't a round"
    );
    w.stop().await;
}

/// A Plan edit cancels a running check as a description edit does (d0133: "the description or the Plan" in 0063); a
/// title-only edit doesn't.
#[tokio::test(flavor = "multi_thread")]
async fn a_plan_edit_cancels_a_check() {
    let w = world(|_| {}).await;
    let t = in_dev(&w, "Search notes").await;
    submit(&w, &t).await;
    let before = verdict(&w, &t, true).await;
    assert_eq!(at(&before), (Some("security"), Status::Checking));
    let mut retitled = before.clone();
    retitled.title = "Search all notes".into();
    w.validation
        .ticket_edited(&before, &retitled)
        .await
        .unwrap();
    assert_eq!(
        at(&ticket(&w, &t).await),
        (Some("security"), Status::Checking)
    );
    let mut after = before.clone();
    after.plan = "## Pieces\n1. Search titles".into();
    w.validation.ticket_edited(&before, &after).await.unwrap();
    assert_eq!(at(&ticket(&w, &t).await), (Some("dev"), Status::Working));
    assert_eq!(
        runs_of(&w, &t, "security").await,
        [Some(RunResult::Cancelled), Some(RunResult::Cancelled)]
    );
    w.stop().await;
}

/// Q344: what Dev's session is told names the check, not "Validation": Security's fail ("Security failed round 1. Run
/// `otaq prime`.", the findings being Security's comment, CMT-05) and Security's cancel when the description changes.
#[tokio::test(flavor = "multi_thread")]
async fn the_messages_to_dev_name_the_check() {
    let w = world(|_| {}).await;
    let t = in_dev(&w, "Search notes").await;
    let agent = w.agent_shell(&t).await;
    let told = |w: &TestService| {
        let session = agent.session.clone();
        let deliveries = w.deliveries.clone();
        async move {
            deliveries
                .pending(&session)
                .await
                .unwrap()
                .into_iter()
                .map(|p| p.text)
                .collect::<Vec<_>>()
        }
    };
    submit(&w, &t).await;
    verdict(&w, &t, true).await;
    verdict(&w, &t, false).await;
    assert_eq!(
        told(&w).await.last().map(String::as_str),
        Some(
            "Security failed round 1: Security reviewer failed, Validator passed. Run `otaq prime`."
        )
    );

    submit(&w, &t).await;
    let before = verdict(&w, &t, true).await;
    assert_eq!(at(&before), (Some("security"), Status::Checking));
    let mut after = before.clone();
    after.body = "Search titles too.".into();
    w.validation.ticket_edited(&before, &after).await.unwrap();
    assert!(
        told(&w).await.contains(
            &"Security was cancelled because the code changed. Run `otaq stage next` when you're done."
                .to_owned()
        )
    );
    w.stop().await;
}
