//! Stages replace the fixed columns and states (T-0400; STG-04, STG-05, STG-06; d0108, d0004, d0034, d0063, d0079):
//! the flow engine's moves row by row on the default flow (SDD §7.3, §7.4), the same engine on the test flow
//! (`tests/flows/every-kind.yaml`: a Fail walks every later stage again, each agent check counts its own fails, a
//! human Fail resets them, a skipped stage gets a skipped run, a human check before a click Delivery offers its
//! choices), the stage runs of a full run of NOTE-4, and no column left in the core. `claude` stands in as `sh`;
//! every data folder is temporary.

use otaq_core::service::testing::said;
use std::collections::HashMap;
use std::path::Path;

use otaq_core::agent_tools::LaunchContext;
use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::board::moves::{DropOn, DropTarget, MoveAction, MoveCheck};
use otaq_core::domain::{Actor, Kind, Place, Status, ValidationTrigger};
use otaq_core::flow::default::{DELIVER, DEV, REVIEW, VALIDATE};
use otaq_core::flow::engine::{self, At, Counters, Move, RunResult, Step};
use otaq_core::flow::{Delivers, DeliveryMode, Flow, default_flow, yaml};
use otaq_core::git::testing::commit;
use otaq_core::review::LocalMergeOutcome;
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::TestService;
use otaq_core::session::SessionKind;

fn every_kind() -> Flow {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/flows/every-kind.yaml");
    yaml::read(&std::fs::read_to_string(path).unwrap())
        .unwrap()
        .flow
}

fn without_validate() -> Flow {
    let mut flow = default_flow();
    flow.stages.retain(|s| s.id != VALIDATE);
    flow
}

/// The engine's step for a ticket with `status` in `place`.
fn go<'f>(flow: &'f Flow, place: &Place, status: Status, mv: Move) -> Step<'f> {
    engine::step(At::new(flow, place, status), mv).unwrap_or_else(|e| panic!("{mv:?}: {e:?}"))
}

fn refused(flow: &Flow, place: &Place, status: Status, mv: Move) -> bool {
    engine::step(At::new(flow, place, status), mv).is_err()
}

fn stage(id: &str) -> Place {
    Place::stage_of(id)
}

fn reason(check: &MoveCheck) -> &str {
    match check {
        MoveCheck::Refused { reason } => reason,
        other => panic!("not refused: {other:?}"),
    }
}

// ---- §7.3 on the default flow, row by row ----

/// Backlog → Ready for Dev (Start, a drag on Ready for Dev or Dev); Ready for Dev → Backlog; a Parked ticket's drag
/// to Dev; the scheduler's start into Dev.
#[test]
fn backlog_and_ready_for_dev() {
    let flow = default_flow();
    for target in [DropTarget::ReadyForDev, DropTarget::Dev] {
        for status in [Status::Backlog, Status::Parked] {
            assert_eq!(
                engine::drag(&flow, &Place::Backlog, status, Kind::Ticket, target),
                MoveCheck::Allowed {
                    action: MoveAction::Start
                },
                "{status} → {target}"
            );
        }
        assert_eq!(
            engine::drag(&flow, &Place::Backlog, Status::Backlog, Kind::Epic, target),
            MoveCheck::Allowed {
                action: MoveAction::QueueEpic
            }
        );
    }
    let start = go(&flow, &Place::Backlog, Status::Backlog, Move::Start);
    assert_eq!(
        (start.place, start.status),
        (Place::Waiting(None), Status::Queued)
    );
    let waiting = Place::Waiting(None);
    assert_eq!(
        engine::drag(
            &flow,
            &waiting,
            Status::Queued,
            Kind::Ticket,
            DropTarget::Backlog
        ),
        MoveCheck::Allowed {
            action: MoveAction::Unqueue
        }
    );
    let begin = go(&flow, &waiting, Status::Queued, Move::Begin);
    assert_eq!(
        (begin.place, begin.status, begin.ended),
        (stage(DEV), Status::Working, None)
    );
}

