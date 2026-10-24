//! A ticket's dependency chain (DEP-05; d0006, d0049): the board's chain highlight, what a queued ticket waits for,
//! and the text of a ticket without links. `otaq dep tree` prints `Board::dependency_tree` (each ticket once, #9).
//!
//! The chain is the ticket's parents and children, transitively.

use std::collections::HashSet;

use super::readiness::readiness;
use super::tickets::Ticket;
use crate::domain::{Status, TicketKey};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chain {
    pub members: HashSet<TicketKey>,
    /// (parent, child) for every link between members.
    pub edges: Vec<(TicketKey, TicketKey)>,
}

/// The chain through `key`, or `None` when the ticket has no links (nothing to highlight).
pub fn chain(key: &TicketKey, tickets: &[Ticket]) -> Option<Chain> {
    let parents_of = |k: &TicketKey| -> Vec<TicketKey> {
        tickets
            .iter()
            .find(|t| &t.key == k)
            .map(|t| t.depends_on.clone())
            .unwrap_or_default()
    };
    let children_of = |k: &TicketKey| -> Vec<TicketKey> {
        tickets
            .iter()
            .filter(|t| t.depends_on.contains(k))
            .map(|t| t.key.clone())
            .collect()
    };
    let mut members: HashSet<TicketKey> = HashSet::from([key.clone()]);
    for walk in [
        &parents_of as &dyn Fn(&TicketKey) -> Vec<TicketKey>,
        &children_of,
    ] {
        let mut stack = vec![key.clone()];
        let mut seen = HashSet::from([key.clone()]);
        while let Some(at) = stack.pop() {
            for next in walk(&at) {
                if seen.insert(next.clone()) {
                    members.insert(next.clone());
                    stack.push(next);
                }
            }
        }
    }
    if members.len() < 2 {
        return None;
    }
    let mut edges: Vec<(TicketKey, TicketKey)> = tickets
        .iter()
        .filter(|t| members.contains(&t.key))
        .flat_map(|t| {
            t.depends_on
                .iter()
                .filter(|p| members.contains(p))
                .map(|p| (p.clone(), t.key.clone()))
                .collect::<Vec<_>>()
        })
        .collect();
    edges.sort();
    Some(Chain { members, edges })
}

/// What a ticket in Dev · Queued waits for, as its card says it: the readiness reason while it waits for its
/// parents or a closed parent blocks it, "waiting for a slot" while the agent limit is full (T-0083). `None`
/// for tickets that aren't Queued, or are ready to start.
pub fn queue_reason(ticket: &Ticket, tickets: &[Ticket]) -> Option<String> {
    match ticket.status {
        Status::Queued | Status::ParentClosed => readiness(ticket, tickets).reason(),
        Status::WaitingForSlot => Some("waiting for a slot".into()),
        _ => None,
    }
}

/// "No dependencies · nothing waits on SHOP-12": the Stack list of a ticket without links.
pub fn empty_text(key: &TicketKey) -> String {
    format!("No dependencies · nothing waits on {key}")
}
