//! **The Delivery** (FLW-15, FLW-16, FLW-17; d0108, d0038, d0050, d0059, d0064, d0018): what happens once a ticket
//! reaches its flow's Delivery stage, in its mode, and when the work comes back from it.
//!
//! - **Automatic mode** ([`run_automatic`], called through [`crate::board::merge::DeliveryLink`] after every move that
//!   can enter the Delivery): `delivers: local` Local-merges without the confirmation, with the squash message the
//!   confirmation would propose: Done · "Merged automatically", a History line, no inbox item and no notification;
//!   the Done cleanup and the waiting tickets follow as after any Local merge (the flow is the user's standing
//!   go-ahead). `delivers: pr` runs the Delivery's agent with its PR skill and no dialog ([`crate::pr::service`]).
//!   With no check passed before it, the ticket records "Delivered without checks". A base that diverged or the
//!   user's dirty checkout stops it, turquoise, with the Local merge's message; Retry merge and Move to Review are
//!   the user's ([`crate::board::merge`]).
//! - **Click mode** stays the user's Local merge or Open PR (0050's dialog).
//! - **A conflict**, in either mode ([`Review::delivery_conflict`]): nothing merged; the ticket's nearest earlier
//!   Agent stage takes it back and its session gets `restack` **without a click** (typed once its input line is
//!   empty, never over the user's text, or resumed): "Merge conflict · fixing". The fixed ticket walks the later
//!   stages again. The third conflict in a row stops it there, turquoise: "Keeps conflicting with `<target>`". A
//!   merge in between resets the count. An epic child's first such stop goes to its live lead instead, once
//!   (LEAD-07, [`crate::epic_lead::stuck`]).
//! - **A failed Merge check** (d0120, [`Review::delivery_check_failed`]): the same path, with a comment "Merge check
//!   `npm test` failed on the result rebased onto main (exit 1). Fix it, then submit again." holding the output as
//!   its report (d0109): "Merge check failed · fixing". It counts toward the same three in a row; when the third is a
//!   failed check the stop says "Merge check keeps failing on `<target>`".
//! - **Send back with PR comments** ([`Review::send_back_pr`]): a card whose PR is open goes back to its nearest
//!   earlier Agent stage, whose session resumes with `address-review` (it reads the unresolved PR comments with `otaq
//!   review feedback`) and the user's optional note; the later stages run again and the Delivery pushes to the same
//!   PR in its mode. On an epic, **Add a child from PR comments** ([`add_child_from_pr`]) adds a Backlog child holding
//!   the unresolved comments instead.

use std::ffi::OsString;

use super::merge_check::capped_output;
use super::{CheckFailure, LocalMergeOutcome, Merge, Return, ReturnOutcome, Review, other};
use crate::board::BoardError;
use crate::board::Refusal;
use crate::board::hand_over::{HandOver, settled};
use crate::board::merge::DELIVERY_CONFLICT_LIMIT;
use crate::board::reports::ReportSource;
use crate::board::ticket_comments::CommentAuthor;
use crate::board::tickets::{NewTicketInput, Ticket};
use crate::domain::validation::{
    keeps_conflicting, keeps_failing_merge_check, merge_check_failed, merge_check_failed_told,
    merged_toast,
};
use crate::domain::{Actor, AutoMergeStop, Status};
use crate::epic_lead::stuck::{FirstStop, hold_in_dev};
use crate::flow::Delivers;
use crate::pr::service::Prs;
use crate::protocol::Event;
use crate::session::deliveries::DeliveryKind;
use crate::session::{SessionKind, Start};
use crate::start::StartOutcome;

/// What one automatic Delivery, or a Delivery's conflict, did.
#[derive(Debug, Clone, PartialEq)]
pub enum Delivered {
    /// Done · "Merged automatically"; `toast` is the event clients show (UI-07).
    Merged { toast: Box<Event> },
    /// Back to the agent with restack.
    Conflict,
    /// The Merge check failed: back to the agent with restack and the comment (d0120).
    CheckFailed,
    /// Turquoise: kept conflicting, base diverged or checkout dirty.
    Stopped(AutoMergeStop),
    /// Kept conflicting for the first time with the epic's lead live: held at work for the lead (LEAD-07).
    WithLead,
    /// The PR Delivery's agent was started with its skill, no dialog.
    PrStarted,
    /// Left waiting in the Delivery for the user's click: uncommitted files in the worktree (never auto-fixed),
    /// the user stopped the Merge check, or the PR couldn't start.
    Left,
    /// Not an automatic Delivery: the ticket isn't waiting in one, or its Delivery is click mode.
    NotAutomatic,
}

