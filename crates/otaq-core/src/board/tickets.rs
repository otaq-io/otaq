//! Tickets and epics: create, edit, key allocation, fields, actor and the event log (TKT-01, TKT-03; d0019, d0029,
//! d0032, d0051, d0059).
//!
//! New tickets and epics land in Backlog. Numbers increase per project and are never handed out again, even after
//! a deletion (d0029); epic children get the epic's next `.n`, also never reused (d0039). Standalone tickets and
//! epics store their base branch ("Start from and merge into"), the project's at creation unless overridden;
//! epic children have none of their own and use the epic's branch (d0051, d0059). External IDs exist only on
//! epics and standalone tickets. Every change records its actor and appends an `events` row.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{FromRow, Sqlite, Transaction};

use super::epic_flight::EpicQueueMode;
use super::{Board, BoardError, Refusal};
use crate::domain::{
    Actor, BuildState, Flags, Kind, MergeCheckState, Place, Priority, ProjectKey, RemoteWarning,
    ResourceState, SchemaOverlap, StartStep, Status, StatusSet, TicketKey, stored_enum,
};
use crate::store::projects::Project;
use crate::store::tickets::TicketRow;
use crate::store::{new_id, now_ms};

/// The branch a ticket starts from and merges into (d0051, d0059).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Base {
    /// A branch of the repository: standalone tickets and epics.
    Branch(String),
    /// The integration branch of this epic: epic children.
    Epic(TicketKey),
}

impl Base {
    /// How the base reads in messages: `main`, or "the epic's branch (SHOP-40)".
    pub fn describe(&self) -> String {
        match self {
            Self::Branch(branch) => format!("`{branch}`"),
            Self::Epic(epic) => format!("the epic's branch ({epic})"),
        }
    }
}

/// A ticket or epic as the board, the editor and the CLI see it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ticket {
    pub id: String,
    pub key: TicketKey,
    pub kind: Kind,
    pub title: String,
    /// "What and why" (Markdown).
    pub body: String,
    /// The effective priority: the ticket's own, else (an epic child) the epic's (TKT-09, d0065).
    #[serde(default)]
    pub priority: Priority,
    /// An epic child without a priority of its own: `priority` is the epic's and follows it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub priority_inherited: bool,
    /// The epic this ticket belongs to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub epic: Option<TicketKey>,
    /// Where it is: Backlog, waiting before an Agent stage, a stage of its flow, or Done (STG-04). In JSON
    /// `"place"` and, in or before a stage, `"stage"` (the stage's id).
    #[serde(flatten)]
    pub place: Place,
    pub status: Status,
    pub base: Base,
    /// The base can't change any more (the ticket has been to Dev; for an epic, a child has).
    pub base_locked: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_id: Option<String>,
    /// Direct parents, in key order.
    pub depends_on: Vec<TicketKey>,
    /// Old keys that still resolve to this ticket (after Move to epic, d0044).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<TicketKey>,
    /// Epics only: the number the next child gets (child numbers are never reused, d0039).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_child_number: Option<u32>,
    pub flags: Flags,
    pub created_by: Actor,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_by: Option<Actor>,
    /// When the ticket first went to Dev; `None` for a ticket that never did.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
    /// When the ticket entered its current status (the age of a needs-you item).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_since: Option<i64>,
    /// When the user's drag last put the ticket in Ready for Dev (Queued).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub queued_at: Option<i64>,
    /// When the queued ticket became ready to start: the start order's last key, "ready the longest" (SCH-01).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ready_since: Option<i64>,
    /// The round of the agent check its markers count for ([`Flags::check`], FLW-13): that check's fails in a row,
    /// or the round it last passed in ("Validated · round 2"). Each check counts its own (`check_counters`, d0063).
    #[serde(default)]
    pub validation_round: u32,
    /// The Delivery's conflicts in a row, in either mode (FLW-17, d0064); a merge resets them.
    #[serde(default)]
    pub auto_merge_conflicts: u32,
    /// The Delivery stopped: why (FLW-17, d0064).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_merge_stop: Option<crate::domain::AutoMergeStop>,
    /// Where the ticket works once started: branch, worktree, ports (T-0046, T-0048).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub work: Option<Work>,
    /// Why a Queued ticket is held or its start stopped halfway (SES-01, SES-02).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_problem: Option<StartProblem>,
    /// Interrupted only: the needs-you status it had when its session ended (REC-01, d0045). The label, its text
    /// (`signal`) and the question stay with the ticket, so answering from the inbox resumes and delivers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interrupted_from: Option<Status>,
    /// What the agent waits for or runs in the background (needs-you family and Working in background, d0042).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signal: Option<AgentSignal>,
    /// Asks you: the open question with its answer options (d0061).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub question: Option<super::questions::HumanQuestion>,
    /// After a push: the PR branch's content differs from what was reviewed (REV-06, T-0145).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pushed_differs: Option<crate::pr::verify::PushedDiff>,
    /// Epics only: how the user queued the epic (d0080, EPC-13). `None` when it was never queued or was taken
    /// back; `Whole` lets new children join the queue; `Single` keeps them "not queued".
    #[serde(
        default,
        skip_serializing_if = "super::epic_flight::EpicQueueMode::is_none"
    )]
    pub epic_queue_mode: super::epic_flight::EpicQueueMode,
    /// Epic children only: the child joined a whole-queued epic's queue when it was created, added by this actor
    /// (the user, or the ticket whose agent ran `otaq create --parent`): "new · added by SHOP-40.2" (EPC-13).
    /// Cleared when the child starts or is taken back.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub joined_queue_by: Option<Actor>,
}

/// The detail of a turquoise or background status: SHOP-23's question, SHOP-22's prompt, the agent's last line,
/// or "background: test run, 1 helper agent".
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentSignal {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// When this status began (the card's "· 6 min").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since: Option<i64>,
    /// Working in background with a scheduled wakeup: when it fires ("wakes up at 4:10pm").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wakeup_at: Option<i64>,
}

/// A started ticket's branch, worktree and ports.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Work {
    /// `otaq/SHOP-18` (the real name is chosen at the first push, M10).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree: Option<String>,
    /// Where the ticket left its base.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_sha: Option<String>,
    /// `$PORT` in the agent's session.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_port: Option<u16>,
    /// `$PORT` in Run and Shell.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_port: Option<u16>,
    /// The branch was pushed: its name can't change any more (GIT-07).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub branch_locked: bool,
    /// The pull request the skill reported (`otaq pr opened`), for Open PR in browser.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pr_url: Option<String>,
    /// Where the build output is: in the worktree, freed into the warm stock at Review entry, or being brought
    /// back (M19-c; d0085, d0086).
    #[serde(default, skip_serializing_if = "BuildState::is_present")]
    pub build_state: BuildState,
    /// Why the last Free failed (Cleanup "could not free").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build_problem: Option<String>,
    /// The ticket resource Otaq created (`OTAQ_RESOURCE_NAME`, M19-d; d0087).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_name: Option<String>,
    #[serde(default, skip_serializing_if = "ResourceState::is_none")]
    pub resource_state: ResourceState,
    /// Other Review tickets that add files under the same schema path (M19-e).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub schema_overlap: Vec<SchemaOverlap>,
    /// The fetch before its start or Local merge failed and it went on from the local base branch (d0121).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_warning: Option<RemoteWarning>,
}

/// The step a start stopped at, and why: "setup command failed" + the command's last lines.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StartProblem {
    pub step: StartStep,
    pub message: String,
}

impl Ticket {
    /// The group of the board the ticket shows in ([`StatusSet::BOARD`]), from its status: the default flow's
    /// columns, until the board's columns come from the flow (STG-08).
    pub fn board_group(&self) -> StatusSet {
        self.status.set().board_group()
    }

    pub fn is_epic(&self) -> bool {
        self.kind == Kind::Epic
    }

    /// Has this ticket ever been to Dev? Never-started tickets can be deleted and moved into an epic (d0044,
    /// d0060).
    pub fn has_started(&self) -> bool {
        self.started_at.is_some()
    }

    /// Turquoise as the user sees it: the agent waits and no terminal of the ticket has focus (ATT-03, d0042).
    pub fn shows_needs_you(&self) -> bool {
        self.status.is_needs_you_family() && !self.flags.your_turn
    }

