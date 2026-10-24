//! The one protocol spoken by the app, the CLI and the background service (d0055): JSON messages over the
//! user-locked local socket, as requests and replies plus a live event stream.
//!
//! # Messages
//!
//! Every message is one JSON object carrying the [`PROTOCOL_VERSION`], an optional request id and exactly one
//! body: `request`, `reply`, `event` or `error`.
//!
//! ```text
//! {"protocol_version":1,"id":7,"request":{"method":"service.status"}}
//! {"protocol_version":1,"id":7,"reply":{"service_status":{"pid":4120,...}}}
//! {"protocol_version":1,"event":{"kind":"test","seq":3}}
//! {"protocol_version":1,"id":8,"error":{"kind":"bad_request","message":"..."}}
//! ```
//!
//! A reply or error answers the request with the same id. Events have no id.
//!
//! # Framing (implementation choice, SDD §26)
//!
//! Newline-delimited JSON: each message is serialized compactly (JSON never contains a raw newline there)
//! and followed by `\n`. A frame may be at most [`MAX_FRAME_BYTES`] long. Chosen over length prefixes because
//! it is trivial to read in logs and to poke at by hand (`nc -U ~/.otaq/otaq.sock`).
//!
//! # Versions
//!
//! A peer with another `protocol_version` is reported as [`DecodeError::IncompatibleVersion`]
//! ([`ErrorKind::IncompatibleProtocol`]), which drives "Restart required" (d0022). The envelope fields and the
//! [`VERSION_INDEPENDENT_METHODS`] (`ping`, `service.status`, `service.stop`) never change incompatibly, so an
//! app can always learn the old service's status and restart it.

use std::fmt;
use std::io;

use crate::agent_tools::Tool;
use crate::board::agent_edits::AgentEdits;
use crate::board::batch::BatchTicket;
use crate::board::comments::{NewComment, ReviewComment, ReviewComments, ReviewFeedback};
use crate::board::conflicts::Conflict;
use crate::board::delete::DeletePreview;
use crate::board::deps::DepTree;
use crate::board::diff::DiffSummary;
use crate::board::epic_flight::{EpicQueueOutcome, TakeBackPlan, UndoQueueOutcome};
use crate::board::epics::ClosePlan;
use crate::board::merge::MergeProposal;
use crate::board::moves::{DropOn, DropTarget};
use crate::board::projects::{AddProject, Branches, FolderProbe, ProjectUpdate};
use crate::board::readiness::Readiness;
use crate::board::review::ApproveCheck;
use crate::board::settings::{EffectiveSettings, GlobalSettings, ProjectSettings, SettingProblem};
use crate::board::snapshot::{BoardSnapshot, ProjectInfo};
use crate::board::tickets::{HistoryEvent, NewTicketInput, Ticket, TicketUpdate};
use crate::domain::{Status, TicketKey};
use crate::git::diff::FileDiff;
use crate::prime::PrimeContext;
use crate::review::{LocalMergeOutcome, PassOutcome, ReturnOutcome};
use crate::scheduler::SchedulerStatus;
use crate::session::deliveries::Delivery;
use crate::session::{SessionInfo, TerminalSize};
use crate::tools::ToolsReport;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Bumped whenever a message changes incompatibly. Adding optional fields or new methods doesn't need a bump.
pub const PROTOCOL_VERSION: u32 = 1;

/// The longest frame accepted, in bytes (newline included).
pub const MAX_FRAME_BYTES: usize = 16 * 1024 * 1024;

/// Requests every service answers whatever protocol version the caller speaks. Their request and reply
/// shapes may only grow optional fields.
pub const VERSION_INDEPENDENT_METHODS: &[&str] = &["ping", "service.status", "service.stop"];

/// This build's ID, generated at compile time by `build.rs`: `<git hash>[-dirty].<build time>`.
/// The app, the CLI and the service all report the same value when built together.
pub fn build_id() -> &'static str {
    env!("BUILD_ID")
}

/// One message on the socket.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub protocol_version: u32,
    /// Set on requests, and on the reply or error that answers one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<u64>,
    /// Who a request asks to be recorded as (`--actor`, for example `agent:SHOP-18`, CLI-04). The service
    /// decides the actor from the resolved caller; an agent always acts as itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caller: Option<String>,
    /// The sending process's PID on requests (CLI-01). The service checks it against the socket's peer PID, so a
    /// client can't claim another process's identity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    #[serde(flatten)]
    pub body: Body,
}

impl Message {
    /// A request from this process: carries its PID.
    pub fn request(id: u64, request: Request) -> Self {
        Self {
            pid: Some(std::process::id()),
            ..Self::new(Some(id), Body::Request(request))
        }
    }

    pub fn reply(id: u64, reply: Reply) -> Self {
        Self::new(Some(id), Body::Reply(reply))
    }

    pub fn error(id: Option<u64>, error: ErrorBody) -> Self {
        Self::new(id, Body::Error(error))
    }

    pub fn event(event: Event) -> Self {
        Self::new(None, Body::Event(event))
    }

    fn new(id: Option<u64>, body: Body) -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION,
            id,
            caller: None,
            pid: None,
            body,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Body {
    Request(Request),
    Reply(Reply),
    Event(Event),
    Error(ErrorBody),
}

/// What a client asks the service. Message names are an implementation choice (SDD §5.4).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "method", content = "params")]
pub enum Request {
    /// Answered with [`Reply::Pong`].
    #[serde(rename = "ping")]
    Ping,
    /// Answered with [`Reply::ServiceStatus`].
    #[serde(rename = "service.status")]
    ServiceStatus,
    /// What this start of the service found (REC-01, REC-06): the interrupted tickets (and which were validating),
    /// the Project agents a restart ended and the unused "New agent"s it discarded. Answered with
    /// [`Reply::StartupReport`].
    #[serde(rename = "service.startup_report")]
    StartupReport,
    /// Asks the service to shut down; answered with [`Reply::Ok`] before it stops.
    #[serde(rename = "service.stop")]
    ServiceStop,
    /// Starts the event stream on this connection; answered with [`Reply::Ok`], then [`Event`]s follow.
    #[serde(rename = "events.subscribe")]
    Subscribe,
    /// The first-run tools checklist and permission-mode check; answered with [`Reply::ToolsCheck`].
    #[serde(rename = "tools.check")]
    ToolsCheck,
    /// A checklist row's **Install** or **Log in** (git, gh, az): its command in a terminal of no project (the
    /// running one if there is one). Answered with [`Reply::Session`]; attach with `session.attach {session}`.
    #[serde(rename = "tools.terminal")]
    ToolTerminalOpen(ToolTerminalParams),
    /// What the first-run screens need; answered with [`Reply::OnboardingState`].
    #[serde(rename = "onboarding.state")]
    OnboardingState,
    /// Who Flow onboarding can ask for a project (ASK-05, d0108): its name and the connected tools. Answered with
    /// [`Reply::OnboardingSources`].
    #[serde(rename = "onboarding.sources")]
    OnboardingSources(ProjectParams),
    /// **Ask <Tool>** (ASK-02, d0108): `{"project": "SHOP", "tool": "opencode"}` starts the tool's flow agent for a
    /// project in Flow setup (the Project agent role's agent on the tool's default model, the `flow` skill), or
    /// returns it while it runs. Refused for a tool that isn't connected, at its plan limit, and for a project not
    /// in Flow setup. Answered with [`Reply::ProjectAgent`]; attach to its `session`.
    #[serde(rename = "onboarding.ask")]
    OnboardingAsk(OnboardingAskParams),
    /// `{"project": "SHOP"}`: the project's Flow setup now (ASK-03, ASK-04, ASK-06): each tool's row and the open
    /// proposals. Answered with [`Reply::FlowSetup`]; refetch on [`Event::FlowSetupChanged`].
    #[serde(rename = "onboarding.flow_setup")]
    OnboardingFlowSetup(ProjectParams),
    /// `{"project": "SHOP"}`: **Use the default** or **Skip, use the default** (ASK-03, ASK-04): the project keeps
    /// the default flow and leaves Flow setup; every open proposal closes and every flow agent is told "The user
    /// chose another flow." and ends. Answered with [`Reply::Ok`].
    #[serde(rename = "onboarding.use_default")]
    OnboardingUseDefault(ProjectParams),
    /// Remembers that the user closed the permission-mode warning; answered with [`Reply::Ok`].
    #[serde(rename = "onboarding.dismiss_permission_warning")]
    DismissPermissionWarning,
    /// The user dismissed the one-time folder trust note (PRJ-08); it never comes back. Answered with [`Reply::Ok`].
    #[serde(rename = "notices.dismiss_trust_note")]
    DismissTrustNote,
    /// What macOS allows Otaq's notifications, Otaq's ask and the blocked banner (ATT-13, ATT-14; d0090). Answered
    /// with [`Reply::NotificationsState`].
    #[serde(rename = "notifications.state")]
    NotificationsState,
    /// The user answered Otaq's ask (Allow, Not now) or pressed Settings' Allow notifications: the ask is never set
    /// pending again, and the permission is read again (the app sends it once more when macOS answered its
    /// question). Answered with [`Reply::Ok`].
    #[serde(rename = "notifications.ask_answered")]
    NotificationsAskAnswered,
    /// The user closed the "macOS is blocking Otaq's notifications" banner: closed until the permission changes.
    /// Answered with [`Reply::Ok`].
    #[serde(rename = "notifications.dismiss_blocked")]
    NotificationsDismissBlocked,

    // Projects (PRJ-01…04). Answered with [`Reply::Projects`], [`Reply::FolderProbe`] or [`Reply::Project`]
    // ([`Reply::ProjectAdded`] for `project.add`).
    #[serde(rename = "project.list")]
    ProjectList,
    /// What adding a folder would give (repo root, suggested key, detected base).
    #[serde(rename = "project.probe")]
    ProjectProbe(PathParams),
    #[serde(rename = "project.add")]
    ProjectAdd(AddProject),
    #[serde(rename = "project.update")]
    ProjectUpdate(ProjectUpdateParams),
    /// Answered with [`Reply::Ok`].
    #[serde(rename = "project.remove")]
    ProjectRemove(KeyParams),
    /// The repository's local and remote branches; answered with [`Reply::Branches`].
    #[serde(rename = "project.branches")]
    ProjectBranches(ProjectParams),
    /// The project's PR provider, detected from `origin`, and its CLI's login (REV-11); answered with
    /// [`Reply::PrProvider`]. Only reads.
    #[serde(rename = "project.pr_provider")]
    ProjectPrProvider(ProjectParams),

    /// A project's columns with sorted cards; answered with [`Reply::Board`].
    #[serde(rename = "board.snapshot")]
    BoardSnapshot(ProjectParams),

