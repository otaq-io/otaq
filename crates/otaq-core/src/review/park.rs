//! Park (CLS-06; d0052, d0026): a Dev ticket dragged back to Backlog after a confirmation ("SHOP-25's agent will be
//! stopped. Its branch, files and conversation are kept."). The agent stops; the branch, the worktree (with
//! uncommitted files) and the session's tool id stay; both ports return to the pool; the card shows "Parked · has
//! work" and holds no agent slot. Dragging it back to Dev resumes the same conversation on its tool with
//! new ports: the start sequence sees the `parked` mark ([`crate::start::Starter`]).
//!
//! An agent whose tool reports its conversation id only after start (Codex, ADP-07) is given up to
//! [`TOOL_ID_WAIT`] for it before it stops; still without one, it is parked anyway and its next start is fresh.
//!
//! The ticket's Run stops too, before the ports are freed, so no program keeps the freed Run port (Q30); the
//! [`on_ticket_parked`](Review::on_ticket_parked) hooks do it. The Shell holds no port and stays open.

use std::time::Duration;

use serde_json::json;

use super::Review;
use super::finish::{FinishedHook, FinishedTicket};
use crate::board::tickets::{EventKind, Ticket, record_event};
use crate::board::{BoardError, Refusal};
use crate::domain::{Actor, Status, StatusSet};
use crate::session::SessionKind;
use crate::store::now_ms;
use crate::store::tickets::set_state_since;

/// How long Park waits for the conversation id of an agent whose tool reports it after start (ADP-07, Q-H).
pub const TOOL_ID_WAIT: Duration = Duration::from_secs(5);

/// "SHOP-25's agent will be stopped. Its branch, files and conversation are kept."
pub fn park_message(key: &str) -> String {
    format!("{key}'s agent will be stopped. Its branch, files and conversation are kept.")
}

impl Review {
    /// Registers a hook run for every parked ticket, before its ports return to the pool (Run stops, Q30).
    pub fn on_ticket_parked(&self, hook: FinishedHook) {
        self.inner
            .parked_hooks
            .lock()
            .expect("parked hooks")
            .push(hook);
    }

    /// **Park**: `confirm` is the user's yes to the confirmation.
    pub async fn park(
        &self,
        key: &str,
        confirm: bool,
        actor: &Actor,
    ) -> Result<Ticket, BoardError> {
        let inner = &self.inner;
        let board = &inner.board;
        let ticket = board.ticket(key).await?;
        if ticket.board_group() != StatusSet::AgentStage {
            return Err(BoardError::refused(
                Refusal::MoveRefused,
                format!("{} isn't in Dev; only Dev tickets are parked", ticket.key),
            ));
        }
        if !confirm {
            return Err(BoardError::refused(
                Refusal::NeedsConfirmation,
                park_message(&ticket.key.to_string()),
            ));
        }
        if let Some(session) = inner.sessions.running_of(&ticket.id, SessionKind::Agent) {
            // A tool that reports its conversation id after start (Codex) may not have yet: wait briefly, so the
            // ticket comes back to the same conversation. Without one, its next start is a fresh conversation.
            if session.tool.is_some()
                && session.tool_session_id.as_deref().is_none_or(str::is_empty)
            {
                let known = inner.sessions.wait_tool_id(&session.id, TOOL_ID_WAIT).await;
                tracing::info!(ticket = %ticket.key, known = known.is_some(), "parking before the tool id was known");
            }
            inner
                .sessions
                .stop(&session.id)
                .await
                .map_err(|e| BoardError::refused(Refusal::SessionEnded, e.to_string()))?;
        }
        // Nothing may keep using the ports once they are freed (d0052, d0026): the Run stops first.
        let parked = FinishedTicket {
            id: ticket.id.clone(),
            key: ticket.key.clone(),
            status: Status::Parked,
        };
        let hooks = inner.parked_hooks.lock().expect("parked hooks").clone();
        for hook in hooks {
            hook(&parked).await;
        }
        inner
            .ports
            .free(&ticket.id)
            .await
            .map_err(|e| BoardError::refused(Refusal::SessionEnded, e.to_string()))?;
        // A ticket that never got to work (no branch yet) simply goes back to Backlog.
        let has_work = ticket.work.as_ref().is_some_and(|w| w.branch.is_some());
        let to = if has_work {
            Status::Parked
        } else {
            Status::Backlog
        };
        let now = now_ms();
        let mut tx = board.store().begin_write().await?;
        sqlx::query(concat!(
            "UPDATE tickets SET stage_id = NULL, state = ?, parked = ?, start_problem_step = NULL,
                                start_problem_message = NULL, signal_text = NULL, signal_since = NULL,
                                wakeup_at = NULL, updated_by = ?, updated_at = ?, ",
            set_state_since!(),
            " WHERE id = ?"
        ))
        .bind(to.as_str())
        .bind(has_work)
        .bind(actor.to_string())
        .bind(now)
        .bind(to.as_str())
        .bind(now)
        .bind(&ticket.id)
        .execute(&mut *tx)
        .await?;
        record_event(
            &mut tx,
            &ticket.id,
            EventKind::Parked,
            actor,
            // The stage it left: dragged out of Backlog it returns there (FLW-18).
            json!({ "from": ticket.status, "to": to, "stage_id": ticket.place.stage_id() }),
        )
        .await?;
        tx.commit().await?;
        tracing::info!(ticket = %ticket.key, has_work, "parked");
        board.ticket_by_id(&ticket.id).await
    }
}