    /// Whether the ticket waits for the user where they can see it: turquoise (unless its terminal has focus) or
    /// the inbox's second group. Such cards float to the top of Dev and count in the project's needs-you number.
    pub fn needs_user(&self) -> bool {
        super::order::needs_user(self.status) && !self.flags.your_turn
    }
}

/// Creates a ticket or epic in Backlog.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewTicketInput {
    /// The project key.
    pub project: String,
    #[serde(default)]
    pub kind: Kind,
    pub title: String,
    #[serde(default)]
    pub body: String,
    /// Default: Normal for tickets and epics; an epic child without one follows its epic (d0065).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<Priority>,
    /// Creates an epic child: the epic's key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub epic: Option<String>,
    /// Overrides the project's base branch (standalone tickets and epics only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_branch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_id: Option<String>,
    /// The user's first comment, with the report the New ticket form took (RPT-03): posted with the ticket, as the
    /// user's, marked "at creation". Only the user sends one (the policy).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_comment: Option<super::ticket_comments::FirstComment>,
}

/// Changes to a ticket's fields. Absent fields stay as they are.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TicketUpdate {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    /// Sets the ticket's own priority (an epic child then stops following its epic).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<Priority>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_branch: Option<String>,
    /// An empty string removes the External ID.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_id: Option<String>,
    /// Stage choices (`--stage Dev=Quick developer`, AGM-11), checked before any field changes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub choices: Vec<super::stage_choices::ChoicePick>,
    /// An agent's choices are the user's (`--by-user`, d0087).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub by_user: bool,
}

stored_enum! {
    /// A field whose changes are recorded.
    pub enum Field ("field") {
        Title = "title",
        Body = "body",
        Priority = "priority",
        BaseBranch = "base_branch",
        ExternalId = "external_id",
    }
}

/// One recorded change: the before/after view of agent edits reads these (TKT-05).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldChange {
    pub field: Field,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
}

