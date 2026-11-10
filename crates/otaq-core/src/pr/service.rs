//! The Open PR and Push to PR flows (REV-10, REV-14; d0050, d0041, d0059), driven by the user's clicks:
//!
//! 1. **Open PR / Push to PR** (Approve ▾): refused with uncommitted files (REV-08); "changed since you reviewed
//!    it" first unless approved anyway (REV-06). The resolved Open PR (or Push) skill runs in the ticket's
//!    session: typed into the live session through the user-click path (unsent text asks first, d0048), or the
//!    stored conversation is resumed with the skill as its first prompt. The ticket stays in Ready for Review;
//!    the flow is "preparing" until the skill runs `otaq pr propose`.
//! 2. **Confirm** (the dialog's Push & open PR): Otaq applies the user's edits itself (reword, rename, lock)
//!    and then types the instruction to push; nothing is pushed before.
//! 3. **Cancel**: the proposal is discarded, nothing was pushed, the ticket stays in Ready for Review.
//! 4. `otaq pr opened <url>`: Approved · PR open with the PR's number.
//!
//! **An automatic Delivery** (FLW-15, d0108 rule 8 and second gap review rule 6) runs the same flow with no click and
//! no dialog ([`Prs::open_automatically`]): the Delivery's agent gets the skill followed by [`automatic_message`] (the
//! Dev stage's session when the Delivery's agent is the same, else a fresh delivery session in the worktree), and
//! once it proposes, Otaq confirms the proposal as it is and tells it to push ([`automatic_instruction`]). Both are
//! queued for its input line, like any automatic message. **While the project is paused** (PAU-04, d0112) an
//! automatic Delivery doesn't start its agent: the card waits in its Delivery and the project's Resume runs it.

use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::json;

use super::flow::{FlowKind, FlowState};
use super::proposal::Proposal;
use crate::agent_tools::{Adapters, FirstPrompt, LaunchRequest, LaunchRole, Launched, Tool};
use crate::agents::gate::{Held, LaunchGate, check};
use crate::agents::{RunsOn, resolve};
use crate::board::review::ApproveCheck;
use crate::board::settings::WorkflowStep;
use crate::board::tickets::{EventKind, Ticket, record_event};
use crate::board::{Board, BoardError, Refusal};
use crate::domain::{Actor, HandOffKind, Status, TicketKey};
use crate::git::merge::uncommitted_files_message;
use crate::naming;
use crate::protocol::Event;
use crate::service::EventBus;
use crate::session::deliveries::{Deliveries, Delivery};
use crate::session::hand_offs::HandOff;
use crate::session::{
    SessionInfo, SessionKind, SessionTicket, Sessions, SpawnSpec, Start, StoredConversation,
    TerminalSize,
};
use crate::store::now_ms;

/// The answer to an Open PR, Push to PR or confirm click.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum PrOutcome {
    /// The skill was started (or told to push); the dialog follows the flow.
    Started { ticket: Box<Ticket> },
    /// The session's input line has unsent text: ask "Keep it and cancel / Clear it and send" (d0048). Nothing
    /// was changed. `unclearable_by` names the tool when the text is a multi-line draft it can't clear (SES-07).
    NeedsConfirmation {
        ticket: TicketKey,
        #[serde(default)]
        unclearable_by: Option<String>,
    },
    /// "SHOP-19 has 2 uncommitted files" · View · Send back (REV-08).
    RefusedUncommitted { files: Vec<String>, message: String },
    /// New commits since the review (REV-06): approve again with `approve_anyway`, or Send back.
    Changed { check: ApproveCheck },
}

/// Runs the PR flows. Cheap to clone.
#[derive(Clone)]
pub struct Prs {
    inner: Arc<Inner>,
}

struct Inner {
    board: Board,
    sessions: Sessions,
    deliveries: Deliveries,
    adapters: Adapters,
    events: EventBus,
    /// The check before a delivery session's launch (set by the service).
    gate: std::sync::OnceLock<LaunchGate>,
}

/// The size a resumed session starts with until a terminal attaches.
const INITIAL_SIZE: TerminalSize = TerminalSize {
    rows: 40,
    cols: 120,
};

/// How a PR step reaches a live session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Typing {
    /// The user's click: typed at once (unsent text asked about first); `clear_first` after "Clear it and send".
    Click { clear_first: bool },
    /// An automatic Delivery's: queued until the session is idle with an empty input line.
    Queued,
}

impl Typing {
    /// Who asked for a session it starts (PAU-04, PAU-05).
    fn start(self) -> Start {
        match self {
            Self::Click { .. } => Start::User,
            Self::Queued => Start::Auto,
        }
    }
}

/// Open PR refused because the built-in skill can't open a PR on this remote: the user's own Open PR skill goes in the
/// flow's delivery (its `skill:`, Edit flow's Done panel), never in an agent's skills.
pub fn own_open_pr_skill(why: &str) -> String {
    format!(
        "{why}: {OWN_OPEN_PR_SKILL} (name it as the delivery\u{2019}s `skill:` in the flow file: Edit flow \u{2192} Done \u{2192} PR skill)"
    )
}

/// The words every "use your own Open PR skill" refusal carries ([`own_open_pr_skill`]).
const OWN_OPEN_PR_SKILL: &str = "use your own Open PR skill";

/// Whether a refusal says to use your own Open PR skill: its toast links to where that skill goes.
pub fn asks_for_own_open_pr_skill(message: &str) -> bool {
    message.contains(OWN_OPEN_PR_SKILL)
}

/// What follows the PR skill's name when an automatic Delivery starts it (FLW-15): the skill proposes as usual and
/// Otaq confirms the proposal itself. It starts with the word `automatic`, which the skill keys on.
pub fn automatic_message() -> String {
    format!(
        "automatic: this project's flow opens the PR with no dialog. Propose as usual with `{} pr propose --stdin`; \
         {} confirms your proposal as it is and then tells you to push.",
        naming::COMMAND_NAME,
        naming::PRODUCT_NAME
    )
}

/// What Otaq types once it confirmed an automatic Delivery's proposal itself (FLW-15, no dialog).
pub fn automatic_instruction(kind: FlowKind) -> String {
    let cmd = naming::COMMAND_NAME;
    match kind {
        FlowKind::Open => format!(
            "The pull request was confirmed automatically (this project's flow opens it with no dialog). Run `{cmd} \
             prime` for the confirmed branch, title and description, then push the branch and open the PR, and report \
             it with `{cmd} pr opened <url>`."
        ),
        FlowKind::Push => format!(
            "The push was confirmed automatically (this project's flow pushes with no dialog). Run `{cmd} prime`, push \
             the branch, then report back with `{cmd} pr opened <url>`."
        ),
    }
}

