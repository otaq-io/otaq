//! A helper's permission prompts and its Idle (d0133 audit 3; d0011, d0016, d0042, d0048).
//!
//! - **A permission prompt** (the tool's hook, [`Signal::NeedsYou`]) goes straight to the user, since only the user can
//!   grant it: it is stored on the helper (`helpers.prompt`) and the ticket is Needs you with "40.3 needs you: allow …"
//!   ([`crate::signals::state`] holds it while it is open), its chip "SHOP-40 · 40.3 needs you". ⌘J and the chip open
//!   the helper's terminal ([`crate::domain::PieceWait::session`]), where the user answers in the tool's own prompt:
//!   Otaq never answers it (d0011, d0016). It closes on the helper's next hook that says its turn moved on, or Enter
//!   in its terminal ([`Helpers::enter`]); a prompt of a helper whose session ended never shows.
//! - **Idle** (a Stop with nothing asked or handed back, [`Signal::Idle`]) is reported to the agent in charge as a
//!   note ("Piece 3's helper stopped without handing back; its last line: …"), never to the user.

use super::{Helper, HelperStatus, Helpers, store};
use crate::board::BoardError;
use crate::board::questions::AnsweredVia;
use crate::caller::Caller;
use crate::domain::{PieceWait, PieceWaitKind, TicketKey};
use crate::session::SessionKind;
use crate::session::deliveries::DeliveryKind;
use crate::signals::Signal;

/// The ticket's line: "40.3 needs you: Allow `rm -rf .next`?" (the tool's prompt as it gives it).
pub fn needs_you_line(label: &str, prompt: &str) -> String {
    match prompt.trim() {
        "" => format!("{label} needs you"),
        prompt => format!("{label} needs you: {prompt}"),
    }
}

/// The chip and the Needs you item: "SHOP-40 · 40.3 needs you".
pub fn needs_you_chip(ticket: &TicketKey, label: &str) -> String {
    format!("{ticket} \u{b7} {label} needs you")
}

/// A helper's open prompt as the ticket's [`PieceWait`].
pub fn needs_you_wait(helper: &Helper, prompt: &str) -> PieceWait {
    let label = helper.label();
    PieceWait {
        piece: helper.piece,
        chip: needs_you_chip(&helper.ticket, &label),
        line: needs_you_line(&label, prompt),
        label,
        kind: PieceWaitKind::NeedsYou,
        session: helper.session.clone(),
        note: None,
    }
}

/// The note typed into the agent in charge when a helper stops without handing back: "Piece 3's helper stopped
/// without handing back; its last line: Tests pass, should I also add the empty state?".
pub fn idle_note(piece: u32, last_line: Option<&str>) -> String {
    match last_line.map(str::trim).filter(|l| !l.is_empty()) {
        Some(line) => {
            format!("Piece {piece}'s helper stopped without handing back; its last line: {line}")
        }
        None => format!("Piece {piece}'s helper stopped without handing back."),
    }
}

impl Helpers {
    /// What follows a helper's hook signals (after [`crate::signals::Signals`] tracked its turn): its prompt opens or
    /// closes, and its Idle goes to the agent in charge. Anyone else's signals change nothing here.
    pub async fn after_signals(
        &self,
        caller: &Caller,
        signals: &[Signal],
    ) -> Result<(), BoardError> {
        let Caller::Helper {
            ticket_id, session, ..
        } = caller
        else {
            return Ok(());
        };
        let pool = self.inner.board.store().pool();
        for signal in signals {
            let Some(helper) = store::of_session(pool, session).await? else {
                return Ok(());
            };
            match signal {
                Signal::NeedsYou { prompt } => {
                    store::set_prompt(pool, &helper.id, prompt).await?;
                    tracing::info!(ticket = %helper.ticket, piece = helper.piece, "a helper's prompt waits for the user");
                    self.wait_changed(ticket_id, None).await?;
                }
                Signal::Ignored => {}
                other => {
                    self.prompt_closed(&helper).await?;
                    if let Signal::Idle { last_line } = other {
                        self.idle(&helper, last_line.as_deref()).await?;
                    }
                }
            }
        }
        Ok(())
    }

