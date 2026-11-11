//! The permission prompts on the screens of tickets' own sessions now (the ticket's agent, its delivery session, a
//! finished ticket's answer session), and which of them read Needs you while the ticket keeps its status.
//!
//! The signals keep every session's open prompts and the one on screen ([`crate::signals::asks`]); for a ticket's
//! own session they mirror the one on screen here, where the board reads it as it loads a ticket. Every prompt drawn
//! lands in [`crate::domain::Flags::prompts_drawn_in`] (its session's header reads "Waiting for you"). One that comes
//! in a turn the user is to be brought to while the ticket waits where it is becomes the ticket's
//! [`crate::domain::SessionPrompt`]: a ticket's session prompt, shown without changing its status
//! ([`shown_without_status`]). The message writer's prompt never does: Review stays calm and the Approve dialogs say
//! so.
//!
//! In memory, like the open prompts: a session that ends takes its prompt with it, and a restart shows none.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::domain::{PromptTurn, Status};
use crate::pr::flow::{FlowKind, FlowState};
use crate::session::SessionKind;
use crate::store::now_ms;

/// The prompt on one session's screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DrawnPrompt {
    pub ticket_id: String,
    pub session: String,
    pub kind: SessionKind,
    pub prompt: String,
    /// When this prompt was drawn.
    pub since: i64,
}

/// Every ticket session's prompt on screen, by session. Cheap to clone; clones share the same state (the board's and
/// the signals').
#[derive(Debug, Clone, Default)]
pub struct SessionPrompts {
    inner: Arc<Mutex<HashMap<String, DrawnPrompt>>>,
}

impl SessionPrompts {
    /// `session` (of `kind`, the ticket `ticket_id`'s) shows `prompt` now. A prompt already shown keeps its time.
    pub fn drawn(&self, ticket_id: &str, session: &str, kind: SessionKind, prompt: &str) {
        let mut all = self.lock();
        if all
            .get(session)
            .is_some_and(|p| p.prompt == prompt && p.ticket_id == ticket_id)
        {
            return;
        }
        all.insert(
            session.to_owned(),
            DrawnPrompt {
                ticket_id: ticket_id.to_owned(),
                session: session.to_owned(),
                kind,
                prompt: prompt.to_owned(),
                since: now_ms(),
            },
        );
    }

    /// `session` shows no prompt any more (answered, its turn went on, or it ended): the ticket it showed for, if any.
    pub fn gone(&self, session: &str) -> Option<String> {
        self.lock().remove(session).map(|p| p.ticket_id)
    }

    /// The prompt on `session`'s screen, if any.
    pub fn on_screen(&self, session: &str) -> Option<String> {
        self.lock().get(session).map(|p| p.prompt.clone())
    }

    /// The ticket's sessions' prompts on screen, oldest first.
    pub fn of_ticket(&self, ticket_id: &str) -> Vec<DrawnPrompt> {
        let mut out: Vec<DrawnPrompt> = self
            .lock()
            .values()
            .filter(|p| p.ticket_id == ticket_id)
            .cloned()
            .collect();
        out.sort_by(|a, b| a.since.cmp(&b.since).then(a.session.cmp(&b.session)));
        out
    }

    pub fn is_empty(&self) -> bool {
        self.lock().is_empty()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, DrawnPrompt>> {
        self.inner.lock().expect("session prompts")
    }
}

/// A ticket's PR flow as [`shown_without_status`] reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlowUnderWay {
    pub kind: FlowKind,
    pub state: FlowState,
    pub automatic: bool,
}

