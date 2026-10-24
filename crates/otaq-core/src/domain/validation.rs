//! Validation and the Delivery's stops (SDD §7.2 M12 rows, §14.0, FLW-15, FLW-17; d0063, d0064, d0108): how a ticket
//! reached Review, the outcome of a validation round, why a Delivery stopped, and the exact texts the board shows for
//! them.
//!
//! Texts with a number or a branch ("Validation failed 3 times", "Keeps conflicting with `main`") are built
//! here, so the card, the panel, the inbox and the CLI read the same.

use std::fmt;

use serde::{Deserialize, Serialize};

use super::stored_enum;

/// "Validating…": the agent submitted and the validator is checking (neutral, d0063).
pub const VALIDATING: &str = "Validating…";
/// The validator waits on a permission prompt (turquoise).
pub const VALIDATOR_NEEDS_YOU: &str = "Needs you · validator";
/// The validator ended its turn without a verdict (turquoise).
pub const NO_RESULT: &str = "Validation ended without a result";
/// The validator changed tracked files or committed: its verdict is thrown away (VAL-02).
pub const DISCARDED: &str = "Validation discarded: the validator changed files";
/// A Delivery's free rebase conflicted; the agent is rebasing with `restack`, no click (FLW-17).
pub const MERGE_CONFLICT_FIXING: &str = "Merge conflict · fixing";
/// Local merge runs the project's Merge check on the merged result before the target moves (d0120).
pub const MERGE_CHECK_RUNNING: &str = "Merging · running the merge check";
/// The Merge check failed: nothing merged, the agent fixes it with `restack`, no click (d0120).
pub const MERGE_CHECK_FAILED_FIXING: &str = "Merge check failed · fixing";
/// Done through an automatic local Delivery (FLW-15, AMG-02).
pub const MERGED_AUTOMATICALLY: &str = "Merged automatically";

/// "Validation failed 3 times": the last round failed (VAL-06).
pub fn failed_times(times: u32) -> String {
    format!("Validation failed {}", times_word(times))
}

/// "1 time", "3 times".
fn times_word(times: u32) -> String {
    if times == 1 {
        "1 time".to_owned()
    } else {
        format!("{times} times")
    }
}

/// What changed under a validation and cancelled it (VAL-08).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancelledBy {
    /// The agent changed tracked files or committed.
    Code,
    /// The user edited the description.
    Description,
}

/// The cancel toast (frame *11 → Success toasts*): "Validation cancelled: SHOP-29’s code changed" · "Round not
/// counted · the agent was told to submit again".
pub fn cancelled_toast(key: &str, by: CancelledBy) -> (String, String) {
    let what = match by {
        CancelledBy::Code => "code",
        CancelledBy::Description => "description",
    };
    (
        format!("Validation cancelled: {key}’s {what} changed"),
        "Round not counted · the agent was told to submit again".to_owned(),
    )
}

/// The automatic local Delivery's toast (frame *11 → Success toasts*): "SHOP-40.1 merged automatically into the
/// Wishlist branch" · "Validated · round 1 · local, not pushed · SHOP-40.2 can start". `into` is "the Wishlist branch"
/// for an epic child, else the base branch; `round` is the round that passed, `None` with no check passed before the
/// Delivery ("Delivered without checks", d0108 rule 8); `started` are the waiting tickets that started after the
/// merge.
pub fn merged_toast(
    key: &str,
    into: &str,
    round: Option<u32>,
    started: &[String],
) -> (String, String) {
    let mut message = match round {
        Some(round) => format!("Validated · round {round} · local, not pushed"),
        None => format!("{DELIVERED_WITHOUT_CHECKS} · local, not pushed"),
    };
    match started {
        [] => {}
        [one] => message.push_str(&format!(" · {one} can start")),
        [rest @ .., last] => {
            message.push_str(&format!(" · {} and {last} can start", rest.join(", ")))
        }
    }
    (format!("{key} merged automatically into {into}"), message)
}

/// "Delivered without checks": an automatic Delivery with no check passed before it (d0108 rule 8).
pub const DELIVERED_WITHOUT_CHECKS: &str = "Delivered without checks";

/// "Keeps conflicting with `main`": a Delivery's third conflict in a row (FLW-17, AMG-03).
pub fn keeps_conflicting(target: &str) -> String {
    format!("Keeps conflicting with `{target}`")
}

/// "Merge check keeps failing on `main`": a Delivery's third conflict or failed Merge check in a row, the last one a
/// failed check (d0120).
pub fn keeps_failing_merge_check(target: &str) -> String {
    format!("Merge check keeps failing on `{target}`")
}

/// The comment a failed Merge check posts, with the output attached (d0120, d0109): "Merge check `npm test` failed on
/// the result rebased onto main (exit 1). Fix it, then submit again." `exit` is `None` when a signal ended it.
pub fn merge_check_failed(command: &str, target: &str, exit: Option<i32>) -> String {
    let exit = exit.map_or_else(|| "killed by a signal".to_owned(), |c| format!("exit {c}"));
    format!(
        "Merge check `{command}` failed on the result rebased onto {target} ({exit}). Fix it, then submit again."
    )
}

