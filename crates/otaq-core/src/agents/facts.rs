//! What the screens show about a ticket's sessions (AGM-05, AGM-09; d0108, d0088, UX-65): who does each stage of its
//! flow (the stage's model, effort and instructions), the sessions that ran (with the tool, the exact model and the
//! effort of each) and its holds, answered by `ticket.agents`; and what each board card says about its running session,
//! answered by `board.agents`. The wording is the view models'; this is only the facts.

use serde::{Deserialize, Serialize};

use super::store::SessionAgentRun;
use super::{AgentHold, HoldReason, ModelRef, SessionRole, Who, resolve};
use crate::agent_tools::Tool;
use crate::board::stage_choices::StageOptions;
use crate::board::stage_runs::StageRun;
use crate::board::tickets::Ticket;
use crate::board::{Board, BoardError};
use crate::domain::{Place, Status, TicketKey};
use crate::flow::{Flow, Stage, StageKind};
use crate::usage::AgentTokens;

/// A stage of the ticket's flow that runs an agent, and who does it (the stage's model, effort and instructions;
/// AGM-05, UX-65).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StageAgent {
    pub stage_id: String,
    /// The stage's name ("Dev").
    pub stage: String,
    /// Which session runs it: an Agent stage's the Developer, an agent check's the Validator.
    pub role: SessionRole,
    /// `None` when the stage names no model yet.
    pub who: Option<Who>,
}

/// The answer to `ticket.agents`: a ticket's sessions, or (no key) a new one's stages in a project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TicketAgents {
    /// `None` for a ticket not created yet (the ⌘N form).
    pub key: Option<TicketKey>,
    pub status: Option<Status>,
    /// A ticket's stages that run an agent, in flow order.
    pub stages: Vec<StageAgent>,
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
    /// A ticket's path (AGM-09): every stage of its flow, in order, with who does each; empty for a new ticket.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub path: Vec<PathStage>,
    /// The ticket's stage runs, oldest first (STG-05): what ran, on which exact model, and how each ended.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub runs: Vec<StageRun>,
    /// Its stages with options and its choice for each (AGM-08, `ticket.choices`); empty on a flow without options.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub choices: Vec<StageOptions>,
}

/// What kind of step a path stage is: a stage of the flow, or its delivery after the last one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PathKind {
    Agent,
    AgentCheck,
    HumanCheck,
    /// The flow's delivery, the path's last step: not a stage.
    Delivery,
}

/// The path step that stands for the flow's delivery ([`PathKind::Delivery`]): its id and name.
pub const DELIVERY_STEP_ID: &str = "delivery";
pub const DELIVERY_STEP: &str = "Delivery";

/// One step of a ticket's path (AGM-09): a stage, and who does it (none for a human check), or the flow's delivery,
/// last, with who writes and opens its PR.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathStage {
    pub stage_id: String,
    /// "Dev".
    pub stage: String,
    pub kind: PathKind,
    /// An agent check's rounds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rounds: Option<u32>,
    /// The delivery: automatic (else click), and what it delivers (`local`, `pr`, `either`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delivery: Option<PathDelivery>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub who: Option<Who>,
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
fn path_of(ticket: &Ticket, flow: &Flow, skips: &[String]) -> Vec<PathStage> {
    let project = ticket.key.project.as_str();
    let mut path = Vec::with_capacity(flow.stages.len() + 1);
    for stage in &flow.stages {
        path.push(PathStage {
            stage_id: stage.id.clone(),
            stage: stage.name.clone(),
            kind: match stage.kind {
                StageKind::Agent => PathKind::Agent,
                StageKind::Check { .. } if stage.is_agent_check() => PathKind::AgentCheck,
                StageKind::Check { .. } => PathKind::HumanCheck,
            },
            rounds: stage.rounds,
            delivery: None,
            who: resolve::stage_who(project, stage).ok(),
            skipped: skips.contains(&stage.id),
        });
    }
    let delivery = &flow.delivery;
    let who = match delivery.can_open_pr() {
        true => resolve::delivery_who_of(project, flow).ok(),
        false => None,
    };
    path.push(PathStage {
        stage_id: DELIVERY_STEP_ID.to_owned(),
        stage: DELIVERY_STEP.to_owned(),
        kind: PathKind::Delivery,
        rounds: None,
        delivery: Some(PathDelivery {
            automatic: delivery.mode == crate::flow::DeliveryMode::Automatic,
            delivers: delivery.delivers.as_str().to_owned(),
        }),
        who,
        skipped: false,
    });
    path
}

/// What a project's board cards say about their sessions (SHL-07's footer; T-0345), answered by `board.agents`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoardAgents {
    /// Every ticket of the project.
    pub cards: Vec<CardAgents>,
    /// The agent tools connected now (`ConnectedTools`, filled by the service): avatars and tool names show once
    /// there are two (SHL-08, d0086; T-0362).
    #[serde(default)]
    pub connected: Vec<Tool>,
}

