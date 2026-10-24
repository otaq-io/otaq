//! Project agents (PAG-01, PAG-04, PAG-06, PAG-07; d0068): agent sessions in a project's own folder (the user's
//! checkout) that investigate and file Backlog tickets. Several per project, each opened by the user's click (the
//! go-ahead, d0018), owned by the background service (d0022).
//!
//! - **Open** takes the user's first message (d0087: sending it is the go-ahead) and starts an agent session with
//!   the Project agent role and the project-agent skill with that message as its first prompt, no ticket
//!   ([`crate::agent_tools::LaunchRole::ProjectAgent`]). The message is its first request from the start: there is
//!   no empty "New agent" session. The check before every launch runs first (SCH-04 `[M23]`,
//!   [`crate::agents::gate`]): a tool that isn't available or is at its limit, or a model no longer listed,
//!   refuses the message with the reason and holds the Project agent (no fallback, Q-B); the message stays in the
//!   pane for the user to send again. A Resume is refused while the usage limit holds. They never count toward the
//!   agent limit, which counts tickets (SCH-03).
//! - **Naming** (PAG-04): "New agent" until named. The agent names itself once with `otaq agent name`; a second
//!   call needs `--rename` (the user asked for it). If it hasn't by the end of its first turn after the first request
//!   (the message it opened with), the request's first words become its name. The user's rename always wins: the
//!   agent can't change it.
//! - **Close** ("End agent" in the pane's menu) ends it and it stays in Recently ended; one that is mid-turn asks
//!   first (`needs_confirmation`, "Stop and close"). **Close ended agents** closes every ended one of a project.
//! - **After a restart** (REC-01) running agents are "Ended by restart" and resume on the user's click. The **10
//!   most recent** ended agents of a project are listed, each resumable (its tool's resume); the tools' saved
//!   conversations are never deleted.
//! - **Epic leads** (d0083) are Project agent rows with an epic ([`lead`]). They are named after their epic and left
//!   out of every Project agent list.

mod checkout;
pub mod first_flow;
pub mod lead;
pub mod prime;

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::agent_tools::{
    Adapters, FirstPrompt, LaunchRequest, LaunchRole, Launched, PROJECT_AGENT_SKILL, Tool,
};
use crate::agents::{Agent, Role, RunsOn, SessionRole, resolve};
use crate::board::questions::{Choice, HumanQuestion};
use crate::board::{Board, BoardError, Refusal};
use crate::domain::stored_enum;
use crate::domain::{Actor, Status, StatusSet, TicketKey};
use crate::naming;
use crate::protocol::Event;
use crate::service::EventBus;
use crate::session::deliveries::{Deliveries, Delivery, IdleSignal};
use crate::session::{
    END_PAUSED, END_SERVICE_STOPPED, SessionAgent, SessionInfo, SessionKind, Sessions, SpawnSpec,
    Start, TerminalSize,
};
use crate::signals::Signal;
use crate::store::projects::Project;
use crate::store::{new_id, now_ms};

/// What an unnamed Project agent is called.
pub const NEW_AGENT: &str = "New agent";

/// How many ended Project agents a project lists (PAG-06).
pub const RECENT: i64 = 10;

/// How many words of the first request make the fallback name, and its longest length in characters.
const FALLBACK_WORDS: usize = 4;
const NAME_MAX_CHARS: usize = 40;

/// What starts a Project agent on the `flow` skill instead of its own (FLW-11): `/flow` as the first message, alone
/// or before a request ("/flow add a Security check"). Settings → Flow's **Change with the Project agent** sends it.
pub const FLOW_COMMAND: &str = "/flow";

/// The skill a first message runs and the request it carries: `/flow …` runs `flow`, anything else the project-agent
/// skill with the whole message.
pub fn first_skill(message: &str) -> (&'static str, Option<String>) {
    let message = message.trim();
    match message.strip_prefix(FLOW_COMMAND) {
        Some(rest) if rest.is_empty() || rest.starts_with(char::is_whitespace) => {
            let rest = rest.trim();
            ("flow", (!rest.is_empty()).then(|| rest.to_owned()))
        }
        _ => (PROJECT_AGENT_SKILL, Some(message.to_owned())),
    }
}

/// Why `project_agent.open` without words is refused: the first message is the go-ahead (d0087).
pub const EMPTY_MESSAGE: &str = "Type a message to start the Project agent";

/// Why no Project agent opens while the plan's usage limit holds (SCH-04).
pub const LIMIT_REFUSAL: &str = "Usage limit reached · Project agents open once it resets";

/// `agent name`, and `ask` without a ticket, from anything but a Project agent.
pub const ONLY_A_PROJECT_AGENT: &str = "Only a Project agent can do this";

/// The size a Project agent starts with until a terminal attaches.
const INITIAL_SIZE: TerminalSize = TerminalSize {
    rows: 40,
    cols: 120,
};

/// The size a flow agent ("Flow setup · Claude Code") starts with: about what its terminal on the Flow setup page
/// shows (420 px wide, `.flow-at-work-terminals`, at the 13 px monospace font), so the output it draws before the
/// page attaches isn't redrawn for a narrower terminal.
const FLOW_AGENT_SIZE: TerminalSize = TerminalSize { rows: 30, cols: 52 };

stored_enum! {
    /// Where a Project agent's name came from (`project_agents.name_source`).
    pub enum NameSource ("name source") {
        /// `otaq agent name`.
        Agent = "agent",
        /// The first words of its first request.
        Fallback = "fallback",
        /// The user's rename; never overwritten.
        User = "user",
    }
}

stored_enum! {
    /// A Project agent's state (`project_agents.state`).
    pub enum AgentState ("project agent state") {
        Running = "running",
        /// It ended (closed, or its agent tool exited).
        Ended = "ended",
        /// A restart ended it: "Ended by restart · Resume".
        EndedByRestart = "ended_by_restart",
        /// The user's Pause ended it (PAU-03, d0112): "Paused · Resume", which continues the same conversation, as
        /// after a restart. The project's Resume resumes it; a restart leaves it as it is.
        Paused = "paused",
    }
}

/// A Project agent as clients see it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectAgent {
    pub id: String,
    /// The project's key.
    pub project: String,
    /// Its name, or "New agent".
    pub name: String,
    /// Where the name came from; none while it is "New agent".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name_source: Option<NameSource>,
    pub state: AgentState,
    /// The agent tool it runs on (ADP-04).
    pub tool: Tool,
    /// The tool's conversation Resume continues (d0045, d0088). Empty for a moment for a tool that reports its
    /// id after start.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_session_id: Option<String>,
    /// The model passed to the tool at launch; `None` when none was.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// It got its first request. Always, since d0087: open records the message it starts with (false only for rows
    /// from before).
    pub got_request: bool,
    /// Its session runs and its turn hasn't ended: closing it asks first.
    pub mid_turn: bool,
    /// Its tool's usage limit stopped it; that tool's Resume continues it.
    pub limit_stopped: bool,
    pub created_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<i64>,
    /// The user closed its tab.
    pub closed: bool,
    /// Its open `ask human` question (d0068, d0061).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub question: Option<HumanQuestion>,
    /// It waits on a permission prompt: the prompt (PAG-07).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub needs_you: Option<String>,
    /// A client's terminal (or `otaq attach`) has focus on its session: its waits raise nothing (ATT-03).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub focused: bool,
    /// Its tokens, every session and day (USG-01).
    #[serde(default)]
    pub tokens: crate::usage::Tokens,
    /// Its newest session (attach to it with `session.attach {session}`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<SessionInfo>,
    /// An epic lead's epic (d0083); its name is then "<epic title> lead".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub epic: Option<TicketKey>,
    /// When its last turn ended, since the service started (in memory): the Ask bar's "New reply" is a turn that
    /// ended after the user last looked at the conversation (SHB-03, d0113).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_ended_at: Option<i64>,
}

