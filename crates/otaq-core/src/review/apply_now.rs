//! **Apply now** carried out (APN-04; the plan and the writes are [`crate::flow::apply_now`]'s): the fresh plan is
//! checked against the window's stamp and picks, every move is decided (the checks that can refuse run first), the
//! flow and the moves are written in one transaction, and only then do the sessions in removed stages stop, the
//! agents picked back get Send back's skill and starts resume.

use std::collections::HashMap;

use super::finish::FinishedTicket;
use super::{ADDRESS_REVIEW_SKILL, Review};
use crate::board::BoardError;
use crate::board::tickets::Ticket;
use crate::domain::{Actor, HandOffKind, Status};
use crate::flow::apply_now::{
    ApplyNowAnswer, ApplyNowParams, ApplyNowPlan, Live, MoveWrite, PlannedWrite, Refreshed,
};
use crate::session::SessionKind;
use crate::session::deliveries::Delivery;
use crate::session::hand_offs::HandOff;
use crate::validation::Validation;

impl Review {
    /// The sessions running now, by ticket: agents and checkers.
    fn live(&self, tickets: &[Ticket]) -> Live {
        let sessions = &self.inner.sessions;
        let mut live = Live::default();
        for ticket in tickets {
            if sessions
                .running_of(&ticket.id, SessionKind::Agent)
                .is_some()
            {
                live.agents.insert(ticket.id.clone());
            }
            if sessions
                .running_of(&ticket.id, SessionKind::Validator)
                .is_some()
            {
                live.checkers.insert(ticket.id.clone());
            }
        }
        live
    }

    /// `flow.apply_now_plan`: what Apply now would do on `project` now; `None` when no change waits.
    pub async fn apply_now_plan(&self, project: &str) -> Result<Option<ApplyNowPlan>, BoardError> {
        let board = &self.inner.board;
        let key = board.project(project).await?.key;
        let live = self.live(&board.tickets(&key).await?);
        board.apply_now_plan(&key, &live).await
    }

    /// **Apply now** (`flow.apply_now`, APN-04): refused with the fresh plan when the board changed since the window's
    /// plan or a pick isn't one of its ticket's choices; "already applied" when the change applied on its own.
    pub async fn apply_now(
        &self,
        params: &ApplyNowParams,
        validation: &Validation,
        actor: &Actor,
    ) -> Result<ApplyNowAnswer, BoardError> {
        let inner = &self.inner;
        let board = &inner.board;
        let key = board.project(&params.project).await?.key;
        // Apply now starts agents on its own: refused while the project is paused (PAU-04).
        crate::agents::pause::refuse_while_paused(board.store(), &key).await?;
        let Some(plan) = self.apply_now_plan(&key).await? else {
            return Ok(match board.applied_since(&key, &params.stamp).await? {
                Some(version) => ApplyNowAnswer::AlreadyApplied {
                    version: Box::new(version),
                },
                None => ApplyNowAnswer::NothingWaits,
            });
        };
        let refreshed = |plan: ApplyNowPlan, why: Refreshed| {
            Ok(ApplyNowAnswer::Refreshed {
                plan: Box::new(plan),
                why,
            })
        };
        if plan.stamp != params.stamp {
            return refreshed(plan, Refreshed::BoardChanged);
        }
        // A pick back to the Agent stage whose session is live types Send back's skill into it, unless the user has
        // text waiting there (d0048): then it stops and resumes.
        let mut kept: HashMap<String, String> = HashMap::new();
        for planned in &plan.moves {
            let ticket = board.ticket(&planned.ticket.to_string()).await?;
            if let Some(session) = inner.sessions.running_of(&ticket.id, SessionKind::Agent)
                && let Some(stage) = self.session_stage(&session.id).await?
                && !inner.sessions.has_unsent_input(&session.id).unwrap_or(true)
            {
                kept.insert(ticket.id, stage);
            }
        }
        let keeps = |ticket: &Ticket, stage: &str| kept.get(&ticket.id).is_some_and(|s| s == stage);
        let writes = match board.apply_now_writes(&plan, &params.picks, &keeps).await? {
            Ok(writes) => writes,
            Err(ticket) => return refreshed(plan, Refreshed::NotAChoice { ticket }),
        };
        let yaml = board
            .waiting_change(&key)
            .await?
            .map(|w| w.yaml)
            .unwrap_or_default();
        let Some(version) = board.write_apply_now(&key, &yaml, &writes, actor).await? else {
            // The board moved between the check and the write: nothing was written.
            return match self.apply_now_plan(&key).await? {
                Some(plan) => refreshed(plan, Refreshed::BoardChanged),
                None => Ok(match board.applied_since(&key, &params.stamp).await? {
                    Some(version) => ApplyNowAnswer::AlreadyApplied {
                        version: Box::new(version),
                    },
                    None => ApplyNowAnswer::NothingWaits,
                }),
            };
        };

        // Written: the sessions in removed stages stop now (ended by Otaq, their tickets stay where Apply now put
        // them), then each move's own start.
        let mut stopped = Vec::new();
        for planned in &writes {
            if self.stop_for_apply_now(planned).await {
                stopped.push(planned.ticket.key.clone());
            }
        }
        for PlannedWrite { ticket, write, .. } in &writes {
            self.after_apply_now(ticket, write, validation).await;
        }
        inner.scheduler.wake();
        let mut moved: Vec<_> = writes.iter().map(|w| w.ticket.key.clone()).collect();
        moved.sort();
        stopped.sort();
        tracing::info!(project = %key, moved = moved.len(), stopped = stopped.len(), "applied now");
        Ok(ApplyNowAnswer::Applied {
            version: Box::new(version),
            moved,
            stopped,
        })
    }

