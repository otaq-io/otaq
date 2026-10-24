//! Kinds, places and statuses (SDD §7.2, STG-04; d0004, d0039, d0108).
//!
//! A ticket is always in one [`Place`] (Backlog, waiting before an Agent stage, a stage of its flow, Done) and has
//! one [`Status`] from the set its place allows. There is no column: the board computes its columns from the flow
//! (STG-08). Priorities are their own type ([`super::Priority`], d0065). There are no stacking states such as
//! Paused or "needs restack" (d0049).

use super::stored_enum;

stored_enum! {
    /// The two ticket types (TKT-02). An epic is a container and a delivery unit (d0039, d0059).
    #[derive(Default)]
    pub enum Kind ("ticket kind") {
        #[default]
        Ticket = "ticket",
        Epic = "epic",
    }
}

stored_enum! {
    /// Every status of STG-04: what a ticket is doing in its [`Place`]. Each status belongs to one
    /// [`StatusSet`], the set a place (and its stage's kind) allows. Flags (pinned, sent back, …) are separate
    /// ([`super::Flags`]). The stored forms are today's, so stored rows, History and the CLI's JSON read the same
    /// (T-0400): only the Rust names that said "validation" or "review" for what any check does were renamed.
    pub enum Status ("status") {
        // Backlog
        /// Not started.
        Backlog = "backlog",
        /// Parked · has work: taken out of its Agent stage; branch, worktree and the agent's conversation kept
        /// (d0052).
        Parked = "parked",

        // Waiting before an Agent stage (Ready for Dev before the first)
        /// Queued, waiting to start: for its dependencies to merge, or for the scheduler (d0049).
        Queued = "queued",
        /// Queued while the optional agent limit is full (d0016).
        WaitingForSlot = "waiting_for_slot",
        /// Held: the plan limit is active, or the base has diverged (d0020, d0059).
        Held = "held",
        /// A parent was closed without merging (d0033, d0044).
        ParentClosed = "parent_closed",
        /// The start sequence stopped halfway; Retry start (d0018).
        StartInterrupted = "start_interrupted",

        // An Agent stage
        /// The agent is working.
        Working = "working",
        /// The turn ended but background tasks or wakeups are pending (neutral, d0042).
        WorkingInBackground = "working_in_background",
        /// Needs-you family: the agent asked the user a question (d0042).
        AsksYou = "asks_you",
        /// Needs-you family: the agent waits on a permission prompt (d0042).
        NeedsYou = "needs_you",
        /// Needs-you family: the agent stopped and waits for input (d0042).
        Idle = "idle",
        /// Stopped by the plan limit (neutral, no needs-you item, d0046).
        WaitingForLimitReset = "waiting_for_limit_reset",
        /// The session died; Resume (d0018, d0045).
        Interrupted = "interrupted",
        /// Resuming failed; Retry or Start from scratch (d0018).
        ResumeFailed = "resume_failed",
        /// A Delivery's rebase conflicted; the agent is fixing it with `restack` (working, d0064, d0108, FLW-17).
        MergeConflictFixing = "merge_conflict_fixing",
        /// Turquoise: the Delivery stopped (keeps conflicting, base diverged, checkout dirty; d0064, FLW-17). It stays
        /// in the Agent stage whose session would fix it, as today's card does in Dev (d0108 second gap review rule
        /// 12).
        DeliveryStopped = "auto_merge_stopped",

        // An agent check
        /// "Validating…": a separate checker session checks the work (neutral, one slot, d0063).
        Checking = "validating",
        /// Turquoise: the checker waits on a permission prompt (d0063).
        CheckerNeedsYou = "validator_needs_you",
        /// Turquoise: the check's last allowed round failed; Move on anyway / another round / answer (d0063).
        CheckFailed = "validation_failed",
        /// Turquoise: the checker ended without a verdict, or its verdict was discarded (d0063).
        CheckNoResult = "validation_no_result",

        // A human check
        /// Waiting for the user's Pass or Fail (today's Ready for Review: submitted by the agent or the user;
        /// d0004, d0052).
        WaitingForYou = "ready_for_review",

        // A Delivery stage
        /// Waiting for the user's Local merge or Open PR click, or for an automatic delivery (d0108).
        ToDeliver = "to_deliver",
        /// A PR is open; waits for Mark merged / Mark closed (d0015, d0017).
        PrOpen = "pr_open",

        // Done
        /// Merged locally or marked merged.
        Merged = "merged",
        /// Closed without merging; reopenable (d0033).
        ClosedNotMerged = "closed_not_merged",
    }
}

