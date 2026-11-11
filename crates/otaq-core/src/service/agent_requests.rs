//! Project agent requests (PAG-01, PAG-04, PAG-06; d0068): open, list, close, resume and rename (the user's), and
//! `otaq agent name`, `ask <KEY>` and `ask human` (the Project agent's own); `prime --project`, Created and
//! changed, and the user's answers.

use super::Shared;
use super::board_requests::error_body;
use crate::board::{BoardError, Refusal};
use crate::caller::Caller;
use crate::project_agents::ONLY_A_PROJECT_AGENT;
use crate::protocol::{ErrorBody, ErrorKind, Reply, Request};

pub(super) async fn handle(
    request: Request,
    caller: &Caller,
    shared: &Shared,
) -> Result<Reply, ErrorBody> {
    let agents = &shared.project_agents;
    let agent = |r: Result<crate::project_agents::ProjectAgent, BoardError>| {
        r.map(|a| Reply::ProjectAgent(Box::new(a)))
            .map_err(|e| error_body(&e))
    };
    match request {
        Request::ProjectAgentOpen(p) => agent(
            agents
                .open_on(&p.project, &p.message, p.agent.as_deref())
                .await,
        ),
        Request::ProjectAgentList(p) => agents
            .list(&p.project)
            .await
            .map(|l| Reply::ProjectAgents(Box::new(l)))
            .map_err(|e| error_body(&e)),
        Request::ProjectAgentClose(p) => agents
            .close(&p.id, p.confirm)
            .await
            .map(|()| Reply::Ok)
            .map_err(|e| error_body(&e)),
        Request::ProjectAgentCloseEnded(p) => agents
            .close_ended(&p.project)
            .await
            .map(|l| Reply::ProjectAgents(Box::new(l)))
            .map_err(|e| error_body(&e)),
        Request::ProjectAgentResume(p) => agent(agents.resume(&p.id).await),
        Request::ProjectAgentRename(p) => agent(agents.rename(&p.id, &p.name).await),
        Request::AgentName(p) => {
            let id = own(caller)?;
            agent(agents.name_by_agent(id, &p.name, p.rename).await)
        }
        Request::AgentAsk(p) => {
            let id = own(caller)?;
            shared
                .messages
                .ask_from_agent(id, &p.target, &p.question, &caller.actor(None))
                .await
                .map(|m| Reply::Message(Box::new(m)))
                .map_err(|e| error_body(&e))
        }
        Request::AgentAskHuman(p) => {
            let id = own(caller)?;
            agent(
                agents
                    .ask_human(id, &p.question, &p.options, &p.notes, p.recommended)
                    .await,
            )
        }
        Request::ProjectAgentAnswer(p) => agents
            .answer(&p.id, &p.choice, p.clear)
            .await
            .map(Reply::Delivery)
            .map_err(|e| error_body(&e)),
        Request::ProjectAgentSend(p) => agents
            .send(&p.id, &p.message)
            .await
            .map(Reply::Delivery)
            .map_err(|e| error_body(&e)),
        Request::ProjectAgentPrime(p) => project_prime(shared, &p.project, caller)
            .await
            .map(|c| Reply::ProjectPrime(Box::new(c)))
            .map_err(|e| error_body(&e)),
        Request::ProjectAgentCreated(p) => agents
            .created_and_changed(&p.id)
            .await
            .map(|c| Reply::CreatedAndChanged(Box::new(c)))
            .map_err(|e| error_body(&e)),
        Request::ProjectAgentUndone(p) => agents
            .undone(&p.id, &p.ticket, &p.change, &caller.actor(None))
            .await
            .map(|()| Reply::Ok)
            .map_err(|e| error_body(&e)),
        other => Err(ErrorBody::new(
            ErrorKind::BadRequest,
            format!("{} isn't a Project agent request", other.method()),
        )),
    }
}

/// The caller's own Project agent; refused for anyone else.
fn own(caller: &Caller) -> Result<&str, ErrorBody> {
    match caller {
        Caller::ProjectAgent { agent, .. } => Ok(agent),
        _ => Err(error_body(&BoardError::refused(
            Refusal::NotAllowed,
            ONLY_A_PROJECT_AGENT,
        ))),
    }
}

/// `otaq prime --project` (PAG-03): the project's context with the stage choices it may set (AGM-06, AGM-08).
/// A Noyan conversation reads the instructions of the agent it runs on.
async fn project_prime(
    shared: &Shared,
    project: &str,
    caller: &Caller,
) -> Result<crate::project_agents::prime::ProjectPrime, BoardError> {
    let conversation = match caller {
        Caller::ProjectAgent { agent, .. } => Some(agent.as_str()),
        _ => None,
    };
    let mut context = shared.board.project_prime(project, conversation).await?;
    context.choices =
        crate::prime::reader_choices(&shared.board, &context.project, &shared.models).await?;
    Ok(context)
}