/// What a live agent session is typed after `restack` when the Merge check failed (CMT-05: the hand-over named, never
/// its text): "SHOP-19's merge check failed on the result rebased onto `main`; the comment has its output. Run `otaq
/// prime`."
pub fn merge_check_failed_told(key: &super::TicketKey, target: &str) -> String {
    format!(
        "{key}'s merge check failed on the result rebased onto `{target}`; the comment has its output. Run `{} prime`.",
        crate::naming::COMMAND_NAME
    )
}

/// "Couldn't deliver · git commit failed (exit 1): …": an automatic Delivery's git step failed (d0121).
pub fn delivery_failed(reason: &str) -> String {
    format!("Couldn\u{2019}t deliver \u{b7} {reason}")
}

stored_enum! {
    /// How a ticket reached Ready for Review (d0063). `None` on tickets that got there before M12 or with
    /// validation off.
    pub enum ReviewEntry ("review entry") {
        /// Passed validation: "Validated · round N".
        Validated = "validated",
        /// No verdict, and the user moved it on: "Validation skipped by you".
        Skipped = "skipped",
        /// The user's Move to next stage into a human check with no agent check before it: "Submitted by you"
        /// (d0052, d0108 rule 10; with an agent check next, the check runs and its pass is `Validated`).
        SubmittedByUser = "submitted_by_user",
        /// Failed every round, moved on by the user: "Failed M times · moved by you" (reads as "did not pass").
        FailedMoved = "failed_moved",
    }
}

impl ReviewEntry {
    /// The card's text; `round` is the ticket's validation round (the round that passed, or the rounds that
    /// failed).
    pub fn label(self, round: u32) -> String {
        match self {
            Self::Validated => format!("Validated · round {round}"),
            Self::Skipped => "Validation skipped by you".to_owned(),
            Self::SubmittedByUser => "Submitted by you".to_owned(),
            Self::FailedMoved => format!("Failed {} · moved by you", times_word(round)),
        }
    }
}

/// The Review entry as a card flag: the entry and its round.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewEntryFlag {
    pub entry: ReviewEntry,
    pub round: u32,
}

impl fmt::Display for ReviewEntryFlag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.entry.label(self.round))
    }
}

/// "Validation failed · round 1 of 3": a working card whose last round failed and went back to the agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationRound {
    pub round: u32,
    pub of: u32,
}

impl fmt::Display for ValidationRound {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Validation failed · round {} of {}", self.round, self.of)
    }
}

stored_enum! {
    /// How the user moved a card past an agent check (`check_counters.moved_past`, FLW-13): either way the card
    /// reaches its Delivery in click mode (d0108 "Move on anyway"; Q277).
    pub enum MovedPast ("moved past") {
        /// Move on anyway after the check's last allowed round failed: "Failed Security 3 times · moved by you".
        Failed = "failed",
        /// Skip this check (on a running check, or one held for the plan limit with no usable fallback), or Move on
        /// anyway after it ended without a result: "Skipped Security · by you".
        Skipped = "skipped",
    }
}

impl MovedPast {
    /// Why the card's Delivery waits for a click: "Failed Security 3 times · moved by you", "Skipped Security · by
    /// you". `fails` are the check's fails in a row when the card was moved on.
    pub fn reason(self, check: &str, fails: u32) -> String {
        match self {
            Self::Failed => format!("Failed {check} {} · moved by you", times_word(fails)),
            Self::Skipped => format!("Skipped {check} · by you"),
        }
    }
}

stored_enum! {
    /// Who started a validation round (`validations.trigger`).
    pub enum ValidationTrigger ("validation trigger") {
        /// The agent's `stage next` (or `review submit`).
        Agent = "agent",
        /// The user's Move to next stage.
        User = "user",
    }
}

stored_enum! {
    /// How a validation round ended (`validations.outcome`, SDD §8). Only `pass` and `fail` count as rounds.
    pub enum ValidationOutcome ("validation outcome") {
        Pass = "pass",
        Fail = "fail",
        /// The code or the description changed through the agent or the user (VAL-08).
        Cancelled = "cancelled",
        /// The validator's turn ended without a verdict (VAL-07).
        NoResult = "no_result",
        /// The validator changed files; its verdict was thrown away (VAL-02).
        Discarded = "discarded",
        /// The validator's session died with the service or the machine (REC-01).
        Interrupted = "interrupted",
        /// The user's Skip this check.
        Skipped = "skipped",
        /// The user's Stop this check, Park or Close.
        Stopped = "stopped",
    }
}

impl ValidationOutcome {
    /// Whether the round counts toward the rounds setting.
    pub fn counts(self) -> bool {
        matches!(self, Self::Pass | Self::Fail)
    }
}