/// The ticket `ticket_id` may have entered its Delivery stage: an automatic Delivery runs now (FLW-15). The merge's
/// toast is published on `events`.
pub async fn run_automatic(
    review: &Review,
    prs: &Prs,
    ticket_id: &str,
) -> Result<Delivered, BoardError> {
    let board = review.board();
    let ticket = board.ticket_by_id(ticket_id).await?;
    if ticket.status != Status::ToDeliver {
        return Ok(Delivered::NotAutomatic);
    }
    let Some(now) = board.delivery_now(&ticket).await? else {
        return Ok(Delivered::NotAutomatic);
    };
    if !now.automatic || !ticket.place.is_in(&now.stage.id) {
        return Ok(Delivered::NotAutomatic);
    }
    let key = ticket.key.to_string();
    match now.delivery.delivers {
        Delivers::Local => review.deliver_locally(&key, now.without_checks).await,
        Delivers::Pr => match prs.open_automatically(&key, now.without_checks).await {
            Ok(()) => Ok(Delivered::PrStarted),
            Err(e) => {
                // Left for the user's Open PR click, with the reason in the log (the card offers the click).
                tracing::warn!(ticket = %ticket.key, error = %e, "the automatic PR didn't start");
                Ok(Delivered::Left)
            }
        },
        Delivers::Either => Ok(Delivered::NotAutomatic),
    }
}

impl Review {
    /// The automatic local Delivery of a ticket waiting in its Delivery stage: the Local merge without the
    /// confirmation. `without_checks` records "Delivered without checks".
    pub async fn deliver_locally(
        &self,
        key: &str,
        without_checks: bool,
    ) -> Result<Delivered, BoardError> {
        let board = &self.inner.board;
        let proposal = board.merge_proposal(key).await?;
        let actor = Actor::current_user();
        let ticket = board.ticket(key).await?;
        let Merge { outcome, failed } = self.merge(key, &proposal.message, true, &actor).await?;
        if let Some(failed) = failed {
            return self.delivery_check_failed(&ticket, &failed, &actor).await;
        }
        Ok(match outcome {
            LocalMergeOutcome::Merged {
                ticket: merged,
                started,
                ..
            } => {
                board
                    .record_delivered_automatically(&merged.id, &proposal.target, without_checks)
                    .await?;
                tracing::info!(ticket = %merged.key, target = %proposal.target, "merged automatically");
                // "into the Wishlist branch" for an epic child, else the base branch.
                let into = match (&ticket.epic, proposal.into_epic) {
                    (Some(epic), true) => match board.ticket(&epic.to_string()).await {
                        Ok(epic) => format!("the {} branch", epic.title),
                        Err(_) => proposal.target.clone(),
                    },
                    _ => proposal.target.clone(),
                };
                let round = (!without_checks).then(|| {
                    ticket
                        .flags
                        .review_entry
                        .map_or(ticket.validation_round, |e| e.round)
                });
                let started: Vec<String> = started.iter().map(ToString::to_string).collect();
                let (title, message) =
                    merged_toast(&ticket.key.to_string(), &into, round, &started);
                Delivered::Merged {
                    toast: Box::new(Event::MergedAutomatically {
                        project: ticket.key.project.to_string(),
                        key: ticket.key.clone(),
                        title,
                        message,
                    }),
                }
            }
            LocalMergeOutcome::Conflict { target, .. } => {
                self.delivery_conflict(&ticket, &target, &actor).await?
            }
            LocalMergeOutcome::RefusedDiverged { message } => {
                board
                    .delivery_stopped(&ticket.id, AutoMergeStop::BaseDiverged, &message, false)
                    .await?;
                Delivered::Stopped(AutoMergeStop::BaseDiverged)
            }
            LocalMergeOutcome::RefusedDirtyCheckout { message } => {
                board
                    .delivery_stopped(&ticket.id, AutoMergeStop::CheckoutDirty, &message, false)
                    .await?;
                Delivered::Stopped(AutoMergeStop::CheckoutDirty)
            }
            LocalMergeOutcome::RefusedUncommitted { message, .. } => {
                tracing::warn!(ticket = %ticket.key, %message, "not merged automatically");
                Delivered::Left
            }
            LocalMergeOutcome::Changed { .. }
            | LocalMergeOutcome::CheckStopped { .. }
            | LocalMergeOutcome::CheckFailed { .. } => Delivered::Left,
        })
    }

    /// A Delivery's rebase conflicted with `target` (FLW-17), in either mode: the nearest earlier Agent stage's
    /// session fixes it with `restack`, no click; the third conflict in a row stops it, turquoise. An epic has no
    /// Agent stage: its conflict stays on its card for the user.
    pub(crate) async fn delivery_conflict(
        &self,
        ticket: &Ticket,
        target: &str,
        actor: &Actor,
    ) -> Result<Delivered, BoardError> {
        self.hand_back(ticket, target, None, actor).await
    }

