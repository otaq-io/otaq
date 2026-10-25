//! What the screens show about a ticket's agents (AGM-05, AGM-09; d0108, d0088): the agent each stage of its flow
//! runs, the sessions that ran (with the agent, the exact model and the effort of each) and its holds, answered by
//! `ticket.agents`; and what each board card says about its running agent, answered by `board.agents`. The wording
//! is the view models' (`stage_choices.rs` draws the path and the choices); this is only the facts.

use serde::{Deserialize, Serialize};

use super::store::SessionAgentRun;
use super::{Agent, AgentHold, FallbackFrom, HoldReason, ModelRef, Role, SessionRole, resolve};
use crate::agent_tools::Tool;
use crate::board::stage_choices::StageOptions;
use crate::board::stage_runs::StageRun;
use crate::board::tickets::Ticket;
use crate::board::{Board, BoardError};
use crate::domain::{Kind, Place, Status, TicketKey};
use crate::flow::{Flow, StageKind};
use crate::usage::AgentTokens;

/// A stage of the ticket's flow that runs an agent, and the agent it runs (the ticket's choice, else the stage's
/// agent; AGM-05, AGM-08).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StageAgent {
    pub stage_id: String,
    /// The stage's name ("Dev").
    pub stage: String,
    /// Which session runs it: an Agent stage's (and a Delivery's) the Developer, an agent check's the Validator.
    pub role: SessionRole,
    /// The Delivery stage (its agent opens the PR).
    pub delivery: bool,
    /// `None` when the stage names an agent that isn't there.
    pub agent: Option<Agent>,
}

/// The answer to `ticket.agents`: a ticket's or an epic's agents, or (no key) a new one's in a project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TicketAgents {
    /// `None` for a ticket not created yet (the ⌘N form).
    pub key: Option<TicketKey>,
    pub kind: Kind,
    pub status: Option<Status>,
    /// Every agent.
    pub agents: Vec<Agent>,
    /// A ticket's stages that run an agent, in flow order; empty for an epic.
    pub stages: Vec<StageAgent>,
    /// An epic's lead: the Epic lead role's agent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lead: Option<Agent>,
    /// Oldest first.
    pub sessions: Vec<SessionAgentRun>,
    /// Tokens by session role and agent ([`crate::usage::Usage::agents_of`]); filled by the service.
    #[serde(default)]
    pub tokens: Vec<AgentTokens>,
    /// The sessions held before their start (SCH-04 `[M23]`, §7.2): a tool at its limit or not available, a model no
    /// longer listed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub holds: Vec<AgentHold>,
    /// Where the ticket is now (`None` for a new one).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub place: Option<Place>,
    /// A ticket's path (AGM-09): every stage of its flow, in order, with the agent each runs for it; empty for an
    /// epic and a new ticket.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub path: Vec<PathStage>,
    /// The ticket's stage runs, oldest first (STG-05): what ran, on which exact model, and how each ended.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub runs: Vec<StageRun>,
    /// Its stages with options and its choice for each (AGM-08, `ticket.choices`); empty on a flow without options.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub choices: Vec<StageOptions>,
}

/// What kind of stage a path stage is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PathKind {
    Agent,
    AgentCheck,
    HumanCheck,
    Delivery,
}

/// One stage of a ticket's path (AGM-09): the stage, how it shows on the board, and the agent it runs for this
/// ticket (its choice, else the stage's agent; none for a human check or a Delivery without one).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathStage {
    pub stage_id: String,
    /// "Dev".
    pub stage: String,
    pub kind: PathKind,
    /// The stage it shows inside (`column: with-previous`): "Dev" for Validate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shown_in: Option<String>,
    /// An agent check's rounds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rounds: Option<u32>,
    /// A Delivery: automatic (else click), and what it delivers (`local`, `pr`, `either`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delivery: Option<PathDelivery>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<Agent>,
    /// The ticket skips it by its choice (AGM-08).
    #[serde(default)]
    pub skipped: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathDelivery {
    pub automatic: bool,
    pub delivers: String,
}

