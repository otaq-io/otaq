//! Closing a ticket's bookkeeping (TKT-07; d0044, d0060): whether a ticket has been to Dev, and the move to Done ·
//! Closed · not merged with its History line. Stopping agents and removing worktrees on close come with sessions
//! (T-0100, [`crate::review`]).

use serde_json::json;
use sqlx::{Sqlite, Transaction};

use super::tickets::{EventKind, Ticket, record_event};
use super::{Board, BoardError};
use crate::domain::{Actor, Status};
use crate::store::now_ms;
use crate::store::tickets::set_state_since;

impl Board {
    /// Whether a ticket has ever been to Dev: a start time, a branch, a worktree or a session row (d0044, d0060).
    pub(crate) async fn has_been_to_dev(&self, ticket: &Ticket) -> Result<bool, BoardError> {
        if ticket.has_started() {
            return Ok(true);
        }
        let traces: i64 = sqlx::query_scalar(
            "SELECT (SELECT COUNT(*) FROM sessions WHERE ticket_id = ?)
                  + (SELECT COUNT(*) FROM tickets WHERE id = ? AND (branch_name IS NOT NULL OR worktree_path IS NOT NULL))",
        )
        .bind(&ticket.id)
        .bind(&ticket.id)
        .fetch_one(self.store.pool())
        .await?;
        Ok(traces > 0)
    }
}

/// Moves a ticket to Done · Closed · not merged and logs it. Agents, worktrees and Run/Shell are handled with
/// sessions (T-0100). Tickets an agent's `otaq stage done` closed before it finished as Done keep their
/// `nothing_to_merge` reason and "Closed · nothing to merge"; nothing closes that way now.
pub(crate) async fn close_bookkeeping(
    tx: &mut Transaction<'_, Sqlite>,
    ticket_id: &str,
    from: Status,
    actor: &Actor,
) -> Result<(), BoardError> {
    let to = Status::ClosedNotMerged;
    let now = now_ms();
    sqlx::query(concat!(
        "UPDATE tickets SET stage_id = NULL, state = ?, outcome = 'closed', nothing_to_merge = NULL, updated_by = ?, ",
        "updated_at = ?, ",
        set_state_since!(),
        " WHERE id = ?"
    ))
    .bind(to.as_str())
    .bind(actor.to_string())
    .bind(now)
    .bind(to.as_str())
    .bind(now)
    .bind(ticket_id)
    .execute(&mut **tx)
    .await?;
    record_event(
        tx,
        ticket_id,
        EventKind::Closed,
        actor,
        json!({"from": from}),
    )
    .await
}
