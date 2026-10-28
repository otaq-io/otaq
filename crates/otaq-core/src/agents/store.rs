//! Thin access to the `agents`, `agent_skills` and `agent_holds` tables and the agent settings (AGM-01, AGM-03;
//! d0108, d0088). The rules (names, models from the tool's list, roles, delete while in use) live in [`super`].
//!
//! A row of a tool this build doesn't know is skipped when read as an [`Agent`].

use sqlx::{FromRow, Sqlite, Transaction};

use super::{
    Agent, AgentHold, AgentModel, CameFrom, Fallbacks, HoldReason, ImportedSkill, Mark, ModelRef,
    Role, SessionRole, SkillRef,
};
use crate::agent_tools::Tool;
use crate::session::END_MODEL_REFUSED;
use crate::store::settings::{AGENT_FALLBACK, DEFAULT_MODEL};
use crate::store::{Store, StoreError, new_id, now_ms};

/// The query's session (its columns prefixed with the literal, `"s."`) started: it didn't end "not started", that is
/// refused on the model (T-0337) or before its tool reported the conversation's id (a failed start, nothing to resume:
/// T-0349, Q130). Bind [`END_MODEL_REFUSED`] for its `?`.
macro_rules! started {
    ($t:literal) => {
        concat!(
            "COALESCE(",
            $t,
            "end_reason, '') <> ? AND NOT (",
            $t,
            "ended_at IS NOT NULL AND COALESCE(",
            $t,
            "tool_session_id, '') = '')"
        )
    };
}

/// A new agent's fields as stored (checked by the rules): `imported` the records of its imported skills.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewAgent {
    pub name: String,
    pub instructions: String,
    pub skills: Vec<SkillRef>,
    pub imported: Vec<ImportedSkill>,
    pub model: AgentModel,
    pub effort: Option<String>,
    pub role: Option<Role>,
}

#[derive(Debug, Clone, FromRow)]
struct AgentRecord {
    id: String,
    name: String,
    instructions: String,
    tool: Option<String>,
    model: Option<String>,
    effort: Option<String>,
    role: Option<String>,
    came_from: String,
    came_from_project: Option<String>,
    mark_text: Option<String>,
    mark_model: Option<String>,
    mark_ticket: Option<String>,
    mark_message: Option<String>,
    marked_at: Option<i64>,
    created_at: i64,
    updated_at: i64,
}

#[derive(Debug, Clone, FromRow)]
struct SkillRecord {
    agent_id: String,
    shipped: Option<String>,
    imported_id: Option<String>,
    name: Option<String>,
    original: Option<String>,
    imported_at: Option<i64>,
    fingerprint: Option<String>,
    warnings_json: String,
}

