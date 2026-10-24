//! The reconcile table of REC-01 and STG-09 (d0018, d0045, d0108), as a pure function: what a ticket becomes when
//! the service finds the session of the stage it is in alive or gone. The table goes by the **kind of that stage**
//! in the project's flow; the default flow's rows are today's.
//!
//! | The stage the ticket is in | Its session alive | Its session gone (reboot, crash, service restart, the tool exited) |
//! |---|---|---|
//! | Agent stage · working, in background, waiting for limit reset, a `restack` fix | reattach silently | **Interrupted** (inbox item, Resume / Resume all) |
//! | Agent stage · turquoise (Asks you, Needs you, Idle) | reattach silently | **Interrupted, keeping the label and question** |
//! | Agent check · Checking, Needs you · validator, waiting for limit reset (**the checker's** session) | reattach silently | **Interrupted** from Checking: Resume starts a fresh check (d0063) |
//! | Agent check · Asks you (**the checker's** session) | reattach silently | **Interrupted, keeping the label and question**; Resume starts a fresh check |
//! | Human check (Ready for Review), click Delivery (to deliver, PR open) | reattach silently | **not** Interrupted; History notes "session ended (restart)" |
//! | Automatic PR Delivery · to deliver (its delivery session) | reattach silently | **Interrupted in the Agent stage that runs it** |
//! | anything else | — | unchanged |
//!
//! The check and Auto-merge stops wait for the user's click and need no session: a restart leaves them as they are.
//!
//! Run and Shell are sessions, not ticket states: a restart ends them as "stopped by restart"
//! ([`crate::session::END_STOPPED_BY_RESTART`]). **Nothing resumes automatically.**

use crate::domain::{Place, Status, StatusSet};
use crate::flow::engine::back_to;
use crate::flow::{Delivers, DeliveryMode, Flow, Stage, StageKind};

/// What reconcile does with a ticket.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Nothing to do: the session is alive (reattach silently), or the state doesn't depend on a session.
    Unchanged,
    /// Interrupted, with an inbox item and Resume. `keep_label`: a needs-you ticket keeps its label and question.
    Interrupt { keep_label: bool },
    /// A human check or a click Delivery: not Interrupted, no inbox item, not in Resume all; History notes that
    /// the session ended. Only a Fail (Send back) resumes it.
    NoteSessionEnded,
    /// The checker is gone: Interrupted from Checking; Resume starts a fresh check, the round doesn't count, the
    /// old checker conversation is never resumed (REC-01, VAL-07).
    InterruptValidation,
    /// The checker asked the user and is gone: Interrupted, keeping its label and question (d0045, d0119); the
    /// round ends interrupted like [`Self::InterruptValidation`]'s, and Resume starts a fresh check.
    InterruptAsking,
    /// An automatic Delivery's session is gone: Interrupted in the Agent stage `stage` that runs it (STG-09).
    InterruptIn { stage: String },
}

/// The kind of the stage a ticket is in, as recovery and Resume need it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StageAt {
    Agent,
    AgentCheck,
    HumanCheck,
    /// `runs_agent`: an automatic PR Delivery, whose session pushes and opens the PR with no click.
    Delivery {
        runs_agent: bool,
    },
}

impl StageAt {
    pub fn of(stage: &Stage) -> Self {
        match stage.kind {
            StageKind::Agent => Self::Agent,
            StageKind::Check { .. } if stage.is_agent_check() => Self::AgentCheck,
            StageKind::Check { .. } => Self::HumanCheck,
            StageKind::Delivery => Self::Delivery {
                runs_agent: stage.delivery.is_some_and(|d| {
                    d.mode == DeliveryMode::Automatic && d.delivers == Delivers::Pr
                }),
            },
        }
    }

    /// The stage a ticket in `place` with `status` is in: the flow's, or (a stage the flow doesn't have, a row
    /// from another flow) what its status's set says, a check's waits included (`interrupted_from`). `None`
    /// outside a stage.
    pub fn find(
        flow: &Flow,
        place: &Place,
        status: Status,
        interrupted_from: Option<Status>,
    ) -> Option<Self> {
        let id = place.stage()?;
        if let Some(stage) = flow.stage(id) {
            return Some(Self::of(stage));
        }
        Some(match status.set() {
            StatusSet::AgentCheck => Self::AgentCheck,
            StatusSet::AgentStage
                if interrupted_from.is_some_and(|s| s.set() == StatusSet::AgentCheck) =>
            {
                Self::AgentCheck
            }
            StatusSet::AgentStage => Self::Agent,
            StatusSet::HumanCheck => Self::HumanCheck,
            StatusSet::Delivery => Self::Delivery { runs_agent: false },
            StatusSet::Backlog | StatusSet::Waiting | StatusSet::Done => return None,
        })
    }

