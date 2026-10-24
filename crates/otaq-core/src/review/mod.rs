//! The user's review actions that reach a ticket's agent session (REV-04, GIT-03, FLW-14; d0034, d0038, d0048,
//! d0108).
//!
//! **Pass** on any human check ([`Review::pass`]) moves the card on, or, directly before a click Delivery, offers the
//! delivery choices (today's Approve → Local merge / Open PR).
//!
//! **Fail** ([`Review::fail`], Send back on the default flow) returns a ticket waiting in a human check to the nearest
//! earlier Agent stage with the round's line comments and the note, the note posted first as the user's comment
//! (d0034, d0108 rule 6, d0109); it then walks every later stage again. The click is the go-ahead (d0018), so it is
//! never held, except by the optional agent limit:
//! - the session is **live**: `/otaq-address-review` is typed into it (a copy it started with, d0118; one it didn't
//!   is named by its file, [`crate::session::deliveries::handoff_sentence`]), with what came back and never its text ("SHOP-18 was sent back to Dev with 2 line comments and a comment. Run `otaq
//!   prime`, then `otaq review feedback`.", CMT-05), at once through the user-click path (unsent text in its input
//!   line answers "needs confirmation" first, d0048) and the ticket is Agent Working;
//! - the session is **gone**: the same click resumes the stored conversation on its tool, in the
//!   ticket's worktree with the skill and that text as its first prompt;
//! - the agent limit is **on and full**: the ticket waits in Queued; the scheduler starts it when a slot frees,
//!   and the skill is typed (or the conversation resumed) then.
//!
//! The agent then fetches the feedback itself with `otaq review feedback` (REV-05).
//!
//! A build freed at Review comes back first (M19-c): at the click for a live session, before the skill is typed;
//! by the start for a resumed or queued one. A failure there never refuses the click.

mod apply_now;
mod close;
pub mod delivery;
pub mod drag;
pub mod finish;
mod merge_check;
pub mod park;
pub mod pr_state;
pub mod resume;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::board::conflicts::{Conflict, ConflictReason};
use crate::board::hand_over::{HandOver, settled};
use crate::board::reports::ReportSource;
use crate::board::review::{ApproveCheck, EPIC_NO_SEND_BACK};
use crate::board::tickets::Ticket;
use crate::board::validation::Transition;
use crate::board::{Board, BoardError, Refusal};
use crate::data_dir::DataDir;
use crate::domain::{Actor, HandOffKind, Status, TicketKey, ValidationTrigger};
use crate::flow::Delivers;
use crate::flow::engine::Counters;
use crate::git::merge::{MergeOutcome, MergeRequest};
use crate::ports::PortPool;
use crate::scheduler::Scheduler;
use crate::session::deliveries::{Deliveries, Delivery};
use crate::session::hand_offs::HandOff;
use crate::session::{SessionKind, Sessions, Start};
use crate::start::{StartOutcome, Starter};
use crate::ticket_env::TicketEnv;
use crate::ticket_env::fetch::FetchWhy;

/// The skill Send back runs (REV-04).
pub const ADDRESS_REVIEW_SKILL: &str = "address-review";
/// The skill a Delivery's conflict runs, with no click: rebase onto the target and resolve the conflicts (GIT-03,
/// FLW-17).
pub const RESTACK_SKILL: &str = "restack";

/// The answer to a click that returns a ticket to its agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum ReturnOutcome {
    /// The ticket is back in Dev: Agent Working with the skill typed or resumed, or Queued while the agent
    /// limit is full (or Start interrupted if the resume couldn't start).
    Done { ticket: Box<Ticket> },
    /// The session's input line has unsent text: ask "Keep it and cancel / Clear it and send" (d0048). Nothing
    /// was changed. `unclearable_by` names the tool when the text is a multi-line draft it can't clear: only Keep
    /// it and cancel (SES-07, Q-N).
    NeedsConfirmation {
        ticket: TicketKey,
        #[serde(default)]
        unclearable_by: Option<String>,
    },
}

