//! Flags: facts about a ticket that are independent of its status (SDD §7.2). A card shows them next to the
//! status; which one wins the card's pill slot is the card's concern (T-0031).

use serde::{Deserialize, Serialize};

use super::{TicketKey, stored_enum};

/// "Sent back · 3 comments · round 2" (d0034); "Sent back · 1 decision changed · round 2" (DEC-06).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SentBack {
    pub comments: u32,
    pub round: u32,
    /// The recorded decisions Change… sent back in this round (DEC-06, d0084).
    #[serde(default)]
    pub decisions: u32,
}

/// "same file as SHOP-40.4": another running child of the same epic has changed these files too, so far
/// (LEAD-09, d0083). Quiet: it wakes no one and pauses no one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SameFile {
    pub other: TicketKey,
    /// The files both changed, sorted.
    pub files: Vec<String>,
}

/// "1 unread note from Wishlist lead": the lead's notes the running child's agent hasn't read yet (LEAD-10, d0083);
/// the hover shows the latest. It clears once they are read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnreadNotes {
    pub count: u32,
    /// The lead's name: "Wishlist lead".
    pub from: String,
    pub latest_text: String,
    /// When the latest was left, in Unix milliseconds.
    pub latest_at: i64,
}

stored_enum! {
    /// The stops a stuck epic child reaches (LEAD-07, d0083): its first one goes to the epic's live lead once.
    pub enum StuckStop ("stuck stop") {
        /// "Validation failed 3 times" (VAL-06, d0063).
        ValidationFailed = "validation_failed",
        /// "Keeps conflicting with `<epic branch>`" (AMG-03, d0064).
        KeepsConflicting = "keeps_conflicting",
        /// "Merge check keeps failing on `<epic branch>`" (d0120).
        KeepsFailingMergeCheck = "keeps_failing_merge_check",
        /// Idle: the agent stopped and waits for input (ATT-01, d0042).
        Idle = "idle",
    }
}

/// "Lead's note sent · validation round 0 of 3: “…”" (LEAD-07, d0083): the one note the epic's lead sent this child
/// at its first stop, which counted as Answer the agent. The second stop's inbox item and the card's hover show it
/// with the lead's view ("LEAD'S NOTE · SENT AFTER ROUND 3").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeadNoteSent {
    /// The lead's name: "Wishlist lead".
    pub lead: String,
    pub text: String,
    /// The lead's view of the cause (`otaq note --view`), when given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view: Option<String>,
    /// The stop the note answered.
    pub stop: StuckStop,
    /// Validation failed: the rounds that had failed when the note was sent ("after round 3").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_rounds: Option<u32>,
    /// When it was sent, in Unix milliseconds.
    pub at: i64,
}

/// "Updated by lead" / "new · added by Wishlist lead" (LEAD-08, d0083): what the epic's lead did to this child. A card
/// shows it only while the child has never started.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ByLead {
    /// The lead's name: "Wishlist lead".
    pub lead: String,
    /// The lead created this child (`create --parent`).
    #[serde(default, skip_serializing_if = "is_false")]
    pub added: bool,
    /// The lead changed it since the user last looked (the "Updated by agent" mark, d0019): an edit or a link.
    #[serde(default, skip_serializing_if = "is_false")]
    pub updated: bool,
    /// The fields those edits set, in the words History uses: "description", "title", "priority".
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<String>,
    /// The parents those links added, oldest first ("now waits for SHOP-40.4"); a card names those still linked.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub waits_for: Vec<TicketKey>,
}

/// Where a ticket's Merge check is (d0120): what the card and the sheet say.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MergeCheckState {
    /// Local merge runs it now, before the target moves; the user can stop it.
    Running,
    /// It failed: nothing was merged and the ticket's agent fixes it.
    Failed,
}

