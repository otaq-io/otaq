//! **Mark merged** and **Mark closed** (REV-13, CLS-02, SCH-05; d0017, d0033, d0049). Otaq never watches
//! PRs (no polling, no webhooks): the user says what happened to an Approved · PR open ticket.
//!
//! Mark merged: Done · Merged, the Done cleanup (agent and Run/Shell stop, ports freed, worktree removed unless
//! dirty), then the base is fetched before the tickets that just became ready start
//! (`Scheduler::on_parent_merged`), and the tickets in Review that merge into the same base are test-merged
//! against it (the early conflict flag). Mark closed: Close ticket after the user's confirmation.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::json;

use super::Review;
use crate::board::tickets::{EventKind, Ticket, record_event};
use crate::board::{BoardError, Refusal};
use crate::domain::{Actor, Status, TicketKey};
use crate::store::now_ms;
use crate::store::tickets::set_state_since;

/// The answer to Mark merged: the ticket, and the waiting tickets that started ("2 waiting tickets started").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarkedMerged {
    pub ticket: Box<Ticket>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub started: Vec<TicketKey>,
}

impl Review {
    /// **Mark merged** on Approved · PR open.
    pub async fn mark_merged(&self, key: &str, actor: &Actor) -> Result<MarkedMerged, BoardError> {
        let inner = &self.inner;
        let board = &inner.board;
        let ticket = board.ticket(key).await?;
        if ticket.status != Status::PrOpen {
            return Err(BoardError::refused(
                Refusal::MoveRefused,
                format!(
                    "{} is {}; Mark merged works on a ticket with an open PR",
                    ticket.key,
                    ticket.status.label()
                ),
            ));
        }
        let to = Status::Merged;
        let now = now_ms();
        // The Delivery finished (STG-06).
        let moved = board
            .moved(&ticket, ticket.status, crate::flow::engine::Move::Delivered)
            .await
            .ok();
        let mut tx = board.store().begin_write().await?;
        if let Some(moved) = &moved {
            moved.record(&mut tx, &ticket.id, None).await?;
        }
        sqlx::query(concat!(
            "UPDATE tickets SET stage_id = NULL, state = ?, outcome = 'merged', conflicts_with = NULL,
                                conflict_files = NULL, conflict_reason = NULL, updated_by = ?, updated_at = ?, ",
            set_state_since!(),
            " WHERE id = ?"
        ))
        .bind(to.as_str())
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
            EventKind::Merged,
            actor,
            json!({ "from": ticket.status, "pr": ticket.flags.pr_number, "marked": true }),
        )
        .await?;
        tx.commit().await?;
        tracing::info!(key = %ticket.key, "marked merged");
        let merged = board.ticket_by_id(&ticket.id).await?;
        // Done: the session ends, Run and Shell stop, ports return, the worktree goes unless it has changes.
        if let Err(e) = self.finish(&merged).await {
            tracing::warn!(ticket = %merged.key, error = %e, "Done cleanup failed");
        }
        // The base is fetched first, then waiting tickets start from it (SCH-05, T-0081).
        let started = match inner.scheduler.on_parent_merged(key).await {
            Ok(plan) => plan.start.into_iter().map(|t| t.key).collect(),
            Err(e) => {
                tracing::warn!(ticket = %ticket.key, error = %e, "can't start the waiting tickets");
                Vec::new()
            }
        };
        // The early conflict flag against the fetched base (GIT-04).
        if let Ok(target) = board.merge_target(&merged).await {
            let project = board.project(ticket.key.project.as_str()).await?;
            if let Err(e) = self
                .flag_conflicts(
                    &ticket.key.project.to_string(),
                    &PathBuf::from(&project.repo_path),
                    &target.target,
                )
                .await
            {
                tracing::warn!(target = %target.target, error = %e, "can't test-merge the tickets in Review");
            }
        }
        Ok(MarkedMerged {
            ticket: Box::new(board.ticket_by_id(&ticket.id).await?),
            started,
        })
    }

    /// **Mark closed** on Approved · PR open, after the user confirmed: closed like Close ticket (branch kept,
    /// worktree removed unless dirty, Run and Shell stop).
    pub async fn mark_closed(
        &self,
        key: &str,
        confirm: bool,
        actor: &Actor,
    ) -> Result<Ticket, BoardError> {
        let board = &self.inner.board;
        let ticket = board.ticket(key).await?;
        if ticket.status != Status::PrOpen {
            return Err(BoardError::refused(
                Refusal::MoveRefused,
                format!(
                    "{} is {}; Mark closed works on a ticket with an open PR",
                    ticket.key,
                    ticket.status.label()
                ),
            ));
        }
        if !confirm {
            return Err(BoardError::refused(
                Refusal::NeedsConfirmation,
                format!(
                    "Mark {}'s PR as closed? The ticket ends Closed · not merged; its branch is kept.",
                    ticket.key
                ),
            ));
        }
        self.close(key, true, actor).await
    }
}