/// Working → Backlog (Park, after a confirmation); Working → Ready for Review (`review submit`, validation off);
/// Working → Validating (validation on, also the user's Move to next stage); the engine's `SubmitByUser` (the
/// board's Move to next stage when no agent check comes next) walks past Validate as skipped.
#[test]
fn from_dev() {
    let flow = default_flow();
    assert_eq!(
        engine::drag(
            &flow,
            &stage(DEV),
            Status::Working,
            Kind::Ticket,
            DropTarget::Backlog
        ),
        MoveCheck::Confirm {
            action: MoveAction::Park
        }
    );
    let off = without_validate();
    let submit = go(&off, &stage(DEV), Status::Working, Move::NextStage);
    assert_eq!(
        (
            submit.place,
            submit.status,
            submit.ended,
            submit.start_check
        ),
        (
            stage(REVIEW),
            Status::WaitingForYou,
            Some(RunResult::Done),
            false
        )
    );
    for status in [Status::Working, Status::Idle, Status::MergeConflictFixing] {
        let submit = go(&flow, &stage(DEV), status, Move::NextStage);
        assert_eq!(
            (submit.place, submit.status, submit.start_check),
            (stage(VALIDATE), Status::Checking, true),
            "{status}"
        );
    }
    let user = go(&flow, &stage(DEV), Status::Idle, Move::SubmitByUser);
    assert_eq!(
        (user.place, user.status),
        (stage(REVIEW), Status::WaitingForYou)
    );
    let skipped: Vec<_> = user
        .passed_through
        .iter()
        .map(|(s, r)| (s.id.as_str(), *r))
        .collect();
    assert_eq!(skipped, [(VALIDATE, RunResult::Skipped)]);
    assert!(!user.automatic);
    assert!(refused(
        &flow,
        &stage(VALIDATE),
        Status::Checking,
        Move::NextStage
    ));
}

/// Validating → Working (fail with rounds left: the findings, the same session; code changed and Stop: not a
/// round); → Ready for Review (pass "Validated · round N", Skip this check "Validation skipped by you"); the last
/// round failed, no verdict: turquoise in the check; then Move on anyway, Give it another round, Answer the
/// agent, Retry validation.
#[test]
fn from_validating() {
    let flow = default_flow();
    let checking = |mv| go(&flow, &stage(VALIDATE), Status::Checking, mv);
    let fail = checking(Move::Fail);
    assert_eq!(
        (
            fail.place,
            fail.status,
            fail.resume,
            fail.counters,
            fail.ended
        ),
        (
            stage(DEV),
            Status::Working,
            true,
            Counters::Count,
            Some(RunResult::Failed)
        )
    );
    let last = engine::step(
        At {
            fails: 2,
            ..At::new(&flow, &stage(VALIDATE), Status::Checking)
        },
        Move::Fail,
    )
    .unwrap();
    assert_eq!(
        (last.place, last.status, last.resume),
        (stage(VALIDATE), Status::CheckFailed, false)
    );
    let back = checking(Move::CancelCheck);
    assert_eq!(
        (back.place, back.status, back.counters, back.ended),
        (
            stage(DEV),
            Status::Working,
            Counters::Keep,
            Some(RunResult::Cancelled)
        )
    );
    let pass = checking(Move::Pass);
    assert_eq!(
        (pass.place, pass.status, pass.ended),
        (
            stage(REVIEW),
            Status::WaitingForYou,
            Some(RunResult::Passed)
        )
    );
    let skip = checking(Move::SkipCheck);
    assert_eq!(
        (skip.place, skip.status, skip.ended, skip.automatic),
        (
            stage(REVIEW),
            Status::WaitingForYou,
            Some(RunResult::Skipped),
            false
        )
    );
    let none = checking(Move::NoResult);
    assert_eq!(
        (none.place, none.status, none.ended),
        (
            stage(VALIDATE),
            Status::CheckNoResult,
            Some(RunResult::NoResult)
        )
    );
    for status in [Status::CheckFailed, Status::CheckNoResult] {
        let anyway = go(&flow, &stage(VALIDATE), status, Move::MoveOnAnyway);
        assert_eq!(
            (anyway.place, anyway.status, anyway.automatic),
            (stage(REVIEW), Status::WaitingForYou, false)
        );
    }
    let another = go(
        &flow,
        &stage(VALIDATE),
        Status::CheckFailed,
        Move::AnotherRound,
    );
    assert_eq!(
        (
            another.place,
            another.status,
            another.resume,
            another.counters
        ),
        (stage(DEV), Status::Working, true, Counters::Keep)
    );
    let answer = go(
        &flow,
        &stage(VALIDATE),
        Status::CheckFailed,
        Move::AnswerAgent,
    );
    assert_eq!(answer.counters, Counters::ResetThis);
    let retry = go(
        &flow,
        &stage(VALIDATE),
        Status::CheckNoResult,
        Move::RetryCheck,
    );
    assert_eq!(
        (retry.place, retry.status, retry.start_check),
        (stage(VALIDATE), Status::Checking, true)
    );
    assert!(refused(
        &flow,
        &stage(VALIDATE),
        Status::CheckFailed,
        Move::RetryCheck
    ));
    assert!(refused(&flow, &stage(DEV), Status::Working, Move::Pass));
}

