//! The focused-terminal rule (ATT-03, d0042): while a ticket's terminal has focus, in the app or through
//! `otaq attach`, the user is looking at it. Its agent's Asks you / Needs you / Idle are still recorded on the
//! ticket, but they raise **no turquoise, no needs-you item and no OS notification**; the ticket shows a quiet "your
//! turn" instead ([`crate::domain::Flags::your_turn`]). When the last focused terminal lets go and the agent still
//! waits, the needs-you state and its needs-you item appear at that moment.
//!
//! Focus is tracked per **client connection** and session: the app reports focus gained and lost for its
//! terminal view, `otaq attach` counts as focused while attached, and a connection that closes loses every
//! focus it held. A ticket is focused while any client holds focus on one of its running sessions. Kept in the
//! service's memory only: after a restart no terminal has focus.
//!
//! The rule applies **per session** (d0063): a ticket's validator is a [`Side`] of its own. While the validator's
//! terminal has focus, "Needs you · validator" stays calm; looking at the agent's terminal (or the Shell) doesn't
//! calm it, and looking at the validator doesn't calm the agent's own waits ([`Focus::covers`]).
//!
//! **Looking at Otaq** (d0090) extends the rule from the focused terminal to the whole window, for OS notifications
//! only: while any app's window is focused (`app.focus`, per client connection), no OS notification goes out
//! ([`Looking`]). `otaq attach` doesn't report it, so it doesn't count.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use crate::domain::Status;

/// One client connection to the service, numbered by the service.
pub type ClientId = u64;

/// Who holds focus on which ticket. Cheap to clone; clones share the same state.
#[derive(Debug, Clone, Default)]
pub struct Focus {
    inner: Arc<Mutex<HashMap<String, HashSet<Holder>>>>,
}

/// Which of a ticket's sessions a terminal looks at: the validator, or the agent's side (the agent and the
/// ticket's other terminals).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Side {
    Agent,
    Validator,
}

/// A client's terminal for one session of a ticket.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Holder {
    session: String,
    client: ClientId,
    side: Side,
}

/// What a focus report changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FocusChange {
    /// Whether any client now holds focus on the session, and whether that changed.
    pub session_focused: bool,
    pub session_changed: bool,
    /// Whether the ticket's focus changed on the session's side (what its card and needs-you item depend on).
    pub ticket_changed: bool,
}

impl Focus {
    /// A client's terminal for `session` (of `ticket_id`, on `side`) gained or lost focus.
    pub fn set(
        &self,
        ticket_id: &str,
        session: &str,
        side: Side,
        client: ClientId,
        focused: bool,
    ) -> FocusChange {
        let mut all = self.inner.lock().expect("focus");
        let ticket_before = holds_side(&all, ticket_id, side);
        let session_before = holds_session(&all, session);
        let holder = Holder {
            session: session.to_owned(),
            client,
            side,
        };
        if focused {
            all.entry(ticket_id.to_owned()).or_default().insert(holder);
        } else if let Some(holders) = all.get_mut(ticket_id) {
            holders.remove(&holder);
            if holders.is_empty() {
                all.remove(ticket_id);
            }
        }
        let ticket_after = holds_side(&all, ticket_id, side);
        let session_after = holds_session(&all, session);
        FocusChange {
            session_focused: session_after,
            session_changed: session_before != session_after,
            ticket_changed: ticket_before != ticket_after,
        }
    }

    /// A client disconnected: it loses every focus it held. Returns the sessions that lost their last holder,
    /// with their ticket, and whether that ticket lost its focus too.
    pub fn client_gone(&self, client: ClientId) -> Vec<Released> {
        self.release(|h| h.client == client)
    }

    /// A session ended: nobody can be looking at it any more.
    pub fn session_gone(&self, session: &str) -> Vec<Released> {
        self.release(|h| h.session == session)
    }