stored_enum! {
    /// Kinds of rows in the `events` log.
    pub enum EventKind ("event kind") {
        Created = "created",
        Updated = "updated",
        Moved = "moved",
        Pinned = "pinned",
        Unpinned = "unpinned",
        DependencyAdded = "dependency_added",
        DependencyRemoved = "dependency_removed",
        MovedToEpic = "moved_to_epic",
        Closed = "closed",
        AgentEditsSeen = "agent_edits_seen",
        /// The start sequence finished: the agent is working (SES-01).
        Started = "started",
        /// The start stopped at a step (SES-02).
        StartInterrupted = "start_interrupted",
        /// Held in Queued: the base diverged (d0059).
        Held = "held",
        /// The user clicked Retry start.
        StartRetried = "start_retried",
        /// Something worth knowing about a start that didn't stop it (a dirty checkout left the base behind).
        StartNote = "start_note",
        /// `otaq stage next` (or `review submit`, or the user's Move to next stage) into a human check: the payload
        /// has the summary (T-0072; Q263).
        ReviewSubmitted = "review_submitted",
        /// An Agent stage ended with `otaq stage next` into anything but a human check (an agent check, the wait
        /// before another Agent stage; Q263, AGM-10): the payload has the stage's name (`stage`), its id
        /// (`stage_id`) and the summary.
        StageFinished = "stage_finished",
        /// Send back: the comments and note went to the agent; the payload has the round (T-0086).
        SentBack = "sent_back",
        /// Local merge: Done · Merged; the payload has the target and the squash commit (T-0089).
        Merged = "merged",
        /// A Delivery's conflict went to the nearest earlier Agent stage with `restack`, no click (FLW-17): payload
        /// `target`, `to`.
        SentToFix = "sent_to_fix",
        /// Send back with PR comments (FLW-16): payload `pr` (its number), `note`, `to`. History reads "Sent back
        /// with PR comments".
        SentBackWithPrComments = "sent_back_with_pr_comments",
        /// Reopen: Done · Closed → Backlog (T-0101).
        Reopened = "reopened",
        /// Park: Dev → Backlog with the work kept (T-0104); payload `from`, `to` and `stage_id`, the stage it left.
        Parked = "parked",
        /// The rebase of a reopened ticket's branch onto the current base, on its way to Dev (GIT-06).
        Rebased = "rebased",
        /// The agent's session ended while it was at work: Interrupted (REC-01). The payload has the reason
        /// (`restart` or the session's end) and the status before.
        Interrupted = "interrupted",
        /// A Ready for Review or Approved ticket's session ended with a restart; not Interrupted (REC-01, d0045).
        SessionEnded = "session_ended",
        /// The user clicked Resume, Resume all or Retry (REC-01, REC-03).
        Resumed = "resumed",
        /// The agent tool couldn't find the conversation to resume (REC-03); the payload has the tool session id.
        ResumeFailed = "resume_failed",
        /// Start from scratch after a failed resume: a new conversation in the same worktree; the payload has the
        /// old tool session id (REC-03).
        StartedFromScratch = "started_from_scratch",
        /// "This is the usage limit" (USG-07): the user folded an Idle agent into the usage-limit banner.
        LimitMarked = "limit_marked",
        /// `otaq branch rename` (GIT-07): the payload has `from` and `to`.
        BranchRenamed = "branch_renamed",
        /// The Open PR / Push skill proposed the PR (REV-10): the payload has the branch and the title.
        PrProposed = "pr_proposed",
        /// The skill reported the PR (`otaq pr opened <url>`): the payload has `url` and `number`.
        PrOpened = "pr_opened",
        /// The user confirmed the PR in the dialog; Otaq applied the edits (REV-10). Payload: kind, branch,
        /// title.
        PrConfirmed = "pr_confirmed",
        /// The user cancelled the Open PR / Push to PR flow before anything was pushed.
        PrCancelled = "pr_cancelled",
        /// An agent asked a linked ticket (MSG-01); logged on both: payload `id`, `to` or `from`, `question`.
        QuestionAsked = "question_asked",
        /// The question was answered; logged on both: payload `id`, `with`, `answer`.
        QuestionAnswered = "question_answered",
        /// A validation round ended (d0063): payload `round`, `outcome`, `counted`; its report is in `validations`.
        ValidationEnded = "validation_ended",
        /// An automatic local Delivery merged the ticket (FLW-15, AMG-02): payload `target`, `without_checks`.
        MergedAutomatically = "merged_automatically",
        /// The Delivery stopped (FLW-17; AMG-03, AMG-04): payload `stop`, `message`.
        AutoMergeStopped = "auto_merge_stopped",
        /// An epic's queue mode changed (d0080): payload `from`, `to` (`none` / `whole` / `single`). History reads
        /// "Queued the whole epic", "Queued one child" or "Taken back".
        EpicQueueModeSet = "epic_queue_mode_set",
        /// A new child went straight to Ready for Dev because its epic is queued as a whole (EPC-13): payload
        /// `epic`. History reads "Joined the queue: SHOP-40 is queued as a whole".
        JoinedQueue = "joined_queue",
        /// `otaq decision record` (DEC-02, d0084): payload `ordinal`, `question`, `built`, `other_options`, `why`.
        /// History reads "Recorded a decision: <question>" · "Built: … · Other option: …".
        DecisionRecorded = "decision_recorded",
        /// Approve kept the ticket's open decisions (DEC-05): payload `count`, `decisions` (`ordinal`, `question`,
        /// `built`). History reads "Kept by you · 1 decision" · "Split fullName … → only in the profile form".
        DecisionsKept = "decisions_kept",
        /// Change… on a decision (DEC-06): payload `ordinal`, `question`, `to` (`sent_back` with `text` and
        /// `round`, or `fix_child` with `child`, `title` and `epic`). History reads "Changed by you → sent back" or
        /// "Changed by you → SHOP-40.7".
        DecisionChanged = "decision_changed",
        /// The epic reached Ready for Review and its lead ended (d0083), logged on the epic: payload `lead` (its
        /// name). History reads "Lead ended · epic in review".
        LeadEnded = "lead_ended",
        /// An epic lead changed one of its epic's children (CLI-07, d0083), on the **epic**: payload `lead` (its
        /// name), `child`, `change` (`updated`, `dep_added`, `dep_removed`, `created`) and `fields` (updated) or
        /// `parent` (links) or `title` (created). History reads "Updated SHOP-40.3 · by Wishlist lead ·
        /// description", "SHOP-40.3 waits for SHOP-40.4 · by Wishlist lead". The child's own event (`updated`,
        /// `dependency_added`, …) carries `lead` too.
        LeadChanged = "lead_changed",
        /// The user confirmed or dismissed a lead's suggestion (LEAD-08), on the child and the epic: payload
        /// `lead`, `child`, `kind` (close / delete), `outcome` (closed / deleted / dismissed) and `reason`.
        /// History reads "Suggested closing SHOP-40.6 · closed by you".
        LeadSuggestion = "lead_suggestion",
        /// The lead marked a recorded decision (DEC-04): payload `ordinal`, `lead`, `source`, `text`. History reads
        /// "Marked by Wishlist lead · contradicts PAY-123: “…”".
        DecisionMarked = "decision_marked",
        /// A child's question and its epic's lead (LEAD-06), on the child and the epic: payload `id` (the
        /// question's), `child`, `lead` (its name), `step` (`asked`: routed to the lead; `answered` with `answer`;
        /// `passed_on` with `reason` and `recommended`, the option's label; `handed_back`: to the user, no answer
        /// from the lead) and `question`. History reads "SHOP-40.2 asked Wishlist lead", "Wishlist lead answered
        /// SHOP-40.2", "Wishlist lead passed SHOP-40.2's question on to you", "SHOP-40.2's question handed back to
        /// you · no answer from Wishlist lead".
        LeadQuestion = "lead_question",
        /// The lead left a note for a running child (LEAD-10, d0083), on the child and the epic: payload `lead`,
        /// `child`, `text`. History reads "Note from Wishlist lead" · "to SHOP-40.4: “…”".
        LeadNote = "lead_note",
        /// A stuck child and its epic's lead (LEAD-07), on the child and the epic: payload `child`, `lead` (its
        /// name), `stop` (validation_failed / keeps_conflicting / idle), `reason` ("Validation failed 3 times") and
        /// `step` (`held`: the first stop went to the lead; `noted`: the lead's note answered it; `handed_back`: it
        /// went to the user, no note from the lead). History reads "SHOP-40.3 stopped · sent to Wishlist lead",
        /// "Wishlist lead sent its note to SHOP-40.3", "SHOP-40.3's stop handed back to you".
        LeadStuck = "lead_stuck",
        /// The project's Template refresh command failed at this ticket's Done (M19-d, d0087): payload `message`.
        /// Done went on. History reads "Template refresh failed" · its first line.
        TemplateRefreshFailed = "template_refresh_failed",
        /// Another Review ticket adds files under the same schema path (M19-e): payload `other`, `path` (the
        /// schema-path glob) and `files` (what `other` adds under it). Recorded once per new overlap; files are never
        /// touched. History reads "Schema path shared with SHOP-21" · "db/migrations/**".
        SchemaOverlap = "schema_overlap",
        /// The fetch before a start or a Local merge failed and the ticket went on from the local base branch
        /// (d0121): payload `went_on` (`started` / `merged`), `branch`, `remote`, `reason` (git's). History reads
        /// "Couldn't update main from origin · started from your local main" · the reason.
        RemoteUnreachable = "remote_unreachable",
        /// A ticket's or an epic's agent row changed (AGT-04, d0086; rows went with M32, AGM-02, and History still
        /// reads the ones recorded before): payload `role`, `agent` (the name now; null = emptied), `from` (the name
        /// before), `set_by` (`user` / `agent` with `picked_by` and `reason`) and, logged on an epic for a child,
        /// `child`. History reads "Developer: Strong (set by you)", "Wishlist lead re-picked SHOP-40.3: Strong →
        /// Light".
        AgentRowSet = "agent_row_set",
        /// A held session started on its tool's fallback, by the user's click (AGM-03): payload `role` (the session
        /// role), `tool` and `model` (the fallback it runs on), `from_tool` and `reason` (`tool_limit`,
        /// `tool_unavailable`, `model_unavailable`). History reads "Started on Codex · Claude Code limit".
        FallbackUsed = "fallback_used",
        /// A ticket's stage choice changed (AGM-08): payload `stage` (its name), `stage_id`, `choice` ("Quick
        /// developer", "skip", "required"), `from` (before; null = nobody had chosen), `set_by` (`user`,
        /// `project_agent`, `epic_lead`, `agent`), `by` ("you", "the Project agent", "Wishlist lead", "Griller"),
        /// `reason` (an agent's) and, logged on an epic for a child, `child`. History reads "Dev: Quick developer
        /// (set by the Project agent)" · "typo fix".
        StageChoiceSet = "stage_choice_set",
        /// The user's Pass on a human check that moved the card on (FLW-14, d0108): payload `stage` (its name),
        /// `stage_id`. History reads "Passed QA" · "by you". A Pass that offered the delivery choices logs the
        /// delivery's own event instead.
        Passed = "passed",
        /// A stage started without the agent the ticket chose for it, which the flow no longer offers (FLW-05):
        /// payload `stage` (its name), `stage_id`, `chose` and `runs` (the stage's own agent). History reads "Quick
        /// developer is no longer an option for Dev · runs Rust developer".
        ChoiceDropped = "choice_dropped",
        /// The user dragged the card to another stage (FLW-18): payload `stage` (where it went, by name),
        /// `stage_id`, `from` (the stage it left, by name), `back` (true for a drag back), `skipped` (the names of
        /// the stages a forward drag walked over), `run_by_you` (a drag back into a stage the ticket skips by its
        /// choice: it runs once) and `note` (the instructions or the check's note). History reads "Moved back to
        /// Dev" · "Grill · run by you" (or the note, or "by you"), "Moved to Review" · "Skipped Validate · by you".
        Dragged = "dragged",
        /// A ticket comment was posted (CMT-14, d0109): payload `n`, `excerpt` (the start of its text), `report`
        /// (`bytes`, `lines`, or null) and `author` (as [`super::ticket_comments::CommentAuthor`]).
        CommentPosted = "comment_posted",
        /// A ticket comment's text changed: payload `n`, `before`, `after` and `author` (the comment's).
        CommentEdited = "comment_edited",
        /// A ticket comment was deleted: payload `n`, `body` (what it said), `report` and `author`; its report stays.
        CommentDeleted = "comment_deleted",
    }
}

/// One entry of a ticket's history (the `events` log): what happened, who did it, when (History tab).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryEvent {
    pub kind: EventKind,
    pub actor: Actor,
    pub at: i64,
    /// The event's details (moves: from/to; starts: session and tool session id; …).
    #[serde(default)]
    pub payload: Value,
}

/// A `tickets` row with its project's key.
#[derive(Debug, Clone, FromRow)]
pub(crate) struct TicketRecord {
    #[sqlx(flatten)]
    pub row: TicketRow,
    pub project_key: String,
}

pub(crate) const SELECT_TICKETS: &str =
    "SELECT t.*, p.key AS project_key FROM tickets t JOIN projects p ON p.id = t.project_id";

