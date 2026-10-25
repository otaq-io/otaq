//! The moves between stages (STG-06, d0108; the default flow's rows are SDD §7.3 and §7.4): one pure place that
//! says, from a project's flow, where a ticket goes. It does no I/O; the board, validation and review code carry
//! the result out (`board/validation.rs::transition`, `board/moves.rs::check_move` and the review moves).
//!
//! - [`step`]: what a [`Move`] does where a ticket stands: the agent's "next stage", an agent check's verdict, a
//!   human check's Pass or Fail, a Delivery finishing, and the user's shortcuts (Skip this check, Move on anyway…).
//!   A Fail returns the card to the check's fails-go-to stage ([`fail_to`], d0131; the nearest earlier Agent stage the
//!   ticket runs when it skips that one), which resumes its session; from there it walks every later stage again. Each agent check counts its own fails in a row ([`Counters`]); a human Fail resets
//!   them all. A human check directly followed by a click Delivery offers the delivery choices as its Pass.
//!   Moving on into an Agent stage waits before it (STG-07): only the scheduler's [`Move::Begin`] enters it.
//! - [`drag`]: a drag's verdict (§7.4, FLW-18): today's drag rules and a drop on any stage, by stage kind.
//! - [`default_stage`]: the default flow's stage for a status, for the writes that don't know the flow yet (until
//!   T-0401–T-0403 move them onto stages).

use serde::{Deserialize, Serialize};

use super::default::{DEV, REVIEW, VALIDATE};
use super::model::{CheckBy, Column, Delivers, DeliveryMode, Flow, Stage, StageKind};
use crate::board::moves::{
    DropOn, DropTarget, MERGED_LOCKED, MoveAction, MoveCheck, ONLY_AGENT_SUBMITS, QUEUE_FIRST,
    STARTS_BY_ITSELF,
};
use crate::domain::{Place, Status, StatusSet, stored_enum};

stored_enum! {
    /// How a stage run ended (STG-05, `stage_runs.result`).
    pub enum RunResult ("stage run result") {
        /// A check passed the work.
        Passed = "passed",
        /// A check failed it.
        Failed = "failed",
        /// An Agent stage or a Delivery finished.
        Done = "done",
        /// The ticket skipped the stage, or the user skipped the running check.
        Skipped = "skipped",
        /// Stopped before it ended: parked, closed, the code changed under a check.
        Cancelled = "cancelled",
        /// Ended without a verdict.
        NoResult = "no_result",
    }
}

/// What a stage run's kind is stored as (`stage_runs.kind`).
pub fn run_kind(kind: StageKind) -> &'static str {
    match kind {
        StageKind::Agent => "agent",
        StageKind::Check { by: CheckBy::Agent } => "check",
        StageKind::Check { by: CheckBy::You } => "you",
        StageKind::Delivery => "delivery",
    }
}

/// The default flow's stage for a status written without a stage: the validation statuses (and an Interrupted or
/// limit-held validation, `interrupted_from`) are Validate's, the other Agent stage statuses Dev's, Ready for
/// Review is Review's, the Delivery statuses Done's. `None` for Backlog, waiting and Done.
pub fn default_stage(status: Status, interrupted_from: Option<Status>) -> Option<&'static str> {
    let check = |s: Status| s.set() == StatusSet::AgentCheck;
    match status.set() {
        StatusSet::Backlog | StatusSet::Waiting | StatusSet::Done => None,
        StatusSet::AgentCheck => Some(VALIDATE),
        StatusSet::AgentStage if interrupted_from.is_some_and(check) => Some(VALIDATE),
        StatusSet::AgentStage => Some(DEV),
        StatusSet::HumanCheck => Some(REVIEW),
        StatusSet::Delivery => Some(super::default::DELIVER),
    }
}

/// The status a ticket has on entering a stage, by its kind.
pub fn entry_status(stage: &Stage) -> Status {
    match stage.kind {
        StageKind::Agent => Status::Working,
        StageKind::Check { by: CheckBy::Agent } => Status::Checking,
        StageKind::Check { by: CheckBy::You } => Status::WaitingForYou,
        StageKind::Delivery => Status::ToDeliver,
    }
}

/// The set of statuses a stage of this kind allows (STG-04).
pub fn status_set(kind: StageKind) -> StatusSet {
    match kind {
        StageKind::Agent => StatusSet::AgentStage,
        StageKind::Check { by: CheckBy::Agent } => StatusSet::AgentCheck,
        StageKind::Check { by: CheckBy::You } => StatusSet::HumanCheck,
        StageKind::Delivery => StatusSet::Delivery,
    }
}

/// An agent check's limit: its rounds (3 when the flow doesn't say) plus the extra rounds the user allowed.
pub fn rounds_of(stage: &Stage, extra: u32) -> u32 {
    stage
        .rounds
        .unwrap_or(crate::flow::check::DEFAULT_VALIDATION_ROUNDS)
        + extra
}