    // Tickets. Mutations answer with the ticket as it is afterwards ([`Reply::Ticket`]).
    #[serde(rename = "ticket.get")]
    TicketGet(KeyParams),
    /// Every ticket and epic of a project, by key; answered with [`Reply::Tickets`].
    #[serde(rename = "ticket.list")]
    TicketList(ProjectParams),
    #[serde(rename = "ticket.create")]
    TicketCreate(NewTicketInput),
    /// Creates several tickets with links between them, all or nothing (`otaq create -f`, `--deps`), and their
    /// agent rows (`agents`, `by_user`; AGT-06). Answered with [`Reply::Tickets`] (the new tickets, in order).
    #[serde(rename = "ticket.create_batch")]
    TicketCreateBatch(BatchParams),
    /// Changes a ticket's fields and its stage choices (`choices`, `by_user`; AGM-11).
    #[serde(rename = "ticket.update")]
    TicketUpdate(TicketUpdateParams),
    #[serde(rename = "ticket.move")]
    TicketMove(MoveParams),
    /// A drop on any stage (FLW-18): the drag rules on the project's flow, carried out with the sessions they stop
    /// and start. `confirmed` and `note` are the dialog's (Send to Dev with the instructions, Run Security with the
    /// note). Answered with [`Reply::Returned`].
    #[serde(rename = "ticket.drag")]
    TicketDrag(DragParams),
    #[serde(rename = "ticket.pin")]
    TicketPin(KeyParams),
    #[serde(rename = "ticket.unpin")]
    TicketUnpin(KeyParams),
    /// Answered with [`Reply::AgentEdits`].
    #[serde(rename = "ticket.agent_edits")]
    TicketAgentEdits(KeyParams),
    /// Answered with [`Reply::Ok`].
    #[serde(rename = "ticket.agent_edits_seen")]
    TicketAgentEditsSeen(KeyParams),
    /// What deleting a never-started ticket would remove; answered with [`Reply::DeletePreview`].
    #[serde(rename = "ticket.delete_preview")]
    TicketDeletePreview(KeyParams),
    /// Deletes a never-started ticket (or epic with its children); answered with [`Reply::DeletePreview`]
    /// (what was removed). Not for agents (CLI-02).
    #[serde(rename = "ticket.delete")]
    TicketDelete(KeyParams),
    /// Retry start (SES-02): a user's click on a ticket whose start stopped (Start interrupted, or Held
    /// because its base diverged). Answered with the ticket back in Queued ([`Reply::Ticket`]); the whole start
    /// sequence then runs again.
    #[serde(rename = "ticket.retry_start")]
    TicketRetryStart(KeyParams),
    /// The context an agent reads first (`otaq prime`, PLG-02). Answered with [`Reply::Prime`].
    #[serde(rename = "ticket.prime")]
    TicketPrime(KeyParams),
    /// `otaq ask human`: the ticket shows Asks you with the question (ATT-01). Answered with
    /// [`Reply::Ticket`].
    #[serde(rename = "ticket.ask_human")]
    TicketAskHuman(AskHumanParams),
    /// `otaq decision record --stdin` (DEC-02, d0084): a recorded decision on the ticket; an agent records on its
    /// own ticket only. Answered with [`Reply::Decision`].
    #[serde(rename = "decision.record")]
    DecisionRecord(DecisionRecordParams),
    /// A ticket's recorded decisions (`otaq show`); answered with [`Reply::Decisions`].
    #[serde(rename = "decision.list")]
    DecisionList(KeyParams),
    /// Change… on a recorded decision in review (DEC-06, d0084): with `text`, Send back with it; with `title` and
    /// `description`, a fix child in the epic's review. Answered with [`Reply::DecisionChanged`].
    #[serde(rename = "decision.change")]
    DecisionChange(DecisionChangeParams),
    /// `otaq decision mark <ID> --source "<source>" --stdin` (DEC-04, d0083): an epic lead marks a child's recorded
    /// decision that a written source contradicts; a second mark replaces the first. Lead only. Answered with
    /// [`Reply::Decision`].
    #[serde(rename = "decision.mark")]
    DecisionMark(DecisionMarkParams),
    /// `otaq stage next --stdin` (and its other name `otaq review submit`): the agent ends its Agent stage with a
    /// summary and the ticket moves to the next stage; refused with uncommitted files (REV-07) and an unread lead
    /// note (CLI-08) (AGM-10). Answered with [`Reply::StageEnded`].
    #[serde(rename = "ticket.stage_next")]
    TicketStageNext(StageEndParams),
    /// `otaq stage done --stdin` (AGM-10): the agent ends the ticket from its Agent stage as Closed · nothing to
    /// merge, the summary its reason; refused when the ticket's branch has commits. Answered with
    /// [`Reply::StageEnded`].
    #[serde(rename = "ticket.stage_done")]
    TicketStageDone(StageEndParams),
    /// `otaq check pass|fail --stdin`: an agent check's verdict and report (VAL-03, AGM-10; its check session
    /// only).
    #[serde(rename = "validation.verdict")]
    ValidationVerdict(VerdictParams),
    /// The user's actions on a check (VAL-06, VAL-07, VAL-09, AGM-10): Stop this check, Skip this check, Move on
    /// anyway, Give it another round, Answer the agent, Retry validation.
    #[serde(rename = "ticket.validation_action")]
    TicketValidationAction(ValidationActionParams),
    /// `otaq prime --validate`: what the validator works from (VAL-10).
    #[serde(rename = "ticket.prime_validate")]
    TicketPrimeValidate(KeyParams),
    /// The ticket's validation rounds, newest first (History, VAL-12): outcome, duration, tokens, report and
    /// notes. Answered with [`Reply::Validations`].
    #[serde(rename = "ticket.validations")]
    TicketValidations(KeyParams),
    /// **Create ticket** from a "Not covered by any ticket" note of the ticket's validation (VAL-12; the user's
    /// click, never the validator's): a prefilled Backlog ticket in the same epic. Answered with [`Reply::Ticket`].
    #[serde(rename = "ticket.create_from_note")]
    TicketCreateFromNote(NoteParams),
    /// `otaq pr propose --stdin` (REV-10): the Open PR / Push skill's proposal (JSON text). Answered with
    /// [`Reply::Ticket`].
    #[serde(rename = "ticket.pr_propose")]
    TicketPrPropose(PrProposeParams),
    /// `otaq pr opened <url>`: the skill reports the PR. Answered with [`Reply::Ticket`].
    #[serde(rename = "ticket.pr_opened")]
    TicketPrOpened(PrOpenedParams),
    /// `otaq branch rename <name>` (GIT-07): refused once the branch was pushed. Answered with
    /// [`Reply::Ticket`].
    #[serde(rename = "ticket.branch_rename")]
    TicketBranchRename(BranchRenameParams),
    /// `otaq ask <KEY> --stdin` (MSG-01): `key` asks the linked `target`. Answered with [`Reply::Message`].
    #[serde(rename = "ticket.ask")]
    TicketAsk(AskTicketParams),
    /// `otaq answer <ID> --stdin`. Answered with [`Reply::Message`].
    #[serde(rename = "message.answer")]
    MessageAnswer(AnswerParams),
    /// The inbox's **Resume SHOP-17** on a question sent to the user (MSG-05): the finished ticket answers it.
    /// Answered with [`Reply::Message`].
    #[serde(rename = "message.resume")]
    MessageResume(IdParams),
    /// `otaq questions`: the questions waiting for the ticket's answer. Answered with [`Reply::Messages`].
    #[serde(rename = "ticket.questions")]
    TicketQuestions(KeyParams),
    /// Every question to or from the ticket, oldest first (the panel's Messages). Answered with
    /// [`Reply::Messages`].
    #[serde(rename = "ticket.messages")]
    TicketMessages(KeyParams),
    /// The ticket's answered `ask human` questions, for the Messages section. Answered with
    /// [`Reply::HumanAnswers`].
    #[serde(rename = "ticket.human_answers")]
    TicketHumanAnswers(KeyParams),
    /// **Open PR** / **Push to PR #N** (REV-10, REV-14): runs the Open PR or Push skill in the ticket's session.
    /// Answered with [`Reply::PrOutcome`].
    #[serde(rename = "ticket.open_pr")]
    TicketOpenPr(OpenPrParams),
    /// The dialog's **Push & open PR** / **Push** with the user's edits. Answered with [`Reply::PrOutcome`].
    #[serde(rename = "ticket.pr_confirm")]
    TicketPrConfirm(PrConfirmParams),
    /// The dialog's **Cancel**: the proposal is discarded. Answered with [`Reply::Ticket`].
    #[serde(rename = "ticket.pr_cancel")]
    TicketPrCancel(KeyParams),
    /// The dialog's **Retry** after the skill reported a problem. Answered with [`Reply::PrOutcome`].
    #[serde(rename = "ticket.pr_retry")]
    TicketPrRetry(OpenPrParams),
    /// What the Open PR / Push to PR dialog shows. Answered with [`Reply::PrFlow`].
    #[serde(rename = "ticket.pr_flow")]
    TicketPrFlow(KeyParams),
    /// The live branch-name check (valid ref, not taken locally or on origin). Answered with
    /// [`Reply::BranchCheck`].
    #[serde(rename = "ticket.pr_branch_check")]
    TicketPrBranchCheck(BranchRenameParams),
    /// **Mark merged** (REV-13): Done · Merged, cleanup, base fetch and waiting tickets started. Answered with
    /// [`Reply::MarkedMerged`].
    #[serde(rename = "ticket.mark_merged")]
    TicketMarkMerged(KeyParams),
    /// **Mark closed** (REV-13, CLS-02): needs `confirm`; closes like Close ticket. Answered with
    /// [`Reply::Ticket`].
    #[serde(rename = "ticket.mark_closed")]
    TicketMarkClosed(CloseParams),
    /// The ticket's events, newest first (History tab). Answered with [`Reply::History`].
    #[serde(rename = "ticket.history")]
    TicketHistory(KeyParams),
    /// **Close ticket** (CLS-01): Done · Closed · not merged from any unfinished state. With a running agent and
    /// no `confirm`, refused with `needs_confirmation` ("it will be stopped first"). Answered with
    /// [`Reply::Ticket`].
    #[serde(rename = "ticket.close")]
    TicketClose(CloseParams),
    /// **Reopen** (CLS-03): Done · Closed → Backlog; Merged tickets are refused. Answered with [`Reply::Ticket`].
    #[serde(rename = "ticket.reopen")]
    TicketReopen(KeyParams),
    /// **Move to next stage** by the user (CLS-05, d0108 rule 10): the agent's `stage next` when the agent isn't
    /// mid-turn (an agent check that comes next runs); refused with uncommitted files.
    /// Answered with [`Reply::Ticket`].
    #[serde(rename = "ticket.submit_by_user")]
    TicketSubmitByUser(SubmitByUserParams),
    /// **Park** (CLS-06): Dev → Backlog keeping the branch, files and conversation; without `confirm`, refused
    /// with `needs_confirmation` and the confirmation's text. Answered with [`Reply::Ticket`].
    #[serde(rename = "ticket.park")]
    TicketPark(CloseParams),
    /// **Resume** an Interrupted ticket (REC-01): the tool resumes its conversation in a new session in the same worktree,
    /// through the start sequence (Queued while the agent limit is full). Answered with [`Reply::Ticket`].
    #[serde(rename = "ticket.resume")]
    TicketResume(KeyParams),
    /// **Retry** after Resume failed: the same resume again (REC-03). Answered with [`Reply::Ticket`].
    #[serde(rename = "ticket.retry_resume")]
    TicketRetryResume(KeyParams),
    /// **Start from scratch** after Resume failed (REC-03): a new conversation in the same worktree. Without
    /// `confirm`, refused with `needs_confirmation` and the confirmation's text. Answered with [`Reply::Ticket`].
    #[serde(rename = "ticket.start_from_scratch")]
    TicketStartFromScratch(CloseParams),
    /// **Resume all** (REC-01, d0045): every Interrupted ticket, of `project` or of every project. Answered with
    /// [`Reply::Resumed`].
    #[serde(rename = "tickets.resume_all")]
    TicketsResumeAll(ResumeAllParams),

    // The inbox (ATT-06…ATT-10).
    /// **Answer** an Asks you / Idle item (ATT-08): typed into the session as the user's click; with unsent text
    /// and no `clear`, answered `needs_confirmation`. An Interrupted ticket that waited for an answer is resumed
    /// with it in one step. Answered with [`Reply::Delivery`].
    #[serde(rename = "inbox.answer")]
    InboxAnswer(InboxAnswerParams),
    /// Every needs-you item across all projects in inbox order, with the counts (ATT-06, ATT-10). Answered with
    /// [`Reply::Inbox`].
    #[serde(rename = "inbox.list")]
    InboxList,
    /// The inbox count in total and per project (ATT-10). Answered with [`Reply::InboxCounts`].
    #[serde(rename = "inbox.counts")]
    InboxCounts,

    // Cleanup (REC-04).
    /// The leftovers no ticket owns, grouped; changes nothing. Answered with [`Reply::CleanupScan`].
    #[serde(rename = "cleanup.scan")]
    CleanupScan,
    /// Deletes exactly the listed items, each still a leftover; without `confirm`, refused with
    /// `needs_confirmation`. Answered with [`Reply::CleanupOutcome`]. Nothing deletes leftovers otherwise.
    #[serde(rename = "cleanup.delete")]
    CleanupDelete(CleanupDeleteParams),

    // Warm builds (WRM-05…WRM-07, d0082).
    /// Every warm build in `<app-data>/warm/`, and whether the data folder's disk can clone. Answered with
    /// [`Reply::WarmList`].
    #[serde(rename = "warm.list")]
    WarmList,
    /// Clears a project's warm build (or, with `repo`, a folder no project owns). Answers at once: cleared, or
    /// waiting for the copies running from it (it clears when they end and publishes [`Event::WarmBuildChanged`]).
    /// Never turns the switch off. Answered with [`Reply::WarmClear`].
    #[serde(rename = "warm.clear")]
    WarmClear(WarmClearParams),

    // Review (REV-01…REV-03).
    /// The ticket's changed files since its `base_sha` (epics: also by child). Answered with
    /// [`Reply::DiffSummary`].
    #[serde(rename = "diff.files")]
    DiffFiles(KeyParams),
    /// One file's hunks; answered with [`Reply::FileDiff`].
    #[serde(rename = "diff.file")]
    DiffFile(DiffFileParams),
    // The Files tab (FTB-01…FTB-04, FTB-06; T-0467): the worktree with its uncommitted edits, from git otherwise.
    /// One folder's entries (`path` empty: the top); answered with [`Reply::FilesTree`].
    #[serde(rename = "files.tree")]
    FilesTree(FilesPathParams),
    /// File names matching `query` across the whole tree; answered with [`Reply::FilesSearch`].
    #[serde(rename = "files.search")]
    FilesSearch(FilesSearchParams),
    /// One file as it is; answered with [`Reply::FileContent`].
    #[serde(rename = "files.read")]
    FilesRead(FilesPathParams),
    /// One file's hunks against the base, uncommitted edits included; answered with [`Reply::FileDiff`].
    #[serde(rename = "files.diff")]
    FilesDiff(FilesPathParams),
    /// Every changed file against the base, uncommitted ones included; answered with [`Reply::FilesChanges`].
    #[serde(rename = "files.changes")]
    FilesChanges(KeyParams),
    // The project Files view (FIL-02…FIL-04, FIL-07, FIL-08, FIL-10; T-0476): the project's base branch from git with
    // every ticket's changes on top, keyed by project.
    /// One folder of the base with each file's owners; answered with [`Reply::ProjectTree`].
    #[serde(rename = "files.project_tree")]
    FilesProjectTree(ProjectFilesPathParams),
    /// File names on the base and in the owners' changes; answered with [`Reply::ProjectSearch`].
    #[serde(rename = "files.project_search")]
    FilesProjectSearch(ProjectFilesSearchParams),
    /// One file as it is on the base (a file only on another base: a ticket's diff); answered with
    /// [`Reply::ProjectFile`].
    #[serde(rename = "files.project_read")]
    FilesProjectRead(ProjectFilesPathParams),
    /// One owner's change of a file; answered with [`Reply::FileDiff`].
    #[serde(rename = "files.project_diff")]
    FilesProjectDiff(ProjectFilesDiffParams),
    /// Every owner's changed files, grouped by ticket; answered with [`Reply::ProjectChanges`].
    #[serde(rename = "files.project_changes")]
    FilesProjectChanges(ProjectParams),
    /// The source picker's rows: the base, epics, tickets with a worktree, tickets without; answered with
    /// [`Reply::ProjectSources`].
    #[serde(rename = "files.project_sources")]
    FilesProjectSources(ProjectParams),
    /// A line comment in the ticket's current round; answered with [`Reply::ReviewComment`].
    #[serde(rename = "review.comment_add")]
    ReviewCommentAdd(NewComment),
    /// A reply in the thread of comment `id` (same file, line and side); answered with
    /// [`Reply::ReviewComment`].
    #[serde(rename = "review.comment_reply")]
    ReviewCommentReply(CommentTextParams),
    /// Changes a pending comment; answered with [`Reply::ReviewComment`].
    #[serde(rename = "review.comment_edit")]
    ReviewCommentEdit(CommentTextParams),
    /// Deletes a pending comment; answered with [`Reply::Ok`].
    #[serde(rename = "review.comment_delete")]
    ReviewCommentDelete(CommentIdParams),
    /// The ticket's comments (every round, or one) with the pending counter; answered with
    /// [`Reply::ReviewComments`].
    #[serde(rename = "review.comments")]
    ReviewComments(CommentsParams),

    // Ticket comments and reports (CMT-01…CMT-06, d0109). Not the line comments above.
    /// Posts a comment with an optional report (a file to copy or pasted text); answered with
    /// [`Reply::TicketComment`]. A checker posts only with its verdict.
    #[serde(rename = "ticket.comment_post")]
    TicketCommentPost(TicketCommentPostParams),
    /// Changes comment `n`'s text; answered with [`Reply::TicketComment`].
    #[serde(rename = "ticket.comment_edit")]
    TicketCommentEdit(TicketCommentEditParams),
    /// Deletes comment `n` (History keeps it, its report stays); answered with [`Reply::TicketComment`], as it was.
    #[serde(rename = "ticket.comment_delete")]
    TicketCommentDelete(TicketCommentParams),
    /// The ticket's comments, oldest first; answered with [`Reply::TicketComments`].
    #[serde(rename = "ticket.comments")]
    TicketComments(KeyParams),
    /// Where comment `n`'s report is, with its size and lines; answered with [`Reply::TicketReport`]. The CLI prints
    /// the file itself (streamed), so a 20 MB report never travels in one frame.
    #[serde(rename = "ticket.report")]
    TicketReport(TicketCommentParams),
    /// Fail on a human check (FLW-14; Send back on the default flow, REV-04): the pending comments and the note go
    /// to the nearest earlier Agent stage's session, and the ticket returns there. Answered with
    /// [`Reply::Returned`]: done, or `needs_confirmation` when the session's input line has unsent text (resend
    /// with `clear_first` for "Clear it and send").
    #[serde(rename = "ticket.fail")]
    TicketFail(FailParams),
    /// Pass on a human check (FLW-14): answered with [`Reply::Passed`]: the delivery choices when a click Delivery
    /// comes next (nothing changed), else the ticket moved on.
    #[serde(rename = "ticket.pass")]
    TicketPass(KeyParams),
    /// The latest Send back's comments and note (`otaq review feedback`, REV-05). Answered with
    /// [`Reply::ReviewFeedback`].
    #[serde(rename = "review.feedback")]
    ReviewFeedback(KeyParams),
    /// What the Local merge confirmation shows: the target branch and the proposed squash message. Answered with
    /// [`Reply::MergeProposal`].
    #[serde(rename = "ticket.merge_proposal")]
    TicketMergeProposal(KeyParams),
    /// **Local merge** with the confirmed squash message (GIT-03). Answered with [`Reply::LocalMerge`]: merged,
    /// refused (uncommitted files, diverged base, dirty checkout) or a conflict.
    #[serde(rename = "ticket.local_merge")]
    TicketLocalMerge(LocalMergeParams),
    /// **Stop** on "Merging · running the merge check" (d0120, Q475): the Merge check of the ticket's Local merge
    /// ends, nothing is merged and the ticket stays where it was; the Local merge request answers
    /// [`LocalMergeOutcome::CheckStopped`]. Answered with [`Reply::Ok`]; refused when it runs none.
    #[serde(rename = "ticket.merge_check_stop")]
    TicketMergeCheckStop(KeyParams),
    /// The check before any approve (REV-06): new commits since the review? Answered with
    /// [`Reply::ApproveCheck`].
    #[serde(rename = "ticket.approve_check")]
    TicketApproveCheck(KeyParams),
    /// The ticket's "conflicts with `main`" with its files, if any. Answered with [`Reply::Conflict`].
    #[serde(rename = "ticket.conflict")]
    TicketConflict(KeyParams),
    /// **Send back with PR comments** (FLW-16): a ticket whose PR is open returns to its nearest earlier Agent
    /// stage, whose session reads the unresolved PR comments (`otaq review feedback`) with the optional note.
    /// Answered with [`Reply::Returned`], like Send back (`clear_first` for "Clear it and send").
    #[serde(rename = "ticket.send_back_pr")]
    TicketSendBackPr(FailParams),
    /// **Add a child from PR comments** (FLW-16): an epic whose PR is open gets a Backlog child holding the PR's
    /// unresolved comments. Answered with [`Reply::Ticket`] (the child).
    #[serde(rename = "ticket.add_child_from_pr")]
    TicketAddChildFromPr(KeyParams),