impl Board {
    /// Creates a ticket or epic in Backlog, or an epic child when `epic` is set.
    pub async fn create_ticket(
        &self,
        input: &NewTicketInput,
        actor: &Actor,
    ) -> Result<Ticket, BoardError> {
        let project = self.project(&input.project).await?;
        super::projects::refuse_in_flow_setup(&project)?;
        let title = required_title(&input.title)?;
        let epic = match &input.epic {
            Some(epic) => Some(self.epic_for_child(&project, epic, input).await?),
            None => None,
        };
        let base_branch = match (&epic, &input.base_branch) {
            (Some(_), _) => None,
            (None, Some(b)) if !b.trim().is_empty() => Some(b.trim().to_owned()),
            (None, _) => Some(project.base_branch.clone()),
        };
        let external_id = non_empty(input.external_id.as_deref());
        // The first comment's text is checked before anything is created; its report is copied once the key is known.
        let first_comment = match &input.first_comment {
            Some(c) => Some((
                super::ticket_comments::check_body(&c.body)?,
                c.report.clone(),
            )),
            None => None,
        };
        // A child of an epic queued as a whole joins the queue on its own (EPC-13, d0080): the user's drag of the
        // epic is the go-ahead, whoever creates the child (the user, an agent, a Project agent).
        let joins_queue = epic.as_ref().is_some_and(joins_epic_queue);
        let status = if joins_queue {
            Status::Queued
        } else {
            Status::Backlog
        };

        let now = now_ms();
        let id = new_id();
        let mut tx = self.store.begin_write().await?;
        let (number, child_number) = match &epic {
            Some(epic) => (
                epic.row.number,
                allocate_child_number(&mut tx, &epic.row.id).await?,
            ),
            None => (allocate_number(&mut tx, &project.id).await?, 0),
        };
        sqlx::query(
            "INSERT INTO tickets (id, project_id, number, epic_id, child_number, kind, title, body, priority,
                                  external_id, stage_id, state, base_branch, created_by, created_at,
                                  updated_at, started_at, queued_at, branch_locked, joined_queue_by)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&project.id)
        .bind(number)
        .bind(epic.as_ref().map(|e| &e.row.id))
        .bind(child_number)
        .bind(input.kind.as_str())
        .bind(&title)
        .bind(&input.body)
        // New tickets and epics are Normal; a child without one follows its epic (d0065).
        .bind(
            input
                .priority
                .or(epic.is_none().then_some(Priority::Normal))
                .map(|p| i64::from(p.number())),
        )
        .bind(&external_id)
        // Backlog, or waiting before the first Agent stage: no stage yet.
        .bind(None::<&str>)
        .bind(status.as_str())
        .bind(&base_branch)
        .bind(actor.to_string())
        .bind(now)
        .bind(now)
        // Queued as by a drag to Ready for Dev (`Board::start`): first start, queue time and the base lock.
        .bind(joins_queue.then_some(now))
        .bind(joins_queue.then_some(now))
        .bind(joins_queue)
        .bind(joins_queue.then(|| actor.to_string()))
        .execute(&mut *tx)
        .await?;
        record_event(
            &mut tx,
            &id,
            EventKind::Created,
            actor,
            json!({"kind": input.kind, "title": title}),
        )
        .await?;
        if let (Some(epic), true) = (&epic, joins_queue) {
            sqlx::query("UPDATE tickets SET branch_locked = TRUE WHERE id = ?")
                .bind(&epic.row.id)
                .execute(&mut *tx)
                .await?;
            let epic_key = key_from_parts(&epic.project_key, epic.row.number, 0);
            record_event(
                &mut tx,
                &id,
                EventKind::JoinedQueue,
                actor,
                json!({"epic": epic_key, "from": Status::Backlog, "to": Status::Queued}),
            )
            .await?;
        }
        let stored = match first_comment {
            Some((body, report)) => {
                let key = key_from_parts(&project.key, number, child_number)
                    .ok_or_else(|| BoardError::NotFound(format!("no key for ticket {id}")))?;
                Some(
                    self.post_first_comment(&mut tx, &project, &id, &key, &body, report, actor)
                        .await?,
                )
            }
            None => None,
        };
        if let Err(e) = tx.commit().await {
            if let Some(Some(stored)) = &stored {
                stored.discard();
            }
            return Err(e.into());
        }
        let ticket = self.ticket_by_id(&id).await?;
        tracing::info!(key = %ticket.key, actor = %actor, "ticket created");
        // A new child returns an epic in Review to Backlog until it is merged (EPC-05).
        if let Some(epic) = &ticket.epic {
            self.epic_back_to_backlog(&epic.to_string(), actor).await?;
        }
        Ok(ticket)
    }

    /// Posts a new ticket's first comment inside its creation's transaction (RPT-03): the user's, "at creation", with
    /// its report copied first. Returns the stored report, which the caller discards when the transaction fails.
    #[allow(clippy::too_many_arguments)]
    async fn post_first_comment(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        project: &Project,
        ticket_id: &str,
        key: &TicketKey,
        body: &str,
        report: Option<super::reports::ReportSource>,
        actor: &Actor,
    ) -> Result<Option<super::reports::StoredReport>, BoardError> {
        let stored = match report {
            Some(source) => {
                let repo = crate::warm::store::repo_name(project);
                Some(self.prepare_report(&repo, key, source).await?)
            }
            None => None,
        };
        let author = super::ticket_comments::CommentAuthor::User {
            name: actor.to_string(),
        };
        let posted = super::ticket_comments::post_comment_in(
            tx,
            ticket_id,
            &author,
            body,
            stored.as_ref(),
            true,
            actor,
        )
        .await;
        if let Err(e) = posted {
            if let Some(stored) = &stored {
                stored.discard();
            }
            return Err(e);
        }
        Ok(stored)
    }

    /// Changes a ticket's fields and records each change with its actor.
    pub async fn update_ticket(
        &self,
        key: &str,
        update: &TicketUpdate,
        actor: &Actor,
    ) -> Result<Ticket, BoardError> {
        self.update_ticket_by(key, update, actor, None).await
    }

    /// [`Self::update_ticket`] naming who made the change in History (CMT-14): an agent's or a lead's edit records its
    /// comment author ("Griller · Grill edited the description"); the user's needs none.
    pub async fn update_ticket_by(
        &self,
        key: &str,
        update: &TicketUpdate,
        actor: &Actor,
        author: Option<&super::ticket_comments::CommentAuthor>,
    ) -> Result<Ticket, BoardError> {
        let ticket = self.ticket(key).await?;
        let mut changes = Vec::new();

        let mut title = ticket.title.clone();
        if let Some(new) = &update.title {
            let new = required_title(new)?;
            if new != title {
                changes.push(change(Field::Title, Some(&title), Some(&new)));
                title = new;
            }
        }
        let mut body = ticket.body.clone();
        if let Some(new) = &update.body
            && *new != body
        {
            changes.push(change(Field::Body, Some(&body), Some(new)));
            body = new.clone();
        }

        // The ticket's own priority; `None` while an epic child follows its epic. Pinning never changes it.
        let mut priority = (!ticket.priority_inherited).then_some(ticket.priority);
        if let Some(new) = update.priority
            && priority != Some(new)
        {
            changes.push(change(
                Field::Priority,
                Some(&ticket.priority.to_string()),
                Some(&new.to_string()),
            ));
            priority = Some(new);
        }

        let mut base_branch = match &ticket.base {
            Base::Branch(b) => Some(b.clone()),
            Base::Epic(_) => None,
        };
        if let Some(new) = &update.base_branch {
            let new = new.trim();
            if let Base::Epic(epic) = &ticket.base {
                return Err(BoardError::refused(
                    Refusal::BaseFromEpic,
                    format!("{} starts from the epic's branch ({epic})", ticket.key),
                ));
            }
            if new.is_empty() {
                return Err(BoardError::refused(
                    Refusal::NoBaseBranch,
                    "The base branch can't be empty",
                ));
            }
            if base_branch.as_deref() != Some(new) {
                if ticket.base_locked {
                    return Err(BoardError::refused(
                        Refusal::BaseLocked,
                        format!(
                            "{} has started; its base branch can't change any more",
                            ticket.key
                        ),
                    ));
                }
                self.check_base_change(&ticket, &Base::Branch(new.to_owned()))
                    .await?;
                changes.push(change(Field::BaseBranch, base_branch.as_deref(), Some(new)));
                base_branch = Some(new.to_owned());
            }
        }

        let mut external_id = ticket.external_id.clone();
        if let Some(new) = &update.external_id {
            let new = non_empty(Some(new));
            if new != external_id {
                if ticket.epic.is_some() {
                    return Err(BoardError::refused(
                        Refusal::ExternalIdOnChild,
                        "Epic children have no External ID; set it on the epic",
                    ));
                }
                changes.push(change(
                    Field::ExternalId,
                    external_id.as_deref(),
                    new.as_deref(),
                ));
                external_id = new;
            }
        }

        if changes.is_empty() {
            return Ok(ticket);
        }
        let mut tx = self.store.begin_write().await?;
        sqlx::query(
            "UPDATE tickets SET title = ?, body = ?, priority = ?, base_branch = ?, external_id = ?,
                                updated_by = ?, updated_at = ?
             WHERE id = ?",
        )
        .bind(&title)
        .bind(&body)
        .bind(priority.map(|p| i64::from(p.number())))
        .bind(&base_branch)
        .bind(&external_id)
        .bind(actor.to_string())
        .bind(now_ms())
        .bind(&ticket.id)
        .execute(&mut *tx)
        .await?;
        record_event(
            &mut tx,
            &ticket.id,
            EventKind::Updated,
            actor,
            match author {
                Some(author) => json!({ "changes": changes, "author": author }),
                None => json!({ "changes": changes }),
            },
        )
        .await?;
        tx.commit().await?;
        self.ticket_by_id(&ticket.id).await
    }

    /// A ticket by key (`SHOP-18`, `SHOP-40.1`), or by an old key kept after Move to epic (d0044).
    pub async fn ticket(&self, key: &str) -> Result<Ticket, BoardError> {
        let id = self.ticket_id(key).await?;
        self.ticket_by_id(&id).await
    }

    /// The id of the ticket a key (or an alias) names.
    pub async fn ticket_id(&self, key: &str) -> Result<String, BoardError> {
        let unknown = || BoardError::NotFound(format!("unknown ticket {key}"));
        let parsed: TicketKey = key
            .parse()
            .map_err(|e: crate::domain::KeyError| BoardError::NotFound(e.to_string()))?;
        let found: Option<String> = sqlx::query_scalar(
            "SELECT t.id FROM tickets t JOIN projects p ON p.id = t.project_id
             WHERE p.key = ? AND t.number = ? AND t.child_number = ?",
        )
        .bind(parsed.project.as_str())
        .bind(i64::from(parsed.number))
        .bind(i64::from(parsed.child.unwrap_or(0)))
        .fetch_optional(self.store.pool())
        .await?;
        if let Some(id) = found {
            return Ok(id);
        }
        sqlx::query_scalar("SELECT ticket_id FROM ticket_key_aliases WHERE old_key = ?")
            .bind(parsed.to_string())
            .fetch_optional(self.store.pool())
            .await?
            .ok_or_else(unknown)
    }

    /// The ticket's history, newest first (History tab, d0045).
    pub async fn history(&self, key: &str) -> Result<Vec<HistoryEvent>, BoardError> {
        let id = self.ticket_id(key).await?;
        let rows: Vec<(String, String, i64, String)> = sqlx::query_as(
            "SELECT kind, actor, at, payload_json FROM events WHERE ticket_id = ? ORDER BY at DESC, id DESC",
        )
        .bind(id)
        .fetch_all(self.store.pool())
        .await?;
        Ok(rows
            .into_iter()
            .filter_map(|(kind, actor, at, payload)| {
                Some(HistoryEvent {
                    kind: kind.parse().ok()?,
                    actor: actor.parse().expect("infallible"),
                    at,
                    payload: serde_json::from_str(&payload).unwrap_or(Value::Null),
                })
            })
            .collect())
    }

    pub async fn ticket_by_id(&self, id: &str) -> Result<Ticket, BoardError> {
        let sql = format!("{SELECT_TICKETS} WHERE t.id = ?");
        let record: TicketRecord = sqlx::query_as(sqlx::AssertSqlSafe(sql))
            .bind(id)
            .fetch_optional(self.store.pool())
            .await?
            .ok_or_else(|| BoardError::NotFound(format!("unknown ticket id {id}")))?;
        let project_id = record.row.project_id.clone();
        // Boxed: assembling is a large future, and nearly every request awaits it (keeps callers' frames small).
        let mut tickets = Box::pin(self.assemble(vec![record], &project_id)).await?;
        Ok(tickets.remove(0))
    }

    /// Every ticket and epic of a project, by key.
    pub async fn tickets(&self, project_key: &str) -> Result<Vec<Ticket>, BoardError> {
        let project = self.project(project_key).await?;
        let sql =
            format!("{SELECT_TICKETS} WHERE t.project_id = ? ORDER BY t.number, t.child_number");
        let records: Vec<TicketRecord> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
            .bind(&project.id)
            .fetch_all(self.store.pool())
            .await?;
        let mut tickets = Box::pin(self.assemble(records, &project.id)).await?;
        // Derived from the whole project: the "Ready to start" hint of Backlog tickets (d0066).
        let ready: Vec<bool> = tickets
            .iter()
            .map(|t| super::readiness::ready_to_start(t, &tickets))
            .collect();
        for (ticket, ready) in tickets.iter_mut().zip(ready) {
            ticket.flags.ready_to_start = ready;
        }
        Ok(tickets)
    }

    /// Turns rows of one project into tickets, with their dependencies.
    async fn assemble(
        &self,
        records: Vec<TicketRecord>,
        project_id: &str,
    ) -> Result<Vec<Ticket>, BoardError> {
        let pool = self.store.pool();
        // Every ticket of the project by id, for epic and dependency keys (links stay within a project).
        let keys: Vec<(String, String, i64, i64, Option<i64>)> = sqlx::query_as(
            "SELECT t.id, p.key, t.number, t.child_number, t.priority
             FROM tickets t JOIN projects p ON p.id = t.project_id WHERE t.project_id = ?",
        )
        .bind(project_id)
        .fetch_all(pool)
        .await?;
        let deps: Vec<(String, String)> = sqlx::query_as(
            "SELECT d.ticket_id, d.depends_on_id FROM ticket_deps d
             JOIN tickets t ON t.id = d.ticket_id WHERE t.project_id = ?",
        )
        .bind(project_id)
        .fetch_all(pool)
        .await?;
        let aliases: Vec<(String, String)> = sqlx::query_as(
            "SELECT a.ticket_id, a.old_key FROM ticket_key_aliases a
             JOIN tickets t ON t.id = a.ticket_id WHERE t.project_id = ? ORDER BY a.old_key",
        )
        .bind(project_id)
        .fetch_all(pool)
        .await?;
        let waiting: std::collections::HashMap<String, i64> = sqlx::query_as(
            "SELECT s.ticket_id, COUNT(*) FROM pending_deliveries d JOIN sessions s ON s.id = d.session_id
             WHERE s.project_id = ? AND s.ticket_id IS NOT NULL GROUP BY s.ticket_id",
        )
        .bind(project_id)
        .fetch_all(pool)
        .await?
        .into_iter()
        .collect();
        // An automatic Delivery under way (FLW-15): its PR flow runs with no dialog, so the card waits for nobody.
        let delivering: std::collections::HashSet<String> = sqlx::query_scalar(
            "SELECT f.ticket_id FROM pr_flows f JOIN tickets t ON t.id = f.ticket_id
             WHERE t.project_id = ? AND f.automatic",
        )
        .bind(project_id)
        .fetch_all(pool)
        .await?
        .into_iter()
        .collect();
        // "answering SHOP-40.2" (MSG-03): the asker of the question a running answer session serves.
        let answering: std::collections::HashMap<String, String> = sqlx::query_as(
            "SELECT m.to_ticket_id, m.from_ticket_id FROM messages m
             JOIN tickets t ON t.id = m.to_ticket_id
             WHERE t.project_id = ? AND m.delivery = 'resumed' AND m.answer IS NULL
               AND EXISTS (SELECT 1 FROM sessions s WHERE s.ticket_id = m.to_ticket_id AND s.kind = 'answer'
                                                     AND s.state = 'running')
             ORDER BY m.asked_at DESC",
        )
        .bind(project_id)
        .fetch_all(pool)
        .await?
        .into_iter()
        .collect();
        // "asked SHOP-40.1 · waiting" (MSG-06): open questions from each ticket, oldest first, until the answer was
        // typed into the asker's session.
        let mut asked: std::collections::HashMap<String, (String, String, u32)> =
            std::collections::HashMap::new();
        let open: Vec<(String, String, String)> = sqlx::query_as(
            "SELECT m.from_ticket_id, m.to_ticket_id, m.question FROM messages m
             JOIN tickets t ON t.id = m.from_ticket_id
             WHERE t.project_id = ? AND m.answer_delivered_at IS NULL ORDER BY m.asked_at, m.id",
        )
        .bind(project_id)
        .fetch_all(pool)
        .await?;
        for (from, to, question) in open {
            asked
                .entry(from)
                .and_modify(|(_, _, more)| *more += 1)
                .or_insert((to, question, 0));
        }
        // Sent comments per ticket and round, for "Sent back · 3 comments · round 2".
        let sent_comments: std::collections::HashMap<(String, i64), i64> = sqlx::query_as(
            "SELECT c.ticket_id, c.round, COUNT(*) FROM review_comments c JOIN tickets t ON t.id = c.ticket_id
             WHERE t.project_id = ? AND c.sent_at IS NOT NULL GROUP BY c.ticket_id, c.round",
        )
        .bind(project_id)
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|(id, round, n): (String, i64, i64)| ((id, round), n))
        .collect();
        // Decisions Change… sent back in the latest round, for "Sent back · 1 decision changed" (DEC-06).
        let changed_decisions: std::collections::HashMap<String, i64> = sqlx::query_as(
            "SELECT d.ticket_id, COUNT(*) FROM decisions d JOIN tickets t ON t.id = d.ticket_id
             WHERE t.project_id = ? AND d.outcome = 'changed_sent_back' AND d.outcome_at = t.sent_back_at
             GROUP BY d.ticket_id",
        )
        .bind(project_id)
        .fetch_all(pool)
        .await?
        .into_iter()
        .collect();
        // Open recorded decisions per ticket, an epic's with its children's, for the card's "N decisions" in Ready
        // for Review (DEC-03).
        let mut open_decisions: std::collections::HashMap<String, i64> =
            std::collections::HashMap::new();
        let open_rows: Vec<(String, Option<String>, i64)> = sqlx::query_as(
            "SELECT t.id, t.epic_id, COUNT(*) FROM decisions d JOIN tickets t ON t.id = d.ticket_id
             WHERE t.project_id = ? AND d.outcome = 'open' GROUP BY t.id, t.epic_id",
        )
        .bind(project_id)
        .fetch_all(pool)
        .await?;
        for (id, epic, n) in open_rows {
            *open_decisions.entry(id).or_default() += n;
            if let Some(epic) = epic {
                *open_decisions.entry(epic).or_default() += n;
            }
        }
        // The latest stage end of each ticket (into a human check or not), for "Submitted by you" (d0052).
        let submits: std::collections::HashMap<String, (String, String)> = sqlx::query_as(
            "SELECT e.ticket_id, e.actor, e.payload_json FROM events e JOIN tickets t ON t.id = e.ticket_id
             WHERE t.project_id = ? AND e.kind IN (?, ?) ORDER BY e.at, e.id",
        )
        .bind(project_id)
        .bind(EventKind::ReviewSubmitted.as_str())
        .bind(EventKind::StageFinished.as_str())
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|(id, actor, payload): (String, String, String)| (id, (actor, payload)))
        .collect();
        // The last failed round's report of each ticket, for "· 2 findings" and the first finding (d0063).
        // Each ticket's last failed check round and its verdict: the newest comment its checker posted in that round
        // (an agent comment from the check's stage, with the round's number; CMT-05, CMT-10). A later failed round
        // without one shows none, never an older round's.
        let verdicts: std::collections::HashMap<String, Option<String>> = sqlx::query_as(
            "SELECT v.ticket_id,
                    (SELECT c.body FROM ticket_comments c JOIN stage_runs r ON r.validation_id = v.id
                     WHERE c.ticket_id = v.ticket_id AND c.author_kind = 'agent' AND c.stage_id = r.stage_id
                       AND c.round = v.round AND c.deleted_at IS NULL
                     ORDER BY c.n DESC LIMIT 1)
             FROM validations v JOIN tickets t ON t.id = v.ticket_id
             WHERE t.project_id = ? AND v.outcome = ?
             ORDER BY v.started_at, v.id",
        )
        .bind(project_id)
        .bind(crate::domain::ValidationOutcome::Fail.as_str())
        .fetch_all(pool)
        .await?
        .into_iter()
        .collect();
        let mut questions: std::collections::HashMap<String, super::questions::OpenQuestion> =
            std::collections::HashMap::new();
        for open in self.open_questions(project_id).await? {
            questions.entry(open.ticket_id.clone()).or_insert(open);
        }
        let agent_edited: std::collections::HashSet<String> = self
            .tickets_updated_by_agent(project_id)
            .await?
            .into_iter()
            .collect();
        let key_of: std::collections::HashMap<&str, TicketKey> = keys
            .iter()
            .filter_map(|(id, project, number, child, _)| {
                Some((id.as_str(), key_from_parts(project, *number, *child)?))
            })
            .collect();
        // Own priorities by id: an epic child without one follows its epic (TKT-09).
        let own_priority: std::collections::HashMap<&str, Priority> = keys
            .iter()
            .filter_map(|(id, _, _, _, p)| Some((id.as_str(), Priority::from_number((*p)?)?)))
            .collect();

        // "1 unread note from Wishlist lead" (LEAD-10).
        let unread_notes = crate::epic_lead::notes::unread_flags(self, project_id).await?;
        // "Lead's note sent" (LEAD-07).
        let lead_notes_sent = crate::epic_lead::stuck::note_flags(self, project_id).await?;
        // "Updated by lead", "new · added by Wishlist lead" (LEAD-08).
        let by_lead = crate::epic_lead::changes::by_lead_flags(self, project_id).await?;
        // "round 1 of 3": the rounds of the agent check each ticket counts, from the project's flow (d0063).
        let project_key: Option<String> =
            sqlx::query_scalar("SELECT key FROM projects WHERE id = ?")
                .bind(project_id)
                .fetch_optional(pool)
                .await?;
        let flow = match project_key {
            Some(key) => self.effective_flow(&key).await?,
            None => crate::flow::default_flow(),
        };
        // Each agent check's own counter (FLW-13).
        let check_counts = super::validation::project_check_counts(pool, project_id).await?;
        // How cards came to wait in a human check or at their Delivery (FLW-14).
        let mut arrivals = self.project_arrivals(project_id, &flow).await?;

        records
            .into_iter()
            .map(|record| {
                let row = record.row;
                let key = key_from_parts(&record.project_key, row.number, row.child_number)
                    .ok_or_else(|| BoardError::NotFound(format!("bad key in ticket {}", row.id)))?;
                let epic = row
                    .epic_id
                    .as_deref()
                    .and_then(|id| key_of.get(id).cloned());
                let base = match (&epic, &row.base_branch) {
                    (Some(epic), _) => Base::Epic(epic.clone()),
                    (None, Some(branch)) => Base::Branch(branch.clone()),
                    (None, None) => Base::Branch(String::new()),
                };
                let mut depends_on: Vec<TicketKey> = deps
                    .iter()
                    .filter(|(child, _)| *child == row.id)
                    .filter_map(|(_, parent)| key_of.get(parent.as_str()).cloned())
                    .collect();
                depends_on.sort();
                let ticket_aliases = aliases
                    .iter()
                    .filter(|(id, _)| *id == row.id)
                    .filter_map(|(_, old)| old.parse().ok())
                    .collect();
                let status: Status =
                    row.state
                        .parse()
                        .map_err(|e: crate::domain::UnknownValue| {
                            BoardError::NotFound(e.to_string())
                        })?;
                let kind: Kind = row.kind.parse().map_err(|e: crate::domain::UnknownValue| {
                    BoardError::NotFound(e.to_string())
                })?;
                let sent_back = row.sent_back_at.map(|_| crate::domain::SentBack {
                    comments: sent_comments
                        .get(&(row.id.clone(), row.sent_back_round))
                        .and_then(|n| u32::try_from(*n).ok())
                        .unwrap_or(0),
                    round: u32::try_from(row.sent_back_round).unwrap_or(0),
                    decisions: changed_decisions
                        .get(&row.id)
                        .and_then(|n| u32::try_from(*n).ok())
                        .unwrap_or(0),
                });
                let submitted_by_you = submits
                    .get(&row.id)
                    .filter(|_| row.state == Status::WaitingForYou.as_str())
                    .filter(|(actor, _)| actor.parse::<Actor>().is_ok_and(|a| !a.is_agent()))
                    // The user's note is their comment now (CMT-05): the card says only that they submitted it.
                    .map(|_| String::new());
                let place = Place::of(
                    status,
                    row.stage_id.as_deref(),
                    row.interrupted_from.as_deref().and_then(|s| s.parse().ok()),
                );
                let counts = check_counts.get(&row.id).cloned().unwrap_or_default();
                let counted = CountedCheck::of(&flow, &place, &counts);
                let merge_check_failed = row.conflicts_with.is_some()
                    && row.conflict_reason.as_deref()
                        == Some(super::conflicts::ConflictReason::MergeCheck.as_str());
                let mut flags = Flags {
                    your_turn: status.is_needs_you_family() && self.focus.covers(&row.id, status),
                    run_port: self.run_ports().get(&row.id),
                    unread_notes: unread_notes.get(&row.id).cloned(),
                    lead_note_sent: lead_notes_sent.get(&row.id).cloned(),
                    by_lead: by_lead.get(&row.id).cloned(),
                    same_file: if crate::epic_lead::overlap::is_running(status) {
                        self.overlaps().same_file(&row.id)
                    } else {
                        Vec::new()
                    },
                    resuming_conversation: (row.parked
                        || row.return_mode.as_deref() == Some(crate::recovery::RETURN_RESUME))
                        && (status.is_queued() || status.at_work()),
                    submitted_by_you,
                    // A failed Merge check isn't a conflict: the card says "Merge check failed · fixing".
                    conflicts_with: row.conflicts_with.clone().filter(|_| !merge_check_failed),
                    conflict_in_review: row.conflicts_with.is_some()
                        && row.conflict_reason.as_deref()
                            == Some(super::conflicts::ConflictReason::TestMerge.as_str()),
                    merge_check: if self.merge_checks().is_running(&row.id) {
                        Some(MergeCheckState::Running)
                    } else if merge_check_failed {
                        Some(MergeCheckState::Failed)
                    } else {
                        None
                    },
                    sent_back,
                    pinned_at: row.pinned_at,
                    pr_number: row.pr_number.and_then(|n| u32::try_from(n).ok()),
                    pr_rework: row.pr_rework && status == Status::WaitingForYou,
                    updated_by_agent: agent_edited.contains(&row.id),
                    last_signal_at: row.last_signal_at,
                    answering: answering
                        .get(&row.id)
                        .and_then(|from| key_of.get(from.as_str()).cloned()),
                    asked: asked
                        .get(&row.id)
                        .and_then(|(to, _, _)| key_of.get(to.as_str()).cloned()),
                    asked_question: asked.get(&row.id).map(|(_, q, _)| q.clone()),
                    asked_more: asked.get(&row.id).map_or(0, |(_, _, more)| *more),
                    messages_waiting: waiting
                        .get(&row.id)
                        .and_then(|n| u32::try_from(*n).ok())
                        .unwrap_or(0),
                    // Only while its agent is at it: a ticket that moved on has nothing to pick up.
                    hand_off: self.hand_offs().kind_of(&row.id).filter(|_| {
                        crate::signals::LIVE.contains(&status)
                            || status == Status::MergeConflictFixing
                    }),
                    review_entry: review_entry_flag(&row, status, &counted),
                    validation_round: failed_round_marker(&row, status, &counted),
                    validating_round: validating_round(status, &counted),
                    check: counted.name.clone(),
                    moved_past_check: counts.moved_past_reason(&flow),
                    arrival: arrivals.remove(&row.id),
                    conflict_files: row
                        .conflict_files
                        .as_deref()
                        .and_then(|f| serde_json::from_str(f).ok())
                        .unwrap_or_default(),
                    merged_automatically: row.merged_automatically && status == Status::Merged,
                    delivered_without_checks: row.delivered_without_checks
                        && status == Status::Merged,
                    delivering_automatically: delivering.contains(&row.id),
                    nothing_to_merge: row
                        .nothing_to_merge
                        .clone()
                        .filter(|_| status == Status::ClosedNotMerged),
                    open_decisions: open_decisions
                        .get(&row.id)
                        .filter(|_| status == Status::WaitingForYou)
                        .and_then(|n| u32::try_from(*n).ok())
                        .unwrap_or(0),
                    asked_lead: questions
                        .get(&row.id)
                        .filter(|_| crate::signals::LIVE.contains(&status))
                        .and_then(|open| {
                            Some(crate::domain::AskedLead {
                                lead: open.with_lead.clone()?,
                                question: open.question.text.clone(),
                            })
                        }),
                    via_lead: questions
                        .get(&row.id)
                        .filter(|_| {
                            status == Status::AsksYou
                                || (status == Status::Interrupted
                                    && row.interrupted_from.as_deref()
                                        == Some(Status::AsksYou.as_str()))
                        })
                        .and_then(|open| Some(open.question.via_lead.as_ref()?.lead.clone())),
                    ..Flags::default()
                };
                if flags.validation_round.is_some() || status == Status::CheckFailed {
                    flags.verdict = verdicts
                        .get(&row.id)
                        .and_then(|body| first_line(body.as_deref()?));
                }
                let port = |p: Option<i64>| p.and_then(|p| u16::try_from(p).ok());
                let work = Work {
                    branch: row.branch_name,
                    worktree: row.worktree_path,
                    base_sha: row.base_sha,
                    agent_port: port(row.agent_port),
                    run_port: port(row.run_port),
                    branch_locked: row.branch_pushed,
                    pr_url: row.pr_url.clone(),
                    build_state: row.build_state.parse().unwrap_or_default(),
                    build_problem: row.build_problem,
                    resource_name: row.resource_name,
                    resource_state: row.resource_state.parse().unwrap_or_default(),
                    schema_overlap: row
                        .schema_overlap
                        .as_deref()
                        .and_then(|j| serde_json::from_str(j).ok())
                        .unwrap_or_default(),
                    remote_warning: row
                        .remote_warning
                        .as_deref()
                        .and_then(|j| serde_json::from_str(j).ok()),
                };
                // While a start runs, its current step is marked here too (no message yet); that isn't a problem.
                let start_problem = row
                    .start_problem_step
                    .as_deref()
                    .filter(|_| matches!(status, Status::Held | Status::StartInterrupted))
                    .and_then(|s| s.parse().ok())
                    .map(|step| StartProblem {
                        step,
                        message: row.start_problem_message.unwrap_or_default(),
                    });
                let interrupted_from = row
                    .interrupted_from
                    .as_deref()
                    .filter(|_| status == Status::Interrupted)
                    .and_then(|s| s.parse::<Status>().ok())
                    .filter(|s| s.is_needs_you_family() || *s == Status::Checking);
                let signal = (status.is_needs_you_family()
                    || status == Status::WorkingInBackground
                    || interrupted_from.is_some())
                .then_some(AgentSignal {
                    text: row.signal_text,
                    since: row.signal_since,
                    wakeup_at: row.wakeup_at,
                });
                let question = (status == Status::AsksYou
                    || interrupted_from == Some(Status::AsksYou))
                .then(|| questions.get(&row.id).map(|open| open.question.clone()))
                .flatten();
                let pushed_differs: Option<crate::pr::verify::PushedDiff> = row
                    .pushed_differs
                    .as_deref()
                    .and_then(|j| serde_json::from_str(j).ok());
                if pushed_differs.is_some() {
                    flags.changed_since_review = true;
                }
                Ok(Ticket {
                    pushed_differs,
                    interrupted_from,
                    question,
                    signal,
                    work: (work != Work::default()).then_some(work),
                    start_problem,
                    key,
                    kind,
                    title: row.title,
                    body: row.body,
                    priority: own_priority
                        .get(row.id.as_str())
                        .or_else(|| row.epic_id.as_deref().and_then(|e| own_priority.get(e)))
                        .copied()
                        .unwrap_or_default(),
                    priority_inherited: row.epic_id.is_some()
                        && !own_priority.contains_key(row.id.as_str()),
                    epic,
                    place,
                    status,
                    base,
                    base_locked: row.branch_locked,
                    external_id: row.external_id,
                    depends_on,
                    aliases: ticket_aliases,
                    next_child_number: (kind == Kind::Epic)
                        .then(|| u32::try_from(row.next_child_number).ok())
                        .flatten(),
                    flags,
                    created_by: row.created_by.parse().expect("infallible"),
                    updated_by: row.updated_by.map(|a| a.parse().expect("infallible")),
                    started_at: row.started_at,
                    created_at: row.created_at,
                    updated_at: row.updated_at,
                    status_since: row.state_since,
                    queued_at: row.queued_at,
                    ready_since: row.ready_since,
                    validation_round: counted.count.shown_round(),
                    auto_merge_conflicts: u32::try_from(row.auto_merge_conflicts).unwrap_or(0),
                    auto_merge_stop: row.auto_merge_stop.as_deref().and_then(|s| s.parse().ok()),
                    epic_queue_mode: (kind == Kind::Epic)
                        .then(|| row.epic_queue_mode.parse().ok())
                        .flatten()
                        .unwrap_or_default(),
                    joined_queue_by: row
                        .joined_queue_by
                        .filter(|_| row.epic_id.is_some())
                        .map(|a| a.parse().expect("infallible")),
                    id: row.id,
                })
            })
            .collect()
    }

    /// Checks that `epic` can take a new child created from `input`.
    async fn epic_for_child(
        &self,
        project: &Project,
        epic: &str,
        input: &NewTicketInput,
    ) -> Result<TicketRecord, BoardError> {
        let epic_ticket = self.ticket(epic).await?;
        if epic_ticket.key.project.as_str() != project.key {
            return Err(BoardError::refused(
                Refusal::OtherProject,
                format!("{} belongs to another project", epic_ticket.key),
            ));
        }
        if !epic_ticket.is_epic() {
            return Err(BoardError::refused(
                Refusal::NotAnEpic,
                format!("{} isn't an epic", epic_ticket.key),
            ));
        }
        if input.kind == Kind::Epic {
            return Err(BoardError::refused(
                Refusal::EpicInEpic,
                "An epic can't be inside another epic",
            ));
        }
        if input
            .base_branch
            .as_deref()
            .is_some_and(|b| !b.trim().is_empty())
        {
            return Err(BoardError::refused(
                Refusal::BaseFromEpic,
                format!(
                    "Epic children start from the epic's branch ({})",
                    epic_ticket.key
                ),
            ));
        }
        if non_empty(input.external_id.as_deref()).is_some() {
            return Err(BoardError::refused(
                Refusal::ExternalIdOnChild,
                "Epic children have no External ID; set it on the epic",
            ));
        }
        let sql = format!("{SELECT_TICKETS} WHERE t.id = ?");
        Ok(sqlx::query_as(sqlx::AssertSqlSafe(sql))
            .bind(&epic_ticket.id)
            .fetch_one(self.store.pool())
            .await?)
    }
}