/// The path of `ticket` on `flow` ([`PathStage`]), skipping `skips`.
async fn path_of(
    board: &Board,
    ticket: &Ticket,
    flow: &Flow,
    skips: &[String],
) -> Result<Vec<PathStage>, BoardError> {
    let mut path = Vec::with_capacity(flow.stages.len());
    for (index, stage) in flow.stages.iter().enumerate() {
        // T-0527: every check agent (one agent per stage here: an agent check's first).
        let agent = match stage.has_agent() {
            true => match board.chosen_agent(ticket, stage).await? {
                Some(chosen) => Some(chosen),
                None => resolve::stage_agent(board, stage).await.ok(),
            },
            false => None,
        };
        path.push(PathStage {
            stage_id: stage.id.clone(),
            stage: stage.name.clone(),
            kind: match stage.kind {
                StageKind::Agent => PathKind::Agent,
                StageKind::Check { .. } if stage.is_agent_check() => PathKind::AgentCheck,
                StageKind::Check { .. } => PathKind::HumanCheck,
                StageKind::Delivery => PathKind::Delivery,
            },
            // A stage whose column is another's (`column: with-previous`) shows inside it.
            shown_in: flow
                .column_owner(index)
                .filter(|&owner| owner != index)
                .and_then(|owner| flow.stages.get(owner))
                .map(|owner| owner.name.clone()),
            rounds: stage.rounds,
            delivery: stage.delivery.map(|d| PathDelivery {
                automatic: d.mode == crate::flow::DeliveryMode::Automatic,
                delivers: d.delivers.as_str().to_owned(),
            }),
            agent,
            skipped: skips.contains(&stage.id),
        });
    }
    Ok(path)
}

/// What a project's board cards say about their agents (SHL-07's footer; T-0345), answered by `board.agents`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoardAgents {
    pub agent_count: usize,
    /// Every ticket of the project (not its epics).
    pub cards: Vec<CardAgents>,
    /// The agent tools connected now (`ConnectedTools`, filled by the service): avatars and tool names show once
    /// there are two (SHL-08, d0086; T-0362).
    #[serde(default)]
    pub connected: Vec<Tool>,
}

/// One ticket's agents for its card: whoever has the turn, and its holds. Nothing before it starts (AGM-09).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CardAgents {
    pub key: TicketKey,
    /// The agent of the running Agent stage session (agent, answer, delivery) and of the running check.
    pub running_developer: Option<String>,
    pub running_validator: Option<String>,
    /// The tool and model each runs on (T-0362), and the fallback it runs on (AGM-03).
    #[serde(default)]
    pub running_developer_on: Option<RunsOnTool>,
    #[serde(default)]
    pub running_validator_on: Option<RunsOnTool>,
    /// The sessions held before they start, with the names the card says.
    #[serde(default)]
    pub holds: Vec<CardHold>,
    /// A ticket waiting before an Agent stage: where it starts, on which agent and model ("Grill · Griller ·
    /// sonnet", AGM-09).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub starts: Option<CardStart>,
    /// The agent whose model the tool refused (a Start failed or a check's No result on the model, AGT-05), for
    /// Open agent….
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refused_agent: Option<String>,
}

/// Where a waiting ticket starts: the stage, the agent it runs there, and that agent's model now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CardStart {
    pub stage: String,
    pub agent: String,
    /// `None` while no default model is set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelRef>,
}

/// A tool and the model a session runs (the one the tool reported, else the one passed), and the fallback it runs on
/// ("on Codex · Claude Code limit").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunsOnTool {
    pub tool: Tool,
    pub model: Option<String>,
    /// The model as passed (`None`: the tool's default): what the footer names ("Rust developer · sonnet").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub passed: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback: Option<FallbackFrom>,
}