/// A project's Project agents: the open ones (tabs) and the most recent ended ones.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectAgentList {
    /// Not closed: running, ended or ended by a restart; oldest first.
    pub open: Vec<ProjectAgent>,
    /// The [`RECENT`] most recently ended ones that got a request, newest first.
    pub recent: Vec<ProjectAgent>,
    /// A first message held before its start (SCH-04 `[M23]`, §7.2): the pane's notice.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub holds: Vec<crate::agents::AgentHold>,
}

/// What the startup reconcile did (REC-01, REC-06).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StartupAgents {
    /// Running Project agents that are now "Ended by restart", by id.
    pub ended: Vec<String>,
    /// "New agent"s that never got a request and were discarded. Only rows from before d0087 (every agent now
    /// opens with its first request), so in practice always 0.
    pub discarded: u32,
    /// Running epic leads that are now "Ended by restart", by id: counted on their own in the reboot summary
    /// ("1 epic lead ended", d0083).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub leads_ended: Vec<String>,
}

/// A Project agent waiting for the user (PAG-07): an inbox item of group 1, labelled with the project and its
/// name instead of a ticket.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentNeedsYou {
    /// The project's key and name ("shop-web").
    pub project: String,
    pub project_name: String,
    /// The Project agent's id and name.
    pub agent: String,
    pub name: String,
    /// An epic lead's epic (d0083): the item opens its Lead tab.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub epic: Option<TicketKey>,
    /// Asks you (`ask human`) or Needs you (a permission prompt).
    pub status: Status,
    /// "asks: Import closed issues too?", "permission: Allow gh issue list?".
    pub reason: String,
    /// Asks you: the question with its options (answered inline with `project_agent.answer`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub question: Option<HumanQuestion>,
    /// When it started waiting.
    pub since: i64,
}

/// What a Project agent created and changed (PAG-05): "Created SHOP-35, SHOP-36 · edited SHOP-31 · 2
/// dependencies".
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreatedAndChanged {
    /// The tickets it created that still exist, in creation order.
    pub created: Vec<CreatedTicket>,
    /// Other tickets it edited, in key order.
    pub edited: Vec<TicketKey>,
    /// Dependencies it added.
    pub dependencies: u32,
    /// The one-line summary.
    pub summary: String,
}

/// A ticket a Project agent created.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreatedTicket {
    pub key: TicketKey,
    pub title: String,
    pub status: Status,
    /// Never started: **Delete** removes it (TKT-08, the 0060 confirmation in the app).
    pub deletable: bool,
}

/// Opens and tracks Project agents. Cheap to clone.
#[derive(Clone)]
pub struct ProjectAgents {
    inner: Arc<Inner>,
}

struct Inner {
    board: Board,
    sessions: Sessions,
    adapters: Adapters,
    events: EventBus,
    /// Whether a session's turn has ended (the signals' turn state).
    idle: Arc<dyn IdleSignal>,
    /// Types the user's answers.
    deliveries: Deliveries,
    /// Agents waiting on a permission prompt: the prompt and since when. In memory: a restart ends them.
    waiting: Mutex<HashMap<String, (String, i64)>>,
    /// When each agent's last turn ended ([`ProjectAgent::turn_ended_at`]). In memory, as `waiting`.
    turn_ends: Mutex<HashMap<String, i64>>,
    /// Epic leads' turns, by Project agent id: why the current turn runs, and why the next one will (d0083).
    lead_turns: Mutex<HashMap<String, lead::LeadTurn>>,
    /// Epic leads seen so far, by Project agent id: their project and epic, for [`Event::LeadChanged`].
    lead_epics: Mutex<HashMap<String, (String, TicketKey)>>,
    /// Questions and first stops sent to the leads, until they act on them (d0083).
    handovers: crate::epic_lead::wake::Handovers,
    /// The check before every launch (set by the service; unset, nothing is checked).
    gate: std::sync::OnceLock<crate::agents::gate::LaunchGate>,
    /// The skill copies each running agent holds in its project's folder (AGM-07).
    checkout: tokio::sync::Mutex<checkout::Held>,
    /// How long an unchosen flow agent may take to read its answer before it ends anyway, in milliseconds
    /// ([`first_flow::ANSWER_GRACE`]; tests shorten it).
    answer_grace_ms: std::sync::atomic::AtomicU64,
}

#[derive(sqlx::FromRow)]
struct Row {
    id: String,
    project_key: String,
    name: Option<String>,
    name_source: Option<String>,
    tool: String,
    tool_session_id: Option<String>,
    model: Option<String>,
    state: String,
    first_request: Option<String>,
    limit_stopped: bool,
    created_at: i64,
    ended_at: Option<i64>,
    closed_at: Option<i64>,
    epic_id: Option<String>,
}

const SELECT: &str =
    "SELECT a.id, p.key AS project_key, a.name, a.name_source, a.tool, a.tool_session_id, a.model,
                             a.state, a.first_request, a.limit_stopped, a.created_at, a.ended_at, a.closed_at, a.epic_id
                      FROM project_agents a JOIN projects p ON p.id = a.project_id";

impl ProjectAgent {
    /// The tool id Resume continues; refused while the tool hasn't reported it.
    fn conversation(&self) -> Result<&str, BoardError> {
        self.tool_session_id.as_deref().ok_or_else(|| {
            BoardError::refused(
                Refusal::SessionEnded,
                format!("{} has no conversation to resume", self.name),
            )
        })
    }
}

impl ProjectAgents {
    pub fn new(
        board: Board,
        sessions: Sessions,
        adapters: Adapters,
        events: EventBus,
        idle: Arc<dyn IdleSignal>,
        deliveries: Deliveries,
    ) -> Self {
        let agents = Self {
            inner: Arc::new(Inner {
                board,
                sessions,
                adapters,
                events,
                idle,
                deliveries,
                waiting: Mutex::new(HashMap::new()),
                turn_ends: Mutex::new(HashMap::new()),
                lead_turns: Mutex::new(HashMap::new()),
                lead_epics: Mutex::new(HashMap::new()),
                handovers: Default::default(),
                gate: std::sync::OnceLock::new(),
                checkout: Default::default(),
                answer_grace_ms: std::sync::atomic::AtomicU64::new(
                    u64::try_from(first_flow::ANSWER_GRACE.as_millis()).unwrap_or(u64::MAX),
                ),
            }),
        };
        // The board tells the leads about queues, merges, closes and reviews (d0083).
        agents.inner.board.leads().set(agents.lead_events());
        agents
    }

    /// Gives the Project agents and leads the service's check before every launch (SCH-04 `[M23]`).
    pub fn set_gate(&self, gate: crate::agents::gate::LaunchGate) {
        let _ = self.inner.gate.set(gate);
    }

    /// The check before a new session of `role` in `scope` on `agent`: what it runs on, or refused with why it is
    /// held (the hold is written).
    async fn checked(
        &self,
        role: SessionRole,
        scope: crate::agents::gate::Scope<'_>,
        agent: &Agent,
    ) -> Result<RunsOn, BoardError> {
        use crate::agents::gate::Gated;
        let Some(gate) = self.inner.gate.get() else {
            let default = self.inner.board.store().default_model().await?;
            return Ok(RunsOn::agent(agent, default.as_ref()));
        };
        match gate.before_launch(role, scope, agent).await? {
            Gated::Launch(runs_on) => Ok(runs_on),
            Gated::Held(held) => Err(BoardError::refused(Refusal::MoveRefused, held.message())),
        }
    }