/// What a PR step types into the ticket's session, or starts or resumes it with.
#[derive(Clone, Copy)]
enum PrStep<'a> {
    /// The Open PR or Push skill, as the Delivery stage's agent runs it, named for the session's tool; nothing is
    /// written into the worktree.
    Skill(WorkflowStep),
    /// Otaq's own words: the confirmed push ([`confirmed_instruction`]).
    Words(&'a str),
}

pub fn confirmed_instruction(kind: FlowKind) -> String {
    let cmd = naming::COMMAND_NAME;
    match kind {
        FlowKind::Open => format!(
            "The user confirmed the pull request. Run `{cmd} prime` for the confirmed branch, title and description, \
             then push the branch and open the PR, and report it with `{cmd} pr opened <url>`."
        ),
        FlowKind::Push => format!(
            "The user confirmed the push. Run `{cmd} prime`, push the branch, then report back with \
             `{cmd} pr opened <url>`."
        ),
    }
}

impl Prs {
    pub fn new(
        board: Board,
        sessions: Sessions,
        deliveries: Deliveries,
        adapters: Adapters,
        events: EventBus,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                board,
                sessions,
                deliveries,
                adapters,
                events,
                gate: std::sync::OnceLock::new(),
            }),
        }
    }

    /// Gives the delivery sessions the check before every launch (SCH-04 `[M23]`).
    pub fn set_gate(&self, gate: LaunchGate) {
        let _ = self.inner.gate.set(gate);
    }

    pub fn board(&self) -> &Board {
        &self.inner.board
    }

    /// **Open PR** or **Push to PR #N** (a ticket that has a PR) on a delivery unit in Ready for Review.
    pub async fn open(
        &self,
        key: &str,
        approve_anyway: bool,
        clear_first: bool,
        _actor: &Actor,
    ) -> Result<PrOutcome, BoardError> {
        let board = &self.inner.board;
        let ticket = board.ticket(key).await?;
        if !matches!(ticket.status, Status::WaitingForYou | Status::ToDeliver) {
            return Err(BoardError::refused(
                Refusal::MoveRefused,
                format!(
                    "{} is {}; Approve & open PR works on a ticket in your review",
                    ticket.key,
                    ticket.status.label()
                ),
            ));
        }
        let kind = match ticket.flags.pr_number {
            Some(_) => FlowKind::Push,
            None => FlowKind::Open,
        };
        let project = ticket.key.project.to_string();
        if kind == FlowKind::Open {
            let provider = self.inner.board.pr_provider_from_remote(&project).await?;
            if let Some(why) = provider.open_pr_blocked() {
                return Err(BoardError::refused(
                    Refusal::NotAllowed,
                    own_open_pr_skill(why),
                ));
            }
        }
        // Whose session runs the skill (d0108 second gap review rule 6); a delivery session that can't start now is
        // refused before anything is made for it.
        let session = self.session_kind_for(&ticket).await;
        if session == SessionKind::Delivery && self.live_of(&ticket, session).is_none() {
            self.delivery_runs_on(&ticket).await?;
        }
        let worktree = worktree_of(&ticket)?;
        let git = board.git()?;
        let files = git.uncommitted_files(&worktree).await?;
        if !files.is_empty() {
            return Ok(PrOutcome::RefusedUncommitted {
                message: uncommitted_files_message(&ticket.key.to_string(), files.len()),
                files,
            });
        }
        if !approve_anyway {
            let check = board.approve_check(key).await?;
            if check != ApproveCheck::Unchanged {
                return Ok(PrOutcome::Changed { check });
            }
        }
        if kind == FlowKind::Open {
            // Nobody waits for a skill: the dialog opens at once on what the delivery's agent wrote while the ticket
            // waited in Review, or on the plain text while it still writes. The skill runs after the confirm.
            // A live session whose tool doesn't have the skill is refused now, before anything is made.
            self.skill_runs_in(&ticket, self.live_of(&ticket, session).as_ref())
                .await?;
            let proposal = self.prepared_proposal(&ticket).await?;
            board.mark_no_pr_provider(&ticket.id, false).await?;
            board.start_pr_flow(&ticket, kind, false).await?;
            board.set_pr_proposal(&ticket.id, &proposal).await?;
            tracing::info!(key = %ticket.key, "PR dialog opened on the prepared message");
            return Ok(PrOutcome::Started {
                ticket: Box::new(board.ticket(key).await?),
            });
        }
        let step = WorkflowStep::Push;
        let skill = board.delivery_skill(&project, step).await?;
        let live = self.live_of(&ticket, session);
        if let Some(outcome) = self.unsent_first(&ticket, live.as_ref(), clear_first)? {
            return Ok(outcome);
        }
        board.start_pr_flow(&ticket, kind, false).await?;
        let started = match (session, &live) {
            (SessionKind::Delivery, None) => self
                .start_delivery(&ticket, PrStep::Skill(step), Start::User)
                .await
                .map(|_| ()),
            _ => {
                self.run_in_session(
                    &ticket,
                    live.as_ref(),
                    PrStep::Skill(step),
                    Typing::Click { clear_first },
                )
                .await
            }
        };
        if let Err(e) = started {
            board.end_pr_flow(&ticket.id).await?;
            self.end_delivery(&ticket).await?;
            return Err(e);
        }
        tracing::info!(key = %ticket.key, %skill, "PR flow started");
        Ok(PrOutcome::Started {
            ticket: Box::new(board.ticket(key).await?),
        })
    }

    /// **Push & open PR** / **Push**: the user's edits are applied, then the skill is told to push.
    pub async fn confirm(
        &self,
        key: &str,
        edits: &Proposal,
        clear_first: bool,
        actor: &Actor,
    ) -> Result<PrOutcome, BoardError> {
        let board = &self.inner.board;
        let ticket = board.ticket(key).await?;
        let flow = board
            .pr_flow(&ticket.id)
            .await?
            .filter(|f| f.state == FlowState::Proposed)
            .ok_or_else(|| {
                BoardError::refused(
                    Refusal::MoveRefused,
                    format!("{} has no proposal to confirm", ticket.key),
                )
            })?;
        let mut edits = edits.clone();
        edits.branch = edits.branch.trim().to_owned();
        let checked = board.check_proposal(&ticket, flow.kind, &edits).await?;
        if flow.kind == FlowKind::Open
            && let super::flow::BranchCheck::Problem { message } =
                board.check_branch_name(key, &checked.branch).await?
        {
            return Err(BoardError::refused(Refusal::BranchTaken, message));
        }
        if let Some(e) = checked.commits.iter().find(|c| c.message.trim().is_empty()) {
            return Err(BoardError::refused(
                Refusal::EmptyField,
                format!("Commit {} has no message", &e.sha[..7.min(e.sha.len())]),
            ));
        }
        let worktree = worktree_of(&ticket)?;
        let files = board.git()?.uncommitted_files(&worktree).await?;
        if !files.is_empty() {
            return Ok(PrOutcome::RefusedUncommitted {
                message: uncommitted_files_message(&ticket.key.to_string(), files.len()),
                files,
            });
        }
        let live = self.live_session(&ticket).await;
        if let Some(outcome) = self.unsent_first(&ticket, live.as_ref(), clear_first)? {
            return Ok(outcome);
        }
        if flow.kind == FlowKind::Open {
            self.skill_runs_in(&ticket, live.as_ref()).await?;
        }
        board
            .apply_confirmed(&ticket, flow.kind, &checked, actor)
            .await?;
        // Approve & open PR is an approve, as a merge is: the first one shows the box for new work and the hints that
        // follow the first approve.
        if flow.kind == FlowKind::Open
            && let Err(e) = board.store().reach_first_approve().await
        {
            tracing::warn!(error = %e, "couldn't remember the first approve");
        }
        // An Open PR's skill starts here, told the user confirmed (it proposed nothing: the delivery's agent wrote the
        // message in Review); a Push's skill proposed, so it is told in words.
        let confirmed = confirmed_instruction(flow.kind);
        let step = match flow.kind {
            FlowKind::Open => PrStep::Skill(WorkflowStep::OpenPr),
            FlowKind::Push => PrStep::Words(&confirmed),
        };
        if let Err(e) = self
            .run_in_session(&ticket, live.as_ref(), step, Typing::Click { clear_first })
            .await
        {
            // Nothing was pushed: the dialog shows why, with Retry.
            board.report_pr_problem(&ticket.id, &e.to_string()).await?;
            return Err(e);
        }
        Ok(PrOutcome::Started {
            ticket: Box::new(board.ticket(key).await?),
        })
    }

    /// **An automatic PR Delivery** (FLW-15): the ticket waits in its Delivery stage; the flow opens (or, with a PR,
    /// pushes to) the PR with no click and no dialog. Refused, nothing started, with uncommitted files, a provider
    /// the built-in skill can't use, or a delivery session that can't start now: the card then waits for the user's
    /// click. `without_checks` records "Delivered without all checks" for when the PR is merged.
    pub async fn open_automatically(
        &self,
        key: &str,
        without_checks: bool,
    ) -> Result<(), BoardError> {
        let board = &self.inner.board;
        let ticket = board.ticket(key).await?;
        // Kept: the project's Resume runs the Deliveries that wait (PAU-04).
        crate::agents::pause::refuse_while_paused(board.store(), ticket.key.project.as_str())
            .await?;
        if ticket.status != Status::ToDeliver {
            return Err(BoardError::refused(
                Refusal::MoveRefused,
                format!(
                    "{} is {}; an automatic PR runs from its Delivery",
                    ticket.key,
                    ticket.status.label()
                ),
            ));
        }
        let kind = match ticket.flags.pr_number {
            Some(_) => FlowKind::Push,
            None => FlowKind::Open,
        };
        let project = ticket.key.project.to_string();
        if kind == FlowKind::Open
            && let Some(why) = board
                .pr_provider_from_remote(&project)
                .await?
                .open_pr_blocked()
        {
            return Err(BoardError::refused(Refusal::NotAllowed, why));
        }
        // The Dev stage's session when the Delivery's agent is the same, else a fresh delivery session (d0108 second
        // gap review rule 6).
        let session = self.session_kind_for(&ticket).await;
        if session == SessionKind::Delivery && self.live_of(&ticket, session).is_none() {
            self.delivery_runs_on(&ticket).await?;
        }
        let worktree = worktree_of(&ticket)?;
        let files = board.git()?.uncommitted_files(&worktree).await?;
        if !files.is_empty() {
            return Err(BoardError::refused(
                Refusal::MoveRefused,
                uncommitted_files_message(&ticket.key.to_string(), files.len()),
            ));
        }
        let step = match kind {
            FlowKind::Open => WorkflowStep::OpenPr,
            FlowKind::Push => WorkflowStep::Push,
        };
        board.mark_no_pr_provider(&ticket.id, false).await?;
        board.start_pr_flow(&ticket, kind, true).await?;
        sqlx::query("UPDATE tickets SET delivered_without_checks = ? WHERE id = ?")
            .bind(without_checks)
            .bind(&ticket.id)
            .execute(board.store().pool())
            .await?;
        let live = self.live_of(&ticket, session);
        let started = match (session, &live) {
            (SessionKind::Delivery, None) => self
                .start_delivery(&ticket, PrStep::Skill(step), Start::Auto)
                .await
                .map(|_| ()),
            _ => {
                self.run_in_session(&ticket, live.as_ref(), PrStep::Skill(step), Typing::Queued)
                    .await
            }
        };
        if let Err(e) = started {
            board.end_pr_flow(&ticket.id).await?;
            self.end_delivery(&ticket).await?;
            return Err(e);
        }
        tracing::info!(key = %ticket.key, ?kind, ?session, "automatic PR started");
        Ok(())
    }

    /// `otaq pr propose --stdin` (REV-10): the proposal is stored for the dialog. An automatic Delivery's is confirmed
    /// at once as it is (FLW-15, no dialog), after the dialog's own checks, and the skill is told to push; a branch
    /// name that is taken or a missing commit message is refused for the skill to propose again.
    pub async fn propose(
        &self,
        key: &str,
        text: &str,
        actor: &Actor,
    ) -> Result<Ticket, BoardError> {
        let board = &self.inner.board;
        let ticket = board.pr_propose(key, text, actor).await?;
        let Some(flow) = board
            .pr_flow(&ticket.id)
            .await?
            .filter(|f| f.automatic && f.state == FlowState::Proposed)
        else {
            return Ok(ticket);
        };
        let proposal = flow.proposal.clone().ok_or_else(|| {
            BoardError::refused(Refusal::BadProposal, "the proposal wasn't stored")
        })?;
        if flow.kind == FlowKind::Open
            && let super::flow::BranchCheck::Problem { message } =
                board.check_branch_name(key, &proposal.branch).await?
        {
            return Err(BoardError::refused(Refusal::BranchTaken, message));
        }
        if let Some(e) = proposal
            .commits
            .iter()
            .find(|c| c.message.trim().is_empty())
        {
            return Err(BoardError::refused(
                Refusal::EmptyField,
                format!("Commit {} has no message", &e.sha[..7.min(e.sha.len())]),
            ));
        }
        board
            .apply_confirmed(&ticket, flow.kind, &proposal, &Actor::current_user())
            .await?;
        let live = self.live_session(&ticket).await;
        self.run_in_session(
            &ticket,
            live.as_ref(),
            PrStep::Words(&automatic_instruction(flow.kind)),
            Typing::Queued,
        )
        .await?;
        tracing::info!(key = %ticket.key, "automatic PR confirmed");
        board.ticket(key).await
    }

    /// **Retry** after the skill reported a problem: the problem is cleared and the skill is asked again (the
    /// skill while preparing, the push after the confirmation).
    pub async fn retry(&self, key: &str, clear_first: bool) -> Result<PrOutcome, BoardError> {
        let board = &self.inner.board;
        let ticket = board.ticket(key).await?;
        let flow = board.pr_flow(&ticket.id).await?.ok_or_else(|| {
            BoardError::refused(
                Refusal::MoveRefused,
                format!("{} has no Open PR in progress", ticket.key),
            )
        })?;
        // The Delivery's own agent's session, never another agent's (d0108 second gap review rule 6).
        let session = self.session_kind_for(&ticket).await;
        let live = self.live_of(&ticket, session);
        if let Some(outcome) = self.unsent_first(&ticket, live.as_ref(), clear_first)? {
            return Ok(outcome);
        }
        let pushing = flow.state == FlowState::Pushing;
        let text = match flow.automatic {
            true => automatic_instruction(flow.kind),
            false => confirmed_instruction(flow.kind),
        };
        let skill_step = match flow.kind {
            FlowKind::Open => WorkflowStep::OpenPr,
            FlowKind::Push => WorkflowStep::Push,
        };
        let step = match pushing {
            true => PrStep::Words(&text),
            false => PrStep::Skill(skill_step),
        };
        sqlx::query("UPDATE pr_flows SET reported = NULL WHERE ticket_id = ?")
            .bind(&ticket.id)
            .execute(board.store().pool())
            .await?;
        match (session, &live) {
            (SessionKind::Delivery, None) if !pushing => self
                .start_delivery(&ticket, step, Start::User)
                .await
                .map(|_| ())?,
            _ => {
                self.run_in_session(&ticket, live.as_ref(), step, Typing::Click { clear_first })
                    .await?
            }
        }
        Ok(PrOutcome::Started {
            ticket: Box::new(board.ticket(key).await?),
        })
    }

    /// **Cancel** before pushing: the proposal is discarded; the ticket stays in Ready for Review.
    pub async fn cancel(&self, key: &str, actor: &Actor) -> Result<Ticket, BoardError> {
        let board = &self.inner.board;
        let ticket = board.ticket(key).await?;
        let Some(flow) = board.pr_flow(&ticket.id).await? else {
            return Ok(ticket);
        };
        if flow.state == FlowState::Pushing {
            return Err(BoardError::refused(
                Refusal::MoveRefused,
                format!(
                    "{}'s PR was already confirmed and is being pushed",
                    ticket.key
                ),
            ));
        }
        board.end_pr_flow(&ticket.id).await?;
        // A click's Open PR ran nothing before the confirm: a delivery session there is the one writing the
        // delivery's message, which goes on.
        if flow.kind == FlowKind::Push || flow.automatic {
            self.end_delivery(&ticket).await?;
        }
        let mut tx = board.store().begin_write().await?;
        record_event(
            &mut tx,
            &ticket.id,
            EventKind::PrCancelled,
            actor,
            json!({ "kind": flow.kind }),
        )
        .await?;
        tx.commit().await?;
        board.ticket(key).await
    }

    /// `otaq pr opened <url>`: the PR is recorded, the flow ends and the ticket is Approved · PR open.
    pub async fn opened(&self, key: &str, url: &str, actor: &Actor) -> Result<Ticket, BoardError> {
        let board = &self.inner.board;
        let under_way = board.pr_flow(&board.ticket(key).await?.id).await?;
        let automatic = under_way.as_ref().is_some_and(|f| f.automatic);
        let ticket = board.pr_opened(key, url, actor).await?;
        board.end_pr_flow(&ticket.id).await?;
        // A delivery session's job is done (EPC-08, FLW-15).
        self.end_delivery(&ticket).await?;
        // A flow under way puts the card in its Delivery as "PR open", wherever a stray session end left it.
        if under_way.is_some()
            || matches!(
                ticket.status,
                Status::WaitingForYou | Status::ToDeliver | Status::PrOpen
            )
        {
            board.record_pr_open(&ticket, actor).await?;
            // The user's Open PR approved it: its decisions are kept by the user, not the skill (DEC-05). An
            // automatic Delivery isn't an approval and keeps nothing, as Auto-merge didn't.
            if !automatic {
                crate::decisions::keep_all(board, key, &Actor::current_user()).await?;
            }
        }
        // The pushed content must be what the user reviewed (REV-10 step 4); reworded commits don't count.
        match board.verify_pushed(&ticket).await {
            Ok(Some(diff)) => {
                tracing::warn!(key = %ticket.key, files = diff.files.len(), "pushed content differs from the review");
                self.inner.events.publish(Event::PushedContentDiffers {
                    project: ticket.key.project.to_string(),
                    key: ticket.key.clone(),
                    message: diff.message(&ticket.key.to_string()),
                });
            }
            Ok(None) => {}
            Err(e) => {
                tracing::warn!(key = %ticket.key, error = %e, "can't compare the pushed content")
            }
        }
        board.ticket(key).await
    }

    /// Whose session runs a ticket's PR steps ([`Board::delivery_session_kind`]).
    async fn session_kind_for(&self, ticket: &Ticket) -> SessionKind {
        self.inner.board.delivery_session_kind(ticket).await
    }

    /// The live session of `kind` for the ticket.
    fn live_of(&self, ticket: &Ticket, kind: SessionKind) -> Option<SessionInfo> {
        self.inner.sessions.running_of(&ticket.id, kind)
    }

    /// The live session a PR step goes to ([`Self::session_kind_for`]).
    async fn live_session(&self, ticket: &Ticket) -> Option<SessionInfo> {
        let kind = self.session_kind_for(ticket).await;
        self.live_of(ticket, kind)
    }

    /// Starts a short-lived delivery session: a new conversation in the ticket's worktree (a Delivery whose agent isn't
    /// the Dev stage's, FLW-15), with the skill in force (named by the adapter) as its first prompt, `OTAQ_TICKET` the
    /// ticket. On the Delivery stage's agent.
    async fn start_delivery(
        &self,
        ticket: &Ticket,
        step: PrStep<'_>,
        start: Start,
    ) -> Result<SessionInfo, BoardError> {
        let runs_on = self.delivery_runs_on(ticket).await?;
        self.inner.adapters.unavailable(runs_on.tool)?;
        let worktree = worktree_of(ticket)?;
        let first_prompt = self.first_prompt(ticket, step).await?;
        // The Delivery's own skill (`skill: my-pr`) runs only on a tool that has it.
        self.refuse_missing_skill(ticket, runs_on.tool, &first_prompt)
            .await?;
        let launched = self.inner.adapters.launch(
            runs_on.tool,
            &LaunchRequest {
                role: LaunchRole::Delivery,
                cwd: worktree,
                project: ticket.key.project.to_string(),
                ticket: Some(ticket.key.clone()),
                port: 0,
                first_prompt: Some(first_prompt),
                resume: None,
                model: runs_on.model.clone(),
                effort: runs_on.effort.clone(),
                title: Some(ticket.title.clone()),
                theme: self.inner.board.theme_in_use().await,
            },
        )?;
        let session = self
            .spawn(ticket, SessionKind::Delivery, launched, &runs_on, start)
            .await?;
        Ok(session)
    }

    /// A skill runs only on a tool that has it: refused, nothing started or typed, when `tool` doesn't have the skill
    /// `prompt` names for the ticket ("Codex doesn't have the skill `$my-pr`"). The user's click gets the refusal; an
    /// automatic Delivery's card waits for the click, as for a tool that can't run.
    async fn refuse_missing_skill(
        &self,
        ticket: &Ticket,
        tool: Option<Tool>,
        prompt: &FirstPrompt,
    ) -> Result<(), BoardError> {
        let (Some(tool), FirstPrompt::Skill { name, .. }) = (tool, prompt) else {
            return Ok(());
        };
        match self
            .inner
            .board
            .skill_missing_on(ticket, tool, Some(name))
            .await
        {
            Some(skill) => {
                tracing::info!(key = %ticket.key, %tool, skill, "the delivery's tool doesn't have its skill: nothing typed");
                Err(BoardError::refused(
                    Refusal::MoveRefused,
                    crate::flow::proposal::missing_skill_text(tool, &skill),
                ))
            }
            None => Ok(()),
        }
    }

    /// What a delivery session runs on: the Delivery stage's agent (AGM-05), through the check before every launch (T-0357's Q3): refused with the hold's message ("Codex
    /// isn't available · logged out", "Claude Code limit reached", "Model unavailable · Codex no longer lists
    /// gpt-5.4") when it can't start now, nothing held (it is the user's click); a family choice resolved to the
    /// model it passes.
    async fn delivery_runs_on(&self, ticket: &Ticket) -> Result<RunsOn, BoardError> {
        let board = &self.inner.board;
        let agent = resolve::delivery_agent(board, ticket.key.project.as_str()).await?;
        let Some(gate) = self.inner.gate.get() else {
            return Ok(RunsOn::agent(
                &agent,
                board.store().default_model().await?.as_ref(),
            ));
        };
        let mut runs_on = gate.runs_on(&agent).await?;
        let on = gate.model_of(&agent).await?;
        match check(&on, &gate.facts().await?) {
            Ok(model) => {
                runs_on.model = model;
                Ok(runs_on)
            }
            Err(blocked) => {
                let held = Held { agent, on, blocked };
                tracing::info!(key = %ticket.key, reason = %held.blocked.reason(), "delivery refused before launch");
                Err(BoardError::refused(Refusal::MoveRefused, held.message()))
            }
        }
    }

    /// Ends a delivery: its session stops. The ticket keeps its worktree until Done.
    async fn end_delivery(&self, ticket: &Ticket) -> Result<(), BoardError> {
        let inner = &self.inner;
        if let Some(session) = inner.sessions.running_of(&ticket.id, SessionKind::Delivery) {
            inner.sessions.stop(&session.id).await.map_err(other)?;
        }
        Ok(())
    }

    async fn spawn(
        &self,
        ticket: &Ticket,
        kind: SessionKind,
        launched: Launched,
        runs_on: &RunsOn,
        start: Start,
    ) -> Result<SessionInfo, BoardError> {
        let board = &self.inner.board;
        // The delivery's agent commits and pushes: never under a borrowed name, new session or resumed.
        board
            .refuse_without_identity(ticket.key.project.as_str())
            .await?;
        let project = board.project(ticket.key.project.as_str()).await?;
        let session = self
            .inner
            .sessions
            .prepared(launched.prepare)
            .spawn(SpawnSpec {
                kind,
                start,
                project_id: project.id,
                project_key: project.key,
                ticket: Some(SessionTicket {
                    id: ticket.id.clone(),
                    key: ticket.key.clone(),
                }),
                program: launched.plan.program,
                args: launched.plan.args,
                cwd: launched.plan.cwd,
                env: launched.plan.env,
                size: INITIAL_SIZE,
                tool: Some(launched.tool),
                tool_session_id: launched.tool_session_id,
                model: launched.model,
                agent: runs_on.agent.clone(),
                effort: runs_on.effort.clone(),
                theme: Some(launched.theme),
            })
            .await
            .map_err(other)?;
        Ok(session)
    }

    /// "Needs confirmation" when the live session's input line has unsent text and the click didn't say to clear
    /// it.
    fn unsent_first(
        &self,
        ticket: &Ticket,
        live: Option<&SessionInfo>,
        clear_first: bool,
    ) -> Result<Option<PrOutcome>, BoardError> {
        if clear_first {
            return Ok(None);
        }
        if let Some(session) = live
            && self
                .inner
                .sessions
                .has_unsent_input(&session.id)
                .map_err(other)?
        {
            return Ok(Some(PrOutcome::NeedsConfirmation {
                ticket: ticket.key.clone(),
                unclearable_by: self
                    .inner
                    .deliveries
                    .unclearable_by(&session.id)
                    .map_err(other)?,
            }));
        }
        Ok(None)
    }

    /// The prompt a session starts or resumes with for `step`: a skill step's skill by its name (the flow's Delivery
    /// `skill:` for Open PR, else Otaq's), or Otaq's own words. Nothing is written into the worktree (T2).
    async fn first_prompt(
        &self,
        ticket: &Ticket,
        step: PrStep<'_>,
    ) -> Result<FirstPrompt, BoardError> {
        Ok(match step {
            PrStep::Skill(step) => FirstPrompt::Skill {
                name: self
                    .inner
                    .board
                    .delivery_skill_name(ticket.key.project.as_str(), step)
                    .await?,
                message: self.told(ticket).await?,
            },
            PrStep::Words(text) => FirstPrompt::Text(text.to_owned()),
        })
    }

    /// What follows the PR step's skill: an automatic Delivery's skill is told so (FLW-15); an Open PR's, started once
    /// the user confirmed the dialog, is told that.
    async fn told(&self, ticket: &Ticket) -> Result<Option<String>, BoardError> {
        let flow = self.inner.board.pr_flow(&ticket.id).await?;
        Ok(match flow {
            Some(f) if f.automatic => Some(automatic_message()),
            Some(f) if f.kind == FlowKind::Open && f.state == FlowState::Pushing => {
                Some(confirmed_instruction(FlowKind::Open))
            }
            _ => None,
        })
    }

    /// Refused, nothing typed, when the live session's tool doesn't have the flow's Open PR skill.
    async fn skill_runs_in(
        &self,
        ticket: &Ticket,
        live: Option<&SessionInfo>,
    ) -> Result<(), BoardError> {
        let Some(session) = live else {
            return Ok(());
        };
        let name = self
            .inner
            .board
            .delivery_skill(ticket.key.project.as_str(), WorkflowStep::OpenPr)
            .await?;
        self.refuse_missing_skill(
            ticket,
            session.tool,
            &FirstPrompt::Skill {
                name,
                message: None,
            },
        )
        .await
    }

    /// What the Open PR dialog opens on: the PR's commits as they are, and the branch, title and description the
    /// delivery's agent wrote in Review; while it still writes (or with none), the ticket's branch, "SHOP-18 Checkout
    /// API" and the agent's summary.
    async fn prepared_proposal(&self, ticket: &Ticket) -> Result<Proposal, BoardError> {
        let board = &self.inner.board;
        let commits = board.pr_commits(ticket).await?;
        if commits.editable().is_empty() {
            return Err(BoardError::refused(
                Refusal::BadProposal,
                format!("{} has no commits to push", ticket.key),
            ));
        }
        let prepared = board
            .delivery_message(&ticket.id)
            .await?
            .filter(super::message::DeliveryMessage::is_ready);
        let summary = board.review_summary(&ticket.id).await?;
        Ok(Proposal {
            branch: prepared
                .as_ref()
                .and_then(|m| m.branch.clone())
                .unwrap_or_else(|| commits.branch.clone()),
            commits: commits
                .editable()
                .iter()
                .map(|c| super::proposal::CommitMessage {
                    sha: c.sha.clone(),
                    message: c.message.clone(),
                })
                .collect(),
            title: prepared.as_ref().map_or_else(
                || format!("{} {}", ticket.key, ticket.title),
                |m| m.title.clone(),
            ),
            description: prepared
                .map(|m| m.description)
                .or(summary)
                .unwrap_or_default(),
        })
    }

    /// The ticket reached Review on a click-mode flow whose `delivery:` names an agent: that agent writes the
    /// delivery's message now ([`super::message`]), so Approve's confirmation opens at once on it. Typed into its live
    /// session once idle, or its conversation resumed with it, or a delivery session started for it. Asked again
    /// only when the branch has new commits since the last message. Nothing happens for a ticket elsewhere, with a PR
    /// open or a PR dialog under way, with no commits, while the project is paused, or when the agent can't start now
    /// (the confirmation then starts from the plain message). Returns whether it asked.
    pub async fn prepare_message(&self, ticket_id: &str) -> Result<bool, BoardError> {
        self.ask_for_message(ticket_id, false).await
    }

    /// Approve's dialogs are opening on a message still being written: when no session of the ticket runs to write
    /// it (it ended, or an older Otaq asked and was closed), the delivery's agent is asked again, though the branch
    /// head is the same; when it can't be, the request is dropped and the dialog opens on the plain message. A writer
    /// that still runs is left alone. Returns whether it asked again.
    pub async fn ask_again_for_message(&self, ticket_id: &str) -> Result<bool, BoardError> {
        let board = &self.inner.board;
        if !board
            .delivery_message(ticket_id)
            .await?
            .is_some_and(|m| !m.is_ready())
        {
            return Ok(false);
        }
        let ticket = board.ticket_by_id(ticket_id).await?;
        let writer_runs = [SessionKind::Delivery, SessionKind::Agent]
            .into_iter()
            .any(|kind| self.live_of(&ticket, kind).is_some());
        if writer_runs {
            return Ok(false);
        }
        tracing::info!(key = %ticket.key, "no session writes the delivery's message: asking again");
        if self.ask_for_message(ticket_id, true).await? {
            return Ok(true);
        }
        board.forget_delivery_message(ticket_id).await?;
        Ok(false)
    }

    /// [`Self::prepare_message`]; `again` asks even for the head already asked for.
    async fn ask_for_message(&self, ticket_id: &str, again: bool) -> Result<bool, BoardError> {
        let board = &self.inner.board;
        let ticket = board.ticket_by_id(ticket_id).await?;
        if !matches!(ticket.status, Status::WaitingForYou | Status::ToDeliver)
            || ticket.flags.pr_number.is_some()
        {
            return Ok(false);
        }
        let project = ticket.key.project.to_string();
        let flow = board.effective_flow(&project).await?;
        let delivery = &flow.delivery;
        let Some(agent) = delivery.agent.clone() else {
            return Ok(false);
        };
        if delivery.is_automatic() {
            return Ok(false);
        }
        let Some(last) = flow.last_stage() else {
            return Ok(false);
        };
        if !ticket.place.is_in(&last.id) || board.pr_flow(&ticket.id).await?.is_some() {
            return Ok(false);
        }
        if board.commits_beyond_base(&ticket).await? == 0 {
            return Ok(false);
        }
        let head = board.branch_head(&ticket).await;
        if !again
            && head.is_some()
            && board
                .delivery_message(&ticket.id)
                .await?
                .is_some_and(|m| m.head == head)
        {
            return Ok(false);
        }
        if crate::agents::pause::refuse_while_paused(board.store(), &project)
            .await
            .is_err()
        {
            return Ok(false);
        }
        let target = board.merge_target(&ticket).await?.target;
        let text = super::message::prompt(
            &ticket.key.to_string(),
            &target,
            delivery.can_open_pr(),
            delivery.can_merge_locally(),
        );
        board
            .start_delivery_message(&ticket.id, &agent, head.as_deref())
            .await?;
        let kind = self.session_kind_for(&ticket).await;
        let sent = match self.live_of(&ticket, kind) {
            // It may wait behind a busy turn or unsent text: once the ticket no longer waits in Review, To deliver or
            // PR open (sent back to Dev, merged, closed) it is dropped, never typed.
            Some(session) => self
                .inner
                .deliveries
                .queue_while(
                    &session.id,
                    crate::session::deliveries::DeliveryKind::AgentMessage,
                    &text,
                    self.still_waits_to_deliver(&ticket.id),
                )
                .await
                .map_err(other),
            None if kind == SessionKind::Delivery => self
                .start_delivery(&ticket, PrStep::Words(&text), Start::Auto)
                .await
                .map(|_| ()),
            None => match self
                .resume(&ticket, kind, PrStep::Words(&text), Start::Auto)
                .await
            {
                Ok(_) => Ok(()),
                Err(BoardError::Refused {
                    code: Refusal::SessionEnded,
                    ..
                }) => self
                    .start_delivery(&ticket, PrStep::Words(&text), Start::Auto)
                    .await
                    .map(|_| ()),
                Err(e) => Err(e),
            },
        };
        if let Err(e) = sent {
            board.forget_delivery_message(&ticket.id).await?;
            tracing::info!(key = %ticket.key, error = %e, "the delivery's agent can't write the message now");
            return Ok(false);
        }
        tracing::info!(key = %ticket.key, %agent, "the delivery's agent writes the message");
        Ok(true)
    }

    /// Whether the ticket still waits in Review, To deliver or PR open, asked when its queued request for the message
    /// comes to be typed. When it no longer does, the request is dropped and counts as never made: the ticket isn't
    /// recorded as being written, so its next arrival in Review asks again, even on the same branch head.
    fn still_waits_to_deliver(&self, ticket_id: &str) -> crate::session::deliveries::StillWanted {
        let (board, id) = (self.inner.board.clone(), ticket_id.to_owned());
        Arc::new(move || {
            let (board, id) = (board.clone(), id.clone());
            Box::pin(async move {
                let waits = board.ticket_by_id(&id).await.is_ok_and(|t| {
                    matches!(
                        t.status,
                        Status::WaitingForYou | Status::ToDeliver | Status::PrOpen
                    )
                });
                if !waits && let Err(e) = board.drop_unwritten_message(&id).await {
                    tracing::warn!(ticket = id, error = %e, "can't forget the dropped request for the message");
                }
                waits
            })
        })
    }

    /// Open PR or Push, by the step's skill or, for Otaq's own words, by the PR flow under way.
    async fn hand_off_kind(
        &self,
        ticket: &Ticket,
        step: PrStep<'_>,
    ) -> Result<HandOffKind, BoardError> {
        let push = match step {
            PrStep::Skill(step) => step == WorkflowStep::Push,
            PrStep::Words(_) => self
                .inner
                .board
                .pr_flow(&ticket.id)
                .await?
                .is_some_and(|f| f.kind == FlowKind::Push),
        };
        Ok(if push {
            HandOffKind::Push
        } else {
            HandOffKind::OpenPr
        })
    }

    /// Types `step` into the live session (through the user-click path, or queued for an automatic Delivery), or
    /// resumes the stored conversation with it as the first prompt.
    async fn run_in_session(
        &self,
        ticket: &Ticket,
        live: Option<&SessionInfo>,
        step: PrStep<'_>,
        typing: Typing,
    ) -> Result<(), BoardError> {
        let deliveries = &self.inner.deliveries;
        match live {
            Some(session) => {
                let text = match step {
                    // Typed by its name: every session has Otaq's skills from its start (T2); a user's skill only
                    // when the session's tool has it.
                    PrStep::Skill(skill_step) => {
                        let skill = self
                            .inner
                            .board
                            .delivery_skill(ticket.key.project.as_str(), skill_step)
                            .await?;
                        let named = FirstPrompt::Skill {
                            name: skill.clone(),
                            message: None,
                        };
                        self.refuse_missing_skill(ticket, session.tool, &named)
                            .await?;
                        deliveries.skill_text(
                            &session.id,
                            &skill,
                            self.told(ticket).await?.as_deref(),
                        )
                    }
                    PrStep::Words(text) => text.to_owned(),
                };
                // Pending for the agent once typed (d0118): "Open PR · waiting for the agent to pick it up".
                let hand_off = HandOff::new(self.hand_off_kind(ticket, step).await?, text);
                let typed = match typing {
                    Typing::Click { clear_first } => {
                        deliveries
                            .hand_off(&session.id, hand_off, clear_first)
                            .await
                    }
                    Typing::Queued => deliveries
                        .queue_hand_off(&session.id, hand_off)
                        .await
                        .map(|()| Delivery::Sent { count: 0 }),
                };
                if let Delivery::NeedsConfirmation { .. } = typed.map_err(other)? {
                    tracing::warn!(key = %ticket.key, "unsent text appeared before the PR step was typed");
                }
                Ok(())
            }
            None => {
                let kind = self.session_kind_for(ticket).await;
                match self.resume(ticket, kind, step, typing.start()).await {
                    Ok(_) => Ok(()),
                    // A delivery session that left no conversation to resume: a fresh one, on the Delivery's agent.
                    Err(BoardError::Refused {
                        code: Refusal::SessionEnded,
                        ..
                    }) if kind == SessionKind::Delivery => self
                        .start_delivery(ticket, step, typing.start())
                        .await
                        .map(|_| ()),
                    Err(e) => Err(e),
                }
            }
        }
    }

    /// The stored conversation of a session of `kind` resumed on its tool and model with `step` first, in the
    /// ticket's worktree; the ticket keeps its status.
    async fn resume(
        &self,
        ticket: &Ticket,
        kind: SessionKind,
        step: PrStep<'_>,
        start: Start,
    ) -> Result<SessionInfo, BoardError> {
        let inner = &self.inner;
        let history = inner
            .sessions
            .history(&SessionTicket {
                id: ticket.id.clone(),
                key: ticket.key.clone(),
            })
            .await
            .map_err(other)?;
        let conversation = history
            .iter()
            .filter(|s| s.kind == kind)
            .find_map(StoredConversation::of)
            .ok_or_else(|| {
                BoardError::refused(
                    Refusal::SessionEnded,
                    format!("{} has no conversation to resume", ticket.key),
                )
            })?;
        let runs_on = RunsOn::resumed(&conversation);
        inner.adapters.unavailable(runs_on.tool)?;
        let worktree = worktree_of(ticket)?;
        let first_prompt = self.first_prompt(ticket, step).await?;
        // Resumed with the Delivery's skill only on a tool that has it.
        self.refuse_missing_skill(ticket, runs_on.tool, &first_prompt)
            .await?;
        let port = ticket.work.as_ref().and_then(|w| w.agent_port).unwrap_or(0);
        let role = match kind {
            SessionKind::Delivery => LaunchRole::Delivery,
            _ => LaunchRole::TicketAgent,
        };
        let launched = inner.adapters.launch(
            runs_on.tool,
            &LaunchRequest {
                role,
                cwd: worktree,
                project: ticket.key.project.to_string(),
                ticket: Some(ticket.key.clone()),
                port,
                first_prompt: Some(first_prompt),
                resume: Some(conversation.id),
                model: runs_on.model.clone(),
                effort: runs_on.effort.clone(),
                title: Some(ticket.title.clone()),
                theme: self.inner.board.theme_in_use().await,
            },
        )?;
        self.spawn(ticket, kind, launched, &runs_on, start).await
    }
}

