//! An agent asks the user to send its ticket back to an earlier stage (`otaq ask human --send-back <stage>`).
//!
//! Any agent working a ticket, a work agent or a checker, that finds a problem belonging to an earlier stage of the
//! ticket's flow (one it skipped or one that already ran) asks the user, naming that stage, its question being the
//! reason. The stage must be one the ticket is past: before the stage it is in now, in its own flow, with a run of it
//! (ran or skipped). A later stage, the stage it is in, or one it never walked through is refused
//! ([`Board::send_back_target`]).
//!
//! The question's two ways out are the user's: **answer here** (an ordinary answer, typed into the agent's session) or
//! **send back**, which is the user's own drag back to that stage ([`crate::review::Review::drag`]), with the skip on
//! it cleared first ([`Board::clear_skip`]): only the user skips a stage, so only the user undoes a skip. The agent
//! never moves a ticket back itself.

use serde_json::json;

use super::questions::SendBackTo;
use super::tickets::Ticket;
use super::{Board, BoardError, Refusal};
use crate::domain::Place;

impl Board {
    /// The earlier stage `name` (a stage's name, ignoring case, or its id) of `ticket`'s flow that its agent may ask
    /// to send it back to, or the refusal saying why not.
    pub async fn send_back_target(
        &self,
        ticket: &Ticket,
        name: &str,
    ) -> Result<SendBackTo, BoardError> {
        let refuse = |message: String| Err(BoardError::refused(Refusal::BadQuestion, message));
        let name = name.trim();
        if name.is_empty() {
            return refuse(
                "Name the earlier stage to send it back to: --send-back \"Product spec\""
                    .to_owned(),
            );
        }
        let flow = self.effective_flow(ticket.key.project.as_str()).await?;
        let folded = name.to_lowercase();
        let Some(target) = flow
            .stages
            .iter()
            .find(|s| s.id == name || s.name.to_lowercase() == folded)
        else {
            let names: Vec<&str> = flow.stages.iter().map(|s| s.name.as_str()).collect();
            return refuse(format!(
                "{}'s flow has no stage {name} (its stages: {})",
                ticket.key,
                names.join(", ")
            ));
        };
        let Place::InStage(here) = &ticket.place else {
            return refuse(format!(
                "{} isn't in a stage: only an agent working a ticket asks to send it back",
                ticket.key
            ));
        };
        let (Some(at), Some(to)) = (flow.position(here), flow.position(&target.id)) else {
            return refuse(format!("{} isn't in a stage of its flow", ticket.key));
        };
        if to >= at {
            let here = flow.stage(here).map_or(here.as_str(), |s| s.name.as_str());
            return refuse(format!(
                "{} isn't before {here}: a ticket is sent back only to an earlier stage",
                target.name
            ));
        }
        let walked: Option<i64> = sqlx::query_scalar(
            "SELECT 1 FROM stage_runs WHERE ticket_id = ? AND stage_id = ? LIMIT 1",
        )
        .bind(&ticket.id)
        .bind(&target.id)
        .fetch_optional(self.store().pool())
        .await?;
        if walked.is_none() {
            return refuse(format!(
                "{} hasn't been through {}: a ticket is sent back only to a stage it is past",
                ticket.key, target.name
            ));
        }
        Ok(SendBackTo {
            stage_id: target.id.clone(),
            stage: target.name.clone(),
        })
    }

    /// Clears `ticket`'s skip of `stage_id`, as the user's own change: the stage runs again as the flow says. The
    /// skip it cleared, if there was one, to put back with [`Board::restore_skip`] when the send back doesn't happen.
    pub async fn clear_skip(
        &self,
        ticket: &Ticket,
        stage_id: &str,
    ) -> Result<Option<ClearedSkip>, BoardError> {
        let row: Option<ClearedSkip> = sqlx::query_as(
            "DELETE FROM ticket_stage_choices WHERE ticket_id = ? AND stage_id = ? AND choice = 'skip'
             RETURNING ticket_id, stage_id, set_by, by_name, reason, set_at",
        )
        .bind(&ticket.id)
        .bind(stage_id)
        .fetch_optional(self.store().pool())
        .await?;
        if row.is_some() {
            tracing::info!(ticket = %ticket.key, stage = stage_id, "skip cleared by a send back");
        }
        Ok(row)
    }

    /// Puts back a skip [`Board::clear_skip`] cleared, as it was.
    pub async fn restore_skip(&self, skip: &ClearedSkip) {
        let put = sqlx::query(
            "INSERT OR IGNORE INTO ticket_stage_choices (ticket_id, stage_id, choice, set_by, by_name, reason, set_at)
             VALUES (?, ?, 'skip', ?, ?, ?, ?)",
        )
        .bind(&skip.ticket_id)
        .bind(&skip.stage_id)
        .bind(&skip.set_by)
        .bind(&skip.by_name)
        .bind(&skip.reason)
        .bind(skip.set_at)
        .execute(self.store().pool())
        .await;
        if let Err(e) = put {
            tracing::warn!(ticket = %skip.ticket_id, error = %e, "can't put the skip back");
        }
    }
}

/// A skip a send back cleared, as it was stored.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct ClearedSkip {
    ticket_id: String,
    stage_id: String,
    set_by: String,
    by_name: Option<String>,
    reason: Option<String>,
    set_at: i64,
}

/// What a send back adds to its drag's History row: the stage whose agent asked, its reason, and whether a skip was
/// cleared.
pub(crate) fn asked_payload(
    asked_by: Option<&str>,
    reason: &str,
    skip_cleared: bool,
) -> serde_json::Value {
    json!({
        "asked_by": asked_by,
        "reason": reason.trim(),
        "skip_cleared": skip_cleared,
    })
}