/// Ready for Review → Dev (Send back: the same session, every counter at 0); Approve offers Local merge or Open PR
/// (one click: the Delivery's own); Local merge → Done · Merged; Open PR → Approved · PR open; Push to PR from
/// Review; PR rework back to Review; Mark merged; Mark closed and Close ticket; Reopen.
#[test]
fn review_and_delivery() {
    let flow = default_flow();
    let review = stage(REVIEW);
    let back = go(&flow, &review, Status::WaitingForYou, Move::HumanFail);
    assert_eq!(
        (
            back.place,
            back.status,
            back.resume,
            back.counters,
            back.ended
        ),
        (
            stage(DEV),
            Status::Working,
            true,
            Counters::ResetAll,
            Some(RunResult::Failed)
        )
    );
    let approve = go(&flow, &review, Status::WaitingForYou, Move::HumanPass);
    assert_eq!(
        (approve.place.clone(), approve.status, approve.choose),
        (
            review.clone(),
            Status::WaitingForYou,
            Some(Delivers::Either)
        ),
        "Approve asks Local merge or Open PR; nothing moves yet"
    );
    assert_eq!(
        engine::drag(
            &flow,
            &review,
            Status::WaitingForYou,
            Kind::Ticket,
            DropTarget::Approved
        ),
        MoveCheck::Choose {
            action: MoveAction::Approve
        }
    );
    let merged = go(&flow, &review, Status::WaitingForYou, Move::Delivered);
    assert_eq!(
        (merged.place, merged.status, merged.ended),
        (Place::Done, Status::Merged, Some(RunResult::Passed))
    );
    let through: Vec<_> = merged
        .passed_through
        .iter()
        .map(|(s, r)| (s.id.as_str(), *r))
        .collect();
    assert_eq!(through, [(DELIVER, RunResult::Done)]);
    let pr = go(&flow, &review, Status::WaitingForYou, Move::PrOpened);
    assert_eq!(
        (pr.place, pr.status, pr.ended),
        (stage(DELIVER), Status::PrOpen, Some(RunResult::Passed))
    );
    let deliver = stage(DELIVER);
    let pushed = go(&flow, &deliver, Status::PrOpen, Move::PrOpened);
    assert_eq!(
        (pushed.place.clone(), pushed.ended),
        (deliver.clone(), None)
    );
    let rework = go(&flow, &deliver, Status::PrOpen, Move::PrRework);
    assert_eq!(
        (rework.place, rework.status),
        (stage(REVIEW), Status::WaitingForYou)
    );
    assert_eq!(
        engine::drag(
            &flow,
            &deliver,
            Status::PrOpen,
            Kind::Ticket,
            DropTarget::ReadyForReview
        ),
        MoveCheck::Allowed {
            action: MoveAction::PrRework
        }
    );
    let marked = go(&flow, &deliver, Status::PrOpen, Move::Delivered);
    assert_eq!(
        (marked.place, marked.status, marked.ended),
        (Place::Done, Status::Merged, Some(RunResult::Done))
    );
    for (place, status) in [
        (deliver.clone(), Status::PrOpen),
        (stage(DEV), Status::Working),
        (Place::Backlog, Status::Backlog),
    ] {
        let closed = go(&flow, &place, status, Move::Close);
        assert_eq!(
            (closed.place, closed.status),
            (Place::Done, Status::ClosedNotMerged)
        );
    }
    assert!(refused(&flow, &Place::Done, Status::Merged, Move::Close));
    assert_eq!(
        engine::drag(
            &flow,
            &Place::Done,
            Status::ClosedNotMerged,
            Kind::Ticket,
            DropTarget::Backlog
        ),
        MoveCheck::Allowed {
            action: MoveAction::Reopen
        }
    );
    assert_eq!(
        reason(&engine::drag(
            &flow,
            &Place::Done,
            Status::Merged,
            Kind::Ticket,
            DropTarget::Backlog
        )),
        "Merged tickets are locked"
    );
}

