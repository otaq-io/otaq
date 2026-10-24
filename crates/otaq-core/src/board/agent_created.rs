//! A ticket an agent created (#47; d0016, d0083): the creating ticket's History logs it ("Created DEMO-12 Fix the
//! build"), and the service tells the app, which shows one quiet toast per burst. An epic lead's new child is
//! already on its epic's History (`lead_changed`, "Added SHOP-40.5 …").

use serde_json::json;

use super::tickets::{EventKind, Ticket, record_event};
use super::{Board, BoardError};
use crate::domain::Actor;

impl Board {
    /// Logs on the ticket `creator_id` that its agent created `child`.
    pub async fn record_created_ticket(
        &self,
        creator_id: &str,
        child: &Ticket,
        actor: &Actor,
    ) -> Result<(), BoardError> {
        let mut tx = self.store.begin_write().await?;
        record_event(
            &mut tx,
            creator_id,
            EventKind::CreatedTicket,
            actor,
            json!({"child": child.key, "title": child.title, "to": child.status}),
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }
}
