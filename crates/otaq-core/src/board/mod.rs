//! The board's rules: projects, tickets, epics, dependencies, moves and pins (SDD §6, §7). The service calls
//! these for the app and the CLI alike, so every rule is enforced in one place (d0055). No UI, no sockets.

pub mod agent_edits;
pub mod batch;
pub mod change_note;
pub mod comments;
pub mod conflicts;
pub mod created;
pub mod delete;
pub mod deps;
pub mod diff;
pub mod epic_flight;
pub mod epics;
pub mod files;
pub mod hand_over;
pub mod human_check;
pub mod merge;
pub mod merge_check;
pub mod moves;
pub mod needs_you;
pub mod order;
pub mod primed;
pub mod project_files;
pub mod projects;
pub mod questions;
pub mod readiness;
pub mod remote;
pub mod reports;
pub mod review;
pub mod send_back;
pub mod settings;
pub mod snapshot;
pub mod stack;
pub mod stage_choices;
pub mod stage_runs;
pub mod ticket_comments;
pub mod tickets;
pub mod validation;

use std::fmt;

use crate::agent_tools::setup::Trust;
use crate::attention::{Focus, LimitStops};
use crate::data_dir::DataDir;
use crate::domain::stored_enum;
use crate::epic_lead::overlap::Overlaps;
use crate::epic_lead::wake::LeadLink;
use crate::git::{Git, GitError};
use crate::session::hand_offs::HandOffs;
use crate::store::{Store, StoreError};
use crate::terminals::run::RunPorts;

/// The store plus the user's git. Cheap to clone.
#[derive(Debug, Clone)]
pub struct Board {
    store: Store,
    /// `None` when git isn't installed; git work then fails with [`GitError::NotFound`].
    git: Option<Git>,
    /// Which tickets' terminals have focus (ATT-03): their waiting agents show "your turn", not turquoise.
    focus: Focus,
    /// Agents stopped by the plan limit, and whether their items are left out (ATT-09).
    limit_stops: LimitStops,
    /// Tickets whose Run is up, with its port: the card's `▶ :4020` (RUN-01).
    run_ports: RunPorts,
    /// Tickets whose Merge check runs now: "Merging · running the merge check" (d0120).
    merge_checks: merge_check::MergeChecks,
    /// Running epic children's changed files: the cards' "same file as SHOP-40.4" (LEAD-09).
    overlaps: Overlaps,
    /// Hand-offs typed into live sessions that their agents haven't picked up yet (d0118).
    hand_offs: HandOffs,
    /// Every owner's changes for the project Files view, per ticket (FIL-10).
    project_files: project_files::ProjectFilesCache,
    /// The epic leads, told when an epic is queued, a child merges or closes, the epic reaches Review (d0083).
    leads: LeadLink,
    /// The agent tools' folder trust, recorded when a project is added (PRJ-06); the service sets the registry.
    trust: Trust,
    /// Runs an automatic Delivery when a move put a ticket into one (FLW-15); the service sets it.
    delivery: merge::DeliveryLink,
    /// The data folder: where Otaq's copies of imported workflow skills live (`skills/<id>`, WSK-03).
    data: DataDir,
    /// Held while a workflow skill setting changes (an import, Re-import, a reset, a settings save), so two
    /// changes never interleave and leave a copy no setting holds.
    skill_changes: std::sync::Arc<tokio::sync::Mutex<()>>,
    /// The stored flow documents read so far, by their text: a version is read from its YAML once
    /// ([`Self::effective_flow`]).
    pub(crate) flows:
        std::sync::Arc<std::sync::Mutex<std::collections::HashMap<String, crate::flow::FlowFile>>>,
}

impl Board {
    pub fn new(store: Store, git: Option<Git>, data: DataDir) -> Self {
        Self {
            store,
            git,
            focus: Focus::default(),
            limit_stops: LimitStops::default(),
            run_ports: RunPorts::default(),
            merge_checks: merge_check::MergeChecks::default(),
            overlaps: Overlaps::default(),
            hand_offs: HandOffs::default(),
            project_files: Default::default(),
            leads: LeadLink::default(),
            trust: Trust::default(),
            delivery: merge::DeliveryLink::default(),
            data,
            skill_changes: Default::default(),
            flows: Default::default(),
        }
    }

    /// A board using the `git` found on `PATH`, if any.
    pub fn with_path_git(store: Store, data: DataDir) -> Self {
        Self::new(store, Git::find().ok(), data)
    }

    /// The data folder.
    pub fn data(&self) -> &DataDir {
        &self.data
    }