    /// The helper's prompt closed: the ticket stops showing it.
    async fn prompt_closed(&self, helper: &Helper) -> Result<(), BoardError> {
        let pool = self.inner.board.store().pool();
        if let Some(prompt) = store::clear_prompt(pool, &helper.id).await? {
            let ticket = self.inner.board.ticket(&helper.ticket.to_string()).await?;
            self.wait_changed(&ticket.id, Some(needs_you_line(&helper.label(), &prompt)))
                .await?;
        }
        Ok(())
    }

    /// A helper at work stopped without asking or handing back: the agent in charge is told (d0133 audit 3).
    async fn idle(&self, helper: &Helper, last_line: Option<&str>) -> Result<(), BoardError> {
        if !matches!(
            helper.status,
            HelperStatus::Working | HelperStatus::SentBack
        ) {
            return Ok(());
        }
        let ticket = self.inner.board.ticket(&helper.ticket.to_string()).await?;
        let Some(in_charge) = self
            .inner
            .sessions
            .running_of(&ticket.id, SessionKind::Agent)
        else {
            return Ok(());
        };
        self.inner
            .deliveries
            .queue_delivery(
                &in_charge.id,
                DeliveryKind::AgentMessage,
                &idle_note(helper.piece, last_line),
            )
            .await
            .map_err(|e| BoardError::refused(crate::board::Refusal::SessionEnded, e.to_string()))?;
        tracing::info!(ticket = %ticket.key, piece = helper.piece, "a helper stopped: the agent in charge is told");
        Ok(())
    }

    /// Enter in a helper's terminal (`line`: the line sent): the user answered its prompt in the tool, so it closes
    /// (ATT-02); else a line that isn't empty answers its passed-on question (d0119), logged as the user's answer.
    pub async fn enter(&self, session: &str, line: Option<&str>) -> Result<(), BoardError> {
        let pool = self.inner.board.store().pool();
        let Some(helper) = store::of_session(pool, session).await? else {
            return Ok(());
        };
        if helper.prompt.is_some() {
            return self.prompt_closed(&helper).await;
        }
        let Some(line) = line.map(str::trim).filter(|l| !l.is_empty()) else {
            return Ok(());
        };
        if let Some(q) = super::questions::open_of(pool, &helper.id).await?
            && q.state == super::questions::HelperQuestionState::PassedOn
        {
            self.user_answered(&helper, &q.question.id, line, AnsweredVia::Terminal)
                .await?;
            tracing::info!(ticket = %helper.ticket, piece = helper.piece, "a helper's question answered in its terminal");
        }
        Ok(())
    }

    /// The helper whose prompt the ticket shows (the longest open, of a session still running) with that prompt.
    pub async fn open_prompt(
        &self,
        ticket_id: &str,
    ) -> Result<Option<(Helper, String)>, BoardError> {
        let pool = self.inner.board.store().pool();
        for helper in store::open_prompts(pool, ticket_id).await? {
            if self.live_session(&helper).is_some()
                && let Some(prompt) = helper.prompt.clone()
            {
                return Ok(Some((helper, prompt)));
            }
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn texts() {
        assert_eq!(
            needs_you_line("40.3", "Allow `rm -rf .next`?"),
            "40.3 needs you: Allow `rm -rf .next`?"
        );
        let key: TicketKey = "SHOP-40".parse().unwrap();
        assert_eq!(
            needs_you_chip(&key, "40.3"),
            "SHOP-40 \u{b7} 40.3 needs you"
        );
        assert_eq!(
            idle_note(3, Some(" Tests pass, should I also add the empty state? ")),
            "Piece 3's helper stopped without handing back; its last line: Tests pass, should I also add the empty \
             state?"
        );
        assert_eq!(
            idle_note(3, None),
            "Piece 3's helper stopped without handing back."
        );
    }
}
