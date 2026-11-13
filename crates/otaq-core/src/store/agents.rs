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
    /// family) and working. Working is the one count of Running, the rail and the tray's "1 agent running"
    /// ([`SessionKind::works_now`]): a ticket's checker during its check, a helper, a delivery or answer session
    /// count as well as the ticket's own agent, and a session kept while the ticket waits for your review doesn't.
    /// So Quit Otaq asks whenever the tray says an agent runs.
    pub async fn agent_counts(&self) -> Result<AgentCounts, StoreError> {
        let mut working = HashSet::new();
        let mut waiting = HashSet::new();
        for (id, status, kind, delivering) in self.live_ticket_sessions().await? {
            if !kind.runs_agent_tool() {
                continue;
            }
            if kind.works_now(status, delivering) {
                working.insert(id);
            } else if status.is_needs_you_family() {
                waiting.insert(id);
            }
        }
        let to_u32 = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
        Ok(AgentCounts {
            working: to_u32(working.len()),
            waiting: to_u32(waiting.len()),
        })
    }

    /// The ids of the tickets with an agent working now ([`SessionKind::works_now`]), every project's: what the
    /// tray's Running list shows, so it counts what Running in the title bar and Quit Otaq's question count.
    pub async fn working_tickets(&self) -> Result<HashSet<String>, StoreError> {
        Ok(self
            .live_ticket_sessions()
            .await?
            .into_iter()
            .filter(|(_, status, kind, delivering)| kind.works_now(*status, *delivering))
            .map(|(id, ..)| id)
            .collect())
    }

    /// Every live session of a ticket: the ticket's id and status, the session's kind and whether the ticket
    /// delivers now (a PR flow of it exists). Rows this build can't read are left out.
    async fn live_ticket_sessions(
        &self,
    ) -> Result<Vec<(String, Status, SessionKind, bool)>, StoreError> {
        let rows: Vec<(String, String, String, bool)> = sqlx::query_as(
            "SELECT t.id, t.state, s.kind, EXISTS (SELECT 1 FROM pr_flows f WHERE f.ticket_id = t.id)
             FROM sessions s JOIN tickets t ON t.id = s.ticket_id
             WHERE s.ended_at IS NULL",
        )
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .filter_map(|(id, state, kind, delivering)| {
                Some((id, state.parse().ok()?, kind.parse().ok()?, delivering))
            })
            .collect())
    }
}