/// Whether a new child of this epic joins the queue (EPC-13): its epic is queued as a whole and not done.
fn joins_epic_queue(epic: &TicketRecord) -> bool {
    epic.row.epic_queue_mode.parse::<EpicQueueMode>().ok() == Some(EpicQueueMode::Whole)
        && epic
            .row
            .state
            .parse::<Status>()
            .is_ok_and(|s| s.set() != StatusSet::Done)
}

/// `SHOP` + 40 + 1 → `SHOP-40.1`; child number 0 means no child.
pub(crate) fn key_from_parts(project: &str, number: i64, child: i64) -> Option<TicketKey> {
    let project = ProjectKey::new(project).ok()?;
    let number = u32::try_from(number).ok()?;
    Some(match child {
        0 => TicketKey::new(project, number),
        n => TicketKey::child(project, number, u32::try_from(n).ok()?),
    })
}

/// The agent check a ticket's round markers count for (FLW-13): its name, its counter and its rounds (the stage's,
/// with the extra ones the user allowed).
struct CountedCheck {
    name: Option<String>,
    count: super::validation::CheckCount,
    rounds: u32,
}

impl CountedCheck {
    fn of(
        flow: &crate::flow::Flow,
        place: &Place,
        counts: &super::validation::CheckCounts,
    ) -> Self {
        let check = counts.counting(flow, place);
        let count = check.map(|c| counts.of(&c.id)).unwrap_or_default();
        let in_it = check.is_some_and(|c| place.stage() == Some(c.id.as_str()));
        Self {
            name: check
                .filter(|_| in_it || count.fails > 0 || count.passed_round.is_some())
                .map(|c| c.name.clone()),
            rounds: flow.check_rounds(check.map(|c| c.id.as_str()), count.extra),
            count,
        }
    }
}