/// The drags §7.4 still refuses (Dev → Ready for Dev), and FLW-18's drops on the default flow: forward from Dev
/// (Validate is the next stage, Review skips it), back from Review and Validate to Dev with the instructions dialog,
/// Review → Validate runs it again with a note; a card dropped where it is stays.
#[test]
fn drags_on_the_default_flow() {
    let flow = default_flow();
    let drag = |place: &Place, status, target: DropOn| {
        engine::drag(&flow, place, status, Kind::Ticket, target)
    };
    let on = DropOn::stage;
    let allowed = |action| MoveCheck::Allowed { action };
    let confirm = |action| MoveCheck::Confirm { action };
    assert_eq!(
        reason(&drag(
            &stage(DEV),
            Status::Working,
            DropTarget::ReadyForDev.into()
        )),
        "Drag it to Backlog to park it"
    );
    assert_eq!(
        drag(&stage(DEV), Status::Idle, on(VALIDATE)),
        allowed(MoveAction::NextStage)
    );
    assert_eq!(
        drag(&stage(DEV), Status::Idle, DropTarget::ReadyForReview.into()),
        allowed(MoveAction::SkipTo)
    );
    assert_eq!(
        drag(
            &stage(VALIDATE),
            Status::Checking,
            DropTarget::ReadyForReview.into()
        ),
        allowed(MoveAction::SkipTo),
        "out of a check: it is skipped"
    );
    for (place, status) in [
        (stage(REVIEW), Status::WaitingForYou),
        (stage(VALIDATE), Status::Checking),
        (stage(VALIDATE), Status::CheckFailed),
        (stage(DELIVER), Status::PrOpen),
    ] {
        assert_eq!(
            drag(&place, status, DropTarget::Dev.into()),
            confirm(MoveAction::SendBack),
            "{status}"
        );
    }
    assert_eq!(
        drag(&stage(REVIEW), Status::WaitingForYou, on(VALIDATE)),
        confirm(MoveAction::RunCheckAgain)
    );
    assert_eq!(
        drag(&stage(VALIDATE), Status::Checking, on(VALIDATE)),
        MoveCheck::Unchanged
    );
    assert_eq!(
        drag(&stage(DELIVER), Status::PrOpen, DropTarget::Approved.into()),
        MoveCheck::Unchanged
    );
    // An epic can't be dragged back (0059).
    assert_eq!(
        reason(&engine::drag(
            &flow,
            &stage(REVIEW),
            Status::WaitingForYou,
            Kind::Epic,
            DropTarget::ReadyForDev
        )),
        "An epic can\u{2019}t be dragged back"
    );
}

// ---- The test flow ----

/// A card walking the test flow: its place and status, each agent check's fails in a row, and the runs it ended.
struct Card<'f> {
    flow: &'f Flow,
    place: Place,
    status: Status,
    fails: HashMap<String, u32>,
    runs: Vec<(String, RunResult)>,
    skips: Vec<String>,
}

