//! "Waiting for the agent to pick it up" (d0118, finding 19 of the 2026-10-24 dogfood report).
//!
//! A hand-off (Send back's `address-review`, a merge conflict's `restack`, Open PR, Push) typed into a live session
//! may not reach the agent: its tool may not run it. From the moment it is **typed** (not while it waits as
//! "1 message waiting", and never for a first prompt at launch) the ticket carries a pending mark, shown on the card
//! and the sheet in place of the working line, until the agent's tool reports activity: a hook from that ticket's
//! session that says where its turn is (the prompt submitted, a turn ending, a prompt for the user; never the status
//! line's model, tokens or limit windows), or an `otaq` command the agent runs itself (not the status line's
//! bookkeeping, d0119). There is no timer (d0086). While it stays, Send again types the same text once more as the
//! user's click (d0048).
//!
//! The marks live in memory: a restart ends every session, and Resume sends the hand-off as a first prompt.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::domain::HandOffKind;

/// A hand-off and the whole line typed for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandOff {
    pub kind: HandOffKind,
    pub text: String,
}

impl HandOff {
    pub fn new(kind: HandOffKind, text: impl Into<String>) -> Self {
        Self {
            kind,
            text: text.into(),
        }
    }
}

/// A ticket's hand-off typed into `session` that its agent hasn't picked up yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pending {
    pub session: String,
    pub hand_off: HandOff,
}

/// Every ticket's pending hand-off, and the queued deliveries that are hand-offs. Cheap to clone; clones share the
/// same state (the board's, the deliveries' and the signals').
#[derive(Debug, Clone, Default)]
pub struct HandOffs {
    inner: Arc<Mutex<Inner>>,
}

#[derive(Debug, Default)]
struct Inner {
    /// By ticket id.
    pending: HashMap<String, Pending>,
    /// By `pending_deliveries.id`: a hand-off waiting for the agent's turn to end, marked once typed.
    queued: HashMap<String, HandOff>,
}

impl HandOffs {
    /// The kind of the ticket's pending hand-off, if it has one.
    pub fn kind_of(&self, ticket_id: &str) -> Option<HandOffKind> {
        self.pending(ticket_id).map(|p| p.hand_off.kind)
    }

    /// The ticket's pending hand-off, if it has one.
    pub fn pending(&self, ticket_id: &str) -> Option<Pending> {
        self.lock().pending.get(ticket_id).cloned()
    }

    /// The hand-off was typed into the ticket's `session`: pending until the agent picks it up.
    pub(crate) fn typed(&self, ticket_id: &str, session: &str, hand_off: HandOff) {
        self.lock().pending.insert(
            ticket_id.to_owned(),
            Pending {
                session: session.to_owned(),
                hand_off,
            },
        );
    }

    /// The queued delivery `delivery` carries a hand-off: it becomes pending once typed.
    pub(crate) fn queued(&self, delivery: &str, hand_off: HandOff) {
        self.lock().queued.insert(delivery.to_owned(), hand_off);
    }

    /// The hand-off a queued delivery carries, now that it is typed.
    pub(crate) fn take_queued(&self, delivery: &str) -> Option<HandOff> {
        self.lock().queued.remove(delivery)
    }

    /// The ticket's agent reported activity: its pending hand-off (if any) is picked up. Whether it had one.
    pub fn picked_up(&self, ticket_id: &str) -> bool {
        self.lock().pending.remove(ticket_id).is_some()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().expect("hand-offs")
    }
}