    // Epics.
    #[serde(rename = "epic.move_to")]
    EpicMoveTo(MoveToEpicParams),
    /// Answered with [`Reply::ClosePlan`].
    #[serde(rename = "epic.close_plan")]
    EpicClosePlan(KeyParams),
    /// Answered with [`Reply::ClosePlan`] (what was closed). Without `confirm`, refused (`needs_confirmation`)
    /// when agents would be stopped.
    #[serde(rename = "epic.close")]
    EpicClose(CloseParams),
    /// **Queue an epic** (EPC-12): every child in Backlog goes to Ready for Dev (parked ones resume), or with
    /// `child` only that one row. Answered with [`Reply::EpicQueued`], what the queue toast reads (EPC-15).
    #[serde(rename = "epic.queue")]
    EpicQueue(EpicQueueParams),
    /// **Take back an epic** (EPC-14): its track dropped on Backlog. Waiting children are unqueued; when nothing
    /// waits, the running ones are parked, which without `confirm` is refused (`needs_confirmation`). Answered with
    /// [`Reply::EpicTakenBack`], the plan carried out.
    #[serde(rename = "epic.take_back")]
    EpicTakeBack(CloseParams),
    /// The queue toast's **Undo** (EPC-15) with the outcome's keys: takes back the ones still waiting, never parks.
    /// Answered with [`Reply::EpicQueueUndone`] ("SHOP-45.1 already started").
    #[serde(rename = "epic.undo_queue")]
    EpicUndoQueue(EpicUndoQueueParams),

    // Dependencies.
    #[serde(rename = "dep.add")]
    DepAdd(DepParams),
    #[serde(rename = "dep.remove")]
    DepRemove(DepParams),
    /// Answered with [`Reply::DepTree`].
    #[serde(rename = "dep.tree")]
    DepTree(KeyParams),
    /// Answered with [`Reply::Cycles`].
    #[serde(rename = "dep.cycles")]
    DepCycles(ProjectParams),
    /// Each unfinished ticket's dependency state (`otaq ready` / `blocked`); answered with
    /// [`Reply::Readiness`].
    #[serde(rename = "dep.readiness")]
    DepReadiness(ProjectParams),
    /// The agent limits as the scheduler sees them now (the header's "3 / 5 agents", Settings' "Right now");
    /// answered with [`Reply::SchedulerStatus`].
    #[serde(rename = "scheduler.status")]
    SchedulerStatus,
    /// Today's tokens across all projects and, with a project, its tickets' tokens (USG-01).
    #[serde(rename = "usage.summary")]
    UsageSummary(UsageParams),
    /// The tokens of each session of a ticket (the History tab, USG-01).
    #[serde(rename = "usage.sessions")]
    UsageSessions(KeyParams),
    /// Activity's data per agent tool (SHL-03, USG-09): agents running, today's tokens, limit windows; answered
    /// with [`Reply::Activity`].
    #[serde(rename = "usage.activity")]
    UsageActivity,
    /// A tool's usage windows (USG-04, USG-09; Claude Code's from `otaq statusline`): they replace the tool's set.
    #[serde(rename = "usage.record_limits")]
    UsageRecordLimits(RecordLimitsParams),
    /// `{"key": "SHOP-12"}`: "This is the usage limit" on that Idle inbox item (USG-07, per tool: USG-10): every Idle
    /// agent on its agent's tool waits for that tool's limit reset. Replies with the tickets it folded into the
    /// tool's banner line.
    #[serde(rename = "usage.mark_limit")]
    UsageMarkLimit(KeyParams),

    // Model lists (AGT-02).
    /// Otaq's copy of a tool's model list, answered at once with [`Reply::Models`] (none before the first read).
    #[serde(rename = "agents.models")]
    AgentsModels(ModelsParams),
    /// Reads a tool's model list again (Settings → Agents opening); answered with [`Reply::Models`] after the
    /// read: the new copy, or the old one when the read failed.
    #[serde(rename = "agents.models_refresh")]
    AgentsModelsRefresh(ModelsParams),
    /// The picker's Retry: reads a tool's model list again at once (and the models.dev catalog when Otaq has no
    /// fresh copy); answered like `agents.models_refresh`.
    #[serde(rename = "agents.models_retry")]
    AgentsModelsRetry(ModelsParams),

    // Settings (SET-01).
    #[serde(rename = "settings.get")]
    SettingsGet,
    #[serde(rename = "settings.set")]
    SettingsSet(GlobalSettings),
    #[serde(rename = "project_settings.get")]
    ProjectSettingsGet(ProjectParams),
    #[serde(rename = "project_settings.set")]
    ProjectSettingsSet(ProjectSettingsParams),
    /// `{"project": "SHOP", "view": {...}}`: stores the project's view preferences (SHB-05), the rest of its
    /// settings unchanged. Answered with [`Reply::Ok`].
    #[serde(rename = "project_view.set")]
    ProjectViewSet(ProjectViewParams),

    // A project's flow (FLW-01).
    /// `{"project": "SHOP"}`: the flow the project runs, its current stored version. Answered with
    /// [`Reply::FlowVersion`].
    #[serde(rename = "flow.current")]
    FlowCurrent(ProjectParams),
    /// `{"project": "SHOP"}`: every flow the project applied, oldest first; the last is the current one. Answered
    /// with [`Reply::FlowVersions`].
    #[serde(rename = "flow.versions")]
    FlowVersions(ProjectParams),
    /// `{"project": "SHOP"}`: the project's open proposal, checked and compared with the flow as it will be now
    /// (FLW-03, FLW-04); during Flow setup the user's own if there is one, else the newest (`onboarding.flow_setup`
    /// lists them all). Answered with [`Reply::FlowProposal`], `null` when there is none.
    #[serde(rename = "flow.proposal")]
    FlowProposal(ProjectParams),
    /// Apply in the flow editor (FLW-05, FLW-06): the open proposal `proposal`, with the user's `choices` (import clashes,
    /// skill folders, local models). Answered with [`Reply::FlowApplied`].
    #[serde(rename = "flow.apply")]
    FlowApply(crate::flow::apply::ApplyParams),
    /// `{"project": "SHOP"}`: Cancel in the flow editor drops the open proposal. Answered with [`Reply::Ok`].
    #[serde(rename = "flow.cancel_proposal")]
    FlowCancelProposal(ProjectParams),
    /// `{"project": "SHOP"}`: the waiting change and the tickets it waits for (FLW-06). Answered with
    /// [`Reply::FlowWaiting`], `null` when nothing waits.
    #[serde(rename = "flow.waiting")]
    FlowWaiting(ProjectParams),
    /// `{"project": "SHOP"}`: Cancel change drops the waiting change (merged ones too); starts resume. Answered with
    /// [`Reply::Ok`].
    #[serde(rename = "flow.cancel_waiting")]
    FlowCancelWaiting(ProjectParams),
    /// `{"project": "SHOP"}`: what Apply now would do on the waiting change (APN-02). Answered with
    /// [`Reply::ApplyNowPlan`], `null` when nothing waits. `flow.apply_now` (APN-04): `{"project", "stamp", "picks":
    /// [{"ticket": "SHOP-19", "to": {"stage": "dev"}}]}`, answered with [`Reply::AppliedNow`].
    #[serde(rename = "flow.apply_now_plan")]
    FlowApplyNowPlan(ProjectParams),
    #[serde(rename = "flow.apply_now")]
    FlowApplyNow(crate::flow::apply_now::ApplyNowParams),
    /// A proposal by the user from an earlier version, another project, a preset, a file or the shipped agents file
    /// (FLW-08…FLW-10); it replaces the open one. Answered with [`Reply::FlowProposal`].
    #[serde(rename = "flow.propose_from")]
    FlowProposeFrom(crate::flow::apply::ProposeFromParams),
    /// `otaq flow show` (FLW-02): `{"project": "SHOP"}`, the applied stages and the agents they name as one document.
    /// Answered with [`Reply::FlowDocument`].
    #[serde(rename = "flow.show")]
    FlowShow(ProjectParams),
    /// `otaq flow propose` (FLW-03, FED-02), from a Project agent's session or the user: a document or another
    /// project's stages, checked, then put in the project's draft with the agent's changes marked (during Flow setup
    /// it replaces only the open one from the same tool, ASK-03). Answered with [`Reply::FlowProposal`].
    #[serde(rename = "flow.propose")]
    FlowPropose(crate::flow::propose::ProposeParams),
    /// Ask for more changes in the flow editor (FLW-04): closes the open proposal; the text goes to the Project agent
    /// that proposed it. Answered with [`Reply::Ok`].
    #[serde(rename = "flow.ask_for_changes")]
    FlowAskForChanges(crate::flow::propose::AskForChangesParams),
    /// The flow editor's draft (FED-02, FED-08): `flow.draft` `{"project": "SHOP"}` reads it, with the stages a
    /// Project agent changed and when it would apply; `flow.draft_update` `{"project", "edit": {"edit": "document",
    /// "yaml"} | {"edit": "undo_stage", "stage"} | {"edit": "undo_all"} | {"edit": "start_from", "source"} |
    /// {"edit": "entry", "proposal"} | {"edit": "flow_in_use"}}` changes it. Both answered with [`Reply::FlowDraft`].
    #[serde(rename = "flow.draft")]
    FlowDraft(ProjectParams),
    #[serde(rename = "flow.draft_update")]
    FlowDraftUpdate(crate::flow::draft::DraftUpdateParams),
    /// `otaq flow write-skill <stage> [--done]` (FED-06): `{"project", "stage"}` gives the folder the Project agent
    /// writes the stage's skill in (the field: "Being written by the Project agent…"); with `"done": true` the folder
    /// is imported as Otaq's own copy and lands in the draft as the stage's skill. Answered with
    /// [`Reply::SkillWrite`].
    #[serde(rename = "flow.write_skill")]
    FlowWriteSkill(crate::flow::write_skill::WriteSkillParams),

    // Sessions (SES-04, SVC-05).
    /// With `key`, every session the ticket ever had (newest first); without, every session in the service's
    /// memory. Answered with [`Reply::Sessions`].
    #[serde(rename = "session.list")]
    SessionList(SessionListParams),
    /// The session's stored output, then its live output as `session_output` events on this connection.
    /// Attaching again on the same connection keeps the live output already flowing, so its events may come
    /// before the reply or start before the reply's `end`: skip by offset. Answered with [`Reply::SessionAttached`].
    #[serde(rename = "session.attach")]
    SessionAttach(AttachParams),
    /// Stops the live output started by `session.attach`; the session keeps running. Answered with
    /// [`Reply::Ok`].
    #[serde(rename = "session.detach")]
    SessionDetach(SessionIdParams),
    /// Answered with [`Reply::Ok`].
    #[serde(rename = "session.resize")]
    SessionResize(ResizeParams),
    /// Keystrokes typed by a person, relayed to the session (SVC-06). Answered with [`Reply::Ok`].
    #[serde(rename = "session.input")]
    SessionInput(InputParams),
    /// **Deliver now** (SES-06): types the session's waiting messages. With unsent text in its input line the
    /// answer is `needs_confirmation`; `clear_first` is the user's "Clear it and send" (SES-07). Answered with
    /// [`Reply::Delivery`].
    #[serde(rename = "session.deliver_now")]
    SessionDeliverNow(DeliverNowParams),
    /// **Send again** (d0118): types the session's hand-off that its agent hasn't picked up yet once more, as the
    /// user's click (d0048: unsent text answers `needs_confirmation`; `clear_first` is "Clear it and send"). Nothing is
    /// typed when it has none. Answered with [`Reply::Delivery`].
    #[serde(rename = "session.send_again")]
    SessionSendAgain(DeliverNowParams),
    /// A client's terminal for the session gained or lost keyboard focus (SES-10). Answered with
    /// [`Reply::Ok`].
    #[serde(rename = "session.focus")]
    SessionFocus(FocusParams),
    /// The client's app window gained or lost focus (d0090): while any app window is focused, no OS notification
    /// goes out. Per connection, like `session.focus`; a closed connection loses it. Answered with [`Reply::Ok`].
    #[serde(rename = "app.focus")]
    AppFocus(AppFocusParams),
    /// "Before the restart" (REC-02): the last messages and actions of the ticket's latest conversation, read
    /// from Claude Code's saved conversation. Answered with [`Reply::BeforeRestart`].
    #[serde(rename = "session.before_restart")]
    SessionBeforeRestart(BeforeRestartParams),

    // Agent signals (SDD §11.4).
    /// An agent tool's hook in the caller's agent session (`otaq hook --tool <tool> <event>`, PLG-04, ADP-03).
    /// Answered with [`Reply::Ok`]; from outside an agent session it changes nothing.
    #[serde(rename = "hook")]
    Hook(HookParams),

    // Run (RUN-01, RUN-04): the project's run command in a ticket's worktree.
    /// Starts the ticket's Run; refused without a run command (`no_run_command`) or a worktree (`no_worktree`).
    #[serde(rename = "run.start")]
    RunStart(KeyParams),
    #[serde(rename = "run.stop")]
    RunStop(KeyParams),
    /// The ticket's Run: state, port, command, Open URL, session.
    #[serde(rename = "run.status")]
    RunStatus(KeyParams),
    /// Every Run that is up, across projects (the header's "▶ 3 processes").
    #[serde(rename = "run.list")]
    RunList,
    /// Ask the agent for the project's run command (RUN-02): answers at once; progress and the outcome arrive as
    /// `run_suggestion` events. Only on the user's click.
    #[serde(rename = "run.suggest")]
    RunSuggest(ProjectParams),
    /// Stops a running Ask the agent.
    #[serde(rename = "run.suggest_cancel")]
    RunSuggestCancel(ProjectParams),
    /// The ticket's Shell (RUN-05): the running or last one, or a new one on first use; `reopen` starts a new one.
    /// Answered with the session; attach to it with `session.attach {key, kind: "shell"}`.
    #[serde(rename = "shell.open")]
    ShellOpen(ShellOpenParams),
    /// A Project terminal of the project (RUN-06, d0068): the newest running one (else the newest ended one still
    /// open, else a new one); `new` always starts a new one, "Terminal 1", "Terminal 2"…. Answered with
    /// [`Reply::ProjectTerminal`]; attach with `session.attach {session}`.
    #[serde(rename = "project_terminal.open")]
    ProjectTerminalOpen(ProjectTerminalParams),
    /// The project's open Project terminals, by number ([`Reply::ProjectTerminals`]).
    #[serde(rename = "project_terminal.list")]
    ProjectTerminalList(ProjectParams),
    /// Closes a Project terminal: its shell ends and it leaves the list.
    #[serde(rename = "project_terminal.close")]
    ProjectTerminalClose(SessionIdParams),
    /// The tickets created from the project's Project terminal since its summary was last dismissed (T-0167).
    #[serde(rename = "project_terminal.created")]
    ProjectTerminalCreated(ProjectParams),
    /// Closes the summary: it lists only tickets created from now on.
    #[serde(rename = "project_terminal.dismiss")]
    ProjectTerminalDismiss(ProjectParams),

