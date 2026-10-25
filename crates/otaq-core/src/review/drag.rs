//! Drags to any stage (FLW-18; d0108 "Dragging a card back to any earlier stage", "Other drags", second gap review
//! rule 2; d0052, d0059): the drops that stop or start sessions, carried out with the rules of
//! [`crate::board::moves::check_with`]. The others (Start, Unqueue, Reopen, PR rework) are the board's
//! ([`Board::move_ticket`]).
//!
//! - **Back to an earlier Agent stage** (after the dialog): the user's instructions are their comment (CMT-05), posted
//!   before that stage's agent goes on. Its own session resumes (typed into it when it is live, as Send back does),
//!   or, when the stage never ran (the ticket skips it by its choice: "Grill · run by you"), a fresh session starts
//!   there; either is told "SHOP-19 was sent back to Dev by you with a comment. Run `otaq prime`.", never the text. An
//!   agent working where the card is now stops first. It counts as a human Fail: every agent check's counter
//!   restarts, and the card then walks every later stage again.
//! - **Back to an agent check** (after the dialog): the check runs again, a fresh round; the user's note is their
//!   comment.
//! - **Back onto a human check**: no dialog; the card waits there, "Moved back by you".
//! - **Forward**: the very next stage is Move to next stage (a human check's Pass); further, the stages between are
//!   marked skipped by you ("Skipped Security · by you") and the Delivery then waits for a click.

use serde_json::json;

use super::{Return, ReturnOutcome, Review};
use crate::board::hand_over::{HandOver, settled};
use crate::board::moves::{DragWrite, DropOn, MoveAction, MoveCheck};
use crate::board::reports::ReportSource;
use crate::board::send_back::{SentBackTo, Tell, waits_before};
use crate::board::tickets::{EventKind, Ticket};
use crate::board::{Board, BoardError, Refusal};
use crate::domain::{Actor, MovedPast, Status};
use crate::flow::engine::{RunResult, Target, skipped_by_drag};
use crate::flow::{Flow, Stage};
use crate::session::{SessionKind, Start};
use crate::start::StartOutcome;
use crate::validation::Validation;

/// A drag back to an earlier Agent stage, as [`Review::return_to_agent`] records it.
pub(crate) struct Back<'a> {
    /// The user's instructions as their comment, if they wrote any.
    pub(super) hand_over: Option<&'a HandOver>,
    pub(super) stage: &'a Stage,
    /// How the run of the stage it leaves ends: failed from a human check (its Fail), cancelled elsewhere.
    ended: RunResult,
    event: serde_json::Value,
    flow: &'a Flow,
}

impl Back<'_> {
    pub(super) async fn record(
        &self,
        board: &Board,
        ticket: &Ticket,
        to: Status,
        skill: Option<&str>,
        actor: &Actor,
    ) -> Result<Option<String>, BoardError> {
        // Queued (the limit is full), it waits before the stage: Ready for Dev before the flow's first.
        let stage = match to.is_queued() {
            true => waits_before(self.flow, &self.stage.id),
            false => Some(self.stage.id.clone()),
        };
        board
            .send_back_to(
                ticket,
                SentBackTo {
                    hand_over: self.hand_over,
                    to,
                    stage: stage.as_deref(),
                    resume_skill: skill,
                    ended: self.ended,
                    event: Some((EventKind::Dragged, self.event.clone())),
                    tell: Some(Tell {
                        stage: &self.stage.name,
                        by_you: true,
                        pr_comments: false,
                        from: None,
                    }),
                    fresh: false,
                },
                actor,
            )
            .await
            .map(|sent| sent.told)
    }
}

/// Which agent sessions stop before a drag moves the card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stop<'a> {
    /// A drag back to an Agent stage: every agent session but that stage's own (if it is the live one) stops, so the
    /// instructions reach the right agent.
    AllBut(Option<&'a str>),
    /// Back onto a check: the agent of the Agent stage the card is in stops.
    WorkingHere,
    /// Forward: an Agent stage's session stays (as after Move to next stage); only a checker stops.
    Forward,
}

/// What a `ticket.drag` asks for besides the target: the dialog's text and its answers.
#[derive(Debug, Clone, Copy, Default)]
pub struct DragAnswers<'a> {
    /// The instructions (back to an Agent stage) or the note (back to an agent check): the user's comment (CMT-05).
    pub note: &'a str,
    /// A report attached to that comment (the dialog's pasted text).
    pub report: Option<&'a ReportSource>,
    /// The dialog was confirmed.
    pub confirmed: bool,
    /// "Clear it and send" after the unsent-text question (d0048).
    pub clear_first: bool,
}

