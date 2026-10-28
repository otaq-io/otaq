//! Tickets: create, edit, key allocation, fields, actor and the event log (TKT-01, TKT-03; d0019, d0029, d0032,
//! d0051, d0059, d0133).
//!
//! New tickets land in Backlog. Numbers increase per project and are never handed out again, even after a deletion
//! (d0029). Every ticket stores its base branch ("Start from and merge into"), the project's at creation unless
//! overridden (d0051), and may have an External ID (d0059). Every change records its actor and appends an `events`
//! row.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{FromRow, Sqlite, Transaction};

use super::{Board, BoardError, Refusal};
use crate::domain::plan::PlanBy;
use crate::domain::{
    Actor, BuildState, Flags, MergeCheckState, Place, Priority, ProjectKey, RemoteWarning,
    ResourceState, SchemaOverlap, StartStep, Status, StatusSet, TicketKey, stored_enum,
};
use crate::store::projects::Project;
use crate::store::tickets::TicketRow;
use crate::store::{new_id, now_ms};

/// The branch a ticket starts from and merges into (d0051, d0059).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Base {
    /// A branch of the repository.
    Branch(String),
}

impl Base {
    /// The branch's name: `main`.
    pub fn branch(&self) -> &str {
        let Self::Branch(branch) = self;
        branch
    }

    /// How the base reads in messages: `main`.
    pub fn describe(&self) -> String {
        format!("`{}`", self.branch())
    }
}

/// A ticket as the board, the editor and the CLI see it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ticket {
    pub id: String,
    pub key: TicketKey,
    pub title: String,
    /// "What and why" (Markdown).
    pub body: String,
    /// The Plan section (Markdown, d0133): empty when the ticket has none; its pieces are
    /// [`crate::domain::plan::pieces`].
    #[serde(default)]
    pub plan: String,
    /// Who wrote the Plan last and when.
    #[serde(default)]
    pub plan_by: Option<PlanBy>,
    #[serde(default)]
    pub plan_at: Option<i64>,
    /// TKT-09, d0065.
    #[serde(default)]
    pub priority: Priority,
    /// Where it is: Backlog, waiting before an Agent stage, a stage of its flow, or Done (STG-04). In JSON
    /// `"place"` and, in or before a stage, `"stage"` (the stage's id).
    #[serde(flatten)]
    pub place: Place,
    pub status: Status,
    pub base: Base,
    /// The base can't change any more (the ticket has been to Dev).
    pub base_locked: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub external_id: Option<String>,
    /// Direct parents, in key order.
    pub depends_on: Vec<TicketKey>,
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
    /// (`signal`) and the question stay with the ticket, so answering from Needs you resumes and delivers.
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

    /// Has this ticket ever been to Dev? Never-started tickets can be deleted (d0060).
    pub fn has_started(&self) -> bool {
        self.started_at.is_some()
    }

    /// Turquoise as the user sees it: the agent waits and no terminal of the ticket has focus (ATT-03, d0042).
    pub fn shows_needs_you(&self) -> bool {
        self.status.is_needs_you_family() && !self.flags.your_turn
    }

    /// Whether the ticket waits for the user where they can see it: turquoise (unless its terminal has focus) or
    /// Needs you's second group. Such cards float to the top of Dev and count in the project's needs-you number.
    pub fn needs_user(&self) -> bool {
        super::order::needs_user(self.status) && !self.flags.your_turn
    }
}

/// Has this ticket been to Dev (any stage of its flow)? Pure: from its place, its status and its work (branch or
/// worktree). A queued ticket that never reached its first stage has **not** (unlike [`Ticket::has_started`], which
/// a queue sets). One waiting before a later Agent stage ran an earlier one. A Closed ticket counts if it was ever
/// queued and kept that mark, or has work.
pub fn been_to_dev(ticket: &Ticket) -> bool {
    let worked = ticket
        .work
        .as_ref()
        .is_some_and(|w| w.branch.is_some() || w.worktree.is_some());
    match &ticket.place {
        Place::InStage(_) | Place::Waiting(Some(_)) => true,
        Place::Backlog => ticket.status == Status::Parked || worked,
        Place::Waiting(None) => worked,
        Place::Done => ticket.status == Status::Merged || worked || ticket.has_started(),
    }
}