/// What happened, or what the user or an agent asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Move {
    /// The agent's "next stage" from an Agent stage (`stage next`), or the user's Move to next stage.
    NextStage,
    /// The board's move for the user's Move to next stage when no agent check comes next (an agent check runs
    /// through `NextStage`, d0108 rule 10): to the next stage, any agent checks walked over counting as skipped
    /// (d0052; never delivered automatically).
    SubmitByUser,
    /// An agent check's `pass`.
    Pass,
    /// An agent check's `fail`.
    Fail,
    /// The checker ended without a verdict, or its verdict was discarded.
    NoResult,
    /// The check stopped without counting: the code or the description changed, or the user's Stop.
    CancelCheck,
    /// A fresh checker after no result.
    RetryCheck,
    /// The user skips the running check (Skip this check).
    SkipCheck,
    /// After the last allowed fail or no result: on to the next stage (Move on anyway).
    MoveOnAnyway,
    /// After the last allowed fail: back to the agent with the findings, one more round allowed.
    AnotherRound,
    /// After the last allowed fail: back to the agent with the user's note; this check's counter restarts.
    AnswerAgent,
    /// A human check's Pass (Approve).
    HumanPass,
    /// A human check's Fail (Send back).
    HumanFail,
    /// The work was delivered: the Local merge landed, or the PR was marked merged.
    Delivered,
    /// The PR was opened (or pushed again).
    PrOpened,
    /// PR rework: back from the open PR to the human check before the Delivery (d0041).
    PrRework,
    /// Send back with PR comments (FLW-16): from the open PR to the nearest earlier Agent stage, whose session
    /// resumes; every later stage runs again and the Delivery pushes to the same PR.
    PrComments,
    /// Mark closed, or Close ticket.
    Close,
    /// Into the queue from Backlog.
    Start,
    /// The scheduler starts a waiting ticket: into its Agent stage.
    Begin,
}

/// What the engine needs to know about a ticket.
#[derive(Debug, Clone, Copy)]
pub struct At<'f, 'a> {
    pub flow: &'f Flow,
    pub place: &'a Place,
    pub status: Status,
    /// The current agent check's fails in a row.
    pub fails: u32,
    /// The current agent check's limit ([`rounds_of`]).
    pub rounds: u32,
    /// The stages this ticket skips: its choices of `skip` ([`crate::board::Board::ticket_skips`], AGM-08).
    pub skips: &'a [String],
}

impl<'f, 'a> At<'f, 'a> {
    /// A ticket in `place` with `status` that skips nothing, its counter at 0 and its check's own rounds.
    pub fn new(flow: &'f Flow, place: &'a Place, status: Status) -> Self {
        let rounds = place
            .stage()
            .and_then(|id| flow.stage(id))
            .map_or(crate::flow::check::DEFAULT_VALIDATION_ROUNDS, |s| {
                rounds_of(s, 0)
            });
        Self {
            flow,
            place,
            status,
            fails: 0,
            rounds,
            skips: &[],
        }
    }

    fn index(&self) -> Option<usize> {
        self.place.stage().and_then(|id| self.flow.position(id))
    }

    fn stage(&self) -> Option<&'f Stage> {
        self.index().map(|i| &self.flow.stages[i])
    }
}

/// Whether the ticket skips `stage` (it can, and chose to).
fn skips(stage: &Stage, skips: &[String]) -> bool {
    stage.can_skip() && !stage.is_delivery() && skips.contains(&stage.id)
}

/// The ticket skips a human check of the flow: its Delivery is then in click mode (d0108: "a ticket that skipped a
/// human check this way always reaches Delivery in click mode").
fn skips_a_human_check(flow: &Flow, skipping: &[String]) -> bool {
    flow.stages
        .iter()
        .any(|s| s.is_human_check() && skips(s, skipping))
}

/// What a ticket's agent-check counters do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Counters {
    Keep,
    /// The current check counts the round that just ended: a fail adds to its fails in a row; a pass ends them (the
    /// default flow shows it as "Validated · round N").
    Count,
    /// The current check's counter restarts at 0 (Answer the agent).
    ResetThis,
    /// Every agent check's counter restarts at 0 (a human Fail, d0063's reset after Send back).
    ResetAll,
}

/// Where a move takes the ticket and what comes with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step<'f> {
    pub place: Place,
    pub status: Status,
    /// The stage the ticket is in afterwards, if any.
    pub stage: Option<&'f Stage>,
    /// How the run of the stage the ticket was in ends; `None` when that run goes on.
    pub ended: Option<RunResult>,
    /// Runs that start and end at once: stages walked over because the ticket skips them (or `SubmitByUser`
    /// walked past them), and a Delivery finished as soon as it was entered.
    pub passed_through: Vec<(&'f Stage, RunResult)>,
    pub counters: Counters,
    /// A fresh checker starts (an agent check was entered, or Retry).
    pub start_check: bool,
    /// An Agent stage that ran before takes the ticket back: its session resumes, with the findings or comments.
    pub resume: bool,
    /// A human check's Pass offers these delivery choices (Local merge, Open PR or both); the ticket stays put
    /// until the user picks one.
    pub choose: Option<Delivers>,
    /// The ticket entered an automatic Delivery stage: it delivers with no click.
    pub automatic: bool,
}