    /// The session reconcile compares with: the checker's in an agent check, else the ticket agent's.
    pub fn checker_session(self) -> bool {
        self == Self::AgentCheck
    }
}

/// The statuses whose agent is at work: their session ending interrupts them.
pub const AT_WORK: &[Status] = &[
    Status::Working,
    Status::WorkingInBackground,
    Status::WaitingForLimitReset,
    Status::MergeConflictFixing,
    Status::AsksYou,
    Status::NeedsYou,
    Status::Idle,
];

/// The statuses of an agent check whose checker is at work (or waits for the plan limit).
pub const CHECKING: &[Status] = &[
    Status::Checking,
    Status::CheckerNeedsYou,
    Status::WaitingForLimitReset,
];

/// The statuses reconcile looks at: every status whose stage's session can be gone.
pub fn watched() -> impl Iterator<Item = Status> {
    Status::ALL
        .iter()
        .copied()
        .filter(|s| AT_WORK.contains(s) || CHECKING.contains(s) || is_waiting_on_user(*s))
}

/// A human check's or a Delivery's wait for the user's click.
fn is_waiting_on_user(status: Status) -> bool {
    matches!(
        status,
        Status::WaitingForYou | Status::ToDeliver | Status::PrOpen
    )
}

/// STG-09's table: a ticket in `place` of `flow` with status `before`, whose stage's session (the checker's in an
/// agent check, [`StageAt::checker_session`]) is alive or gone.
pub fn reconcile(
    flow: &Flow,
    place: &Place,
    before: Status,
    interrupted_from: Option<Status>,
    session_alive: bool,
) -> Outcome {
    if session_alive {
        return Outcome::Unchanged;
    }
    let Some(at) = StageAt::find(flow, place, before, interrupted_from) else {
        return Outcome::Unchanged;
    };
    match at {
        StageAt::Agent if before.is_agent_waiting() => Outcome::Interrupt { keep_label: true },
        StageAt::Agent if AT_WORK.contains(&before) => Outcome::Interrupt { keep_label: false },
        StageAt::AgentCheck if CHECKING.contains(&before) => Outcome::InterruptValidation,
        StageAt::AgentCheck if before == Status::AsksYou => Outcome::InterruptAsking,
        StageAt::HumanCheck if before == Status::WaitingForYou => Outcome::NoteSessionEnded,
        StageAt::Delivery { runs_agent: true } if before == Status::ToDeliver => {
            match place
                .stage()
                .and_then(|id| back_to(flow, id, &[]))
                .map(|s| s.id.clone())
            {
                Some(stage) => Outcome::InterruptIn { stage },
                None => Outcome::Unchanged,
            }
        }
        StageAt::Delivery { .. } if matches!(before, Status::ToDeliver | Status::PrOpen) => {
            Outcome::NoteSessionEnded
        }
        _ => Outcome::Unchanged,
    }
}

/// What Resume does with an Interrupted ticket (STG-09, d0108: "Resume reopens the session of the stage the card
/// is in").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResumeAs {
    /// A fresh check of the agent check it is in; the round doesn't count (VAL-07).
    FreshCheck,
    /// The session of the Agent stage it is in, continued.
    Reopen,
}