impl Board {
    /// An automatic PR delivery left waiting because no PR provider matches the remote (`on`), or no longer
    /// (`off`): the card says so with Set up… ([`crate::domain::Flags::pr_provider_not_detected`]). Kept in the
    /// ticket's start-problem columns while it waits To deliver.
    pub(crate) async fn mark_no_pr_provider(
        &self,
        ticket_id: &str,
        on: bool,
    ) -> Result<(), BoardError> {
        if on {
            sqlx::query(
                "UPDATE tickets SET start_problem_step = ?, start_problem_message = ? WHERE id = ? AND state = ?",
            )
            .bind(crate::board::tickets::NO_PR_PROVIDER_MARK)
            .bind(super::provider::NO_PROVIDER)
            .bind(ticket_id)
            .bind(Status::ToDeliver.as_str())
            .execute(self.store().pool())
            .await?;
        } else {
            sqlx::query(
                "UPDATE tickets SET start_problem_step = NULL, start_problem_message = NULL
                 WHERE id = ? AND start_problem_step = ?",
            )
            .bind(ticket_id)
            .bind(crate::board::tickets::NO_PR_PROVIDER_MARK)
            .execute(self.store().pool())
            .await?;
        }
        Ok(())
    }

    /// Approved · PR open (REV-13): after the skill reported the PR.
    pub(crate) async fn record_pr_open(
        &self,
        ticket: &Ticket,
        actor: &Actor,
    ) -> Result<(), BoardError> {
        let to = Status::PrOpen;
        // Into the Delivery stage, "PR open" (STG-06): from the human check whose Pass offered it, or (pushed again)
        // where it is.
        let moved = self
            .moved(ticket, ticket.status, crate::flow::engine::Move::PrOpened)
            .await
            .ok();
        let delivery = match &moved {
            Some(_) => None,
            None => self.delivery_now(ticket).await?.map(|d| d.stage.id),
        };
        let stage = moved
            .as_ref()
            .and_then(|m| m.place.stage_id().map(str::to_owned))
            .or(delivery)
            .or_else(|| crate::flow::engine::default_stage(to, None).map(str::to_owned));
        let now = now_ms();
        let mut tx = self.store().begin_write().await?;
        if let Some(moved) = &moved {
            moved.record(&mut tx, &ticket.id, None).await?;
        }
        sqlx::query(concat!(
            "UPDATE tickets SET stage_id = ?, state = ?, interrupted_from = NULL, updated_by = ?, updated_at = ?, ",
            crate::store::tickets::set_state_since!(),
            " WHERE id = ?"
        ))
        .bind(stage)
        .bind(to.as_str())
        .bind(actor.to_string())
        .bind(now)
        .bind(to.as_str())
        .bind(now)
        .bind(&ticket.id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }
}

/// The ticket's worktree folder.
fn worktree_of(ticket: &Ticket) -> Result<PathBuf, BoardError> {
    ticket
        .work
        .as_ref()
        .and_then(|w| w.worktree.as_ref())
        .map(PathBuf::from)
        .ok_or_else(|| {
            BoardError::refused(
                Refusal::NoWorktree,
                format!("{} has no worktree", ticket.key),
            )
        })
}

fn other(e: impl std::fmt::Display) -> BoardError {
    BoardError::refused(Refusal::SessionEnded, e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The open-pr and push skills wait for these words before they push.
    #[test]
    fn the_confirmation_says_the_user_confirmed() {
        assert!(
            confirmed_instruction(FlowKind::Open)
                .starts_with("The user confirmed the pull request.")
        );
        assert!(confirmed_instruction(FlowKind::Push).starts_with("The user confirmed the push."));
        for kind in [FlowKind::Open, FlowKind::Push] {
            let text = confirmed_instruction(kind);
            assert!(text.contains(&format!("`{} prime`", naming::COMMAND_NAME)));
            assert!(!text.contains('\n'), "one line, typed with Enter");
        }
    }

    /// "Use your own Open PR skill" points at the delivery's PR skill in Edit flow, never at the agent's skills.
    #[test]
    fn own_open_pr_skill_names_the_flows_delivery() {
        let text = own_open_pr_skill("PR provider not detected");
        assert_eq!(
            text,
            "PR provider not detected: use your own Open PR skill (name it as the delivery\u{2019}s `skill:` in \
             the flow file: Edit flow \u{2192} Done \u{2192} PR skill)"
        );
        assert!(asks_for_own_open_pr_skill(&text));
        assert!(!asks_for_own_open_pr_skill("Nothing to push"));
        assert!(!text.contains("Settings"));
        assert!(!text.contains("agent's skills") && !text.contains("agent\u{2019}s skills"));
    }
}