impl<'f> Step<'f> {
    fn stay(at: &At<'f, '_>, status: Status) -> Self {
        Self {
            place: at.place.clone(),
            status,
            stage: at.stage(),
            ended: None,
            passed_through: Vec::new(),
            counters: Counters::Keep,
            start_check: false,
            resume: false,
            choose: None,
            automatic: false,
        }
    }

    fn to(at: &At<'f, '_>, place: Place, status: Status, stage: Option<&'f Stage>) -> Self {
        Self {
            place,
            status,
            stage,
            ..Self::stay(at, status)
        }
    }

    fn ended(mut self, result: RunResult) -> Self {
        self.ended = Some(result);
        self
    }

    fn counters(mut self, counters: Counters) -> Self {
        self.counters = counters;
        self
    }
}

/// Why a move doesn't apply where the ticket stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotNow(pub String);

/// The stages after `from` (an index; `None` = before the first stage) up to the first one the ticket runs:
/// those it skips, then the one it enters. `None` when nothing follows (after the Delivery comes Done).
fn walk<'f>(
    flow: &'f Flow,
    skipping: &[String],
    from: Option<usize>,
    also_skip: impl Fn(&Stage) -> bool,
) -> (Vec<&'f Stage>, Option<&'f Stage>) {
    let start = from.map_or(0, |i| i + 1);
    let mut skipped = Vec::new();
    for stage in &flow.stages[start.min(flow.stages.len())..] {
        if skips(stage, skipping) || (also_skip(stage) && !stage.is_delivery()) {
            skipped.push(stage);
        } else {
            return (skipped, Some(stage));
        }
    }
    (skipped, None)
}

/// The nearest earlier Agent stage the ticket runs: where a drag back's nearest stage, Give it another round, Answer
/// the agent, a Delivery's conflict, a failed Merge check and PR comments send it (d0108 smaller rule 1).
pub fn back_to<'f>(flow: &'f Flow, from: &str, skips: &[String]) -> Option<&'f Stage> {
    let index = flow.position(from)?;
    flow.stages[..index]
        .iter()
        .rev()
        .find(|s| s.is_agent_stage() && !self::skips(s, skips))
}

/// Where a Fail at the check `check` sends this ticket (d0131): the check's fails-go-to stage ([`Flow::fail_to_of`]:
/// its `fail_to`, else the nearest Agent stage before it), or, when the ticket skips that stage, the nearest Agent
/// stage before the check that the ticket runs. `None` when the ticket runs no Agent stage before it.
pub fn fail_to<'f>(flow: &'f Flow, check: &str, skips: &[String]) -> Option<&'f Stage> {
    let named = flow.stage(check).and_then(|c| flow.fail_to_of(c));
    match named {
        Some(stage) if !self::skips(stage, skips) => Some(stage),
        _ => back_to(flow, check, skips),
    }
}

/// The first Agent stage the ticket runs: where a start takes it, and the stages it skips before it.
pub fn first_stage<'f>(flow: &'f Flow, skips: &[String]) -> (Vec<&'f Stage>, Option<&'f Stage>) {
    // Ready for Dev waits before the first Agent stage the ticket runs (checks never come before one, STG-02).
    walk(flow, skips, None, |s| !s.is_agent_stage())
}

/// Where a status written by the validation's own bookkeeping belongs, from the stage the ticket is in: a check's
/// status in that check, or (from its Agent stage) the next agent check; an Agent
/// stage's status in that stage, or (from a check) the Agent stage before it. `None` when the stage stays.
pub fn stage_for<'f>(flow: &'f Flow, place: &Place, status: Status) -> Option<&'f Stage> {
    let current = flow.stage(place.stage()?)?;
    let index = flow.position(&current.id)?;
    match status.set() {
        StatusSet::AgentCheck if current.is_agent_stage() => {
            flow.stages[index + 1..].iter().find(|s| s.is_agent_check())
        }
        StatusSet::AgentStage if current.is_agent_check() => back_to(flow, &current.id, &[]),
        _ => None,
    }
}

/// The first human check after `from`: where Move on anyway takes a card.
pub fn next_human_check<'f>(flow: &'f Flow, from: &str) -> Option<&'f Stage> {
    let index = flow.position(from)?;
    flow.stages[index + 1..].iter().find(|s| s.is_human_check())
}

/// The stage whose column a stage shows in ([`Flow::column_owner`]).
fn owner<'f>(flow: &'f Flow, stage: &Stage) -> Option<&'f Stage> {
    flow.position(&stage.id)
        .and_then(|i| flow.column_owner(i))
        .map(|i| &flow.stages[i])
}