    /// The startup reconcile (REC-01, d0068): running Project agents ended with the previous service and show
    /// "Ended by restart · Resume". A "New agent" that never got a request disappears: unreachable since d0087
    /// (open records the first request), kept to tidy rows a data folder may still hold from before.
    pub async fn startup(&self) -> Result<StartupAgents, BoardError> {
        let pool = self.inner.board.store().pool();
        // An epic lead stays with its epic even before its first request (d0083).
        let discarded = sqlx::query(
            "DELETE FROM project_agents WHERE first_request IS NULL AND epic_id IS NULL",
        )
        .execute(pool)
        .await?
        .rows_affected();
        let ended: Vec<(String, Option<String>)> = sqlx::query_as(
            "UPDATE project_agents SET state = ?, ended_at = ? WHERE state = ? RETURNING id, epic_id",
        )
        .bind(AgentState::EndedByRestart.as_str())
        .bind(now_ms())
        .bind(AgentState::Running.as_str())
        .fetch_all(pool)
        .await?;
        // Epic leads aren't Project agents in the reboot summary; they are counted on their own.
        let (leads, agents): (Vec<_>, Vec<_>) =
            ended.into_iter().partition(|(_, epic)| epic.is_some());
        let report = StartupAgents {
            ended: agents.into_iter().map(|(id, _)| id).collect(),
            discarded: u32::try_from(discarded).unwrap_or(u32::MAX),
            leads_ended: leads.into_iter().map(|(id, _)| id).collect(),
        };
        if !report.ended.is_empty() || report.discarded > 0 || !report.leads_ended.is_empty() {
            tracing::info!(
                ended = report.ended.len(),
                discarded = report.discarded,
                leads_ended = report.leads_ended.len(),
                "project agents after the restart"
            );
        }
        // No agent runs now: the skill copies a crash left in the projects' folders go (AGM-07).
        self.remove_checkout_leftovers().await;
        Ok(report)
    }

    /// Opens a new Project agent in `project` with the user's first `message` (PAG-01, d0087): sending it is the
    /// go-ahead. The session starts with the skill and the message as one first prompt, and the message is its
    /// first request. Refused without words, and when the check before the launch holds it.
    pub async fn open(&self, project: &str, message: &str) -> Result<ProjectAgent, BoardError> {
        self.open_on(project, message, None).await
    }

    /// [`Self::open`] on the agent `agent` instead of the agent with the Project agent role (the pane's "New agent on
    /// Sonnet", M23).
    pub async fn open_on(
        &self,
        project: &str,
        message: &str,
        agent: Option<&str>,
    ) -> Result<ProjectAgent, BoardError> {
        let project = self.inner.board.project(project).await?;
        let message = message.trim();
        if message.is_empty() {
            return Err(BoardError::refused(Refusal::EmptyField, EMPTY_MESSAGE));
        }
        // The agent asked for ("New agent on Sonnet", the user's pick), else the agent with the Project agent role
        // (AGM-05).
        let store = self.inner.board.store();
        let agent = match agent {
            Some(id) => store
                .agent(id)
                .await?
                .ok_or_else(|| BoardError::NotFound(format!("no agent {id}")))?,
            None => resolve::role_agent(store, Role::ProjectAgent).await?,
        };
        let runs_on = self
            .checked(
                SessionRole::ProjectAgent,
                crate::agents::gate::Scope::Project {
                    id: &project.id,
                    name: &project.name,
                },
                &agent,
            )
            .await?;
        self.start(&project, message, &agent, &runs_on, None).await
    }

