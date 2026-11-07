//! A stage runs only on a tool that has its skill.
//!
//! A stage due to start on a tool that doesn't have the skill it names (Otaq's finder, [`Board::skill_tools`], for
//! the ticket's worktree) doesn't start: the check before every launch ([`crate::agents::gate`]) holds it like a
//! tool that can't run, with the hold reason [`crate::agents::HoldReason::MissingSkill`] and the skill's name. The
//! ticket waits in Needs you, "Codex doesn't have the skill `/team-validate`", with **Check again** (the finder is
//! asked again and the stage starts if the skill is found) and **Change agent…** (only agents whose tool has the
//! skill). The same check runs when a skill is typed into a session already running (Open PR into the live Dev
//! session, a queued return): there nothing is typed.
//!
//! Otaq's own skills are had by every tool, so a stage running one of them never waits for its skill.

use std::collections::HashMap;

use super::tickets::Ticket;
use super::{Board, BoardError};
use crate::agent_tools::{Tool, is_own_skill};
use crate::agents::{HoldReason, SessionRole, resolve};
use crate::domain::{MissingSkill, Status};

impl Board {
    /// The user's skill a new session of `role` on `ticket` starts with: the Agent stage's `skill:` for the
    /// Developer, the agent check's for the Validator. `None` for a stage that names none or names one of Otaq's own
    /// (every tool has those), and for the Project agent.
    pub(crate) async fn held_stage_skill(
        &self,
        ticket: &Ticket,
        role: SessionRole,
    ) -> Result<Option<String>, BoardError> {
        let flow = self.effective_flow(ticket.key.project.as_str()).await?;
        let skips = self.ticket_skips(ticket, &flow).await?;
        let stage = match role {
            SessionRole::Developer => resolve::agent_stage_of(&flow, ticket, &skips),
            SessionRole::Validator => resolve::check_stage_of(&flow, ticket, &skips),
            SessionRole::ProjectAgent => None,
        };
        Ok(stage
            .and_then(|s| s.skill.as_deref())
            .map(str::trim)
            .filter(|name| !name.is_empty() && !is_own_skill(name))
            .map(str::to_owned))
    }

    /// `skill` when `tool` doesn't have it for `ticket` (its worktree, or the base it will be made from); `None`
    /// when it does, for Otaq's own skills and for no skill.
    pub(crate) async fn skill_missing_on(
        &self,
        ticket: &Ticket,
        tool: Tool,
        skill: Option<&str>,
    ) -> Option<String> {
        let name = skill.map(str::trim).filter(|n| !n.is_empty())?;
        if is_own_skill(name) {
            return None;
        }
        let key = ticket.key.to_string();
        let has = self
            .tool_has_skill(ticket.key.project.as_str(), Some(&key), tool, name)
            .await;
        (!has).then(|| name.to_owned())
    }
}

/// Each ticket of `project_id` held because its stage's tool doesn't have the stage's skill, by ticket id, while
/// the held stage still waits: a held Developer's ticket is Held, a held check's ticket is still in its check.
pub(crate) async fn project_missing_skills(
    pool: &sqlx::SqlitePool,
    project_id: &str,
) -> Result<HashMap<String, MissingSkill>, BoardError> {
    let rows: Vec<(String, String, Option<String>)> = sqlx::query_as(
        "SELECT h.ticket_id, h.tool, h.skill FROM agent_holds h
         JOIN tickets t ON t.id = h.ticket_id
         WHERE t.project_id = ? AND h.reason = ?
           AND ((h.role = ? AND t.state = ?) OR (h.role = ? AND t.state IN (?, ?)))
         ORDER BY h.since, h.id",
    )
    .bind(project_id)
    .bind(HoldReason::MissingSkill.as_str())
    .bind(SessionRole::Developer.as_str())
    .bind(Status::Held.as_str())
    .bind(SessionRole::Validator.as_str())
    .bind(Status::Checking.as_str())
    .bind(Status::WaitingForLimitReset.as_str())
    .fetch_all(pool)
    .await?;
    let mut out = HashMap::new();
    for (ticket, tool, skill) in rows {
        let (Ok(tool), Some(skill)) = (tool.parse::<Tool>(), skill) else {
            continue;
        };
        out.entry(ticket).or_insert(MissingSkill { tool, skill });
    }
    Ok(out)
}