    // Project agents (PAG-01, PAG-04, PAG-06; d0068).
    /// Opens a new Project agent in the project with the user's first message (sending it is the go-ahead, d0087):
    /// the skill and the message are its first prompt. Refused without words, and while the usage limit holds.
    /// Answered with [`Reply::ProjectAgent`]; attach to its `session`.
    #[serde(rename = "project_agent.open")]
    ProjectAgentOpen(ProjectAgentOpenParams),
    /// The project's open Project agents and its 10 most recent ended ones ([`Reply::ProjectAgents`]).
    #[serde(rename = "project_agent.list")]
    ProjectAgentList(ProjectParams),
    /// Closes a Project agent ("End agent"): it ends and stays in Recently ended; a running one mid-turn needs
    /// `confirm`. Answered with [`Reply::Ok`].
    #[serde(rename = "project_agent.close")]
    ProjectAgentClose(ProjectAgentCloseParams),
    /// **Close ended agents** (d0087): closes every ended, not yet closed Project agent of the project (not epic
    /// leads). Answered with the project's [`Reply::ProjectAgents`].
    #[serde(rename = "project_agent.close_ended")]
    ProjectAgentCloseEnded(ProjectParams),
    /// Resumes an ended Project agent's conversation (Ended by restart · Resume, Recently ended). Answered with
    /// [`Reply::ProjectAgent`].
    #[serde(rename = "project_agent.resume")]
    ProjectAgentResume(IdParams),
    /// The user's rename; the agent never overwrites it. Answered with [`Reply::ProjectAgent`].
    #[serde(rename = "project_agent.rename")]
    ProjectAgentRename(ProjectAgentRenameParams),
    /// `otaq agent name "<name>"` from a Project agent session (PAG-04): names it once; again only with
    /// `rename`. Answered with [`Reply::ProjectAgent`].
    #[serde(rename = "agent.name")]
    AgentName(AgentNameParams),
    /// `otaq ask <KEY> --stdin` from a Project agent (PAG-02): to any ticket, §16 rules; the answer is typed
    /// into its session. Answered with [`Reply::Message`].
    #[serde(rename = "agent.ask")]
    AgentAsk(AgentAskParams),
    /// `otaq ask human --stdin` from a Project agent: the question waits on it for the user's answer.
    /// Answered with [`Reply::ProjectAgent`].
    #[serde(rename = "agent.ask_human")]
    AgentAskHuman(AgentAskHumanParams),
    /// The user's answer to a Project agent's question, typed into its session (with unsent text and no `clear`:
    /// `needs_confirmation`). Answered with [`Reply::Delivery`].
    #[serde(rename = "project_agent.answer")]
    ProjectAgentAnswer(ProjectAgentAnswerParams),
    /// The user's message for a running Project agent from the Ask bar's Continue (SHB-03, d0113), typed into its
    /// session (with unsent text: `needs_confirmation`, nothing typed). Answered with [`Reply::Delivery`].
    #[serde(rename = "project_agent.send")]
    ProjectAgentSend(ProjectAgentSendParams),
    /// `otaq prime --project` (PAG-03). Answered with [`Reply::ProjectPrime`].
    #[serde(rename = "project_agent.prime")]
    ProjectAgentPrime(ProjectParams),
    /// A Project agent's "Created and changed" (PAG-05). Answered with [`Reply::CreatedAndChanged`].
    #[serde(rename = "project_agent.created")]
    ProjectAgentCreated(IdParams),

    // Epic leads (LEAD-02, LEAD-12; d0083).
    /// **Open lead**: starts the epic's lead with no first message, or resumes an ended one (the user's click).
    /// Refused while it is live, while the epic is in review, and during the usage limit. Answered with
    /// [`Reply::Lead`]; attach to its agent's `session`.
    #[serde(rename = "lead.open")]
    LeadOpen(LeadParams),
    /// Resumes the epic's lead's conversation (Ended by restart · Resume). Answered with [`Reply::Lead`].
    #[serde(rename = "lead.resume")]
    LeadResume(LeadParams),
    /// `otaq suggest close|delete <KEY> --stdin` (LEAD-08): an epic lead suggests closing or deleting a never-started
    /// child of its epic; an inbox item the user confirms or dismisses. Lead only. Answered with
    /// [`Reply::Suggestion`].
    #[serde(rename = "lead.suggest")]
    LeadSuggest(SuggestParams),
    /// The user's Close… / Delete… on a lead's suggestion: runs the usual close (d0044) or delete (d0060) of the
    /// child and marks the suggestion confirmed. Answered with [`Reply::Suggestion`].
    #[serde(rename = "suggestion.confirm")]
    SuggestionConfirm(IdParams),
    /// The user's Dismiss on a lead's suggestion; it doesn't wake the lead. Answered with [`Reply::Suggestion`].
    #[serde(rename = "suggestion.dismiss")]
    SuggestionDismiss(IdParams),
    /// Every epic lead of the project, with its status (the board's Lead Status, T-0289). Answered with
    /// [`Reply::Leads`].
    #[serde(rename = "lead.list")]
    LeadList(ProjectParams),
    /// `otaq ask human --pass-on <QUESTION-ID> [--option …] [--recommended N] --stdin` (LEAD-06): the epic's lead
    /// passes a child's question that waits for it on to the user, with its recommendation and reason. Lead only.
    /// Answered with [`Reply::LeadQuestion`].
    #[serde(rename = "lead.pass_on")]
    LeadPassOn(PassOnParams),
    /// `otaq questions` from an epic's lead (LEAD-06): its children's questions waiting for it. Lead only. Answered
    /// with [`Reply::LeadQuestions`].
    #[serde(rename = "lead.questions")]
    LeadQuestions,
    /// `otaq prime --epic` (LEAD-03): the epic lead's context, for the lead (`$OTAQ_EPIC`) or the user's terminal.
    /// Answered with [`Reply::EpicPrime`].
    #[serde(rename = "lead.prime")]
    LeadPrime(KeyParams),
    /// `otaq note <KEY> --stdin` (LEAD-10): an epic lead leaves an unread note for a running child of its epic. Lead
    /// only. Answered with [`Reply::LeadNote`].
    #[serde(rename = "lead.note")]
    LeadNote(LeadNoteParams),
    /// `otaq notes` (LEAD-10): the ticket's unread lead notes, marked read by this request. Answered with
    /// [`Reply::LeadNotes`].
    #[serde(rename = "ticket.notes")]
    TicketNotes(KeyParams),
    /// The epic lead's log on a ticket (T-0290): its children's questions to the lead and the lead's notes, for
    /// the Messages section of the epic and the child. Answered with [`Reply::LeadLog`].
    #[serde(rename = "ticket.lead_log")]
    TicketLeadLog(KeyParams),
    /// `otaq show <KEY> --terminal` (SHL-10, CLI-09, d0087): the ticket's terminal as the user sees it, as plain
    /// text; read-only, for the user, a Project agent of its project and the lead of its epic. Answered with
    /// [`Reply::TicketTerminal`].
    #[serde(rename = "ticket.terminal")]
    TicketTerminal(TerminalParams),

    // Agents (AGM-01…AGM-05; d0108, d0088). The user's requests (Settings → Agents); agents never change agents.
    /// Every agent (with its mark), the default model, each tool's fallback and the move-over's notice. Answered
    /// with [`Reply::Agents`].
    #[serde(rename = "agents.list")]
    AgentsList,
    /// `{"name": "Rust developer", "instructions": "…", "skills": [{"shipped": "work-ticket"}], "model": {"kind":
    /// "tool", "tool": "claude", "model": "opus"}, "effort": "high"}`: New agent ("Added by you"); the name new
    /// ignoring case (not `skip` or `_…`), the model from the tool's list, the effort one of the tool's. Answered with
    /// [`Reply::Agent`].
    #[serde(rename = "agents.add")]
    AgentsAdd(crate::agents::AgentFields),
    /// `{"id": "0199…", …the fields}`: the whole page; a model change clears the mark. Applies to the next session
    /// that starts. Answered with [`Reply::Agent`].
    #[serde(rename = "agents.edit")]
    AgentsEdit(AgentEditParams),
    /// `{"id": "0199…"}`: Duplicate's draft, "<name> copy", not saved. Answered with [`Reply::AgentDraft`].
    #[serde(rename = "agents.duplicate")]
    AgentsDuplicate(IdParams),
    /// `{"id": "0199…"}`: deletes an agent nothing uses. Refused (`agent_in_use`) for a role's agent ("Planner is
    /// your Project agent. Give the role to another agent first") and while a stage names it ("Griller is used in
    /// shop-web · Grill. Change that stage first.").
    #[serde(rename = "agents.delete")]
    AgentsDelete(IdParams),
    /// `{"id": "0199…"}`: what uses an agent, before Delete asks. Answered with [`Reply::AgentInUse`].
    #[serde(rename = "agents.in_use")]
    AgentsInUse(IdParams),
    /// `{"role": "project-agent", "agent": "0199…"}`: gives the role to another agent, in one step. Answered with
    /// [`Reply::Agent`] (the new holder).
    #[serde(rename = "agents.move_role")]
    AgentsMoveRole(MoveRoleParams),
    /// `{"model": {"tool": "claude", "model": "sonnet"}}`: Default model for new agents (AGM-03). Answered with
    /// [`Reply::Agents`].
    #[serde(rename = "agents.default_model")]
    AgentsDefaultModel(DefaultModelParams),
    /// `{"tool": "claude", "fallback": {"tool": "codex", "model": "newest:gpt-sol"}}`, or without `fallback` for
    /// none: one line of Fallback per tool (AGM-03). Answered with [`Reply::Agents`].
    #[serde(rename = "agents.fallback")]
    AgentsFallback(FallbackParams),
    /// `{"id": "0199…", "folder": "/abs/my-commit"}`: Import skill folder… onto the agent (WSK-02, WSK-03). Answered
    /// with [`Reply::Agent`], or refused (`bad_skill_folder`) with nothing changed.
    #[serde(rename = "agents.import_skill")]
    AgentsImportSkill(ImportSkillParams),
    /// `{"id": "0199…", "skill": "<imported id>"}`: Re-import from the remembered original (WSK-04);
    /// `original_not_found` when it is gone. Answered with [`Reply::Agent`].
    #[serde(rename = "agents.reimport_skill")]
    AgentsReimportSkill(ReimportSkillParams),
    /// `{"agent": "0199…", "project": "SHOP"}`: "What this agent reads" (AGR-01…AGR-03), read-only: its
    /// instructions, the project's notes, a skill part per stage it runs and one for its role, its other skills.
    /// Answered with [`Reply::AgentReads`].
    #[serde(rename = "agents.reads")]
    AgentsReads(crate::agent_reads::AgentReadsParams),
    /// **Pause** (PAU-01, PAU-02; d0112), `{"scope": "project", "project": "SHOP"}` or `{"scope": "all"}`: sets the
    /// pause first, then stops every agent session there at once; `agents.resume` lifts it and resumes what it
    /// stopped (PAU-06); `agents.pause_counts` reads the menus' counts. Each answered with [`Reply::PauseCounts`];
    /// every change publishes [`Event::PauseChanged`]. The user's only: agents are refused.
    #[serde(rename = "agents.pause")]
    AgentsPause(crate::agents::pause::PauseScope),
    #[serde(rename = "agents.resume")]
    AgentsResume(crate::agents::pause::PauseScope),
    #[serde(rename = "agents.pause_counts")]
    AgentsPauseCounts(crate::agents::pause::PauseScope),
    /// `{"key": "SHOP-19"}`, or `{"project": "SHOP", "new_epic": true}` for the ⌘N form: the agent each stage of the
    /// ticket's flow runs (an epic's lead), the sessions with their agents, the holds. Answered with
    /// [`Reply::TicketAgents`].
    #[serde(rename = "ticket.agents")]
    TicketAgents(TicketAgentsParams),
    /// `{"project": "SHOP"}`: what each board card says about its agents (the footer's name and who set it, who
    /// has the turn; T-0345). Answered with [`Reply::BoardAgents`].
    #[serde(rename = "board.agents")]
    BoardAgents(ProjectParams),
    /// Stage choices (AGM-08, AGM-09; T-0407). `{"key": "SHOP-19"}`, or `{"project": "SHOP"}` for the ⌘N form: the
    /// stages with options, each with what it offers, the ticket's choice and its lock. Answered with
    /// [`Reply::TicketChoices`].
    #[serde(rename = "ticket.choices")]
    TicketChoices(TicketChoicesParams),
    /// `{"key": "SHOP-19", "stage": "Dev", "choice": "Quick developer"}` (or `skip`, `required`): the user's choice
    /// for one stage, until it starts. Answered with [`Reply::TicketChoices`].
    #[serde(rename = "ticket.set_choice")]
    TicketSetChoice(SetChoiceParams),

    // More agent tools (M23; d0086, d0088). The user's requests; each answers "not built yet" until its ticket lands.
    /// Every agent tool's state on Settings → Agent tools (T-0347; T-0348 adds the rest of each row). Answered
    /// with [`Reply::AgentTools`].
    #[serde(rename = "agent_tools.list")]
    AgentToolsList,
    /// `{"tool": "codex"}`, or `{}` for every tool: detect again (Re-check, the page opening; T-0348).
    #[serde(rename = "agent_tools.recheck")]
    AgentToolsRecheck(RecheckParams),
    /// `{"tool": "copilot", "hidden": true}`: a found tool set Hidden so it doesn't count, or shown again.
    /// Answered with [`Reply::AgentTools`].
    #[serde(rename = "agent_tools.set_hidden")]
    AgentToolsSetHidden(SetHiddenParams),
    /// `{"tool": "codex"}`: **Sign in…**, the tool's own login in a terminal titled "Codex sign in" (T-0348).
    /// Answered with [`Reply::ProjectTerminal`] (number 0: no project's); attach with `session.attach {session}`.
    #[serde(rename = "agent_tools.sign_in")]
    AgentToolsSignIn(ToolParams),
    /// `{"tool": "codex"}`: **Install**, the row's How to install command in a terminal titled "Installing Codex",
    /// on the user's click only (PLT-03 revised). Answered with [`Reply::ProjectTerminal`] (number 0); the tool is
    /// checked again when it ends.
    #[serde(rename = "agent_tools.install")]
    AgentToolsInstall(ToolParams),
    /// `{"tool": "codex"}`: **Disconnect…**, removing Otaq's own entries from the tool's config (T-0348).
    #[serde(rename = "agent_tools.disconnect")]
    AgentToolsDisconnect(ToolParams),
    /// `{"tool": "claude"}`: Resume for one tool's agents waiting on its limit (USG-10; T-0358): "continue" for each,
    /// the tool's limit cleared, its holds lifted. Replies with [`Reply::Resumed`].
    #[serde(rename = "limits.resume_tool")]
    LimitsResumeTool(ToolParams),
    /// `{"tool": "claude"}`: **Use fallback now** (AGM-03): every ticket waiting on the tool starts on its fallback;
    /// refused when it has none or it can't start either. Replies with [`Reply::StartedOnFallback`].
    #[serde(rename = "limits.use_fallback_now")]
    LimitsUseFallbackNow(ToolParams),
    /// `{"key": "SHOP-31"}`: **Use fallback** for a ticket waiting on its tool or its model (AGM-03).
    #[serde(rename = "ticket.use_fallback")]
    TicketUseFallback(KeyParams),
    /// `{}` or `{"record": {"kind": "ticket", "key": "NOTE-2"}}`: the command palette's recent items (KEY-06, d0097),
    /// with the use recorded first when given. Answered with [`Reply::RecentItems`], newest first.
    #[serde(rename = "recent.items")]
    RecentItems(RecentItemsParams),
}

/// `onboarding.ask`: the project and the tool to ask (ASK-02).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OnboardingAskParams {
    pub project: String,
    pub tool: Tool,
}

/// A request about one agent tool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolParams {
    pub tool: Tool,
}

/// `agent_tools.recheck`: one tool, or every tool when `tool` is absent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecheckParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<Tool>,
}

/// `agent_tools.set_hidden`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetHiddenParams {
    pub tool: Tool,
    pub hidden: bool,
}

/// `agents.move_role`: the role and the agent that takes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MoveRoleParams {
    pub role: crate::agents::Role,
    pub agent: String,
}

/// `agents.default_model`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DefaultModelParams {
    pub model: crate::agents::ModelRef,
}

/// `agents.fallback`: `fallback` `None` = none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FallbackParams {
    pub tool: Tool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback: Option<crate::agents::ModelRef>,
}

/// `agents.import_skill`: the agent and the picked folder (absolute).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportSkillParams {
    pub id: String,
    pub folder: std::path::PathBuf,
}

/// `agents.reimport_skill`: the agent and its imported skill's id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReimportSkillParams {
    pub id: String,
    pub skill: String,
}

/// `recent.items`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecentItemsParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub record: Option<crate::store::recents::RecentUse>,
}

/// `agents.edit`: the agent's id and its whole page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentEditParams {
    pub id: String,
    #[serde(flatten)]
    pub fields: crate::agents::AgentFields,
}

/// `ticket.agents`: an existing ticket by `key`, else a new one in `project` (an epic when `new_epic`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TicketAgentsParams {
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default)]
    pub project: Option<String>,
    #[serde(default)]
    pub new_epic: bool,
}

/// `ticket.choices`: an existing ticket by `key`, else a new one in `project`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TicketChoicesParams {
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default)]
    pub project: Option<String>,
}

/// `ticket.set_choice`: a stage by name or id, and an agent's name, `skip` or `required`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetChoiceParams {
    pub key: String,
    pub stage: String,
    pub choice: String,
}

/// `{"id": "<question id>", "options": ["Logged-in users only", "Guests too"], "notes": [null, "Kept in the
/// browser."], "recommended": 1, "reason": "PAY-123 only mentions signed-in users."}`; without `options` the child's
/// options and notes stay.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PassOnParams {
    pub id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<String>,
    /// Each option's note (ATT-17), in the options' order, `null` for an option without one; omitted when none has
    /// one.
    #[serde(default, skip_serializing_if = "crate::board::questions::no_notes")]
    pub notes: Vec<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recommended: Option<u32>,
    pub reason: String,
}