stored_enum! {
    /// Why a Delivery stopped and turned the card turquoise (FLW-17; AMG-03, AMG-04). Kept by its old name: the
    /// stored column is `auto_merge_stop`.
    pub enum AutoMergeStop ("auto-merge stop") {
        /// Three conflicts in a row: "Keeps conflicting with `<target>`".
        KeepsConflicting = "keeps_conflicting",
        /// Three conflicts or failed Merge checks in a row, the last a failed check: "Merge check keeps failing on
        /// `<target>`" (d0120).
        KeepsFailingMergeCheck = "keeps_failing_merge_check",
        /// The base diverged from its remote (GIT-01).
        BaseDiverged = "base_diverged",
        /// The user's checkout of the target branch has uncommitted changes (GIT-03).
        CheckoutDirty = "checkout_dirty",
        /// A git step of the automatic Delivery failed (a hook refused the commit): "Couldn't deliver · …" (d0121).
        DeliveryFailed = "delivery_failed",
    }
}

stored_enum! {
    /// The user's actions on a validation (VAL-06, VAL-07, VAL-09): the panel's buttons and "…" menu items.
    pub enum ValidationAction ("validation action") {
        /// While validating: the validator stops, the round doesn't count, back to Agent Working.
        Stop = "stop",
        /// While validating: Ready for Review, "Validation skipped by you", never auto-merged.
        Skip = "skip",
        /// After the last failed round, or no result: Ready for Review without a pass, never auto-merged.
        MoveOnAnyway = "move_on_anyway",
        /// After the last failed round: the findings go to the agent, one more round is allowed.
        AnotherRound = "another_round",
        /// After the last failed round: the user's note goes with the findings, the counter restarts at 0.
        AnswerAgent = "answer_agent",
        /// After no result: a fresh validator, not a round.
        Retry = "retry",
        /// After a Delivery's stop (FLW-17): back to the Delivery, which runs again when it is automatic.
        RetryMerge = "retry_merge",
        /// After a Delivery's stop: back to where the user delivers (the default flow's Review, still "Validated ·
        /// round N"), not merged again.
        MoveToReview = "move_to_review",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The exact texts of SDD §7.2.
    #[test]
    fn labels() {
        assert_eq!(VALIDATING, "Validating…");
        assert_eq!(
            ValidationRound { round: 1, of: 3 }.to_string(),
            "Validation failed · round 1 of 3"
        );
        assert_eq!(failed_times(3), "Validation failed 3 times");
        assert_eq!(failed_times(1), "Validation failed 1 time");
        assert_eq!(
            ReviewEntry::FailedMoved.label(1),
            "Failed 1 time · moved by you"
        );
        assert_eq!(keeps_conflicting("main"), "Keeps conflicting with `main`");
        let entries: Vec<String> = ReviewEntry::ALL.iter().map(|e| e.label(2)).collect();
        assert_eq!(
            entries,
            [
                "Validated · round 2",
                "Validation skipped by you",
                "Submitted by you",
                "Failed 2 times · moved by you"
            ]
        );
    }

    /// The toasts of frame *11 → Success toasts* (T-0196).
    #[test]
    fn toasts() {
        assert_eq!(
            merged_toast(
                "SHOP-40.1",
                "the Wishlist branch",
                Some(1),
                &["SHOP-40.2".into()]
            ),
            (
                "SHOP-40.1 merged automatically into the Wishlist branch".to_owned(),
                "Validated · round 1 · local, not pushed · SHOP-40.2 can start".to_owned()
            )
        );
        assert_eq!(
            merged_toast("SHOP-18", "main", Some(2), &[]).1,
            "Validated · round 2 · local, not pushed"
        );
        assert_eq!(
            merged_toast("SHOP-18", "main", None, &[]).1,
            "Delivered without checks · local, not pushed"
        );
        assert_eq!(
            merged_toast(
                "SHOP-1",
                "main",
                Some(1),
                &["SHOP-2".into(), "SHOP-3".into(), "SHOP-4".into()]
            )
            .1,
            "Validated · round 1 · local, not pushed · SHOP-2, SHOP-3 and SHOP-4 can start"
        );
        assert_eq!(
            cancelled_toast("SHOP-29", CancelledBy::Code),
            (
                "Validation cancelled: SHOP-29’s code changed".to_owned(),
                "Round not counted · the agent was told to submit again".to_owned()
            )
        );
        assert_eq!(
            cancelled_toast("SHOP-29", CancelledBy::Description).0,
            "Validation cancelled: SHOP-29’s description changed"
        );
    }

    /// T-0417 (FLW-13): why a card moved past a check reaches its Delivery in click mode.
    #[test]
    fn moved_past_reasons() {
        assert_eq!(
            MovedPast::Failed.reason("Security", 3),
            "Failed Security 3 times · moved by you"
        );
        assert_eq!(
            MovedPast::Failed.reason("Validate", 1),
            "Failed Validate 1 time · moved by you"
        );
        assert_eq!(
            MovedPast::Skipped.reason("Security", 0),
            "Skipped Security · by you"
        );
    }

    #[test]
    fn only_verdicts_count_as_rounds() {
        let counted: Vec<_> = ValidationOutcome::ALL
            .iter()
            .filter(|o| o.counts())
            .collect();
        assert_eq!(
            counted,
            [&ValidationOutcome::Pass, &ValidationOutcome::Fail]
        );
    }
}
