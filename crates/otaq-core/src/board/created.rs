//! What was created in a Project terminal (TKT-08, RUN-06; d0060, d0019, d0031; *17 · Project terminal → The
//! review moment*): the service records, by caller identity (CLI-01), which Project terminal session each new
//! ticket came from, whether the user or an agent they started there ran `otaq create`. The drawer's summary
//! lists the tickets created from the project's current terminal since the user last dismissed it.

use serde::{Deserialize, Serialize};

use super::tickets::Ticket;
use super::{Board, BoardError};
use crate::store::now_ms;

/// The tickets created from the project's newest Project terminal since its summary was last dismissed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreatedHere {
    /// The Project terminal session; `None` when the project never had one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
    /// In creation order.
    pub tickets: Vec<Ticket>,
}

impl Board {
    /// Records that these tickets were created from the Project terminal `session`.
    pub async fn mark_created_in(
        &self,
        ticket_ids: &[String],
        session: &str,
    ) -> Result<(), BoardError> {
        let mut tx = self.store().begin_write().await?;
        for id in ticket_ids {
            sqlx::query("UPDATE tickets SET created_in_session = ? WHERE id = ?")
                .bind(session)
                .bind(id)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// What the project's newest Project terminal created since the summary was last dismissed.
    pub async fn created_in_terminal(&self, project: &str) -> Result<CreatedHere, BoardError> {
        let project = self.project(project).await?;
        let newest: Option<(String, Option<i64>)> = sqlx::query_as(
            "SELECT id, summary_dismissed_at FROM sessions
             WHERE project_id = ? AND kind = 'project' AND ticket_id IS NULL
             ORDER BY started_at DESC, id DESC LIMIT 1",
        )
        .bind(&project.id)
        .fetch_optional(self.store().pool())
        .await?;
        let Some((session, dismissed_at)) = newest else {
            return Ok(CreatedHere::default());
        };
        let ids: Vec<(String,)> = sqlx::query_as(
            "SELECT id FROM tickets WHERE created_in_session = ? AND created_at > ?
             ORDER BY created_at, number",
        )
        .bind(&session)
        .bind(dismissed_at.unwrap_or(0))
        .fetch_all(self.store().pool())
        .await?;
        let all = self.tickets(&project.key).await?;
        let tickets = ids
            .into_iter()
            .filter_map(|(id,)| all.iter().find(|t| t.id == id).cloned())
            .collect();
        Ok(CreatedHere {
            session: Some(session),
            tickets,
        })
    }

    /// The user closed the summary: it lists only tickets created after now.
    pub async fn dismiss_created(&self, project: &str) -> Result<(), BoardError> {
        let project = self.project(project).await?;
        sqlx::query(
            "UPDATE sessions SET summary_dismissed_at = ?
             WHERE id = (SELECT id FROM sessions WHERE project_id = ? AND kind = 'project' AND ticket_id IS NULL
                         ORDER BY started_at DESC, id DESC LIMIT 1)",
        )
        .bind(now_ms())
        .bind(&project.id)
        .execute(self.store().pool())
        .await?;
        Ok(())
    }
}