/// Entering `stage` from where `at` stands, after walking over `skipped`.
fn enter<'f>(at: &At<'f, '_>, skipped: Vec<&'f Stage>, stage: Option<&'f Stage>) -> Step<'f> {
    let mut step = match stage {
        None => Step::to(at, Place::Done, Status::Merged, None),
        Some(stage) => {
            let mut step = Step::to(
                at,
                Place::stage_of(&stage.id),
                entry_status(stage),
                Some(stage),
            );
            step.start_check = stage.is_agent_check();
            step.automatic = stage
                .delivery
                .is_some_and(|d| d.mode == DeliveryMode::Automatic)
                && !skips_a_human_check(at.flow, at.skips);
            step
        }
    };
    step.passed_through = skipped
        .into_iter()
        .map(|s| (s, RunResult::Skipped))
        .collect();
    step
}

/// Moving on from a stage to `stage`, after walking over `skipped`: an Agent stage is waited for (STG-07, d0108
/// "waiting is automatic before every Agent stage"): the ticket waits before it, as Ready for Dev waits before the
/// first, until the scheduler starts it ([`Move::Begin`]). Any other stage is entered at once.
fn move_on<'f>(at: &At<'f, '_>, skipped: Vec<&'f Stage>, stage: Option<&'f Stage>) -> Step<'f> {
    match stage {
        Some(stage) if stage.is_agent_stage() => {
            let mut step = Step::to(
                at,
                Place::Waiting(Some(stage.id.clone())),
                Status::Queued,
                None,
            );
            step.passed_through = skipped
                .into_iter()
                .map(|s| (s, RunResult::Skipped))
                .collect();
            step
        }
        _ => enter(at, skipped, stage),
    }
}

/// Which Agent stage a move back goes to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Back {
    /// The check's fails-go-to stage ([`fail_to`]): an agent check's Fail and a human check's Fail (d0131).
    FailTo,
    /// The nearest earlier one ([`back_to`]): everything else that goes back (d0108 smaller rule 1).
    Nearest,
}

/// Back to an earlier Agent stage, which resumes its session.
fn back<'f>(at: &At<'f, '_>, from: &Stage, rule: Back) -> Result<Step<'f>, NotNow> {
    let to = match rule {
        Back::FailTo => fail_to(at.flow, &from.id, at.skips),
        Back::Nearest => back_to(at.flow, &from.id, at.skips),
    };
    let to = to.ok_or_else(|| {
        NotNow(format!(
            "{} has no Agent stage before it to send the ticket back to",
            from.name
        ))
    })?;
    let mut step = Step::to(at, Place::stage_of(&to.id), Status::Working, Some(to));
    step.resume = true;
    Ok(step)
}

fn agent_works(status: Status) -> bool {
    matches!(
        status,
        Status::Working
            | Status::WorkingInBackground
            | Status::AsksYou
            | Status::NeedsYou
            | Status::Idle
            | Status::MergeConflictFixing
    )
}

fn checking(status: Status) -> bool {
    matches!(status, Status::Checking | Status::CheckerNeedsYou)
}