    /// The Merge check failed on the merged result (d0120), in either mode: as a conflict ([`Self::delivery_conflict`])
    /// with the comment "Merge check `npm test` failed on the result rebased onto main (exit 1). Fix it, then submit
    /// again." and the output as its report (d0109), posted as the user's (the click or the flow is their go-ahead).
    /// An epic gets the comment on its card.
    pub(crate) async fn delivery_check_failed(
        &self,
        ticket: &Ticket,
        failed: &CheckFailure,
        actor: &Actor,
    ) -> Result<Delivered, BoardError> {
        let board = &self.inner.board;
        let body = merge_check_failed(&failed.command, &failed.target, failed.exit);
        let report = match capped_output(&failed.output) {
            Ok(path) => Some(ReportSource::Path(path)),
            Err(e) => {
                tracing::warn!(ticket = %ticket.key, error = %e, "can't read the merge check's output");
                None
            }
        };
        let author = CommentAuthor::User {
            name: actor.to_string(),
        };
        let hand_over = board.hand_over(ticket, author, &body, report).await?;
        let result = self
            .hand_back(ticket, &failed.target, hand_over.as_ref(), actor)
            .await;
        settled(board, hand_over.as_ref(), result).await
    }

    /// A conflict (`check` `None`) or a failed Merge check (`check`, its comment) goes back to the agent, or stops
    /// after the third in a row: both count toward the same limit (d0120).
    async fn hand_back(
        &self,
        ticket: &Ticket,
        target: &str,
        check: Option<&HandOver>,
        actor: &Actor,
    ) -> Result<Delivered, BoardError> {
        let board = &self.inner.board;
        if ticket.is_epic() {
            if let Some(comment) = check {
                post_now(board, ticket, comment, actor).await?;
            }
            return Ok(Delivered::Left);
        }
        let ticket = board.ticket_by_id(&ticket.id).await?;
        if ticket.auto_merge_conflicts + 1 < DELIVERY_CONFLICT_LIMIT {
            self.hand_conflict_to_agent(&ticket, target, check, actor)
                .await?;
            return Ok(match check {
                Some(_) => Delivered::CheckFailed,
                None => Delivered::Conflict,
            });
        }
        if let Some(comment) = check {
            post_now(board, &ticket, comment, actor).await?;
        }
        let (stop, text, first) = match check {
            Some(_) => (
                AutoMergeStop::KeepsFailingMergeCheck,
                keeps_failing_merge_check(target),
                FirstStop::keeps_failing_merge_check(target),
            ),
            None => (
                AutoMergeStop::KeepsConflicting,
                keeps_conflicting(target),
                FirstStop::keeps_conflicting(target),
            ),
        };
        // An epic child's first "Keeps conflicting" goes to its live lead once (LEAD-07): held at work, the conflict
        // counted.
        let held = board.leads().takes_stop(&ticket).await;
        if held {
            hold_in_dev(board, &ticket.id, true).await?;
            if board.leads().child_stuck(&ticket, &first).await {
                return Ok(Delivered::WithLead);
            }
        }
        board
            .delivery_stopped(&ticket.id, stop, &text, !held)
            .await?;
        Ok(Delivered::Stopped(stop))
    }

    /// Back to the agent with restack, no click: queued for its input line like any automatic message, or the
    /// conversation resumed with it. A failed Merge check's comment (`check`) is posted with the move, and a live
    /// session is told where to read it.
    async fn hand_conflict_to_agent(
        &self,
        ticket: &Ticket,
        target: &str,
        check: Option<&HandOver>,
        actor: &Actor,
    ) -> Result<(), BoardError> {
        let inner = &self.inner;
        let action = Return::Fix {
            target,
            hand_over: check,
        };
        let skill = action.skill();
        let full = self.limit_full().await?;
        let live = inner.sessions.running_of(&ticket.id, SessionKind::Agent);
        match (&live, full) {
            (Some(session), false) => {
                // A build freed at Review comes back first (M19-c); a queued or resumed ticket gets it at its start.
                let _held = inner
                    .ticket_env
                    .fetch_for_agent(ticket, action.fetch_why(), actor)
                    .await;
                self.record(ticket, &action, Status::Working, None, actor)
                    .await?;
                let worktree = ticket
                    .work
                    .as_ref()
                    .and_then(|w| w.worktree.as_deref())
                    .map(std::path::Path::new);
                let told = check.map(|_| merge_check_failed_told(&ticket.key, target));
                let text = inner
                    .deliveries
                    .skill_line(&inner.board, &session.id, worktree, skill, told.as_deref())
                    .await;
                inner
                    .deliveries
                    .queue_delivery(&session.id, DeliveryKind::Skill, &text)
                    .await
                    .map_err(other)?;
            }
            (_, true) => {
                self.record(ticket, &action, Status::Queued, Some(skill), actor)
                    .await?;
                inner.scheduler.wake();
            }
            (None, false) => {
                self.record(ticket, &action, Status::Queued, Some(skill), actor)
                    .await?;
                match inner
                    .starter
                    .start(&ticket.key.to_string(), actor, Start::Auto)
                    .await?
                {
                    StartOutcome::Started(_) | StartOutcome::Skipped | StartOutcome::Kept => {}
                    StartOutcome::Held { message } | StartOutcome::Interrupted { message, .. } => {
                        tracing::warn!(ticket = %ticket.key, %message, "resuming the agent stopped");
                    }
                }
            }
        }
        inner.board.count_delivery_conflict(&ticket.id).await?;
        tracing::info!(ticket = %ticket.key, %target, merge_check = check.is_some(), "conflict handed to the agent");
        Ok(())
    }