    /// Starts a new Project agent in `project` on `agent` with `runs_on` (already checked) and the first `message`;
    /// `name` names it from the start (a flow agent, "Flow setup · OpenCode"), as the user's name would.
    async fn start(
        &self,
        project: &Project,
        message: &str,
        agent: &Agent,
        runs_on: &RunsOn,
        name: Option<&str>,
    ) -> Result<ProjectAgent, BoardError> {
        let id = new_id();
        // The agent's skills, copied into the project's folder while it runs (AGM-07); `/flow` runs the flow
        // skill (FLW-11).
        let (skill, asks) = first_skill(message);
        let first_prompt = self
            .copy_into_checkout(
                &id,
                Path::new(&project.repo_path),
                Some(agent),
                Some(FirstPrompt::Skill {
                    name: skill.to_owned(),
                    message: asks,
                }),
            )
            .await;
        let launched = match self.launch(
            runs_on.tool,
            LaunchRequest {
                model: runs_on.model.clone(),
                effort: runs_on.effort.clone(),
                ..request(
                    &self.inner.board,
                    project,
                    LaunchRole::ProjectAgent,
                    first_prompt,
                )
                .await
            },
        ) {
            Ok(launched) => launched,
            Err(e) => {
                self.release_checkout(&id).await;
                return Err(e);
            }
        };
        sqlx::query(
            "INSERT INTO project_agents (id, project_id, tool, tool_session_id, model, agent_id, agent_name, effort,
                                         state, first_request, created_at, name, name_source)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&project.id)
        .bind(launched.tool.as_str())
        .bind(&launched.tool_session_id)
        .bind(&launched.model)
        .bind(runs_on.agent.as_ref().map(|a| &a.id))
        .bind(runs_on.agent.as_ref().map(|a| &a.name))
        .bind(&runs_on.effort)
        .bind(AgentState::Running.as_str())
        .bind(message)
        .bind(now_ms())
        .bind(name)
        .bind(name.map(|_| NameSource::User.as_str()))
        .execute(self.inner.board.store().pool())
        .await?;
        // Only flow agents are named from the start.
        let size = if name.is_some() {
            FLOW_AGENT_SIZE
        } else {
            INITIAL_SIZE
        };
        if let Err(e) = self
            // The user's first message (or Ask each tool): it starts while paused too (PAU-05).
            .spawn(project, launched, &id, size, runs_on, None, Start::User)
            .await
        {
            self.release_checkout(&id).await;
            sqlx::query("DELETE FROM project_agents WHERE id = ?")
                .bind(&id)
                .execute(self.inner.board.store().pool())
                .await?;
            return Err(e);
        }
        tracing::info!(project = %project.key, agent = id, "project agent opened");
        self.changed(&project.key, &id);
        self.get(&id).await
    }

    /// Resumes an ended Project agent's conversation (Ended by restart · Resume, Recently ended, PAG-06): a new
    /// session with its tool's resume, the user's click. A running one is returned as it is. Refused while its tool
    /// is at its limit (USG-10: another tool's limit doesn't hold it).
    pub async fn resume(&self, id: &str) -> Result<ProjectAgent, BoardError> {
        let agent = self.get(id).await?;
        if let Some(epic) = &agent.epic {
            // An epic lead resumes as one: its role and its epic (d0083).
            return Box::pin(self.leads().resume(&epic.to_string()))
                .await
                .map(|lead| lead.agent);
        }
        if agent.state == AgentState::Running {
            return Ok(agent);
        }
        let runs_on = self.runs_on(&agent).await?;
        self.refuse_during_limit(runs_on.tool).await?;
        self.inner.adapters.unavailable(runs_on.tool)?;
        let project = self.inner.board.project(&agent.project).await?;
        let conversation = agent.conversation()?.to_owned();
        self.copy_stored_agent_into_checkout(&agent.id, &project, &runs_on)
            .await?;
        let launched = self.launch(
            runs_on.tool,
            LaunchRequest {
                resume: Some(conversation),
                model: runs_on.model.clone(),
                effort: runs_on.effort.clone(),
                ..request(&self.inner.board, &project, LaunchRole::ProjectAgent, None).await
            },
        );
        let started = match launched {
            Ok(launched) => {
                self.restart(&agent, &project, launched, &runs_on, None, Start::User)
                    .await
            }
            Err(e) => Err(e),
        };
        if let Err(e) = started {
            self.release_checkout(&agent.id).await;
            return Err(e);
        }
        tracing::info!(agent = id, "project agent resumed");
        self.changed(&agent.project, id);
        self.get(id).await
    }

    /// What an ended agent's Resume runs on (ADP-04, d0088): the tool, model, effort and agent its row stored at its
    /// start, whatever the agent is set to now.
    async fn runs_on(&self, agent: &ProjectAgent) -> Result<RunsOn, BoardError> {
        let stored: Option<(Option<String>, Option<String>, Option<String>)> =
            sqlx::query_as("SELECT agent_id, agent_name, effort FROM project_agents WHERE id = ?")
                .bind(&agent.id)
                .fetch_optional(self.inner.board.store().pool())
                .await?;
        let (id, name, effort) = stored.unwrap_or_default();
        Ok(RunsOn {
            tool: Some(agent.tool),
            model: agent.model.clone(),
            effort,
            agent: id.zip(name).map(|(id, name)| SessionAgent { id, name }),
            fallback: None,
        })
    }

    /// Before a resume: the skills of the agent `runs_on` stored (its agent now, if it is still there) are copied into
    /// the project's folder for the agent `id` (AGM-07).
    async fn copy_stored_agent_into_checkout(
        &self,
        id: &str,
        project: &Project,
        runs_on: &RunsOn,
    ) -> Result<(), BoardError> {
        let agent = match &runs_on.agent {
            Some(stored) => self.inner.board.store().agent(&stored.id).await?,
            None => None,
        };
        self.copy_into_checkout(id, Path::new(&project.repo_path), agent.as_ref(), None)
            .await;
        Ok(())
    }

    /// Starts a new session for the ended agent `agent` with `launched` (its conversation resumed) on `runs_on`'s
    /// agent: running again, its end and closed marks cleared; put back as it was when the session can't start.
    async fn restart(
        &self,
        agent: &ProjectAgent,
        project: &Project,
        launched: Launched,
        runs_on: &RunsOn,
        lead: Option<(&str, &TicketKey)>,
        start: Start,
    ) -> Result<(), BoardError> {
        let id = agent.id.as_str();
        self.set_state(id, AgentState::Running).await?;
        sqlx::query(
            "UPDATE project_agents SET ended_at = NULL, closed_at = NULL, limit_stopped = 0 WHERE id = ?",
        )
        .bind(id)
        .execute(self.inner.board.store().pool())
        .await?;
        if let Err(e) = self
            .spawn(project, launched, id, INITIAL_SIZE, runs_on, lead, start)
            .await
        {
            self.set_state(id, agent.state).await?;
            sqlx::query("UPDATE project_agents SET ended_at = ?, closed_at = ? WHERE id = ?")
                .bind(agent.ended_at)
                .bind(agent.closed.then(now_ms))
                .bind(id)
                .execute(self.inner.board.store().pool())
                .await?;
            return Err(e);
        }
        Ok(())
    }

    /// Closes a Project agent (PAG-06; "End agent" in the pane's menu, d0087): a running one mid-turn needs
    /// `confirm` ("Stop and close"); otherwise it ends and leaves the switcher (it stays in Recently ended).
    pub async fn close(&self, id: &str, confirm: bool) -> Result<(), BoardError> {
        let agent = self.get(id).await?;
        if agent.mid_turn && !confirm {
            return Err(BoardError::refused(
                Refusal::NeedsConfirmation,
                format!(
                    "{} is in the middle of a turn. Stop it and close it?",
                    agent.name
                ),
            ));
        }
        sqlx::query(
            "UPDATE project_agents SET state = CASE WHEN state = ? THEN ? ELSE state END,
                                       ended_at = COALESCE(ended_at, ?), closed_at = ?
             WHERE id = ?",
        )
        .bind(AgentState::Running.as_str())
        .bind(AgentState::Ended.as_str())
        .bind(now_ms())
        .bind(now_ms())
        .bind(id)
        .execute(self.inner.board.store().pool())
        .await?;
        if let Some(session) = agent.session.as_ref().filter(|s| s.is_running()) {
            self.inner
                .sessions
                .stop(&session.id)
                .await
                .map_err(|e| BoardError::refused(Refusal::SessionEnded, e.to_string()))?;
        }
        tracing::info!(agent = id, "project agent closed");
        self.changed(&agent.project, id);
        Ok(())
    }

    /// **Close ended agents** (SHL-04, d0087): every Project agent of `project` that has ended (Ended, Ended by
    /// restart, Paused) and isn't closed yet is closed; it leaves the switcher and stays in Recently ended. Running agents
    /// and epic leads are left alone. The project's list afterwards.
    pub async fn close_ended(&self, project: &str) -> Result<ProjectAgentList, BoardError> {
        let project = self.inner.board.project(project).await?;
        let closed: Vec<(String,)> = sqlx::query_as(
            "UPDATE project_agents SET closed_at = ?
             WHERE project_id = ? AND epic_id IS NULL AND closed_at IS NULL AND state IN (?, ?, ?)
             RETURNING id",
        )
        .bind(now_ms())
        .bind(&project.id)
        .bind(AgentState::Ended.as_str())
        .bind(AgentState::EndedByRestart.as_str())
        .bind(AgentState::Paused.as_str())
        .fetch_all(self.inner.board.store().pool())
        .await?;
        tracing::info!(project = %project.key, closed = closed.len(), "ended project agents closed");
        for (id,) in &closed {
            self.changed(&project.key, id);
        }
        self.list(&project.key).await
    }

    /// The user's rename (PAG-04): always wins, and the agent can't change it afterwards.
    pub async fn rename(&self, id: &str, name: &str) -> Result<ProjectAgent, BoardError> {
        let name = clean_name(name)?;
        let agent = self.get(id).await?;
        lead::refuse_naming(&agent)?;
        self.set_name(id, &name, NameSource::User).await?;
        self.changed(&agent.project, id);
        self.get(id).await
    }

    /// `otaq agent name "<name>"` from the Project agent `id` (PAG-04): once, again only with `rename` (the
    /// user asked it to), never over the user's own name. A fallback name isn't its own, so it may replace it.
    pub async fn name_by_agent(
        &self,
        id: &str,
        name: &str,
        rename: bool,
    ) -> Result<ProjectAgent, BoardError> {
        let name = clean_name(name)?;
        let agent = self.get(id).await?;
        lead::refuse_naming(&agent)?;
        let refuse = |message: String| Err(BoardError::refused(Refusal::NotAllowed, message));
        match agent.name_source {
            Some(NameSource::User) => {
                return refuse(format!(
                    "The user named this Project agent \"{}\"; their name stays",
                    agent.name
                ));
            }
            Some(NameSource::Agent) if !rename => {
                return refuse(format!(
                    "This Project agent is already named \"{}\". Rename it only when the user asks \
                     (`{} agent name --rename \"<name>\"`)",
                    agent.name,
                    naming::COMMAND_NAME
                ));
            }
            _ => {}
        }
        self.set_name(id, &name, NameSource::Agent).await?;
        self.changed(&agent.project, id);
        self.get(id).await
    }

    /// The project-agent skill as the agent `id`'s tool names it (ADP-05); a new session's tool when the agent
    /// is gone.
    async fn skill_invocation(&self, id: &str) -> String {
        let tool = self.get(id).await.ok().map(|agent| agent.tool);
        self.inner
            .adapters
            .of_session(tool)
            .skill_reference(PROJECT_AGENT_SKILL)
    }

    /// A hook signal of the Project agent `id` (§11.4 M12 rows): its first request is kept (open already recorded
    /// it, d0087; an epic lead opened without a message gets it here), the fallback name is
    /// given at the end of its first turn after it, and the plan limit's stop is recorded.
    pub async fn hook(&self, id: &str, signal: &Signal) -> Result<(), BoardError> {
        let pool = self.inner.board.store().pool();
        let mut changed = false;
        if let Signal::Working {
            prompt: Some(prompt),
        } = signal
            && let Some(request) = request_text(prompt, &self.skill_invocation(id).await)
        {
            changed |= sqlx::query(
                "UPDATE project_agents SET first_request = ? WHERE id = ? AND first_request IS NULL",
            )
            .bind(&request)
            .bind(id)
            .execute(pool)
            .await?
            .rows_affected()
                > 0;
        }
        if matches!(signal, Signal::Working { .. }) {
            changed |= sqlx::query(
                "UPDATE project_agents SET limit_stopped = 0 WHERE id = ? AND limit_stopped = 1",
            )
            .bind(id)
            .execute(pool)
            .await?
            .rows_affected()
                > 0;
        }
        // A permission prompt waits for the user until the turn moves on (PAG-07); Project agents are never Idle.
        match signal {
            Signal::NeedsYou { prompt } => {
                self.inner
                    .waiting
                    .lock()
                    .expect("waiting agents")
                    .insert(id.to_owned(), (prompt.clone(), now_ms()));
                changed = true;
            }
            s if matches!(s, Signal::Working { .. }) || s.ends_turn() => {
                changed |= self.clear_waiting(id);
            }
            _ => {}
        }
        if let Signal::LimitReached { .. } = signal {
            sqlx::query("UPDATE project_agents SET limit_stopped = 1 WHERE id = ?")
                .bind(id)
                .execute(pool)
                .await?;
            changed = true;
        }
        if signal.ends_turn() {
            self.inner
                .turn_ends
                .lock()
                .expect("turn ends")
                .insert(id.to_owned(), now_ms());
            let unnamed: Option<(Option<String>,)> = sqlx::query_as(
                "SELECT first_request FROM project_agents
                 WHERE id = ? AND name_source IS NULL AND epic_id IS NULL",
            )
            .bind(id)
            .fetch_optional(pool)
            .await?;
            if let Some((Some(request),)) = unnamed
                && let Some(name) = fallback_name(&request)
            {
                sqlx::query(
                    "UPDATE project_agents SET name = ?, name_source = ? WHERE id = ? AND name_source IS NULL",
                )
                .bind(&name)
                .bind(NameSource::Fallback.as_str())
                .bind(id)
                .execute(pool)
                .await?;
            }
            // Its turn ended: mid-turn changed, and maybe its name.
            changed = true;
        }
        changed |= self.lead_signal(id, signal).await;
        if changed || matches!(signal, Signal::Working { .. }) {
            let project: Option<(String,)> = sqlx::query_as(
                "SELECT p.key FROM project_agents a JOIN projects p ON p.id = a.project_id WHERE a.id = ?",
            )
            .bind(id)
            .fetch_optional(pool)
            .await?;
            if let Some((project,)) = project {
                self.changed(&project, id);
            }
        }
        Ok(())
    }

    /// `otaq ask human --stdin` from the Project agent `id` (PAG-02, d0061): the question, with 2–4 options
    /// or free text, waits on the Project agent for the user's answer.
    pub async fn ask_human(
        &self,
        id: &str,
        question: &str,
        options: &[String],
        notes: &[Option<String>],
        recommended: Option<u32>,
    ) -> Result<ProjectAgent, BoardError> {
        let question = question.trim();
        if question.is_empty() {
            return Err(BoardError::refused(
                Refusal::BadQuestion,
                "Write the question on stdin",
            ));
        }
        let agent = self.get(id).await?;
        self.inner
            .board
            .store_agent_question(id, question, options, notes, recommended)
            .await?;
        tracing::info!(agent = id, "project agent asks the user");
        self.changed(&agent.project, id);
        self.get(id).await
    }

    /// The user's answer to the Project agent's question, typed into its session as the user's click (with
    /// unsent text in its input line and no `clear`: `needs_confirmation`, d0048).
    pub async fn answer(
        &self,
        id: &str,
        choice: &Choice,
        clear: bool,
    ) -> Result<Delivery, BoardError> {
        let agent = self.get(id).await?;
        let question = agent.question.as_ref().ok_or_else(|| {
            BoardError::refused(
                Refusal::MoveRefused,
                format!("{} isn't waiting for an answer", agent.name),
            )
        })?;
        let text = crate::attention::inbox::answer_text(Some(question), choice)?;
        let session = agent
            .session
            .as_ref()
            .filter(|s| s.is_running())
            .ok_or_else(|| {
                BoardError::refused(
                    Refusal::SessionEnded,
                    format!("{}'s session has ended", agent.name),
                )
            })?;
        let deliveries = &self.inner.deliveries;
        let typed = if clear {
            deliveries.clear_and_send(&session.id, Some(&text)).await
        } else {
            deliveries.deliver_from_click(&session.id, &text).await
        };
        let delivery =
            typed.map_err(|e| BoardError::refused(Refusal::SessionEnded, e.to_string()))?;
        if matches!(delivery, Delivery::Sent { .. }) {
            self.inner.board.record_answer(&question.id, &text).await?;
            self.changed(&agent.project, id);
        }
        Ok(delivery)
    }

    /// The user's message for a running Project agent, typed into its session as the user's click: the Ask bar's
    /// Continue (SHB-03, d0113). With unsent text in its input line: `needs_confirmation` (d0048), nothing typed.
    pub async fn send(&self, id: &str, message: &str) -> Result<Delivery, BoardError> {
        if message.trim().is_empty() {
            return Err(BoardError::refused(
                Refusal::EmptyField,
                "Type a message to send",
            ));
        }
        let agent = self.get(id).await?;
        let session = agent
            .session
            .as_ref()
            .filter(|s| s.is_running())
            .ok_or_else(|| {
                BoardError::refused(
                    Refusal::SessionEnded,
                    format!("{}'s session has ended", agent.name),
                )
            })?;
        let delivery = self
            .inner
            .deliveries
            .deliver_from_click(&session.id, message.trim())
            .await
            .map_err(|e| BoardError::refused(Refusal::SessionEnded, e.to_string()))?;
        if matches!(delivery, Delivery::Sent { .. }) {
            // Continue answers its open question (d0119).
            self.answered_in_terminal(id, message).await?;
            self.changed(&agent.project, id);
        }
        Ok(delivery)
    }

    /// What the Project agent created and changed (PAG-05), from its events.
    pub async fn created_and_changed(&self, id: &str) -> Result<CreatedAndChanged, BoardError> {
        let agent = self.get(id).await?;
        let actor = Actor::ProjectAgent(id.to_owned()).to_string();
        let events: Vec<(String, String)> =
            sqlx::query_as("SELECT ticket_id, kind FROM events WHERE actor = ? ORDER BY at, id")
                .bind(&actor)
                .fetch_all(self.inner.board.store().pool())
                .await?;
        let tickets = self.inner.board.tickets(&agent.project).await?;
        let by_id = |ticket: &str| tickets.iter().find(|t| t.id == ticket);
        let mut out = CreatedAndChanged::default();
        let mut created_ids: Vec<&str> = Vec::new();
        for (ticket, kind) in &events {
            match kind.as_str() {
                "created" => {
                    if let Some(t) = by_id(ticket) {
                        created_ids.push(&t.id);
                        out.created.push(CreatedTicket {
                            key: t.key.clone(),
                            title: t.title.clone(),
                            status: t.status,
                            deletable: t.board_group() == StatusSet::Backlog && !t.has_started(),
                        });
                    }
                }
                "dependency_added" => out.dependencies += 1,
                _ => {}
            }
        }
        for (ticket, kind) in &events {
            if kind == "updated"
                && !created_ids.contains(&ticket.as_str())
                && let Some(t) = by_id(ticket)
                && !out.edited.contains(&t.key)
            {
                out.edited.push(t.key.clone());
            }
        }
        out.edited.sort();
        out.summary = summary(&out);
        Ok(out)
    }

    /// The Project agent is at it again (an `otaq` command of it that acts, or Enter in its terminal): a permission
    /// prompt it waited on was answered (ATT-02).
    pub async fn active(&self, id: &str) -> Result<(), BoardError> {
        if self.clear_waiting(id) {
            let agent = self.get(id).await?;
            self.changed(&agent.project, id);
        }
        Ok(())
    }

    /// Enter in a session's terminal: if it is a Project agent's, it is active. `line` (the first line sent that
    /// isn't empty) answers its open question unless a permission prompt shows on top of it (d0119).
    pub async fn enter(&self, session: &str, line: Option<&str>) -> Result<(), BoardError> {
        let agent: Option<(Option<String>,)> =
            sqlx::query_as("SELECT project_agent_id FROM sessions WHERE id = ?")
                .bind(session)
                .fetch_optional(self.inner.board.store().pool())
                .await?;
        match agent {
            Some((Some(agent),)) => {
                self.lead_typed(&agent);
                let prompt = self
                    .inner
                    .waiting
                    .lock()
                    .expect("waiting agents")
                    .contains_key(&agent);
                if let Some(line) = line.filter(|_| !prompt) {
                    self.answered_in_terminal(&agent, line).await?;
                }
                self.active(&agent).await
            }
            _ => Ok(()),
        }
    }

    /// The user sent `line` to the Project agent `id` (Enter in its terminal, or the Ask bar's Continue): it answers
    /// the agent's open question, if any (d0119).
    async fn answered_in_terminal(&self, id: &str, line: &str) -> Result<(), BoardError> {
        if let Some(question) = self.inner.board.answer_agent_in_terminal(id, line).await? {
            tracing::info!(
                agent = id,
                question = question.id,
                "question answered in the terminal"
            );
            let agent = self.get(id).await?;
            self.changed(&agent.project, id);
        }
        Ok(())
    }

    fn clear_waiting(&self, id: &str) -> bool {
        self.inner
            .waiting
            .lock()
            .expect("waiting agents")
            .remove(id)
            .is_some()
    }

    /// Every running Project agent that waits for the user (PAG-07), oldest first: a question or a permission
    /// prompt, unless its terminal has focus (ATT-03).
    pub async fn needs_you(&self) -> Result<Vec<AgentNeedsYou>, BoardError> {
        let rows: Vec<Row> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "{SELECT} WHERE a.state = ? ORDER BY a.created_at, a.id"
        )))
        .bind(AgentState::Running.as_str())
        .fetch_all(self.inner.board.store().pool())
        .await?;
        let mut items = Vec::new();
        for row in rows {
            let agent = self.agent(row).await?;
            if agent.focused {
                continue;
            }
            let project = self.inner.board.project(&agent.project).await?;
            let item = |status, reason: String, question, since| AgentNeedsYou {
                project: project.key.clone(),
                project_name: project.name.clone(),
                agent: agent.id.clone(),
                name: agent.name.clone(),
                epic: agent.epic.clone(),
                status,
                reason,
                question,
                since,
            };
            // A permission prompt shows on top of its question (d0119).
            if let Some(prompt) = &agent.needs_you {
                let since = self
                    .inner
                    .waiting
                    .lock()
                    .expect("waiting agents")
                    .get(&agent.id)
                    .map_or(0, |(_, at)| *at);
                items.push(item(
                    Status::NeedsYou,
                    format!("permission: {}", prompt.trim()),
                    None,
                    since,
                ));
            } else if let Some(q) = &agent.question {
                items.push(item(
                    Status::AsksYou,
                    format!("asks: {}", q.text.trim()),
                    Some(q.clone()),
                    q.asked_at,
                ));
            }
        }
        items.sort_by(|a, b| a.since.cmp(&b.since).then(a.agent.cmp(&b.agent)));
        Ok(items)
    }

    /// A project's open Project agents and its most recent ended ones.
    pub async fn list(&self, project: &str) -> Result<ProjectAgentList, BoardError> {
        let project = self.inner.board.project(project).await?;
        let pool = self.inner.board.store().pool();
        let open: Vec<Row> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "{SELECT} WHERE a.project_id = ? AND a.closed_at IS NULL AND a.epic_id IS NULL
             ORDER BY a.created_at, a.id"
        )))
        .bind(&project.id)
        .fetch_all(pool)
        .await?;
        // `first_request IS NOT NULL` leaves out "New agent"s that never got a request: unreachable since d0087
        // (open records it), kept for rows a data folder may still hold from before.
        let recent: Vec<Row> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "{SELECT} WHERE a.project_id = ? AND a.state <> ? AND a.first_request IS NOT NULL
               AND a.epic_id IS NULL
             ORDER BY a.ended_at DESC, a.id DESC LIMIT ?"
        )))
        .bind(&project.id)
        .bind(AgentState::Running.as_str())
        .bind(RECENT)
        .fetch_all(pool)
        .await?;
        let mut list = ProjectAgentList::default();
        for row in open {
            list.open.push(self.agent(row).await?);
        }
        for row in recent {
            list.recent.push(self.agent(row).await?);
        }
        list.holds = self.inner.board.store().project_holds(&project.id).await?;
        Ok(list)
    }

    /// One Project agent.
    pub async fn get(&self, id: &str) -> Result<ProjectAgent, BoardError> {
        let row: Row = sqlx::query_as(sqlx::AssertSqlSafe(format!("{SELECT} WHERE a.id = ?")))
            .bind(id)
            .fetch_optional(self.inner.board.store().pool())
            .await?
            .ok_or_else(|| BoardError::NotFound(format!("no Project agent {id}")))?;
        self.agent(row).await
    }

    /// The running Project agents stopped by the plan limit, with their sessions (their tool's Resume continues them).
    pub async fn limit_stopped(&self) -> Result<Vec<(String, String)>, BoardError> {
        let rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT a.id, s.id FROM project_agents a JOIN sessions s ON s.project_agent_id = a.id
             WHERE a.limit_stopped = 1 AND a.state = ? AND s.ended_at IS NULL",
        )
        .bind(AgentState::Running.as_str())
        .fetch_all(self.inner.board.store().pool())
        .await?;
        Ok(rows)
    }

    async fn agent(&self, row: Row) -> Result<ProjectAgent, BoardError> {
        let newest: Option<(String,)> = sqlx::query_as(
            "SELECT id FROM sessions WHERE project_agent_id = ? ORDER BY started_at DESC, id DESC LIMIT 1",
        )
        .bind(&row.id)
        .fetch_optional(self.inner.board.store().pool())
        .await?;
        let session = match newest {
            Some((session,)) => match self.inner.sessions.info(&session) {
                Ok(info) => Some(info),
                Err(_) => self.inner.sessions.stored(&session).await.ok(),
            },
            None => None,
        };
        let mid_turn = session
            .as_ref()
            .is_some_and(|s| s.is_running() && !self.inner.idle.is_idle(&s.id));
        let name_source = row.name_source.and_then(|s| s.parse().ok());
        let question = self.inner.board.open_agent_question(&row.id).await?;
        let running = session.as_ref().is_some_and(SessionInfo::is_running);
        let needs_you = self
            .inner
            .waiting
            .lock()
            .expect("waiting agents")
            .get(&row.id)
            .map(|(prompt, _)| prompt.clone())
            .filter(|_| running);
        let focused = session
            .as_ref()
            .filter(|s| s.is_running())
            .is_some_and(|s| self.inner.sessions.is_focused(&s.id).unwrap_or(false));
        let epic = match &row.epic_id {
            Some(epic_id) => Some(self.inner.board.ticket_by_id(epic_id).await?),
            None => None,
        };
        if let Some(epic) = &epic {
            self.inner
                .lead_epics
                .lock()
                .expect("lead epics")
                .insert(row.id.clone(), (row.project_key.clone(), epic.key.clone()));
        }
        let usage: (Option<i64>, Option<i64>, Option<i64>, Option<i64>) = sqlx::query_as(
            "SELECT SUM(u.input), SUM(u.cached), SUM(u.output), SUM(u.reasoning)
             FROM usage u JOIN sessions s ON s.id = u.session_id WHERE s.project_agent_id = ?",
        )
        .bind(&row.id)
        .fetch_one(self.inner.board.store().pool())
        .await?;
        let turn_ended_at = self
            .inner
            .turn_ends
            .lock()
            .expect("turn ends")
            .get(&row.id)
            .copied();
        Ok(ProjectAgent {
            question,
            needs_you,
            focused,
            turn_ended_at,
            tokens: crate::usage::tokens(usage),
            name: match &epic {
                Some(epic) => lead::lead_name(&epic.title),
                None => row
                    .name
                    .filter(|_| name_source.is_some())
                    .unwrap_or_else(|| NEW_AGENT.to_owned()),
            },
            name_source: name_source.filter(|_| epic.is_none()),
            epic: epic.map(|e| e.key),
            state: row.state.parse().unwrap_or(AgentState::Ended),
            id: row.id,
            project: row.project_key,
            tool: row
                .tool
                .parse()
                .map_err(|e| sqlx::Error::Decode(Box::new(e)))?,
            tool_session_id: row.tool_session_id,
            model: row.model,
            got_request: row.first_request.is_some(),
            mid_turn,
            limit_stopped: row.limit_stopped,
            created_at: row.created_at,
            ended_at: row.ended_at,
            closed: row.closed_at.is_some(),
            session,
        })
    }

    #[allow(clippy::too_many_arguments)]
    async fn spawn(
        &self,
        project: &Project,
        launched: Launched,
        id: &str,
        size: TerminalSize,
        runs_on: &RunsOn,
        lead: Option<(&str, &TicketKey)>,
        start: Start,
    ) -> Result<SessionInfo, BoardError> {
        let sessions = &self.inner.sessions;
        let session = sessions
            .prepared(launched.prepare)
            .spawn(SpawnSpec {
                kind: SessionKind::ProjectAgent,
                start,
                project_id: project.id.clone(),
                project_key: project.key.clone(),
                ticket: None,
                program: launched.plan.program,
                args: launched.plan.args,
                cwd: launched.plan.cwd,
                env: launched.plan.env,
                size,
                tool: Some(launched.tool),
                tool_session_id: launched.tool_session_id,
                model: launched.model,
                agent: runs_on.agent.clone(),
                effort: runs_on.effort.clone(),
                fallback: runs_on.fallback.clone(),
                theme: Some(launched.theme),
            })
            .await
            .map_err(|e| BoardError::refused(Refusal::SessionEnded, e.to_string()))?;
        sessions
            .set_project_agent(&session.id, id)
            .await
            .map_err(|e| BoardError::refused(Refusal::SessionEnded, e.to_string()))?;
        if let Some((epic_id, epic)) = lead {
            sessions
                .set_epic_lead(&session.id, epic_id, epic)
                .map_err(|e| BoardError::refused(Refusal::SessionEnded, e.to_string()))?;
        }
        // The agent tool exiting on its own ends the agent; the service stopping doesn't (the next start reconciles);
        // a Pause makes it Paused (PAU-03).
        let this = self.clone();
        let (session_id, id, project) = (session.id.clone(), id.to_owned(), project.key.clone());
        tokio::spawn(async move {
            let Some(ended) = this.inner.sessions.wait_ended(&session_id).await else {
                return;
            };
            // Its skill copies leave the project's folder with it, whatever ended it (AGM-07).
            this.release_checkout(&id).await;
            if ended.end_reason.as_deref() == Some(END_SERVICE_STOPPED) {
                return;
            }
            let state = if ended.end_reason.as_deref() == Some(END_PAUSED) {
                AgentState::Paused
            } else {
                AgentState::Ended
            };
            match this.record_end(&id, &session_id, state).await {
                Ok(_) => this.changed(&project, &id),
                Err(e) => tracing::warn!(agent = id, error = %e, "can't record the agent's end"),
            }
            // A lead that ends hands back what it was sent (d0083).
            this.lead_session_ended(&id).await;
        });
        Ok(session)
    }

    /// Records that the agent `id`'s session `session_id` ended, as `state`, if that is its latest session and the
    /// agent still runs. Whether it did.
    async fn record_end(
        &self,
        id: &str,
        session_id: &str,
        state: AgentState,
    ) -> Result<bool, BoardError> {
        let done = sqlx::query(
            "UPDATE project_agents SET state = ?, ended_at = ?
             WHERE id = ? AND state = ?
               AND ? = (SELECT id FROM sessions WHERE project_agent_id = ?
                        ORDER BY started_at DESC, id DESC LIMIT 1)",
        )
        .bind(state.as_str())
        .bind(now_ms())
        .bind(id)
        .bind(AgentState::Running.as_str())
        .bind(session_id)
        .bind(id)
        .execute(self.inner.board.store().pool())
        .await?;
        Ok(done.rows_affected() > 0)
    }

    /// The Project agents and epic leads of the sessions a **Pause** ended (PAU-03, d0112) become Paused at once,
    /// whether or not their end watchers ran yet (they do the same). Their ids.
    pub async fn paused(&self, stopped: &[SessionInfo]) -> Result<Vec<String>, BoardError> {
        let mut paused = Vec::new();
        for session in stopped
            .iter()
            .filter(|s| s.kind == SessionKind::ProjectAgent)
        {
            let agent: Option<(String,)> = sqlx::query_as(
                "SELECT project_agent_id FROM sessions WHERE id = ? AND project_agent_id IS NOT NULL",
            )
            .bind(&session.id)
            .fetch_optional(self.inner.board.store().pool())
            .await?;
            let Some((id,)) = agent else {
                continue;
            };
            if self
                .record_end(&id, &session.id, AgentState::Paused)
                .await?
            {
                self.changed(&session.project, &id);
            }
            paused.push(id);
        }
        Ok(paused)
    }

    /// The Project agents and epic leads of `project` a Pause ended (Paused), oldest first: what its Resume resumes
    /// (PAU-06).
    pub async fn paused_of(&self, project: &str) -> Result<Vec<String>, BoardError> {
        let project = self.inner.board.project(project).await?;
        let ids: Vec<(String,)> = sqlx::query_as(
            "SELECT id FROM project_agents WHERE project_id = ? AND state = ? ORDER BY created_at, id",
        )
        .bind(&project.id)
        .bind(AgentState::Paused.as_str())
        .fetch_all(self.inner.board.store().pool())
        .await?;
        Ok(ids.into_iter().map(|(id,)| id).collect())
    }

    async fn set_name(&self, id: &str, name: &str, source: NameSource) -> Result<(), BoardError> {
        sqlx::query("UPDATE project_agents SET name = ?, name_source = ? WHERE id = ?")
            .bind(name)
            .bind(source.as_str())
            .bind(id)
            .execute(self.inner.board.store().pool())
            .await?;
        Ok(())
    }

    async fn set_state(&self, id: &str, state: AgentState) -> Result<(), BoardError> {
        sqlx::query("UPDATE project_agents SET state = ? WHERE id = ?")
            .bind(state.as_str())
            .bind(id)
            .execute(self.inner.board.store().pool())
            .await?;
        Ok(())
    }

    /// A resume on `tool` while that tool is at its limit is refused; new sessions are the gate's.
    async fn refuse_during_limit(&self, tool: Option<Tool>) -> Result<(), BoardError> {
        if self.limit_held_on(tool).await? {
            return Err(BoardError::refused(Refusal::MoveRefused, LIMIT_REFUSAL));
        }
        Ok(())
    }

    /// Whether `tool`'s usage limit holds (USG-10; `None`: the new-session tool): only that tool's agents wait.
    pub(crate) async fn limit_held_on(&self, tool: Option<Tool>) -> Result<bool, BoardError> {
        let tool = tool.unwrap_or_else(|| self.inner.adapters.for_new_session().tool());
        Ok(crate::usage::plan::limit_of(self.inner.board.store(), tool)
            .await?
            .is_some_and(|l| l.reset_seen_at.is_none()))
    }

    /// Builds the launch of `request` on `tool` (a resume: the tool the conversation ran on).
    fn launch(&self, tool: Option<Tool>, request: LaunchRequest) -> Result<Launched, BoardError> {
        Ok(self.inner.adapters.launch(tool, &request)?)
    }

    fn changed(&self, project: &str, id: &str) {
        self.inner.events.publish(Event::ProjectAgentChanged {
            project: project.to_owned(),
            id: id.to_owned(),
        });
        let lead = self
            .inner
            .lead_epics
            .lock()
            .expect("lead epics")
            .get(id)
            .cloned();
        if let Some((project, epic)) = lead {
            self.inner
                .events
                .publish(Event::LeadChanged { project, epic });
        }
    }
}

