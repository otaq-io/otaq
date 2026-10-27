//! One reminder before Idle (d0042, refined 2026-10-28).
//!
//! A ticket's agent that ends its turn in an Agent stage without `stage next`, `stage done` or `ask human` (and with
//! nothing in the background) is first reminded to end its stage: [`reminder_text`] is queued as an automatic delivery
//! ([`DeliveryKind::StageReminder`], typed once the agent is idle and its input line is empty) and the card stays
//! Working. If the turn the reminder started ends the same way, the ticket shows Idle as before.
//!
//! No reminder for a turn the user's own message typed in the agent's terminal started (an answer to an open question
//! typed there still gets one), while a helper isn't merged, while the ticket has an open question or another message
//! waits for the session, or when the ticket isn't Working in an Agent stage (validating, delivering, paused).

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use super::Signals;
use crate::board::BoardError;
use crate::domain::Status;
use crate::helpers::HelperStatus;
use crate::naming;
use crate::session::deliveries::{Deliveries, DeliveryKind};

/// What the reminder says.
pub fn reminder_text() -> String {
    let (product, cmd) = (naming::PRODUCT_NAME, naming::COMMAND_NAME);
    format!(
        "{product}: you ended your turn but your stage is still open. If your work is done, commit everything and end your \
         stage with `{cmd} stage next --stdin` (or `{cmd} stage done --stdin` when there is nothing to deliver). If you \
         need the user, ask with `{cmd} ask human --stdin`. If you are waiting for something else, say what in one line."
    )
}

/// Each ticket's reminder facts, by ticket id. They live in memory, like the turns.
pub(super) struct Reminders {
    /// Off only in tests of other turn-end rules ([`Signals::set_stage_reminders`]).
    on: AtomicBool,
    /// Enter in the agent's terminal sent the user's own message (not an answer to an open question): the turn it
    /// starts is a chat with the user.
    chatting: Mutex<HashSet<String>>,
    /// A reminder was queued: the turn it starts gets none.
    reminded: Mutex<HashSet<String>>,
    /// What types the reminder (set once at service start).
    deliveries: OnceLock<Deliveries>,
}

impl Default for Reminders {
    fn default() -> Self {
        Self {
            on: AtomicBool::new(true),
            chatting: Mutex::default(),
            reminded: Mutex::default(),
            deliveries: OnceLock::new(),
        }
    }
}

impl Signals {
    /// Gives the signals what types the reminder (once, at service start).
    pub fn set_deliveries(&self, deliveries: Deliveries) {
        let _ = self.inner.reminders.deliveries.set(deliveries);
    }

    /// Turns the reminder off or back on (tests of other turn-end rules; it is on otherwise).
    pub fn set_stage_reminders(&self, on: bool) {
        self.inner.reminders.on.store(on, Ordering::Relaxed);
    }

    /// The user sent a line of their own in `ticket_id`'s agent terminal: the turn it starts gets no reminder.
    pub(super) fn chatting(&self, ticket_id: &str) {
        self.inner
            .reminders
            .chatting
            .lock()
            .expect("chatting")
            .insert(ticket_id.to_owned());
    }

    /// The agent of `ticket_id` (in `session`) ended its turn as Idle: when the reminder is due, it is queued and the
    /// turn end changes nothing ([`super::Signal::TurnEnded`]); otherwise the ticket shows Idle. Either way the turn's
    /// chat and reminder marks are used up.
    pub(super) async fn remind_instead_of_idle(
        &self,
        ticket_id: &str,
        session: &str,
    ) -> Result<bool, BoardError> {
        let reminders = &self.inner.reminders;
        let chatting = reminders
            .chatting
            .lock()
            .expect("chatting")
            .remove(ticket_id);
        let reminded = reminders
            .reminded
            .lock()
            .expect("reminded")
            .remove(ticket_id);
        let Some(deliveries) = reminders
            .deliveries
            .get()
            .filter(|_| reminders.on.load(Ordering::Relaxed))
        else {
            return Ok(false);
        };
        if chatting || reminded || !self.reminder_due(ticket_id, session, deliveries).await? {
            return Ok(false);
        }
        reminders
            .reminded
            .lock()
            .expect("reminded")
            .insert(ticket_id.to_owned());
        tracing::info!(
            ticket = ticket_id,
            session,
            "stage still open at turn end: reminder queued"
        );
        if let Err(e) = deliveries
            .queue_delivery(session, DeliveryKind::StageReminder, &reminder_text())
            .await
        {
            tracing::warn!(session, error = %e, "can't queue the stage reminder");
            reminders
                .reminded
                .lock()
                .expect("reminded")
                .remove(ticket_id);
            return Ok(false);
        }
        Ok(true)
    }

    /// Whether the ticket's agent should be reminded: at work in an Agent stage, no open question, every helper merged
    /// and nothing else waiting to be typed into `session`.
    async fn reminder_due(
        &self,
        ticket_id: &str,
        session: &str,
        deliveries: &Deliveries,
    ) -> Result<bool, BoardError> {
        // Working in background too: a quiet tool's turn end once what it left running ends (ADP-12).
        if !matches!(
            self.status_of(ticket_id).await?,
            Some(Status::Working | Status::WorkingInBackground)
        ) {
            return Ok(false);
        }
        let pool = self.inner.board.store().pool();
        let stage_kind: Option<String> = sqlx::query_scalar(
            "SELECT kind FROM stage_runs WHERE ticket_id = ? AND ended_at IS NULL ORDER BY started_at DESC LIMIT 1",
        )
        .bind(ticket_id)
        .fetch_optional(pool)
        .await?;
        if stage_kind.as_deref() != Some("agent") {
            return Ok(false);
        }
        if self
            .inner
            .board
            .open_user_question(ticket_id)
            .await?
            .is_some()
        {
            return Ok(false);
        }
        let helpers = crate::helpers::store::of_ticket(pool, ticket_id).await?;
        if helpers.iter().any(|h| h.status != HelperStatus::Merged) {
            return Ok(false);
        }
        match deliveries.pending(session).await {
            Ok(waiting) => Ok(waiting.is_empty()),
            Err(e) => {
                tracing::warn!(session, error = %e, "can't read the waiting messages");
                Ok(false)
            }
        }
    }
}
