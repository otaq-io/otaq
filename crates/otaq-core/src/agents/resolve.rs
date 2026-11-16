//! Which model runs (AGM-05; UX-65, UX-69, UX-70; d0088): a session in a stage runs **the stage's** model, effort and
//! instructions, as the project's flow says them (a ticket can't pick its own, UX-72); the PR-writing session runs
//! **the delivery's** (or, with none, the first Agent stage's); a Noyan conversation runs **"Noyan starts on"** (an
//! app setting) unless it picked another before its first message.
//!
//! The model is resolved by the check before every launch ([`super::gate`]): `default` becomes the default model,
//! and the tool, exact model and effort are saved on the session and its stage run.

use super::{About, SessionRole, Who};
use crate::board::tickets::Ticket;
use crate::board::{Board, BoardError, Refusal};
use crate::domain::Place;
use crate::flow::{Flow, Stage};
use crate::store::Store;

/// The refusal for a stage that names no model: "Dev has no model: pick one in Edit flow".
fn no_model(what: &str) -> BoardError {
    BoardError::refused(
        Refusal::UnknownAgent,
        format!("{what} has no model: pick one in Edit flow"),
    )
}

/// Who does `stage` of `project`'s flow (by key; AGM-05): its model, effort and instructions. Refused for a stage
/// that runs no agent, or names no model.
pub fn stage_who(project: &str, stage: &Stage) -> Result<Who, BoardError> {
    if !stage.runs_agent() {
        return Err(BoardError::refused(
            Refusal::UnknownAgent,
            format!("{} runs no agent", stage.name),
        ));
    }
    Who::of(
        &stage.who,
        About::Stage {
            project: project.to_owned(),
            stage: stage.id.clone(),
        },
    )
    .ok_or_else(|| no_model(&stage.name))
}

/// Who does the stage `stage_id` for `ticket` (AGM-05): the stage's own, as its project's flow says now.
pub async fn running_who(
    board: &Board,
    ticket: &Ticket,
    stage_id: &str,
) -> Result<Who, BoardError> {
    let project = ticket.key.project.as_str();
    let flow = board.effective_flow(project).await?;
    let stage = flow
        .stage(stage_id)
        .ok_or_else(|| BoardError::NotFound(format!("{project} has no stage {stage_id}")))?;
    stage_who(project, stage)
}

/// Who a new Noyan conversation runs as: "Noyan starts on" (an app setting; `default` while none was set).
pub async fn noyan_who(store: &Store) -> Result<Who, BoardError> {
    Ok(store.noyan_starts_on().await?.who())
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

/// Who a ticket's new session of `role` runs as (AGM-05): an Agent stage's ([`agent_stage_of`]), an agent check's
/// ([`check_stage_of`]), or "Noyan starts on" for a Noyan conversation.
pub async fn session_who(
    board: &Board,
    ticket: &Ticket,
    role: SessionRole,
) -> Result<Who, BoardError> {
    if role == SessionRole::ProjectAgent {
        return noyan_who(board.store()).await;
    }
    let project = ticket.key.project.as_str();
    let flow = board.effective_flow(project).await?;
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
    stage_who(project, stage)
}

/// Who writes and opens a project's PRs (the PR path, AGM-05, UX-69): the flow's delivery's model, effort and
/// instructions, else the first Agent stage's.
pub async fn delivery_who(board: &Board, project: &str) -> Result<Who, BoardError> {
    let flow = board.effective_flow(project).await?;
    delivery_who_of(project, &flow)
}

/// [`delivery_who`] in `flow`, the flow of `project` (its key).
pub fn delivery_who_of(project: &str, flow: &Flow) -> Result<Who, BoardError> {
    if let Some(who) = Who::of(
        &flow.delivery.who,
        About::Delivery {
            project: project.to_owned(),
        },
    ) {
        return Ok(who);
    }
    let stage = flow
        .first_agent_stage()
        .ok_or_else(|| BoardError::NotFound(format!("{project}'s flow has no agent to deliver")))?;
    stage_who(project, stage)
}