/// "Validated · round 2" and the like: how the ticket reached Review (d0063). Shown in Review, Approved and Done.
fn review_entry_flag(
    row: &TicketRow,
    status: Status,
    counted: &CountedCheck,
) -> Option<crate::domain::ReviewEntryFlag> {
    let entry: crate::domain::ReviewEntry = row.review_entry.as_deref()?.parse().ok()?;
    matches!(
        status.set(),
        StatusSet::HumanCheck | StatusSet::Delivery | StatusSet::Done
    )
    .then(|| crate::domain::ReviewEntryFlag {
        entry,
        round: counted.count.shown_round(),
    })
}

/// "Validation failed · round 1 of 3": the agent works again after a failed round (VAL-04), counted on the check
/// that failed it. The rounds are that check's; "Give it another round" adds to them.
fn failed_round_marker(
    row: &TicketRow,
    status: Status,
    counted: &CountedCheck,
) -> Option<crate::domain::ValidationRound> {
    let round = Some(counted.count.fails).filter(|r| *r > 0)?;
    let working = matches!(
        status,
        Status::Working
            | Status::WorkingInBackground
            | Status::AsksYou
            | Status::NeedsYou
            | Status::Idle
            | Status::WaitingForLimitReset
    );
    (working && row.review_entry.is_none()).then_some(crate::domain::ValidationRound {
        round,
        of: counted.rounds,
    })
}

