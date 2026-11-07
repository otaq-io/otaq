//! Which agent runs (AGM-05; d0108, d0088): a session in a stage runs the ticket's choice for that stage (AGM-08,
//! [`crate::board::stage_choices`]), else **the stage's agent** (the agent the flow names); a Noyan conversation runs
//! **the agent Noyan uses** (an app setting) unless it was started on another. A flow names agents by name, looked up among the agents ignoring case; a rename renames the agent in
//! every stored flow ([`crate::flow::store`]), so it changes nothing. An unknown name is a check error naming the
//! stage ([`crate::flow::check_agents`]).
//!
//! The model is resolved by the check before every launch ([`super::gate`]): `default` becomes the default model,
//! and the agent, tool, exact model and effort are saved on the session and its stage run.

use super::{Agent, SessionRole};
use crate::board::tickets::Ticket;
use crate::board::{Board, BoardError, Refusal};
use crate::domain::Place;
use crate::flow::{Flow, Stage};
use crate::store::{Store, StoreError};

/// The agent a flow's `name` stands for: the agent with that name, ignoring case.
pub async fn named(store: &Store, name: &str) -> Result<Option<Agent>, StoreError> {
    store.agent_named(name).await
}

/// The refusal for a stage whose agent isn't one of the agents: "Dev names the agent Rust developer, which isn't one
/// of your agents".
fn unknown(stage: &Stage, name: &str) -> BoardError {
    BoardError::refused(
        Refusal::UnknownAgent,
        crate::flow::check::unknown_agent_message(stage, name),
    )
}

/// The stage's agent (AGM-05): the agent `stage` names (an Agent stage's, an agent check's). Refused
/// for a stage that runs no agent, or names one that isn't there.
pub async fn stage_agent(board: &Board, stage: &Stage) -> Result<Agent, BoardError> {
    let Some(name) = stage.agent.as_ref() else {
        return Err(BoardError::refused(
            Refusal::UnknownAgent,
            format!("{} runs no agent", stage.name),
        ));
    };
    named(board.store(), name)
        .await?
        .ok_or_else(|| unknown(stage, name))
}

/// Whether `stage` itself names `agent` (its agent), by name ignoring case
/// as [`named`] looks it up.
pub fn stage_names(stage: &Stage, agent: &Agent) -> bool {
    stage
        .named_agents()
        .any(|name| name.trim().to_lowercase() == agent.name.to_lowercase())
}

/// The running agent of `ticket` in the stage `stage_id` (AGM-05): the ticket's choice for it (AGM-08), else the
/// stage's agent.
pub async fn running_agent(
    board: &Board,
    ticket: &Ticket,
    stage_id: &str,
) -> Result<Agent, BoardError> {
    let flow = board.effective_flow(ticket.key.project.as_str()).await?;
    let stage = flow.stage(stage_id).ok_or_else(|| {
        BoardError::NotFound(format!(
            "{} has no stage {stage_id}",
            ticket.key.project.as_str()
        ))
    })?;
    match board.chosen_agent(ticket, stage).await? {
        Some(agent) => Ok(agent),
        None => stage_agent(board, stage).await,
    }
}

/// The agent Noyan uses (an app setting). Refused while none is set: the conversation waits for the user to pick one,
/// never starts on some other agent.
pub async fn noyan_agent(store: &Store) -> Result<Agent, BoardError> {
    store.noyan_agent().await?.ok_or_else(|| {
        BoardError::refused(
            Refusal::UnknownAgent,
            "Noyan has no agent: pick the one it uses in Settings \u{2192} Agents",
        )
    })
}

/// The Agent stage a ticket's agent session starts or continues in: the one it waits before, the Agent stage it is in,
/// else (from a check, coming back) the nearest earlier Agent stage, else the first one it runs. `skips`: the stages
/// it skips by its choices (AGM-08), never started.
pub fn agent_stage_of<'f>(flow: &'f Flow, ticket: &Ticket, skips: &[String]) -> Option<&'f Stage> {
    let at = match &ticket.place {
        Place::Waiting(Some(id)) | Place::InStage(id) => flow.stage(id),
        _ => None,
    };
    let first = || crate::flow::engine::first_stage(flow, skips).1;
    match at {
        Some(stage) if stage.is_agent_stage() => Some(stage),
        Some(stage) => crate::flow::engine::back_to(flow, &stage.id, skips).or_else(first),
        None => first(),
    }
}

/// The agent check a ticket's check session runs in: the one it is in, else the first agent check after its Agent
/// stage it doesn't skip, else the flow's first.
pub fn check_stage_of<'f>(flow: &'f Flow, ticket: &Ticket, skips: &[String]) -> Option<&'f Stage> {
    let at = ticket.place.stage().and_then(|id| flow.stage(id));
    let runs = |s: &&Stage| s.is_agent_check() && !skips.contains(&s.id);
    match at {
        Some(stage) if stage.is_agent_check() => Some(stage),
        Some(stage) => {
            let index = flow.position(&stage.id)?;
            flow.stages[index..]
                .iter()
                .find(runs)
                .or_else(|| flow.stages.iter().find(runs))
        }
        None => flow.stages.iter().find(runs),
    }
}

/// The agent a ticket's new session of `role` runs (AGM-05): an Agent stage's ([`agent_stage_of`]), an agent check's
/// ([`check_stage_of`]), or the agent Noyan uses for a Noyan conversation.
pub async fn session_agent(
    board: &Board,
    ticket: &Ticket,
    role: SessionRole,
) -> Result<Agent, BoardError> {
    if role == SessionRole::ProjectAgent {
        return noyan_agent(board.store()).await;
    }
    let flow = board.effective_flow(ticket.key.project.as_str()).await?;
    let skips = board.ticket_skips(ticket, &flow).await?;
    let stage = match role {
        SessionRole::Validator => check_stage_of(&flow, ticket, &skips),
        _ => agent_stage_of(&flow, ticket, &skips),
    }
    .ok_or_else(|| {
        BoardError::NotFound(format!(
            "{}'s flow has no stage for its {}",
            ticket.key,
            role.label()
        ))
    })?;
    running_agent(board, ticket, &stage.id.clone()).await
}

/// The agent that opens a project's PRs (the PR path, AGM-05): the one its flow's delivery names, else the first
/// Agent stage's.
pub async fn delivery_agent(board: &Board, project: &str) -> Result<Agent, BoardError> {
    let flow = board.effective_flow(project).await?;
    if let Some(name) = flow.delivery.agent.as_deref() {
        return named(board.store(), name).await?.ok_or_else(|| {
            BoardError::refused(
                Refusal::UnknownAgent,
                format!("The delivery names the agent {name}, which isn't one of your agents"),
            )
        });
    }
    let stage = flow
        .first_agent_stage()
        .ok_or_else(|| BoardError::NotFound(format!("{project}'s flow has no agent to deliver")))?;
    stage_agent(board, stage).await
}

/// Whether the flow's delivery names `agent` as the one that opens its PRs, by name ignoring case as [`named`] looks
/// it up.
pub fn delivery_names(flow: &Flow, agent: &Agent) -> bool {
    flow.delivery
        .agent
        .as_deref()
        .is_some_and(|name| name.trim().to_lowercase() == agent.name.to_lowercase())
}
