//! Asking a tool for a flow during Flow setup (ASK-02, ASK-03, ASK-04; d0108 "Each connected tool's agent looks for
//! itself, when the user asks", d0086, d0068).
//!
//! - **Asking** ([`ProjectAgents::ask_for_flow`]): the user's **Ask <Tool>** for a project in Flow setup starts a
//!   **flow agent**: a Project agent with the Project agent role's agent (its instructions and skills, AGM-07),
//!   launched on the asked tool with **no model option** (the tool's own default, "tool default", d0086), on the
//!   `flow` skill with [`first_flow_message`]. It is named [`flow_agent_name`] ("Flow setup · OpenCode") from the
//!   start. Only a connected tool is asked, never at its plan limit; nothing starts on its own. One flow agent per
//!   tool per project: asking again returns the running one.
//! - **Where it stands** ([`ProjectAgents::flow_setup`]): one row per connected (or found, not signed in) tool and
//!   the open proposals, for the go-ahead, the agents at work and the proposals side by side. Clients refetch it on
//!   [`Event::FlowSetupChanged`], which [`ProjectAgents::relay_flow_setup`] sends whenever a flow agent, a proposal,
//!   a tool's state or a limit of a project in Flow setup changes.
//! - **Settling** ([`ProjectAgents::settle_flow_setup`]): Use this flow, a preset applied, Use the default or Skip
//!   ends Flow setup. Every other open proposal closes, and every other flow agent is told
//!   [`CHOSE_ANOTHER_FLOW`] (typed as the user's click is, never over unsent text) and ends to Recently ended once
//!   its turn on that answer ends ([`ANSWER_GRACE`] at most); one that isn't running, or has unsent text, ends at
//!   once.

use std::sync::atomic::Ordering;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::watch;

use super::{AgentState, FLOW_COMMAND, ProjectAgent, ProjectAgents, first_skill};
use crate::agent_tools::Tool;
use crate::agent_tools::connected::{AgentToolState, ToolStatus};
use crate::agents::{Agent, ModelCatalog, Role, RunsOn, resolve};
use crate::board::{BoardError, Refusal};
use crate::flow::apply::FlowEvent;
use crate::flow::proposal::{Proposal, ProposedBy};
use crate::protocol::Event;
use crate::store::projects::Project;

/// What a flow agent that wasn't chosen is told before it ends (ASK-03).
pub const CHOSE_ANOTHER_FLOW: &str = "The user chose another flow.";

/// How long an unchosen flow agent may take to read [`CHOSE_ANOTHER_FLOW`] and end its turn before it ends anyway
/// (a tool that never reports its turn's end).
pub const ANSWER_GRACE: Duration = Duration::from_secs(30);

/// How often the end of that turn is looked for.
const TURN_POLL: Duration = Duration::from_millis(100);

/// Why asking a tool that is found but signed out is refused: the row offers Sign in… (ASK-02).
pub const NOT_SIGNED_IN: &str = "Couldn\u{2019}t start \u{b7} not signed in";

/// The fixed part of [`first_flow_message`] that marks a Project agent as a flow agent.
const FIRST_FLOW_MARK: &str = "'s first flow. Look at how the user works with ";

/// A flow agent's name: "Flow setup · OpenCode".
pub fn flow_agent_name(tool: Tool) -> String {
    format!("Flow setup \u{b7} {}", tool.display_name())
}

/// Why asking for a project whose flow is settled is refused.
pub fn not_in_flow_setup(project_name: &str) -> String {
    format!("{project_name}'s flow is set up already")
}

/// A flow agent's first message (ASK-02): `/flow`, then what to look at, the same for every tool, with the tool and
/// the project filled in. Otaq checks no sources: the agent finds its own tool's history itself.
pub fn first_flow_message(project_name: &str, tool: Tool) -> String {
    format!(
        "{FLOW_COMMAND} This is {project_name}{FIRST_FLOW_MARK}{tool}: your own history on this Mac (find it \
         yourself; read only what the user typed, never login or credential data) and this repo (`git log`, branch \
         names, CI files, PR templates, `CONTRIBUTING.md`, and its AI files: `AGENTS.md`, `CLAUDE.md`, `.claude/`, \
         `.cursor/rules` and the like). Nothing else: not GitHub or GitLab, not skills or plugins in the home \
         folder. Then propose a whole flow.",
        tool = tool.display_name(),
    )
}