/// A hold ([`super::AgentHold`]) as a card says it: the agent it would have started and its tool's fallback.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CardHold {
    pub role: SessionRole,
    pub reason: HoldReason,
    pub tool: Tool,
    pub model: Option<String>,
    /// The agent it would have started ("Developer").
    pub agent: Option<String>,
    /// Its agent's id, for Open agent….
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    /// The tool's fallback, also when it can't start either (`fallback_held`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback: Option<ModelRef>,
    /// Use fallback is offered: the fallback can take it now.
    pub can_use_fallback: bool,
    /// The fallback is blocked as well: "Claude Code and its fallback Codex are both at their limits".
    pub fallback_held: bool,
    /// **Skip this check** is offered: an agent check held for its tool's plan limit with no usable fallback (none
    /// set, or the fallback at its limit too; FLW-13, Q277). Never on a held Agent stage.
    #[serde(default)]
    pub can_skip_check: bool,
    /// A limit's hold: when the tool's limit resets, when known ("Claude Code limit · resets 3:45 pm", Q154).
    #[serde(default)]
    pub resets_at: Option<i64>,
}

impl CardHold {
    pub fn of(hold: &AgentHold, agents: &[Agent], fallbacks: &super::Fallbacks) -> Self {
        let agent = hold
            .agent_id
            .as_ref()
            .and_then(|id| agents.iter().find(|a| &a.id == id));
        Self {
            role: hold.role,
            reason: hold.reason,
            tool: hold.tool,
            model: hold.model.clone(),
            agent: agent.map(|a| a.name.clone()),
            agent_id: hold.agent_id.clone(),
            can_use_fallback: hold.fallback_tool.is_some() && !hold.fallback_held,
            fallback: fallbacks.get(&hold.tool).cloned(),
            fallback_held: hold.fallback_held,
            can_skip_check: crate::validation::skip_at_limit(hold),
            resets_at: None,
        }
    }
}

/// The stages of `project`'s flow that run an agent, each with its agent.
async fn stage_agents(board: &Board, project: &str) -> Result<Vec<StageAgent>, BoardError> {
    let flow = board.effective_flow(project).await?;
    let mut stages = Vec::new();
    // T-0527: every check agent (one agent per stage here: an agent check's first).
    for stage in flow.stages.iter().filter(|s| s.has_agent()) {
        stages.push(StageAgent {
            stage_id: stage.id.clone(),
            stage: stage.name.clone(),
            role: match stage.is_agent_check() {
                true => SessionRole::Validator,
                false => SessionRole::Developer,
            },
            delivery: stage.is_delivery(),
            agent: resolve::stage_agent(board, stage).await.ok(),
        });
    }
    Ok(stages)
}

/// What a ticket of `kind` not created yet in `project` runs: its flow's stages, nothing ran.
pub async fn new_ticket_agents(
    board: &Board,
    project: &str,
    kind: Kind,
) -> Result<TicketAgents, BoardError> {
    let store = board.store();
    let (stages, lead) = match kind {
        Kind::Ticket => (stage_agents(board, project).await?, None),
        Kind::Epic => (Vec::new(), store.role_agent(Role::EpicLead).await?),
    };
    Ok(TicketAgents {
        key: None,
        kind,
        status: None,
        agents: store.agents().await?,
        stages,
        lead,
        sessions: Vec::new(),
        tokens: Vec::new(),
        holds: Vec::new(),
        place: None,
        path: Vec::new(),
        runs: Vec::new(),
        choices: Vec::new(),
    })
}

/// Everything the sheet shows of `key`'s agents ([`TicketAgents`]).
pub async fn ticket_agents(board: &Board, key: &str) -> Result<TicketAgents, BoardError> {
    let ticket = board.ticket(key).await?;
    let mut agents = new_ticket_agents(board, ticket.key.project.as_str(), ticket.kind).await?;
    // The agent the ticket chose for a stage instead of its usual one (AGM-08), its path, runs and choices (AGM-09).
    if ticket.kind == Kind::Ticket {
        let flow = board.effective_flow(ticket.key.project.as_str()).await?;
        for stage_agent in &mut agents.stages {
            if let Some(stage) = flow.stage(&stage_agent.stage_id)
                && let Some(chosen) = board.chosen_agent(&ticket, stage).await?
            {
                stage_agent.agent = Some(chosen);
            }
        }
        let skips = board.ticket_skips(&ticket, &flow).await?;
        agents.path = path_of(board, &ticket, &flow, &skips).await?;
        agents.runs = board.stage_runs(key).await?;
        agents.choices = board.ticket_choices(key).await?.stages;
    }
    agents.place = Some(ticket.place.clone());
    let store = board.store();
    let mut sessions = store.session_agents(&ticket.id).await?;
    for session in &mut sessions {
        session.moved_on = crate::session::moved_on(store.pool(), &session.id).await?;
    }
    agents.holds = store.ticket_holds(&ticket.id).await?;
    agents.sessions = sessions;
    agents.key = Some(ticket.key);
    agents.status = Some(ticket.status);
    Ok(agents)
}

