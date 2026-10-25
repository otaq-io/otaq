//! The ticket is one (d0133 audit 5; d0112, d0052, d0108, d0018, d0045, d0122): Pause, Park and a drag back out of the
//! stage stop the agent in charge and every helper and keep every worktree; Close ends them. After a restart the
//! ticket is one Interrupted item, "Interrupted · 2 helpers were working". **Resume** (and un-park) continues the
//! agent in charge with 0122's message, then each helper whose status was `working` or `sent back`, each in its own
//! conversation, told "You were interrupted by a restart. Carry on with piece 3." ([`Helpers::resume_after`]). A helper
//! that was being reviewed or merged isn't resumed; a held one starts when its hold lifts.
//!
//! Also what the board reads for the card: which helper waits for the user ([`piece_wait`]) and which ones Resume
//! brings back ([`to_resume`]).

use std::path::Path;

use super::{Helper, Helpers, questions, store, waits};
use crate::agent_tools::FirstPrompt;
use crate::agents::RunsOn;
use crate::board::BoardError;
use crate::domain::{PieceWait, Status};
use crate::session::StoredConversation;

/// "1 helper was working", "2 helpers were working".
pub fn helpers_were_working(count: usize) -> String {
    match count {
        1 => "1 helper was working".to_owned(),
        n => format!("{n} helpers were working"),
    }
}

/// The Interrupted item and card: "Interrupted · 2 helpers were working"; "Interrupted" without helpers to resume.
pub fn interrupted_line(count: usize) -> String {
    match count {
        0 => Status::Interrupted.label().to_owned(),
        n => format!(
            "{} \u{b7} {}",
            Status::Interrupted.label(),
            helpers_were_working(n)
        ),
    }
}

/// What a resumed helper is told first: "You were interrupted by a restart. Carry on with piece 3."
pub fn carry_on_text(piece: u32) -> String {
    format!("You were interrupted by a restart. Carry on with piece {piece}.")
}

/// The helpers Resume (or un-park) brings back, by label ("40.2"): those that were working or sent back, while the
/// ticket is Interrupted, Resume failed or Parked.
pub fn to_resume(helpers: &[Helper], status: Status) -> Vec<String> {
    if !matches!(
        status,
        Status::Interrupted | Status::ResumeFailed | Status::Parked
    ) {
        return Vec::new();
    }
    helpers
        .iter()
        .filter(|h| h.resumes())
        .map(Helper::label)
        .collect()
}

/// The helper the ticket's turquoise status is about (d0133 audit 3), if any: its passed-on question (`question`: the
/// ticket's open question to the user, with the helper that asked it and the agent in charge's note) while the ticket
/// shows Asks you, or its prompt while the ticket shows Needs you with that prompt's line. An Interrupted ticket that
/// kept its label (d0045) keeps it too.
pub fn piece_wait(
    helpers: &[Helper],
    status: Status,
    interrupted_from: Option<Status>,
    signal_text: Option<&str>,
    question: Option<(Option<&str>, &str, Option<&str>)>,
) -> Option<PieceWait> {
    let shown = match status {
        Status::Interrupted => interrupted_from?,
        other => other,
    };
    match shown {
        Status::AsksYou => {
            let (Some(helper_id), text, note) = question? else {
                return None;
            };
            let helper = helpers.iter().find(|h| h.id == helper_id)?;
            Some(questions::asks_wait(helper, text, note))
        }
        Status::NeedsYou => {
            let text = signal_text?;
            helpers.iter().find_map(|h| {
                let prompt = h.prompt.as_deref()?;
                (waits::needs_you_line(&h.label(), prompt) == text)
                    .then(|| waits::needs_you_wait(h, prompt))
            })
        }
        _ => None,
    }
}

impl Helpers {
    /// The agent in charge of the ticket `ticket_id` came back in its conversation (Resume, Retry, or un-park; its
    /// session `in_charge` runs): each helper that was working or sent back resumes its own conversation, told to
    /// carry on (or given the user's answer that waited for it), and the questions still with the agent in charge are
    /// told to it again. Returns the pieces resumed.
    pub async fn resume_after(
        &self,
        ticket_id: &str,
        in_charge: &str,
    ) -> Result<Vec<u32>, BoardError> {
        let board = &self.inner.board;
        let pool = board.store().pool();
        let helpers = self.of_ticket(ticket_id).await?;
        if helpers.is_empty() {
            return Ok(Vec::new());
        }
        let ticket = board.ticket_by_id(ticket_id).await?;
        // Every prompt was its old session's: the tools ask again after a resume.
        store::clear_prompts(pool, ticket_id).await?;
        let lock = self.lock_of(ticket_id);
        let _one = lock.lock().await;
        let mut resumed = Vec::new();
        for helper in helpers.iter().filter(|h| h.resumes()) {
            if self.live_session(helper).is_some() {
                continue;
            }
            match self.resume_one(&ticket, helper).await {
                Ok(()) => resumed.push(helper.piece),
                Err(e) => {
                    tracing::warn!(ticket = %ticket.key, piece = helper.piece, error = %e, "a helper didn't resume");
                }
            }
        }
        self.tell_in_charge_again(&ticket, in_charge).await?;
        if !resumed.is_empty() {
            tracing::info!(ticket = %ticket.key, ?resumed, "helpers resumed");
        }
        Ok(resumed)
    }