/// Whether a Project agent's first request is a flow agent's ([`first_flow_message`]).
pub fn is_flow_agent_request(first_request: &str) -> bool {
    let (skill, asks) = first_skill(first_request);
    skill == "flow"
        && asks.is_some_and(|a| a.starts_with("This is ") && a.contains(FIRST_FLOW_MARK))
}

/// What a flow agent runs on (ASK-02, d0086): the asked tool with no model option and no effort (the role agent's
/// effort belongs to its own model), as the role's agent.
pub fn flow_runs_on(agent: &Agent, tool: Tool) -> RunsOn {
    RunsOn {
        tool: Some(tool),
        model: None,
        effort: None,
        agent: Some(agent.session_agent()),
        fallback: None,
    }
}

/// A project's Flow setup now (`onboarding.flow_setup`): every tool that can be asked with its row, and the open
/// proposals.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlowSetup {
    /// The project's key and name.
    pub project: String,
    pub project_name: String,
    /// Since when it is in Flow setup; `None` once its flow is settled.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since: Option<i64>,
    /// One row per connected tool and per tool found but not signed in, in d0086's order.
    pub tools: Vec<FlowSetupTool>,
    /// The open proposals, oldest first: one per flow agent, and the user's own (a preset being looked at).
    pub proposals: Vec<Proposal>,
}

/// One tool's row in Flow setup (ASK-06).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlowSetupTool {
    pub tool: Tool,
    #[serde(flatten)]
    pub row: FlowSetupRow,
}

/// Where a tool's flow agent stands (ASK-06). `agent` is the flow agent's id (its terminal, Stop, Try again).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "row", rename_all = "snake_case")]
pub enum FlowSetupRow {
    /// Not asked yet: **Ask <Tool>**.
    NotAsked,
    /// Its flow agent works, since its session started: "Working · 2 min".
    Working { agent: String, since: i64 },
    /// The tool is at its plan limit, or its flow agent was stopped by it: "Plan limit · resets 3:45 pm". `agent`
    /// when one was asked (it picks up where it stopped).
    PlanLimit {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        agent: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        resets_at: Option<i64>,
    },
    /// Found, not signed in: "Couldn't start · not signed in", **Sign in…**.
    NotSignedIn,
    /// Its flow agent ended without a proposal: **Try again** resumes it.
    Ended { agent: String },
    /// Its flow agent proposed: the open proposal `proposal`.
    Proposed { agent: String, proposal: u32 },
}

/// What settling a Flow setup closed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Settled {
    /// The project was in Flow setup (and isn't any more).
    pub was_in_flow_setup: bool,
    /// The open proposals it closed.
    pub proposals: Vec<u32>,
    /// The flow agents it ended, by id.
    pub agents: Vec<String>,
}

impl ProjectAgents {
    /// **Ask <Tool>** for `project` (ASK-02): starts the tool's flow agent, or returns it while it runs. Refused when
    /// the project isn't in Flow setup, when the tool isn't connected (`state`: its state on Settings → Agent tools),
    /// and at its plan limit.
    pub async fn ask_for_flow(
        &self,
        project: &str,
        tool: Tool,
        state: &AgentToolState,
    ) -> Result<ProjectAgent, BoardError> {
        let project = self.inner.board.project(project).await?;
        if project.flow_setup_since.is_none() {
            return Err(BoardError::refused(
                Refusal::NotAllowed,
                not_in_flow_setup(&project.name),
            ));
        }
        if let Some(running) = self.running_flow_agent(&project, tool).await? {
            return Ok(running);
        }
        match state {
            AgentToolState::Connected { .. } => {}
            AgentToolState::FoundNotSignedIn { .. } => {
                return Err(BoardError::refused(Refusal::NotAllowed, NOT_SIGNED_IN));
            }
            other => {
                let reason = other.unavailable_reason().unwrap_or_default();
                return Err(BoardError::refused(
                    Refusal::NotAllowed,
                    format!(
                        "{} isn\u{2019}t available \u{b7} {reason}",
                        tool.display_name()
                    ),
                ));
            }
        }
        self.refuse_during_limit(Some(tool)).await?;
        let agent = resolve::role_agent(self.inner.board.store(), Role::ProjectAgent).await?;
        let started = self
            .start(
                &project,
                &first_flow_message(&project.name, tool),
                &agent,
                &flow_runs_on(&agent, tool),
                Some(&flow_agent_name(tool)),
            )
            .await?;
        tracing::info!(project = %project.key, %tool, agent = started.id, "flow agent asked");
        Ok(started)
    }