/// One ticket's sessions for its card: whoever has the turn, and its holds. Nothing before it starts (AGM-09).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CardAgents {
    pub key: TicketKey,
    /// The name of the running Agent stage session (agent, answer, delivery) and of the running check: its tool's
    /// ("Claude Code"; an older session's agent's).
    pub running_developer: Option<String>,
    pub running_validator: Option<String>,
    /// The tool and model each runs on (T-0362).
    #[serde(default)]
    pub running_developer_on: Option<RunsOnTool>,
    #[serde(default)]
    pub running_validator_on: Option<RunsOnTool>,
    /// The sessions held before they start, with what the card says.
    #[serde(default)]
    pub holds: Vec<CardHold>,
    /// A ticket waiting before an Agent stage: where it starts, on which tool and model ("Product spec · Claude Code ·
    /// sonnet", AGM-09).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub starts: Option<CardStart>,
    /// The stage (its id) whose model the tool refused (a Start failed or a check's No result on the model, AGT-05),
    /// for Edit flow….
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refused_stage: Option<String>,
    /// The tool the stage the ticket is in or waits before runs on ("Claude Code"). `None` in Backlog, Done and a stage
    /// that runs no agent. The search's `by:` reads it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage_agent: Option<String>,
    /// The ticket is in an agent check stage of its project's flow ([`at_agent_check`]): the check, not the developer,
    /// has it, also while it waits for its tool's limit or was interrupted (C3c). The card names the check's tool.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub at_check: bool,
    /// The stages the user chose to skip for this ticket, by name, in flow order: the card's "Skips Product spec".
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skips: Vec<String>,
}

/// Where a waiting ticket starts: the stage, the tool it runs there, and its model now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CardStart {
    pub stage: String,
    /// The tool's name ("Claude Code").
    pub agent: String,
    /// `None` while no default model is set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<ModelRef>,
}

/// A tool and the model a session runs (the one the tool reported, else the one passed).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunsOnTool {
    pub tool: Tool,
    pub model: Option<String>,
    /// The model as passed (`None`: the tool's default): what the footer names ("Claude Code · sonnet").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub passed: Option<String>,
}

/// A hold ([`super::AgentHold`]) as a card says it: the stage that waits for its tool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CardHold {
    pub role: SessionRole,
    pub reason: HoldReason,
    pub tool: Tool,
    pub model: Option<String>,
    /// The stage it would have started (its id), for Edit flow….
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage_id: Option<String>,
    /// **Skip this check** is offered: an agent check held for its tool's plan limit (FLW-13, Q277). Never on a
    /// held Agent stage.
    #[serde(default)]
    pub can_skip_check: bool,
    /// A limit's hold: when the tool's limit resets, when known ("Claude Code limit · resets 3:45 pm", Q154).
    #[serde(default)]
    pub resets_at: Option<i64>,
    /// A hold for its skill: the skill its tool doesn't have ("Codex doesn't have the skill `$team-validate`").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skill: Option<String>,
}

impl CardHold {
    /// `hold` as a card says it, held at the stage `stage_id`.
    pub fn of(hold: &AgentHold, stage_id: Option<String>) -> Self {
        Self {
            role: hold.role,
            reason: hold.reason,
            tool: hold.tool,
            model: hold.model.clone(),
            stage_id,
            can_skip_check: crate::validation::skip_at_limit(hold),
            resets_at: None,
            skill: hold.skill.clone(),
        }
    }
}

/// The stage of `ticket` on `flow` a session of `role` runs in (skipping `skips`): an Agent stage's, an agent check's.
pub fn stage_of_role<'f>(
    flow: &'f Flow,
    ticket: &Ticket,
    skips: &[String],
    role: SessionRole,
) -> Option<&'f Stage> {
    match role {
        SessionRole::Developer => resolve::agent_stage_of(flow, ticket, skips),
        SessionRole::Validator => resolve::check_stage_of(flow, ticket, skips),
        SessionRole::ProjectAgent => None,
    }
}

/// The stages of `project`'s flow that run an agent, each with who does it.
async fn stage_agents(board: &Board, project: &str) -> Result<Vec<StageAgent>, BoardError> {
    let flow = board.effective_flow(project).await?;
    Ok(flow
        .stages
        .iter()
        .filter(|s| s.runs_agent())
        .map(|stage| StageAgent {
            stage_id: stage.id.clone(),
            stage: stage.name.clone(),
            role: match stage.is_agent_check() {
                true => SessionRole::Validator,
                false => SessionRole::Developer,
            },
            who: resolve::stage_who(project, stage).ok(),
        })
        .collect())
}