impl<'f> Card<'f> {
    fn new(flow: &'f Flow, skips: &[&str]) -> Self {
        Self {
            flow,
            place: Place::Waiting(None),
            status: Status::Queued,
            fails: HashMap::new(),
            runs: Vec::new(),
            skips: skips.iter().map(|s| (*s).to_owned()).collect(),
        }
    }

    fn at(&self) -> Option<&str> {
        self.place.stage()
    }

    fn apply(&mut self, mv: Move) -> Step<'f> {
        let current = self.at().map(str::to_owned);
        let fails = current
            .as_ref()
            .and_then(|id| self.fails.get(id))
            .copied()
            .unwrap_or(0);
        let rounds = current
            .as_ref()
            .and_then(|id| self.flow.stage(id))
            .map_or(3, |s| engine::rounds_of(s, 0));
        let step = engine::step(
            At {
                fails,
                rounds,
                skips: &self.skips,
                ..At::new(self.flow, &self.place, self.status)
            },
            mv,
        )
        .unwrap_or_else(|e| panic!("{mv:?} at {:?}: {e:?}", self.place));
        if let (Some(result), Some(id)) = (step.ended, &current) {
            self.runs.push((id.clone(), result));
        }
        for (stage, result) in &step.passed_through {
            self.runs.push((stage.id.clone(), *result));
        }
        match (step.counters, &current) {
            (Counters::Count, Some(id)) if step.ended == Some(RunResult::Failed) => {
                *self.fails.entry(id.clone()).or_default() += 1;
            }
            (Counters::ResetThis, Some(id)) => {
                self.fails.insert(id.clone(), 0);
            }
            (Counters::ResetAll, _) => self.fails.clear(),
            _ => {}
        }
        self.place = step.place.clone();
        self.status = step.status;
        step
    }
}

/// A Security fail returns to Dev (its session resumes) and the card walks Validate, Security and QA again; each
/// check counts its own fails, Security stops at its 2; a QA Fail goes to Grill (its `fail_to`) and resets every
/// counter.
#[test]
fn a_fail_walks_every_later_stage_again() {
    let flow = every_kind();
    let mut card = Card::new(&flow, &[]);
    card.apply(Move::Begin);
    assert_eq!(card.at(), Some("grill"));
    card.apply(Move::NextStage);
    // Dev is an Agent stage: the card waits before it until the scheduler starts it (STG-07).
    assert_eq!(
        (&card.place, card.status),
        (&Place::Waiting(Some("dev".into())), Status::Queued)
    );
    card.apply(Move::Begin);
    assert_eq!(card.at(), Some("dev"));
    card.apply(Move::NextStage);
    assert_eq!(
        (card.at(), card.status),
        (Some("validate"), Status::Checking)
    );
    let step = card.apply(Move::Pass);
    assert!(step.start_check, "Security starts its own checker");
    assert_eq!(
        (card.at(), card.status),
        (Some("security"), Status::Checking)
    );

    let back = card.apply(Move::Fail);
    assert_eq!((card.at(), card.status), (Some("dev"), Status::Working));
    assert!(back.resume, "Dev's session resumes with the findings");
    assert_eq!(card.fails["security"], 1);
    // Every later stage again: Validate first, though it passed before.
    card.apply(Move::NextStage);
    assert_eq!(card.at(), Some("validate"));
    card.apply(Move::Fail);
    assert_eq!(card.at(), Some("dev"));
    assert_eq!(
        (card.fails["validate"], card.fails["security"]),
        (1, 1),
        "each check counts its own"
    );
    card.apply(Move::NextStage);
    card.apply(Move::Pass);
    assert_eq!(card.at(), Some("security"));
    card.apply(Move::Fail);
    assert_eq!(
        (card.at(), card.status),
        (Some("security"), Status::CheckFailed),
        "Security's 2 rounds are used: it stops, turquoise"
    );
    card.apply(Move::MoveOnAnyway);
    assert_eq!(
        (card.at(), card.status),
        (Some("qa"), Status::WaitingForYou)
    );
    // A QA Fail: back to Grill, where QA's fails go (d0131), every counter at 0; from there every later stage again,
    // Dev included.
    let fail = card.apply(Move::HumanFail);
    assert_eq!(fail.counters, Counters::ResetAll);
    assert!(fail.resume, "Grill's session resumes with the comments");
    assert_eq!(card.at(), Some("grill"));
    assert!(card.fails.values().all(|n| *n == 0) || card.fails.is_empty());
    card.apply(Move::NextStage);
    assert_eq!(card.place, Place::Waiting(Some("dev".into())));
    card.apply(Move::Begin);
    card.apply(Move::NextStage);
    card.apply(Move::Pass);
    card.apply(Move::Fail);
    assert_eq!(
        (card.at(), card.status),
        (Some("dev"), Status::Working),
        "Security may fail again after the reset"
    );
    let stages: Vec<&str> = card.runs.iter().map(|(s, _)| s.as_str()).collect();
    assert_eq!(
        stages,
        [
            "grill", "dev", "validate", "security", "dev", "validate", "dev", "validate",
            "security", "qa", "grill", "dev", "validate", "security"
        ]
    );
}

