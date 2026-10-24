//! Resume, Resume all, Retry and Start from scratch (REC-01, REC-03; d0018, d0045): the user's clicks that bring
//! an Interrupted or Resume failed ticket's agent back. **Nothing here runs on its own.**
//!
//! - **Resume** reopens the session of the stage the card is in (STG-09, d0108): in an Agent stage it continues
//!   that stage's stored conversation on the tool and model it ran on (its adapter's resume), in a new session in
//!   the same worktree; in an agent check it starts a fresh check ([`crate::recovery::resume_as`]).
//!   It goes through the start sequence like a parked ticket (worktree kept, no base sync or setup); with the
//!   agent limit full, the ticket waits in Queued for a slot like a Send back. Its first prompt is what was
//!   waiting (a Send back's skill, an inbox answer), else "You were interrupted by a restart. Carry on…"
//!   ([`crate::recovery::carry_on_text`], d0122), passed when the tool starts.
//! - **Resume all** resumes exactly the Interrupted Dev tickets ([`crate::recovery::resume_all_eligible`]).
//! - When the tool can't find the conversation to resume the ticket becomes **Resume failed**
//!   ([`crate::recovery::Recovery`]); **Retry** runs the same resume again, **Start from scratch** (after a
//!   confirmation) starts a new conversation in the same worktree. Branch, commits and files are kept; every
//!   session and its tool id stay in History.
//! - **While the project is paused** (PAU-05, d0112) Resume, an inbox answer, Retry and Start from scratch are the
//!   user's click on this one ticket: its agent starts and the pause stays. Resume all leaves paused projects alone
//!   (their Resume is the pause's).

use serde_json::json;

use super::Review;
use crate::board::tickets::{EventKind, Ticket, record_event};
use crate::board::{BoardError, Refusal};
use crate::domain::{Actor, Status, TicketKey};
use crate::recovery::{RETURN_RESUME, RETURN_SCRATCH, ResumeAs, resume_all_eligible, resume_as};
use crate::session::{SessionKind, Start};
use crate::start::StartOutcome;
use crate::store::now_ms;
use crate::store::tickets::set_state_since;

/// "Start SHOP-25 from scratch? …": what Start from scratch keeps and what it doesn't.
pub fn start_from_scratch_message(key: &str) -> String {
    format!(
        "Start {key} from scratch? A new Claude session starts in the same worktree: the branch, commits and \
         files are kept, but the old conversation isn't continued."
    )
}

/// Resumes a ticket interrupted while it validated (by its id), started as `Start` says.
pub type ValidationResume = std::sync::Arc<
    dyn Fn(
            String,
            Start,
        )
            -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), BoardError>> + Send>>
        + Send
        + Sync,
>;

impl Review {
    /// **Resume** an Interrupted ticket.
    pub async fn resume(&self, key: &str, actor: &Actor) -> Result<Ticket, BoardError> {
        let ticket = self.inner.board.ticket(key).await?;
        if ticket.status != Status::Interrupted {
            return Err(BoardError::refused(
                Refusal::MoveRefused,
                format!(
                    "{} is {}; only Interrupted tickets are resumed",
                    ticket.key,
                    ticket.status.label()
                ),
            ));
        }
        // It was in an agent check: a fresh check, not counted; the agent's conversation isn't touched (VAL-07).
        let flow = self
            .inner
            .board
            .effective_flow(ticket.key.project.as_str())
            .await?;
        if resume_as(&flow, &ticket.place, ticket.interrupted_from) == ResumeAs::FreshCheck {
            let hook = self
                .inner
                .validation_resume
                .lock()
                .expect("validation resume")
                .clone();
            if let Some(hook) = hook {
                hook(ticket.id.clone(), Start::User).await?;
                return self.inner.board.ticket_by_id(&ticket.id).await;
            }
        }
        self.bring_back(
            &ticket,
            RETURN_RESUME,
            None,
            EventKind::Resumed,
            json!({}),
            actor,
            Start::User,
        )
        .await
    }