impl MergeCheckState {
    /// "Merging · running the merge check", "Merge check failed · fixing".
    pub fn label(self) -> &'static str {
        match self {
            Self::Running => super::validation::MERGE_CHECK_RUNNING,
            Self::Failed => super::validation::MERGE_CHECK_FAILED_FIXING,
        }
    }
}

/// Every flag of SDD §7.2. Absent flags are `None`, `false` or zero.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Flags {
    /// When the ticket was pinned, in Unix milliseconds (d0047). Pinned is not a status.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pinned_at: Option<i64>,
    /// Sent back · N comments · round K (d0034).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sent_back: Option<SentBack>,
    /// Conflicts with `<base>`: the base branch it conflicts with (d0038).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conflicts_with: Option<String>,
    /// The conflict came from the test merge after another ticket merged, while this one waited in Review (d0127):
    /// while its agent fixes it the card says "Conflicts with main · fixing". Not a flag of its own.
    #[serde(default, skip_serializing_if = "is_false")]
    pub conflict_in_review: bool,
    /// Changed since you reviewed it (d0043).
    #[serde(default, skip_serializing_if = "is_false")]
    pub changed_since_review: bool,
    /// N messages waiting to be typed into the session (d0048).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub messages_waiting: u32,
    /// A hand-off typed into the live session that its agent hasn't picked up yet: "Sent back · waiting for the agent
    /// to pick it up" (d0118). Cleared by the session's next hook or `otaq` command; never stored (a restart drops it).
    /// Not a flag of its own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hand_off: Option<super::HandOffKind>,
    /// Run is running, on this port: `▶ :4020` (d0035).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_port: Option<u16>,
    /// Updated by agent (d0019).
    #[serde(default, skip_serializing_if = "is_false")]
    pub updated_by_agent: bool,
    /// Answering another ticket's question: "answering SHOP-40.2" (d0030).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answering: Option<TicketKey>,
    /// Asked another ticket and waits for the answer: "asked SHOP-40.1 · waiting" (d0030). The oldest open
    /// question; it lasts until the answer was typed into this ticket's session (MSG-06).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asked: Option<TicketKey>,
    /// The oldest open question's text ("Waiting for an answer: …").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asked_question: Option<String>,
    /// More open questions besides the oldest: "asked SHOP-40.1 +1 · waiting".
    #[serde(default, skip_serializing_if = "is_zero")]
    pub asked_more: u32,
    /// When the agent last signalled, in Unix milliseconds (d0027).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_signal_at: Option<i64>,
    /// Tokens used so far (d0020).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokens: Option<u64>,
    /// The ticket ran a conversation whose token count couldn't be read (Claude Code's file is missing or
    /// changed format): shown as "—" (USG-01). Not a flag of its own.
    #[serde(default, skip_serializing_if = "is_false")]
    pub tokens_unknown: bool,
    /// PR #N (d0015, d0017).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pr_number: Option<u32>,
    /// Back from Approved for PR rework and not submitted again: "Back for rework · Send back to start the agent"
    /// (d0041). Once resubmitted, Approve offers Push to PR.
    #[serde(default, skip_serializing_if = "is_false")]
    pub pr_rework: bool,
    /// "PR provider not detected" (d0050).
    #[serde(default, skip_serializing_if = "is_false")]
    pub pr_provider_not_detected: bool,
    /// In Review because the user submitted it (d0052): the note, possibly empty ("Submitted by you · “…”").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub submitted_by_you: Option<String>,
    /// A parked ticket back in Dev, about to continue its conversation: "Resuming conversation…" (d0052).
    #[serde(default, skip_serializing_if = "is_false")]
    pub resuming_conversation: bool,
    /// How it reached Review (d0063): "Validated · round 2", "Submitted by you"…
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_entry: Option<super::ReviewEntryFlag>,
    /// A working card whose last validation round failed and went back to the agent: "Validation failed ·
    /// round 1 of 3".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation_round: Option<super::ValidationRound>,
    /// Validating… or Needs you · validator: the round the validator checks, "validator · round 1 of 3" (d0063).
    /// Not a flag of its own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validating_round: Option<super::ValidationRound>,
    /// The agent check the round markers count for, by name ("Security"): the check the card is in, else the one
    /// that counted last, the one that sent it back (FLW-13, each check counts its own). Not a flag of its own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub check: Option<String>,
    /// Why its Delivery waits for a click: the checks the user moved it past in this walk ("Failed Security 3 times ·
    /// moved by you", "Skipped Security · by you"; d0108 "Move on anyway", FLW-13). Not a flag of its own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub moved_past_check: Option<String>,
    /// How it came to wait in a human check or at its Delivery ("Passed Security · round 2", "Skipped Security · by
    /// you", "Moved back by you"; FLW-14, [`crate::board::human_check::Arrival`]). Not a flag of its own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arrival: Option<crate::board::human_check::Arrival>,
    /// The last failed round's verdict, while the agent fixes it or after the last round failed: the first line of
    /// the comment its checker posted in that round ("Fail: the 10,000 row limit isn't enforced; …"), which the card
    /// shows with "· see comment" (CMT-10, d0109). Not a flag of its own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verdict: Option<String>,
    /// The files the recorded conflict touches: "Merge conflict · fixing · lib/wishlist.ts" (d0064). Not a flag
    /// of its own.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conflict_files: Vec<String>,
    /// Merged by an automatic local Delivery: "Merged automatically" (d0064, d0108).
    #[serde(default, skip_serializing_if = "is_false")]
    pub merged_automatically: bool,
    /// Done through an automatic Delivery with no check passed before it: "Delivered without checks" (amber, d0108
    /// rule 8).
    #[serde(default, skip_serializing_if = "is_false")]
    pub delivered_without_checks: bool,
    /// An automatic Delivery is under way (FLW-15): its agent opens or pushes the PR with no dialog. The card waits
    /// for nobody, so it isn't in the inbox's group 3. Not a flag of its own.
    #[serde(default, skip_serializing_if = "is_false")]
    pub delivering_automatically: bool,
    /// Closed by an agent's `otaq stage done` (AGM-10): "Closed · nothing to merge", with the agent's reason (empty
    /// when it gave none). Not a flag of its own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nothing_to_merge: Option<String>,
    /// The Merge check (d0120): "Merging · running the merge check" while Local merge runs it, "Merge check failed ·
    /// fixing" while the agent fixes what it found. Not a flag of its own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merge_check: Option<MergeCheckState>,
    /// A Backlog ticket (not an epic) whose dependencies are all merged: the quiet "Ready to start" hint. It never
    /// starts by itself (d0066, SCH-02).
    #[serde(default, skip_serializing_if = "is_false")]
    pub ready_to_start: bool,
    /// A needs-you status while the user's terminal for the waiting session has focus (ATT-03, d0042; per session,
    /// so the validator's prompt calms only with the validator's terminal, d0063): the card stays calm, no inbox
    /// item, no notification; the terminal strip shows "your turn".
    #[serde(default, skip_serializing_if = "is_false")]
    pub your_turn: bool,
    /// A running epic child that changed a file another running child of its epic changed too: one entry per
    /// other child, by key (LEAD-09, d0083).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub same_file: Vec<SameFile>,
    /// "asked lead · waiting" (LEAD-06, d0083): the ticket's `ask human` went to its epic's live lead, which hasn't
    /// answered or passed it on yet. Display only, ranked as "asked SHOP-40.1 · waiting" (MSG-06), never turquoise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asked_lead: Option<AskedLead>,
    /// "Asks you · via Wishlist lead" (LEAD-06): the open question was passed on by the epic's lead (its name);
    /// the question carries the lead's recommendation and reason.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub via_lead: Option<String>,
    /// The lead's unread notes to this child (LEAD-10, d0083).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unread_notes: Option<UnreadNotes>,
    /// "1 decision": the open recorded decisions of a ticket in Ready for Review, an epic's with every child's
    /// (DEC-03, d0084). Zero in every other column and once approved. Not a flag of its own: a neutral count in the
    /// card's meta row.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub open_decisions: u32,
    /// The lead's one note at this child's first stop (LEAD-07, d0083); it stays once sent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lead_note_sent: Option<LeadNoteSent>,
    /// What the epic's lead did to this child (LEAD-08, d0083).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by_lead: Option<ByLead>,
}