/// Grill skipped (its `skip` option taken): the start goes straight to Dev, and Grill has a skipped run. QA skipped
/// too: its Delivery is in click mode (AGM-08).
#[test]
fn a_skipped_stage_gets_a_skipped_run() {
    let flow = every_kind();
    let mut card = Card::new(&flow, &["grill"]);
    let begin = card.apply(Move::Begin);
    assert_eq!(card.at(), Some("dev"));
    let skipped: Vec<_> = begin
        .passed_through
        .iter()
        .map(|(s, r)| (s.id.as_str(), *r))
        .collect();
    assert_eq!(skipped, [("grill", RunResult::Skipped)]);
    // Dev can't be skipped (no option), and a Fail never goes back into a skipped stage.
    assert_eq!(
        engine::back_to(&flow, "validate", &card.skips).map(|s| s.id.as_str()),
        Some("dev")
    );
    let mut both = Card::new(&flow, &["grill", "qa"]);
    both.apply(Move::Begin);
    both.apply(Move::NextStage);
    both.apply(Move::Pass);
    let into_ship = both.apply(Move::Pass);
    assert_eq!(both.at(), Some("ship"), "QA skipped");
    assert_eq!(
        both.runs.last().map(|(s, r)| (s.as_str(), *r)),
        Some(("qa", RunResult::Skipped))
    );
    assert!(
        !into_ship.automatic,
        "QA, a human check, was skipped: the automatic PR Delivery waits for a click (d0108)"
    );
}

/// QA (a human check) passes into the automatic PR Delivery: the card enters it. With the Delivery in click mode
/// QA's Pass offers the Delivery's choice instead and the card stays in QA until the user picks.
#[test]
fn a_human_check_before_a_click_delivery_offers_its_choices() {
    let flow = every_kind();
    let qa = stage("qa");
    let pass = go(&flow, &qa, Status::WaitingForYou, Move::HumanPass);
    assert_eq!(
        (pass.place, pass.status, pass.choose, pass.automatic),
        (stage("ship"), Status::ToDeliver, None, true)
    );
    let mut click = flow.clone();
    let ship = click.stages.iter_mut().find(|s| s.id == "ship").unwrap();
    ship.delivery.as_mut().unwrap().mode = DeliveryMode::Click;
    let pass = go(&click, &qa, Status::WaitingForYou, Move::HumanPass);
    assert_eq!(
        (pass.place, pass.status, pass.choose),
        (qa.clone(), Status::WaitingForYou, Some(Delivers::Pr))
    );
    let opened = go(&click, &qa, Status::WaitingForYou, Move::PrOpened);
    assert_eq!(
        (opened.place, opened.status),
        (stage("ship"), Status::PrOpen)
    );
    // A human check not right before the Delivery passes on to the next stage (Review in a custom flow, say).
    let pass = go(&flow, &stage("security"), Status::Checking, Move::Pass);
    assert_eq!((pass.place, pass.status), (qa, Status::WaitingForYou));
}

// ---- Stage runs on the board ----