/// `{"key": "SHOP-40.4", "text": "40.1 returns items newest first …"}`; to a stuck child also `"view": "The agent
/// keeps changing the API instead of the test"` (`--view`, LEAD-07), shown only if the child reaches the user later.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeadNoteParams {
    pub key: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view: Option<String>,
}

/// `{"key": "SHOP-40.6", "kind": "close", "reason": "PAY-123 doesn't ask for an export."}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SuggestParams {
    pub key: String,
    pub kind: crate::epic_lead::suggest::SuggestionKind,
    pub reason: String,
}

/// `{"id": "SHOP-40.1#1", "source": "PAY-123", "text": "items are listed in the order they were added"}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecisionMarkParams {
    pub id: String,
    pub source: String,
    pub text: String,
}

/// `{"epic": "SHOP-40"}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeadParams {
    pub epic: String,
}

/// `{"target": "SHOP-18", "question": "…"}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentAskParams {
    pub target: String,
    pub question: String,
}

/// `{"question": "Import closed issues too?", "options": ["Yes", "No"], "recommended": 2}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentAskHumanParams {
    pub question: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<String>,
    /// Each option's note (ATT-17), in the options' order, `null` for an option without one; omitted when none has
    /// one.
    #[serde(default, skip_serializing_if = "crate::board::questions::no_notes")]
    pub notes: Vec<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recommended: Option<u32>,
}

/// `{"id": "…", "choice": {"option": 1}}`; `clear` is the user's "Clear it and send".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectAgentAnswerParams {
    pub id: String,
    pub choice: crate::board::questions::Choice,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub clear: bool,
}

/// `{"id": "…", "message": "and the cart page?"}`: the user's message for a running Project agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectAgentSendParams {
    pub id: String,
    pub message: String,
}

/// `{"project": "SHOP", "message": "Why is checkout slow?"}`: open a Project agent with its first message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectAgentOpenParams {
    pub project: String,
    pub message: String,
    /// The named agent to start on instead of the project's Project agent default: the pane's "New agent on
    /// Sonnet" when the default's tool can't run (M23, CHANGES-v §4).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
}

/// `{"id": "…", "confirm": true}`: close a Project agent, stopping it mid-turn when confirmed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectAgentCloseParams {
    pub id: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub confirm: bool,
}

/// `{"id": "…", "name": "Checkout slowness"}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectAgentRenameParams {
    pub id: String,
    pub name: String,
}

/// `{"name": "Checkout slowness"}`; `rename` when the user asked the agent for a new name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentNameParams {
    pub name: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub rename: bool,
}

/// `otaq hook --tool claude Stop` (ADP-03): the tool, its event name as the tool spells it, and the JSON it passed
/// on stdin. Only the tool's adapter reads the event and the payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HookParams {
    pub tool: Tool,
    pub event: String,
    #[serde(default)]
    pub payload: Value,
    /// The session's role (`validator`, `project_agent`) from its environment; the agent's sessions have none
    /// (d0063, d0068). The service still tells sessions apart by their processes (CLI-01).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
}

/// `agents.models`, `agents.models_refresh` and `agents.models_retry`: whose list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelsParams {
    pub tool: Tool,
}

/// `usage.record_limits`: a tool's current usage windows, as its adapter read them from its report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecordLimitsParams {
    pub tool: Tool,
    pub windows: Vec<crate::agent_tools::LimitWindow>,
}

/// `{"key": "SHOP-23", "question": "…", "options": ["Postmark", "SES"], "recommended": 1}` (d0061).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AskHumanParams {
    pub key: String,
    pub question: String,
    /// 2–4 answer options, or none for a free-text question.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<String>,
    /// Each option's note (ATT-17), in the options' order, `null` for an option without one; omitted when none has
    /// one.
    #[serde(default, skip_serializing_if = "crate::board::questions::no_notes")]
    pub notes: Vec<Option<String>>,
    /// The recommended option, 1-based.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recommended: Option<u32>,
}

/// `{"key": "SHOP-23", "question": "…", "built": "…", "other_options": ["…"], "why": "…"}` (DEC-02).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecisionRecordParams {
    pub key: String,
    #[serde(flatten)]
    pub decision: crate::decisions::NewDecision,
}

/// Change… (DEC-06): `{"id": "SHOP-23#1", "text": "Split fullName everywhere"}` sends SHOP-23 back (`clear_first`
/// is the user's "Clear it and send"); `{"id": "SHOP-40.1#1", "title": "Change: …", "description": "…"}` adds a fix
/// child to the epic.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecisionChangeParams {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub clear_first: bool,
}

/// `{"key": "SHOP-18", "path": "lib/pricing.ts"}`; `child` (epics) limits it to that child's squash commit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffFileParams {
    pub key: String,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub child: Option<String>,
    /// Only what changed after this commit (the reviewed head: "changed since you reviewed it", REV-06).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since: Option<String>,
}

/// `{"key": "SHOP-22", "path": "src/middleware.ts"}`: a file or folder of the ticket's Files tab; `child` (epics)
/// reads that child's change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FilesPathParams {
    pub key: String,
    #[serde(default)]
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub child: Option<String>,
}

/// `{"key": "SHOP-22", "query": "log"}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FilesSearchParams {
    pub key: String,
    pub query: String,
}

/// `{"project": "SHOP", "path": "src/lib"}`: a file or folder of the project Files view; `base_only` lists the base's
/// entries without reading any ticket's changes (the tree shows at once, the marks follow).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectFilesPathParams {
    pub project: String,
    #[serde(default)]
    pub path: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub base_only: bool,
}

/// `{"project": "SHOP", "query": "pricing"}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectFilesSearchParams {
    pub project: String,
    pub query: String,
}

/// `{"project": "SHOP", "path": "src/lib/pricing.ts", "ticket": "SHOP-29"}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectFilesDiffParams {
    pub project: String,
    pub path: String,
    pub ticket: String,
}

/// `{"key": "SHOP-22", "note": "…"}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubmitByUserParams {
    pub key: String,
    #[serde(default)]
    pub note: String,
}

/// `{"key": "SHOP-25"}`; `limit` caps the entries (default 40).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BeforeRestartParams {
    pub key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}

/// `{"key": "SHOP-23", "choice": {"option": 1}}` or `{"key": "SHOP-23", "choice": {"other": "Mailgun"}}`;
/// `clear` is the user's "Clear it and send".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InboxAnswerParams {
    pub key: String,
    pub choice: crate::board::questions::Choice,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub clear: bool,
}

/// `{"items": [{"group": "branches", "id": "SHOP:otaq/SHOP-9"}], "confirm": true}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CleanupDeleteParams {
    pub items: Vec<crate::recovery::cleanup::CleanupTarget>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub confirm: bool,
}

/// `{"project": "SHOP"}`, or `{"repo": "old-app"}` for a warm build no project owns.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WarmClearParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
}

/// `{}` for every project, `{"project": "SHOP"}` for one.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResumeAllParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
}

/// `{"key": "SHOP-19", "confirm": true}`: `confirm` is the user's yes to stopping running agents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloseParams {
    pub key: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub confirm: bool,
}

/// `{"key": "SHOP-18", "message": "SHOP-18: Checkout API\n\n…"}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalMergeParams {
    pub key: String,
    pub message: String,
    /// The user saw "changed since you reviewed it" and chose Approve anyway (REV-06).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub approve_anyway: bool,
}

/// `{"key": "SHOP-18", "note": "…"}`: a Fail's comments; `clear_first` is the user's "Clear it and send". The note
/// is the user's comment (CMT-05), `report` a report attached to it (the dialog's pasted text, CMT-13).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FailParams {
    pub key: String,
    #[serde(default)]
    pub note: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub clear_first: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub report: Option<crate::board::reports::ReportSource>,
}

/// `{"id": "…", "body": "…"}`: a reply to, or the new text of, a comment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommentTextParams {
    pub id: String,
    pub body: String,
}

/// `{"id": "…"}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommentIdParams {
    pub id: String,
}

/// `{"key": "SHOP-18"}`, or `{"key": "SHOP-18", "round": 2}` for one round.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommentsParams {
    pub key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub round: Option<u32>,
}

/// `{"key": "SHOP-34", "body": "…"}`, with `"report": {"path": "/abs/test-run.txt"}` or `{"text": "…"}`: a new
/// ticket comment (CMT-06, CMT-11). `"tell_agent": true` also tells the ticket's running agent (or check) to read it
/// (d0123): the user's comments only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TicketCommentPostParams {
    pub key: String,
    pub body: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub report: Option<crate::board::reports::ReportSource>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub tell_agent: bool,
}

/// `{"key": "SHOP-34", "n": 2, "body": "…"}`: comment 2's new text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TicketCommentEditParams {
    pub key: String,
    pub n: u32,
    pub body: String,
}

/// `{"key": "SHOP-34", "n": 4}`: one ticket comment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TicketCommentParams {
    pub key: String,
    pub n: u32,
}

/// `{"key": "SHOP-18", "summary": "…"}`: how an agent ends its stage (`stage next`: the summary; `stage done`: the
/// reason), its comment (CMT-05); `report_file` (`{"path": "/abs/test-run.txt"}`) attaches a report (CMT-06).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StageEndParams {
    pub key: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub report_file: Option<crate::board::reports::ReportSource>,
}

/// `{"key": "SHOP-32", "action": "answer_agent", "note": "…"}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValidationActionParams {
    pub key: String,
    pub action: crate::domain::ValidationAction,
    /// Answer the agent's note.
    #[serde(default)]
    pub note: String,
}

/// `{"key": "SHOP-29", "note": "Not covered by any ticket: refunds endpoint"}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteParams {
    pub key: String,
    pub note: String,
}

/// `{"key": "SHOP-29", "pass": false, "report": "…"}`: an agent check's verdict (VAL-03, `otaq check pass|fail`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerdictParams {
    pub key: String,
    pub pass: bool,
    /// The findings (one line each, file and line) and the notes: the checker's comment (CMT-05).
    #[serde(default)]
    pub report: String,
    /// `--report-file`: the full output attached to the comment (CMT-06).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub report_file: Option<crate::board::reports::ReportSource>,
}

/// `{"key": "SHOP-18", "proposal": "{…}"}`: the proposal as the skill wrote it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrProposeParams {
    pub key: String,
    pub proposal: String,
}

/// `{"key": "SHOP-18", "url": "https://github.com/acme/shop-web/pull/481"}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrOpenedParams {
    pub key: String,
    pub url: String,
}

/// `{"key": "SHOP-20", "target": "SHOP-18", "question": "…"}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AskTicketParams {
    pub key: String,
    pub target: String,
    pub question: String,
}

/// `{"id": "0199…"}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdParams {
    pub id: String,
}

/// `{"id": "0199…", "answer": "…"}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnswerParams {
    pub id: String,
    pub answer: String,
}

/// `{"key": "SHOP-18", "approve_anyway": false, "clear": false}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenPrParams {
    pub key: String,
    /// The user chose Approve anyway after "changed since you reviewed it" (REV-06).
    #[serde(default)]
    pub approve_anyway: bool,
    /// "Clear it and send" after a needs-confirmation answer (d0048).
    #[serde(default)]
    pub clear: bool,
}

/// The confirmed proposal (every field as the user left it).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrConfirmParams {
    pub key: String,
    pub proposal: crate::pr::proposal::Proposal,
    #[serde(default)]
    pub clear: bool,
}

/// `{"key": "SHOP-18", "name": "bugfix/SHOP-18-login-redirect"}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BranchRenameParams {
    pub key: String,
    pub name: String,
}

/// A batch of new tickets for one project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatchParams {
    pub project: String,
    pub tickets: Vec<BatchTicket>,
    /// The tickets' stage choices are the user's (`--by-user`, d0087): written *set by you*.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub by_user: bool,
}

/// `{"path": "/Users/me/code/shop-web"}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathParams {
    pub path: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageParams {
    /// The project whose tickets' tokens to include (its key).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
}

/// `{"key": "SHOP-18"}` (a ticket) or `{"key": "SHOP"}` (a project).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyParams {
    pub key: String,
}

/// `{"key": "SHOP-22"}`: the ticket's agent session; `{"key": "SHOP-29", "stage": "security"}`: the session of the
/// agent running that stage (its id or name, any case), refused unless the ticket is in it (FLW-20).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalParams {
    pub key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage: Option<String>,
}

/// The answer to `ticket.terminal`: the screen and the lines above it as plain text
/// ([`crate::session::screen::render`]), and the session it came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TicketTerminal {
    pub key: TicketKey,
    pub session: String,
    pub text: String,
}

/// `{"project": "SHOP"}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectParams {
    pub project: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectUpdateParams {
    /// The project's key. A `key` among the changes is the (refused) attempt to change it.
    pub project: String,
    #[serde(flatten)]
    pub update: ProjectUpdate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TicketUpdateParams {
    pub key: String,
    #[serde(flatten)]
    pub update: TicketUpdate,
}

/// A drop or "Start". `confirmed` answers a confirmation, a choice or a warning ("Start anyway").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MoveParams {
    pub key: String,
    pub target: DropTarget,
    #[serde(default)]
    pub confirmed: bool,
}

/// A drop on any stage: where (a board target, or `{"stage": "security"}`), and the dialog's answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DragParams {
    pub key: String,
    pub target: DropOn,
    #[serde(default)]
    pub confirmed: bool,
    /// The instructions for the Agent stage, or the note for the check: the user's comment (CMT-05).
    #[serde(default)]
    pub note: String,
    /// "Clear it and send" after the unsent-text question (d0048).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub clear_first: bool,
    /// A report attached to the note's comment (the dialog's pasted text, CMT-13).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub report: Option<crate::board::reports::ReportSource>,
}

/// An epic dropped on Ready for Dev or Dev (or ⇧⌘D on it); `child` when one row of it was dropped.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EpicQueueParams {
    /// The epic, e.g. SHOP-45.
    pub key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub child: Option<String>,
}

/// The queue toast's Undo: the epic and the keys its queue moved (the outcome's `queued` and `resumed`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EpicUndoQueueParams {
    /// The epic, e.g. SHOP-45.
    pub key: String,
    #[serde(default)]
    pub keys: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MoveToEpicParams {
    pub key: String,
    pub epic: String,
}

/// `child` depends on `parent`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DepParams {
    pub child: String,
    pub parent: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectSettingsParams {
    pub project: String,
    /// Boxed: the settings are the largest request by far (clippy `large_enum_variant`); the JSON is the same.
    pub settings: Box<ProjectSettings>,
}

/// A project's view preferences (`project_view.set`, SHB-05).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectViewParams {
    pub project: String,
    pub view: crate::board::settings::ViewPrefs,
}

/// A checklist row's tool and what to run for it (`tools.terminal`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolTerminalParams {
    pub tool: crate::tools::Tool,
    pub action: crate::tools::ToolAction,
}

/// A ticket's Shell; `reopen` is the user's click after it ended.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellOpenParams {
    pub key: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub reopen: bool,
}

/// A project's Project terminal; `new` is the user's click for another one (or Reopen after one ended).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectTerminalParams {
    pub project: String,
    #[serde(default, alias = "reopen", skip_serializing_if = "std::ops::Not::not")]
    pub new: bool,
}

/// `{"key": "SHOP-18"}` for a ticket's sessions, `{}` for every session in memory.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionListParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
}

/// A session by id (`{"session": "…"}`), a ticket's newest agent session by key (`{"key": "SHOP-18"}`), its newest
/// session of another kind (`{"key": "SHOP-18", "kind": "shell"}`), the one running a stage (`{"key": "SHOP-29",
/// "stage": "security"}`), or a project's Project terminal
/// (`{"project": "SHOP", "kind": "project"}`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionTarget {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// The kind of the ticket's session (agent when not given).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<crate::session::SessionKind>,
    /// With `key`: the session of the agent running this stage of the ticket's flow (its id or name, any case),
    /// refused unless the ticket is in it (FLW-20); `kind` is then not given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage: Option<String>,
    /// A project's key, for its Project terminal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
}