/// The set of statuses a place allows (STG-04): the place, and for a stage its kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StatusSet {
    Backlog,
    /// Waiting before an Agent stage.
    Waiting,
    AgentStage,
    AgentCheck,
    HumanCheck,
    Delivery,
    Done,
}

impl StatusSet {
    /// The board's six groups of cards, left to right, as the default flow's board shows them until its columns
    /// come from the flow (STG-08): an agent check's cards are with its Agent stage's (Dev: Working · Validating).
    pub const BOARD: [StatusSet; 6] = [
        Self::Backlog,
        Self::Waiting,
        Self::AgentStage,
        Self::HumanCheck,
        Self::Delivery,
        Self::Done,
    ];

    /// The board group this set's cards are in ([`Self::BOARD`]).
    pub fn board_group(self) -> Self {
        match self {
            Self::AgentCheck => Self::AgentStage,
            other => other,
        }
    }

    /// A set of a stage, not Backlog, waiting or Done.
    pub fn in_stage(self) -> bool {
        !matches!(self, Self::Backlog | Self::Waiting | Self::Done)
    }

    /// An agent works there: an Agent stage or an agent check (today's Dev column).
    pub fn at_work(self) -> bool {
        matches!(self, Self::AgentStage | Self::AgentCheck)
    }
}

/// Where a ticket is (STG-04, d0108): Backlog, waiting before an Agent stage, in a stage of its project's flow, or
/// Done. The stage is the flow stage's permanent id; a ticket stores it (`tickets.stage_id`), and nothing stores a
/// column: the board computes its columns from the flow (STG-08).
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(tag = "place", content = "stage", rename_all = "snake_case")]
pub enum Place {
    Backlog,
    /// Waiting before an Agent stage: `None` before the first one (Ready for Dev), else that stage's id.
    Waiting(Option<String>),
    InStage(String),
    Done,
}

impl Place {
    /// In the stage `id`.
    pub fn stage_of(id: &str) -> Self {
        Self::InStage(id.to_owned())
    }

    /// The place a stored status and stage id say: the status's set decides Backlog, waiting and Done; a status of
    /// a stage is in its stored stage, or (a row written without one) in the default flow's stage for it.
    pub fn of(status: Status, stage_id: Option<&str>, interrupted_from: Option<Status>) -> Self {
        match status.set() {
            StatusSet::Backlog => Self::Backlog,
            StatusSet::Waiting => Self::Waiting(stage_id.map(str::to_owned)),
            StatusSet::Done => Self::Done,
            _ => Self::InStage(
                stage_id
                    .or_else(|| crate::flow::engine::default_stage(status, interrupted_from))
                    .unwrap_or(crate::flow::default::DEV)
                    .to_owned(),
            ),
        }
    }

    /// The stage the ticket is in.
    pub fn stage(&self) -> Option<&str> {
        match self {
            Self::InStage(id) => Some(id),
            _ => None,
        }
    }

    /// What `tickets.stage_id` holds: the stage it is in, or the Agent stage it waits before (none for Ready for
    /// Dev, Backlog and Done).
    pub fn stage_id(&self) -> Option<&str> {
        match self {
            Self::InStage(id) | Self::Waiting(Some(id)) => Some(id),
            _ => None,
        }
    }