/// The answer to a Pass on a human check (FLW-14).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum PassOutcome {
    /// Directly before a click Delivery: nothing changed; the user picks Local merge or Open PR (`either`: both),
    /// as today's Approve offers them.
    Choose { delivers: Delivers },
    /// The card moved on (the ticket afterwards).
    Moved { ticket: Box<Ticket> },
}

/// The summary a Pass into an agent check records for the human check it leaves.
pub const PASSED_BY_YOU: &str = "Passed by you";

/// The answer to a Local merge click (GIT-03, REV-08).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum LocalMergeOutcome {
    /// Done · Merged into `target`; `started` are the waiting tickets the scheduler started after it.
    Merged {
        ticket: Box<Ticket>,
        target: String,
        /// The squash commit on the target.
        commit: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        started: Vec<TicketKey>,
    },
    /// "SHOP-19 has 2 uncommitted files" · View · Send back.
    RefusedUncommitted { files: Vec<String>, message: String },
    /// "`develop` and `origin/develop` have diverged. Push or pull, then retry".
    RefusedDiverged { message: String },
    /// "shop-web has uncommitted changes on main. Clean up and retry."
    RefusedDirtyCheckout { message: String },
    /// The free rebase conflicted: nothing was merged; the ticket went back to its nearest earlier Agent stage,
    /// whose session fixes it with `restack` (no click), or stopped there after the third conflict in a row
    /// (FLW-17). An epic's conflict stays on its card.
    Conflict { target: String, files: Vec<String> },
    /// The Merge check `command` failed on the merged result (d0120; `exit` `None`: a signal ended it): nothing was
    /// merged; the ticket went back to its nearest earlier Agent stage with a comment holding the output as its
    /// report, whose session fixes it with `restack` (no click), or stopped there after the third conflict or
    /// failed check in a row. An epic's stays on its card, with the comment.
    CheckFailed {
        target: String,
        command: String,
        exit: Option<i32>,
    },
    /// The user stopped the Merge check (Q475): nothing was merged and the ticket stays where it was.
    CheckStopped { target: String },
    /// New commits since the review (REV-06): nothing was done; approve again with `approve_anyway`, or Send
    /// back.
    Changed { check: ApproveCheck },
}

/// Runs review actions. Cheap to clone.
#[derive(Clone)]
pub struct Review {
    inner: Arc<Inner>,
}

struct Inner {
    board: Board,
    data: DataDir,
    sessions: Sessions,
    deliveries: Deliveries,
    starter: Starter,
    scheduler: Scheduler,
    /// One Local merge at a time: they share the scratch worktree and move base branches, and each Merge check
    /// sees the merges before it (d0120).
    merging: tokio::sync::Mutex<()>,
    ports: PortPool,
    /// Done releases a clean worktree's build into the stock (WRM-03), unless it was freed at Review (M19-c).
    ticket_env: TicketEnv,
    finished_hooks: Mutex<Vec<finish::FinishedHook>>,
    /// Run for every parked ticket before its ports are freed (Q30: Park stops Run).
    parked_hooks: Mutex<Vec<finish::FinishedHook>>,
    /// Resumes a ticket interrupted while it validated: a fresh validation, not the agent (d0063).
    validation_resume: Mutex<Option<resume::ValidationResume>>,
}

