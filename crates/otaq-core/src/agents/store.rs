//! Thin access to the `agent_holds` table, the settings about models (the account's default model, "Noyan starts
//! on") and the sessions' facts (AGM-03, UX-70; d0108, d0088). The rules live in [`super`].

use sqlx::FromRow;

use super::{AgentHold, HoldReason, ModelRef, NoyanStartsOn, SessionRole};
use crate::agent_tools::Tool;
use crate::session::END_MODEL_REFUSED;
use crate::store::settings::{DEFAULT_MODEL, NOYAN_STARTS_ON};
use crate::store::{Store, StoreError};

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

impl Store {
    /// "Noyan starts on" (UX-70): `default` while none was set.
    pub async fn noyan_starts_on(&self) -> Result<NoyanStartsOn, StoreError> {
        Ok(self
            .setting(NOYAN_STARTS_ON)
            .await?
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default())
    }

    /// Sets "Noyan starts on". The caller checked it.
    pub async fn set_noyan_starts_on(&self, on: &NoyanStartsOn) -> Result<(), StoreError> {
        let json = serde_json::to_string(on).expect("a setting serializes");
        self.set_setting(NOYAN_STARTS_ON, &json).await
    }

    /// The account's default model (AGM-03); `None` until a tool connects.
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
    /// and its model (the one the tool reported, else the one passed; the card's avatar, T-0362).
    pub async fn running_agent_names(
        &self,
        project_id: &str,
    ) -> Result<Vec<RunningAgent>, StoreError> {
        let rows: Vec<RunningRecord> = sqlx::query_as(
            "SELECT s.ticket_id, s.kind, s.agent_name AS name, s.tool, COALESCE(s.model_ran, s.model) AS model,
                    s.model AS passed
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
            "SELECT s.id, s.kind, s.state, s.started_at, s.ended_at, s.tool, s.runs_for, s.agent_name, s.model,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HoldScope<'a> {
    Ticket(&'a str),
    Project(&'a str),
}

impl Store {
    /// Holds `hold`'s session (SCH-04 `[M23]`): one row per ticket (or project) and session role, replacing an
    /// earlier hold of it. A hold that stays for the same reason keeps its `since`.
    pub async fn set_agent_hold(&self, hold: &AgentHold) -> Result<(), StoreError> {
        let conflict = if hold.ticket_id.is_some() {
            "(ticket_id, role) WHERE ticket_id IS NOT NULL"
        } else {
            "(project_id, role) WHERE project_id IS NOT NULL"
        };
        let sql = format!(
            "INSERT INTO agent_holds (id, ticket_id, project_id, role, reason, tool, model, skill, since)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT {conflict} DO UPDATE SET
                 since = CASE WHEN agent_holds.reason = excluded.reason THEN agent_holds.since
                              ELSE excluded.since END,
                 reason = excluded.reason, tool = excluded.tool, model = excluded.model,
                 skill = excluded.skill"
        );
        sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(&hold.id)
            .bind(&hold.ticket_id)
            .bind(&hold.project_id)
            .bind(hold.role.as_str())
            .bind(hold.reason.as_str())
            .bind(hold.tool.as_str())
            .bind(&hold.model)
            .bind(&hold.skill)
            .bind(hold.since)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    /// Lifts the hold of `role` in `scope`. Returns whether there was one.
    pub async fn clear_agent_hold(
        &self,
        scope: HoldScope<'_>,
        role: SessionRole,
    ) -> Result<bool, StoreError> {
        let (column, id) = match scope {
            HoldScope::Ticket(id) => ("ticket_id", id),
            HoldScope::Project(id) => ("project_id", id),
        };
        let sql = format!("DELETE FROM agent_holds WHERE {column} = ? AND role = ?");
        let done = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(id)
            .bind(role.as_str())
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
        let holds = match scope {
            HoldScope::Ticket(id) => self.ticket_holds(id).await?,
            HoldScope::Project(id) => self.project_holds(id).await?,
        };
        Ok(holds.into_iter().find(|h| h.role == role))
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

    /// Every hold on a tool, oldest first (the banner's line and Resume per tool).
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
    skill: Option<String>,
    since: i64,
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
            model: self.model,
            skill: self.skill,
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
    /// What it ran for ([`super::About::scope`]: a stage, the delivery; `None` for an older session) and the name
    /// shown for it then (its tool's; an older session's agent's).
    pub runs_for: Option<String>,
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
    runs_for: Option<String>,
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
            runs_for: self.runs_for,
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