/// Every card's agent facts in `project` ([`BoardAgents`]): read in a few queries, whatever the board's size.
pub async fn board_agents(board: &Board, project: &str) -> Result<BoardAgents, BoardError> {
    let project = board.project(project).await?;
    let store = board.store();
    let agents = store.agents().await?;
    let fallbacks = store.fallbacks().await?;
    let running = store.running_agent_names(&project.id).await?;
    let holds = store.project_ticket_holds(&project.id).await?;
    // The tools at their limits and when each resets (a limit's hold says it).
    let limits = crate::usage::plan::limits(store).await?;
    let flow = board.effective_flow(project.key.as_str()).await?;
    let default_model = store.default_model().await?;
    let resets_of = |hold: &AgentHold| {
        limits
            .iter()
            .filter(|l| hold.reason == HoldReason::ToolLimit && l.tool == hold.tool)
            .find(|l| l.reset_seen_at.is_none())
            .and_then(|l| l.resets_at)
    };
    let mut cards = Vec::new();
    for ticket in board.tickets(&project.key).await? {
        if ticket.kind != Kind::Ticket {
            continue;
        }
        let running_of = |role| {
            running
                .iter()
                .rev()
                .find(|r| r.ticket_id == ticket.id && r.role == role)
        };
        let name_of = |role| running_of(role).map(|r| r.name.clone());
        let on_of = |role| {
            running_of(role).and_then(|r| {
                Some(RunsOnTool {
                    tool: r.tool?,
                    model: r.model.clone(),
                    passed: r.passed.clone(),
                    fallback: r.fallback.clone(),
                })
            })
        };
        let starts = match ticket.place.is_waiting() {
            true => card_start(board, &ticket, &flow, default_model.as_ref()).await?,
            false => None,
        };
        let refused_agent = match crate::board::needs_you::change_agent(&ticket) {
            Some(role) => store
                .session_agents(&ticket.id)
                .await?
                .into_iter()
                .rev()
                .find(|s| s.role == role)
                .and_then(|s| s.agent_id),
            None => None,
        };
        cards.push(CardAgents {
            running_developer: name_of(SessionRole::Developer),
            running_validator: name_of(SessionRole::Validator),
            running_developer_on: on_of(SessionRole::Developer),
            running_validator_on: on_of(SessionRole::Validator),
            holds: holds
                .iter()
                .filter(|h| h.ticket_id.as_deref() == Some(ticket.id.as_str()))
                .map(|h| CardHold {
                    resets_at: resets_of(h),
                    ..CardHold::of(h, &agents, &fallbacks)
                })
                .collect(),
            starts,
            refused_agent,
            key: ticket.key,
        });
    }
    Ok(BoardAgents {
        agent_count: agents.len(),
        cards,
        connected: Vec::new(),
    })
}

/// Where a waiting `ticket` starts ([`CardStart`]): the Agent stage it waits before (its first on the flow, past the
/// stages it skips), and the agent it runs there.
async fn card_start(
    board: &Board,
    ticket: &Ticket,
    flow: &Flow,
    default_model: Option<&ModelRef>,
) -> Result<Option<CardStart>, BoardError> {
    let skips = board.ticket_skips(ticket, flow).await?;
    let Some(stage) = resolve::agent_stage_of(flow, ticket, &skips) else {
        return Ok(None);
    };
    let Ok(agent) = resolve::running_agent(board, ticket, &stage.id).await else {
        return Ok(None);
    };
    Ok(Some(CardStart {
        stage: stage.name.clone(),
        model: agent.model.on(default_model),
        agent: agent.name,
    }))
}