    pub fn is_backlog(&self) -> bool {
        *self == Self::Backlog
    }

    /// Waiting before an Agent stage, Ready for Dev included.
    pub fn is_waiting(&self) -> bool {
        matches!(self, Self::Waiting(_))
    }

    /// Waiting before the first Agent stage: the Ready for Dev column (d0066).
    pub fn is_ready_for_dev(&self) -> bool {
        *self == Self::Waiting(None)
    }

    pub fn is_done(&self) -> bool {
        *self == Self::Done
    }

    /// In the stage `id`.
    pub fn is_in(&self, id: &str) -> bool {
        self.stage() == Some(id)
    }
}

impl Status {
    /// The needs-you family: the agent waits for the user (d0042), and from M12 the check and Auto-merge stops
    /// (d0063, d0064).
    pub const NEEDS_YOU_FAMILY: &'static [Status] = &[
        Self::AsksYou,
        Self::NeedsYou,
        Self::Idle,
        Self::CheckerNeedsYou,
        Self::CheckFailed,
        Self::CheckNoResult,
        Self::DeliveryStopped,
    ];

    /// The set this status belongs to (STG-04). Interrupted and Waiting for limit reset also stand for a check
    /// whose session ended or waits (`interrupted_from` says so); their set is the Agent stage's.
    pub fn set(self) -> StatusSet {
        match self {
            Self::Backlog | Self::Parked => StatusSet::Backlog,
            Self::Queued
            | Self::WaitingForSlot
            | Self::Held
            | Self::ParentClosed
            | Self::StartInterrupted => StatusSet::Waiting,
            Self::Working
            | Self::WorkingInBackground
            | Self::AsksYou
            | Self::NeedsYou
            | Self::Idle
            | Self::WaitingForLimitReset
            | Self::Interrupted
            | Self::ResumeFailed
            | Self::MergeConflictFixing
            | Self::DeliveryStopped => StatusSet::AgentStage,
            Self::Checking | Self::CheckerNeedsYou | Self::CheckFailed | Self::CheckNoResult => {
                StatusSet::AgentCheck
            }
            Self::WaitingForYou => StatusSet::HumanCheck,
            Self::ToDeliver | Self::PrOpen => StatusSet::Delivery,
            Self::Merged | Self::ClosedNotMerged => StatusSet::Done,
        }
    }

    /// Whether this is one of the waiting statuses before an Agent stage (Ready for Dev's, d0066).
    pub fn is_queued(self) -> bool {
        self.set() == StatusSet::Waiting
    }

    /// An agent works on the ticket: an Agent stage's or an agent check's status (today's Dev column).
    pub fn at_work(self) -> bool {
        self.set().at_work()
    }

    pub fn is_needs_you_family(self) -> bool {
        Self::NEEDS_YOU_FAMILY.contains(&self)
    }

    /// The agent's own wait (Asks you, Needs you, Idle): its input, a command or an answer clears it (d0042).
    /// The check and Auto-merge stops are turquoise too, but only their own actions clear them.
    pub const AGENT_WAITS: &'static [Status] = &[Self::AsksYou, Self::NeedsYou, Self::Idle];

    pub fn is_agent_waiting(self) -> bool {
        Self::AGENT_WAITS.contains(&self)
    }

    /// The label shown on the card's status pill.
    pub fn label(self) -> &'static str {
        match self {
            Self::Backlog => "Backlog",
            Self::Parked => "Parked · has work",
            Self::Queued => "Ready for Dev",
            Self::WaitingForSlot => "Waiting for a slot",
            Self::Held => "Held",
            Self::ParentClosed => "Parent closed",
            Self::StartInterrupted => "Start failed",
            Self::Working => "Working",
            Self::WorkingInBackground => "Working in background",
            Self::AsksYou => "Asks you",
            Self::NeedsYou => "Needs you",
            Self::Idle => "Idle",
            Self::WaitingForLimitReset => "Waiting for limit reset",
            Self::Interrupted => "Interrupted",
            Self::ResumeFailed => "Resume failed",
            Self::Checking => super::validation::VALIDATING,
            Self::CheckerNeedsYou => super::validation::VALIDATOR_NEEDS_YOU,
            // The card adds the count: "Validation failed 3 times" (validation::failed_times).
            Self::CheckFailed => "Validation failed",
            Self::CheckNoResult => super::validation::NO_RESULT,
            Self::MergeConflictFixing => super::validation::MERGE_CONFLICT_FIXING,
            // The card names the reason ("Keeps conflicting with `main`", base diverged, checkout dirty).
            Self::DeliveryStopped => "Delivery stopped",
            Self::WaitingForYou => "Ready for Review",
            Self::ToDeliver => "To deliver",
            Self::PrOpen => "PR open",
            Self::Merged => "Merged",
            Self::ClosedNotMerged => "Closed · not merged",
        }
    }
}