    /// Stops what ran for `ticket` where it was: its checker, and its agent unless the agent stays (an agent in a
    /// surviving earlier stage, as a forward drag keeps it, or the session a pick back types into). Whether one
    /// stopped.
    async fn stop_for_apply_now(&self, planned: &PlannedWrite) -> bool {
        let ticket = &planned.ticket;
        let inner = &self.inner;
        let board = &inner.board;
        let mut stopped = false;
        if let Some(checker) = inner
            .sessions
            .running_of(&ticket.id, SessionKind::Validator)
        {
            if let Err(e) = inner.sessions.stop(&checker.id).await {
                tracing::warn!(ticket = %ticket.key, error = %e, "can't stop the checker for Apply now");
            }
            stopped = true;
            if ticket.status == Status::WaitingForLimitReset
                && let Err(e) = board
                    .store()
                    .clear_agent_hold(
                        crate::agents::store::HoldScope::Ticket(&ticket.id),
                        crate::agents::SessionRole::Validator,
                    )
                    .await
            {
                tracing::warn!(ticket = %ticket.key, error = %e, "can't clear the checker's hold");
            }
        }
        let Some(agent) = inner.sessions.running_of(&ticket.id, SessionKind::Agent) else {
            return stopped;
        };
        let stops = planned.stops_agent();
        if stops && let Err(e) = inner.sessions.stop(&agent.id).await {
            tracing::warn!(ticket = %ticket.key, error = %e, "can't stop the agent for Apply now");
        }
        stopped || (stops && planned.from_agent_stage)
    }

    /// What follows a move once written: a Park frees the ticket's ports, an agent picked back gets Send back's skill
    /// in its live session, a check entered starts its checker, a Delivery entered may deliver by itself; anything
    /// waiting starts through the scheduler.
    async fn after_apply_now(&self, ticket: &Ticket, write: &MoveWrite, validation: &Validation) {
        let inner = &self.inner;
        let board = &inner.board;
        match write {
            MoveWrite::Park { .. } => {
                let parked = FinishedTicket {
                    id: ticket.id.clone(),
                    key: ticket.key.clone(),
                    status: Status::Parked,
                };
                let hooks = inner.parked_hooks.lock().expect("parked hooks").clone();
                for hook in hooks {
                    hook(&parked).await;
                }
                if let Err(e) = inner.ports.free(&ticket.id).await {
                    tracing::warn!(ticket = %ticket.key, error = %e, "can't free the parked ticket's ports");
                }
            }
            MoveWrite::SendBack {
                to: Status::Working,
                ..
            } => {
                let Some(session) = inner.sessions.running_of(&ticket.id, SessionKind::Agent)
                else {
                    return;
                };
                let worktree = ticket
                    .work
                    .as_ref()
                    .and_then(|w| w.worktree.as_deref())
                    .map(std::path::Path::new);
                let text = inner
                    .deliveries
                    .skill_line(board, &session.id, worktree, ADDRESS_REVIEW_SKILL, None)
                    .await;
                match inner
                    .deliveries
                    .hand_off(
                        &session.id,
                        HandOff::new(HandOffKind::SendBack, text),
                        false,
                    )
                    .await
                {
                    Ok(Delivery::Sent { .. }) => {}
                    Ok(Delivery::NeedsConfirmation { .. }) => tracing::warn!(
                        ticket = %ticket.key,
                        "unsent text appeared before Send back's skill was typed"
                    ),
                    Err(e) => {
                        tracing::warn!(ticket = %ticket.key, error = %e, "can't type Send back's skill")
                    }
                }
            }
            MoveWrite::SendBack { .. } => {}
            MoveWrite::Drag(drag) => {
                // Into an agent check: its round is open, a checker starts.
                if drag.check_head.is_some() {
                    if let Err(e) = validation.start_dragged_check(&ticket.id).await {
                        tracing::warn!(ticket = %ticket.key, error = %e, "the check didn't start");
                    }
                } else if drag.moved.status == Status::ToDeliver {
                    board.delivery().arrived(&ticket.id).await;
                }
            }
        }
    }
}