/// The turn whose prompt, on a session of `kind`, reads Needs you while the ticket keeps `status`; `None` when the
/// prompt shows only in its session (and wherever its own turn says so).
///
/// A delivery skill turn the user started (Push to PR, its Retry, Open PR once confirmed) or one that runs by itself
/// (an automatic PR) brings the user to its prompt: nothing else would, and the agent waits until someone looks. The
/// message writer's turn doesn't: it runs while Review waits with no PR flow, or while a clicked Open PR's dialog
/// shows its proposal before the confirm, and its prompt is met in the Approve dialogs.
pub fn shown_without_status(
    status: Status,
    kind: SessionKind,
    flow: Option<FlowUnderWay>,
) -> Option<PromptTurn> {
    if !matches!(kind, SessionKind::Agent | SessionKind::Delivery)
        || !matches!(
            status,
            Status::WaitingForYou | Status::ToDeliver | Status::PrOpen
        )
    {
        return None;
    }
    let flow = flow?;
    match flow.kind {
        FlowKind::Push => Some(PromptTurn::PushToPr),
        // A clicked Open PR runs its skill only once the user confirmed: before that, the delivery's agent may
        // still be writing the message the dialog opened on.
        FlowKind::Open if flow.automatic || flow.state == FlowState::Pushing => {
            Some(PromptTurn::OpenPr)
        }
        FlowKind::Open => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flow(kind: FlowKind, state: FlowState, automatic: bool) -> Option<FlowUnderWay> {
        Some(FlowUnderWay {
            kind,
            state,
            automatic,
        })
    }

    /// Push to PR, its Retry and an automatic PR read Needs you at every step of their flow; a clicked Open PR only
    /// once confirmed; the message writer (no flow, or an Open PR's dialog before the confirm) never.
    #[test]
    fn which_turns_read_needs_you() {
        use FlowKind::{Open, Push};
        use FlowState::{Preparing, Proposed, Pushing};
        let review = Status::WaitingForYou;
        let agent = SessionKind::Agent;
        for state in [Preparing, Proposed, Pushing] {
            assert_eq!(
                shown_without_status(review, agent, flow(Push, state, false)),
                Some(PromptTurn::PushToPr),
                "{state:?}"
            );
            assert_eq!(
                shown_without_status(
                    Status::ToDeliver,
                    SessionKind::Delivery,
                    flow(Open, state, true)
                ),
                Some(PromptTurn::OpenPr),
                "automatic {state:?}"
            );
        }
        assert_eq!(
            shown_without_status(review, agent, flow(Open, Pushing, false)),
            Some(PromptTurn::OpenPr)
        );
        for state in [Preparing, Proposed] {
            assert_eq!(
                shown_without_status(review, agent, flow(Open, state, false)),
                None,
                "the writer, {state:?}"
            );
        }
        assert_eq!(
            shown_without_status(review, agent, None),
            None,
            "the writer"
        );
    }

    /// Only a ticket waiting where it is, and only its agent's or delivery's session: a working ticket's prompt reads
    /// Needs you through its status, a checker's or a helper's through their own.
    #[test]
    fn only_a_waiting_tickets_agent_or_delivery_session() {
        let push = flow(FlowKind::Push, FlowState::Preparing, false);
        assert_eq!(
            shown_without_status(Status::Working, SessionKind::Agent, push),
            None
        );
        assert_eq!(
            shown_without_status(Status::Merged, SessionKind::Agent, push),
            None
        );
        for kind in [
            SessionKind::Validator,
            SessionKind::Helper,
            SessionKind::Answer,
            SessionKind::Shell,
        ] {
            assert_eq!(
                shown_without_status(Status::WaitingForYou, kind, push),
                None,
                "{kind:?}"
            );
        }
    }

    #[test]
    fn a_prompt_redrawn_keeps_its_time_and_goes_with_its_session() {
        let p = SessionPrompts::default();
        p.drawn("t1", "s1", SessionKind::Agent, "git push");
        let since = p.of_ticket("t1")[0].since;
        std::thread::sleep(std::time::Duration::from_millis(2));
        p.drawn("t1", "s1", SessionKind::Agent, "git push");
        assert_eq!(p.of_ticket("t1")[0].since, since);
        assert_eq!(p.on_screen("s1").as_deref(), Some("git push"));
        assert_eq!(p.gone("s1").as_deref(), Some("t1"));
        assert!(p.of_ticket("t1").is_empty());
        assert_eq!(p.gone("s1"), None);
    }
}