stored_enum! {
    /// The steps of the start sequence, in order (SES-01). A start that stops names the step it stopped at.
    pub enum StartStep ("start step") {
        /// Resolve the base branch (the epic's branch for an epic child).
        Base = "base",
        /// Fetch and fast-forward the base; hold if it diverged (d0059).
        SyncBase = "sync_base",
        /// Branch, worktree and base_sha.
        Worktree = "worktree",
        EnvFiles = "env_files",
        /// The project's Create command makes the ticket's own resources (M19-d, d0087).
        Resources = "resources",
        Setup = "setup",
        Ports = "ports",
        /// Start the agent session.
        Launch = "launch",
        /// Not a step: the tool refused the Developer's model on its first turn (AGT-05, d0088). The ticket counts
        /// as not started (Start failed) until Retry start or a new Developer row.
        Model = "model",
        /// Not a step: the Developer's tool is at its limit; only that tool's agents hold (SCH-04 `[M23]`, d0086).
        /// Set with the ticket's `agent_holds` row (T-0357), which says which tool.
        ToolLimit = "tool_limit",
        /// Not a step: the Developer's tool isn't available (not installed, logged out, hidden, update needed;
        /// d0086).
        ToolUnavailable = "tool_unavailable",
        /// Not a step: the Developer's model is no longer in its tool's list; nothing started (§7.2, d0088).
        ModelUnavailable = "model_unavailable",
    }
}