    /// **Send back with PR comments** (FLW-16): a ticket whose PR is open goes back to its nearest earlier Agent
    /// stage with `address-review`; the optional `note` (with `report`, the dialog's pasted text) is the user's comment
    /// (CMT-05). `clear_first` is the user's "Clear it and send" after a "needs confirmation" answer. An epic adds a
    /// child instead ([`add_child_from_pr`]).
    pub async fn send_back_pr(
        &self,
        key: &str,
        note: &str,
        report: Option<crate::board::reports::ReportSource>,
        clear_first: bool,
        actor: &Actor,
    ) -> Result<ReturnOutcome, BoardError> {
        let ticket = self.inner.board.ticket(key).await?;
        if ticket.is_epic() {
            return Err(BoardError::refused(
                Refusal::MoveRefused,
                format!(
                    "{} is an epic: add a child from the PR comments instead",
                    ticket.key
                ),
            ));
        }
        if ticket.status != Status::PrOpen {
            return Err(BoardError::refused(
                Refusal::MoveRefused,
                format!(
                    "{} is {}; Send back with PR comments works on a ticket whose PR is open",
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
        let action = Return::PrComments {
            hand_over: hand_over.as_ref(),
        };
        self.return_to_agent(&ticket, action, clear_first, actor)
            .await
    }
}

/// Posts a hand-over comment on its own: a failed Merge check's on an epic, or on the stop that ends the retries.
async fn post_now(
    board: &crate::board::Board,
    ticket: &Ticket,
    comment: &HandOver,
    actor: &Actor,
) -> Result<(), BoardError> {
    let mut tx = board.store().begin_write().await?;
    comment.post(&mut tx, &ticket.id, actor).await?;
    tx.commit().await?;
    Ok(())
}

/// The title of the child that holds an epic's unresolved PR comments.
pub fn pr_child_title(number: u32) -> String {
    format!("Address the comments on PR #{number}")
}

/// **Add a child from PR comments** (FLW-16, d0108 second gap review rule 5): an epic whose PR is open gets a Backlog
/// child holding the PR's unresolved comments, fetched now through `gh` / `az` on `path`. The user starts it like any
/// child; once it merges, the epic is reviewed and pushes to the same PR.
pub async fn add_child_from_pr(
    board: &crate::board::Board,
    key: &str,
    path: &OsString,
    actor: &Actor,
) -> Result<Ticket, BoardError> {
    let epic = board.ticket(key).await?;
    if !epic.is_epic() || epic.status != Status::PrOpen {
        return Err(BoardError::refused(
            Refusal::MoveRefused,
            format!(
                "{} isn't an epic whose PR is open; Add a child from PR comments works on one",
                epic.key
            ),
        ));
    }
    let feedback = board.pr_feedback(key, path).await?.ok_or_else(|| {
        BoardError::refused(Refusal::MoveRefused, format!("{} has no PR", epic.key))
    })?;
    let mut body = format!(
        "The unresolved comments on PR #{} of {} ({}):\n",
        feedback.number, epic.key, epic.title
    );
    if feedback.comments.is_empty() {
        body.push_str("\nNone were found.\n");
    }
    for comment in &feedback.comments {
        let place = match (&comment.file, comment.line) {
            (Some(file), Some(line)) => format!("`{file}:{line}` "),
            (Some(file), None) => format!("`{file}` "),
            _ => String::new(),
        };
        body.push_str(&format!(
            "\n- {place}{}: {}",
            comment.author,
            comment.text.trim()
        ));
    }
    if let Some(note) = &feedback.note {
        body.push_str(&format!("\n\n{note}"));
    }
    board
        .create_ticket(
            &NewTicketInput {
                project: epic.key.project.to_string(),
                title: pr_child_title(feedback.number),
                body,
                epic: Some(epic.key.to_string()),
                ..Default::default()
            },
            actor,
        )
        .await
}
