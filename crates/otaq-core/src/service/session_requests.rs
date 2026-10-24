//! Session requests: list, attach (stored output + live output on this connection), detach, resize, input
//! (SES-04, SVC-05, SVC-06) and Deliver now (SES-06, SES-07).

use serde_json::json;

use super::Shared;
use super::board_requests::error_body;
use super::connection::Then;
use crate::attention::ClientId;
use crate::board::Refusal;
use crate::protocol::{
    ErrorBody, ErrorKind, Reply, Request, SessionAttach, SessionTarget, decode_bytes, encode_bytes,
};
use crate::session::{SessionError, SessionKind, SessionTicket};

pub(super) async fn handle(
    request: Request,
    client: ClientId,
    shared: &Shared,
) -> Result<(Reply, Then), ErrorBody> {
    let sessions = &shared.sessions;
    match request {
        Request::SessionList(p) => {
            let list = match p.key {
                Some(key) => {
                    let ticket = ticket(shared, &key).await?;
                    sessions.history(&ticket).await.map_err(session_error)?
                }
                None => sessions.in_memory(),
            };
            Ok((Reply::Sessions(list), Then::Continue))
        }
        Request::SessionAttach(p) => {
            let id = session_id(shared, p.target).await?;
            let (attached, answers) = sessions
                .attach_answering(&id, !p.quiet)
                .map_err(session_error)?;
            let reply = Reply::SessionAttached(Box::new(SessionAttach {
                session: attached.info,
                scrollback: encode_bytes(&attached.scrollback),
                start: attached.start,
                end: attached.end,
            }));
            Ok((
                reply,
                Then::Attach {
                    session: id,
                    output: attached.output,
                    answers,
                },
            ))
        }
        Request::SessionDetach(p) => Ok((Reply::Ok, Then::Detach(p.session))),
        Request::SessionResize(p) => {
            sessions.resize(&p.session, p.size).map_err(session_error)?;
            Ok((Reply::Ok, Then::Continue))
        }
        Request::SessionInput(p) => {
            let bytes = decode_bytes(&p.data).map_err(|e| {
                ErrorBody::new(ErrorKind::BadRequest, format!("input isn't base64: {e}"))
            })?;
            if p.reply {
                sessions.write(&p.session, &bytes).map_err(session_error)?;
                return Ok((Reply::Ok, Then::Continue));
            }
            let effect = sessions.input(&p.session, &bytes).map_err(session_error)?;
            // Enter in the ticket's terminal is the user's answer: the needs-you label clears (ATT-02). Not in its
            // Shell tab or Run, which are "not the agent". In the agent's own terminal the first line sent answers
            // an open question (d0119).
            let info = sessions.info(&p.session).ok();
            let line = effect
                .sent
                .iter()
                .find(|line| !line.is_empty())
                .map(String::as_str);
            if effect.enters > 0
                && let Some(info) = info.as_ref().filter(|i| {
                    !matches!(
                        i.kind,
                        crate::session::SessionKind::Shell | crate::session::SessionKind::Run
                    )
                })
                && let Some(key) = &info.ticket
                && let Ok(id) = shared.board.ticket_id(&key.to_string()).await
            {
                let line = line.filter(|_| info.kind == SessionKind::Agent);
                if let Err(e) = shared.signals.enter(&id, line).await {
                    tracing::warn!(session = p.session, error = %e, "can't clear the signal");
                }
            }
            // Enter in a Project agent's terminal answers its permission prompt (ATT-02, d0068), or its open question
            // (d0119).
            if effect.enters > 0
                && info
                    .as_ref()
                    .is_some_and(|i| i.kind == crate::session::SessionKind::ProjectAgent)
                && let Err(e) = shared.project_agents.enter(&p.session, line).await
            {
                tracing::warn!(session = p.session, error = %e, "can't clear the Project agent's wait");
            }
            // Any key may answer a tool's own prompt without a signal (Copilot's Esc, ADP-08).
            if !bytes.is_empty()
                && let Err(e) = shared.signals.typed(&p.session).await
            {
                tracing::warn!(session = p.session, error = %e, "can't check the typed key");
            }
            // An Enter or a backspace may have emptied the line: waiting messages may go now.
            if !sessions.has_unsent_input(&p.session).unwrap_or(true) {
                let deliveries = shared.deliveries.clone();
                let session = p.session.clone();
                tokio::spawn(async move {
                    if let Err(e) = deliveries.deliver_if_ready(&session).await {
                        tracing::warn!(session, error = %e, "delivery failed");
                    }
                });
            }
            Ok((Reply::Ok, Then::Continue))
        }
        Request::SessionSendAgain(p) => {
            let id = session_id(shared, p.target).await?;
            let delivery = shared
                .deliveries
                .send_again(&id, p.clear_first)
                .await
                .map_err(session_error)?;
            Ok((Reply::Delivery(delivery), Then::Continue))
        }
        Request::SessionFocus(p) => {
            sessions
                .set_focused(&p.session, client, p.focused)
                .map_err(session_error)?;
            Ok((Reply::Ok, Then::Continue))
        }
        Request::SessionDeliverNow(p) => {
            let id = session_id(shared, p.target).await?;
            let delivery = if p.clear_first {
                shared.deliveries.clear_and_send(&id, None).await
            } else {
                shared.deliveries.deliver_now(&id).await
            }
            .map_err(session_error)?;
            Ok((Reply::Delivery(delivery), Then::Continue))
        }
        other => Err(ErrorBody::new(
            ErrorKind::BadRequest,
            format!("{} isn't a session request", other.method()),
        )),
    }
}

