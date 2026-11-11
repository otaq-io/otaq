//! Otaq's own git operations running on a ticket now (Approve & merge, a start): the step git is really on and the
//! user's hook that runs, for the card and the Merging dialog ("Merging · running your pre-commit hook · 40 s").
//! Kept in memory only: an operation never outlives the service, so a restart shows none.
//!
//! [`GitSteps::begin`] marks the ticket and gives the [`GitWatch`] its `git` reports to
//! ([`crate::board::Board::ticket_git`]); every new step and every hook that starts or ends tells clients the ticket
//! changed, so the card and the dialog follow it with the dialog closed or open. The guard ends it. Nothing here stops
//! a step or sets it a time limit: a slow hook runs as long as it takes.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use super::tickets::Ticket;
use crate::domain::{GitHookRunning, GitOperation, GitProgress};
use crate::git::{GitWatch, shorten_home};
use crate::protocol::Event;
use crate::service::EventBus;

/// The tickets running one of Otaq's git operations now. Cheap to clone.
#[derive(Debug, Clone, Default)]
pub struct GitSteps {
    running: Arc<Mutex<HashMap<String, Operation>>>,
    /// Told on every change, so the card changes at once; the service sets it.
    events: Arc<OnceLock<EventBus>>,
}

#[derive(Debug)]
struct Operation {
    operation: GitOperation,
    target: String,
    watch: GitWatch,
}

impl GitSteps {
    /// Sets where the changes are published (the service's event bus).
    pub fn set_events(&self, events: EventBus) {
        let _ = self.events.set(events);
    }

    /// Marks `ticket` as running `operation` (into or from `target`) until the returned guard is dropped. The guard's
    /// [`Running::watch`] is the one its `git` reports to.
    pub(crate) fn begin(&self, ticket: &Ticket, operation: GitOperation, target: &str) -> Running {
        let watch = GitWatch::new();
        self.running.lock().expect("git steps").insert(
            ticket.id.clone(),
            Operation {
                operation,
                target: target.to_owned(),
                watch: watch.clone(),
            },
        );
        self.changed(ticket);
        // Every new step and every hook that starts or ends: the ticket changed.
        let mut changes = watch.subscribe();
        let (steps, followed) = (self.clone(), ticket.clone());
        let follow = tokio::spawn(async move {
            while changes.changed().await.is_ok() {
                steps.changed(&followed);
            }
        });
        Running {
            steps: self.clone(),
            ticket: ticket.clone(),
            watch,
            follow,
        }
    }

    /// The watch of the operation the ticket with this id runs now, if any.
    pub fn watch(&self, ticket_id: &str) -> Option<GitWatch> {
        self.running
            .lock()
            .expect("git steps")
            .get(ticket_id)
            .map(|o| o.watch.clone())
    }

    /// Where the operation the ticket with this id runs now is, if it runs one.
    pub fn progress(&self, ticket_id: &str) -> Option<GitProgress> {
        let running = self.running.lock().expect("git steps");
        let op = running.get(ticket_id)?;
        let activity = op.watch.activity();
        Some(GitProgress {
            operation: op.operation,
            target: op.target.clone(),
            step: activity.step,
            earlier: activity.earlier,
            hook: activity.hook.map(|hook| GitHookRunning {
                name: hook.name,
                folder: shorten_home(&hook.folder.display().to_string()),
                since_ms: hook.since_ms,
            }),
        })
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

/// A running operation's mark ([`GitSteps::begin`]): dropping it ends what the card says about it.
pub(crate) struct Running {
    steps: GitSteps,
    ticket: Ticket,
    watch: GitWatch,
    follow: tokio::task::JoinHandle<()>,
}

impl Running {
    /// The watch the operation's `git` reports to.
    pub(crate) fn watch(&self) -> &GitWatch {
        &self.watch
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        self.follow.abort();
        self.steps
            .running
            .lock()
            .expect("git steps")
            .remove(&self.ticket.id);
        self.steps.changed(&self.ticket);
    }
}