impl StartStep {
    /// How the step reads on a card: "setup command failed".
    pub fn failure_label(self) -> &'static str {
        match self {
            Self::Base => "base branch not found",
            Self::SyncBase => "updating the base failed",
            Self::Worktree => "creating the worktree failed",
            Self::EnvFiles => "copying env files failed",
            Self::Resources => "creating ticket resources failed",
            Self::Setup => "setup command failed",
            Self::Ports => "no free ports",
            Self::Launch => "starting the agent failed",
            Self::Model => "couldn't run the model",
            Self::ToolLimit => "the tool is at its limit",
            Self::ToolUnavailable => "the tool isn't available",
            Self::ModelUnavailable => "Model unavailable",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip<T>(all: &[T])
    where
        T: Copy + PartialEq + std::fmt::Debug + std::str::FromStr + serde::Serialize,
        T: serde::de::DeserializeOwned,
        T: std::fmt::Display,
        <T as std::str::FromStr>::Err: std::fmt::Debug,
    {
        let mut seen = std::collections::HashSet::new();
        for value in all {
            let stored = value.to_string();
            assert!(seen.insert(stored.clone()), "{stored} used twice");
            assert_eq!(stored.parse::<T>().unwrap(), *value);
            let json = serde_json::to_string(value).unwrap();
            assert_eq!(json, format!("\"{stored}\""));
            assert_eq!(serde_json::from_str::<T>(&json).unwrap(), *value);
        }
    }

    #[test]
    fn every_enum_round_trips_through_its_stored_form() {
        round_trip(Kind::ALL);
        round_trip(Status::ALL);
        assert!("paused".parse::<Status>().is_err());
        assert!(
            "epic_in_progress".parse::<Status>().is_err(),
            "never written"
        );
    }

    #[test]
    fn two_kinds() {
        assert_eq!(Kind::ALL, &[Kind::Ticket, Kind::Epic]);
    }

    /// STG-04: every set has a status; today's fourteen Dev statuses are the Agent stage's and the agent check's.
    #[test]
    fn every_status_has_a_set_and_every_set_a_status() {
        use StatusSet as S;
        let sets = [
            S::Backlog,
            S::Waiting,
            S::AgentStage,
            S::AgentCheck,
            S::HumanCheck,
            S::Delivery,
            S::Done,
        ];
        for set in sets {
            assert!(Status::ALL.iter().any(|s| s.set() == set), "{set:?}");
        }
        assert_eq!(Status::ALL.iter().filter(|s| s.at_work()).count(), 14);
        assert_eq!(
            Status::ALL
                .iter()
                .filter(|s| s.set() == S::AgentCheck)
                .map(|s| s.label())
                .collect::<Vec<_>>(),
            [
                "Validating…",
                "Needs you · validator",
                "Validation failed",
                "Validation ended without a result"
            ]
        );
        for status in Status::ALL {
            assert_eq!(status.is_queued(), status.set() == S::Waiting, "{status}");
        }
    }

    /// The place a stored row reads as: the set decides Backlog, waiting and Done; a stage status keeps its stage,
    /// and a row without one falls back to the default flow's stage for it.
    #[test]
    fn places_from_stored_rows() {
        use crate::flow::default::{DELIVER, DEV, REVIEW, VALIDATE};
        assert_eq!(Place::of(Status::Parked, Some(DEV), None), Place::Backlog);
        assert_eq!(Place::of(Status::Queued, None, None), Place::Waiting(None));
        assert!(Place::of(Status::Held, None, None).is_ready_for_dev());
        assert_eq!(
            Place::of(Status::Queued, Some(DEV), None),
            Place::Waiting(Some(DEV.into()))
        );
        assert_eq!(Place::of(Status::Merged, None, None), Place::Done);
        assert_eq!(
            Place::of(Status::Checking, Some("security"), None),
            Place::stage_of("security")
        );
        for (status, from, stage) in [
            (Status::Working, None, DEV),
            (Status::DeliveryStopped, None, DEV),
            (Status::Checking, None, VALIDATE),
            (Status::Interrupted, Some(Status::Checking), VALIDATE),
            (Status::Interrupted, Some(Status::Idle), DEV),
            (Status::WaitingForYou, None, REVIEW),
            (Status::PrOpen, None, DELIVER),
        ] {
            assert_eq!(
                Place::of(status, None, from),
                Place::stage_of(stage),
                "{status}"
            );
        }
        let json = serde_json::to_value(Place::stage_of(DEV)).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"place": "in_stage", "stage": "dev"})
        );
        assert_eq!(
            serde_json::to_value(Place::Backlog).unwrap(),
            serde_json::json!({"place": "backlog"})
        );
    }

    #[test]
    fn needs_you_family_names_match_the_stored_states() {
        let names: Vec<_> = Status::NEEDS_YOU_FAMILY
            .iter()
            .map(|s| s.as_str())
            .collect();
        assert_eq!(
            names,
            [
                "asks_you",
                "needs_you",
                "idle",
                "validator_needs_you",
                "validation_failed",
                "validation_no_result",
                "auto_merge_stopped"
            ]
        );
        for status in [Status::Checking, Status::MergeConflictFixing] {
            assert!(!status.is_needs_you_family(), "{status} is neutral");
        }
    }

    #[test]
    fn no_stacking_states() {
        for status in Status::ALL {
            let stored = status.as_str();
            assert!(
                !stored.contains("pause") && !stored.contains("restack"),
                "{stored} is a stacking state (d0049)"
            );
        }
    }
}