/// An ordinary command standing in for `claude` (agent and validator alike).
fn stand_in() -> LaunchContext {
    LaunchContext {
        bin_dir: "/nonexistent".into(),
        plugin_dir: "/nonexistent/plugin".into(),
        path: std::env::var_os("PATH").unwrap_or_default(),
        claude: ClaudeLaunch {
            program: vec![
                "sh".into(),
                "-c".into(),
                "echo started; exec cat".into(),
                "sh".into(),
            ],
            config_dir: None,
        },
    }
}

fn me() -> Actor {
    Actor::User("me".into())
}

/// The example's pocket-notes, key NOTE.
async fn pocket_notes(w: &TestService) {
    let repo = w.repos.repo("pocket-notes", 1);
    w.board
        .add_project(&otaq_core::board::projects::AddProject {
            path: repo.display().to_string(),
            key: Some("NOTE".into()),
            ..Default::default()
        })
        .await
        .unwrap();
}

async fn until_status(w: &TestService, key: &str, status: Status) {
    for _ in 0..500 {
        if w.status(key).await == status {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    panic!("{key} never became {status}");
}

/// What a session's row says it ran on: its agent's name and its model.
async fn ran_on(w: &TestService, session: &str) -> (Option<String>, Option<String>) {
    sqlx::query_as("SELECT agent_name, COALESCE(model_ran, model) FROM sessions WHERE id = ?")
        .bind(session)
        .fetch_one(w.board.store().pool())
        .await
        .unwrap()
}

/// A full run of NOTE-4 "Search notes" on pocket-notes (Validation on): Dev (its agent session), Validate round 1
/// (the validator's session) passed, Review passed by Approve → Local merge, Done's Delivery done; each run with its
/// stage, kind, session, agent and model, round and result.
#[tokio::test(flavor = "multi_thread")]
async fn a_full_run_records_every_stage() {
    let w = TestService::start_with(|config| ServiceConfig {
        launch: Some(stand_in()),
        ..config
    })
    .await;
    pocket_notes(&w).await;
    for title in [
        "Sort notes by date",
        "Export as CSV",
        "Pin a note",
        "Search notes",
    ] {
        w.create("NOTE", title).await;
    }
    let key = "NOTE-4";
    let t = w.board.ticket(key).await.unwrap();
    w.board
        .move_ticket(key, DropTarget::ReadyForDev, true, &me())
        .await
        .unwrap();
    w.starter
        .start(key, &me(), otaq_core::session::Start::Auto)
        .await
        .unwrap();
    until_status(&w, key, Status::Working).await;
    let agent = w
        .sessions
        .running_of(&t.id, SessionKind::Agent)
        .expect("the agent runs");
    let t = w.board.ticket(key).await.unwrap();
    assert_eq!(t.place, Place::stage_of(DEV));
    let worktree = t.work.as_ref().unwrap().worktree.clone().unwrap();
    commit(Path::new(&worktree), "search.ts", "search\n");

    let validating = w
        .validation
        .submit(
            key,
            said(&w.board, key, "Search works.").await.as_ref(),
            &Actor::Agent(t.key.clone()),
            ValidationTrigger::Agent,
        )
        .await
        .unwrap();
    assert_eq!(
        (validating.place, validating.status),
        (Place::stage_of(VALIDATE), Status::Checking)
    );
    let validator = w
        .sessions
        .running_of(&t.id, SessionKind::Validator)
        .expect("a validator runs");
    let reviewed = w
        .validation
        .verdict(
            key,
            true,
            said(&w.board, key, "All good.").await.as_ref(),
            &validator.id,
        )
        .await
        .unwrap();
    assert_eq!(
        (reviewed.place, reviewed.status),
        (Place::stage_of(REVIEW), Status::WaitingForYou)
    );
    let merged = w
        .review
        .local_merge(key, "NOTE-4: Search notes", false, &me())
        .await
        .unwrap();
    assert!(
        matches!(merged, LocalMergeOutcome::Merged { .. }),
        "{merged:?}"
    );
    let done = w.board.ticket(key).await.unwrap();
    assert_eq!((done.place, done.status), (Place::Done, Status::Merged));

    let runs = w.board.stage_runs(key).await.unwrap();
    let shape: Vec<_> = runs
        .iter()
        .map(|r| {
            (
                r.stage.as_str(),
                r.kind.as_str(),
                r.session.as_deref(),
                r.round,
                r.result,
            )
        })
        .collect();
    assert_eq!(
        shape,
        [
            (
                DEV,
                "agent",
                Some(agent.id.as_str()),
                None,
                Some(RunResult::Done)
            ),
            (
                VALIDATE,
                "check",
                Some(validator.id.as_str()),
                Some(1),
                Some(RunResult::Passed)
            ),
            (REVIEW, "you", None, None, Some(RunResult::Passed)),
            (DELIVER, "delivery", None, None, Some(RunResult::Done)),
        ]
    );
    for (run, session) in [(&runs[0], &agent.id), (&runs[1], &validator.id)] {
        let (name, model) = ran_on(&w, session).await;
        assert_eq!(
            (run.agent.clone(), run.model.clone()),
            (name, model),
            "{}",
            run.stage
        );
    }
    assert_eq!(runs[0].summary.as_deref(), Some("Search works."));
    assert_eq!(runs[1].summary.as_deref(), Some("All good."));
    assert!(runs.iter().all(|r| r.ended_at.is_some()));
    w.stop().await;
}

/// The user's Move to next stage runs Validate (d0108 rule 10; it used to skip it) and Skip this check records
/// Validate's run as skipped, then Review runs; a Send back fails Review's run. With no live agent the ticket waits
/// before Dev (the stand-in may start it there at once).
#[tokio::test(flavor = "multi_thread")]
async fn skipped_and_failed_runs() {
    let w = TestService::start_with(|config| ServiceConfig {
        launch: Some(stand_in()),
        ..config
    })
    .await;
    pocket_notes(&w).await;
    let t = w.create("NOTE", "Export as CSV").await;
    w.give_worktree(&t).await;
    w.force_status(&t, Status::Idle).await;
    let checking = w
        .validation
        .submit_by_user("NOTE-1", "", &me())
        .await
        .unwrap();
    assert_eq!(checking.status, Status::Checking);
    w.validation
        .user_action(
            "NOTE-1",
            otaq_core::domain::ValidationAction::Skip,
            "",
            &me(),
        )
        .await
        .unwrap();
    w.review
        .fail("NOTE-1", "Use a comma", None, false, &me())
        .await
        .unwrap();
    let runs: Vec<_> = w
        .board
        .stage_runs("NOTE-1")
        .await
        .unwrap()
        .into_iter()
        .map(|r| (r.stage, r.result))
        .take(3)
        .collect();
    assert_eq!(
        runs,
        [
            (DEV.to_owned(), Some(RunResult::Done)),
            (VALIDATE.to_owned(), Some(RunResult::Skipped)),
            (REVIEW.to_owned(), Some(RunResult::Failed)),
        ]
    );
    let back = w.board.ticket("NOTE-1").await.unwrap();
    assert!(
        back.place.is_ready_for_dev() || back.place == Place::stage_of(DEV),
        "{:?}",
        back.place
    );
    w.stop().await;
}

// ---- No column left ----

/// STG-04: nothing in the core stores or matches on a board column: the old type is gone and no source names
/// `board_column` (migrations up to 0046 excepted). The flow's own `column: own | with-previous` stays in `flow/`.
#[test]
fn no_column_left_in_the_core() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut offenders = Vec::new();
    let mut stack = vec![src.clone()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().is_none_or(|e| e != "rs") || path.starts_with(src.join("flow")) {
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap();
            for (n, line) in text.lines().enumerate() {
                let code = line.split("//").next().unwrap_or_default();
                let names_a_column = code
                    .split(|c: char| !c.is_alphanumeric() && c != '_')
                    .any(|word| word == "Column" || word == "board_column");
                if names_a_column {
                    offenders.push(format!("{}:{}: {line}", path.display(), n + 1));
                }
            }
        }
    }
    assert!(offenders.is_empty(), "{offenders:#?}");
}
