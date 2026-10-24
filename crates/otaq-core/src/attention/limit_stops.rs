//! Agents stopped by a tool's usage limit raise no needs-you items while that tool's limit line is up (ATT-09, d0046;
//! per tool from M23, USG-10): one banner line for the situation, not one item per agent (P6).
//!
//! The switches and the marks live in the service's memory. The plan-limit work turns a tool's switch on while its
//! limit is active and marks the agents that limit stopped, with the tool; a ticket in **Waiting for limit reset**
//! that isn't marked counts as stopped by any active limit. The needs-you items (and so Needs you, its counts and
//! back-pressure) leave those tickets out; another tool's limit-stopped agents still raise theirs.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use crate::agent_tools::Tool;
use crate::board::tickets::Ticket;
use crate::domain::Status;

/// The switches and the marked tickets. Cheap to clone; clones share the same state.
#[derive(Debug, Clone, Default)]
pub struct LimitStops {
    inner: Arc<Mutex<Suppression>>,
}

/// A snapshot of [`LimitStops`], applied when deriving needs-you items.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Suppression {
    /// The tools whose limit line is up: their limit-stopped agents raise no items.
    pub limited: HashSet<Tool>,
    /// Tickets whose agent a tool's limit stopped (by id), with that tool.
    pub marked: HashMap<String, Tool>,
}

impl Suppression {
    /// Whether the ticket's item is left out.
    pub fn hides(&self, ticket: &Ticket) -> bool {
        match self.marked.get(&ticket.id) {
            Some(tool) => self.limited.contains(tool),
            None => ticket.status == Status::WaitingForLimitReset && !self.limited.is_empty(),
        }
    }
}

impl LimitStops {
    /// Turns `tool`'s switch on or off; returns whether it changed.
    pub fn set_suppress(&self, tool: Tool, on: bool) -> bool {
        let mut state = self.inner.lock().expect("limit stops");
        if on {
            state.limited.insert(tool)
        } else {
            state.limited.remove(&tool)
        }
    }

    /// Marks a ticket's agent as stopped by `tool`'s limit, or (`None`) not any more.
    pub fn mark(&self, ticket_id: &str, by: Option<Tool>) {
        let mut state = self.inner.lock().expect("limit stops");
        match by {
            Some(tool) => {
                state.marked.insert(ticket_id.to_owned(), tool);
            }
            None => {
                state.marked.remove(ticket_id);
            }
        }
    }

    /// The tickets marked as stopped by `tool`'s limit.
    pub fn marked_by(&self, tool: Tool) -> Vec<String> {
        let state = self.inner.lock().expect("limit stops");
        let mut ids: Vec<String> = state
            .marked
            .iter()
            .filter(|(_, t)| **t == tool)
            .map(|(id, _)| id.clone())
            .collect();
        ids.sort();
        ids
    }

    pub fn snapshot(&self) -> Suppression {
        self.inner.lock().expect("limit stops").clone()
    }
}
