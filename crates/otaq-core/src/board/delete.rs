//! Deleting tickets that never went to Dev (TKT-08; d0060, d0029, d0018, d0044). Nothing exists on disk for
//! them (no branch, worktree or conversation), so a delete is pure bookkeeping, but it is still a deletion: the
//! user confirms it, from what [`Board::delete_preview`] names (the links that go with it). Anything that has been
//! to Dev can only be Closed (d0033). The number is never handed out again (d0029); the key stops resolving.

use serde::{Deserialize, Serialize};

use super::tickets::Ticket;
use super::{Board, BoardError, Refusal};
use crate::domain::TicketKey;

/// What the delete confirmation names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeletePreview {
    pub key: TicketKey,
    pub title: String,
    /// Links that go with it: (dependent, dependency) where one side stays.
    pub links: Vec<(TicketKey, TicketKey)>,
}

impl DeletePreview {
    /// "Delete SHOP-21 Order confirmation email?"
    pub fn question(&self) -> String {
        format!("Delete {} {}?", self.key, self.title)
    }

    /// One line per link that will be removed: "SHOP-21 is a dependency of SHOP-22; that link will be removed".
    pub fn link_lines(&self) -> Vec<String> {
        self.links
            .iter()
            .map(|(dependent, dependency)| {
                format!("{dependency} is a dependency of {dependent}; that link will be removed")
            })
            .collect()
    }
}

impl Board {
    /// What deleting `key` would remove, or why it can't be deleted.
    pub async fn delete_preview(&self, key: &str) -> Result<DeletePreview, BoardError> {
        let (ticket, tickets) = self.deletion(key).await?;
        let mut links = Vec::new();
        for t in &tickets {
            for parent in &t.depends_on {
                if (t.key == ticket.key) != (*parent == ticket.key) {
                    links.push((t.key.clone(), parent.clone()));
                }
            }
        }
        links.sort();
        Ok(DeletePreview {
            key: ticket.key.clone(),
            title: ticket.title.clone(),
            links,
        })
    }

    /// Deletes a never-started ticket, with its links, review and message rows, its comments and their reports.
    pub async fn delete_ticket(&self, key: &str) -> Result<DeletePreview, BoardError> {
        let preview = self.delete_preview(key).await?;
        let (ticket, _) = self.deletion(key).await?;
        // Its comments' reports go with it (CMT-03); the rows go by cascade.
        let reports = self.report_files_of(&[ticket.id.as_str()]).await?;
        let mut tx = self.store.begin_write().await?;
        sqlx::query("DELETE FROM tickets WHERE id = ?")
            .bind(&ticket.id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        self.remove_reports(&reports);
        tracing::info!(key = %ticket.key, "ticket deleted");
        Ok(preview)
    }

    /// The ticket and the project's tickets; or the refusal.
    async fn deletion(&self, key: &str) -> Result<(Ticket, Vec<Ticket>), BoardError> {
        let ticket = self.ticket(key).await?;
        let tickets = self.tickets(ticket.key.project.as_str()).await?;
        if self.has_been_to_dev(&ticket).await? {
            return Err(BoardError::refused(
                Refusal::Started,
                format!("{} has been to Dev. Close it instead.", ticket.key),
            ));
        }
        Ok((ticket, tickets))
    }
}