    /// The lock every workflow skill setting change holds ([`Self::skill_changes`]).
    pub(crate) async fn lock_skill_changes(&self) -> tokio::sync::MutexGuard<'_, ()> {
        self.skill_changes.lock().await
    }

    pub fn store(&self) -> &Store {
        &self.store
    }

    /// The focused-terminal registry; the service's sessions share it.
    pub fn focus(&self) -> &Focus {
        &self.focus
    }

    /// The plan-limit suppression of needs-you items (ATT-09).
    pub fn limit_stops(&self) -> &LimitStops {
        &self.limit_stops
    }

    /// The tickets whose Run is up; the service's Runs keep it.
    pub fn run_ports(&self) -> &RunPorts {
        &self.run_ports
    }

    /// The Merge checks running now (d0120); Local merge keeps it.
    pub fn merge_checks(&self) -> &merge_check::MergeChecks {
        &self.merge_checks
    }

    /// Running epic children's changed files; the scheduler's passes keep it.
    pub fn overlaps(&self) -> &Overlaps {
        &self.overlaps
    }

    /// The hand-offs waiting for their agents to pick them up (d0118); the deliveries and the signals share it.
    pub fn hand_offs(&self) -> &HandOffs {
        &self.hand_offs
    }

    /// The epic leads' side of board changes (d0083); the Project agents set it.
    pub fn leads(&self) -> &LeadLink {
        &self.leads
    }

    /// The automatic Delivery's side of board moves (FLW-15).
    pub fn delivery(&self) -> &merge::DeliveryLink {
        &self.delivery
    }

    /// The agent tools' folder trust (PRJ-06, PRJ-08).
    pub fn trust(&self) -> &Trust {
        &self.trust
    }

    pub(crate) fn git(&self) -> Result<&Git, BoardError> {
        self.git.as_ref().ok_or(BoardError::Git(GitError::NotFound))
    }
}