/// What `mv` does where `at` stands.
pub fn step<'f>(at: At<'f, '_>, mv: Move) -> Result<Step<'f>, NotNow> {
    let not_now = || {
        Err(NotNow(format!(
            "{mv:?} doesn't apply while the ticket is {}",
            at.status.label()
        )))
    };
    let current = at.stage();
    let kind = current.map(|s| s.kind);
    let is = |k: StageKind| kind == Some(k);
    let agent_check = is(StageKind::Check { by: CheckBy::Agent });
    let human_check = is(StageKind::Check { by: CheckBy::You });
    let forward = |also_skip: &dyn Fn(&Stage) -> bool| {
        let (skipped, next) = walk(at.flow, at.skips, at.index(), also_skip);
        move_on(&at, skipped, next)
    };
    Ok(match mv {
        Move::Start if at.place.is_backlog() => {
            Step::to(&at, Place::Waiting(None), Status::Queued, None)
        }
        Move::Begin if at.place.is_waiting() => {
            let target = match at.place {
                Place::Waiting(Some(id)) => (Vec::new(), at.flow.stage(id)),
                _ => first_stage(at.flow, at.skips),
            };
            match target {
                (skipped, Some(stage)) => enter(&at, skipped, Some(stage)),
                (_, None) => return not_now(),
            }
        }
        Move::NextStage if is(StageKind::Agent) && agent_works(at.status) => {
            forward(&|_| false).ended(RunResult::Done)
        }
        Move::SubmitByUser if is(StageKind::Agent) && agent_works(at.status) => {
            let mut step = forward(&|s: &Stage| s.is_agent_check()).ended(RunResult::Done);
            step.automatic = false;
            step
        }
        Move::Pass if agent_check && checking(at.status) => forward(&|_| false)
            .ended(RunResult::Passed)
            .counters(Counters::Count),
        Move::Fail if agent_check && checking(at.status) => {
            let stage = current.expect("in a check");
            let step = if at.fails + 1 >= at.rounds {
                Step::stay(&at, Status::CheckFailed)
            } else {
                back(&at, stage, Back::FailTo)?
            };
            step.ended(RunResult::Failed).counters(Counters::Count)
        }
        Move::NoResult if agent_check && checking(at.status) => {
            Step::stay(&at, Status::CheckNoResult).ended(RunResult::NoResult)
        }
        Move::CancelCheck if agent_check && checking(at.status) => {
            let mut step =
                back(&at, current.expect("in a check"), Back::Nearest)?.ended(RunResult::Cancelled);
            step.resume = false;
            step
        }
        Move::RetryCheck if agent_check && at.status == Status::CheckNoResult => {
            let mut step = Step::stay(&at, Status::Checking);
            step.start_check = true;
            step
        }
        Move::SkipCheck if agent_check && checking(at.status) => {
            let mut step = forward(&|_| false).ended(RunResult::Skipped);
            step.automatic = false;
            step
        }
        Move::MoveOnAnyway
            if agent_check && matches!(at.status, Status::CheckFailed | Status::CheckNoResult) =>
        {
            let mut step = forward(&|_| false);
            step.automatic = false;
            step
        }
        Move::AnotherRound if agent_check && at.status == Status::CheckFailed => {
            back(&at, current.expect("in a check"), Back::Nearest)?
        }
        Move::AnswerAgent if agent_check && at.status == Status::CheckFailed => {
            back(&at, current.expect("in a check"), Back::Nearest)?.counters(Counters::ResetThis)
        }
        Move::HumanPass if human_check && at.status == Status::WaitingForYou => {
            let index = at.index().expect("in a check");
            match at.flow.stages.get(index + 1) {
                // The Delivery's own click is the Pass's (today's Approve → Local merge / Open PR).
                Some(next) if next.delivery.is_some_and(|d| d.mode == DeliveryMode::Click) => {
                    let mut step = Step::stay(&at, at.status);
                    step.choose = next.delivery.map(|d| d.delivers);
                    step
                }
                _ => forward(&|_| false).ended(RunResult::Passed),
            }
        }
        Move::HumanFail if human_check && at.status == Status::WaitingForYou => {
            back(&at, current.expect("in a check"), Back::FailTo)?
                .ended(RunResult::Failed)
                .counters(Counters::ResetAll)
        }
        // From the human check whose Pass offered the choice, or from the Delivery stage itself.
        Move::Delivered | Move::PrOpened
            if human_check
                && at.status == Status::WaitingForYou
                && at
                    .index()
                    .and_then(|i| at.flow.stages.get(i + 1))
                    .is_some_and(Stage::is_delivery) =>
        {
            let delivery = &at.flow.stages[at.index().expect("in a check") + 1];
            if mv == Move::Delivered {
                let mut step =
                    Step::to(&at, Place::Done, Status::Merged, None).ended(RunResult::Passed);
                step.passed_through = vec![(delivery, RunResult::Done)];
                step
            } else {
                Step::to(
                    &at,
                    Place::stage_of(&delivery.id),
                    Status::PrOpen,
                    Some(delivery),
                )
                .ended(RunResult::Passed)
            }
        }
        Move::Delivered if is(StageKind::Delivery) => {
            Step::to(&at, Place::Done, Status::Merged, None).ended(RunResult::Done)
        }
        Move::PrOpened if is(StageKind::Delivery) => Step::stay(&at, Status::PrOpen),
        Move::PrRework if is(StageKind::Delivery) && at.status == Status::PrOpen => {
            let index = at.index().expect("in a stage");
            let check = at.flow.stages[..index]
                .iter()
                .rev()
                .find(|s| s.is_human_check())
                .ok_or_else(|| NotNow("No human check before the Delivery".to_owned()))?;
            Step::to(
                &at,
                Place::stage_of(&check.id),
                Status::WaitingForYou,
                Some(check),
            )
            .ended(RunResult::Cancelled)
        }
        Move::PrComments if is(StageKind::Delivery) && at.status == Status::PrOpen => {
            back(&at, current.expect("in a stage"), Back::Nearest)?
                .ended(RunResult::Cancelled)
                .counters(Counters::ResetAll)
        }
        Move::Close if !at.place.is_done() => {
            Step::to(&at, Place::Done, Status::ClosedNotMerged, None).ended(RunResult::Cancelled)
        }
        _ => return not_now(),
    })
}

/// Where a drag drops a card, in flow terms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target<'f> {
    Backlog,
    /// Ready for Dev.
    Waiting,
    Stage(&'f Stage),
    Done,
}

impl DropTarget {
    /// The place this board target means in `flow`: Dev is the first Agent stage's column, Ready for Review the
    /// human check before the Delivery, Approved the Delivery (the default flow's Review column and its Approved
    /// section). `None` when the flow has no such stage.
    pub fn in_flow(self, flow: &Flow) -> Option<Target<'_>> {
        let delivery = flow.stages.iter().position(Stage::is_delivery);
        Some(match self {
            Self::Backlog => Target::Backlog,
            Self::ReadyForDev => Target::Waiting,
            Self::Dev => Target::Stage(flow.first_agent_stage()?),
            Self::ReadyForReview => Target::Stage(
                flow.stages[..delivery.unwrap_or(flow.stages.len())]
                    .iter()
                    .rev()
                    .find(|s| s.is_human_check())?,
            ),
            Self::Approved => Target::Stage(&flow.stages[delivery?]),
            Self::Done => Target::Done,
        })
    }
}