/// `session.attach`: the session, and whether this client's terminal stays silent to the program's questions.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttachParams {
    #[serde(flatten)]
    pub target: SessionTarget,
    /// The terminal doesn't answer the program's questions (a read-only view). While no terminal of any client
    /// answers, the service answers the colour questions itself (d0117 point 4). The latest attach of a session
    /// on a connection says it for that connection.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub quiet: bool,
}

impl From<SessionTarget> for AttachParams {
    /// An attach whose terminal answers the program's questions.
    fn from(target: SessionTarget) -> Self {
        Self {
            target,
            quiet: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionIdParams {
    pub session: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResizeParams {
    pub session: String,
    #[serde(flatten)]
    pub size: TerminalSize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliverNowParams {
    #[serde(flatten)]
    pub target: SessionTarget,
    /// The user chose "Clear it and send".
    #[serde(default)]
    pub clear_first: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FocusParams {
    pub session: String,
    pub focused: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppFocusParams {
    pub focused: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputParams {
    pub session: String,
    /// The bytes typed, base64 ([`encode_bytes`]).
    pub data: String,
    /// The client's terminal answering the program (cursor position, colours), not a person typing: the bytes
    /// don't touch the unsent-input flag.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub reply: bool,
}

/// The answer to `session.attach`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionAttach {
    pub session: SessionInfo,
    /// The stored output, base64 ([`encode_bytes`]).
    pub scrollback: String,
    /// The offset of the first stored byte in the session's whole output.
    pub start: u64,
    /// The offset after the last stored byte; `session_output` events continue from here.
    pub end: u64,
}

/// Terminal bytes travel as base64 inside JSON (they need not be UTF-8).
pub fn encode_bytes(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

pub fn decode_bytes(text: &str) -> Result<Vec<u8>, base64::DecodeError> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.decode(text)
}

impl Request {
    /// The method name as sent on the wire.
    pub fn method(&self) -> &'static str {
        match self {
            Self::Ping => "ping",
            Self::ServiceStatus => "service.status",
            Self::ServiceStop => "service.stop",
            Self::StartupReport => "service.startup_report",
            Self::Subscribe => "events.subscribe",
            Self::ToolsCheck => "tools.check",
            Self::ToolTerminalOpen(_) => "tools.terminal",
            Self::OnboardingState => "onboarding.state",
            Self::OnboardingSources(_) => "onboarding.sources",
            Self::OnboardingAsk(_) => "onboarding.ask",
            Self::OnboardingFlowSetup(_) => "onboarding.flow_setup",
            Self::OnboardingUseDefault(_) => "onboarding.use_default",
            Self::DismissPermissionWarning => "onboarding.dismiss_permission_warning",
            Self::DismissTrustNote => "notices.dismiss_trust_note",
            Self::NotificationsState => "notifications.state",
            Self::NotificationsAskAnswered => "notifications.ask_answered",
            Self::NotificationsDismissBlocked => "notifications.dismiss_blocked",
            Self::ProjectList => "project.list",
            Self::ProjectProbe(_) => "project.probe",
            Self::ProjectAdd(_) => "project.add",
            Self::ProjectUpdate(_) => "project.update",
            Self::ProjectRemove(_) => "project.remove",
            Self::ProjectBranches(_) => "project.branches",
            Self::ProjectPrProvider(_) => "project.pr_provider",
            Self::BoardSnapshot(_) => "board.snapshot",
            Self::TicketGet(_) => "ticket.get",
            Self::TicketList(_) => "ticket.list",
            Self::TicketCreate(_) => "ticket.create",
            Self::TicketCreateBatch(_) => "ticket.create_batch",
            Self::TicketUpdate(_) => "ticket.update",
            Self::TicketMove(_) => "ticket.move",
            Self::TicketDrag(_) => "ticket.drag",
            Self::TicketPin(_) => "ticket.pin",
            Self::TicketUnpin(_) => "ticket.unpin",
            Self::TicketAgentEdits(_) => "ticket.agent_edits",
            Self::TicketAgentEditsSeen(_) => "ticket.agent_edits_seen",
            Self::TicketDeletePreview(_) => "ticket.delete_preview",
            Self::TicketDelete(_) => "ticket.delete",
            Self::TicketRetryStart(_) => "ticket.retry_start",
            Self::TicketHistory(_) => "ticket.history",
            Self::TicketClose(_) => "ticket.close",
            Self::TicketReopen(_) => "ticket.reopen",
            Self::TicketSubmitByUser(_) => "ticket.submit_by_user",
            Self::TicketPark(_) => "ticket.park",
            Self::TicketResume(_) => "ticket.resume",
            Self::TicketRetryResume(_) => "ticket.retry_resume",
            Self::TicketStartFromScratch(_) => "ticket.start_from_scratch",
            Self::TicketsResumeAll(_) => "tickets.resume_all",
            Self::InboxAnswer(_) => "inbox.answer",
            Self::InboxList => "inbox.list",
            Self::InboxCounts => "inbox.counts",
            Self::CleanupScan => "cleanup.scan",
            Self::CleanupDelete(_) => "cleanup.delete",
            Self::WarmList => "warm.list",
            Self::WarmClear(_) => "warm.clear",
            Self::DiffFiles(_) => "diff.files",
            Self::DiffFile(_) => "diff.file",
            Self::FilesTree(_) => "files.tree",
            Self::FilesSearch(_) => "files.search",
            Self::FilesRead(_) => "files.read",
            Self::FilesDiff(_) => "files.diff",
            Self::FilesChanges(_) => "files.changes",
            Self::FilesProjectTree(_) => "files.project_tree",
            Self::FilesProjectSearch(_) => "files.project_search",
            Self::FilesProjectRead(_) => "files.project_read",
            Self::FilesProjectDiff(_) => "files.project_diff",
            Self::FilesProjectChanges(_) => "files.project_changes",
            Self::FilesProjectSources(_) => "files.project_sources",
            Self::ReviewCommentAdd(_) => "review.comment_add",
            Self::ReviewCommentReply(_) => "review.comment_reply",
            Self::ReviewCommentEdit(_) => "review.comment_edit",
            Self::ReviewCommentDelete(_) => "review.comment_delete",
            Self::ReviewComments(_) => "review.comments",
            Self::TicketCommentPost(_) => "ticket.comment_post",
            Self::TicketCommentEdit(_) => "ticket.comment_edit",
            Self::TicketCommentDelete(_) => "ticket.comment_delete",
            Self::TicketComments(_) => "ticket.comments",
            Self::TicketReport(_) => "ticket.report",
            Self::TicketFail(_) => "ticket.fail",
            Self::TicketPass(_) => "ticket.pass",
            Self::ReviewFeedback(_) => "review.feedback",
            Self::TicketMergeProposal(_) => "ticket.merge_proposal",
            Self::TicketLocalMerge(_) => "ticket.local_merge",
            Self::TicketMergeCheckStop(_) => "ticket.merge_check_stop",
            Self::TicketApproveCheck(_) => "ticket.approve_check",
            Self::TicketConflict(_) => "ticket.conflict",
            Self::TicketSendBackPr(_) => "ticket.send_back_pr",
            Self::TicketAddChildFromPr(_) => "ticket.add_child_from_pr",
            Self::TicketPrime(_) => "ticket.prime",
            Self::TicketAskHuman(_) => "ticket.ask_human",
            Self::DecisionRecord(_) => "decision.record",
            Self::DecisionList(_) => "decision.list",
            Self::DecisionChange(_) => "decision.change",
            Self::DecisionMark(_) => "decision.mark",
            Self::TicketStageNext(_) => "ticket.stage_next",
            Self::TicketStageDone(_) => "ticket.stage_done",
            Self::ValidationVerdict(_) => "validation.verdict",
            Self::TicketPrimeValidate(_) => "ticket.prime_validate",
            Self::TicketValidations(_) => "ticket.validations",
            Self::TicketCreateFromNote(_) => "ticket.create_from_note",
            Self::TicketValidationAction(_) => "ticket.validation_action",
            Self::TicketPrPropose(_) => "ticket.pr_propose",
            Self::TicketPrOpened(_) => "ticket.pr_opened",
            Self::TicketBranchRename(_) => "ticket.branch_rename",
            Self::TicketAsk(_) => "ticket.ask",
            Self::MessageAnswer(_) => "message.answer",
            Self::MessageResume(_) => "message.resume",
            Self::TicketQuestions(_) => "ticket.questions",
            Self::TicketMessages(_) => "ticket.messages",
            Self::TicketHumanAnswers(_) => "ticket.human_answers",
            Self::TicketOpenPr(_) => "ticket.open_pr",
            Self::TicketPrConfirm(_) => "ticket.pr_confirm",
            Self::TicketPrCancel(_) => "ticket.pr_cancel",
            Self::TicketPrRetry(_) => "ticket.pr_retry",
            Self::TicketPrFlow(_) => "ticket.pr_flow",
            Self::TicketPrBranchCheck(_) => "ticket.pr_branch_check",
            Self::TicketMarkMerged(_) => "ticket.mark_merged",
            Self::TicketMarkClosed(_) => "ticket.mark_closed",
            Self::EpicMoveTo(_) => "epic.move_to",
            Self::EpicClosePlan(_) => "epic.close_plan",
            Self::EpicClose(_) => "epic.close",
            Self::EpicQueue(_) => "epic.queue",
            Self::EpicTakeBack(_) => "epic.take_back",
            Self::EpicUndoQueue(_) => "epic.undo_queue",
            Self::DepAdd(_) => "dep.add",
            Self::DepRemove(_) => "dep.remove",
            Self::DepTree(_) => "dep.tree",
            Self::DepCycles(_) => "dep.cycles",
            Self::DepReadiness(_) => "dep.readiness",
            Self::SchedulerStatus => "scheduler.status",
            Self::UsageSummary(_) => "usage.summary",
            Self::UsageSessions(_) => "usage.sessions",
            Self::UsageActivity => "usage.activity",
            Self::UsageRecordLimits(_) => "usage.record_limits",
            Self::AgentsModels(_) => "agents.models",
            Self::AgentsModelsRefresh(_) => "agents.models_refresh",
            Self::AgentsModelsRetry(_) => "agents.models_retry",
            Self::UsageMarkLimit(_) => "usage.mark_limit",
            Self::SettingsGet => "settings.get",
            Self::SettingsSet(_) => "settings.set",
            Self::ProjectSettingsGet(_) => "project_settings.get",
            Self::ProjectSettingsSet(_) => "project_settings.set",
            Self::ProjectViewSet(_) => "project_view.set",
            Self::FlowCurrent(_) => "flow.current",
            Self::FlowVersions(_) => "flow.versions",
            Self::FlowProposal(_) => "flow.proposal",
            Self::FlowApply(_) => "flow.apply",
            Self::FlowCancelProposal(_) => "flow.cancel_proposal",
            Self::FlowWaiting(_) => "flow.waiting",
            Self::FlowCancelWaiting(_) => "flow.cancel_waiting",
            Self::FlowApplyNowPlan(_) => "flow.apply_now_plan",
            Self::FlowApplyNow(_) => "flow.apply_now",
            Self::FlowProposeFrom(_) => "flow.propose_from",
            Self::FlowShow(_) => "flow.show",
            Self::FlowPropose(_) => "flow.propose",
            Self::FlowAskForChanges(_) => "flow.ask_for_changes",
            Self::FlowDraft(_) => "flow.draft",
            Self::FlowDraftUpdate(_) => "flow.draft_update",
            Self::FlowWriteSkill(_) => "flow.write_skill",
            Self::SessionList(_) => "session.list",
            Self::SessionAttach(_) => "session.attach",
            Self::SessionDetach(_) => "session.detach",
            Self::SessionResize(_) => "session.resize",
            Self::SessionInput(_) => "session.input",
            Self::SessionDeliverNow(_) => "session.deliver_now",
            Self::SessionSendAgain(_) => "session.send_again",
            Self::SessionFocus(_) => "session.focus",
            Self::AppFocus(_) => "app.focus",
            Self::SessionBeforeRestart(_) => "session.before_restart",
            Self::Hook(_) => "hook",
            Self::RunStart(_) => "run.start",
            Self::RunStop(_) => "run.stop",
            Self::RunStatus(_) => "run.status",
            Self::RunList => "run.list",
            Self::RunSuggest(_) => "run.suggest",
            Self::RunSuggestCancel(_) => "run.suggest_cancel",
            Self::ShellOpen(_) => "shell.open",
            Self::ProjectTerminalOpen(_) => "project_terminal.open",
            Self::ProjectTerminalList(_) => "project_terminal.list",
            Self::ProjectTerminalClose(_) => "project_terminal.close",
            Self::ProjectTerminalCreated(_) => "project_terminal.created",
            Self::ProjectTerminalDismiss(_) => "project_terminal.dismiss",
            Self::ProjectAgentOpen(_) => "project_agent.open",
            Self::ProjectAgentList(_) => "project_agent.list",
            Self::ProjectAgentClose(_) => "project_agent.close",
            Self::ProjectAgentCloseEnded(_) => "project_agent.close_ended",
            Self::ProjectAgentResume(_) => "project_agent.resume",
            Self::ProjectAgentRename(_) => "project_agent.rename",
            Self::AgentName(_) => "agent.name",
            Self::AgentAsk(_) => "agent.ask",
            Self::AgentAskHuman(_) => "agent.ask_human",
            Self::ProjectAgentAnswer(_) => "project_agent.answer",
            Self::ProjectAgentSend(_) => "project_agent.send",
            Self::ProjectAgentPrime(_) => "project_agent.prime",
            Self::ProjectAgentCreated(_) => "project_agent.created",
            Self::LeadOpen(_) => "lead.open",
            Self::LeadResume(_) => "lead.resume",
            Self::LeadSuggest(_) => "lead.suggest",
            Self::SuggestionConfirm(_) => "suggestion.confirm",
            Self::SuggestionDismiss(_) => "suggestion.dismiss",
            Self::LeadList(_) => "lead.list",
            Self::LeadPassOn(_) => "lead.pass_on",
            Self::LeadQuestions => "lead.questions",
            Self::LeadPrime(_) => "lead.prime",
            Self::LeadNote(_) => "lead.note",
            Self::TicketNotes(_) => "ticket.notes",
            Self::TicketLeadLog(_) => "ticket.lead_log",
            Self::TicketTerminal(_) => "ticket.terminal",
            Self::AgentsList => "agents.list",
            Self::AgentsAdd(_) => "agents.add",
            Self::AgentsEdit(_) => "agents.edit",
            Self::AgentsDuplicate(_) => "agents.duplicate",
            Self::AgentsDelete(_) => "agents.delete",
            Self::AgentsInUse(_) => "agents.in_use",
            Self::AgentsMoveRole(_) => "agents.move_role",
            Self::AgentsDefaultModel(_) => "agents.default_model",
            Self::AgentsFallback(_) => "agents.fallback",
            Self::AgentsImportSkill(_) => "agents.import_skill",
            Self::AgentsReimportSkill(_) => "agents.reimport_skill",
            Self::AgentsReads(_) => "agents.reads",
            Self::AgentsPause(_) => "agents.pause",
            Self::AgentsResume(_) => "agents.resume",
            Self::AgentsPauseCounts(_) => "agents.pause_counts",
            Self::TicketAgents(_) => "ticket.agents",
            Self::BoardAgents(_) => "board.agents",
            Self::TicketChoices(_) => "ticket.choices",
            Self::TicketSetChoice(_) => "ticket.set_choice",
            Self::AgentToolsList => "agent_tools.list",
            Self::AgentToolsRecheck(_) => "agent_tools.recheck",
            Self::AgentToolsSetHidden(_) => "agent_tools.set_hidden",
            Self::AgentToolsSignIn(_) => "agent_tools.sign_in",
            Self::AgentToolsInstall(_) => "agent_tools.install",
            Self::AgentToolsDisconnect(_) => "agent_tools.disconnect",
            Self::LimitsResumeTool(_) => "limits.resume_tool",
            Self::LimitsUseFallbackNow(_) => "limits.use_fallback_now",
            Self::TicketUseFallback(_) => "ticket.use_fallback",
            Self::RecentItems(_) => "recent.items",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reply {
    Ok,
    Pong,
    ServiceStatus(ServiceStatus),
    StartupReport(Box<crate::recovery::StartupReport>),
    ToolsCheck(ToolsReport),
    OnboardingState(OnboardingState),
    OnboardingSources(crate::onboarding::Sources),
    FlowSetup(Box<crate::project_agents::first_flow::FlowSetup>),
    NotificationsState(crate::attention::os_permission::NotificationsState),
    Projects(Vec<ProjectInfo>),
    Project(ProjectInfo),
    /// The answer to `project.add`: the project, and whether the agent tools' trust was recorded for its folder
    /// (PRJ-06). No `trust` when no tool's config file was located. A failed trust never fails the add.
    ProjectAdded {
        project: ProjectInfo,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        trust: Option<crate::agent_tools::SetupOutcome>,
    },
    FolderProbe(FolderProbe),
    Branches(Branches),
    PrProvider(crate::pr::provider::PrProvider),
    PrOutcome(crate::pr::service::PrOutcome),
    PrFlow(Box<crate::pr::flow::PrFlowView>),
    BranchCheck(crate::pr::flow::BranchCheck),
    MarkedMerged(crate::review::pr_state::MarkedMerged),
    Message(Box<crate::messages::Message>),
    Messages(Vec<crate::messages::Message>),
    HumanAnswers(Vec<crate::board::questions::HumanAnswer>),
    Decision(Box<crate::decisions::Decision>),
    Decisions(Vec<crate::decisions::Decision>),
    DecisionChanged(crate::decisions::ChangeOutcome),
    Suggestion(Box<crate::epic_lead::suggest::Suggestion>),
    LeadQuestion(Box<crate::epic_lead::questions::LeadQuestion>),
    LeadQuestions(Vec<crate::epic_lead::questions::LeadQuestion>),
    EpicPrime(Box<crate::epic_lead::prime::EpicPrime>),
    LeadNote(Box<crate::epic_lead::notes::LeadNote>),
    LeadNotes(Vec<crate::epic_lead::notes::LeadNote>),
    LeadLog(Box<crate::epic_lead::log::LeadLog>),
    Board(BoardSnapshot),
    Ticket(Box<Ticket>),
    /// The answer to `ticket.stage_next` and `ticket.stage_done` (AGM-10): the ticket and what the agent is told
    /// ("Finished Dev. Validate is running; …").
    StageEnded(Box<crate::board::review::StageEnded>),
    Validations(Vec<crate::board::validation::ValidationRecord>),
    Tickets(Vec<Ticket>),
    Readiness(Vec<Readiness>),
    SchedulerStatus(SchedulerStatus),
    Usage(Box<crate::usage::UsageSummary>),
    SessionUsage(Vec<crate::usage::SessionUsage>),
    Activity(Box<crate::usage::activity::Activity>),
    AgentEdits(AgentEdits),
    ClosePlan(ClosePlan),
    EpicQueued(EpicQueueOutcome),
    EpicTakenBack(TakeBackPlan),
    EpicQueueUndone(UndoQueueOutcome),
    DeletePreview(DeletePreview),
    DepTree(DepTree),
    /// Dependency loops, each starting and ending on the same ticket; empty on a valid database.
    Cycles(Vec<Vec<TicketKey>>),
    GlobalSettings(Box<GlobalSettingsReply>),
    ProjectSettings(Box<ProjectSettingsReply>),
    FlowVersion(Box<crate::flow::FlowVersion>),
    FlowVersions(Vec<crate::flow::FlowVersion>),
    FlowProposal(Option<Box<crate::flow::proposal::Proposal>>),
    FlowDraft(Box<crate::flow::draft::Draft>),
    SkillWrite(Box<crate::flow::write_skill::SkillWrite>),
    FlowApplied(Box<crate::flow::apply::Applied>),
    FlowWaiting(Option<Box<crate::flow::apply::WaitingChange>>),
    ApplyNowPlan(Option<Box<crate::flow::apply_now::ApplyNowPlan>>),
    AppliedNow(Box<crate::flow::apply_now::ApplyNowAnswer>),
    FlowDocument(Box<crate::flow::propose::FlowDocument>),
    Sessions(Vec<SessionInfo>),
    SessionAttached(Box<SessionAttach>),
    Delivery(Delivery),
    History(Vec<HistoryEvent>),
    Prime(Box<PrimeContext>),
    PrimeValidate(Box<crate::prime::ValidatePrime>),
    DiffSummary(Box<DiffSummary>),
    FileDiff(Box<FileDiff>),
    FilesTree(Box<crate::board::files::FilesTree>),
    FilesSearch(Box<crate::board::files::FilesSearch>),
    FileContent(Box<crate::board::files::FileContent>),
    FilesChanges(Box<crate::board::files::FilesChanges>),
    ProjectTree(Box<crate::board::project_files::ProjectTree>),
    ProjectSearch(Box<crate::board::project_files::ProjectSearch>),
    ProjectFile(Box<crate::board::project_files::ProjectFile>),
    ProjectChanges(Box<crate::board::project_files::ProjectChanges>),
    ProjectSources(Box<crate::board::project_files::ProjectSources>),
    ReviewComment(ReviewComment),
    ReviewComments(ReviewComments),
    TicketComment(Box<crate::board::ticket_comments::TicketComment>),
    TicketComments(crate::board::ticket_comments::TicketComments),
    TicketReport(Box<crate::board::ticket_comments::ReportFile>),
    Returned(ReturnOutcome),
    Passed(PassOutcome),
    ReviewFeedback(ReviewFeedback),
    MergeProposal(MergeProposal),
    LocalMerge(LocalMergeOutcome),
    ApproveCheck(ApproveCheck),
    Conflict(Option<Conflict>),
    /// The tickets resumed (`tickets.resume_all`, or a tool's Resume after its limit), in key order.
    Resumed(Vec<TicketKey>),
    /// The tickets "This is the usage limit" folded into the banner.
    LimitMarked(Vec<TicketKey>),
    /// The tickets Use fallback now started on the tool's fallback, in key order.
    StartedOnFallback(Vec<TicketKey>),
    BeforeRestart(Box<crate::recovery::BeforeRestart>),
    Inbox(Box<crate::attention::inbox::InboxList>),
    InboxCounts(crate::attention::inbox::InboxCounts),
    CleanupScan(Vec<crate::recovery::cleanup::CleanupItem>),
    CleanupOutcome(crate::recovery::cleanup::CleanupOutcome),
    WarmList(Box<crate::warm::store::WarmList>),
    WarmClear(crate::warm::store::ClearOutcome),
    Run(Box<crate::terminals::run::RunInfo>),
    Runs(Vec<crate::terminals::run::RunInfo>),
    Session(Box<SessionInfo>),
    CreatedHere(Box<crate::board::created::CreatedHere>),
    ProjectTerminal(Box<crate::terminals::shell::ProjectTerminal>),
    ProjectTerminals(Vec<crate::terminals::shell::ProjectTerminal>),
    ProjectAgent(Box<crate::project_agents::ProjectAgent>),
    ProjectAgents(Box<crate::project_agents::ProjectAgentList>),
    ProjectPrime(Box<crate::project_agents::prime::ProjectPrime>),
    CreatedAndChanged(Box<crate::project_agents::CreatedAndChanged>),
    Lead(Box<crate::project_agents::lead::Lead>),
    Leads(Vec<crate::project_agents::lead::Lead>),
    TicketTerminal(Box<TicketTerminal>),
    /// A tool's model list as Otaq's copy has it; `None` before it was first read.
    Models(Option<Box<crate::agent_tools::models::ModelCopy>>),
    Agents(Box<crate::agents::AgentList>),
    Agent(Box<crate::agents::Agent>),
    AgentDraft(Box<crate::agents::AgentFields>),
    TicketAgents(Box<crate::agents::facts::TicketAgents>),
    BoardAgents(Box<crate::agents::facts::BoardAgents>),
    TicketChoices(Box<crate::board::stage_choices::TicketChoices>),
    AgentInUse(Box<crate::agents::InUse>),
    AgentReads(Box<crate::agent_reads::AgentReads>),
    /// What Pause and Resume cover (`agents.pause`, `agents.resume`, `agents.pause_counts`).
    PauseCounts(Box<crate::agents::pause::PauseCounts>),
    /// Every agent tool's row (`agent_tools.list`, `agent_tools.set_hidden`).
    AgentTools(Box<crate::agent_tools::connected::AgentToolsList>),
    /// The command palette's recent items, newest first (`recent.items`).
    RecentItems(Vec<crate::store::recents::RecentItem>),
}

/// The global settings with any values flagged as invalid.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GlobalSettingsReply {
    pub settings: GlobalSettings,
    pub problems: Vec<SettingProblem>,
}

/// A project's settings with any values flagged as invalid.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectSettingsReply {
    pub settings: ProjectSettings,
    pub problems: Vec<SettingProblem>,
    /// What the project uses after falling back to the global settings, e.g. that Auto-merge does nothing
    /// while validation is off (d0064).
    pub effective: EffectiveSettings,
}

/// The answer to `onboarding.state`: the first run, and the banners kept by the service. A change to the folder
/// trust fields sends [`Event::TrustChanged`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OnboardingState {
    /// First run means no project is stored yet (UI-08).
    pub has_projects: bool,
    pub permission_warning_dismissed: bool,
    /// The one-time note that Otaq recorded Claude Code's trust for projects added earlier (PRJ-08): shown until
    /// `notices.dismiss_trust_note`.
    #[serde(default)]
    pub trust_note: bool,
    /// The tools the note names, in d0086's order: those whose trust a startup pass recorded, and a tool connected
    /// later ("Otaq let Codex work in…"), once per tool.
    #[serde(default)]
    pub trust_note_tools: Vec<Tool>,
    /// Stored projects whose folder trust couldn't be recorded, by key (PRJ-08): one warning banner. A project
    /// leaves the list when a later startup records its trust.
    #[serde(default)]
    pub trust_failed: Vec<crate::agent_tools::setup::TrustFailure>,
}

/// The service's answer to `service.status`. Version-independent: fields may be added (with defaults), never
/// renamed or removed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceStatus {
    pub pid: u32,
    pub build_id: String,
    pub protocol_version: u32,
    /// When the service started, in Unix milliseconds.
    pub started_at: i64,
    pub uptime_secs: u64,
    /// Agents working right now, for "Restart now (3 agents working, 2 waiting)" (d0022).
    #[serde(default)]
    pub agents_working: u32,
    /// Agents waiting for the user (the needs-you family, d0042).
    #[serde(default)]
    pub agents_waiting: u32,
    /// This run of the service followed a crash or a reboot (its previous run didn't stop cleanly; REC-06).
    #[serde(default)]
    pub unclean_start: bool,
    /// Tickets the startup reconcile set to Interrupted ("4 sessions were interrupted", REC-01, REC-06), outside the
    /// paused projects (PAU-07: theirs wait for Resume).
    #[serde(default)]
    pub interrupted_at_start: u32,
    /// The paused projects with their counts (PAU-01, PAU-07; d0112), by name.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub paused: Vec<crate::agents::pause::ProjectPause>,
}

/// Something that happened in the service, pushed to subscribers (SDD §5.4).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Event {
    /// Only used by tests, so the event stream is testable before real events exist.
    Test { seq: u64 },
    /// A ticket or epic changed in any way (fields, column, links, pin, key). Refetch it or the board.
    TicketChanged {
        project: String,
        id: String,
        key: TicketKey,
    },
    /// A ticket's status changed (sent with [`Event::TicketChanged`]).
    CardStatusChanged {
        project: String,
        id: String,
        key: TicketKey,
        status: Status,
    },
    /// A ticket is gone (deleted, or its project removed).
    TicketDeleted {
        project: String,
        id: String,
        key: TicketKey,
    },
    /// A project was added, changed or removed. Refetch `project.list`.
    ProjectsChanged,
    /// A project was paused (`paused_since` set) or resumed (`None`) (PAU-01, d0112); sent with
    /// [`Event::ProjectsChanged`].
    PauseChanged {
        project: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        paused_since: Option<i64>,
    },
    /// A project's flow: a proposal opened or closed, a change waits, a new flow applied, a waiting change cancelled
    /// (FLW-03…FLW-06). Refetch `flow.proposal`, `flow.waiting` or the board.
    FlowChanged {
        project: String,
        change: crate::flow::apply::FlowEvent,
    },
    /// Output of a session this connection attached to (only sent to that connection). `data` is base64.
    SessionOutput {
        session: String,
        /// Where `data` starts in the session's whole output.
        offset: u64,
        data: String,
    },
    /// A session started or ended.
    SessionState { session: SessionInfo },
    /// How many automatic deliveries wait for a session ("1 message waiting · Deliver now", SES-06).
    MessagesWaiting {
        session: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        project: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ticket: Option<TicketKey>,
        count: u32,
    },
    /// A client's terminal for a session gained or lost focus.
    SessionFocus {
        session: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ticket: Option<TicketKey>,
        focused: bool,
    },
    /// The needs-you items changed (an item came, went or changed group or reason): refetch `inbox.list`. Carries
    /// the new counts for the header and the project switcher (ATT-10). Not sent when only ages grew.
    InboxChanged {
        counts: crate::attention::inbox::InboxCounts,
    },
    /// An OS notification was clicked (ATT-11, d0090): bring the app forward on its target (a ticket's or a
    /// validator's terminal, a Project agent, a lead, the inbox, the board).
    OpenFromNotification {
        target: crate::attention::notify::Target,
    },
    /// Token counts changed (USG-01): refetch `usage.summary`.
    UsageChanged,
    /// A ticket's Run changed state (RUN-01): refetch `run.status` / `run.list`.
    RunChanged { project: String, key: TicketKey },
    /// Ask the agent's progress and outcome for a project's run command (RUN-02).
    RunSuggestion {
        project: String,
        update: crate::terminals::suggest::SuggestUpdate,
    },
    /// A Run was stopped because its ticket reached Done or was closed: a toast ("Run stopped · SHOP-19 is done").
    RunStopped {
        project: String,
        key: TicketKey,
        message: String,
    },
    /// After a push, the PR branch's content differs from what the user reviewed (REV-06, T-0145): a toast
    /// ("SHOP-18 changed since you reviewed it · the pushed content differs in 2 files").
    PushedContentDiffers {
        project: String,
        key: TicketKey,
        message: String,
    },
    /// A toast (UI-07, d0064): an automatic local Delivery merged a ticket ("SHOP-40.1 merged automatically into the
    /// Wishlist branch" · "Validated · round 1 · local, not pushed · SHOP-40.2 can start"). No notification.
    MergedAutomatically {
        project: String,
        key: TicketKey,
        title: String,
        message: String,
    },
    /// A toast (UI-07, d0063): a validation was cancelled because the code or the description changed
    /// ("Validation cancelled: SHOP-29’s code changed" · "Round not counted · the agent was told to submit again").
    ValidationCancelled {
        project: String,
        key: TicketKey,
        title: String,
        message: String,
    },
    /// A ticket's agent or an epic lead created a ticket (#47; d0016: a toast, no notification): "Ticket created by
    /// agent" · "DEMO-7 created DEMO-12 → Backlog". `by` names the creator as the toast does (the ticket's key, or
    /// the lead's name); `status` is where the new ticket landed (Backlog, or Queued in its epic's queue). Not sent
    /// for the user's tickets nor a Project agent's (its pane lists what it created).
    CreatedByAgent {
        project: String,
        key: TicketKey,
        title: String,
        by: String,
        status: Status,
    },
    /// A question or answer between agents was typed into the other agent's session (MSG-02, MSG-06): the
    /// receiving ticket's key; both tickets' cards and Messages refetch.
    MessageDelivered {
        project: String,
        key: TicketKey,
        message: String,
    },
    /// A Project agent was opened, named, closed, resumed, or its turn or state changed (d0068): refetch
    /// `project_agent.list`.
    ProjectAgentChanged { project: String, id: String },
    /// A project's Flow setup changed (ASK-04, ASK-06): a flow agent, a proposal, a tool's state or a limit, or
    /// its flow got settled. Refetch `onboarding.flow_setup` (and `project.list` once it is settled).
    FlowSetupChanged { project: String },
    /// An epic's lead was opened, resumed or ended, or its status changed (d0083): refetch the epic's lead.
    LeadChanged { project: String, epic: TicketKey },
    /// The global settings were saved (`settings.set`): refetch `settings.get`. Every window applies a new theme
    /// at once (UIT-07).
    SettingsChanged,
    /// The named agents or the role defaults (app-wide or a project's) changed (AGT-01…AGT-03): refetch
    /// `agents.list` (and the project's settings).
    AgentsChanged,
    /// The folder trust notices changed (the one-time note, the projects whose trust couldn't be recorded;
    /// PRJ-08): refetch `onboarding.state`.
    TrustChanged,
    /// What macOS allows, the ask or the blocked banner changed (ATT-13, ATT-14): the new `notifications.state`.
    NotificationsChanged {
        state: crate::attention::os_permission::NotificationsState,
    },
    /// An agent tool's row on Settings → Agent tools changed (a recheck, Hidden, a setup, Sign in… ended, a note):
    /// refetch `agent_tools.list`.
    AgentToolsChanged,
    /// A project's warm build was kept, replaced or cleared, or a Clear started waiting for a copy (WRM-06):
    /// refetch `warm.list`. `repo` is its folder in `warm/`; `project` the key of the project it belongs to.
    WarmBuildChanged {
        repo: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        project: Option<String>,
    },
    /// Enter was pressed in a session's terminal (clears needs-you signals from M4, ATT-02).
    SessionEnter {
        session: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ticket: Option<TicketKey>,
    },
}

/// The `{"error": {...}}` body, used on the socket and printed by the CLI with `--json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ErrorBody {
    pub kind: ErrorKind,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl ErrorBody {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            data: None,
        }
    }
}

impl fmt::Display for ErrorBody {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    /// The peer speaks another protocol version ("Restart required", d0022).
    IncompatibleProtocol,
    /// The message couldn't be parsed or isn't a valid request.
    BadRequest,
    /// The request was valid but the service failed to carry it out.
    Internal,
    /// A board rule refused the request; `data.refusal` says which (for example `cycle`, `cross_base`).
    Refused,
    /// No such project or ticket.
    NotFound,
    /// The background service isn't reachable.
    Disconnected,
    /// Wrong command-line usage.
    Usage,
    /// A kind this build doesn't know (sent by a newer peer).
    #[serde(other)]
    Unknown,
}

/// Why a frame couldn't be turned into a [`Message`].
#[derive(Debug)]
pub enum DecodeError {
    /// Not JSON, or not a message this build understands.
    Malformed(serde_json::Error),
    /// A well-formed message from a peer with another protocol version.
    IncompatibleVersion(ForeignMessage),
}

impl DecodeError {
    pub fn kind(&self) -> ErrorKind {
        match self {
            Self::Malformed(_) => ErrorKind::BadRequest,
            Self::IncompatibleVersion(_) => ErrorKind::IncompatibleProtocol,
        }
    }
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Malformed(e) => write!(f, "malformed message: {e}"),
            Self::IncompatibleVersion(m) => write!(
                f,
                "incompatible protocol version {} (this build speaks {PROTOCOL_VERSION})",
                m.protocol_version
            ),
        }
    }
}