/// A session by id, the newest session of a kind (agent by default) of a ticket by key or the one running its stage
/// (FLW-20), or a project's Project terminal.
async fn session_id(shared: &Shared, target: SessionTarget) -> Result<String, ErrorBody> {
    let mut kind = target.kind.unwrap_or(SessionKind::Agent);
    match (target.session, target.key, target.project) {
        (Some(id), _, _) => Ok(id),
        (None, Some(key), _) => {
            if let Some(stage) = &target.stage {
                let full = shared
                    .board
                    .ticket(&key)
                    .await
                    .map_err(|e| error_body(&e))?;
                let (of_stage, stage) =
                    super::board_requests::stage_session(&shared.board, &full, stage)
                        .await
                        .map_err(|e| error_body(&e))?;
                kind = of_stage;
                if kind == SessionKind::Validator {
                    return shared
                        .sessions
                        .running_of(&full.id, kind)
                        .map(|s| s.id)
                        .ok_or_else(|| {
                            ErrorBody::new(
                                ErrorKind::NotFound,
                                format!("{}'s {} check isn't running", full.key, stage.name),
                            )
                        });
                }
            }
            let ticket = ticket(shared, &key).await?;
            shared
                .sessions
                .latest_of(&ticket.id, kind)
                .map(|s| s.id)
                .ok_or_else(|| {
                    ErrorBody::new(
                        ErrorKind::NotFound,
                        match kind {
                            SessionKind::Agent => format!("{} has no live session", ticket.key),
                            other => format!("{} has no live {other} session", ticket.key),
                        },
                    )
                })
        }
        (None, None, Some(project)) => shared
            .shells
            .latest_project_terminal(&project)
            .map(|t| t.session.id)
            .ok_or_else(|| {
                ErrorBody::new(
                    ErrorKind::NotFound,
                    format!("{project} has no Project terminal"),
                )
            }),
        (None, None, None) => Err(ErrorBody::new(
            ErrorKind::BadRequest,
            "name a session, a ticket key or a project",
        )),
    }
}

async fn ticket(shared: &Shared, key: &str) -> Result<SessionTicket, ErrorBody> {
    let ticket = shared.board.ticket(key).await.map_err(|e| error_body(&e))?;
    Ok(SessionTicket {
        id: ticket.id,
        key: ticket.key,
    })
}

pub(super) fn session_error(e: SessionError) -> ErrorBody {
    let refused = |code: Refusal, message: String| {
        let mut body = ErrorBody::new(ErrorKind::Refused, message);
        body.data = Some(json!({ "refusal": code }));
        body
    };
    match e {
        SessionError::NotFound(_) => ErrorBody::new(ErrorKind::NotFound, e.to_string()),
        SessionError::AlreadyRunning { .. } => refused(Refusal::AlreadyRunning, e.to_string()),
        SessionError::Ended(_) => refused(Refusal::SessionEnded, e.to_string()),
        SessionError::Paused { .. } => refused(Refusal::NotAllowed, e.to_string()),
        SessionError::Spawn(_) | SessionError::Io(_) | SessionError::Store(_) => {
            tracing::error!(error = %e, "session request failed");
            ErrorBody::new(ErrorKind::Internal, e.to_string())
        }
    }
}