impl DropOn {
    /// The place this drop means in `flow`: a board target as [`DropTarget::in_flow`] says, a stage by its id.
    pub fn in_flow<'f>(&self, flow: &'f Flow) -> Option<Target<'f>> {
        match self {
            Self::Board(target) => target.in_flow(flow),
            Self::Stage { stage } => flow.stage(stage).map(Target::Stage),
        }
    }
}

/// [`drag_with`] for a ticket that skips nothing.
pub fn drag(flow: &Flow, place: &Place, status: Status, target: impl Into<DropOn>) -> MoveCheck {
    drag_with(flow, place, status, &[], &target.into())
}

/// The stages a forward drag from `from` (an index; the stage itself when `skip_from`) to `to` walks over: those
/// between them the ticket doesn't already skip by its choices (`skips`).
pub fn skipped_by_drag<'f>(
    flow: &'f Flow,
    skips: &[String],
    from: usize,
    to: usize,
    skip_from: bool,
) -> Vec<&'f Stage> {
    let first = if skip_from { from } else { from + 1 };
    flow.stages[first.min(to)..to]
        .iter()
        .filter(|s| !self::skips(s, skips))
        .collect()
}

/// Where a drag the board carries out by itself takes the ticket (FLW-18): back onto a human check (no dialog; the run
/// it leaves ends cancelled), or forward to `to` past the stages between, which get skipped runs. The run it leaves
/// ends skipped when it was a check (you skipped it), done when it was an Agent stage (as Move to next stage). Forward
/// into an Agent stage waits before it (STG-07); a Delivery entered this way never delivers by itself.
pub fn dragged<'f>(at: At<'f, '_>, to: &'f Stage) -> Step<'f> {
    let flow = at.flow;
    let target = flow.position(&to.id).unwrap_or(flow.stages.len());
    let from = at.place.stage_id().and_then(|id| flow.position(id));
    let back = from.is_some_and(|a| target < a);
    let current = at.stage();
    if back {
        let mut step = Step::to(&at, Place::stage_of(&to.id), entry_status(to), Some(to));
        step.ended = current.map(|_| RunResult::Cancelled);
        return step;
    }
    let between = match from {
        Some(a) => skipped_by_drag(flow, at.skips, a, target, false),
        None => Vec::new(),
    };
    let mut step = move_on(&at, between, Some(to));
    step.ended = current.map(|s| match s.kind {
        StageKind::Agent => RunResult::Done,
        _ => RunResult::Skipped,
    });
    step.start_check = to.is_agent_check();
    step.automatic = false;
    step
}

/// Whether a card can be dragged forward to a later stage of `flow` (d0108 "Other drags"): on every flow, the
/// default one included, where Dev → Review skips Validate and Dev → Approved skips Review too (T-0421's parity
/// question to the user; `false` here would keep today's refusals for those drops).
fn forward_drags(_flow: &Flow) -> bool {
    true
}