    /// `project`'s running flow agent on `tool`, if any.
    async fn running_flow_agent(
        &self,
        project: &Project,
        tool: Tool,
    ) -> Result<Option<ProjectAgent>, BoardError> {
        Ok(self
            .flow_agents(project)
            .await?
            .into_iter()
            .find(|a| a.tool == tool && a.state == AgentState::Running))
    }

    /// Every flow agent of `project`, newest first, closed ones too.
    async fn flow_agents(&self, project: &Project) -> Result<Vec<ProjectAgent>, BoardError> {
        let rows: Vec<(String, Option<String>)> = sqlx::query_as(
            "SELECT id, first_request FROM project_agents WHERE project_id = ?
             ORDER BY created_at DESC, id DESC",
        )
        .bind(&project.id)
        .fetch_all(self.inner.board.store().pool())
        .await?;
        let mut agents = Vec::new();
        for (id, first) in rows {
            if first.as_deref().is_some_and(is_flow_agent_request) {
                agents.push(self.get(&id).await?);
            }
        }
        Ok(agents)
    }

    /// `onboarding.flow_setup`: `project`'s Flow setup now, its tools' rows from `tools` (Settings → Agent tools'
    /// states) and its open proposals checked against `models`.
    pub async fn flow_setup(
        &self,
        project: &str,
        tools: &[ToolStatus],
        models: &dyn ModelCatalog,
    ) -> Result<FlowSetup, BoardError> {
        let board = &self.inner.board;
        let project = board.project(project).await?;
        let agents = self.flow_agents(&project).await?;
        let proposals = board.proposals(&project.key, models).await?;
        let mut rows = Vec::new();
        for status in tools {
            let signed_out = matches!(status.state, AgentToolState::FoundNotSignedIn { .. });
            if !status.state.is_connected() && !signed_out {
                continue;
            }
            let tool = status.tool;
            let agent = agents.iter().find(|a| a.tool == tool);
            let proposed = proposals
                .iter()
                .rev()
                .find(|p| p.proposed_by.tool() == Some(tool));
            let running = agent.is_some_and(|a| a.state == AgentState::Running);
            let row = if let Some((p, ProposedBy::ProjectAgent { agent, .. })) =
                proposed.map(|p| (p, &p.proposed_by))
            {
                FlowSetupRow::Proposed {
                    agent: agent.clone(),
                    proposal: p.id,
                }
            } else if agent.is_some_and(|a| running && a.limit_stopped)
                || (!running && self.limit_held_on(Some(tool)).await?)
            {
                FlowSetupRow::PlanLimit {
                    agent: agent.map(|a| a.id.clone()),
                    resets_at: crate::usage::plan::limit_of(board.store(), tool)
                        .await?
                        .and_then(|l| l.resets_at),
                }
            } else if signed_out && !running {
                FlowSetupRow::NotSignedIn
            } else if let Some(a) = agent.filter(|_| running) {
                FlowSetupRow::Working {
                    agent: a.id.clone(),
                    since: a.session.as_ref().map_or(a.created_at, |s| s.started_at),
                }
            } else if let Some(a) = agent {
                FlowSetupRow::Ended {
                    agent: a.id.clone(),
                }
            } else {
                FlowSetupRow::NotAsked
            };
            rows.push(FlowSetupTool { tool, row });
        }
        Ok(FlowSetup {
            project: project.key,
            project_name: project.name,
            since: project.flow_setup_since,
            tools: rows,
            proposals,
        })
    }