/// A question waiting for the epic's lead (LEAD-06).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AskedLead {
    /// "Wishlist lead".
    pub lead: String,
    /// "Should the button show a filled heart once the item is saved?"
    pub question: String,
}

fn is_false(b: &bool) -> bool {
    !b
}

fn is_zero(n: &u32) -> bool {
    *n == 0
}

stored_enum! {
    /// The name of each flag, for code that ranks or lists them.
    pub enum Flag ("flag") {
        Pinned = "pinned",
        SentBack = "sent_back",
        ConflictsWithBase = "conflicts_with_base",
        ChangedSinceReview = "changed_since_review",
        MessagesWaiting = "messages_waiting",
        RunRunning = "run_running",
        UpdatedByAgent = "updated_by_agent",
        Answering = "answering",
        Asked = "asked",
        LastSignal = "last_signal",
        Tokens = "tokens",
        PrNumber = "pr_number",
        PrRework = "pr_rework",
        PrProviderNotDetected = "pr_provider_not_detected",
        SubmittedByYou = "submitted_by_you",
        ResumingConversation = "resuming_conversation",
        ReviewEntry = "review_entry",
        ValidationRound = "validation_round",
        MergedAutomatically = "merged_automatically",
        DeliveredWithoutChecks = "delivered_without_checks",
        YourTurn = "your_turn",
        SameFile = "same_file",
        AskedLead = "asked_lead",
        ViaLead = "via_lead",
        UnreadNotes = "unread_notes",
        LeadNoteSent = "lead_note_sent",
        ByLead = "by_lead",
    }
}