impl std::error::Error for DecodeError {}

/// A message from a peer speaking another protocol version, kept raw. Only its envelope and the
/// [`VERSION_INDEPENDENT_METHODS`] can be relied on.
#[derive(Debug, Clone, PartialEq)]
pub struct ForeignMessage {
    pub protocol_version: u32,
    pub id: Option<u64>,
    pub raw: Value,
}

impl ForeignMessage {
    /// The request method, if this is a request.
    pub fn method(&self) -> Option<&str> {
        self.raw.get("request")?.get("method")?.as_str()
    }

    /// Whether this is a request every service answers regardless of version.
    pub fn is_version_independent_request(&self) -> bool {
        self.method()
            .is_some_and(|m| VERSION_INDEPENDENT_METHODS.contains(&m))
    }

    /// Reads the message with this build's types. Works for the version-independent requests and replies.
    pub fn read_as_current(&self) -> Option<Message> {
        let mut raw = self.raw.clone();
        raw.as_object_mut()?
            .insert("protocol_version".into(), PROTOCOL_VERSION.into());
        serde_json::from_value(raw).ok()
    }
}

/// Serializes a message as one frame: compact JSON followed by `\n`.
pub fn encode(message: &Message) -> Vec<u8> {
    let mut frame = serde_json::to_vec(message).expect("messages always serialize");
    frame.push(b'\n');
    frame
}