impl Review {
    /// **A drop on any stage** (FLW-18): decided by the flow's drag rules for the ticket, then carried out. A drop that
    /// needs the dialog is refused with [`Refusal::NeedsConfirmation`] until `answers.confirmed`. Answers with the
    /// ticket afterwards, or (a drag back to an Agent stage whose live session has unsent text) the question first.
    pub async fn drag(
        &self,
        key: &str,
        target: &DropOn,
        answers: DragAnswers<'_>,
        validation: &Validation,
        actor: &Actor,
    ) -> Result<ReturnOutcome, BoardError> {
        let board = self.board();
        let ticket = board.ticket(key).await?;
        // While a flow change waits for a span, a drop past its edge stops there (FED-08).
        let target = &board.span_edge(&ticket, target).await?;
        let check = board.check_drag(&ticket, target).await?;
        let action = match &check {
            MoveCheck::Unchanged => return done(board, key).await,
            MoveCheck::Refused { reason } => {
                return Err(BoardError::refused(Refusal::MoveRefused, reason.clone()));
            }
            MoveCheck::Confirm { .. } | MoveCheck::Choose { .. } if !answers.confirmed => {
                return Err(BoardError::refused(
                    Refusal::NeedsConfirmation,
                    "This move needs your confirmation",
                ));
            }
            _ => check.action().expect("allowed moves have an action"),
        };
        let flow = board.effective_flow(ticket.key.project.as_str()).await?;
        let to = match target.in_flow(&flow) {
            Some(Target::Stage(stage)) => Some(stage),
            _ => None,
        };
        match (action, to) {
            (MoveAction::SendBack, Some(to)) => {
                self.send_back_by_drag(&ticket, &flow, to, answers, validation, actor)
                    .await
            }
            (MoveAction::RunCheckAgain, Some(to)) => {
                // The note is checked and its report copied, then uncommitted changes refused, before anything stops.
                let hand_over = board
                    .user_hand_over(&ticket, answers.note, answers.report.cloned(), actor)
                    .await?;
                let ran = async {
                    let head = board.submit_head(&ticket).await?;
                    self.stop_where_it_is(&ticket, &flow, Stop::WorkingHere, validation)
                        .await?;
                    let event = event_of(&flow, &ticket, to, true, &[], false, answers.note);
                    let moved = board.drag_moved(&ticket, &to.id).await?;
                    board
                        .record_drag(
                            &ticket,
                            DragWrite {
                                moved,
                                reset: true,
                                marks: Vec::new(),
                                check_head: Some(head),
                                hand_over: hand_over.clone().map(Box::new),
                                event,
                            },
                            actor,
                        )
                        .await
                }
                .await;
                settled(board, hand_over.as_ref(), ran).await?;
                validation.start_dragged_check(&ticket.id).await?;
                done(board, key).await
            }
            (MoveAction::MoveBack, Some(to)) => {
                self.stop_where_it_is(&ticket, &flow, Stop::WorkingHere, validation)
                    .await?;
                let event = event_of(&flow, &ticket, to, true, &[], false, "");
                let moved = board.drag_moved(&ticket, &to.id).await?;
                board
                    .record_drag(
                        &ticket,
                        DragWrite {
                            moved,
                            reset: true,
                            marks: Vec::new(),
                            check_head: None,
                            hand_over: None,
                            event,
                        },
                        actor,
                    )
                    .await?;
                done(board, key).await
            }
            (MoveAction::NextStage, Some(_))
                if here(&flow, &ticket).is_some_and(Stage::is_agent_stage) =>
            {
                validation.submit_by_user(key, "", actor).await?;
                done(board, key).await
            }
            (MoveAction::NextStage, Some(_)) => {
                self.pass(key, validation, actor).await?;
                done(board, key).await
            }
            (MoveAction::SkipTo, Some(to)) => {
                self.skip_to(&ticket, &flow, to, validation, actor).await?;
                done(board, key).await
            }
            _ => {
                board
                    .move_ticket(key, target.clone(), answers.confirmed, actor)
                    .await?;
                done(board, key).await
            }
        }
    }

    /// Back to the earlier Agent stage `to` with the user's instructions as their comment (refused whole when too
    /// long or its report is missing, before anything stops).
    async fn send_back_by_drag(
        &self,
        ticket: &Ticket,
        flow: &Flow,
        to: &Stage,
        answers: DragAnswers<'_>,
        validation: &Validation,
        actor: &Actor,
    ) -> Result<ReturnOutcome, BoardError> {
        let board = &self.inner.board;
        let hand_over = board
            .user_hand_over(ticket, answers.note, answers.report.cloned(), actor)
            .await?;
        let outcome = self
            .send_back_with(
                ticket,
                flow,
                to,
                answers,
                hand_over.as_ref(),
                validation,
                actor,
            )
            .await;
        if matches!(outcome, Ok(ReturnOutcome::NeedsConfirmation { .. }))
            && let Some(hand_over) = &hand_over
        {
            hand_over.discard_unposted(board).await;
        }
        settled(board, hand_over.as_ref(), outcome).await
    }