impl Flags {
    pub fn is_pinned(&self) -> bool {
        self.pinned_at.is_some()
    }

    /// The flags that are set, in declaration order.
    pub fn set(&self) -> Vec<Flag> {
        let present = [
            (Flag::Pinned, self.pinned_at.is_some()),
            (Flag::SentBack, self.sent_back.is_some()),
            (Flag::ConflictsWithBase, self.conflicts_with.is_some()),
            (Flag::ChangedSinceReview, self.changed_since_review),
            (Flag::MessagesWaiting, self.messages_waiting > 0),
            (Flag::RunRunning, self.run_port.is_some()),
            (Flag::UpdatedByAgent, self.updated_by_agent),
            (Flag::Answering, self.answering.is_some()),
            (Flag::Asked, self.asked.is_some()),
            (Flag::LastSignal, self.last_signal_at.is_some()),
            (Flag::Tokens, self.tokens.is_some()),
            (Flag::PrNumber, self.pr_number.is_some()),
            (Flag::PrRework, self.pr_rework),
            (Flag::PrProviderNotDetected, self.pr_provider_not_detected),
            (Flag::SubmittedByYou, self.submitted_by_you.is_some()),
            (Flag::ResumingConversation, self.resuming_conversation),
            (Flag::ReviewEntry, self.review_entry.is_some()),
            (Flag::ValidationRound, self.validation_round.is_some()),
            (Flag::MergedAutomatically, self.merged_automatically),
            (Flag::DeliveredWithoutChecks, self.delivered_without_checks),
            (Flag::YourTurn, self.your_turn),
            (Flag::SameFile, !self.same_file.is_empty()),
            (Flag::AskedLead, self.asked_lead.is_some()),
            (Flag::ViaLead, self.via_lead.is_some()),
            (Flag::UnreadNotes, self.unread_notes.is_some()),
            (Flag::LeadNoteSent, self.lead_note_sent.is_some()),
            (Flag::ByLead, self.by_lead.is_some()),
        ];
        present
            .into_iter()
            .filter_map(|(flag, on)| on.then_some(flag))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_set() -> Flags {
        Flags {
            pinned_at: Some(1),
            sent_back: Some(SentBack {
                comments: 3,
                round: 2,
                decisions: 0,
            }),
            conflicts_with: Some("main".into()),
            conflict_in_review: true,
            changed_since_review: true,
            messages_waiting: 2,
            hand_off: None,
            run_port: Some(4020),
            updated_by_agent: true,
            answering: Some("SHOP-40.2".parse().unwrap()),
            asked: Some("SHOP-40.1".parse().unwrap()),
            asked_question: Some("Which endpoint?".into()),
            asked_more: 1,
            last_signal_at: Some(5),
            tokens: Some(18_400),
            tokens_unknown: false,
            pr_number: Some(482),
            pr_rework: true,
            pr_provider_not_detected: true,
            submitted_by_you: Some("Paging works".into()),
            resuming_conversation: true,
            review_entry: Some(crate::domain::ReviewEntryFlag {
                entry: crate::domain::ReviewEntry::Validated,
                round: 2,
            }),
            validation_round: Some(crate::domain::ValidationRound { round: 1, of: 3 }),
            validating_round: Some(crate::domain::ValidationRound { round: 2, of: 3 }),
            check: Some("Security".into()),
            moved_past_check: Some("Skipped Security · by you".into()),
            arrival: Some(crate::board::human_check::Arrival::Passed {
                stage: "Security".into(),
                round: Some(2),
            }),
            verdict: Some("Fail: the total ignores the coupon".into()),
            conflict_files: vec!["lib/wishlist.ts".into()],
            merged_automatically: true,
            delivered_without_checks: true,
            delivering_automatically: true,
            nothing_to_merge: Some("Already fixed in SHOP-31.".into()),
            merge_check: Some(MergeCheckState::Failed),
            ready_to_start: true,
            your_turn: true,
            same_file: vec![SameFile {
                other: "SHOP-40.4".parse().unwrap(),
                files: vec!["lib/wishlist.ts".into()],
            }],
            asked_lead: Some(AskedLead {
                lead: "Wishlist lead".into(),
                question: "Should the button show a filled heart?".into(),
            }),
            via_lead: Some("Wishlist lead".into()),
            unread_notes: Some(UnreadNotes {
                count: 1,
                from: "Wishlist lead".into(),
                latest_text: "Use the same order on the shared list.".into(),
                latest_at: 7,
            }),
            open_decisions: 1,
            lead_note_sent: Some(LeadNoteSent {
                lead: "Wishlist lead".into(),
                text: "The API returns `items[]`; fix the test, don't change the API.".into(),
                view: Some("The agent keeps changing the API instead of the test.".into()),
                stop: StuckStop::ValidationFailed,
                after_rounds: Some(3),
                at: 8,
            }),
            by_lead: Some(ByLead {
                lead: "Wishlist lead".into(),
                added: true,
                updated: true,
                fields: vec!["description".into()],
                waits_for: vec!["SHOP-40.4".parse().unwrap()],
            }),
        }
    }

    #[test]
    fn every_flag_is_reported_once() {
        assert_eq!(all_set().set(), Flag::ALL);
        assert!(Flags::default().set().is_empty());
    }

    #[test]
    fn round_trips_and_stays_small_when_empty() {
        let flags = all_set();
        let json = serde_json::to_string(&flags).unwrap();
        assert_eq!(serde_json::from_str::<Flags>(&json).unwrap(), flags);
        assert_eq!(serde_json::to_string(&Flags::default()).unwrap(), "{}");
        for flag in Flag::ALL {
            assert_eq!(flag.as_str().parse::<Flag>().unwrap(), *flag);
        }
    }
}