/// Parses one frame (with or without its trailing newline). The protocol version is checked before the body,
/// so a peer with another version is reported as such even if its messages no longer parse.
pub fn decode(frame: &[u8]) -> Result<Message, DecodeError> {
    let raw: Value = serde_json::from_slice(frame).map_err(DecodeError::Malformed)?;
    let version = raw
        .get("protocol_version")
        .and_then(Value::as_u64)
        .and_then(|v| u32::try_from(v).ok());
    match version {
        Some(v) if v != PROTOCOL_VERSION => Err(DecodeError::IncompatibleVersion(ForeignMessage {
            protocol_version: v,
            id: raw.get("id").and_then(Value::as_u64),
            raw,
        })),
        _ => serde_json::from_value(raw).map_err(DecodeError::Malformed),
    }
}

/// Reads frames from a byte stream. Handles partial reads; a frame may arrive in any number of pieces.
pub struct FrameReader<R> {
    inner: R,
    buf: Vec<u8>,
}

impl<R: AsyncBufRead + Unpin> FrameReader<R> {
    pub fn new(inner: R) -> Self {
        Self {
            inner,
            buf: Vec::new(),
        }
    }

    /// The next frame without its newline, or `None` at a clean end of stream.
    pub async fn next_frame(&mut self) -> io::Result<Option<&[u8]>> {
        self.buf.clear();
        let limit = MAX_FRAME_BYTES as u64;
        let n = (&mut self.inner)
            .take(limit)
            .read_until(b'\n', &mut self.buf)
            .await?;
        if n == 0 {
            return Ok(None);
        }
        if self.buf.last() != Some(&b'\n') {
            let error = if self.buf.len() >= MAX_FRAME_BYTES {
                io::Error::new(io::ErrorKind::InvalidData, "frame too large")
            } else {
                io::Error::new(io::ErrorKind::UnexpectedEof, "stream ended mid-frame")
            };
            return Err(error);
        }
        self.buf.pop();
        Ok(Some(&self.buf))
    }
}

/// Writes one message as a frame and flushes it.
pub async fn write_message<W: AsyncWrite + Unpin>(w: &mut W, message: &Message) -> io::Result<()> {
    w.write_all(&encode(message)).await?;
    w.flush().await
}

#[cfg(test)]
mod tests {
    use std::pin::Pin;
    use std::task::{Context, Poll};

    use serde_json::json;
    use tokio::io::{AsyncRead, BufReader, ReadBuf};

    use super::*;

    fn status() -> ServiceStatus {
        ServiceStatus {
            pid: 4120,
            build_id: "abc.1".into(),
            protocol_version: PROTOCOL_VERSION,
            started_at: 1_791_504_000_000,
            uptime_secs: 42,
            agents_working: 3,
            agents_waiting: 2,
            ..Default::default()
        }
    }

    fn samples() -> Vec<Message> {
        vec![
            Message::request(1, Request::Ping),
            Message::request(2, Request::ServiceStatus),
            Message::request(3, Request::ServiceStop),
            Message::request(6, Request::Subscribe),
            Message::request(7, Request::ToolsCheck),
            Message::request(8, Request::OnboardingState),
            Message::request(9, Request::DismissPermissionWarning),
            Message::request(9, Request::DismissTrustNote),
            Message::reply(
                8,
                Reply::OnboardingState(OnboardingState {
                    has_projects: false,
                    permission_warning_dismissed: true,
                    trust_note: true,
                    trust_note_tools: vec![crate::agent_tools::Tool::Codex],
                    trust_failed: vec![crate::agent_tools::setup::TrustFailure {
                        project_key: "NOTE".into(),
                        tool: crate::agent_tools::Tool::Claude,
                        reason: "Claude Code's config file couldn't be written".into(),
                    }],
                }),
            ),
            Message::reply(1, Reply::Pong),
            Message::reply(2, Reply::ServiceStatus(status())),
            Message::reply(3, Reply::Ok),
            Message::event(Event::Test { seq: 9 }),
            Message::request(
                10,
                Request::TicketMove(MoveParams {
                    key: "SHOP-18".into(),
                    target: DropTarget::ReadyForDev,
                    confirmed: true,
                }),
            ),
            Message::request(
                11,
                Request::TicketUpdate(TicketUpdateParams {
                    key: "SHOP-18".into(),
                    update: TicketUpdate {
                        title: Some("Checkout".into()),
                        ..Default::default()
                    },
                }),
            ),
            Message::request(12, Request::ProjectList),
            Message::event(Event::CardStatusChanged {
                project: "SHOP".into(),
                id: "id".into(),
                key: "SHOP-40.1".parse().unwrap(),
                status: Status::Queued,
            }),
            Message::event(Event::ProjectsChanged),
            Message::request(
                14,
                Request::AgentsPause(crate::agents::pause::PauseScope::All),
            ),
            Message::event(Event::PauseChanged {
                project: "SHOP".into(),
                paused_since: Some(1),
            }),
            Message::event(Event::TrustChanged),
            Message::request(13, Request::NotificationsState),
            Message::request(13, Request::NotificationsAskAnswered),
            Message::request(13, Request::NotificationsDismissBlocked),
            Message::event(Event::NotificationsChanged {
                state: crate::attention::os_permission::NotificationsState {
                    permission: crate::attention::notify::PermissionState::TurnedOff,
                    ask_pending: false,
                    blocked_banner: true,
                },
            }),
            Message::error(
                Some(4),
                ErrorBody::new(ErrorKind::BadRequest, "unknown method"),
            ),
            Message {
                caller: Some("agent:SHOP-18".into()),
                ..Message::request(5, Request::Ping)
            },
        ]
    }

    #[test]
    fn round_trips() {
        for message in samples() {
            let frame = encode(&message);
            assert_eq!(frame.last(), Some(&b'\n'));
            assert_eq!(
                frame.iter().filter(|b| **b == b'\n').count(),
                1,
                "one line per message"
            );
            assert_eq!(decode(&frame).unwrap(), message);
        }
    }

    #[test]
    fn wire_shapes() {
        let as_json = |m: &Message| serde_json::to_value(m).unwrap();
        assert_eq!(
            as_json(&Message::request(7, Request::ServiceStatus)),
            json!({"protocol_version": 1, "id": 7, "pid": std::process::id(), "request": {"method": "service.status"}})
        );
        assert_eq!(
            as_json(&Message::error(
                Some(8),
                ErrorBody::new(ErrorKind::IncompatibleProtocol, "x")
            )),
            json!({"protocol_version": 1, "id": 8, "error": {"kind": "incompatible_protocol", "message": "x"}})
        );
        assert_eq!(
            as_json(&Message::request(
                4,
                Request::DepAdd(DepParams {
                    child: "SHOP-20".into(),
                    parent: "SHOP-18".into()
                })
            )),
            json!({"protocol_version": 1, "id": 4, "pid": std::process::id(), "request": {"method": "dep.add", "params": {"child": "SHOP-20", "parent": "SHOP-18"}}})
        );
        assert_eq!(
            as_json(&Message::event(Event::Test { seq: 3 })),
            json!({"protocol_version": 1, "event": {"kind": "test", "seq": 3}})
        );
        for m in VERSION_INDEPENDENT_METHODS {
            assert!(
                [Request::Ping, Request::ServiceStatus, Request::ServiceStop]
                    .iter()
                    .any(|r| r.method() == *m)
            );
        }
    }

    #[test]
    fn unknown_error_kinds_are_tolerated() {
        let m = decode(
            br#"{"protocol_version":1,"id":1,"error":{"kind":"from_the_future","message":"m"}}"#,
        )
        .unwrap();
        let Body::Error(e) = m.body else {
            panic!("{m:?}")
        };
        assert_eq!(e.kind, ErrorKind::Unknown);
    }

    #[test]
    fn version_mismatch_is_a_distinct_error() {
        let frame = br#"{"protocol_version":2,"id":5,"request":{"method":"tickets.frobnicate","params":{"x":1}}}"#;
        let err = decode(frame).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::IncompatibleProtocol);
        let DecodeError::IncompatibleVersion(foreign) = err else {
            panic!()
        };
        assert_eq!(foreign.protocol_version, 2);
        assert_eq!(foreign.id, Some(5));
        assert_eq!(foreign.method(), Some("tickets.frobnicate"));
        assert!(!foreign.is_version_independent_request());
    }

    #[test]
    fn version_independent_messages_read_across_versions() {
        let request = br#"{"protocol_version":9,"id":1,"request":{"method":"service.status"}}"#;
        let DecodeError::IncompatibleVersion(foreign) = decode(request).unwrap_err() else {
            panic!()
        };
        assert!(foreign.is_version_independent_request());
        assert_eq!(
            foreign.read_as_current().unwrap().body,
            Body::Request(Request::ServiceStatus)
        );

        // A newer service's status reply, with a field this build doesn't know.
        let mut reply =
            serde_json::to_value(Message::reply(1, Reply::ServiceStatus(status()))).unwrap();
        reply["protocol_version"] = json!(9);
        reply["reply"]["service_status"]["new_field"] = json!(true);
        let DecodeError::IncompatibleVersion(foreign) =
            decode(&serde_json::to_vec(&reply).unwrap()).unwrap_err()
        else {
            panic!()
        };
        assert_eq!(
            foreign.read_as_current().unwrap().body,
            Body::Reply(Reply::ServiceStatus(status()))
        );
    }

    #[test]
    fn malformed_frames() {
        for frame in [
            &b"not json"[..],
            br#"{"protocol_version":1}"#,
            br#"{"id":1,"request":{"method":"ping"}}"#,
        ] {
            let err = decode(frame).unwrap_err();
            assert_eq!(err.kind(), ErrorKind::BadRequest, "{frame:?}");
        }
    }

    #[test]
    fn build_id_has_a_hash_and_a_time() {
        let id = build_id();
        let (hash, secs) = id.rsplit_once('.').expect(id);
        assert!(!hash.is_empty(), "{id}");
        assert!(secs.parse::<u64>().unwrap() > 0, "{id}");
    }

    /// Yields at most `chunk` bytes per read, to exercise partial reads.
    struct Trickle {
        data: Vec<u8>,
        pos: usize,
        chunk: usize,
    }

    impl AsyncRead for Trickle {
        fn poll_read(
            mut self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
            buf: &mut ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            let end = (self.pos + self.chunk)
                .min(self.data.len())
                .min(self.pos + buf.remaining());
            let piece = self.data[self.pos..end].to_vec();
            buf.put_slice(&piece);
            self.pos = end;
            Poll::Ready(Ok(()))
        }
    }

    fn trickle(data: Vec<u8>, chunk: usize) -> FrameReader<BufReader<Trickle>> {
        FrameReader::new(BufReader::with_capacity(
            4,
            Trickle {
                data,
                pos: 0,
                chunk,
            },
        ))
    }

    #[tokio::test]
    async fn framing_with_partial_reads() {
        let messages = samples();
        let stream: Vec<u8> = messages.iter().flat_map(encode).collect();
        for chunk in [1, 3, 7, 1000] {
            let mut reader = trickle(stream.clone(), chunk);
            let mut got = Vec::new();
            while let Some(frame) = reader.next_frame().await.unwrap() {
                got.push(decode(frame).unwrap());
            }
            assert_eq!(got, messages, "chunk size {chunk}");
        }
    }

    #[tokio::test]
    async fn stream_ending_mid_frame_is_an_error() {
        let mut stream = encode(&Message::request(1, Request::Ping));
        stream.extend_from_slice(br#"{"protocol_version":1,"#);
        let mut reader = trickle(stream, 5);
        assert!(reader.next_frame().await.unwrap().is_some());
        let err = reader.next_frame().await.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof);
    }

    #[tokio::test]
    async fn write_then_read() {
        let (client, server) = tokio::io::duplex(64);
        let (_, mut write_half) = tokio::io::split(client);
        let (read_half, _) = tokio::io::split(server);
        let messages = samples();
        let sent = messages.clone();
        let writer = tokio::spawn(async move {
            for m in &sent {
                write_message(&mut write_half, m).await.unwrap();
            }
        });
        let mut reader = FrameReader::new(BufReader::new(read_half));
        for expected in &messages {
            let frame = reader.next_frame().await.unwrap().unwrap();
            assert_eq!(&decode(frame).unwrap(), expected);
        }
        writer.await.unwrap();
    }
}