    /// Resume an Interrupted ticket with `answer` as the resumed conversation's first prompt: the inbox answer
    /// to a question asked before the restart, in one click (ATT-08, d0045).
    /// Resumes the agent's conversation of a ticket whose session is gone with `prompt` as its first prompt: a
    /// validation's findings or "cancelled" for an agent that a restart ended (VAL-07, d0063). Its ports and the
    /// agent limit apply as for any resume. Automatic (`start`), it waits in Queued while the project is paused.
    pub async fn resume_with_prompt(
        &self,
        key: &str,
        prompt: &str,
        actor: &Actor,
        start: Start,
    ) -> Result<Ticket, BoardError> {
        let ticket = self.inner.board.ticket(key).await?;
        self.bring_back(
            &ticket,
            RETURN_RESUME,
            Some(prompt),
            EventKind::Resumed,
            json!({ "for": "validation" }),
            actor,
            start,
        )
        .await
    }

    /// Registers how a ticket interrupted while validating is resumed (the validation service, d0063).
    pub fn on_resume_validation(&self, hook: ValidationResume) {
        *self
            .inner
            .validation_resume
            .lock()
            .expect("validation resume") = Some(hook);
    }

    pub async fn resume_with_answer(
        &self,
        key: &str,
        answer: &str,
        actor: &Actor,
    ) -> Result<Ticket, BoardError> {
        let ticket = self.inner.board.ticket(key).await?;
        if ticket.status != Status::Interrupted {
            return Err(BoardError::refused(
                Refusal::MoveRefused,
                format!(
                    "{} is {}; only Interrupted tickets are resumed",
                    ticket.key,
                    ticket.status.label()
                ),
            ));
        }
        self.bring_back(
            &ticket,
            RETURN_RESUME,
            Some(answer),
            EventKind::Resumed,
            json!({ "answer": true }),
            actor,
            Start::User,
        )
        .await
    }

    /// **Retry** after Resume failed: the same resume again.
    pub async fn retry_resume(&self, key: &str, actor: &Actor) -> Result<Ticket, BoardError> {
        let ticket = self.resume_failed(key).await?;
        self.bring_back(
            &ticket,
            RETURN_RESUME,
            None,
            EventKind::Resumed,
            json!({ "retry": true }),
            actor,
            Start::User,
        )
        .await
    }

    /// **Start from scratch** after Resume failed, or for an Interrupted ticket whose conversation is no longer
    /// available ("Before the restart" says so): `confirm` is the user's yes to [`start_from_scratch_message`].
    pub async fn start_from_scratch(
        &self,
        key: &str,
        confirm: bool,
        actor: &Actor,
    ) -> Result<Ticket, BoardError> {
        let ticket = self.inner.board.ticket(key).await?;
        if !matches!(ticket.status, Status::ResumeFailed | Status::Interrupted) {
            return Err(BoardError::refused(
                Refusal::MoveRefused,
                format!(
                    "{} is {}; Start from scratch follows an interrupted session",
                    ticket.key,
                    ticket.status.label()
                ),
            ));
        }
        if !confirm {
            return Err(BoardError::refused(
                Refusal::NeedsConfirmation,
                start_from_scratch_message(&ticket.key.to_string()),
            ));
        }
        let old = crate::session::StoredConversation::latest_agent(
            self.inner.board.store().pool(),
            &ticket.id,
        )
        .await?;
        self.bring_back(
            &ticket,
            RETURN_SCRATCH,
            None,
            EventKind::StartedFromScratch,
            json!({
                "tool": old.as_ref().map(|c| c.tool),
                "old_tool_session_id": old.map(|c| c.id),
            }),
            actor,
            Start::User,
        )
        .await
    }

    /// **Resume all**: every Interrupted ticket, of one project or of all (the inbox is global), except a paused
    /// project's (PAU-04: its Resume is the pause's). Returns the tickets resumed, in key order.
    pub async fn resume_all(
        &self,
        project: Option<&str>,
        actor: &Actor,
    ) -> Result<Vec<TicketKey>, BoardError> {
        let board = &self.inner.board;
        let projects = match project {
            Some(key) => vec![board.project(key).await?.key],
            None => board.projects().await?.into_iter().map(|p| p.key).collect(),
        };
        let mut resumed = Vec::new();
        for project in projects {
            if crate::agents::pause::is_paused(board.store(), &project).await? {
                continue;
            }
            let tickets = board.tickets(&project).await?;
            for key in resume_all_eligible(&tickets) {
                self.resume(&key.to_string(), actor).await?;
                resumed.push(key);
            }
        }
        resumed.sort();
        Ok(resumed)
    }

