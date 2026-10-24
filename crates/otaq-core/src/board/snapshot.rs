//! What the app's board and project switcher read: a project's columns with sorted cards, and per-project
//! counts.

use serde::{Deserialize, Serialize};

use super::needs_you::{ProjectTickets, needs_you_items_with};
use super::order::sort_column;
use super::tickets::Ticket;
use super::{Board, BoardError};
use crate::domain::{StatusSet, TicketKey};
use crate::store::now_ms;
use crate::store::projects::Project;

/// A project as the switcher shows it (UI-01).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectInfo {
    pub key: String,
    pub name: String,
    pub repo_path: String,
    pub base_branch: String,
    /// Agents working right now (live agent sessions).
    pub agents_working: u32,
    /// The project's needs-you items, the needs-you count of the project switcher (ATT-10): needs-you family,
    /// Interrupted, Start interrupted, Resume failed, Parent closed, Ready for Review.
    pub needs_you: u32,
    /// Since when it is in Flow setup (ASK-04): it shows no board ("pocket-api · Flow setup"); `None` once its flow
    /// is settled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flow_setup_since: Option<i64>,
    /// Since when it is paused (PAU-01, d0112): nothing starts an agent there on its own until the user's Resume.
    /// `None` while it runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paused_since: Option<i64>,
}

/// One group of cards, in board order (SDD §7.5): the statuses of one of [`StatusSet::BOARD`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColumnSnapshot {
    pub column: StatusSet,
    pub cards: Vec<Ticket>,
}

/// A project's board.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoardSnapshot {
    pub project: ProjectInfo,
    /// The six groups, left to right (the board draws the human check and the Delivery as one Review column,
    /// d0087, d0108).
    pub columns: Vec<ColumnSnapshot>,
}

impl BoardSnapshot {
    /// Every ticket on the board, in no particular order.
    pub fn tickets(&self) -> impl Iterator<Item = &Ticket> {
        self.columns.iter().flat_map(|c| c.cards.iter())
    }

    pub fn ticket(&self, key: &TicketKey) -> Option<&Ticket> {
        self.tickets().find(|t| &t.key == key)
    }
}

/// Builds the columns from a project's tickets.
pub fn columns(tickets: &[Ticket]) -> Vec<ColumnSnapshot> {
    StatusSet::BOARD
        .iter()
        .map(|column| {
            let order = sort_column(*column, tickets);
            let pick = |keys: &[TicketKey]| {
                keys.iter()
                    .filter_map(|k| tickets.iter().find(|t| &t.key == k).cloned())
                    .collect()
            };
            ColumnSnapshot {
                column: *column,
                cards: pick(&order.cards),
            }
        })
        .collect()
}

impl Board {
    pub async fn board_snapshot(&self, project: &str) -> Result<BoardSnapshot, BoardError> {
        let tickets = self.tickets(project).await?;
        let project = self.project(project).await?;
        Ok(BoardSnapshot {
            project: self.project_info(&project, &tickets).await?,
            columns: columns(&tickets),
        })
    }

    /// Every project with its counts, oldest first.
    pub async fn project_infos(&self) -> Result<Vec<ProjectInfo>, BoardError> {
        let mut infos = Vec::new();
        for project in self.projects().await? {
            let tickets = self.tickets(&project.key).await?;
            infos.push(self.project_info(&project, &tickets).await?);
        }
        Ok(infos)
    }

    pub async fn project_info_of(&self, key: &str) -> Result<ProjectInfo, BoardError> {
        let project = self.project(key).await?;
        let tickets = self.tickets(key).await?;
        self.project_info(&project, &tickets).await
    }

    async fn project_info(
        &self,
        project: &Project,
        tickets: &[Ticket],
    ) -> Result<ProjectInfo, BoardError> {
        let working: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sessions s JOIN tickets t ON t.id = s.ticket_id
             WHERE t.project_id = ? AND s.kind = 'agent' AND s.ended_at IS NULL",
        )
        .bind(&project.id)
        .fetch_one(self.store.pool())
        .await?;
        // The project's needs-you count (ATT-10): the same items Needs you lists for it.
        let flow = self.effective_flow(&project.key).await?;
        let needs_you = needs_you_items_with(
            &[ProjectTickets {
                key: &project.key,
                name: &project.name,
                tickets,
                flow: &flow,
            }],
            now_ms(),
            &self.limit_stops().snapshot(),
        )
        .len();
        Ok(ProjectInfo {
            key: project.key.clone(),
            name: project.name.clone(),
            repo_path: project.repo_path.clone(),
            base_branch: project.base_branch.clone(),
            agents_working: u32::try_from(working).unwrap_or(0),
            needs_you: u32::try_from(needs_you).unwrap_or(u32::MAX),
            flow_setup_since: project.flow_setup_since,
            paused_since: project.paused_since,
        })
    }
}