/// "validator · round 1 of 3": the round the validator is checking (Validating…, Needs you · validator; d0063).
fn validating_round(
    status: Status,
    counted: &CountedCheck,
) -> Option<crate::domain::ValidationRound> {
    matches!(status, Status::Checking | Status::CheckerNeedsYou).then(|| {
        crate::domain::ValidationRound {
            round: counted.count.fails + 1,
            of: counted.rounds,
        }
    })
}

/// A comment's first line with text, trimmed: what a card shows of a verdict.
fn first_line(body: &str) -> Option<String> {
    body.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_owned)
}

/// Takes the project's next ticket number. Numbers are never handed out twice (d0029).
async fn allocate_number(
    tx: &mut Transaction<'_, Sqlite>,
    project_id: &str,
) -> Result<i64, BoardError> {
    Ok(sqlx::query_scalar(
        "UPDATE projects SET next_ticket_number = next_ticket_number + 1 WHERE id = ?
         RETURNING next_ticket_number - 1",
    )
    .bind(project_id)
    .fetch_one(&mut **tx)
    .await?)
}

/// Takes the epic's next child number, never handed out twice within the epic (d0039).
pub(crate) async fn allocate_child_number(
    tx: &mut Transaction<'_, Sqlite>,
    epic_id: &str,
) -> Result<i64, BoardError> {
    Ok(sqlx::query_scalar(
        "UPDATE tickets SET next_child_number = next_child_number + 1 WHERE id = ?
         RETURNING next_child_number - 1",
    )
    .bind(epic_id)
    .fetch_one(&mut **tx)
    .await?)
}

/// Appends a row to the `events` log.
pub(crate) async fn record_event(
    tx: &mut Transaction<'_, Sqlite>,
    ticket_id: &str,
    kind: EventKind,
    actor: &Actor,
    payload: Value,
) -> Result<(), BoardError> {
    sqlx::query(
        "INSERT INTO events (id, ticket_id, kind, actor, payload_json, at) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(new_id())
    .bind(ticket_id)
    .bind(kind.as_str())
    .bind(actor.to_string())
    .bind(payload.to_string())
    .bind(now_ms())
    .execute(&mut **tx)
    .await?;
    Ok(())
}

fn change(field: Field, before: Option<&str>, after: Option<&str>) -> FieldChange {
    FieldChange {
        field,
        before: before.map(str::to_owned),
        after: after.map(str::to_owned),
    }
}

fn required_title(title: &str) -> Result<String, BoardError> {
    let title = title.trim();
    if title.is_empty() {
        Err(BoardError::refused(
            Refusal::EmptyField,
            "A ticket needs a title",
        ))
    } else {
        Ok(title.to_owned())
    }
}

fn non_empty(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_owned)
}