    async fn resume_failed(&self, key: &str) -> Result<Ticket, BoardError> {
        let ticket = self.inner.board.ticket(key).await?;
        if ticket.status != Status::ResumeFailed {
            return Err(BoardError::refused(
                Refusal::MoveRefused,
                format!(
                    "{} is {}; Retry follows a failed resume",
                    ticket.key,
                    ticket.status.label()
                ),
            ));
        }
        Ok(ticket)
    }

    /// The Agent stage a resumed ticket waits before: the one it is in, or (from a check) the nearest earlier one;
    /// `None` for the flow's first, which is Ready for Dev as today.
    async fn waits_before(&self, ticket: &Ticket) -> Result<Option<String>, BoardError> {
        let Some(id) = ticket.place.stage() else {
            return Ok(None);
        };
        let flow = self
            .inner
            .board
            .effective_flow(ticket.key.project.as_str())
            .await?;
        let stage = match flow.stage(id) {
            Some(stage) if stage.is_agent_stage() => Some(stage),
            Some(_) => crate::flow::engine::back_to(&flow, id, &[]),
            None => None,
        };
        Ok(stage.and_then(|s| crate::board::send_back::waits_before(&flow, &s.id)))
    }

    /// Back to Queued with how to continue (`mode`), then the start sequence right away (as `start` says: an
    /// automatic one waits in Queued while the project is paused), or a wait for a slot while the agent limit is full.
    #[allow(clippy::too_many_arguments)]
    async fn bring_back(
        &self,
        ticket: &Ticket,
        mode: &str,
        prompt: Option<&str>,
        kind: EventKind,
        mut payload: serde_json::Value,
        actor: &Actor,
        start: Start,
    ) -> Result<Ticket, BoardError> {
        let inner = &self.inner;
        let board = &inner.board;
        if inner
            .sessions
            .running_of(&ticket.id, SessionKind::Agent)
            .is_some()
        {
            return Err(BoardError::refused(
                Refusal::AlreadyRunning,
                format!("{} already has a running agent session", ticket.key),
            ));
        }
        let full = self.limit_full().await?;
        let waits_before = self.waits_before(ticket).await?;
        let now = now_ms();
        // Resume and Retry keep the first prompt that never arrived (a Send back's skill, an inbox answer) and send
        // it with the resumed conversation; Start from scratch begins with the ticket prompt (d0018), so it drops
        // them (Q25).
        let scratch = mode == RETURN_SCRATCH;
        let mut tx = board.store().begin_write().await?;
        sqlx::query(concat!(
            "UPDATE tickets SET stage_id = ?, state = ?, return_mode = ?,
                                resume_prompt = CASE WHEN ? THEN NULL ELSE COALESCE(?, resume_prompt) END,
                                resume_skill = CASE WHEN ? THEN NULL ELSE resume_skill END, queued_at = ?,
                                start_problem_step = NULL, start_problem_message = NULL, signal_text = NULL,
                                signal_since = NULL, wakeup_at = NULL, updated_by = ?, updated_at = ?, ",
            set_state_since!(),
            " WHERE id = ?"
        ))
        .bind(waits_before.as_deref())
        .bind(Status::Queued.as_str())
        .bind(mode)
        .bind(scratch)
        .bind(prompt)
        .bind(scratch)
        .bind(now)
        .bind(actor.to_string())
        .bind(now)
        .bind(Status::Queued.as_str())
        .bind(now)
        .bind(&ticket.id)
        .execute(&mut *tx)
        .await?;
        payload["from"] = json!(ticket.status);
        record_event(&mut tx, &ticket.id, kind, actor, payload).await?;
        tx.commit().await?;
        tracing::info!(ticket = %ticket.key, mode, full, "resuming");
        if full {
            inner.scheduler.wake();
        } else {
            match inner
                .starter
                .start(&ticket.key.to_string(), actor, start)
                .await?
            {
                StartOutcome::Started(_) | StartOutcome::Skipped | StartOutcome::Kept => {}
                StartOutcome::Held { message } | StartOutcome::Interrupted { message, .. } => {
                    tracing::warn!(ticket = %ticket.key, %message, "resuming the agent stopped");
                }
            }
        }
        board.ticket_by_id(&ticket.id).await
    }
}