    /// Ends `project`'s Flow setup (ASK-03, ASK-04): its flow is settled (Use this flow, a preset applied, Use the
    /// default, Skip). Every open proposal left closes; every flow agent but `keep` (the chosen proposal's) is told
    /// [`CHOSE_ANOTHER_FLOW`] and ends to Recently ended. Nothing happens for a project that isn't in Flow setup.
    pub async fn settle_flow_setup(
        &self,
        project: &str,
        keep: Option<&str>,
    ) -> Result<Settled, BoardError> {
        let board = &self.inner.board;
        let project = board.project(project).await?;
        if project.flow_setup_since.is_none() {
            return Ok(Settled::default());
        }
        let mut settled = Settled {
            was_in_flow_setup: true,
            ..Settled::default()
        };
        for row in board.proposal_rows(&project.key).await? {
            board.drop_proposal(row.id).await?;
            settled.proposals.push(row.id);
        }
        for agent in self.flow_agents(&project).await? {
            if keep == Some(agent.id.as_str()) || agent.closed {
                continue;
            }
            let told = match agent.session.as_ref().filter(|s| s.is_running()) {
                Some(session) => self.tell_unchosen(&agent, &session.id).await,
                None => false,
            };
            if told {
                // It reads the answer and ends its turn, then it ends (in the background: the click returns now).
                let (this, id, session) = (self.clone(), agent.id.clone(), agent.session.clone());
                let session = session.map(|s| s.id).unwrap_or_default();
                tokio::spawn(async move { this.end_after_answer(&id, &session).await });
            } else {
                self.close(&agent.id, true).await?;
            }
            settled.agents.push(agent.id);
        }
        board.store().end_flow_setup(&project.id).await?;
        tracing::info!(project = %project.key, proposals = settled.proposals.len(),
                       agents = settled.agents.len(), "flow setup settled");
        let events = &self.inner.events;
        for &proposal in &settled.proposals {
            events.publish(Event::FlowChanged {
                project: project.key.clone(),
                change: FlowEvent::ProposalClosed { proposal },
            });
        }
        events.publish(Event::FlowSetupChanged {
            project: project.key.clone(),
        });
        events.publish(Event::ProjectsChanged);
        Ok(settled)
    }

    /// Types [`CHOSE_ANOTHER_FLOW`] into the unchosen flow agent's `session`, as the user's click is typed (d0048):
    /// never over unsent text. Whether it was typed.
    async fn tell_unchosen(&self, agent: &ProjectAgent, session: &str) -> bool {
        match self
            .inner
            .deliveries
            .deliver_from_click(session, CHOSE_ANOTHER_FLOW)
            .await
        {
            Ok(crate::session::deliveries::Delivery::Sent { .. }) => true,
            Ok(_) => {
                tracing::info!(agent = agent.id, "unsent text: the flow agent ends untold");
                false
            }
            Err(e) => {
                tracing::warn!(agent = agent.id, error = %e, "can't tell the flow agent");
                false
            }
        }
    }

    /// Ends the told flow agent `id` once its turn on the answer ends (its next idle signal), its session ends, or
    /// [`ANSWER_GRACE`] passes, whichever comes first.
    async fn end_after_answer(&self, id: &str, session: &str) {
        let grace = Duration::from_millis(self.inner.answer_grace_ms.load(Ordering::Relaxed));
        let waited = tokio::time::timeout(grace, async {
            loop {
                tokio::time::sleep(TURN_POLL).await;
                let running = self
                    .inner
                    .sessions
                    .info(session)
                    .is_ok_and(|i| i.is_running());
                if !running || self.inner.idle.is_idle(session) {
                    return;
                }
            }
        })
        .await;
        if waited.is_err() {
            tracing::info!(
                agent = id,
                "the flow agent didn't end its turn: it ends now"
            );
        }
        match self.close(id, true).await {
            Ok(()) => {
                if let Ok(agent) = self.get(id).await {
                    self.inner.events.publish(Event::FlowSetupChanged {
                        project: agent.project,
                    });
                }
            }
            Err(e) => tracing::warn!(agent = id, error = %e, "can't end the flow agent"),
        }
    }

    /// Shortens [`ANSWER_GRACE`] (tests).
    pub fn set_answer_grace(&self, grace: Duration) {
        self.inner.answer_grace_ms.store(
            u64::try_from(grace.as_millis()).unwrap_or(u64::MAX),
            Ordering::Relaxed,
        );
    }