/// A new conversation in `project`'s own folder for `role`, with no ticket and no port, told the theme in use; the
/// caller sets the epic, or the conversation and model to resume.
async fn request(
    board: &Board,
    project: &Project,
    role: LaunchRole,
    first_prompt: Option<FirstPrompt>,
) -> LaunchRequest {
    LaunchRequest {
        role,
        cwd: std::path::PathBuf::from(&project.repo_path),
        project: project.key.clone(),
        ticket: None,
        epic: None,
        port: 0,
        first_prompt,
        resume: None,
        model: None,
        effort: None,
        title: None,
        theme: board.theme_in_use().await,
    }
}

/// "Created SHOP-35, SHOP-36 · edited SHOP-31 · 2 dependencies"; "Nothing created or changed yet".
fn summary(c: &CreatedAndChanged) -> String {
    let keys = |keys: Vec<&TicketKey>| {
        keys.iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    };
    let mut parts = Vec::new();
    if !c.created.is_empty() {
        parts.push(format!(
            "Created {}",
            keys(c.created.iter().map(|t| &t.key).collect())
        ));
    }
    if !c.edited.is_empty() {
        parts.push(format!("edited {}", keys(c.edited.iter().collect())));
    }
    match c.dependencies {
        0 => {}
        1 => parts.push("1 dependency".to_owned()),
        n => parts.push(format!("{n} dependencies")),
    }
    if parts.is_empty() {
        return "Nothing created or changed yet".to_owned();
    }
    let mut text = parts.join(" · ");
    if let Some(first) = text.get(..1) {
        text = first.to_uppercase() + &text[1..];
    }
    text
}