    /// One helper back in its worktree: its conversation continued on the tool, model and agent it ran on (a new one
    /// when its tool never said its id), first told what waited for it, else to carry on.
    async fn resume_one(
        &self,
        ticket: &crate::board::tickets::Ticket,
        helper: &Helper,
    ) -> Result<(), BoardError> {
        let board = &self.inner.board;
        let pool = board.store().pool();
        let worktree = Path::new(&helper.worktree);
        if !worktree.join(".git").exists() {
            return Err(BoardError::refused(
                crate::board::Refusal::NoWorktree,
                format!("{}'s worktree is gone", helper.label()),
            ));
        }
        let project = board.project(ticket.key.project.as_str()).await?;
        // Its last session as the service knows it (in memory since a Pause or Park), else as stored (a restart).
        let last = match &helper.session {
            Some(session) => match self.inner.sessions.info(session) {
                Ok(info) => Some(info),
                Err(_) => self.inner.sessions.stored(session).await.ok(),
            },
            None => None,
        };
        let conversation = last.as_ref().and_then(StoredConversation::of);
        let prompt = helper
            .resume_prompt
            .clone()
            .unwrap_or_else(|| carry_on_text(helper.piece));
        let runs_on = RunsOn {
            tool: conversation
                .as_ref()
                .map(|c| c.tool)
                .or(last.as_ref().and_then(|s| s.tool)),
            model: conversation.as_ref().and_then(|c| c.model.clone()),
            effort: conversation.as_ref().and_then(|c| c.effort.clone()),
            agent: conversation
                .as_ref()
                .and_then(|c| c.agent.clone())
                .or_else(|| last.as_ref().and_then(|s| s.agent.clone())),
            fallback: last.as_ref().and_then(|s| s.fallback.clone()),
        };
        let agent = match &runs_on.agent {
            Some(agent) => board.store().agent(&agent.id).await?,
            None => None,
        };
        let agent = match agent {
            Some(agent) => agent,
            None => {
                crate::agents::resolve::session_agent(
                    board,
                    ticket,
                    crate::agents::SessionRole::Developer,
                )
                .await?
            }
        };
        let first = match &conversation {
            Some(_) => FirstPrompt::Text(prompt),
            None => {
                self.skill_prompt(
                    ticket,
                    format!(
                        "{} {prompt}",
                        super::first_prompt(&ticket.key, helper.piece, &helper.title)
                    ),
                )
                .await?
            }
        };
        // Its row names the new session before the program starts (an immediate `otaq` command finds it); a resume
        // that fails names the previous one again.
        let session_id = crate::store::new_id();
        store::set_session(pool, &helper.id, Some(&session_id)).await?;
        let launched = self
            .launch(
                ticket,
                &project,
                worktree,
                helper.piece,
                &helper.title,
                &runs_on,
                &agent,
                session_id,
                conversation.map(|c| c.id),
                first,
            )
            .await;
        let session = match launched {
            Ok(session) => session,
            Err(e) => {
                store::set_session(pool, &helper.id, helper.session.as_deref()).await?;
                return Err(e);
            }
        };
        store::resumed(pool, &helper.id, &session.id).await?;
        self.record(ticket, helper, "resumed", None).await?;
        Ok(())
    }

    /// The ticket's agent in charge was stopped with it (Park, a drag back out of the stage): every helper stops too and
    /// keeps its worktree and status, so resuming picks them up; Park returns their ports to the pool, as the ticket's.
    pub async fn stopped_with_ticket(
        &self,
        ticket_id: &str,
        free_ports: bool,
    ) -> Result<(), BoardError> {
        self.stop_all(ticket_id).await?;
        if free_ports {
            store::free_ports(self.inner.board.store().pool(), ticket_id).await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn texts() {
        assert_eq!(
            interrupted_line(2),
            "Interrupted \u{b7} 2 helpers were working"
        );
        assert_eq!(
            interrupted_line(1),
            "Interrupted \u{b7} 1 helper was working"
        );
        assert_eq!(interrupted_line(0), "Interrupted");
        assert_eq!(
            carry_on_text(3),
            "You were interrupted by a restart. Carry on with piece 3."
        );
    }
}
