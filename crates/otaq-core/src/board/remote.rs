//! An unreachable remote (d0121): when the fetch before a start or a Local merge into a base branch fails, a ticket
//! whose flow delivers by Local merge or lets the user pick (`delivers: local | either`) goes on from the local base
//! branch, with a warning kept on the ticket and a History line; a flow that delivers by PR stops as before. The
//! warning goes at the next fetch for the ticket that works. An automatic Delivery whose git step fails says so on the
//! card ("Couldn't deliver · <reason>", Retry merge) instead of only in the log.

use serde_json::json;

use super::tickets::{EventKind, Ticket, record_event};
use super::{Board, BoardError};
use crate::domain::validation::delivery_failed;
use crate::domain::{Actor, AutoMergeStop, RemoteWarning, Status};
use crate::flow::Delivers;
use crate::git::GitError;

impl Board {
    /// Whether `ticket` goes on from its local base branch when the fetch fails: unless its project's flow delivers
    /// by PR, which needs the remote anyway. An epic child counts as its epic, the delivery unit (d0059).
    pub async fn goes_on_without_remote(&self, ticket: &Ticket) -> Result<bool, BoardError> {
        let flow = self.effective_flow(ticket.key.project.as_str()).await?;
        let delivers = flow
            .stages
            .iter()
            .find_map(|s| s.delivery)
            .map(|d| d.delivers);
        Ok(delivers != Some(Delivers::Pr))
    }

    /// The ticket went on without its remote: the warning is kept on it and History logs it.
    pub async fn record_remote_warning(
        &self,
        ticket_id: &str,
        warning: &RemoteWarning,
        actor: &Actor,
    ) -> Result<(), BoardError> {
        let stored = serde_json::to_string(warning).expect("serializable");
        let mut tx = self.store.begin_write().await?;
        sqlx::query("UPDATE tickets SET remote_warning = ? WHERE id = ?")
            .bind(&stored)
            .bind(ticket_id)
            .execute(&mut *tx)
            .await?;
        record_event(
            &mut tx,
            ticket_id,
            EventKind::RemoteUnreachable,
            actor,
            json!(warning),
        )
        .await?;
        tx.commit().await?;
        tracing::warn!(ticket = ticket_id, remote = %warning.remote, reason = %warning.reason,
                       "went on without the remote");
        Ok(())
    }

    /// A fetch for the ticket worked: its warning goes.
    pub async fn clear_remote_warning(&self, ticket_id: &str) -> Result<(), BoardError> {
        sqlx::query(
            "UPDATE tickets SET remote_warning = NULL WHERE id = ? AND remote_warning IS NOT NULL",
        )
        .bind(ticket_id)
        .execute(self.store.pool())
        .await?;
        Ok(())
    }

    /// An automatic Delivery's git step failed: the ticket stops turquoise with "Couldn't deliver · <reason>" and
    /// Retry merge, as a diverged base does. Nothing happens to a ticket that already left its Delivery.
    pub async fn automatic_delivery_failed(
        &self,
        ticket_id: &str,
        error: &GitError,
    ) -> Result<(), BoardError> {
        let ticket = self.ticket_by_id(ticket_id).await?;
        if ticket.status != Status::ToDeliver {
            return Ok(());
        }
        let message = delivery_failed(&error.to_string());
        self.delivery_stopped(ticket_id, AutoMergeStop::DeliveryFailed, &message, false)
            .await
    }
}