/// What Resume does with an Interrupted ticket in `place`, interrupted from `interrupted_from`: a fresh check in
/// an agent check (or a row that says it was checking, whatever stage it names).
pub fn resume_as(flow: &Flow, place: &Place, interrupted_from: Option<Status>) -> ResumeAs {
    let checking = interrupted_from.is_some_and(|s| s.set() == StatusSet::AgentCheck);
    match StageAt::find(flow, place, Status::Interrupted, interrupted_from) {
        Some(StageAt::AgentCheck) => ResumeAs::FreshCheck,
        _ if checking => ResumeAs::FreshCheck,
        _ => ResumeAs::Reopen,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flow::default::{DELIVER, DEV, REVIEW, VALIDATE};
    use crate::flow::default_flow;

    fn in_stage(status: Status) -> Place {
        Place::of(status, None, None)
    }

    fn default(status: Status, alive: bool) -> Outcome {
        reconcile(&default_flow(), &in_stage(status), status, None, alive)
    }

    #[test]
    fn working_background_and_limit_wait_become_interrupted() {
        for status in [
            Status::Working,
            Status::WorkingInBackground,
            Status::WaitingForLimitReset,
        ] {
            assert_eq!(
                default(status, false),
                Outcome::Interrupt { keep_label: false },
                "{status}"
            );
        }
    }

    #[test]
    fn needs_you_becomes_interrupted_and_keeps_its_label() {
        for status in Status::AGENT_WAITS {
            assert_eq!(
                default(*status, false),
                Outcome::Interrupt { keep_label: true },
                "{status}"
            );
        }
    }

    #[test]
    fn review_and_approved_only_note_the_end() {
        assert_eq!(
            default(Status::WaitingForYou, false),
            Outcome::NoteSessionEnded
        );
        assert_eq!(default(Status::PrOpen, false), Outcome::NoteSessionEnded);
    }

    #[test]
    fn a_live_session_is_reattached_silently() {
        for status in Status::ALL {
            assert_eq!(default(*status, true), Outcome::Unchanged, "{status}");
        }
    }

    #[test]
    fn everything_else_is_left_alone() {
        for status in [
            Status::Backlog,
            Status::Parked,
            Status::Queued,
            Status::WaitingForSlot,
            Status::Held,
            Status::ParentClosed,
            Status::StartInterrupted,
            Status::Interrupted,
            Status::ResumeFailed,
            Status::Merged,
            Status::ClosedNotMerged,
        ] {
            assert_eq!(default(status, false), Outcome::Unchanged, "{status}");
        }
    }

    /// T-0188 (REC-01, d0063): the validator gone → Interrupted from Validating; alive → nothing. The M12 stops
    /// wait for the user with or without one.
    #[test]
    fn the_m12_rows() {
        for status in [Status::Checking, Status::CheckerNeedsYou] {
            assert_eq!(default(status, false), Outcome::InterruptValidation);
            assert_eq!(default(status, true), Outcome::Unchanged);
        }
        // A checker waiting for the plan limit is the check's (in Validate).
        let waiting = Place::stage_of(VALIDATE);
        assert_eq!(
            reconcile(
                &default_flow(),
                &waiting,
                Status::WaitingForLimitReset,
                Some(Status::Checking),
                false
            ),
            Outcome::InterruptValidation
        );
        for status in [
            Status::CheckFailed,
            Status::CheckNoResult,
            Status::DeliveryStopped,
        ] {
            assert_eq!(default(status, false), Outcome::Unchanged, "{status}");
        }
        assert_eq!(
            default(Status::MergeConflictFixing, false),
            Outcome::Interrupt { keep_label: false }
        );
        // Its agent asked the user during the check: Interrupted, keeping the question (T-0490).
        let asking = |alive| {
            reconcile(
                &default_flow(),
                &Place::stage_of(VALIDATE),
                Status::AsksYou,
                None,
                alive,
            )
        };
        assert_eq!(asking(false), Outcome::InterruptAsking);
        assert_eq!(asking(true), Outcome::Unchanged);
    }

    /// The default flow's stages are the ones today's statuses lived in.
    #[test]
    fn the_default_flows_stage_kinds() {
        let flow = default_flow();
        for (id, at) in [
            (DEV, StageAt::Agent),
            (VALIDATE, StageAt::AgentCheck),
            (REVIEW, StageAt::HumanCheck),
            (DELIVER, StageAt::Delivery { runs_agent: false }),
        ] {
            assert_eq!(StageAt::of(flow.stage(id).unwrap()), at, "{id}");
        }
        assert_eq!(
            resume_as(&flow, &Place::stage_of(VALIDATE), Some(Status::Checking)),
            ResumeAs::FreshCheck
        );
        assert_eq!(
            resume_as(&flow, &Place::stage_of(DEV), Some(Status::Idle)),
            ResumeAs::Reopen
        );
        assert_eq!(
            resume_as(&flow, &Place::stage_of(DEV), None),
            ResumeAs::Reopen
        );
    }

    fn every_kind() -> Flow {
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/flows/every-kind.yaml");
        crate::flow::yaml::read(&std::fs::read_to_string(path).unwrap())
            .unwrap()
            .flow
    }

    /// STG-09 on the test flow (grill → dev → validate → security → qa → ship): the stage's kind decides.
    #[test]
    fn the_test_flows_rows() {
        let flow = every_kind();
        let at = |stage: &str, status: Status, from: Option<Status>| {
            reconcile(&flow, &Place::stage_of(stage), status, from, false)
        };
        // Grill working → Interrupted in Grill; Grill asking keeps its label.
        assert_eq!(
            at("grill", Status::Working, None),
            Outcome::Interrupt { keep_label: false }
        );
        assert_eq!(
            at("grill", Status::AsksYou, None),
            Outcome::Interrupt { keep_label: true }
        );
        // Security checking (or waiting for the plan limit) → a fresh Security on Resume.
        for status in [Status::Checking, Status::CheckerNeedsYou] {
            assert_eq!(at("security", status, None), Outcome::InterruptValidation);
        }
        assert_eq!(
            at(
                "security",
                Status::WaitingForLimitReset,
                Some(Status::Checking)
            ),
            Outcome::InterruptValidation
        );
        assert_eq!(
            at("security", Status::AsksYou, None),
            Outcome::InterruptAsking
        );
        for status in [Status::CheckFailed, Status::CheckNoResult] {
            assert_eq!(at("security", status, None), Outcome::Unchanged);
        }
        // QA (a check by you) → a note only.
        assert_eq!(
            at("qa", Status::WaitingForYou, None),
            Outcome::NoteSessionEnded
        );
        // The automatic PR Delivery's session → Interrupted in the Agent stage that runs it; an open PR waits.
        assert_eq!(
            at("ship", Status::ToDeliver, None),
            Outcome::InterruptIn {
                stage: "dev".into()
            }
        );
        assert_eq!(at("ship", Status::PrOpen, None), Outcome::NoteSessionEnded);
        // Alive: nothing, whatever the stage.
        assert_eq!(
            reconcile(
                &flow,
                &Place::stage_of("security"),
                Status::Checking,
                None,
                true
            ),
            Outcome::Unchanged
        );

        assert_eq!(
            resume_as(&flow, &Place::stage_of("security"), Some(Status::Checking)),
            ResumeAs::FreshCheck
        );
        assert_eq!(
            resume_as(&flow, &Place::stage_of("grill"), None),
            ResumeAs::Reopen
        );
        assert_eq!(
            resume_as(&flow, &Place::stage_of("grill"), Some(Status::AsksYou)),
            ResumeAs::Reopen
        );
    }

    /// An epic at its stop (Review on the default flow, an implicit "Review epic" on a flow without a human check)
    /// is in a human check: a note only.
    #[test]
    fn an_epic_at_its_stop_gets_a_note() {
        use crate::board::epic_flight::{REVIEW_EPIC, epic_flow};
        let mut no_qa = every_kind();
        no_qa.stages.retain(|s| s.id != "qa");
        for (flow, stop) in [(default_flow(), REVIEW), (no_qa, REVIEW_EPIC)] {
            let flow = epic_flow(&flow);
            assert_eq!(
                reconcile(
                    &flow,
                    &Place::stage_of(stop),
                    Status::WaitingForYou,
                    None,
                    false
                ),
                Outcome::NoteSessionEnded,
                "{stop}"
            );
        }
    }

    /// A stage the flow doesn't have (a row from another flow) goes by its status's set.
    #[test]
    fn a_stage_the_flow_doesnt_have() {
        let flow = default_flow();
        let at = |status: Status, from: Option<Status>| {
            StageAt::find(&flow, &Place::stage_of("grill"), status, from)
        };
        assert_eq!(at(Status::Working, None), Some(StageAt::Agent));
        assert_eq!(at(Status::Checking, None), Some(StageAt::AgentCheck));
        assert_eq!(
            at(Status::WaitingForLimitReset, Some(Status::Checking)),
            Some(StageAt::AgentCheck)
        );
        assert_eq!(at(Status::WaitingForYou, None), Some(StageAt::HumanCheck));
        assert_eq!(
            at(Status::PrOpen, None),
            Some(StageAt::Delivery { runs_agent: false })
        );
        assert_eq!(
            StageAt::find(&flow, &Place::Waiting(None), Status::Queued, None),
            None
        );
    }
}