/// What a ticket not created yet in `project` runs: its flow's stages, nothing ran.
pub async fn new_ticket_agents(board: &Board, project: &str) -> Result<TicketAgents, BoardError> {
    Ok(TicketAgents {
        key: None,
        status: None,
        stages: stage_agents(board, project).await?,
        sessions: Vec::new(),
        tokens: Vec::new(),
        holds: Vec::new(),
        place: None,
        path: Vec::new(),
        runs: Vec::new(),
        choices: Vec::new(),
    })
}

/// Everything the sheet shows of `key`'s sessions ([`TicketAgents`]).
pub async fn ticket_agents(board: &Board, key: &str) -> Result<TicketAgents, BoardError> {
    let ticket = board.ticket(key).await?;
    let mut agents = new_ticket_agents(board, ticket.key.project.as_str()).await?;
    // Its path, runs and choices (AGM-09).
    let flow = board.effective_flow(ticket.key.project.as_str()).await?;
    let skips = board.ticket_skips(&ticket, &flow).await?;
    agents.path = path_of(&ticket, &flow, &skips);
    agents.runs = board.stage_runs(key).await?;
    agents.choices = board.ticket_choices(key).await?.stages;
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

/// Every card's facts in `project` ([`BoardAgents`]): read in a few queries, whatever the board's size.
pub async fn board_agents(board: &Board, project: &str) -> Result<BoardAgents, BoardError> {
    let project = board.project(project).await?;
    let store = board.store();
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
                })
            })
        };
        let skipped = match ticket.place {
            Place::Done => Vec::new(),
            _ => board.ticket_skips(&ticket, &flow).await?,
        };
        let starts = match ticket.place.is_waiting() {
            true => card_start(&ticket, &flow, &skipped, default_model.as_ref()),
            false => None,
        };
        let refused_stage = crate::board::needs_you::fix_model(&ticket)
            .and_then(|role| stage_of_role(&flow, &ticket, &skipped, role))
            .map(|s| s.id.clone());
        let stage_agent = match &starts {
            Some(start) => Some(start.agent.clone()),
            None => stage_tool_of(&ticket, &flow, default_model.as_ref()),
        };
        let skips = flow
            .stages
            .iter()
            .filter(|s| skipped.contains(&s.id))
            .map(|s| s.name.clone())
            .collect();
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
                    ..CardHold::of(
                        h,
                        stage_of_role(&flow, &ticket, &skipped, h.role).map(|s| s.id.clone()),
                    )
                })
                .collect(),
            starts,
            refused_stage,
            stage_agent,
            at_check: at_agent_check(&ticket, &flow),
            skips,
            key: ticket.key,
        });
    }
    Ok(BoardAgents {
        cards,
        connected: Vec::new(),
    })
}

/// Whether `ticket` is in an agent check stage of `flow` (C3c): the check has it, whatever its status says (Checking,
/// a check that ended without passing, or one waiting for its tool's limit or interrupted). A ticket sent back to an
/// Agent stage is in that stage, not here.
pub fn at_agent_check(ticket: &Ticket, flow: &Flow) -> bool {
    matches!(ticket.place, Place::InStage(_))
        && ticket
            .place
            .stage()
            .and_then(|id| flow.stage(id))
            .is_some_and(|s| s.is_agent_check())
}

/// The tool the stage `ticket` is in runs on ([`CardAgents::stage_agent`]), by its name.
fn stage_tool_of(ticket: &Ticket, flow: &Flow, default: Option<&ModelRef>) -> Option<String> {
    let stage = match &ticket.place {
        Place::InStage(id) | Place::Waiting(Some(id)) => flow.stage(id),
        Place::Waiting(None) => flow.first_agent_stage(),
        Place::Backlog | Place::Done => None,
    }?;
    let who = resolve::stage_who(ticket.key.project.as_str(), stage).ok()?;
    let on = who.model.on(default)?;
    Some(on.tool.display_name().to_owned())
}

/// Where a waiting `ticket` starts ([`CardStart`]): the Agent stage it waits before (its first on the flow, past the
/// stages it skips), and the tool and model it runs there.
fn card_start(
    ticket: &Ticket,
    flow: &Flow,
    skips: &[String],
    default_model: Option<&ModelRef>,
) -> Option<CardStart> {
    let stage = resolve::agent_stage_of(flow, ticket, skips)?;
    let who = resolve::stage_who(ticket.key.project.as_str(), stage).ok()?;
    let model = who.model.on(default_model);
    Some(CardStart {
        stage: stage.name.clone(),
        agent: model
            .as_ref()
            .map_or_else(|| stage.name.clone(), |m| m.tool.display_name().to_owned()),
        model,
    })
}