/// A drag's verdict (§7.4, FLW-18), by the kinds of the stages it leaves and enters. On top of today's rules (Start,
/// Unqueue, Park, Approve, PR rework, Reopen), any stage takes a drop (d0108 "Dragging a card back", "Other drags"):
/// back to an earlier Agent stage asks for its agent's instructions ([`MoveAction::SendBack`]), back to an agent check
/// asks for a note ([`MoveAction::RunCheckAgain`]), back onto a human check needs nothing ([`MoveAction::MoveBack`]);
/// forward, the very next stage is Move to next stage (or a human check's Pass) and anything further skips the stages
/// between ([`MoveAction::SkipTo`]); a parked card dropped anywhere returns to the stage it left. `skips` are the stages the ticket skips by its choices (they don't count as the "next" stage).
pub fn drag_with(
    flow: &Flow,
    place: &Place,
    status: Status,
    skips: &[String],
    target: &DropOn,
) -> MoveCheck {
    let refused = |reason: &str| MoveCheck::Refused {
        reason: reason.to_owned(),
    };
    let Some(to) = target.in_flow(flow) else {
        return refused(ONLY_AGENT_SUBMITS);
    };
    if status == Status::Merged && to != Target::Done {
        return refused(MERGED_LOCKED);
    }
    let from = match place {
        Place::Backlog => Target::Backlog,
        Place::Waiting(None) => Target::Waiting,
        // Waiting before a later Agent stage shows in that stage's column: dropped there, it stays.
        Place::Waiting(Some(id)) => match flow.stage(id) {
            Some(stage) if to == Target::Stage(stage) => return MoveCheck::Unchanged,
            _ => Target::Waiting,
        },
        Place::Done => Target::Done,
        Place::InStage(id) => match flow.stage(id) {
            Some(stage) => Target::Stage(stage),
            // A stage the flow no longer has (Validation turned off while it validated): where its kind shows.
            None => match status.set() {
                StatusSet::HumanCheck => DropTarget::ReadyForReview,
                StatusSet::Delivery => DropTarget::Approved,
                _ => DropTarget::Dev,
            }
            .in_flow(flow)
            .unwrap_or(Target::Backlog),
        },
    };
    if from == to {
        return MoveCheck::Unchanged;
    }
    let at_work = |s: &Stage| s.is_agent_stage() || s.is_agent_check();
    let index = |s: &Stage| flow.position(&s.id).unwrap_or(usize::MAX);
    let right_after = |a: &Stage, b: &Stage| index(b) == index(a).wrapping_add(1);
    match (from, to) {
        // Dropped on the column it shows in: an agent check folded into its Agent stage's column.
        (Target::Stage(a), Target::Stage(b))
            if at_work(a) && a.column == Column::WithPrevious && owner(flow, a) == Some(b) =>
        {
            MoveCheck::Unchanged
        }
        // The go-ahead; on an Agent stage it waits until it can start (d0066). A parked card goes back to the stage
        // it left wherever it is dropped (d0052, FLW-18).
        (Target::Backlog, Target::Waiting) => MoveCheck::Allowed {
            action: MoveAction::Start,
        },
        (Target::Backlog, Target::Stage(b)) if b.is_agent_stage() || status == Status::Parked => {
            MoveCheck::Allowed {
                action: MoveAction::Start,
            }
        }
        (Target::Waiting, Target::Backlog) => MoveCheck::Allowed {
            action: MoveAction::Unqueue,
        },
        (Target::Waiting, Target::Stage(b)) if at_work(b) => refused(STARTS_BY_ITSELF),
        (Target::Stage(a), Target::Waiting) if at_work(a) => {
            refused("Drag it to Backlog to park it")
        }
        (Target::Stage(a), Target::Backlog) if at_work(a) => MoveCheck::Confirm {
            action: MoveAction::Park,
        },
        (Target::Stage(a), Target::Stage(b))
            if a.is_human_check() && b.is_delivery() && right_after(a, b) =>
        {
            MoveCheck::Choose {
                action: MoveAction::Approve,
            }
        }
        (Target::Stage(a), Target::Stage(b))
            if a.is_delivery() && b.is_human_check() && right_after(b, a) =>
        {
            MoveCheck::Allowed {
                action: MoveAction::PrRework,
            }
        }
        // Back to an earlier stage (d0108 "Dragging a card back").
        (Target::Stage(a), Target::Stage(b)) if index(b) < index(a) => {
            if b.is_agent_stage() {
                MoveCheck::Confirm {
                    action: MoveAction::SendBack,
                }
            } else if b.is_agent_check() {
                MoveCheck::Confirm {
                    action: MoveAction::RunCheckAgain,
                }
            } else {
                MoveCheck::Allowed {
                    action: MoveAction::MoveBack,
                }
            }
        }
        // Forward (d0108 "Other drags"): the very next stage the ticket runs is Move to next stage (a human check's
        // Pass); further, or out of an agent check, the stages between are skipped by you.
        (Target::Stage(a), Target::Stage(b)) if forward_drags(flow) => {
            let skipped = skipped_by_drag(flow, skips, index(a), index(b), a.is_agent_check());
            let action = if skipped.is_empty() {
                MoveAction::NextStage
            } else {
                MoveAction::SkipTo
            };
            MoveCheck::Allowed { action }
        }
        (Target::Done, Target::Backlog) => MoveCheck::Allowed {
            action: MoveAction::Reopen,
        },
        (_, Target::Stage(b)) if b.is_human_check() => refused(ONLY_AGENT_SUBMITS),
        (Target::Done, _) => refused("Drag it to Backlog to reopen it first"),
        (Target::Backlog, Target::Stage(b)) if b.is_agent_check() => refused(QUEUE_FIRST),
        (Target::Stage(a), Target::Waiting | Target::Stage(_))
            if a.is_human_check() && !matches!(to, Target::Stage(b) if b.is_delivery()) =>
        {
            refused("Use Send back to return it to Dev with your comments")
        }
        (_, Target::Waiting) => {
            refused("Drag it back through Ready for Review: Approved tickets return with PR rework")
        }
        (_, Target::Stage(b)) if at_work(b) => {
            refused("Drag it back through Ready for Review: Approved tickets return with PR rework")
        }
        (_, Target::Stage(_)) => refused("Approve a ticket from Ready for Review"),
        (_, Target::Done) => refused("Use Close ticket, Approve or Mark merged to finish a ticket"),
        (_, Target::Backlog) => {
            refused("Only Ready for Dev, Dev and Closed tickets go back to Backlog")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flow::default::{DELIVER, default_flow};

    #[test]
    fn default_stages_of_every_status() {
        for status in Status::ALL {
            let stage = default_stage(*status, None);
            assert_eq!(stage.is_some(), status.set().in_stage(), "{status}");
            assert!(stage.is_none_or(|id| default_flow().stage(id).is_some()));
        }
        assert_eq!(default_stage(Status::PrOpen, None), Some(DELIVER));
        assert_eq!(
            default_stage(Status::WaitingForLimitReset, Some(Status::Checking)),
            Some(VALIDATE)
        );
    }

    #[test]
    fn the_first_stage_and_back() {
        let flow = default_flow();
        assert_eq!(first_stage(&flow, &[]).1.map(|s| s.id.as_str()), Some(DEV));
        assert_eq!(
            back_to(&flow, REVIEW, &[]).map(|s| s.id.as_str()),
            Some(DEV)
        );
        assert_eq!(back_to(&flow, DEV, &[]), None);
        assert_eq!(
            next_human_check(&flow, DEV).map(|s| s.id.as_str()),
            Some(REVIEW)
        );
    }

    /// The test flow: Grill (skippable) · Dev · Validate · Security · QA (its fails go to Grill) · Done.
    fn every_kind() -> Flow {
        let yaml = include_str!("../../tests/flows/every-kind.yaml");
        crate::flow::yaml::read(yaml).unwrap().flow
    }

    /// Where `mv` takes a ticket in `stage` with `status` that skips `skips`.
    fn moved(flow: &Flow, stage: &str, status: Status, skips: &[String], mv: Move) -> String {
        let place = Place::stage_of(stage);
        let at = At {
            skips,
            ..At::new(flow, &place, status)
        };
        let step = step(at, mv).unwrap();
        assert!(step.resume, "{mv:?} resumes the stage it goes back to");
        step.place.stage().unwrap().to_owned()
    }

    #[test]
    fn a_fail_goes_to_the_checks_stage() {
        let flow = every_kind();
        let skip_grill = ["grill".to_owned()];
        // Set and not skipped: QA's fails go to Grill, not the nearest (Dev).
        assert_eq!(
            fail_to(&flow, "qa", &[]).map(|s| s.id.as_str()),
            Some("grill")
        );
        // Set and skipped: the nearest Agent stage before QA that the ticket runs.
        assert_eq!(
            fail_to(&flow, "qa", &skip_grill).map(|s| s.id.as_str()),
            Some("dev")
        );
        // Unset: the nearest.
        assert_eq!(
            fail_to(&flow, "validate", &[]).map(|s| s.id.as_str()),
            Some("dev")
        );
        assert_eq!(fail_to(&flow, "grill", &[]), None);
    }

    #[test]
    fn human_and_agent_fails_alike() {
        let mut flow = every_kind();
        let skip_grill = ["grill".to_owned()];
        let human = |flow: &Flow, skips: &[String]| {
            moved(flow, "qa", Status::WaitingForYou, skips, Move::HumanFail)
        };
        assert_eq!(human(&flow, &[]), "grill");
        assert_eq!(human(&flow, &skip_grill), "dev");
        // A Validate fail in a flow where Validate's target isn't the nearest stage.
        let agent = |flow: &Flow, skips: &[String]| {
            moved(flow, "validate", Status::Checking, skips, Move::Fail)
        };
        assert_eq!(agent(&flow, &[]), "dev", "unset: the nearest");
        flow.stages[2].fail_to = Some("grill".into());
        assert_eq!(agent(&flow, &[]), "grill");
        assert_eq!(agent(&flow, &skip_grill), "dev");
        // The last allowed fail stays in the check, whatever its target.
        let place = Place::stage_of("validate");
        let last = At {
            fails: 2,
            ..At::new(&flow, &place, Status::Checking)
        };
        assert_eq!(step(last, Move::Fail).unwrap().status, Status::CheckFailed);
    }

    #[test]
    fn the_other_ways_back_keep_the_nearest_stage() {
        let mut flow = every_kind();
        flow.stages[2].fail_to = Some("grill".into());
        for mv in [Move::AnotherRound, Move::AnswerAgent] {
            assert_eq!(
                moved(&flow, "validate", Status::CheckFailed, &[], mv),
                "dev"
            );
        }
        let place = Place::stage_of("ship");
        let pr = At::new(&flow, &place, Status::PrOpen);
        assert_eq!(
            step(pr, Move::PrComments).unwrap().place,
            Place::stage_of("dev")
        );
        let place = Place::stage_of("validate");
        let cancel = At::new(&flow, &place, Status::Checking);
        assert_eq!(
            step(cancel, Move::CancelCheck).unwrap().place,
            Place::stage_of("dev")
        );
        // A drag back goes to the stage it was dropped on, whatever the checks' targets.
        let place = Place::stage_of("qa");
        let at = At::new(&flow, &place, Status::WaitingForYou);
        let dev = flow.stage("dev").unwrap();
        assert_eq!(dragged(at, dev).place, Place::stage_of("dev"));
    }

    #[test]
    fn nowhere_to_go_names_the_check() {
        let mut flow = every_kind();
        // Only reachable through a ticket's skips (M46's skippable human check): here Dev turned skippable too.
        flow.stages[1].options = vec![crate::flow::model::StageOption::Skip];
        let skips = ["grill".to_owned(), "dev".to_owned()];
        let place = Place::stage_of("qa");
        let at = At {
            skips: &skips,
            ..At::new(&flow, &place, Status::WaitingForYou)
        };
        assert_eq!(
            step(at, Move::HumanFail).unwrap_err().0,
            "QA has no Agent stage before it to send the ticket back to"
        );
    }
}