    #[allow(clippy::too_many_arguments)]
    async fn send_back_with(
        &self,
        ticket: &Ticket,
        flow: &Flow,
        to: &Stage,
        answers: DragAnswers<'_>,
        hand_over: Option<&HandOver>,
        validation: &Validation,
        actor: &Actor,
    ) -> Result<ReturnOutcome, BoardError> {
        let inner = &self.inner;
        let board = &inner.board;
        let ran = board.stage_ran(&ticket.id, &to.id).await?;
        let skips = board.ticket_skips(ticket, flow).await?;
        let run_by_you = skips.contains(&to.id);
        // Its own session, if it is the live one, gets the instructions typed: unsent text in it asks first, before
        // anything stops (d0048).
        let live = inner.sessions.running_of(&ticket.id, SessionKind::Agent);
        let keep = match &live {
            Some(session)
                if ran
                    && self
                        .session_stage(&session.id)
                        .await?
                        .is_none_or(|s| s == to.id) =>
            {
                Some(session.id.clone())
            }
            _ => None,
        };
        if let Some(id) = &keep
            && !answers.clear_first
            && inner.sessions.has_unsent_input(id).map_err(super::other)?
        {
            return Ok(ReturnOutcome::NeedsConfirmation {
                ticket: ticket.key.clone(),
                unclearable_by: inner.deliveries.unclearable_by(id).map_err(super::other)?,
            });
        }
        let in_human_check = here(flow, ticket).is_some_and(Stage::is_human_check);
        let ended = if in_human_check {
            RunResult::Failed
        } else {
            RunResult::Cancelled
        };
        self.stop_where_it_is(ticket, flow, Stop::AllBut(keep.as_deref()), validation)
            .await?;
        let ticket = board.ticket_by_id(&ticket.id).await?;
        let event = event_of(flow, &ticket, to, true, &[], run_by_you, answers.note);
        if ran {
            let back = Back {
                hand_over,
                stage: to,
                ended,
                event,
                flow,
            };
            return self
                .return_with(&ticket, &Return::DragBack(back), answers.clear_first, actor)
                .await;
        }
        // The stage never ran: a fresh session there, its skill followed by what came back (FLW-18).
        board
            .send_back_to(
                &ticket,
                SentBackTo {
                    hand_over,
                    to: Status::Queued,
                    stage: Some(&to.id),
                    resume_skill: None,
                    ended,
                    event: Some((EventKind::Dragged, event)),
                    tell: Some(Tell {
                        stage: &to.name,
                        by_you: true,
                        pr_comments: false,
                        from: None,
                    }),
                    fresh: true,
                },
                actor,
            )
            .await?;
        if self.limit_full().await? {
            inner.scheduler.wake();
        } else {
            match inner
                .starter
                .start(&ticket.key.to_string(), actor, Start::Auto)
                .await?
            {
                StartOutcome::Started(_) | StartOutcome::Skipped | StartOutcome::Kept => {}
                StartOutcome::Held { message } | StartOutcome::Interrupted { message, .. } => {
                    tracing::warn!(ticket = %ticket.key, %message, "the stage dragged back to didn't start");
                }
            }
        }
        done(board, &ticket.key.to_string()).await
    }

    /// Forward to `to` past the stages between: each is marked skipped by you, and the stage the card leaves too when
    /// it is a check (failed at its limit: "Failed Security 3 times · moved by you").
    async fn skip_to(
        &self,
        ticket: &Ticket,
        flow: &Flow,
        to: &Stage,
        validation: &Validation,
        actor: &Actor,
    ) -> Result<(), BoardError> {
        let board = &self.inner.board;
        let skips = board.ticket_skips(ticket, flow).await?;
        let current = here(flow, ticket);
        let from = current.and_then(|s| flow.position(&s.id)).unwrap_or(0);
        let target = flow.position(&to.id).unwrap_or(flow.stages.len());
        let leaves_check = current.is_some_and(|s| !s.is_agent_stage());
        let skipped = skipped_by_drag(flow, &skips, from, target, leaves_check);
        let marks: Vec<(String, MovedPast)> = skipped
            .iter()
            .map(|s| {
                let how = match ticket.status {
                    Status::CheckFailed if Some(s.id.as_str()) == ticket.place.stage() => {
                        MovedPast::Failed
                    }
                    _ => MovedPast::Skipped,
                };
                (s.id.clone(), how)
            })
            .collect();
        let names: Vec<&str> = skipped.iter().map(|s| s.name.as_str()).collect();
        let event = event_of(flow, ticket, to, false, &names, false, "");
        let check_head = match to.is_agent_check() {
            true => Some(board.submit_head(ticket).await?),
            false => None,
        };
        self.stop_where_it_is(ticket, flow, Stop::Forward, validation)
            .await?;
        let moved = board.drag_moved(ticket, &to.id).await?;
        let waits = moved.place.is_waiting();
        let after = board
            .record_drag(
                ticket,
                DragWrite {
                    moved,
                    reset: false,
                    marks,
                    check_head: check_head.clone(),
                    hand_over: None,
                    event,
                },
                actor,
            )
            .await?;
        if check_head.is_some() {
            validation.start_dragged_check(&after.id).await?;
        } else if waits {
            self.inner.scheduler.wake();
        } else {
            // Into a Delivery: it waits for the click, as for any card moved past a check (FLW-15).
            board.delivery().arrived(&after.id).await;
        }
        Ok(())
    }