    /// Sends [`Event::FlowSetupChanged`] for a project in Flow setup whenever what [`Self::flow_setup`] answers may
    /// have changed: one of its Project agents or its proposals, a tool's state, a limit. Runs until `stop`.
    pub async fn relay_flow_setup(self, mut stop: watch::Receiver<bool>) {
        let mut listener = self.inner.events.listen();
        loop {
            let event = tokio::select! {
                biased;
                _ = stop.wait_for(|s| *s) => return,
                event = listener.receiver.recv() => event,
            };
            let projects = match event {
                Ok(
                    Event::ProjectAgentChanged { project, .. } | Event::FlowChanged { project, .. },
                ) => {
                    vec![project]
                }
                Ok(Event::AgentToolsChanged | Event::UsageChanged) => self.in_flow_setup().await,
                Ok(_) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                    self.in_flow_setup().await
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
            };
            for project in projects {
                let setup = self.inner.board.project(&project).await;
                if setup.is_ok_and(|p| p.flow_setup_since.is_some()) {
                    self.inner
                        .events
                        .publish(Event::FlowSetupChanged { project });
                }
            }
        }
    }

    /// The keys of the projects in Flow setup.
    async fn in_flow_setup(&self) -> Vec<String> {
        match self.inner.board.projects().await {
            Ok(projects) => projects
                .into_iter()
                .filter(|p| p.flow_setup_since.is_some())
                .map(|p| p.key)
                .collect(),
            Err(e) => {
                tracing::warn!(error = %e, "can't list the projects in flow setup");
                Vec::new()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_message_names_the_tool_and_the_project_and_what_to_look_at() {
        let message = first_flow_message("pocket-notes", Tool::OpenCode);
        assert_eq!(
            message,
            format!(
                "/flow This is pocket-notes's first flow. Look at how the user works with {}: your own history on \
                 this Mac (find it yourself; read only what the user typed, never login or credential data) and \
                 this repo (`git log`, branch names, CI files, PR templates, `CONTRIBUTING.md`, and its AI files: \
                 `AGENTS.md`, `CLAUDE.md`, `.claude/`, `.cursor/rules` and the like). Nothing else: not GitHub or \
                 GitLab, not skills or plugins in the home folder. Then propose a whole flow.",
                Tool::OpenCode.display_name()
            )
        );
        let (skill, asks) = first_skill(&message);
        assert_eq!(skill, "flow");
        assert!(
            asks.unwrap()
                .starts_with("This is pocket-notes's first flow.")
        );
    }

    #[test]
    fn a_flow_agent_is_known_by_its_first_message() {
        for &tool in Tool::ALL {
            assert!(is_flow_agent_request(&first_flow_message("shop-web", tool)));
        }
        assert!(!is_flow_agent_request("/flow add a Security check"));
        assert!(!is_flow_agent_request("/flow"));
        assert!(!is_flow_agent_request(
            "This is shop-web's first flow. Look at how the user works with OpenCode"
        ));
    }

    #[test]
    fn a_flow_agent_runs_on_the_asked_tool_with_no_model_and_no_effort() {
        // The role's agent on Claude Code's opus with an effort: neither goes to the asked tool.
        let agent = Agent {
            id: "pa".into(),
            name: "Noyan".into(),
            instructions: String::new(),
            skills: Vec::new(),
            imported: Vec::new(),
            model: crate::agents::AgentModel::Tool {
                tool: Tool::Claude,
                model: Some("opus".into()),
            },
            effort: Some("high".into()),
            role: Some(Role::ProjectAgent),
            came_from: crate::agents::CameFrom::DefaultPreset,
            mark: None,
            created_at: 0,
            updated_at: 0,
        };
        let runs_on = flow_runs_on(&agent, Tool::OpenCode);
        assert_eq!(runs_on.tool, Some(Tool::OpenCode));
        assert_eq!(runs_on.model, None);
        assert_eq!(runs_on.effort, None);
        assert_eq!(runs_on.fallback, None);
        assert_eq!(runs_on.agent, Some(agent.session_agent()));
    }

    #[test]
    fn a_flow_agent_is_named_after_its_tool() {
        assert_eq!(
            flow_agent_name(Tool::OpenCode),
            format!("Flow setup \u{b7} {}", Tool::OpenCode.display_name())
        );
    }
}
