//! The Merge checks running now (d0120, Q475): the card's "Merging · running the merge check" and its Stop. Kept in
//! memory only: a check never outlives the service (its process ends with it), so a restart shows none.
//!
//! [`crate::review::Review`] runs the check inside Local merge ([`crate::git::merge`]), one merge at a time per
//! service; [`MergeChecks::begin`] marks the ticket while it runs and [`MergeChecks::stop`] is the user's Stop.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use tokio::sync::oneshot;

use super::tickets::Ticket;
use crate::protocol::Event;
use crate::service::EventBus;

/// The tickets whose Merge check runs now. Cheap to clone.
#[derive(Debug, Clone, Default)]
pub struct MergeChecks {
    running: Arc<Mutex<HashMap<String, Option<oneshot::Sender<()>>>>>,
    /// Told when a check starts or ends, so the card changes at once; the service sets it.
    events: Arc<OnceLock<EventBus>>,
}

impl MergeChecks {
    /// Sets where a check's start and end are published (the service's event bus).
    pub fn set_events(&self, events: EventBus) {
        let _ = self.events.set(events);
    }

    /// Whether the ticket with this id is running its Merge check now.
    pub fn is_running(&self, ticket_id: &str) -> bool {
        self.running
            .lock()
            .expect("merge checks")
            .contains_key(ticket_id)
    }

    /// Marks `ticket`'s check as running until the returned guard is dropped; the receiver fires on the user's
    /// Stop.
    pub(crate) fn begin(&self, ticket: &Ticket) -> (Running, oneshot::Receiver<()>) {
        let (stop, stopped) = oneshot::channel();
        self.running
            .lock()
            .expect("merge checks")
            .insert(ticket.id.clone(), Some(stop));
        self.changed(ticket);
        let running = Running {
            checks: self.clone(),
            ticket: ticket.clone(),
        };
        (running, stopped)
    }

    /// The user's Stop of a running check (Q475): the command ends, nothing is merged and the ticket stays where it
    /// was. `false` when the ticket runs none.
    pub fn stop(&self, ticket_id: &str) -> bool {
        let stop = self
            .running
            .lock()
            .expect("merge checks")
            .get_mut(ticket_id)
            .and_then(Option::take);
        match stop {
            Some(stop) => {
                let _ = stop.send(());
                true
            }
            None => false,
        }
    }

    fn changed(&self, ticket: &Ticket) {
        if let Some(events) = self.events.get() {
            events.publish(Event::TicketChanged {
                project: ticket.key.project.to_string(),
                id: ticket.id.clone(),
                key: ticket.key.clone(),
            });
        }
    }
}

/// A running check's mark ([`MergeChecks::begin`]): dropping it ends "Merging · running the merge check".
pub(crate) struct Running {
    checks: MergeChecks,
    ticket: Ticket,
}

impl Drop for Running {
    fn drop(&mut self) {
        self.checks
            .running
            .lock()
            .expect("merge checks")
            .remove(&self.ticket.id);
        self.checks.changed(&self.ticket);
    }
}
