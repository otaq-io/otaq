//! A ticket session's open permission prompt, whatever the ticket's status.
//!
//! A ticket's status follows its agent's prompts only while the agent is at work ([`super::LIVE`]). The agent that
//! writes the delivery's message works while the ticket waits in Review, where its prompt must not move the card: the
//! ticket stays calm, with no Needs you item or notification. Approve's dialogs ask here instead whether the writer
//! waits for permission, and offer its terminal to answer it.
//!
//! Per session, in memory like the turns: set by a prompt of the tool, cleared by any other signal of it, by an
//! `otaq` command it runs that acts, and by the user's key in its terminal (the answer; a tool may send nothing
//! after it until its turn ends). A change tells clients the ticket changed, so an open dialog asks again.

use super::Signals;
use super::hooks::Signal;
use crate::board::BoardError;
use crate::domain::{Status, TicketKey};
use crate::protocol::Event;
use crate::session::SessionKind;

impl Signals {
    /// The permission prompt `session`'s tool shows now, if any.
    pub fn open_prompt(&self, session: &str) -> Option<String> {
        self.inner.asks.lock().expect("asks").get(session).cloned()
    }

    /// A signal of a ticket's agent session: a prompt opens, anything else but an ignored hook closes it.
    pub(super) fn track_prompt(
        &self,
        ticket: &TicketKey,
        ticket_id: &str,
        session: &str,
        signal: &Signal,
    ) {
        let changed = {
            let mut asks = self.inner.asks.lock().expect("asks");
            match signal {
                Signal::NeedsYou { prompt } => {
                    asks.insert(session.to_owned(), prompt.clone()).as_ref() != Some(prompt)
                }
                Signal::Ignored => false,
                _ => asks.remove(session).is_some(),
            }
        };
        if changed {
            self.prompt_changed(ticket, ticket_id);
        }
    }

    /// The session's prompt was answered (the user's key in its terminal, or a command of the agent's that acts).
    pub fn prompt_answered(&self, ticket: &TicketKey, ticket_id: &str, session: &str) {
        let removed = self
            .inner
            .asks
            .lock()
            .expect("asks")
            .remove(session)
            .is_some();
        if removed {
            self.prompt_changed(ticket, ticket_id);
        }
    }

    /// The user's key in the ticket's agent terminal answered its open prompt. On a tool whose prompt takes a key alone
    /// (Codex's "y", [`crate::agent_tools::AgentTool::key_alone_approves_prompt`]) nothing else will say so until the
    /// turn ends or asks again, so a card showing Needs you is Working again now, as after Enter.
    pub async fn key_answered_prompt(
        &self,
        ticket: &TicketKey,
        ticket_id: &str,
        session: &str,
    ) -> Result<(), BoardError> {
        self.prompt_answered(ticket, ticket_id, session);
        let Ok(info) = self.inner.sessions.info(session) else {
            return Ok(());
        };
        let alone = info
            .tool
            .is_some_and(|tool| self.inner.adapters.get(tool).key_alone_approves_prompt());
        if alone
            && info.kind == SessionKind::Agent
            && self.status_of(ticket_id).await? == Some(Status::NeedsYou)
        {
            self.answered(ticket_id).await?;
        }
        Ok(())
    }

    fn prompt_changed(&self, ticket: &TicketKey, ticket_id: &str) {
        self.inner.events.publish(Event::TicketChanged {
            project: ticket.project.to_string(),
            id: ticket_id.to_owned(),
            key: ticket.clone(),
        });
    }
}