impl Review {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        board: Board,
        data: DataDir,
        sessions: Sessions,
        deliveries: Deliveries,
        starter: Starter,
        scheduler: Scheduler,
        ports: PortPool,
        ticket_env: TicketEnv,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                board,
                data,
                merging: tokio::sync::Mutex::new(()),
                ports,
                ticket_env,
                finished_hooks: Mutex::new(Vec::new()),
                parked_hooks: Mutex::new(Vec::new()),
                validation_resume: Mutex::new(None),
                sessions,
                deliveries,
                starter,
                scheduler,
            }),
        }
    }

    pub fn board(&self) -> &Board {
        &self.inner.board
    }

    /// **Fail** on a human check (FLW-14; Send back on the default flow, REV-04): `note` (with `report`, the dialog's
    /// pasted text) is the user's comment and the pending line comments go with it to the nearest earlier Agent
    /// stage's session, which continues there; every agent check's counter restarts. `clear_first` is the user's
    /// "Clear it and send" after a "needs confirmation" answer. A note over the limit refuses the whole Fail.
    pub async fn fail(
        &self,
        key: &str,
        note: &str,
        report: Option<ReportSource>,
        clear_first: bool,
        actor: &Actor,
    ) -> Result<ReturnOutcome, BoardError> {
        let ticket = self.inner.board.ticket(key).await?;
        if ticket.is_epic() {
            return Err(BoardError::refused(Refusal::MoveRefused, EPIC_NO_SEND_BACK));
        }
        if ticket.status != Status::WaitingForYou {
            return Err(BoardError::refused(
                Refusal::MoveRefused,
                format!(
                    "{} is {}; Send back works on a ticket in Ready for Review",
                    ticket.key,
                    ticket.status.label()
                ),
            ));
        }
        let hand_over = self
            .inner
            .board
            .user_hand_over(&ticket, note, report, actor)
            .await?;
        let action = Return::SendBack {
            hand_over: hand_over.as_ref(),
        };
        self.return_to_agent(&ticket, action, clear_first, actor)
            .await
    }

    /// **Pass** on a human check (FLW-14, STG-06): directly before a click Delivery it changes nothing and offers the
    /// delivery choices (an epic child's: Local merge only, d0108 rule 7); otherwise the card moves to the next stage
    /// the ticket runs: another human check, an agent check (its checker starts, through `validation`), the wait
    /// before an Agent stage (the scheduler starts it), or its Delivery.
    pub async fn pass(
        &self,
        key: &str,
        validation: &crate::validation::Validation,
        actor: &Actor,
    ) -> Result<PassOutcome, BoardError> {
        let board = &self.inner.board;
        let ticket = board.ticket(key).await?;
        let moved = board.pass_step(&ticket).await?;
        if let Some(delivers) = moved.choose {
            let delivers = match ticket.epic {
                Some(_) => Delivers::Local,
                None => delivers,
            };
            return Ok(PassOutcome::Choose { delivers });
        }
        if moved.start_check {
            // As a submit into an agent check: the round opens on the reviewed head, then its checker starts.
            let head = board.submit_head(&ticket).await?;
            board.record_pass_runs(&ticket, &moved).await?;
            let into = Transition {
                to: moved.status,
                place: moved.place.clone(),
                entry: None,
                counters: Counters::Keep,
                outcome: None,
                start_validator: true,
                deliver_findings: false,
                tell_cancelled: false,
                extra_round: false,
                skipped: Vec::new(),
            };
            board
                .enter_validation(
                    &ticket,
                    &into,
                    &head,
                    PASSED_BY_YOU,
                    None,
                    ValidationTrigger::User,
                    false,
                    actor,
                )
                .await?;
            validation.start_held(&ticket.id).await?;
        } else {
            board.record_pass(&ticket, &moved, actor).await?;
            if moved.place.is_waiting() {
                self.inner.scheduler.wake();
            }
            // Into an automatic Delivery: it delivers now, unless the card was moved past a check (FLW-15).
            board.delivery().arrived(&ticket.id).await;
        }
        Ok(PassOutcome::Moved {
            ticket: Box::new(board.ticket(key).await?),
        })
    }

    /// **Local merge** (GIT-03): the user's approve action that spends no tokens. `message` is the confirmed
    /// squash message ([`Board::merge_proposal`] proposes one). A conflict goes to the agent with `restack` at once,
    /// no click (FLW-17, [`Self::delivery_conflict`]).
    pub async fn local_merge(
        &self,
        key: &str,
        message: &str,
        approve_anyway: bool,
        actor: &Actor,
    ) -> Result<LocalMergeOutcome, BoardError> {
        let Merge { outcome, failed } = self.merge(key, message, approve_anyway, actor).await?;
        match (&outcome, failed) {
            (LocalMergeOutcome::Conflict { target, .. }, _) => {
                let ticket = self.inner.board.ticket(key).await?;
                self.delivery_conflict(&ticket, target, actor).await?;
            }
            (_, Some(failed)) => {
                let ticket = self.inner.board.ticket(key).await?;
                self.delivery_check_failed(&ticket, &failed, actor).await?;
            }
            _ => {}
        }
        Ok(outcome)
    }

    /// Stops the Merge check `key`'s Local merge runs now (Q475): nothing is merged and the ticket stays where it
    /// was. Refused when it runs none.
    pub async fn stop_merge_check(&self, key: &str) -> Result<(), BoardError> {
        let board = &self.inner.board;
        let ticket = board.ticket(key).await?;
        if board.merge_checks().stop(&ticket.id) {
            return Ok(());
        }
        Err(BoardError::refused(
            Refusal::MoveRefused,
            format!("{} isn't running its merge check", ticket.key),
        ))
    }

    /// The Local merge itself, from a human check that offers it or from the Delivery stage (the click, or an
    /// automatic Delivery): a conflict or a failed Merge check is only recorded on the ticket.
    async fn merge(
        &self,
        key: &str,
        message: &str,
        approve_anyway: bool,
        actor: &Actor,
    ) -> Result<Merge, BoardError> {
        let inner = &self.inner;
        let board = &inner.board;
        let _one_at_a_time = inner.merging.lock().await;
        let ticket = board.ticket(key).await?;
        if !matches!(ticket.status, Status::WaitingForYou | Status::ToDeliver) {
            return Err(BoardError::refused(
                Refusal::MoveRefused,
                format!(
                    "{} is {}; Local merge works on a ticket in Ready for Review",
                    ticket.key,
                    ticket.status.label()
                ),
            ));
        }
        if message.trim().is_empty() {
            return Err(BoardError::refused(
                Refusal::EmptyField,
                "Write the squash message first",
            ));
        }
        if !approve_anyway {
            let check = board.approve_check(key).await?;
            if check != ApproveCheck::Unchanged {
                return Ok(LocalMergeOutcome::Changed { check }.into());
            }
        }
        let target = board.merge_target(&ticket).await?;
        let project = board.project(ticket.key.project.as_str()).await?;
        let repo = PathBuf::from(&project.repo_path);
        let repo_name = repo
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| project.name.clone());
        // With the project's variables: the merge's rebase, commit and checkouts run the user's hooks (ENV-02).
        let git = &board.project_git(&project.key).await?;
        let settings = board.project_settings(&project.key).await?;
        let command = settings.merge_check().map(str::to_owned);
        let check = command.as_deref().map(|command| {
            merge_check::merge_check(
                board,
                &inner.data,
                inner.ticket_env.seeder(),
                git,
                &project,
                &settings,
                &ticket,
                command,
            )
        });
        let recheck = board.merge_check_due(&ticket.id).await?;
        let request = MergeRequest {
            check,
            recheck,
            scratch: inner.data.scratch_path(&repo_name).map_err(other)?,
            repo,
            project_name: project.name.clone(),
            key: ticket.key.to_string(),
            branch: target.branch.clone(),
            worktree: ticket
                .work
                .as_ref()
                .and_then(|w| w.worktree.as_ref())
                .map(PathBuf::from),
            target: target.target.clone(),
            sync_target: !target.into_epic,
            message: message.trim().to_owned(),
        };
        let branch_ref = format!("refs/heads/{}", target.branch);
        let head_before = git.resolve(&request.repo, &branch_ref).await?;
        let outcome = git.local_merge(&request).await;
        // The free rebase isn't a change to review (GIT-05), even when a later step failed.
        if !matches!(outcome, Ok(MergeOutcome::Merged { .. }))
            && let (Some(before), Ok(Some(after))) = (
                head_before.as_deref(),
                git.resolve(&request.repo, &branch_ref).await,
            )
        {
            board.record_otaq_rebase(&ticket.id, before, &after).await?;
            // Rebased onto new commits and no check passed on them: the next merge checks even so (d0120).
            if before != after {
                board.set_merge_check_due(&ticket.id, true).await?;
            }
        }
        if matches!(outcome, Ok(MergeOutcome::CheckFailed { .. })) {
            board.set_merge_check_due(&ticket.id, true).await?;
        }
        let mut failed = None;
        let outcome = match outcome? {
            MergeOutcome::Merged { commit, onto, .. } => {
                let merged = board
                    .record_local_merge(&ticket, &target.target, &commit, &onto, actor)
                    .await?;
                // Done: the session ends, ports return, the worktree goes unless it has changes (REC-05).
                if let Err(e) = self.finish(&merged).await {
                    tracing::warn!(ticket = %merged.key, error = %e, "Done cleanup failed");
                }
                // An epic child merged: the epic may be ready for its own review (EPC-04).
                if let Some(epic) = &merged.epic {
                    board.reevaluate_epic(&epic.to_string(), actor).await?;
                }
                board
                    .leads()
                    .child_finished(&merged, crate::epic_lead::wake::Finished::Merged)
                    .await;
                let merged = board.ticket_by_id(&merged.id).await?;
                // The early conflict flag on every other ticket in Review that merges into the same branch.
                if let Err(e) = self
                    .flag_conflicts(
                        &ticket.key.project.to_string(),
                        &request.repo,
                        &target.target,
                    )
                    .await
                {
                    tracing::warn!(target = %target.target, error = %e, "can't test-merge the tickets in Review");
                }
                // Waiting tickets whose last parent this was start now (T-0081).
                let started = match inner.scheduler.on_parent_merged(key).await {
                    Ok(plan) => plan.start.into_iter().map(|t| t.key).collect(),
                    Err(e) => {
                        tracing::warn!(ticket = %ticket.key, error = %e, "can't start the waiting tickets");
                        Vec::new()
                    }
                };
                LocalMergeOutcome::Merged {
                    ticket: Box::new(merged),
                    target: target.target,
                    commit,
                    started,
                }
            }
            MergeOutcome::RefusedUncommitted { files, message } => {
                LocalMergeOutcome::RefusedUncommitted { files, message }
            }
            MergeOutcome::RefusedDiverged { message } => {
                LocalMergeOutcome::RefusedDiverged { message }
            }
            MergeOutcome::RefusedDirtyCheckout { message } => {
                LocalMergeOutcome::RefusedDirtyCheckout { message }
            }
            MergeOutcome::Conflict { files } => {
                board
                    .set_conflict(
                        &ticket.id,
                        &Conflict {
                            target: target.target.clone(),
                            files: files.clone(),
                            reason: ConflictReason::Merge,
                        },
                    )
                    .await?;
                LocalMergeOutcome::Conflict {
                    target: target.target,
                    files,
                }
            }
            MergeOutcome::CheckFailed { exit, output } => {
                // Recorded like a conflict, with no files: it clears when the agent moves on again.
                board
                    .set_conflict(
                        &ticket.id,
                        &Conflict {
                            target: target.target.clone(),
                            files: Vec::new(),
                            reason: ConflictReason::MergeCheck,
                        },
                    )
                    .await?;
                let command = command.unwrap_or_default();
                failed = Some(CheckFailure {
                    target: target.target.clone(),
                    command: command.clone(),
                    exit,
                    output,
                });
                LocalMergeOutcome::CheckFailed {
                    target: target.target,
                    command,
                    exit,
                }
            }
            MergeOutcome::CheckStopped => LocalMergeOutcome::CheckStopped {
                target: target.target,
            },
        };
        Ok(Merge { outcome, failed })
    }

    /// After a merge into `target` (GIT-04, d0038): a read-only test merge of every ticket in Ready for Review that
    /// merges into it; "conflicts with `target`" is set or cleared. No files touched, no tokens.
    pub async fn flag_conflicts(
        &self,
        project: &str,
        repo: &std::path::Path,
        target: &str,
    ) -> Result<(), BoardError> {
        let board = &self.inner.board;
        let git = board.git()?;
        for ticket in board.tickets(project).await? {
            if ticket.status != Status::WaitingForYou {
                continue;
            }
            let Ok(merge) = board.merge_target(&ticket).await else {
                continue;
            };
            if merge.target != target {
                continue;
            }
            match git.test_merge(repo, target, &merge.branch).await? {
                Some(files) => {
                    board
                        .set_conflict(
                            &ticket.id,
                            &Conflict {
                                target: target.to_owned(),
                                files,
                                reason: ConflictReason::Merge,
                            },
                        )
                        .await?;
                    tracing::info!(ticket = %ticket.key, target, "conflicts with the merged branch");
                }
                None => {
                    if board
                        .conflict(&ticket.id)
                        .await?
                        .is_some_and(|c| c.reason == ConflictReason::Merge)
                    {
                        board.clear_conflict(&ticket.id).await?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Whether the optional agent limit is on and every slot is taken (d0016). Back-pressure doesn't hold a
    /// user's click.
    pub async fn limit_full(&self) -> Result<bool, BoardError> {
        let status = self.inner.scheduler.status().await?;
        Ok(status.limits.enabled && status.holding >= status.limits.max_concurrency.max(1))
    }

    /// The action's bookkeeping: posts the user's comment, moves the ticket to `to` (Agent Working or Queued) with the
    /// skill still to run, if any. Returns what the agent is told after the skill (stored already for a queued
    /// ticket).
    async fn record(
        &self,
        ticket: &Ticket,
        action: &Return<'_>,
        to: Status,
        skill: Option<&str>,
        actor: &Actor,
    ) -> Result<Option<String>, BoardError> {
        let board = &self.inner.board;
        match action {
            Return::SendBack { hand_over } => board
                .record_send_back(ticket, *hand_over, to, skill, actor)
                .await
                .map(|sent| sent.told),
            Return::Fix { target, hand_over } => board
                .record_conflict_fix(ticket, target, *hand_over, to, skill, actor)
                .await
                .map(|()| None),
            Return::PrComments { hand_over } => board
                .record_send_back_pr(ticket, *hand_over, to, skill, actor)
                .await
                .map(Some),
            Return::DragBack(back) => back.record(board, ticket, to, skill, actor).await,
        }
    }

    /// Returns a ticket to its agent with the action's skill: types it into the live session, resumes the
    /// conversation with it, or queues the ticket while the limit is full. The user's comment's report, if any, is
    /// discarded when nothing was posted (unsent text asked first, or a refusal).
    async fn return_to_agent(
        &self,
        ticket: &Ticket,
        action: Return<'_>,
        clear_first: bool,
        actor: &Actor,
    ) -> Result<ReturnOutcome, BoardError> {
        let hand_over = action.hand_over();
        let outcome = self.return_with(ticket, &action, clear_first, actor).await;
        if matches!(outcome, Ok(ReturnOutcome::NeedsConfirmation { .. }))
            && let Some(hand_over) = hand_over
        {
            hand_over.discard_unposted(&self.inner.board).await;
        }
        settled(&self.inner.board, hand_over, outcome).await
    }

    async fn return_with(
        &self,
        ticket: &Ticket,
        action: &Return<'_>,
        clear_first: bool,
        actor: &Actor,
    ) -> Result<ReturnOutcome, BoardError> {
        let inner = &self.inner;
        let skill = action.skill();
        let full = self.limit_full().await?;
        let live = inner.sessions.running_of(&ticket.id, SessionKind::Agent);
        if !full
            && !clear_first
            && let Some(session) = &live
            && inner
                .sessions
                .has_unsent_input(&session.id)
                .map_err(other)?
        {
            return Ok(ReturnOutcome::NeedsConfirmation {
                ticket: ticket.key.clone(),
                unclearable_by: inner
                    .deliveries
                    .unclearable_by(&session.id)
                    .map_err(other)?,
            });
        }
        let key = ticket.key.to_string();
        match (&live, full) {
            (_, true) => {
                self.record(ticket, action, Status::Queued, Some(skill), actor)
                    .await?;
                inner.scheduler.wake();
            }
            (Some(session), false) => {
                // The build first, then the skill; the ticket's lock is held until it is in Dev, so no Free
                // takes the build away meanwhile.
                let _held = inner
                    .ticket_env
                    .fetch_for_agent(ticket, action.fetch_why(), actor)
                    .await;
                let told = self
                    .record(ticket, action, Status::Working, None, actor)
                    .await?;
                let worktree = ticket
                    .work
                    .as_ref()
                    .and_then(|w| w.worktree.as_deref())
                    .map(std::path::Path::new);
                // The skill, then what came back (never the note's text: the agent reads it in `prime`, CMT-05).
                let text = inner
                    .deliveries
                    .skill_line(&inner.board, &session.id, worktree, skill, told.as_deref())
                    .await;
                // Pending for the agent once typed (d0118).
                let typed = inner
                    .deliveries
                    .hand_off(
                        &session.id,
                        HandOff::new(action.hand_off(), text),
                        clear_first,
                    )
                    .await;
                match typed.map_err(other)? {
                    Delivery::Sent { .. } => {}
                    // Text was typed in the moment between the check and the send: it stays as the user typed it.
                    Delivery::NeedsConfirmation { .. } => tracing::warn!(
                        ticket = %ticket.key,
                        "unsent text appeared before the skill was typed"
                    ),
                }
            }
            (None, false) => {
                self.record(ticket, action, Status::Queued, Some(skill), actor)
                    .await?;
                match inner.starter.start(&key, actor, Start::Auto).await? {
                    StartOutcome::Started(_) | StartOutcome::Skipped | StartOutcome::Kept => {}
                    StartOutcome::Held { message } | StartOutcome::Interrupted { message, .. } => {
                        tracing::warn!(ticket = %ticket.key, %message, "resuming the agent stopped");
                    }
                }
            }
        }
        Ok(ReturnOutcome::Done {
            ticket: Box::new(inner.board.ticket(&key).await?),
        })
    }
}

/// A Local merge's answer, with what the hand-back of a failed Merge check needs.
struct Merge {
    outcome: LocalMergeOutcome,
    failed: Option<CheckFailure>,
}

impl From<LocalMergeOutcome> for Merge {
    fn from(outcome: LocalMergeOutcome) -> Self {
        Self {
            outcome,
            failed: None,
        }
    }
}

/// A failed Merge check (d0120): the file holding its output.
pub(crate) struct CheckFailure {
    pub(crate) target: String,
    pub(crate) command: String,
    pub(crate) exit: Option<i32>,
    pub(crate) output: PathBuf,
}

/// What returns a ticket to its agent.
enum Return<'a> {
    /// The user's note as their comment, if they wrote one.
    SendBack { hand_over: Option<&'a HandOver> },
    /// A Delivery's conflict (FLW-17), or its failed Merge check with the comment to post (d0120).
    Fix {
        target: &'a str,
        hand_over: Option<&'a HandOver>,
    },
    /// Send back with PR comments (FLW-16): the agent reads them with `otaq review feedback`.
    PrComments { hand_over: Option<&'a HandOver> },
    /// A drag back to an earlier Agent stage with the user's instructions (FLW-18).
    DragBack(drag::Back<'a>),
}

impl<'a> Return<'a> {
    fn hand_over(&self) -> Option<&'a HandOver> {
        match self {
            Self::SendBack { hand_over } | Self::PrComments { hand_over } => *hand_over,
            Self::DragBack(back) => back.hand_over,
            Self::Fix { hand_over, .. } => *hand_over,
        }
    }

    fn skill(&self) -> &'static str {
        match self {
            Self::SendBack { .. } | Self::PrComments { .. } | Self::DragBack(_) => {
                ADDRESS_REVIEW_SKILL
            }
            Self::Fix { .. } => RESTACK_SKILL,
        }
    }

    /// What the card says while the agent hasn't picked it up (d0118).
    fn hand_off(&self) -> HandOffKind {
        match self {
            Self::SendBack { .. } | Self::PrComments { .. } | Self::DragBack(_) => {
                HandOffKind::SendBack
            }
            // A failed Merge check comes back with its comment (d0120).
            Self::Fix {
                hand_over: Some(_), ..
            } => HandOffKind::MergeCheck,
            Self::Fix { .. } => HandOffKind::MergeConflict,
        }
    }

    fn fetch_why(&self) -> FetchWhy {
        match self {
            Self::SendBack { .. } | Self::PrComments { .. } | Self::DragBack(_) => {
                FetchWhy::SendBack
            }
            Self::Fix { .. } => FetchWhy::Fix,
        }
    }
}

fn other(e: impl std::fmt::Display) -> BoardError {
    BoardError::refused(Refusal::SessionEnded, e.to_string())
}
