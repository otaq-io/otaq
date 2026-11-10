//! Counts of agents for the "Service update ready · Restart now (3 agents working, 2 waiting)" banner (d0022) and
//! for the question Quit Otaq asks while agents work. Computed from stored state; zero until sessions exist (M3).

use std::collections::HashSet;

use super::{Store, StoreError};
use crate::domain::Status;
use crate::session::SessionKind;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AgentCounts {
    pub working: u32,
    pub waiting: u32,
}

impl Store {
    /// Tickets with a live agent session (not ended), once each, split into waiting for the user (the needs-you
    /// family) and working. Working follows the same rule as Running and the tray's "1 agent running"
    /// ([`SessionKind::at_work_on`]): a ticket's checker during its check, a helper, a delivery or answer session
    /// count as well as the ticket's own agent, and a session kept while the ticket waits for your review doesn't.
    /// So Quit Otaq asks whenever the tray says an agent runs.
    pub async fn agent_counts(&self) -> Result<AgentCounts, StoreError> {
        let rows: Vec<(String, String, String, bool)> = sqlx::query_as(
            "SELECT t.id, t.state, s.kind, EXISTS (SELECT 1 FROM pr_flows f WHERE f.ticket_id = t.id)
             FROM sessions s JOIN tickets t ON t.id = s.ticket_id
             WHERE s.ended_at IS NULL",
        )
        .fetch_all(self.pool())
        .await?;
        let mut working = HashSet::new();
        let mut waiting = HashSet::new();
        for (id, state, kind, delivering) in rows {
            let (Ok(status), Ok(kind)) = (state.parse::<Status>(), kind.parse::<SessionKind>())
            else {
                continue;
            };
            if !kind.runs_agent_tool() {
                continue;
            }
            if status.is_needs_you_family() {
                waiting.insert(id);
            } else if kind.at_work_on(status, delivering) {
                working.insert(id);
            }
        }
        let to_u32 = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
        Ok(AgentCounts {
            working: to_u32(working.len()),
            waiting: to_u32(waiting.len()),
        })
    }
}