/// The request in a UserPromptSubmit prompt: the prompt without `skill`, the invocation that starts the session
/// (`/otaq-project-agent`, alone it is no request). `None` when nothing is left.
pub fn request_text(prompt: &str, skill: &str) -> Option<String> {
    let prompt = prompt.trim();
    let rest = prompt.strip_prefix(skill).unwrap_or(prompt).trim();
    (!rest.is_empty()).then(|| rest.to_owned())
}

/// The fallback name (PAG-04): the request's first words, at most 40 characters, without trailing
/// punctuation. `None` for a request without words.
pub fn fallback_name(request: &str) -> Option<String> {
    let words: Vec<&str> = request.split_whitespace().take(FALLBACK_WORDS).collect();
    let mut name: String = words.join(" ").chars().take(NAME_MAX_CHARS).collect();
    name = name
        .trim_end_matches(|c: char| c.is_ascii_punctuation() || c.is_whitespace())
        .to_owned();
    (!name.is_empty()).then_some(name)
}

/// A name as given: one line, spaces collapsed, at most 40 characters; refused when empty.
fn clean_name(name: &str) -> Result<String, BoardError> {
    let name: String = name
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(NAME_MAX_CHARS)
        .collect();
    if name.is_empty() {
        return Err(BoardError::refused(
            Refusal::EmptyField,
            "A Project agent's name can't be empty",
        ));
    }
    Ok(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_skill_invocation_is_no_request() {
        let skill = format!("/{}", naming::copy_name(PROJECT_AGENT_SKILL));
        assert_eq!(request_text(&skill, &skill), None);
        assert_eq!(request_text(&format!("  {skill}  "), &skill), None);
        assert_eq!(
            request_text(&format!("{skill} why is checkout slow?"), &skill).as_deref(),
            Some("why is checkout slow?")
        );
        assert_eq!(
            request_text("Import the GitHub issues labelled ready", &skill).as_deref(),
            Some("Import the GitHub issues labelled ready")
        );
        assert_eq!(request_text("   ", &skill), None);
    }

    #[test]
    fn slash_flow_runs_the_flow_skill() {
        assert_eq!(first_skill("/flow"), ("flow", None));
        assert_eq!(
            first_skill(" /flow  add a Security check "),
            ("flow", Some("add a Security check".to_owned()))
        );
        assert_eq!(
            first_skill("/flowers are nice"),
            (PROJECT_AGENT_SKILL, Some("/flowers are nice".to_owned()))
        );
        assert_eq!(
            first_skill("Why is checkout slow?"),
            (
                PROJECT_AGENT_SKILL,
                Some("Why is checkout slow?".to_owned())
            )
        );
    }

    #[test]
    fn the_fallback_is_the_first_words() {
        assert_eq!(
            fallback_name("Why is checkout so slow? Make tickets for it").as_deref(),
            Some("Why is checkout so")
        );
        assert_eq!(
            fallback_name("Footer audit.").as_deref(),
            Some("Footer audit")
        );
        assert_eq!(
            fallback_name("Supercalifragilisticexpialidocious-and-then-some-more-text")
                .map(|n| n.len()),
            Some(40)
        );
        assert_eq!(fallback_name(" ?! "), None);
    }

    #[test]
    fn the_summary_names_what_it_did() {
        let key = |k: &str| k.parse::<TicketKey>().unwrap();
        let created = |k: &str| CreatedTicket {
            key: key(k),
            title: "t".into(),
            status: Status::Backlog,
            deletable: true,
        };
        let mut c = CreatedAndChanged::default();
        assert_eq!(summary(&c), "Nothing created or changed yet");
        c.created = vec![created("SHOP-35"), created("SHOP-36")];
        c.edited = vec![key("SHOP-31")];
        c.dependencies = 2;
        assert_eq!(
            summary(&c),
            "Created SHOP-35, SHOP-36 · edited SHOP-31 · 2 dependencies"
        );
        c.created.clear();
        c.dependencies = 1;
        assert_eq!(summary(&c), "Edited SHOP-31 · 1 dependency");
    }

    #[test]
    fn names_are_one_line_and_not_empty() {
        assert_eq!(
            clean_name("  Checkout \n slowness ").unwrap(),
            "Checkout slowness"
        );
        assert_eq!(
            clean_name("  ").unwrap_err().refusal(),
            Some(Refusal::EmptyField)
        );
    }
}