    /// Whether any client holds focus on one of the ticket's sessions.
    pub fn is_focused(&self, ticket_id: &str) -> bool {
        self.inner
            .lock()
            .expect("focus")
            .get(ticket_id)
            .is_some_and(|h| !h.is_empty())
    }

    /// Whether any client holds focus on one of the ticket's sessions on `side`.
    pub fn side_focused(&self, ticket_id: &str, side: Side) -> bool {
        holds_side(&self.inner.lock().expect("focus"), ticket_id, side)
    }

    /// Whether the user is looking at the session that waits in `status` (ATT-03 per session): the validator's
    /// terminal for Needs you · validator, the agent's side for the agent's own waits, any terminal of the ticket
    /// for the validation and Auto-merge stops (no session waits there; the ticket is in front of the user).
    pub fn covers(&self, ticket_id: &str, status: Status) -> bool {
        match status {
            Status::CheckerNeedsYou => self.side_focused(ticket_id, Side::Validator),
            s if s.is_agent_waiting() => self.side_focused(ticket_id, Side::Agent),
            _ => self.is_focused(ticket_id),
        }
    }

    /// Whether any client holds focus on this session.
    pub fn session_focused(&self, session: &str) -> bool {
        holds_session(&self.inner.lock().expect("focus"), session)
    }

    fn release(&self, drop_it: impl Fn(&Holder) -> bool) -> Vec<Released> {
        let mut all = self.inner.lock().expect("focus");
        let mut released: Vec<Released> = Vec::new();
        for (ticket_id, holders) in all.iter_mut() {
            let dropped: Vec<Holder> = holders.iter().filter(|h| drop_it(h)).cloned().collect();
            if dropped.is_empty() {
                continue;
            }
            for holder in &dropped {
                holders.remove(holder);
            }
            let mut sessions: Vec<(String, Side)> =
                dropped.into_iter().map(|h| (h.session, h.side)).collect();
            sessions.sort_by(|a, b| a.0.cmp(&b.0));
            sessions.dedup();
            for (session, side) in sessions {
                if !holders.iter().any(|h| h.session == session) {
                    released.push(Released {
                        ticket_id: ticket_id.clone(),
                        session,
                        ticket_lost_focus: !holders.iter().any(|h| h.side == side),
                    });
                }
            }
        }
        all.retain(|_, holders| !holders.is_empty());
        released
    }
}

/// A session that lost its last focus holder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Released {
    pub ticket_id: String,
    pub session: String,
    /// No terminal on the session's side of the ticket has focus any more.
    pub ticket_lost_focus: bool,
}

fn holds_side(all: &HashMap<String, HashSet<Holder>>, ticket_id: &str, side: Side) -> bool {
    all.get(ticket_id)
        .is_some_and(|h| h.iter().any(|h| h.side == side))
}

fn holds_session(all: &HashMap<String, HashSet<Holder>>, session: &str) -> bool {
    all.values().flatten().any(|h| h.session == session)
}

/// The app clients whose window is focused (in front, with keyboard focus): while any is, the user is looking at
/// Otaq (d0090). A closed connection loses it. Cheap to clone; clones share the same state. Kept in memory only.
#[derive(Debug, Clone, Default)]
pub struct Looking {
    clients: Arc<Mutex<HashSet<ClientId>>>,
}

impl Looking {
    /// A client's window gained or lost focus. Returns whether anyone looking changed.
    pub fn set(&self, client: ClientId, focused: bool) -> bool {
        let mut clients = self.clients.lock().expect("looking");
        let before = !clients.is_empty();
        if focused {
            clients.insert(client);
        } else {
            clients.remove(&client);
        }
        before != !clients.is_empty()
    }

    /// A client disconnected: its window doesn't count any more.
    pub fn client_gone(&self, client: ClientId) {
        self.set(client, false);
    }

