//! Who-does-the-work requests that aren't a flow's (UX-70, UX-74): "Noyan starts on", the model and effort new Noyan
//! conversations start on. The user's only ([`super::permissions`] refuses agents). A change publishes
//! `agents_changed`, which also has the holds checked again.

use super::Shared;
use super::board_requests::error_body;
use crate::agents::{AgentModel, NoyanStartsOn, effort_refusal};
use crate::board::{BoardError, Refusal};
use crate::protocol::{ErrorBody, ErrorKind, Event, Reply, Request};

pub(super) async fn handle(request: Request, shared: &Shared) -> Result<Reply, ErrorBody> {
    let store = shared.board.store();
    let answer = match request {
        Request::NoyanStartsOn => store
            .noyan_starts_on()
            .await
            .map(|on| Reply::NoyanStartsOn(Box::new(on)))
            .map_err(BoardError::from),
        Request::NoyanSetStartsOn(on) => match checked(&on) {
            Ok(on) => {
                let saved = async {
                    store.set_noyan_starts_on(&on).await?;
                    // A saved model change clears Noyan's mark (AGT-05).
                    crate::agents::mark::clear(store, &crate::agents::About::Noyan).await?;
                    Ok::<_, crate::store::StoreError>(())
                };
                match saved.await {
                    Ok(()) => {
                        shared.events.publish(Event::AgentsChanged);
                        Ok(Reply::NoyanStartsOn(Box::new(on)))
                    }
                    Err(e) => Err(e.into()),
                }
            }
            Err(e) => Err(e),
        },
        other => {
            return Err(ErrorBody::new(
                ErrorKind::BadRequest,
                format!("{} isn't a Noyan setting request", other.method()),
            ));
        }
    };
    answer.map_err(|e| error_body(&e))
}

/// "Noyan starts on" as stored: its model one that reads (`default`, a tool, `tool:model`), its effort one of its
/// tool's levels (UX-75).
fn checked(on: &NoyanStartsOn) -> Result<NoyanStartsOn, BoardError> {
    let model = match on.model.as_deref().map(str::trim).filter(|m| !m.is_empty()) {
        Some(text) => {
            AgentModel::parse(text).map_err(|e| BoardError::refused(Refusal::ModelNotListed, e))?
        }
        None => AgentModel::Default,
    };
    let effort = on
        .effort
        .as_deref()
        .map(str::trim)
        .filter(|e| !e.is_empty());
    if let Some(level) = effort
        && let Some(refused) = effort_refusal(&model, level)
    {
        return Err(BoardError::refused(Refusal::BadEffort, refused));
    }
    Ok(NoyanStartsOn {
        model: Some(model.text()),
        effort: effort.map(str::to_owned),
    })
}