/// A never-started ticket: in Backlog or Ready for Dev, and it has never been to Dev ([`been_to_dev`]).
pub fn is_never_started(ticket: &Ticket) -> bool {
    (ticket.place.is_backlog() || ticket.place.is_waiting()) && !been_to_dev(ticket)
}

/// Creates a ticket in Backlog.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewTicketInput {
    /// The project key.
    pub project: String,
    pub title: String,
    #[serde(default)]
    pub body: String,
    /// The Plan section (d0133); empty for none.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub plan: String,
    /// Default: Normal (d0065).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<Priority>,
    /// Overrides the project's base branch.
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
    /// The Plan section (d0133); an empty string removes it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan: Option<String>,
    /// Sets the ticket's priority.
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
        /// The Plan section (d0133): History keeps every version, as the description's (0109).
        Plan = "plan",
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
        /// `otaq decision record` (DEC-02, d0084): payload `ordinal`, `question`, `built`, `other_options`, `why`.
        /// History reads "Recorded a decision: <question>" · "Built: … · Other option: …".
        DecisionRecorded = "decision_recorded",
        /// Approve kept the ticket's open decisions (DEC-05): payload `count`, `decisions` (`ordinal`, `question`,
        /// `built`). History reads "Kept by you · 1 decision" · "Split fullName … → only in the profile form".
        DecisionsKept = "decisions_kept",
        /// Change… on a decision (DEC-06): payload `ordinal`, `question`, `to` (`sent_back`), `text` and `round`.
        /// History reads "Changed by you → sent back".
        DecisionChanged = "decision_changed",
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
        /// A ticket's agent row changed (AGT-04, d0086; rows went with M32, AGM-02, and History still reads the ones
        /// recorded before): payload `role`, `agent` (the name now; null = emptied), `from` (the name before) and
        /// `set_by` (`user` / `agent` with `picked_by` and `reason`). History reads "Developer: Strong (set by
        /// you)".
        AgentRowSet = "agent_row_set",
        /// A held session started on its tool's fallback, by the user's click (AGM-03): payload `role` (the session
        /// role), `tool` and `model` (the fallback it runs on), `from_tool` and `reason` (`tool_limit`,
        /// `tool_unavailable`, `model_unavailable`). History reads "Started on Codex · Claude Code limit".
        FallbackUsed = "fallback_used",
        /// A ticket's stage choice changed (AGM-08): payload `stage` (its name), `stage_id`, `choice` ("Quick
        /// developer", "skip", "required"), `from` (before; null = nobody had chosen), `set_by` (`user`,
        /// `project_agent`, `agent`), `by` ("you", "Noyan", "Spec writer") and `reason` (an agent's). History reads
        /// "Dev: Quick developer (set by Noyan)" · "typo fix".
        StageChoiceSet = "stage_choice_set",
        /// The user's Pass on a human check that moved the card on (FLW-14, d0108): payload `stage` (its name),
        /// `stage_id`. History reads "You approved it at QA" (d0093). A Pass that offered the delivery choices logs the
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
        /// Dev" · "Product spec · run by you" (or the note, or "by you"), "Moved to Review" · "Skipped Validate · by
        /// you".
        Dragged = "dragged",
        /// One of the ticket's helpers changed (d0133): payload `piece`, `label` ("40.3"), `title`, `what` (`started`,
        /// `held`, `sent_back`, `handed_back`, `merged`) and `note` (the first line of a note or a hand-back). History
        /// reads "40.3 Wishlist page · merged".
        Helper = "helper",
        /// A ticket comment was posted (CMT-14, d0109): payload `n`, `excerpt` (the start of its text), `report`
        /// (`bytes`, `lines`, or null) and `author` (as [`super::ticket_comments::CommentAuthor`]).
        CommentPosted = "comment_posted",
        /// A ticket comment's text changed: payload `n`, `before`, `after` and `author` (the comment's).
        CommentEdited = "comment_edited",
        /// A ticket comment was deleted: payload `n`, `body` (what it said), `report` and `author`; its report stays.
        CommentDeleted = "comment_deleted",
        /// The ticket's agent created another ticket with `otaq create` (#47): logged on the creator, payload
        /// `child` (its key), `title`, `to` (the column it landed in). History reads "Created DEMO-12 Fix the
        /// build" · "by DEMO-7's agent · in Backlog".
        CreatedTicket = "created_ticket",
        /// The user ticked or unticked a manual check (d0125): payload `n`, `text` and `ticked`. History reads
        /// "Ticked a manual check" (or "Unticked a manual check") · the item, quoted.
        ManualCheckTicked = "manual_check_ticked",
        /// The ticket's agent changed or deleted a never-started ticket it created (d0129): logged on the creator,
        /// payload `ticket` (its key) and `change` (as [`super::fixes::Fix`]). History reads "Edited SHOP-42" ·
        /// "title, priority" or "Deleted SHOP-43" · "merged into SHOP-42".
        FixedCreatedTicket = "fixed_created_ticket",
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
    /// Creates a ticket in Backlog.
    pub async fn create_ticket(
        &self,
        input: &NewTicketInput,
        actor: &Actor,
    ) -> Result<Ticket, BoardError> {
        let project = self.project(&input.project).await?;
        super::projects::refuse_in_flow_setup(&project)?;
        let title = required_title(&input.title)?;
        let base_branch = match &input.base_branch {
            Some(b) if !b.trim().is_empty() => b.trim().to_owned(),
            _ => project.base_branch.clone(),
        };
        let external_id = non_empty(input.external_id.as_deref());
        let plan_by = if input.plan.trim().is_empty() {
            None
        } else {
            Some(self.plan_writer(actor, None).await)
        };
        // The first comment's text is checked before anything is created; its report is copied once the key is known.
        let first_comment = match &input.first_comment {
            Some(c) => Some((
                super::ticket_comments::check_body(&c.body)?,
                c.report.clone(),
            )),
            None => None,
        };
        let now = now_ms();
        let id = new_id();
        let mut tx = self.store.begin_write().await?;
        let number = allocate_number(&mut tx, &project.id).await?;
        sqlx::query(
            "INSERT INTO tickets (id, project_id, number, title, body, plan, plan_by, plan_at, priority, external_id,
                                  stage_id, state, base_branch, created_by, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&project.id)
        .bind(number)
        .bind(&title)
        .bind(&input.body)
        .bind(if plan_by.is_some() { input.plan.as_str() } else { "" })
        .bind(plan_by.as_ref().map(ToString::to_string))
        .bind(plan_by.as_ref().map(|_| now))
        // New tickets are Normal (d0065).
        .bind(i64::from(input.priority.unwrap_or_default().number()))
        .bind(&external_id)
        // Backlog: no stage yet.
        .bind(None::<&str>)
        .bind(Status::Backlog.as_str())
        .bind(&base_branch)
        .bind(actor.to_string())
        .bind(now)
        .bind(now)
        .execute(&mut *tx)
        .await?;
        record_event(
            &mut tx,
            &id,
            EventKind::Created,
            actor,
            // A ticket filed with a Plan names its writer, as a Plan edit does.
            match &plan_by {
                Some(by) => json!({"title": title, "plan_by": by.to_string()}),
                None => json!({"title": title}),
            },
        )
        .await?;
        let stored = match first_comment {
            Some((body, report)) => {
                let key = key_from_parts(&project.key, number)
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

    /// [`Self::update_ticket`] naming who made the change in History (CMT-14): an agent's edit records its comment
    /// author ("Spec writer · Product spec edited the description"); the user's needs none.
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
        let mut plan = ticket.plan.clone();
        if let Some(new) = &update.plan {
            let new = if new.trim().is_empty() { "" } else { new };
            if new != plan {
                changes.push(change(Field::Plan, Some(&plan), Some(new)));
                plan = new.to_owned();
            }
        }

        // Pinning never changes the priority.
        let mut priority = ticket.priority;
        if let Some(new) = update.priority
            && priority != new
        {
            changes.push(change(
                Field::Priority,
                Some(&ticket.priority.to_string()),
                Some(&new.to_string()),
            ));
            priority = new;
        }

        let mut base_branch = Some(ticket.base.branch().to_owned());
        if let Some(new) = &update.base_branch {
            let new = new.trim();
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
        let now = now_ms();
        let (plan_by, plan_at) = if plan == ticket.plan {
            (ticket.plan_by.clone(), ticket.plan_at)
        } else if plan.is_empty() {
            (None, None)
        } else {
            (Some(self.plan_writer(actor, author).await), Some(now))
        };
        let mut tx = self.store.begin_write().await?;
        sqlx::query(
            "UPDATE tickets SET title = ?, body = ?, plan = ?, plan_by = ?, plan_at = ?, priority = ?,
                                base_branch = ?, external_id = ?, updated_by = ?, updated_at = ?
             WHERE id = ?",
        )
        .bind(&title)
        .bind(&body)
        .bind(&plan)
        .bind(plan_by.as_ref().map(ToString::to_string))
        .bind(plan_at)
        .bind(i64::from(priority.number()))
        .bind(&base_branch)
        .bind(&external_id)
        .bind(actor.to_string())
        .bind(now)
        .bind(&ticket.id)
        .execute(&mut *tx)
        .await?;
        let mut payload = match author {
            Some(author) => json!({ "changes": changes, "author": author }),
            None => json!({ "changes": changes }),
        };
        // Who wrote the new Plan, as `plan_by` names it ("stage:Milestone plan"): the sheet's "Written by Milestone
        // plan · edited by you" reads the writers back from History (d0133).
        if plan != ticket.plan
            && let Some(by) = &plan_by
        {
            payload["plan_by"] = json!(by.to_string());
        }
        record_event(&mut tx, &ticket.id, EventKind::Updated, actor, payload).await?;
        tx.commit().await?;
        self.ticket_by_id(&ticket.id).await
    }

    /// Who writes the Plan (`plan_by`, d0133): the user, Noyan, or a ticket's agent by the name of the stage it writes
    /// from ("Milestone plan"): its comment author's stage, else the stage its ticket is in.
    async fn plan_writer(
        &self,
        actor: &Actor,
        author: Option<&super::ticket_comments::CommentAuthor>,
    ) -> PlanBy {
        let Actor::Agent(key) = actor else {
            return PlanBy::of(actor, None);
        };
        let from_author = match author {
            Some(super::ticket_comments::CommentAuthor::Agent {
                stage: Some(stage), ..
            }) => Some(stage.clone()),
            _ => None,
        };
        let stage = match from_author {
            Some(stage) => Some(stage),
            None => self
                .ticket(&key.to_string())
                .await
                .ok()
                .and_then(|t| t.place.stage_id().map(str::to_owned)),
        };
        let name = match &stage {
            Some(id) => self
                .effective_flow(key.project.as_str())
                .await
                .ok()
                .and_then(|flow| flow.stage(id).map(|s| s.name.clone()))
                .or_else(|| Some(id.clone())),
            None => None,
        };
        PlanBy::of(actor, name.as_deref())
    }

    /// A ticket by key (`SHOP-18`).
    pub async fn ticket(&self, key: &str) -> Result<Ticket, BoardError> {
        let id = self.ticket_id(key).await?;
        self.ticket_by_id(&id).await
    }

    /// The id of the ticket a key names.
    pub async fn ticket_id(&self, key: &str) -> Result<String, BoardError> {
        let parsed: TicketKey = key
            .parse()
            .map_err(|e: crate::domain::KeyError| BoardError::NotFound(e.to_string()))?;
        sqlx::query_scalar(
            "SELECT t.id FROM tickets t JOIN projects p ON p.id = t.project_id
             WHERE p.key = ? AND t.number = ?",
        )
        .bind(parsed.project.as_str())
        .bind(i64::from(parsed.number))
        .fetch_optional(self.store.pool())
        .await?
        .ok_or_else(|| BoardError::NotFound(format!("unknown ticket {key}")))
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

    /// Every ticket of a project, by key.
    pub async fn tickets(&self, project_key: &str) -> Result<Vec<Ticket>, BoardError> {
        let project = self.project(project_key).await?;
        let sql = format!("{SELECT_TICKETS} WHERE t.project_id = ? ORDER BY t.number");
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
        // Every ticket of the project by id, for dependency keys (links stay within a project).
        let keys: Vec<(String, String, i64)> = sqlx::query_as(
            "SELECT t.id, p.key, t.number
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
        // "answering SHOP-42" (MSG-03): the asker of the question a running answer session serves.
        let answering: std::collections::HashMap<String, String> = sqlx::query_as(
            "SELECT m.to_ticket_id, m.from_ticket_id FROM messages m
             JOIN tickets t ON t.id = m.to_ticket_id
             WHERE t.project_id = ? AND m.delivery = 'resumed' AND m.answer IS NULL
               AND EXISTS (SELECT 1 FROM sessions s WHERE s.ticket_id = m.to_ticket_id AND s.kind = 'answer'
                                                     AND s.state = 'running')
             ORDER BY m.asked_at DESC, m.id DESC",
        )
        .bind(project_id)
        .fetch_all(pool)
        .await?
        .into_iter()
        .collect();
        // "asked SHOP-41 · waiting" (MSG-06): open questions from each ticket, oldest first, until the answer was
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
        // Open recorded decisions per ticket, for the card's "N decisions" in Ready for Review (DEC-03).
        let open_decisions: std::collections::HashMap<String, i64> = sqlx::query_as(
            "SELECT t.id, COUNT(*) FROM decisions d JOIN tickets t ON t.id = d.ticket_id
             WHERE t.project_id = ? AND d.outcome = 'open' GROUP BY t.id",
        )
        .bind(project_id)
        .fetch_all(pool)
        .await?
        .into_iter()
        .collect();
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
        // The tickets' helpers (d0133): which one waits for the user, and which ones Resume brings back.
        let mut helpers: std::collections::HashMap<TicketKey, Vec<crate::helpers::Helper>> =
            std::collections::HashMap::new();
        for helper in crate::helpers::store::of_project(pool, project_id).await? {
            helpers
                .entry(helper.ticket.clone())
                .or_default()
                .push(helper);
        }
        let agent_edited: std::collections::HashSet<String> = self
            .tickets_updated_by_agent(project_id)
            .await?
            .into_iter()
            .collect();
        let key_of: std::collections::HashMap<&str, TicketKey> = keys
            .iter()
            .filter_map(|(id, project, number)| {
                Some((id.as_str(), key_from_parts(project, *number)?))
            })
            .collect();
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
        // Each ticket's latest round of a Check stage with several check agents (d0130).
        let check_rounds = project_check_rounds(pool, project_id).await?;

        records
            .into_iter()
            .map(|record| {
                let row = record.row;
                let key = key_from_parts(&record.project_key, row.number)
                    .ok_or_else(|| BoardError::NotFound(format!("bad key in ticket {}", row.id)))?;
                let base = Base::Branch(row.base_branch.clone().unwrap_or_default());
                let mut depends_on: Vec<TicketKey> = deps
                    .iter()
                    .filter(|(child, _)| *child == row.id)
                    .filter_map(|(_, parent)| key_of.get(parent.as_str()).cloned())
                    .collect();
                depends_on.sort();
                let status: Status =
                    row.state
                        .parse()
                        .map_err(|e: crate::domain::UnknownValue| {
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
                    ..Flags::default()
                };
                // While the card is in the check, or back from it, each check agent's verdict (d0130).
                flags.check_round = check_rounds
                    .get(&row.id)
                    .filter(|_| {
                        flags.validation_round.is_some()
                            || matches!(
                                status,
                                Status::Checking
                                    | Status::CheckerNeedsYou
                                    | Status::CheckNoResult
                                    | Status::CheckFailed
                                    | Status::WaitingForLimitReset
                            )
                    })
                    .cloned()
                    .map(|mut round| {
                        round.back_in = flags
                            .validation_round
                            .and(place.stage())
                            .and_then(|id| flow.stage(id))
                            .filter(|s| s.is_agent_stage())
                            .map(|s| s.name.clone());
                        round
                    });
                // One verdict is one checker's: a round of several says each agent's on the card instead.
                if (flags.validation_round.is_some() || status == Status::CheckFailed)
                    && flags.check_round.is_none()
                {
                    flags.verdict = verdicts
                        .get(&row.id)
                        .and_then(|body| first_line(body.as_deref()?));
                }
                let own_helpers = helpers.get(&key).map(Vec::as_slice).unwrap_or_default();
                flags.piece_wait = crate::helpers::restart::piece_wait(
                    own_helpers,
                    status,
                    row.interrupted_from.as_deref().and_then(|s| s.parse().ok()),
                    row.signal_text.as_deref(),
                    questions.get(&row.id).map(|open| {
                        (
                            open.helper_id.as_deref(),
                            open.question.text.as_str(),
                            open.pass_on_note.as_deref(),
                        )
                    }),
                );
                flags.helpers_to_resume = crate::helpers::restart::to_resume(own_helpers, status);
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
                    title: row.title,
                    body: row.body,
                    plan: row.plan,
                    plan_by: row.plan_by.map(|by| by.parse().expect("infallible")),
                    plan_at: row.plan_at,
                    priority: Priority::from_number(row.priority).unwrap_or_default(),
                    place,
                    status,
                    base,
                    base_locked: row.branch_locked,
                    external_id: row.external_id,
                    depends_on,
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
                    id: row.id,
                })
            })
            .collect()
    }
}

/// `SHOP` + 40 → `SHOP-40`.
pub(crate) fn key_from_parts(project: &str, number: i64) -> Option<TicketKey> {
    let project = ProjectKey::new(project).ok()?;
    Some(TicketKey::new(project, u32::try_from(number).ok()?))
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

/// Each ticket's latest validation round of the project when two or more check agents check in it (d0130): every
/// agent's latest row in the stage's order, with its session's tool and model, and whether a change already cancelled
/// a round with that number.
async fn project_check_rounds(
    pool: &sqlx::SqlitePool,
    project_id: &str,
) -> Result<std::collections::HashMap<String, crate::domain::CheckRoundFlag>, BoardError> {
    type Row = (
        String,
        String,
        i64,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    );
    let rows: Vec<Row> = sqlx::query_as(
        "SELECT v.ticket_id, COALESCE(v.round_id, v.id), v.round, v.agent, v.outcome, s.tool,
                COALESCE(s.model_ran, s.model)
         FROM validations v JOIN tickets t ON t.id = v.ticket_id
         LEFT JOIN sessions s ON s.id = v.session_id
         WHERE t.project_id = ? ORDER BY v.started_at, v.rowid",
    )
    .bind(project_id)
    .fetch_all(pool)
    .await?;
    let mut by_ticket: std::collections::HashMap<String, Vec<Row>> =
        std::collections::HashMap::new();
    for row in rows {
        by_ticket.entry(row.0.clone()).or_default().push(row);
    }
    Ok(by_ticket
        .into_iter()
        .filter_map(|(ticket, rows)| {
            let (_, latest, round, ..) = rows.last()?.clone();
            let mut agents: Vec<crate::domain::CheckAgentFlag> = Vec::new();
            for (_, _, _, agent, outcome, tool, model) in rows.iter().filter(|r| r.1 == latest) {
                let flag = crate::domain::CheckAgentFlag {
                    name: agent.clone()?,
                    tool: tool.as_deref().and_then(|t| t.parse().ok()),
                    model: model.clone(),
                    outcome: outcome.as_deref().and_then(|o| o.parse().ok()),
                };
                // A re-run adds a row for its agent: the latest counts.
                match agents.iter_mut().find(|a| a.name == flag.name) {
                    Some(slot) => *slot = flag,
                    None => agents.push(flag),
                }
            }
            let cancelled = crate::domain::ValidationOutcome::Cancelled.as_str();
            let again = rows
                .iter()
                .any(|r| r.1 != latest && r.2 == round && r.4.as_deref() == Some(cancelled));
            (agents.len() > 1).then(|| {
                (
                    ticket,
                    crate::domain::CheckRoundFlag {
                        round: u32::try_from(round).unwrap_or(0),
                        agents,
                        again,
                        back_in: None,
                    },
                )
            })
        })
        .collect())
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