    /// Stops what works where the card is before a drag moves it: its checker in an agent check (the round ends
    /// stopped, or skipped for a forward drag), and, as `stop` says, agent sessions.
    async fn stop_where_it_is(
        &self,
        ticket: &Ticket,
        flow: &Flow,
        stop: Stop<'_>,
        validation: &Validation,
    ) -> Result<(), BoardError> {
        let inner = &self.inner;
        let Some(current) = here(flow, ticket) else {
            return Ok(());
        };
        if current.is_agent_check() {
            validation
                .stop_for_drag(ticket, stop == Stop::Forward)
                .await?;
        }
        let Some(session) = inner.sessions.running_of(&ticket.id, SessionKind::Agent) else {
            return Ok(());
        };
        let stops = match stop {
            Stop::AllBut(keep) => keep != Some(session.id.as_str()),
            Stop::WorkingHere => current.is_agent_stage(),
            Stop::Forward => false,
        };
        if stops && let Err(e) = inner.sessions.stop(&session.id).await {
            tracing::warn!(ticket = %ticket.key, error = %e, "can't stop the agent before the drag");
        }
        // The ticket is one (d0133 audit 5): its helpers stop with the agent in charge, worktrees kept.
        if stops {
            let stopped = super::finish::FinishedTicket {
                id: ticket.id.clone(),
                key: ticket.key.clone(),
                status: ticket.status,
            };
            let hooks = inner.drag_stopped_hooks.lock().expect("drag hooks").clone();
            for hook in hooks {
                hook(&stopped).await;
            }
        }
        Ok(())
    }

    /// Registers a hook run when a drag stopped the ticket's agent where the card was (its helpers stop too).
    pub fn on_agent_stopped_by_drag(&self, hook: super::finish::FinishedHook) {
        self.inner
            .drag_stopped_hooks
            .lock()
            .expect("drag hooks")
            .push(hook);
    }

    /// The Agent stage a session ran, if it ran one ([`crate::session::own_stage`]).
    pub(super) async fn session_stage(&self, session: &str) -> Result<Option<String>, BoardError> {
        Ok(crate::session::own_stage(self.inner.board.store().pool(), session).await?)
    }
}

impl Board {
    /// Whether the ticket ran the Agent stage `stage` in a session of its own (its session can resume).
    pub(crate) async fn stage_ran(&self, ticket_id: &str, stage: &str) -> Result<bool, BoardError> {
        let ran: Option<i64> = sqlx::query_scalar(
            "SELECT 1 FROM stage_runs WHERE ticket_id = ? AND stage_id = ? AND kind = 'agent'
             AND session_id IS NOT NULL LIMIT 1",
        )
        .bind(ticket_id)
        .bind(stage)
        .fetch_optional(self.store().pool())
        .await?;
        Ok(ran.is_some())
    }
}

/// The stage the ticket is in.
fn here<'f>(flow: &'f Flow, ticket: &Ticket) -> Option<&'f Stage> {
    ticket.place.stage().and_then(|id| flow.stage(id))
}

/// History's [`EventKind::Dragged`] payload.
fn event_of(
    flow: &Flow,
    ticket: &Ticket,
    to: &Stage,
    back: bool,
    skipped: &[&str],
    run_by_you: bool,
    note: &str,
) -> serde_json::Value {
    let from = ticket
        .place
        .stage_id()
        .map(|id| flow.stage(id).map_or(id, |s| s.name.as_str()));
    let note = note.trim();
    json!({
        "stage": to.name,
        "stage_id": to.id,
        "from": from,
        "back": back,
        "skipped": skipped,
        "run_by_you": run_by_you,
        "note": (!note.is_empty()).then_some(note),
    })
}

async fn done(board: &Board, key: &str) -> Result<ReturnOutcome, BoardError> {
    Ok(ReturnOutcome::Done {
        ticket: Box::new(board.ticket(key).await?),
    })
}