    /// Whether any client's window is focused.
    pub fn any(&self) -> bool {
        !self.clients.lock().expect("looking").is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// T-0371 (d0090): looking while any app window is focused; a closed connection stops looking.
    #[test]
    fn looking_while_any_window_is_focused() {
        let looking = Looking::default();
        assert!(!looking.any());
        assert!(looking.set(1, true));
        assert!(!looking.set(2, true), "already looking");
        assert!(!looking.set(1, false));
        assert!(looking.any());
        looking.client_gone(2);
        assert!(!looking.any());
        assert!(!looking.set(3, false));
    }

    #[test]
    fn a_ticket_stays_focused_while_any_client_holds_it() {
        let focus = Focus::default();
        let app = focus.set("t1", "s1", Side::Agent, 1, true);
        assert!(app.ticket_changed && app.session_changed && app.session_focused);
        let attach = focus.set("t1", "s1", Side::Agent, 2, true);
        assert!(!attach.ticket_changed && !attach.session_changed);
        // The app leaves; attach still looks at it.
        let left = focus.set("t1", "s1", Side::Agent, 1, false);
        assert!(!left.ticket_changed && left.session_focused);
        assert!(focus.is_focused("t1"));
        let gone = focus.client_gone(2);
        assert_eq!(
            gone,
            vec![Released {
                ticket_id: "t1".into(),
                session: "s1".into(),
                ticket_lost_focus: true,
            }]
        );
        assert!(!focus.is_focused("t1"));
        assert!(!focus.session_focused("s1"));
    }

    #[test]
    fn repeated_reports_change_nothing() {
        let focus = Focus::default();
        focus.set("t1", "s1", Side::Agent, 1, true);
        assert_eq!(
            focus.set("t1", "s1", Side::Agent, 1, true),
            FocusChange {
                session_focused: true,
                session_changed: false,
                ticket_changed: false,
            }
        );
        focus.set("t1", "s1", Side::Agent, 1, false);
        assert!(!focus.set("t1", "s1", Side::Agent, 1, false).ticket_changed);
        assert!(focus.client_gone(1).is_empty());
    }

    #[test]
    fn an_ended_session_releases_its_ticket() {
        let focus = Focus::default();
        focus.set("t1", "s1", Side::Agent, 1, true);
        focus.set("t2", "s2", Side::Agent, 1, true);
        let gone = focus.session_gone("s1");
        assert_eq!(gone.len(), 1);
        assert!(gone[0].ticket_lost_focus);
        assert!(!focus.is_focused("t1"));
        assert!(focus.is_focused("t2"));
    }

    /// d0063: the validator is a side of its own. Looking at the agent doesn't calm the validator's prompt, and
    /// looking at the validator doesn't calm the agent's question; the stops calm with any terminal.
    #[test]
    fn the_rule_applies_per_session() {
        let focus = Focus::default();
        let agent = focus.set("t1", "agent", Side::Agent, 1, true);
        assert!(agent.ticket_changed);
        assert!(!focus.covers("t1", Status::CheckerNeedsYou));
        assert!(focus.covers("t1", Status::AsksYou));
        assert!(focus.covers("t1", Status::CheckFailed));

        // The same client moves to the validator: the ticket's validator side changes.
        assert!(
            focus
                .set("t1", "agent", Side::Agent, 1, false)
                .ticket_changed
        );
        let validator = focus.set("t1", "validator", Side::Validator, 1, true);
        assert!(validator.ticket_changed && validator.session_focused);
        assert!(focus.covers("t1", Status::CheckerNeedsYou));
        assert!(!focus.covers("t1", Status::NeedsYou));
        assert!(focus.covers("t1", Status::DeliveryStopped));

        // A second client on the agent while the first looks at the validator: both sides count.
        assert!(
            focus
                .set("t1", "agent", Side::Agent, 2, true)
                .ticket_changed
        );
        let gone = focus.client_gone(1);
        assert_eq!(
            gone,
            vec![Released {
                ticket_id: "t1".into(),
                session: "validator".into(),
                ticket_lost_focus: true,
            }]
        );
        assert!(!focus.covers("t1", Status::CheckerNeedsYou));
        assert!(focus.covers("t1", Status::Idle));
    }
}