impl AgentRecord {
    fn into_agent(self, skills: &[SkillRecord]) -> Option<Agent> {
        let model = match self.tool.as_deref() {
            None => AgentModel::Default,
            Some(tool) => {
                let Ok(tool) = tool.parse() else {
                    tracing::warn!(id = self.id, tool, "an agent of an unknown tool");
                    return None;
                };
                AgentModel::Tool {
                    tool,
                    model: self.model,
                }
            }
        };
        let mark = match (self.mark_text, self.marked_at) {
            (Some(text), Some(at)) => Some(Mark {
                text,
                model: self.mark_model,
                ticket: self.mark_ticket,
                message: self.mark_message,
                at,
            }),
            _ => None,
        };
        let came_from = match self.came_from.as_str() {
            "agents_file" => CameFrom::AgentsFile,
            "default_preset" => CameFrom::DefaultPreset,
            "flow" => CameFrom::Flow {
                project: self.came_from_project.unwrap_or_default(),
            },
            _ => CameFrom::You,
        };
        let mut refs = Vec::new();
        let mut imported = Vec::new();
        for skill in skills.iter().filter(|s| s.agent_id == self.id) {
            match (&skill.shipped, &skill.imported_id) {
                (Some(name), _) => refs.push(SkillRef::Shipped(name.clone())),
                (None, Some(id)) => {
                    refs.push(SkillRef::Imported(id.clone()));
                    imported.push(ImportedSkill {
                        id: id.clone(),
                        name: skill.name.clone().unwrap_or_default(),
                        original: skill.original.clone().unwrap_or_default(),
                        imported_at: skill.imported_at.unwrap_or_default(),
                        fingerprint: skill.fingerprint.clone().unwrap_or_default(),
                        warnings: serde_json::from_str(&skill.warnings_json).unwrap_or_default(),
                    });
                }
                (None, None) => {}
            }
        }
        Some(Agent {
            id: self.id,
            name: self.name,
            instructions: self.instructions,
            skills: refs,
            imported,
            model,
            effort: self.effort,
            role: self.role.and_then(|r| r.parse().ok()),
            came_from,
            mark,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

fn came_from_columns(came_from: &CameFrom) -> (&'static str, Option<&str>) {
    match came_from {
        CameFrom::AgentsFile => ("agents_file", None),
        CameFrom::DefaultPreset => ("default_preset", None),
        CameFrom::You => ("you", None),
        CameFrom::Flow { project } => ("flow", Some(project)),
    }
}

impl Store {
    /// Every agent, oldest first.
    pub async fn agents(&self) -> Result<Vec<Agent>, StoreError> {
        let rows: Vec<AgentRecord> =
            sqlx::query_as("SELECT * FROM agents ORDER BY created_at, rowid")
                .fetch_all(self.pool())
                .await?;
        let skills: Vec<SkillRecord> =
            sqlx::query_as("SELECT * FROM agent_skills ORDER BY agent_id, position")
                .fetch_all(self.pool())
                .await?;
        Ok(rows
            .into_iter()
            .filter_map(|r| r.into_agent(&skills))
            .collect())
    }

    pub async fn agent(&self, id: &str) -> Result<Option<Agent>, StoreError> {
        let row: Option<AgentRecord> = sqlx::query_as("SELECT * FROM agents WHERE id = ?")
            .bind(id)
            .fetch_optional(self.pool())
            .await?;
        let Some(row) = row else { return Ok(None) };
        let skills: Vec<SkillRecord> =
            sqlx::query_as("SELECT * FROM agent_skills WHERE agent_id = ? ORDER BY position")
                .bind(id)
                .fetch_all(self.pool())
                .await?;
        Ok(row.into_agent(&skills))
    }

    /// The agent called `name`, ignoring case.
    pub async fn agent_named(&self, name: &str) -> Result<Option<Agent>, StoreError> {
        let folded = name.trim().to_lowercase();
        Ok(self
            .agents()
            .await?
            .into_iter()
            .find(|a| a.name.to_lowercase() == folded))
    }

    /// The agent holding `role` (AGM-01: exactly one does once the agents file was applied).
    pub async fn role_agent(&self, role: Role) -> Result<Option<Agent>, StoreError> {
        let id: Option<String> = sqlx::query_scalar("SELECT id FROM agents WHERE role = ?")
            .bind(role.as_str())
            .fetch_optional(self.pool())
            .await?;
        match id {
            Some(id) => self.agent(&id).await,
            None => Ok(None),
        }
    }

    /// Inserts an agent and returns its id. The caller checked the fields.
    pub async fn insert_agent(
        &self,
        new: &NewAgent,
        came_from: &CameFrom,
        id: Option<&str>,
    ) -> Result<String, StoreError> {
        let mut tx = self.begin_write().await?;
        let id = Self::insert_agent_in(&mut tx, new, came_from, id).await?;
        tx.commit().await?;
        Ok(id)
    }

    /// Inserts an agent in `tx` (with `id` when given: the move-over keeps a named agent's) and returns its id.
    pub(crate) async fn insert_agent_in(
        tx: &mut Transaction<'_, Sqlite>,
        new: &NewAgent,
        came_from: &CameFrom,
        id: Option<&str>,
    ) -> Result<String, StoreError> {
        let id = id.map_or_else(new_id, str::to_owned);
        let now = now_ms();
        let (from, project) = came_from_columns(came_from);
        let (tool, model) = model_columns(&new.model);
        sqlx::query(
            "INSERT INTO agents (id, name, instructions, tool, model, effort, role, came_from, came_from_project,
                                 created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&new.name)
        .bind(&new.instructions)
        .bind(tool)
        .bind(model)
        .bind(&new.effort)
        .bind(new.role.map(Role::as_str))
        .bind(from)
        .bind(project)
        .bind(now)
        .bind(now)
        .execute(&mut **tx)
        .await?;
        write_skills(tx, &id, &new.skills, &new.imported).await?;
        Ok(id)
    }

    /// Changes an agent's fields (its role stays); `clear_mark` removes its mark (a saved model change, AGT-05).
    pub async fn update_agent(
        &self,
        id: &str,
        fields: &NewAgent,
        clear_mark: bool,
    ) -> Result<(), StoreError> {
        let mut tx = self.begin_write().await?;
        let old: Option<String> = sqlx::query_scalar("SELECT name FROM agents WHERE id = ?")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;
        if let Some(old) = old.filter(|old| *old != fields.name) {
            // Flows name agents by name: a rename follows into every stored flow (AGM-01).
            Store::rename_agent_in_flows_in(&mut tx, &old, &fields.name).await?;
        }
        let (tool, model) = model_columns(&fields.model);
        sqlx::query(
            "UPDATE agents SET name = ?, instructions = ?, tool = ?, model = ?, effort = ?, updated_at = ?
             WHERE id = ?",
        )
        .bind(&fields.name)
        .bind(&fields.instructions)
        .bind(tool)
        .bind(model)
        .bind(&fields.effort)
        .bind(now_ms())
        .bind(id)
        .execute(&mut *tx)
        .await?;
        if clear_mark {
            sqlx::query(
                "UPDATE agents SET mark_text = NULL, mark_model = NULL, mark_ticket = NULL, mark_message = NULL,
                                   marked_at = NULL
                 WHERE id = ?",
            )
            .bind(id)
            .execute(&mut *tx)
            .await?;
        }
        sqlx::query("DELETE FROM agent_skills WHERE agent_id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        write_skills(&mut tx, id, &fields.skills, &fields.imported).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Updates every agent's record of the imported skill `skill.id` (a Re-import: the copy changed in place).
    pub async fn update_imported_skill(&self, skill: &ImportedSkill) -> Result<(), StoreError> {
        sqlx::query(
            "UPDATE agent_skills SET name = ?, original = ?, imported_at = ?, fingerprint = ?, warnings_json = ?
             WHERE imported_id = ?",
        )
        .bind(&skill.name)
        .bind(&skill.original)
        .bind(skill.imported_at)
        .bind(&skill.fingerprint)
        .bind(serde_json::to_string(&skill.warnings).expect("warnings serialize"))
        .bind(&skill.id)
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// The ids of every imported skill an agent holds (Otaq's copies to keep).
    pub async fn imported_skill_ids(&self) -> Result<Vec<String>, StoreError> {
        Ok(sqlx::query_scalar(
            "SELECT DISTINCT imported_id FROM agent_skills WHERE imported_id IS NOT NULL",
        )
        .fetch_all(self.pool())
        .await?)
    }

    pub async fn delete_agent(&self, id: &str) -> Result<(), StoreError> {
        sqlx::query("DELETE FROM agents WHERE id = ?")
            .bind(id)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// Gives `role` to the agent `to`, taking it from the agent that held it, in one transaction.
    pub async fn move_role(&self, role: Role, to: &str) -> Result<(), StoreError> {
        let mut tx = self.begin_write().await?;
        sqlx::query("UPDATE agents SET role = NULL, updated_at = ? WHERE role = ? AND id <> ?")
            .bind(now_ms())
            .bind(role.as_str())
            .bind(to)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE agents SET role = ?, updated_at = ? WHERE id = ?")
            .bind(role.as_str())
            .bind(now_ms())
            .bind(to)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    /// The default model for new agents (AGM-03); `None` until a tool connects.
    pub async fn default_model(&self) -> Result<Option<ModelRef>, StoreError> {
        Ok(self
            .setting(DEFAULT_MODEL)
            .await?
            .and_then(|json| serde_json::from_str(&json).ok()))
    }

    pub async fn set_default_model(&self, model: &ModelRef) -> Result<(), StoreError> {
        let json = serde_json::to_string(model).expect("a model serializes");
        self.set_setting(DEFAULT_MODEL, &json).await
    }

    /// Each tool's fallback (AGM-03).
    pub async fn fallbacks(&self) -> Result<Fallbacks, StoreError> {
        Ok(self
            .setting(AGENT_FALLBACK)
            .await?
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default())
    }

    pub async fn set_fallbacks(&self, fallbacks: &Fallbacks) -> Result<(), StoreError> {
        let json = serde_json::to_string(fallbacks).expect("fallbacks serialize");
        self.set_setting(AGENT_FALLBACK, &json).await
    }

    /// Whether the ticket's agent has started: it has an Agent stage's session that didn't end in "not started" (its
    /// first turn refused on the model, T-0337, or the tool ended before its conversation's id arrived, T-0349).
    pub async fn developer_started(&self, ticket_id: &str) -> Result<bool, StoreError> {
        Ok(sqlx::query_scalar(concat!(
            "SELECT EXISTS (SELECT 1 FROM sessions WHERE ticket_id = ? AND kind = 'agent' AND ",
            started!(""),
            ")"
        ))
        .bind(ticket_id)
        .bind(END_MODEL_REFUSED)
        .fetch_one(self.pool())
        .await?)
    }

    /// The agent of each running session of a project's tickets, by ticket id: the developer's side (agent, answer,
    /// delivery) or the validator's (the board's footer names whoever has the turn, T-0345), with the tool it runs on
    /// and its model (the one the tool reported, else the one passed; the card's avatar, T-0362) and the fallback it
    /// runs on (AGM-03).
    pub async fn running_agent_names(
        &self,
        project_id: &str,
    ) -> Result<Vec<RunningAgent>, StoreError> {
        let rows: Vec<RunningRecord> = sqlx::query_as(
            "SELECT s.ticket_id, s.kind, s.agent_name AS name, s.tool, COALESCE(s.model_ran, s.model) AS model,
                    s.model AS passed, s.fallback_from, s.fallback_reason
             FROM sessions s JOIN tickets t ON t.id = s.ticket_id
             WHERE t.project_id = ? AND s.state = 'running' AND s.agent_name IS NOT NULL
               AND s.kind IN ('agent', 'answer', 'delivery', 'validator')
             ORDER BY s.started_at, s.id",
        )
        .bind(project_id)
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| RunningAgent {
                role: match r.kind.as_str() {
                    "validator" => SessionRole::Validator,
                    _ => SessionRole::Developer,
                },
                ticket_id: r.ticket_id,
                name: r.name,
                tool: r.tool.and_then(|t| t.parse().ok()),
                model: r.model,
                passed: r.passed,
                fallback: fallback_of(r.fallback_from.as_deref(), r.fallback_reason.as_deref()),
            })
            .collect())
    }

    /// The ticket's agent sessions with the agent each ran on (the sheet and History), with what `SessionInfo` doesn't
    /// carry: the session role, and each session's tokens and round outcome. Oldest first.
    pub async fn session_agents(
        &self,
        ticket_id: &str,
    ) -> Result<Vec<SessionAgentRun>, StoreError> {
        let rows: Vec<SessionAgentRunRecord> = sqlx::query_as(
            "SELECT s.id, s.kind, s.state, s.started_at, s.ended_at, s.tool, s.agent_id, s.agent_name, s.model,
                    s.model_ran, s.effort,
                    (SELECT SUM(u.input + u.output + COALESCE(u.reasoning, 0)) FROM usage u
                     WHERE u.session_id = s.id) AS tokens,
                    (SELECT v.outcome FROM validations v WHERE v.session_id = s.id
                     ORDER BY v.started_at DESC, v.rowid DESC LIMIT 1) AS round_outcome
             FROM sessions s
             WHERE s.ticket_id = ? AND s.kind IN ('agent', 'answer', 'delivery', 'validator')
             ORDER BY s.started_at, s.id",
        )
        .bind(ticket_id)
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(SessionAgentRunRecord::into_session)
            .collect())
    }
}

/// The fallback a session's columns say it runs on.
pub(crate) fn fallback_of(tool: Option<&str>, reason: Option<&str>) -> Option<super::FallbackFrom> {
    Some(super::FallbackFrom {
        tool: tool?.parse().ok()?,
        reason: reason?.parse().ok()?,
    })
}

fn model_columns(model: &AgentModel) -> (Option<&'static str>, Option<&str>) {
    match model {
        AgentModel::Default => (None, None),
        AgentModel::Tool { tool, model } => (Some(tool.as_str()), model.as_deref()),
    }
}

async fn write_skills(
    tx: &mut Transaction<'_, Sqlite>,
    agent_id: &str,
    skills: &[SkillRef],
    imported: &[ImportedSkill],
) -> Result<(), StoreError> {
    for (position, skill) in skills.iter().enumerate() {
        let position = i64::try_from(position).unwrap_or(i64::MAX);
        match skill {
            SkillRef::Shipped(name) => {
                sqlx::query(
                    "INSERT INTO agent_skills (agent_id, position, shipped) VALUES (?, ?, ?)",
                )
                .bind(agent_id)
                .bind(position)
                .bind(name)
                .execute(&mut **tx)
                .await?;
            }
            SkillRef::Imported(id) => {
                let Some(record) = imported.iter().find(|s| &s.id == id) else {
                    continue;
                };
                sqlx::query(
                    "INSERT INTO agent_skills (agent_id, position, imported_id, name, original, imported_at,
                                               fingerprint, warnings_json)
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                )
                .bind(agent_id)
                .bind(position)
                .bind(id)
                .bind(&record.name)
                .bind(&record.original)
                .bind(record.imported_at)
                .bind(&record.fingerprint)
                .bind(serde_json::to_string(&record.warnings).expect("warnings serialize"))
                .execute(&mut **tx)
                .await?;
            }
        }
    }
    Ok(())
}

/// Where an agent hold belongs: a ticket's session, one check agent of a ticket's agent check (d0130
/// row 17), or a project's Project agent. A ticket's scope covers every check agent of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HoldScope<'a> {
    Ticket(&'a str),
    Check { ticket: &'a str, agent: &'a str },
    Project(&'a str),
}

impl Store {
    /// Holds `hold`'s session (SCH-04 `[M23]`): one row per ticket (or project), session role and check agent,
    /// replacing an earlier hold of it. A hold that stays for the same reason keeps its `since` and the user's Use
    /// fallback.
    pub async fn set_agent_hold(&self, hold: &AgentHold) -> Result<(), StoreError> {
        let conflict = if hold.ticket_id.is_some() {
            "(ticket_id, role, check_agent) WHERE ticket_id IS NOT NULL"
        } else {
            "(project_id, role) WHERE project_id IS NOT NULL"
        };
        let sql = format!(
            "INSERT INTO agent_holds (id, ticket_id, project_id, role, reason, tool, model, agent_id, fallback_tool,
                                      fallback_held, fallback_asked, since, check_agent)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT {conflict} DO UPDATE SET
                 since = CASE WHEN agent_holds.reason = excluded.reason THEN agent_holds.since
                              ELSE excluded.since END,
                 reason = excluded.reason, tool = excluded.tool, model = excluded.model,
                 agent_id = excluded.agent_id, fallback_tool = excluded.fallback_tool,
                 fallback_held = excluded.fallback_held,
                 fallback_asked = agent_holds.fallback_asked OR excluded.fallback_asked"
        );
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(&hold.id)
            .bind(&hold.ticket_id)
            .bind(&hold.project_id)
            .bind(hold.role.as_str())
            .bind(hold.reason.as_str())
            .bind(hold.tool.as_str())
            .bind(&hold.model)
            .bind(&hold.agent_id)
            .bind(hold.fallback_tool.map(Tool::as_str))
            .bind(hold.fallback_held)
            .bind(hold.fallback_asked)
            .bind(hold.since)
            .bind(hold.check_agent.as_deref().unwrap_or_default())
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// The user's Use fallback on one held session (the hold `hold_id`): its next start launches on its tool's
    /// fallback; a round's other check agents are left as they are (d0130 row 17).
    pub async fn ask_fallback(&self, hold_id: &str) -> Result<bool, StoreError> {
        let done = sqlx::query("UPDATE agent_holds SET fallback_asked = TRUE WHERE id = ?")
            .bind(hold_id)
            .execute(self.pool())
            .await?;
        Ok(done.rows_affected() > 0)
    }

    /// Lifts the hold of `role` in `scope`. Returns whether there was one.
    pub async fn clear_agent_hold(
        &self,
        scope: HoldScope<'_>,
        role: SessionRole,
    ) -> Result<bool, StoreError> {
        let (column, id, agent) = match scope {
            HoldScope::Ticket(id) => ("ticket_id", id, None),
            HoldScope::Check { ticket, agent } => ("ticket_id", ticket, Some(agent)),
            HoldScope::Project(id) => ("project_id", id, None),
        };
        let sql = format!(
            "DELETE FROM agent_holds WHERE {column} = ? AND role = ? AND (? IS NULL OR check_agent = ?)"
        );
        let done = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(id)
            .bind(role.as_str())
            .bind(agent)
            .bind(agent)
            .execute(self.pool())
            .await?;
        Ok(done.rows_affected() > 0)
    }

    /// The hold of `role` in `scope`, if any (a ticket's scope: its first of that role).
    pub async fn agent_hold(
        &self,
        scope: HoldScope<'_>,
        role: SessionRole,
    ) -> Result<Option<AgentHold>, StoreError> {
        let (holds, agent) = match scope {
            HoldScope::Ticket(id) => (self.ticket_holds(id).await?, None),
            HoldScope::Check { ticket, agent } => (self.ticket_holds(ticket).await?, Some(agent)),
            HoldScope::Project(id) => (self.project_holds(id).await?, None),
        };
        Ok(holds
            .into_iter()
            .find(|h| h.role == role && agent.is_none_or(|a| h.check_agent.as_deref() == Some(a))))
    }

    /// The holds of a ticket, by role.
    pub async fn ticket_holds(&self, ticket_id: &str) -> Result<Vec<AgentHold>, StoreError> {
        self.holds_where("ticket_id = ?", ticket_id).await
    }

    /// The holds of a project's Project agent (its first message).
    pub async fn project_holds(&self, project_id: &str) -> Result<Vec<AgentHold>, StoreError> {
        self.holds_where("project_id = ?", project_id).await
    }

    /// The holds of every ticket of a project, oldest first (the board's cards, T-0362).
    pub async fn project_ticket_holds(
        &self,
        project_id: &str,
    ) -> Result<Vec<AgentHold>, StoreError> {
        self.holds_where(
            "ticket_id IN (SELECT id FROM tickets WHERE project_id = ?)",
            project_id,
        )
        .await
    }

    /// Every hold on a tool, oldest first (the banner's line, Resume and Use fallback now per tool).
    pub async fn holds_on_tool(&self, tool: Tool) -> Result<Vec<AgentHold>, StoreError> {
        self.holds_where("tool = ?", tool.as_str()).await
    }

    /// Every hold for a reason, oldest first.
    pub async fn holds_for(&self, reason: HoldReason) -> Result<Vec<AgentHold>, StoreError> {
        self.holds_where("reason = ?", reason.as_str()).await
    }

    async fn holds_where(&self, filter: &str, value: &str) -> Result<Vec<AgentHold>, StoreError> {
        let sql = format!("SELECT * FROM agent_holds WHERE {filter} ORDER BY since, id");
        let rows: Vec<AgentHoldRecord> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
            .bind(value)
            .fetch_all(self.pool())
            .await?;
        Ok(rows
            .into_iter()
            .filter_map(AgentHoldRecord::into_hold)
            .collect())
    }
}

#[derive(Debug, Clone, FromRow)]
struct AgentHoldRecord {
    id: String,
    ticket_id: Option<String>,
    project_id: Option<String>,
    role: String,
    reason: String,
    tool: String,
    model: Option<String>,
    agent_id: Option<String>,
    fallback_tool: Option<String>,
    fallback_held: bool,
    fallback_asked: bool,
    since: i64,
    check_agent: String,
}

impl AgentHoldRecord {
    /// `None` for a row of a tool this build doesn't know.
    fn into_hold(self) -> Option<AgentHold> {
        Some(AgentHold {
            role: self.role.parse().ok()?,
            reason: self.reason.parse().ok()?,
            tool: self.tool.parse().ok()?,
            id: self.id,
            ticket_id: self.ticket_id,
            project_id: self.project_id,
            check_agent: Some(self.check_agent).filter(|a| !a.is_empty()),
            model: self.model,
            agent_id: self.agent_id,
            fallback_tool: self.fallback_tool.and_then(|t| t.parse().ok()),
            fallback_held: self.fallback_held,
            fallback_asked: self.fallback_asked,
            since: self.since,
        })
    }
}

#[derive(Debug, Clone, FromRow)]
struct RunningRecord {
    ticket_id: String,
    kind: String,
    name: String,
    tool: Option<String>,
    model: Option<String>,
    passed: Option<String>,
    fallback_from: Option<String>,
    fallback_reason: Option<String>,
}

/// One running session of a ticket ([`Store::running_agent_names`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunningAgent {
    pub ticket_id: String,
    pub role: SessionRole,
    /// The agent's name at start.
    pub name: String,
    /// `None` for a row of a tool this build doesn't know (or written before tools were stored).
    pub tool: Option<Tool>,
    /// The model the tool reported, else the one passed.
    pub model: Option<String>,
    /// The model as passed (`None`: the tool's default).
    pub passed: Option<String>,
    /// It runs on its tool's fallback (AGM-03).
    pub fallback: Option<super::FallbackFrom>,
}

/// One agent session of a ticket and the agent it ran on ([`Store::session_agents`]).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SessionAgentRun {
    pub id: String,
    /// Developer (agent, answer and PR sessions) or Validator.
    pub role: SessionRole,
    pub running: bool,
    pub started_at: i64,
    pub ended_at: Option<i64>,
    /// The tool it ran on (M23: the sheet names it once several tools are connected).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<Tool>,
    /// The agent at start: its id (the agent may be gone since) and its name then.
    pub agent_id: Option<String>,
    pub agent_name: Option<String>,
    /// The model as passed (`None` = tool default) and the exact model the tool reported.
    pub model: Option<String>,
    pub model_ran: Option<String>,
    /// The effort passed, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<String>,
    /// Shown tokens (input, output, reasoning); `None` before any were read.
    pub tokens: Option<u64>,
    /// A validator session's round outcome (`pass`, `fail`, `no_result`, …).
    pub round_outcome: Option<String>,
    /// The exact model moved on since the previous run on the same agent ([`crate::session::moved_on`]); filled by
    /// the ticket's agent facts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub moved_on: Option<crate::session::MovedOn>,
}

#[derive(Debug, Clone, FromRow)]
struct SessionAgentRunRecord {
    id: String,
    kind: String,
    state: String,
    started_at: i64,
    ended_at: Option<i64>,
    tool: Option<String>,
    agent_id: Option<String>,
    agent_name: Option<String>,
    model: Option<String>,
    model_ran: Option<String>,
    effort: Option<String>,
    tokens: Option<i64>,
    round_outcome: Option<String>,
}

impl SessionAgentRunRecord {
    fn into_session(self) -> SessionAgentRun {
        let role = match self.kind.as_str() {
            "validator" => SessionRole::Validator,
            _ => SessionRole::Developer,
        };
        SessionAgentRun {
            id: self.id,
            role,
            running: self.state == "running",
            started_at: self.started_at,
            ended_at: self.ended_at,
            tool: self.tool.and_then(|t| t.parse().ok()),
            agent_id: self.agent_id,
            agent_name: self.agent_name,
            model: self.model,
            model_ran: self.model_ran,
            effort: self.effort,
            tokens: self.tokens.and_then(|t| u64::try_from(t).ok()),
            round_outcome: self.round_outcome,
            moved_on: None,
        }
    }
}