stored_enum! {
    /// Why a request was refused, so a client can place the message (for example next to a form field).
    pub enum Refusal ("refusal") {
        /// The folder doesn't exist or isn't inside a git repository.
        NotAGitRepo = "not_a_git_repo",
        /// The repository is already a project.
        AlreadyAdded = "already_added",
        /// The project key's format is wrong.
        BadKey = "bad_key",
        /// Another project uses the key.
        KeyTaken = "key_taken",
        /// The project key can't change once added (d0029).
        KeyLocked = "key_locked",
        /// No base branch could be detected or the given one is empty.
        NoBaseBranch = "no_base_branch",
        /// A required field (a name, a title) is empty.
        EmptyField = "empty_field",
        /// Epic children have no base branch of their own: they use the epic's branch (d0051, d0059).
        BaseFromEpic = "base_from_epic",
        /// The ticket has started; its base branch is locked (d0051).
        BaseLocked = "base_locked",
        /// Epic children have no External ID (d0059).
        ExternalIdOnChild = "external_id_on_child",
        /// The ticket named as an epic isn't one.
        NotAnEpic = "not_an_epic",
        /// Epics can't be nested: one level only (d0039).
        EpicInEpic = "epic_in_epic",
        /// The tickets belong to different projects (d0024).
        OtherProject = "other_project",
        /// A ticket can't depend on itself.
        SelfDependency = "self_dependency",
        /// The link exists already.
        DuplicateDependency = "duplicate_dependency",
        /// An epic child can't depend on its own epic.
        DependsOnOwnEpic = "depends_on_own_epic",
        /// The link would close a loop (DEP-01).
        Cycle = "cycle",
        /// Linked tickets must use the same base branch (DEP-02, d0051).
        CrossBase = "cross_base",
        /// The ticket is already an epic child; a child can't move to another epic (d0039).
        AlreadyInEpic = "already_in_epic",
        /// The ticket has been to Dev; only never-started tickets may do this (d0044, d0060).
        Started = "started",
        /// The drag rules don't allow this move (SDD §7.4); the message says why.
        MoveRefused = "move_refused",
        /// The move needs the user's confirmation or choice first.
        NeedsConfirmation = "needs_confirmation",
        /// Allowed by the rules, but the feature that carries it out isn't built yet.
        NotAvailableYet = "not_available_yet",
        /// The caller may not do this (an agent deleting tickets, CLI-02).
        NotAllowed = "not_allowed",
        /// The ticket already has a running agent session (SES-04).
        AlreadyRunning = "already_running",
        /// The session has ended: it takes no input or resize.
        SessionEnded = "session_ended",
        /// The worktree has uncommitted files (REV-07, d0050).
        Uncommitted = "uncommitted",
        /// A question or answer is empty or malformed.
        BadQuestion = "bad_question",
        /// The ticket has no branch yet (it never started), so it has no diff.
        NoBranch = "no_branch",
        /// The review comment was sent with Send back and is read-only now (REV-03).
        CommentSent = "comment_sent",
        /// The project has no run command yet; the app asks for one (RUN-02).
        NoRunCommand = "no_run_command",
        /// The ticket has no worktree (it never started, or it is Done): no Run or Shell.
        NoWorktree = "no_worktree",
        /// The Open PR / Push skill's proposal is malformed or doesn't match the ticket's commits (REV-10).
        BadProposal = "bad_proposal",
        /// A proposed flow breaks a rule, or Apply was asked while problems remain (FLW-03): one problem per line,
        /// each naming its stage or agent.
        BadFlow = "bad_flow",
        /// The branch name isn't a valid branch name.
        BadBranchName = "bad_branch_name",
        /// The branch name is taken, locally or on the remote (REV-10).
        BranchTaken = "branch_taken",
        /// The branch was pushed; its name is locked (GIT-07, d0050).
        BranchLocked = "branch_locked",
        /// `pr opened` got something that isn't a pull request URL.
        BadPrUrl = "bad_pr_url",
        /// Another agent has this name, ignoring case (AGM-01).
        NameTaken = "name_taken",
        /// An agent's name is `skip` or starts with `_` (AGM-01, Q256).
        BadName = "bad_name",
        /// The model isn't in the tool's own list (AGT-02, d0086).
        ModelNotListed = "model_not_listed",
        /// The effort level isn't one of the tool's, or the tool has none (AGM-01).
        BadEffort = "bad_effort",
        /// A shipped skill Otaq doesn't ship, or an imported skill that isn't there (AGM-01).
        BadSkill = "bad_skill",
        /// The agent holds a role, or a stage names it (AGM-01): the message says which.
        AgentInUse = "agent_in_use",
        /// A stage names an agent that isn't one of the agents (AGM-01).
        UnknownAgent = "unknown_agent",
        /// A tool's fallback is on the same tool: "A fallback must be on another tool" (AGM-03).
        BadFallback = "bad_fallback",
        /// A workflow skill folder can't be imported (WSK-02): no SKILL.md, front matter that doesn't read, or a
        /// copy that failed. Nothing changed.
        BadSkillFolder = "bad_skill_folder",
        /// Re-import's remembered original folder is gone (WSK-04); Otaq's copy stays.
        OriginalNotFound = "original_not_found",
        /// The agent has no such imported skill to re-import.
        NotImported = "not_imported",
        /// A stage choice that isn't one of the stage's options, a stage without options, or an epic (AGM-08).
        NotAnOption = "not_an_option",
        /// The stage has started: its choice is locked (AGM-08).
        ChoiceLocked = "choice_locked",
        /// A ticket comment over the size limit (CMT-02): the message says how long it is.
        TooLong = "too_long",
        /// A comment's report can't be read or stored, or is over 20 MB (CMT-03, CMT-06); nothing was posted.
        BadReport = "bad_report",
        /// A Files path that isn't one of the ticket's files: absolute, with `..` or `.git`, or a symlink out of
        /// the worktree (FTB-03, d0087).
        BadPath = "bad_path",
    }
}

#[derive(Debug)]
pub enum BoardError {
    /// A rule refused the request. The message is written for people.
    Refused {
        code: Refusal,
        message: String,
    },
    /// No such project or ticket.
    NotFound(String),
    Git(GitError),
    Store(StoreError),
}

impl BoardError {
    pub fn refused(code: Refusal, message: impl Into<String>) -> Self {
        Self::Refused {
            code,
            message: message.into(),
        }
    }

    /// The refusal code, if a rule refused the request.
    pub fn refusal(&self) -> Option<Refusal> {
        match self {
            Self::Refused { code, .. } => Some(*code),
            _ => None,
        }
    }
}

impl fmt::Display for BoardError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Refused { message, .. } => f.write_str(message),
            Self::NotFound(what) => f.write_str(what),
            Self::Git(e) => write!(f, "{e}"),
            Self::Store(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for BoardError {}

impl From<StoreError> for BoardError {
    fn from(e: StoreError) -> Self {
        Self::Store(e)
    }
}

impl From<sqlx::Error> for BoardError {
    fn from(e: sqlx::Error) -> Self {
        Self::Store(StoreError::Database(e))
    }
}

impl From<GitError> for BoardError {
    fn from(e: GitError) -> Self {
        Self::Git(e)
    }
}
